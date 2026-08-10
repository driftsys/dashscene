//! The demonstration, as `dashscene_desktop::App` sees it (story #572; the
//! integration half extracted at story #794).
//!
//! # What is left here, and why that is the point
//!
//! The window-to-surface handoff, the frame loop, the generation gate,
//! rebuilding on resize and the `.dsb` load are **not here any more**. They are
//! `dashscene-desktop`, because every windowed embedder would otherwise write
//! them.
//!
//! What remains is the demonstration, and it is exactly what an embedder writes
//! for itself: which scene to draw and when to move to the next one, the
//! scripted signal producer and the thread it runs on, the input mapping, the
//! painter choice behind the swap key, and the frame-cost instrument.
//!
//! P3 still holds, and for the same reason it did when the loop was here: the
//! scripted producer sends a [`dashscene_desktop::Waker`] from its own thread
//! and writes nothing, and the loop applies the change on its own thread before
//! the next `tick`.

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_desktop::{App, DesktopError, Present, PresentError, Reaction, Scene, Waker};
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes};

use crate::painter::Choice;

/// Builds a scene into `arena` for a drawable of `width` x `height` physical
/// pixels, and returns the live scene the loop ticks.
///
/// The builder owns its solver: `LiveScene` retains one and reuses it for every
/// reflow.
pub type SceneBuilder = fn(&mut Arena, u32, u32) -> LiveScene;

/// Applies the scene's scripted signal change for pulse number `index`.
///
/// A plain `fn` rather than a closure, and a `u64` rather than captured state,
/// so the phase lives in this demonstration and the scene stays a pure function
/// of it — which is what lets [`Showcase::build`] re-apply the current phase to
/// a scene the loop just rebuilt for a new extent.
///
/// [`Showcase::build`]: dashscene_desktop::App::build
pub type ScenePulse = fn(&mut LiveScene, u64);

/// Runs the scene's own variant switch (story #573). The one thing input does
/// that a signal write cannot express, because `Txn::set_variant` needs the
/// arena.
///
/// A scene owns the switch because a scene owns its content: it declares the
/// variant set inside its own builder, where it holds the arena, and hands this
/// demonstration a function that switches it. Nothing here holds a
/// `VariantSetId`, a member list or a node name — see [`crate::input`], and
/// issue #625 for the seam this closes.
pub type SceneAction = fn(&mut LiveScene, &mut Arena);

/// The window's requested size, in logical pixels.
///
/// This demonstration's own choice, and not the integration crate's default —
/// which is what an embedder that names no window gets, and is deliberately a
/// separate decision from this one.
const WINDOW_SIZE: LogicalSize<u32> = LogicalSize::new(960, 600);

/// The key that swaps the painter on the running window (story #585).
///
/// A demonstration key, not an integration one: a scene declares the signal the
/// pointer drives and the function a key runs, and which painter draws it is
/// neither. `P` is unmapped by every scene, so nothing it did before is lost.
const SWAP_PAINTER: KeyCode = KeyCode::KeyP;

/// How often the placeholder driver pulses the scene's signals.
///
/// Long enough that the loop visibly settles and parks between pulses, which
/// is what makes the idle skip observable in the log rather than asserted.
const PULSE_INTERVAL: Duration = Duration::from_millis(2500);

/// How long a scene is shown for before this demonstration moves to the next
/// one, when it has more than one to show (issue #628).
///
/// Expressed as a number of pulse intervals because the scene changes *on* a
/// pulse: it waits for this long to elapse and then advances at the next pulse,
/// so a scene is never cut mid-transition. Four rather than one because each
/// scene's script alternates direction, so an even count leaves the scene at the
/// phase it started from.
///
/// **The deadline is elapsed time, not a count of pulse events.** It was
/// written that way because [`spawn_pulse_driver`] used to free-run: it slept
/// and sent whether or not the loop had consumed the last message, so on a slow
/// frame several pulses queued and arrived together, and counting the events
/// gave later scenes a single pulse each while the first cycle got four.
///
/// The driver no longer does that — it is one-shot and rearmed
/// ([`spawn_pulses`], issue #629) — so counting would now be sound. Elapsed time
/// stays, because it is the better basis anyway: it says what it means, that a
/// scene holds the window for a duration a person can watch, and it does not
/// depend on the pulse rate remaining what it happens to be today.
const PULSES_PER_SCENE: u32 = 4;

/// How long each scene holds the window, derived from the two constants above
/// so the two cannot drift apart.
const SCENE_DWELL: Duration =
    Duration::from_millis(PULSE_INTERVAL.as_millis() as u64 * PULSES_PER_SCENE as u64);

/// One entry in the scene list: what to build, how to script it, and the name
/// to report when it comes up.
///
/// These are held rather than the `corpus/showcase/` type, because this host is
/// not allowed to know what the content is — `demo/` holds the demonstration
/// and `corpus/showcase/` holds the scenes (epic #568). The name is carried only
/// so the log can say which scene is on screen.
pub struct SceneEntry {
    pub name: &'static str,
    pub build: SceneBuilder,
    pub pulse: ScenePulse,
    /// The name of the scalar signal this scene lets input drive. Opaque here,
    /// passed to `LiveScene::signal_named` and never read.
    pub signal: &'static str,
    /// What the variant key does in this scene, or `None` when the scene
    /// declares no variant set.
    pub action: Option<SceneAction>,
}

/// Opens a window and runs the showcase until it is closed.
///
/// **The length of `scenes` is the mode.** One scene runs exactly as it did
/// before this parameter existed and never advances. More than one, and this
/// moves to the next every [`PULSES_PER_SCENE`] pulses and cycles back to the
/// first, so the vocabulary checklist can be walked in a single run. Nothing
/// distinguishes the two paths except the count, so there is no flag that can
/// disagree with the list.
pub fn run(
    title: &'static str,
    scenes: Vec<SceneEntry>,
    painter: Choice,
) -> Result<(), DesktopError> {
    assert!(!scenes.is_empty(), "the showcase needs at least one scene");
    dashscene_desktop::run(Showcase {
        title,
        painter,
        scenes,
        current: 0,
        scene_since: Instant::now(),
        pulse_index: 0,
        rearm: None,
        timing: Timing::enabled(),
    })
}

/// The demonstration's state, and everything the loop asks it for.
struct Showcase {
    title: &'static str,
    /// Which painter is drawing. Chosen on the command line and swapped by
    /// [`SWAP_PAINTER`] — see [`crate::painter`] for why a run-time choice is a
    /// property of this demonstration and of nothing that ships.
    painter: Choice,
    /// Every scene to show, in the order they are shown. Never empty — [`run`]
    /// rejects an empty list, so `scenes[current]` is always a scene.
    scenes: Vec<SceneEntry>,
    /// Which of [`Showcase::scenes`] is on screen.
    current: usize,
    /// When [`Showcase::current`] came on screen, so its turn is measured
    /// against the wall clock rather than against a count of pulse events — see
    /// [`PULSES_PER_SCENE`] for why. Host-side only: nothing at or below
    /// `LiveScene` reads a clock, which is the invariant
    /// `demo/tests/clock_invariant.rs` asserts.
    scene_since: Instant,
    /// The scripted pulse currently in effect. The scene is a pure function of
    /// it, so a rebuild can restore the phase.
    pulse_index: u64,
    /// Rearms the pulse driver, which sends one pulse and then waits. Held so
    /// that the wake handler releases the next one only after this one has been
    /// applied, which is what stops pulses queueing (issue #629). `None` until
    /// the loop hands over its waker.
    rearm: Option<mpsc::Sender<()>>,
    /// The frame-cost instrument, when the environment asked for one. `None` on
    /// an ordinary run, which then pays nothing for it (story #586).
    timing: Option<Timing>,
}

impl App for Showcase {
    fn window(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title(self.title)
            .with_inner_size(WINDOW_SIZE)
    }

    /// Binds the currently chosen painter.
    ///
    /// The one place either presenter is constructed, so the swap key and the
    /// first frame cannot build them differently. Overriding this is the whole
    /// reason `dashscene-desktop` publishes the trait rather than hardcoding
    /// its own presenter.
    fn presenter(&mut self, window: Arc<Window>) -> Result<Box<dyn Present>, PresentError> {
        self.painter.presenter(window)
    }

    /// Builds the current scene, and restores the phase it was at.
    ///
    /// Re-applying the pulse is this method's job rather than the loop's: a
    /// rebuild produces a scene holding none of the writes made into the old
    /// one, and at the same phase, so a resize would otherwise snap every
    /// signal back to its initial value part-way through a transition.
    ///
    /// A rebuild rather than a re-solve, because a scene built in code derives
    /// every offset and size from the extent in Rust. It costs the scene's
    /// animation state: springs restart from their seeded values and run to the
    /// current phase again, which is visible if the window is dragged during a
    /// transition.
    fn build(&mut self, arena: &mut Arena, width: u32, height: u32) -> LiveScene {
        let entry = &self.scenes[self.current];
        let (build, pulse) = (entry.build, entry.pulse);
        let mut live = build(arena, width, height);
        pulse(&mut live, self.pulse_index);
        live
    }

    /// Spawns the placeholder signal producer: one wake every
    /// [`PULSE_INTERVAL`], from a thread that is not the loop's.
    ///
    /// It exists to exercise the wake path rather than to be a feature — the
    /// showcase scenes script their own signals. It reads a clock (it sleeps),
    /// which is allowed: it is host-side, above `LiveScene`.
    fn started(&mut self, waker: Waker) {
        self.rearm = Some(spawn_pulse_driver(waker));
    }

    /// Tells the running scene which painter is drawing it, so the badge names
    /// the painter on screen.
    ///
    /// Called after a rebuild and after a painter swap, which is exactly when
    /// the badge is stale: a fresh scene carries no write, and a swap changes
    /// what the write should say.
    ///
    /// A scene that declares no badge signal takes no write. That is what makes
    /// the `--dsb` run need no special case: a loaded document carries no such
    /// signal, and its solver holds no typesetter, so a label could not be
    /// staged there anyway.
    ///
    /// Writing the signal is also what makes a swap re-solve, but not because
    /// of the text it carries: a write that only changes text or opacity is
    /// paint-only and commits through the cached-rect replay, which stages no
    /// glyph runs at all. The badge binds the same signal to its pill's width
    /// as well (`corpus/showcase/src/badge.rs`'s "Why the pill's width is
    /// bound"), and a width write on a container with children cannot patch a
    /// single cached rect, so it forces the tick to solve. That is what commits
    /// through the scene's own solver and re-stages every glyph run, the
    /// incoming name's included — without a rebuild, which is what keeps a swap
    /// showing the difference between the two painters rather than between two
    /// runs.
    fn attached(&mut self, scene: Scene<'_>, _presenter: &str) {
        let value = self.painter.badge_value();
        if let Some(signal) = scene.live.signal_named(showcase::badge::BACKEND) {
            scene.live.set(signal, value);
        }
    }

    /// Applies one pulse to the running scene, and advances to the next scene
    /// when this one's turn is over.
    fn woken(&mut self, scene: Option<Scene<'_>>) -> Reaction {
        let reaction = self.pulse(scene);
        // Rearm only now. The driver is one-shot and has been waiting since it
        // sent this pulse, so releasing it here — after the pulse has been
        // applied, and on every path above — is what keeps at most one in
        // flight (issue #629). A closed channel means the driver thread has
        // ended, which happens only as the loop shuts down.
        if let Some(rearm) = self.rearm.as_ref() {
            let _ = rearm.send(());
        }
        reaction
    }

    fn event(&mut self, event: &WindowEvent, scene: Scene<'_>) -> Reaction {
        match event {
            // Story #573: the pointer drives the current scene's own named
            // signal.
            WindowEvent::CursorMoved { position, .. } => {
                let signal = self.scenes[self.current].signal;
                let changed =
                    crate::input::cursor_moved(scene.live, signal, position.x, scene.extent.0);
                if changed {
                    Reaction::Redraw
                } else {
                    Reaction::Ignored
                }
            }
            // Story #573: two keys drive that same signal to either end of its
            // range, and one runs the scene's own variant switch. `repeat` is
            // excluded so holding a key neither floods the signal nor spins the
            // variant.
            WindowEvent::KeyboardInput { event, .. } => {
                // A zero extent means there is no drawable, and that is the one
                // state in which a scene advance leaves `current` pointing at a
                // scene the arena was not built from — where the incoming
                // scene's action would run against the outgoing scene's arena
                // and `Txn::set_variant` would panic on a handle that arena does
                // not carry. A window with no drawable is minimised and receives
                // no key events, so this guards a state rather than a case that
                // fires.
                let drawable = scene.extent.0 > 0 && scene.extent.1 > 0;
                if !drawable || event.state != ElementState::Pressed || event.repeat {
                    return Reaction::Ignored;
                }
                let PhysicalKey::Code(code) = event.physical_key else {
                    return Reaction::Ignored;
                };
                // The painter swap is this demonstration's own, and is checked
                // before the scene's keys so that a scene can never take it
                // over.
                if code == SWAP_PAINTER {
                    self.painter = self.painter.other();
                    return Reaction::Rebind;
                }
                let entry = &self.scenes[self.current];
                let (signal, action) = (entry.signal, entry.action);
                if crate::input::key(code, signal, action, scene.live, scene.arena) {
                    Reaction::Redraw
                } else {
                    Reaction::Ignored
                }
            }
            _ => Reaction::Ignored,
        }
    }

    fn measured(&mut self, tick: Duration, present: Option<Duration>, presenter: &str) {
        let (Some(timing), Some(present)) = (self.timing.as_mut(), present) else {
            return;
        };
        let scene = self.scenes[self.current].name;
        timing.push(scene, presenter, tick, present);
        timing.report(scene, presenter);
    }

    fn note(&mut self, message: &str) {
        eprintln!("demo: {message}");
    }
}

impl Showcase {
    /// One pulse: script the current scene, or end its turn.
    ///
    /// Split out of [`App::woken`] so that the rearm cannot be missed: this
    /// body has several early returns, and the caller rearms the driver after it
    /// returns by any of them (issue #629). Rearming on only some of them would
    /// stall the driver permanently rather than merely drop a pulse.
    fn pulse(&mut self, scene: Option<Scene<'_>>) -> Reaction {
        let Some(scene) = scene else {
            // The window does not exist yet. Dropping this pulse costs one
            // interval; the next one still comes, because the caller rearms the
            // driver on this path too.
            return Reaction::Ignored;
        };
        let pulse = self.scenes[self.current].pulse;
        self.pulse_index += 1;
        pulse(scene.live, self.pulse_index);

        // With more than one scene to show, this pulse may be the one that ends
        // the current scene's turn. Decided on elapsed time rather than on
        // `pulse_index`, for the reason [`SCENE_DWELL`] records, but acted on
        // here so the change lands on a pulse boundary rather than part-way
        // through a transition. The check runs after the pulse has been applied,
        // so each scene is seen through `PULSES_PER_SCENE` complete cycles
        // rather than that many minus one.
        if self.scenes.len() > 1 && self.scene_since.elapsed() >= SCENE_DWELL {
            return self.advance_scene(scene.extent);
        }

        // Not `Redraw`: the pulse wrote signals, so the generation will report
        // the change by itself and a frame that paints because it advanced is
        // the contract. Forcing here would paint a pulse that changed nothing.
        Reaction::Frame
    }

    /// Moves to the next scene in the list, cycling back to the first.
    ///
    /// The phase resets to zero because the incoming scene has its own script
    /// and the outgoing scene's pulse count means nothing to it. That is the one
    /// way this differs from the resize path, which keeps the phase precisely
    /// because it is the *same* scene at a new extent.
    ///
    /// Rebuilding drops the outgoing scene's arena, so nothing of it survives
    /// into the next — which is also what makes this a fair way to watch each
    /// scene, since every one starts from its own seeded values.
    /// `extent` is the drawable, and it decides whether the change is announced.
    /// A zero dimension is a minimised window: the loop drops the rebuild, so
    /// nothing comes on screen and saying that it did would be a line the
    /// picture does not match. The scene index still moves, and the first frame
    /// after the window is restored builds it — which is the behaviour this had
    /// before the loop was extracted, where the whole method returned early on a
    /// zero extent and printed nothing.
    fn advance_scene(&mut self, extent: (u32, u32)) -> Reaction {
        self.current = (self.current + 1) % self.scenes.len();
        self.pulse_index = 0;
        self.scene_since = Instant::now();
        if extent.0 > 0 && extent.1 > 0 {
            eprintln!(
                "demo: scene {} — {}",
                self.current + 1,
                self.scenes[self.current].name
            );
        }
        // A new arena means a generation the window has never shown, so the
        // generation alone cannot report that the next frame must paint — which
        // is why this is a rebuild and not a frame.
        Reaction::Rebuild
    }
}

/// The placeholder signal producer: one wake every [`PULSE_INTERVAL`], from a
/// thread that is not the loop's.
///
/// Returns the rearm handle. The driver sends **one** wake and then waits to be
/// rearmed, so at most one is ever in flight; see [`spawn_pulses`].
fn spawn_pulse_driver(waker: Waker) -> mpsc::Sender<()> {
    spawn_pulses(PULSE_INTERVAL, move || waker.wake())
}

/// The driver's timing and handshake, with the transport left to the caller so
/// a test can supply one (issue #629).
///
/// One-shot and rearmed, rather than free-running. The loop sleeps, sends, and
/// then **blocks until the host rearms it**, so a second pulse cannot be queued
/// while the first is unhandled.
///
/// It used to sleep and send unconditionally. Whenever a frame ran longer than
/// the interval — routine in a debug build, where the `surfaces` scene costs
/// about 28 ms per frame at 1920x1200 — the driver ran ahead of the loop and
/// the queued pulses then arrived together. The scene jumped through its script
/// instead of stepping through it, and only the first of each burst reported the
/// park, because the park report is taken once. Issue #628's scene advance is
/// keyed on elapsed time rather than on a count of pulses precisely because
/// counting them was unreliable while this stood.
///
/// The handshake also ties the driver to the loop's state. A free-running thread
/// kept sending while the loop was deliberately parked; this one cannot get
/// ahead of what has been applied.
///
/// `send` reports whether the message reached the loop. Both it returning
/// `false` and the rearm channel closing mean the loop is gone, and the thread
/// ends.
fn spawn_pulses(
    interval: Duration,
    mut send: impl FnMut() -> bool + Send + 'static,
) -> mpsc::Sender<()> {
    let (rearm, rearmed) = mpsc::channel::<()>();
    thread::spawn(move || {
        loop {
            thread::sleep(interval);
            if !send() {
                // The event loop has exited and will not run another frame.
                return;
            }
            if rearmed.recv().is_err() {
                // The host dropped its end: the loop is shutting down.
                return;
            }
        }
    });
    rearm
}

/// The environment variable that turns the frame-cost instrument on.
///
/// Off by default, so an ordinary run is unmeasured and pays nothing.
const TIMING_VAR: &str = "DASHSCENE_FRAME_TIMING";

/// How many presents one report covers — the sample size
/// `docs/technotes/frame-budget.md` states for its own blit
/// measurement, so the two are read in the same units.
const TIMING_SAMPLE: usize = 240;

/// Per-frame costs, collected while the showcase runs (story #586).
///
/// **This exists because the measurement it serves was ad-hoc.** The v0.14
/// frame budget's most consequential finding — that the `softbuffer` blit was a
/// larger term than the painter for two of the three scenes — came from
/// instrumenting the host by hand and was not reproducible afterwards. Story
/// #585 then replaced that blit with a surface present, and the question of what
/// happened to the term is exactly the kind that a one-off instrument cannot
/// answer twice.
///
/// It times `tick` and `present` separately, because they are the two halves the
/// v0.14 table separates. `present` is the whole of the drawing: it is `paint`
/// plus whatever putting the frame on the window costs, which is the blit for
/// the reference painter and a swapchain present for the lean one.
struct Timing {
    tick: Vec<f64>,
    present: Vec<f64>,
    /// What the sample in hand is a sample *of* — the scene and the presenter.
    ///
    /// A sample is discarded when either changes part-way through. Scenes
    /// advance on a dwell timer and painters swap on a key, so a mean taken
    /// across either boundary describes neither side of it — which is not
    /// hypothetical: the first sweep run with this instrument was taken while
    /// the painter was swapped mid-run, and it reported one painter's numbers
    /// under the other's name.
    of: Option<(String, String)>,
}

impl Timing {
    /// A collector, or `None` when the environment has not asked for one.
    fn enabled() -> Option<Self> {
        std::env::var_os(TIMING_VAR).map(|_| Timing {
            tick: Vec::with_capacity(TIMING_SAMPLE),
            present: Vec::with_capacity(TIMING_SAMPLE),
            of: None,
        })
    }

    fn push(&mut self, scene: &str, presenter: &str, tick: Duration, present: Duration) {
        let now = (scene.to_owned(), presenter.to_owned());
        if self.of.as_ref() != Some(&now) {
            if self.of.is_some() && !self.present.is_empty() {
                eprintln!(
                    "demo: frame timing — {} sample(s) discarded, the scene or painter changed \
                     part-way through",
                    self.present.len(),
                );
            }
            self.tick.clear();
            self.present.clear();
            self.of = Some(now);
        }
        self.tick.push(tick.as_secs_f64() * 1000.0);
        self.present.push(present.as_secs_f64() * 1000.0);
    }

    /// Reports and clears once a full sample is in hand.
    ///
    /// Reported per sample rather than at exit, because the showcase advances
    /// through scenes and a mean over all of them would describe none of them.
    /// `presenter` is `Present::name` rather than the painter enum, because it
    /// names the whole path being timed — the reference painter's includes the
    /// blit, which is the term this instrument exists to watch.
    fn report(&mut self, scene: &str, presenter: &str) {
        if self.present.len() < TIMING_SAMPLE {
            return;
        }
        let stat = |values: &mut Vec<f64>| {
            values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
            (mean, at(0.5), at(0.95), at(1.0))
        };
        let (tick_mean, ..) = stat(&mut self.tick);
        let (mean, p50, p95, max) = stat(&mut self.present);
        eprintln!(
            "demo: {scene} through {presenter} over {TIMING_SAMPLE} presents — tick \
             {tick_mean:.2} ms, present mean {mean:.2} p50 {p50:.2} p95 {p95:.2} max {max:.2} ms \
             ({:.1} fps if unpaced)",
            1000.0 / (mean + tick_mean),
        );
        self.tick.clear();
        self.present.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A short interval, so a test that waits several of them still finishes in
    /// well under a second. The property under test is the handshake, not the
    /// duration, and `spawn_pulses` takes the interval precisely so a test does
    /// not have to wait `PULSE_INTERVAL`.
    const TICK: Duration = Duration::from_millis(5);

    /// Waits for `count` to reach `want`, or gives up. Polling rather than
    /// sleeping a fixed span: the assertion is about what the driver did, and a
    /// slow machine should make this test slower, never wrong.
    fn wait_for(count: &AtomicUsize, want: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if count.load(Ordering::SeqCst) >= want {
                return true;
            }
            thread::sleep(Duration::from_millis(1));
        }
        false
    }

    /// The defect issue #629 reported: the driver ran ahead of the loop.
    ///
    /// Twenty intervals elapse and the driver is never rearmed. It must have
    /// sent exactly one pulse. Before the handshake it sent one per interval,
    /// and the queued messages then arrived together — which is what made a
    /// scene jump through its script instead of stepping through it.
    #[test]
    fn an_unrearmed_driver_sends_one_pulse_and_then_waits() {
        let count = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        // Held, not dropped: dropping the sender closes the channel and ends
        // the thread, which would pass this test for the wrong reason.
        let _rearm = spawn_pulses(TICK, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            true
        });

        assert!(wait_for(&count, 1), "the driver never sent its first pulse");
        thread::sleep(TICK * 20);

        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "an unrearmed driver sent more than one pulse, so it can still run \
             ahead of the loop"
        );
    }

    /// The other half: rearming releases exactly one more pulse, so the loop
    /// gets a pulse per rearm rather than a burst.
    #[test]
    fn each_rearm_releases_exactly_one_more_pulse() {
        let count = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        let rearm = spawn_pulses(TICK, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            true
        });

        for want in 1..=3 {
            assert!(wait_for(&count, want), "pulse {want} never arrived");
            // Settle past the next interval to show the driver is waiting on
            // the rearm rather than on the clock.
            thread::sleep(TICK * 3);
            assert_eq!(
                count.load(Ordering::SeqCst),
                want,
                "the driver ran ahead instead of waiting to be rearmed"
            );
            rearm.send(()).expect("the driver thread is still waiting");
        }

        assert!(
            wait_for(&count, 4),
            "the driver stopped after its rearms rather than sending again"
        );
    }

    /// A transport that reports the loop is gone ends the thread rather than
    /// spinning. Observable as the count never passing one, with no rearm ever
    /// sent — the thread returns before it waits.
    #[test]
    fn a_closed_transport_ends_the_driver() {
        let count = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        let _rearm = spawn_pulses(TICK, move || {
            counted.fetch_add(1, Ordering::SeqCst);
            false
        });

        assert!(wait_for(&count, 1), "the driver never attempted a send");
        thread::sleep(TICK * 20);

        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "the driver kept sending after the loop reported it had exited"
        );
    }
}
