// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <cstdint>
#include <memory>

namespace Elaris {

// Copies a page frame on the GPU into one of a few IOSurfaces of its own. The host's GPU keeps hold of whatever it
// last drew from, and the Compositor repaints none of its backing stores while another process still holds their
// surfaces, so the host is given these copies and Ladybird's backing stores go back to the Compositor at once.
class FrameBlitter {
  public:
    enum class Result {
        Copied,
        // Every surface is still on screen or waiting for the host's GPU. Try again after release_completed().
        Busy,
        Failed,
    };

    static std::unique_ptr<FrameBlitter> create();
    ~FrameBlitter();

    // Copies the top-left width x height pixels of `source` (an IOSurfaceRef) for frame `serial`. On success `out` is
    // a borrowed IOSurfaceRef of exactly that size, valid until release_completed() frees it.
    Result copy(void* source, int width, int height, uint64_t serial, void** out);

    // Frees the surfaces of frames the host replaced with a frame its GPU has finished drawing.
    void release_completed(uint64_t completed_serial);

    struct Impl;

  private:
    FrameBlitter();
    std::unique_ptr<Impl> m_impl;
};

} // namespace Elaris
