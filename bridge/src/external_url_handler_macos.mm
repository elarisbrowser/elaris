// SPDX-License-Identifier: GPL-3.0-only

#include "external_url_handler.h"

#include <AK/ByteString.h>
#include <LibCore/EventLoop.h>

#import <Cocoa/Cocoa.h>

namespace Elaris {

static NSString* ns_string(StringView string) {
    return [[NSString alloc] initWithBytes:string.characters_without_null_termination()
                                    length:string.length()
                                  encoding:NSUTF8StringEncoding];
}

static String from_ns_string(NSString* string) {
    char const* utf8 = [string UTF8String];
    return MUST(String::from_utf8(StringView{utf8, strlen(utf8)}));
}

class AppKitExternalURLHandler final : public WebView::ExternalURLHandler {
  public:
    AppKitExternalURLHandler(String name, bool is_elaris, NSURL* application)
        : WebView::ExternalURLHandler(move(name), is_elaris), m_application(application) {}

    void launch(URL::URL const& url, LaunchCallback callback) const override {
        NSURL* target = [NSURL URLWithString:ns_string(url.serialize().bytes_as_string_view())];
        if (!target) {
            callback(false);
            return;
        }
        // NSWorkspace replies on its own queue; the answer is handed back to the owner loop.
        auto loop = Core::EventLoop::current_weak();
        __block auto pending = move(callback);
        [[NSWorkspace sharedWorkspace] openURLs:@[ target ]
                           withApplicationAtURL:m_application
                                  configuration:[NSWorkspaceOpenConfiguration configuration]
                              completionHandler:^(NSRunningApplication* application, NSError* error) {
                                  auto owner = loop->take();
                                  if (!owner.is_alive())
                                      return;
                                  owner->deferred_invoke([callback = move(pending), success = application && !error] {
                                      callback(success);
                                  });
                                  owner->wake();
                              }];
    }

  private:
    NSURL* m_application;
};

void resolve_external_url_handler(URL::URL const& url, WebView::ExternalURLHandlerCallback callback) {
    NSURL* target = [NSURL URLWithString:ns_string(url.serialize().bytes_as_string_view())];
    NSURL* application = target ? [[NSWorkspace sharedWorkspace] URLForApplicationToOpenURL:target] : nil;
    if (!application) {
        callback(nullptr);
        return;
    }
    NSString* name = [[NSFileManager defaultManager] displayNameAtPath:[application path]];
    NSURL* handler_bundle = [application URLByResolvingSymlinksInPath];
    NSURL* own_bundle = [[[NSBundle mainBundle] bundleURL] URLByResolvingSymlinksInPath];
    callback(adopt_ref(*new AppKitExternalURLHandler(name ? from_ns_string(name) : "another application"_string,
                                                     [handler_bundle isEqual:own_bundle], application)));
}

} // namespace Elaris
