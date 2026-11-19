#include <elaris_ladybird.h>

#include <AK/ByteString.h>
#include <AK/LexicalPath.h>
#include <AK/OwnPtr.h>
#include <AK/String.h>
#include <AK/Utf16String.h>

#include <LibCore/EventLoop.h>
#include <LibCore/Promise.h>
#include <LibCore/StandardPaths.h>
#include <LibCore/Timer.h>
#include <LibFileSystem/FileSystem.h>
#include <LibGfx/Bitmap.h>
#include <LibGfx/SharedImageBuffer.h>
#include <LibGfx/SystemTheme.h>
#include <LibURL/Parser.h>
#include <LibURL/URL.h>
#include <LibWebCommon/Loader/DownloadFilename.h>
#include <LibWebCommon/Page/InputEvent.h>
#include <LibWebCommon/PixelUnits.h>
#include <LibWebCommon/UIEvents/KeyCode.h>
#include <LibWebCommon/WebView/Utilities.h>
#include <LibWebView/Application.h>
#include <LibWebView/ContextMenuRequest.h>
#include <LibWebView/FileDownloader.h>
#include <LibWebView/HeadlessWebView.h>
#include <LibWebView/PageBackingStores.h>
#include <LibWebView/Settings.h>

#include <algorithm>
#include <array>
#include <cerrno>
#include <cmath>
#include <cstring>
#include <deque>
#include <map>
#include <memory>
#include <string>
#include <thread>
#include <vector>

static Core::EventLoop* s_event_loop;
static std::thread::id s_event_loop_thread;
static Optional<AK::ByteString> s_download_directory_override;

static ErrorOr<AK::LexicalPath> unique_download_path(AK::ByteString const& directory, AK::ByteString const& file);
static std::string to_utf8(URL::URL const& url);

class ElarisApplication final : public WebView::Application {
    WEB_VIEW_APPLICATION(ElarisApplication)

  public:
    ElarisLadybird* host{};
    explicit ElarisApplication(Optional<AK::ByteString> helper_directory = {})
        : WebView::Application(AK::move(helper_directory)) {}
    ErrorOr<AK::LexicalPath> default_path_for_downloaded_file(AK::ByteString const& file) const override;

  private:
    void open_navigation_in_new_tab(Web::HTML::PreparedNavigationDescriptor, Web::HTML::ActivateTab,
                                    WebView::ViewImplementation const*) const override;
    Core::EventLoop& create_platform_event_loop() override;

    bool should_coordinate_browser_process() const override {
        return false;
    }

    // Pulling past a scroller's edge and springing back needs to know when the fingers lift, which the host reports
    // with each scroll's gesture phase. Hosts report it from macOS touchpads.
    bool platform_scrolls_elastically() const override {
#if defined(AK_OS_MACOS)
        return true;
#else
        return false;
#endif
    }
};

// A headless view whose frames the host reads from CPU memory, as Ladybird's TerminalWebView does.
class ElarisWebView final : public WebView::HeadlessWebView {
  public:
    static AK::NonnullOwnPtr<ElarisWebView> create(Core::AnonymousBuffer theme, Web::DevicePixelSize size) {
        auto view = AK::adopt_own(*new ElarisWebView(AK::move(theme), size));
        view->initialize_tab(Web::HTML::VisibilityState::Visible);
        view->set_presents_from_cpu_memory(true);
        return view;
    }

    void set_viewport(Web::DevicePixelSize size, double device_pixel_ratio) {
        m_device_pixel_ratio = device_pixel_ratio;
        reset_viewport_size(size);
    }

    Gfx::IntRect damage() const {
        return m_damage;
    }
    void clear_damage() {
        m_damage = {};
    }
    uint64_t serial() const {
        return m_serial;
    }

    // Called when whether the view can go back or forward changes.
    AK::Function<void()> on_navigation_state_change;
    AK::Function<void(WebView::ContextMenuRequest)> on_menu;
    AK::Function<void(Web::HTML::PreparedNavigationDescriptor, Web::HTML::ActivateTab)> on_open_tab;

    void did_request_context_menu(WebView::ContextMenuRequest request) override {
        if (on_menu)
            on_menu(AK::move(request));
    }

    WebView::ViewImplementation* create_view_for_new_tab_or_window(WebView::IsPrivate) override {
        return nullptr;
    }

  private:
    void did_change_state() override {
        if (on_navigation_state_change)
            on_navigation_state_change();
    }

    ElarisWebView(Core::AnonymousBuffer theme, Web::DevicePixelSize size) : HeadlessWebView(AK::move(theme), size) {}

    void did_accept_presented_backing_store(i32, Gfx::IntRect damage_rect) override {
        m_damage = m_damage.is_empty() ? damage_rect : m_damage.united(damage_rect);
        ++m_serial;
    }

    Gfx::IntRect m_damage;
    uint64_t m_serial{};
};

class ElarisDownloadObserver;

struct ElarisLadybird {
    ElarisLadybirdEventCallback callback{};
    void* context{};
    std::thread::id owner_thread;
    std::vector<ElarisLadybirdView*> views;
    std::unique_ptr<ElarisDownloadObserver> download_observer;
};

struct ElarisLadybirdView {
    ElarisLadybird* bridge{};
    uint64_t id{};
    AK::NonnullOwnPtr<ElarisWebView> view;
    Gfx::IntSize copied_size;
    uint64_t next_request{};
    uint64_t menu_request{};
    Optional<Web::HTML::PreparedNavigationDescriptor> menu_target;
    std::map<uint64_t, Web::HTML::PreparedNavigationDescriptor> pending_tabs;
    // ViewImplementation reports find callbacks in the order its page IPC handles requests.
    std::deque<uint64_t> pending_find_requests;
};

static AK::OwnPtr<ElarisApplication> s_application;
static thread_local std::string s_last_error;

namespace {

// Ladybird's bundled Serenity theme uses a rust-colored selection wash. Keep Elaris page
// selections blue, with a lighter variant for dark pages.
constexpr Gfx::Color LIGHT_SELECTION_COLOR{0x0a, 0x66, 0xd0};
constexpr Gfx::Color DARK_SELECTION_COLOR{0x60, 0xa5, 0xfa, 0x99};

void apply_selection_colors(Gfx::SystemTheme& theme) {
    theme.color[(int)Gfx::ColorRole::Selection] = LIGHT_SELECTION_COLOR.value();
    theme.color[(int)Gfx::ColorRole::LightSelection] = LIGHT_SELECTION_COLOR.value();
    theme.color[(int)Gfx::ColorRole::DarkSelection] = DARK_SELECTION_COLOR.value();
}

} // namespace

class ElarisDownloadObserver final : public WebView::FileDownloaderObserver {
  public:
    explicit ElarisDownloadObserver(ElarisLadybird& bridge) : m_bridge(bridge) {
        // Incomplete records from a previous browser run are deliberately not resumed in this
        // milestone. Cancelling them also removes Ladybird's temporary files and stored records.
        Vector<u64> restored;
        for (auto const& download : WebView::Application::the().file_downloader().downloads()) {
            if (WebView::FileDownloader::status_is_active(download.status))
                restored.append(download.id);
        }
        for (auto id : restored)
            WebView::Application::the().file_downloader().cancel_download(id);
    }

    void download_added(WebView::FileDownloader::Download const& download) override {
        emit_download(download);
    }
    void download_updated(WebView::FileDownloader::Download const& download) override {
        emit_download(download);
    }

  private:
    void emit_download(WebView::FileDownloader::Download const& download) {
        if (!m_bridge.callback)
            return;
        auto url = to_utf8(download.url);
        auto path = download.destination.string();
        auto filename = AK::ByteString{download.destination.basename()};
        auto error = download.error.has_value() ? download.error->to_byte_string() : AK::ByteString{};
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_DOWNLOAD;
        event.text = url.c_str();
        event.selection = filename.characters();
        event.destination = path.characters();
        event.error = error.is_empty() ? nullptr : error.characters();
        event.download_id = download.id;
        event.bytes_downloaded = download.downloaded_size;
        event.has_total_bytes = download.total_size.has_value();
        event.total_bytes = download.total_size.value_or(0);
        event.download_status = static_cast<ElarisLadybirdDownloadStatus>(static_cast<int>(download.status) + 1);
        m_bridge.callback(m_bridge.context, &event);
    }

    ElarisLadybird& m_bridge;
};

static bool download_path_is_available(AK::LexicalPath const& path) {
    if (FileSystem::exists(path.string()))
        return false;
    for (auto const& download : WebView::Application::the().file_downloader().downloads()) {
        if (WebView::FileDownloader::status_is_active(download.status) &&
            download.destination.string() == path.string())
            return false;
    }
    return true;
}

static ErrorOr<AK::LexicalPath> unique_download_path(AK::ByteString const& directory, AK::ByteString const& file) {
    if (!FileSystem::is_directory(directory))
        return Error::from_errno(ENOENT);

    auto safe_filename = Web::sanitize_suggested_download_filename(file);
    auto destination = AK::LexicalPath::join(directory, safe_filename.view());
    if (download_path_is_available(destination))
        return destination;

    for (u64 index = 1;; ++index) {
        auto candidate_filename = Web::numbered_download_filename(safe_filename, index);
        auto candidate = AK::LexicalPath::join(directory, candidate_filename.view());
        if (download_path_is_available(candidate))
            return candidate;
    }
}

ErrorOr<AK::LexicalPath> ElarisApplication::default_path_for_downloaded_file(AK::ByteString const& file) const {
    if (!s_download_directory_override.has_value())
        return WebView::Application::default_path_for_downloaded_file(file);
    return unique_download_path(*s_download_directory_override, file);
}

Core::EventLoop& ElarisApplication::create_platform_event_loop() {
    if (s_event_loop)
        return *s_event_loop;
    return Core::EventLoop::initialize_for_current_thread();
}

static int32_t fail(char const* message) {
    s_last_error = message;
    return -1;
}

static int32_t succeed() {
    s_last_error.clear();
    return 0;
}

// Ladybird's event loop and views belong to the thread that initialized the bridge; only waking is safe elsewhere.
static bool on_owner_thread(ElarisLadybird const* bridge) {
    return bridge->owner_thread == std::this_thread::get_id();
}

static void emit(ElarisLadybirdView const* view, ElarisLadybirdEventKind kind, std::string text = {},
                 int32_t value = 0) {
    if (!view->bridge->callback)
        return;
    ElarisLadybirdEvent event{};
    event.kind = kind;
    event.view_id = view->id;
    event.text = text.c_str();
    event.value = value;
    view->bridge->callback(view->bridge->context, &event);
}

static void emit_favicon(ElarisLadybirdView const* view, Optional<Gfx::Bitmap const&> favicon) {
    if (!view->bridge->callback)
        return;

    std::vector<uint8_t> rgba;
    ElarisLadybirdEvent event{};
    event.kind = ELARIS_LADYBIRD_FAVICON;
    event.view_id = view->id;
    event.text = "";
    if (favicon.has_value()) {
        auto const& bitmap = *favicon;
        auto const width = bitmap.width();
        auto const height = bitmap.height();
        if (width > 0 && height > 0) {
            auto const row_bytes = static_cast<size_t>(width) * 4;
            rgba.resize(row_bytes * static_cast<size_t>(height));
            for (int y = 0; y < height; ++y) {
                for (int x = 0; x < width; ++x) {
                    auto color = bitmap.get_pixel(x, y);
                    auto alpha = color.alpha();
                    auto straight_alpha = [alpha, premultiplied = bitmap.alpha_type() ==
                                                                  Gfx::AlphaType::Premultiplied](uint8_t channel) {
                        if (alpha == 0)
                            return uint8_t{0};
                        if (!premultiplied)
                            return channel;
                        return static_cast<uint8_t>(std::min(255, (channel * 255 + alpha / 2) / alpha));
                    };
                    auto offset = static_cast<size_t>(y) * row_bytes + static_cast<size_t>(x) * 4;
                    rgba[offset] = straight_alpha(color.red());
                    rgba[offset + 1] = straight_alpha(color.green());
                    rgba[offset + 2] = straight_alpha(color.blue());
                    rgba[offset + 3] = alpha;
                }
            }
            event.favicon_rgba = rgba.data();
            event.favicon_width = width;
            event.favicon_height = height;
            event.favicon_stride = static_cast<int32_t>(row_bytes);
        }
    }
    view->bridge->callback(view->bridge->context, &event);
}

static std::string to_utf8(AK::Utf16String const& value) {
    auto utf8 = value.to_utf8();
    return {reinterpret_cast<char const*>(utf8.bytes().data()), utf8.bytes().size()};
}

static std::string to_utf8(URL::URL const& url) {
    auto utf8 = url.to_string().to_byte_string();
    return {utf8.characters(), utf8.length()};
}

void ElarisApplication::open_navigation_in_new_tab(Web::HTML::PreparedNavigationDescriptor navigation,
                                                   Web::HTML::ActivateTab activate,
                                                   WebView::ViewImplementation const* source) const {
    // Only registered Elaris views receive input. Auxiliary headless views have no Elaris tab.
    if (!host)
        return;
    for (auto* handle : host->views) {
        if (handle->view.ptr() == source) {
            handle->view->on_open_tab(AK::move(navigation), activate);
            return;
        }
    }
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
    s_download_directory_override.clear();

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
    // Ladybird's platform initialization during create() resets the root from its own search, so set it again.
    WebView::s_ladybird_resource_root = resource_dir;
    s_event_loop = &Core::EventLoop::current();
    s_event_loop_thread = std::this_thread::get_id();
    *out_bridge = new ElarisLadybird{
        .callback = callback,
        .context = context,
        .owner_thread = std::this_thread::get_id(),
        .views = {},
        .download_observer = nullptr,
    };
    s_application->host = *out_bridge;
    (*out_bridge)->download_observer = std::make_unique<ElarisDownloadObserver>(**out_bridge);
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_download_directory(ElarisLadybird* bridge, char const* path) {
    if (!bridge)
        return fail("bridge is required");
    if (!on_owner_thread(bridge))
        return fail("download directory must be set on the initialization thread");
    if (!path) {
        s_download_directory_override.clear();
        return succeed();
    }
    AK::LexicalPath directory{AK::ByteString{path}};
    if (!directory.is_absolute())
        return fail("download directory must be an absolute path");
    s_download_directory_override = directory.string();
    return succeed();
}

extern "C" int32_t elaris_ladybird_start_download(ElarisLadybird* bridge, char const* input_url,
                                                  char const* suggested_filename, uint64_t* out_download_id) {
    if (out_download_id)
        *out_download_id = 0;
    if (!bridge || !input_url || !out_download_id)
        return fail("bridge, URL, and download id output are required");
    if (!on_owner_thread(bridge))
        return fail("download must start on the initialization thread");
    auto parsed = URL::Parser::basic_parse(AK::StringView(input_url, std::char_traits<char>::length(input_url)));
    if (!parsed.has_value() || !parsed->scheme().is_one_of("http"sv, "https"sv))
        return fail("download URL must use HTTP or HTTPS");

    auto filename =
        suggested_filename && suggested_filename[0] ? AK::ByteString{suggested_filename} : parsed->basename();
    if (filename.is_empty())
        filename = AK::ByteString{"download"};
    auto destination = s_application->default_path_for_downloaded_file(filename);
    if (destination.is_error())
        return fail("could not select a destination in the Downloads directory");

    *out_download_id = s_application->file_downloader().download_file(
        WebView::IsPrivate::No, parsed.value(), destination.release_value(),
        WebView::FileDownloader::MayReplaceDestination::No);
    return succeed();
}

extern "C" int32_t elaris_ladybird_cancel_download(ElarisLadybird* bridge, uint64_t download_id) {
    if (!bridge)
        return fail("bridge is required");
    if (!on_owner_thread(bridge))
        return fail("download cancellation must run on the initialization thread");
    s_application->file_downloader().cancel_download(download_id);
    return succeed();
}

extern "C" int32_t elaris_ladybird_cancel_active_downloads(ElarisLadybird* bridge) {
    if (!bridge)
        return fail("bridge is required");
    if (!on_owner_thread(bridge))
        return fail("download shutdown must run on the initialization thread");
    Vector<u64> active_ids;
    for (auto const& download : s_application->file_downloader().downloads()) {
        if (WebView::FileDownloader::status_is_active(download.status))
            active_ids.append(download.id);
    }
    for (auto id : active_ids)
        s_application->file_downloader().cancel_download(id);
    return succeed();
}

extern "C" int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, uint64_t view_id,
                                               ElarisLadybirdView** out_view) {
    if (out_view)
        *out_view = nullptr;
    if (!bridge || !out_view || !s_application)
        return fail("invalid bridge or output pointer");
    if (!on_owner_thread(bridge))
        return fail("view creation must run on the initialization thread");
    auto theme_path = AK::LexicalPath::join(WebView::s_ladybird_resource_root, "themes"sv, "Default.ini"sv);
    auto theme = Gfx::load_system_theme(theme_path.string());
    if (theme.is_error())
        return fail("could not load Ladybird's Default.ini theme");

    auto theme_buffer = theme.release_value();
    apply_selection_colors(*theme_buffer.data<Gfx::SystemTheme>());

    auto view = ElarisWebView::create(AK::move(theme_buffer), {1024, 768});
    auto* handle = new ElarisLadybirdView{bridge, view_id, AK::move(view), {}, 0, 0, {}, {}, {}};
    bridge->views.push_back(handle);
    // The callbacks live in the view, which the handle owns, so they never outlive it.
    handle->view->on_url_change = [handle](auto const& url) { emit(handle, ELARIS_LADYBIRD_NAVIGATION, to_utf8(url)); };
    handle->view->on_title_change = [handle](auto const& title) {
        emit(handle, ELARIS_LADYBIRD_TITLE, to_utf8(title));
    };
    handle->view->on_loading_state_change = [handle](bool loading) {
        emit(handle, ELARIS_LADYBIRD_LOADING, {}, loading ? 1 : 0);
    };
    handle->view->on_favicon_change = [handle](Optional<Gfx::Bitmap const&> favicon) { emit_favicon(handle, favicon); };
    handle->view->on_ready_to_paint = [handle] { emit(handle, ELARIS_LADYBIRD_FRAME); };
    handle->view->on_navigation_state_change = [handle] {
        auto const& view = *handle->view;
        emit(handle, ELARIS_LADYBIRD_HISTORY, {},
             (view.can_navigate_back() ? ELARIS_LADYBIRD_HISTORY_BACK : 0) |
                 (view.can_navigate_forward() ? ELARIS_LADYBIRD_HISTORY_FORWARD : 0));
    };
    handle->view->on_top_level_navigation_commit = [handle] {
        emit(handle, ELARIS_LADYBIRD_NAVIGATION_COMMITTED, to_utf8(handle->view->url()));
    };
    handle->view->on_browser_history_traversal_complete = [handle] {
        emit(handle, ELARIS_LADYBIRD_NAVIGATION_COMMITTED, to_utf8(handle->view->url()));
    };
    handle->view->on_find_in_page = [handle](size_t current_match_index, Optional<size_t> const& total_match_count) {
        if (!handle->bridge->callback || handle->pending_find_requests.empty())
            return;
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_FIND_RESULT;
        event.view_id = handle->id;
        event.text = "";
        event.request_id = handle->pending_find_requests.front();
        handle->pending_find_requests.pop_front();
        event.current_match_index = static_cast<uint64_t>(current_match_index);
        event.has_total_match_count = total_match_count.has_value();
        if (total_match_count.has_value())
            event.total_match_count = static_cast<uint64_t>(*total_match_count);
        handle->bridge->callback(handle->bridge->context, &event);
    };
    handle->view->on_open_tab = [handle](auto navigation, auto activate) {
        if (!handle->bridge->callback)
            return;
        auto url = to_utf8(navigation.url);
        auto request = ++handle->next_request;
        handle->pending_tabs.emplace(request, AK::move(navigation));
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_NEW_TAB;
        event.view_id = handle->id;
        event.text = url.c_str();
        event.request_id = request;
        event.value = activate == Web::HTML::ActivateTab::Yes;
        handle->bridge->callback(handle->bridge->context, &event);
    };
    handle->view->on_menu = [handle](WebView::ContextMenuRequest menu) {
        if (!handle->bridge->callback)
            return;
        handle->menu_target =
            menu.kind == WebView::ContextMenuKind::Link ? AK::move(menu.link_navigation) : AK::move(menu.navigation);
        auto url = handle->menu_target.has_value() ? to_utf8(handle->menu_target->url) : std::string{};
        auto selection = menu.selected_text.value_or(String{}).to_byte_string();
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_CONTEXT_MENU;
        event.view_id = handle->id;
        event.request_id = handle->menu_request = ++handle->next_request;
        event.text = url.c_str();
        event.selection = selection.characters();
        event.value = (menu.kind == WebView::ContextMenuKind::Image ? 1 : 0) | (menu.is_editable ? 2 : 0);
        handle->bridge->callback(handle->bridge->context, &event);
    };
    handle->view->on_input_method_state_change = [handle] {
        if (!handle->bridge->callback)
            return;
        auto const& state = handle->view->input_method_state();
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_IME_CARET;
        event.view_id = handle->id;
        event.value = state.is_enabled;
        if (state.caret_rect.has_value()) {
            auto rect = state.caret_rect->to_type<int>();
            event.caret[0] = rect.x();
            event.caret[1] = rect.y();
            event.caret[2] = rect.width();
            event.caret[3] = rect.height();
        }
        handle->bridge->callback(handle->bridge->context, &event);
    };
    *out_view = handle;
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_display_refresh_rate(ElarisLadybirdView* view, double refresh_rate) {
    if (!view || !(refresh_rate > 0.0) || !std::isfinite(refresh_rate))
        return fail("a view and positive finite display refresh rate are required");
    if (!on_owner_thread(view->bridge))
        return fail("display refresh rate must be set on the initialization thread");
    view->view->set_display_metadata({}, refresh_rate, false);
    return succeed();
}

extern "C" int32_t elaris_ladybird_load(ElarisLadybirdView* handle, char const* url) {
    if (!handle || !url)
        return fail("view and URL are required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    auto parsed = URL::Parser::basic_parse(AK::StringView(url, std::char_traits<char>::length(url)));
    if (!parsed.has_value())
        return fail("invalid URL");
    handle->view->load(parsed.release_value());
    return succeed();
}

extern "C" int32_t elaris_ladybird_load_prepared(ElarisLadybirdView* target, uint64_t source, uint64_t request) {
    if (!target || !on_owner_thread(target->bridge))
        return fail("invalid input view");
    for (auto* handle : target->bridge->views) {
        if (handle->id != source)
            continue;
        auto item = handle->pending_tabs.extract(request);
        if (!item.empty())
            target->view->load(AK::move(item.mapped()));
        break;
    }
    return succeed();
}

extern "C" int32_t elaris_ladybird_open_context_target(ElarisLadybirdView* handle, uint64_t request, int32_t new_tab) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("invalid input view");
    if (handle->menu_request == request && handle->menu_target.has_value()) {
        auto navigation = handle->menu_target.release_value();
        if (new_tab)
            handle->view->open_navigation_in_new_tab(AK::move(navigation), Web::HTML::ActivateTab::No);
        else
            handle->view->load(AK::move(navigation));
    }
    return succeed();
}

extern "C" int32_t elaris_ladybird_find(ElarisLadybirdView* handle, char const* query, uint64_t request_id) {
    if (!handle || !query)
        return fail("view and find query are required");
    if (!on_owner_thread(handle->bridge))
        return fail("find requests must run on the initialization thread");
    handle->pending_find_requests.push_back(request_id);
    auto utf8_query = AK::StringView(query, std::char_traits<char>::length(query));
    handle->view->find_in_page(Utf16String::from_utf8(utf8_query), CaseSensitivity::CaseInsensitive, true);
    return succeed();
}

extern "C" int32_t elaris_ladybird_find_next(ElarisLadybirdView* handle, uint64_t request_id) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("find navigation must run on the initialization thread");
    handle->pending_find_requests.push_back(request_id);
    handle->view->find_in_page_next_match();
    return succeed();
}

extern "C" int32_t elaris_ladybird_find_previous(ElarisLadybirdView* handle, uint64_t request_id) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("find navigation must run on the initialization thread");
    handle->pending_find_requests.push_back(request_id);
    handle->view->find_in_page_previous_match();
    return succeed();
}

extern "C" int32_t elaris_ladybird_find_end(ElarisLadybirdView* handle) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("find dismissal must run on the initialization thread");
    handle->view->find_in_page_end();
    return succeed();
}

extern "C" int32_t elaris_ladybird_selected_text(ElarisLadybirdView* handle, uint64_t request_id) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("selected text must be read on the initialization thread");
    auto weak_view = handle->view->make_weak_ptr();
    handle->view->selected_text()->when_resolved(
        [weak_view = AK::move(weak_view), handle, request_id](ByteString const& selection) {
            if (!weak_view || !handle->bridge->callback)
                return;
            ElarisLadybirdEvent event{};
            event.kind = ELARIS_LADYBIRD_SELECTED_TEXT;
            event.view_id = handle->id;
            event.text = selection.is_empty() ? "" : selection.characters();
            event.request_id = request_id;
            handle->bridge->callback(handle->bridge->context, &event);
        });
    return succeed();
}

extern "C" int32_t elaris_ladybird_edit(ElarisLadybirdView* handle, ElarisEdit command, uint8_t const* bytes,
                                        size_t length) {
    if (!handle || !on_owner_thread(handle->bridge) || (!bytes && length))
        return fail("invalid edit");
    auto text = AK::StringView(reinterpret_cast<char const*>(bytes), length);
    auto& view = *handle->view;
    switch (command) {
    case ELARIS_EDIT_COPY:
    case ELARIS_EDIT_CUT: {
        auto promise = command == ELARIS_EDIT_COPY ? view.selected_text() : view.cut_selected_text();
        promise->when_resolved([weak = view.make_weak_ptr(), handle](ByteString const& selection) {
            if (weak && !selection.is_empty())
                emit(handle, ELARIS_LADYBIRD_CLIPBOARD, {selection.characters(), selection.length()});
        });
        break;
    }
    case ELARIS_EDIT_PASTE: {
        auto string = String::from_utf8(text);
        if (string.is_error())
            return fail("invalid paste text");
        s_application->set_clipboard_text(string.release_value());
        view.paste_from_clipboard(Web::Clipboard::PasteMode::PlainTextOnly);
        break;
    }
    case ELARIS_EDIT_SELECT_ALL:
        view.select_all();
        break;
    case ELARIS_EDIT_MARK:
        view.set_marked_text_from_input_method(Utf16String::from_utf8(text));
        break;
    case ELARIS_EDIT_COMMIT:
        view.commit_text_from_input_method(Utf16String::from_utf8(text));
        break;
    default:
        return fail("unknown edit");
    }
    return succeed();
}

extern "C" int32_t elaris_ladybird_reload(ElarisLadybirdView* handle) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    handle->view->reload();
    return succeed();
}

extern "C" int32_t elaris_ladybird_stop_loading(ElarisLadybirdView* handle) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    handle->view->stop_loading();
    return succeed();
}

extern "C" int32_t elaris_ladybird_traverse_history(ElarisLadybirdView* handle, int32_t delta) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    handle->view->traverse_the_history_by_delta(delta);
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_visible(ElarisLadybirdView* handle, int32_t visible) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("visibility must change on the initialization thread");
    handle->view->set_system_visibility_state(visible ? Web::HTML::VisibilityState::Visible
                                                      : Web::HTML::VisibilityState::Hidden);
    return succeed();
}

extern "C" int32_t elaris_ladybird_resize(ElarisLadybirdView* handle, int32_t width, int32_t height,
                                          double device_pixel_ratio) {
    if (!handle || width <= 0 || height <= 0 || !(device_pixel_ratio > 0))
        return fail("view, positive dimensions and a positive device pixel ratio are required");
    if (!on_owner_thread(handle->bridge))
        return fail("resize must run on the initialization thread");
    handle->view->set_viewport({width, height}, device_pixel_ratio);
    return succeed();
}

// The displayed frame's CPU bitmap. Backing stores are reused, so the bitmap can be larger than the frame.
static RefPtr<Gfx::Bitmap> displayed_bitmap(ElarisLadybirdView const& handle, Gfx::IntSize& size,
                                            double& device_pixel_ratio) {
    auto frame = handle.view->backing_stores().displayed_frame();
    if (!frame.has_value() || !frame->shared_image_buffer || frame->size.is_empty())
        return nullptr;
    auto bitmap = frame->shared_image_buffer->bitmap_if_present();
    if (!bitmap)
        return nullptr;
    size = {std::min(frame->size.width(), bitmap->width()), std::min(frame->size.height(), bitmap->height())};
    device_pixel_ratio = frame->device_pixel_ratio;
    return bitmap;
}

extern "C" int32_t elaris_ladybird_frame(ElarisLadybirdView* handle, ElarisLadybirdFrame* out_frame) {
    if (!handle || !out_frame)
        return fail("view and output frame are required");
    if (!on_owner_thread(handle->bridge))
        return fail("frames must be read on the initialization thread");
    Gfx::IntSize size;
    double device_pixel_ratio = 1;
    if (!displayed_bitmap(*handle, size, device_pixel_ratio)) {
        s_last_error.clear();
        return ELARIS_LADYBIRD_NO_FRAME;
    }
    auto damage = size == handle->copied_size ? handle->view->damage().intersected({{}, size}) : Gfx::IntRect{{}, size};
    *out_frame = {size.width(),       size.height(),
                  size.width() * 4,   ELARIS_LADYBIRD_PIXEL_FORMAT_RGBA8,
                  device_pixel_ratio, damage.x(),
                  damage.y(),         damage.width(),
                  damage.height(),    handle->view->serial()};
    return succeed();
}

extern "C" int32_t elaris_ladybird_copy_frame(ElarisLadybirdView* handle, uint8_t* rgba, size_t length) {
    if (!handle || !rgba)
        return fail("view and pixel buffer are required");
    if (!on_owner_thread(handle->bridge))
        return fail("frames must be read on the initialization thread");
    Gfx::IntSize size;
    double device_pixel_ratio = 1;
    auto bitmap = displayed_bitmap(*handle, size, device_pixel_ratio);
    if (!bitmap)
        return fail("the view has no frame to copy");
    auto row_length = static_cast<size_t>(size.width()) * 4;
    if (length != row_length * static_cast<size_t>(size.height()))
        return fail("pixel buffer does not match the displayed frame");

    // Whole pixels at a time: the source is BGRx or RGBx in native order; the host wants RGBA bytes.
    auto red_is_low_byte =
        bitmap->format() == Gfx::BitmapFormat::RGBA8888 || bitmap->format() == Gfx::BitmapFormat::RGBx8888;
    for (int y = 0; y < size.height(); ++y) {
        auto const* source = bitmap->scanline(y);
        auto* destination = rgba + static_cast<size_t>(y) * row_length;
        for (int x = 0; x < size.width(); ++x) {
            auto pixel = source[x];
            if (!red_is_low_byte)
                pixel = (pixel & 0x0000ff00U) | ((pixel >> 16) & 0xffU) | ((pixel & 0xffU) << 16);
            pixel |= 0xff000000U;
            std::memcpy(destination + static_cast<size_t>(x) * 4, &pixel, sizeof(pixel));
        }
    }
    handle->copied_size = size;
    handle->view->clear_damage();
    return succeed();
}

static Web::UIEvents::KeyModifier to_modifiers(int32_t modifiers) {
    auto result = Web::UIEvents::KeyModifier::Mod_None;
    if (modifiers & ELARIS_LADYBIRD_MOD_ALT)
        result |= Web::UIEvents::KeyModifier::Mod_Alt;
    if (modifiers & ELARIS_LADYBIRD_MOD_CTRL)
        result |= Web::UIEvents::KeyModifier::Mod_Ctrl;
    if (modifiers & ELARIS_LADYBIRD_MOD_SHIFT)
        result |= Web::UIEvents::KeyModifier::Mod_Shift;
    if (modifiers & ELARIS_LADYBIRD_MOD_SUPER)
        result |= Web::UIEvents::KeyModifier::Mod_Super;
    return result;
}

static Web::UIEvents::MouseButton to_buttons(int32_t buttons) {
    auto result = Web::UIEvents::MouseButton::None;
    if (buttons & ELARIS_LADYBIRD_BUTTON_PRIMARY)
        result |= Web::UIEvents::MouseButton::Primary;
    if (buttons & ELARIS_LADYBIRD_BUTTON_SECONDARY)
        result |= Web::UIEvents::MouseButton::Secondary;
    if (buttons & ELARIS_LADYBIRD_BUTTON_MIDDLE)
        result |= Web::UIEvents::MouseButton::Middle;
    return result;
}

extern "C" int32_t elaris_ladybird_mouse_event(ElarisLadybirdView* handle, ElarisLadybirdMouseEvent const* event) {
    if (!handle || !event)
        return fail("view and mouse event are required");
    if (!on_owner_thread(handle->bridge))
        return fail("input must be sent on the initialization thread");

    Web::MouseEvent::Type type;
    switch (event->type) {
    case ELARIS_LADYBIRD_MOUSE_DOWN:
        type = Web::MouseEvent::Type::MouseDown;
        break;
    case ELARIS_LADYBIRD_MOUSE_UP:
        type = Web::MouseEvent::Type::MouseUp;
        break;
    case ELARIS_LADYBIRD_MOUSE_MOVE:
        type = Web::MouseEvent::Type::MouseMove;
        break;
    case ELARIS_LADYBIRD_MOUSE_CANCEL:
        type = Web::MouseEvent::Type::MouseCancel;
        break;
    case ELARIS_LADYBIRD_MOUSE_LEAVE:
        type = Web::MouseEvent::Type::MouseLeave;
        break;
    case ELARIS_LADYBIRD_MOUSE_WHEEL:
        type = Web::MouseEvent::Type::MouseWheel;
        break;
    default:
        return fail("unknown mouse event type");
    }

    Web::ScrollGesturePhase phase;
    switch (event->type == ELARIS_LADYBIRD_MOUSE_WHEEL ? event->wheel_phase : ELARIS_LADYBIRD_WHEEL_PHASE_NONE) {
    case ELARIS_LADYBIRD_WHEEL_PHASE_NONE:
        phase = Web::ScrollGesturePhase::None;
        break;
    case ELARIS_LADYBIRD_WHEEL_PHASE_ONGOING:
        phase = Web::ScrollGesturePhase::Ongoing;
        break;
    case ELARIS_LADYBIRD_WHEEL_PHASE_MOMENTUM:
        phase = Web::ScrollGesturePhase::Momentum;
        break;
    case ELARIS_LADYBIRD_WHEEL_PHASE_ENDED:
        phase = Web::ScrollGesturePhase::Ended;
        break;
    default:
        return fail("unknown wheel phase");
    }

    auto precision = event->wheel_precise ? Web::WheelDeltaPrecision::Precise : Web::WheelDeltaPrecision::Discrete;
    Web::DevicePixelPoint position{event->x, event->y};
    handle->view->enqueue_input_event(Web::MouseEvent{
        type,
        position,
        position,
        to_buttons(event->button),
        to_buttons(event->buttons),
        to_modifiers(event->modifiers),
        event->wheel_delta_x,
        event->wheel_delta_y,
        Web::wheel_tick_120ths_from_pixel_delta(event->wheel_delta_x, precision),
        Web::wheel_tick_120ths_from_pixel_delta(event->wheel_delta_y, precision),
        precision,
        phase,
        event->click_count,
        nullptr,
    });
    return succeed();
}

extern "C" int32_t elaris_ladybird_key_event(ElarisLadybirdView* handle, ElarisLadybirdKeyEvent const* event) {
    if (!handle || !event)
        return fail("view and key event are required");
    if (!on_owner_thread(handle->bridge))
        return fail("input must be sent on the initialization thread");
    if (event->type != ELARIS_LADYBIRD_KEY_PRESS && event->type != ELARIS_LADYBIRD_KEY_RELEASE)
        return fail("unknown key event type");

    auto key = Web::UIEvents::KeyCode::Key_Invalid;
    u32 code_point = 0;
    switch (event->key) {
    case ELARIS_LADYBIRD_KEY_CHARACTER:
        key = Web::UIEvents::code_point_to_key_code(event->code_point);
        code_point = event->code_point;
        break;
    case ELARIS_LADYBIRD_KEY_RETURN:
        key = Web::UIEvents::KeyCode::Key_Return;
        code_point = '\n';
        break;
    case ELARIS_LADYBIRD_KEY_BACKSPACE:
        key = Web::UIEvents::KeyCode::Key_Backspace;
        break;
    case ELARIS_LADYBIRD_KEY_DELETE:
        key = Web::UIEvents::KeyCode::Key_Delete;
        break;
    case ELARIS_LADYBIRD_KEY_TAB:
        key = Web::UIEvents::KeyCode::Key_Tab;
        code_point = '\t';
        break;
    case ELARIS_LADYBIRD_KEY_ESCAPE:
        key = Web::UIEvents::KeyCode::Key_Escape;
        break;
    case ELARIS_LADYBIRD_KEY_LEFT:
        key = Web::UIEvents::KeyCode::Key_Left;
        break;
    case ELARIS_LADYBIRD_KEY_RIGHT:
        key = Web::UIEvents::KeyCode::Key_Right;
        break;
    case ELARIS_LADYBIRD_KEY_UP:
        key = Web::UIEvents::KeyCode::Key_Up;
        break;
    case ELARIS_LADYBIRD_KEY_DOWN:
        key = Web::UIEvents::KeyCode::Key_Down;
        break;
    case ELARIS_LADYBIRD_KEY_HOME:
        key = Web::UIEvents::KeyCode::Key_Home;
        break;
    case ELARIS_LADYBIRD_KEY_END:
        key = Web::UIEvents::KeyCode::Key_End;
        break;
    case ELARIS_LADYBIRD_KEY_PAGE_UP:
        key = Web::UIEvents::KeyCode::Key_PageUp;
        break;
    case ELARIS_LADYBIRD_KEY_INSERT:
        key = Web::UIEvents::KeyCode::Key_Insert;
        break;
    case ELARIS_LADYBIRD_KEY_PRINTSCREEN:
        key = Web::UIEvents::KeyCode::Key_PrintScreen;
        break;
    case ELARIS_LADYBIRD_KEY_SCROLLLOCK:
        key = Web::UIEvents::KeyCode::Key_ScrollLock;
        break;
    case ELARIS_LADYBIRD_KEY_PAUSE:
        key = Web::UIEvents::KeyCode::Key_PauseBreak;
        break;
    case ELARIS_LADYBIRD_KEY_SHIFTLEFT:
        key = Web::UIEvents::KeyCode::Key_LeftShift;
        break;
    case ELARIS_LADYBIRD_KEY_SHIFTRIGHT:
        key = Web::UIEvents::KeyCode::Key_RightShift;
        break;
    case ELARIS_LADYBIRD_KEY_CONTROLLEFT:
        key = Web::UIEvents::KeyCode::Key_LeftControl;
        break;
    case ELARIS_LADYBIRD_KEY_CONTROLRIGHT:
        key = Web::UIEvents::KeyCode::Key_RightControl;
        break;
    case ELARIS_LADYBIRD_KEY_ALTLEFT:
        key = Web::UIEvents::KeyCode::Key_LeftAlt;
        break;
    case ELARIS_LADYBIRD_KEY_ALTRIGHT:
        key = Web::UIEvents::KeyCode::Key_RightAlt;
        break;
    case ELARIS_LADYBIRD_KEY_SUPERLEFT:
        key = Web::UIEvents::KeyCode::Key_LeftSuper;
        break;
    case ELARIS_LADYBIRD_KEY_SUPERRIGHT:
        key = Web::UIEvents::KeyCode::Key_RightSuper;
        break;
    case ELARIS_LADYBIRD_KEY_PAGE_DOWN:
        key = Web::UIEvents::KeyCode::Key_PageDown;
        break;
    default:
        if (event->key < ELARIS_LADYBIRD_KEY_F1 || event->key > ELARIS_LADYBIRD_KEY_F12)
            return fail("unknown key");
        key = static_cast<Web::UIEvents::KeyCode>(static_cast<int>(Web::UIEvents::KeyCode::Key_F1) + event->key -
                                                  ELARIS_LADYBIRD_KEY_F1);
        break;
    }

    auto type = event->type == ELARIS_LADYBIRD_KEY_PRESS ? Web::KeyEvent::Type::KeyDown : Web::KeyEvent::Type::KeyUp;
    auto modifiers = to_modifiers(event->modifiers);
    // Shortcut chords (Ctrl or Command held) do not type their character.
    auto is_shortcut = (event->modifiers & (ELARIS_LADYBIRD_MOD_CTRL | ELARIS_LADYBIRD_MOD_SUPER)) != 0;
    auto should_insert_text = type == Web::KeyEvent::Type::KeyDown && code_point != 0 && !is_shortcut;
    handle->view->enqueue_input_event(
        Web::KeyEvent{type, key, modifiers, code_point, event->repeat != 0, should_insert_text, nullptr});
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_focus(ElarisLadybirdView* handle, int32_t focused) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("focus must change on the initialization thread");
    handle->view->set_has_system_focus(focused != 0);
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_color_scheme(ElarisLadybird* bridge, int32_t dark) {
    if (!bridge || !s_application)
        return fail("Ladybird is not initialized");
    if (!on_owner_thread(bridge))
        return fail("the colour scheme must change on the initialization thread");
    // A view's own scheme is replaced by this setting whenever a page attaches to it, so the setting is what lasts.
    auto& settings = WebView::Application::settings();
    auto appearance = settings.appearance();
    auto scheme = dark ? WebView::ThemePreference::Dark : WebView::ThemePreference::Light;
    if (appearance.website_color_scheme != scheme) {
        appearance.website_color_scheme = scheme;
        settings.set_appearance(appearance);
    }
    return succeed();
}

extern "C" int32_t elaris_ladybird_pump(ElarisLadybird* bridge) {
    if (!bridge || !s_application)
        return fail("Ladybird is not initialized");
    if (!on_owner_thread(bridge))
        return fail("event loop must be pumped on the initialization thread");
    s_event_loop->pump(Core::EventLoop::WaitMode::PollForEvents);
    return succeed();
}

extern "C" int32_t elaris_ladybird_wait(ElarisLadybird* bridge, int32_t timeout_us) {
    if (!bridge || !s_application)
        return fail("Ladybird is not initialized");
    if (!on_owner_thread(bridge))
        return fail("event loop must be pumped on the initialization thread");
    // A timer bounds the blocking pump, so it returns by the timeout even with nothing else to do.
    RefPtr<Core::Timer> timer;
    if (timeout_us >= 0) {
        timer = Core::Timer::create_single_shot(AK::Duration::from_microseconds(timeout_us), [] {});
        timer->start();
    }
    s_event_loop->pump(Core::EventLoop::WaitMode::WaitForEvents);
    if (timer)
        timer->stop();
    return succeed();
}

struct ElarisLadybirdWaker {
    NonnullRefPtr<Core::WeakEventLoopReference> event_loop;
};

extern "C" ElarisLadybirdWaker* elaris_ladybird_create_waker(ElarisLadybird* bridge) {
    if (!bridge || !s_application || !on_owner_thread(bridge)) {
        fail("a waker must be created on the initialization thread");
        return nullptr;
    }
    return new ElarisLadybirdWaker{Core::EventLoop::current_weak()};
}

extern "C" void elaris_ladybird_wake(ElarisLadybirdWaker* waker) {
    if (!waker)
        return;
    // The weak reference is revoked when the event loop goes, so this is safe from any thread at any time.
    if (auto event_loop = waker->event_loop->take(); event_loop.is_alive())
        event_loop->wake();
}

extern "C" void elaris_ladybird_destroy_waker(ElarisLadybirdWaker* waker) {
    delete waker;
}

extern "C" void elaris_ladybird_destroy_view(ElarisLadybirdView* handle) {
    if (!handle)
        return;
    if (!on_owner_thread(handle->bridge)) {
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
    if (!on_owner_thread(bridge)) {
        fail("shutdown must run on the initialization thread");
        return;
    }
    (void)elaris_ladybird_cancel_active_downloads(bridge);
    bridge->download_observer.reset();
    for (auto* view : bridge->views) {
        view->view->request_close();
        delete view;
    }
    bridge->views.clear();
    delete bridge;
    s_application = nullptr;
    s_download_directory_override.clear();
}

extern "C" char const* elaris_ladybird_last_error(void) {
    return s_last_error.c_str();
}
