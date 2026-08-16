//! The render thread, the looper and the `AChoreographer` callback (D6).
//!
//! Compiled on Android and nowhere else — every function here binds an NDK
//! symbol. What it drives is [`Frames`], so the loop is written once and the
//! scene source is the implementation's business; see
//! `crates/dashscene-android/src/frames.rs` for why that line is drawn where
//! it is. A source path rather than a link because the `frames` module is
//! private, so it appears in no rendered documentation on any target.
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
use crate::machine::{Action, LoopState, pack};
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
/// `surfaceDestroyed` with a live surface.
///
/// **So a missed release is a silent freeze, not a kill.** This comment used to
/// say an application-not-responding kill would end it; that was measured on
/// 2026-08-15 and is not what happens. `surfaceDestroyed` blocked for 128 s in
/// #960's reproducer and no `am_anr` was raised at all. The wait reports itself
/// for that reason, which is the only thing that ends up in logcat.
///
/// **Scoped to that case, and not a claim that a blocked main thread is never
/// killed.** An application-not-responding kill needs something to time out —
/// input dispatch, a broadcast, a service — and #960's transition delivers
/// none of them: the window is going away and nothing is being dispatched to
/// it. [`start`]'s note about `surfaceCreated` is a different moment, at
/// launch, where input can well be pending, and nothing here measured it.
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
    //
    // The reporter is what makes a wait that will not end visible: it does not
    // shorten the wait — returning here with a live surface is the
    // use-after-free — it says the UI thread is still parked. Nothing else
    // does: no application-not-responding kill fired in any run of #960's
    // reproducer, including a 128 s block.
    host.handshake.request_teardown(|elapsed| {
        // **The observation, not the diagnosis.** An earlier wording said
        // the loop was "still inside a call that cannot be interrupted",
        // which this line cannot know: a debug teardown measured at 1.15 s
        // can pass two seconds while progressing normally through `detach`,
        // `ds_runtime_free` and the wgpu device drop, and that wording
        // would have sent the next reader looking at `Frames::attach`. What
        // is true is only that the loop has not released yet.
        log(&format!(
            "surfaceDestroyed has been waiting {} s for the frame loop to \
                 release the surface",
            elapsed.as_secs()
        ));
    });
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
    let frames = frames();

    // **Built un-attached, and it takes the surface up itself** (issue #1083).
    // The teardown check that has to run before an acquisition binds no NDK
    // symbol, so by this crate's rule it belongs in [`crate::machine`] with a
    // test — and it could not go there while the state was built from an
    // already-attached `Frames`, because the state that carries the handshake
    // did not exist until after the attach had returned. `LoopState::start`
    // reads the request, writes the marker pair and attaches; the rebuild path
    // runs the same code. Every logcat line the sequence used to write is still
    // written, by that one path rather than by two copies of it.
    let mut state = LoopState::new(frames, window, extent_cell, Arc::clone(&handshake));

    // Whether a surface was ever taken up. `shut_down` runs either way — it is
    // what gives up an attach that failed partway — but the closing line below
    // must not claim a detach that did not happen. Logcat is the only witness a
    // device gives, and reading it is how issue #960 was finally diagnosed.
    let attached = state.start();

    if attached {
        // SAFETY: this thread prepared a looper, which is what `getInstance`
        // needs.
        let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
        if choreographer.is_null() {
            // No instance means no frame will ever run. Torn down rather than
            // left as a live handle over a loop that does not exist.
            log("AChoreographer_getInstance returned null — no frame loop");
        } else {
            // **Leaked, deliberately, and this is a use-after-free fix rather
            // than an oversight.** `on_vsync` re-posts itself before the loop
            // notices a teardown request, so the loop almost always exits with
            // a callback still registered — and a posted vsync cannot be
            // cancelled. A `LoopState` on this thread's stack would die while
            // that callback still pointed at it, so it is leaked and `running`
            // is what tells a late callback there is nothing left to do.
            //
            // **Only from here**, because this is the first point at which a
            // callback that can outlive this thread exists. Neither refusal
            // above posts one, so neither needs the leak and neither takes it.
            //
            // **What stays leaked is this struct and no implementation.**
            // `LoopState::shut_down` drops the `Frames` box, so a scene, a
            // document and a font file are no longer retained for the life of
            // the process once per surface cycle — which on Android is every
            // rotation (issue #1085).
            //
            // Not *nothing*, and the difference is worth stating: the struct is
            // tens of bytes, and it holds the last strong reference to the
            // `Handshake` and to the extent cell once `destroy` has freed the
            // `AndroidHost`, so two small allocations leak with it. That is the
            // residue, against the 328 324 B the harness's own cascade came to.
            let state: &'static mut LoopState = Box::leak(Box::new(state));

            handshake.started();
            post_vsync(choreographer, state);
            // The loop is the looper's: `pollOnce` dispatches the vsync
            // callback, which draws and re-posts. The teardown check sits
            // between polls rather than inside the callback, so a request never
            // waits on a frame that is mid-flight.
            while state.is_running() && !handshake.teardown_requested() {
                // A 100 ms timeout rather than an indefinite wait, so the
                // teardown check runs even if vsync stops arriving — which is
                // what a surface that has gone away looks like from here.
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

            // D4's ordering, and the only one that is correct: tell a callback
            // the choreographer may still hold that there is nothing left to
            // do, drop the surface, and only then let the UI thread release the
            // window. The first two are `shut_down`'s, which holds them
            // together where they can be read as one rule; the release happens
            // as `_release` drops, immediately after this returns.
            state.shut_down();
            log("surface detached");
            return;
        }
    }

    // Reached by three outcomes, and `start` has already said which: a teardown
    // requested before the surface was taken up, an attach that failed, or a
    // null choreographer over a surface that came up fine.
    //
    // **`shut_down` is not a no-op on the middle one**, which is the reason to
    // name all three rather than call them "the refusals". An attach fails
    // *partway*: `DocumentFrames::attach` stores the runtime before anything
    // else can fail precisely so this call has the pointer to free, and on the
    // surface path a wgpu device goes with it. Deleting this because nothing
    // was attached would reinstate a once-per-surface-cycle leak.
    //
    // Nothing was posted on any of the three, so this state is dropped rather
    // than leaked.
    state.shut_down();
    if attached {
        // The choreographer was null: a surface was taken up and is now given
        // up, so this reports what happened. On the other refusal there was no
        // surface, and writing this would be a detach that did not occur.
        log("surface detached");
    }
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
