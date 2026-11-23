#include <algorithm>
#include <arpa/inet.h>
#include <atomic>
#include <cctype>
#include <chrono>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <netinet/in.h>
#include <string>
#include <string_view>
#include <sys/socket.h>
#include <thread>
#include <unistd.h>
#include <utility>
#include <vector>

#include "dev_options.h"
#include "private_session.h"

using namespace std::chrono_literals;

struct DownloadState {
    uint64_t id{};
    int32_t status{};
    uint64_t bytes{};
    uint64_t total{};
    bool has_total{};
    std::string url;
    std::string filename;
    std::string destination;
    std::string error;
    bool can_resume{};
};

struct Events {
    std::map<uint64_t, DownloadState> downloads;
    /* Downloads the bridge marked private, which only private windows list. */
    std::map<uint64_t, DownloadState> private_downloads;
    std::map<uint64_t, std::string> titles;
    std::map<uint64_t, uint64_t> frames;
    std::map<uint64_t, uint64_t> callbacks;
    bool saw_unknown_length{};
};

static void on_event(void* context, ElarisLadybirdEvent const* event) {
    auto& events = *static_cast<Events*>(context);
    ++events.callbacks[event->view_id];
    if (event->kind == ELARIS_LADYBIRD_TITLE) {
        events.titles[event->view_id] = event->text ? event->text : "";
        return;
    }
    if (event->kind == ELARIS_LADYBIRD_FRAME) {
        ++events.frames[event->view_id];
        return;
    }
    if (event->kind != ELARIS_LADYBIRD_DOWNLOAD)
        return;
    DownloadState state{
        .id = event->download_id,
        .status = event->download_status,
        .bytes = event->bytes_downloaded,
        .total = event->total_bytes,
        .has_total = event->has_total_bytes != 0,
        .url = event->text ? event->text : "",
        .filename = event->selection ? event->selection : "",
        .destination = event->destination ? event->destination : "",
        .error = event->error ? event->error : "",
        .can_resume = (event->value & 2) != 0,
    };
    events.saw_unknown_length |= state.status == ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS && !state.has_total;
    auto& store = (event->value & 1) ? events.private_downloads : events.downloads;
    store[state.id] = std::move(state);
}

class HttpFixture {
  public:
    HttpFixture() {
        m_listener = ::socket(AF_INET, SOCK_STREAM, 0);
        if (m_listener < 0) {
            m_error = "could not create local HTTP fixture socket";
            return;
        }
        int reuse = 1;
        setsockopt(m_listener, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
        sockaddr_in address{};
        address.sin_family = AF_INET;
        address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        address.sin_port = 0;
        if (::bind(m_listener, reinterpret_cast<sockaddr*>(&address), sizeof(address)) != 0 ||
            listen(m_listener, 16) != 0) {
            m_error = "could not start local HTTP fixture server";
            close(m_listener);
            m_listener = -1;
            return;
        }
        socklen_t length = sizeof(address);
        if (getsockname(m_listener, reinterpret_cast<sockaddr*>(&address), &length) != 0) {
            m_error = "could not inspect local HTTP fixture server";
            close(m_listener);
            m_listener = -1;
            return;
        }
        m_base_url = "http://127.0.0.1:" + std::to_string(ntohs(address.sin_port));
        m_accept_thread = std::thread([this] { accept_connections(); });
        m_valid = true;
    }

    ~HttpFixture() {
        m_stopping = true;
        if (m_listener >= 0) {
            shutdown(m_listener, SHUT_RDWR);
            close(m_listener);
        }
        if (m_accept_thread.joinable())
            m_accept_thread.join();
        for (auto& worker : m_workers) {
            if (worker.joinable())
                worker.join();
        }
    }

    std::string url(std::string const& path) const {
        return m_base_url + path;
    }
    bool valid() const {
        return m_valid;
    }
    std::string const& error() const {
        return m_error;
    }

  private:
    void accept_connections() {
        while (!m_stopping) {
            auto client = accept(m_listener, nullptr, nullptr);
            if (client < 0) {
                if (m_stopping)
                    break;
                continue;
            }
#ifdef SO_NOSIGPIPE
            int no_sigpipe = 1;
            setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &no_sigpipe, sizeof(no_sigpipe));
#endif
            std::lock_guard lock(m_workers_lock);
            m_workers.emplace_back([this, client] { respond(client); });
        }
    }

    static bool send_all(int socket, char const* data, size_t size) {
        while (size > 0) {
#ifdef MSG_NOSIGNAL
            auto written = send(socket, data, size, MSG_NOSIGNAL);
#else
            auto written = send(socket, data, size, 0);
#endif
            if (written <= 0)
                return false;
            data += written;
            size -= static_cast<size_t>(written);
        }
        return true;
    }

    void respond(int socket) {
        std::string request;
        char buffer[2048];
        while (request.find("\r\n\r\n") == std::string::npos && request.size() < 16384) {
            auto count = recv(socket, buffer, sizeof(buffer), 0);
            if (count <= 0)
                break;
            request.append(buffer, static_cast<size_t>(count));
        }
        auto first_space = request.find(' ');
        auto second_space = first_space == std::string::npos ? std::string::npos : request.find(' ', first_space + 1);
        auto path = first_space == std::string::npos || second_space == std::string::npos
                        ? std::string{}
                        : request.substr(first_space + 1, second_space - first_space - 1);

        if (path.starts_with("/privacy/")) {
            auto headers = request.substr(0, request.find("\r\n\r\n"));
            std::transform(headers.begin(), headers.end(), headers.begin(),
                           [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
            auto cookie = headers.find("\r\ncookie:");
            auto cookie_line = cookie == std::string::npos
                                   ? std::string{}
                                   : headers.substr(cookie, headers.find("\r\n", cookie + 2) - cookie);
            auto received = cookie_line.find("session=") == std::string::npos ? "empty" : "unexpected";
            auto value = cookie_line.find("session=");
            if (value != std::string::npos && cookie_line.find("session=", value + 1) == std::string::npos) {
                auto token = cookie_line.substr(value, cookie_line.find(';', value) - value);
                if (token == "session=normal")
                    received = "normal";
                if (token == "session=private")
                    received = "private";
            }
            std::string response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n"
                                   "Cache-Control: no-store\r\nConnection: close\r\n";
            std::string body = "<!doctype html><script>";
            if (path == "/privacy/normal" || path == "/privacy/private") {
                auto const value = path.substr(std::string("/privacy/").size());
                response += "Set-Cookie: session=" + value + "; Path=/; HttpOnly\r\n";
                body += "localStorage.setItem('session','" + value +
                        "');"
                        "sessionStorage.setItem('session','" +
                        value + "');";
            }
            body += std::string("document.title = '") + received +
                    ":' + (localStorage.getItem('session') || 'empty') + "
                    "':' + (sessionStorage.getItem('session') || 'empty');</script>";
            response += "Content-Length: " + std::to_string(body.size()) + "\r\n\r\n" + body;
            (void)send_all(socket, response.data(), response.size());
            close(socket);
            return;
        }

        if (path == "/missing") {
            auto response = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found";
            (void)send_all(socket, response, strlen(response));
            close(socket);
            return;
        }

        if (path == "/link-page") {
            std::string body = "<!doctype html><title>Download Link</title><a href=\"/link-file\" "
                               "download=\"link-suggested.txt\" style=\"font-size:24px\">Download fixture</a>";
            auto header = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\nContent-Length: " +
                          std::to_string(body.size()) + "\r\n\r\n";
            (void)send_all(socket, header.data(), header.size());
            (void)send_all(socket, body.data(), body.size());
            close(socket);
            return;
        }

        if (path == "/resumable") {
            serve_ranged(socket, request);
            return;
        }

        auto const is_unknown = path == "/unknown";
        auto const is_slow = path == "/slow" || path == "/slow-attachment";
        auto const is_attachment = path == "/attachment" || path == "/slow-attachment";
        auto const size = is_slow ? 24U * 1024U * 1024U : path == "/large" ? 3U * 1024U * 1024U : 64U * 1024U;
        std::string headers = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/octet-stream\r\n";
        if (!is_unknown)
            headers += "Content-Length: " + std::to_string(size) + "\r\n";
        if (is_attachment)
            headers += "Content-Disposition: attachment; filename=\"page-suggested.txt\"\r\n";
        headers += "\r\n";
        if (!send_all(socket, headers.data(), headers.size())) {
            close(socket);
            return;
        }

        std::string chunk(16U * 1024U, 'x');
        auto remaining = size;
        while (remaining > 0) {
            auto count = std::min(remaining, static_cast<unsigned>(chunk.size()));
            if (!send_all(socket, chunk.data(), count))
                break;
            remaining -= count;
            if (is_slow || path == "/large")
                std::this_thread::sleep_for(1ms);
        }
        shutdown(socket, SHUT_RDWR);
        close(socket);
    }

    // A slow file that honours one `Range: bytes=a-b` request, so a download can pause and resume.
    static void serve_ranged(int socket, std::string const& request) {
        constexpr uint64_t size = 8U * 1024U * 1024U;
        uint64_t first = 0;
        uint64_t last = size - 1;
        auto headers = request.substr(0, request.find("\r\n\r\n"));
        std::transform(headers.begin(), headers.end(), headers.begin(),
                       [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
        constexpr std::string_view range_prefix = "\r\nrange: bytes=";
        auto range = headers.find(range_prefix);
        if (range != std::string::npos) {
            auto start = range + range_prefix.size();
            auto spec = headers.substr(start, headers.find("\r\n", start) - start);
            auto dash = spec.find('-');
            first = std::stoull(spec.substr(0, dash));
            if (dash + 1 < spec.size())
                last = std::min<uint64_t>(std::stoull(spec.substr(dash + 1)), size - 1);
        }
        std::string response = range == std::string::npos ? "HTTP/1.1 200 OK\r\n" : "HTTP/1.1 206 Partial Content\r\n";
        response += "Connection: close\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\n"
                    "ETag: \"resumable-v1\"\r\nCache-Control: no-store\r\n";
        response += "Content-Length: " + std::to_string(last - first + 1) + "\r\n";
        if (range != std::string::npos)
            response += "Content-Range: bytes " + std::to_string(first) + "-" + std::to_string(last) + "/" +
                        std::to_string(size) + "\r\n";
        response += "\r\n";
        if (send_all(socket, response.data(), response.size())) {
            std::string chunk(16U * 1024U, 'r');
            for (auto remaining = last - first + 1; remaining > 0;) {
                auto count = std::min<uint64_t>(remaining, chunk.size());
                if (!send_all(socket, chunk.data(), count))
                    break;
                remaining -= count;
                std::this_thread::sleep_for(1ms);
            }
        }
        shutdown(socket, SHUT_RDWR);
        close(socket);
    }

    int m_listener{-1};
    std::atomic<bool> m_stopping{};
    std::string m_base_url;
    std::thread m_accept_thread;
    std::mutex m_workers_lock;
    std::vector<std::thread> m_workers;
    std::string m_error;
    bool m_valid{};
};

static bool check(int32_t result, char const* operation) {
    if (result == 0)
        return true;
    std::cerr << operation << " failed: " << elaris_ladybird_last_error() << '\n';
    return false;
}

template <typename Predicate>
static bool pump_until(ElarisLadybird* bridge, Predicate predicate, std::chrono::seconds timeout = 30s) {
    auto deadline = std::chrono::steady_clock::now() + timeout;
    while (!predicate()) {
        if (std::chrono::steady_clock::now() >= deadline || !check(elaris_ladybird_pump(bridge), "pump"))
            return false;
        std::this_thread::sleep_for(4ms);
    }
    return true;
}

static bool start(ElarisLadybird* bridge, std::string const& url, char const* filename, uint64_t& id) {
    return check(elaris_ladybird_start_download(bridge, url.c_str(), filename, &id), "start download");
}

static bool finished(Events const& events, uint64_t id, int32_t status) {
    auto it = events.downloads.find(id);
    return it != events.downloads.end() && it->second.status == status;
}

static bool has_temporary_download(std::filesystem::path const& directory, uintmax_t minimum_bytes = 0) {
    std::error_code error;
    std::filesystem::directory_iterator entry(directory, error);
    std::filesystem::directory_iterator end;
    while (!error && entry != end) {
        if (entry->path().extension() == ".download") {
            if (!minimum_bytes)
                return true;
            std::error_code size_error;
            auto size = entry->file_size(size_error);
            if (!size_error && size >= minimum_bytes)
                return true;
        }
        entry.increment(error);
    }
    return false;
}

static bool test_private_session(ElarisLadybird* bridge, HttpFixture const& fixture, Events& events,
                                 std::filesystem::path const& downloads_dir) {
    using View = std::unique_ptr<ElarisLadybirdView, decltype(&elaris_ladybird_destroy_view)>;
    auto create = [&](uint64_t id, bool private_view) {
        ElarisLadybirdView* raw = nullptr;
        auto factory = private_view ? elaris_ladybird_test_create_private_view : elaris_ladybird_create_view;
        check(factory(bridge, id, &raw), "create privacy test view");
        return View(raw, elaris_ladybird_destroy_view);
    };
    auto load = [&](View const& view, uint64_t id, char const* route, char const* expected) {
        if (!view)
            return false;
        events.titles.erase(id);
        if (!check(elaris_ladybird_load(view.get(), fixture.url(route).c_str()), "load privacy probe"))
            return false;
        if (pump_until(bridge, [&] { return events.titles[id] == expected; }))
            return true;
        std::cerr << "Privacy probe " << route << " for view " << id << " expected " << expected << ", observed "
                  << events.titles[id] << '\n';
        return false;
    };

    auto normal = create(201, false);
    if (!load(normal, 201, "/privacy/normal", "empty:normal:normal") ||
        !load(normal, 201, "/privacy/read", "normal:normal:normal"))
        return false;
    auto first = create(202, true);
    auto second = create(203, true);
    if (!load(first, 202, "/privacy/read", "empty:empty:empty") ||
        !load(first, 202, "/privacy/private", "empty:private:private") ||
        !load(second, 203, "/privacy/read", "private:private:empty") ||
        !load(normal, 201, "/privacy/read", "normal:normal:normal"))
        return false;
    if (elaris_ladybird_test_reset_private_session(bridge) == 0) {
        std::cerr << "Private session reset succeeded while views remained open\n";
        return false;
    }
    first.reset();
    if (!load(second, 203, "/privacy/read-again", "private:private:empty"))
        return false;
    auto private_download = downloads_dir / "private-probe.bin";
    auto download_url = fixture.url("/small");
    if (!check(elaris_ladybird_test_start_private_download(second.get(), download_url.c_str(), "private-probe.bin"),
               "start private download"))
        return false;
    // The file and its private-download event arrive on separate pumps, so wait for both.
    if (!pump_until(bridge,
                    [&] { return std::filesystem::exists(private_download) && !events.private_downloads.empty(); })) {
        std::cerr << "Private download did not finish or was not reported as private\n";
        return false;
    }
    if (!events.downloads.empty()) {
        std::cerr << "Private download reached the ordinary download list\n";
        return false;
    }
    if (!elaris_ladybird_test_private_request_servers(bridge)) {
        std::cerr << "Privacy probe did not create a private RequestServer\n";
        return false;
    }
    auto active_path = downloads_dir / "private-active.bin";
    if (!check(elaris_ladybird_test_start_private_download(second.get(), fixture.url("/slow").c_str(),
                                                           "private-active.bin"),
               "start active private download") ||
        !pump_until(bridge, [&] { return has_temporary_download(downloads_dir, 1); }) ||
        std::filesystem::exists(active_path)) {
        std::cerr << "Expected a private download with unfinished staging data\n";
        return false;
    }
    second.reset();
    auto closed_callbacks = events.callbacks[203];
    if (!check(elaris_ladybird_test_reset_private_session(bridge), "reset private browsing session"))
        return false;

    if (elaris_ladybird_test_private_request_servers(bridge)) {
        std::cerr << "A private RequestServer survived reset\n";
        return false;
    }
    if (!std::filesystem::exists(private_download)) {
        std::cerr << "Private reset removed a completed download\n";
        return false;
    }
    for (uint64_t id = 204; id < 207; ++id) {
        auto fresh = create(id, true);
        if (!load(fresh, id, "/privacy/read", "empty:empty:empty") ||
            !load(fresh, id, "/privacy/private", "empty:private:private"))
            return false;
        fresh.reset();
        if (!check(elaris_ladybird_test_reset_private_session(bridge), "reset after reopening private session"))
            return false;
        if (elaris_ladybird_test_private_request_servers(bridge)) {
            std::cerr << "A private RequestServer survived a subsequent reset\n";
            return false;
        }
    }
    if (!load(normal, 201, "/privacy/read", "normal:normal:normal"))
        return false;
    if (std::filesystem::exists(active_path) || has_temporary_download(downloads_dir) ||
        events.callbacks[203] != closed_callbacks || !events.downloads.empty()) {
        std::cerr << "Private reset left download data or delivered callbacks from the closed session\n";
        return false;
    }
    return true;
}

int main(int argc, char** argv) {
    apply_dev_options_from_environment();
    if (argc != 3) {
        std::cerr << "usage: elaris_ladybird_download_smoke RESOURCE_DIR ISOLATED_TEST_ROOT\n";
        return 2;
    }

    auto root = std::filesystem::weakly_canonical(std::filesystem::absolute(argv[2]));
    auto profile = root / "profile";
    auto downloads_dir = root / "Downloads";
    auto home = std::getenv("HOME");
    std::error_code filesystem_error;
    if (home) {
        auto user_downloads = std::filesystem::weakly_canonical(std::filesystem::path(home) / "Downloads");
        auto relative = root.lexically_relative(user_downloads);
        if (relative.empty() || *relative.begin() != "..") {
            std::cerr << "download smoke root is inside the user's Downloads directory\n";
            return 1;
        }
    }
    if (profile == downloads_dir) {
        std::cerr << "download smoke paths are not isolated\n";
        return 1;
    }
    std::filesystem::remove_all(root, filesystem_error);
    if (filesystem_error) {
        std::cerr << "could not clear isolated download smoke root: " << filesystem_error.message() << '\n';
        return 1;
    }
    std::filesystem::create_directories(downloads_dir, filesystem_error);
    if (filesystem_error) {
        std::cerr << "could not create isolated Downloads directory: " << filesystem_error.message() << '\n';
        return 1;
    }
    std::filesystem::create_directories(profile, filesystem_error);
    if (filesystem_error) {
        std::cerr << "could not create isolated profile directory: " << filesystem_error.message() << '\n';
        return 1;
    }

    HttpFixture fixture;
    if (!fixture.valid()) {
        std::cerr << fixture.error() << '\n';
        return 1;
    }
    Events events;
    ElarisLadybird* bridge = nullptr;
    if (!check(elaris_ladybird_create(argv[1], profile.c_str(), on_event, &events, &bridge), "initialize") ||
        !check(elaris_ladybird_set_download_directory(bridge, downloads_dir.c_str()), "isolate Downloads"))
        return 1;

    bool passed = test_private_session(bridge, fixture, events, downloads_dir);
    uint64_t unsafe_id{};
    passed = passed && start(bridge, fixture.url("/small"), "../../unsafe.txt", unsafe_id) &&
             pump_until(bridge, [&] { return finished(events, unsafe_id, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED); });
    auto unsafe = events.downloads[unsafe_id];
    if (passed &&
        (unsafe.filename != "unsafe.txt" || std::filesystem::path(unsafe.destination).parent_path() != downloads_dir ||
         !std::filesystem::exists(unsafe.destination))) {
        std::cerr << "unsafe filename escaped the isolated destination or did not complete\n";
        passed = false;
    }

    uint64_t duplicate_one{};
    uint64_t duplicate_two{};
    passed = passed && start(bridge, fixture.url("/large"), "same.bin", duplicate_one) &&
             start(bridge, fixture.url("/large"), "same.bin", duplicate_two) && pump_until(bridge, [&] {
                 return finished(events, duplicate_one, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED) &&
                        finished(events, duplicate_two, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED);
             });
    if (passed && events.downloads[duplicate_one].destination == events.downloads[duplicate_two].destination) {
        std::cerr << "duplicate downloads overwrote one another\n";
        passed = false;
    }

    uint64_t unknown_id{};
    passed = passed && start(bridge, fixture.url("/unknown"), "unknown.bin", unknown_id) &&
             pump_until(bridge, [&] { return finished(events, unknown_id, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED); });
    if (passed && !events.saw_unknown_length) {
        std::cerr << "unknown content length was never reported as unknown\n";
        passed = false;
    }

    ElarisLadybirdView* link_view = nullptr;
    if (passed)
        passed = check(elaris_ladybird_create_view(bridge, 98, &link_view), "create download link tab") &&
                 check(elaris_ladybird_resize(link_view, 640, 480, 1), "size download link tab") &&
                 check(elaris_ladybird_load(link_view, fixture.url("/link-page").c_str()), "load download link page") &&
                 pump_until(bridge, [&] { return events.titles[98] == "Download Link" && events.frames[98] > 0; });
    if (passed) {
        ElarisLadybirdMouseEvent click{};
        click.type = ELARIS_LADYBIRD_MOUSE_DOWN;
        click.x = 42;
        click.y = 20;
        click.button = ELARIS_LADYBIRD_BUTTON_PRIMARY;
        click.buttons = ELARIS_LADYBIRD_BUTTON_PRIMARY;
        click.click_count = 1;
        passed = check(elaris_ladybird_mouse_event(link_view, &click), "press download link");
        click.type = ELARIS_LADYBIRD_MOUSE_UP;
        click.buttons = 0;
        passed = passed && check(elaris_ladybird_mouse_event(link_view, &click), "release download link");
    }
    uint64_t link_download_id{};
    if (passed)
        passed = pump_until(bridge, [&] {
            for (auto const& [id, state] : events.downloads) {
                if (state.url.find("/link-file") != std::string::npos &&
                    state.status == ELARIS_LADYBIRD_DOWNLOAD_COMPLETED) {
                    link_download_id = id;
                    return true;
                }
            }
            return false;
        });
    if (passed && events.downloads[link_download_id].filename != "link-suggested.txt") {
        std::cerr << "download attribute filename did not reach the download observer\n";
        passed = false;
    }
    if (link_view)
        elaris_ladybird_destroy_view(link_view);

    uint64_t missing_id{};
    passed = passed && start(bridge, fixture.url("/missing"), "missing.bin", missing_id) &&
             pump_until(bridge, [&] { return finished(events, missing_id, ELARIS_LADYBIRD_DOWNLOAD_FAILED); });
    if (passed && events.downloads[missing_id].error.empty()) {
        std::cerr << "failed download did not report an error\n";
        passed = false;
    }
    if (passed &&
        (std::filesystem::exists(events.downloads[missing_id].destination) || has_temporary_download(downloads_dir))) {
        std::cerr << "failed download left a final or temporary file behind\n";
        passed = false;
    }

    uint64_t cancelled_id{};
    passed = passed && start(bridge, fixture.url("/slow"), "cancelled.bin", cancelled_id) && pump_until(bridge, [&] {
                 auto it = events.downloads.find(cancelled_id);
                 return it != events.downloads.end() && it->second.status == ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS &&
                        it->second.bytes > 0;
             });
    if (passed)
        passed = check(elaris_ladybird_cancel_download(bridge, cancelled_id), "cancel download") &&
                 pump_until(bridge, [&] { return finished(events, cancelled_id, ELARIS_LADYBIRD_DOWNLOAD_CANCELLED); });
    if (passed && (std::filesystem::exists(events.downloads[cancelled_id].destination) ||
                   has_temporary_download(downloads_dir))) {
        std::cerr << "cancelled download left a final or temporary file behind\n";
        passed = false;
    }

    ElarisLadybirdView* view = nullptr;
    if (passed)
        passed = check(elaris_ladybird_create_view(bridge, 99, &view), "create download source tab") &&
                 check(elaris_ladybird_load(view, fixture.url("/slow-attachment").c_str()), "load attachment URL") &&
                 pump_until(bridge, [&] {
                     return std::any_of(events.downloads.begin(), events.downloads.end(), [](auto const& entry) {
                         auto const& state = entry.second;
                         return state.url.find("/slow-attachment") != std::string::npos &&
                                state.status == ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS && state.bytes > 0;
                     });
                 });
    uint64_t attachment_id{};
    if (passed) {
        for (auto const& [id, state] : events.downloads) {
            if (state.url.find("/slow-attachment") != std::string::npos) {
                attachment_id = id;
                break;
            }
        }
        elaris_ladybird_destroy_view(view);
        passed =
            pump_until(bridge, [&] { return finished(events, attachment_id, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED); });
        if (passed && events.downloads[attachment_id].filename != "page-suggested.txt") {
            std::cerr << "Content-Disposition filename did not reach the download observer\n";
            passed = false;
        }
    } else if (view) {
        elaris_ladybird_destroy_view(view);
    }

    uint64_t ranged_id{};
    passed = passed && start(bridge, fixture.url("/resumable"), "ranged.bin", ranged_id) && pump_until(bridge, [&] {
                 auto it = events.downloads.find(ranged_id);
                 return it != events.downloads.end() && it->second.can_resume && it->second.bytes > 0;
             });
    passed = passed && check(elaris_ladybird_pause_download(bridge, ranged_id, 1), "pause download") &&
             pump_until(bridge, [&] { return finished(events, ranged_id, ELARIS_LADYBIRD_DOWNLOAD_PAUSED); });
    passed = passed && check(elaris_ladybird_pause_download(bridge, ranged_id, 0), "resume download") &&
             pump_until(bridge, [&] { return finished(events, ranged_id, ELARIS_LADYBIRD_DOWNLOAD_COMPLETED); });
    if (passed) {
        auto const& ranged = events.downloads[ranged_id];
        std::error_code error;
        auto size = std::filesystem::file_size(ranged.destination, error);
        if (error || size != 8U * 1024U * 1024U) {
            std::cerr << "a paused and resumed download did not finish whole: " << size << " bytes\n";
            passed = false;
        }
    } else {
        auto const& ranged = events.downloads[ranged_id];
        std::cerr << "a resumable download did not pause and resume: status " << ranged.status << ", can resume "
                  << ranged.can_resume << ", " << ranged.bytes << " bytes, error '" << ranged.error << "'\n";
    }

    uint64_t shutdown_id{};
    if (passed)
        passed = start(bridge, fixture.url("/slow"), "shutdown.bin", shutdown_id) &&
                 pump_until(bridge,
                            [&] {
                                auto it = events.downloads.find(shutdown_id);
                                return it != events.downloads.end() &&
                                       it->second.status == ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS &&
                                       it->second.bytes > 0;
                            }) &&
                 check(elaris_ladybird_cancel_active_downloads(bridge), "cancel on shutdown") &&
                 pump_until(bridge, [&] { return finished(events, shutdown_id, ELARIS_LADYBIRD_DOWNLOAD_CANCELLED); });
    if (passed && has_temporary_download(downloads_dir)) {
        std::cerr << "shutdown cancellation left a temporary file behind\n";
        passed = false;
    }

    uint64_t destroy_id{};
    if (passed)
        passed = start(bridge, fixture.url("/slow"), "destroy.bin", destroy_id) && pump_until(bridge, [&] {
                     auto it = events.downloads.find(destroy_id);
                     return it != events.downloads.end() && it->second.status == ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS &&
                            it->second.bytes > 0;
                 });

    if (passed) {
        auto final_path = std::filesystem::path(events.downloads[destroy_id].destination);
        elaris_ladybird_destroy(bridge);
        bridge = nullptr;
        if (!finished(events, destroy_id, ELARIS_LADYBIRD_DOWNLOAD_CANCELLED) || std::filesystem::exists(final_path) ||
            has_temporary_download(downloads_dir)) {
            std::cerr << "destroying the bridge did not safely cancel the active download\n";
            passed = false;
        }
    }

    if (bridge)
        elaris_ladybird_destroy(bridge);
    std::filesystem::remove_all(root, filesystem_error);
    return passed ? 0 : 1;
}
