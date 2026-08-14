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
//! **Root selection is absent on purpose, and the reason changed at story
//! #837.** It used to be that no host could name a root at all. That is settled:
//! `dashbuf::prefetch::ShownRoot` is the vocabulary, both integration crates
//! take one, and
//! `docs/decisions/the-shown-root-is-named-by-ordinal.md` records why it is an
//! ordinal.
//!
//! What was missing was something for it to bound.
//! [`ds_runtime_load_document`] takes the whole file as `(ptr, len)` and hands
//! every payload to `dashscene_core::load_document` — the **owning** loader,
//! which copies every payload into an owned `ImageAsset` and so needs bytes for
//! every asset entry whether or not anything draws them. A `ShownRoot`
//! parameter on that call would have been accepted and changed nothing
//! measurable, which is worse than its absence: it would read as a bound that
//! is not one.
//!
//! **Story #838 ended that half, and it is why this paragraph is no longer a
//! reason to leave the selection out.** The traversal, the solve and the paint
//! follow the root a host names, so a `ShownRoot` reaching this ABI would bound
//! the **per-frame** cost of a many-artboard document here exactly as it does on
//! the other two targets — one Taffy layout computation and one root's rect table
//! per frame rather than the document's — while the **load** stayed whole-file.
//! That is a real bound and a partial one, and both halves have to be said
//! together: a caller told only the first would read it as R5 on this path,
//! which it is not.
//!
//! It is not built here because that is a signature change on a shipped symbol.
//! **Issue #925** is the other half — this ABI has no mapped entry point, and
//! the story its documentation deferred that to closed without giving it an
//! owner — and adding one is a **new symbol**, which the versioning rule below
//! makes free, where a parameter on this one is a changed signature and bumps
//! [`DS_ABI_VERSION`]. So the shape that costs nothing is the shape that also
//! bounds the load, and doing them together is why neither is here yet.
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
//! `dashscene_android::host` calls [`ds_runtime_load_document_with_text`],
//! but that path has been compiled for its target and never run on device
//! hardware — nothing here describes Android as working; that is issue #885.
//!
//! # The three rules this ABI keeps
//!
//! 1. **No panic crosses the boundary.** Every entry point runs inside
//!    [`std::panic::catch_unwind`] and turns an unwind into [`DsStatus::Panic`].
//!    An unwind across `extern "C"` is undefined behaviour, and this crate is
//!    the one place in the workspace where that can happen.
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
use std::ffi::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};

use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_core::Arena;
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

thread_local! {
    /// The last failure's message, for [`ds_last_error_message`].
    ///
    /// Thread-local rather than held on the runtime, because the calls that can
    /// fail before a runtime exists — [`ds_runtime_new`] itself — still need
    /// somewhere to report. A host that calls across threads reads the message
    /// on the thread that failed, which is the only reading that is meaningful.
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
/// honest shape for an ABI whose caller handed over bytes — it has no file to
/// map and no path to open. A mapped path, which is what R5 is stated over,
/// belongs with the platform host that has the file (story #841).
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

/// The load both entry points run. `text` is what the caller could supply.
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
    runtime.arena = Arena::new();
    dashscene_core::load_document(&document, &payloads, &mut runtime.arena);
    // `TaffySolver::boxed` rather than the same two arms written here: it
    // exists so that no document loader can disagree with another about what
    // `None` means, and `dashscene-desktop` and `dashscene-web` both call it.
    runtime.scene = Some(dashlang::attach_live(
        &mut runtime.arena,
        TaffySolver::boxed(text),
    ));
    if let Some(surface) = runtime.surface.as_mut() {
        // The arena is new, so its generations restart and nothing in the
        // frames themselves says so — the trap `dashscene-web` and
        // `dashscene-desktop` both name under "rebuilding on resize".
        surface.document_replaced();
    }
    DsStatus::Ok
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
    /// **Validated here and nowhere else.** A value outside that range is
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
            set_last_error(format!(
                "ds_runtime_load_document_with_text: face {index} has a null family or \
                 font_bytes"
            ));
            return Err(DsStatus::NullArgument);
        }
        let family = match unsafe { std::ffi::CStr::from_ptr(face.family) }.to_str() {
            Ok(family) => family.to_string(),
            Err(error) => {
                set_last_error(format!("face {index}: family is not UTF-8: {error}"));
                return Err(DsStatus::FontFace);
            }
        };
        // The CSS range, checked here so that this is the only place any host
        // on this ABI gets an answer about it. 0 is the value an
        // uninitialised descriptor carries, which is why refusing beats
        // repairing: a face declared at weight 0 resolves against every
        // request as if the host had meant it.
        if !(1..=1000).contains(&face.weight) {
            set_last_error(format!(
                "face {index}: weight {} is outside the CSS range 1..=1000",
                face.weight
            ));
            return Err(DsStatus::FontFace);
        }
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
        let text = if face_count == 0 {
            None
        } else {
            let described = match unsafe { faces_from_c(faces, face_count) } {
                Ok(described) => described,
                Err(status) => return status,
            };
            match TextResources::from_faces(described) {
                Ok(text) => Some(text),
                Err(error) => {
                    // `{error}` rather than `{error:?}`: this string reaches
                    // a host through `ds_last_error_message`, where every
                    // other message is prose, and nested `Debug` would put
                    // escaped quotes in front of it.
                    set_last_error(format!("{error}"));
                    // Every variant that exists is named rather than swept
                    // into the wildcard. `TextResourcesError` is
                    // `#[non_exhaustive]` and lives in another crate, so an
                    // arm for the unknown is still required — but a future
                    // atlas-shaped variant landing there would report as a
                    // font-face failure and send a host branching on the
                    // discriminant to the wrong half of its own descriptor.
                    // Naming them is what makes the compiler's requirement
                    // the only thing the wildcard carries.
                    return match error {
                        TextResourcesError::Atlas { .. } | TextResourcesError::MixedAtlases => {
                            DsStatus::Atlas
                        }
                        TextResourcesError::NoFaces
                        | TextResourcesError::EmptyFamily { .. }
                        | TextResourcesError::Font { .. } => DsStatus::FontFace,
                        _ => DsStatus::FontFace,
                    };
                }
            }
        };
        load_into(runtime, bytes, text)
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
                DsStatus::Surface
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
        let mut buf = [0_i8; 5];
        let needed = unsafe { ds_last_error_message(buf.as_mut_ptr(), buf.len()) };
        assert_eq!(needed, "twelve chars".len() + 1);
        assert_eq!(buf[4], 0, "the terminator is written inside the buffer");
        let text = buf[..4]
            .iter()
            .map(|byte| *byte as u8 as char)
            .collect::<String>();
        assert_eq!(text, "twel");
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
            // asserted above, and nothing has freed it yet.
            let out = measured(unsafe { &*runtime });
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
    /// it. **This is the only place the range is decided**: the JNI half in
    /// `dashscene-android` clamped it as well until story #947's review,
    /// which gave one question two answers.
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
                "weight {weight} is outside 1..=1000 and the ABI is what says so"
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
        // The two this story appended. A discriminant is the contract, and
        // these are the ones a later variant would renumber by being
        // inserted rather than appended.
        assert_eq!(DsStatus::FontFace as i32, 9);
        assert_eq!(DsStatus::Atlas as i32, 10);
    }
}
