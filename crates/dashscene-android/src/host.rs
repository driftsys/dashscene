//! The document path, and the JNI entry points a Kotlin host binds to.
//!
//! Compiled on Android and nowhere else. What runs the frames is
//! [`crate::loop_`]; this module is one [`Frames`] implementation — the one that
//! draws a compiled `.dsb` — plus the JNI surface over it.
//!
//! # Why this one goes through the C ABI
//!
//! D2 of `docs/decisions/host-integration-in-three-layers.md` says every
//! platform host sits on the C ABI, and this is the path that does: it drives
//! `dashscene-ffi` through its own entry points as a C caller would, which is
//! what established the ABI was sufficient for layer 0 — and what established
//! that it was not quite, since `ds_runtime_detach_surface` had to be added for
//! the destroy handshake.
//!
//! A scene built in code cannot go through it. `SceneBuilder` needs an `Arena`,
//! and the ABI's arena lives inside an opaque `DsRuntime`; a builder entry point
//! is layer 2 (D8) and is deferred with its layer. So `demo-android` implements
//! [`Frames`] over `dashscene-gpu` and `dashlang` directly instead, and the two
//! paths meet at the trait rather than at a second frame loop.

use std::ffi::c_void;

use dashscene_ffi::{DsRuntime, DsStatus, DsSurfaceKind};
use jni::errors::LogErrorAndDefault;
use jni::objects::{JByteArray, JClass, JIntArray, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jint, jlong};
use jni::{Env, EnvUnowned};

use crate::frames::{AttachError, Frames, Step};
use crate::log;
use crate::loop_::{self, AndroidHost};

/// Reads the ABI's last error, for a log line that says what actually failed.
fn last_error() -> String {
    // SAFETY: a null buffer with a zero capacity is the documented size query.
    let needed = unsafe { dashscene_ffi::ds_last_error_message(std::ptr::null_mut(), 0) };
    if needed <= 1 {
        return String::new();
    }
    let mut buffer = vec![0_u8; needed];
    // SAFETY: `buffer` is writable for `needed` bytes, which is what the size
    // query asked for.
    unsafe {
        dashscene_ffi::ds_last_error_message(buffer.as_mut_ptr().cast(), buffer.len());
    }
    buffer.pop();
    String::from_utf8_lossy(&buffer).into_owned()
}

/// One face as this host holds it, so `attach` can rebuild the borrowed
/// `DsFontFace` array on every surface cycle.
///
/// The family is a `CString` rather than a `String` because the ABI takes a
/// NUL-terminated pointer, and building one per attach would leave the
/// pointer dangling the moment the temporary dropped.
struct OwnedFace {
    family: std::ffi::CString,
    weight: u16,
    font: Vec<u8>,
    atlas_png: Vec<u8>,
    atlas_metrics: Vec<u8>,
}

/// A compiled `.dsb`, drawn through the C ABI.
struct DocumentFrames {
    runtime: *mut DsRuntime,
    /// The bytes, **kept for the life of this object**.
    ///
    /// The ABI's load is the owning path — `dashscene_core::load_document`
    /// copies every payload — so holding them costs a second copy of the file.
    /// They are held anyway, because a rebuild after a recoverable surface loss
    /// detaches (which frees the runtime and the document with it) and attaches
    /// again, and an attach needs bytes. Taking them on the first attach left
    /// the rebuild failing with "no document bytes" every time, which turned the
    /// one remedy `Step::Rebuild` exists to provide into a guaranteed way to
    /// kill the loop.
    document: Vec<u8>,
    /// The cascade and its sheets, **kept for the life of this object**, for
    /// the same reason `document` is: a rebuild after a recoverable surface
    /// loss detaches — which frees the runtime — and attaches again, and an
    /// attach needs them. Empty is a document loaded without text.
    faces: Vec<OwnedFace>,
}

impl Frames for DocumentFrames {
    unsafe fn attach(
        &mut self,
        window: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), AttachError> {
        let mut runtime: *mut DsRuntime = std::ptr::null_mut();
        // SAFETY: `runtime` is a valid writable out-pointer.
        if unsafe { dashscene_ffi::ds_runtime_new(&mut runtime) } != DsStatus::Ok {
            return Err(format!("ds_runtime_new: {}", last_error()));
        }
        // Stored **before** anything else can fail, so `detach` — which the loop
        // calls even when this returns `Err` — has the pointer to free. Holding
        // it in a local and returning early is how the runtime, and on the
        // attach path the wgpu device inside it, leaked once per surface cycle.
        self.runtime = runtime;

        // An empty sheet is a null pointer, so the Java surface can say what
        // the C one can. `Vec::as_ptr` on an empty vector returns a dangling
        // but NON-NULL pointer, so a Kotlin host passing `ByteArray(0)` — the
        // natural way to say "this face has no sheet" — would otherwise build
        // a descriptor the ABI reads as "atlas present, length 0" and fails to
        // decode. Both empty is now the measure-only cascade the C header
        // documents, and one empty against one filled is still `DS_ATLAS`,
        // which is the half-described face the ABI refuses on purpose.
        let sheet = |bytes: &[u8]| {
            if bytes.is_empty() {
                std::ptr::null()
            } else {
                bytes.as_ptr()
            }
        };
        let descriptors: Vec<dashscene_ffi::DsFontFace> = self
            .faces
            .iter()
            .map(|face| dashscene_ffi::DsFontFace {
                family: face.family.as_ptr(),
                weight: face.weight,
                face_index: 0,
                font_bytes: face.font.as_ptr(),
                font_len: face.font.len(),
                atlas_png: sheet(&face.atlas_png),
                atlas_png_len: face.atlas_png.len(),
                atlas_metrics: sheet(&face.atlas_metrics),
                atlas_metrics_len: face.atlas_metrics.len(),
            })
            .collect();
        // SAFETY: `runtime` is live, `document` is a readable slice, and every
        // pointer in `descriptors` borrows a field of `self.faces`, which
        // outlives this call.
        let loaded = unsafe {
            dashscene_ffi::ds_runtime_load_document_with_text(
                runtime,
                self.document.as_ptr(),
                self.document.len(),
                descriptors.as_ptr(),
                descriptors.len(),
            )
        };
        if loaded != DsStatus::Ok {
            return Err(format!("load_document: {loaded:?} {}", last_error()));
        }

        // SAFETY: the loop promises `window` outlives this object, which is
        // exactly what `ds_runtime_attach_surface` asks.
        let attached = unsafe {
            dashscene_ffi::ds_runtime_attach_surface(
                runtime,
                DsSurfaceKind::AndroidNdk as i32,
                window,
                std::ptr::null_mut(),
                width,
                height,
            )
        };
        if attached != DsStatus::Ok {
            return Err(format!("attach_surface: {attached:?} {}", last_error()));
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> bool {
        // SAFETY: `runtime` is live and this is the only thread calling it.
        let resized = unsafe { dashscene_ffi::ds_runtime_resize(self.runtime, width, height) };
        if resized != DsStatus::Ok {
            log(&format!("resize: {resized:?} {}", last_error()));
            return false;
        }
        true
    }

    fn frame(&mut self, dt: f32, forced: bool) -> Step {
        let mut advanced = false;
        // SAFETY: `runtime` is live; `advanced` is a valid out-pointer.
        let ticked = unsafe { dashscene_ffi::ds_runtime_tick(self.runtime, dt, &mut advanced) };
        if ticked != DsStatus::Ok {
            log(&format!("tick: {ticked:?} {}", last_error()));
            return Step::Stop;
        }
        if !advanced && !forced {
            // The idle skip. A static document draws once and then costs a tick
            // a frame and nothing else.
            return Step::Continue;
        }
        // SAFETY: `runtime` is live and no other call is in flight.
        let drawn = unsafe { dashscene_ffi::ds_runtime_draw(self.runtime, std::ptr::null_mut()) };
        if drawn != DsStatus::Ok {
            log(&format!("draw: {drawn:?} {}", last_error()));
            // **`DsStatus::SurfaceLost` and nothing else** (issue #884). Until
            // it existed the ABI flattened every surface failure into
            // `DsStatus::Surface`, so this host rebuilt on all of them and
            // relied on the loop's bound on consecutive rebuilds to stop an
            // unrecoverable one spinning — a guess, and the only host that had
            // to make one. `DsStatus::Surface` now means the presenter cannot
            // be recovered by rebuilding it, which is what `FrameError::Lost`
            // being the sole recoverable case says.
            //
            // The bound stays, and is not made redundant by this: a surface
            // genuinely lost on every frame is a device that has gone away, and
            // the rebuild is then a remedy that keeps not working.
            return if drawn == DsStatus::SurfaceLost {
                Step::Rebuild
            } else {
                Step::Stop
            };
        }
        Step::Continue
    }

    fn detach(&mut self) {
        // The document is **not** dropped here: `detach` is also the first half
        // of a rebuild, and the `attach` that follows needs it. It goes when
        // this object does, which the loop's leaked state makes the end of the
        // process — the cost of a second copy of the file, recorded rather than
        // hidden.
        //
        // Tolerates being called after a failed `attach`, which the loop does.
        if self.runtime.is_null() {
            return;
        }
        // The surface first, then the runtime. Both before the loop releases the
        // handshake, which is what D4 requires of the first and what issue #872
        // records as unnecessary for the second.
        //
        // SAFETY: `runtime` is live and no other call is in flight — this is the
        // only thread that has ever touched it.
        unsafe {
            dashscene_ffi::ds_runtime_detach_surface(self.runtime, std::ptr::null_mut());
            dashscene_ffi::ds_runtime_free(self.runtime);
        }
        self.runtime = std::ptr::null_mut();
    }
}

/// Acquires the window from `surface`, builds a [`DocumentFrames`] from
/// `document` and `faces`, and starts its frame loop.
///
/// Shared by `nativeSurfaceCreated` and `nativeSurfaceCreatedWithText`, which
/// differ only in what they put in `faces`. The three `unsafe` blocks here —
/// acquiring the window, starting the frame loop, and releasing the window on
/// a failed start — used to be duplicated between the two entry points; this
/// crate's own history has an example of exactly that leak shape (see the
/// comment on `self.runtime =
/// runtime` in [`DocumentFrames::attach`]), and issue #945 is the general
/// case: a host rule written twice can drift once. This module compiles for
/// one target that no test in this repository can reach, which is what makes
/// that drift worth spending a helper on rather than tolerating it.
///
/// Returns an opaque handle, or 0 if the window or the thread could not be
/// obtained. **A non-zero handle does not mean the runtime started** — see
/// [`crate::loop_::start`] for why that is deliberate.
///
/// `nativeIsRunning` answers whether the loop has **ended**, and not whether
/// it has come up: it reports [`Handshake::is_running`], which is true for
/// `Starting` as well as `Running`, and the render thread reports `started()`
/// only once its attach has returned. So a thread still inside an attach — up
/// to 218 s on an unoptimized build (issue #960) — answers `true`, which is
/// the same answer a drawing loop gives. What separates them is the pair of
/// log lines around the attach: `attaching a WxH surface` with no
/// `attached a WxH surface` after it.
///
/// [`Handshake::is_running`]: crate::Handshake::is_running
fn start_document_host(
    env: &mut Env<'_>,
    surface: &JObject<'_>,
    document: Vec<u8>,
    faces: Vec<OwnedFace>,
    width: jint,
    height: jint,
) -> jni::errors::Result<jlong> {
    // The one call that must happen on this thread: it needs the `JNIEnv`
    // and the live `jobject`, and both are the UI thread's.
    //
    // SAFETY: `env` and `surface` are the JVM's own, valid for this call.
    let window = unsafe {
        ndk_sys::ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast())
    };
    if window.is_null() {
        log("ANativeWindow_fromSurface returned null");
        return Ok(0);
    }
    // A factory: the value is built on the render thread. Only `document` and
    // `faces` cross, and both are `Send`.
    let frames = move || -> Box<dyn Frames> {
        Box::new(DocumentFrames {
            runtime: std::ptr::null_mut(),
            document,
            faces,
        })
    };
    // SAFETY: `window` is the reference `fromSurface` returned, which this
    // crate owns until the handshake completes.
    let host = unsafe {
        loop_::start(
            window.cast(),
            frames,
            width.max(0) as u32,
            height.max(0) as u32,
        )
    };
    if host.is_null() {
        // SAFETY: the one reference `fromSurface` gave this crate.
        unsafe { ndk_sys::ANativeWindow_release(window) };
        return Ok(0);
    }
    Ok(host as jlong)
}

/// Creates a host that draws a compiled `.dsb`, and starts its frame loop.
///
/// `surface` is the `android.view.Surface` the view handed over; `document` is
/// the `.dsb` bytes.
///
/// Returns an opaque handle, or 0 if the window or the thread could not be
/// obtained. **A non-zero handle does not mean the runtime started** — see
/// [`crate::loop_::start`] for why that is deliberate, and
/// [`start_document_host`] for what `nativeIsRunning` does and does not
/// answer.
///
/// # Safety
///
/// Called by the JVM with a valid environment and a live `Surface`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreated<'local>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    surface: JObject<'local>,
    document: JByteArray<'local>,
    width: jint,
    height: jint,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<jlong> {
            let bytes = env.convert_byte_array(&document)?;
            start_document_host(env, &surface, bytes, Vec::new(), width, height)
        })
        .resolve::<LogErrorAndDefault>()
}

/// Creates a host that draws a compiled `.dsb` with the fonts its text
/// needs, and starts its frame loop.
///
/// The five arrays are parallel and must be the same length: one entry per
/// face — a family name, a CSS weight, a font file's bytes, and the
/// committed MSDF sheet the face's glyphs sample. A length disagreement is a
/// 0 handle and a log line rather than a cascade assembled from entries that
/// do not belong together.
///
/// **This is a subset of what `DsFontFace` carries.** There is no array for
/// the face's index within a collection: every face is declared at index 0,
/// so a `.ttc` reaches only its first face through this entry point. Issue
/// #981 carries the rest.
///
/// The weight is checked by the ABI, in `1..=1000`, and by nothing here —
/// what this rejects is only a value a `u16` cannot carry.
///
/// **Nothing bakes an atlas at run time**, so a host reads these from its
/// own assets. `nativeSurfaceCreated` is this call with no faces.
///
/// # Safety
///
/// Called by the JVM with a valid environment and a live `Surface`.
///
/// One parameter per JNI argument is the only shape a native method binds
/// to, so this stays over clippy's default threshold.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedWithText<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    surface: JObject<'local>,
    document: JByteArray<'local>,
    families: JObjectArray<'local, JString<'local>>,
    weights: JIntArray<'local>,
    fonts: JObjectArray<'local, JByteArray<'local>>,
    atlas_pngs: JObjectArray<'local, JByteArray<'local>>,
    atlas_metrics: JObjectArray<'local, JByteArray<'local>>,
    width: jint,
    height: jint,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<jlong> {
            let bytes = env.convert_byte_array(&document)?;

            // Every array must agree with `families`. `weights` is checked
            // apart from the other three because it is a primitive array and
            // they are object arrays, so the two cannot share one list.
            let count = families.len(env)?;
            let mismatched = if weights.len(env)? != count {
                Some("weights")
            } else if fonts.len(env)? != count {
                Some("fonts")
            } else if atlas_pngs.len(env)? != count {
                Some("atlasPngs")
            } else if atlas_metrics.len(env)? != count {
                Some("atlasMetrics")
            } else {
                None
            };
            if let Some(what) = mismatched {
                log(&format!(
                    "nativeSurfaceCreatedWithText: {what} has a different length from families"
                ));
                return Ok(0);
            }

            let mut faces = Vec::with_capacity(count);
            for index in 0..count {
                let mut weight = [0_i32; 1];
                weights.get_region(env, index as jint, &mut weight)?;
                // **The CSS range is not checked here.** `DsFontFace::weight`
                // is where a weight is judged, and one rule in one place is
                // why: this clamped as well until story #947's review, so a
                // Kotlin host and a C host got different answers to the same
                // question. What is refused here is only what a `u16` cannot
                // carry to the ABI at all — a truncating cast would turn
                // 65 936 into 400 and the ABI would accept it.
                let Ok(weight) = u16::try_from(weight[0]) else {
                    log(&format!(
                        "nativeSurfaceCreatedWithText: face {index} declares weight {}, \
                         which does not fit the descriptor; the accepted range is 1..=1000",
                        weight[0]
                    ));
                    return Ok(0);
                };

                // A frame per face, because each `get_element` below returns a
                // local reference and JNI guarantees only 16 slots without
                // asking for more. Four per face means a five-face cascade
                // exhausts the frame the JVM gave this call. Everything that
                // leaves the frame is owned, so nothing here outlives it.
                let (name, font, atlas_png, atlas_metrics_bytes) = env.with_local_frame(
                    8,
                    |env| -> jni::errors::Result<(String, Vec<u8>, Vec<u8>, Vec<u8>)> {
                        let name = families.get_element(env, index)?.try_to_string(env)?;
                        let font: JByteArray = fonts.get_element(env, index)?;
                        let font = env.convert_byte_array(&font)?;
                        let atlas_png: JByteArray = atlas_pngs.get_element(env, index)?;
                        let atlas_png = env.convert_byte_array(&atlas_png)?;
                        let atlas_metrics_bytes: JByteArray =
                            atlas_metrics.get_element(env, index)?;
                        let atlas_metrics_bytes = env.convert_byte_array(&atlas_metrics_bytes)?;
                        Ok((name, font, atlas_png, atlas_metrics_bytes))
                    },
                )?;

                let Ok(family) = std::ffi::CString::new(name) else {
                    log("nativeSurfaceCreatedWithText: a family name contains a NUL");
                    return Ok(0);
                };
                faces.push(OwnedFace {
                    family,
                    weight,
                    font,
                    atlas_png,
                    atlas_metrics: atlas_metrics_bytes,
                });
            }

            start_document_host(env, &surface, bytes, faces, width, height)
        })
        .resolve::<LogErrorAndDefault>()
}

/// Reports a new **physical**-pixel extent. Picked up by the next frame.
///
/// # Safety
///
/// `handle` must be a live handle from `nativeSurfaceCreated`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceChanged(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
    width: jint,
    height: jint,
) {
    // SAFETY: the caller promises `handle` is live.
    unsafe {
        loop_::resize(
            handle as *mut AndroidHost,
            width.max(0) as u32,
            height.max(0) as u32,
        )
    };
}

/// **The destroy handshake.** Blocks until rendering has stopped and the surface
/// has been dropped, then releases the window.
///
/// # Safety
///
/// `handle` must be a live handle from `nativeSurfaceCreated`, and must not be
/// used again afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceDestroyed(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) {
    // SAFETY: the caller promises this is the handle's last use.
    unsafe { loop_::destroy(handle as *mut AndroidHost) };
    log("surfaceDestroyed returning — the surface is gone");
}

/// The ABI generation this library was built with, for a host that checks.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeAbiVersion(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
) -> jint {
    dashscene_ffi::ds_abi_version() as jint
}

/// Whether the frame loop is still live.
///
/// # Safety
///
/// `handle` must be a live handle from `nativeSurfaceCreated`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeIsRunning(
    _env: EnvUnowned<'_>,
    _class: JClass<'_>,
    handle: jlong,
) -> jboolean {
    // SAFETY: the caller promises `handle` is live.
    unsafe { loop_::is_running(handle as *const AndroidHost) }
}
