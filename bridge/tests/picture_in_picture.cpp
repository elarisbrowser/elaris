// Checks that a page's Picture-in-Picture request opens a floating panel, and that the bridge can be torn down while
// that panel is open.

#include <elaris_ladybird.h>

#include <chrono>
#include <iostream>
#include <string>

namespace {

struct Events {
    std::string title;
};

void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = *static_cast<Events*>(context);
    if (event->kind == ELARIS_LADYBIRD_TITLE && event->text)
        events.title = event->text;
}

bool check(int32_t result, char const* operation) {
    if (result == 0)
        return true;
    std::cerr << operation << " failed: " << elaris_ladybird_last_error() << '\n';
    return false;
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 5) {
        std::cerr << "usage: elaris_ladybird_picture_in_picture <resource-dir> <profile-dir> <page> <video>\n";
        return 2;
    }
    // The page takes the video's file URL from its fragment.
    std::string url = std::string("file://") + argv[3] + "#file://" + argv[4];

    Events events;
    ElarisLadybird* bridge = nullptr;
    if (!check(elaris_ladybird_create(argv[1], argv[2], on_event, &events, &bridge), "create"))
        return 1;

    ElarisLadybirdView* view = nullptr;
    if (!check(elaris_ladybird_create_view(bridge, 1, &view), "create view") ||
        !check(elaris_ladybird_set_visible(view, 1), "show view") ||
        !check(elaris_ladybird_resize(view, 1024, 768, 1.0), "resize view") ||
        !check(elaris_ladybird_load(view, url.c_str()), "load page"))
        return 1;

    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
    auto pending = [&] {
        return events.title != "pip-entered" && events.title.rfind("pip-failed", 0) != 0 &&
               std::chrono::steady_clock::now() < deadline;
    };
    // The video is ready once the page says so. A click then gives the page the user gesture that PiP needs.
    while (events.title != "pip-ready" && pending()) {
        (void)elaris_ladybird_wait(bridge, 100000);
        (void)elaris_ladybird_pump(bridge);
    }
    ElarisLadybirdMouseEvent press{};
    press.type = ELARIS_LADYBIRD_MOUSE_DOWN;
    press.x = 100;
    press.y = 100;
    press.button = ELARIS_LADYBIRD_BUTTON_PRIMARY;
    press.buttons = ELARIS_LADYBIRD_BUTTON_PRIMARY;
    press.click_count = 1;
    ElarisLadybirdMouseEvent release = press;
    release.type = ELARIS_LADYBIRD_MOUSE_UP;
    release.buttons = 0;
    if (!check(elaris_ladybird_mouse_event(view, &press), "press") ||
        !check(elaris_ladybird_mouse_event(view, &release), "release"))
        return 1;
    while (pending()) {
        (void)elaris_ladybird_wait(bridge, 100000);
        (void)elaris_ladybird_pump(bridge);
    }

    bool entered = events.title == "pip-entered";
    if (!entered)
        std::cerr << "picture-in-picture did not open; last title: \"" << events.title << "\"\n";

    // The panel stays open while the bridge is torn down, so teardown must close it first.
    elaris_ladybird_destroy_view(view);
    elaris_ladybird_destroy(bridge);
    return entered ? 0 : 1;
}
