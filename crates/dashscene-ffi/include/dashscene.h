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
 * move it; changing a signature or renumbering a variant does. So does moving an
 * existing condition onto a different discriminant, even when the variant it
 * moves to is itself new and appended: a host that knew the old value stops
 * recognising the condition, which is a break the first two clauses do not
 * catch. SurfaceLost is the case that showed it, and it shipped before this
 * clause existed. Call ds_abi_version() once and refuse a value you do not
 * recognise — the alternative is discovering the mismatch as a corrupted
 * argument.
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
   * empty, or it is not UTF-8 — or, for ds_runtime_load_document_mapped_range,
   * the byte range it names is not inside the file. Only the two calls that
   * take a path report it. */
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
   * call and you cannot re-enter one. It becomes reachable when some entry
   * point takes a callback, and then it names a runtime that is alive and
   * still has to be freed, so it must not be read as "give up on the runtime".
   *
   * Story #859's data plane was named here as the candidate and is NOT one:
   * ds_runtime_acquire_frame hands out memory and takes no function pointer,
   * so a host's workers read rows without calling in. Nothing in this ABI
   * re-enters it yet. */
  DS_BAD_HANDLE = 16,
  /* The handle was minted on a different thread, which may still hold it or
   * may have exited. The remedy is to call from the thread that created it. */
  DS_WRONG_THREAD = 17,
  /* No handle could be minted: this thread already holds the maximum number of
   * live runtimes, or the process has drawn every thread number a handle can
   * carry. Never a wrap. */
  DS_HANDLES_EXHAUSTED = 18,

  /* A frame lease is outstanding and the call would have invalidated the views
   * ds_runtime_acquire_frame handed out. Reported by ds_runtime_tick, every
   * loader, ds_runtime_free, and a second acquire. The remedy is always
   * ds_runtime_release_frame.
   *
   * Additive in effect as well as in value: nothing could reach this before the
   * lease existed, so a host built against an older header meets it only on a
   * call it could not have made. DS_ABI_VERSION does not move. */
  DS_FRAME_LEASED = 19,

  /* ds_runtime_atlas was asked for an index the loaded document's atlas set
   * does not hold.
   *
   * A caller error rather than a document one: a GlyphRun's atlas field always
   * names a row of the set the same load installed, so an index past the end
   * came from the caller. NEVER A CLAMP — the nearest atlas is a different
   * face's sheet, and sampling it draws the wrong glyphs rather than failing.
   *
   * Additive in effect as well as in value, like DS_FRAME_LEASED: nothing
   * could reach it before ds_runtime_atlas existed. DS_ABI_VERSION does not
   * move. */
  DS_NO_SUCH_ATLAS = 20
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
 * A LOAD THAT FAILS RELEASES NOTHING. Every status any loader returns is raised
 * before the arena is replaced, so a refused load leaves the previously loaded
 * document drawable and its mapping held. Do not unlink the previous file until
 * a later load has answered DS_OK. DS_PANIC is the one
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
 * Loads a .dsb by MAPPING the length bytes at offset inside path, bounded by
 * the root that is shown.
 *
 * ds_runtime_load_document_mapped is this call over a whole file, and
 * everything it documents holds here: the bound on what is read, the arena
 * owning the mapping, the root being named once, and a load that fails
 * releasing nothing.
 *
 * WHY A BYTE RANGE AND NOT A SECOND PATH. A .dsb does not always begin a file.
 * An Android APK stores an uncompressed asset as a range inside base.apk and
 * the platform reports that range — AssetManager.openFd gives a start offset
 * and a length — rather than a path of its own. Extracting the document so the
 * call above could take it is a full copy of the file, which is the cost
 * mapping exists to avoid.
 *
 * offset needs no alignment, and one that is not a page boundary is what an
 * APK gives you: zipalign aligns an ordinary stored entry to 4 bytes and
 * page-aligns shared objects only. That shifts the document's own page
 * boundaries against the process's, so a load faults some pages it otherwise
 * would not. It is a cost and not a refusal: nothing reads alignment, and the
 * shift is already there at offset 0 on any host whose page size is LARGER
 * than the format's 4096 — 16 KiB hosts included, which Android 15 requires
 * new devices to support. A host whose page size divides 4096 does not have
 * it, which is why the claim names the direction. See
 * docs/decisions/the-document-is-mapped-where-it-is-packed.md D4.
 *
 * A RANGE NAMING BYTES THE FILE DOES NOT HAVE IS DS_MAP, refused here rather
 * than mapped: mmap past the end of a file succeeds and answers SIGBUS when
 * the page is touched, which arrives with nothing naming the range that caused
 * it. So is a length of 0. There is no sentinel meaning "to the end of the
 * file", for the same reason shown_root has none meaning "every root" — a
 * caller that wants the whole file has ds_runtime_load_document_mapped.
 *
 * Adding this symbol did not move DS_ABI_VERSION, and it adds no DsStatus
 * variant: every failure it reports is one the whole-file loader already
 * reports.
 */
DsStatus ds_runtime_load_document_mapped_range(DsRuntime runtime,
                                               const char *path, uint64_t offset,
                                               uint64_t length,
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
 * A borrowed, contiguous array of rows the runtime owns.
 *
 * You read it. You never free it. The bytes belong to the runtime's committed
 * tables and are valid until ds_runtime_release_frame returns.
 *
 * ptr is NULL exactly when count is 0, so an empty table needs no special case
 * and you never hold a pointer that names nothing.
 *
 * stride is NOT redundant with your own sizeof. It is this build's row size,
 * and comparing it against your sizeof before you read a row is how a layout
 * change becomes an error you report rather than geometry you draw wrong.
 * RectEntry went from 28 bytes to 40 at story #770, so this is not theoretical.
 *
 * stride is reported for an EMPTY array too, so you can validate all of them at
 * the top of the frame. Most documents leave several empty — a scene with no
 * gradients, no images and no blurs leaves most of them empty — and a stride of
 * 0 there would make that check reject every ordinary document.
 */
typedef struct DsSlice {
  const void *ptr;
  size_t count;  /* rows, not bytes */
  size_t stride; /* one row's size in bytes, in this build */
} DsSlice;

/*
 * One committed frame, as arrays you draw from.
 *
 * This is the inverse of ds_runtime_draw: that call hands dashscene a surface
 * and lets it paint, this hands you the tables and lets you paint. An engine
 * host with its own renderer — Unity over BatchRendererGroup — needs the
 * second.
 *
 * HOW THE ARRAYS RELATE. rects is the frame; every other array is either
 * indexed by a field on a row or is the flat backing an index names.
 *
 *   RectEntry.paint    -> paint_entries
 *   PaintEntry ranges  -> extra_fills, strokes, shapes, shadows, blurs
 *   PaintKind.index    -> solids | gradients | image_fills, by its tag
 *   Gradient.stops     -> gradient_stops
 *   RectEntry.clip     -> clip_regions, whose rows are ranges into clip_boxes
 *   ImageFill.image    -> image_entries, whose offset/len index image_payload
 *   GlyphRun.glyphs    -> glyph_quads
 *   groups             -> rect ranges that composite offscreen, and their alpha
 *
 * The row types are boundary B's, declared in crates/dashpaint and held to a C
 * representation by crates/dashpaint-abi. This header does not redeclare them:
 * a second declaration is a second place for them to go stale, and the layout
 * functions in that crate are how a consumer checks its own.
 *
 * WHAT IS NOT HERE. The glyph atlases, and they are not missing — they are
 * somewhere else. dashpaint::Atlas is an encoded sheet, four scalars and a
 * glyph list rather than a row, and it belongs to the LOAD rather than to the
 * commit: nothing here replaces it, so re-reading it per frame would be work
 * for a value that cannot have changed. Read it with ds_runtime_atlas, once
 * per load, keyed by a GlyphRun's atlas field. See DsAtlas below.
 */
typedef struct DsFrame {
  /* The commit this frame is. It moves when a tick commits.
   *
   * NOT AN IDENTITY ACROSS A LOAD. Each load installs a fresh arena whose
   * generation restarts, so a reloaded document's first frame can carry a
   * generation you have already drawn. Compare generations only within one
   * document, and read document_replaced to learn when that changed. */
  uint64_t generation;

  /* Discard every cached per-rect thing you hold: this frame's rect indices do
   * not name what the last one's did.
   *
   * True when a load has installed a fresh arena since your previous acquire,
   * or when the commit renumbered the rect table. Cleared by the acquire that
   * reports it, so you see each replacement exactly once.
   *
   * A host that hands dashscene a surface gets this as an internal call the
   * painter receives. You have no surface, so this member is how it reaches
   * you. */
  bool document_replaced;

  DsSlice rects;
  DsSlice groups;
  DsSlice dirty; /* uint32_t rect indices, relative to the PREVIOUS commit */

  DsSlice paint_entries;
  DsSlice extra_fills;
  DsSlice strokes;
  DsSlice shapes;
  DsSlice solids;
  DsSlice gradients;
  DsSlice gradient_stops;
  DsSlice image_fills;
  DsSlice shadows;
  DsSlice blurs;

  DsSlice clip_regions;
  DsSlice clip_boxes;

  DsSlice image_entries;
  /* uint8_t. Read only the ranges image_entries name — never the whole slice.
   *
   * For a mapped load this IS THE WHOLE .dsb FILE, not the assets: the entries'
   * offsets are file offsets. Uploading or hashing the slice wholesale touches
   * every page of the document and defeats the bound the mapped load exists
   * for. */
  DsSlice image_payload;

  DsSlice glyph_runs;
  DsSlice glyph_quads;
} DsFrame;

/*
 * Takes a lease on the committed frame and writes its arrays to out.
 *
 * THE LEASE. While one is outstanding, every call that would commit is refused
 * with DS_FRAME_LEASED: ds_runtime_tick, every loader, ds_runtime_free,
 * and a second acquire. That is what makes the borrowed views safe rather than
 * merely documented — a commit is the only thing that replaces the tables they
 * point into.
 *
 * RELEASE AFTER YOUR READERS FINISH, not when the call that dispatched them
 * returns. If you hand these pointers to worker threads, release once those
 * workers have completed — for a Unity host that means after Unity completes
 * the JobHandle, not on return from OnPerformCulling.
 *
 * The workers make no call into this library. They read memory. So nothing here
 * is thread-affine for them: the acquire and the release are the only calls,
 * and both are on the runtime's own thread like every other entry point.
 *
 * A FORGOTTEN RELEASE REFUSES EVERY LATER TICK. That is a real failure mode and
 * it is the intended one: it is diagnosable, where reading a freed table is not.
 *
 * Requires a document. A tick is not required: loading commits, so a frame is
 * available before the first ds_runtime_tick — and on a static document the
 * first tick commits nothing, so it is the same frame.
 *
 * ON FAILURE the frame is emptied — every count 0, every ptr NULL, every stride
 * still this build's row size — so a caller that ignores the status holds a
 * frame with no rows rather than uninitialised memory. That holds for every
 * status this call returns, DS_PANIC and DS_NULL_ARGUMENT included — the one
 * case with no write is a NULL out itself, where there is nowhere to write.
 * (DS_NULL_ARGUMENT is also what a handle of 0 gets, and that path does empty
 * the frame.)
 *
 * EXCEPT ON DS_FRAME_LEASED, which leaves *out EXACTLY AS YOU PASSED IT IN.
 * That is the one failure where your frame may be the live one: if you loop
 * with a single DsFrame and miss a release, emptying it would take away the
 * only copy of the pointers your workers are still reading. The corollary is
 * that a DS_FRAME_LEASED return tells you nothing about *out — if you passed an
 * uninitialised one, it is still uninitialised.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_acquire_frame(DsRuntime runtime, DsFrame *out);

/*
 * Ends the lease. Every pointer in that frame is invalid once this returns.
 *
 * PASS drawn NON-ZERO IF YOU PAINTED THIS FRAME. That marks the commit shown,
 * so a settled scene stops reporting out_advanced and you can idle. Pass 0 if
 * you took the frame and did not paint it — read its generation, decided
 * nothing was visible, ran out of budget — and it stays worth drawing.
 *
 * IT IS AN int32_t AND NOT A bool, AND THAT IS NOT A STYLE CHOICE. A bool
 * crossing INTO this library has exactly two valid bit patterns, and any other
 * is undefined behaviour where the arguments bind — before anything here can
 * turn it into a status. Every other bool on this surface is one WE write
 * through an out-pointer. Declaring this one as bool in a binding would
 * reintroduce that, and would also be an ABI mismatch, since the library takes
 * four bytes here.
 *
 * (The same hazard is why ds_runtime_attach_surface's kind takes an integer on
 * the Rust side. It is declared DsSurfaceKind here, which is sound because the
 * two are both four bytes; bool and int32_t are not, so this one is declared
 * as what it is.)
 *
 * It is a parameter rather than something this call assumes, and the difference
 * from ds_runtime_draw is why. That call also marks a commit shown without
 * knowing what reached the screen, but calling it is OPTIONAL. Releasing is
 * MANDATORY: nothing can tick again until the lease ends. So a release cannot
 * mean "I consumed this frame" on its own without counting an acquire you took
 * only to read a generation.
 *
 * Releasing without a lease succeeds and says so: out_was_leased, if non-NULL,
 * receives whether one was outstanding. A teardown path does not have to track
 * whether it is mid-frame. drawn is ignored when there was no lease.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_release_frame(DsRuntime runtime, int32_t drawn,
                                  bool *out_was_leased);

/*
 * One MSDF glyph atlas: the sheet, the four scalars a painter shades with, and
 * the per-glyph placement its runs resolve against.
 *
 * THE HALF OF THE TEXT SEAM A DsFrame CANNOT CARRY. The frame hands you the
 * runs and their quads, and a quad is a glyph id and a pen position — from that
 * alone you can compute neither the quad's corners nor its texture
 * coordinates. This is what closes it.
 *
 * WHY THIS IS A CALL AND NOT A MEMBER OF DsFrame. An atlas set is installed by
 * a LOAD and is not part of a commit. As a frame member it would say the set
 * can change per commit — it cannot — and you would have to invent your own
 * change detection to avoid re-uploading a texture every frame. It would also
 * need a new boundary-B row type, because dashpaint::Atlas is not a row. And
 * adding members to DsFrame changes its layout, where adding a symbol does
 * not, so DS_ABI_VERSION stays where it is.
 *
 * THE SHEET CROSSES TOO, AND THAT IS NOT REDUNDANT. png carries the bytes you
 * handed to ds_runtime_load_document_with_text as DsFontFace.atlas_png, so it
 * may look as though you already hold them. You hold them and CANNOT TELL
 * WHICH IS WHICH: an atlas index is the typesetter's font slot, and the
 * library builds that order by grouping your faces by family — trimmed, and
 * ASCII-case-insensitively — before flattening family-major. List one family's
 * faces non-contiguously and the atlas order is not your argument order, so
 * pairing
 * by array index samples another face's sheet RATHER THAN FAILING. That is the
 * hazard DsFontFace's own comment names for the library's internal pairing;
 * this closes it for you, and costs a pointer and a length because the runtime
 * already owns the encoded bytes.
 *
 * LIFETIME. Every pointer belongs to the runtime and is valid until the next
 * load or ds_runtime_free, whichever comes first. NOT until the next commit:
 * unlike a DsFrame, nothing here is replaced by a tick, which is what lets you
 * upload a sheet once per document. No lease is taken and none is required.
 */
typedef struct DsAtlas {
  /* The sheet's extent in texels. Never zero. */
  uint32_t width;
  uint32_t height;
  /* The size, in texels per em, the sheet was rendered at. Never zero.
   *
   * A uint32_t here and a u16 inside the library, widened because a two-byte
   * member among four-byte ones costs padding this header would have to name
   * and saves nothing. The domain is unchanged. */
  uint32_t px_per_em;
  /* The MSDF distance range in atlas TEXELS. Always finite and greater than
   * zero.
   *
   * Your screen-pixel range is distance_range_px * run.size / px_per_em.
   * plane_em and atlas_px bake the range into the bounds, so this scales the
   * sharpness of the edge and not the size. */
  float distance_range_px;
  /* uint8_t — the encoded sheet. ALWAYS A PNG: an atlas whose payload was not
   * one was refused with DS_ATLAS at load, against a header carrying the
   * extent above. */
  DsSlice png;
  /* dashpaint::AtlasGlyph rows — the placement of every glyph that paints,
   * SORTED AND UNIQUE BY glyph_id, so you may binary-search them. The row type
   * is boundary B's, held to a C representation by crates/dashpaint-abi like
   * every row a DsFrame carries.
   *
   * A glyph id with no row here draws nothing: an empty outline such as a
   * space, or a codepoint outside the sheet's charset. Coverage is settled at
   * build time by the atlas closure, and there is no runtime atlas rebuild to
   * ask for. */
  DsSlice glyphs;
} DsAtlas;

/*
 * How many glyph atlases the loaded document's runs sample.
 *
 * 0 for a document loaded without faces and for the measure-only cascade —
 * both stage no glyph runs, so neither is an error. A GlyphRun's atlas field
 * is an index below this count.
 *
 * Requires a document: without one this is DS_NO_DOCUMENT and not 0, because
 * "no document" and "a document with no text" are different answers.
 *
 * Takes no lease and is refused by none: the set belongs to the load rather
 * than to a commit, so this answers the same whether or not a frame is
 * outstanding.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_atlas_count(DsRuntime runtime, size_t *out_count);

/*
 * Describes the atlas at index and writes it to out.
 *
 * index is a GlyphRun's atlas field. An index at or past
 * ds_runtime_atlas_count's answer is DS_NO_SUCH_ATLAS rather than a clamp.
 *
 * READ IT ONCE PER LOAD, NOT ONCE PER FRAME. Upload each sheet when a frame
 * reports document_replaced and keep the texture until the next one.
 *
 * ON FAILURE the atlas is emptied — every count 0, every pointer NULL, every
 * scalar 0, every stride still this build's row size — so a caller that
 * ignores the status holds an atlas describing nothing rather than
 * uninitialised memory. The one case with no write is a NULL out itself.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_atlas(DsRuntime runtime, uint32_t index, DsAtlas *out);

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
