// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <AK/Function.h>
#include <AK/NonnullOwnPtr.h>
#include <AK/kmalloc.h>

#include <string>

namespace Elaris {

enum class MediaCommand {
    Play,
    Pause,
    Toggle,
};

// Routes the keyboard's media keys and the system's Now Playing controls to the browser. Registering and removing the
// handlers runs on the main thread. Commands arrive there too, so their receiver must hand them to the owner thread.
class MediaControls {
  public:
    AK_ALLOC_WITH_KMALLOC;

    static NonnullOwnPtr<MediaControls> create(AK::Function<void(MediaCommand)> on_command);
    ~MediaControls();

    // Shows the title in Now Playing, and whether it is playing.
    void set_now_playing(std::string const& title, bool playing);
    void clear_now_playing();

  private:
    explicit MediaControls(void* implementation);

    void* m_implementation;
};

} // namespace Elaris
