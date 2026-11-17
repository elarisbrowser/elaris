#include <elaris_ladybird.h>

#include <AK/ByteString.h>
#include <AK/LexicalPath.h>
#include <AK/OwnPtr.h>
#include <AK/String.h>
#include <AK/Utf16String.h>

#include <LibCore/EventLoop.h>
#include <LibCore/Timer.h>
#include <LibGfx/Bitmap.h>
#include <LibGfx/SharedImageBuffer.h>
#include <LibGfx/SystemTheme.h>
#include <LibURL/Parser.h>
#include <LibURL/URL.h>
#include <LibWebCommon/Page/InputEvent.h>
#include <LibWebCommon/PixelUnits.h>
#include <LibWebCommon/UIEvents/KeyCode.h>
#include <LibWebCommon/WebView/Utilities.h>
#include <LibWebView/Application.h>
#include <LibWebView/HeadlessWebView.h>
#include <LibWebView/PageBackingStores.h>
#include <LibWebView/Settings.h>

#include <algorithm>
#include <array>
#include <cstring>
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

struct ElarisLadybird {
    ElarisLadybirdEventCallback callback{};
    void* context{};
    std::thread::id owner_thread;
    std::vector<ElarisLadybirdView*> views;
};

struct ElarisLadybirdView {
    ElarisLadybird* bridge{};
    uint64_t id{};
    AK::NonnullOwnPtr<ElarisWebView> view;
    Gfx::IntSize copied_size;
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
    ElarisLadybirdEvent event{kind, view->id, text.c_str(), value, nullptr, 0, 0, 0};
    view->bridge->callback(view->bridge->context, &event);
}

static void emit_favicon(ElarisLadybirdView const* view, Optional<Gfx::Bitmap const&> favicon) {
    if (!view->bridge->callback)
        return;

    std::vector<uint8_t> rgba;
    ElarisLadybirdEvent event{ELARIS_LADYBIRD_FAVICON, view->id, "", 0, nullptr, 0, 0, 0};
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
    // Ladybird's platform initialization during create() resets the root from its own search, so set it again.
    WebView::s_ladybird_resource_root = resource_dir;
    s_event_loop = &Core::EventLoop::current();
    s_event_loop_thread = std::this_thread::get_id();
    *out_bridge = new ElarisLadybird{callback, context, std::this_thread::get_id(), {}};
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

    auto view = ElarisWebView::create(theme.release_value(), {1024, 768});
    auto* handle = new ElarisLadybirdView{bridge, view_id, AK::move(view), {}};
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
    *out_view = handle;
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

extern "C" int32_t elaris_ladybird_reload(ElarisLadybirdView* handle) {
    if (!handle)
        return fail("view is required");
    if (!on_owner_thread(handle->bridge))
        return fail("navigation must run on the initialization thread");
    handle->view->reload();
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
    case ELARIS_LADYBIRD_KEY_PAGE_DOWN:
        key = Web::UIEvents::KeyCode::Key_PageDown;
        break;
    default:
        return fail("unknown key");
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
