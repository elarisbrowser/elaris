#ifndef ELARIS_LADYBIRD_H
#define ELARIS_LADYBIRD_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ElarisLadybird ElarisLadybird;
typedef struct ElarisLadybirdView ElarisLadybirdView;

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

/* All calls, including callbacks, run on the thread that initializes the bridge. */
int32_t elaris_ladybird_create(const char* resource_dir, const char* profile_dir,
    ElarisLadybirdEventCallback callback, void* context, ElarisLadybird** out_bridge);
int32_t elaris_ladybird_create_view(ElarisLadybird* bridge, ElarisLadybirdView** out_view);
int32_t elaris_ladybird_load(ElarisLadybirdView* view, const char* url);
/* Polls available Ladybird work without waiting; call regularly from the host event loop. */
int32_t elaris_ladybird_pump(ElarisLadybird* bridge);
void elaris_ladybird_destroy_view(ElarisLadybirdView* view);
void elaris_ladybird_destroy(ElarisLadybird* bridge);
const char* elaris_ladybird_last_error(void);

#ifdef __cplusplus
}
#endif
#endif
