// SPDX-License-Identifier: GPL-3.0-only

#include "picture_in_picture.h"

#include <AK/Function.h>
#include <cstdlib>
#include <cstring>

#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>

static constexpr CGFloat panel_corner_radius = 12;
static constexpr CGFloat panel_screen_margin = 16;

// Panels and their layers belong to the main thread. Work that is already there runs in place, so it never waits on itself.
static void run_on_main(void (^work)(void)) {
    if ([NSThread isMainThread])
        work();
    else
        dispatch_sync(dispatch_get_main_queue(), work);
}

struct PanelImplementation;

@interface ElarisPictureInPictureView : NSView
// Read and written only on the main thread. Cleared before the panel's implementation is deleted.
@property (nonatomic) PanelImplementation* implementation;
@property (nonatomic) BOOL closed;
@end

struct PanelImplementation {
    NSPanel* panel{};
    ElarisPictureInPictureView* view{};
    Elaris::PictureInPicturePanel::Handlers handlers;
    double scale{1};
};

@implementation ElarisPictureInPictureView

- (BOOL)isFlipped {
    return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent*)event {
    return YES;
}

// A press on the picture moves the panel, while the page still receives the press itself.
- (BOOL)mouseDownCanMoveWindow {
    return YES;
}

- (void)updateTrackingAreas {
    [super updateTrackingAreas];
    for (NSTrackingArea* area in self.trackingAreas)
        [self removeTrackingArea:area];
    auto options = NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingActiveAlways | NSTrackingInVisibleRect;
    [self addTrackingArea:[[NSTrackingArea alloc] initWithRect:self.bounds options:options owner:self userInfo:nil]];
}

- (void)setFrameSize:(NSSize)size {
    [super setFrameSize:size];
    if (self.closed || !self.implementation || !self.implementation->handlers.on_resize)
        return;
    self.implementation->handlers.on_resize(Gfx::IntSize(static_cast<int>(size.width), static_cast<int>(size.height)));
}

- (void)sendMouse:(ElarisLadybirdMouseType)type button:(int32_t)button event:(NSEvent*)event {
    PanelImplementation* implementation = self.implementation;
    if (self.closed || !implementation || !implementation->handlers.on_mouse)
        return;
    NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
    ElarisLadybirdMouseEvent mouse{};
    mouse.type = type;
    mouse.x = static_cast<int32_t>(round(point.x * implementation->scale));
    mouse.y = static_cast<int32_t>(round(point.y * implementation->scale));
    mouse.button = button;
    // NSEvent's pressed-button bits match the bridge's: primary, secondary, then middle.
    mouse.buttons = static_cast<int32_t>(NSEvent.pressedMouseButtons & 7);
    auto flags = event.modifierFlags;
    if (flags & NSEventModifierFlagOption)
        mouse.modifiers |= ELARIS_LADYBIRD_MOD_ALT;
    if (flags & NSEventModifierFlagControl)
        mouse.modifiers |= ELARIS_LADYBIRD_MOD_CTRL;
    if (flags & NSEventModifierFlagShift)
        mouse.modifiers |= ELARIS_LADYBIRD_MOD_SHIFT;
    if (flags & NSEventModifierFlagCommand)
        mouse.modifiers |= ELARIS_LADYBIRD_MOD_SUPER;
    mouse.click_count = static_cast<int32_t>(event.clickCount);
    implementation->handlers.on_mouse(mouse);
}

- (void)mouseDown:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_DOWN button:ELARIS_LADYBIRD_BUTTON_PRIMARY event:event];
}

- (void)mouseUp:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_UP button:ELARIS_LADYBIRD_BUTTON_PRIMARY event:event];
}

- (void)rightMouseDown:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_DOWN button:ELARIS_LADYBIRD_BUTTON_SECONDARY event:event];
}

- (void)rightMouseUp:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_UP button:ELARIS_LADYBIRD_BUTTON_SECONDARY event:event];
}

- (void)mouseMoved:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_MOVE button:0 event:event];
}

- (void)mouseDragged:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_MOVE button:0 event:event];
}

- (void)mouseExited:(NSEvent*)event {
    [self sendMouse:ELARIS_LADYBIRD_MOUSE_LEAVE button:0 event:event];
}

@end

namespace Elaris {

static void release_pixels(void* info, void const* data, size_t size) {
    (void)info;
    (void)size;
    std::free(const_cast<void*>(data));
}

static void create_panel(PanelImplementation* implementation, NSSize size) {
    NSRect screen = NSScreen.mainScreen ? NSScreen.mainScreen.visibleFrame : NSZeroRect;
    NSRect frame = NSMakeRect(NSMaxX(screen) - size.width - panel_screen_margin, NSMinY(screen) + panel_screen_margin,
                              size.width, size.height);
    NSWindowStyleMask style = NSWindowStyleMaskBorderless | NSWindowStyleMaskResizable |
                              NSWindowStyleMaskNonactivatingPanel | NSWindowStyleMaskFullSizeContentView;
    NSPanel* panel = [[NSPanel alloc] initWithContentRect:frame styleMask:style backing:NSBackingStoreBuffered defer:NO];
    panel.releasedWhenClosed = NO;
    panel.floatingPanel = YES;
    panel.level = NSFloatingWindowLevel;
    panel.opaque = NO;
    panel.backgroundColor = NSColor.clearColor;
    panel.hasShadow = YES;
    panel.hidesOnDeactivate = NO;
    panel.movableByWindowBackground = YES;
    panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary;

    ElarisPictureInPictureView* view = [[ElarisPictureInPictureView alloc] initWithFrame:NSMakeRect(0, 0, size.width, size.height)];
    view.implementation = implementation;
    view.wantsLayer = YES;
    view.layer.cornerRadius = panel_corner_radius;
    view.layer.masksToBounds = YES;
    view.layer.contentsGravity = kCAGravityResize;
    view.layer.contentsScale = panel.backingScaleFactor;
    panel.contentView = view;

    implementation->scale = panel.backingScaleFactor;
    implementation->panel = panel;
    implementation->view = view;
    [panel orderFrontRegardless];
}

Gfx::IntSize PictureInPicturePanel::screen_size() {
    __block NSSize size = NSZeroSize;
    run_on_main(^{
        if (NSScreen.mainScreen)
            size = NSScreen.mainScreen.visibleFrame.size;
    });
    return Gfx::IntSize(static_cast<int>(size.width), static_cast<int>(size.height));
}

NonnullOwnPtr<PictureInPicturePanel> PictureInPicturePanel::create(Gfx::IntSize size, Handlers handlers) {
    auto* implementation = new PanelImplementation{};
    implementation->handlers = AK::move(handlers);
    NSSize content = NSMakeSize(size.width(), size.height());
    run_on_main(^{
        create_panel(implementation, content);
    });
    return AK::adopt_own(*new PictureInPicturePanel(implementation));
}

PictureInPicturePanel::PictureInPicturePanel(void* implementation)
    : m_implementation(implementation) {}

PictureInPicturePanel::~PictureInPicturePanel() {
    auto* implementation = static_cast<PanelImplementation*>(m_implementation);
    run_on_main(^{
        implementation->view.closed = YES;
        implementation->view.implementation = nullptr;
        [implementation->panel close];
        // Release the window and its view on the main thread, where AppKit expects them to go.
        implementation->panel = nil;
        implementation->view = nil;
    });
    delete implementation;
}

double PictureInPicturePanel::scale() const {
    return static_cast<PanelImplementation*>(m_implementation)->scale;
}

void PictureInPicturePanel::configure(Gfx::IntSize size, Gfx::IntSize video_size, Gfx::IntSize minimum,
                                      Gfx::IntSize maximum) {
    auto* implementation = static_cast<PanelImplementation*>(m_implementation);
    NSSize content = NSMakeSize(size.width(), size.height());
    NSSize aspect = NSMakeSize(video_size.width(), video_size.height());
    NSSize low = NSMakeSize(minimum.width(), minimum.height());
    NSSize high = NSMakeSize(maximum.width(), maximum.height());
    run_on_main(^{
        NSPanel* panel = implementation->panel;
        panel.contentAspectRatio = aspect;
        panel.contentMinSize = low;
        panel.contentMaxSize = high;
        [panel setContentSize:content];
    });
}

void PictureInPicturePanel::show_frame(std::vector<uint8_t> rgba, int width, int height) {
    auto* implementation = static_cast<PanelImplementation*>(m_implementation);
    size_t length = static_cast<size_t>(width) * static_cast<size_t>(height) * 4;
    if (rgba.size() != length)
        return;
    // CoreGraphics keeps the pixels until the image is released, so they are copied into storage it can free.
    auto* pixels = std::malloc(length);
    if (!pixels)
        return;
    std::memcpy(pixels, rgba.data(), length);
    // The view is only read on the main thread, and it outlives this frame: a closed panel skips the frame.
    ElarisPictureInPictureView* view = implementation->view;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (view.closed) {
            std::free(pixels);
            return;
        }
        CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
        CGDataProviderRef provider = CGDataProviderCreateWithData(nullptr, pixels, length, release_pixels);
        // Frames are opaque RGBA bytes in memory order, so the alpha byte is skipped.
        auto info = static_cast<CGBitmapInfo>(static_cast<uint32_t>(kCGBitmapByteOrderDefault) |
                                              static_cast<uint32_t>(kCGImageAlphaNoneSkipLast));
        CGImageRef image = CGImageCreate(width, height, 8, 32, static_cast<size_t>(width) * 4, space, info, provider,
                                         nullptr, false, kCGRenderingIntentDefault);
        view.layer.contents = (__bridge id)image;
        CGImageRelease(image);
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(space);
    });
}

void PictureInPicturePanel::hide() {
    auto* implementation = static_cast<PanelImplementation*>(m_implementation);
    run_on_main(^{
        [implementation->panel orderOut:nil];
    });
}

} // namespace Elaris
