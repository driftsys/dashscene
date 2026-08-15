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

use std::ffi::{CStr, c_void};

use dashscene_ffi::{DsRuntime, DsStatus, DsSurfaceKind};
use jni::errors::LogErrorAndDefault;
use jni::objects::{JByteArray, JClass, JObject, JObjectArray, JString};
use jni::strings::JNIStr;
use jni::sys::{jboolean, jint, jlong};
use jni::{Env, EnvUnowned, jni_sig};

use crate::face;
use crate::frames::{AttachError, Frames, Step};
use crate::log;
use crate::loop_::{self, AndroidHost};

/// One [`crate::face`] name as JNI wants it.
///
/// A `const fn` so every call site below can bind the result to a `const`, and
/// the conversion happens when this crate is compiled rather than on the UI
/// thread inside a JNI entry point. `JNIStr::from_cstr` validates modified
/// UTF-8; every name in `face` is ASCII and therefore cannot fail, so the
/// `panic!` is a compile error that no build can reach — which is the point of
/// doing it here rather than unwrapping at run time.
const fn jni_name(name: &'static CStr) -> &'static JNIStr {
    match JNIStr::from_cstr(name) {
        Some(name) => name,
        None => panic!("a DsFace field name is not valid modified UTF-8"),
    }
}

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
    face_index: u32,
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
                face_index: face.face_index,
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
        // this object does, which is `LoopState::shut_down` — the loop drops
        // the implementation there rather than retaining it inside its leaked
        // state (issue #1085), so the second copy of the file costs the surface
        // cycle it was kept for rather than the life of the process.
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
/// One `DsFace` per face, in cascade order — a family name, a CSS weight, the
/// face's index inside a collection, a font file's bytes, and the committed
/// MSDF sheet the face's glyphs sample.
///
/// # Why a descriptor class and not six parallel arrays
///
/// `docs/design/host-integration.md` carries the argument, and this does not
/// restate it. It was written out here, in `DsFace.java`, in
/// `DashsceneNative.java` and in the record, and two of its claims were wrong
/// in all four copies at once — which is the case for keeping it in one place
/// rather than an aesthetic preference.
///
/// The short of it: five parallel arrays could not carry
/// `DsFontFace::face_index`, and a sixth would have widened a length agreement
/// that nothing checks. One array of descriptors makes that disagreement
/// unrepresentable.
///
/// # What is checked here, and what is not
///
/// The ABI judges the values — a family that is empty or only whitespace, a
/// weight outside `1..=1000`, font bytes that do not parse — so that a Kotlin
/// host and a C host get the same answer to the same input. This rejects only
/// what cannot cross to the descriptor at all: a null face or field, a negative
/// `faceIndex`, a weight a `u16` cannot hold, or a family carrying a NUL.
///
/// **Nothing bakes an atlas at run time**, so a host reads these from its own
/// assets. `nativeSurfaceCreated` is this call with no faces.
///
/// # Safety
///
/// Called by the JVM with a valid environment and a live `Surface`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedWithText<
    'local,
>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    surface: JObject<'local>,
    document: JByteArray<'local>,
    faces: JObjectArray<'local, JObject<'local>>,
    width: jint,
    height: jint,
) -> jlong {
    unowned
        .with_env(|env| -> jni::errors::Result<jlong> {
            let bytes = env.convert_byte_array(&document)?;

            let count = faces.len(env)?;
            let mut owned = Vec::with_capacity(count);
            for index in 0..count {
                // A frame per face. Each element and each object field is a
                // local reference, and JNI guarantees only 16 slots without
                // asking; five per face means a four-face cascade exhausts the
                // frame the JVM gave this call. Everything that leaves the
                // frame is owned, so nothing here outlives it.
                let face =
                    env.with_local_frame(8, |env| -> jni::errors::Result<Option<OwnedFace>> {
                        read_face(env, &faces, index)
                    })?;
                let Some(face) = face else {
                    // `read_face` has already said which face and why.
                    return Ok(0);
                };
                owned.push(face);
            }

            start_document_host(env, &surface, bytes, owned, width, height)
        })
        .resolve::<LogErrorAndDefault>()
}

/// Reads one `DsFace` out of `faces`, or `None` after logging why it could not.
///
/// Split out because the entry point above is the JNI signature and this is the
/// descriptor, and because a `?` here would report a JNI error where the caller
/// wants a zero handle. `None` is this side's refusal; a `jni::errors::Error` is
/// the JVM's.
fn read_face<'frame, 'array>(
    env: &mut Env<'frame>,
    faces: &JObjectArray<'array, JObject<'array>>,
    index: usize,
) -> jni::errors::Result<Option<OwnedFace>> {
    // **The six names, from the one list this crate holds** (issue #1089).
    // Spelled in `crate::face` rather than here, because a host test can read
    // that module and compare it against `DsFace.java`'s own declarations — and
    // nothing else can: this file is behind the platform `cfg`, so no test
    // binary links it and no gate lints it.
    const FAMILY: &JNIStr = jni_name(face::FAMILY);
    const WEIGHT: &JNIStr = jni_name(face::WEIGHT);
    const FACE_INDEX: &JNIStr = jni_name(face::FACE_INDEX);
    const FONT: &JNIStr = jni_name(face::FONT);
    const ATLAS_PNG: &JNIStr = jni_name(face::ATLAS_PNG);
    const ATLAS_METRICS: &JNIStr = jni_name(face::ATLAS_METRICS);

    let face = faces.get_element(env, index)?;
    if face.is_null() {
        log(&format!(
            "nativeSurfaceCreatedWithText: face {index} is null"
        ));
        return Ok(None);
    }

    let family = env.get_field(&face, FAMILY, jni_sig!("Ljava/lang/String;"))?;
    let family: JString = env.cast_local::<JString>(family.l()?)?;
    if family.is_null() {
        log(&format!(
            "nativeSurfaceCreatedWithText: face {index} has no {}",
            FAMILY.to_str()
        ));
        return Ok(None);
    }
    let family = family.try_to_string(env)?;
    // The CSS range is **not** checked here. `DsFontFace::weight` is where a
    // weight is judged, and one rule in one place is why: this clamped as well
    // until story #947's review, so a Kotlin host and a C host got different
    // answers to the same question. What is refused is only what a `u16` cannot
    // carry to the ABI at all — a truncating cast would turn 65 936 into 400
    // and the ABI would accept it.
    let weight = env.get_field(&face, WEIGHT, jni_sig!("I"))?.i()?;
    let Ok(weight) = u16::try_from(weight) else {
        log(&format!(
            "nativeSurfaceCreatedWithText: face {index} declares weight {weight}, which does \
             not fit the descriptor; the accepted range is 1..=1000"
        ));
        return Ok(None);
    };
    // `face_index` is a `u32` in the descriptor and a signed `int` in Java, so
    // the only value that cannot cross is a negative one. Refused rather than
    // clamped: a negative index is a caller's mistake, and 0 is a real face.
    let face_index = env.get_field(&face, FACE_INDEX, jni_sig!("I"))?.i()?;
    let Ok(face_index) = u32::try_from(face_index) else {
        log(&format!(
            "nativeSurfaceCreatedWithText: face {index} declares faceIndex {face_index}, which \
             is not an index"
        ));
        return Ok(None);
    };

    // The diagnostic names the field by reading the same value the lookup uses,
    // rather than repeating it as a literal. A second spelling here would
    // survive a rename and name a field that no longer exists, on the exact
    // path a reader consults when the load has failed.
    let mut bytes_field = |name: &JNIStr| -> jni::errors::Result<Option<Vec<u8>>> {
        let array = env.get_field(&face, name, jni_sig!("[B"))?.l()?;
        let array: JByteArray = env.cast_local::<JByteArray>(array)?;
        if array.is_null() {
            log(&format!(
                "nativeSurfaceCreatedWithText: face {index} has no {}",
                name.to_str()
            ));
            return Ok(None);
        }
        Ok(Some(env.convert_byte_array(&array)?))
    };
    // Three bindings rather than one tuple, so a face refused on its first
    // field stops there. A tuple evaluates all three whatever the first
    // answers: it would copy the remaining arrays out of the JVM for a face
    // already refused — 63 940 B of PNG and 4 448 B of metrics on the
    // harness's own cascade — and log three lines for one bad face, which
    // reads as three problems.
    let Some(font) = bytes_field(FONT)? else {
        return Ok(None);
    };
    let Some(atlas_png) = bytes_field(ATLAS_PNG)? else {
        return Ok(None);
    };
    let Some(atlas_metrics) = bytes_field(ATLAS_METRICS)? else {
        return Ok(None);
    };

    let Ok(family) = std::ffi::CString::new(family) else {
        log(&format!(
            "nativeSurfaceCreatedWithText: face {index} has a family name containing a NUL"
        ));
        return Ok(None);
    };
    Ok(Some(OwnedFace {
        family,
        weight,
        face_index,
        font,
        atlas_png,
        atlas_metrics,
    }))
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
