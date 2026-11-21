// SPDX-License-Identifier: GPL-3.0-only

#include <AK/ByteString.h>

#include <gio/gio.h>

#include "external_url_handler.h"

namespace Elaris {

static constexpr auto elaris_desktop_id = "org.elarisbrowser.Elaris.desktop"sv;

class GioExternalURLHandler final : public WebView::ExternalURLHandler {
  public:
    GioExternalURLHandler(String name, bool is_elaris, GAppInfo* application)
        : WebView::ExternalURLHandler(move(name), is_elaris), m_application(application) {}

    ~GioExternalURLHandler() override {
        g_object_unref(m_application);
    }

    void launch(URL::URL const& url, LaunchCallback callback) const override {
        auto uri = url.serialize().to_byte_string();
        GList uris{const_cast<char*>(uri.characters()), nullptr, nullptr};
        GError* error = nullptr;
        bool launched = g_app_info_launch_uris(m_application, &uris, nullptr, &error);
        if (error)
            g_error_free(error);
        callback(launched);
    }

  private:
    GAppInfo* m_application;
};

void resolve_external_url_handler(URL::URL const& url, WebView::ExternalURLHandlerCallback callback) {
    auto scheme = url.scheme().to_byte_string();
    GAppInfo* application = g_app_info_get_default_for_uri_scheme(scheme.characters());
    if (!application) {
        callback(nullptr);
        return;
    }
    char const* id = g_app_info_get_id(application);
    char const* name = g_app_info_get_display_name(application);
    auto display_name = name ? MUST(String::from_utf8(StringView{name, strlen(name)})) : "another application"_string;
    bool is_elaris = id && StringView{id, strlen(id)} == elaris_desktop_id;
    callback(adopt_ref(*new GioExternalURLHandler(move(display_name), is_elaris, application)));
}

} // namespace Elaris
