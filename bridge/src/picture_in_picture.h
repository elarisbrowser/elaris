// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <elaris_ladybird.h>

#include <AK/Function.h>
#include <AK/NonnullOwnPtr.h>
#include <AK/kmalloc.h>
#include <LibGfx/Size.h>
#include <vector>

namespace Elaris {

// A borderless panel that floats above other windows and shows the frames of one Picture-in-Picture view. Creating,
// configuring and destroying it block until the main thread has run the work, so the main thread must never wait on
// the owner thread. Mouse and resize callbacks run on the main thread.
class PictureInPicturePanel {
public:
    AK_ALLOC_WITH_KMALLOC;

    struct Handlers {
        AK::Function<void(ElarisLadybirdMouseEvent)> on_mouse;
        // The panel's new content size, in points, after the person resizes it.
        AK::Function<void(Gfx::IntSize)> on_resize;
    };

    // The main screen's visible area, in points.
    static Gfx::IntSize screen_size();
    static NonnullOwnPtr<PictureInPicturePanel> create(Gfx::IntSize size, Handlers);
    ~PictureInPicturePanel();

    // The backing scale of the panel's screen, so device pixels are points times this.
    double scale() const;

    // Sizes are in points. The panel keeps the video's aspect ratio between the minimum and maximum sizes.
    void configure(Gfx::IntSize size, Gfx::IntSize video_size, Gfx::IntSize minimum, Gfx::IntSize maximum);
    // RGBA8 pixels in device pixels, row by row, with no padding.
    void show_frame(std::vector<uint8_t> rgba, int width, int height);
    void hide();

private:
    explicit PictureInPicturePanel(void* implementation);

    void* m_implementation;
};

}
