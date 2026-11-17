#include <elaris_ladybird.h>

#include <AK/ByteString.h>
#include <AK/LexicalPath.h>
#include <AK/OwnPtr.h>
#include <AK/String.h>
#include <AK/Utf16String.h>

#include <LibCore/EventLoop.h>
#include <LibGfx/SystemTheme.h>
#include <LibURL/Parser.h>
#include <LibURL/URL.h>
#include <LibWebCommon/PixelUnits.h>
#include <LibWebCommon/WebView/Utilities.h>
#include <LibWebView/Application.h>
#include <LibWebView/HeadlessWebView.h>

#include <algorithm>
#include <array>
#include <string>
#include <thread>
#include <vector>

static Core::EventLoop* s_event_loop;
static std::thread::id s_event_loop_thread;

class ElarisApplication final : public WebView::Application {
    WEB_VIEW_APPLICATION(ElarisApplication)

  public:
    explicit ElarisApplication(Optional<AK::ByteString> helper_directory = {})
        : WebView::Application(AK::move(helper_directory)) {}

  private:
    Core::EventLoop& create_platform_event_loop() override;

    bool should_coordinate_browser_process() const override {
        return false;
    }
};

struct ElarisLadybird {
    ElarisLadybirdEventCallback callback{};
    void* context{};
    std::thread::id owner_thread;
    std::vector<ElarisLadybirdView*> views;
};

struct ElarisLadybirdView {
    ElarisLadybird* bridge{};
    AK::NonnullOwnPtr<WebView::HeadlessWebView> view;
};

static AK::OwnPtr<ElarisApplication> s_application;
static thread_local std::string s_last_error;

Core::EventLoop& ElarisApplication::create_platform_event_loop() {
    if (s_event_loop)
        return *s_event_loop;
    return Core::EventLoop::initialize_for_current_thread();
}

static int32_t fail(char const* message) {
    s_last_error = message;
    return -1;
}

static void emit(ElarisLadybird* bridge, ElarisLadybirdEventKind kind, std::string text = {}, int32_t value = 0) {
    if (!bridge || !bridge->callback)
        return;
    ElarisLadybirdEvent event{kind, text.c_str(), value};
    bridge->callback(bridge->context, &event);
}

static std::string to_utf8(AK::Utf16String const& value) {
    auto utf8 = value.to_utf8();
    return {reinterpret_cast<char const*>(utf8.bytes().data()), utf8.bytes().size()};
}

static std::string to_utf8(URL::URL const& url) {
    auto utf8 = url.to_string().to_byte_string();
    return {utf8.characters(), utf8.length()};
}

extern "C" int32_t elaris_ladybird_create(char const* resource_dir, char const* profile_dir,
                                          ElarisLadybirdEventCallback callback, void* context,
                                          ElarisLadybird** out_bridge) {
    if (out_bridge)
        *out_bridge = nullptr;
    if (!resource_dir || !profile_dir || !out_bridge)
        return fail("resource_dir, profile_dir, and out_bridge are required");
    if (s_application)
        return fail("Ladybird is already initialized in this process");
    if (s_event_loop_thread != std::thread::id{} && s_event_loop_thread != std::this_thread::get_id())
        return fail("Ladybird must be initialized on the event loop's original thread");
    if (!s_event_loop && Core::EventLoop::is_running()) {
        s_event_loop = &Core::EventLoop::current();
        s_event_loop_thread = std::this_thread::get_id();
    }

    WebView::s_ladybird_resource_root = resource_dir;

    // Headless manual mode initializes Ladybird without starting its blocking execute() loop.
    AK::StringView arguments[]{
        "elaris"sv,
        "--headless=manual"sv,
        "--profile-path"sv,
        AK::StringView(profile_dir, std::char_traits<char>::length(profile_dir)),
    };
    Main::Arguments main_arguments{0, nullptr, arguments};
    auto application = ElarisApplication::create(main_arguments, Optional<AK::ByteString>{ELARIS_LADYBIRD_HELPER_DIR});
    if (application.is_error()) {
        if (Core::EventLoop::is_running()) {
            s_event_loop = &Core::EventLoop::current();
            s_event_loop_thread = std::this_thread::get_id();
        }
        return fail("Ladybird application initialization failed");
    }

    s_application = application.release_value();
    WebView::s_ladybird_resource_root = resource_dir;
    s_event_loop = &Core::EventLoop::current();
    s_event_loop_thread = std::this_thread::get_id();
    *out_bridge = new ElarisLadybird{callback, context, std::this_thread::get_id(), {}};
    s_last_error.clear();
    return 0;
}

extern "C" int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, ElarisLadybirdView** out_view) {
    if (out_view)
        *out_view = nullptr;
    if (!bridge || !out_view || !s_application)
        return fail("invalid bridge or output pointer");
    if (bridge->owner_thread != std::this_thread::get_id())
        return fail("view creation must run on the initialization thread");
    auto theme_path = AK::LexicalPath::join(WebView::s_ladybird_resource_root, "themes"sv, "Default.ini"sv);
    auto theme = Gfx::load_system_theme(theme_path.string());
    if (theme.is_error())
        return fail("could not load Ladybird's Default.ini theme");

    auto view = WebView::HeadlessWebView::create(theme.release_value(), {1024, 768});
    auto* handle = new ElarisLadybirdView{bridge, AK::move(view)};
    bridge->views.push_back(handle);
    handle->view->on_url_change = [bridge](auto const& url) { emit(bridge, ELARIS_LADYBIRD_NAVIGATION, to_utf8(url)); };
    handle->view->on_title_change = [bridge](auto const& title) {
        emit(bridge, ELARIS_LADYBIRD_TITLE, to_utf8(title));
    };
    handle->view->on_loading_state_change = [bridge](bool loading) {
        emit(bridge, ELARIS_LADYBIRD_LOADING, {}, loading ? 1 : 0);
    };
    handle->view->on_ready_to_paint = [bridge] { emit(bridge, ELARIS_LADYBIRD_FRAME); };
    *out_view = handle;
    s_last_error.clear();
    return 0;
}

extern "C" int32_t elaris_ladybird_load(ElarisLadybirdView* handle, char const* url) {
    if (!handle || !url)
        return fail("view and URL are required");
    if (handle->bridge->owner_thread != std::this_thread::get_id())
        return fail("navigation must run on the initialization thread");
    auto parsed = URL::Parser::basic_parse(AK::StringView(url, std::char_traits<char>::length(url)));
    if (!parsed.has_value())
        return fail("invalid URL");
    handle->view->load(parsed.release_value());
    s_last_error.clear();
    return 0;
}

extern "C" int32_t elaris_ladybird_resize(ElarisLadybirdView* handle, int32_t width, int32_t height) {
    if (!handle || width <= 0 || height <= 0)
        return fail("view and positive dimensions are required");
    if (handle->bridge->owner_thread != std::this_thread::get_id())
        return fail("resize must run on the initialization thread");
    handle->view->reset_viewport_size({width, height});
    s_last_error.clear();
    return 0;
}

extern "C" int32_t elaris_ladybird_pump(ElarisLadybird* bridge) {
    if (!bridge || !s_application)
        return fail("Ladybird is not initialized");
    if (bridge->owner_thread != std::this_thread::get_id())
        return fail("event loop must be pumped on the initialization thread");
    s_event_loop->pump(Core::EventLoop::WaitMode::PollForEvents);
    s_last_error.clear();
    return 0;
}

extern "C" void elaris_ladybird_destroy_view(ElarisLadybirdView* handle) {
    if (!handle)
        return;
    if (handle->bridge->owner_thread != std::this_thread::get_id()) {
        fail("view destruction must run on the initialization thread");
        return;
    }
    handle->view->request_close();
    auto& views = handle->bridge->views;
    views.erase(std::remove(views.begin(), views.end(), handle), views.end());
    delete handle;
}

extern "C" void elaris_ladybird_destroy(ElarisLadybird* bridge) {
    if (!bridge)
        return;
    if (bridge->owner_thread != std::this_thread::get_id()) {
        fail("shutdown must run on the initialization thread");
        return;
    }
    for (auto* view : bridge->views) {
        view->view->request_close();
        delete view;
    }
    bridge->views.clear();
    delete bridge;
    s_application = nullptr;
}

extern "C" char const* elaris_ladybird_last_error(void) {
    return s_last_error.c_str();
}
