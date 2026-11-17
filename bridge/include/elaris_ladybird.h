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
} ElarisLadybirdEventKind;

typedef struct {
    ElarisLadybirdEventKind kind;
    const char* text; /* Borrowed until the callback returns; empty when unused. */
    int32_t value;    /* Loading state for LOADING; zero otherwise. */
} ElarisLadybirdEvent;

typedef void (*ElarisLadybirdEventCallback)(void* context, const ElarisLadybirdEvent* event);

/* Returned by elaris_ladybird_frame() while the view has not presented a frame yet. */
#define ELARIS_LADYBIRD_NO_FRAME 1

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

typedef enum {
    ELARIS_LADYBIRD_MOUSE_DOWN = 1,
    ELARIS_LADYBIRD_MOUSE_UP = 2,
    ELARIS_LADYBIRD_MOUSE_MOVE = 3,
    ELARIS_LADYBIRD_MOUSE_LEAVE = 4,
    ELARIS_LADYBIRD_MOUSE_WHEEL = 5,
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
int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, ElarisLadybirdView** out_view);
int32_t elaris_ladybird_load(ElarisLadybirdView* view, const char* url);
/* Sets the viewport in device pixels and the device pixel ratio (display scale) it is laid out at. */
int32_t elaris_ladybird_resize(ElarisLadybirdView* view, int32_t width, int32_t height, double device_pixel_ratio);
/* Describes the frame on display, or returns ELARIS_LADYBIRD_NO_FRAME. */
int32_t elaris_ladybird_frame(ElarisLadybirdView* view, ElarisLadybirdFrame* out_frame);
/* Copies the frame on display into rgba, which holds exactly stride * height bytes of the frame
 * elaris_ladybird_frame() last described, and clears its damage. The copy is the caller's; no Ladybird memory
 * outlives this call. */
int32_t elaris_ladybird_copy_frame(ElarisLadybirdView* view, uint8_t* rgba, size_t length);
int32_t elaris_ladybird_mouse_event(ElarisLadybirdView* view, const ElarisLadybirdMouseEvent* event);
int32_t elaris_ladybird_key_event(ElarisLadybirdView* view, const ElarisLadybirdKeyEvent* event);
int32_t elaris_ladybird_set_focus(ElarisLadybirdView* view, int32_t focused);
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
