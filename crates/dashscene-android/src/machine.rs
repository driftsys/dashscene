//! The frame loop's decidable half: the state machine one vsync runs
//! (issue #888).
//!
//! # Why it is a module of its own, on no Android API
//!
//! The frame loop in `loop_` is entirely behind `#[cfg(target_os = "android")]`,
//! because every function in it binds an NDK symbol. So `just test`, `just
//! test-regression` and the `android-build` CI job — a **compile only** — all
//! pass with the loop stalled.
//!
//! That is not hypothetical. The review of PR #887 found the loop permanently
//! stalling after a successful surface rebuild, because the recovery returned
//! before re-posting the vsync callback. The repair for that one put the
//! rebuild counter's reset on the path both outcomes reach, which made the
//! give-up branch unreachable — issue #940. Three consecutive repairs to one
//! recovery path have broken it, and each was invisible for this single reason.
//!
//! **What a frame decides needs no NDK symbol.** Only three things in the
//! callback do: reading the frame time, getting the choreographer, and posting
//! the next callback. Everything between them — the frame delta, the resize
//! acceptance, the `forced` flag, the rebuild bound and whether to reschedule at
//! all — is a function of this crate's own state and of [`Frames`], which an
//! implementation supplies. So it sits here, outside the platform gate, with
//! tests, exactly as [`crate::handshake`] does and for the same reason.
//!
//! The callback keeps the three NDK calls and nothing else.

// Off Android, `loop_` is not compiled, so nothing outside the tests
// below constructs any of this. That is the arrangement working rather than a
// problem — the whole point of the module is that the decidable half is
// reachable from a host test — and the allowance is narrowed to the target where
// it is true, so a genuinely unused item on Android is still reported.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::frames::{Frames, Step};
use crate::log;

/// Packs a physical-pixel extent into one word, so it is published and read
/// atomically as a pair.
pub(crate) fn pack(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

/// The inverse of [`pack`].
pub(crate) fn unpack(value: u64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}

/// How many consecutive surface rebuilds the loop will attempt before giving up.
///
/// A rebuild that works is followed by a frame, and a frame resets the count —
/// so this bounds a surface that is being lost *repeatedly*, which is what a
/// removed device or an unrecoverable driver reset looks like. The same bound,
/// for the same reason, that `dashscene-web` and `dashscene-desktop` carry.
pub(crate) const MAX_CONSECUTIVE_REBUILDS: u32 = 3;

/// What the vsync callback must do once [`LoopState::step`] has returned.
///
/// The step decides; the callback is left holding only the NDK calls that carry
/// the decision out. Returned rather than inferred from [`LoopState::running`]
/// so that "do not reschedule" is stated once, by the code that knows why.
///
/// `#[must_use]` because dropping it is the whole failure it exists to prevent:
/// a callback that ignored this and posted unconditionally would keep
/// scheduling a loop that had stopped, and `on_vsync` is behind the platform
/// `cfg` where no test can see it and the `android-build` job only compiles.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Post the next vsync callback.
    Reschedule,
    /// Post nothing. The loop has stopped, or had already stopped before this
    /// callback arrived.
    Stop,
}

/// The render thread's own state, reachable from the vsync callback.
pub(crate) struct LoopState {
    frames: Box<dyn Frames>,
    /// The window, kept so a lost surface can be rebuilt from it. Live for the
    /// whole loop: the destroy handshake is what makes that true.
    window: usize,
    extent: Arc<AtomicU64>,
    /// The extent the surface is currently configured for.
    configured: (u32, u32),
    /// The previous vsync's timestamp, for the frame delta.
    previous: Option<i64>,
    /// Why the next frame must draw whatever the generation says. Consumed by
    /// the frame that acts on it.
    forced: bool,
    /// Cleared when the loop should stop rescheduling.
    running: bool,
    /// Vsync callbacks seen and frames handed to [`Frames::frame`]. Reported
    /// periodically, because "the surface attached" and "the loop is running"
    /// are different claims and only the second is what a picture depends on.
    vsyncs: u64,
    frames_run: u64,
    /// Consecutive surface rebuilds with no frame between them. See
    /// [`MAX_CONSECUTIVE_REBUILDS`].
    rebuilds: u32,
}

impl LoopState {
    /// The state a freshly attached surface starts in.
    ///
    /// `frames` has already had [`Frames::attach`] called on it, and
    /// `configured` is the extent that call was given.
    pub(crate) fn new(
        frames: Box<dyn Frames>,
        window: usize,
        extent: Arc<AtomicU64>,
        configured: (u32, u32),
    ) -> Self {
        Self {
            frames,
            window,
            extent,
            configured,
            previous: None,
            // The first frame is one of the cases the generation cannot report.
            forced: true,
            running: true,
            vsyncs: 0,
            frames_run: 0,
            rebuilds: 0,
        }
    }

    /// Whether the loop should still be scheduling frames.
    ///
    /// Read by the poll loop between polls. Not the negation of "a teardown was
    /// requested": a loop that stopped on its own — a failed rebuild, or the
    /// bound giving up — is not running while nobody has asked it for anything.
    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    /// Stops the loop and drops whatever the implementation holds.
    ///
    /// **The order is D4's and is the only correct one**: tell a callback the
    /// choreographer may still hold that there is nothing left to do, and only
    /// then release the surface. Detaching first would leave a dispatched
    /// callback free to call [`Frames::frame`] on an implementation that has
    /// already given up its surface.
    ///
    /// Here rather than in the caller because it binds no NDK symbol, which is
    /// this module's whole criterion. What the tests below can see is that it
    /// stops the loop and detaches exactly once, and that a callback arriving
    /// afterwards does nothing; the ordering of the two statements is not
    /// observable from outside and is held by this comment and by review.
    pub(crate) fn shut_down(&mut self) {
        self.running = false;
        self.frames.detach();
    }

    /// One frame's worth of decisions, given the vsync timestamp in nanoseconds.
    ///
    /// Takes the timestamp rather than a delta so that the first frame's "there
    /// is no previous vsync, so `dt` is zero" rule is decided here too, with
    /// everything else it has to stay consistent with.
    pub(crate) fn step(&mut self, now: i64) -> Action {
        // The loop has ended. Nothing to draw into, and nothing to reschedule.
        // A posted vsync cannot be cancelled, so a callback arriving after the
        // loop stopped is ordinary rather than exceptional.
        if !self.running {
            return Action::Stop;
        }

        self.vsyncs += 1;
        if self.vsyncs == 1 {
            log("first vsync callback");
        }

        let dt = match self.previous {
            // Nanoseconds to seconds. Raw from here: `LiveScene::tick` applies
            // both the ceiling and the floor, so the rule has one statement
            // rather than one per host (story #810).
            Some(previous) => (now - previous) as f32 / 1_000_000_000.0,
            None => 0.0,
        };
        self.previous = Some(now);

        // The extent the UI thread last reported. Checked every frame rather
        // than through a message, because `surfaceChanged` and this loop are on
        // different threads and a message would need a channel for one `u32`
        // pair.
        let wanted = unpack(self.extent.load(Ordering::Acquire));
        if wanted != self.configured && wanted.0 > 0 && wanted.1 > 0 {
            // Recorded only when it was taken up. A refused extent — one past
            // the adapter maximum (issue #714) — is offered again next frame
            // rather than believed, which is what stops one refusal leaving the
            // scene laid out for the old size for the rest of the surface's
            // life.
            if self.frames.resize(wanted.0, wanted.1) {
                self.configured = wanted;
                // A reconfigured swapchain has drawn nothing, and the
                // generation cannot report that.
                self.forced = true;
            }
        }

        let forced = self.forced;
        self.forced = false;
        match self.frames.frame(dt, forced) {
            Step::Continue => {
                self.frames_run += 1;
                if self.frames_run == 1 {
                    log("first frame");
                } else if self.frames_run.is_multiple_of(240) {
                    // The only continuous evidence a device gives that the loop
                    // is still alive. Without it, a loop that stopped and a loop
                    // that is idle look identical in logcat.
                    log(&format!(
                        "{} frames over {} vsyncs",
                        self.frames_run, self.vsyncs
                    ));
                }
                // **Here, and not on a tail both outcomes reach.** A frame that
                // reached the window means whatever was recovered from is behind
                // us. That is true of this path and false of the rebuild below,
                // which reaches the reschedule with no frame having been drawn —
                // so a reset placed after the `match` ran the counter 0 → 1 → 0,
                // never reached 2, and left a surface lost on every frame
                // rebuilding for as long as the process lived (issue #940).
                // `dashscene-web` resets inside its own success arm for the same
                // reason.
                self.rebuilds = 0;
            }
            Step::Rebuild => {
                // The remedy `dashscene_gpu::FrameError::is_recoverable` names,
                // and the reason the seam has this variant: a host that could
                // only say `Stop` would have no way to honour a rule every other
                // host does.
                //
                // The scene and the clock are untouched; only the device is new.

                // **Spent before the rebuild, not after it.** Both siblings
                // bound the attempt ahead of the expensive part — `dashscene-web`
                // returns before an adapter is asked for, `dashscene-desktop`
                // before the rebind — so both perform `MAX_CONSECUTIVE_REBUILDS`
                // attempts. Counting afterwards performs one more: a full detach,
                // a fresh adapter, device and pipeline set on this thread, on the
                // order of a second, and then thrown away unused because the very
                // next statement gives up. What the constant says is what the
                // three hosts must each do.
                self.rebuilds += 1;
                if self.rebuilds > MAX_CONSECUTIVE_REBUILDS {
                    log("the surface was lost again after every rebuild — giving up");
                    self.running = false;
                    return Action::Stop;
                }

                log("the surface was lost — rebuilding");
                self.frames.detach();
                let (width, height) = self.configured;
                // SAFETY: the window outlives the loop — the destroy handshake
                // is what makes that true — so it is as live now as it was at
                // attach.
                match unsafe {
                    self.frames
                        .attach(self.window as *mut c_void, width, height)
                } {
                    Ok(()) => {
                        // The new device has drawn nothing and the scene has not
                        // changed, so the generation cannot ask for this frame.
                        self.forced = true;
                        // **Falls through to the reschedule below.** Returning
                        // here instead is what a first cut did, and it left a
                        // recovered surface with no pending callback and no way
                        // to acquire one: the loop stayed `running`, the poll
                        // loop spun on its 100 ms timeout, `is_running` kept
                        // answering true, and the window was frozen until
                        // `surfaceDestroyed`. A recovery that stops the thing it
                        // recovered is worse than no recovery, because it
                        // reports success.
                    }
                    Err(error) => {
                        log(&format!("could not rebuild the surface: {error}"));
                        self.running = false;
                        return Action::Stop;
                    }
                }
            }
            Step::Stop => {
                log("the frame source asked the loop to stop");
                self.running = false;
                return Action::Stop;
            }
        }

        Action::Reschedule
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use super::*;
    use crate::frames::AttachError;

    /// The window handle the loop carries.
    ///
    /// **A token, and never dereferenced.** [`Frames::attach`] is `unsafe`
    /// because an implementation may hand the pointer to
    /// `SurfaceRenderer::for_android_ndk`; [`ScriptedFrames`] records it and does
    /// nothing else with it, so the obligation the trait places on a caller is
    /// discharged by the implementation this module calls. A distinctive value
    /// so that "the rebuild re-attached the same window" is an assertion rather
    /// than a coincidence.
    const WINDOW: usize = 0xDEAD_BEEF;

    /// The nominal vsync interval at 60 Hz, in nanoseconds.
    const FRAME_NANOS: i64 = 16_666_667;

    /// What the state machine asked of its [`Frames`], in order.
    #[derive(Default)]
    struct Recording {
        /// One entry per [`Frames::frame`], holding the delta and the `forced`
        /// flag that call was given.
        frames: Vec<(f32, bool)>,
        resizes: Vec<(u32, u32)>,
        attaches: Vec<(usize, u32, u32)>,
        detaches: usize,
    }

    /// A [`Frames`] that answers from a script and records what it was asked.
    ///
    /// The outcome script is consumed one entry per frame and falls back to
    /// [`Step::Continue`] once it is empty, so a test writes only the frames it
    /// cares about.
    struct ScriptedFrames {
        outcomes: VecDeque<Step>,
        /// What each successive `attach` answers. Empty means it succeeds.
        attaches: VecDeque<Result<(), AttachError>>,
        /// Whether `resize` takes the new extent up.
        accept_resize: bool,
        recording: Rc<RefCell<Recording>>,
    }

    impl Frames for ScriptedFrames {
        unsafe fn attach(
            &mut self,
            window: *mut c_void,
            width: u32,
            height: u32,
        ) -> Result<(), AttachError> {
            self.recording
                .borrow_mut()
                .attaches
                .push((window as usize, width, height));
            self.attaches.pop_front().unwrap_or(Ok(()))
        }

        fn resize(&mut self, width: u32, height: u32) -> bool {
            self.recording.borrow_mut().resizes.push((width, height));
            self.accept_resize
        }

        fn frame(&mut self, dt: f32, forced: bool) -> Step {
            self.recording.borrow_mut().frames.push((dt, forced));
            self.outcomes.pop_front().unwrap_or(Step::Continue)
        }

        fn detach(&mut self) {
            self.recording.borrow_mut().detaches += 1;
        }
    }

    /// A state machine over a scripted [`Frames`], already attached at
    /// `configured` — which is where `loop_::start` hands it over.
    ///
    /// Deliberately not square and not a swap of itself, so that a width read as
    /// a height cannot pass.
    fn scripted(
        outcomes: Vec<Step>,
        attaches: Vec<Result<(), AttachError>>,
        accept_resize: bool,
    ) -> (LoopState, Arc<AtomicU64>, Rc<RefCell<Recording>>) {
        let configured = (1080, 2400);
        let recording = Rc::new(RefCell::new(Recording::default()));
        let extent = Arc::new(AtomicU64::new(pack(configured.0, configured.1)));
        let frames = ScriptedFrames {
            outcomes: outcomes.into(),
            attaches: attaches.into(),
            accept_resize,
            recording: Rc::clone(&recording),
        };
        let state = LoopState::new(Box::new(frames), WINDOW, Arc::clone(&extent), configured);
        (state, extent, recording)
    }

    /// Drives `count` vsyncs at 60 Hz and returns what each one decided.
    ///
    /// The clock continues from the vsyncs the state has already seen rather
    /// than restarting at one, so calling this twice does not hand `step` a
    /// timestamp earlier than the one before it. `AChoreographer`'s frame times
    /// are monotonic, and a fixture that goes backwards is testing an input the
    /// platform cannot produce.
    fn run(state: &mut LoopState, count: usize) -> Vec<Action> {
        let seen = state.vsyncs;
        (1..=count as u64)
            .map(|frame| state.step((seen + frame) as i64 * FRAME_NANOS))
            .collect()
    }

    /// **The bound gives up.** Issue #940: it could not, because the counter was
    /// reset on the path both outcomes reach, so it ran 0 → 1 → 0 forever and a
    /// surface lost on every single frame rebuilt without end.
    ///
    /// One more rebuild than the bound allows: the first
    /// `MAX_CONSECUTIVE_REBUILDS` reschedule, and the one after it stops.
    #[test]
    fn the_bound_gives_up_after_consecutive_rebuilds_with_no_frame_between_them() {
        let attempts = MAX_CONSECUTIVE_REBUILDS as usize + 1;
        let (mut state, _extent, recording) =
            scripted(vec![Step::Rebuild; attempts], Vec::new(), true);

        let actions = run(&mut state, attempts);

        let (last, rescheduled) = actions.split_last().expect("one action per vsync");
        assert!(
            rescheduled
                .iter()
                .all(|action| *action == Action::Reschedule),
            "a rebuild inside the bound must reschedule — a recovery that stops \
             the loop it recovered is the PR #887 defect: {actions:?}"
        );
        assert_eq!(
            *last,
            Action::Stop,
            "the {attempts}th consecutive rebuild is past a bound of \
             {MAX_CONSECUTIVE_REBUILDS} and must give up, not reschedule"
        );
        assert!(
            !state.running,
            "giving up must clear `running`, or the poll loop spins on its 100 ms \
             timeout and `is_running` keeps answering true"
        );
        assert_eq!(
            recording.borrow().attaches.len(),
            MAX_CONSECUTIVE_REBUILDS as usize,
            "the count is spent before the rebuild, so the step that gives up \
             re-attaches nothing — acquiring a device and discarding it unused is \
             what counting afterwards costs, and neither sibling pays it"
        );
    }

    /// The other half of the same bound, and what stops it being "fixed" by
    /// deleting the reset: a frame that reached the window puts the count back.
    ///
    /// Two rebuilds, a frame, then a full run of rebuilds. If the frame did not
    /// clear the count the run would give up one step early.
    #[test]
    fn a_frame_that_reached_the_window_starts_the_count_again() {
        let mut outcomes = vec![Step::Rebuild, Step::Rebuild, Step::Continue];
        outcomes.extend(vec![Step::Rebuild; MAX_CONSECUTIVE_REBUILDS as usize]);
        let total = outcomes.len();
        let (mut state, _extent, _recording) = scripted(outcomes, Vec::new(), true);

        let actions = run(&mut state, total);

        assert!(
            actions.iter().all(|action| *action == Action::Reschedule),
            "the frame at index 2 cleared the count, so the {MAX_CONSECUTIVE_REBUILDS} \
             rebuilds after it are all inside the bound: {actions:?}"
        );
        assert!(state.running, "nothing here reached the bound");
    }

    /// A rebuild re-attaches the same window at the configured extent and keeps
    /// the loop scheduled. PR #887's defect was the reschedule going missing.
    #[test]
    fn a_rebuild_detaches_re_attaches_the_same_window_and_reschedules() {
        let (mut state, _extent, recording) = scripted(vec![Step::Rebuild], Vec::new(), true);

        assert_eq!(state.step(FRAME_NANOS), Action::Reschedule);

        let recording = recording.borrow();
        assert_eq!(recording.detaches, 1, "the lost surface is dropped first");
        assert_eq!(
            recording.attaches,
            vec![(WINDOW, 1080, 2400)],
            "the same window, at the extent the surface is configured for"
        );
    }

    /// A rebuild whose re-attach fails stops the loop rather than rescheduling
    /// into a surface that does not exist.
    #[test]
    fn a_rebuild_that_cannot_re_attach_stops_the_loop() {
        let (mut state, _extent, _recording) = scripted(
            vec![Step::Rebuild],
            vec![Err("no adapter".to_owned())],
            true,
        );

        assert_eq!(state.step(FRAME_NANOS), Action::Stop);
        assert!(!state.running);
    }

    /// **A refused resize is offered again.** `Frames::resize` answering `false`
    /// — an extent past the adapter maximum, issue #714 — must not be recorded
    /// as configured, or the scene stays laid out for the old size for the rest
    /// of the surface's life.
    #[test]
    fn a_refused_resize_is_offered_again_on_the_next_frame() {
        let (mut state, extent, recording) = scripted(Vec::new(), Vec::new(), false);
        extent.store(pack(720, 1612), Ordering::Release);

        run(&mut state, 3);

        assert_eq!(
            recording.borrow().resizes,
            vec![(720, 1612); 3],
            "a refusal is not believed, so the same extent is offered every frame"
        );
    }

    /// An accepted resize is recorded, so it is offered once and not again.
    #[test]
    fn an_accepted_resize_is_offered_once() {
        let (mut state, extent, recording) = scripted(Vec::new(), Vec::new(), true);
        extent.store(pack(720, 1612), Ordering::Release);

        run(&mut state, 3);

        assert_eq!(
            recording.borrow().resizes,
            vec![(720, 1612)],
            "once taken up, the extent matches what is configured and is not \
             offered again"
        );
    }

    /// **`forced` is consumed by the frame that acts on it**, and set again by
    /// each case the generation cannot report: the first frame, an accepted
    /// resize, and a rebuild.
    #[test]
    fn forced_is_consumed_by_the_frame_that_acts_on_it() {
        let (mut state, extent, recording) = scripted(
            vec![Step::Continue, Step::Continue, Step::Rebuild],
            Vec::new(),
            true,
        );

        // Frames one and two: the first is forced because the surface has drawn
        // nothing, the second has nothing to force it.
        run(&mut state, 2);
        // Frame three takes up a new extent, and frame four sees the flag it set.
        extent.store(pack(720, 1612), Ordering::Release);
        run(&mut state, 2);

        assert_eq!(
            recording
                .borrow()
                .frames
                .iter()
                .map(|(_, forced)| *forced)
                .collect::<Vec<_>>(),
            vec![true, false, true, true],
            "forced on the first frame, cleared by it, set again by the accepted \
             resize on frame three, and set again by frame three's rebuild"
        );
    }

    /// A callback that arrives after the loop stopped does nothing at all — it
    /// does not draw, and it does not reschedule.
    ///
    /// Ordinary rather than exceptional: a posted vsync cannot be cancelled, so
    /// the loop almost always ends with one still registered.
    #[test]
    fn a_callback_that_arrives_after_the_loop_stopped_does_nothing() {
        let (mut state, _extent, recording) = scripted(vec![Step::Stop], Vec::new(), true);

        assert_eq!(state.step(FRAME_NANOS), Action::Stop);
        assert_eq!(state.step(2 * FRAME_NANOS), Action::Stop);
        assert_eq!(state.step(3 * FRAME_NANOS), Action::Stop);

        assert_eq!(
            recording.borrow().frames.len(),
            1,
            "only the frame that asked to stop ran; the two callbacks after it \
             must not reach the implementation"
        );
    }

    /// The first frame has no previous vsync to measure against, and every frame
    /// after it carries the interval.
    #[test]
    fn the_first_frame_has_no_delta_and_the_next_one_measures_the_interval() {
        let (mut state, _extent, recording) = scripted(Vec::new(), Vec::new(), true);

        run(&mut state, 2);

        let recording = recording.borrow();
        let deltas: Vec<f32> = recording.frames.iter().map(|(dt, _)| *dt).collect();
        assert_eq!(deltas[0], 0.0, "there is no previous vsync to subtract");
        // 1e-6 rather than something tighter: one f32 ulp at 0.0167 is about
        // 1.9e-9, so a smaller tolerance is exact equality wearing a tolerance's
        // clothes and would fail on a change as harmless as computing the delta
        // in f64 and casting at the end. 1e-6 is still far below any drift worth
        // reporting.
        assert!(
            (deltas[1] - FRAME_NANOS as f32 / 1_000_000_000.0).abs() < 1e-6,
            "the second frame carries one vsync interval, got {}",
            deltas[1]
        );
    }

    /// Shutting down stops the loop, releases what the implementation holds, and
    /// silences every callback that arrives afterwards.
    ///
    /// The last part is what makes it safe: `detach` is the point the destroy
    /// handshake waits on, and a late callback that still reached the
    /// implementation would be touching a surface that has already been given
    /// up. What this cannot see is the *order* of the two statements inside
    /// `shut_down` — a double that observed the flag from inside `detach` would
    /// be asserting on the fixture rather than on the loop.
    #[test]
    fn shutting_down_stops_the_loop_detaches_once_and_silences_later_callbacks() {
        let (mut state, _extent, recording) = scripted(Vec::new(), Vec::new(), true);
        run(&mut state, 1);
        assert!(state.is_running(), "one ordinary frame leaves it running");

        state.shut_down();

        assert!(!state.is_running(), "the poll loop reads this to leave");
        assert_eq!(
            recording.borrow().detaches,
            1,
            "the surface is released exactly once"
        );
        assert_eq!(
            state.step(9 * FRAME_NANOS),
            Action::Stop,
            "a posted vsync cannot be cancelled, so one can still arrive here"
        );
        assert_eq!(
            recording.borrow().frames.len(),
            1,
            "and it must not reach an implementation that has given up its surface"
        );
        assert_eq!(recording.borrow().detaches, 1, "nor detach a second time");
    }

    /// A zero extent is refused before it reaches [`Frames::resize`].
    ///
    /// `surfaceChanged` reports 0x0 during teardown and on some backgrounding
    /// transitions. Recording one as configured would leave the swapchain sized
    /// for a drawable with no pixels, and the guard that stops it is one `&&`
    /// away from deleted — with nothing else failing when it goes.
    #[test]
    fn a_zero_extent_is_never_offered_to_the_implementation() {
        let (mut state, extent, recording) = scripted(Vec::new(), Vec::new(), true);
        extent.store(pack(0, 0), Ordering::Release);

        run(&mut state, 3);

        assert!(
            recording.borrow().resizes.is_empty(),
            "a zero extent differs from what is configured, so only the \
             non-zero test keeps it out of the implementation: {:?}",
            recording.borrow().resizes
        );
    }
}
