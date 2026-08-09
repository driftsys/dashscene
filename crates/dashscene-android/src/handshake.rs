//! The destroy handshake — D4 of
//! `docs/decisions/host-integration-in-three-layers.md` (story #841).
//!
//! `ANativeWindow_fromSurface` acquires a reference, and **when
//! `surfaceDestroyed` returns the Surface is invalid**. The frame loop runs on
//! its own thread (D6), so that callback has to block until the loop has stopped
//! and the `wgpu::Surface` built from the window has been dropped. Getting it
//! wrong is use-after-free on rotation, backgrounding and split-screen — the
//! classic native crash on Android, and the reason the decision record names it
//! as a rule rather than leaving it to whoever writes the host.
//!
//! **This is stronger than `Drawn::No`, and the two must not be confused.**
//! `Drawn::No` says a frame did not reach the window and the next one may;
//! destruction says tear the renderer down. The former is a scheduling concern
//! (story #586), this is a lifetime one.
//!
//! # Why it is a type of its own, on no Android API
//!
//! Because it is the part that can be wrong without a device. Everything else in
//! this crate is `#[cfg(target_os = "android")]` and compiles nowhere else, so no
//! `cargo test` can reach it — the same problem `dashscene-web` has with its
//! `requestAnimationFrame` loop, answered the same way: keep the decidable part
//! outside the platform gate.
//!
//! A lifetime bug here does not need a fast GPU to reproduce, and it does not
//! need Android either. Two threads and a flag are enough to express the
//! ordering the callback depends on, which is what the tests below drive.

use std::sync::{Condvar, Mutex};

/// What the render thread is doing, as the UI thread sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Running, or idle with the surface still attached.
    Holding,
    /// The UI thread has asked for teardown and is waiting.
    TeardownRequested,
    /// The render thread has stopped drawing and dropped the surface. The
    /// window can be released.
    Released,
}

/// The rendezvous between the thread that owns the window and the thread that
/// draws into it.
///
/// One direction only: the UI thread asks and waits, the render thread
/// acknowledges. Nothing here hands a value across — the surface is dropped on
/// the render thread, by the thread that built it, and this type carries only
/// the fact that it happened.
pub struct Handshake {
    state: Mutex<State>,
    changed: Condvar,
}

impl Default for Handshake {
    fn default() -> Self {
        Self::new()
    }
}

impl Handshake {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::Holding),
            changed: Condvar::new(),
        }
    }

    /// Asks the render thread to tear down, and **blocks until it has**.
    ///
    /// Called from `surfaceDestroyed`, on the UI thread. When this returns, the
    /// surface has been dropped and the `ANativeWindow` can be released.
    ///
    /// Blocking the UI thread is the correct behaviour here and not an
    /// oversight: Android's contract is that the Surface stays valid until the
    /// callback returns, so returning early is exactly the bug. The wait is
    /// bounded by one frame in practice, because the render thread checks
    /// between frames rather than inside one.
    pub fn request_teardown(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if *state == State::Holding {
            *state = State::TeardownRequested;
            self.changed.notify_all();
        }
        // `while` rather than `if`: a condvar may wake spuriously, and a wait
        // that trusted a single wake would return with the surface still alive,
        // which is the whole failure this type prevents.
        while *state != State::Released {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    /// Whether the UI thread has asked for teardown.
    ///
    /// The render thread reads this between frames. It does not block: a loop
    /// that waited here would stop drawing while nothing had asked it to.
    pub fn teardown_requested(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        *state == State::TeardownRequested
    }

    /// Reports that the surface has been dropped, releasing
    /// [`Handshake::request_teardown`].
    ///
    /// **Call this after the surface is gone, never before.** It is the whole of
    /// the promise the UI thread is waiting on, and calling it early is
    /// indistinguishable — to that thread — from not having the handshake at
    /// all.
    pub fn released(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        *state = State::Released;
        self.changed.notify_all();
    }

    /// Puts the handshake back to holding, for the next surface.
    ///
    /// A view goes through create/destroy many times over one runtime's life —
    /// every rotation is one — so this is the ordinary case rather than a reset
    /// after a failure.
    pub fn rearm(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        *state = State::Holding;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::*;

    /// The property the whole type exists for: the caller does not come back
    /// until the surface has been dropped.
    ///
    /// The "surface" is an `AtomicBool` that the render thread clears, standing
    /// for the `wgpu::Surface` drop. If `request_teardown` returns while it is
    /// still set, `surfaceDestroyed` would have returned while a live surface
    /// held the window — which is the use-after-free.
    #[test]
    fn teardown_does_not_return_until_the_surface_is_dropped() {
        let handshake = Arc::new(Handshake::new());
        let surface_alive = Arc::new(AtomicBool::new(true));

        let render = {
            let handshake = Arc::clone(&handshake);
            let surface_alive = Arc::clone(&surface_alive);
            std::thread::spawn(move || {
                while !handshake.teardown_requested() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                // The render thread's own ordering, and the one that matters:
                // stop drawing, drop the surface, *then* acknowledge.
                std::thread::sleep(Duration::from_millis(20));
                surface_alive.store(false, Ordering::SeqCst);
                handshake.released();
            })
        };

        handshake.request_teardown();
        assert!(
            !surface_alive.load(Ordering::SeqCst),
            "request_teardown returned while the surface was still alive — this is the \
             use-after-free D4 exists to prevent"
        );
        render.join().unwrap();
    }

    /// Teardown requested before the render thread ever looks is still seen.
    ///
    /// A surface destroyed immediately after being created is ordinary — a
    /// rotation during startup produces one — and a handshake that only worked
    /// when the render thread was already waiting would deadlock on it.
    #[test]
    fn a_request_that_arrives_first_is_not_lost() {
        let handshake = Arc::new(Handshake::new());
        assert!(!handshake.teardown_requested());

        let render = {
            let handshake = Arc::clone(&handshake);
            std::thread::spawn(move || {
                // Deliberately late: the request is already in by the time this
                // runs.
                std::thread::sleep(Duration::from_millis(20));
                assert!(
                    handshake.teardown_requested(),
                    "the request was made before this thread looked, and was lost"
                );
                handshake.released();
            })
        };

        handshake.request_teardown();
        render.join().unwrap();
    }

    /// Create/destroy happens many times over one runtime's life — every
    /// rotation is one — so the handshake has to work more than once.
    #[test]
    fn a_rearmed_handshake_blocks_again() {
        let handshake = Arc::new(Handshake::new());

        for round in 0..3 {
            handshake.rearm();
            let surface_alive = Arc::new(AtomicBool::new(true));
            let render = {
                let handshake = Arc::clone(&handshake);
                let surface_alive = Arc::clone(&surface_alive);
                std::thread::spawn(move || {
                    while !handshake.teardown_requested() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                    surface_alive.store(false, Ordering::SeqCst);
                    handshake.released();
                })
            };
            handshake.request_teardown();
            assert!(
                !surface_alive.load(Ordering::SeqCst),
                "round {round} returned with the surface still alive"
            );
            render.join().unwrap();
        }
    }

    /// The render thread does not block on the question, because a loop that
    /// waited here would stop drawing while nothing had asked it to.
    #[test]
    fn asking_whether_teardown_was_requested_does_not_block() {
        let handshake = Handshake::new();
        for _ in 0..1000 {
            assert!(!handshake.teardown_requested());
        }
    }
}
