#include <cmath>
#ifdef ELARIS_BUILD_SMOKE_TESTS
#include <LibWebView/RequestServerManager.h>

#include "../tests/private_session.h"
#endif

#include <elaris_ladybird.h>

#include <AK/ByteString.h>
#include <AK/JsonArray.h>
#include <AK/JsonObject.h>
#include <AK/LexicalPath.h>
#include <AK/OwnPtr.h>
#include <AK/String.h>
#include <AK/Utf16String.h>

#include <LibCore/EventLoop.h>
#include <LibCore/GeolocationProvider.h>
#include <LibCore/Promise.h>
#include <LibCore/StandardPaths.h>
#include <LibCore/Timer.h>
#include <LibFileSystem/FileSystem.h>
#include <LibGfx/Bitmap.h>
#include <LibGfx/SharedImageBuffer.h>
#include <LibGfx/SystemTheme.h>
#include <LibURL/Parser.h>
#include <LibURL/URL.h>
#include <LibWebCommon/HTML/ColorPickerUpdateState.h>
#include <LibWebCommon/Loader/DownloadFilename.h>
#include <LibWebCommon/Page/InputEvent.h>
#include <LibWebCommon/PixelUnits.h>
#include <LibWebCommon/UIEvents/KeyCode.h>
#include <LibWebCommon/WebView/AccessibilityNodeData.h>
#include <LibWebCommon/WebView/Utilities.h>
#include <LibWebView/Application.h>
#include <LibWebView/BrowsingSession.h>
#include <LibWebView/CanonicalDocument.h>
#include <LibWebView/CanonicalTraversable.h>
#include <LibWebView/ContextMenuRequest.h>
#include <LibWebView/FileDownloader.h>
#include <LibWebView/HeadlessWebView.h>
#include <LibWebView/PageBackingStores.h>
#include <LibWebView/PermissionManager.h>
#include <LibWebView/Settings.h>
#include <LibWebView/Utilities.h>
#if defined(AK_OS_MACOS)
#include <UI/Qt/MicrophoneAuthorizerMacOS.h>

#include "frame_blitter_macos.h"
#include "notification_backend_macos.h"
#include "spell_checker_macos.h"
#elif defined(AK_OS_LINUX)
#include <LibWebView/NotificationBackendFreedesktop.h>
#endif

#include <algorithm>
#include <array>
#include <cerrno>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <map>
#include <unistd.h>
#include <memory>
#include <set>
#include <string>
#include <string_view>
#include <thread>
#include <utility>
#include <vector>

#include "external_url_handler.h"
#include "peek_disposition.h"

#if defined(AK_OS_MACOS)
#include <memory>
#include <vector>

#include "media_controls.h"
#include "picture_in_picture.h"
#endif
#include <LibCompositing/PictureInPictureControls.h>
#include <LibWebView/PictureInPictureManager.h>

static Core::EventLoop* s_event_loop;
static std::thread::id s_event_loop_thread;
static Optional<AK::ByteString> s_download_directory_override;

static ErrorOr<AK::LexicalPath> unique_download_path(AK::ByteString const& directory, AK::ByteString const& file);
static std::string to_utf8(URL::URL const& url);

// An opt-in location source for the loopback permission smoke page.
class TestLocationProvider final : public Core::GeolocationProvider {
    AK_ALLOC_WITH_KMALLOC;

  public:
    ErrorOr<RequestId, Core::GeolocationError> request_current_position(SuccessCallback success,
                                                                        ErrorCallback) override {
        auto id = ++m_next_id;
        auto timer = Core::Timer::create_single_shot(AK::Duration::from_milliseconds(1),
                                                     [this, id, success = AK::move(success)] {
                                                         auto timer = m_pending.extract(id);
                                                         success({51.5, -0.12, 100.0, {}, {}, {}, {}});
                                                     });
        timer->start();
        m_pending.emplace(id, AK::move(timer));
        return id;
    }

    void cancel_current_position_request(RequestId id) override {
        m_pending.erase(id);
    }

    ErrorOr<WatchId, Core::GeolocationError> start_watching_position(SuccessCallback success,
                                                                     ErrorCallback error) override {
        return request_current_position(AK::move(success), AK::move(error));
    }

    void stop_watching_position(WatchId id) override {
        cancel_current_position_request(id);
    }
    Core::GeolocationAuthorization authorization() const override {
        return Core::GeolocationAuthorization::Authorized;
    }

  private:
    RequestId m_next_id{};
    std::map<RequestId, NonnullRefPtr<Core::Timer>> m_pending;
};

#if defined(AK_OS_LINUX)
// Whether the host asked for frames in GPU memory (`ELARIS_GPU_FRAMES`, which the engine reads as well).
static bool gpu_frames_requested() {
    auto const* value = getenv("ELARIS_GPU_FRAMES");
    return value && std::string_view(value) != "0";
}
#endif

class ElarisApplication final : public WebView::Application {
    WEB_VIEW_APPLICATION(ElarisApplication)

  public:
    ElarisLadybird* host{};
    explicit ElarisApplication(Optional<AK::ByteString> helper_directory = {})
        : WebView::Application(AK::move(helper_directory)) {
#if defined(AK_OS_MACOS)
        set_spell_checker(Elaris::make_spell_checker_macos());
#endif
    }
    // The host process is Elaris, not a Ladybird browser, so its own crashes are never captured or reported.
    bool captures_browser_crashes() const override { return false; }
    NonnullOwnPtr<WebView::NotificationBackend> create_notification_backend() override {
#if defined(AK_OS_MACOS)
        return Elaris::make_notification_backend_macos();
#elif defined(AK_OS_LINUX)
        return make<WebView::NotificationBackendFreedesktop>();
#else
        return WebView::Application::create_notification_backend();
#endif
    }

    ErrorOr<AK::LexicalPath> default_path_for_downloaded_file(AK::ByteString const& file) const override;
#if defined(AK_OS_MACOS) || defined(AK_OS_LINUX)
    void resolve_external_url_handler(URL::URL const& url,
                                      WebView::ExternalURLHandlerCallback callback) const override {
        Elaris::resolve_external_url_handler(url, AK::move(callback));
    }
#endif
    void display_error_dialog(AK::StringView message) const override;

    bool supports_picture_in_picture() const override {
#if defined(AK_OS_MACOS)
        return true;
#else
        return false;
#endif
    }
    AK::OwnPtr<WebView::PictureInPictureWindow> create_picture_in_picture_window(WebView::WebContentPage& requesting_page,
                                                                                 WebView::CanonicalTraversable& traversable,
                                                                                 Gfx::IntSize video_size) override;

  private:
#if defined(AK_OS_MACOS)
    NonnullOwnPtr<WebView::MicrophoneAuthorizer> create_microphone_authorizer() override {
        return make<Ladybird::MicrophoneAuthorizerMacOS>();
    }
#endif

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

#if defined(AK_OS_LINUX)
    // Rune imports only linear dmabufs, so that is the only layout the Compositor may give a frame when frames are
    // shared with the host's GPU.
    Vector<u64> platform_presentation_drm_format_modifiers() const override {
        if (!gpu_frames_requested())
            return {};
        return {0 /* DRM_FORMAT_MOD_LINEAR */};
    }
#endif
};

#if defined(AK_OS_MACOS)
// Work that another thread hands to the owner thread. The owner clears `live` before it tears down what the work uses,
// so work that runs later does nothing.
struct OwnerLink {
    NonnullRefPtr<Core::WeakEventLoopReference> loop = Core::EventLoop::current_weak();
    bool live{true};
};

static void post_to_owner(std::shared_ptr<OwnerLink> const& link, AK::Function<void()> work) {
    auto owner = link->loop->take();
    if (!owner)
        return;
    owner->deferred_invoke([link, work = AK::move(work)] {
        if (link->live)
            work();
    });
    owner->wake();
}
#endif

// A headless view whose frames the host reads from CPU memory, as Ladybird's TerminalWebView does.
class ElarisWebView final : public WebView::HeadlessWebView {
  public:
    static AK::NonnullOwnPtr<ElarisWebView> create(Core::AnonymousBuffer theme, Web::DevicePixelSize size,
                                                   WebView::IsPrivate mode, WebView::CanonicalTraversable* popup) {
        auto view = AK::adopt_own(*new ElarisWebView(AK::move(theme), size, mode));
        if (popup)
            view->initialize_tab(Web::HTML::VisibilityState::Visible, *popup);
        else
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

#if defined(AK_OS_LINUX)
    // Presents frames in dmabufs for the host's GPU to read in place. The Compositor may paint into a backing store
    // again only once the host has read the frame in it, which the host reports.
    bool set_gpu_frames(bool enabled) {
        m_gpu_frames = enabled;
        set_presents_from_cpu_memory(!enabled);
        if (!enabled)
            release_read(UINT64_MAX);
        return true;
    }
    bool gpu_frames() const {
        return m_gpu_frames;
    }

    // Lets the Compositor paint into the backing stores of frames up to `read_serial` again.
    void release_read(uint64_t read_serial) {
        m_read_serial = std::max(m_read_serial, read_serial);
        auto const done = [&](auto const& held) { return held.frame_serial <= m_read_serial; };
        for (auto const& held : m_held_stores) {
            if (done(held))
                ViewImplementation::release_backing_store(held.bitmap_id);
        }
        std::erase_if(m_held_stores, done);
    }

    uint64_t frame_serial_of_store(i32 bitmap_id) const {
        auto it = m_store_serials.find(bitmap_id);
        return it == m_store_serials.end() ? 0 : it->second;
    }
#endif
#if defined(AK_OS_MACOS)
    // Shows frames to the host as GPU surfaces: each is copied on the GPU into a surface of the blitter's, which the
    // host samples in place. The backing stores themselves are released as they would be for CPU reads.
    bool set_gpu_frames(bool enabled) {
        if (!enabled) {
            m_blitter.reset();
            return true;
        }
        if (!m_blitter)
            m_blitter = Elaris::FrameBlitter::create();
        return m_blitter != nullptr;
    }
    Elaris::FrameBlitter* blitter() const {
        return m_blitter.get();
    }
    bool gpu_frames() const {
        return m_blitter != nullptr;
    }
#endif

    // Called when whether the view can go back or forward changes.
    AK::Function<void()> on_navigation_state_change;
    AK::Function<void(WebView::ContextMenuRequest)> on_menu;
    AK::Function<void(Web::HTML::PreparedNavigationDescriptor, Web::HTML::ActivateTab)> on_open_tab;
    // Called on the owner thread each time the view presents a frame.
    AK::Function<void()> on_presented;

    void set_owner(WebView::ViewImplementation const& owner) {
        set_owner_view(owner);
    }

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

    ElarisWebView(Core::AnonymousBuffer theme, Web::DevicePixelSize size, WebView::IsPrivate mode)
        : HeadlessWebView(AK::move(theme), size, mode) {}

#if defined(AK_OS_LINUX)
    void release_backing_store(i32 bitmap_id) override {
        // A replaced backing store is free once the host read the frame it held. A frame the host never drew is
        // covered by the next one it read.
        auto frame_serial = frame_serial_of_store(bitmap_id);
        if (m_gpu_frames && bitmap_id >= 0 && frame_serial > m_read_serial)
            m_held_stores.push_back({bitmap_id, frame_serial});
        else
            ViewImplementation::release_backing_store(bitmap_id);
    }
#endif

    void did_accept_presented_backing_store(i32 bitmap_id, Gfx::IntRect damage_rect) override {
        m_damage = m_damage.is_empty() ? damage_rect : m_damage.united(damage_rect);
        ++m_serial;
#if defined(AK_OS_LINUX)
        m_store_serials[bitmap_id] = m_serial;
#else
        (void)bitmap_id;
#endif
        if (on_presented)
            on_presented();
    }

    Gfx::IntRect m_damage;
    uint64_t m_serial{};
#if defined(AK_OS_MACOS)
    std::unique_ptr<Elaris::FrameBlitter> m_blitter;
#endif
#if defined(AK_OS_LINUX)
    struct HeldStore {
        i32 bitmap_id;
        uint64_t frame_serial;
    };
    bool m_gpu_frames{};
    uint64_t m_read_serial{};
    // The newest frame each backing store showed, and the stores waiting for the host to read theirs.
    std::map<i32, uint64_t> m_store_serials;
    std::vector<HeldStore> m_held_stores;
#endif
};

class ElarisDownloadObserver;

struct ElarisLadybird {
    ElarisLadybirdEventCallback callback{};
    void* context{};
    std::thread::id owner_thread;
    std::vector<ElarisLadybirdView*> views;
    std::unique_ptr<ElarisDownloadObserver> download_observer;
    uint64_t next_picker_request{};
    bool permissions_initialized{};
    // Elaris's browser defaults. Ladybird only prompts when a site has no decision, so a blocked default
    // turns that prompt into a silent refusal and leaves explicit site exceptions in charge.
    std::set<Web::Permissions::PermissionName> blocked_by_default;
    // The view with system focus, which receives application-level error notices.
    ElarisLadybirdView* focused_view{};
#if defined(AK_OS_MACOS)
    // Media keys and Now Playing. Commands reach the owner thread only while `media_link` is live.
    std::shared_ptr<OwnerLink> media_link;
    AK::OwnPtr<Elaris::MediaControls> media_controls;
#endif
};

struct PendingPageDialog {
    uint64_t id{};
    ElarisLadybirdDialogKind kind{};
    std::string message;
    Utf16String prompt_text;
    AK::Function<void(bool)> before_unload;
};

enum class FormPicker { None, File, Color, Date, Select };

struct PendingPermission {
    uint64_t id;
    Web::HTML::CrossProcessId document;
    WebView::PermissionPromptRequest request;
    uint64_t generation;
    WebView::PermissionPromptCallback reply;
    std::string requesting_origin;
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
    PendingPageDialog dialog;
    FormPicker picker{FormPicker::None};
    uint64_t picker_id{};
    Optional<Web::HTML::CrossProcessId> picker_document;
    std::vector<uint32_t> picker_options;
    bool picker_allows_multiple{};
    double page_zoom{1.0};
    bool web_fullscreen{};
    std::deque<PendingPermission> permissions{};
    ElarisLadybirdView* popup_source{};
    uint64_t popup_request{};
    bool popup_closed{};
    // Ladybird asks about one external link at a time per view and waits for this reply.
    struct {
        uint64_t id{};
        int32_t kind{};
        std::string url;
        std::string origin;
        std::string application;
        AK::Function<void(bool)> reply;
    } external_url{};
    bool accessibility{};
    // A Picture-in-Picture window's own view. Rust never sees it, and media keys leave it out.
    bool picture_in_picture{};
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
        // Ladybird restores the resumable downloads a previous run paused; report them so the
        // host lists them, paused, for the user to resume or cancel.
        for (auto const& download : WebView::Application::the().file_downloader().downloads())
            emit_download(download);
    }

    void download_added(WebView::FileDownloader::Download const& download) override {
        emit_download(download);
    }
    void download_updated(WebView::FileDownloader::Download const& download) override {
        emit_download(download);
    }

  private:
    void emit_download(WebView::FileDownloader::Download const& download) {
        // Value 1 marks a private window's download, which only private windows list.
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
        // Bit 1: a private window's download. Bit 2: it can resume. Bit 4: it waits to retry.
        event.value = (download.is_private == WebView::IsPrivate::Yes ? 1 : 0) | (download.can_resume ? 2 : 0) |
                      (download.is_waiting_to_retry ? 4 : 0);
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

static void set_web_fullscreen(ElarisLadybirdView& handle, bool fullscreen) {
    if (std::exchange(handle.web_fullscreen, fullscreen) == fullscreen)
        return;
    handle.view->set_is_fullscreen(fullscreen ? Web::ViewportIsFullscreen::Yes : Web::ViewportIsFullscreen::No);
    emit(&handle, ELARIS_LADYBIRD_WEB_FULLSCREEN, {}, fullscreen);
}

extern "C" int32_t elaris_ladybird_exit_web_fullscreen(ElarisLadybirdView* handle) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("fullscreen exit requires the view owner thread");
    if (handle->web_fullscreen)
        handle->view->exit_fullscreen();
    return succeed();
}

static void emit_external_url(ElarisLadybirdView const& handle) {
    if (!handle.bridge->callback)
        return;
    auto const& pending = handle.external_url;
    ElarisLadybirdEvent event{};
    event.kind = ELARIS_LADYBIRD_EXTERNAL_URL;
    event.view_id = handle.id;
    event.request_id = pending.id;
    event.value = pending.kind;
    event.text = pending.url.c_str();
    event.destination = pending.origin.c_str();
    event.selection = pending.application.c_str();
    handle.bridge->callback(handle.bridge->context, &event);
}

static void show_external_url(ElarisLadybirdView& handle, int32_t kind, std::string url, std::string origin,
                              std::string application, AK::Function<void(bool)> reply) {
    if (auto previous = std::exchange(handle.external_url.reply, {}))
        previous(false);
    handle.external_url = {++handle.next_request, kind,           AK::move(url), AK::move(origin),
                           AK::move(application), AK::move(reply)};
    emit_external_url(handle);
}

// A navigation, crash or close declines the open request and removes any notice.
static void cancel_external_url(ElarisLadybirdView& handle) {
    if (!handle.external_url.kind)
        return;
    auto reply = std::exchange(handle.external_url.reply, {});
    handle.external_url = {};
    if (reply)
        reply(false);
    emit_external_url(handle);
}

extern "C" int32_t elaris_ladybird_reply_to_external_url(ElarisLadybirdView* handle, uint64_t request_id,
                                                         int32_t open) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("external link replies require the view owner thread");
    if (!request_id || handle->external_url.id != request_id)
        return succeed();
    auto reply = std::exchange(handle->external_url.reply, {});
    handle->external_url = {};
    emit_external_url(*handle);
    if (reply)
        reply(open == 1);
    return succeed();
}

static AK::JsonObject accessibility_node_json(WebView::AccessibilityNodeData const& node) {
    using Node = WebView::AccessibilityNodeData;
    AK::JsonObject object;
    object.set("id"sv, node.id);
    AK::JsonArray children;
    for (auto child : node.child_ids)
        children.must_append(child);
    object.set("children"sv, move(children));
    object.set("role"sv, node.role);
    object.set("name"sv, node.name);
    object.set("description"sv, node.description);
    object.set("value"sv, node.value);
    object.set("url"sv, node.url);
    AK::JsonArray bounds;
    for (auto value : {node.bounds.x(), node.bounds.y(), node.bounds.width(), node.bounds.height()})
        bounds.must_append(value);
    object.set("bounds"sv, move(bounds));
    object.set("focused"sv, node.is_focused);
    object.set("disabled"sv, node.is_disabled);
    object.set("heading_level"sv, node.heading_level);
    object.set("level"sv, node.hierarchical_level);
    object.set("live"sv, node.live);
    object.set("selected"sv, node.is_selected);
    object.set("checked"sv, static_cast<u8>(node.checked_state));
    object.set("expanded"sv, static_cast<u8>(node.expanded_state));
    object.set("pressed"sv, node.pressed_state == Node::PressedState::Pressed);
    object.set("editable"sv, node.is_editable);
    object.set("multiline"sv, node.is_multi_line);
    object.set("password"sv, node.is_password);
    object.set("read_only"sv, node.is_read_only);
    object.set("required"sv, node.is_required);
    object.set("invalid"sv, node.is_invalid);
    if (node.value_numeric.has_value())
        object.set("value_numeric"sv, *node.value_numeric);
    if (node.value_minimum.has_value())
        object.set("value_minimum"sv, *node.value_minimum);
    if (node.value_maximum.has_value())
        object.set("value_maximum"sv, *node.value_maximum);
    if (node.value_step.has_value())
        object.set("value_step"sv, *node.value_step);
    object.set("selection_start"sv, node.selection_start);
    object.set("selection_end"sv, node.selection_end);
    object.set("caret"sv, node.caret_offset);
    return object;
}

static void emit_accessibility(ElarisLadybirdView const& handle, int32_t kind, AK::JsonObject const& payload) {
    if (!handle.accessibility || !handle.id)
        return;
    auto json = payload.serialized().to_byte_string();
    emit(&handle, ELARIS_LADYBIRD_ACCESSIBILITY, std::string{json.characters(), json.length()}, kind);
}

static AK::JsonArray accessibility_nodes_json(Vector<WebView::AccessibilityNodeData> const& nodes) {
    AK::JsonArray array;
    for (auto const& node : nodes)
        array.must_append(accessibility_node_json(node));
    return array;
}

static void install_accessibility(ElarisLadybirdView& handle) {
    handle.view->on_accessibility_tree_received = [&handle](u64 sequence, auto nodes) {
        AK::JsonObject payload;
        payload.set("seq"sv, sequence);
        payload.set("nodes"sv, accessibility_nodes_json(nodes));
        emit_accessibility(handle, 1, payload);
    };
    handle.view->on_accessibility_tree_changed = [&handle](u64 base, u64 sequence, auto delta) {
        AK::JsonObject payload;
        payload.set("base"sv, base);
        payload.set("seq"sv, sequence);
        payload.set("nodes"sv, accessibility_nodes_json(delta.changed_nodes));
        AK::JsonArray removed;
        for (auto id : delta.removed_ids)
            removed.must_append(id);
        payload.set("removed"sv, move(removed));
        emit_accessibility(handle, 2, payload);
    };
    handle.view->on_accessibility_focus_changed = [&handle](i64 node) {
        AK::JsonObject payload;
        payload.set("focus"sv, node);
        emit_accessibility(handle, 3, payload);
    };
}

extern "C" int32_t elaris_ladybird_set_accessibility(ElarisLadybirdView* handle, int32_t enabled) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("accessibility requires the view owner thread");
    handle->accessibility = enabled != 0;
    if (handle->accessibility)
        handle->view->request_accessibility_tree();
    return succeed();
}

extern "C" int32_t elaris_ladybird_accessibility_action(ElarisLadybirdView* handle, int64_t node, char const* action) {
    if (!handle || !on_owner_thread(handle->bridge) || !action)
        return fail("invalid accessibility action");
    auto name = AK::StringView{action, strlen(action)};
    if (!name.is_one_of("press"sv, "focus"sv, "scroll_into_view"sv))
        return fail("unknown accessibility action");
    handle->view->perform_accessibility_action(node, MUST(String::from_utf8(name)));
    return succeed();
}

extern "C" int32_t elaris_ladybird_accessibility_select(ElarisLadybirdView* handle, int64_t node, int32_t start,
                                                        int32_t end) {
    if (!handle || !on_owner_thread(handle->bridge) || start < 0 || end < 0)
        return fail("invalid accessibility selection");
    auto action = start == end ? "set_caret_offset"_string : "set_selection"_string;
    handle->view->perform_accessibility_text_action(node, action, start, end, {});
    return succeed();
}

void ElarisApplication::display_error_dialog(AK::StringView message) const {
    auto* view = host ? host->focused_view : nullptr;
    if (!view || !view->id) {
        warnln("{}", message);
        return;
    }
    show_external_url(*view, 2, std::string{message.characters_without_null_termination(), message.length()}, {}, {},
                      {});
}

static void emit_permission(ElarisLadybirdView const& handle, PendingPermission const* pending,
                            ElarisLadybirdEventKind kind, int32_t value) {
    if (!handle.bridge->callback)
        return;
    ElarisLadybirdEvent event{};
    event.kind = kind;
    event.view_id = handle.id;
    event.value = value;
    if (!pending) {
        handle.bridge->callback(handle.bridge->context, &event);
        return;
    }
    auto origin = pending->request.top_level_origin.serialize();
    auto key = Web::Permissions::to_string(pending->request.name);
    event.request_id = pending->id;
    auto key_bytes = AK::ByteString(key);
    // Keep the borrowed strings alive through the callback.
    auto origin_bytes = origin.to_byte_string();
    event.text = origin_bytes.characters();
    event.selection = key_bytes.characters();
    event.destination = pending->requesting_origin.c_str();
    handle.bridge->callback(handle.bridge->context, &event);
}

static void emit_permission_prompt(ElarisLadybirdView const& handle) {
    auto const* pending = handle.permissions.empty() ? nullptr : &handle.permissions.front();
    auto value = !pending ? 0 : pending->request.kind == WebView::PermissionPromptKind::Question ? 1 : 2;
    emit_permission(handle, pending, ELARIS_LADYBIRD_PERMISSION_PROMPT, value);
}

static void cancel_permissions(ElarisLadybirdView& handle) {
    if (handle.permissions.empty())
        return;
    auto pending = std::exchange(handle.permissions, {});
    for (auto& item : pending)
        item.reply(Web::Permissions::PromptOutcome::Cancelled);
    emit_permission_prompt(handle);
}

static WebView::CanonicalDocument const* active_permission_document(ElarisLadybirdView const& handle,
                                                                    Web::HTML::CrossProcessId document_id) {
    WebView::CanonicalDocument const* active = nullptr;
    handle.view->traversable().for_each_in_inclusive_subtree([&](auto const& navigable) {
        auto const& document = navigable.active_document();
        if (document.id() != document_id || !document.host())
            return IterationDecision::Continue;
        active = &document;
        return IterationDecision::Break;
    });
    return active;
}

static bool permission_still_pending(ElarisLadybirdView const& handle, PendingPermission const& pending) {
    if (!active_permission_document(handle, pending.document))
        return false;
    if (pending.request.kind != WebView::PermissionPromptKind::Question)
        return true;
    auto const& manager = *handle.view->session().permission_manager;
    auto const& request = pending.request;
    return manager.decision_generation(request.name, request.top_level_origin) == pending.generation &&
           manager.effective_state(request.name, request.top_level_origin) == Web::Permissions::EffectiveState::Prompt;
}

static void discard_stale_permissions(ElarisLadybird& bridge) {
    for (auto* handle : bridge.views) {
        auto& prompts = handle->permissions;
        if (prompts.empty())
            continue;
        auto first = prompts.front().id;
        for (auto it = prompts.begin(); it != prompts.end();) {
            if (permission_still_pending(*handle, *it)) {
                ++it;
                continue;
            }
            auto reply = AK::move(it->reply);
            it = prompts.erase(it);
            reply(Web::Permissions::PromptOutcome::Cancelled);
        }
        if (prompts.empty() || prompts.front().id != first)
            emit_permission_prompt(*handle);
    }
}

static Optional<Web::Permissions::PermissionName> permission_name(AK::StringView key) {
    return Web::Permissions::permission_name_from_string(key == "location"sv ? "geolocation"sv : key);
}

extern "C" int32_t elaris_ladybird_set_permission_default(ElarisLadybird* bridge, char const* key, int32_t blocked) {
    if (!bridge || !on_owner_thread(bridge) || !key || (blocked != 0 && blocked != 1))
        return fail("invalid default permission");
    if (auto name = permission_name({key, strlen(key)}); name.has_value()) {
        if (blocked)
            bridge->blocked_by_default.insert(*name);
        else
            bridge->blocked_by_default.erase(*name);
    }
    return succeed();
}

static Optional<WebView::PermissionDecision> permission_choice(int32_t choice) {
    if (choice == 1)
        return WebView::PermissionDecision::Allow;
    if (choice == 2)
        return WebView::PermissionDecision::Block;
    return {};
}

extern "C" int32_t elaris_ladybird_set_permission(ElarisLadybird* bridge, char const* site, char const* key,
                                                  int32_t choice) {
    if (!bridge || !on_owner_thread(bridge) || !site || !key || choice < 0 || choice > 2)
        return fail("invalid site permission");
    auto name = permission_name(AK::StringView{key, strlen(key)});
    auto url = URL::Parser::basic_parse(AK::StringView{site, strlen(site)});
    if (name.has_value() && url.has_value() && (url->scheme() == "http"sv || url->scheme() == "https"sv))
        WebView::Application::default_session().permission_manager->set_decision(*name, url->origin(),
                                                                                 permission_choice(choice));
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_permissions(ElarisLadybird* bridge, char const* json) {
    if (!bridge || !on_owner_thread(bridge) || !json)
        return fail("invalid site permissions");
    if (bridge->permissions_initialized)
        return succeed();
    auto data = JsonValue::from_string(AK::StringView{json, strlen(json)});
    if (data.is_error() || !data.value().is_array())
        return fail("invalid site permission list");
    for (auto const& row : data.value().as_array().values()) {
        if (!row.is_array() || row.as_array().size() != 3)
            return fail("invalid site permission row");
        auto const& values = row.as_array();
        if (!values.at(0).is_string() || !values.at(1).is_string() || !values.at(2).get_i32().has_value())
            return fail("invalid site permission row");
    }
    auto& manager = *WebView::Application::default_session().permission_manager;
    for (auto name : Web::Permissions::PERMISSION_NAMES) {
        for (auto const& site : manager.site_decisions(name))
            manager.set_decision(name, site.origin, {});
    }
    for (auto const& row : data.value().as_array().values()) {
        auto const& values = row.as_array();
        auto site = values.at(0).as_string().to_byte_string();
        auto key = values.at(1).as_string().to_byte_string();
        elaris_ladybird_set_permission(bridge, site.characters(), key.characters(), values.at(2).get_i32().value());
    }
    bridge->permissions_initialized = true;
    return succeed();
}

extern "C" int32_t elaris_ladybird_reply_to_permission(ElarisLadybirdView* handle, uint64_t id, int32_t choice) {
    if (!handle || !on_owner_thread(handle->bridge) || choice < 0 || choice > 2)
        return fail("invalid permission response");
    if (handle->permissions.empty() || handle->permissions.front().id != id)
        return succeed();
    auto pending = AK::move(handle->permissions.front());
    handle->permissions.pop_front();
    using Outcome = Web::Permissions::PromptOutcome;
    auto outcome = Outcome::Cancelled;
    auto& manager = *handle->view->session().permission_manager;
    auto const& request = pending.request;
    auto before = manager.decision(request.name, request.top_level_origin);
    if (request.kind == WebView::PermissionPromptKind::Question && permission_still_pending(*handle, pending)) {
        if (choice == 1)
            outcome = Outcome::AllowAlways;
        else if (choice == 2)
            outcome = Outcome::Block;
    }
    if (request.kind == WebView::PermissionPromptKind::SystemDeniedNotice)
        handle->view->did_show_permission_prompt(request);
    pending.reply(outcome);
    if (outcome != Outcome::Cancelled && handle->view->is_private() == WebView::IsPrivate::No &&
        before != permission_choice(choice) &&
        manager.decision(request.name, request.top_level_origin) == permission_choice(choice))
        emit_permission(*handle, &pending, ELARIS_LADYBIRD_PERMISSION_SAVED, choice);
    emit_permission_prompt(*handle);
    return succeed();
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

static void emit_page_dialog(ElarisLadybirdView const& handle) {
    if (!handle.bridge->callback)
        return;
    auto const& dialog = handle.dialog;
    auto prompt = to_utf8(dialog.prompt_text);
    ElarisLadybirdEvent event{};
    event.kind = ELARIS_LADYBIRD_PAGE_DIALOG;
    event.view_id = handle.id;
    event.request_id = dialog.id;
    event.value = dialog.kind;
    event.text = dialog.message.c_str();
    event.selection = prompt.c_str();
    handle.bridge->callback(handle.bridge->context, &event);
}

static void finish_page_dialog(ElarisLadybirdView& handle, bool accepted, Utf16String response = {}) {
    auto dialog = AK::move(handle.dialog);
    handle.dialog = {};
    switch (dialog.kind) {
    case ELARIS_LADYBIRD_DIALOG_CLOSED:
        return;
    case ELARIS_LADYBIRD_DIALOG_ALERT:
        handle.view->alert_closed();
        break;
    case ELARIS_LADYBIRD_DIALOG_CONFIRM:
        handle.view->confirm_closed(accepted);
        break;
    case ELARIS_LADYBIRD_DIALOG_PROMPT:
        handle.view->prompt_closed(accepted ? Optional<Utf16String>{AK::move(response)} : Optional<Utf16String>{});
        break;
    case ELARIS_LADYBIRD_DIALOG_BEFORE_UNLOAD:
        dialog.before_unload(accepted);
        break;
    }
    emit_page_dialog(handle);
}

static void show_page_dialog(ElarisLadybirdView& handle, ElarisLadybirdDialogKind kind, std::string message,
                             Utf16String prompt_text = {}, AK::Function<void(bool)> before_unload = {}) {
    finish_page_dialog(handle, false);
    handle.dialog = {++handle.next_request, kind, AK::move(message), AK::move(prompt_text), AK::move(before_unload)};
    if (handle.bridge->callback)
        emit_page_dialog(handle);
    else
        finish_page_dialog(handle, kind == ELARIS_LADYBIRD_DIALOG_BEFORE_UNLOAD);
}

static void cancel_form_picker(ElarisLadybirdView& handle, bool notify_page) {
    auto picker = handle.picker;
    handle.picker = FormPicker::None;
    handle.picker_id = 0;
    handle.picker_document.clear();
    handle.picker_options.clear();
    handle.picker_allows_multiple = false;
    if (picker == FormPicker::None)
        return;
    if (notify_page) {
        switch (picker) {
        case FormPicker::None:
            break;
        case FormPicker::File:
            handle.view->file_picker_closed({});
            break;
        case FormPicker::Color:
            handle.view->color_picker_update({}, Web::HTML::ColorPickerUpdateState::Closed);
            break;
        case FormPicker::Date:
            handle.view->date_picker_closed({});
            break;
        case FormPicker::Select:
            handle.view->select_dropdown_closed({});
            break;
        }
    }
    emit(&handle, ELARIS_LADYBIRD_FORM_PICKER);
}

static void show_form_picker(ElarisLadybirdView& handle, FormPicker picker, AK::JsonObject const& payload) {
    handle.picker = picker;
    handle.picker_id = ++handle.bridge->next_picker_request;
    handle.picker_document = handle.view->traversable().active_document().id();
    if (!handle.bridge->callback) {
        cancel_form_picker(handle, true);
        return;
    }
    auto json = payload.serialized().to_byte_string();
    ElarisLadybirdEvent event{};
    event.kind = ELARIS_LADYBIRD_FORM_PICKER;
    event.view_id = handle.id;
    event.request_id = handle.picker_id;
    event.text = json.characters();
    handle.bridge->callback(handle.bridge->context, &event);
}

static void install_form_pickers(ElarisLadybirdView& handle) {
    handle.view->on_request_file_picker = [&handle](auto const& filter, auto multiple) {
        if (handle.picker == FormPicker::File)
            return;
        cancel_form_picker(handle, true);
        AK::JsonArray extensions;
        for (auto const& item : filter.filters) {
            if (auto const* extension = item.template get_pointer<Web::HTML::FileFilter::Extension>())
                extensions.must_append(extension->value.to_utf8());
        }
        AK::JsonObject payload;
        payload.set("kind"sv, "file"sv);
        handle.picker_allows_multiple = multiple == Web::HTML::AllowMultipleFiles::Yes;
        payload.set("multiple"sv, handle.picker_allows_multiple);
        payload.set("extensions"sv, AK::move(extensions));
        show_form_picker(handle, FormPicker::File, payload);
    };
    handle.view->on_request_color_picker = [&handle](auto color) {
        if (handle.picker == FormPicker::Color)
            return;
        cancel_form_picker(handle, true);
        AK::JsonObject payload;
        payload.set("kind"sv, "color"sv);
        payload.set("value"sv,
                    MUST(String::formatted("#{:02x}{:02x}{:02x}", color.red(), color.green(), color.blue())));
        show_form_picker(handle, FormPicker::Color, payload);
    };
    handle.view->on_request_date_picker = [&handle](auto const& type, auto const& value, auto const& minimum,
                                                    auto const& maximum) {
        if (handle.picker == FormPicker::Date)
            return;
        cancel_form_picker(handle, true);
        AK::JsonObject payload;
        payload.set("kind"sv, "date"sv);
        payload.set("input_type"sv, type.to_utf8());
        payload.set("value"sv, value.to_utf8());
        payload.set("minimum"sv, minimum.to_utf8());
        payload.set("maximum"sv, maximum.to_utf8());
        show_form_picker(handle, FormPicker::Date, payload);
    };
    handle.view->on_request_select_dropdown = [&handle](auto position, auto minimum_width, auto items) {
        if (handle.picker == FormPicker::Select)
            return;
        cancel_form_picker(handle, true);
        AK::JsonArray rows;
        auto append_option = [&](auto const& option) {
            AK::JsonObject row;
            row.set("kind"sv, "option"sv);
            row.set("id"sv, option.id);
            row.set("label"sv, option.label.to_utf8());
            row.set("disabled"sv, option.disabled);
            row.set("selected"sv, option.selected);
            if (!option.disabled)
                handle.picker_options.push_back(option.id);
            rows.must_append(AK::move(row));
        };
        for (auto const& item : items) {
            item.visit([&](Web::HTML::SelectItemOption const& option) { append_option(option); },
                       [&](Web::HTML::SelectItemOptionGroup const& group) {
                           AK::JsonObject row;
                           row.set("kind"sv, "group"sv);
                           row.set("label"sv, group.label.to_utf8());
                           rows.must_append(AK::move(row));
                           for (auto const& option : group.items)
                               append_option(option);
                       },
                       [&](Web::HTML::SelectItemSeparator const&) {
                           AK::JsonObject row;
                           row.set("kind"sv, "separator"sv);
                           rows.must_append(AK::move(row));
                       });
        }
        AK::JsonObject payload;
        payload.set("kind"sv, "select"sv);
        payload.set("x"sv, position.x());
        payload.set("y"sv, position.y());
        payload.set("minimum_width"sv, minimum_width);
        payload.set("rows"sv, AK::move(rows));
        show_form_picker(handle, FormPicker::Select, payload);
    };
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

#if defined(AK_OS_MACOS)
// Pauses the media a view is playing, and resumes only what such a pause stopped, so the media keys act on what the
// person was watching.
static constexpr auto pause_media_script = R"js((() => {
    for (const media of document.querySelectorAll('video, audio')) {
        if (!media.paused) {
            media.__elarisResume = true;
            media.pause();
        }
    }
})())js"sv;

static constexpr auto resume_media_script = R"js((() => {
    for (const media of document.querySelectorAll('video, audio')) {
        if (media.__elarisResume) {
            media.__elarisResume = false;
            const playing = media.play();
            if (playing && playing.catch)
                playing.catch(() => {});
        }
    }
})())js"sv;

// Runs on the owner thread. The media playing in a tab is the target, and failing that the focused tab is.
static void media_command(ElarisLadybird& bridge, Elaris::MediaCommand command) {
    ElarisLadybirdView* target = nullptr;
    for (auto* view : bridge.views) {
        if (!view->picture_in_picture && view->view->is_playing_audio()) {
            target = view;
            break;
        }
    }
    if (!target && bridge.focused_view && !bridge.focused_view->picture_in_picture)
        target = bridge.focused_view;
    if (!target)
        return;
    bool pause = command == Elaris::MediaCommand::Pause ||
                 (command == Elaris::MediaCommand::Toggle && target->view->is_playing_audio());
    target->view->run_javascript(MUST(String::from_utf8(pause ? pause_media_script : resume_media_script)));
}
#endif

// Shows the playing tab in Now Playing, or clears it when no tab is playing.
static void update_now_playing(ElarisLadybird& bridge) {
#if defined(AK_OS_MACOS)
    if (!bridge.media_controls)
        return;
    for (auto const* view : bridge.views) {
        if (!view->picture_in_picture && view->view->is_playing_audio()) {
            bridge.media_controls->set_now_playing(to_utf8(view->view->title()), true);
            return;
        }
    }
    bridge.media_controls->clear_now_playing();
#else
    (void)bridge;
#endif
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
    if (auto* fixture = std::getenv("ELARIS_TEST_LOCATION"); fixture && std::string_view(fixture) == "1") {
        Core::GeolocationProvider::set_provider_functions(
            []() -> ErrorOr<NonnullOwnPtr<Core::GeolocationProvider>> { return make<TestLocationProvider>(); },
            [] { return true; });
    }

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
        .next_picker_request = 0,
        .permissions_initialized = false,
        .blocked_by_default = {},
        .focused_view = nullptr,
#if defined(AK_OS_MACOS)
        .media_link = nullptr,
        .media_controls = nullptr,
#endif
    };
    s_application->host = *out_bridge;
#if defined(AK_OS_MACOS)
    auto* bridge = *out_bridge;
    bridge->media_link = std::make_shared<OwnerLink>();
    bridge->media_controls = Elaris::MediaControls::create([link = bridge->media_link, bridge](Elaris::MediaCommand command) {
        post_to_owner(link, [bridge, command] { media_command(*bridge, command); });
    });
#endif
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

extern "C" int32_t elaris_ladybird_pause_download(ElarisLadybird* bridge, uint64_t download_id, int32_t pause) {
    if (!bridge || !on_owner_thread(bridge))
        return fail("pausing a download must run on the initialization thread");
    if (pause)
        s_application->file_downloader().pause_download(download_id);
    else
        s_application->file_downloader().resume_download(download_id);
    return succeed();
}

extern "C" int32_t elaris_ladybird_suspend_downloads(ElarisLadybird* bridge) {
    if (!bridge || !on_owner_thread(bridge))
        return fail("download shutdown must run on the initialization thread");
    // Resumable transfers pause, so Ladybird restores them next launch; the rest cannot continue.
    auto& downloader = s_application->file_downloader();
    downloader.pause_active_downloads();
    downloader.cancel_unresumable_downloads();
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

static int32_t create_view(ElarisLadybird* bridge, uint64_t view_id, WebView::IsPrivate mode,
                           ElarisLadybirdView** out_view, WebView::CanonicalTraversable* popup = nullptr) {
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

    auto view = ElarisWebView::create(AK::move(theme_buffer), {1024, 768}, mode, popup);
    auto* handle = new ElarisLadybirdView{bridge, view_id, AK::move(view),   {}, 0,  0,  {},   {},
                                          {},     {},      FormPicker::None, 0,  {}, {}, false};
    bridge->views.push_back(handle);
    // The callbacks live in the view, which the handle owns, so they never outlive it.
    if (auto* fixture = std::getenv("ELARIS_TEST_MICROPHONE_DENIED"); fixture && std::string_view(fixture) == "1") {
        using Authorization = WebView::MicrophoneAuthorization;
        handle->view->set_microphone_authorization_for_testing(Authorization::Denied, Authorization::Denied, false);
    }
    install_form_pickers(*handle);
    handle->view->on_request_permission = [handle](auto const& request, auto reply) {
        if (request.is_file || request.top_level_origin.is_opaque() || !handle->bridge->callback ||
            handle->bridge->blocked_by_default.contains(request.name)) {
            reply(Web::Permissions::PromptOutcome::Cancelled);
            return;
        }
        auto id = request.document_id.value_or(handle->view->traversable().active_document().id());
        auto* document = active_permission_document(*handle, id);
        if (!document) {
            reply(Web::Permissions::PromptOutcome::Cancelled);
            return;
        }
        auto origin = document->origin().serialize().to_byte_string();
        auto generation =
            handle->view->session().permission_manager->decision_generation(request.name, request.top_level_origin);
        handle->permissions.push_back(
            {++handle->next_request, id, request, generation, AK::move(reply), origin.characters()});
        if (handle->permissions.size() == 1)
            emit_permission_prompt(*handle);
    };
    handle->view->on_cancel_permission_prompt = [handle](auto const& request) {
        auto& permissions = handle->permissions;
        if (permissions.empty())
            return;
        auto first = permissions.front().id;
        std::erase_if(permissions, [&](auto const& pending) { return pending.request.is_same_request_as(request); });
        if (permissions.empty() || permissions.front().id != first)
            emit_permission_prompt(*handle);
    };
    install_accessibility(*handle);
    handle->view->on_request_external_url_confirmation = [handle](auto const& url, auto const& origin,
                                                                  auto const& application, auto reply) {
        if (!handle->id) {
            reply(false);
            return;
        }
        show_external_url(*handle, 1, to_utf8(url), origin.serialize().to_byte_string().characters(),
                          application.application_name().to_byte_string().characters(), AK::move(reply));
    };
    handle->view->on_tab_indicator_change = [handle](WebView::TabIndicator indicator, bool visible) {
        switch (indicator) {
        case WebView::TabIndicator::UsingLocation:
            emit(handle, ELARIS_LADYBIRD_PERMISSION_USE, "location", visible);
            break;
        case WebView::TabIndicator::UsingMicrophone:
            emit(handle, ELARIS_LADYBIRD_PERMISSION_USE, "microphone", visible);
            break;
        case WebView::TabIndicator::PlayingAudio:
            update_now_playing(*handle->bridge);
            break;
        }
    };
    handle->view->on_new_web_view = [handle](auto activate, auto const&, auto& traversable) -> String {
        // The popup gets Ladybird's actual traversable, so window.opener and the returned WindowProxy
        // continue to work. A pending popup has no host view id until the tab strip adopts it.
        if (!handle->id || !handle->bridge->callback)
            return {};
        ElarisLadybirdView* child{};
        if (create_view(handle->bridge, 0, handle->view->is_private(), &child, &traversable) != 0)
            return {};
        child->popup_source = handle;
        child->popup_request = ++handle->next_request;
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_POPUP;
        event.view_id = handle->id;
        event.request_id = child->popup_request;
        event.value = activate == Web::HTML::ActivateTab::Yes;
        handle->bridge->callback(handle->bridge->context, &event);
        return child->view->handle();
    };
    if (popup) {
        handle->view->on_close = [handle] {
            handle->popup_closed = true;
            cancel_permissions(*handle);
            cancel_external_url(*handle);
            emit(handle, ELARIS_LADYBIRD_POPUP_CLOSED);
        };
    }
    handle->view->on_finish_handling_mouse_event = [handle](auto const& event, auto result) {
        if (event.type == Web::MouseEvent::Type::MouseWheel)
            emit(handle, ELARIS_LADYBIRD_WHEEL_RESULT, {}, result == Web::EventResult::Cancelled ? 1 : 0);
    };
    handle->view->on_fullscreen_window = [handle] { set_web_fullscreen(*handle, true); };
    handle->view->on_exit_fullscreen_window = [handle] { set_web_fullscreen(*handle, false); };
    handle->view->on_request_alert = [handle](auto const& message) {
        show_page_dialog(*handle, ELARIS_LADYBIRD_DIALOG_ALERT, to_utf8(message));
    };
    handle->view->on_request_confirm = [handle](auto const& message) {
        show_page_dialog(*handle, ELARIS_LADYBIRD_DIALOG_CONFIRM, to_utf8(message));
    };
    handle->view->on_request_prompt = [handle](auto const& message, auto const& default_text) {
        show_page_dialog(*handle, ELARIS_LADYBIRD_DIALOG_PROMPT, to_utf8(message), default_text);
    };
    handle->view->on_request_before_unload = [handle](auto const& source, auto complete) {
        show_page_dialog(*handle, ELARIS_LADYBIRD_DIALOG_BEFORE_UNLOAD,
                         {reinterpret_cast<char const*>(source.bytes().data()), source.bytes().size()}, {},
                         AK::move(complete));
    };
    handle->view->on_request_set_prompt_text = [handle](auto const& text) {
        if (handle->dialog.kind == ELARIS_LADYBIRD_DIALOG_PROMPT) {
            handle->dialog.prompt_text = text;
            emit_page_dialog(*handle);
        }
    };
    handle->view->on_request_accept_dialog = [handle] {
        finish_page_dialog(*handle, true, handle->dialog.prompt_text);
    };
    handle->view->on_request_dismiss_dialog = [handle] { finish_page_dialog(*handle, false); };
    handle->view->on_before_browser_initiated_navigation = [handle] {
        finish_page_dialog(*handle, false);
        cancel_form_picker(*handle, true);
    };
    handle->view->on_link_hover = [handle](auto const& url) {
        emit(handle, ELARIS_LADYBIRD_HOVERED_LINK, to_utf8(url));
    };
    handle->view->on_link_unhover = [handle] { emit(handle, ELARIS_LADYBIRD_HOVERED_LINK); };
    handle->view->on_enter_tooltip_area = [handle](auto const& tooltip) {
        emit(handle, ELARIS_LADYBIRD_TOOLTIP, {tooltip.characters(), tooltip.length()});
    };
    handle->view->on_leave_tooltip_area = [handle] { emit(handle, ELARIS_LADYBIRD_TOOLTIP); };
    handle->view->on_cursor_change = [handle](Gfx::Cursor const& cursor) {
        auto const* standard = cursor.get_pointer<Gfx::StandardCursor>();
        auto name = Gfx::standard_cursor_to_string(standard ? *standard : Gfx::StandardCursor::Arrow);
        emit(handle, ELARIS_LADYBIRD_CURSOR, {name.characters_without_null_termination(), name.length()});
    };
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
        cancel_external_url(*handle);
        set_web_fullscreen(*handle, false);
        if (handle->view->zoom_level() != handle->page_zoom)
            handle->view->set_zoom(handle->page_zoom);
        cancel_form_picker(*handle, true);
        emit(handle, ELARIS_LADYBIRD_NAVIGATION_COMMITTED, to_utf8(handle->view->url()));
        if (handle->view->traversable().active_document().creation_url() == URL::about_error())
            emit(handle, ELARIS_LADYBIRD_LOAD_ERROR, "This page could not be loaded.");
    };
    handle->view->on_web_content_crashed = [handle](auto) {
        set_web_fullscreen(*handle, false);
        cancel_permissions(*handle);
        cancel_external_url(*handle);
        emit(handle, ELARIS_LADYBIRD_PERMISSION_USE, "location", false);
        emit(handle, ELARIS_LADYBIRD_PERMISSION_USE, "microphone", false);
        cancel_form_picker(*handle, false);
        handle->dialog = {};
        emit_page_dialog(*handle);
        emit(handle, ELARIS_LADYBIRD_LOAD_ERROR, "This page stopped responding.");
    };
    handle->view->on_browser_history_traversal_complete = [handle] {
        cancel_form_picker(*handle, true);
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
        event.kind = activate == Elaris::peek_disposition ? ELARIS_LADYBIRD_PEEK : ELARIS_LADYBIRD_NEW_TAB;
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

#if defined(AK_OS_MACOS)
// Shows a view's Picture-in-Picture video in a floating panel. The view is a page of its own, and the panel shows the
// frames it presents. Input and size changes from the panel reach the owner thread, which owns the view.
class ElarisPictureInPictureWindow final : public WebView::PictureInPictureWindow {
  public:
    AK_ALLOC_WITH_KMALLOC;

    ElarisPictureInPictureWindow(ElarisLadybirdView& handle, Gfx::IntSize video_size)
        : m_handle(&handle)
        , m_screen(Elaris::PictureInPicturePanel::screen_size())
        , m_size(initial_size(video_size, m_screen))
        , m_link(std::make_shared<OwnerLink>()) {
        report_page_close_of(*m_handle->view);

        Elaris::PictureInPicturePanel::Handlers handlers;
        handlers.on_mouse = [link = m_link, this](ElarisLadybirdMouseEvent event) {
            post_to_owner(link, [this, event] { elaris_ladybird_mouse_event(m_handle, &event); });
        };
        handlers.on_resize = [link = m_link, this](Gfx::IntSize size) {
            post_to_owner(link, [this, size] { apply_panel_size(size); });
        };
        m_panel = Elaris::PictureInPicturePanel::create(m_size, AK::move(handlers));
        m_scale = m_panel->scale();
        m_handle->view->set_viewport(device_size(m_size), m_scale);
        m_panel->configure(m_size, video_size, minimum_size(video_size), maximum_size(video_size, m_screen));

        m_handle->view->enable_picture_in_picture_controls();
        m_handle->view->on_picture_in_picture_control_activated = [this](Compositing::PictureInPictureControl control) {
            // Going back to the tab leaves the video playing there, as Ladybird's Qt frontend does.
            if (on_close)
                on_close(control == Compositing::PictureInPictureControl::BackToTab ? VideoKeepsPlaying::Yes
                                                                                    : VideoKeepsPlaying::No);
        };
        m_handle->view->on_presented = [this] { present_frame(); };
    }

    ~ElarisPictureInPictureWindow() override {
        m_link->live = false;
        m_panel = nullptr;
        elaris_ladybird_destroy_view(m_handle);
    }

    Gfx::IntSize size() const override { return m_size; }
    String handle() const override { return m_handle->view->handle(); }
    void hide() override { m_panel->hide(); }

    void set_video_size(Gfx::IntSize video_size) override {
        auto size = size_for_video_size(m_size, video_size, m_screen);
        m_panel->configure(size, video_size, minimum_size(video_size), maximum_size(video_size, m_screen));
        apply_panel_size(size);
    }

  private:
    void apply_panel_size(Gfx::IntSize size) {
        if (size == m_size)
            return;
        m_size = size;
        m_handle->view->set_viewport(device_size(size), m_scale);
        if (on_resize)
            on_resize(size);
    }

    Web::DevicePixelSize device_size(Gfx::IntSize size) const {
        return Gfx::IntSize(static_cast<int>(size.width() * m_scale), static_cast<int>(size.height() * m_scale))
            .to_type<Web::DevicePixels>();
    }

    void present_frame() {
        ElarisLadybirdFrame frame{};
        if (elaris_ladybird_frame(m_handle, &frame) != 0)
            return;
        std::vector<uint8_t> pixels(static_cast<size_t>(frame.width) * static_cast<size_t>(frame.height) * 4);
        if (elaris_ladybird_copy_frame(m_handle, pixels.data(), pixels.size()) != 0)
            return;
        m_panel->show_frame(std::move(pixels), frame.width, frame.height);
    }

    ElarisLadybirdView* m_handle;
    Gfx::IntSize m_screen;
    Gfx::IntSize m_size;
    double m_scale{1};
    std::shared_ptr<OwnerLink> m_link;
    AK::OwnPtr<Elaris::PictureInPicturePanel> m_panel;
};

AK::OwnPtr<WebView::PictureInPictureWindow> ElarisApplication::create_picture_in_picture_window(
    WebView::WebContentPage& requesting_page, WebView::CanonicalTraversable& traversable, Gfx::IntSize video_size) {
    if (!host)
        return {};
    auto& owner = requesting_page.view();
    ElarisLadybirdView* handle = nullptr;
    auto mode = owner.is_private() == WebView::IsPrivate::Yes ? WebView::IsPrivate::Yes : WebView::IsPrivate::No;
    if (create_view(host, 0, mode, &handle, &traversable) != 0 || !handle)
        return {};
    handle->picture_in_picture = true;
    handle->view->set_owner(owner);
    return AK::make<ElarisPictureInPictureWindow>(*handle, video_size);
}
#else
AK::OwnPtr<WebView::PictureInPictureWindow> ElarisApplication::create_picture_in_picture_window(
    WebView::WebContentPage&, WebView::CanonicalTraversable&, Gfx::IntSize) {
    return {};
}
#endif

extern "C" int32_t elaris_ladybird_adopt_popup(ElarisLadybird* bridge, uint64_t source, uint64_t request_id,
                                               uint64_t id, ElarisLadybirdView** out_view) {
    if (!bridge || !on_owner_thread(bridge) || !out_view || !id)
        return fail("invalid popup adoption");
    *out_view = nullptr;
    auto entry = std::find_if(bridge->views.begin(), bridge->views.end(), [&](auto const* candidate) {
        return candidate->popup_source && candidate->popup_source->id == source &&
               candidate->popup_request == request_id;
    });
    if (entry == bridge->views.end())
        return succeed();
    auto* popup = *entry;
    if (popup->popup_closed) {
        elaris_ladybird_destroy_view(popup);
        return succeed();
    }
    popup->popup_source = nullptr;
    popup->id = id;
    auto& view = *popup->view;
    emit(popup, ELARIS_LADYBIRD_NAVIGATION_COMMITTED, to_utf8(view.url()));
    emit(popup, ELARIS_LADYBIRD_TITLE, to_utf8(view.title()));
    emit(popup, ELARIS_LADYBIRD_LOADING, {}, view.is_loading());
    emit(popup, ELARIS_LADYBIRD_FRAME);
    emit_permission_prompt(*popup);
    for (auto const& [indicator, key] : {std::pair{WebView::TabIndicator::UsingLocation, "location"},
                                         std::pair{WebView::TabIndicator::UsingMicrophone, "microphone"}})
        emit(popup, ELARIS_LADYBIRD_PERMISSION_USE, key, view.tab_indicator_visible(indicator));
    if (popup->web_fullscreen)
        emit(popup, ELARIS_LADYBIRD_WEB_FULLSCREEN, {}, 1);
    *out_view = popup;
    return succeed();
}

extern "C" int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, uint64_t view_id,
                                               ElarisLadybirdView** out_view) {
    return create_view(bridge, view_id, WebView::IsPrivate::No, out_view);
}

extern "C" int32_t elaris_ladybird_clear_browsing_data(ElarisLadybird* bridge, int64_t since_ms, int32_t flags,
                                                       uint64_t request_id) {
    if (!bridge || !s_application || !on_owner_thread(bridge))
        return fail("clearing browsing data requires the bridge owner thread");
    using Options = WebView::Application::ClearBrowsingDataOptions;
    auto selected = [flags](int32_t flag) { return flags & flag ? Options::Delete::Yes : Options::Delete::No; };
    Options options;
    if (since_ms >= 0)
        options.since = UnixDateTime::from_milliseconds_since_epoch(since_ms);
    options.delete_cached_files = selected(ELARIS_LADYBIRD_CLEAR_CACHE);
    options.delete_site_data = selected(ELARIS_LADYBIRD_CLEAR_SITE_DATA);
    options.delete_download_history = selected(ELARIS_LADYBIRD_CLEAR_DOWNLOAD_HISTORY);
    auto report = [bridge, request_id](bool cleared) {
        if (!bridge->callback)
            return;
        ElarisLadybirdEvent event{};
        event.kind = ELARIS_LADYBIRD_BROWSING_DATA_CLEARED;
        event.request_id = request_id;
        event.value = cleared;
        bridge->callback(bridge->context, &event);
    };
    s_application->clear_browsing_data(options)
        ->when_resolved([report](Empty) { report(true); })
        .when_rejected([report](Error const&) { report(false); });
    return succeed();
}

static Optional<Web::HTML::AutoplayPolicy> autoplay_policy(Optional<i32> value) {
    switch (value.value_or(-1)) {
    case 0:
        return Web::HTML::AutoplayPolicy::AllowAudioAndVideo;
    case 1:
        return Web::HTML::AutoplayPolicy::BlockAudio;
    case 2:
        return Web::HTML::AutoplayPolicy::BlockAudioAndVideo;
    default:
        return {};
    }
}

extern "C" int32_t elaris_ladybird_set_autoplay(ElarisLadybird* bridge, int32_t default_policy, char const* json) {
    if (!bridge || !s_application || !on_owner_thread(bridge) || !json)
        return fail("autoplay settings require the bridge owner thread");
    auto policy = autoplay_policy(default_policy);
    auto rows = JsonValue::from_string(AK::StringView{json, strlen(json)});
    if (!policy.has_value() || rows.is_error() || !rows.value().is_array())
        return fail("invalid autoplay settings");
    OrderedHashMap<String, Web::HTML::AutoplayPolicy> sites;
    for (auto const& row : rows.value().as_array().values()) {
        if (!row.is_array() || row.as_array().size() != 2 || !row.as_array().at(0).is_string())
            return fail("invalid autoplay site row");
        auto site_policy = autoplay_policy(row.as_array().at(1).get_i32());
        if (!site_policy.has_value())
            return fail("invalid autoplay site policy");
        sites.set(row.as_array().at(0).as_string(), *site_policy);
    }
    auto& settings = WebView::Application::settings();
    settings.remove_all_autoplay_site_filters();
    for (auto const& [site, site_policy] : sites)
        settings.add_autoplay_site_filter(site, site_policy);
    settings.set_autoplay_policy(*policy);
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_content_blocking(ElarisLadybird* bridge, int32_t enabled, char const* json) {
    if (!bridge || !s_application || !on_owner_thread(bridge) || !json)
        return fail("content blocking requires the bridge owner thread");
    auto rows = JsonValue::from_string(AK::StringView{json, strlen(json)});
    if (rows.is_error() || !rows.value().is_array())
        return fail("invalid content blocker lists");
    HashTable<String> selected;
    for (auto const& row : rows.value().as_array().values()) {
        if (!row.is_string())
            return fail("invalid content blocker list identifier");
        selected.set(row.as_string());
    }

    auto& settings = WebView::Application::settings();
    bool enabling_list = false;
    for (auto const& list : Vector<WebView::ContentBlockerList>(settings.content_blocker_lists())) {
        auto should_enable = selected.contains(list.identifier);
        enabling_list = enabling_list || (should_enable && !list.enabled);
        if (should_enable != list.enabled)
            settings.set_content_blocker_list_enabled(list.identifier, should_enable);
    }

    // Engine downloads lists only when filter list updates turn on, or on its daily check. Turning them off first makes
    // turning them back on fetch the list that was just enabled.
    bool updating = enabled != 0;
    if (updating && enabling_list && settings.filter_list_updates_enabled())
        settings.set_filter_list_updates_enabled(false);
    settings.set_content_blocker_enabled(updating);
    settings.set_filter_list_updates_enabled(updating);
    return succeed();
}

extern "C" int32_t elaris_ladybird_create_private_view(ElarisLadybird* bridge, uint64_t view_id,
                                                       ElarisLadybirdView** out_view) {
    return create_view(bridge, view_id, WebView::IsPrivate::Yes, out_view);
}

extern "C" int32_t elaris_ladybird_reset_private_session(ElarisLadybird* bridge) {
    if (!bridge || !s_application || !on_owner_thread(bridge))
        return fail("private session reset requires the bridge owner thread");
    for (auto const* handle : bridge->views) {
        if (handle->view->is_private() == WebView::IsPrivate::Yes)
            return fail("close all private views before resetting the session");
    }
    s_application->reset_private_browsing_session();
    return succeed();
}

#ifdef ELARIS_BUILD_SMOKE_TESTS
extern "C" int32_t elaris_ladybird_test_create_private_view(ElarisLadybird* bridge, uint64_t view_id,
                                                            ElarisLadybirdView** out_view) {
    return elaris_ladybird_create_private_view(bridge, view_id, out_view);
}

extern "C" int32_t elaris_ladybird_test_reset_private_session(ElarisLadybird* bridge) {
    return elaris_ladybird_reset_private_session(bridge);
}
extern "C" int32_t elaris_ladybird_test_start_private_download(ElarisLadybirdView* view, char const* url,
                                                               char const* filename) {
    if (!view || !url || !filename || !on_owner_thread(view->bridge) ||
        view->view->is_private() != WebView::IsPrivate::Yes)
        return fail("private download probe requires a private view");
    auto parsed = URL::Parser::basic_parse(AK::StringView(url, std::char_traits<char>::length(url)));
    if (!parsed.has_value())
        return fail("invalid test URL");
    auto destination = s_application->default_path_for_downloaded_file(filename);
    if (destination.is_error())
        return fail("could not create private download destination");
    s_application->file_downloader().download_file(WebView::IsPrivate::Yes, *parsed, destination.release_value(),
                                                   WebView::FileDownloader::MayReplaceDestination::No);
    return succeed();
}

extern "C" uint64_t elaris_ladybird_test_private_request_servers(ElarisLadybird* bridge) {
    if (!bridge || !s_application || !on_owner_thread(bridge))
        return 0;
    uint64_t count = 0;
    WebView::Application::request_server_manager().for_each_instance([&](auto& instance) {
        if (instance.session().is_private() == WebView::IsPrivate::Yes)
            ++count;
        return IterationDecision::Continue;
    });
    return count;
}
#endif

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

extern "C" int32_t elaris_ladybird_discard_prepared(ElarisLadybirdView* handle, uint64_t request) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("invalid input view");
    handle->pending_tabs.erase(request);
    return succeed();
}

extern "C" int32_t elaris_ladybird_load_prepared(ElarisLadybirdView* target, uint64_t source, uint64_t request) {
    if (!target || !on_owner_thread(target->bridge))
        return fail("invalid input view");
    for (auto* handle : target->bridge->views) {
        if (handle->id != source)
            continue;
        if (handle->view->session().id() != target->view->session().id())
            return fail("prepared navigation cannot cross browsing sessions");
        auto item = handle->pending_tabs.extract(request);
        if (!item.empty()) {
            target->view->load(AK::move(item.mapped()));
            return succeed();
        }
        break;
    }
    return fail("the prepared link is no longer available");
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

extern "C" int32_t elaris_ladybird_reply_to_dialog(ElarisLadybirdView* handle, uint64_t request_id, int32_t accepted,
                                                   uint8_t const* response, size_t response_size) {
    if (!handle || !on_owner_thread(handle->bridge) || (response_size && !response))
        return fail("invalid page dialog response");
    if (!request_id || handle->dialog.id != request_id)
        return succeed();
    auto bytes = response_size ? AK::StringView{response, response_size} : AK::StringView{};
    if (!Utf8View(bytes).validate())
        return fail("invalid UTF-8 page dialog response");
    auto text = Utf16String::from_utf8(bytes);
    finish_page_dialog(*handle, accepted != 0, AK::move(text));
    return succeed();
}

extern "C" int32_t elaris_ladybird_reply_to_picker(ElarisLadybirdView* handle, uint64_t request_id,
                                                   uint8_t const* response, size_t response_size) {
    if (!handle || !on_owner_thread(handle->bridge) || (!response && response_size))
        return fail("invalid form picker response");
    if (!request_id || handle->picker_id != request_id)
        return succeed();
    if (!response || handle->picker_document != handle->view->traversable().active_document().id()) {
        cancel_form_picker(*handle, true);
        return succeed();
    }
    auto value = AK::StringView{response, response_size};
    switch (handle->picker) {
    case FormPicker::None:
        return succeed();
    case FormPicker::File: {
        Vector<Web::HTML::SelectedFile> files;
        auto paths = value.split_view('\0');
        if (paths.size() > 1 && !handle->picker_allows_multiple)
            return fail("file picker accepts only one file");
        for (auto path : paths) {
            auto filename = AK::LexicalPath::basename(path);
            if (!Utf8View(filename).validate())
                continue;
            auto file = WebView::create_selected_file(AK::ByteString{path});
            if (!file.is_error())
                files.append(file.release_value());
        }
        handle->view->file_picker_closed(AK::move(files));
        break;
    }
    case FormPicker::Color:
        handle->view->color_picker_update(Gfx::Color::from_string(value), Web::HTML::ColorPickerUpdateState::Closed);
        break;
    case FormPicker::Date:
        if (!Utf8View(value).validate())
            return fail("invalid date picker text");
        handle->view->date_picker_closed(Utf16String::from_utf8(value));
        break;
    case FormPicker::Select: {
        auto option = value.to_number<uint32_t>();
        if (option.has_value() && std::find(handle->picker_options.begin(), handle->picker_options.end(), *option) ==
                                      handle->picker_options.end())
            return fail("invalid select option");
        handle->view->select_dropdown_closed(option);
        break;
    }
    }
    cancel_form_picker(*handle, false);
    return succeed();
}

extern "C" int32_t elaris_ladybird_preflight_close(ElarisLadybirdView* handle) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("close preflight requires the view owner thread");
    auto report = [handle](bool approved) { emit(handle, ELARIS_LADYBIRD_CLOSE_DECISION, {}, approved); };
    if (handle->view->needs_beforeunload_check()) {
        finish_page_dialog(*handle, false);
        handle->view->request_close_preflight(AK::move(report));
    } else {
        report(true);
    }
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

extern "C" int32_t elaris_ladybird_navigation_items(ElarisLadybirdView* handle, int32_t direction) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("session history requires a live view on the initialization thread");
    if (direction != -1 && direction != 1)
        return fail("invalid session history direction");

    AK::JsonArray items;
    for (auto const& target : handle->view->session_history_traversal_items(direction)) {
        AK::JsonObject item;
        item.set("step"sv, target.step);
        item.set("url"sv, target.url.to_string());
        items.must_append(AK::move(item));
    }
    auto json = items.serialized().to_byte_string();
    emit(handle, ELARIS_LADYBIRD_NAVIGATION_ITEMS, {json.characters(), json.length()});
    return succeed();
}

extern "C" int32_t elaris_ladybird_traverse_to_step(ElarisLadybirdView* handle, int32_t step) {
    if (!handle || !on_owner_thread(handle->bridge))
        return fail("session history requires a live view on the initialization thread");
    handle->view->traverse_the_history_to_step(step);
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

extern "C" int32_t elaris_ladybird_reload_bypassing_cache(ElarisLadybirdView* handle) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    handle->view->reload(WebView::BypassCache::Yes);
    return succeed();
}

extern "C" int32_t elaris_ladybird_set_zoom(ElarisLadybirdView* handle, double zoom) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("zoom must run on the initialization thread");
    if (!std::isfinite(zoom) || zoom <= 0)
        return fail("zoom must be finite and positive");
    handle->view->set_zoom(zoom);
    handle->page_zoom = handle->view->zoom_level();
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
#if defined(AK_OS_MACOS) || defined(AK_OS_LINUX)
        // A frame shared with the host's GPU may have no CPU bitmap at all.
        auto displayed = handle->view->backing_stores().displayed_frame();
        if (handle->view->gpu_frames() && displayed.has_value() && !displayed->size.is_empty()) {
            *out_frame = {displayed->size.width(),       displayed->size.height(), displayed->size.width() * 4,
                          ELARIS_LADYBIRD_PIXEL_FORMAT_RGBA8, displayed->device_pixel_ratio, 0, 0,
                          displayed->size.width(),       displayed->size.height(), handle->view->serial()};
            return succeed();
        }
#endif
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

extern "C" int32_t elaris_ladybird_gpu_frames_supported() {
#if defined(AK_OS_MACOS) || (defined(AK_OS_LINUX) && defined(USE_VULKAN_DMABUF_IMAGES))
    return 1;
#else
    return 0;
#endif
}

extern "C" int32_t elaris_ladybird_set_gpu_frames(ElarisLadybirdView* handle, int32_t enabled) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("frame presentation must change on the initialization thread");
#if defined(AK_OS_MACOS) || (defined(AK_OS_LINUX) && defined(USE_VULKAN_DMABUF_IMAGES))
    if (!handle->view->set_gpu_frames(enabled != 0))
        return fail("no GPU is available to share frames with");
    return succeed();
#else
    (void)enabled;
    return fail("GPU frames are not available on this platform");
#endif
}

extern "C" int32_t elaris_ladybird_gpu_frame(ElarisLadybirdView* handle, ElarisLadybirdGpuFrame* out_frame) {
    if (!handle || !out_frame)
        return fail("view and output frame are required");
    if (!on_owner_thread(handle->bridge))
        return fail("frames must be read on the initialization thread");
#if defined(AK_OS_MACOS)
    auto* blitter = handle->view->blitter();
    if (!blitter)
        return fail("the view does not present GPU frames");
    auto frame = handle->view->backing_stores().displayed_frame();
    if (!frame.has_value() || !frame->shared_image_buffer || frame->size.is_empty()) {
        s_last_error.clear();
        return ELARIS_LADYBIRD_NO_FRAME;
    }
    auto const& source = frame->shared_image_buffer->iosurface_handle();
    // The backing store can be larger than the frame, which is its top-left corner.
    auto width = std::min(frame->size.width(), static_cast<int>(source.width()));
    auto height = std::min(frame->size.height(), static_cast<int>(source.height()));
    void* copy = nullptr;
    switch (blitter->copy(source.core_foundation_pointer(), width, height, handle->view->serial(), &copy)) {
    case Elaris::FrameBlitter::Result::Copied:
        break;
    case Elaris::FrameBlitter::Result::Busy:
        s_last_error.clear();
        return ELARIS_LADYBIRD_BUSY;
    case Elaris::FrameBlitter::Result::Failed:
        return fail("could not copy the frame on the GPU");
    }
    *out_frame = {};
    out_frame->io_surface = copy;
    out_frame->width = width;
    out_frame->height = height;
    out_frame->device_pixel_ratio = frame->device_pixel_ratio;
    out_frame->serial = handle->view->serial();
    return succeed();
#elif defined(AK_OS_LINUX) && defined(USE_VULKAN_DMABUF_IMAGES)
    if (!handle->view->gpu_frames())
        return fail("the view does not present GPU frames");
    auto frame = handle->view->backing_stores().displayed_frame();
    if (!frame.has_value() || !frame->shared_image_buffer || !frame->bitmap_id.has_value() || frame->size.is_empty()) {
        s_last_error.clear();
        return ELARIS_LADYBIRD_NO_FRAME;
    }
    auto const* dmabuf = frame->shared_image_buffer->linux_dmabuf_handle();
    if (!dmabuf || dmabuf->planes.size() != 1)
        return fail("the frame is not a single-plane dmabuf");
    auto const& plane = dmabuf->planes[0];
    if (plane.object_index >= dmabuf->objects.size())
        return fail("the frame's dmabuf plane has no object");
    // The host owns the descriptor it is given, so it keeps the buffer past this view's own use of it.
    auto fd = dup(dmabuf->objects[plane.object_index].fd());
    if (fd < 0)
        return fail("could not duplicate the frame's dmabuf descriptor");
    *out_frame = {};
    out_frame->dmabuf_fd = fd;
    out_frame->dmabuf_offset = plane.offset;
    out_frame->dmabuf_stride = plane.pitch;
    out_frame->dmabuf_modifier = dmabuf->modifier;
    out_frame->dmabuf_format = dmabuf->drm_format;
    out_frame->bitmap_id = *frame->bitmap_id;
    out_frame->width = frame->size.width();
    out_frame->height = frame->size.height();
    out_frame->device_pixel_ratio = frame->device_pixel_ratio;
    out_frame->serial = handle->view->serial();
    return succeed();
#else
    return fail("GPU frames are not available on this platform");
#endif
}

extern "C" int32_t elaris_ladybird_release_frames(ElarisLadybirdView* handle, uint64_t completed_serial) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("frames must be released on the initialization thread");
#if defined(AK_OS_MACOS)
    if (auto* blitter = handle->view->blitter())
        blitter->release_completed(completed_serial);
#elif defined(AK_OS_LINUX)
    handle->view->release_read(completed_serial);
#else
    (void)completed_serial;
#endif
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
    if (focused)
        handle->bridge->focused_view = handle;
    else if (handle->bridge->focused_view == handle)
        handle->bridge->focused_view = nullptr;
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
    discard_stale_permissions(*bridge);
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
    discard_stale_permissions(*bridge);
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
    // The host has already committed to removing this view. Keep its page alive just long
    // enough to run Ladybird's close path without raising a dialog for a removed tab.
    cancel_permissions(*handle);
    cancel_external_url(*handle);
    if (handle->bridge->focused_view == handle)
        handle->bridge->focused_view = nullptr;
    auto& views = handle->bridge->views;
    auto pending_child = [&] {
        return std::find_if(views.begin(), views.end(),
                            [handle](auto const* view) { return view->popup_source == handle; });
    };
    while (auto* child = [&]() -> ElarisLadybirdView* {
        auto it = pending_child();
        return it == views.end() ? nullptr : *it;
    }())
        elaris_ladybird_destroy_view(child);
    auto close = handle->popup_closed ? AK::Function<void()>{} : handle->view->prepare_for_close_without_prompting();
    views.erase(std::remove(views.begin(), views.end(), handle), views.end());
    update_now_playing(*handle->bridge);
    delete handle;
    if (close)
        close();
}

extern "C" void elaris_ladybird_destroy(ElarisLadybird* bridge) {
    if (!bridge)
        return;
    if (!on_owner_thread(bridge)) {
        fail("shutdown must run on the initialization thread");
        return;
    }
    (void)elaris_ladybird_suspend_downloads(bridge);
    bridge->download_observer.reset();
#if defined(AK_OS_MACOS)
    // Commands and windows must stop before the views they use are destroyed.
    if (bridge->media_link)
        bridge->media_link->live = false;
    bridge->media_controls = nullptr;
#endif
    // Picture-in-Picture windows own views of this bridge, so they close before the other views do.
    WebView::PictureInPictureManager::the().destroy_all_windows();
    while (!bridge->views.empty())
        elaris_ladybird_destroy_view(bridge->views.back());
    delete bridge;
    s_application = nullptr;
    s_download_directory_override.clear();
}

extern "C" char const* elaris_ladybird_last_error(void) {
    return s_last_error.c_str();
}
