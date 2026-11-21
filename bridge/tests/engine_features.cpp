// Checks two engine behaviours that Elaris relies on without any code of its own: the bundled site compatibility
// rules load and override the user agent for the sites they name, and a PDF opens in Ladybird's pdf.js viewer
// instead of downloading.

#include <elaris_ladybird.h>

#include <AK/JsonValue.h>

#include <LibURL/Parser.h>
#include <LibWebCommon/Loader/SiteCompatibility.h>
#include <LibWebCommon/Loader/UserAgent.h>
#include <LibWebView/Application.h>

#include <chrono>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <string>
#include <thread>

namespace {

struct Events {
    std::string title;
    bool saw_viewer_title = false;
    bool downloaded = false;
};

void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = *static_cast<Events*>(context);
    if (event->kind == ELARIS_LADYBIRD_DOWNLOAD)
        events.downloaded = true;
    if (event->kind != ELARIS_LADYBIRD_TITLE)
        return;
    events.title = event->text ? event->text : "";
    events.saw_viewer_title = events.saw_viewer_title || events.title == "PDF.js viewer";
}

bool check(int32_t result, char const* operation) {
    if (result == 0)
        return true;
    std::cerr << operation << " failed: " << elaris_ladybird_last_error() << '\n';
    return false;
}

// Applies the bundled rules to a page origin, the way a fetch from that page does.
bool user_agent_for(WebView::Application const& application, char const* page, String& user_agent) {
    auto url = URL::Parser::basic_parse(StringView(page, strlen(page)));
    if (!url.has_value())
        return false;
    auto data = Web::SiteCompatibilityData::from_json(application.site_compatibility_data());
    if (data.is_error())
        return false;
    auto origin = url->origin();
    user_agent = data.value().user_agent_for(origin, origin, Web::default_user_agent);
    return true;
}

bool site_compatibility_loads() {
    auto const& application = WebView::Application::the();
    String cnn_user_agent;
    String example_user_agent;
    if (!application.site_compatibility_data().is_array() ||
        application.site_compatibility_data().as_array().is_empty()) {
        std::cerr << "the bundled site compatibility data did not load\n";
        return false;
    }
    if (!user_agent_for(application, "https://cnn.com/", cnn_user_agent) ||
        !user_agent_for(application, "https://example.com/", example_user_agent)) {
        std::cerr << "could not apply the site compatibility data\n";
        return false;
    }
    if (cnn_user_agent.bytes_as_string_view().contains("Ladybird"sv)) {
        std::cerr << "cnn.com should hide Ladybird from its user agent\n";
        return false;
    }
    if (!example_user_agent.bytes_as_string_view().contains("Ladybird"sv)) {
        std::cerr << "example.com should keep Ladybird in its user agent\n";
        return false;
    }
    return true;
}

bool pdf_opens_in_viewer(ElarisLadybird* bridge, Events& events, std::string const& url) {
    ElarisLadybirdView* view = nullptr;
    if (!check(elaris_ladybird_create_view(bridge, 1, &view), "create view") ||
        !check(elaris_ladybird_resize(view, 800, 600, 1), "resize view") ||
        !check(elaris_ladybird_load(view, url.c_str()), "load PDF"))
        return false;

    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(30);
    while (!(events.saw_viewer_title && events.title == "sample.pdf") && std::chrono::steady_clock::now() < deadline) {
        if (!check(elaris_ladybird_pump(bridge), "pump"))
            break;
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    elaris_ladybird_destroy_view(view);

    if (events.downloaded) {
        std::cerr << "the PDF downloaded instead of opening in the viewer\n";
        return false;
    }
    if (!events.saw_viewer_title || events.title != "sample.pdf") {
        std::cerr << "the PDF viewer did not open the document; last title '" << events.title << "'\n";
        return false;
    }
    return true;
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 4) {
        std::cerr << "usage: elaris_ladybird_engine_features RESOURCE_DIR PROFILE_DIR PDF_FILE\n";
        return 2;
    }

    Events events;
    ElarisLadybird* bridge = nullptr;
    auto profile_dir = std::filesystem::absolute(argv[2]).string();
    if (!check(elaris_ladybird_create(argv[1], profile_dir.c_str(), on_event, &events, &bridge), "initialize"))
        return 1;

    auto pdf_url = "file://" + std::filesystem::absolute(argv[3]).string();
    bool passed = site_compatibility_loads() && pdf_opens_in_viewer(bridge, events, pdf_url);
    elaris_ladybird_destroy(bridge);
    return passed ? 0 : 1;
}
