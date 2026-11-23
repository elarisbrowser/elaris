/* SPDX-License-Identifier: GPL-3.0-only */
#pragma once

#include <elaris_ladybird.h>

#include <cstdint>
#include <cstdlib>
#include <string_view>

// Passes the development and test switches from the environment to the bridge, as the Rust host does in debug builds.
inline void apply_dev_options_from_environment() {
    auto enabled = [](char const* name) {
        auto const* value = std::getenv(name);
        return value && std::string_view(value) == "1";
    };
    uint32_t options = 0;
    if (enabled("ELARIS_TEST_DISABLE_SANDBOX"))
        options |= ELARIS_LADYBIRD_DEV_SANDBOX_OFF;
    if (enabled("ELARIS_TEST_LOCATION"))
        options |= ELARIS_LADYBIRD_DEV_TEST_LOCATION;
    if (enabled("ELARIS_TEST_MICROPHONE_DENIED"))
        options |= ELARIS_LADYBIRD_DEV_MICROPHONE_DENIED;
    auto const* gpu_frames = std::getenv("ELARIS_GPU_FRAMES");
    if (gpu_frames && std::string_view(gpu_frames) != "0")
        options |= ELARIS_LADYBIRD_DEV_GPU_FRAMES;
    elaris_ladybird_set_dev_options(options);
}
