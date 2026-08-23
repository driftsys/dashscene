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
//! Because it is a part that can be wrong without a device. The platform half of
//! this crate is `#[cfg(target_os = "android")]` and compiles nowhere else, so no
//! `cargo test` can reach it — the same problem `dashscene-web` has with its
//! `requestAnimationFrame` loop, answered the same way: keep the decidable part
//! outside the platform gate.
//!
//! **This was the only such part until issue #888.** The frame loop's state
//! machine is now a second one, in [`crate::machine`], lifted out for this exact
//! reason after three consecutive repairs to its recovery path shipped broken.
//! A decision that binds no NDK symbol belongs beside these two, not inside
//! `loop_`.
//!
//! A lifetime bug here does not need a fast GPU to reproduce, and it does not
//! need Android either. Two threads and a flag are enough to express the
//! ordering the callback depends on, which is what the tests below drive.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long [`Handshake::request_teardown`] waits before it starts saying so,
/// and the interval between reports afterwards.
///
/// **Two seconds is set by the slowest ordinary teardown, not by the fastest.**
/// An ordinary teardown must pass through in silence, or the report is noise on
/// every rotation and every backgrounding — and noise is what stops the real
/// report being read. The measurements it has to clear, all on an emulator:
///
/// ```text
/// release, backgrounding      80 ms
/// release, split-screen       27 ms
/// debug, first teardown     1.15 s
/// ```
///
/// The debug figure is the one that sets it, and it is still the one to set it:
/// `just android` builds debug and `_apk-harness` defaults
/// `DASHSCENE_ANDROID_PROFILE` to it, so debug is what an ordinary run packages
/// unless a caller names otherwise. An interval of one second would report on an
/// ordinary debug teardown; two clears it with room and still catches a wait
/// that will not end inside the first few seconds of it.
///
/// **It is no longer the build every Android exercise has used**, which this
/// comment said until issue #1187: the split-screen run that passes needs a
/// release library, because a debug one takes over 218 s from cold launch to
/// first frame (issue #960). That does not move the interval — a release
/// teardown is faster and clears two seconds by more — but it is why the debug
/// figure is now the slowest case rather than the only one.
///
/// **It is not a caller's to choose** (issue #1082).
/// [`Handshake::request_teardown`] reads this; the interval is an argument only
/// on the crate-private `request_teardown_every`, which is what the tests
/// drive. Exported all the same, because it is the cadence an embedder reads in
/// logcat and there is nowhere else to read it from.
pub const REPORT_EVERY: Duration = Duration::from_secs(2);

/// Whether a wait that has run for `elapsed`, and last reported at `reported`,
/// is due to report again.
///
/// A function rather than the expression inline, for this module's own reason:
/// a condvar wakes for three causes — the timeout, a notification, and
/// spuriously — and only the first two carry any relation to elapsed time. The
/// third is what makes "report every interval" a rule that can be wrong
/// without a device, and a rule that can be wrong without a device belongs
/// where a host test can reach it (issue #888).
fn report_due(elapsed: Duration, reported: Duration, every: Duration) -> bool {
    elapsed >= reported + every
}

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
    /// instance. Measured on an emulator: **80 ms** backgrounding and **27 ms**
    /// on a split-screen transition for a release build, and **1.15 s** for the
    /// first teardown of a debug one. That is a main-thread block on every
    /// rotation and every backgrounding, and it is the cost of holding no state
    /// across a surface cycle; issue #872 carries it.
    ///
    /// **It is not bounded by the loop either, and that is where it can become
    /// a freeze.** The render thread reads the request only once it has
    /// attached, so an attach still in flight holds this wait for its whole
    /// duration. Measured on an emulator, from a cold launch to the first
    /// frame: **0.74 s for a release build**, and **over 218 s for a debug
    /// one**, which was still running when the measurement was abandoned
    /// (issue #960). `just android` builds debug, so it is the build an
    /// ordinary run packages; [`REPORT_EVERY`] carries why it is no longer the
    /// build every Android exercise has used. That correction landed on the
    /// constant under issue #1187 and missed this copy, which went on asserting
    /// it until 2026-08-23.
    ///
    /// # What `waiting` is for
    ///
    /// It is called with the elapsed time once every [`REPORT_EVERY`] for as
    /// long as the wait continues.
    /// **This does not shorten the wait** — a timeout means returning from
    /// `surfaceDestroyed` with a live surface, which is exactly the
    /// use-after-free this type exists to prevent, and the release-build
    /// measurements above say the wait itself is not the defect. What it
    /// changes is that a wait that will not end says so. Before it,
    /// `surfaceDestroyed` could block for minutes writing nothing, and **no
    /// application-not-responding kill fires** to end it — no `am_anr` was
    /// raised in any run of #960's reproducer, including a 128 s block. It
    /// took a person watching an emulator to notice.
    ///
    /// The cadence is [`REPORT_EVERY`] and is not an argument here. The
    /// crate-private `request_teardown_every` is what names it, and it is
    /// crate-private for the reason issue #1082 gives.
    pub fn request_teardown(&self, waiting: impl Fn(Duration)) {
        self.request_teardown_every(REPORT_EVERY, waiting);
    }

    /// [`Handshake::request_teardown`] with the reporting interval named.
    ///
    /// The interval exists so the rule is testable in milliseconds: a test that
    /// had to wait [`REPORT_EVERY`] to see one report would cost more wall time
    /// than the whole sanity tier.
    ///
    /// **`pub(crate)`, and that is the whole of issue #1082.** It used to be
    /// public, and `Duration::ZERO` with it: `wait_timeout` returns at once
    /// every pass and `report_due(elapsed, reported, ZERO)` reduces to
    /// `elapsed >= reported`, which always holds — so the wait became a busy
    /// spin on the **UI thread** emitting one report per iteration, for the
    /// whole of a teardown that is already the thing being complained about.
    ///
    /// A floor would have bounded that spin rather than removed it: one report
    /// per millisecond, across the 218 s attach this module measures, is still
    /// tens of thousands of writes from inside `surfaceDestroyed`. Not being
    /// reachable from outside this crate is what removes it.
    ///
    /// `report_every` must be non-zero. A `debug_assert!` is a real gate for
    /// that now and would not have been before: every caller is in this crate,
    /// and every tier that runs this crate's tests runs them in debug — so
    /// there is no build on which a violation reaches a device unannounced.
    pub(crate) fn request_teardown_every(
        &self,
        report_every: Duration,
        waiting: impl Fn(Duration),
    ) {
        debug_assert!(
            !report_every.is_zero(),
            "a zero reporting interval makes every pass due, so the wait spins \
             on the UI thread emitting one report per iteration"
        );
        let started = Instant::now();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // `Starting` as well as `Running`: a surface destroyed before the first
        // frame is ordinary — a rotation during startup is one — and a
        // handshake that only accepted a running loop would wait forever for a
        // thread that had not reached its loop yet.
        if matches!(*state, State::Starting | State::Running) {
            *state = State::TeardownRequested;
            self.changed.notify_all();
        }
        // A loop rather than a `while` over the guard, because the guard has to
        // be dropped in the middle of it — see the report below. Spurious wakes
        // are why the state is re-read every pass and never trusted from one
        // wake: a wait that believed a single wake would return with the
        // surface still alive, which is the whole failure this type prevents.
        // `wait_timeout` rather than an untimed `wait` for the same reason the
        // loop's own poll has a timeout: a report is due even when nothing
        // wakes this thread at all, and a render thread that is not coming back
        // to the poll loop wakes nothing.
        //
        // **This comment said "stuck inside one uninterruptible call" until
        // 2026-08-23, and that is not what it was doing.** Measured on an
        // emulator that day, over a 645 s attach: runnable in all 61 samples,
        // 546.6 s of CPU on one thread, and preempted rather than blocked —
        // across all four cores, which is why the record states it in
        // CPU-seconds. The
        // timeout is unaffected — a thread computing without yielding wakes
        // this one no more often than one that cannot wake it — but the
        // mechanism named here was wrong. See `docs/design/android-toolchain.md`,
        // "The debug attach on the automotive image, bounded".
        let mut reported = Duration::ZERO;
        loop {
            // **Tested before waiting, not after.** A loop that ended by itself
            // reaches `Released` with nobody having asked it for anything, and
            // the teardown that arrives afterwards must return at once. Written
            // the other way round, with the wait first, that case sat through a
            // full interval with no notifier left to wake it — 2 s of blocked
            // UI thread, emitting nothing, on a path that used to return in
            // microseconds. It is also the ordinary shape of #960's own
            // emulator, where the attach fails and the thread releases before
            // any surface is destroyed.
            if *state == State::Released {
                return;
            }
            // Elapsed rather than the timeout flag: a spurious wake would leave
            // that flag clear while the interval had passed anyway, and
            // reporting is a function of elapsed time rather than of how this
            // thread came to be awake.
            let elapsed = started.elapsed();
            if !report_due(elapsed, reported, report_every) {
                // **The remainder of this interval, not a fresh one.** A wake
                // that is not due to report — a spurious one, or a notify that
                // did not carry `Released` — would otherwise restart the full
                // interval, so a wake at 1.99 s pushes the first report to
                // 3.99 s and the documented cadence becomes a lower bound. The
                // subtraction cannot underflow: `report_due` being false is
                // exactly `elapsed < reported + report_every`.
                let remaining = (reported + report_every).saturating_sub(elapsed);
                let (next, _) = self
                    .changed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|error| error.into_inner());
                state = next;
                continue;
            }
            reported = elapsed;
            // **The guard is dropped first, and this is a deadlock fix rather
            // than tidiness.** `waiting` is the caller's code and this type is
            // public, so a reporter that asks this same handshake anything —
            // `is_running()` is the obvious one to want in a log line — would
            // block on a `std::sync::Mutex` that is not reentrant, and hang the
            // UI thread forever: the exact silent freeze this reporting exists
            // to end. Even the reporter the host actually passes writes to
            // logcat, and holding the lock across that write parks the render
            // thread's `released()` behind a socket write.
            drop(state);
            waiting(elapsed);
            // Re-taken and the loop re-entered, so the release the render
            // thread may have made while the lock was down is read at the top
            // rather than assumed here.
            state = self.state.lock().unwrap_or_else(|error| error.into_inner());
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    /// A reporter for the tests that are not about reporting.
    ///
    /// Named rather than a closure at each call site so that "this test does
    /// not care what the wait says" reads as a statement rather than as an
    /// empty body someone might fill in.
    fn silent(_elapsed: Duration) {}

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

        handshake.request_teardown_every(Duration::from_millis(50), silent);
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

        handshake.request_teardown_every(Duration::from_millis(50), silent);
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
            handshake.request_teardown_every(Duration::from_millis(50), silent);
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

        // Must return rather than wait for a thread that has already gone —
        // **and must return at once.** The interval is a whole second so that
        // "returned" and "returned after one wait cycle" are different
        // outcomes: the reporting rewrite tested the state only after waiting,
        // which sat this case through a full interval with no notifier left to
        // wake it, blocking the UI thread for the whole of it and emitting
        // nothing. A second is far above any scheduling noise and far below the
        // interval it would have waited.
        let started = std::time::Instant::now();
        handshake.request_teardown_every(Duration::from_secs(1), silent);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "a handshake already released must not wait — took {:?}",
            started.elapsed()
        );
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
        handshake.request_teardown_every(Duration::from_millis(50), silent);
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

    /// **A wait that will not end says so, repeatedly, and still does not
    /// return early** (issue #960).
    ///
    /// The render thread here stands for one stuck inside `Frames::attach`:
    /// it never looks at the request, so nothing ever notifies this condvar
    /// and only the timeout wakes it. That is the case a plain `wait` reports
    /// nothing about — `surfaceDestroyed` blocked for 128 s writing not one
    /// line, and no application-not-responding kill fired to end it.
    ///
    /// More than one report, because a single line at the two-second mark
    /// cannot be told apart from a slow teardown that then completed.
    ///
    /// **Driven by the reports rather than by the clock.** The render thread
    /// waits for the third report before releasing, so the test's own progress
    /// is what ends it — no sleep to be scheduled past, and nothing that reads
    /// differently on a loaded runner under concurrent nextest. Written first
    /// as a 120 ms sleep against a 20 ms interval, which needed the UI thread
    /// to complete two wait cycles inside that window and would have gone red
    /// for a scheduling delay rather than for a defect.
    #[test]
    fn a_wait_that_outlasts_the_interval_reports_and_keeps_reporting() {
        /// Enough to say "keeps reporting" rather than "reported".
        const WANTED: usize = 3;

        let handshake = Arc::new(Handshake::new());
        let reports = Arc::new(AtomicUsize::new(0));
        let surface_alive = Arc::new(AtomicBool::new(true));

        let render = {
            let handshake = Arc::clone(&handshake);
            let surface_alive = Arc::clone(&surface_alive);
            let reports = Arc::clone(&reports);
            std::thread::spawn(move || {
                // Deaf to the request until the wait has reported several
                // times, which is what an attach in flight is.
                while reports.load(Ordering::SeqCst) < WANTED {
                    std::thread::sleep(Duration::from_millis(1));
                }
                surface_alive.store(false, Ordering::SeqCst);
                handshake.released();
            })
        };

        let counter = Arc::clone(&reports);
        // The reporter asks this same handshake a question. That is the
        // ordinary thing to want in a log line, and it deadlocks against a
        // non-reentrant mutex if `request_teardown` calls it holding the
        // guard — so this call site is the regression test for that as much
        // as it is for the counting.
        let asked = Arc::clone(&handshake);
        handshake.request_teardown_every(Duration::from_millis(5), move |_| {
            assert!(
                asked.teardown_requested(),
                "the wait is reporting, so the request it is waiting on is in"
            );
            counter.fetch_add(1, Ordering::SeqCst);
        });

        assert!(
            !surface_alive.load(Ordering::SeqCst),
            "reporting must not have shortened the wait — returning here with a \
             live surface is the use-after-free D4 exists to prevent"
        );
        assert!(
            reports.load(Ordering::SeqCst) >= WANTED,
            "the wait must keep reporting, got {}",
            reports.load(Ordering::SeqCst)
        );
        render.join().unwrap();
    }

    /// **A wake before the interval has passed is not a report** — which is
    /// the whole of what `reported` is for, and what no timing test can pin.
    ///
    /// A condvar wakes for three causes: the timeout, a notification, and
    /// spuriously. Only the first is bounded below by the interval, so the
    /// arithmetic is what keeps a spurious wake at 10 ms from writing "has
    /// been waiting 0 s". The two tests around it drive real threads and real
    /// clocks and can observe none of that: with the guard deleted they both
    /// still pass, because in each of them the only wake that is not the
    /// timeout is the release itself. Measured, not assumed — the guard was
    /// mutated away and neither of them failed.
    #[test]
    fn a_report_is_due_only_once_per_interval() {
        let every = Duration::from_secs(2);

        assert!(
            !report_due(Duration::from_millis(10), Duration::ZERO, every),
            "a spurious wake 10 ms in is not a report"
        );
        assert!(
            !report_due(every - Duration::from_millis(1), Duration::ZERO, every),
            "one millisecond short of the interval is still short of it"
        );
        assert!(
            report_due(every, Duration::ZERO, every),
            "the interval having passed is exactly when the first report is due"
        );
        assert!(
            !report_due(Duration::from_millis(2500), every, every),
            "half an interval after a report is not another report"
        );
        assert!(
            report_due(Duration::from_secs(4), every, every),
            "a full interval after a report is the next one"
        );

        // **Why the interval is not a caller's to choose** (issue #1082), as
        // arithmetic. With no interval at all every pass is due, including one
        // a microsecond after the last report — so the wait emits one report
        // per loop iteration, on the UI thread, for the whole of the teardown.
        // Nothing in this function can refuse that; what removes it is
        // `request_teardown_every` being crate-private, so `Duration::ZERO`
        // cannot arrive from outside.
        assert!(
            report_due(Duration::from_micros(1), Duration::ZERO, Duration::ZERO),
            "a zero interval makes the very first pass due"
        );
        assert!(
            report_due(
                Duration::from_micros(2),
                Duration::from_micros(1),
                Duration::ZERO
            ),
            "and it is still due one microsecond after the last report, which is \
             the spin"
        );
    }

    /// The ordinary teardown says nothing at all.
    ///
    /// A release build's handshake was measured at 80 ms against a
    /// two-second interval, so every rotation and every backgrounding must
    /// pass through silently. A report that fired on the ordinary path would
    /// be noise in the one log the device gives, and noise is what stops the
    /// real report being read.
    #[test]
    fn a_teardown_that_completes_inside_the_interval_reports_nothing() {
        let handshake = Arc::new(Handshake::new());
        let reports = Arc::new(AtomicUsize::new(0));

        let render = {
            let handshake = Arc::clone(&handshake);
            std::thread::spawn(move || {
                while !handshake.teardown_requested() {
                    std::thread::sleep(Duration::from_millis(1));
                }
                handshake.released();
            })
        };

        let counter = Arc::clone(&reports);
        handshake.request_teardown_every(Duration::from_secs(30), move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(
            reports.load(Ordering::SeqCst),
            0,
            "a teardown well inside the interval must not report"
        );
        render.join().unwrap();
    }
}
