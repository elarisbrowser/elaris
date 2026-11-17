#include <elaris_ladybird.h>

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <thread>

struct Events {
    unsigned navigation{};
    unsigned title{};
    unsigned loading_started{};
    unsigned loading_finished{};
    unsigned frames{};
    std::string last_title;
};

static void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = *static_cast<Events*>(context);
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
    }
}

static bool check(int32_t result, char const* operation) {
    if (result == 0)
        return true;
    std::cerr << operation << " failed: " << elaris_ladybird_last_error() << '\n';
    return false;
}

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: elaris_ladybird_smoke RESOURCE_DIR PROFILE_DIR HTML_FILE\n";
        return 2;
    }

    ElarisLadybird* bridge = nullptr;
    if (elaris_ladybird_create(nullptr, argv[2], nullptr, nullptr, &bridge) == 0 || bridge)
        return 1;
    if (elaris_ladybird_create_view(nullptr, nullptr) == 0 || !elaris_ladybird_last_error()[0])
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

    Events events;
    if (!check(elaris_ladybird_create(argv[1], profile_dir.c_str(), on_event, &events, &bridge), "initialize"))
        return 1;

    bool passed = true;
    for (int cycle = 0; cycle < 3 && passed; ++cycle) {
        ElarisLadybirdView* view = nullptr;
        passed = check(elaris_ladybird_create_view(bridge, &view), "create view");
        if (!passed)
            break;

        if (elaris_ladybird_resize(view, 0, 600) == 0) {
            std::cerr << "zero-width resize unexpectedly succeeded\n";
            passed = false;
        }
        int32_t cross_thread_resize = 0;
        std::thread other_thread([&] { cross_thread_resize = elaris_ladybird_resize(view, 801, 601); });
        other_thread.join();
        if (cross_thread_resize == 0) {
            std::cerr << "cross-thread resize unexpectedly succeeded\n";
            passed = false;
        }

        auto file = std::filesystem::absolute(argv[3]).string();
        auto url = "file://" + file;
        passed = check(elaris_ladybird_load(view, url.c_str()), "navigate") &&
                 check(elaris_ladybird_resize(view, 800 + cycle, 600 + cycle), "resize");

        auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
        while (passed &&
               (events.navigation == 0 || events.title == 0 || events.loading_started == 0 ||
                events.loading_finished == 0 || events.frames == 0) &&
               std::chrono::steady_clock::now() < deadline) {
            passed = check(elaris_ladybird_pump(bridge), "pump");
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (events.navigation == 0 || events.title == 0 || events.loading_started == 0 ||
            events.loading_finished == 0 || events.frames == 0 || events.last_title != "Elaris Bridge Smoke") {
            std::cerr << "expected navigation, title, loading, and ready-to-paint callbacks; observed title='"
                      << events.last_title << "'\n";
            passed = false;
        }
        elaris_ladybird_destroy_view(view);
        events = {};
        auto close_deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(100);
        while (std::chrono::steady_clock::now() < close_deadline) {
            passed = check(elaris_ladybird_pump(bridge), "pump while closing view");
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
    }
    elaris_ladybird_destroy(bridge);
    bridge = nullptr;
    if (passed) {
        passed = check(elaris_ladybird_create(argv[1], profile_dir.c_str(), nullptr, nullptr, &bridge), "reinitialize");
        if (passed)
            elaris_ladybird_destroy(bridge);
    }
    return passed ? 0 : 1;
}
