// SPDX-License-Identifier: GPL-3.0-only

#include "media_controls.h"

#include <AK/Function.h>

#import <Cocoa/Cocoa.h>
#import <MediaPlayer/MediaPlayer.h>

namespace Elaris {

static void run_on_main(void (^work)(void)) {
    if ([NSThread isMainThread])
        work();
    else
        dispatch_sync(dispatch_get_main_queue(), work);
}

struct MediaImplementation {
    AK::Function<void(MediaCommand)> on_command;
    id play_target{};
    id pause_target{};
    id toggle_target{};
};

NonnullOwnPtr<MediaControls> MediaControls::create(AK::Function<void(MediaCommand)> on_command) {
    auto* implementation = new MediaImplementation{};
    implementation->on_command = AK::move(on_command);
    run_on_main(^{
        MPRemoteCommandCenter* center = MPRemoteCommandCenter.sharedCommandCenter;
        // The handlers run on the main thread and only forward the command, so the pointer stays valid until they are removed.
        implementation->play_target = [center.playCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent*) {
            implementation->on_command(MediaCommand::Play);
            return MPRemoteCommandHandlerStatusSuccess;
        }];
        implementation->pause_target = [center.pauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent*) {
            implementation->on_command(MediaCommand::Pause);
            return MPRemoteCommandHandlerStatusSuccess;
        }];
        implementation->toggle_target = [center.togglePlayPauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(MPRemoteCommandEvent*) {
            implementation->on_command(MediaCommand::Toggle);
            return MPRemoteCommandHandlerStatusSuccess;
        }];
        center.playCommand.enabled = YES;
        center.pauseCommand.enabled = YES;
        center.togglePlayPauseCommand.enabled = YES;
    });
    return AK::adopt_own(*new MediaControls(implementation));
}

MediaControls::MediaControls(void* implementation)
    : m_implementation(implementation) {}

MediaControls::~MediaControls() {
    auto* implementation = static_cast<MediaImplementation*>(m_implementation);
    run_on_main(^{
        MPRemoteCommandCenter* center = MPRemoteCommandCenter.sharedCommandCenter;
        [center.playCommand removeTarget:implementation->play_target];
        [center.pauseCommand removeTarget:implementation->pause_target];
        [center.togglePlayPauseCommand removeTarget:implementation->toggle_target];
        MPNowPlayingInfoCenter.defaultCenter.nowPlayingInfo = nil;
        MPNowPlayingInfoCenter.defaultCenter.playbackState = MPNowPlayingPlaybackStateStopped;
    });
    delete implementation;
}

void MediaControls::set_now_playing(std::string const& title, bool playing) {
    NSString* name = [NSString stringWithUTF8String:title.c_str()];
    run_on_main(^{
        MPNowPlayingInfoCenter* center = MPNowPlayingInfoCenter.defaultCenter;
        center.nowPlayingInfo = @{
            MPMediaItemPropertyTitle : name,
            MPNowPlayingInfoPropertyPlaybackRate : @(playing ? 1.0 : 0.0),
        };
        center.playbackState = playing ? MPNowPlayingPlaybackStatePlaying : MPNowPlayingPlaybackStatePaused;
    });
}

void MediaControls::clear_now_playing() {
    run_on_main(^{
        MPNowPlayingInfoCenter.defaultCenter.nowPlayingInfo = nil;
        MPNowPlayingInfoCenter.defaultCenter.playbackState = MPNowPlayingPlaybackStateStopped;
    });
}

} // namespace Elaris
