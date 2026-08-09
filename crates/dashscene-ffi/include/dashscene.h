/*
 * dashscene C ABI — the surface every platform host sits on.
 *
 * D2 of docs/decisions/host-integration-in-three-layers.md. Kotlin reaches this
 * through JNI; Swift would reach the same symbols through this header when iOS
 * lands in v1.
 *
 * Hand-written rather than generated. The header IS the contract, so it is
 * reviewed as one — a generated file would make the contract a side effect of a
 * build step, and a diff nobody reads.
 *
 * VERSIONING. ds_abi_version() returns DS_ABI_VERSION, which is not the crate's
 * semantic version. Adding a symbol, or a DsStatus variant at the tail, does not
 * move it; changing a signature or renumbering a variant does. Call it once and
 * refuse a value you do not recognise — the alternative is discovering the
 * mismatch as a corrupted argument.
 */

#ifndef DASHSCENE_H
#define DASHSCENE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define DS_ABI_VERSION 1u

/*
 * Why a call did not succeed.
 *
 * These discriminants are the contract. Branch on them; never parse the message
 * from ds_last_error_message, which is diagnostic and promises nothing.
 */
typedef enum DsStatus {
  DS_OK = 0,
  /* A pointer argument that must be non-null was null. */
  DS_NULL_ARGUMENT = 1,
  /* The bytes are not a .dsb this runtime can open. */
  DS_OPEN = 2,
  /* The document opened but does not pass the referential load gate. */
  DS_GATE = 3,
  /* The surface could not be created, or the painter could not start on it. */
  DS_SURFACE = 4,
  /* The handle kind is not one this build supports. */
  DS_UNSUPPORTED_HANDLE = 5,
  /* The call needs a document and none is loaded. */
  DS_NO_DOCUMENT = 6,
  /* The call needs a surface and none is attached. */
  DS_NO_SURFACE = 7,
  /*
   * A panic was caught at the boundary. The library is in an unspecified state:
   * free the runtime and make no further calls on it.
   */
  DS_PANIC = 8
} DsStatus;

/* Which platform handle ds_runtime_attach_surface's pointers carry. */
typedef enum DsSurfaceKind {
  /*
   * window is an ANativeWindow *, from ANativeWindow_fromSurface.
   * display is ignored.
   */
  DS_SURFACE_ANDROID_NDK = 0
} DsSurfaceKind;

/* A live runtime. Opaque: the layout is free to change without moving the ABI
 * version. */
typedef struct DsRuntime DsRuntime;

/* Returns DS_ABI_VERSION. Cannot fail and takes no handle, so a host can ask
 * before it commits to anything. */
uint32_t ds_abi_version(void);

/* Creates an empty runtime — no document, no surface — and writes it to out. */
DsStatus ds_runtime_new(DsRuntime **out);

/* Frees a runtime. NULL is accepted and does nothing, like free(). */
void ds_runtime_free(DsRuntime *runtime);

/*
 * Loads a .dsb held in memory.
 *
 * This is the owning path: every payload is copied, so the cost tracks the file
 * rather than the shown root. That is the honest shape for a caller that handed
 * over bytes. A mapped load belongs with the platform host that has the file.
 */
DsStatus ds_runtime_load_document(DsRuntime *runtime, const uint8_t *bytes,
                                  size_t len);

/*
 * Hands a platform surface to the painter. width and height are device pixels.
 *
 * window must stay live until the surface is replaced or the runtime is freed.
 * On Android that is the surfaceDestroyed handshake: the callback must not
 * return until rendering has stopped.
 */
/*
 * kind is declared as DsSurfaceKind here for readability and for C's own type
 * checking, and the library validates it as a plain integer. That asymmetry is
 * deliberate: binding an out-of-range value to a Rust enum is undefined
 * behaviour at the call boundary, before any handler could run, so an unknown
 * kind must be rejectable rather than merely unmatched. An unrecognised value
 * returns DS_UNSUPPORTED_HANDLE.
 */
DsStatus ds_runtime_attach_surface(DsRuntime *runtime, DsSurfaceKind kind,
                                   void *window, void *display, uint32_t width,
                                   uint32_t height);

/* Resizes the surface. width and height are device pixels. */
DsStatus ds_runtime_resize(DsRuntime *runtime, uint32_t width, uint32_t height);

/*
 * Advances the scene by dt seconds. out_advanced, if non-NULL, receives whether
 * the generation moved — which is what says a frame is worth drawing.
 */
DsStatus ds_runtime_tick(DsRuntime *runtime, float dt, bool *out_advanced);

/*
 * Draws the committed frame and puts it on the surface.
 *
 * out_drawn, if non-NULL, receives whether a frame actually reached the window.
 * It can be false for a reason that is not an error — a zero extent, or a
 * surface that had to be reconfigured — which is why it is separate from the
 * status. A frame that reached the window is marked shown, so the next tick's
 * out_advanced means "changed since the frame you saw".
 *
 * Adding this symbol did not move DS_ABI_VERSION: by the rule at the top of
 * this header, a new symbol is additive and a host built against an older
 * header keeps working.
 */
DsStatus ds_runtime_draw(DsRuntime *runtime, bool *out_drawn);

/*
 * Copies the last failure's message into buf as NUL-terminated UTF-8.
 *
 * Returns the bytes the message needs including the terminator, so passing NULL
 * or a short buffer tells you what to allocate. Nothing is written when buf is
 * NULL or cap is 0. A short buffer truncates and still terminates.
 */
size_t ds_last_error_message(char *buf, size_t cap);

#ifdef __cplusplus
}
#endif

#endif /* DASHSCENE_H */
