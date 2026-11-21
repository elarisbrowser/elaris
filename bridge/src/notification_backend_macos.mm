// SPDX-License-Identifier: GPL-3.0-only
//
// Adapted from Ladybird's NotificationBackendMacOS.mm (BSD-2-Clause, Copyright (c) 2026-present,
// the Ladybird developers) so replies run on the Ladybird owner thread rather than the main queue.

#include "notification_backend_macos.h"

#include <AK/ByteString.h>
#include <AK/HashMap.h>
#include <AK/HashTable.h>
#include <LibCore/EventLoop.h>
#include <LibGfx/ImageFormats/PNGWriter.h>
#include <LibWebView/NotificationAttribution.h>

#include <memory>
#include <optional>

#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>

static NSString* const ElarisNotificationCategory = @"org.elarisbrowser.Elaris.web-notification";

namespace Elaris {

class NotificationBackendMacOS;

// Runs work on the owner loop only while the backend is alive. Both checks happen on that thread.
struct OwnerLoop {
    NonnullRefPtr<Core::WeakEventLoopReference> loop;
    std::weak_ptr<NotificationBackendMacOS*> backend;

    void post(Function<void(NotificationBackendMacOS&)> work) const {
        auto strong = loop->take();
        if (!strong.is_alive())
            return;
        strong->deferred_invoke([backend = backend, work = move(work)] {
            if (auto alive = backend.lock())
                work(**alive);
        });
        strong->wake();
    }
};

}

@interface ElarisNotificationCenterDelegate : NSObject <UNUserNotificationCenterDelegate>
- (instancetype)initWithOwner:(Elaris::OwnerLoop)owner;
@end

namespace Elaris {

static NSString* ns_string(String const& string) {
    return [NSString stringWithUTF8String:string.to_byte_string().characters()];
}

static String from_ns_string(NSString* string) {
    char const* utf8 = [string UTF8String];
    return MUST(String::from_utf8(StringView{utf8, strlen(utf8)}));
}

class NotificationBackendMacOS final : public WebView::NotificationBackend {
  public:
    NotificationBackendMacOS()
        : m_self(std::make_shared<NotificationBackendMacOS*>(this)),
          m_owner{Core::EventLoop::current_weak(), m_self} {
        if (![[NSBundle mainBundle] bundleIdentifier]) {
            dbgln("Notifications: Elaris is not running from an application bundle");
            return;
        }
        m_center = [UNUserNotificationCenter currentNotificationCenter];
        m_delegate = [[ElarisNotificationCenterDelegate alloc] initWithOwner:m_owner];
        m_center.delegate = m_delegate;
        m_launch_token = from_ns_string([[NSUUID UUID] UUIDString]);
        // The center reports a user's dismissal only for a category that asks for it.
        auto* category = [UNNotificationCategory categoryWithIdentifier:ElarisNotificationCategory
                                                                actions:@[]
                                                      intentIdentifiers:@[]
                                                                options:UNNotificationCategoryOptionCustomDismissAction];
        [m_center setNotificationCategories:[NSSet setWithObject:category]];
    }

    ~NotificationBackendMacOS() override {
        if (m_center && m_center.delegate == m_delegate)
            m_center.delegate = nil;
    }

    bool os_authorization_is_undetermined() const override { return m_authorization == Authorization::Unknown; }

    void show(WebView::PlatformNotification const& notification) override {
        if (!m_center) {
            if (on_failed)
                on_failed(notification.id);
            return;
        }
        request_authorization_then([notification](NotificationBackendMacOS& self, bool granted) {
            if (self.m_closed_before_shown.remove(notification.id))
                return;
            if (!granted) {
                if (self.on_failed)
                    self.on_failed(notification.id);
                return;
            }
            self.deliver(notification);
        });
    }

    void close(u64 id) override {
        auto identifier = m_identifier_for_id.take(id);
        if (!identifier.has_value()) {
            // Not handed to the center yet; the pending show finds this and does nothing.
            if (m_authorization == Authorization::Pending) {
                m_closed_before_shown.set(id);
                if (on_closed)
                    on_closed(id, ByUser::No);
            }
            return;
        }
        m_id_for_identifier.remove(*identifier);
        NSArray* identifiers = @[ ns_string(*identifier) ];
        [m_center removeDeliveredNotificationsWithIdentifiers:identifiers];
        [m_center removePendingNotificationRequestsWithIdentifiers:identifiers];
        if (on_closed)
            on_closed(id, ByUser::No);
    }

    void did_respond(String const& identifier, bool dismissed) {
        auto id = m_id_for_identifier.take(identifier);
        if (!id.has_value())
            return;
        m_identifier_for_id.remove(*id);
        if (!dismissed && on_clicked)
            on_clicked(*id);
        if (on_closed)
            on_closed(*id, ByUser::Yes);
    }

  private:
    enum class Authorization { Unknown, Pending, Granted, Denied };

    void request_authorization_then(Function<void(NotificationBackendMacOS&, bool)> then) {
        switch (m_authorization) {
        case Authorization::Granted:
        case Authorization::Denied:
            then(*this, m_authorization == Authorization::Granted);
            return;
        case Authorization::Pending:
            m_waiting.append(move(then));
            return;
        case Authorization::Unknown:
            break;
        }
        m_authorization = Authorization::Pending;
        m_waiting.append(move(then));
        auto owner = m_owner;
        [m_center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                                completionHandler:^(BOOL granted, NSError*) {
                                    owner.post([granted = static_cast<bool>(granted)](auto& self) {
                                        self.m_authorization = granted ? Authorization::Granted : Authorization::Denied;
                                        for (auto& waiting : exchange(self.m_waiting, {}))
                                            waiting(self, granted);
                                    });
                                }];
    }

    void deliver(WebView::PlatformNotification const& notification) {
        auto* content = [[UNMutableNotificationContent alloc] init];
        content.title = ns_string(WebView::platform_text(notification.data.title));
        content.subtitle = ns_string(notification.attribution);
        content.body = ns_string(WebView::platform_text(notification.data.body));
        content.threadIdentifier = ns_string(notification.attribution);
        content.categoryIdentifier = ElarisNotificationCategory;
        if (notification.data.silent != true)
            content.sound = [UNNotificationSound defaultSound];
        bool replaces = notification.replaces.has_value();
        if (replaces && !notification.data.renotify)
            content.interruptionLevel = UNNotificationInterruptionLevelPassive;

        NSURL* icon_file = attach_icon(content, notification);

        // Reusing the replaced notification's identifier makes the center swap it in place.
        String identifier;
        if (replaces)
            identifier = m_identifier_for_id.take(*notification.replaces).value_or({});
        if (identifier.is_empty())
            identifier = MUST(String::formatted("{}-{}", m_launch_token, notification.id));
        m_identifier_for_id.set(notification.id, identifier);
        m_id_for_identifier.set(identifier, notification.id);

        auto* request = [UNNotificationRequest requestWithIdentifier:ns_string(identifier) content:content trigger:nil];
        auto owner = m_owner;
        auto id = notification.id;
        [m_center addNotificationRequest:request
                   withCompletionHandler:^(NSError* error) {
                       if (icon_file)
                           [[NSFileManager defaultManager] removeItemAtURL:icon_file error:nil];
                       owner.post([id, failed = error != nil](auto& self) {
                           if (failed) {
                               if (auto identifier = self.m_identifier_for_id.take(id); identifier.has_value())
                                   self.m_id_for_identifier.remove(*identifier);
                               if (self.on_failed)
                                   self.on_failed(id);
                           } else if (self.on_shown) {
                               self.on_shown(id);
                           }
                       });
                   }];
    }

    // The center copies attachments from files, so the icon is written to a temporary PNG.
    static NSURL* attach_icon(UNMutableNotificationContent* content, WebView::PlatformNotification const& notification) {
        if (!notification.icon)
            return nil;
        auto png = Gfx::PNGWriter::encode(*notification.icon);
        if (png.is_error())
            return nil;
        auto path = ByteString::formatted("{}elaris-notification-icon-{}.png", [NSTemporaryDirectory() UTF8String],
                                          notification.id);
        NSURL* file = [NSURL fileURLWithPath:[NSString stringWithUTF8String:path.characters()]];
        NSData* data = [NSData dataWithBytes:png.value().data() length:png.value().size()];
        if (![data writeToURL:file atomically:YES])
            return nil;
        if (auto* attachment = [UNNotificationAttachment attachmentWithIdentifier:@"icon" URL:file options:nil error:nil])
            content.attachments = @[ attachment ];
        return file;
    }

    std::shared_ptr<NotificationBackendMacOS*> m_self;
    OwnerLoop m_owner;
    UNUserNotificationCenter* m_center{nil};
    ElarisNotificationCenterDelegate* m_delegate{nil};
    String m_launch_token;
    Authorization m_authorization{Authorization::Unknown};
    Vector<Function<void(NotificationBackendMacOS&, bool)>> m_waiting;
    // Identifiers carry this launch's token, so a click on a notification left over from an earlier run is ignored.
    HashMap<String, u64> m_id_for_identifier;
    HashMap<u64, String> m_identifier_for_id;
    HashTable<u64> m_closed_before_shown;
};

NonnullOwnPtr<WebView::NotificationBackend> make_notification_backend_macos() {
    return make<NotificationBackendMacOS>();
}

}

@implementation ElarisNotificationCenterDelegate {
    std::optional<Elaris::OwnerLoop> _owner;
}

- (instancetype)initWithOwner:(Elaris::OwnerLoop)owner {
    if ((self = [super init]))
        _owner = owner;
    return self;
}

// Notifications show as banners even while Elaris is frontmost.
- (void)userNotificationCenter:(UNUserNotificationCenter*)center
       willPresentNotification:(UNNotification*)notification
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completionHandler {
    completionHandler(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionSound);
}

- (void)userNotificationCenter:(UNUserNotificationCenter*)center
    didReceiveNotificationResponse:(UNNotificationResponse*)response
             withCompletionHandler:(void (^)(void))completionHandler {
    auto identifier = Elaris::from_ns_string(response.notification.request.identifier);
    bool dismissed = [response.actionIdentifier isEqualToString:UNNotificationDismissActionIdentifier];
    _owner->post([identifier, dismissed](auto& self) { self.did_respond(identifier, dismissed); });
    completionHandler();
}

@end
