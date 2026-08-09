//! The JNI entry points, the render thread, and the `AChoreographer` loop.
//!
//! Compiled on Android and nowhere else. Every function here binds an NDK or JNI
//! symbol, so nothing in this module can be reached by `cargo test` — which is
//! why [`crate::Handshake`], the part that can be wrong without a device, is not
//! in it.
//!
//! # The threads, and which one does what
//!
//! Two, and the split is D4's and D6's between them.
//!
//! The **UI thread** owns the view and receives `surfaceCreated`,
//! `surfaceChanged` and `surfaceDestroyed`. It is the only thread that may call
//! `ANativeWindow_fromSurface` — the call needs a `JNIEnv` and a live `jobject`,
//! both of which are that thread's — and it is the thread that must not return
//! from `surfaceDestroyed` early.
//!
//! The **render thread** owns the runtime: the arena, the scene, the painter and
//! the surface. It prepares a looper, takes vsync from `AChoreographer`, and
//! ticks and draws. Nothing producer-side runs on it that the host did not ask
//! for, which is P3.
//!
//! The `ANativeWindow *` crosses between them exactly once per surface, as a
//! `usize` inside [`Spawn`]. It is reference-counted, and this crate holds a
//! reference of its own from `ANativeWindow_acquire` until the handshake
//! completes — so the pointer the render thread holds stays valid even if the
//! framework's own reference goes early.
//!
//! # Why the runtime is created on the render thread
//!
//! `wgpu`'s device and queue are used from one thread here, and the simplest way
//! to guarantee that is for the thread that draws to be the thread that built
//! them. The alternative — creating the runtime on the UI thread and sending it
//! — would put a `DsRuntime *` across a thread boundary for no gain, and the ABI
//! is explicit that no two calls may be in flight on one runtime.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use dashscene_ffi::{DsRuntime, DsStatus, DsSurfaceKind};
use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JByteArray, JClass, JObject};
use jni::sys::{jboolean, jint, jlong};

use crate::Handshake;

/// The tag every log line from this crate carries.
const TAG: &str = "dashscene";

/// What one live view amounts to, as the UI thread holds it.
///
/// Handed to Kotlin as a `jlong` and handed back on every call. Opaque there,
/// exactly as `DsRuntime *` is opaque to C.
struct Host {
    handshake: Arc<Handshake>,
    /// The extent the UI thread last reported, read by the render thread on the
    /// frame after it changes. An atomic pair rather than a lock, because it is
    /// written from one thread, read from one thread, and never needs to be
    /// consistent with anything else.
    width: Arc<AtomicU32>,
    height: Arc<AtomicU32>,
    render: Option<std::thread::JoinHandle<()>>,
    /// The window this crate acquired a reference to, released after the
    /// handshake completes.
    window: usize,
}

/// What the render thread is given at spawn.
///
/// Everything crosses the boundary here, once, rather than through a channel:
/// the surface's life is bounded by the thread's, so there is nothing to send
/// afterwards except the teardown request, which is the handshake's.
struct Spawn {
    window: usize,
    width: Arc<AtomicU32>,
    height: Arc<AtomicU32>,
    handshake: Arc<Handshake>,
    document: Vec<u8>,
}

/// The render thread's own state, reachable from the vsync callback.
struct Frame {
    runtime: *mut DsRuntime,
    width: Arc<AtomicU32>,
    height: Arc<AtomicU32>,
    /// The extent the surface is currently configured for.
    configured: (u32, u32),
    /// The timestamp of the previous vsync, for the frame delta.
    previous: Option<i64>,
    /// Why the next frame must draw whatever the tick says. The ABI's
    /// `ds_runtime_detach_surface` documents this obligation: the scene did not
    /// change while the surface was gone and the new device has drawn nothing,
    /// so the gate has to be overridden once. Both other hosts carry the same
    /// flag.
    forced: bool,
    /// Cleared by the callback when the loop should stop rescheduling.
    running: bool,
    /// Vsync callbacks seen, and frames actually drawn. Reported periodically,
    /// because "the surface attached" and "the loop is running" are different
    /// claims and only the second one is what a picture depends on.
    vsyncs: u64,
    draws: u64,
}

fn log(message: &str) {
    // `println!` reaches logcat on Android through the runtime's own stdout
    // redirection only when the app enables it, so this writes to the log
    // directly. `__android_log_write` takes a NUL-terminated string.
    let Ok(text) = std::ffi::CString::new(message) else {
        return;
    };
    let Ok(tag) = std::ffi::CString::new(TAG) else {
        return;
    };
    // SAFETY: both pointers are NUL-terminated and live for the call.
    unsafe {
        ndk_sys::__android_log_write(
            ndk_sys::android_LogPriority::ANDROID_LOG_INFO.0 as std::os::raw::c_int,
            tag.as_ptr(),
            text.as_ptr(),
        );
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

/// The frame loop, on its own thread.
///
/// Builds the runtime, loads the document, attaches the surface, and then hands
/// itself to `AChoreographer` until the handshake asks it to stop.
fn render_thread(spawn: Spawn) {
    let Spawn {
        window,
        width,
        height,
        handshake,
        document,
    } = spawn;

    // A looper is what `AChoreographer` posts its callbacks to, and a thread
    // that has not prepared one has no choreographer to get.
    // SAFETY: called once, on this thread, before any other looper call.
    unsafe { ndk_sys::ALooper_prepare(0) };

    let mut runtime: *mut DsRuntime = std::ptr::null_mut();
    // SAFETY: `runtime` is a valid writable out-pointer.
    if unsafe { dashscene_ffi::ds_runtime_new(&mut runtime) } != DsStatus::Ok {
        log(&format!("ds_runtime_new failed: {}", last_error()));
        handshake.released();
        return;
    }

    // SAFETY: `runtime` is live, and `document` is a readable slice.
    let loaded = unsafe {
        dashscene_ffi::ds_runtime_load_document(runtime, document.as_ptr(), document.len())
    };
    if loaded != DsStatus::Ok {
        log(&format!(
            "load_document failed: {:?} {}",
            loaded,
            last_error()
        ));
        // SAFETY: `runtime` came from `ds_runtime_new` and nothing else holds it.
        unsafe { dashscene_ffi::ds_runtime_free(runtime) };
        handshake.released();
        return;
    }

    let extent = (
        width.load(Ordering::Acquire),
        height.load(Ordering::Acquire),
    );
    // SAFETY: `window` is a live `ANativeWindow *` — this crate holds a
    // reference to it from `ANativeWindow_acquire`, released only after the
    // handshake completes, which is exactly the lifetime
    // `ds_runtime_attach_surface` asks for.
    let attached = unsafe {
        dashscene_ffi::ds_runtime_attach_surface(
            runtime,
            DsSurfaceKind::AndroidNdk as i32,
            window as *mut c_void,
            std::ptr::null_mut(),
            extent.0,
            extent.1,
        )
    };
    if attached != DsStatus::Ok {
        log(&format!(
            "attach_surface failed: {:?} {}",
            attached,
            last_error()
        ));
        // SAFETY: as above.
        unsafe { dashscene_ffi::ds_runtime_free(runtime) };
        handshake.released();
        return;
    }
    log(&format!(
        "attached a {}x{} surface — dashscene ABI {}",
        extent.0,
        extent.1,
        dashscene_ffi::ds_abi_version()
    ));

    let mut frame = Frame {
        runtime,
        width,
        height,
        configured: extent,
        previous: None,
        // The first frame is one of the cases the generation cannot report.
        forced: true,
        running: true,
        vsyncs: 0,
        draws: 0,
    };

    // SAFETY: this thread has prepared a looper, which is what `getInstance`
    // requires.
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
        log("AChoreographer_getInstance returned null");
    } else {
        post_vsync(choreographer, &mut frame);
        // The loop is the looper's: `pollOnce` dispatches the vsync callback,
        // which draws and re-posts. The teardown check sits between polls
        // rather than inside the callback, so a request is never waiting on a
        // frame that is mid-flight.
        while frame.running && !handshake.teardown_requested() {
            // A 100 ms timeout rather than an indefinite wait, so the teardown
            // check runs even if vsync stops arriving — which is what a
            // surface that has gone away looks like from here.
            // SAFETY: called on the thread that prepared the looper.
            unsafe {
                ndk_sys::ALooper_pollOnce(
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
        }
    }

    // D4's ordering, and the only ordering that is correct: stop drawing, drop
    // the surface, and only then tell the UI thread it may release the window.
    //
    // SAFETY: `runtime` is live and no other call is in flight on it — this is
    // the only thread that has ever touched it, and the vsync callback cannot
    // run while this thread is here rather than in `pollOnce`.
    unsafe { dashscene_ffi::ds_runtime_detach_surface(runtime, std::ptr::null_mut()) };
    // SAFETY: as above; nothing else holds this pointer.
    unsafe { dashscene_ffi::ds_runtime_free(runtime) };
    log("surface detached and runtime freed");
    // Last, and only here. Everything the UI thread is waiting on is done.
    handshake.released();
}

/// Posts the next vsync callback.
fn post_vsync(choreographer: *mut ndk_sys::AChoreographer, frame: *mut Frame) {
    // SAFETY: `choreographer` is this thread's instance, and `frame` outlives
    // every callback — it is a local of `render_thread`, which does not return
    // until the loop has stopped.
    unsafe {
        ndk_sys::AChoreographer_postVsyncCallback(choreographer, Some(on_vsync), frame.cast());
    }
}

/// One frame, on the render thread, driven by vsync.
///
/// `AChoreographer_postVsyncCallback` is `__INTRODUCED_IN(33)` and the link
/// level is 33, so it is reachable unconditionally — no runtime API guard and no
/// `postFrameCallback64` fallback branch. That is the whole consequence of the
/// API floor story #862 set.
unsafe extern "C" fn on_vsync(
    data: *const ndk_sys::AChoreographerFrameCallbackData,
    user: *mut c_void,
) {
    let frame = user.cast::<Frame>();
    if frame.is_null() {
        return;
    }
    // SAFETY: `user` is the `*mut Frame` handed to `post_vsync`, which points at
    // a local of `render_thread` that outlives every callback.
    let frame = unsafe { &mut *frame };

    frame.vsyncs += 1;
    if frame.vsyncs == 1 {
        log("first vsync callback");
    }

    // SAFETY: `data` is the callback's own argument, valid for this call.
    let now = unsafe { ndk_sys::AChoreographerFrameCallbackData_getFrameTimeNanos(data) };
    let dt = match frame.previous {
        // Nanoseconds to seconds. Raw from here: `LiveScene::tick` applies both
        // the ceiling and the floor, so the rule has one statement rather than
        // one per host (story #810).
        Some(previous) => (now - previous) as f32 / 1_000_000_000.0,
        None => 0.0,
    };
    frame.previous = Some(now);

    // The extent the UI thread last reported. Checked every frame rather than
    // through a message, because `surfaceChanged` and this loop are on
    // different threads and a message would need a channel for one `u32` pair.
    let wanted = (
        frame.width.load(Ordering::Acquire),
        frame.height.load(Ordering::Acquire),
    );
    if wanted != frame.configured && wanted.0 > 0 && wanted.1 > 0 {
        // SAFETY: `runtime` is live and this is the only thread calling it.
        let resized =
            unsafe { dashscene_ffi::ds_runtime_resize(frame.runtime, wanted.0, wanted.1) };
        if resized == DsStatus::Ok {
            frame.configured = wanted;
            // A reconfigured swapchain has drawn nothing, and the generation
            // cannot report that.
            frame.forced = true;
        } else {
            log(&format!("resize failed: {:?} {}", resized, last_error()));
        }
    }

    let mut advanced = false;
    // SAFETY: `runtime` is live; `advanced` is a valid out-pointer.
    let ticked = unsafe { dashscene_ffi::ds_runtime_tick(frame.runtime, dt, &mut advanced) };
    if ticked != DsStatus::Ok {
        log(&format!("tick failed: {:?} {}", ticked, last_error()));
        frame.running = false;
        return;
    }

    if advanced || frame.forced {
        frame.forced = false;
        // SAFETY: `runtime` is live and no other call is in flight.
        let drawn = unsafe { dashscene_ffi::ds_runtime_draw(frame.runtime, std::ptr::null_mut()) };
        if drawn != DsStatus::Ok {
            log(&format!("draw failed: {:?} {}", drawn, last_error()));
            frame.running = false;
            return;
        }
        frame.draws += 1;
        if frame.draws == 1 {
            log("first frame drawn");
        } else if frame.draws % 120 == 0 {
            log(&format!(
                "{} frames drawn over {} vsyncs",
                frame.draws, frame.vsyncs
            ));
        }
    }

    // Rescheduled only while the loop is running, so a frame that failed does
    // not queue another.
    if frame.running {
        // SAFETY: this thread prepared the looper, so it has an instance.
        let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
        if !choreographer.is_null() {
            post_vsync(choreographer, frame);
        }
    }
}

/// Creates a host and starts its render thread.
///
/// `surface` is the `android.view.Surface` the view handed over; `document` is
/// the `.dsb` bytes. Returns an opaque handle, or 0 on failure.
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
            // SAFETY: `env` and `surface` are the JVM's own, valid for this
            // call.
            let window = unsafe {
                ndk_sys::ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast())
            };
            if window.is_null() {
                log("ANativeWindow_fromSurface returned null");
                return Ok(0);
            }
            Ok(start(window, bytes, width, height))
        })
        .resolve::<LogErrorAndDefault>()
}

/// Acquires the window, starts the render thread, and boxes the host.
///
/// Split out of the JNI entry point so that everything past the `JNIEnv` — which
/// is the only part that belongs to the UI thread's environment — is ordinary
/// Rust.
fn start(window: *mut ndk_sys::ANativeWindow, bytes: Vec<u8>, width: jint, height: jint) -> jlong {
    // `fromSurface` already acquires a reference. This second one is this
    // crate's own, and it is what makes the pointer's life *ours* rather than
    // the framework's: it is released after the handshake completes, so the
    // render thread's surface can never outlive the window it was built from.
    //
    // SAFETY: `window` is non-null and was just obtained.
    unsafe { ndk_sys::ANativeWindow_acquire(window) };

    let handshake = Arc::new(Handshake::new());
    let width_cell = Arc::new(AtomicU32::new(width.max(0) as u32));
    let height_cell = Arc::new(AtomicU32::new(height.max(0) as u32));

    let spawn = Spawn {
        window: window as usize,
        width: Arc::clone(&width_cell),
        height: Arc::clone(&height_cell),
        handshake: Arc::clone(&handshake),
        document: bytes,
    };
    let render = std::thread::Builder::new()
        .name("dashscene-frame".to_owned())
        .spawn(move || render_thread(spawn));
    let render = match render {
        Ok(render) => render,
        Err(error) => {
            log(&format!("could not start the frame thread: {error}"));
            // SAFETY: both references this crate holds are given back.
            unsafe {
                ndk_sys::ANativeWindow_release(window);
                ndk_sys::ANativeWindow_release(window);
            }
            return 0;
        }
    };

    let host = Box::new(Host {
        handshake,
        width: width_cell,
        height: height_cell,
        render: Some(render),
        window: window as usize,
    });
    Box::into_raw(host) as jlong
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
    if handle == 0 {
        return;
    }
    // SAFETY: `handle` is a pointer from `nativeSurfaceCreated`, which the
    // caller promises is still live.
    let host = unsafe { &*(handle as *const Host) };
    host.width.store(width.max(0) as u32, Ordering::Release);
    host.height.store(height.max(0) as u32, Ordering::Release);
}

/// **The destroy handshake.** Blocks until rendering has stopped and the surface
/// has been dropped, then releases the window.
///
/// This is what `surfaceDestroyed` calls, and it must not return early: when the
/// callback returns, the framework's Surface is invalid, and a render thread
/// still holding a `wgpu::Surface` built from it is a use-after-free on
/// rotation, backgrounding and split-screen.
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
    if handle == 0 {
        return;
    }
    // SAFETY: `handle` came from `Box::into_raw` in `nativeSurfaceCreated`, and
    // the caller promises this is its last use.
    let mut host = unsafe { Box::from_raw(handle as *mut Host) };

    // Blocks. This is the whole point of the call.
    host.handshake.request_teardown();
    // The thread has acknowledged, so joining is bounded — and joining rather
    // than trusting the acknowledgement alone means the thread's own stack, and
    // anything still on it, is gone before the window is released.
    if let Some(render) = host.render.take()
        && render.join().is_err()
    {
        log("the frame thread panicked before it stopped");
    }

    // Only now. Both references — the one `fromSurface` took and the one this
    // crate acquired — are given back, and the window may go.
    //
    // SAFETY: this crate holds exactly two references to `window`, and nothing
    // uses the pointer after this.
    unsafe {
        let window = host.window as *mut ndk_sys::ANativeWindow;
        ndk_sys::ANativeWindow_release(window);
        ndk_sys::ANativeWindow_release(window);
    }
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

/// Whether the frame thread is still running, for a host that wants to report
/// it. Not part of the lifecycle.
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
    if handle == 0 {
        return false;
    }
    // SAFETY: `handle` is a pointer from `nativeSurfaceCreated`.
    let host = unsafe { &*(handle as *const Host) };
    !host.handshake.teardown_requested()
}
