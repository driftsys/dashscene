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
//! **Root selection is absent on purpose.** The ABI would carry it, but no host
//! can name a root yet: both integration crates call
//! `dashbuf::prefetch::first_root`, and the selection concept is story #837 on
//! the `main` track. Adding a parameter now would mean inventing a vocabulary
//! that story is about to settle. It joins when #837 lands, and the versioning
//! rule below says what that costs.
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
use dashscene_engine::TaffySolver;
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
        runtime.scene = Some(dashlang::attach_live(
            &mut runtime.arena,
            Box::new(TaffySolver::new()),
        ));
        if let Some(surface) = runtime.surface.as_mut() {
            // The arena is new, so its generations restart and nothing in the
            // frames themselves says so — the trap `dashscene-web` and
            // `dashscene-desktop` both name under "rebuilding on resize".
            surface.document_replaced();
        }
        DsStatus::Ok
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
            let take = bytes.len().min(cap - 1);
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
}
