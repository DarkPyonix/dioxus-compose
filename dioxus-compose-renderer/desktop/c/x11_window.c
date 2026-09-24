// An X11 window and GLX framebuffer owned by the renderer. XWayland accepts the same
// X11 connection on Wayland desktops. Skia paints; this file only presents and records.
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/keysym.h>
#include <GL/gl.h>
#include <GL/glx.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum {
    DXC_EVENT_POINTER_MOVE = 1,
    DXC_EVENT_POINTER_DOWN = 2,
    DXC_EVENT_POINTER_UP = 3,
    DXC_EVENT_SCROLL = 4,
    DXC_EVENT_KEY_DOWN = 5,
    DXC_EVENT_KEY_UP = 6,
    // XIM, ibus and fcitx text composition is separate work. These kinds are declared
    // for the shared reader, but this window does not populate text events.
    DXC_EVENT_TEXT_COMMIT = 7,
    DXC_EVENT_TEXT_COMPOSE = 8,
};

#define DXC_TEXT_BYTES 96
#define DXC_EVENT_CAPACITY 256

struct dxc_event {
    int32_t kind;
    float x;
    float y;
    int32_t buttons;
    int32_t modifiers;
    int32_t key_code;
    int32_t code_point;
    char text[DXC_TEXT_BYTES];
};

// Five pointer slots, matching the size and offsets of the other two windows. The
// members name X11 and GLX resources at the corresponding positions.
struct dxc_native_window {
    void *window;
    void *view;
    void *device;
    void *queue;
    void *layer;
};

static Display *dxc_display;
static Window dxc_window;
static GLXContext dxc_context;
static Colormap dxc_colormap;
static Atom dxc_delete_window;
static int dxc_width;
static int dxc_height;
static int dxc_closed;
static struct dxc_event dxc_events[DXC_EVENT_CAPACITY];
static int dxc_event_head;
static int dxc_event_count;

static void dxc_push_event(struct dxc_event event) {
    if (dxc_event_count == DXC_EVENT_CAPACITY) {
        dxc_event_head = (dxc_event_head + 1) % DXC_EVENT_CAPACITY;
        dxc_event_count--;
    }
    dxc_events[(dxc_event_head + dxc_event_count) % DXC_EVENT_CAPACITY] = event;
    dxc_event_count++;
}

static int32_t dxc_buttons(unsigned int state) {
    return ((state & Button1Mask) ? 1 : 0) |
           ((state & Button3Mask) ? 2 : 0) |
           ((state & Button2Mask) ? 4 : 0);
}

// The shared Compose reader uses these four bits, as NSEvent does. Translate at the
// boundary so the same reader receives the same meaning on every desktop.
static int32_t dxc_modifiers(unsigned int state) {
    return ((state & ShiftMask) ? (1 << 17) : 0) |
           ((state & ControlMask) ? (1 << 18) : 0) |
           ((state & Mod1Mask) ? (1 << 19) : 0) |
           ((state & Mod4Mask) ? (1 << 20) : 0);
}

// The special key values are the shared reader's values. Printable keys also carry
// their code point, obtained from XLookupString for the current keyboard layout.
static int32_t dxc_key_code(KeySym key) {
    switch (key) {
        case XK_Return: case XK_KP_Enter: return 0x24;
        case XK_Tab: case XK_ISO_Left_Tab: return 0x30;
        case XK_space: return 0x31;
        case XK_BackSpace: return 0x33;
        case XK_Escape: return 0x35;
        case XK_Delete: return 0x75;
        case XK_Left: return 0x7B;
        case XK_Right: return 0x7C;
        case XK_Down: return 0x7D;
        case XK_Up: return 0x7E;
        case XK_Home: return 0x73;
        case XK_End: return 0x77;
        case XK_Prior: return 0x74;
        case XK_Next: return 0x79;
        default: return 0;
    }
}

static void dxc_pump_events(void) {
    while (dxc_display != NULL && XPending(dxc_display) > 0) {
        XEvent event;
        XNextEvent(dxc_display, &event);
        struct dxc_event record;
        memset(&record, 0, sizeof record);
        switch (event.type) {
            case MotionNotify:
                record.kind = DXC_EVENT_POINTER_MOVE;
                record.x = (float)event.xmotion.x;
                record.y = (float)event.xmotion.y;
                record.buttons = dxc_buttons(event.xmotion.state);
                record.modifiers = dxc_modifiers(event.xmotion.state);
                break;
            case ButtonPress:
            case ButtonRelease: {
                unsigned int button = event.xbutton.button;
                if (button >= 4 && button <= 7) {
                    if (event.type == ButtonRelease) continue;
                    record.kind = DXC_EVENT_SCROLL;
                    record.x = button == 6 ? -3.0f : button == 7 ? 3.0f : 0.0f;
                    record.y = button == 4 ? -3.0f : button == 5 ? 3.0f : 0.0f;
                } else {
                    record.kind = event.type == ButtonPress ? DXC_EVENT_POINTER_DOWN : DXC_EVENT_POINTER_UP;
                    record.x = (float)event.xbutton.x;
                    record.y = (float)event.xbutton.y;
                    record.buttons = dxc_buttons(event.xbutton.state);
                    int32_t bit = button == 1 ? 1 : button == 3 ? 2 : button == 2 ? 4 : 0;
                    if (event.type == ButtonPress) record.buttons |= bit;
                    else record.buttons &= ~bit;
                }
                record.modifiers = dxc_modifiers(event.xbutton.state);
                break;
            }
            case KeyPress:
            case KeyRelease: {
                KeySym symbol = NoSymbol;
                char bytes[32];
                int count = XLookupString(&event.xkey, bytes, sizeof bytes, &symbol, NULL);
                record.kind = event.type == KeyPress ? DXC_EVENT_KEY_DOWN : DXC_EVENT_KEY_UP;
                record.key_code = dxc_key_code(symbol);
                record.modifiers = dxc_modifiers(event.xkey.state);
                if (count == 1 && (unsigned char)bytes[0] >= 32) {
                    record.code_point = (unsigned char)bytes[0];
                }
                break;
            }
            case ConfigureNotify:
                dxc_width = event.xconfigure.width;
                dxc_height = event.xconfigure.height;
                continue;
            case ClientMessage:
                if ((Atom)event.xclient.data.l[0] == dxc_delete_window) dxc_closed = 1;
                continue;
            case DestroyNotify:
                dxc_closed = 1;
                continue;
            default:
                continue;
        }
        dxc_push_event(record);
    }
}

int32_t dxc_native_poll_event(struct dxc_event *out) {
    if (dxc_event_count == 0) dxc_pump_events();
    if (dxc_event_count == 0) return 0;
    *out = dxc_events[dxc_event_head];
    dxc_event_head = (dxc_event_head + 1) % DXC_EVENT_CAPACITY;
    dxc_event_count--;
    return 1;
}

int32_t dxc_native_window_open(const char *title, int32_t width, int32_t height,
                               struct dxc_native_window *out) {
    memset(out, 0, sizeof *out);
    dxc_display = XOpenDisplay(NULL);
    if (dxc_display == NULL) return 1;
    int screen = DefaultScreen(dxc_display);
    int attributes[] = { GLX_RGBA, GLX_DOUBLEBUFFER, GLX_RED_SIZE, 8,
                         GLX_GREEN_SIZE, 8, GLX_BLUE_SIZE, 8, None };
    XVisualInfo *visual = glXChooseVisual(dxc_display, screen, attributes);
    if (visual == NULL) { XCloseDisplay(dxc_display); dxc_display = NULL; return 2; }
    dxc_colormap = XCreateColormap(dxc_display, RootWindow(dxc_display, screen),
                                   visual->visual, AllocNone);
    XSetWindowAttributes settings;
    memset(&settings, 0, sizeof settings);
    settings.colormap = dxc_colormap;
    settings.event_mask = ExposureMask | StructureNotifyMask | PointerMotionMask |
                          ButtonPressMask | ButtonReleaseMask | KeyPressMask | KeyReleaseMask;
    dxc_window = XCreateWindow(dxc_display, RootWindow(dxc_display, screen), 0, 0,
                               (unsigned int)width, (unsigned int)height, 0, visual->depth,
                               InputOutput, visual->visual, CWColormap | CWEventMask, &settings);
    dxc_context = glXCreateContext(dxc_display, visual, NULL, True);
    XFree(visual);
    if (dxc_window == None || dxc_context == NULL ||
        !glXMakeCurrent(dxc_display, dxc_window, dxc_context)) {
        if (dxc_context != NULL) glXDestroyContext(dxc_display, dxc_context);
        if (dxc_window != None) XDestroyWindow(dxc_display, dxc_window);
        XFreeColormap(dxc_display, dxc_colormap);
        XCloseDisplay(dxc_display);
        dxc_display = NULL;
        return 3;
    }
    dxc_delete_window = XInternAtom(dxc_display, "WM_DELETE_WINDOW", False);
    XSetWMProtocols(dxc_display, dxc_window, &dxc_delete_window, 1);
    XStoreName(dxc_display, dxc_window, title);
    XMapWindow(dxc_display, dxc_window);
    XFlush(dxc_display);
    dxc_width = width;
    dxc_height = height;
    dxc_closed = 0;
    out->window = (void *)(uintptr_t)dxc_window;
    out->view = dxc_display;
    out->device = dxc_context;
    out->queue = dxc_display;
    out->layer = (void *)(uintptr_t)dxc_window;
    return 0;
}

void dxc_native_window_size(void *window_pointer, int32_t *width, int32_t *height, float *scale) {
    (void)window_pointer;
    dxc_pump_events();
    *width = dxc_width;
    *height = dxc_height;
    *scale = 1.0f;
}

int32_t dxc_native_frame_begin(void *window_pointer) {
    (void)window_pointer;
    dxc_pump_events();
    if (dxc_closed || dxc_width <= 0 || dxc_height <= 0) return 1;
    if (!glXMakeCurrent(dxc_display, dxc_window, dxc_context)) return 1;
    return 0;
}

void dxc_native_frame_end(void *display_pointer) {
    (void)display_pointer;
    if (!dxc_closed) glXSwapBuffers(dxc_display, dxc_window);
}
