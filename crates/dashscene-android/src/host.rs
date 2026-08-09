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
//! `usize` inside [`Spawn`]. It is reference-counted, and the reference
//! `ANativeWindow_fromSurface` returns belongs to this crate: it is released
//! only after the handshake completes, so the pointer the render thread holds
//! stays valid for as long as the surface built from it.
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

/// Releases the handshake however this thread leaves.
///
/// A `Drop` guard rather than a call on each path, because the paths are not
/// the whole set: a panic anywhere in the render thread unwinds past every
/// explicit `released()`, and the UI thread is parked in `request_teardown`
/// waiting for one. That wait has no timeout — deliberately, since a timeout
/// would mean returning from `surfaceDestroyed` with a live surface — so a
/// missed release is an application-not-responding kill rather than a bad
/// frame.
struct ReleaseOnExit(Arc<Handshake>);

impl Drop for ReleaseOnExit {
    fn drop(&mut self) {
        self.0.released();
    }
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

    // Armed before anything that can fail, so every exit from here on releases
    // the UI thread.
    let _release = ReleaseOnExit(Arc::clone(&handshake));

    // A looper is what `AChoreographer` posts its callbacks to, and a thread
    // that has not prepared one has no choreographer to get.
    // SAFETY: called once, on this thread, before any other looper call.
    unsafe { ndk_sys::ALooper_prepare(0) };

    let mut runtime: *mut DsRuntime = std::ptr::null_mut();
    // SAFETY: `runtime` is a valid writable out-pointer.
    if unsafe { dashscene_ffi::ds_runtime_new(&mut runtime) } != DsStatus::Ok {
        log(&format!("ds_runtime_new failed: {}", last_error()));
        return;
    }

    // SAFETY: `runtime` is live, and `document` is a readable slice.
    let loaded = unsafe {
        dashscene_ffi::ds_runtime_load_document(runtime, document.as_ptr(), document.len())
    };
    // The load is the owning path — `ds_runtime_load_document` documents that
    // `dashscene_core::load_document` copies every payload — so these bytes are
    // dead from here. Dropped rather than carried for the life of the thread,
    // because the Java side is holding a copy too.
    drop(document);

    if loaded != DsStatus::Ok {
        log(&format!(
            "load_document failed: {:?} {}",
            loaded,
            last_error()
        ));
        // SAFETY: `runtime` came from `ds_runtime_new` and nothing else holds it.
        unsafe { dashscene_ffi::ds_runtime_free(runtime) };
        return;
    }

    let extent = (
        width.load(Ordering::Acquire),
        height.load(Ordering::Acquire),
    );
    // SAFETY: `window` is a live `ANativeWindow *` — this crate holds the
    // reference `ANativeWindow_fromSurface` returned, released only after the
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
        return;
    }
    log(&format!(
        "attached a {}x{} surface — dashscene ABI {}",
        extent.0,
        extent.1,
        dashscene_ffi::ds_abi_version()
    ));

    // **Leaked, deliberately, and this is the fix for a use-after-free.**
    // `on_vsync` re-posts itself before the loop notices a teardown request, so
    // when the loop exits there is almost always a callback still registered
    // with the choreographer — and nothing can cancel one. A `Frame` on this
    // thread's stack would die while that callback still pointed at it, and the
    // `DsRuntime` it names would already be freed. Leaking it costs a few dozen
    // bytes per surface cycle and makes the pending callback's read valid; the
    // fields below are what tell it there is nothing left to do.
    let frame: &'static mut Frame = Box::leak(Box::new(Frame {
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
    }));

    // SAFETY: this thread has prepared a looper, which is what `getInstance`
    // requires.
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
        // No looper instance means no frame will ever run. Reported and torn
        // down rather than left as a live handle over a loop that does not
        // exist: `Handshake::is_running` answers `false` once this returns,
        // which is what a host asking about it reads.
        log("AChoreographer_getInstance returned null — no frame loop");
    } else {
        handshake.started();
        post_vsync(choreographer, frame);
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

    // Told first, and before the runtime is freed: a callback the choreographer
    // still holds reads these and returns without touching anything. `running`
    // stops it rescheduling, and the null runtime is what makes a callback that
    // slips through harmless rather than a use-after-free.
    frame.running = false;
    frame.runtime = std::ptr::null_mut();

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
    // The release happens as `_release` drops, immediately after this returns —
    // last, and after everything the UI thread is waiting on is done.
}

/// Posts the next vsync callback.
fn post_vsync(choreographer: *mut ndk_sys::AChoreographer, frame: *mut Frame) {
    // SAFETY: `choreographer` is this thread's instance, and `frame` points at a
    // leaked allocation, so it outlives every callback including one still
    // posted after the loop has ended. A posted vsync callback cannot be
    // cancelled, so outliving the loop is the property this needs — not the
    // other way round.
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
    // SAFETY: `user` is the `*mut Frame` handed to `post_vsync`. That allocation
    // is leaked by `render_thread`, so it stays readable even after the loop has
    // ended and the thread has gone — which is the state this callback can
    // legitimately arrive in, because a posted vsync cannot be cancelled.
    let frame = unsafe { &mut *frame };

    // The loop has ended and the runtime has been freed. Nothing to draw into,
    // and nothing to reschedule.
    if !frame.running || frame.runtime.is_null() {
        return;
    }

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
/// the `.dsb` bytes.
///
/// Returns an opaque handle, or 0 if the window or the thread could not be
/// obtained. **A non-zero handle does not mean the runtime started**: acquiring
/// an adapter and a device takes on the order of a second, and blocking the UI
/// thread inside `surfaceCreated` for that long is an
/// application-not-responding risk. So the handle is returned as soon as the
/// thread is spawned, and whether the loop came up is asked separately through
/// `nativeIsRunning`. A handle whose runtime failed is still a valid handle and
/// must still be passed to `nativeSurfaceDestroyed`.
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
    // No second `ANativeWindow_acquire`. `ANativeWindow_fromSurface` already
    // returns a reference this crate owns and must release, and that one
    // reference is what keeps the pointer alive until the handshake completes.
    // A second acquire would only add a count to keep balanced across four
    // sites for no behavioural gain — and a later reader trimming one release
    // as a duplicate would leak the window, or trimming both would underflow.

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
            // SAFETY: the one reference `fromSurface` gave this crate.
            unsafe { ndk_sys::ANativeWindow_release(window) };
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

    // Only now, and the ordering is D4's: the render thread has stopped and its
    // surface is dropped, so releasing the window cannot pull it from under a
    // live surface.
    //
    // SAFETY: this crate holds exactly one reference to `window` — the one
    // `ANativeWindow_fromSurface` returned — and nothing uses the pointer after
    // this.
    unsafe { ndk_sys::ANativeWindow_release(host.window as *mut ndk_sys::ANativeWindow) };
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

/// Whether the frame loop is still live, for a host that wants to report it.
/// Not part of the lifecycle.
///
/// Answers [`Handshake::is_running`] rather than the negation of
/// "was teardown requested". A loop that stopped on its own — a failed tick or
/// draw, or a choreographer this thread could not get — reaches its end without
/// anyone having asked, and the negated question would call that still running,
/// which is exactly the case a caller asks this in.
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
    host.handshake.is_running()
}
