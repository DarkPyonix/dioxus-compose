// A window of our own on Windows, with a Direct3D 12 swapchain in it and no toolkit
// between.
//
// The pair of `appkit_window.m`. The renderer draws with Skia into a swapchain buffer;
// everything between that buffer and the screen belongs to Win32 and DXGI, and this file
// is what talks to them: a window class, a window, an adapter, a device, a queue and a
// swapchain. What the toolkit was doing here was translating the same few things into
// Java and back, and each translation has been somewhere a frame went wrong.
//
// Nothing here draws. The pixels are Skia's, as they already were.
//
// The same C symbols the macOS file exports, because the Kotlin side reaches them by name
// and only one of the two files is ever compiled into an image. What the five pointers in
// `struct dxc_native_window` mean is this platform's business; what `struct dxc_event`
// looks like is not, and it is declared here field for field as the macOS file declares
// it so that one piece of Kotlin can read either.
//
// Two of the four walls the macOS window ran into are not here. A window may be created
// on any thread on Windows, and the thread that created it is the thread its messages are
// delivered to, so there is no main-thread hop: the renderer opens its own window on the
// thread it draws from. Nor is there a view that hands out a drawable only inside its own
// callback; a swapchain answers whenever it is asked. The other two walls stand. Skia's
// recording is not its submission, and a word value lives only in straight-line code, and
// both of those are the Kotlin side's to keep.

// Windows 10, because the per-monitor DPI calls below arrived with it and a window that
// asks the monitor how big a point is was the whole reason for asking.
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif
#define WIN32_LEAN_AND_MEAN
// Every call here is the wide one by name. This is for the few things that are constants
// rather than calls, `IDC_ARROW` among them, which resolve to one width or the other
// through a macro and would otherwise hand a narrow string to a wide function.
#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
// The Direct3D and DXGI interfaces as C macros. Without this the headers offer only the
// C++ member functions, and there is no C++ in this project's boundary.
#define COBJMACROS

#include <windows.h>
#include <windowsx.h>
#include <d3d12.h>
#include <dxgi1_4.h>
#include <stdint.h>
#include <string.h>
#include "win32_resize.h"

// What happened in the window, waiting to be read.
//
// A queue and not a call. Calling Kotlin from here would mean the shell deciding when the
// Host runs, and the Host's state belongs to the thread that draws; it is also a call
// that can arrive in the middle of a message the window is still handling, which is a
// place no renderer wants to be resumed. The window writes events down and the renderer
// empties them once a frame, which is the same shape the macOS side already has.
enum {
    DXC_EVENT_POINTER_MOVE = 1,
    DXC_EVENT_POINTER_DOWN = 2,
    DXC_EVENT_POINTER_UP = 3,
    DXC_EVENT_SCROLL = 4,
    DXC_EVENT_KEY_DOWN = 5,
    DXC_EVENT_KEY_UP = 6,
    // Named here so the two desktops agree about what a kind number means. Nothing on
    // this one sends them yet.
    DXC_EVENT_TEXT_COMMIT = 7,
    DXC_EVENT_TEXT_COMPOSE = 8,
};

// Room for what an input method is composing, which is a syllable or a word and never a
// document. Declared here as well as on the other desktop because one Kotlin reader reads
// both, and a test compares the two declarations for exactly that reason.
#define DXC_TEXT_BYTES 96

struct dxc_event {
    int32_t kind;
    // In pixels from the top left of the client area, which is what a scene measures in.
    float x;
    float y;
    int32_t buttons;
    int32_t modifiers;
    // The platform's own key number, and the character it would type. Which Compose key
    // that is gets decided on the other side, where the table lives.
    int32_t key_code;
    int32_t code_point;
    // UTF-8, ending at the first zero. Empty for everything that is not text.
    //
    // Nothing fills this yet. Text arrives through an input method, and this window has
    // no answer for one: on this platform that means IMM32, and the composition messages
    // are the next thing to write here. Until then a field in this window takes the
    // characters its keys produce and composes nothing, which is English and no more.
    char text[DXC_TEXT_BYTES];
};

// Room for a burst rather than for a session. A queue that fills is a queue nobody is
// draining, and holding a thousand stale mouse moves helps no one.
#define DXC_EVENT_CAPACITY 256

// How many buffers the swapchain flips between. Two is what a window that waits for the
// vertical blank needs; a third only buys anything to a renderer that runs ahead of the
// screen, and this one does not.
#define DXC_BUFFER_COUNT 2

// The format the swapchain and Skia have to agree on. Named here as a number because the
// Kotlin side has to pass the same one to Skia and cannot see this header.
#define DXC_SWAPCHAIN_FORMAT DXGI_FORMAT_R8G8B8A8_UNORM

// No lock. On macOS the events arrive on AppKit's thread and are read on the renderer's,
// so that queue is guarded; here the window belongs to the thread that draws and the
// message pump runs inside the read below, so one thread writes and the same one reads.
// A modal loop (the user dragging the window's edge) dispatches on that thread too.
static struct dxc_event dxc_events[DXC_EVENT_CAPACITY];
static int dxc_event_head;
static int dxc_event_count;

static HWND dxc_window;
static IDXGISwapChain3 *dxc_swapchain;
static ID3D12Device *dxc_device;
static ID3D12CommandQueue *dxc_queue;
static ID3D12Resource *dxc_buffers[DXC_BUFFER_COUNT];
static ID3D12CommandAllocator *dxc_allocator;
static ID3D12GraphicsCommandList *dxc_commands;
static ID3D12Fence *dxc_fence;
static HANDLE dxc_fence_signalled;
static UINT64 dxc_fence_value;
static UINT dxc_frame_index;
// The size the window has been given and the size it is drawn at, which are the same
// except while a resize is being taken. A swapchain cannot be refitted while the buffer
// being refitted is the one being drawn into, so the size is written down here and acted
// on where a frame begins.
static struct dxc_resize dxc_sizing;
// Set when the window has gone, so the frame loop stops rather than drawing into nothing.
static int dxc_window_gone;

// A frame, asked for by the window rather than by the loop that usually draws them.
//
// Registered by the renderer, which is the only thing that can draw: the pixels are
// Skia's and this file has never had any. It is needed because Windows runs a loop of its
// own inside `DefWindowProc` while the reader drags the window's edge, and for the whole
// of that drag the renderer's frame loop is stopped inside the message that began it. A
// size written down there is a size nothing draws until the drag ends.
//
// This is the renderer's platform code calling the renderer, on the one thread both live
// on. Nothing of the Host's crosses here and no new boundary entry point is involved: the
// window asks its own renderer to draw, which is what the frame loop would have done had
// it been given a turn.
typedef void (*dxc_draw_frame_fn)(void *isolate_thread);
static dxc_draw_frame_fn dxc_draw_frame;
// The renderer's thread, as the renderer named it when it registered. Handed back with
// every call because the other side is a Java runtime and cannot be entered without it.
static void *dxc_draw_thread;

/**
 * Lets the renderer be asked for a frame from inside a message.
 *
 * A null callback is how it is taken away again, which the renderer does before it closes
 * the scene: a message arriving after that would be a frame drawn into a scene that has
 * gone.
 */
void dxc_native_set_draw_callback(dxc_draw_frame_fn callback, void *isolate_thread) {
    dxc_draw_frame = callback;
    dxc_draw_thread = isolate_thread;
}

/**
 * Asks for a frame now, where a frame can be drawn at all.
 *
 * Nothing happens before the renderer has registered, which covers the first few messages
 * a window receives while it is still being built.
 */
static void dxc_draw_one_frame(void) {
    if (dxc_draw_frame != NULL) {
        dxc_draw_frame(dxc_draw_thread);
    }
}

static void dxc_push_event(struct dxc_event event) {
    if (dxc_event_count < DXC_EVENT_CAPACITY) {
        int slot = (dxc_event_head + dxc_event_count) % DXC_EVENT_CAPACITY;
        dxc_events[slot] = event;
        dxc_event_count++;
    } else {
        // Full: the oldest goes. A dropped move from a while ago is a position that has
        // already been overtaken, and dropping the newest would leave the pointer
        // somewhere it no longer is.
        dxc_events[dxc_event_head] = event;
        dxc_event_head = (dxc_event_head + 1) % DXC_EVENT_CAPACITY;
    }
}

/** Which mouse buttons are down, as bit zero for the left one and bit one for the right. */
static int32_t dxc_pressed_buttons(void) {
    int32_t buttons = 0;
    if (GetKeyState(VK_LBUTTON) < 0) buttons |= 1;
    if (GetKeyState(VK_RBUTTON) < 0) buttons |= 2;
    if (GetKeyState(VK_MBUTTON) < 0) buttons |= 4;
    return buttons;
}

/**
 * Which modifier keys are held.
 *
 * Bits of our own choosing rather than a Win32 value, because there is no Win32 value:
 * the platform answers one key at a time. Shift, control, alt, then the Windows key.
 * What they mean to Compose is decided on the other side, where the table lives.
 */
static int32_t dxc_held_modifiers(void) {
    int32_t modifiers = 0;
    if (GetKeyState(VK_SHIFT) < 0) modifiers |= 1;
    if (GetKeyState(VK_CONTROL) < 0) modifiers |= 2;
    if (GetKeyState(VK_MENU) < 0) modifiers |= 4;
    if (GetKeyState(VK_LWIN) < 0 || GetKeyState(VK_RWIN) < 0) modifiers |= 8;
    return modifiers;
}

/** How far one notch of the wheel is meant to move, as the reader set it. */
static float dxc_wheel_lines(void) {
    UINT lines = 3;
    if (!SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &lines, 0)) {
        lines = 3;
    }
    // A wheel set to move a page at a time answers with a sentinel rather than a count.
    if (lines == 0 || lines == WHEEL_PAGESCROLL) {
        lines = 3;
    }
    return (float)lines;
}

static void dxc_push_pointer(int32_t kind, LPARAM where) {
    struct dxc_event record;
    memset(&record, 0, sizeof record);
    record.kind = kind;
    record.x = (float)GET_X_LPARAM(where);
    record.y = (float)GET_Y_LPARAM(where);
    record.buttons = dxc_pressed_buttons();
    record.modifiers = dxc_held_modifiers();
    dxc_push_event(record);
}

static void dxc_push_scroll(float x, float y) {
    struct dxc_event record;
    memset(&record, 0, sizeof record);
    record.kind = DXC_EVENT_SCROLL;
    // The wheel's travel rides in the same two fields the pointer uses, because a scroll
    // has no position of its own beyond where the pointer already is.
    record.x = x;
    record.y = y;
    record.buttons = dxc_pressed_buttons();
    record.modifiers = dxc_held_modifiers();
    dxc_push_event(record);
}

static void dxc_push_key(int32_t kind, WPARAM key) {
    struct dxc_event record;
    memset(&record, 0, sizeof record);
    record.kind = kind;
    record.buttons = dxc_pressed_buttons();
    record.modifiers = dxc_held_modifiers();
    record.key_code = (int32_t)key;
    // The character the key carries with no modifier applied, which is what the macOS
    // side puts here. Windows delivers typed text as a separate message, so it is asked
    // for rather than waited for. A dead key answers with its top bit set, and the
    // character underneath is the part worth keeping.
    UINT typed = MapVirtualKeyW((UINT)key, MAPVK_VK_TO_CHAR);
    record.code_point = (int32_t)(typed & 0x7fffffffu);
    dxc_push_event(record);
}

// Named apart from the one in `renderer_entry.c`, which subclasses the toolkit's frame
// to reclaim its caption. That one goes looking for a window of AWT's class and will
// not find this one, so the two never meet; the names are kept distinct anyway,
// because a reader who found both would have every reason to think they were.
static LRESULT CALLBACK dxc_native_window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
    switch (message) {
    case WM_MOUSEMOVE:
        // No tracking area. Windows delivers a move whenever the pointer is over the
        // client area, so hover, which is half of what a desktop control does, arrives
        // without having to be asked for.
        dxc_push_pointer(DXC_EVENT_POINTER_MOVE, lparam);
        return 0;
    case WM_LBUTTONDOWN:
    case WM_RBUTTONDOWN:
    case WM_MBUTTONDOWN:
        // Held so that a drag leaving the window still reports where it went, and so the
        // release that ends it is heard at all.
        SetCapture(window);
        dxc_push_pointer(DXC_EVENT_POINTER_DOWN, lparam);
        return 0;
    case WM_LBUTTONUP:
    case WM_RBUTTONUP:
    case WM_MBUTTONUP:
        ReleaseCapture();
        dxc_push_pointer(DXC_EVENT_POINTER_UP, lparam);
        return 0;
    case WM_MOUSEWHEEL:
        dxc_push_scroll(0.0f,
            (float)GET_WHEEL_DELTA_WPARAM(wparam) / (float)WHEEL_DELTA * dxc_wheel_lines());
        return 0;
    case WM_MOUSEHWHEEL:
        dxc_push_scroll(
            (float)GET_WHEEL_DELTA_WPARAM(wparam) / (float)WHEEL_DELTA * dxc_wheel_lines(),
            0.0f);
        return 0;
    case WM_KEYDOWN:
    case WM_SYSKEYDOWN:
        // Anything held with alt, and F10, arrive as a system key. Answered here rather
        // than passed on, because the default handler puts the window into menu mode on a
        // keystroke the scene was meant to read. Alt and F4 together is the exception: it
        // is how a window is closed from the keyboard, and the handler that does that is
        // the one being stepped around.
        dxc_push_key(DXC_EVENT_KEY_DOWN, wparam);
        if (message == WM_SYSKEYDOWN && wparam == VK_F4) {
            break;
        }
        return 0;
    case WM_KEYUP:
    case WM_SYSKEYUP:
        dxc_push_key(DXC_EVENT_KEY_UP, wparam);
        return 0;
    case WM_ENTERSIZEMOVE:
        // The reader has taken hold of an edge, or of the title bar. From here until the
        // matching message below, everything this window hears is dispatched from a loop
        // inside `DefWindowProc` rather than from the renderer's frame loop, and that
        // loop does not return until the reader lets go.
        dxc_resize_begin_drag(&dxc_sizing);
        return 0;
    case WM_EXITSIZEMOVE:
        // Let go. The frame loop has its turns back, so a size arriving after this is
        // written down and taken by the next frame.
        dxc_resize_end_drag(&dxc_sizing);
        return 0;
    case WM_SIZE:
        // Written down rather than acted on. The buffer being refitted may be the one the
        // frame in flight is drawing into, so the swapchain is refitted where a frame
        // begins instead. Nothing to do while minimised: the client area is empty and a
        // swapchain cannot have a zero dimension.
        if (dxc_swapchain != NULL && wparam != SIZE_MINIMIZED) {
            dxc_resize_note(&dxc_sizing, (int32_t)LOWORD(lparam), (int32_t)HIWORD(lparam));
            // Inside a drag the note is not enough. Nothing is going to come back and
            // read it: the frame loop is stopped several frames back inside the press
            // that began the drag, and what the screen shows meanwhile is the last frame
            // the window drew, stretched or cut to whatever size the window now is. The
            // frame is drawn here instead, inside this message, which is the only place
            // that runs while the reader is dragging.
            if (dxc_resize_draw_here(&dxc_sizing)) {
                dxc_draw_one_frame();
            }
        }
        return 0;
    case WM_ERASEBKGND:
        // Answered so the window is never painted white between frames. Every pixel of
        // the client area comes from the swapchain.
        return 1;
    case WM_CLOSE:
        DestroyWindow(window);
        return 0;
    case WM_DESTROY:
        dxc_window = NULL;
        dxc_window_gone = 1;
        PostQuitMessage(0);
        return 0;
    default:
        break;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

/**
 * Hands Windows the messages it has been holding.
 *
 * Here rather than in a loop of its own because Windows delivers messages to the thread
 * that made the window, and that is the thread the renderer draws from: a pump anywhere
 * else would be a pump the window never hears. Run when the queue has nothing left, which
 * is twice a frame: once to fill it and once to find it empty.
 */
static void dxc_pump_messages(void) {
    MSG message;
    while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

/**
 * Lets the window answer for itself for a moment.
 *
 * Called once a frame. The thread that draws is the thread Windows delivers to, so a loop
 * that never gave it a turn would be a window that heard nothing.
 *
 * The wait is for something to arrive rather than for the clock. A frame that drew has
 * already waited for the screen inside `Present`, and the caller asks for no wait at all
 * in that case; a window with nothing happening is asked to rest for a frame's length,
 * and comes back the moment anything is pressed.
 */
void dxc_native_pump(double seconds) {
    if (dxc_window != NULL && seconds > 0.0) {
        DWORD wait = (DWORD)(seconds * 1000.0 + 0.5);
        if (wait > 0) {
            // Returns at once where something is already waiting, which is what the last
            // flag asks for. Without it a message that arrived before this call would be
            // paid for with a whole frame of sleeping.
            MsgWaitForMultipleObjectsEx(0, NULL, wait, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
    }
    dxc_pump_messages();
}

/** True once the reader has closed the window. */
int32_t dxc_native_window_closed(void) {
    return dxc_window_gone ? 1 : 0;
}

/**
 * The menu bar this platform does not have.
 *
 * Named because one piece of Kotlin drives both desktops and asks for this by name on
 * each. macOS keeps its application menu outside the window, and the shortcuts a reader
 * expects there do nothing without it. Windows keeps nothing outside the window: closing
 * is alt with F4 and the system menu, which the default handler already answers, and the
 * editing shortcuts belong to whatever holds focus, which is the scene.
 */
void dxc_native_install_menu(const char *application_name) {
    (void)application_name;
}

/** Takes the oldest event, or answers zero when there is none. */
int32_t dxc_native_poll_event(struct dxc_event *out) {
    if (dxc_event_count == 0) {
        dxc_pump_messages();
    }
    if (dxc_event_count == 0) {
        return 0;
    }
    *out = dxc_events[dxc_event_head];
    dxc_event_head = (dxc_event_head + 1) % DXC_EVENT_CAPACITY;
    dxc_event_count--;
    return 1;
}

struct dxc_native_window {
    void *window;
    void *device;
    void *queue;
    void *adapter;
    void *swapchain;
};

/** Waits until the queue has finished everything put on it. */
static void dxc_wait_for_gpu(void) {
    if (dxc_queue == NULL || dxc_fence == NULL) {
        return;
    }
    UINT64 mark = ++dxc_fence_value;
    if (FAILED(ID3D12CommandQueue_Signal(dxc_queue, dxc_fence, mark))) {
        return;
    }
    if (ID3D12Fence_GetCompletedValue(dxc_fence) < mark) {
        if (SUCCEEDED(ID3D12Fence_SetEventOnCompletion(dxc_fence, mark, dxc_fence_signalled))) {
            WaitForSingleObject(dxc_fence_signalled, INFINITE);
        }
    }
}

static void dxc_release_buffers(void) {
    for (int index = 0; index < DXC_BUFFER_COUNT; index++) {
        if (dxc_buffers[index] != NULL) {
            ID3D12Resource_Release(dxc_buffers[index]);
            dxc_buffers[index] = NULL;
        }
    }
}

static int32_t dxc_acquire_buffers(void) {
    for (int index = 0; index < DXC_BUFFER_COUNT; index++) {
        HRESULT taken = IDXGISwapChain3_GetBuffer(
            dxc_swapchain, (UINT)index, &IID_ID3D12Resource, (void **)&dxc_buffers[index]);
        if (FAILED(taken)) {
            dxc_release_buffers();
            return 1;
        }
    }
    return 0;
}

/**
 * Lets go of a window that was only half made.
 *
 * Everything the caller had reached by the time it failed, and the globals with it, so
 * that a later frame finds no window rather than a window missing a piece of itself.
 */
static void dxc_abandon_window(IDXGIAdapter1 *adapter) {
    dxc_release_buffers();
    if (dxc_commands != NULL) { ID3D12GraphicsCommandList_Release(dxc_commands); dxc_commands = NULL; }
    if (dxc_allocator != NULL) { ID3D12CommandAllocator_Release(dxc_allocator); dxc_allocator = NULL; }
    if (dxc_fence != NULL) { ID3D12Fence_Release(dxc_fence); dxc_fence = NULL; }
    if (dxc_fence_signalled != NULL) { CloseHandle(dxc_fence_signalled); dxc_fence_signalled = NULL; }
    if (dxc_swapchain != NULL) { IDXGISwapChain3_Release(dxc_swapchain); dxc_swapchain = NULL; }
    if (dxc_queue != NULL) { ID3D12CommandQueue_Release(dxc_queue); dxc_queue = NULL; }
    if (dxc_device != NULL) { ID3D12Device_Release(dxc_device); dxc_device = NULL; }
    if (adapter != NULL) { IDXGIAdapter1_Release(adapter); }
    if (dxc_window != NULL) { DestroyWindow(dxc_window); dxc_window = NULL; }
}

static float dxc_scale_of(HWND window) {
    UINT dpi = GetDpiForWindow(window);
    if (dpi == 0) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    return (float)dpi / (float)USER_DEFAULT_SCREEN_DPI;
}

static const wchar_t *DXC_WINDOW_CLASS = L"DioxusComposeWindow";

static int32_t dxc_register_class(void) {
    static int registered;
    if (registered) {
        return 0;
    }
    WNDCLASSEXW description;
    memset(&description, 0, sizeof description);
    description.cbSize = sizeof description;
    // Redrawn whole on either axis changing, because the swapchain owns every pixel and
    // has no use for a partial invalidation.
    description.style = CS_HREDRAW | CS_VREDRAW;
    description.lpfnWndProc = dxc_native_window_proc;
    description.hInstance = GetModuleHandleW(NULL);
    description.hCursor = LoadCursorW(NULL, IDC_ARROW);
    // No background brush. Windows would otherwise fill the client area with it before
    // the first frame lands, which reads as a white flash on a dark scene.
    description.hbrBackground = NULL;
    description.lpszClassName = DXC_WINDOW_CLASS;
    if (RegisterClassExW(&description) == 0) {
        return 1;
    }
    registered = 1;
    return 0;
}

/**
 * Opens a window with a Direct3D 12 swapchain filling it.
 *
 * Returns zero on success. Anything else says which part of the machine did not answer,
 * and those are the failures here that are not a mistake of ours: a machine with no
 * Direct3D 12 adapter has nothing this path can use.
 */
int32_t dxc_native_window_open(
    const char *title,
    int32_t width,
    int32_t height,
    struct dxc_native_window *out
) {
    // Asked for before the window exists, so the sizes below are read in real pixels
    // rather than in the ones Windows would have stretched for us. The shim this library
    // is entered through asks for the same thing at startup, and asking twice costs a
    // refusal nobody reads; this file opening a window without it would cost a window at
    // the wrong size.
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    if (dxc_register_class() != 0) {
        return 1;
    }

    wchar_t wide_title[256];
    if (MultiByteToWideChar(CP_UTF8, 0, title, -1, wide_title,
                            (int)(sizeof wide_title / sizeof *wide_title)) == 0) {
        wide_title[0] = L'\0';
    }

    HWND window = CreateWindowExW(
        0,
        DXC_WINDOW_CLASS,
        wide_title,
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT, CW_USEDEFAULT, width, height,
        NULL, NULL, GetModuleHandleW(NULL), NULL);
    if (window == NULL) {
        return 2;
    }

    // The size that was asked for is in points, and the window was made in whatever
    // Windows took those numbers to be. Now that there is a window there is a monitor to
    // ask, so the client area is set to the pixels those points come to.
    UINT dpi = GetDpiForWindow(window);
    if (dpi == 0) {
        dpi = USER_DEFAULT_SCREEN_DPI;
    }
    RECT wanted;
    wanted.left = 0;
    wanted.top = 0;
    wanted.right = MulDiv(width, (int)dpi, USER_DEFAULT_SCREEN_DPI);
    wanted.bottom = MulDiv(height, (int)dpi, USER_DEFAULT_SCREEN_DPI);
    AdjustWindowRectExForDpi(&wanted, WS_OVERLAPPEDWINDOW, FALSE, 0, dpi);
    int outer_width = wanted.right - wanted.left;
    int outer_height = wanted.bottom - wanted.top;
    // Centred on the part of the screen a window is meant to sit in. Where that cannot be
    // asked for, the window keeps the place Windows chose for it rather than being moved
    // to a corner that was never a position.
    RECT work;
    UINT placement = SWP_NOZORDER | SWP_NOMOVE;
    int left = 0;
    int top = 0;
    if (SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0)) {
        left = work.left + ((work.right - work.left) - outer_width) / 2;
        top = work.top + ((work.bottom - work.top) - outer_height) / 2;
        placement = SWP_NOZORDER;
    }
    SetWindowPos(window, NULL, left, top, outer_width, outer_height, placement);

    RECT client;
    GetClientRect(window, &client);
    UINT pixel_width = (UINT)(client.right - client.left);
    UINT pixel_height = (UINT)(client.bottom - client.top);
    if (pixel_width == 0 || pixel_height == 0) {
        DestroyWindow(window);
        return 3;
    }

    IDXGIFactory4 *factory = NULL;
    if (FAILED(CreateDXGIFactory2(0, &IID_IDXGIFactory4, (void **)&factory))) {
        DestroyWindow(window);
        return 4;
    }

    // The first adapter that is a real one and can make a device. A software adapter is
    // skipped rather than taken: it would draw, slowly, and hide the fact that the
    // machine has nothing to draw with.
    IDXGIAdapter1 *adapter = NULL;
    ID3D12Device *device = NULL;
    for (UINT index = 0;
         IDXGIFactory4_EnumAdapters1(factory, index, &adapter) != DXGI_ERROR_NOT_FOUND;
         index++) {
        DXGI_ADAPTER_DESC1 description;
        if (SUCCEEDED(IDXGIAdapter1_GetDesc1(adapter, &description)) &&
            (description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) == 0 &&
            SUCCEEDED(D3D12CreateDevice((IUnknown *)adapter, D3D_FEATURE_LEVEL_11_0,
                                        &IID_ID3D12Device, (void **)&device))) {
            break;
        }
        IDXGIAdapter1_Release(adapter);
        adapter = NULL;
    }
    if (device == NULL) {
        if (adapter != NULL) IDXGIAdapter1_Release(adapter);
        IDXGIFactory4_Release(factory);
        DestroyWindow(window);
        return 5;
    }

    D3D12_COMMAND_QUEUE_DESC queue_description;
    memset(&queue_description, 0, sizeof queue_description);
    queue_description.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    queue_description.Flags = D3D12_COMMAND_QUEUE_FLAG_NONE;
    ID3D12CommandQueue *queue = NULL;
    if (FAILED(ID3D12Device_CreateCommandQueue(device, &queue_description,
                                               &IID_ID3D12CommandQueue, (void **)&queue))) {
        ID3D12Device_Release(device);
        IDXGIAdapter1_Release(adapter);
        IDXGIFactory4_Release(factory);
        DestroyWindow(window);
        return 6;
    }

    DXGI_SWAP_CHAIN_DESC1 swapchain_description;
    memset(&swapchain_description, 0, sizeof swapchain_description);
    swapchain_description.Width = pixel_width;
    swapchain_description.Height = pixel_height;
    swapchain_description.Format = DXC_SWAPCHAIN_FORMAT;
    swapchain_description.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    swapchain_description.BufferCount = DXC_BUFFER_COUNT;
    swapchain_description.SampleDesc.Count = 1;
    swapchain_description.SwapEffect = DXGI_SWAP_EFFECT_FLIP_DISCARD;
    swapchain_description.AlphaMode = DXGI_ALPHA_MODE_IGNORE;
    IDXGISwapChain1 *first = NULL;
    HRESULT made = IDXGIFactory4_CreateSwapChainForHwnd(
        factory, (IUnknown *)queue, window, &swapchain_description, NULL, NULL, &first);
    if (FAILED(made)) {
        ID3D12CommandQueue_Release(queue);
        ID3D12Device_Release(device);
        IDXGIAdapter1_Release(adapter);
        IDXGIFactory4_Release(factory);
        DestroyWindow(window);
        return 7;
    }
    // DXGI answers alt-enter by putting the window into its own idea of full screen,
    // which is a mode nothing here knows how to draw in.
    IDXGIFactory4_MakeWindowAssociation(factory, window, DXGI_MWA_NO_ALT_ENTER);
    IDXGIFactory4_Release(factory);

    IDXGISwapChain3 *swapchain = NULL;
    // The third revision is the one that will say which buffer is next, and a swapchain
    // that flips has no other way of telling.
    HRESULT upgraded = IDXGISwapChain1_QueryInterface(first, &IID_IDXGISwapChain3,
                                                      (void **)&swapchain);
    IDXGISwapChain1_Release(first);
    if (FAILED(upgraded)) {
        ID3D12CommandQueue_Release(queue);
        ID3D12Device_Release(device);
        IDXGIAdapter1_Release(adapter);
        DestroyWindow(window);
        return 8;
    }

    dxc_window = window;
    dxc_device = device;
    dxc_queue = queue;
    dxc_swapchain = swapchain;
    // The size frames are drawn at from here until something resizes the window. Written
    // down now so that the size the window reports as it is shown, which is this one, is
    // recognised as the size the swapchain already is.
    dxc_resize_fitted(&dxc_sizing, (int32_t)pixel_width, (int32_t)pixel_height);

    if (dxc_acquire_buffers() != 0) {
        dxc_abandon_window(adapter);
        return 9;
    }

    // A list of our own, holding one barrier and nothing else. Skia records and submits
    // its own work; what it does not do is put the buffer back into the state a swapchain
    // will accept for presenting, and a buffer presented from any other state is a buffer
    // the debug layer rejects and a driver is free to mishandle.
    if (FAILED(ID3D12Device_CreateCommandAllocator(device, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                                   &IID_ID3D12CommandAllocator,
                                                   (void **)&dxc_allocator)) ||
        FAILED(ID3D12Device_CreateCommandList(device, 0, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                              dxc_allocator, NULL,
                                              &IID_ID3D12GraphicsCommandList,
                                              (void **)&dxc_commands)) ||
        FAILED(ID3D12Device_CreateFence(device, 0, D3D12_FENCE_FLAG_NONE, &IID_ID3D12Fence,
                                        (void **)&dxc_fence))) {
        dxc_abandon_window(adapter);
        return 10;
    }
    ID3D12GraphicsCommandList_Close(dxc_commands);
    dxc_fence_signalled = CreateEventW(NULL, FALSE, FALSE, NULL);
    if (dxc_fence_signalled == NULL) {
        dxc_abandon_window(adapter);
        return 11;
    }

    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    SetFocus(window);

    out->window = (void *)window;
    out->device = (void *)device;
    out->queue = (void *)queue;
    out->adapter = (void *)adapter;
    out->swapchain = (void *)swapchain;
    return 0;
}

/**
 * What the window is drawn at, in pixels, and how many of them go to a point.
 *
 * The swapchain's size and not the client area's. The two agree as soon as a resize has
 * been taken, and where one was refused they do not: a buffer described to Skia as bigger
 * than it is would be painted past its end. The client area answers only before there is
 * a swapchain to ask.
 */
void dxc_native_window_size(void *window_pointer, int32_t *width, int32_t *height, float *scale) {
    HWND window = (HWND)window_pointer;
    *width = 0;
    *height = 0;
    *scale = 1.0f;
    if (window == NULL) {
        return;
    }
    *scale = dxc_scale_of(window);
    DXGI_SWAP_CHAIN_DESC1 description;
    if (dxc_swapchain != NULL &&
        SUCCEEDED(IDXGISwapChain3_GetDesc1(dxc_swapchain, &description))) {
        *width = (int32_t)description.Width;
        *height = (int32_t)description.Height;
        return;
    }
    RECT client;
    if (GetClientRect(window, &client)) {
        *width = (int32_t)(client.right - client.left);
        *height = (int32_t)(client.bottom - client.top);
    }
}

/**
 * Answers the buffer this frame paints into.
 *
 * Non-zero when there is nothing to paint into: the window has been closed, or it is
 * minimised, or the swapchain could not be made to fit a size it has just been given.
 * None of those is an error. The frame is skipped and the next one asks again.
 */
int32_t dxc_native_frame_begin(void *swapchain_pointer, void **texture_out) {
    IDXGISwapChain3 *swapchain = (IDXGISwapChain3 *)swapchain_pointer;
    if (swapchain == NULL || dxc_window == NULL) {
        return 1;
    }

    // The size the window was last given, where that is not the size it is already drawn
    // at. Showing the window reports a size as well, and it is the size the swapchain was
    // just made, so the common case costs a comparison rather than a round of releasing
    // and taking back every buffer.
    int32_t wanted_width = 0;
    int32_t wanted_height = 0;
    if (dxc_resize_take(&dxc_sizing, &wanted_width, &wanted_height)) {
        // Nothing may still be reading the buffers when they are let go, and a swapchain
        // refuses to be refitted while anything holds one.
        dxc_wait_for_gpu();
        dxc_release_buffers();
        HRESULT resized = IDXGISwapChain3_ResizeBuffers(
            swapchain, DXC_BUFFER_COUNT, (UINT)wanted_width, (UINT)wanted_height,
            DXC_SWAPCHAIN_FORMAT, 0);
        // A refusal leaves the swapchain the size it was, so the old buffers are taken
        // back and the window carries on drawing at the size it had. Losing this frame
        // is a stretched image for a moment; not taking them back is a window that
        // stays black from here on.
        if (FAILED(resized)) {
            dxc_acquire_buffers();
            return 2;
        }
        if (dxc_acquire_buffers() != 0) {
            return 2;
        }
        dxc_resize_fitted(&dxc_sizing, wanted_width, wanted_height);
    }

    dxc_frame_index = IDXGISwapChain3_GetCurrentBackBufferIndex(swapchain);
    if (dxc_buffers[dxc_frame_index] == NULL) {
        return 3;
    }
    *texture_out = (void *)dxc_buffers[dxc_frame_index];
    return 0;
}

/** Puts the painted buffer on the screen. */
void dxc_native_frame_end(void *queue_pointer) {
    ID3D12CommandQueue *queue = (ID3D12CommandQueue *)queue_pointer;
    if (queue == NULL || dxc_swapchain == NULL || dxc_buffers[dxc_frame_index] == NULL ||
        dxc_allocator == NULL || dxc_commands == NULL) {
        return;
    }

    // Skia drew into this buffer, so it left it as a render target, and that is what the
    // barrier says it is coming from. The Kotlin side declares the buffer to Skia as
    // being ready to present, which is what it is put back to here, so the two
    // descriptions stay true of the same buffer frame after frame.
    ID3D12CommandAllocator_Reset(dxc_allocator);
    ID3D12GraphicsCommandList_Reset(dxc_commands, dxc_allocator, NULL);
    D3D12_RESOURCE_BARRIER barrier;
    memset(&barrier, 0, sizeof barrier);
    barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    barrier.Flags = D3D12_RESOURCE_BARRIER_FLAG_NONE;
    barrier.Transition.pResource = dxc_buffers[dxc_frame_index];
    barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
    barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
    ID3D12GraphicsCommandList_ResourceBarrier(dxc_commands, 1, &barrier);
    ID3D12GraphicsCommandList_Close(dxc_commands);
    ID3D12CommandList *lists[1];
    lists[0] = (ID3D12CommandList *)(void *)dxc_commands;
    ID3D12CommandQueue_ExecuteCommandLists(queue, 1, lists);

    // One, so the frame waits for the screen. A window that presents without waiting
    // spends a machine to draw frames nobody sees.
    IDXGISwapChain3_Present(dxc_swapchain, 1, 0);

    // The next frame will paint into a buffer this one may still be reading from, and a
    // swapchain two buffers deep comes back around immediately.
    dxc_wait_for_gpu();
}
