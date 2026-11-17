#include <elaris_ladybird.h>

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <string>
#include <thread>
#include <vector>

struct Events {
    unsigned navigation{};
    unsigned title{};
    unsigned loading_started{};
    unsigned loading_finished{};
    unsigned frames{};
    int32_t history{};
    std::string last_title;
};

// Each view's events, by the id the test gave it.
using EventsByView = std::map<uint64_t, Events>;

static void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = (*static_cast<EventsByView*>(context))[event->view_id];
    switch (event->kind) {
    case ELARIS_LADYBIRD_NAVIGATION:
        ++events.navigation;
        break;
    case ELARIS_LADYBIRD_TITLE:
        ++events.title;
        events.last_title = event->text;
        break;
    case ELARIS_LADYBIRD_LOADING:
        event->value ? ++events.loading_started : ++events.loading_finished;
        break;
    case ELARIS_LADYBIRD_FRAME:
        ++events.frames;
        break;
    case ELARIS_LADYBIRD_HISTORY:
        events.history = event->value;
        break;
    }
}

// Whether the displayed frame has the requested size and scale, and the page's background color.
static bool frame_matches(ElarisLadybirdView* view, int32_t width, int32_t height, double device_pixel_ratio) {
    ElarisLadybirdFrame frame{};
    if (elaris_ladybird_frame(view, &frame) != 0 || frame.width != width || frame.height != height ||
        frame.device_pixel_ratio != device_pixel_ratio)
        return false;
    if (frame.format != ELARIS_LADYBIRD_PIXEL_FORMAT_RGBA8 || frame.stride != width * 4 || frame.damage_width <= 0)
        return false;
    std::vector<uint8_t> rgba(static_cast<size_t>(frame.stride) * frame.height);
    if (elaris_ladybird_copy_frame(view, rgba.data(), rgba.size() - 1) == 0)
        return false;
    if (elaris_ladybird_copy_frame(view, rgba.data(), rgba.size()) != 0)
        return false;
    auto const* corner = &rgba[(static_cast<size_t>(height - 1) * width + width - 1) * 4];
    return corner[0] == 0 && corner[1] == 128 && corner[2] == 255 && corner[3] == 255;
}

static bool check(int32_t result, char const* operation) {
    if (result == 0)
        return true;
    std::cerr << operation << " failed: " << elaris_ladybird_last_error() << '\n';
    return false;
}

// Pumps until `done` holds, for up to 30 seconds.
template <typename Done> static bool pump_until(ElarisLadybird* bridge, Done done) {
    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
    while (!done()) {
        if (std::chrono::steady_clock::now() >= deadline || !check(elaris_ladybird_pump(bridge), "pump"))
            return false;
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return true;
}

// Two views at once: each one's events carry its own id, a hidden one keeps its page, and history moves back and
// forward within one view without touching the other.
static bool two_views(ElarisLadybird* bridge, EventsByView& events, std::string const& url) {
    ElarisLadybirdView* first = nullptr;
    ElarisLadybirdView* second = nullptr;
    auto other = "data:text/html,<title>Second</title>";
    bool passed = check(elaris_ladybird_create_view(bridge, 10, &first), "create first view") &&
                  check(elaris_ladybird_create_view(bridge, 11, &second), "create second view") &&
                  check(elaris_ladybird_resize(first, 400, 300, 1), "resize first view") &&
                  check(elaris_ladybird_resize(second, 400, 300, 1), "resize second view") &&
                  check(elaris_ladybird_load(first, url.c_str()), "load first view") &&
                  check(elaris_ladybird_load(second, other), "load second view");
    passed = passed && pump_until(bridge, [&] {
                 return events[10].last_title == "Elaris Bridge Smoke" && events[11].last_title == "Second";
             });
    if (!passed)
        std::cerr << "each view should see only its own title\n";

    // The hidden first view keeps its page while the second navigates and goes back.
    passed = passed && check(elaris_ladybird_set_visible(first, 0), "hide first view") &&
             check(elaris_ladybird_load(second, url.c_str()), "navigate second view") &&
             pump_until(bridge, [&] { return events[11].history == ELARIS_LADYBIRD_HISTORY_BACK; }) &&
             check(elaris_ladybird_traverse_history(second, -1), "go back") &&
             pump_until(bridge,
                        [&] {
                            return events[11].last_title == "Second" &&
                                   events[11].history == ELARIS_LADYBIRD_HISTORY_FORWARD;
                        }) &&
             check(elaris_ladybird_reload(second), "reload") &&
             check(elaris_ladybird_set_visible(first, 1), "show first view");
    if (passed && (events[10].history != 0 || events[10].last_title != "Elaris Bridge Smoke")) {
        std::cerr << "the second view's history reached the first\n";
        passed = false;
    }
    if (!passed)
        std::cerr << "history should move within the second view alone\n";

    elaris_ladybird_destroy_view(first);
    elaris_ladybird_destroy_view(second);
    return passed;
}

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: elaris_ladybird_smoke RESOURCE_DIR PROFILE_DIR HTML_FILE\n";
        return 2;
    }

    ElarisLadybird* bridge = nullptr;
    if (elaris_ladybird_create(nullptr, argv[2], nullptr, nullptr, &bridge) == 0 || bridge)
        return 1;
    if (elaris_ladybird_create_view(nullptr, 0, nullptr) == 0 || !elaris_ladybird_last_error()[0])
        return 1;

    auto profile_dir = std::filesystem::absolute(argv[2]);
    std::filesystem::create_directories(profile_dir);
    auto profile_blocker = profile_dir.string() + "-blocker";
    {
        std::ofstream blocker(profile_blocker);
        blocker << "not a directory";
    }
    if (elaris_ladybird_create(argv[1], profile_blocker.c_str(), nullptr, nullptr, &bridge) == 0 || bridge)
        return 1;
    std::filesystem::remove(profile_blocker);

    EventsByView events_by_view;
    if (!check(elaris_ladybird_create(argv[1], profile_dir.c_str(), on_event, &events_by_view, &bridge), "initialize"))
        return 1;

    bool passed = true;
    for (int cycle = 0; cycle < 3 && passed; ++cycle) {
        ElarisLadybirdView* view = nullptr;
        uint64_t view_id = cycle + 1;
        auto& events = events_by_view[view_id];
        passed = check(elaris_ladybird_create_view(bridge, view_id, &view), "create view");
        if (!passed)
            break;

        ElarisLadybirdFrame no_frame{};
        if (elaris_ladybird_frame(view, &no_frame) != ELARIS_LADYBIRD_NO_FRAME) {
            std::cerr << "a new view unexpectedly reported a frame\n";
            passed = false;
        }
        if (elaris_ladybird_resize(view, 0, 600, 1) == 0 || elaris_ladybird_resize(view, 800, 600, 0) == 0) {
            std::cerr << "invalid resize unexpectedly succeeded\n";
            passed = false;
        }
        int32_t cross_thread_resize = 0;
        std::thread other_thread([&] { cross_thread_resize = elaris_ladybird_resize(view, 801, 601, 1); });
        other_thread.join();
        if (cross_thread_resize == 0) {
            std::cerr << "cross-thread resize unexpectedly succeeded\n";
            passed = false;
        }

        auto file = std::filesystem::absolute(argv[3]).string();
        auto url = "file://" + file;
        int32_t width = 800 + cycle;
        int32_t height = 600 + cycle;
        double device_pixel_ratio = cycle == 1 ? 2 : 1;
        passed = check(elaris_ladybird_load(view, url.c_str()), "navigate") &&
                 check(elaris_ladybird_resize(view, width, height, device_pixel_ratio), "resize");

        bool frame_ready = false;
        auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
        while (passed &&
               (events.navigation == 0 || events.title == 0 || events.loading_started == 0 ||
                events.loading_finished == 0 || events.frames == 0 || !frame_ready) &&
               std::chrono::steady_clock::now() < deadline) {
            passed = check(elaris_ladybird_pump(bridge), "pump");
            frame_ready = frame_ready || frame_matches(view, width, height, device_pixel_ratio);
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        // A still page has nothing more to do, so a timed wait ends at its timeout, and an untimed one when woken.
        auto wait_start = std::chrono::steady_clock::now();
        passed = passed && check(elaris_ladybird_wait(bridge, 20000), "timed wait");
        if (passed && std::chrono::steady_clock::now() - wait_start > std::chrono::milliseconds(500)) {
            std::cerr << "a timed wait outlasted its timeout\n";
            passed = false;
        }
        passed = passed && check(elaris_ladybird_set_color_scheme(bridge, 1), "dark colour scheme") &&
                 check(elaris_ladybird_set_color_scheme(bridge, 0), "light colour scheme");
        auto* waker = elaris_ladybird_create_waker(bridge);
        if (passed && !waker) {
            std::cerr << "no waker: " << elaris_ladybird_last_error() << '\n';
            passed = false;
        }
        if (passed) {
            std::thread wake_from_another_thread([waker] {
                std::this_thread::sleep_for(std::chrono::milliseconds(20));
                elaris_ladybird_wake(waker);
            });
            passed = check(elaris_ladybird_wait(bridge, -1), "woken wait");
            wake_from_another_thread.join();
        }
        elaris_ladybird_destroy_waker(waker);
        if (!frame_ready) {
            std::cerr << "expected a " << width << "x" << height << " frame at scale " << device_pixel_ratio
                      << " showing the page background\n";
            passed = false;
        }
        if (events.navigation == 0 || events.title == 0 || events.loading_started == 0 ||
            events.loading_finished == 0 || events.frames == 0 || events.last_title != "Elaris Bridge Smoke") {
            std::cerr << "expected navigation, title, loading, and ready-to-paint callbacks; observed title='"
                      << events.last_title << "'\n";
            passed = false;
        }
        if (events_by_view.size() != static_cast<size_t>(cycle) + 1) {
            std::cerr << "events arrived for a view other than view " << view_id << '\n';
            passed = false;
        }
        elaris_ladybird_destroy_view(view);
        auto close_deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(100);
        while (std::chrono::steady_clock::now() < close_deadline) {
            passed = check(elaris_ladybird_pump(bridge), "pump while closing view");
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
    }
    passed = passed && two_views(bridge, events_by_view, "file://" + std::filesystem::absolute(argv[3]).string());
    elaris_ladybird_destroy(bridge);
    bridge = nullptr;
    if (passed) {
        passed = check(elaris_ladybird_create(argv[1], profile_dir.c_str(), nullptr, nullptr, &bridge), "reinitialize");
        if (passed)
            elaris_ladybird_destroy(bridge);
    }
    return passed ? 0 : 1;
}
