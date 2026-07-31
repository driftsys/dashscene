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
use std::thread;
use std::time::{Duration, Instant};

use dashlang::LiveScene;
use dashscene_core::Arena;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::present::{Present, PresentError, SkiaPresenter};

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

/// Opens a window, binds the Skia presenter to it, and runs the frame loop
/// until the window is closed.
pub fn run(
    title: &'static str,
    scene: SceneBuilder,
    pulse: ScenePulse,
) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    // The starting mode. The first frame replaces it: `WaitUntil` while the
    // generation advances, `Wait` once it is steady.
    event_loop.set_control_flow(ControlFlow::Wait);
    spawn_pulse_driver(event_loop.create_proxy());

    let mut host = Host {
        title,
        scene,
        pulse,
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
fn spawn_pulse_driver(proxy: EventLoopProxy<Wake>) {
    thread::spawn(move || {
        loop {
            thread::sleep(PULSE_INTERVAL);
            if proxy.send_event(Wake::Pulse).is_err() {
                // The event loop has exited and will not run another frame.
                return;
            }
        }
    });
}

/// What the loop was doing when it last parked, so waking can report what did
/// **not** happen while it was parked.
struct Parked {
    since: Instant,
    ticks: u64,
    presents: u64,
}

struct Host {
    title: &'static str,
    scene: SceneBuilder,
    pulse: ScenePulse,
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
        let pulse = self.pulse;
        self.arena = Arena::new();
        let mut live = (self.scene)(&mut self.arena, width, height);
        // Restore the phase, so a resize resumes the scene where it was rather
        // than snapping every signal back to its initial value.
        pulse(&mut live, self.pulse_index);
        self.live = Some(live);
        // Generations count from a new arena, so the number the window shows
        // no longer names anything in this scene.
        self.shown = None;
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
        let presenter = match SkiaPresenter::new(Arc::clone(&window)) {
            Ok(presenter) => presenter,
            Err(error) => return self.fail(event_loop, error),
        };

        // Physical pixels, so the scene fills the drawable on a high-density
        // display instead of occupying a corner of it.
        let size = window.inner_size();
        let painter = presenter.name();
        self.extent = (size.width, size.height);
        self.window = Some(window);
        self.presenter = Some(Box::new(presenter));
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

        // The first frame is one of the five the generation cannot report.
        self.force("the first frame");
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Pulse => {
                let pulse = self.pulse;
                let Some(live) = self.live.as_mut() else {
                    // The window does not exist yet. Dropping this pulse costs
                    // one interval; the next one arrives on schedule.
                    return;
                };
                self.pulse_index += 1;
                pulse(live, self.pulse_index);

                if let Some(parked) = self.parked.take() {
                    eprintln!(
                        "demo: woken by pulse {} after {:.2} s parked — {} ticks and {} presents \
                         ran while parked",
                        self.pulse_index,
                        parked.since.elapsed().as_secs_f32(),
                        self.ticks - parked.ticks,
                        self.presents - parked.presents,
                    );
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
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
            WindowEvent::RedrawRequested => self.frame(event_loop),
            _ => {}
        }
    }
}
