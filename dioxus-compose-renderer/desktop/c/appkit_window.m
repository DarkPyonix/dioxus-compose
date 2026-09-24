// A window of our own, with a Metal layer in it and no toolkit between.
//
// The renderer draws with Skia into a Metal texture. Everything between that texture and
// the screen is AppKit's, and AppKit is what this file talks to: a window, a view, a
// layer, a device and a queue. What the toolkit was doing here was translating the same
// few things into Java and back, and each translation has been somewhere a frame went
// wrong: a window that insists it is opaque, a peer rebuilt under a surface, a title bar
// belonging to a class we cannot reach.
//
// Nothing here draws. The pixels are Skia's, as they already were.
//
// A layer rather than an MTKView. A view of that kind vends its drawable inside its own
// drawing callback and answers nil outside it, so a renderer that decides for itself when
// a frame happens gets a window that stays black. The layer hands one over whenever it is
// asked, which is the arrangement the frame clock already assumes.

#import <AppKit/AppKit.h>
#import <QuartzCore/CAMetalLayer.h>
#import <Metal/Metal.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>

// What happened in the window, waiting to be read.
//
// A queue and not a call. The events arrive on the thread AppKit answers on and the Host
// lives on the one the renderer draws from, so a call would cross between them on every
// click: the boundary is a direct call on one thread and the Host keeps its state there.
// Emptied once per frame instead, which is the same shape the rest of this already has.
enum {
    DXC_EVENT_POINTER_MOVE = 1,
    DXC_EVENT_POINTER_DOWN = 2,
    DXC_EVENT_POINTER_UP = 3,
    DXC_EVENT_SCROLL = 4,
    DXC_EVENT_KEY_DOWN = 5,
    DXC_EVENT_KEY_UP = 6,
};

struct dxc_event {
    int32_t kind;
    // In points from the top left of the content, which is what a scene measures in.
    float x;
    float y;
    int32_t buttons;
    int32_t modifiers;
    // The platform's own key number, and the character it would type. Which Compose key
    // that is gets decided on the other side, where the table lives.
    int32_t key_code;
    int32_t code_point;
};

// Room for a burst rather than for a session. A queue that fills is a queue nobody is
// draining, and holding a thousand stale mouse moves helps no one.
#define DXC_EVENT_CAPACITY 256

static struct dxc_event dxc_events[DXC_EVENT_CAPACITY];
static int dxc_event_head;
static int dxc_event_count;
static pthread_mutex_t dxc_event_lock = PTHREAD_MUTEX_INITIALIZER;

static void dxc_push_event(struct dxc_event event) {
    pthread_mutex_lock(&dxc_event_lock);
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
    pthread_mutex_unlock(&dxc_event_lock);
}

/** Takes the oldest event, or answers zero when there is none. */
int32_t dxc_native_poll_event(struct dxc_event *out) {
    int32_t taken = 0;
    pthread_mutex_lock(&dxc_event_lock);
    if (dxc_event_count > 0) {
        *out = dxc_events[dxc_event_head];
        dxc_event_head = (dxc_event_head + 1) % DXC_EVENT_CAPACITY;
        dxc_event_count--;
        taken = 1;
    }
    pthread_mutex_unlock(&dxc_event_lock);
    return taken;
}

/**
 * The view the window is filled with.
 *
 * It exists to receive. AppKit sends mouse and key events to the view under the pointer
 * and to the one holding focus, and a plain NSView answers none of them; everything here
 * turns one into a record and puts it on the queue above.
 */
@interface DxcView : NSView
@end

@implementation DxcView

- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)isFlipped { return YES; }

// The click that brings a window forward is delivered as well as being spent on the
// bringing. Without this the first press on a control nobody has focused yet is eaten by
// the activation, which reads to the reader as a control that ignored them once.
- (BOOL)acceptsFirstMouse:(NSEvent *)event { return YES; }

- (void)dxcSend:(int32_t)kind event:(NSEvent *)event {
    NSPoint where = [self convertPoint:event.locationInWindow fromView:nil];
    struct dxc_event record;
    memset(&record, 0, sizeof record);
    record.kind = kind;
    record.x = (float)where.x;
    record.y = (float)where.y;
    record.buttons = (int32_t)NSEvent.pressedMouseButtons;
    record.modifiers = (int32_t)event.modifierFlags;
    if (kind == DXC_EVENT_SCROLL) {
        // The wheel's travel rides in the same two fields the pointer uses, because a
        // scroll has no position of its own beyond where the pointer already is.
        record.x = (float)event.scrollingDeltaX;
        record.y = (float)event.scrollingDeltaY;
    }
    if (kind == DXC_EVENT_KEY_DOWN || kind == DXC_EVENT_KEY_UP) {
        record.key_code = (int32_t)event.keyCode;
        NSString *typed = event.charactersIgnoringModifiers;
        record.code_point = typed.length > 0 ? (int32_t)[typed characterAtIndex:0] : 0;
    }
    dxc_push_event(record);
}

- (void)mouseMoved:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_MOVE event:event]; }
- (void)mouseDragged:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_MOVE event:event]; }
- (void)mouseDown:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_DOWN event:event]; }
- (void)mouseUp:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_UP event:event]; }
- (void)rightMouseDown:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_DOWN event:event]; }
- (void)rightMouseUp:(NSEvent *)event { [self dxcSend:DXC_EVENT_POINTER_UP event:event]; }
- (void)scrollWheel:(NSEvent *)event { [self dxcSend:DXC_EVENT_SCROLL event:event]; }
- (void)keyDown:(NSEvent *)event { [self dxcSend:DXC_EVENT_KEY_DOWN event:event]; }
- (void)keyUp:(NSEvent *)event { [self dxcSend:DXC_EVENT_KEY_UP event:event]; }

// Without a tracking area the view hears a moving pointer only while a button is held,
// and hover is half of what a desktop control does.
- (void)updateTrackingAreas {
    for (NSTrackingArea *area in self.trackingAreas) {
        [self removeTrackingArea:area];
    }
    NSTrackingAreaOptions options = NSTrackingMouseMoved | NSTrackingActiveInKeyWindow |
        NSTrackingInVisibleRect;
    [self addTrackingArea:[[NSTrackingArea alloc] initWithRect:self.bounds
                                                      options:options
                                                        owner:self
                                                     userInfo:nil]];
    [super updateTrackingAreas];
}

@end

struct dxc_native_window {
    void *window;
    void *view;
    void *device;
    void *queue;
    void *layer;
};

/**
 * Runs a block on the main thread and waits for it.
 *
 * AppKit answers on one thread and the renderer runs on another, which is the arrangement
 * the shell already sets up: the main thread is in `[NSApp run]` and serves its queue.
 * Straight through when already there, because dispatching to the queue you are on and
 * then waiting for it is a deadlock.
 */
static void dxc_on_main(void (^work)(void)) {
    if ([NSThread isMainThread]) {
        work();
    } else {
        dispatch_sync(dispatch_get_main_queue(), work);
    }
}

/**
 * Opens a window with a Metal layer filling it.
 *
 * Returns zero on success; a non-zero answer means the machine has no Metal device, which
 * is the one failure here that is not a mistake of ours.
 */
int32_t dxc_native_window_open(
    const char *title,
    int32_t width,
    int32_t height,
    struct dxc_native_window *out
) {
    __block int32_t status = 0;
    dxc_on_main(^{
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (device == nil) {
            status = 1;
            return;
        }
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (queue == nil) {
            status = 2;
            return;
        }

        NSRect frame = NSMakeRect(0, 0, width, height);
        NSWindowStyleMask mask = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
            NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable |
            NSWindowStyleMaskFullSizeContentView;
        NSWindow *window = [[NSWindow alloc] initWithContentRect:frame
                                                      styleMask:mask
                                                        backing:NSBackingStoreBuffered
                                                          defer:NO];
        window.title = [NSString stringWithUTF8String:title];
        window.titlebarAppearsTransparent = YES;
        window.titleVisibility = NSWindowTitleHidden;
        window.releasedWhenClosed = NO;

        DxcView *view = [[DxcView alloc] initWithFrame:frame];
        CAMetalLayer *layer = [CAMetalLayer layer];
        layer.device = device;
        layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
        // Skia reads the texture back in places, and a framebuffer-only one cannot be.
        layer.framebufferOnly = NO;
        // Drawn at the density of the screen the window is on rather than in points, so
        // text is as sharp as the display can draw it.
        layer.contentsScale = window.backingScaleFactor;
        layer.drawableSize = CGSizeMake(width * layer.contentsScale,
                                        height * layer.contentsScale);
        view.wantsLayer = YES;
        view.layer = layer;

        window.contentView = view;
        [window makeFirstResponder:view];
        window.acceptsMouseMovedEvents = YES;
        [window center];
        [window makeKeyAndOrderFront:nil];
        [NSApp activateIgnoringOtherApps:YES];

        // Held past this scope. The window owns the view, and the renderer owns the
        // window until it closes it.
        out->window = (__bridge_retained void *)window;
        out->view = (__bridge_retained void *)view;
        out->device = (__bridge_retained void *)device;
        out->queue = (__bridge_retained void *)queue;
        out->layer = (__bridge_retained void *)layer;
    }
    });
    return status;
}

/** What the layer is drawn at, in pixels, and how many of them go to a point. */
void dxc_native_window_size(void *layer_pointer, int32_t *width, int32_t *height, float *scale) {
    dxc_on_main(^{
    @autoreleasepool {
        CAMetalLayer *layer = (__bridge CAMetalLayer *)layer_pointer;
        CGSize size = layer.drawableSize;
        *width = (int32_t)size.width;
        *height = (int32_t)size.height;
        *scale = (float)layer.contentsScale;
    }
    });
}

// The drawable the frame being painted belongs to.
//
// Held here between beginning a frame and ending it, because the renderer takes a texture
// and gives back pixels, and the drawable the texture came from is what has to be handed
// to the screen afterwards. One window's worth: a second window would make this a field
// of the window rather than a file-level one.
static id<CAMetalDrawable> dxc_pending_drawable;

/**
 * Takes the next drawable and answers the texture to paint into.
 *
 * Zero when the system has none to give, which happens when frames are being produced
 * faster than the screen takes them. That is not an error: the frame is skipped and the
 * next one asks again.
 */
int32_t dxc_native_frame_begin(void *layer_pointer, void **texture_out) {
    __block int32_t status = 0;
    dxc_on_main(^{
    @autoreleasepool {
        CAMetalLayer *layer = (__bridge CAMetalLayer *)layer_pointer;
        id<CAMetalDrawable> drawable = [layer nextDrawable];
        if (drawable == nil) {
            status = 1;
            return;
        }
        dxc_pending_drawable = drawable;
        *texture_out = (__bridge void *)drawable.texture;
    }
    });
    return status;
}

/** Puts the painted drawable on the screen and lets it go. */
void dxc_native_frame_end(void *queue_pointer) {
    dxc_on_main(^{
    @autoreleasepool {
        if (dxc_pending_drawable == nil) {
            return;
        }
        id<MTLCommandQueue> queue = (__bridge id<MTLCommandQueue>)queue_pointer;
        id<MTLCommandBuffer> buffer = [queue commandBuffer];
        [buffer presentDrawable:dxc_pending_drawable];
        [buffer commit];
        dxc_pending_drawable = nil;
    }
    });
}
