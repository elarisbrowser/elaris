// SPDX-License-Identifier: GPL-3.0-only

#include "frame_blitter_macos.h"

#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>

namespace Elaris {

// One more than the frame on screen and the frame being drawn, so the next copy rarely waits.
static constexpr size_t SURFACE_COUNT = 3;
static constexpr uint32_t PIXEL_FORMAT_BGRA = 'BGRA';

struct Slot {
    IOSurfaceRef surface{nullptr};
    id<MTLTexture> texture{nil};
    int width{};
    int height{};
    // The frame the surface shows, or 0 while it is free.
    uint64_t serial{};
};

struct FrameBlitter::Impl {
    id<MTLDevice> device{nil};
    id<MTLCommandQueue> queue{nil};
    Slot slots[SURFACE_COUNT];

    ~Impl() {
        for (auto& slot : slots)
            discard(slot);
    }

    static void discard(Slot& slot) {
        slot.texture = nil;
        if (slot.surface)
            CFRelease(slot.surface);
        slot = {};
    }

    static id<MTLTexture> texture_for(id<MTLDevice> device, IOSurfaceRef surface, int width, int height) {
        auto* descriptor = [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm
                                                                              width:static_cast<NSUInteger>(width)
                                                                             height:static_cast<NSUInteger>(height)
                                                                          mipmapped:NO];
        descriptor.storageMode = MTLStorageModeShared;
        descriptor.usage = MTLTextureUsageShaderRead;
        return [device newTextureWithDescriptor:descriptor iosurface:surface plane:0];
    }

    // Makes the free slot hold a width x height surface.
    bool prepare(Slot& slot, int width, int height) {
        if (slot.surface && slot.width == width && slot.height == height)
            return true;
        discard(slot);
        NSDictionary* properties = @{
            (id)kIOSurfaceWidth : @(width),
            (id)kIOSurfaceHeight : @(height),
            (id)kIOSurfaceBytesPerElement : @4,
            (id)kIOSurfacePixelFormat : @(PIXEL_FORMAT_BGRA),
            (id)kIOSurfaceBytesPerRow : @(IOSurfaceAlignProperty(kIOSurfaceBytesPerRow, static_cast<size_t>(width) * 4)),
        };
        auto surface = IOSurfaceCreate((__bridge CFDictionaryRef)properties);
        if (!surface)
            return false;
        auto texture = texture_for(device, surface, width, height);
        if (!texture) {
            CFRelease(surface);
            return false;
        }
        slot = {surface, texture, width, height, 0};
        return true;
    }
};

FrameBlitter::FrameBlitter() = default;
FrameBlitter::~FrameBlitter() = default;

std::unique_ptr<FrameBlitter> FrameBlitter::create() {
    auto impl = std::make_unique<Impl>();
    impl->device = MTLCreateSystemDefaultDevice();
    if (!impl->device)
        return nullptr;
    impl->queue = [impl->device newCommandQueue];
    if (!impl->queue)
        return nullptr;
    std::unique_ptr<FrameBlitter> blitter(new FrameBlitter);
    blitter->m_impl = std::move(impl);
    return blitter;
}

FrameBlitter::Result FrameBlitter::copy(void* source, int width, int height, uint64_t serial, void** out) {
    auto* free_slot = static_cast<Slot*>(nullptr);
    // Prefer a surface that is already the right size.
    for (auto& slot : m_impl->slots) {
        if (slot.serial == 0 && slot.surface && slot.width == width && slot.height == height) {
            free_slot = &slot;
            break;
        }
    }
    for (auto& slot : m_impl->slots) {
        if (!free_slot && slot.serial == 0)
            free_slot = &slot;
    }
    if (!free_slot)
        return Result::Busy;
    if (!m_impl->prepare(*free_slot, width, height))
        return Result::Failed;

    @autoreleasepool {
        auto source_surface = static_cast<IOSurfaceRef>(source);
        auto source_texture = Impl::texture_for(m_impl->device, source_surface,
                                                static_cast<int>(IOSurfaceGetWidth(source_surface)),
                                                static_cast<int>(IOSurfaceGetHeight(source_surface)));
        if (!source_texture)
            return Result::Failed;
        id<MTLCommandBuffer> commands = [m_impl->queue commandBuffer];
        id<MTLBlitCommandEncoder> blit = [commands blitCommandEncoder];
        [blit copyFromTexture:source_texture
                  sourceSlice:0
                  sourceLevel:0
                 sourceOrigin:MTLOriginMake(0, 0, 0)
                   sourceSize:MTLSizeMake(static_cast<NSUInteger>(width), static_cast<NSUInteger>(height), 1)
                    toTexture:free_slot->texture
             destinationSlice:0
             destinationLevel:0
            destinationOrigin:MTLOriginMake(0, 0, 0)];
        [blit endEncoding];
        [commands commit];
        // GPU reads of an IOSurface wait for work already scheduled to write it, so the host may draw the copy once
        // the copy is scheduled. Waiting for it to finish instead queued the engine thread behind the GPU's other
        // work, up to 12 ms from idle, and the frame missed the host's next draw. The command buffer keeps the
        // source in use until the copy is done, so the Compositor does not paint into it before then.
        [commands waitUntilScheduled];
        if (commands.status == MTLCommandBufferStatusError)
            return Result::Failed;
    }
    free_slot->serial = serial;
    *out = free_slot->surface;
    return Result::Copied;
}

void FrameBlitter::release_completed(uint64_t completed_serial) {
    for (auto& slot : m_impl->slots) {
        if (slot.serial != 0 && slot.serial < completed_serial)
            slot.serial = 0;
    }
}

void FrameBlitter::trim() {
    for (auto& slot : m_impl->slots) {
        if (slot.serial == 0)
            Impl::discard(slot);
    }
}

}
