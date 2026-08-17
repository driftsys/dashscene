//! The C ABI every platform host sits on (story #840, D2 of
//! `docs/decisions/host-integration-in-three-layers.md`).
//!
//! Kotlin reaches this through JNI, and Swift would reach the same symbols
//! through the same header when iOS lands in v1. An out-of-process AIDL service,
//! if it is ever taken, would be another client of these symbols rather than a
//! second runtime — which is what keeps that deferral additive.
//!
//! # What is here, and what is deliberately not
//!
//! **Layer 0's needs and no more**: create and destroy a runtime, load a
//! document, hand it a surface, drive the tick, resize, and report why something
//! failed. Signals (layer 1) and the builder projection (layer 2) are deferred
//! with their layers, and D8 already records that a chatty handle-based builder
//! ABI is affordable because scenes are built outside the frame loop — so
//! nothing here has to pre-empt that shape.
//!
//! **Root selection is on the mapped entry point and on no other, and that
//! asymmetry is the design rather than an omission** (issue #925).
//! `dashbuf::prefetch::ShownRoot` is the vocabulary, `dashscene-desktop` and
//! `dashscene-web` both take one, and
//! `docs/decisions/the-shown-root-is-named-by-ordinal.md` records why it is an
//! ordinal. **`dashscene-android` takes one too since issue #1035**:
//! `nativeSurfaceCreatedMapped` passes a path and an ordinal and reaches
//! [`ds_runtime_load_document_mapped`], so the crate that motivated the bounded
//! path is no longer the one paying the unbounded cost. Its byte-taking entry
//! points remain, reaching [`ds_runtime_load_document_with_text`], for a host
//! that holds bytes rather than a file.
//!
//! **An APK asset is not a path**, which is what made that host the last to
//! arrive: an asset compressed inside the APK cannot be mapped, and an
//! uncompressed one is reachable only as a descriptor plus an offset and a
//! length. The host extracts the document to app storage once and passes that
//! path, so no descriptor-taking variant was needed. That variant — and the
//! `dashbuf::map` constructor over a range of an open descriptor that it needs
//! — stays deferred, as it was when #925 landed.
//!
//! [`ds_runtime_load_document_mapped`] takes a path, maps it, and reads out of
//! the file's cold half only the assets the named root's subtree draws. That is
//! R5 on this ABI, and it is the whole reason the symbol exists.
//!
//! [`ds_runtime_load_document`] takes the whole file as `(ptr, len)` and hands
//! every payload to `dashscene_core::load_document` — the **owning** loader,
//! which copies every payload into an owned `ImageAsset` and so needs bytes for
//! every asset entry whether or not anything draws them. **A `ShownRoot`
//! parameter on that call was rejected rather than deferred**: it would have
//! been accepted and changed nothing measurable, which is worse than its
//! absence, because it would read as a bound that is not one. It would also
//! have been a changed signature on a shipped symbol, which bumps
//! [`DS_ABI_VERSION`], where a new symbol is free.
//!
//! So the two loads differ in what they can promise, and a caller chooses by
//! what it holds. Bytes it already read: [`ds_runtime_load_document`], whole
//! file, no bound, and the cost is the file's. A file on disk:
//! [`ds_runtime_load_document_mapped`], and the cost is the artboard's.
//!
//! Story #838 is what made the second worth building — the traversal, the solve
//! and the paint follow the root a host names, so the bound reaches the frame
//! loop and not only the load.
//!
//! **The root is named once, at load.** There is no symbol here for changing it
//! afterwards, so a renumbering can only be raised by the load's own commit,
//! and `load_into` and `load_mapped_into` both report `document_replaced`
//! immediately after it.
//!
//! [`ds_runtime_tick`] reads the renumbering gate anyway, through
//! `LiveScene::take_renumbering` (issue #945). That is the rule rather
//! than a fix for a defect reachable today: the gate is the same one
//! `dashscene-desktop` and `dashscene-web` read, stated once in `dashlang`
//! instead of copied into each host, and this crate held no copy at all while
//! AGENTS.md listed three integration surfaces. It is correct here and
//! unreachable, deliberately — so the day a root-switching symbol lands, this
//! host is already right rather than quietly wrong.
//!
//! # What a host supplies for text
//!
//! [`ds_runtime_load_document_with_text`] takes the fonts and atlases a
//! document's text needs, because the document carries neither and cannot:
//! `docs/decisions/font-resolution-order.md` makes an embedded font step 1
//! and records why nothing implements it, and a rasterised atlas must never
//! be embedded at all — it is a result, and P1 forbids results in the
//! document.
//!
//! A face ([`DsFontFace`]) is its font file's bytes plus the family and the
//! CSS weight it stands for, the weight in `1..=1000` and refused as
//! [`DsStatus::FontFace`] outside it. An atlas is a committed MSDF sheet: a PNG and
//! the postcard metrics blob beside it, which is what `corpus/atlas/*/`
//! holds. **A face's `atlas_png` and `atlas_metrics` must both be null or
//! both point at real bytes.** Both null is the measure-only cascade — text
//! is shaped and measured, and no glyph run is staged. Exactly one null is
//! [`DsStatus::Atlas`], not a silent fall back to measure-only, and so is a
//! mixed set where some faces in the call carry a sheet and some do not.
//!
//! **Nothing bakes an atlas at run time**, and that is a constraint a host
//! plans around rather than a gap that will close on its own.
//! `dashscene_typeset::atlas::generate` shells out to an external pinned
//! binary and reads its font from a path
//! (`docs/decisions/atlas-gen-external-pinned-binary.md`). So a sheet is
//! built where the build runs, and travels with the host.
//!
//! [`ds_runtime_load_document`] is the same call with no faces, and stays
//! exactly that: a document loaded through it lays its text nodes out as
//! **empty leaves** and draws **no glyphs**, and the damage is not confined
//! to the missing letters — a hug-sized text node that measures to nothing
//! makes its siblings lay out around a box the design did not specify. That
//! is now a choice a caller makes rather than one made for it, which is what
//! story #947 changed.
//!
//! `dashscene_android::host` reaches both loads — [`ds_runtime_load_document_with_text`]
//! from its byte-taking entry points and [`ds_runtime_load_document_mapped`]
//! from `nativeSurfaceCreatedMapped`, which passes a path and an ordinal
//! (issue #1035). Its lifecycle harness runs on an emulator; the showcase's
//! frame-rate deliverable needs target hardware, which is issue #885, so
//! nothing here describes Android as measured.
//!
//! # The three rules this ABI keeps
//!
//! 1. **No panic crosses the boundary. Every entry point runs inside
//!    [`std::panic::catch_unwind`]** — stated as an absolute so a new one
//!    cannot satisfy it by its author asserting it cannot panic, which is how
//!    [`ds_last_error_message`] came to be unguarded once. An unwind across
//!    `extern "C"` is undefined behaviour.
//!
//!    `ds_abi_version` is the single exception and returns a `const`. It is
//!    named here rather than left to be discovered, and it is the only body in
//!    this crate simple enough to earn that.
//!
//!    Those returning a [`DsStatus`] use `guard`, which turns an unwind
//!    into [`DsStatus::Panic`]. [`ds_runtime_free`] and
//!    [`ds_last_error_message`] catch one directly instead, because neither
//!    has a status to report it in — the first returns `void` and the second a
//!    length — so each swallows it rather than naming it.
//!    [`ds_abi_version`] is the only entry point with no `catch_unwind` at
//!    all, and it returns a constant.
//!
//!    The property is **catching an unwind**, not calling `guard`: two entry
//!    points hold it without the helper, so counting `guard` under-reports.
//!
//!    This is **not** the workspace's only `extern` boundary. `dashscene-android`
//!    has an `AChoreographer` callback the NDK invokes and six JNI entry points,
//!    `demo-android` four more, and `dashc` five — none of them catches an
//!    unwind. This rule is this crate's; it is not evidence about theirs.
//! 2. **No failure is representable only as a formatted string.** [`DsStatus`]
//!    is a stable enum and is the contract; the message from
//!    [`ds_last_error_message`] is diagnostic and carries no promises. That is
//!    the lesson issues #815 and #819 record one layer up, where an adapter was
//!    reachable only as a pre-formatted line.
//! 3. **Every pointer is checked.** A null where a value is required is
//!    [`DsStatus::NullArgument`] rather than a dereference.
//!
//! # Versioning
//!
//! [`ds_abi_version`] returns [`DS_ABI_VERSION`], which is **not** the crate's
//! semantic version. It is a single integer, and the rule is:
//!
//! - Adding a symbol, or a variant at the **tail** of [`DsStatus`], does not
//!   change it. A host built against an older header keeps working.
//! - Changing or removing a symbol's signature, or renumbering a `DsStatus`
//!   variant, bumps it.
//!
//! A host should call [`ds_abi_version`] once and refuse to run against a value
//! it does not recognise, because the alternative is discovering the mismatch as
//! a corrupted argument.

use std::cell::RefCell;
use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use dashbuf::map::MappedFile;
use dashbuf::prefetch::ShownRoot;
use dashbuf::residency::BlobResidency;
use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_core::{Arena, MappedPayload, Region};
use dashscene_engine::{TaffySolver, TextResources, TextResourcesError};
use dashscene_gpu::{Changes, Drawn, GpuPainter, SurfaceRenderer};

/// The ABI generation. See the module's "Versioning" section for what moves it.
pub const DS_ABI_VERSION: u32 = 1;

/// Returns [`DS_ABI_VERSION`], so a host can refuse a library it does not know.
///
/// Deliberately the one entry point that cannot fail and takes no handle: a
/// host has to be able to ask before it commits to anything.
#[unsafe(no_mangle)]
pub extern "C" fn ds_abi_version() -> u32 {
    DS_ABI_VERSION
}

/// Why a call did not succeed.
///
/// `#[repr(i32)]` and explicitly numbered, because the discriminants are the
/// contract rather than an implementation detail — a variant inserted in the
/// middle would silently renumber every one after it. New variants go at the
/// tail.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsStatus {
    /// The call succeeded.
    Ok = 0,
    /// A pointer argument that must be non-null was null.
    NullArgument = 1,
    /// The bytes are not a `.dsb` this runtime can open.
    Open = 2,
    /// The document opened but does not pass the referential load gate.
    Gate = 3,
    /// The surface could not be created, or the painter could not start on it.
    Surface = 4,
    /// The handle kind is not one this build supports. A host asking for
    /// [`DsSurfaceKind::AndroidNdk`] on a non-Android build gets this.
    UnsupportedHandle = 5,
    /// The call needs a document and none is loaded.
    NoDocument = 6,
    /// The call needs a surface and none is attached.
    NoSurface = 7,
    /// A panic was caught at the boundary. The library is left in an
    /// unspecified state and the runtime should be freed without further calls.
    Panic = 8,
    /// A face descriptor is unusable: its `family` is not UTF-8, its `family`
    /// is empty or only whitespace, its `weight` is outside `1..=1000`, or its
    /// `font_bytes` do not parse as a font face.
    FontFace = 9,
    /// An atlas is unusable: its `atlas_metrics` did not decode, its
    /// `atlas_png` is not a PNG header carrying the extent those metrics
    /// declare, a glyph in those metrics is described by exactly one of its
    /// two quads, or the set is mixed — some faces carrying a sheet and some
    /// not. The atlas list is indexed by font slot, so a short one resolves
    /// past its end.
    Atlas = 10,
    /// The path could not be used: nothing is there, it cannot be read, it is
    /// empty, or it is not UTF-8. Only [`ds_runtime_load_document_mapped`]
    /// reports it, because it is the only entry point that takes a path.
    Map = 11,
    /// The ordinal names no root in this document.
    ///
    /// The message from [`ds_last_error_message`] carries the ordinal that was
    /// asked for and the count the document does carry, which is what tells an
    /// out-of-range ask apart from a document with no roots at all.
    NoSuchRoot = 12,
    /// The file's payloads are derivations rather than the document's own
    /// canonical bytes.
    ///
    /// A mapped load reads no payload header by design, so binding these would
    /// tag one format as another with nothing downstream to catch it (issue
    /// #640). This library ships no profile and cannot name a rung, so it
    /// refuses the file rather than drawing the wrong thing.
    Derived = 13,
    /// An asset the shown root draws did not hash to what its entry names.
    Payload = 14,
    /// The frame failed because the surface was **lost**, and rebuilding the
    /// presenter is the remedy (issue #884).
    ///
    /// Only [`ds_runtime_draw`] reports it, and only for the one case
    /// `dashscene_gpu::FrameError::is_recoverable` names. Every other surface
    /// failure stays [`DsStatus::Surface`], including a swapchain still out of
    /// date after being reconfigured and a validation error raised inside the
    /// acquire: rebuilding for either is a loop rebuilding a device to meet the
    /// same failure.
    ///
    /// **This is the rule story #834 put in one place, reaching the ABI.**
    /// `dashscene-web`, `dashscene-desktop` and `demo-android` all branch on
    /// `is_recoverable` directly; a host on this ABI could not, so
    /// `dashscene-android` treated every `Surface` as recoverable and relied on
    /// its own bound on consecutive rebuilds to stop an unrecoverable one
    /// spinning. That was a guess, and this is the answer.
    ///
    /// **Additive, so [`DS_ABI_VERSION`] does not move** — but read the next
    /// paragraph before relying on that, because this variant is not purely
    /// additive in effect.
    ///
    /// A lost swapchain used to arrive as [`DsStatus::Surface`] and now arrives
    /// here, so an existing condition changed which discriminant it reports. A
    /// host built against a header that predates this sees a value it does not
    /// recognise and stops. That **loses a recovery it did have** — the host in
    /// this repository is the proof, since it rebuilt on every
    /// `DsStatus::Surface` — so it is not the free change the module's
    /// versioning rule describes, which covers adding a variant rather than
    /// re-routing a condition onto one.
    ///
    /// It is taken anyway, and deliberately: the failure direction is a loop
    /// that stops rather than one that acts on a value it cannot interpret,
    /// there is exactly one host on this ABI and it is updated in the same
    /// change, and issue #884 specifies the version not moving. What the rule
    /// does not yet say is what a re-routed condition costs; that is a gap in
    /// the rule rather than in this variant.
    SurfaceLost = 15,
}

/// Which platform handle the pointers in [`ds_runtime_attach_surface`] carry.
///
/// D3 gives each platform a small handle type, so this enum is the ABI's half
/// of that: one tag per platform, and the conversion lives behind the `cfg` for
/// the platform that owns it.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsSurfaceKind {
    /// `window` is an `ANativeWindow *`, from `ANativeWindow_fromSurface`.
    /// `display` is ignored — `RawDisplayHandle::Android` carries nothing.
    AndroidNdk = 0,
}

// **A false positive** (issue #1086). The initializer below already *is*
// `const { … }`, which is exactly what `missing_const_for_thread_local` asks
// for, and clippy fires anyway when the target is `aarch64-linux-android`. The
// host and `wasm32` runs are clean on the same source, so it is the lint rather
// than the code.
//
// **`allow`, not `expect`, and the reason is measured.** `expect` was written
// first, because it deletes itself: it errors as "this lint expectation is
// unfulfilled" once a release stops firing the lint. It does not survive here,
// because whether the lint fires is not a property of the target alone —
//
//     host aarch64-apple-darwin,   target aarch64-linux-android   fires
//     host x86_64-unknown-linux-gnu, target aarch64-linux-android  does not
//
// — both on rustc/clippy 1.97.1 (8bab26f4f 2026-07-14), the same build. So
// `expect` is fulfilled on a developer's machine and unfulfilled on the runner,
// and `-D warnings` implies `-D unfulfilled_lint_expectations`: the attribute
// that silences one turns the other red. That is not a hypothetical — it is how
// this branch's first CI run failed. `allow` is inert either way.
//
// The `cfg_attr` stays: the lint reaches no other target, and an unconditional
// allowance would silence a real occurrence on the host build.
thread_local! {
    /// The last failure's message, for [`ds_last_error_message`].
    ///
    /// Thread-local rather than held on the runtime, because the calls that can
    /// fail before a runtime exists — [`ds_runtime_new`] itself — still need
    /// somewhere to report. A host that calls across threads reads the message
    /// on the thread that failed, which is the only reading that is meaningful.
    #[cfg_attr(
        target_os = "android",
        allow(
            clippy::missing_const_for_thread_local,
            reason = "false positive: the initializer below is already \
                      `const { … }` (issue #1086)"
        )
    )]
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_last_error(message: impl Into<String>) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message.into());
}

/// Runs `body`, turning an unwind into [`DsStatus::Panic`] rather than letting
/// it cross `extern "C"`.
fn guard(body: impl FnOnce() -> DsStatus) -> DsStatus {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(status) => status,
        Err(_) => {
            // Deliberately not the panic payload: formatting it here would run
            // arbitrary `Display` code on the way out of a panic.
            set_last_error("a panic was caught at the ABI boundary");
            DsStatus::Panic
        }
    }
}

/// A live runtime: the arena, the scene over it, and the surface it draws to.
///
/// Opaque to C. A host holds a `DsRuntime *` and nothing else, so the layout
/// below is free to change without moving [`DS_ABI_VERSION`].
pub struct DsRuntime {
    arena: Arena,
    scene: Option<LiveScene>,
    surface: Option<SurfaceRenderer>,
    /// Boundary B's implementation: it turns the committed tables into an
    /// instance buffer and knows nothing about the window. Held rather than
    /// built per frame, because it owns the packing buffers whose byte ranges
    /// the dirty set decides to upload.
    painter: GpuPainter,
}

/// Creates an empty runtime and writes it to `out`.
///
/// The runtime holds no document and no surface yet; both are separate calls,
/// for the reason `dashscene-web` gives about its own split — a scene built in
/// code needs an extent to build *for*, so a host attaches, reads the extent,
/// and then loads.
///
/// # Safety
///
/// `out` must be a valid, writable `DsRuntime *`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_new(out: *mut *mut DsRuntime) -> DsStatus {
    guard(|| {
        if out.is_null() {
            set_last_error("ds_runtime_new: out is null");
            return DsStatus::NullArgument;
        }
        let runtime = Box::new(DsRuntime {
            arena: Arena::new(),
            scene: None,
            surface: None,
            painter: GpuPainter::new(),
        });
        unsafe { *out = Box::into_raw(runtime) };
        DsStatus::Ok
    })
}

/// Frees a runtime. Null is accepted and does nothing, like `free`.
///
/// # Safety
///
/// `runtime` must be a pointer from [`ds_runtime_new`] that has not already been
/// freed, and no other call may be in flight on it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_free(runtime: *mut DsRuntime) {
    if runtime.is_null() {
        return;
    }
    // Not `guard`: this returns no status, and a panic in `Drop` is not
    // something a status could report anyway.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        drop(unsafe { Box::from_raw(runtime) });
    }));
}

/// Loads a `.dsb` held in memory.
///
/// This is the **owning** path: `dashscene_core::load_document` copies every
/// payload, so the cost tracks the file rather than the shown root. That is the
/// honest shape for a caller that handed over bytes it already holds — there is
/// no file here to map and no root to bound by.
///
/// **A caller holding the file rather than its bytes wants
/// [`ds_runtime_load_document_mapped`] instead**, which maps it and reads only
/// what the shown root draws. This doc comment deferred that path to story
/// #841 until issue #925; the story closed without doing it, and the deferral
/// outlived it by pointing at an owner that never took it.
///
/// # Safety
///
/// `bytes` must point to `len` readable bytes, and `runtime` must be a live
/// pointer from [`ds_runtime_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_load_document(
    runtime: *mut DsRuntime,
    bytes: *const u8,
    len: usize,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() || bytes.is_null() {
            set_last_error("ds_runtime_load_document: runtime or bytes is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };
        load_into(runtime, bytes, None)
    })
}

/// Drops the runtime's current document: the scene first, then the arena it
/// indexes.
///
/// **The two are one statement, which is why they are one function.** A
/// `LiveScene` holds the [`NodeId`](dashscene_core::NodeId)s of the arena it was
/// attached to, so a runtime carrying the previous scene beside a fresh arena
/// drives ids against an arena that does not have them — wrong nodes written, or
/// an index panic, silently in a release build. Every loader replaces the arena,
/// and none of them may leave the scene behind.
///
/// It is the **panic** path that makes the pairing load-bearing rather than
/// tidy. Nothing between this call and a loader's reassignment of
/// `runtime.scene` returns, but the calls in between can unwind — the loaders'
/// own `load_document` and `load_document_mapped` each carry a `# Panics`
/// clause, `show_appended_root` panics by design, and `dashlang::attach_live`
/// carries one too and commits through the solver besides — and `guard` turns an
/// unwind into [`DsStatus::Panic`] with the runtime still alive. The next
/// [`ds_runtime_tick`] then answers [`DsStatus::NoDocument`], which is true,
/// instead of ticking the previous document's scene against the new arena.
///
/// **The mapped loader had the pairing and the byte-taking one did not**, which
/// is issue #1183: one invariant, two call sites, and nothing holding them
/// together. A third loader is deferred rather than rejected — see this module's
/// note on descriptors — so the pairing is a function rather than a convention.
///
/// It also frees the previous arena **before** the next document is built rather
/// than after, which is what keeps a second load off a peak of two whole arenas.
///
/// # Where a caller puts it
///
/// **Below every step that can return a status**, so a refused load leaves the
/// previously loaded document drawable. That ordering belongs to each loader
/// rather than to this function, and each carries a test for it.
fn drop_document(runtime: &mut DsRuntime) {
    runtime.scene = None;
    runtime.arena = Arena::new();
}

/// The load both entry points run. `text` is what the caller could supply.
///
/// Every failure this returns is raised **before** [`drop_document`], so a
/// refused load leaves a previously loaded document drawable —
/// `load_mapped_into`'s promise, on this path, and the reason the document is
/// dropped after the open and the gate rather than beside the rest of the setup.
///
/// [`ds_runtime_load_document_with_text`] can also refuse **above** this
/// function, in `text_from_c`, which is why no test drives that entry point for
/// the property: a cascade rejected there has not reached the document at all.
fn load_into(runtime: &mut DsRuntime, bytes: &[u8], text: Option<TextResources>) -> DsStatus {
    let (document, payloads) = match dashbuf::open_verified(bytes) {
        Ok(opened) => opened,
        Err(error) => {
            set_last_error(format!("{error:?}"));
            return DsStatus::Open;
        }
    };
    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        set_last_error(format!("{report:?}"));
        return DsStatus::Gate;
    }
    // A fresh arena per load, so a second load does not stack a second
    // document on the first. The generation restart that implies is exactly
    // what `document_replaced` is for, and it is reported below.
    //
    // Here rather than higher up: everything above can still return, and
    // `a_refused_byte_load_leaves_the_loaded_document_drawable` guards that.
    // `load_document` below carries the unwind `drop_document` exists for
    // (issue #1183).
    drop_document(runtime);
    dashscene_core::load_document(&document, &payloads, &mut runtime.arena);
    // `TaffySolver::boxed` rather than the same two arms written here: it
    // exists so that no document loader can disagree with another about what
    // `None` means, and `dashscene-desktop` and `dashscene-web` both call it.
    runtime.scene = Some(dashlang::attach_live(
        &mut runtime.arena,
        TaffySolver::boxed(text),
    ));
    announce_document_replaced(runtime);
    DsStatus::Ok
}

/// Tells an attached surface that the document under it was replaced.
///
/// The other half of [`drop_document`], and a function for the same reason: a
/// loader that dropped the old document and forgot this leaves the painter
/// describing the outgoing arena while the incoming one's generations restart
/// from zero — the failure `dashscene-desktop` and `dashscene-web` both name
/// under "rebuilding on resize". Nothing in the frames themselves says the arena
/// changed, so this call is the only notice there is.
///
/// **Called after the new scene is attached, not beside the drop**, which is why
/// it is not part of [`drop_document`]: what it announces is a document that has
/// been replaced, and until the reassignment there is not one.
fn announce_document_replaced(runtime: &mut DsRuntime) {
    if let Some(surface) = runtime.surface.as_mut() {
        surface.document_replaced();
    }
}

/// The mapped load, bounded by `shown_root`.
///
/// Every failure this returns is raised **before** the runtime's arena is
/// replaced, so a refused load leaves a previously loaded document drawable.
/// That is why the arena assignment sits below all of the fallible steps rather
/// than beside the rest of the setup.
fn load_mapped_into(
    runtime: &mut DsRuntime,
    path: &str,
    shown_root: ShownRoot,
    text: Option<TextResources>,
) -> DsStatus {
    let file = match MappedFile::open(path) {
        Ok(file) => Arc::new(file),
        Err(error) => {
            set_last_error(format!("{path}: {error}"));
            return DsStatus::Map;
        }
    };
    let bytes = file.bytes();

    // Reads the envelope, every structured section and the binding, and stops
    // at where each payload lies. No blob page is faulted in by this call —
    // `dashbuf::open` rather than `open_verified`, which is the whole
    // difference between this entry point and the owning one.
    let (document, wanted) = match dashbuf::open(bytes) {
        Ok(opened) => opened,
        Err(error) => {
            // Named, unlike `load_into`'s copy of this arm. That one was handed
            // anonymous bytes and has nothing to name; this one was given a
            // path, and these two statuses are the likeliest to fire on a bad
            // file — a host retrying a second document must be able to tell the
            // two failures apart.
            set_last_error(format!("{path}: {error:?}"));
            return DsStatus::Open;
        }
    };

    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        set_last_error(format!("{path}: {report:?}"));
        return DsStatus::Gate;
    }

    // Bound as canonical, and refused when that would be a lie. This path reads
    // no payload header, so a derivation bound here would be tagged with the
    // format the entry names and nothing downstream would catch it (issue
    // #640). This crate ships no profile and cannot name a rung.
    if let Some(index) = dashscene_core::first_derived_payload(&document, &wanted) {
        set_last_error(format!(
            "{path}: asset entry {index} resolves to a derived payload, and a mapped load reads \
             no payload header, so binding it would tag one format as another"
        ));
        return DsStatus::Derived;
    }

    let root = match dashbuf::prefetch::resolve(&document, shown_root) {
        Some(root) => root,
        None => {
            set_last_error(format!(
                "{path}: ordinal {} names no root, and the document carries {}",
                shown_root.ordinal(),
                dashbuf::prefetch::root_count(&document),
            ));
            return DsStatus::NoSuchRoot;
        }
    };

    // The prefetch, and the whole of what this reads out of the file's cold
    // half: the assets the shown root's subtree draws, proven one at a time.
    // Everything else stays cold, which is what makes cold start track the root
    // being drawn rather than the file's size (R5).
    //
    // A row bound below whose payload was not touched is not proven, and that
    // is safe only because the traversal is confined to the same root: story
    // #838 made the solve, the committed table and the paint follow it, so a
    // row no rect references is a row no painter resolves.
    let residency = BlobResidency::new();
    for index in dashbuf::prefetch::assets_of_root(&document, root) {
        let want = &wanted[index as usize];
        let payload = &bytes[want.range.start as usize..want.range.end as usize];
        if let Err(error) = residency.touch(want, payload) {
            set_last_error(format!("{path}: {error:?}"));
            return DsStatus::Payload;
        }
    }

    // One `MappedPayload` per asset entry, in entry order — exactly the order
    // `dashbuf::open` returns its `Wanted`s in, undeduplicated, so nothing here
    // reorders or expands.
    let payloads: Vec<MappedPayload> = wanted
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
        .collect();

    // A fresh arena per load, so a second load does not stack a second document
    // on the first. `Txn::use_mapped_pool` refuses an arena whose image table
    // already holds rows, whatever put them there, and this is what keeps that
    // condition out of reach.
    //
    // Here rather than higher up: nothing below returns, so this could sit
    // beside the rest of the setup, and it does not — every failure above is
    // raised before the arena is replaced, which is what
    // `a_refused_mapped_load_leaves_the_loaded_document_drawable` asserts.
    // The calls that can unwind between here and the reassignment are
    // `load_document_mapped` on a payload past 4 GiB, `show_appended_root` by
    // design, and `attach_live`; the byte-taking loader's window holds the
    // last of those and `load_document`.
    drop_document(runtime);
    // Zero, and stated as a literal rather than measured, because
    // `drop_document` above installed a new arena and a measurement here could
    // only ever return 0 —
    // reading it back would suggest this path can see a non-empty arena, which
    // is exactly the confusion `show_appended_root`'s parameter exists to
    // resolve. The two hosts that pass a real value take a caller-supplied
    // `&mut Arena`; this entry point owns its arena, so the answer is a
    // property of the code rather than of the caller.
    let roots_before = 0;
    // The region the table points into is this same mapping, shared rather than
    // opened again: the arena holds its own reference, so the mapping outlives
    // this function and unmaps when the arena it fed is replaced. That is why
    // no field on `DsRuntime` holds it and why the C caller keeps no lifetime
    // rule.
    let region: Arc<dyn Region> = file.clone();
    dashscene_core::load_document_mapped(&document, region, &payloads, &mut runtime.arena);
    // The runtime's half of the bound the prefetch above took (story #838,
    // issue #822). The correction from the document ordinal to the arena node,
    // and the argument for its panic, are `show_appended_root`'s own
    // documentation.
    dashscene_core::show_appended_root(
        &document,
        shown_root,
        roots_before,
        &path,
        &mut runtime.arena,
    );

    runtime.scene = Some(dashlang::attach_live(
        &mut runtime.arena,
        TaffySolver::boxed(text),
    ));
    announce_document_replaced(runtime);
    DsStatus::Ok
}

/// The faces a caller supplied, assembled — or the status that says why they
/// could not be. `Ok(None)` is the measure-only cascade: no faces were offered.
///
/// Both loading entry points read faces the same way, and the assembly happens
/// **before** the document is opened so that a bad cascade is reported as
/// itself rather than as whatever the document turned out to be.
/// `tests/abi.c` depends on that ordering.
///
/// # Safety
///
/// `faces` must point to `face_count` readable [`DsFontFace`] whose own
/// pointers are valid for the lengths beside them.
unsafe fn text_from_c(
    faces: *const DsFontFace,
    face_count: usize,
) -> Result<Option<TextResources>, DsStatus> {
    if face_count == 0 {
        return Ok(None);
    }
    let described = unsafe { faces_from_c(faces, face_count) }?;
    match TextResources::from_faces(described) {
        Ok(text) => Ok(Some(text)),
        Err(error) => {
            // `{error}` rather than `{error:?}`: this string reaches a host
            // through `ds_last_error_message`, where every other message is
            // prose, and nested `Debug` would put escaped quotes in front of
            // it.
            set_last_error(format!("{error}"));
            // Every variant that exists is named rather than swept into the
            // wildcard. `TextResourcesError` is `#[non_exhaustive]` and lives
            // in another crate, so an arm for the unknown is still required —
            // but a future atlas-shaped variant landing there would report as a
            // font-face failure and send a host branching on the discriminant
            // to the wrong half of its own descriptor. Naming them is what
            // makes the compiler's requirement the only thing the wildcard
            // carries.
            Err(match error {
                TextResourcesError::Atlas { .. } | TextResourcesError::MixedAtlases => {
                    DsStatus::Atlas
                }
                TextResourcesError::NoFaces
                | TextResourcesError::EmptyFamily { .. }
                | TextResourcesError::Weight { .. }
                | TextResourcesError::Font { .. } => DsStatus::FontFace,
                _ => DsStatus::FontFace,
            })
        }
    }
}

/// One face a host hands [`ds_runtime_load_document_with_text`], with the
/// atlas its shaped glyphs sample.
///
/// **The atlas is in here rather than in a second array on purpose.** The
/// atlas list is indexed by the font slot of the face that shaped a glyph,
/// so a list in any other order samples the wrong face rather than failing.
/// Pairing them here means the library builds both from one walk and a
/// caller cannot get the order wrong — including when it lists one family's
/// faces non-contiguously.
///
/// `atlas_png` and `atlas_metrics` must both be null or both point at real
/// bytes. Both null is the measure-only cascade: text is shaped and
/// measured, and no glyph run is staged. Exactly one null is
/// [`DsStatus::Atlas`], not a silent fall back to measure-only — and so is a
/// mixed set across faces, where some carry a sheet and some do not.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DsFontFace {
    /// The family, as NUL-terminated UTF-8. Faces sharing a name become one
    /// family however they are ordered in the array.
    pub family: *const c_char,
    /// The CSS weight this face stands for, in `1..=1000`.
    ///
    /// **Validated once, in `dashscene_engine::TextResources::from_faces`**
    /// since issue #1206 — not here, and not in a host. It was here and that
    /// constructor did not check, so the same descriptor was refused on this
    /// route and accepted on the Rust one. A value outside that range is
    /// [`DsStatus::FontFace`], naming the face's index and the value —
    /// including 0, which is what an uninitialised descriptor carries and
    /// which no CSS weight can be. Every host on this ABI inherits the one
    /// rule rather than repairing the value in its own way; the JNI half in
    /// `dashscene-android` clamped it until story #947's review, which meant
    /// two answers to one question.
    pub weight: u16,
    /// Which face within a collection. Zero for a single-face file.
    pub face_index: u32,
    pub font_bytes: *const u8,
    pub font_len: usize,
    pub atlas_png: *const u8,
    pub atlas_png_len: usize,
    pub atlas_metrics: *const u8,
    pub atlas_metrics_len: usize,
}

/// Reads the descriptors into owned bytes, or says which pointer was null.
///
/// # Safety
///
/// `faces` must point to `count` readable descriptors whose own pointers are
/// valid for the lengths beside them.
unsafe fn faces_from_c(
    faces: *const DsFontFace,
    count: usize,
) -> Result<Vec<dashscene_engine::FaceBytes>, DsStatus> {
    let faces = unsafe { std::slice::from_raw_parts(faces, count) };
    let mut out = Vec::with_capacity(count);
    for (index, face) in faces.iter().enumerate() {
        if face.family.is_null() || face.font_bytes.is_null() {
            // No entry-point prefix: this helper is shared by
            // `ds_runtime_load_document_with_text` and
            // `ds_runtime_load_document_mapped` since the latter landed, so
            // naming one of them here would send a reader of the other's log to
            // a symbol it never called. The sibling messages below already
            // prefix with `face {index}` and nothing else.
            set_last_error(format!("face {index} has a null family or font_bytes"));
            return Err(DsStatus::NullArgument);
        }
        let family = match unsafe { std::ffi::CStr::from_ptr(face.family) }.to_str() {
            Ok(family) => family.to_string(),
            Err(error) => {
                set_last_error(format!("face {index}: family is not UTF-8: {error}"));
                return Err(DsStatus::FontFace);
            }
        };
        // **The CSS range is not checked here** (issue #1206). It was, and
        // `dashscene_engine::TextResources::from_faces` did not — so the same
        // descriptor was refused on this route and accepted on the Rust one,
        // which PR #1197 widened the audience for by re-exporting `FaceBytes`
        // from both facades. The rule moved to that constructor, which every
        // route reaches, and a second copy here would be the predicate drift
        // this repository keeps paying for.
        //
        // A weight outside it is still `DsStatus::FontFace` with the same
        // sentence — `TextResourcesError::Weight` maps to it below — but **the
        // precedence changed and that is observable**. The refusal now runs
        // after every check in this loop rather than in the middle of it, so a
        // descriptor that is wrong in two ways at once reports the other one: a
        // face carrying both weight 0 and a half-null atlas pair answered
        // `DsStatus::FontFace` before and answers `DsStatus::Atlas` now.
        //
        // Accepted rather than repaired. Reporting the first fault found is
        // arbitrary whichever order it is in, and the alternative is a second
        // copy of the range predicate here — the drift issue #1206 exists to
        // remove. `a_face_wrong_in_two_ways_reports_the_atlas` pins it so the
        // order is a decision rather than a side effect.
        // Both null is the measure-only cascade. Exactly one null is a face
        // that half-described its atlas — silently falling back to
        // measure-only there would draw no glyphs for it while reporting
        // success, which is the silent gap P4 forbids by name.
        let atlas = if face.atlas_png.is_null() && face.atlas_metrics.is_null() {
            None
        } else if face.atlas_png.is_null() {
            set_last_error(format!(
                "face {index}: atlas_metrics is set but atlas_png is null"
            ));
            return Err(DsStatus::Atlas);
        } else if face.atlas_metrics.is_null() {
            set_last_error(format!(
                "face {index}: atlas_png is set but atlas_metrics is null"
            ));
            return Err(DsStatus::Atlas);
        } else {
            Some(dashscene_engine::AtlasBytes {
                png: unsafe { std::slice::from_raw_parts(face.atlas_png, face.atlas_png_len) }
                    .to_vec(),
                metrics: unsafe {
                    std::slice::from_raw_parts(face.atlas_metrics, face.atlas_metrics_len)
                }
                .to_vec(),
            })
        };
        out.push(dashscene_engine::FaceBytes {
            family,
            weight: face.weight,
            font: unsafe { std::slice::from_raw_parts(face.font_bytes, face.font_len) }.to_vec(),
            face_index: face.face_index,
            atlas,
        });
    }
    Ok(out)
}

/// Loads a `.dsb` held in memory, with the fonts and atlases its text needs.
///
/// [`ds_runtime_load_document`] is this call with no faces, and stays
/// exactly that. A null `faces`, or a zero `face_count`, is a document
/// loaded without text: its text nodes lay out as empty leaves and no glyph
/// run is staged.
///
/// **What a host must supply, and what it cannot get here.** A face is its
/// font file's bytes plus the family and weight it stands for. An atlas is a
/// committed MSDF sheet — a PNG and the metrics blob beside it — and
/// **nothing bakes one at run time**: the generator is an external pinned
/// binary that reads a font from a path, so these arrive with the host or
/// its text is measured and never drawn.
///
/// Adding this symbol did not move [`DS_ABI_VERSION`].
///
/// # Safety
///
/// `bytes` must point to `len` readable bytes, `runtime` must be live, and
/// `faces` must point to `face_count` readable [`DsFontFace`] whose own
/// pointers are valid for the lengths beside them. Nothing is retained:
/// every byte is copied before this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_load_document_with_text(
    runtime: *mut DsRuntime,
    bytes: *const u8,
    len: usize,
    faces: *const DsFontFace,
    face_count: usize,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() || bytes.is_null() {
            set_last_error("ds_runtime_load_document_with_text: runtime or bytes is null");
            return DsStatus::NullArgument;
        }
        if faces.is_null() && face_count != 0 {
            set_last_error(
                "ds_runtime_load_document_with_text: faces is null but face_count is not 0",
            );
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };

        // The faces are read and assembled BEFORE the document is opened, so
        // a bad cascade is reported as itself rather than as whatever the
        // document turned out to be. `tests/abi.c` depends on that ordering.
        let text = match unsafe { text_from_c(faces, face_count) } {
            Ok(text) => text,
            Err(status) => return status,
        };
        load_into(runtime, bytes, text)
    })
}

/// Loads a `.dsb` by **mapping** it from `path`, bounded by the root that
/// `shown_root` names.
///
/// The bounded counterpart of [`ds_runtime_load_document`], and the C ABI's
/// first expression of R5. The file is mapped rather than read, no payload is
/// copied, and the only bytes touched out of the file's cold half are the
/// assets the shown root's subtree draws — so the cost of opening a file tracks
/// the artboard being shown rather than the file's size. Issue #925 is what
/// this closes, and until it there was no mapped path here at all.
///
/// `shown_root` is a document ordinal and is **required**. There is no sentinel
/// for "every root": a caller that wants every root has
/// [`ds_runtime_load_document`] and pays the owning cost knowingly, and a bound
/// that can be switched off reads as a bound when it is not one.
///
/// `faces` and `face_count` carry the same rule as
/// [`ds_runtime_load_document_with_text`]: a null `faces`, or a zero
/// `face_count`, loads without text, and text nodes then lay out as **empty
/// leaves** and draw no glyphs.
///
/// **The mapping is the runtime's, and the caller has no lifetime rule to
/// keep.** The arena holds a reference to it and each load installs a fresh
/// arena, so the previous mapping unmaps when the previous arena drops. That
/// property is why this takes a path rather than a caller-supplied region,
/// where "keep this mapping alive until the document is replaced" would have
/// been a contract enforced only by prose across this boundary.
///
/// **What it does not do:** it names the shown root once, at load, and there is
/// no symbol here for changing it afterwards. A host that wants a different
/// artboard loads again.
///
/// Adding this symbol did not move [`DS_ABI_VERSION`], and neither did the four
/// [`DsStatus`] variants it reports — they are appended at the tail.
///
/// # Safety
///
/// `path` must be a NUL-terminated string, `runtime` must be a live pointer
/// from [`ds_runtime_new`], and `faces` must point to `face_count` readable
/// [`DsFontFace`] whose own pointers are valid for the lengths beside them.
/// Nothing about the faces is retained: every byte is copied before this
/// returns. The file itself is mapped and **is** retained, by the arena.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_load_document_mapped(
    runtime: *mut DsRuntime,
    path: *const c_char,
    shown_root: u32,
    faces: *const DsFontFace,
    face_count: usize,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() || path.is_null() {
            set_last_error("ds_runtime_load_document_mapped: runtime or path is null");
            return DsStatus::NullArgument;
        }
        if faces.is_null() && face_count != 0 {
            set_last_error(
                "ds_runtime_load_document_mapped: faces is null but face_count is not 0",
            );
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let path = match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(path) => path,
            Err(_) => {
                set_last_error("ds_runtime_load_document_mapped: path is not UTF-8");
                return DsStatus::Map;
            }
        };

        let text = match unsafe { text_from_c(faces, face_count) } {
            Ok(text) => text,
            Err(status) => return status,
        };
        load_mapped_into(runtime, path, ShownRoot::nth(shown_root), text)
    })
}

/// Advances the scene by `dt` seconds and reports whether anything changed.
///
/// `out_advanced` receives whether the commit moved the generation, which is
/// what says a frame is worth drawing. Delegated to `LiveScene`, not restated:
/// story #810 moved that rule onto `dashlang` so every host reads one copy.
///
/// # Safety
///
/// `runtime` must be live, and `out_advanced` must be writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_tick(
    runtime: *mut DsRuntime,
    dt: f32,
    out_advanced: *mut bool,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() {
            set_last_error("ds_runtime_tick: runtime is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let Some(scene) = runtime.scene.as_mut() else {
            set_last_error("ds_runtime_tick: no document loaded");
            return DsStatus::NoDocument;
        };
        scene.tick(dt, &mut runtime.arena);

        // A commit that renumbered the rect table is the one case where the
        // painter's per-document state goes stale **without** the arena being
        // replaced: the same rect index names a different node afterwards
        // (story #838). The load path reports the arena case; this reports the
        // other, through the same call.
        //
        // The gate is `LiveScene`'s, so this host reads the rule the other two
        // read rather than holding a third copy of it (issue #945). It is the
        // rule, not a fix for a defect reachable today: this ABI names the
        // shown root once, inside the load, and offers no symbol to change it
        // afterwards — so nothing here can raise a renumbering that the load's
        // own `document_replaced` has not already covered. It is here so that
        // the day a root-switching symbol lands, the host is already right
        // rather than quietly wrong.
        if let Some(surface) = runtime.surface.as_mut()
            && scene.take_renumbering(&runtime.arena)
        {
            surface.document_replaced();
        }

        if !out_advanced.is_null() {
            unsafe { *out_advanced = scene.advanced() };
        }
        DsStatus::Ok
    })
}

/// Resizes the surface. `width` and `height` are **device** pixels.
///
/// Android's `surfaceChanged` reports physical pixels, which is what this takes
/// and what `SurfaceRenderer::resize` already guards against the adapter maximum
/// (issue #714).
///
/// # Safety
///
/// `runtime` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_resize(
    runtime: *mut DsRuntime,
    width: u32,
    height: u32,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() {
            set_last_error("ds_runtime_resize: runtime is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let Some(surface) = runtime.surface.as_mut() else {
            set_last_error("ds_runtime_resize: no surface attached");
            return DsStatus::NoSurface;
        };
        match surface.resize(width, height) {
            Ok(()) => DsStatus::Ok,
            Err(error) => {
                set_last_error(format!("{error:?}"));
                DsStatus::Surface
            }
        }
    })
}

/// Draws the committed frame and puts it on the surface.
///
/// `out_drawn` receives whether a frame actually reached the window. It can be
/// false for a reason that is not an error — a zero extent, or a surface that
/// had to be reconfigured — which is why it is separate from the status.
///
/// The sequence mirrors `dashscene-desktop`'s `GpuPresenter::present`, the
/// reference implementation of this seam: the painter packs the committed
/// tables into an instance buffer, then the renderer uploads and presents. The
/// dirty rects go to both, and the generation travels with them to the renderer
/// so a declined frame breaks the chain by arithmetic rather than by anyone
/// remembering to say so.
///
/// The commit is marked shown whenever presenting returns, **not** only when a
/// frame reached the window. That is what `LiveScene::advanced`'s own
/// documentation requires — a present can return without drawing, and "nothing
/// here tries to detect that, and a host should not either". `out_drawn` is
/// still reported, because a host may want it for its own pacing; it must not
/// be used to decide what was shown.
///
/// # Safety
///
/// `runtime` must be live, and `out_drawn` must be writable or null. No other
/// call may be in flight on `runtime`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_draw(
    runtime: *mut DsRuntime,
    out_drawn: *mut bool,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() {
            set_last_error("ds_runtime_draw: runtime is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        if runtime.scene.is_none() {
            set_last_error("ds_runtime_draw: no document loaded");
            return DsStatus::NoDocument;
        }
        let Some(surface) = runtime.surface.as_mut() else {
            set_last_error("ds_runtime_draw: no surface attached");
            return DsStatus::NoSurface;
        };
        let scene = runtime.arena.committed();
        let changes = Changes {
            rects: scene.dirty(),
            generation: scene.generation(),
        };
        runtime.painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            Some(changes.rects),
        );
        let presented = surface.present(
            runtime.painter.instances(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.glyphs(),
            Some(changes),
        );
        match presented {
            Ok(drawn) => {
                if !out_drawn.is_null() {
                    unsafe { *out_drawn = drawn == Drawn::Yes };
                }
                // Unconditionally on `Ok`, not only on `Drawn::Yes`, which is
                // what both reference hosts do and what `LiveScene::advanced`'s
                // own documentation requires: "a present can return `Ok`
                // without drawing — a zero extent, an occluded window, or an
                // acquire that timed out. Nothing here tries to detect that,
                // and a host should not either." Gating on `Drawn::Yes` would
                // leave `advanced()` true on every tick while the window is
                // occluded, so a host that idles on it would never idle.
                if let Some(scene) = runtime.scene.as_mut() {
                    scene.mark_shown();
                }
                DsStatus::Ok
            }
            Err(error) => {
                set_last_error(format!("{error:?}"));
                // **The one place this ABI can classify a surface failure**, and
                // the reason issue #884 was about `ds_runtime_draw` rather than
                // about `DsStatus` generally: `present` fails with a
                // `FrameError`, which carries the rule, and the other two
                // surface failures — `resize` and the attach — fail with a
                // `RendererError`, which does not describe a lost swapchain at
                // all. Reporting a lost surface there would be inventing a
                // classification rather than forwarding one.
                if error.is_recoverable() {
                    DsStatus::SurfaceLost
                } else {
                    DsStatus::Surface
                }
            }
        }
    })
}

/// Copies the last failure's message into `buf` as NUL-terminated UTF-8.
///
/// Returns the number of bytes the message needs **including** the terminator,
/// so a caller passing a null `buf` or a short one learns the size to allocate.
/// Nothing is written when `buf` is null or `cap` is zero.
///
/// The message is diagnostic. Branch on [`DsStatus`], never on this text.
///
/// # Safety
///
/// `buf` must be null, or writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_last_error_message(buf: *mut c_char, cap: usize) -> usize {
    // Guarded like every other entry point. It was not, and Rule 1 in the
    // module documentation claimed it was — the function a host calls to find
    // out what went wrong is the worst one to leave unguarded. Zero is
    // returned on an unwind, which a caller reads as "no message".
    catch_unwind(AssertUnwindSafe(|| {
        LAST_ERROR.with(|slot| {
            let message = slot.borrow();
            let bytes = message.as_bytes();
            let needed = bytes.len() + 1;
            if buf.is_null() || cap == 0 {
                return needed;
            }
            // Truncate to fit, always leaving room for the terminator. A truncated
            // diagnostic is better than a caller having to size a buffer correctly
            // before it can read why sizing failed.
            //
            // **On a character boundary**, not a byte one. These messages are
            // `format!("{error:?}")` over validator reports and `dashbuf`
            // errors, which carry document text, so a cut mid-character would
            // leave a partial multi-byte sequence in a buffer this promises is
            // UTF-8 — and a caller doing a strict decode (Swift's
            // `String(cString:)`, JNI's `NewStringUTF`) gets a failure instead
            // of the error it asked for.
            let mut take = bytes.len().min(cap - 1);
            while take > 0 && !message.is_char_boundary(take) {
                take -= 1;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), take);
                *buf.add(take) = 0;
            }
            needed
        })
    }))
    .unwrap_or(0)
}

/// Hands a platform surface to the painter.
///
/// `kind` says what `window` and `display` are; see [`DsSurfaceKind`]. `width`
/// and `height` are device pixels.
///
/// The conversion for each platform lives behind that platform's `cfg`, which is
/// D3's "each platform contributes a small handle type" — nothing in the painter
/// moves for it. A build that does not have the arm for `kind` returns
/// [`DsStatus::UnsupportedHandle`] rather than failing to compile, so one
/// library serves every host and says which it can.
///
/// # Safety
///
/// `window` must be a valid handle of the kind `kind` names, and must outlive
/// every later call on `runtime` until the surface is replaced or the runtime is
/// freed. On Android that is D4's rule: `surfaceDestroyed` must not return until
/// rendering has stopped.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_attach_surface(
    runtime: *mut DsRuntime,
    kind: i32,
    window: *mut std::ffi::c_void,
    _display: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() || window.is_null() {
            set_last_error("ds_runtime_attach_surface: runtime or window is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        // `kind` is an `i32` and not a `DsSurfaceKind`, deliberately. Binding a
        // C integer to a `#[repr(i32)]` enum parameter *constructs* that enum,
        // and a bit pattern with no declared discriminant is undefined
        // behaviour the instant the call binds its arguments — before `guard`
        // runs, so `catch_unwind` could never see it. An uninitialised local on
        // the C side, or a header newer than the library once a second variant
        // exists, is enough to produce one. Validating an integer is the only
        // shape that stays sound.
        match kind {
            kind if kind == DsSurfaceKind::AndroidNdk as i32 => {
                attach_android(runtime, window, width, height)
            }
            other => {
                set_last_error(format!("ds_runtime_attach_surface: unknown kind {other}"));
                DsStatus::UnsupportedHandle
            }
        }
    })
}

/// Drops the surface, and reports whether there was one.
///
/// **The other half of D4's destroy handshake, and the reason it exists.** On
/// Android `surfaceDestroyed` must not return until rendering has stopped and
/// the `wgpu::Surface` built from that window has been dropped — otherwise the
/// window is freed underneath a live surface, which is use-after-free on
/// rotation, backgrounding and split-screen. A host cannot honour that with
/// [`ds_runtime_free`] alone: freeing the whole runtime would drop the document
/// and the arena with it, and the surface comes and goes many times over one
/// document's life.
///
/// After this returns the runtime keeps its document and its scene, so a later
/// [`ds_runtime_attach_surface`] resumes the same picture on a new window. The
/// painter is kept too — it holds packing buffers and knows nothing about the
/// window.
///
/// **The first frame after re-attaching must be drawn whatever the tick says.**
/// The scene did not change while the surface was gone, so
/// `out_advanced` from [`ds_runtime_tick`] will be false, and the new device has
/// drawn nothing — a host that only draws when the tick advanced would show an
/// empty window until something else moved the scene. That obligation is the
/// host's rather than this ABI's, for the reason the gate is the host's
/// everywhere else: `dashscene-desktop` calls it a forced redraw and
/// `dashscene-web` carries the same flag.
///
/// `out_had_surface` receives whether a surface was attached. Detaching twice
/// is not an error: a host tearing down on a path it cannot fully predict
/// should be able to ask for this unconditionally.
///
/// Adding this symbol did not move [`DS_ABI_VERSION`]: by the rule in the
/// module documentation, a new symbol does not change it.
///
/// # Safety
///
/// `runtime` must be live, and `out_had_surface` must be writable or null. **No
/// other call may be in flight on `runtime`** — that is what the caller's own
/// handshake is for, and this function cannot check it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_detach_surface(
    runtime: *mut DsRuntime,
    out_had_surface: *mut bool,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() {
            set_last_error("ds_runtime_detach_surface: runtime is null");
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        // `take` and drop. The drop is the point of the call: it is what
        // releases the `wgpu::Surface` holding the `ANativeWindow`, and it
        // happens here rather than at some later cleanup precisely so the
        // caller can order it before releasing the window.
        let had_surface = runtime.surface.take().is_some();
        if !out_had_surface.is_null() {
            unsafe { *out_had_surface = had_surface };
        }
        DsStatus::Ok
    })
}

/// The Android arm of [`ds_runtime_attach_surface`].
#[cfg(target_os = "android")]
fn attach_android(
    runtime: &mut DsRuntime,
    window: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> DsStatus {
    let Some(window) = std::ptr::NonNull::new(window) else {
        set_last_error("ds_runtime_attach_surface: window is null");
        return DsStatus::NullArgument;
    };
    // The handle wrapper lives in the painter, beside `for_canvas`, so this
    // crate names no `wgpu` type — the property `crates/dashscene-gpu/Cargo.toml`
    // records for the canvas case. SAFETY: the caller of
    // `ds_runtime_attach_surface` promises the `ANativeWindow *` outlives every
    // later call on this runtime, which is exactly what `for_android_ndk` asks.
    // Dropped **before** the replacement is attempted, and this ordering is the
    // contract rather than tidiness. A host attaching window B over window A
    // reads a returned call as the replacement point and releases A; leaving
    // A's surface live on failure would then have the next draw present into a
    // freed `ANativeWindow` and return `DsStatus::Ok`, and
    // `ds_runtime_detach_surface` report a surface the host believes is gone.
    // It is also what the `# Safety` note means by "until the surface is
    // replaced": after this call there is no old surface, whatever the status.
    runtime.surface = None;
    match unsafe { SurfaceRenderer::for_android_ndk(window, width, height) } {
        Ok(surface) => {
            runtime.surface = Some(surface);
            DsStatus::Ok
        }
        Err(error) => {
            set_last_error(format!("{error:?}"));
            DsStatus::Surface
        }
    }
}

/// The non-Android stand-in for [`attach_android`].
///
/// Desktop and web reach the painter through `dashscene-desktop` and
/// `dashscene-web`, which own their own handoffs, so this ABI has no arm for
/// them and says so rather than pretending.
#[cfg(not(target_os = "android"))]
fn attach_android(
    _runtime: &mut DsRuntime,
    _window: *mut std::ffi::c_void,
    _width: u32,
    _height: u32,
) -> DsStatus {
    set_last_error("ds_runtime_attach_surface: AndroidNdk handles need an Android build");
    DsStatus::UnsupportedHandle
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live runtime behind an opaque parameter, for
    /// `rust/access-invalid-pointer` (issue #979).
    ///
    /// **This exists for the query and not for the code.** The dereference is
    /// sound either way: `ds_runtime_new` writes `Box::into_raw`, the caller
    /// asserts it answered [`DsStatus::Ok`] and wrote a non-null handle, and
    /// nothing frees it before the read. What CodeQL cannot model is the FFI
    /// out-pointer write — the local starts as `std::ptr::null_mut()` and is
    /// passed by `&mut` into an `extern "C"` function, so the query watches it
    /// get initialised to null and never learns what came back.
    ///
    /// Alerts 4 and 5 were dismissed as false positives on PR #978 and PR
    /// #1054. Issue #979 predicted a third, on the grounds that the alert is a
    /// property of the **shape** rather than of any one test, and named the
    /// shape that would close it: the `&mut *runtime` dereferences in the entry
    /// points are **not** flagged, because there the pointer is an opaque
    /// parameter behind an early return rather than a local the query watched.
    /// (That issue counted seven of them and there are eight — it was written
    /// before `ds_runtime_load_document_mapped` landed. Re-derive rather than
    /// trusting either number.)
    /// This is that shape, and it is the thing that issue said to try first.
    ///
    /// **It does not clear the alert, and that is measured rather than
    /// predicted.** CodeQL re-ran on the pull request that added this and
    /// raised alert 10 — `rust/access-invalid-pointer`, on the dereference
    /// *inside* this function. So the query follows the local across a call in
    /// the same file, and what protects the eight `&mut *runtime` dereferences
    /// in the entry points is not "the pointer is a parameter" but that it
    /// arrives from **outside the analysed body** — a C caller's, which the
    /// query cannot see initialised at all.
    ///
    /// The function is kept anyway: it moves one dismissal to one site instead
    /// of one per test, which is the practical half of what issue #979 asked
    /// for. That issue stays open for the remaining half, with this measurement
    /// on it, and its third option — that a dismissal is the right permanent
    /// answer here — is now the live one.
    ///
    /// It weakens no assertion: both callers keep their status checks against
    /// the real exported symbols, and what moves behind this function is only
    /// the read.
    ///
    /// **The borrow is bounded by a caller-chosen lifetime, not `'static`.**
    /// An unbounded one compiles `let a = &live(r).arena; ds_runtime_free(r);
    /// a.roots();` in silence, and both call sites read up to the statement
    /// before their `ds_runtime_free`. A generic parameter costs nothing and
    /// takes the constraint back.
    ///
    /// # Safety
    ///
    /// `runtime` is non-null and points at a live `DsRuntime` that outlives
    /// `'a`.
    unsafe fn live<'a>(runtime: *mut DsRuntime) -> &'a DsRuntime {
        assert!(!runtime.is_null(), "a live runtime is never null here");
        // SAFETY: the caller promises a live, non-null handle that outlives
        // `'a`.
        unsafe { &*runtime }
    }

    /// **Every `extern "C"` entry point runs inside `catch_unwind`, and
    /// `ds_abi_version` is the one exception** — rule 1 of
    /// `docs/design/c-abi.md`, asserted rather than asserted about (issue
    /// #1190).
    ///
    /// # Why prose was not enough
    ///
    /// That rule has been wrong in each direction already. `ds_last_error_message`
    /// was unguarded while the rule claimed it was guarded, which its own
    /// comment records; story #843 then read "nine calls to `guard`" as "three
    /// unguarded" and wrote that into four documents. Neither is a subtle
    /// mistake and neither was caught, because nothing read the code.
    ///
    /// # Why over the source text
    ///
    /// A panic that crosses `extern "C"` is undefined behaviour, so the
    /// property cannot be exercised by calling the entry points and panicking
    /// inside them — the test for a missing guard is the thing the guard
    /// exists to prevent. What can be checked is that each body reaches
    /// `guard`, and that is a fact about the text.
    ///
    /// `include_str!` on this crate's own source is the same technique
    /// `dashscene-android`'s `entry` module uses to compare its JNI names
    /// against `demo-android`'s, for the same reason.
    ///
    /// # What it does not check
    ///
    /// - That the guard **wraps the whole body**. A `guard` call after an
    ///   unguarded dereference would pass. What it catches is the omission that
    ///   has actually happened twice, which is an entry point with no guard at
    ///   all.
    /// - An entry point exported under a name that is not `ds_`-prefixed. Every
    ///   one is, and `dashscene.h` is where that convention is stated; a
    ///   symbol breaking it is invisible here.
    ///
    /// It **does** check the two holes a first cut left. A `guard(` inside a
    /// comment no longer satisfies it, because comment lines are stripped
    /// before the body is searched — an entry point saying "not wrapped in
    /// `guard(` because this cannot panic" would otherwise pass with no guard,
    /// which is `ds_last_error_message`'s original failure exactly. And the
    /// signature must be unindented, so an `extern "C" fn ds_*` nested in a
    /// `mod` cannot borrow a sibling's `guard` call by running its "body" to
    /// the enclosing module's brace.
    #[test]
    fn every_entry_point_but_the_version_is_guarded() {
        /// The one entry point that is deliberately not guarded: it reads a
        /// constant and cannot panic, and a host calls it before it has a
        /// runtime to report an error through.
        const UNGUARDED: &str = "ds_abi_version";

        let source = include_str!("lib.rs");
        let mut entry_points = Vec::new();
        let mut unguarded = Vec::new();

        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            // A top-level entry point: unindented, and the signature line
            // carries both `extern "C"` and the name. `#[unsafe(no_mangle)]`
            // and the doc comment sit above it and are not read.
            // Unindented, so a nested item cannot take the enclosing module's
            // closing brace as its own and sweep a sibling's `guard` call into
            // its body.
            if line.starts_with([' ', '\t', '#']) || !line.contains("extern \"C\" fn ds_") {
                continue;
            }
            let name = line
                .split("extern \"C\" fn ")
                .nth(1)
                .and_then(|rest| rest.split(['(', '<']).next())
                .expect("a name follows `extern \"C\" fn`");
            entry_points.push(name);

            // The body: to the first unindented `}`, which is where rustfmt
            // puts a top-level item's closing brace.
            let body: String = lines[i + 1..]
                .iter()
                .take_while(|line| !line.starts_with('}'))
                // Comments stripped, so a body that only *mentions* `guard(`
                // does not satisfy the search. `//` inside a string literal
                // would strip too much and is worth nothing here: over-stripping
                // can only report a guarded entry point as unguarded, which
                // fails loudly rather than passing quietly.
                .map(|line| line.split("//").next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n");
            if !body.contains("guard(") && !body.contains("catch_unwind") {
                unguarded.push(name);
            }
        }

        // Guards the matcher, not the property. A signature style this stopped
        // recognising would find no entry points and report none unguarded,
        // which is the failure this test exists to be immune to.
        //
        // Against the header rather than a number, so it cannot go stale the
        // way a `>= 12` would: a floor equal to today's count passes a matcher
        // that finds twelve of thirteen, and the missed one is then reported as
        // neither an entry point nor unguarded — which is exactly the shape of
        // the miscount this test was written to end.
        // The distinct `ds_*(` names the header declares. Distinct rather than
        // a line count, because a name appears in prose above its declaration
        // as often as in it.
        let mut declared: Vec<&str> = include_str!("../include/dashscene.h")
            .split("ds_")
            .skip(1)
            .filter_map(|rest| {
                let name = rest.split('(').next()?;
                (rest.len() > name.len()
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
                .then_some(name)
            })
            .collect();
        declared.sort_unstable();
        declared.dedup();
        assert!(
            entry_points.len() >= declared.len(),
            "the committed header declares {} `ds_` functions and this test found {} \
             `extern \"C\"` items, so its matcher is missing some rather than the ABI having \
             shrunk. Header: {declared:?}. Found: {entry_points:?}",
            declared.len(),
            entry_points.len()
        );
        assert!(
            entry_points.contains(&UNGUARDED),
            "{UNGUARDED} was not found at all, so the exception below cannot be the one this rule \
             names"
        );

        assert_eq!(
            unguarded,
            vec![UNGUARDED],
            "rule 1 of `docs/design/c-abi.md` says every entry point runs inside `catch_unwind` \
             with {UNGUARDED} as the single exception. These do not reach `guard`: {unguarded:?}. \
             A panic crossing `extern \"C\"` is undefined behaviour, so an entry point that can \
             panic needs the guard; one that provably cannot needs this test's exception list \
             widened and the record amended with it."
        );
    }

    /// The version is readable without a runtime, which is the property a host
    /// depends on when it decides whether to go on.
    #[test]
    fn the_version_is_reachable_before_anything_else() {
        assert_eq!(ds_abi_version(), DS_ABI_VERSION);
    }

    #[test]
    fn a_runtime_round_trips() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert!(!runtime.is_null());
        unsafe { ds_runtime_free(runtime) };
    }

    #[test]
    fn a_null_out_pointer_is_a_status_and_not_a_dereference() {
        assert_eq!(
            unsafe { ds_runtime_new(std::ptr::null_mut()) },
            DsStatus::NullArgument
        );
    }

    #[test]
    fn freeing_null_is_allowed() {
        unsafe { ds_runtime_free(std::ptr::null_mut()) };
    }

    /// The tick needs a document, and says so rather than ticking nothing.
    #[test]
    fn ticking_without_a_document_reports_no_document() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        let mut advanced = true;
        assert_eq!(
            unsafe { ds_runtime_tick(runtime, 0.016, &mut advanced) },
            DsStatus::NoDocument
        );
        unsafe { ds_runtime_free(runtime) };
    }

    #[test]
    fn resizing_without_a_surface_reports_no_surface() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe { ds_runtime_resize(runtime, 100, 100) },
            DsStatus::NoSurface
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// Detaching reports whether there was anything to detach, and detaching
    /// twice is not an error.
    ///
    /// A host tearing down on a path it cannot fully predict — `surfaceDestroyed`
    /// after a failed `surfaceCreated`, say — has to be able to ask
    /// unconditionally. `out_had_surface` is how it finds out, rather than a
    /// status it would have to treat as benign.
    #[test]
    fn detaching_without_a_surface_is_allowed_and_says_there_was_none() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);

        let mut had = true;
        assert_eq!(
            unsafe { ds_runtime_detach_surface(runtime, &mut had) },
            DsStatus::Ok
        );
        assert!(
            !had,
            "there was no surface, and the call said there was one"
        );

        // Twice, because the handshake is allowed to be conservative.
        assert_eq!(
            unsafe { ds_runtime_detach_surface(runtime, std::ptr::null_mut()) },
            DsStatus::Ok
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// A null runtime is a status rather than a dereference, like every other
    /// entry point.
    #[test]
    fn detaching_a_null_runtime_is_a_status() {
        assert_eq!(
            unsafe { ds_runtime_detach_surface(std::ptr::null_mut(), std::ptr::null_mut()) },
            DsStatus::NullArgument
        );
    }

    /// Detaching leaves the runtime usable: the document and the scene survive,
    /// so re-attaching resumes the same picture rather than needing a reload.
    ///
    /// Asserted through the tick, which is the call that needs a scene: a
    /// detach that dropped it would answer `NoDocument` here.
    #[test]
    fn detaching_keeps_the_document() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        // No document loaded, so the tick's answer is `NoDocument` before and
        // after. The point is that detaching does not change it to something
        // else — a detach that reset the runtime would be caught by the pair.
        let before = unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) };
        assert_eq!(
            unsafe { ds_runtime_detach_surface(runtime, std::ptr::null_mut()) },
            DsStatus::Ok
        );
        let after = unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) };
        assert_eq!(before, after);
        unsafe { ds_runtime_free(runtime) };
    }

    /// Bytes that are not a document fail as a status, not as a panic — which is
    /// the rule the whole boundary exists to keep.
    #[test]
    fn junk_bytes_are_an_open_failure_and_not_a_panic() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        let junk = [0_u8; 32];
        let status = unsafe { ds_runtime_load_document(runtime, junk.as_ptr(), junk.len()) };
        assert_eq!(status, DsStatus::Open);
        unsafe { ds_runtime_free(runtime) };
    }

    /// The committed table's row count, which is the loaded document's and no
    /// one else's — an arena replaced by a load that was refused reads 0.
    ///
    /// **Takes the handle and checks it here**, where
    /// `a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text`'s
    /// `measured` takes a reference and is checked at its one call site. Both
    /// answer CodeQL's `rust/access-invalid-pointer`, and this shape is the one
    /// that survives being called more than once: with the check at the call
    /// site, a second dereference further down the test — past a load, past a
    /// tick, or inside a loop — is out of reach of it again, which is what the
    /// rule reported against the first cut of these two tests.
    fn committed_rows(runtime: *const DsRuntime) -> usize {
        // `as_ref` rather than `&*` behind an `assert!`: it is the conversion
        // that carries the null check in its type, and CodeQL's
        // `rust/access-invalid-pointer` reads the raw dereference as
        // unguarded whatever assertion precedes it — which is what it reported
        // against the first two cuts of this helper.
        //
        // SAFETY: `ds_runtime_new` answered Ok where this handle was made, and
        // nothing has freed it yet; null is the one other case and `as_ref`
        // answers `None` for it.
        let runtime = unsafe { runtime.as_ref() }.expect("the handle under test is live");
        runtime.arena.committed().rects().len()
    }

    /// A document that opens and then fails the gate: one root node naming a
    /// paint entry the document does not carry.
    ///
    /// The other refusal a byte-taking load can make. Junk bytes never reach
    /// `validate_document` at all, so without this the pair below would cover
    /// one of the two arms and claim both.
    fn gate_failing_document() -> Vec<u8> {
        use dashbuf::{Document as Doc, DocumentArgs, NO_PARENT, Node, NodeArgs};
        use flatbuffers::FlatBufferBuilder;

        let mut builder = FlatBufferBuilder::new();
        let nodes = vec![Node::create(
            &mut builder,
            &NodeArgs {
                parent: NO_PARENT,
                // The document declares no paints, so this resolves to
                // nothing and `check_node_links` reports it as
                // `paint.entry-out-of-range`.
                paint_entry: 3,
                ..Default::default()
            },
        )];
        let nodes = builder.create_vector(&nodes);
        let document = Doc::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                ..Default::default()
            },
        );
        builder.finish(document, None);

        let bank = dashbuf::bank::ColdBank::raw(std::iter::empty());
        dashbuf::bank::assemble(builder.finished_data(), &bank).expect("the fixture assembles")
    }

    /// A **refused** byte-taking load leaves the document already loaded still
    /// drawable — the pair to
    /// [`a_refused_mapped_load_leaves_the_loaded_document_drawable`], over
    /// `load_into`, which is what both byte-taking entry points run.
    ///
    /// It drives it through [`ds_runtime_load_document`] alone.
    /// [`ds_runtime_load_document_with_text`]'s own refusals are raised in
    /// `text_from_c` **above** `load_into`, so a cascade rejected there has not
    /// reached the document and there is nothing for this property to be about.
    ///
    /// It guards the ordering issue #1183's fix depends on rather than the fix
    /// itself. `drop_document` sits **below** the open and the gate; moving it
    /// above either would discard a good document on a load that was refused,
    /// and nothing else in this file would notice. Both refusal arms are here
    /// for that reason: `Open`, which is raised before the document is even
    /// read, and `Gate`, which is raised after it.
    ///
    /// **What is asserted is the committed table, not that a scene exists.** A
    /// tick answering `Ok` says only that `runtime.scene` is `Some`, which is
    /// still true of the state this whole fix is about — the previous
    /// document's scene beside a fresh arena — so a test resting on the status
    /// alone would pass over an arena replaced above the gate. The row count is
    /// a property of the document that was loaded, and a replaced arena reads 0.
    ///
    /// The **panic** path the fix exists for is not covered, and cannot be from
    /// here — see the mapped pair's own note for why.
    #[test]
    fn a_refused_byte_load_leaves_the_loaded_document_drawable() {
        let document = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../goldens/dsb/v07-text-hug-in-fill.dsb"
        ))
        .expect("the committed fixture is present");

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe { ds_runtime_load_document(runtime, document.as_ptr(), document.len()) },
            DsStatus::Ok
        );
        assert_eq!(
            unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) },
            DsStatus::Ok,
            "the first load left a scene to tick"
        );
        let loaded = committed_rows(runtime);
        assert!(loaded > 0, "the fixture commits rects to compare against");

        let junk = [0_u8; 32];
        let refused = gate_failing_document();
        for (bytes, expected) in [
            (junk.as_slice(), DsStatus::Open),
            (refused.as_slice(), DsStatus::Gate),
        ] {
            assert_eq!(
                unsafe { ds_runtime_load_document(runtime, bytes.as_ptr(), bytes.len()) },
                expected
            );
            assert_eq!(
                unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) },
                DsStatus::Ok,
                "a load refused as {expected:?} must leave the previously loaded document \
                 drawable, not discard it"
            );
            assert_eq!(
                committed_rows(runtime),
                loaded,
                "and drawable means the document that was loaded, not whatever a refused load \
                 left behind: {expected:?}"
            );
        }
        unsafe { ds_runtime_free(runtime) };
    }

    /// The size query works before the buffer exists, so a caller can size one.
    #[test]
    fn the_error_message_reports_the_size_it_needs() {
        set_last_error("twelve chars");
        let needed = unsafe { ds_last_error_message(std::ptr::null_mut(), 0) };
        assert_eq!(needed, "twelve chars".len() + 1);
    }

    /// A short buffer truncates and still terminates, rather than overrunning.
    #[test]
    fn a_short_error_buffer_truncates_and_terminates() {
        set_last_error("twelve chars");
        // `[c_char; 5]`, not `[i8; 5]`: `c_char` is `i8` on this machine and
        // **`u8` on Android**, so the literal spelling compiled here and
        // nowhere a platform host runs. Nothing caught that until `just
        // android-lint` existed (issue #1086) — `just android` builds the lib
        // and not its tests, so this target was compiled by no gate at all.
        let mut buf: [c_char; 5] = [0; 5];
        let needed = unsafe { ds_last_error_message(buf.as_mut_ptr(), buf.len()) };
        assert_eq!(needed, "twelve chars".len() + 1);
        assert_eq!(buf[4], 0, "the terminator is written inside the buffer");
        // Read back as a C string rather than casting each element. `c_char` is
        // `i8` here and `u8` on Android, so a per-byte `as u8` is a real
        // conversion on one target and a no-op clippy refuses on the other —
        // there is no spelling of that cast which passes both. This is also
        // nearer the contract: what the ABI promises is a NUL-terminated
        // string, not a byte array.
        //
        // SAFETY: the call above wrote into `buf`, and the assertion above says
        // the terminator is inside it.
        let text = unsafe { CStr::from_ptr(buf.as_ptr()) };
        assert_eq!(text.to_bytes(), b"twel");
    }

    /// On a host build the Android arm declines rather than failing to compile,
    /// which is what lets one library serve every host.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn an_android_handle_on_a_host_build_is_declined() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        let mut fake = 0_u8;
        let status = unsafe {
            ds_runtime_attach_surface(
                runtime,
                DsSurfaceKind::AndroidNdk as i32,
                std::ptr::from_mut(&mut fake).cast(),
                std::ptr::null_mut(),
                64,
                64,
            )
        };
        assert_eq!(status, DsStatus::UnsupportedHandle);
        unsafe { ds_runtime_free(runtime) };
    }

    /// An unknown tag is rejected rather than merely unmatched.
    ///
    /// The reason `kind` is an `i32` and not a `DsSurfaceKind`: binding an
    /// out-of-range value to a `#[repr(i32)]` enum parameter is undefined
    /// behaviour at the call boundary, before any handler could run. This is
    /// the test that fails if someone "tidies" the signature back to the enum.
    #[test]
    fn an_unknown_surface_kind_is_rejected() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        let mut fake = 0_u8;
        let status = unsafe {
            ds_runtime_attach_surface(
                runtime,
                9999,
                std::ptr::from_mut(&mut fake).cast(),
                std::ptr::null_mut(),
                64,
                64,
            )
        };
        assert_eq!(status, DsStatus::UnsupportedHandle);
        unsafe { ds_runtime_free(runtime) };
    }

    /// **A loaded document draws its text**, which is the story's whole
    /// deliverable, asserted on the **committed** tables rather than on the
    /// arena calls — the painter reads `committed()`, and a test that asserted
    /// the document would pass while the feature rendered nothing.
    ///
    /// The `None` half beside it is the pre-#947 picture, and is what says the
    /// fonts are the cause rather than the document.
    /// `docs/decisions/font-resolution-order.md` records this same fixture
    /// measuring four rects, zero glyph runs, and its text node at 0 x 0.
    #[test]
    fn a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text() {
        let document = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../goldens/dsb/v07-text-hug-in-fill.dsb"
        ))
        .expect("the committed text fixture is present");
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.png"
        ))
        .expect("the committed sheet is present");
        let metrics = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.metrics"
        ))
        .expect("the committed metrics are present");

        // Held in a local: the pointer must outlive the call.
        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let face = DsFontFace {
            family: family.as_ptr(),
            weight: 400,
            face_index: 0,
            font_bytes: font.as_ptr(),
            font_len: font.len(),
            atlas_png: png.as_ptr(),
            atlas_png_len: png.len(),
            atlas_metrics: metrics.as_ptr(),
            atlas_metrics_len: metrics.len(),
        };

        /// Glyph runs, and the text node's resolved size.
        ///
        /// Takes a reference rather than the handle, so the one dereference
        /// sits at the call site beside the check that earns it. Passing the
        /// raw pointer in and dereferencing it here put the deref out of
        /// reach of the null check, which is what CodeQL's
        /// `rust/access-invalid-pointer` reported against this test.
        fn measured(runtime: &DsRuntime) -> (usize, f32, f32) {
            let scene = runtime.arena.committed();
            let row = (0..scene.rects().len() as u32)
                .find(|&row| runtime.arena.text(scene.node_of(row)).is_some())
                .expect("the fixture carries a text node");
            let rect = scene.rects()[row as usize];
            (scene.glyphs().runs().len(), rect.w, rect.h)
        }

        let load = |faces: *const DsFontFace, count: usize| {
            let mut runtime = std::ptr::null_mut();
            assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
            assert!(
                !runtime.is_null(),
                "ds_runtime_new answered Ok, so it wrote a live handle"
            );
            assert_eq!(
                unsafe {
                    ds_runtime_load_document_with_text(
                        runtime,
                        document.as_ptr(),
                        document.len(),
                        faces,
                        count,
                    )
                },
                DsStatus::Ok
            );
            // SAFETY: `ds_runtime_new` answered Ok and wrote a non-null handle,
            // asserted above, and nothing has freed it yet. Through `live` for
            // the reason that function gives (issue #979).
            let out = measured(unsafe { live(runtime) });
            unsafe { ds_runtime_free(runtime) };
            out
        };

        let (runs, width, height) = load(&face, 1);
        assert!(
            runs > 0,
            "the host supplied a face and its sheet, so the document's text must reach the \
             painter as glyph runs"
        );
        assert!(
            width > 1.0 && height > 1.0,
            "and the hug-sized text node must measure to its shaped size rather than \
             collapse: {width} x {height}"
        );
        assert_eq!(
            load(std::ptr::null(), 0),
            (0, 0.0, 0.0),
            "and without them it is the pre-#947 picture — no glyphs, and a text node that \
             makes its siblings lay out around a box the design did not specify"
        );
    }

    /// A null face array with a non-zero count is a status, not a dereference.
    #[test]
    fn a_null_face_array_with_a_count_is_a_status() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        let junk = [0_u8; 32];
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    junk.as_ptr(),
                    junk.len(),
                    std::ptr::null(),
                    3,
                )
            },
            DsStatus::NullArgument
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// Bytes that are not a face are `FontFace` — not a panic, and not `Open`.
    /// The faces are validated before the document is opened, which is what
    /// makes junk document bytes safe to use here.
    #[test]
    fn junk_font_bytes_are_a_font_face_status() {
        let family = std::ffi::CString::new("Junk").expect("no interior nul");
        let not_a_font = [0_u8; 64];
        let face = DsFontFace {
            family: family.as_ptr(),
            weight: 400,
            face_index: 0,
            font_bytes: not_a_font.as_ptr(),
            font_len: not_a_font.len(),
            atlas_png: std::ptr::null(),
            atlas_png_len: 0,
            atlas_metrics: std::ptr::null(),
            atlas_metrics_len: 0,
        };
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    &face,
                    1,
                )
            },
            DsStatus::FontFace
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// A weight outside the CSS range is `FontFace`, refused rather than
    /// repaired.
    ///
    /// 0 is the value an uninitialised descriptor carries, and a face
    /// declared at it resolves against every request as if the host had meant
    /// it.
    ///
    /// **The range is decided one layer down since issue #1206**, in
    /// `dashscene_engine::TextResources::from_faces`, and this test asserts the
    /// status a C caller sees rather than where it is produced. That is the
    /// point of the move: `from_faces` is what every route reaches, and it
    /// accepted the descriptor this ABI refused until the check moved. Two
    /// earlier attempts to hold the rule elsewhere are why it is stated
    /// this way — the JNI half in `dashscene-android` clamped it as well until
    /// story #947's review, which gave one question two answers.
    ///
    /// The font bytes are real, so nothing but the weight can be what is
    /// refused.
    #[test]
    fn a_weight_outside_the_css_range_is_a_font_face_status() {
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let not_a_document = [0_u8; 32];

        for weight in [0, 1001, u16::MAX] {
            let face = DsFontFace {
                family: family.as_ptr(),
                weight,
                face_index: 0,
                font_bytes: font.as_ptr(),
                font_len: font.len(),
                atlas_png: std::ptr::null(),
                atlas_png_len: 0,
                atlas_metrics: std::ptr::null(),
                atlas_metrics_len: 0,
            };
            let mut runtime = std::ptr::null_mut();
            assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
            assert_eq!(
                unsafe {
                    ds_runtime_load_document_with_text(
                        runtime,
                        not_a_document.as_ptr(),
                        not_a_document.len(),
                        &face,
                        1,
                    )
                },
                DsStatus::FontFace,
                "weight {weight} is outside 1..=1000, so the load is refused as FontFace"
            );
            unsafe { ds_runtime_free(runtime) };
        }

        // The ends of the range are accepted, so the check is a range and not
        // a rejection of everything unusual. The document is still junk, so
        // the status that proves the faces passed is `Open`.
        for weight in [1, 400, 1000] {
            let face = DsFontFace {
                family: family.as_ptr(),
                weight,
                face_index: 0,
                font_bytes: font.as_ptr(),
                font_len: font.len(),
                atlas_png: std::ptr::null(),
                atlas_png_len: 0,
                atlas_metrics: std::ptr::null(),
                atlas_metrics_len: 0,
            };
            let mut runtime = std::ptr::null_mut();
            assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
            assert_eq!(
                unsafe {
                    ds_runtime_load_document_with_text(
                        runtime,
                        not_a_document.as_ptr(),
                        not_a_document.len(),
                        &face,
                        1,
                    )
                },
                DsStatus::Open,
                "weight {weight} is inside 1..=1000, so the faces assemble and the junk \
                 document is what fails"
            );
            unsafe { ds_runtime_free(runtime) };
        }
    }

    /// The message a host reads for a cascade failure is **prose**.
    ///
    /// `TextResourcesError` had no `Display` until story #947's review, so
    /// this string was nested `Debug` — `Font { index: 1, message:
    /// "FontParse(\"...\")" }`, escaped quotes and all — where every other
    /// message on this path is a sentence. Asserted on the punctuation that
    /// only `Debug` produces, so a caller reverting to `{:?}` fails here.
    #[test]
    fn a_cascade_failure_reports_a_sentence_rather_than_debug() {
        let family = std::ffi::CString::new("Junk").expect("no interior nul");
        let not_a_font = [0_u8; 64];
        let face = DsFontFace {
            family: family.as_ptr(),
            weight: 400,
            face_index: 0,
            font_bytes: not_a_font.as_ptr(),
            font_len: not_a_font.len(),
            atlas_png: std::ptr::null(),
            atlas_png_len: 0,
            atlas_metrics: std::ptr::null(),
            atlas_metrics_len: 0,
        };
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    &face,
                    1,
                )
            },
            DsStatus::FontFace
        );
        unsafe { ds_runtime_free(runtime) };

        let message = LAST_ERROR.with(|slot| slot.borrow().clone());
        assert!(
            message.starts_with("face 0: "),
            "the message names the descriptor it came from: {message}"
        );
        assert!(
            !message.contains('{') && !message.contains('\\'),
            "a Rust struct literal and an escaped quote are what `{{:?}}` produces, and \
             neither belongs in a host-facing message: {message}"
        );
    }

    /// A mixed set — some faces carrying a sheet and some not — is `Atlas`.
    /// The atlas list is indexed by font slot, so a short list would resolve
    /// past its end; refusing the set is what keeps that indexing sound.
    #[test]
    fn a_mixed_set_of_faces_is_an_atlas_status() {
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.png"
        ))
        .expect("the committed sheet is present");
        let metrics = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.metrics"
        ))
        .expect("the committed metrics are present");

        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let faces = [
            DsFontFace {
                family: family.as_ptr(),
                weight: 400,
                face_index: 0,
                font_bytes: font.as_ptr(),
                font_len: font.len(),
                atlas_png: png.as_ptr(),
                atlas_png_len: png.len(),
                atlas_metrics: metrics.as_ptr(),
                atlas_metrics_len: metrics.len(),
            },
            DsFontFace {
                family: family.as_ptr(),
                weight: 700,
                face_index: 0,
                font_bytes: font.as_ptr(),
                font_len: font.len(),
                atlas_png: std::ptr::null(),
                atlas_png_len: 0,
                atlas_metrics: std::ptr::null(),
                atlas_metrics_len: 0,
            },
        ];
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    faces.as_ptr(),
                    faces.len(),
                )
            },
            DsStatus::Atlas
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// Atlas metrics that do not decode are `Atlas`, not a panic and not `Ok`.
    #[test]
    fn atlas_metrics_that_do_not_decode_are_an_atlas_status() {
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.png"
        ))
        .expect("the committed sheet is present");
        let not_metrics = [0xff_u8; 32];

        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let face = DsFontFace {
            family: family.as_ptr(),
            weight: 400,
            face_index: 0,
            font_bytes: font.as_ptr(),
            font_len: font.len(),
            atlas_png: png.as_ptr(),
            atlas_png_len: png.len(),
            atlas_metrics: not_metrics.as_ptr(),
            atlas_metrics_len: not_metrics.len(),
        };
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    &face,
                    1,
                )
            },
            DsStatus::Atlas
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// **A descriptor wrong in two ways reports the atlas, not the weight**,
    /// and that is a change issue #1206 made rather than an accident.
    ///
    /// The CSS range moved to `TextResources::from_faces`, which runs after
    /// `faces_from_c` has walked every descriptor — so a face carrying both
    /// weight 0 and a half-null atlas pair answered `DsStatus::FontFace`
    /// before and answers `DsStatus::Atlas` now.
    ///
    /// Pinned rather than repaired. Reporting the first fault found is
    /// arbitrary whichever order it is in, and repairing it would mean a second
    /// copy of the range predicate at the ABI — the drift #1206 exists to
    /// remove. What is not acceptable is the order being unrecorded, which is
    /// how a host branching on the status learns it changed from a support
    /// thread.
    #[test]
    fn a_face_wrong_in_two_ways_reports_the_atlas() {
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.png"
        ))
        .expect("the committed sheet is present");

        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let face = DsFontFace {
            family: family.as_ptr(),
            // Outside 1..=1000, which `from_faces` refuses.
            weight: 0,
            face_index: 0,
            font_bytes: font.as_ptr(),
            font_len: font.len(),
            // And a half-described atlas, which `faces_from_c` refuses first.
            atlas_png: png.as_ptr(),
            atlas_png_len: png.len(),
            atlas_metrics: std::ptr::null(),
            atlas_metrics_len: 0,
        };
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    &face,
                    1,
                )
            },
            DsStatus::Atlas,
            "the atlas pair is judged in `faces_from_c` and the weight in `from_faces` after it"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// Exactly one atlas pointer set is `Atlas`, not a silent fall back to
    /// measure-only. Only both null is the legitimate measure-only cascade.
    #[test]
    fn a_face_with_only_one_atlas_pointer_set_is_an_atlas_status() {
        let font = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/fonts/inter/Inter-Regular.otf"
        ))
        .expect("the corpus font is present");
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii/atlas.png"
        ))
        .expect("the committed sheet is present");

        let family = std::ffi::CString::new("Inter").expect("no interior nul");
        let face = DsFontFace {
            family: family.as_ptr(),
            weight: 400,
            face_index: 0,
            font_bytes: font.as_ptr(),
            font_len: font.len(),
            atlas_png: png.as_ptr(),
            atlas_png_len: png.len(),
            atlas_metrics: std::ptr::null(),
            atlas_metrics_len: 0,
        };
        let not_a_document = [0_u8; 32];
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_with_text(
                    runtime,
                    not_a_document.as_ptr(),
                    not_a_document.len(),
                    &face,
                    1,
                )
            },
            DsStatus::Atlas
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// The shipped symbols are unchanged, which is what "additive" has to mean.
    #[test]
    fn the_abi_version_did_not_move() {
        assert_eq!(DS_ABI_VERSION, 1);
        assert_eq!(DsStatus::Panic as i32, 8);
        // The two story #947 appended. A discriminant is the contract, and
        // these are the ones a later variant would renumber by being
        // inserted rather than appended.
        assert_eq!(DsStatus::FontFace as i32, 9);
        assert_eq!(DsStatus::Atlas as i32, 10);
        // The four the mapped load appended (issue #925). The version did not
        // move for them either, which is the rule this test exists to hold:
        // appending is free, inserting is not.
        assert_eq!(DsStatus::Map as i32, 11);
        assert_eq!(DsStatus::NoSuchRoot as i32, 12);
        assert_eq!(DsStatus::Derived as i32, 13);
        assert_eq!(DsStatus::Payload as i32, 14);
        // The one issue #884 appended, so a host on this ABI can honour the
        // rule `FrameError::is_recoverable` states and every other host reads
        // directly. Appended, so the version did not move for it either.
        assert_eq!(DsStatus::SurfaceLost as i32, 15);
    }

    /// A two-root `.dsb`, RAW, with `corrupt`'s payload one byte wrong.
    ///
    /// Copied from `dashscene-desktop`'s own test module rather than shared.
    /// No committed fixture is this shape: every `goldens/dsb` document has one
    /// root, and over a one-root document "the shown root's assets" and "every
    /// asset in the file" are the same set — so nothing built from one can tell
    /// a load bounded by the shown root from a load of the whole table. Two
    /// roots, one payload each, is the smallest document that can.
    ///
    /// Each root's paint is an image fill naming its own asset, so root 0's
    /// subtree reaches asset 0 and nothing else. That disjointness is what
    /// makes the corrupted payload a proof rather than a coincidence.
    fn two_root_document(corrupt: usize) -> Vec<u8> {
        use dashbuf::{
            AssetEntry, AssetEntryArgs, AssetKind, Document as Doc, DocumentArgs, Fill, ImageFill,
            ImageFillArgs, ImageFormat, NO_PARENT, Node, NodeArgs, Paint, PaintArgs,
        };
        use flatbuffers::FlatBufferBuilder;

        // Distinct bytes and distinct lengths, so a swapped pair is visible.
        let payloads = [vec![0xA1u8; 64], vec![0xB2u8; 96]];
        let mut builder = FlatBufferBuilder::new();

        let entries: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let hash = builder.create_vector(blake3::hash(payload).as_bytes());
                AssetEntry::create(
                    &mut builder,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: ImageFormat::Png,
                        kind: AssetKind::Image,
                        width: 8,
                        height: 8,
                    },
                )
            })
            .collect();
        let assets = builder.create_vector(&entries);

        let paints: Vec<_> = [0u32, 1]
            .into_iter()
            .map(|image| {
                let fill = ImageFill::create(
                    &mut builder,
                    &ImageFillArgs {
                        image,
                        ..Default::default()
                    },
                );
                Paint::create(
                    &mut builder,
                    &PaintArgs {
                        fill_type: Fill::ImageFill,
                        fill: Some(fill.as_union_value()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let paints = builder.create_vector(&paints);

        let nodes: Vec<_> = [0u32, 1]
            .into_iter()
            .map(|paint_entry| {
                Node::create(
                    &mut builder,
                    &NodeArgs {
                        parent: NO_PARENT,
                        paint_entry,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let nodes = builder.create_vector(&nodes);

        let document = Doc::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                paints: Some(paints),
                assets: Some(assets),
                ..Default::default()
            },
        );
        builder.finish(document, None);

        let bank = dashbuf::bank::ColdBank::raw(payloads.iter().map(Vec::as_slice));
        let mut file =
            dashbuf::bank::assemble(builder.finished_data(), &bank).expect("the fixture assembles");

        // The blob sections are in asset-entry order for a RAW assembly, and
        // the section table is left untouched — so the file still records what
        // each payload should hash to, and only a read of the bytes can notice.
        let container = dashbuf::container::Container::parse(&file).expect("the fixture parses");
        let blobs: Vec<_> = container
            .sections()
            .filter(|entry| entry.kind == dashbuf::container::SectionKind::Blob as u16)
            .collect();
        assert_eq!(blobs.len(), payloads.len(), "one blob per payload");
        let at = blobs[corrupt].offset as usize;
        file[at] ^= 0xFF;
        file
    }

    /// Writes `bytes` to a file in `dir` and returns the C string for its path.
    ///
    /// The path has to outlive the call, and a `CString` built inline in an
    /// argument list would be dropped at the end of the statement.
    fn written(dir: &tempfile::TempDir, name: &str, bytes: Vec<u8>) -> std::ffi::CString {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("the fixture writes");
        std::ffi::CString::new(path.to_str().expect("the temp path is UTF-8"))
            .expect("a temp path holds no interior NUL")
    }

    /// Loading bounded to the healthy root succeeds **because** the other
    /// root's payload is never touched.
    ///
    /// The fixture's unshown root carries a payload one byte wrong, so a load
    /// that read the whole asset table could not return `Ok`: `BlobResidency`
    /// would refuse it. This is the positive half of the bound — R5 on this
    /// path — and it is an assertion about what was *not* read, which no
    /// counter on this path could make, because a mapped load reads no payload
    /// byte and a counter here could only ever report zero.
    #[test]
    fn a_mapped_load_reads_only_the_shown_root() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(&dir, "two-root.dsb", two_root_document(1));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 0, std::ptr::null(), 0)
            },
            DsStatus::Ok,
            "root 1's payload is one byte wrong, and a load bounded to root 0 must never read it"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// And the other direction: the residency check is reached at all.
    ///
    /// Without this, the test above would pass just as well over a load that
    /// verified nothing. The pair is what makes the bound falsifiable rather
    /// than merely green.
    #[test]
    fn a_corrupt_payload_in_the_shown_root_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(&dir, "corrupt.dsb", two_root_document(0));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 0, std::ptr::null(), 0)
            },
            DsStatus::Payload,
            "root 0's own payload is corrupted, so bounding the load to it must refuse the file"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// An ordinal past the last root is refused, and the message says what the
    /// document does carry — which is what tells an out-of-range ask apart
    /// from a document with no roots at all.
    #[test]
    fn an_ordinal_past_the_last_root_is_refused() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(&dir, "two-root.dsb", two_root_document(0));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 7, std::ptr::null(), 0)
            },
            DsStatus::NoSuchRoot
        );

        let message = last_error();
        assert!(
            message.contains("carries 2"),
            "the message must name the count the document carries, and said: {message}"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// The last error, read the way `ds_last_error_message` documents.
    ///
    /// **The return value is the size *needed*, including the terminator — not
    /// the number of bytes written.** Slicing a fixed buffer by it both keeps
    /// the trailing NUL and indexes past the end as soon as the message is
    /// longer than the buffer, which for these tests is a message carrying a
    /// `tempfile` path and so depends on `TMPDIR`. Query with `(null, 0)`,
    /// allocate what it asks for, then drop the terminator — the same sequence
    /// `dashscene_android::host::last_error` uses.
    fn last_error() -> String {
        let needed = unsafe { ds_last_error_message(std::ptr::null_mut(), 0) };
        if needed <= 1 {
            return String::new();
        }
        let mut buffer = vec![0u8; needed];
        let again = unsafe { ds_last_error_message(buffer.as_mut_ptr().cast(), buffer.len()) };
        assert_eq!(
            again, needed,
            "the size must not change when a buffer is passed"
        );
        buffer.pop();
        String::from_utf8(buffer).expect("the message is UTF-8")
    }

    /// The mirror image of `a_mapped_load_reads_only_the_shown_root`, with the
    /// corruption and the ordinal both moved.
    ///
    /// **This pair is what says the ordinal is read rather than accepted and
    /// ignored.** With only the ordinal-0 case, bounding the prefetch to root 0
    /// unconditionally would pass every test in this file. `dashscene-desktop`
    /// carries the same pair over the same fixture and says so at its own call
    /// site; the first cut of this crate's tests kept only half of it.
    #[test]
    fn showing_the_second_root_leaves_the_first_roots_payload_cold() {
        let dir = tempfile::tempdir().expect("a temp dir");
        // Root 0's payload is the corrupt one this time, and root 1 is shown.
        let path = written(&dir, "two-root.dsb", two_root_document(0));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 1, std::ptr::null(), 0)
            },
            DsStatus::Ok,
            "root 0's payload is one byte wrong, and a load bounded to root 1 must never read it"
        );

        // And the root that ended up shown is the one asked for, not the
        // document's first. A prefetch bounded correctly while the traversal is
        // confined to the wrong root would draw the wrong artboard with nothing
        // to report it, which is the conflation issue #943 records.
        // Through `live` for the reason that function gives (issue #979).
        let arena = &unsafe { live(runtime) }.arena;
        let shown = arena
            .committed()
            .shown_root()
            .expect("the load named a shown root");
        assert_eq!(
            shown,
            arena.roots()[1],
            "ordinal 1 must name the document's second root in the arena"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// And the second root's own corruption is still refused when it is the one
    /// shown — the other half of the swap.
    #[test]
    fn showing_the_second_root_refuses_its_own_corrupted_payload() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = written(&dir, "corrupt.dsb", two_root_document(1));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 1, std::ptr::null(), 0)
            },
            DsStatus::Payload,
            "root 1's own payload is corrupted, so bounding the load to it must refuse the file"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// A **refused** load leaves the document already loaded still drawable.
    ///
    /// That is what `load_mapped_into` promises in prose — "every failure this
    /// returns is raised before the runtime's arena is replaced" — and it is
    /// worth an assertion rather than a reading, because the order of the
    /// fallible steps against [`drop_document`] is the whole of it. Move the
    /// residency walk below that call and this fails.
    ///
    /// It says nothing about the **panic** path, which is why
    /// [`drop_document`] clears the scene rather than leaving it until the end:
    /// an unwind between the two would otherwise leave a new arena paired with
    /// the previous scene's `NodeId`s. `guard` makes that state reachable and no
    /// test here can force it, so the clearing is argued at that function rather
    /// than covered.
    ///
    /// **It asserts the committed table, not the tick's status**, for the reason
    /// its byte-taking pair does: a tick answers `Ok` whenever `runtime.scene`
    /// is `Some`, which is true of the broken state as well, so the status alone
    /// passes over an arena replaced above the residency walk.
    ///
    /// It covers `Payload` and none of this path's five other refusals — `Map`,
    /// `Open`, `Gate`, `Derived` and `NoSuchRoot`. `Map` and `NoSuchRoot` have
    /// tests of their own, but each loads into a fresh runtime and never ticks,
    /// so neither says a previous document survived; mapped `Open`, mapped
    /// `Gate` and `Derived` are reached by no test in this crate at all.
    #[test]
    fn a_refused_mapped_load_leaves_the_loaded_document_drawable() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let good = written(&dir, "good.dsb", two_root_document(1));
        let bad = written(&dir, "bad.dsb", two_root_document(0));

        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, good.as_ptr(), 0, std::ptr::null(), 0)
            },
            DsStatus::Ok
        );
        assert_eq!(
            unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) },
            DsStatus::Ok,
            "the first load left a scene to tick"
        );
        let loaded = committed_rows(runtime);
        assert!(loaded > 0, "the fixture commits rects to compare against");

        // A load refused at the residency walk, which sits above
        // `drop_document`.
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, bad.as_ptr(), 0, std::ptr::null(), 0)
            },
            DsStatus::Payload
        );
        assert_eq!(
            unsafe { ds_runtime_tick(runtime, 0.016, std::ptr::null_mut()) },
            DsStatus::Ok,
            "a refused load must leave the previously loaded document drawable, not discard it"
        );
        assert_eq!(
            committed_rows(runtime),
            loaded,
            "and drawable means the document that was loaded, not an emptied arena a scene is \
             still attached to"
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// A null path is a status, not a dereference.
    #[test]
    fn a_null_path_is_a_status_and_not_a_dereference() {
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, std::ptr::null(), 0, std::ptr::null(), 0)
            },
            DsStatus::NullArgument
        );
        unsafe { ds_runtime_free(runtime) };
    }

    /// A path nothing is at reports `Map` rather than opening an empty
    /// document and failing later as something else.
    #[test]
    fn a_path_that_does_not_exist_reports_map() {
        let path = std::ffi::CString::new("/nonexistent/no-such.dsb").expect("no interior NUL");
        let mut runtime = std::ptr::null_mut();
        assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
        assert_eq!(
            unsafe {
                ds_runtime_load_document_mapped(runtime, path.as_ptr(), 0, std::ptr::null(), 0)
            },
            DsStatus::Map
        );
        unsafe { ds_runtime_free(runtime) };
    }
}
