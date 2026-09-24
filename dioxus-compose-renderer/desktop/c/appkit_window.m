// A window of our own, with a Metal view in it and no toolkit between.
//
// The renderer draws with Skia into a Metal drawable. Everything between that drawable
// and the screen is AppKit's, and AppKit is what this file talks to: a window, a view,
// a device and a queue. What the toolkit was doing here was translating the same four
// things into Java and back, and each translation has been somewhere a frame went wrong:
// a window that insists it is opaque, a peer rebuilt under a surface, a title bar that
// belongs to a class we cannot reach.
//
// Nothing here draws. The pixels are Skia's, as they already were.

#import <AppKit/AppKit.h>
#import <MetalKit/MetalKit.h>
#include <stdint.h>

struct dxc_native_window {
    void *window;
    void *view;
    void *device;
    void *queue;
};

/**
 * Opens a window with a Metal view filling it.
 *
 * Must be called on the main thread, which is the one AppKit answers on. Returns zero on
 * success; a non-zero answer means the machine has no Metal device, which is the one
 * failure that is not a mistake of ours.
 */
int32_t dxc_native_window_open(
    const char *title,
    int32_t width,
    int32_t height,
    struct dxc_native_window *out
) {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (device == nil) {
            return 1;
        }
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (queue == nil) {
            return 2;
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

        MTKView *view = [[MTKView alloc] initWithFrame:frame device:device];
        // The renderer decides when a frame happens. A view that drives its own clock
        // would draw whether or not anything changed, which is the opposite of what the
        // frame budget asks for.
        view.paused = YES;
        view.enableSetNeedsDisplay = NO;
        view.framebufferOnly = NO;
        view.colorPixelFormat = MTLPixelFormatBGRA8Unorm;

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
        return 0;
    }
}

/** What the view is drawn at, in pixels rather than points. */
void dxc_native_window_size(void *view_pointer, int32_t *width, int32_t *height, float *scale) {
    @autoreleasepool {
        MTKView *view = (__bridge MTKView *)view_pointer;
        CGSize size = view.drawableSize;
        *width = (int32_t)size.width;
        *height = (int32_t)size.height;
        *scale = (float)(view.window.backingScaleFactor);
    }
}

/**
 * Puts the drawable the renderer has just painted on the screen.
 *
 * Skia was handed the view and took its drawable; this ends the frame that took it. A
 * frame that painted nothing is still presented, because a drawable that is taken and
 * not presented is a drawable the system never gets back.
 */
void dxc_native_window_present(void *view_pointer, void *queue_pointer) {
    @autoreleasepool {
        MTKView *view = (__bridge MTKView *)view_pointer;
        id<MTLCommandQueue> queue = (__bridge id<MTLCommandQueue>)queue_pointer;
        id<CAMetalDrawable> drawable = view.currentDrawable;
        if (drawable == nil) {
            return;
        }
        id<MTLCommandBuffer> buffer = [queue commandBuffer];
        [buffer presentDrawable:drawable];
        [buffer commit];
        [view draw];
    }
}

/** Runs the event loop until the window closes. */
void dxc_native_window_run(void) {
    [NSApp run];
}
