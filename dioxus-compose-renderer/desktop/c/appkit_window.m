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

        NSView *view = [[NSView alloc] initWithFrame:frame];
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
