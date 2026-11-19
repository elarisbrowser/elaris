#include <elaris_ladybird.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <string>
#include <thread>
#include <vector>

struct FindResult {
    uint64_t request_id{};
    uint64_t current_match_index{};
    uint64_t total_match_count{};
    bool has_total_match_count{};
};

struct Events {
    unsigned navigation{};
    unsigned committed_navigation{};
    unsigned new_tabs{};
    uint64_t peek_request{};
    std::array<int32_t, 4> peek_bounds{};
    unsigned load_errors{};
    std::string peek_url;
    bool foreground{};
    std::string link_menu;
    std::string clipboard;
    bool editing{};
    unsigned title{};
    unsigned loading_started{};
    unsigned loading_finished{};
    unsigned frames{};
    unsigned favicons{};
    int32_t history{};
    int32_t favicon_width{};
    int32_t favicon_height{};
    std::string last_title;
    std::vector<FindResult> find_results;
    uint64_t selected_text_request{};
    std::string selected_text;
};

// Each view's events, by the id the test gave it.
using EventsByView = std::map<uint64_t, Events>;

static bool check(int32_t result, char const* operation);
template <typename Done> static bool pump_until(ElarisLadybird* bridge, Done done);

static void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = (*static_cast<EventsByView*>(context))[event->view_id];
    switch (event->kind) {
    case ELARIS_LADYBIRD_NEW_TAB:
        ++events.new_tabs;
        events.foreground = event->value;
        break;
    case ELARIS_LADYBIRD_PEEK:
        events.peek_request = event->request_id;
        events.peek_url = event->text;
        std::copy(std::begin(event->link_bounds), std::end(event->link_bounds), events.peek_bounds.begin());
        break;
    case ELARIS_LADYBIRD_LOAD_ERROR:
        ++events.load_errors;
        break;
    case ELARIS_LADYBIRD_CONTEXT_MENU:
        events.link_menu = event->text;
        break;
    case ELARIS_LADYBIRD_CLIPBOARD:
        events.clipboard = event->text;
        break;
    case ELARIS_LADYBIRD_IME_CARET:
        events.editing = event->value;
        break;
    case ELARIS_LADYBIRD_NAVIGATION:
        ++events.navigation;
        break;
    case ELARIS_LADYBIRD_NAVIGATION_COMMITTED:
        ++events.committed_navigation;
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
    case ELARIS_LADYBIRD_FAVICON:
        if (event->favicon_rgba && event->favicon_width > 0 && event->favicon_height > 0 &&
            event->favicon_stride >= event->favicon_width * 4) {
            ++events.favicons;
            events.favicon_width = event->favicon_width;
            events.favicon_height = event->favicon_height;
        }
        break;
    case ELARIS_LADYBIRD_DOWNLOAD:
        break;
    case ELARIS_LADYBIRD_FIND_RESULT:
        events.find_results.push_back({event->request_id, event->current_match_index, event->total_match_count,
                                       event->has_total_match_count != 0});
        break;
    case ELARIS_LADYBIRD_SELECTED_TEXT:
        events.selected_text_request = event->request_id;
        events.selected_text = event->text;
        break;
    }
}

static bool page_find(ElarisLadybird* bridge, ElarisLadybirdView* view, Events& events) {
    auto expect_result = [&](uint64_t request_id, uint64_t current, uint64_t total) {
        auto const result_count = events.find_results.size();
        if (!pump_until(bridge, [&] { return events.find_results.size() > result_count; }))
            return false;
        auto const& result = events.find_results.back();
        return result.request_id == request_id && result.current_match_index == current &&
               result.has_total_match_count && result.total_match_count == total;
    };

    uint8_t empty = 0;
    if (!check(elaris_ladybird_edit(view, ELARIS_EDIT_SELECT_ALL, &empty, 0), "select page text") ||
        !check(elaris_ladybird_selected_text(view, 700), "read selected page text") ||
        !pump_until(bridge, [&] { return events.selected_text_request == 700; }) ||
        events.selected_text.find("Selectable page text") == std::string::npos) {
        std::cerr << "selected webpage text should be returned asynchronously\n";
        return false;
    }

    if (!check(elaris_ladybird_find(view, "needle", 701), "find query") || !expect_result(701, 0, 3) ||
        !check(elaris_ladybird_find_next(view, 702), "find next") || !expect_result(702, 1, 3) ||
        !check(elaris_ladybird_find_previous(view, 703), "find previous") || !expect_result(703, 0, 3) ||
        !check(elaris_ladybird_find_previous(view, 704), "find previous wrap") || !expect_result(704, 2, 3) ||
        !check(elaris_ladybird_find(view, "absent", 705), "find no matches") || !expect_result(705, 0, 0)) {
        std::cerr << "Ladybird find results should count, navigate, and wrap actual page text\n";
        return false;
    }
    return check(elaris_ladybird_find_end(view), "clear find highlights");
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

// Drive the fixed-position link and input in smoke.html at scale one.
static bool page_input(ElarisLadybird* bridge, ElarisLadybirdView* view, uint64_t view_id, EventsByView& all_events) {
    auto& events = all_events.at(view_id);
    auto click = [&](int button, int mods, int y) {
        ElarisLadybirdMouseEvent event{};
        event.x = 30;
        event.y = y;
        event.button = button;
        event.buttons = button;
        event.click_count = 1;
        event.modifiers = mods;
        event.type = ELARIS_LADYBIRD_MOUSE_DOWN;
        if (!check(elaris_ladybird_mouse_event(view, &event), "mouse down"))
            return false;
        event.buttons = 0;
        event.type = ELARIS_LADYBIRD_MOUSE_UP;
        return check(elaris_ladybird_mouse_event(view, &event), "mouse up");
    };
    auto edit = [&](ElarisEdit command, std::string const& text = {}) {
        return check(elaris_ladybird_edit(view, command, reinterpret_cast<uint8_t const*>(text.data()), text.size()),
                     "edit");
    };
    auto expect = [&](auto predicate, char const* name) {
        if (pump_until(bridge, predicate))
            return true;
        std::cerr << "page input failed: " << name << "; title=" << events.last_title << "\n";
        return false;
    };
#ifdef __APPLE__
    constexpr int new_tab_mod = ELARIS_LADYBIRD_MOD_SUPER;
#else
    constexpr int new_tab_mod = ELARIS_LADYBIRD_MOD_CTRL;
#endif
    if (!check(elaris_ladybird_set_focus(view, 1), "focus") || !click(ELARIS_LADYBIRD_BUTTON_SECONDARY, 0, 20) ||
        !expect([&] { return events.link_menu.ends_with("smoke.html#target"); }, "link menu") ||
        !click(ELARIS_LADYBIRD_BUTTON_PRIMARY, new_tab_mod, 20) ||
        !expect([&] { return events.new_tabs == 1 && !events.foreground; }, "modified link") ||
        !click(ELARIS_LADYBIRD_BUTTON_MIDDLE, 0, 20) ||
        !expect([&] { return events.new_tabs == 2 && !events.foreground; }, "middle link") ||
        !click(ELARIS_LADYBIRD_BUTTON_PRIMARY, new_tab_mod | ELARIS_LADYBIRD_MOD_SHIFT, 20) ||
        !expect([&] { return events.new_tabs == 3 && events.foreground; }, "foreground modified link"))
        return false;

    auto commits_before_peek = events.committed_navigation;
    if (!click(ELARIS_LADYBIRD_BUTTON_PRIMARY, ELARIS_LADYBIRD_MOD_SHIFT, 20) ||
        !expect([&] { return events.peek_url.ends_with("smoke.html#target"); }, "Shift-click requests Peek") ||
        events.peek_bounds != std::array<int32_t, 4>{-22, -12, 150, 32} ||
        events.committed_navigation != commits_before_peek || events.new_tabs != 3)
        return false;
    constexpr uint64_t peek_id = 100;
    ElarisLadybirdView* preview = nullptr;
    auto& preview_events = all_events[peek_id];
    auto preview_loaded =
        check(elaris_ladybird_create_view(bridge, peek_id, &preview), "create Peek view") &&
        check(elaris_ladybird_resize(preview, 320, 300, 1), "resize Peek view") &&
        check(elaris_ladybird_load_prepared(preview, view_id, events.peek_request), "load Peek link") &&
        expect(
            [&] {
                return preview_events.committed_navigation && preview_events.frames && preview_events.loading_finished;
            },
            "Peek commits and paints independently");
    auto missing_page = events.peek_url.substr(0, events.peek_url.find_last_of('/')) + "/missing-peek-smoke.html";
    auto error_reported = preview_loaded &&
                          check(elaris_ladybird_load(preview, missing_page.c_str()), "load missing Peek page") &&
                          expect([&] { return preview_events.load_errors == 1; }, "Peek reports load failure");
    if (preview)
        elaris_ladybird_destroy_view(preview);
    all_events.erase(peek_id);
    if (!error_reported || !click(ELARIS_LADYBIRD_BUTTON_PRIMARY, ELARIS_LADYBIRD_MOD_SHIFT, 52) ||
        !expect([&] { return events.editing; }, "editable caret") || events.new_tabs != 3)
        return false;
    ElarisLadybirdKeyEvent key{ELARIS_LADYBIRD_KEY_PRESS, ELARIS_LADYBIRD_KEY_CHARACTER, 0, 'a', 0};
    if (!check(elaris_ladybird_key_event(view, &key), "key down"))
        return false;
    key.type = ELARIS_LADYBIRD_KEY_RELEASE;
    return check(elaris_ladybird_key_event(view, &key), "key up") &&
           expect([&] { return events.last_title == "input:a"; }, "typing") && edit(ELARIS_EDIT_MARK, "k") &&
           edit(ELARIS_EDIT_COMMIT, "界") &&
           expect([&] { return events.last_title == "input:a界"; }, "composition commit") &&
           edit(ELARIS_EDIT_SELECT_ALL) && edit(ELARIS_EDIT_COPY) &&
           expect([&] { return events.clipboard == "a界"; }, "copy selection") && edit(ELARIS_EDIT_CUT) &&
           expect([&] { return events.last_title == "input:"; }, "cut") && edit(ELARIS_EDIT_PASTE, "pasted") &&
           expect([&] { return events.last_title == "input:pasted"; }, "paste");
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
                  check(elaris_ladybird_resize(second, 360, 280, 1), "resize second view") &&
                  check(elaris_ladybird_load(first, url.c_str()), "load first view") &&
                  check(elaris_ladybird_load(second, other), "load second view");
    passed = passed && pump_until(bridge, [&] {
                 ElarisLadybirdFrame left{}, right{};
                 return events[10].last_title == "Elaris Bridge Smoke" && events[11].last_title == "Second" &&
                        events[10].frames > 0 && events[11].frames > 0 && elaris_ladybird_frame(first, &left) == 0 &&
                        elaris_ladybird_frame(second, &right) == 0 && left.width == 400 && left.height == 300 &&
                        right.width == 360 && right.height == 280;
             });
    if (!passed)
        std::cerr << "both visible views should produce frames at their own sizes and report their own titles\n";

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
             check(elaris_ladybird_stop_loading(second), "stop reload") &&
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
               (events.navigation == 0 || events.committed_navigation == 0 || events.title == 0 ||
                events.loading_started == 0 || events.loading_finished == 0 || events.frames == 0 ||
                events.favicons == 0 || !frame_ready) &&
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
        if (events.navigation == 0 || events.committed_navigation == 0 || events.title == 0 ||
            events.loading_started == 0 || events.loading_finished == 0 || events.frames == 0 || events.favicons == 0 ||
            events.favicon_width <= 0 || events.favicon_height <= 0 || events.last_title != "Elaris Bridge Smoke") {
            std::cerr << "expected navigation, title, loading, favicon, and ready-to-paint callbacks; observed title='"
                      << events.last_title << "'\n";
            passed = false;
        }
        if (events_by_view.size() != static_cast<size_t>(cycle) + 1) {
            std::cerr << "events arrived for a view other than view " << view_id << '\n';
            passed = false;
        }
        if (passed && cycle == 0)
            passed = page_find(bridge, view, events);
        if (passed && cycle == 0)
            passed = page_input(bridge, view, view_id, events_by_view);
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
