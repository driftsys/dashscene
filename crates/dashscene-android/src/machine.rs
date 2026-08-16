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

use crate::frames::{AttachError, Frames, Step};
use crate::handshake::Handshake;
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

/// Whether an extent describes a drawable at all.
///
/// `surfaceChanged` reports 0x0 during teardown and on some backgrounding
/// transitions. **What that costs is not a crash**, and issue #1094 says
/// otherwise: `dashscene_gpu::SurfaceRenderer::configure` returns early on a
/// zero dimension and its `present` reports nothing drawn for one, so no zero
/// reaches `wgpu::Surface::configure`. What it costs is an adapter, a device
/// and the whole pipeline set acquired for a drawable with no pixels — 0.74 s
/// on an emulator for a release build and over 218 s for a debug one (issue
/// #960) — after which nothing is drawn until a drawable extent arrives, with
/// `surfaceDestroyed` parked behind the acquisition when the 0x0 came from a
/// teardown. `check_extent` naming that refusal rather than accepting it
/// silently is issue #1149, in the painter.
///
/// **One function because it is one rule** (issue #1094): [`publish_extent`]
/// asks it of what the UI thread reports, and [`LoopState::step`] asks it again
/// of what it reads back. A second copy of a rule is how this crate's recovery
/// path was broken three times.
fn is_drawable((width, height): (u32, u32)) -> bool {
    width > 0 && height > 0
}

/// Publishes an extent the UI thread reported, **unless it is not a drawable**
/// (issue #1094).
///
/// The cell is the only channel between `surfaceChanged` and the render thread,
/// and `loop_::resize` is its only writer. Storing a 0x0 into it destroys the
/// last extent that described a drawable, and the loop then has nothing to
/// build a surface for: [`LoopState::start`] reads this cell, so a
/// `surfaceChanged(0, 0)` landing between `loop_::start` spawning the render
/// thread and that thread reaching its acquisition was the extent the surface
/// got taken up at.
///
/// Dropping the report instead leaves the cell holding the most recent extent
/// that *was* a drawable, which is what both readers want. Keeping the created
/// extent beside the cell and falling back to it was written first and is
/// worse: after `surfaceChanged(1080, 2400)`, `surfaceChanged(720, 1612)` and
/// `surfaceChanged(0, 0)` it attaches at 1080x2400, a size the surface has not
/// had for two callbacks, where this attaches at 720x1612.
///
/// **Here rather than in `loop_::resize`** by this crate's own rule: the
/// decision binds no NDK symbol, and `loop_` is behind
/// `#[cfg(target_os = "android")]` where `just test`, `just test-regression`
/// and the `android-build` job — a compile only — all pass with it deleted.
/// `loop_::resize` is left holding the pointer check and nothing else.
pub(crate) fn publish_extent(cell: &AtomicU64, width: u32, height: u32) {
    if !is_drawable((width, height)) {
        // Logcat is the only witness a device gives, and a window that stopped
        // following its surface is otherwise a report with no trace of the
        // report that was dropped. Bounded by UI callbacks rather than by
        // frames: `surfaceChanged` runs on a lifecycle transition.
        log(&format!(
            "surfaceChanged reported {width}x{height}, which is not a drawable — not \
             publishing it"
        ));
        return;
    }
    cell.store(pack(width, height), Ordering::Release);
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

/// Which of the two acquisitions [`LoopState::acquire`] is making.
///
/// The path is one, which is the point of issue #1083: the teardown check, the
/// marker pair and the attach are the same rule, and a second copy of a rule is
/// how this crate's recovery path was broken three times. Only the wording
/// differs, and the wording is worth keeping — logcat is the only witness a
/// device gives, and "not attaching" and "not rebuilding the lost surface" are
/// different stories about the same surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Acquire {
    /// The surface is being taken up for the first time.
    First,
    /// A surface reported lost is being rebuilt.
    Rebuild,
}

/// What [`LoopState::shut_down`] leaves behind in place of the implementation
/// it drops (issue #1085).
///
/// **A value rather than an `Option`, and that is the point.** This struct is
/// leaked — a posted vsync callback cannot be cancelled, so the state has to
/// stay readable after the loop ends — so whatever it still holds is retained
/// for the life of the process, once per surface cycle. Dropping the real
/// implementation is what bounds that. Holding `Option<Box<dyn Frames>>`
/// instead put an unreachable `None` in front of every use, and the three
/// branches that answered it disagreed about whether to log and whether to
/// clear `running` — which is the shape of the defect this module exists to
/// keep out of the loop. There is nothing to disagree about here.
///
/// Zero-sized, so `Box::new(Released)` allocates nothing.
///
/// Every answer is the one a stopped loop needs: nothing to attach, no extent
/// taken up, and stop.
struct Released;

impl Frames for Released {
    unsafe fn attach(
        &mut self,
        _window: *mut c_void,
        _width: u32,
        _height: u32,
    ) -> Result<(), AttachError> {
        Err("the loop has shut down".to_owned())
    }

    fn resize(&mut self, _width: u32, _height: u32) -> bool {
        false
    }

    fn frame(&mut self, _dt: f32, _forced: bool) -> Step {
        Step::Stop
    }

    fn detach(&mut self) {}
}

/// The render thread's own state, reachable from the vsync callback.
pub(crate) struct LoopState {
    /// What draws. **Replaced by [`Released`] in [`LoopState::shut_down`]**,
    /// which drops it — the whole of issue #1085.
    ///
    /// This struct is leaked, so whatever it still holds is retained with it,
    /// once per surface cycle, for the life of the process. [`Frames::detach`]
    /// asks an implementation to release what it owns for exactly that reason,
    /// and `DocumentFrames` deliberately does not release its document or its
    /// faces, because a rebuild after a recoverable surface loss needs them.
    /// That retention was about 1 kB when the trade was priced and 328 324 B
    /// once the harness staged a font file and a committed sheet — every
    /// rotation, every split-screen transition and every backgrounding.
    ///
    /// Dropping the box bounds what the leak retains to this struct's own
    /// fields, whatever an implementation holds. That is a property the crate
    /// can enforce, where the trait's requirement is only a request.
    frames: Box<dyn Frames>,
    /// The window, kept so a lost surface can be rebuilt from it. Live for the
    /// whole loop: the destroy handshake is what makes that true.
    window: usize,
    extent: Arc<AtomicU64>,
    /// The extent the surface is currently configured for, and `(0, 0)` until
    /// [`LoopState::start`] has taken one up.
    ///
    /// Written in two places and no others: `start`, from the cell, and the
    /// accepted-resize arm of [`LoopState::step`]. [`LoopState::acquire`] only
    /// reads it.
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
    /// Read before **both** of this crate's device acquisitions, by the one
    /// path they share — see [`LoopState::acquire`].
    ///
    /// The rebuild's copy of that check moved here at issue #960; the first
    /// attach's stayed in `loop_::render_thread`, behind the platform `cfg`,
    /// because the state it would have read did not exist until after the
    /// attach had returned. Issue #1083 is that gap, and [`LoopState::start`]
    /// is what closes it: the state is built un-attached and makes its own
    /// first attach, so there is one guarded path rather than one guarded and
    /// one compiled-but-never-run.
    handshake: Arc<Handshake>,
}

impl LoopState {
    /// The state a loop begins in, with **nothing attached yet**.
    ///
    /// [`LoopState::start`] is what takes the surface up; until it has, there
    /// is no configured extent and the implementation has had no call made on
    /// it.
    pub(crate) fn new(
        frames: Box<dyn Frames>,
        window: usize,
        extent: Arc<AtomicU64>,
        handshake: Arc<Handshake>,
    ) -> Self {
        Self {
            frames,
            window,
            extent,
            // Nothing is taken up yet, and `start` overwrites this with the
            // extent it attaches at.
            configured: (0, 0),
            previous: None,
            // The first frame is one of the cases the generation cannot report.
            forced: true,
            running: true,
            vsyncs: 0,
            frames_run: 0,
            rebuilds: 0,
            handshake,
        }
    }

    /// **Takes up the surface for the first time**, and reports whether the
    /// loop should run.
    ///
    /// Here rather than in `loop_::render_thread`, which is the whole of issue
    /// #1083. The teardown check before an acquisition binds no NDK symbol, so
    /// by this crate's own rule it belongs where a host test can reach it — and
    /// until this moved, the first attach's copy of that rule was the one
    /// decision still behind `#[cfg(target_os = "android")]`, where
    /// `android-build` compiles it and no test tier runs it. That is the exact
    /// arrangement that shipped three consecutive broken repairs to the
    /// recovery path (issues #888, #940).
    ///
    /// `false` leaves the loop stopped and the caller goes straight to
    /// [`LoopState::shut_down`].
    ///
    /// `#[must_use]` for the reason [`Action`] carries one: the only caller is
    /// in `loop_`, behind the platform `cfg` where no test can see it and
    /// `android-build` only compiles. A refactor writing `state.start();`
    /// would leak the state and post a vsync callback for a surface the
    /// teardown check had just refused, and nothing but a device would say so.
    ///
    /// # What the cell holds, and why this needs no guard of its own (issue #1094)
    ///
    /// The extent is read here rather than handed in, so the surface is taken
    /// up at the last extent the UI thread reported: `surfaceChanged` can land
    /// between `loop_::start` spawning this thread and this thread getting
    /// here. **What lands could be 0x0** — that is what `surfaceChanged`
    /// reports during teardown and on some backgrounding transitions — and it
    /// was read as an extent like any other and handed to `Frames::attach`.
    ///
    /// That is closed at the writer rather than here: [`publish_extent`] keeps
    /// a report that is not a drawable out of the cell, so what this reads is
    /// the most recent extent that was one. Guarding here as well would be a
    /// second copy of the rule with nothing better to offer — the fallback it
    /// would need is the extent the loop was created with, which is staler than
    /// what the cell already holds.
    ///
    /// **The cell can still hold a 0x0 that never went through
    /// [`publish_extent`]**: `loop_::start` seeds it with whatever
    /// `nativeSurfaceCreated` was given, and a host that starts a loop for a
    /// surface with no pixels gets an attach at 0x0. That is the host's call to
    /// make and its cost is bounded — the surface is left unconfigured, nothing
    /// is drawn, and the first drawable `surfaceChanged` recovers it through
    /// [`LoopState::step`]. Refusing instead strands it: `start` answering
    /// `false` stops the render thread, and no later `surfaceChanged` restarts
    /// it.
    #[must_use]
    pub(crate) fn start(&mut self) -> bool {
        self.configured = unpack(self.extent.load(Ordering::Acquire));
        self.acquire(Acquire::First)
    }

    /// **The guarded acquisition both attaches run through**, and the answer to
    /// issue #1083.
    ///
    /// Returns whether the surface was taken up. `false` always leaves
    /// `running` clear, so a caller that ignored the answer still stops —
    /// `#[must_use]` all the same, for the reason [`LoopState::start`] gives.
    #[must_use]
    fn acquire(&mut self, which: Acquire) -> bool {
        // **Asked before the acquisition, not after it.** An attach acquires an
        // adapter, a device and the whole pipeline set: 0.74 s for a release
        // build on an emulator and over 218 s for a debug one (issue #960).
        // `surfaceDestroyed` is parked in `request_teardown` for the whole of
        // it, and a surface that has already been asked to go away has no use
        // for a device.
        //
        // It closes the window before the attach and **not the one inside it**:
        // once the call below is entered nothing here runs again until it
        // returns, which is the whole of issue #960.
        if self.handshake.teardown_requested() {
            log(match which {
                Acquire::First => {
                    "teardown was requested before the surface was taken up — not attaching"
                }
                Acquire::Rebuild => "teardown was requested — not rebuilding the lost surface",
            });
            self.running = false;
            return false;
        }

        if which == Acquire::Rebuild {
            log("the surface was lost — rebuilding");
            // The lost surface is given up first — and after the check above,
            // because dropping it for a rebuild that is not going to happen
            // gains a pending teardown nothing.
            self.frames.detach();
        }

        let (width, height) = self.configured;
        // **The marker pair, on both paths.** An acquisition in flight and one
        // that never started are otherwise the same picture in logcat — no line
        // either way — which is what left issue #960 reading as "draws nothing
        // and reports nothing" and made a wedged acquisition take a person
        // watching an emulator to find. Every acquisition writes `attaching`
        // and then exactly one of `attached`, `attach failed:` or `could not
        // rebuild the surface:` — that is the three-way reading the harness and
        // the split-screen recipe both state (issue #1080), and a path writing
        // none of the four would make it false. A missing `attached` alone
        // means nothing: only `attaching` with none of the other three after it
        // is a thread still inside the call.
        log(&format!("attaching a {width}x{height} surface"));
        // SAFETY: the window outlives the loop — the destroy handshake is what
        // makes that true — so it is as live now as it was when the thread was
        // handed it.
        let attached = unsafe {
            self.frames
                .attach(self.window as *mut c_void, width, height)
        };
        match attached {
            Ok(()) => {
                // The other half of the marker pair. Without it a successful
                // acquisition leaves an `attaching` line with nothing after it,
                // which is exactly the shape a wedge is read by.
                log(&format!("attached a {width}x{height} surface"));
                // The device has drawn nothing and the scene has not changed,
                // so the generation cannot ask for this frame.
                self.forced = true;
                true
            }
            Err(error) => {
                log(&match which {
                    Acquire::First => format!("attach failed: {error}"),
                    Acquire::Rebuild => format!("could not rebuild the surface: {error}"),
                });
                self.running = false;
                false
            }
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
    /// stops the loop, detaches exactly once and drops the implementation, and
    /// that a callback arriving afterwards does nothing; the ordering of the
    /// statements is not observable from outside and is held by this comment
    /// and by review.
    ///
    /// **The implementation is dropped, and not merely detached** (issue
    /// #1085). This struct is leaked, so anything it still holds is retained
    /// for the life of the process — once per rotation, per split-screen
    /// transition and per backgrounding. `detach` asks an implementation to
    /// release what it owns, and `DocumentFrames` deliberately keeps its
    /// document and its faces because a rebuild needs them; taking the box here
    /// makes that keeping cost nothing past the surface it was for.
    pub(crate) fn shut_down(&mut self) {
        self.running = false;
        self.frames.detach();
        // Dropped on the render thread, before the loop releases the handshake
        // and therefore before the UI thread releases the window. Nothing here
        // holds the surface any more — `detach` above is what the handshake
        // waits on — so this is the implementation's own memory and nothing
        // else. The assignment is what drops it; [`Released`] is what a late
        // callback finds, and it needs no branch anywhere else.
        self.frames = Box::new(Released);
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
        if wanted != self.configured && is_drawable(wanted) {
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
        // Bound before the match rather than matched on directly, so the borrow
        // of `self.frames` ends here — the rebuild arm below calls back into
        // `self`.
        let step = self.frames.frame(dt, forced);
        match step {
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

                // **The same path the first attach takes** — the teardown
                // check, the give-up, the marker pair and the attach are one
                // rule, and issue #1083 is what it cost to have written it
                // twice. `acquire` clears `running` on every refusal, so a
                // `false` here is already a stopped loop.
                //
                // **Falls through to the reschedule below when it succeeds.**
                // Returning `Action::Reschedule` from inside this arm is what a
                // first cut did, and it left a recovered surface with no
                // pending callback and no way to acquire one: the loop stayed
                // `running`, the poll loop spun on its 100 ms timeout,
                // `is_running` kept answering true, and the window was frozen
                // until `surfaceDestroyed`. A recovery that stops the thing it
                // recovered is worse than no recovery, because it reports
                // success.
                if !self.acquire(Acquire::Rebuild) {
                    return Action::Stop;
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
    use std::cell::{Cell, RefCell};
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
        /// Set when this value is dropped, so a test can tell being detached
        /// from being released (issue #1085). `Rc<Cell<_>>` rather than a field
        /// read afterwards, because the whole question is whether the value is
        /// still there to read.
        dropped: Rc<Cell<bool>>,
    }

    impl Drop for ScriptedFrames {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
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

    /// The extent every fixture below starts at.
    ///
    /// Deliberately not square and not a swap of itself, so that a width read as
    /// a height cannot pass.
    const CONFIGURED: (u32, u32) = (1080, 2400);

    /// An un-attached state machine and everything a test needs to observe it.
    ///
    /// A struct rather than a tuple because the tuple crossed
    /// `clippy::type_complexity` at five members, and because `dropped` and
    /// `recording` answer questions that are easy to swap when they are
    /// positional.
    struct Fixture {
        state: LoopState,
        extent: Arc<AtomicU64>,
        recording: Rc<RefCell<Recording>>,
        handshake: Arc<Handshake>,
        /// Set when the scripted implementation is dropped (issue #1085).
        dropped: Rc<Cell<bool>>,
    }

    /// An un-attached state machine over a scripted [`Frames`], exactly as
    /// `loop_::render_thread` builds one.
    fn un_started(
        outcomes: Vec<Step>,
        attaches: Vec<Result<(), AttachError>>,
        accept_resize: bool,
    ) -> Fixture {
        let recording = Rc::new(RefCell::new(Recording::default()));
        let extent = Arc::new(AtomicU64::new(pack(CONFIGURED.0, CONFIGURED.1)));
        let handshake = Arc::new(Handshake::new());
        let dropped = Rc::new(Cell::new(false));
        let frames = ScriptedFrames {
            outcomes: outcomes.into(),
            attaches: attaches.into(),
            accept_resize,
            recording: Rc::clone(&recording),
            dropped: Rc::clone(&dropped),
        };
        let state = LoopState::new(
            Box::new(frames),
            WINDOW,
            Arc::clone(&extent),
            Arc::clone(&handshake),
        );
        Fixture {
            state,
            extent,
            recording,
            handshake,
            dropped,
        }
    }

    /// A state machine that has **already made its first attach**, which is
    /// where the vsync callback takes over.
    ///
    /// The fixture's own first attach always succeeds and is then wiped from the
    /// recording, so `attaches` describes the attaches *after* it and every
    /// assertion below counts only what the frames under test asked for.
    ///
    /// **Returns the same [`Fixture`] [`un_started`] does** (issue #1093). It
    /// returned a four-tuple, and a second wrapper returned three of those four,
    /// so one fixture had three arities and eleven tests destructured it
    /// positionally — which put `dropped` out of reach of every test written
    /// against a tuple, including all of the rebuild ones. Named fields cost a
    /// prefix per use and make every observation reachable from every test.
    fn scripted(
        outcomes: Vec<Step>,
        attaches: Vec<Result<(), AttachError>>,
        accept_resize: bool,
    ) -> Fixture {
        // Prepended rather than the caller writing it: the script is about the
        // rebuilds a test is exercising, and a fixture that made its own first
        // attach consume the first scripted answer would fail every rebuild
        // test for a reason that has nothing to do with rebuilding.
        let mut attaches: VecDeque<_> = attaches.into();
        attaches.push_front(Ok(()));
        let mut fixture = un_started(outcomes, attaches.into(), accept_resize);
        assert!(
            fixture.state.start(),
            "the fixture's own first attach is scripted to succeed"
        );
        *fixture.recording.borrow_mut() = Recording::default();
        fixture
    }

    /// Parks a thread in [`Handshake::request_teardown`] and waits for the
    /// request to land.
    ///
    /// `request_teardown` blocks by design, so the request has to come from
    /// another thread. Only the handshake crosses: `LoopState` holds an `Rc` and
    /// cannot. The returned handle must be joined after [`Handshake::released`].
    fn park_a_teardown(handshake: &Arc<Handshake>) -> std::thread::JoinHandle<()> {
        let waiter = {
            let handshake = Arc::clone(handshake);
            std::thread::spawn(move || {
                handshake.request_teardown_every(std::time::Duration::from_secs(30), |_| {});
            })
        };
        while !handshake.teardown_requested() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        waiter
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
        let mut fixture = scripted(vec![Step::Rebuild; attempts], Vec::new(), true);

        let actions = run(&mut fixture.state, attempts);

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
            !fixture.state.running,
            "giving up must clear `running`, or the poll loop spins on its 100 ms \
             timeout and `is_running` keeps answering true"
        );
        assert_eq!(
            fixture.recording.borrow().attaches.len(),
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
        let mut fixture = scripted(outcomes, Vec::new(), true);

        let actions = run(&mut fixture.state, total);

        assert!(
            actions.iter().all(|action| *action == Action::Reschedule),
            "the frame at index 2 cleared the count, so the {MAX_CONSECUTIVE_REBUILDS} \
             rebuilds after it are all inside the bound: {actions:?}"
        );
        assert!(fixture.state.running, "nothing here reached the bound");
    }

    /// A rebuild re-attaches the same window at the configured extent and keeps
    /// the loop scheduled. PR #887's defect was the reschedule going missing.
    #[test]
    fn a_rebuild_detaches_re_attaches_the_same_window_and_reschedules() {
        let mut fixture = scripted(vec![Step::Rebuild], Vec::new(), true);

        assert_eq!(fixture.state.step(FRAME_NANOS), Action::Reschedule);

        let recording = fixture.recording.borrow();
        assert_eq!(recording.detaches, 1, "the lost surface is dropped first");
        assert_eq!(
            recording.attaches,
            vec![(WINDOW, 1080, 2400)],
            "the same window, at the extent the surface is configured for"
        );
    }

    /// **A teardown asked for while the surface is lost stops the loop instead
    /// of acquiring a device for a surface that is going away** (issue #960).
    ///
    /// The rebuild is the second place this crate attaches, and it was the
    /// unguarded one: the first attach reads the request in
    /// `loop_::render_thread`, and this path read it nowhere. On a debug build
    /// that re-attach was measured at over 218 s, and `surfaceDestroyed` is
    /// parked for the whole of it — so the wedge the issue reports had a second
    /// entrance that the fix for the first did not close.
    #[test]
    fn a_teardown_requested_while_the_surface_is_lost_stops_instead_of_re_attaching() {
        let mut fixture = scripted(vec![Step::Rebuild], Vec::new(), true);
        let waiter = park_a_teardown(&fixture.handshake);

        assert_eq!(
            fixture.state.step(FRAME_NANOS),
            Action::Stop,
            "the surface was lost and a teardown is pending — rebuilding here \
             acquires a device the handshake is already waiting to have dropped"
        );
        assert!(
            !fixture.state.running,
            "and the poll loop must read that it stopped"
        );
        assert!(
            fixture.recording.borrow().attaches.is_empty(),
            "no re-attach: the whole point is that the acquisition does not \
             start, got {:?}",
            fixture.recording.borrow().attaches
        );

        // The loop would reach `shut_down` and drop its `ReleaseOnExit`; here
        // that is done by hand so the parked thread comes back.
        fixture.handshake.released();
        waiter.join().unwrap();
    }

    /// **A teardown asked for before the surface has been taken up stops
    /// instead of attaching** (issues #960, #1083).
    ///
    /// This is the check that could not be tested at all until the first attach
    /// moved here: it lived in `loop_::render_thread`, behind
    /// `#[cfg(target_os = "android")]`, where `just test`,
    /// `just test-regression` and the `android-build` job — a compile only —
    /// all pass with it deleted. Its sibling on the rebuild path has had a test
    /// since #960; this one is what #1083 was filed for.
    ///
    /// A rotation during startup produces exactly this: the surface is asked to
    /// go away before the render thread has reached its acquisition, and
    /// `surfaceDestroyed` is parked in `request_teardown` for as long as that
    /// acquisition takes — over 218 s on a debug build.
    #[test]
    fn a_teardown_requested_before_the_first_attach_stops_instead_of_attaching() {
        let mut fixture = un_started(Vec::new(), Vec::new(), true);
        let waiter = park_a_teardown(&fixture.handshake);

        assert!(
            !fixture.state.start(),
            "a teardown is pending — taking the surface up here acquires a \
             device the handshake is already waiting to have dropped"
        );
        assert!(
            !fixture.state.is_running(),
            "and the poll loop must read that it stopped"
        );
        assert!(
            fixture.recording.borrow().attaches.is_empty(),
            "no attach: the whole point is that the acquisition does not start, \
             got {:?}",
            fixture.recording.borrow().attaches
        );

        // **What `render_thread` does next**, and the reason `Frames::detach`'s
        // contract now names this case: the refusal falls through to
        // `shut_down`, so an implementation is detached having never been
        // attached at all. Before the first attach moved here that path
        // returned without detaching, and an implementation written against
        // the old contract would be the one to find out.
        fixture.state.shut_down();
        assert_eq!(
            fixture.recording.borrow().detaches,
            1,
            "detached exactly once, with no attach behind it"
        );
        assert!(fixture.dropped.get(), "and the implementation is dropped");

        fixture.handshake.released();
        waiter.join().unwrap();
    }

    /// The first attach takes the surface up at the extent the UI thread last
    /// reported, and leaves the loop running.
    ///
    /// The extent is read by `start` rather than handed to the constructor, so
    /// a `surfaceChanged` that lands between the thread being spawned and the
    /// thread getting here is the one the surface is built for.
    #[test]
    fn the_first_attach_takes_the_surface_up_at_the_reported_extent() {
        let mut fixture = un_started(Vec::new(), Vec::new(), true);
        fixture.extent.store(pack(720, 1612), Ordering::Release);

        assert!(
            fixture.state.start(),
            "an attach that succeeds starts the loop"
        );
        assert!(fixture.state.is_running());
        assert_eq!(
            fixture.recording.borrow().attaches,
            vec![(WINDOW, 720, 1612)],
            "the window the loop was given, at the extent the UI thread last \
             reported rather than the one the thread was spawned with"
        );

        // And it is recorded as configured: the same extent must not then be
        // offered to `resize` as though it were a change.
        run(&mut fixture.state, 1);
        assert!(
            fixture.recording.borrow().resizes.is_empty(),
            "the attach extent is what the surface is configured for, got {:?}",
            fixture.recording.borrow().resizes
        );
    }

    /// **A report that is not a drawable never reaches the cell, so the first
    /// attach uses the last extent that was one** (issue #1094).
    ///
    /// The whole of the defect, driven through the writer the UI thread calls
    /// rather than by storing into the cell directly: `surfaceChanged` reports
    /// 0x0 during teardown and on some backgrounding transitions, and one
    /// landing between `loop_::start` spawning the render thread and
    /// `LoopState::start` reading the cell was the extent the surface got taken
    /// up at.
    ///
    /// **Three reports, not one**, and that is what makes this test observe the
    /// fix rather than agree with it. A fallback to the extent the loop was
    /// created with — the first shape this took — attaches at `CONFIGURED`
    /// here, because it has no memory of the 720x1612 in between. Keeping the
    /// cell's last drawable value attaches at 720x1612, which is the size the
    /// surface actually has.
    ///
    /// Every partly-zero pair, not only 0x0: a zero on either axis is the same
    /// rule, so a one-sided case must not slip past.
    #[test]
    fn a_report_that_is_not_a_drawable_never_displaces_the_last_one_that_was() {
        for not_drawable in [(0, 0), (0, 2400), (1080, 0)] {
            let mut fixture = un_started(Vec::new(), Vec::new(), true);
            publish_extent(&fixture.extent, 720, 1612);
            publish_extent(&fixture.extent, not_drawable.0, not_drawable.1);

            assert!(
                fixture.state.start(),
                "an attach that succeeds starts the loop"
            );
            assert_eq!(
                fixture.recording.borrow().attaches,
                vec![(WINDOW, 720, 1612)],
                "the last extent that described a drawable, not the {not_drawable:?} \
                 reported after it and not the {CONFIGURED:?} the loop was \
                 created with"
            );
        }
    }

    /// A drawable report does reach the cell, which is what stops the guard
    /// above being satisfied by a writer that stores nothing at all.
    #[test]
    fn a_drawable_report_reaches_the_cell() {
        let extent = Arc::new(AtomicU64::new(pack(CONFIGURED.0, CONFIGURED.1)));

        publish_extent(&extent, 720, 1612);

        assert_eq!(
            unpack(extent.load(Ordering::Acquire)),
            (720, 1612),
            "a report that describes a drawable is what the cell is for"
        );
    }

    /// **A cell seeded with a 0x0 still attaches**, and the loop recovers on the
    /// first drawable report.
    ///
    /// `loop_::start` seeds the cell with whatever `nativeSurfaceCreated` was
    /// given, and that seed does not go through [`publish_extent`] — so a host
    /// that starts a loop for a surface with no pixels reaches `attach` at 0x0
    /// however the writer is guarded.
    ///
    /// Refusing there is the obvious reading of issue #1094 and is worse than
    /// the defect: `start` answering `false` stops the render thread, and
    /// nothing restarts it — a later `surfaceChanged` would publish into a cell
    /// no thread is reading any more, leaving the window blank until the surface
    /// cycled. This pins the recovery that makes attaching the right answer.
    #[test]
    fn a_cell_seeded_with_no_drawable_extent_still_starts_and_recovers() {
        let mut fixture = un_started(Vec::new(), Vec::new(), true);
        fixture.extent.store(pack(0, 0), Ordering::Release);

        assert!(
            fixture.state.start(),
            "refusing here strands the surface: no later surfaceChanged reaches \
             a thread that has stopped"
        );
        assert!(fixture.state.is_running());
        assert_eq!(
            fixture.recording.borrow().attaches,
            vec![(WINDOW, 0, 0)],
            "there is nothing better to attach at, and the alternative is a \
             render thread that has stopped"
        );

        publish_extent(&fixture.extent, 720, 1612);
        run(&mut fixture.state, 1);

        assert_eq!(
            fixture.recording.borrow().resizes,
            vec![(720, 1612)],
            "the first drawable report is a change from the 0x0 that was \
             configured, so `step` takes it up"
        );
    }

    /// A first attach that fails stops the loop and gives up whatever it built
    /// partway.
    ///
    /// An attach fails **partway**, and what it built before failing is exactly
    /// what leaks: `DocumentFrames::attach` creates the runtime first and stores
    /// it before anything else can fail, precisely so a `detach` has the pointer
    /// to free. On a device where the attach keeps failing — issue #960's
    /// emulator is one — that is a whole runtime, and on the surface path a wgpu
    /// device with it, once per surface cycle.
    #[test]
    fn a_first_attach_that_fails_stops_the_loop_and_still_detaches() {
        let mut fixture = un_started(Vec::new(), vec![Err("no adapter".to_owned())], true);

        assert!(
            !fixture.state.start(),
            "the attach failed, so there is no loop"
        );
        assert!(!fixture.state.is_running());
        assert_eq!(
            fixture.recording.borrow().attaches,
            vec![(WINDOW, CONFIGURED.0, CONFIGURED.1)],
            "it was attempted once and not retried"
        );

        // The caller goes straight to `shut_down`, which is what gives up the
        // partway attach.
        fixture.state.shut_down();
        assert_eq!(
            fixture.recording.borrow().detaches,
            1,
            "`Frames::detach` tolerates having nothing to release, and a failed \
             attach is exactly the case it tolerates it for"
        );
        assert!(fixture.dropped.get(), "and the implementation goes with it");
    }

    /// **Shutting down drops the implementation, and not only its surface**
    /// (issue #1085).
    ///
    /// The loop's state is leaked — a posted vsync callback cannot be cancelled,
    /// so it has to stay readable after the loop ends — and it holds the
    /// `Frames` box. `Frames::detach` asks an implementation to release what it
    /// owns for that reason, and `DocumentFrames` deliberately keeps its
    /// document and its faces because a rebuild needs them: about 328 kB per
    /// surface cycle once the harness staged a font file and a committed sheet,
    /// against about 1 kB when the trade was priced. A surface cycle is every
    /// rotation, every split-screen transition and every backgrounding.
    ///
    /// Detaching is what the destroy handshake waits on; dropping is what bounds
    /// the leak to this crate's own fields whatever an implementation holds.
    /// Nothing but the drop can say the second happened, which is the one
    /// assertion this makes that
    /// `shutting_down_stops_the_loop_detaches_once_and_silences_later_callbacks`
    /// cannot — the silencing and the detach-once are that test's, and are not
    /// repeated here.
    #[test]
    fn shutting_down_drops_the_implementation_rather_than_retaining_it() {
        let mut fixture = un_started(Vec::new(), Vec::new(), true);
        assert!(fixture.state.start());
        run(&mut fixture.state, 1);
        assert!(
            !fixture.dropped.get(),
            "a running loop still holds what it draws with"
        );

        fixture.state.shut_down();

        assert_eq!(
            fixture.recording.borrow().detaches,
            1,
            "the surface is given up first — that is what the handshake waits on"
        );
        assert!(
            fixture.dropped.get(),
            "and then the implementation itself, or the leaked state retains it \
             for the life of the process, once per rotation"
        );
    }

    /// A rebuild whose re-attach fails stops the loop rather than rescheduling
    /// into a surface that does not exist.
    ///
    /// And the implementation it built partway is dropped, not merely detached
    /// — the same rule as the first attach's, on the path that runs once per
    /// recoverable surface loss rather than once per surface. Reachable only
    /// since issue #1093: the rebuild tests destructured a tuple that left
    /// `dropped` out.
    #[test]
    fn a_rebuild_that_cannot_re_attach_stops_the_loop() {
        let mut fixture = scripted(
            vec![Step::Rebuild],
            vec![Err("no adapter".to_owned())],
            true,
        );

        assert_eq!(fixture.state.step(FRAME_NANOS), Action::Stop);
        assert!(!fixture.state.running);

        // What `render_thread` does next once the poll loop reads `is_running`.
        fixture.state.shut_down();
        assert!(
            fixture.dropped.get(),
            "a re-attach fails partway too, and what it built is retained for \
             the life of the process unless the box goes with it"
        );
    }

    /// **A refused resize is offered again.** `Frames::resize` answering `false`
    /// — an extent past the adapter maximum, issue #714 — must not be recorded
    /// as configured, or the scene stays laid out for the old size for the rest
    /// of the surface's life.
    #[test]
    fn a_refused_resize_is_offered_again_on_the_next_frame() {
        let mut fixture = scripted(Vec::new(), Vec::new(), false);
        fixture.extent.store(pack(720, 1612), Ordering::Release);

        run(&mut fixture.state, 3);

        assert_eq!(
            fixture.recording.borrow().resizes,
            vec![(720, 1612); 3],
            "a refusal is not believed, so the same extent is offered every frame"
        );
    }

    /// An accepted resize is recorded, so it is offered once and not again.
    #[test]
    fn an_accepted_resize_is_offered_once() {
        let mut fixture = scripted(Vec::new(), Vec::new(), true);
        fixture.extent.store(pack(720, 1612), Ordering::Release);

        run(&mut fixture.state, 3);

        assert_eq!(
            fixture.recording.borrow().resizes,
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
        let mut fixture = scripted(
            vec![Step::Continue, Step::Continue, Step::Rebuild],
            Vec::new(),
            true,
        );

        // Frames one and two: the first is forced because the surface has drawn
        // nothing, the second has nothing to force it.
        run(&mut fixture.state, 2);
        // Frame three takes up a new extent, and frame four sees the flag it set.
        fixture.extent.store(pack(720, 1612), Ordering::Release);
        run(&mut fixture.state, 2);

        assert_eq!(
            fixture
                .recording
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
        let mut fixture = scripted(vec![Step::Stop], Vec::new(), true);

        assert_eq!(fixture.state.step(FRAME_NANOS), Action::Stop);
        assert_eq!(fixture.state.step(2 * FRAME_NANOS), Action::Stop);
        assert_eq!(fixture.state.step(3 * FRAME_NANOS), Action::Stop);

        assert_eq!(
            fixture.recording.borrow().frames.len(),
            1,
            "only the frame that asked to stop ran; the two callbacks after it \
             must not reach the implementation"
        );
    }

    /// The first frame has no previous vsync to measure against, and every frame
    /// after it carries the interval.
    #[test]
    fn the_first_frame_has_no_delta_and_the_next_one_measures_the_interval() {
        let mut fixture = scripted(Vec::new(), Vec::new(), true);

        run(&mut fixture.state, 2);

        let recording = fixture.recording.borrow();
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
        let mut fixture = scripted(Vec::new(), Vec::new(), true);
        run(&mut fixture.state, 1);
        assert!(
            fixture.state.is_running(),
            "one ordinary frame leaves it running"
        );

        fixture.state.shut_down();

        assert!(
            !fixture.state.is_running(),
            "the poll loop reads this to leave"
        );
        assert_eq!(
            fixture.recording.borrow().detaches,
            1,
            "the surface is released exactly once"
        );
        assert_eq!(
            fixture.state.step(9 * FRAME_NANOS),
            Action::Stop,
            "a posted vsync cannot be cancelled, so one can still arrive here"
        );
        assert_eq!(
            fixture.recording.borrow().frames.len(),
            1,
            "and it must not reach an implementation that has given up its surface"
        );
        assert_eq!(
            fixture.recording.borrow().detaches,
            1,
            "nor detach a second time"
        );
    }

    /// A zero extent is refused before it reaches [`Frames::resize`].
    ///
    /// `surfaceChanged` reports 0x0 during teardown and on some backgrounding
    /// transitions. Recording one as configured would leave the swapchain sized
    /// for a drawable with no pixels, and the guard that stops it is one `&&`
    /// away from deleted — with nothing else failing when it goes.
    ///
    /// **A zero on either axis, not only 0x0.** Only the square case was driven
    /// until issue #1094, so weakening [`is_drawable`]'s `&&` to `||` failed
    /// nothing on this path — the exact mutation the paragraph above warns
    /// about, left open by the fixture it was written against.
    #[test]
    fn a_zero_extent_is_never_offered_to_the_implementation() {
        for extent in [(0, 0), (0, 2400), (1080, 0)] {
            let mut fixture = scripted(Vec::new(), Vec::new(), true);
            fixture
                .extent
                .store(pack(extent.0, extent.1), Ordering::Release);

            run(&mut fixture.state, 3);

            assert!(
                fixture.recording.borrow().resizes.is_empty(),
                "{extent:?} differs from what is configured, so only the \
                 drawable test keeps it out of the implementation: {:?}",
                fixture.recording.borrow().resizes
            );
        }
    }
}
