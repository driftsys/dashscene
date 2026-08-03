//! The host: a window, a clock, and the frame loop that drives them (story
//! #572).
//!
//! One frame is four steps, and nothing else:
//!
//! ```text
//! dt = min(elapsed, 100 ms)
//! LiveScene::tick(dt, &mut arena)
//! arena.committed()
//! present
//! ```
//!
//! P3 holds by construction. The host owns time and nothing producer-side runs
//! inside the loop: `tick` takes `dt` as a parameter, every signal change is
//! applied on this thread before `tick` reads it, and a producer that lives
//! outside the loop reaches it only by asking the loop to run a frame
//! ([`Wake`]).
//!
//! # The clock is read here and nowhere below
//!
//! The frame clock is read in [`Host::frame`], and the clock is read twice
//! more — in [`Host::park`] and on waking — only to time the loop's own wait
//! for a log line. No crate at or below `LiveScene` may read a clock at all —
//! that is what makes an animation test reproducible, and
//! `demo/tests/clock_invariant.rs` asserts it rather than leaving it to
//! review. The clamp, the absence of an accumulator, and the invariant are
//! argued in
//! `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
//!
//! # There is no accumulator
//!
//! `dashcue` already splits an `advance(dt)` into equal substeps below its
//! stability bound (`docs/decisions/dashcue-spring-uses-semi-implicit-euler.md`),
//! so a fixed-step accumulator here would reimplement that substepping one
//! layer up. The clamp is the whole of the guard, and it guards frame *cost*:
//! substep count scales with `dt`, so an unbounded `dt` is an unbounded substep
//! burst.
//!
//! # An idle frame neither paints nor presents
//!
//! `LiveScene::tick` returns the commit generation and holds it steady on an
//! idle frame. The host records the generation the window currently shows and
//! skips both `paint` and `present` when `tick` returns that same value. This
//! needs no new API: no flag, no extra return value, and no change to
//! `dashpaint` or boundary B.
//!
//! It is a requirement rather than an optimisation because no painter has a
//! partial-redraw path — `dashscene-skia`'s retained mode patches its instance
//! buffer and still redraws every quad — so a static screen costs a full frame
//! of fill every frame, and not running the frame is the only thing that
//! removes that cost.
//!
//! The generation reports document and animation change only, so five cases
//! force a redraw independently of it. Four are handled here — the first
//! frame, a resize or surface reconfigure, a scale-factor change, and
//! re-exposure after occlusion. The fifth, a lost surface or recreated
//! swapchain, cannot arise behind `SkiaPresenter`: `softbuffer` hands back a
//! fresh buffer on every `buffer_mut()` and has no lost-surface condition, and
//! story #571 deliberately gave [`PresentError`](crate::present::PresentError)
//! no `Lost` variant because a wgpu presenter recovers its own surface inside
//! `present`. When `dashscene-wgpu` lands, a presenter that recreated its
//! swapchain reports it and the host forces a redraw through the same
//! [`Host::force`] entry point. It is named here so the case is findable, not
//! implemented speculatively for a presenter that does not exist.
//!
//! # The wait mode follows from that, and so does the wake mechanism
//!
//! The loop paces itself at [`FRAME_INTERVAL`] while the generation advances
//! and waits for an event while it is steady, rather than waking sixty times a
//! second to redraw an unchanged screen.
//!
//! A producer outside the loop therefore needs a way to wake it. That
//! mechanism is a `winit` [`EventLoopProxy`] carrying a [`Wake`] message, and
//! it is the mechanism stories #573 and #574 use: input arrives as ordinary
//! window events and needs no proxy, while a scripted or externally fed signal
//! producer sends a [`Wake`] from its own thread and the host applies the
//! change on the loop's thread before the next `tick`.

use std::error::Error;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use dashlang::LiveScene;
use dashscene_core::Arena;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::painter::Choice;
use crate::present::{Present, PresentError};

/// Builds a scene into `arena` for a drawable of `width` x `height` physical
/// pixels, and returns the live scene the loop ticks.
///
/// The extent is passed in rather than fixed because the window's physical
/// size is only known once the window exists, and on a high-density display it
/// is not the logical size that was asked for. The builder owns its solver:
/// `LiveScene` retains one and reuses it for every reflow.
pub type SceneBuilder = fn(&mut Arena, u32, u32) -> LiveScene;

/// Applies the scene's scripted signal change for pulse number `index`.
///
/// A plain `fn` rather than a closure or a trait object, and a `u64` rather
/// than captured state, so the phase lives in the host and the scene stays a
/// pure function of it — which is what lets [`Host::rebuild`] re-apply the
/// current phase to a scene it just rebuilt for a new extent.
///
/// Story #574 replaces this with the showcase scenes' own scripting. Story
/// #573's input mapping does not go through here: input is a window event, and
/// the host applies it directly.
pub type ScenePulse = fn(&mut LiveScene, u64);

/// Runs the scene's own variant switch (story #573). The one thing input does
/// that a signal write cannot express, because `Txn::set_variant` needs the
/// arena.
///
/// A scene owns the switch because a scene owns its content: it declares the
/// variant set inside its own builder, where it holds the arena, and hands the
/// host a function that switches it. The host holds no `VariantSetId`, no
/// member list and no node name — see [`crate::input`], and issue #625 for the
/// seam this closes.
pub type SceneAction = fn(&mut LiveScene, &mut Arena);

/// The window's requested size, in logical pixels.
const WINDOW_SIZE: LogicalSize<u32> = LogicalSize::new(960, 600);

/// The largest `dt` handed to `LiveScene::tick`, whatever the wall clock says.
///
/// **A convention, not a derived bound.** The lower bound is real — it has to
/// sit above ordinary hitches or it fires in normal operation — but nothing
/// distinguishes 100 ms from Unity's 333 ms, and deriving it needs a frame
/// budget this project does not have. The binding rule is that every product
/// painter's host clamps at the *same* value, and that it is configured rather
/// than inherited from an engine default. Argued in
/// `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
const MAX_FRAME_DELTA: Duration = Duration::from_millis(100);

/// The pace the loop runs at while the generation advances: 60 Hz.
///
/// `ControlFlow::WaitUntil` rather than `ControlFlow::Poll`, because polling
/// spins as fast as the machine allows and this is meant to be a frame rate. A
/// frame that overruns the interval leaves the deadline already past, so the
/// loop wakes immediately and the pacing degrades to running flat out rather
/// than to dropping frames.
const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

/// The key that swaps the painter on the running window (story #585).
///
/// Handled here rather than in [`crate::input`], because it is not a scene's
/// input: a scene declares the signal the pointer drives and the function a key
/// runs, and which painter draws it is neither. `P` is unmapped by every scene,
/// so nothing it did before is lost.
const SWAP_PAINTER: KeyCode = KeyCode::KeyP;

/// How often the placeholder driver pulses the scene's signals.
///
/// Long enough that the loop visibly settles and parks between pulses, which
/// is what makes the idle skip observable in the log rather than asserted.
const PULSE_INTERVAL: Duration = Duration::from_millis(2500);

/// A message from outside the event loop asking the host to run a frame.
///
/// This is the wake mechanism the loop's wait mode requires. While the
/// generation is steady the loop is parked in `ControlFlow::Wait`, so a
/// producer that is not driven by a window event — a scripted sequence, a
/// timer, a data feed — cannot reach the scene without one.
///
/// The message carries the *intent* and not the signal write. `LiveScene` is
/// owned by the host and lives on the loop's thread, so the sender asks and
/// the host applies, which is what keeps P3 true across the thread boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// Advance the scene's scripted pulse by one, then run a frame.
    Pulse,
}

/// One entry in the host's scene list: what to build, how to script it, and
/// the name to report when it comes up.
///
/// The host holds these rather than the `corpus/showcase/` type, because the
/// host is not allowed to know what the content is — `demo/` holds the loop and
/// `corpus/showcase/` holds the scenes (epic #568). The name is carried only so
/// the log can say which scene is on screen.
pub struct SceneEntry {
    pub name: &'static str,
    pub build: SceneBuilder,
    pub pulse: ScenePulse,
    /// The name of the scalar signal this scene lets input drive. Opaque to
    /// the host, which passes it to `LiveScene::signal_named` and never reads
    /// it.
    pub signal: &'static str,
    /// What the variant key does in this scene, or `None` when the scene
    /// declares no variant set.
    pub action: Option<SceneAction>,
}

/// How long a scene is shown for before the host moves to the next one, when
/// it has more than one to show (issue #628).
///
/// Expressed as a number of pulse intervals because the scene changes *on* a
/// pulse: the host waits for this long to elapse and then advances at the next
/// pulse, so a scene is never cut mid-transition. Four rather than one because
/// each scene's script alternates direction, so an even count leaves the scene
/// at the phase it started from.
///
/// **The deadline is elapsed time, not a count of pulse events.** It was
/// written that way because [`spawn_pulse_driver`] used to free-run: it slept
/// and sent whether or not the host had consumed the last message, so on a slow
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

/// Opens a window, binds `painter`'s presenter to it, and runs the frame loop
/// until the window is closed.
///
/// **The length of `scenes` is the mode.** One scene runs exactly as it did
/// before this parameter existed and never advances. More than one, and the
/// host moves to the next every [`PULSES_PER_SCENE`] pulses and cycles back to
/// the first, so the vocabulary checklist can be walked in a single run.
/// Nothing distinguishes the two paths except the count, so there is no flag
/// that can disagree with the list.
pub fn run(
    title: &'static str,
    scenes: Vec<SceneEntry>,
    painter: Choice,
) -> Result<(), Box<dyn Error>> {
    assert!(!scenes.is_empty(), "the host needs at least one scene");
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    // The starting mode. The first frame replaces it: `WaitUntil` while the
    // generation advances, `Wait` once it is steady.
    event_loop.set_control_flow(ControlFlow::Wait);
    let rearm = spawn_pulse_driver(event_loop.create_proxy());

    let mut host = Host {
        rearm,
        title,
        painter,
        scenes,
        current: 0,
        scene_since: Instant::now(),
        window: None,
        presenter: None,
        arena: Arena::new(),
        live: None,
        extent: (0, 0),
        previous_frame: None,
        shown: None,
        forced: None,
        pulse_index: 0,
        ticks: 0,
        presents: 0,
        parked: None,
        failure: None,
    };
    event_loop.run_app(&mut host)?;
    match host.failure {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// The placeholder signal producer: one [`Wake::Pulse`] every
/// [`PULSE_INTERVAL`], from a thread that is not the loop's.
///
/// It exists to exercise the wake path rather than to be a feature — story
/// #574's scenes script their own signals and send their own wakes. It reads a
/// clock (it sleeps), which is allowed: it is host-side, above `LiveScene`.
///
/// Returns the rearm handle. The driver sends **one** pulse and then waits to
/// be rearmed, so at most one is ever in flight; see [`spawn_pulses`].
fn spawn_pulse_driver(proxy: EventLoopProxy<Wake>) -> mpsc::Sender<()> {
    spawn_pulses(PULSE_INTERVAL, move || {
        proxy.send_event(Wake::Pulse).is_ok()
    })
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
/// instead of stepping through it, and only the first of each burst reported
/// the park, because `Host::parked` is taken once. Issue #628's scene advance
/// is keyed on elapsed time rather than on a count of pulses precisely because
/// counting them was unreliable while this stood.
///
/// The handshake also ties the driver to the loop's state. A free-running
/// thread kept sending while the loop was deliberately parked; this one cannot
/// get ahead of what the host has applied.
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

/// What the loop was doing when it last parked, so waking can report what did
/// **not** happen while it was parked.
struct Parked {
    since: Instant,
    ticks: u64,
    presents: u64,
}

struct Host {
    /// Rearms the pulse driver, which sends one pulse and then waits. Held so
    /// that [`Host::user_event`] can release the next one only after this one
    /// has been applied, which is what stops pulses queueing (issue #629).
    rearm: mpsc::Sender<()>,
    title: &'static str,
    /// Which painter is drawing. Chosen on the command line and swapped by
    /// [`SWAP_PAINTER`] — see [`crate::painter`] for why a run-time choice is a
    /// property of this demonstration and of nothing that ships.
    painter: Choice,
    /// Every scene the host was asked to show, in the order it shows them.
    /// Never empty — [`run`] rejects an empty list, so `scenes[current]` is
    /// always a scene.
    scenes: Vec<SceneEntry>,
    /// Which of [`Host::scenes`] is on screen.
    current: usize,
    /// When [`Host::current`] came on screen, so its turn is measured against
    /// the wall clock rather than against a count of pulse events — see
    /// [`PULSES_PER_SCENE`] for why, and for why elapsed time stays now that
    /// pulses no longer arrive in bursts. Host-side only: nothing at or below
    /// `LiveScene` reads a clock, which is the invariant
    /// `demo/tests/clock_invariant.rs` asserts.
    scene_since: Instant,
    window: Option<Arc<Window>>,
    presenter: Option<Box<dyn Present>>,
    arena: Arena,
    live: Option<LiveScene>,
    /// The drawable extent the host last acted on, in physical pixels. A
    /// `Resized` that repeats it changes nothing and is dropped here, before
    /// it costs a scene rebuild.
    extent: (u32, u32),
    /// When the previous frame ran. `None` means the frame clock is stopped:
    /// on the first frame, and across the loop's own wait.
    previous_frame: Option<Instant>,
    /// The generation the window currently shows. `None` before the first
    /// present, and after a rebuild, whose generations count from a new arena.
    shown: Option<u64>,
    /// Why the next frame must paint whatever the generation says. Consumed by
    /// the frame that acts on it.
    forced: Option<&'static str>,
    /// The scripted pulse currently in effect. The scene is a pure function of
    /// it, so a rebuild can restore the phase.
    pulse_index: u64,
    /// How many frames have called `tick`, and how many of those presented.
    /// The gap between them is the idle skip, and it is reported rather than
    /// claimed.
    ticks: u64,
    presents: u64,
    parked: Option<Parked>,
    /// The first error that stopped the loop. `ApplicationHandler`'s methods
    /// return nothing, so a failure is parked here and reported by [`run`]
    /// rather than printed and forgotten.
    failure: Option<Box<dyn Error>>,
}

impl Host {
    /// Records `failure` and asks the event loop to stop. The first failure
    /// wins: a later one is a consequence of the state the first left behind.
    fn fail(&mut self, event_loop: &ActiveEventLoop, failure: impl Error + 'static) {
        if self.failure.is_none() {
            self.failure = Some(Box::new(failure));
        }
        event_loop.exit();
    }

    /// One frame.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        // The frame clock, read once, here. `saturating_duration_since` rather
        // than subtraction because a monotonic clock is only guaranteed
        // non-decreasing, and a zero-length frame is a correct answer where a
        // panic is not.
        let now = Instant::now();
        let dt = match self.previous_frame {
            Some(previous) => now.saturating_duration_since(previous).min(MAX_FRAME_DELTA),
            // The clock was stopped: this is the first frame, or the first one
            // after the loop parked. No animation time passed during the
            // loop's own wait, because nothing was animating through it, so
            // the frame that ends the wait starts from zero rather than from
            // however long the window sat untouched. This is what makes the
            // clamp a guard against *external* stalls only.
            None => Duration::ZERO,
        };
        self.previous_frame = Some(now);

        let Some(live) = self.live.as_mut() else {
            return;
        };
        let generation = live.tick(dt.as_secs_f32(), &mut self.arena);
        self.ticks += 1;

        let advanced = self.shown != Some(generation);
        let forced = self.forced.take();
        if !advanced && let Some(reason) = forced {
            // The generation could not report this one, which is the whole
            // reason the forced list exists. Reported so the path is visible
            // when it fires rather than only when it fails to.
            eprintln!("demo: forced redraw — {reason}");
        }
        if (advanced || forced.is_some())
            && let Err(error) = self.paint(generation)
        {
            return self.fail(event_loop, error);
        }

        if advanced {
            self.parked = None;
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + FRAME_INTERVAL));
        } else {
            // Steady generation: the scene has settled. A frame that painted
            // only because something forced it still lands here, because a
            // forced repaint is not motion.
            self.park(event_loop, generation);
        }
    }

    /// Draws the committed scene and posts it.
    fn paint(&mut self, generation: u64) -> Result<(), PresentError> {
        let (Some(window), Some(presenter)) = (self.window.as_ref(), self.presenter.as_mut())
        else {
            return Ok(());
        };
        // Tells the compositor a frame is about to be posted, so it can
        // schedule the next one; winit asks for it immediately before
        // presenting.
        window.pre_present_notify();
        presenter.present(self.arena.committed())?;
        self.shown = Some(generation);
        self.presents += 1;
        Ok(())
    }

    /// Stops the loop until an event arrives.
    fn park(&mut self, event_loop: &ActiveEventLoop, generation: u64) {
        event_loop.set_control_flow(ControlFlow::Wait);
        // The frame clock stops with the loop; see the `None` arm in `frame`.
        self.previous_frame = None;
        if self.parked.is_none() {
            eprintln!(
                "demo: settled at generation {generation} after {} ticks and {} presents — \
                 waiting for an event",
                self.ticks, self.presents
            );
            self.parked = Some(Parked {
                since: Instant::now(),
                ticks: self.ticks,
                presents: self.presents,
            });
        }
    }

    /// Swaps the painter on the running window, keeping everything else.
    ///
    /// The arena, the live scene, the frame clock and the pulse phase are all
    /// untouched, so the next frame is the same frame drawn by the other
    /// painter. That is the whole instrument story #585 was for: the difference
    /// on screen is the difference between the painters, and not between two
    /// runs.
    ///
    /// The outgoing presenter is dropped **before** the incoming one is built.
    /// Both own a surface on this one window — a CPU framebuffer on one side, a
    /// swapchain on the other — and holding two at once is the state neither
    /// windowing backend is asked to support.
    fn swap_painter(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let next = self.painter.other();
        self.presenter = None;
        match next.presenter(window) {
            Ok(presenter) => {
                eprintln!("demo: painter is now {}", presenter.name());
                self.presenter = Some(presenter);
                self.painter = next;
                // The incoming presenter adopted the window's extent at
                // construction, so nothing is re-solved and the scene keeps its
                // state. It has drawn nothing, though, and the generation is
                // unchanged — which is the fifth case the forced list exists
                // for.
                self.force("a painter swap");
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    /// Makes the next frame paint and present whatever the generation says.
    fn force(&mut self, reason: &'static str) {
        self.forced = Some(reason);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// The resize path: reconfigure the presenter, re-solve the document for
    /// the new extent, and repaint.
    fn resized(&mut self, event_loop: &ActiveEventLoop, width: u32, height: u32) {
        if (width, height) == self.extent {
            return;
        }
        self.extent = (width, height);

        let resized = match self.presenter.as_mut() {
            Some(presenter) => presenter.resize(width, height),
            None => Ok(()),
        };
        if let Err(error) = resized {
            return self.fail(event_loop, error);
        }

        // A zero dimension is a minimised window. There is nothing to solve
        // for and nothing to draw into; the next non-zero `Resized` rebuilds.
        if width > 0 && height > 0 {
            self.rebuild(width, height);
            eprintln!("demo: resized to {width}x{height} physical pixels — scene rebuilt");
        }
        self.force("a resize");
    }

    /// Builds the scene for `width` x `height` into a fresh arena.
    ///
    /// A rebuild rather than a re-solve, because this host's placeholder scene
    /// derives every offset and size from the extent in Rust. It costs the
    /// scene's animation state: springs restart from their seeded values and
    /// run to the current phase again, which is visible if the window is
    /// dragged during a transition. Story #574's scenes remove the cost rather
    /// than the host doing so — a scene whose root fills the drawable takes a
    /// new extent through a signal, and then a resize is an ordinary frame.
    fn rebuild(&mut self, width: u32, height: u32) {
        let pulse = self.scenes[self.current].pulse;
        // The presenter is about to be handed frames from an arena that has
        // never existed before. Anything it holds per document — the lean
        // painter keeps a copy of what its instance buffer contains — describes
        // the outgoing one, and the incoming arena's generations start again,
        // so nothing in the frames themselves says so
        // (`Present::document_replaced`).
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.document_replaced();
        }
        self.arena = Arena::new();
        let mut live = (self.scenes[self.current].build)(&mut self.arena, width, height);
        // Restore the phase, so a resize resumes the scene where it was rather
        // than snapping every signal back to its initial value.
        pulse(&mut live, self.pulse_index);
        self.live = Some(live);
        // Generations count from a new arena, so the number the window shows
        // no longer names anything in this scene.
        self.shown = None;
    }

    /// Applies one pulse to the running scene, and advances to the next scene
    /// when this one's turn is over.
    ///
    /// Split out of [`Host::user_event`] so that the rearm cannot be missed:
    /// this body has two early returns, and the caller rearms the driver after
    /// it returns by any of its paths (issue #629). Rearming on only some of
    /// them would stall the driver permanently rather than merely drop a pulse.
    fn apply_pulse(&mut self) {
        let pulse = self.scenes[self.current].pulse;
        let Some(live) = self.live.as_mut() else {
            // The window does not exist yet. Dropping this pulse costs one
            // interval; the next one still comes, because the caller
            // rearms the driver on this path too.
            return;
        };
        self.pulse_index += 1;
        pulse(live, self.pulse_index);

        // With more than one scene to show, this pulse may be the one that ends
        // the current scene's turn. Decided on elapsed time rather than on
        // `pulse_index`, for the reason [`SCENE_DWELL`] records, but acted on
        // here so the change lands on a pulse boundary rather than part-way
        // through a transition. The check runs after the pulse has been
        // applied, so each scene is seen through `PULSES_PER_SCENE` complete
        // cycles rather than that many minus one.
        if self.scenes.len() > 1 && self.scene_since.elapsed() >= SCENE_DWELL {
            self.report_park();
            self.advance_scene();
            return;
        }

        self.report_park();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Reports what did **not** happen while the loop was parked, if it was.
    ///
    /// Shared by the two things a pulse can do — script the current scene, or
    /// end its turn — so that a scene change reports the idle interval it woke
    /// from rather than swallowing it. That report is the only evidence the
    /// idle-frame skip produces at run time, so it must not be lost on the one
    /// pulse in every [`PULSES_PER_SCENE`] that changes the scene.
    fn report_park(&mut self) {
        let Some(parked) = self.parked.take() else {
            return;
        };
        eprintln!(
            "demo: woken by pulse {} after {:.2} s parked — {} ticks and {} presents ran while \
             parked",
            self.pulse_index,
            parked.since.elapsed().as_secs_f32(),
            self.ticks - parked.ticks,
            self.presents - parked.presents,
        );
    }

    /// Moves to the next scene in the list, cycling back to the first.
    ///
    /// The phase resets to zero because the incoming scene has its own script
    /// and the outgoing scene's pulse count means nothing to it. That is the
    /// one way this differs from [`Host::rebuild`]'s resize path, which keeps
    /// the phase precisely because it is the *same* scene at a new extent.
    ///
    /// Rebuilding drops the outgoing scene's arena, so nothing of it survives
    /// into the next — which is also what makes this a fair way to watch each
    /// scene, since every one starts from its own seeded values.
    fn advance_scene(&mut self) {
        self.current = (self.current + 1) % self.scenes.len();
        self.pulse_index = 0;
        self.scene_since = Instant::now();

        let (width, height) = self.extent;
        if width == 0 || height == 0 {
            // No drawable yet, so there is nothing to rebuild against. The
            // scene index has moved; the first frame builds it.
            return;
        }
        self.rebuild(width, height);

        let scene = &self.scenes[self.current];
        eprintln!("demo: scene {} — {}", self.current + 1, scene.name);
        // A new arena means a generation the host has never shown, so the
        // generation alone cannot report that this frame must paint.
        self.force("a scene change");
    }
}

impl ApplicationHandler<Wake> for Host {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        // The `WaitUntil` deadline the previous frame set has arrived, so ask
        // for the next one. This is the whole of the loop's pacing.
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` fires again after a suspend on the platforms that suspend.
        // The window and its surface survive that here, so rebuilding them
        // would drop a live surface for no reason.
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(self.title)
            .with_inner_size(WINDOW_SIZE);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };
        let presenter = match self.painter.presenter(Arc::clone(&window)) {
            Ok(presenter) => presenter,
            Err(error) => return self.fail(event_loop, error),
        };

        // Physical pixels, so the scene fills the drawable on a high-density
        // display instead of occupying a corner of it.
        let size = window.inner_size();
        // Copied out: the name borrows the presenter, and the presenter is
        // about to move into the host.
        let painter = presenter.name().to_owned();
        self.extent = (size.width, size.height);
        self.window = Some(window);
        self.presenter = Some(presenter);
        self.rebuild(size.width, size.height);

        eprintln!(
            "demo: {painter} — {}x{} physical pixels, {} rects",
            size.width,
            size.height,
            self.arena.committed().rects().len()
        );
        eprintln!(
            "demo: frame loop — dt clamped to {} ms, {} Hz while the generation advances, \
             waiting for an event while it is steady",
            MAX_FRAME_DELTA.as_millis(),
            (1.0 / FRAME_INTERVAL.as_secs_f32()).round(),
        );
        eprintln!(
            "demo: press {SWAP_PAINTER:?} to swap the painter on this window, same scene and \
             same clock"
        );

        // The first frame is one of the five the generation cannot report.
        self.force("the first frame");
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Pulse => {
                self.apply_pulse();
                // Rearm only now. The driver is one-shot and has been waiting
                // since it sent this pulse, so releasing it here — after the
                // pulse has been applied, and after every early return above —
                // is what keeps at most one in flight (issue #629). A closed
                // channel means the driver thread has ended, which happens only
                // as the loop shuts down.
                let _ = self.rearm.send(());
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resized(event_loop, size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The new physical extent, if there is one, arrives as its own
                // `Resized`. This case is in the forced list precisely because
                // there may not be one: a scale-factor change can report an
                // unchanged physical size, and then the resize path correctly
                // does nothing while the picture still has to be redrawn.
                eprintln!("demo: scale factor is now {scale_factor}");
                self.force("a scale-factor change");
            }
            // Not every platform preserves the contents of an occluded
            // surface, and the generation cannot report that they were lost.
            WindowEvent::Occluded(false) => self.force("re-exposure after occlusion"),
            // Story #573: the pointer drives the current scene's own named
            // signal. No wake proxy is needed — this event already reaches a
            // parked loop, exactly as story #572 anticipated.
            WindowEvent::CursorMoved { position, .. } => {
                let (signal, width) = (self.scenes[self.current].signal, self.extent.0);
                let changed = match self.live.as_mut() {
                    Some(live) => crate::input::cursor_moved(live, signal, position.x, width),
                    None => false,
                };
                if changed {
                    self.force("a pointer move");
                }
            }
            // Story #573: two keys drive that same signal to either end of its
            // range, and one runs the scene's own variant switch. `repeat` is
            // excluded so holding a key neither floods the signal nor spins the
            // variant.
            WindowEvent::KeyboardInput { event, .. } => {
                // A zero extent means there is no drawable, and that is the
                // one state in which [`Host::advance_scene`] leaves `current`
                // pointing at a scene the arena was not built from — where the
                // incoming scene's action would run against the outgoing
                // scene's arena and `Txn::set_variant` would panic on a handle
                // that arena does not carry. A window with no drawable is
                // minimised and receives no key events, so this guards a state
                // rather than a case that fires.
                let drawable = self.extent.0 > 0 && self.extent.1 > 0;
                if drawable
                    && event.state == ElementState::Pressed
                    && !event.repeat
                    && let PhysicalKey::Code(code) = event.physical_key
                {
                    // The painter swap is the host's own, and is checked before
                    // the scene's keys so that a scene can never take it over.
                    if code == SWAP_PAINTER {
                        return self.swap_painter(event_loop);
                    }
                    // Copied out before the arena is borrowed: both are `Copy`,
                    // and the alternative is holding a borrow of `self.scenes`
                    // across the call.
                    let entry = &self.scenes[self.current];
                    let (signal, action) = (entry.signal, entry.action);
                    let changed = match self.live.as_mut() {
                        Some(live) => {
                            crate::input::key(code, signal, action, live, &mut self.arena)
                        }
                        None => false,
                    };
                    if changed {
                        self.force("a key press");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
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

    /// The other half: rearming releases exactly one more pulse, so the host
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
