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

#define DS_ABI_VERSION 2u

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
  /*
   * A surface failure that rebuilding the presenter does not fix. THREE
   * different conditions reach it, so branch on which call returned it rather
   * than on this value alone: ds_runtime_attach_surface could not create the
   * surface or start the painter on it, ds_runtime_resize was refused (an
   * extent past the device maximum, which is neither fatal nor fixed by
   * rebuilding), and ds_runtime_draw failed for a reason that is not a lost
   * swapchain. A lost swapchain is DS_SURFACE_LOST at the tail of this enum.
   */
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
  DS_PANIC = 8,
  /* A face descriptor is unusable: family is not UTF-8, family is empty or
   * only whitespace, weight is outside 1..=1000, or font_bytes do not parse
   * as a font face. */
  DS_FONT_FACE = 9,
  /* An atlas is unusable: atlas_metrics did not decode, atlas_png is not a
   * PNG header carrying the extent those metrics declare, a glyph in those
   * metrics is described by exactly one of its two quads, or the set is
   * mixed — some faces carrying a sheet and some not. */
  DS_ATLAS = 10,
  /* The path could not be used: nothing is there, it cannot be read, it is
   * empty, or it is not UTF-8. Only ds_runtime_load_document_mapped reports
   * it, because it is the only call that takes a path. */
  DS_MAP = 11,
  /* The ordinal names no root in this document. The message from
   * ds_last_error_message carries the ordinal asked for and the count the
   * document does carry, which is what tells an out-of-range ask apart from a
   * document with no roots at all. */
  DS_NO_SUCH_ROOT = 12,
  /* The file's payloads are derivations rather than the document's own
   * canonical bytes. A mapped load reads no payload header, so binding these
   * would tag one format as another with nothing downstream to catch it; the
   * file is refused instead. */
  DS_DERIVED = 13,
  /* An asset the shown root draws did not hash to what its entry names. */
  DS_PAYLOAD = 14,
  /*
   * The frame failed because the surface was LOST, and rebuilding the presenter
   * is the remedy. Only ds_runtime_draw reports it.
   *
   * Everything else stays DS_SURFACE, whose own comment above lists what
   * reaches it. Stated once, there: two copies of that list is how one of them
   * goes stale.
   *
   * Bound your consecutive rebuilds even so. A surface lost on every frame is
   * a device that has gone away, and the remedy keeps not working.
   */
  DS_SURFACE_LOST = 15,
  /* The handle named no runtime the calling thread can reach right now:
   * it was freed, or it was never one this library minted. Either way the
   * remedy is to stop using it.
   *
   * One further cause exists and NO HOST CAN REACH IT TODAY: a call already
   * in flight on that same handle, which leaves the runtime checked out. No
   * entry point takes a function pointer, so nothing of yours runs during a
   * call and you cannot re-enter one. It becomes reachable when a callback
   * does — story #859's data plane is the candidate — and then it names a
   * runtime that is alive and still has to be freed, so it must not be read
   * as "give up on the runtime". */
  DS_BAD_HANDLE = 16,
  /* The handle was minted on a different thread, which may still hold it or
   * may have exited. The remedy is to call from the thread that created it. */
  DS_WRONG_THREAD = 17,
  /* No handle could be minted: this thread already holds the maximum number of
   * live runtimes, or the process has drawn every thread number a handle can
   * carry. Never a wrap. */
  DS_HANDLES_EXHAUSTED = 18
} DsStatus;

/* Which platform handle ds_runtime_attach_surface's pointers carry. */
typedef enum DsSurfaceKind {
  /*
   * window is an ANativeWindow *, from ANativeWindow_fromSurface.
   * display is ignored.
   */
  DS_SURFACE_ANDROID_NDK = 0
} DsSurfaceKind;

/*
 * One face, with the atlas its shaped glyphs sample.
 *
 * The atlas is in here rather than in a second array on purpose: the atlas
 * list is indexed by the font slot of the face that shaped a glyph, so a
 * list in any other order samples the wrong face RATHER THAN FAILING.
 * Pairing them here means the library builds both from one walk and you
 * cannot get the order wrong — including when you list one family's faces
 * non-contiguously.
 *
 * atlas_png and atlas_metrics must both be NULL or both point at real bytes.
 * Both NULL is the measure-only cascade, where text is shaped and measured
 * and no glyph is drawn. Exactly one NULL is DS_ATLAS, not a silent fall
 * back to measure-only — and so is a mixed set across faces, where some
 * carry a sheet and some do not.
 *
 * weight must be in 1..=1000, the CSS range. Outside it is DS_FONT_FACE,
 * naming the face's index and the value — including 0, which is what an
 * uninitialised struct carries and which no CSS weight can be. This is the
 * one place the range is enforced — inside the library, below this ABI, since
 * issue #1206 — so a host that binds to this ABI inherits
 * the rule rather than repairing the value in its own way.
 */
typedef struct DsFontFace {
  const char *family;  /* NUL-terminated UTF-8 */
  uint16_t weight;     /* CSS weight, 1..=1000 */
  uint32_t face_index; /* index within a collection; 0 for one face */
  const uint8_t *font_bytes;
  size_t font_len;
  const uint8_t *atlas_png;
  size_t atlas_png_len;
  const uint8_t *atlas_metrics;
  size_t atlas_metrics_len;
} DsFontFace;

/*
 * A live runtime, named by an opaque handle.
 *
 * NOT AN ADDRESS. Do not dereference it, do arithmetic on it, or invent one.
 * A handle names at most one runtime for the life of the process: freeing a
 * runtime retires its value and no later runtime is ever given it again. So a
 * stale handle, a forged handle, and a handle used from a thread other than
 * the one that created it each produce a DsStatus rather than undefined
 * behaviour.
 *
 * THREAD-AFFINE. A runtime is reachable only from the thread whose
 * ds_runtime_new minted it. From any other thread every call answers
 * DS_WRONG_THREAD, including after the minting thread has exited — the two
 * cases are deliberately not distinguished, because telling them apart needs a
 * process-wide registry of live threads and that is the shared state this
 * design exists to avoid.
 *
 * NO OTHER CALL MAY BE IN FLIGHT ON THE SAME RUNTIME. This is the rule for
 * every ds_runtime_* entry point, and this is the one place it is stated.
 * Since story #1226 it is also checked: a re-entrant call on the same handle
 * is refused with DS_BAD_HANDLE rather than aliasing the runtime. A call on a
 * DIFFERENT runtime is fine. Nothing can break the rule today in any case,
 * because no entry point calls back into host code.
 *
 * 0 is never a live runtime. ds_runtime_new writes 0 on every failure, so a
 * caller that ignores the status still holds a value that cannot resolve.
 */
typedef uint64_t DsRuntime;

/* Returns DS_ABI_VERSION. Cannot fail and takes no handle, so a host can ask
 * before it commits to anything. */
uint32_t ds_abi_version(void);

/* Creates an empty runtime — no document, no surface — and writes it to out. */
DsStatus ds_runtime_new(DsRuntime *out);

/*
 * Frees the runtime a handle names, and retires the handle.
 *
 * 0 is accepted and does nothing, exactly where NULL was. Freeing a handle
 * twice, freeing one this library never minted, or freeing from a thread other
 * than the one that created it are each reported — DS_BAD_HANDLE or
 * DS_WRONG_THREAD — rather than being undefined behaviour the caller has to
 * prevent. That is what this handle exists for.
 */
DsStatus ds_runtime_free(DsRuntime runtime);

/*
 * Loads a .dsb held in memory.
 *
 * This is the owning path: every payload is copied, so the cost tracks the file
 * rather than the shown root. That is the honest shape for a caller that handed
 * over bytes it already holds.
 *
 * If you have the file rather than its bytes, ds_runtime_load_document_mapped
 * is the bounded path and costs less.
 */
DsStatus ds_runtime_load_document(DsRuntime runtime, const uint8_t *bytes,
                                  size_t len);

/*
 * Loads a .dsb held in memory, with the fonts and atlases its text needs.
 *
 * ds_runtime_load_document is this call with no faces. A NULL faces, or a
 * zero face_count, loads without text: text nodes lay out as empty leaves
 * and no glyph is drawn.
 *
 * WHAT YOU MUST SUPPLY. A face is a font file's bytes plus the family and
 * weight it stands for. An atlas is a committed MSDF sheet — a PNG and the
 * metrics blob beside it. NOTHING BAKES ONE AT RUN TIME: the generator is
 * an external pinned binary that reads a font from a path, so these arrive
 * with you or your text is measured and never drawn.
 *
 * The faces are validated before the document is opened, so a bad cascade is
 * reported as itself rather than as whatever the document turned out to be.
 *
 * Nothing is retained: every byte is copied before this returns.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_load_document_with_text(DsRuntime runtime,
                                            const uint8_t *bytes, size_t len,
                                            const DsFontFace *faces,
                                            size_t face_count);

/*
 * Loads a .dsb by MAPPING it from path, bounded by the root that is shown.
 *
 * The bounded counterpart of ds_runtime_load_document. The file is mapped
 * rather than read and no payload is copied; the only bytes touched out of the
 * file's cold half are the assets the shown root's subtree draws. So the cost
 * of opening tracks the artboard you are showing rather than the size of the
 * file, which is what the other two hosts already did and this one did not.
 *
 * path is NUL-terminated UTF-8.
 *
 * shown_root is a document ordinal and is REQUIRED. There is no value meaning
 * "every root": a caller that wants every root has ds_runtime_load_document
 * and pays the owning cost knowingly. An ordinal past the last root is
 * DS_NO_SUCH_ROOT rather than a silent clamp.
 *
 * faces carries the same rule as ds_runtime_load_document_with_text: a NULL
 * faces, or a zero face_count, loads without text, and text nodes then lay out
 * as empty leaves and draw no glyphs.
 *
 * THE MAPPING IS THE RUNTIME'S. You keep no lifetime rule and you must not
 * unlink or rewrite the file while it is loaded: the arena holds the mapping,
 * and a load that SUCCEEDS installs a fresh arena, so the previous mapping is
 * released then or at ds_runtime_free.
 *
 * A LOAD THAT FAILS RELEASES NOTHING. Every status any of the three loaders
 * returns is raised before the arena is replaced, so a refused load leaves the
 * previously loaded document drawable and its mapping held. Do not unlink the
 * previous file until a later load has answered DS_OK. DS_PANIC is the one
 * answer that says nothing either way: the runtime is alive, but where the
 * unwind happened decides what it still holds, and the next ds_runtime_tick
 * answers DS_NO_DOCUMENT when the load had got as far as replacing the arena.
 *
 * THE ROOT IS NAMED ONCE, at load. There is no call for changing it
 * afterwards; load again to show a different artboard.
 *
 * Adding this symbol did not move DS_ABI_VERSION, and neither did the four
 * statuses it reports: they are appended at the tail of DsStatus.
 */
DsStatus ds_runtime_load_document_mapped(DsRuntime runtime, const char *path,
                                         uint32_t shown_root,
                                         const DsFontFace *faces,
                                         size_t face_count);

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
DsStatus ds_runtime_attach_surface(DsRuntime runtime, DsSurfaceKind kind,
                                   void *window, void *display, uint32_t width,
                                   uint32_t height);

/*
 * Drops the surface, keeping the document and the scene.
 *
 * The other half of Android's destroy handshake. surfaceDestroyed must not
 * return until rendering has stopped and the surface built from that window has
 * been dropped; this is the call that drops it, and it is separate from
 * ds_runtime_free because the surface comes and goes many times over one
 * document's life.
 *
 * out_had_surface, if non-NULL, receives whether one was attached. Detaching
 * twice is not an error.
 *
 * The first draw after re-attaching must happen whatever ds_runtime_tick's
 * out_advanced says: the scene did not change while the surface was gone, and
 * the new device has drawn nothing.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_detach_surface(DsRuntime runtime, bool *out_had_surface);

/* Resizes the surface. width and height are device pixels. */
DsStatus ds_runtime_resize(DsRuntime runtime, uint32_t width, uint32_t height);

/*
 * Advances the scene by dt seconds. out_advanced, if non-NULL, receives whether
 * the generation moved — which is what says a frame is worth drawing.
 *
 * THIS CALL MAY TOUCH THE ATTACHED SURFACE. A commit that changed the shown
 * root renumbers the rect table, and the tick reports that to the painter so it
 * forgets what it uploaded. So the tick takes the same "no other call in flight
 * on this runtime" rule the rest of this header states, rather than being a
 * scene-only call you could make beside a draw.
 *
 * No load in this ABI can raise that today: a document's root is named once,
 * inside the load, and no call changes it afterwards. The report is here so a
 * host is already correct if that changes.
 */
DsStatus ds_runtime_tick(DsRuntime runtime, float dt, bool *out_advanced);

/*
 * Draws the committed frame and puts it on the surface.
 *
 * out_drawn, if non-NULL, receives whether a frame actually reached the window.
 * It can be false for a reason that is not an error — a zero extent, or a
 * surface that had to be reconfigured — which is why it is separate from the
 * status.
 *
 * The commit is marked shown whenever this returns DS_OK, NOT only when a frame
 * reached the window. That is deliberate and it is what LiveScene::advanced
 * requires: a present can return without drawing, nothing can reliably detect
 * that, and gating on out_drawn would leave out_advanced true on every tick
 * while the window is occluded, so a host that idled on it would never idle.
 *
 * So out_drawn is for a host's own pacing and must NOT be used to decide what
 * was shown. This header said the opposite until story #842 — that a frame
 * reaching the window was what marked it shown — which is the copy a host
 * implements against.
 *
 * Adding this symbol did not move DS_ABI_VERSION: by the rule at the top of
 * this header, a new symbol is additive and a host built against an older
 * header keeps working.
 */
DsStatus ds_runtime_draw(DsRuntime runtime, bool *out_drawn);

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
