#ifndef ELARIS_LADYBIRD_H
#define ELARIS_LADYBIRD_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ElarisLadybird ElarisLadybird;
typedef struct ElarisLadybirdView ElarisLadybirdView;
typedef struct ElarisLadybirdWaker ElarisLadybirdWaker;

typedef enum {
    ELARIS_LADYBIRD_NAVIGATION = 1,
    ELARIS_LADYBIRD_TITLE = 2,
    ELARIS_LADYBIRD_LOADING = 3,
    ELARIS_LADYBIRD_FRAME = 4,
    ELARIS_LADYBIRD_HISTORY = 5, /* Whether the view can go back or forward changed. */
    ELARIS_LADYBIRD_FAVICON = 6,
    ELARIS_LADYBIRD_NEW_TAB,
    ELARIS_LADYBIRD_CONTEXT_MENU,
    ELARIS_LADYBIRD_CLIPBOARD,
    ELARIS_LADYBIRD_IME_CARET,
    ELARIS_LADYBIRD_NAVIGATION_COMMITTED = 11,
    ELARIS_LADYBIRD_DOWNLOAD = 12,
    ELARIS_LADYBIRD_FIND_RESULT = 13,
    ELARIS_LADYBIRD_SELECTED_TEXT = 14,
    ELARIS_LADYBIRD_PEEK = 15,
    ELARIS_LADYBIRD_LOAD_ERROR = 16,
    ELARIS_LADYBIRD_HOVERED_LINK = 17,
    ELARIS_LADYBIRD_TOOLTIP = 18,
    ELARIS_LADYBIRD_CURSOR = 19,
    ELARIS_LADYBIRD_PAGE_DIALOG = 20,
    ELARIS_LADYBIRD_CLOSE_DECISION = 21,
    ELARIS_LADYBIRD_FORM_PICKER = 22,      /* JSON in text, empty when dismissed. */
    ELARIS_LADYBIRD_NAVIGATION_ITEMS = 23, /* JSON session history targets in text. */
    ELARIS_LADYBIRD_PERMISSION_PROMPT = 24,
    ELARIS_LADYBIRD_PERMISSION_SAVED = 25,
    ELARIS_LADYBIRD_PERMISSION_USE = 26, /* Permission key in text, visible flag in value. */
    ELARIS_LADYBIRD_WEB_FULLSCREEN = 27, /* Whether the page entered element fullscreen. */
    ELARIS_LADYBIRD_POPUP = 28,          /* Pending popup handle in request_id, activation in value. */
    ELARIS_LADYBIRD_POPUP_CLOSED = 29,
    /* URL in text, initiator origin in destination, application in selection. Value 1 asks to open the
     * link, 2 reports that it cannot be opened, and 0 clears the notice. */
    ELARIS_LADYBIRD_EXTERNAL_URL = 30,
    /* Browser-level, so view_id is 0. request_id echoes clear_browsing_data; value 0 reports a failure. */
    ELARIS_LADYBIRD_BROWSING_DATA_CLEARED = 31,
    /* JSON in text while accessibility is on. Value 1 is a whole tree {"seq","nodes"}, 2 a change
     * {"base","seq","nodes","removed"}, and 3 a focus change {"focus"}. */
    ELARIS_LADYBIRD_ACCESSIBILITY = 32,
    /* A wheel event finished. Value 1 when the page cancelled it with preventDefault, otherwise 0. Scrolls
     * do not count, so the host may still use a sideways wheel for a history swipe. */
    ELARIS_LADYBIRD_WHEEL_RESULT = 33,
} ElarisLadybirdEventKind;

/* Permission event text is the origin and selection is the permission key.
 * Prompt value 1 is a question, 2 is an OS denial notice, and 0 clears the prompt.
 * Saved value 1 is Allow and 2 is Block. */
int32_t elaris_ladybird_reply_to_permission(ElarisLadybirdView*, uint64_t request_id, int32_t choice);
/* Replace the ordinary session's site decisions with rows of [origin, key, choice]. */
int32_t elaris_ladybird_set_permissions(ElarisLadybird*, char const* json);
int32_t elaris_ladybird_set_permission_default(ElarisLadybird*, char const* key, int32_t blocked);
/* Bind a popup opened by source to a host view id. */
int32_t elaris_ladybird_adopt_popup(ElarisLadybird*, uint64_t source, uint64_t request_id, uint64_t view_id,
                                    ElarisLadybirdView**);
int32_t elaris_ladybird_exit_web_fullscreen(ElarisLadybirdView*);
/* Starts or stops reporting the page's accessibility tree as ACCESSIBILITY events. Starting asks for
 * the whole tree again, which also recovers from a change that did not apply. */
int32_t elaris_ladybird_set_accessibility(ElarisLadybirdView*, int32_t enabled);
/* Asks the page to "press", "focus" or "scroll_into_view" an accessibility node. */
int32_t elaris_ladybird_accessibility_action(ElarisLadybirdView*, int64_t node_id, char const* action);
/* Sets the caret or selection in a text node, in Unicode character offsets. */
int32_t elaris_ladybird_accessibility_select(ElarisLadybirdView*, int64_t node_id, int32_t start, int32_t end);
/* Answers the view's current EXTERNAL_URL event; stale request ids are ignored. */
int32_t elaris_ladybird_reply_to_external_url(ElarisLadybirdView*, uint64_t request_id, int32_t open);
/* Choice 0 resets a permission, 1 allows it, and 2 blocks it. */
int32_t elaris_ladybird_set_permission(ElarisLadybird*, char const* origin, char const* key, int32_t choice);

typedef enum {
    ELARIS_LADYBIRD_DIALOG_CLOSED = 0,
    ELARIS_LADYBIRD_DIALOG_ALERT = 1,
    ELARIS_LADYBIRD_DIALOG_CONFIRM,
    ELARIS_LADYBIRD_DIALOG_PROMPT,
    ELARIS_LADYBIRD_DIALOG_BEFORE_UNLOAD,
} ElarisLadybirdDialogKind;

typedef enum {
    ELARIS_LADYBIRD_DOWNLOAD_IN_PROGRESS = 1,
    ELARIS_LADYBIRD_DOWNLOAD_PAUSED,
    ELARIS_LADYBIRD_DOWNLOAD_COMPLETED,
    ELARIS_LADYBIRD_DOWNLOAD_CANCELLED,
    ELARIS_LADYBIRD_DOWNLOAD_FAILED,
} ElarisLadybirdDownloadStatus;

/* Bit flags for HISTORY events' value. */
#define ELARIS_LADYBIRD_HISTORY_BACK 1
#define ELARIS_LADYBIRD_HISTORY_FORWARD 2

typedef struct {
    ElarisLadybirdEventKind kind;
    uint64_t view_id;            /* The id the host gave the view that raised the event. */
    const char* text;            /* Borrowed until the callback returns; empty when unused. */
    int32_t value;               /* Loading state for LOADING, HISTORY_* flags for HISTORY; zero otherwise. */
    const uint8_t* favicon_rgba; /* Borrowed RGBA8 pixels for FAVICON, null when cleared. */
    int32_t favicon_width;
    int32_t favicon_height;
    int32_t favicon_stride;
    uint64_t request_id;
    const char* selection;
    int32_t caret[4]; /* Device pixels, relative to the viewport. */
    uint64_t download_id;
    uint64_t bytes_downloaded;
    uint64_t total_bytes;
    int32_t has_total_bytes;
    ElarisLadybirdDownloadStatus download_status;
    const char* destination; /* Borrowed UTF-8 path, valid until the callback returns. */
    const char* error;       /* Borrowed UTF-8 error, valid until the callback returns. */
    uint64_t current_match_index;
    uint64_t total_match_count;
    int32_t has_total_match_count;
} ElarisLadybirdEvent;

typedef void (*ElarisLadybirdEventCallback)(void* context, const ElarisLadybirdEvent* event);

/* Returned by elaris_ladybird_frame() while the view has not presented a frame yet. */
#define ELARIS_LADYBIRD_NO_FRAME 1
/* Returned by elaris_ladybird_gpu_frame() while every GPU surface is still in the host's hands. Ask again after
 * elaris_ladybird_release_frames(). */
#define ELARIS_LADYBIRD_BUSY 2

typedef enum {
    /* Four bytes per pixel in R, G, B, A order; alpha is always opaque (255). */
    ELARIS_LADYBIRD_PIXEL_FORMAT_RGBA8 = 1,
} ElarisLadybirdPixelFormat;

/* The frame on display, described in device pixels. */
typedef struct {
    int32_t width;
    int32_t height;
    int32_t stride; /* Bytes per row elaris_ladybird_copy_frame() writes: width * 4. */
    ElarisLadybirdPixelFormat format;
    double device_pixel_ratio;
    /* What changed since the last copy (the whole frame after a size change). Empty when nothing did. */
    int32_t damage_x;
    int32_t damage_y;
    int32_t damage_width;
    int32_t damage_height;
    uint64_t serial; /* Increases with every frame the view presents. */
} ElarisLadybirdFrame;

/* The frame on display, shared with the host's GPU instead of copied.
 *
 * macOS: io_surface is a borrowed IOSurfaceRef of exactly width x height pixels, the bridge's own copy of the frame,
 * which stays unchanged until elaris_ladybird_release_frames() reports a newer frame drawn.
 *
 * Linux: dmabuf_fd is a descriptor the host owns for the frame's single-plane dmabuf (DRM format dmabuf_format, at
 * dmabuf_offset with rows of dmabuf_stride bytes, laid out by dmabuf_modifier). bitmap_id names the backing store,
 * whose dmabuf the same descriptor refers to every time it shows a frame. Ladybird does not paint into the store again
 * until elaris_ladybird_release_frames() reports this frame, or a newer one, read. */
typedef struct {
    void* io_surface;
    int32_t dmabuf_fd;
    uint32_t dmabuf_offset;
    uint32_t dmabuf_stride;
    uint32_t dmabuf_format;
    uint64_t dmabuf_modifier;
    int32_t bitmap_id;
    int32_t width;
    int32_t height;
    double device_pixel_ratio;
    uint64_t serial; /* As in ElarisLadybirdFrame. */
} ElarisLadybirdGpuFrame;

typedef enum {
    ELARIS_LADYBIRD_MOUSE_DOWN = 1,
    ELARIS_LADYBIRD_MOUSE_UP = 2,
    ELARIS_LADYBIRD_MOUSE_MOVE = 3,
    ELARIS_LADYBIRD_MOUSE_LEAVE = 4,
    ELARIS_LADYBIRD_MOUSE_WHEEL = 5,
    ELARIS_LADYBIRD_MOUSE_CANCEL = 6,
} ElarisLadybirdMouseType;

/* Bit flags for buttons. */
#define ELARIS_LADYBIRD_BUTTON_PRIMARY 1
#define ELARIS_LADYBIRD_BUTTON_SECONDARY 2
#define ELARIS_LADYBIRD_BUTTON_MIDDLE 4

/* Bit flags for modifiers. SUPER is Command on macOS. */
#define ELARIS_LADYBIRD_MOD_ALT 1
#define ELARIS_LADYBIRD_MOD_CTRL 2
#define ELARIS_LADYBIRD_MOD_SHIFT 4
#define ELARIS_LADYBIRD_MOD_SUPER 8

/* Where a touchpad scroll is in its gesture. Plain wheel notches have no gesture. */
typedef enum {
    ELARIS_LADYBIRD_WHEEL_PHASE_NONE = 0,
    ELARIS_LADYBIRD_WHEEL_PHASE_ONGOING = 1,  /* Fingers on the touchpad. */
    ELARIS_LADYBIRD_WHEEL_PHASE_MOMENTUM = 2, /* Coasting after the fingers lifted. */
    ELARIS_LADYBIRD_WHEEL_PHASE_ENDED = 3,    /* The fingers lifted, or the momentum stopped. */
} ElarisLadybirdWheelPhase;

typedef struct {
    ElarisLadybirdMouseType type;
    int32_t x; /* Device pixels from the view's top-left. */
    int32_t y;
    int32_t button;  /* The button that changed, for DOWN and UP. */
    int32_t buttons; /* Every button held after the event. */
    int32_t modifiers;
    /* WHEEL only: logical pixels to scroll the page by; positive scrolls toward its end (down, right). */
    double wheel_delta_x;
    double wheel_delta_y;
    int32_t wheel_precise; /* Nonzero for touchpad-like pixel deltas rather than wheel notches. */
    ElarisLadybirdWheelPhase wheel_phase;
    int32_t click_count;
} ElarisLadybirdMouseEvent;

typedef enum {
    ELARIS_LADYBIRD_KEY_PRESS = 1,
    ELARIS_LADYBIRD_KEY_RELEASE = 2,
} ElarisLadybirdKeyType;

typedef enum {
    ELARIS_LADYBIRD_KEY_CHARACTER = 1, /* The key that types code_point. */
    ELARIS_LADYBIRD_KEY_RETURN,
    ELARIS_LADYBIRD_KEY_BACKSPACE,
    ELARIS_LADYBIRD_KEY_DELETE,
    ELARIS_LADYBIRD_KEY_TAB,
    ELARIS_LADYBIRD_KEY_ESCAPE,
    ELARIS_LADYBIRD_KEY_LEFT,
    ELARIS_LADYBIRD_KEY_RIGHT,
    ELARIS_LADYBIRD_KEY_UP,
    ELARIS_LADYBIRD_KEY_DOWN,
    ELARIS_LADYBIRD_KEY_HOME,
    ELARIS_LADYBIRD_KEY_END,
    ELARIS_LADYBIRD_KEY_PAGE_UP,
    ELARIS_LADYBIRD_KEY_PAGE_DOWN,
    ELARIS_LADYBIRD_KEY_INSERT,
    ELARIS_LADYBIRD_KEY_PRINTSCREEN,
    ELARIS_LADYBIRD_KEY_SCROLLLOCK,
    ELARIS_LADYBIRD_KEY_PAUSE,
    ELARIS_LADYBIRD_KEY_SHIFTLEFT,
    ELARIS_LADYBIRD_KEY_SHIFTRIGHT,
    ELARIS_LADYBIRD_KEY_CONTROLLEFT,
    ELARIS_LADYBIRD_KEY_CONTROLRIGHT,
    ELARIS_LADYBIRD_KEY_ALTLEFT,
    ELARIS_LADYBIRD_KEY_ALTRIGHT,
    ELARIS_LADYBIRD_KEY_SUPERLEFT,
    ELARIS_LADYBIRD_KEY_SUPERRIGHT,
    ELARIS_LADYBIRD_KEY_F1 = 32,
    ELARIS_LADYBIRD_KEY_F12 = 43,
} ElarisLadybirdKey;

typedef struct {
    ElarisLadybirdKeyType type;
    ElarisLadybirdKey key;
    int32_t modifiers;
    uint32_t code_point; /* CHARACTER only. */
    int32_t repeat;
} ElarisLadybirdKeyEvent;

/* All calls, including callbacks, run on the thread that initializes the bridge. */
int32_t elaris_ladybird_create(const char* resource_dir, const char* profile_dir, ElarisLadybirdEventCallback callback,
                               void* context, ElarisLadybird** out_bridge);
/* An optional per-application destination override. NULL restores the operating system Downloads directory. */
int32_t elaris_ladybird_set_download_directory(ElarisLadybird* bridge, const char* path);
/* Starts an HTTP(S) URL through Ladybird's request server and returns its Ladybird-side ID. */
int32_t elaris_ladybird_start_download(ElarisLadybird* bridge, const char* url, const char* suggested_filename,
                                       uint64_t* out_download_id);
int32_t elaris_ladybird_cancel_download(ElarisLadybird* bridge, uint64_t download_id);
/* Cancels every in-flight download. */
int32_t elaris_ladybird_cancel_active_downloads(ElarisLadybird* bridge);
/* Pauses (pause 1) or resumes (pause 0) a download. DOWNLOAD events' value carries bit 1 for a private
 * window's download, bit 2 when it can resume, and bit 4 while it waits to retry. */
int32_t elaris_ladybird_pause_download(ElarisLadybird* bridge, uint64_t download_id, int32_t pause);
/* Before shutdown: pauses resumable downloads, which Ladybird restores next launch, and cancels the rest. */
int32_t elaris_ladybird_suspend_downloads(ElarisLadybird* bridge);
/* Creates a view; every event it raises carries view_id, which the host chooses. */
int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, uint64_t view_id, ElarisLadybirdView** out_view);
/* Creates a view in the shared private session. Its cookies, storage and cache never reach disk. */
int32_t elaris_ladybird_create_private_view(ElarisLadybird*, uint64_t view_id, ElarisLadybirdView** out_view);
enum {
    ELARIS_LADYBIRD_CLEAR_CACHE = 1,
    ELARIS_LADYBIRD_CLEAR_SITE_DATA = 2,
    ELARIS_LADYBIRD_CLEAR_DOWNLOAD_HISTORY = 4,
};
/* Clears the selected engine data last used at or after since_ms (Unix milliseconds; negative clears all
 * time). Completion arrives as BROWSING_DATA_CLEARED with the same request id. */
int32_t elaris_ladybird_clear_browsing_data(ElarisLadybird*, int64_t since_ms, int32_t flags, uint64_t request_id);
/* Replaces the autoplay default and site exceptions. Policies are 0 allow, 1 block audio, 2 block all;
 * json is an array of [origin, policy] rows. */
int32_t elaris_ladybird_set_autoplay(ElarisLadybird*, int32_t default_policy, char const* json);
/* Turns the content blocker on or off. Filter lists refresh while it is on, so turning it on fetches them. json is an
 * array of the built-in list identifiers to enable; the others are disabled. Enabling a list while the blocker is on
 * fetches it straight away. */
int32_t elaris_ladybird_set_content_blocking(ElarisLadybird*, int32_t enabled, char const* json);
/* Starts a fresh private session. Fails while any private view is open. */
int32_t elaris_ladybird_reset_private_session(ElarisLadybird*);
/* Sets the maximum frame rate reported to pages and used by Ladybird's compositor. */
int32_t elaris_ladybird_set_display_refresh_rate(ElarisLadybirdView* view, double refresh_rate);
int32_t elaris_ladybird_load(ElarisLadybirdView* view, const char* url);
int32_t elaris_ladybird_navigation_items(ElarisLadybirdView* view, int32_t direction);
int32_t elaris_ladybird_traverse_to_step(ElarisLadybirdView* view, int32_t step);
int32_t elaris_ladybird_reload(ElarisLadybirdView* view);
int32_t elaris_ladybird_reload_bypassing_cache(ElarisLadybirdView* view);
int32_t elaris_ladybird_set_zoom(ElarisLadybirdView* view, double zoom);
int32_t elaris_ladybird_stop_loading(ElarisLadybirdView* view);
/* Moves through the view's session history: -1 goes back, 1 forward. */
int32_t elaris_ladybird_traverse_history(ElarisLadybirdView* view, int32_t delta);
/* Whether the view is on screen. Hidden views stop painting and pages see document.hidden. */
int32_t elaris_ladybird_set_visible(ElarisLadybirdView* view, int32_t visible);
/* Sets the viewport in device pixels and the device pixel ratio (display scale) it is laid out at. */
int32_t elaris_ladybird_resize(ElarisLadybirdView* view, int32_t width, int32_t height, double device_pixel_ratio);
/* Describes the frame on display, or returns ELARIS_LADYBIRD_NO_FRAME. */
int32_t elaris_ladybird_frame(ElarisLadybirdView* view, ElarisLadybirdFrame* out_frame);
/* Copies the frame on display into rgba, which holds exactly stride * height bytes of the frame
 * elaris_ladybird_frame() last described, and clears its damage. The copy is the caller's; no Ladybird memory
 * outlives this call. */
int32_t elaris_ladybird_copy_frame(ElarisLadybirdView* view, uint8_t* rgba, size_t length);
/* Whether this build can share frames with the host's GPU: macOS always, Linux when Ladybird was built with Vulkan
 * dmabuf images. Elaris shows frames through the CPU otherwise. */
int32_t elaris_ladybird_gpu_frames_supported(void);
/* Makes elaris_ladybird_gpu_frame() available for the view (macOS and Linux): frames are shared with the host's GPU
 * and sampled in place, with no CPU copy. */
int32_t elaris_ladybird_set_gpu_frames(ElarisLadybirdView* view, int32_t enabled);
/* Copies the frame on display into a GPU surface, or returns ELARIS_LADYBIRD_NO_FRAME or ELARIS_LADYBIRD_BUSY. */
int32_t elaris_ladybird_gpu_frame(ElarisLadybirdView* view, ElarisLadybirdGpuFrame* out_frame);
/* Reports the newest frame serial the host's GPU has finished with (macOS: drawn; Linux: read), freeing the surfaces
 * of older frames. */
int32_t elaris_ladybird_release_frames(ElarisLadybirdView* view, uint64_t completed_serial);
int32_t elaris_ladybird_mouse_event(ElarisLadybirdView* view, const ElarisLadybirdMouseEvent* event);
typedef enum {
    ELARIS_EDIT_COPY,
    ELARIS_EDIT_CUT,
    ELARIS_EDIT_PASTE,
    ELARIS_EDIT_SELECT_ALL,
    ELARIS_EDIT_MARK,
    ELARIS_EDIT_COMMIT,
} ElarisEdit;
int32_t elaris_ladybird_edit(ElarisLadybirdView*, ElarisEdit, const uint8_t* text, size_t length);
int32_t elaris_ladybird_discard_prepared(ElarisLadybirdView*, uint64_t request);
int32_t elaris_ladybird_load_prepared(ElarisLadybirdView*, uint64_t source, uint64_t request);
int32_t elaris_ladybird_open_context_target(ElarisLadybirdView*, uint64_t request, int32_t new_tab);
/* Applies a media context menu action to the media the menu was opened on. */
typedef enum {
    ELARIS_LADYBIRD_MEDIA_TOGGLE_PLAY = 1,
    ELARIS_LADYBIRD_MEDIA_TOGGLE_MUTE = 2,
    ELARIS_LADYBIRD_MEDIA_TOGGLE_PICTURE_IN_PICTURE = 3,
} ElarisLadybirdMediaAction;
int32_t elaris_ladybird_media_action(ElarisLadybirdView*, int32_t action);
/* Searches case-insensitively, highlights all matches, and reports the request ID with the result. */
int32_t elaris_ladybird_find(ElarisLadybirdView*, const char* query, uint64_t request_id);
int32_t elaris_ladybird_find_next(ElarisLadybirdView*, uint64_t request_id);
int32_t elaris_ladybird_find_previous(ElarisLadybirdView*, uint64_t request_id);
/* PAGE_DIALOG events carry the dialog kind in value and prompt default in selection. */
int32_t elaris_ladybird_reply_to_dialog(ElarisLadybirdView*, uint64_t request_id, int32_t accepted,
                                        const uint8_t* response, size_t response_size);
/* A null response cancels. File paths use the platform filesystem encoding, separated by NUL bytes. */
int32_t elaris_ladybird_reply_to_picker(ElarisLadybirdView*, uint64_t request_id, const uint8_t* response,
                                        size_t response_size);
int32_t elaris_ladybird_preflight_close(ElarisLadybirdView*);
int32_t elaris_ladybird_find_end(ElarisLadybirdView*);
/* Reads the current webpage selection and returns it asynchronously with the request ID. */
int32_t elaris_ladybird_selected_text(ElarisLadybirdView*, uint64_t request_id);
int32_t elaris_ladybird_key_event(ElarisLadybirdView* view, const ElarisLadybirdKeyEvent* event);
int32_t elaris_ladybird_set_focus(ElarisLadybirdView* view, int32_t focused);
/* Sets the colour scheme every page sees through prefers-color-scheme: nonzero for dark, zero for light. It is
 * Ladybird's website colour scheme setting, kept in the profile, so pages created later start in it too. */
int32_t elaris_ladybird_set_color_scheme(ElarisLadybird* bridge, int32_t dark);
/* Polls available Ladybird work without waiting; call regularly from the host event loop. */
int32_t elaris_ladybird_pump(ElarisLadybird* bridge);
/* Runs Ladybird work, blocking until there is some or timeout_us passes; a negative timeout waits until woken. For a
 * host that gives Ladybird a thread of its own. */
int32_t elaris_ladybird_wait(ElarisLadybird* bridge, int32_t timeout_us);
/* Wakes elaris_ladybird_wait from any thread. Create it on the bridge's thread; it stays safe to wake and destroy from
 * any thread, also after the bridge is gone. */
ElarisLadybirdWaker* elaris_ladybird_create_waker(ElarisLadybird* bridge);
void elaris_ladybird_wake(ElarisLadybirdWaker* waker);
void elaris_ladybird_destroy_waker(ElarisLadybirdWaker* waker);
void elaris_ladybird_destroy_view(ElarisLadybirdView* view);
void elaris_ladybird_destroy(ElarisLadybird* bridge);
const char* elaris_ladybird_last_error(void);

#ifdef __cplusplus
}
#endif
#endif
