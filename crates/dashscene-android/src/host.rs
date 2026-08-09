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
use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JByteArray, JClass, JObject};
use jni::sys::{jboolean, jint, jlong};

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

/// A compiled `.dsb`, drawn through the C ABI.
struct DocumentFrames {
    runtime: *mut DsRuntime,
    /// The bytes, held only until the load. The ABI's load is the **owning**
    /// path — `dashscene_core::load_document` copies every payload — so they are
    /// dead weight afterwards and are dropped there.
    document: Option<Vec<u8>>,
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

        let Some(document) = self.document.take() else {
            return Err("no document bytes".to_owned());
        };
        // SAFETY: `runtime` is live and `document` is a readable slice.
        let loaded = unsafe {
            dashscene_ffi::ds_runtime_load_document(runtime, document.as_ptr(), document.len())
        };
        drop(document);
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
            // `DsStatus::Surface` is every surface failure flattened into one
            // status — the ABI cannot say which, because `FrameError` does not
            // cross it. Rebuilding is the remedy for the recoverable half and
            // is harmless for the rest, which the loop's own bound on
            // consecutive rebuilds is what makes true. Issue #884 carries
            // giving the ABI the distinction.
            return if drawn == DsStatus::Surface {
                Step::Rebuild
            } else {
                Step::Stop
            };
        }
        Step::Continue
    }

    fn detach(&mut self) {
        // Tolerates being called after a failed `attach`, which the loop does,
        // and after a previous detach on the rebuild path.
        self.document = None;
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

/// Creates a host that draws a compiled `.dsb`, and starts its frame loop.
///
/// `surface` is the `android.view.Surface` the view handed over; `document` is
/// the `.dsb` bytes.
///
/// Returns an opaque handle, or 0 if the window or the thread could not be
/// obtained. **A non-zero handle does not mean the runtime started** — see
/// [`crate::loop_::start`] for why that is deliberate — and `nativeIsRunning` is
/// what answers it.
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
            // The one call that must happen on this thread: it needs the
            // `JNIEnv` and the live `jobject`, and both are the UI thread's.
            //
            // SAFETY: `env` and `surface` are the JVM's own, valid for this call.
            let window = unsafe {
                ndk_sys::ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast())
            };
            if window.is_null() {
                log("ANativeWindow_fromSurface returned null");
                return Ok(0);
            }
            // A factory: the value is built on the render thread. Only the
            // bytes cross, and a `Vec<u8>` is `Send`.
            let frames = move || -> Box<dyn Frames> {
                Box::new(DocumentFrames {
                    runtime: std::ptr::null_mut(),
                    document: Some(bytes),
                })
            };
            // SAFETY: `window` is the reference `fromSurface` returned, which
            // this crate owns until the handshake completes.
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
