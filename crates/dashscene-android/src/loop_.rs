//! The render thread, the looper and the `AChoreographer` callback (D6).
//!
//! Compiled on Android and nowhere else — every function here binds an NDK
//! symbol. What it drives is [`Frames`], so the loop is written once and the
//! scene source is the implementation's business; see [`crate::frames`] for why
//! that line is drawn where it is.
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
//! The **render thread** owns whatever draws: the arena, the scene, the painter
//! and the surface, all inside the [`Frames`] implementation. It prepares a
//! looper, takes vsync, and ticks and draws. Nothing producer-side runs on it
//! that the host did not put there, which is P3.
//!
//! The `ANativeWindow *` crosses between them exactly once per surface, as a
//! `usize`. The reference `ANativeWindow_fromSurface` returns belongs to this
//! crate and is released only after the handshake completes, so the pointer the
//! render thread holds stays valid for as long as the surface built from it.
//!
//! # Why the surface is taken up on the render thread
//!
//! `wgpu`'s device and queue are used from one thread, and the simplest way to
//! guarantee that is for the thread that draws to be the thread that built them.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::frames::Frames;
use crate::machine::{Action, LoopState, pack, unpack};
use crate::{Handshake, log};

/// One live view, as the UI thread holds it.
///
/// Handed to a host language as an opaque pointer and handed back on every call.
pub struct AndroidHost {
    handshake: Arc<Handshake>,
    /// The extent the UI thread last reported, read by the render thread on the
    /// frame after it changes. An atomic pair rather than a lock: written from
    /// one thread, read from one thread, and never needing to be consistent
    /// with anything else.
    /// The extent the UI thread last reported, packed into one word.
    ///
    /// **One atomic, not two.** Stored as two, `surfaceChanged` interleaving
    /// between the loop's two loads yields a (new width, old height) pair that
    /// never existed — which passes the changed-and-non-zero test, is accepted,
    /// is recorded as configured, and costs a full scene rebuild and a
    /// swapchain reconfigure at an aspect ratio nothing asked for.
    extent: Arc<AtomicU64>,
    render: Option<std::thread::JoinHandle<()>>,
    /// The window this crate holds a reference to, released after the handshake
    /// completes.
    window: usize,
}

/// Releases the handshake however this thread leaves.
///
/// A `Drop` guard rather than a call on each path, because the paths are not the
/// whole set: a panic anywhere unwinds past every explicit `released()`, and the
/// UI thread is parked in `request_teardown` waiting for one. That wait has no
/// timeout — deliberately, since a timeout means returning from
/// `surfaceDestroyed` with a live surface — so a missed release is an
/// application-not-responding kill rather than a bad frame.
struct ReleaseOnExit(Arc<Handshake>);

impl Drop for ReleaseOnExit {
    fn drop(&mut self) {
        self.0.released();
    }
}

/// Starts the frame loop for `frames` on `window`.
///
/// `frames` is a **factory**, not a value, and is called on the render thread. A
/// [`Frames`] implementation owns an `Arena` and a `LiveScene`, which hold a
/// boxed solver and boxed closures that are not `Send`, so the value cannot
/// cross the thread boundary — only the factory can. Building it there is also
/// what keeps `wgpu`'s device and queue on the thread that draws.
///
/// Returns an opaque host pointer. **A non-null return does not mean the loop
/// came up**: taking up a surface means acquiring an adapter and a device, which
/// takes on the order of a second, and blocking the UI thread inside
/// `surfaceCreated` for that long is an application-not-responding risk. So this
/// returns as soon as the thread is spawned, and whether the loop is live is
/// asked separately through [`is_running`].
///
/// # Safety
///
/// `window` must be a live `ANativeWindow *` whose reference this crate owns.
/// The caller must pass the returned pointer to [`destroy`] exactly once.
pub unsafe fn start<F>(window: *mut c_void, frames: F, width: u32, height: u32) -> *mut AndroidHost
where
    F: FnOnce() -> Box<dyn Frames> + Send + 'static,
{
    let handshake = Arc::new(Handshake::new());
    let extent_cell = Arc::new(AtomicU64::new(pack(width, height)));

    let spawn_window = window as usize;
    let spawn = (Arc::clone(&handshake), Arc::clone(&extent_cell));
    let render = std::thread::Builder::new()
        .name("dashscene-frame".to_owned())
        .spawn(move || {
            let (handshake, extent) = spawn;
            render_thread(spawn_window, frames, handshake, extent);
        });
    let render = match render {
        Ok(render) => render,
        Err(error) => {
            log(&format!("could not start the frame thread: {error}"));
            return std::ptr::null_mut();
        }
    };

    Box::into_raw(Box::new(AndroidHost {
        handshake,
        extent: extent_cell,
        render: Some(render),
        window: window as usize,
    }))
}

/// Reports a new **physical**-pixel extent. Picked up by the next frame.
///
/// # Safety
///
/// `host` must be a live pointer from [`start`].
pub unsafe fn resize(host: *mut AndroidHost, width: u32, height: u32) {
    if host.is_null() {
        return;
    }
    // SAFETY: the caller promises `host` is live.
    let host = unsafe { &*host };
    host.extent.store(pack(width, height), Ordering::Release);
}

/// Whether the frame loop is still live.
///
/// # Safety
///
/// `host` must be a live pointer from [`start`].
pub unsafe fn is_running(host: *const AndroidHost) -> bool {
    if host.is_null() {
        return false;
    }
    // SAFETY: the caller promises `host` is live.
    unsafe { &*host }.handshake.is_running()
}

/// **The destroy handshake.** Blocks until the loop has stopped and the surface
/// has been dropped, then releases the window.
///
/// This is what `surfaceDestroyed` calls, and it must not return early: when
/// that callback returns the framework's Surface is invalid, and a render thread
/// still holding a surface built from it is a use-after-free on rotation,
/// backgrounding and split-screen.
///
/// # Safety
///
/// `host` must be a live pointer from [`start`], and must not be used again.
pub unsafe fn destroy(host: *mut AndroidHost) {
    if host.is_null() {
        return;
    }
    // SAFETY: the caller promises this is `host`'s last use.
    let mut host = unsafe { Box::from_raw(host) };

    // Blocks. This is the whole point of the call.
    host.handshake.request_teardown();
    // Joining as well as waiting on the acknowledgement: the thread's own stack,
    // and anything still on it, is gone before the window is released.
    if let Some(render) = host.render.take()
        && render.join().is_err()
    {
        log("the frame thread panicked before it stopped");
    }

    // Only now, and the ordering is D4's.
    //
    // SAFETY: this crate holds exactly one reference to `window` — the one
    // `ANativeWindow_fromSurface` returned — and nothing uses it after this.
    unsafe { ndk_sys::ANativeWindow_release(host.window as *mut ndk_sys::ANativeWindow) };
}

/// The frame loop, on its own thread.
fn render_thread<F>(
    window: usize,
    frames: F,
    handshake: Arc<Handshake>,
    extent_cell: Arc<AtomicU64>,
) where
    F: FnOnce() -> Box<dyn Frames>,
{
    // Armed before anything that can fail, so every exit from here on releases
    // the UI thread.
    let _release = ReleaseOnExit(Arc::clone(&handshake));

    // A looper is what `AChoreographer` posts its callbacks to, and a thread
    // that has not prepared one has no choreographer to get.
    // SAFETY: called once, on this thread, before any other looper call.
    unsafe { ndk_sys::ALooper_prepare(0) };

    // Built here, on the thread that will use it, for the reason `start`
    // records.
    let mut frames = frames();

    let extent = unpack(extent_cell.load(Ordering::Acquire));
    // SAFETY: `window` is live — this crate holds the reference
    // `ANativeWindow_fromSurface` returned, released only after the handshake
    // completes, which is exactly the lifetime `Frames::attach` is promised.
    let attached = unsafe { frames.attach(window as *mut c_void, extent.0, extent.1) };
    if let Err(error) = attached {
        log(&format!("attach failed: {error}"));
        // `detach` is not called: `attach` failed, so there is nothing to drop,
        // and calling it would ask an implementation to undo what it never did.
        return;
    }
    log(&format!("attached a {}x{} surface", extent.0, extent.1));

    // **Leaked, deliberately, and this is a use-after-free fix rather than an
    // oversight.** `on_vsync` re-posts itself before the loop notices a teardown
    // request, so the loop almost always exits with a callback still registered
    // — and a posted vsync cannot be cancelled. A `Loop` on this thread's stack
    // would die while that callback still pointed at it, so it is leaked and
    // `running` is what tells a late callback there is nothing left to do.
    //
    // **What is leaked is this struct, which holds the `Frames` box.** That is
    // why `Frames::detach` is required to release what the implementation owns:
    // a scene left inside here is retained for the life of the process, once per
    // surface cycle, and on Android a surface cycle is every rotation. The
    // struct itself is tens of bytes; what an implementation leaves in it is
    // not bounded by anything this crate can enforce.
    let state: &'static mut LoopState = Box::leak(Box::new(LoopState::new(
        frames,
        window,
        extent_cell,
        extent,
    )));

    // SAFETY: this thread prepared a looper, which is what `getInstance` needs.
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
        // No instance means no frame will ever run. Torn down rather than left
        // as a live handle over a loop that does not exist.
        log("AChoreographer_getInstance returned null — no frame loop");
    } else {
        handshake.started();
        post_vsync(choreographer, state);
        // The loop is the looper's: `pollOnce` dispatches the vsync callback,
        // which draws and re-posts. The teardown check sits between polls rather
        // than inside the callback, so a request never waits on a frame that is
        // mid-flight.
        while state.is_running() && !handshake.teardown_requested() {
            // A 100 ms timeout rather than an indefinite wait, so the teardown
            // check runs even if vsync stops arriving — which is what a surface
            // that has gone away looks like from here.
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

    // D4's ordering, and the only one that is correct: tell a callback the
    // choreographer may still hold that there is nothing left to do, drop the
    // surface, and only then let the UI thread release the window. The first two
    // are `shut_down`'s, which holds them together where they can be read as one
    // rule; the release happens as `_release` drops, immediately after this
    // returns.
    state.shut_down();
    log("surface detached");
}

/// Posts the next vsync callback.
fn post_vsync(choreographer: *mut ndk_sys::AChoreographer, state: *mut LoopState) {
    // SAFETY: `choreographer` is this thread's instance, and `state` points at a
    // leaked allocation, so it outlives every callback including one still
    // posted after the loop has ended. A posted vsync cannot be cancelled, so
    // outliving the loop is the property this needs.
    unsafe {
        ndk_sys::AChoreographer_postVsyncCallback(choreographer, Some(on_vsync), state.cast());
    }
}

/// One frame, on the render thread, driven by vsync.
///
/// `AChoreographer_postVsyncCallback` is `__INTRODUCED_IN(33)` and the link level
/// is 33, so it is reachable unconditionally — no runtime API guard and no
/// `postFrameCallback64` fallback branch. That is the whole consequence of the
/// API floor story #862 set.
unsafe extern "C" fn on_vsync(
    data: *const ndk_sys::AChoreographerFrameCallbackData,
    user: *mut c_void,
) {
    let state = user.cast::<LoopState>();
    if state.is_null() {
        return;
    }
    // SAFETY: `user` is the `*mut LoopState` handed to `post_vsync`, which points
    // at a leaked allocation — readable even after the loop has ended and the
    // thread has gone, which is a state this callback can legitimately arrive in.
    let state = unsafe { &mut *state };

    // **Read before the stopped-loop check, which is the other way round from
    // how this used to be written.** That check moved inside `step`, so a
    // callback arriving after the loop stopped now makes this one NDK read
    // before returning. Deliberate: `data` is the callback's own argument and is
    // valid for the call whatever the loop is doing, and keeping the guard in
    // one place is worth more than saving one read on a path that runs at most
    // once per surface. Do not "restore" the early return here — it would put
    // the rule back in two places, which is how the recovery below was broken
    // three times.
    //
    // SAFETY: `data` is the callback's own argument, valid for this call.
    let now = unsafe { ndk_sys::AChoreographerFrameCallbackData_getFrameTimeNanos(data) };

    // Everything the frame decides. Held outside this file — and outside the
    // platform `cfg` — because nothing in it binds an NDK symbol, and because
    // three consecutive repairs to the recovery path below were shipped broken
    // while every test tier passed (issues #888, #940).
    if state.step(now) == Action::Stop {
        return;
    }

    // SAFETY: this thread prepared the looper, so it has an instance.
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if !choreographer.is_null() {
        post_vsync(choreographer, state);
    }
}
