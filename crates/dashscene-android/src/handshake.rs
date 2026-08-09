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
    /// Spawned, and not yet drawing. A surface can be asked to tear down before
    /// it ever started, and a rotation during startup produces exactly that.
    Starting,
    /// Running, or idle with the surface still attached.
    Running,
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
            state: Mutex::new(State::Starting),
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
    /// callback returns, so returning early is exactly the bug.
    ///
    /// **The wait is not bounded by one frame.** The render thread checks
    /// between frames rather than inside one, so noticing the request is cheap
    /// — but what it does next is drop the surface, and on this host it also
    /// frees the runtime, which tears down the wgpu device, adapter and
    /// instance. Measured on an emulator: **88-115 ms** for a release build,
    /// and **1.15 s** for the first teardown of a debug one. That is a
    /// main-thread block on every rotation and every backgrounding, and it is
    /// the cost of holding no state across a surface cycle; issue #872 carries
    /// it.
    pub fn request_teardown(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // `Starting` as well as `Running`: a surface destroyed before the first
        // frame is ordinary — a rotation during startup is one — and a
        // handshake that only accepted a running loop would wait forever for a
        // thread that had not reached its loop yet.
        if matches!(*state, State::Starting | State::Running) {
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

    /// The render thread has a surface and is drawing.
    ///
    /// Does not overwrite a teardown that arrived first, which is the case a
    /// rotation during startup produces: the request wins, and the loop sees it
    /// on its first check.
    pub fn started(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if *state == State::Starting {
            *state = State::Running;
            self.changed.notify_all();
        }
    }

    /// Whether the render thread is still live.
    ///
    /// True while it is starting or running, false once it has released. This
    /// is **not** the negation of [`Handshake::teardown_requested`]: a loop that
    /// stopped on its own — a failed tick or draw — reaches `Released` without
    /// anyone having requested teardown, and asking the negated question would
    /// answer that it was still running.
    pub fn is_running(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        matches!(*state, State::Starting | State::Running)
    }

    /// Releases without waiting for a request, for a loop that ended by itself.
    ///
    /// A failed tick or draw stops the loop while the UI thread has asked for
    /// nothing. The state still has to reach `Released`, so that a teardown
    /// arriving afterwards returns rather than waiting for a thread that has
    /// already gone.
    pub fn ended(&self) {
        self.released();
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
    /// rotation is one — so the handshake has to work over a sequence of them.
    ///
    /// A fresh `Handshake` per surface, which is what the host builds: nothing
    /// puts a used one back to the start, because a state moved backwards is a
    /// waiter left parked on a state that has gone further away.
    #[test]
    fn each_surface_gets_its_own_handshake_and_each_one_blocks() {
        for round in 0..3 {
            let handshake = Arc::new(Handshake::new());
            let surface_alive = Arc::new(AtomicBool::new(true));
            let render = {
                let handshake = Arc::clone(&handshake);
                let surface_alive = Arc::clone(&surface_alive);
                std::thread::spawn(move || {
                    handshake.started();
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

    /// A loop that ended by itself is not running, and a teardown asked for
    /// afterwards returns rather than waiting for a thread that has gone.
    ///
    /// This is the case `is_running` exists to answer correctly and the negated
    /// `teardown_requested` answered wrongly: nothing requested teardown, so
    /// that question says `false`, and "not requested" is not "still running".
    #[test]
    fn a_loop_that_ended_by_itself_is_not_running_and_does_not_block_a_teardown() {
        let handshake = Handshake::new();
        handshake.started();
        assert!(handshake.is_running());

        // A failed tick or draw, with nobody having asked for anything.
        handshake.ended();
        assert!(
            !handshake.is_running(),
            "a loop that stopped on its own still reports itself running"
        );
        assert!(
            !handshake.teardown_requested(),
            "nothing requested teardown, so this must stay false — which is why \
             it cannot be used to answer whether the loop is alive"
        );

        // Must return rather than wait for a thread that has already gone.
        handshake.request_teardown();
    }

    /// A teardown that arrives before the loop starts must still be seen, and
    /// must not be overwritten by the loop reporting that it started.
    #[test]
    fn a_teardown_during_startup_wins_over_the_loop_starting() {
        let handshake = Arc::new(Handshake::new());
        let render = {
            let handshake = Arc::clone(&handshake);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                // The loop reaches its "I am up" point after the request.
                handshake.started();
                assert!(
                    handshake.teardown_requested(),
                    "started() overwrote a teardown that had already arrived"
                );
                handshake.released();
            })
        };
        handshake.request_teardown();
        render.join().unwrap();
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
