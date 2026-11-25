// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <AK/Optional.h>

#include <LibGfx/GpuDeviceIdentity.h>

#include <sys/types.h>

namespace Elaris {

// The Vulkan device behind `device`, the DRM device (either of its nodes) the window system presents on, as Ladybird's
// own Linux UI finds it. The Compositor paints there, so the window system can show its frames without copying them
// over from another GPU.
Optional<Gfx::GpuDeviceIdentity> presentation_gpu_identity(dev_t device);

} // namespace Elaris
