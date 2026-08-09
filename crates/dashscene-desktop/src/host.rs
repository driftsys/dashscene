//! The window, the clock, and the frame loop that drives them (story #572,
//! extracted at story #794).
//!
//! One frame is four steps, and nothing else:
//!
//! ```text
//! dt = elapsed                          // this loop's clock
//! LiveScene::tick(dt, &mut arena)       // clamps dt, returns the generation
//! arena.committed()
//! present                               // only when LiveScene::advanced()
//! ```
//!
//! P3 holds by construction. The host owns time and nothing producer-side runs
//! inside the loop: `tick` takes `dt` as a parameter, every signal change is
//! applied on this thread before `tick` reads it, and a producer that lives
//! outside the loop reaches it only by asking the loop to run a frame
//! ([`Waker`]).
//!
//! # The clock is read here and nowhere below
//!
//! The frame clock is read in `Host::frame`, and twice more — on parking and
//! on waking — only to time the loop's own wait for a diagnostic line. No crate
//! at or below `LiveScene` may read a clock at all: that is what makes an
//! animation test reproducible, and `demo/tests/clock_invariant.rs` asserts it
//! rather than leaving it to review. The clamp itself is **not** applied here:
//! story #810 moved it into `LiveScene::tick`, so one value serves every host
//! rather than one per host. What stays here is the clock — what "elapsed"
//! means, and when it is stopped. The clamp, the absence of an accumulator, and
//! the invariant are argued in
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
//! `LiveScene::tick` holds the commit generation steady on an idle frame, and
//! `LiveScene::advanced` reports whether it moved since the last
//! `LiveScene::mark_shown`. The loop skips both paint and present when it has
//! not. That gate was each host's own until story #810 gave it one owner, so
//! two hosts could not disagree with it.
//!
//! It is a requirement rather than an optimisation because no painter has a
//! partial-redraw path — the retained mode patches its instance buffer and
//! still redraws every quad — so a static screen costs a full frame of fill
//! every frame, and not running the frame is the only thing that removes that
//! cost.
//!
//! The generation reports document and animation change only, so five cases
//! force a redraw independently of it: the first frame, a resize or surface
//! reconfigure, a scale-factor change, re-exposure after occlusion, and a lost
//! surface.
//!
//! **The fifth was not handled at all until story #834.**
//! `dashscene_gpu::FrameError::Lost` says the recovery is to rebuild the
//! presenter, which is exactly what [`Reaction::Rebind`] does — and a present
//! failure was fatal here, so the embedder was never asked and that entry point
//! could not be reached from the failure it answers. The loop now classifies the
//! failure itself through [`crate::recovery`] and rebinds without asking, which
//! is the one case where it does not need to: the remedy is the crate's own and
//! an embedder that overrode [`App::presenter`] gets its override called again.
//!
//! # The wait mode follows from that, and so does the wake mechanism
//!
//! The loop paces itself at [`FRAME_INTERVAL`] while the generation advances
//! and waits for an event while it is steady, rather than waking sixty times a
//! second to redraw an unchanged screen.
//!
//! A producer outside the loop therefore needs a way to wake it. That mechanism
//! is [`Waker`], handed to the embedder in [`App::started`] before the loop
//! runs, and it is what a scripted or externally fed signal producer sends from
//! its own thread — the loop then calls [`App::woken`] on its own thread,
//! before the next `tick`, which is what keeps P3 true across the thread
//! boundary. Input needs none of it: a window event already reaches a parked
//! loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashlang::LiveScene;
use dashscene_core::Arena;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::DesktopError;
use crate::present::{GpuPresenter, Present, PresentError};
use crate::recovery::{Recovery, recovery};

/// The pace the loop runs at while the generation advances: 60 Hz.
///
/// `ControlFlow::WaitUntil` rather than `ControlFlow::Poll`, because polling
/// spins as fast as the machine allows and this is meant to be a frame rate. A
/// frame that overruns the interval leaves the deadline already past, so the
/// loop wakes immediately and the pacing degrades to running flat out rather
/// than to dropping frames.
pub const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

/// The window size an embedder gets without asking, in logical pixels.
const DEFAULT_WINDOW_SIZE: LogicalSize<u32> = LogicalSize::new(960, 600);

/// How many consecutive presenter rebinds the loop will attempt before giving
/// up.
///
/// A rebind that works is followed by a frame, and a frame resets the count — so
/// this bounds a surface that is being lost *repeatedly*, which is what a removed
/// GPU or an unrecoverable driver reset looks like. Three rather than one,
/// because a single loss during a driver reset is exactly the case worth
/// recovering from.
const MAX_CONSECUTIVE_REBINDS: u32 = 3;

/// The message the loop's own user event carries.
///
/// Every variant carries an *intent* and no payload: `LiveScene` is owned by the
/// loop and lives on its thread, so the sender asks and the loop applies.
/// Widening this to carry data would be widening it to carry producer work
/// across a thread boundary, which is what P3 forbids. That is the rule, not the
/// variant count — story #834 added the second variant without touching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wake {
    /// Run a frame, and let the generation decide whether it paints.
    Frame,
    /// Stop the loop. [`run`] returns.
    Stop,
}

/// Asks the loop to run a frame, or to stop, from any thread.
///
/// This is what the loop's wait mode requires: while the generation is steady
/// the loop is parked in `ControlFlow::Wait`, so a producer that is not driven
/// by a window event — a scripted sequence, a timer, a data feed — cannot reach
/// the scene without one. Handed to the embedder by [`App::started`].
///
/// # Stopping, and what stays inherent to `winit`
///
/// [`Waker::stop`] is how an embedder ends a loop it did not start a window
/// for. `WindowEvent::CloseRequested` already ends one that owns its window; an
/// embedder driving an externally-owned lifetime has no such event, which is
/// the half of issue #820 that could be closed.
///
/// What is **not** removed, because `winit`'s model does not admit it: [`run`]
/// owns the calling thread until the loop ends. There is no handle returned
/// before it, because there is no "before it" on this platform — the event loop
/// is entered and the thread is inside it. So the stop is a message rather than
/// a handle, and [`App::started`] is where it is handed over, which is the last
/// point that runs on the caller's own thread.
#[derive(Clone)]
pub struct Waker(EventLoopProxy<Wake>);

impl Waker {
    /// Asks the loop to run a frame, reporting whether the message reached it.
    ///
    /// `false` means the event loop has exited and will not run another frame,
    /// which is the signal a driver thread ends on rather than an error to
    /// report.
    pub fn wake(&self) -> bool {
        self.0.send_event(Wake::Frame).is_ok()
    }

    /// Asks the loop to stop, reporting whether the message reached it.
    ///
    /// [`run`] returns `Ok(())` afterwards unless a failure had already been
    /// recorded: stopping on request is not itself a failure. `false` means the
    /// loop had already ended.
    ///
    /// Requesting a stop is not the same as the window closing, and both are
    /// ordinary. This one is for the embedder that does not own the window's
    /// lifetime and therefore never sees `CloseRequested` (issue #820).
    pub fn stop(&self) -> bool {
        self.0.send_event(Wake::Stop).is_ok()
    }
}

/// The live scene and the arena behind it, as an embedder is handed them.
///
/// Both, because the two things an embedder does from an event need different
/// halves: writing a signal needs the scene, and switching a variant needs the
/// arena as well, since `Txn::set_variant` is staged on it.
pub struct Scene<'a> {
    pub live: &'a mut LiveScene,
    pub arena: &'a mut Arena,
    /// The drawable extent in physical pixels, for an embedder mapping a
    /// pointer position onto a normalised signal.
    pub extent: (u32, u32),
}

/// What the loop should do about something the embedder just handled.
///
/// Returned rather than done directly, because each of these has to happen at a
/// point in the frame the embedder cannot reach: a rebuild drops the arena the
/// caller is borrowing, and a rebind drops a presenter that owns the window's
/// surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Nothing happened. A parked loop stays parked.
    Ignored,
    /// Run a frame, and let the generation decide whether it paints.
    ///
    /// This is what an embedder returns after writing a signal: `tick` picks
    /// the write up, the generation advances, and the frame paints because it
    /// advanced. A write that changed nothing costs one tick and no paint,
    /// which is the idle skip working rather than a frame going missing.
    Frame,
    /// Run a frame and paint it **whatever** the generation says.
    ///
    /// The stronger form, for the cases the generation cannot report: a
    /// presenter that recreated its swapchain and has drawn nothing into the
    /// new one, or a write whose effect the generation does not cover. Prefer
    /// [`Reaction::Frame`] — forcing a paint the generation would not have
    /// asked for is the cost the idle skip exists to avoid.
    Redraw,
    /// Build the scene again, into a fresh arena.
    ///
    /// The loop calls [`App::build`] for the current extent, tells the
    /// presenter the document was replaced, and forces a redraw. An embedder
    /// asks for this when *what* it wants drawn has changed, rather than the
    /// size it is drawn at — the loop already rebuilds on resize by itself.
    Rebuild,
    /// Drop the presenter and ask [`App::presenter`] for another one.
    ///
    /// The arena, the live scene, the frame clock and the embedder's own state
    /// are untouched, so the next frame is the same frame drawn by a different
    /// painter. The outgoing presenter is dropped **before** the incoming one
    /// is built: both own a surface on one window, and holding two at once is
    /// the state no windowing backend is asked to support.
    Rebind,
}

/// What an embedder supplies to the loop.
///
/// Only [`App::build`] has no default. A minimal embedder writes that one
/// method, gets a window and the lean painter, and draws.
///
/// # Where the rebuild trap is handled, and why it is not a parameter here
///
/// A scene rebuilt for a new extent is a **new** scene, holding none of the
/// signal writes the embedder made into the old one. An embedder that tracks
/// what it has already applied — which it must, since writing the same signal
/// every frame marks its binding dirty, so `tick` never takes its idle early
/// return and the loop never parks — would otherwise write nothing into the
/// new scene, and the picture would silently revert to its initial state on the
/// first resize.
///
/// `dashscene-web` names that case with a `FrameKind::Rebuilt` passed to its
/// per-frame hook, because its hook is a closure that cannot see the
/// embedder's own state. Here it needs no name: [`App::build`] is a method on
/// the embedder, so it re-applies its own phase in the same call that builds
/// the scene, and anything derived from the *presenter* rather than from the
/// scene is re-established by [`App::attached`].
pub trait App {
    /// Builds the scene for a drawable of `width` x `height` **physical**
    /// pixels, and returns the live scene the loop ticks.
    ///
    /// The extent is passed in rather than fixed because the window's physical
    /// size is only known once the window exists, and on a high-density display
    /// it is not the logical size that was asked for. A loaded document carries
    /// its own resolved size and ignores both — see [`crate::Document::load`].
    ///
    /// Called again on every resize and on every [`Reaction::Rebuild`], into a
    /// fresh arena each time. Re-apply whatever state the scene is a function
    /// of here; see the trait documentation for why that is this method's job.
    fn build(&mut self, arena: &mut Arena, width: u32, height: u32) -> LiveScene;

    /// The window to open.
    ///
    /// Defaults to a 960x600 logical window titled "dashscene".
    fn window(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title("dashscene")
            .with_inner_size(DEFAULT_WINDOW_SIZE)
    }

    /// Binds a presenter to the window.
    ///
    /// Defaults to [`GpuPresenter`], so an embedder that does not care writes
    /// nothing. Override it to select among several — which is the only reason
    /// this is a seam rather than a fixed call, and why the trait it returns is
    /// published (issue #794).
    ///
    /// Called once at startup and again on every [`Reaction::Rebind`].
    fn presenter(&mut self, window: Arc<Window>) -> Result<Box<dyn Present>, PresentError> {
        Ok(Box::new(GpuPresenter::new(window)?))
    }

    /// The loop is about to run, and this is how to wake it from another
    /// thread.
    ///
    /// Called once, before the first frame. An embedder with no off-thread
    /// producer ignores it.
    fn started(&mut self, _waker: Waker) {}

    /// A scene has just been built, or a presenter has just been bound.
    ///
    /// Both are cases where something the embedder derived from one of them is
    /// now stale: a fresh scene holds no signal the embedder wrote, and a fresh
    /// presenter has a different [`Present::name`]. Writing that derived state
    /// here rather than in [`App::build`] is what lets one implementation serve
    /// both, since a rebind does not rebuild and a rebuild does not rebind.
    fn attached(&mut self, _scene: Scene<'_>, _presenter: &str) {}

    /// A window event the loop did not consume.
    ///
    /// The loop consumes exactly what it owns — close, resize, scale factor,
    /// **re-exposure** after occlusion, and redraw — and passes everything else
    /// through untouched, including every input event. Occlusion itself is not
    /// in that list: only `Occluded(false)` is consumed, because only
    /// re-exposure forces a redraw, so `Occluded(true)` arrives here like any
    /// other event.
    fn event(&mut self, _event: &WindowEvent, _scene: Scene<'_>) -> Reaction {
        Reaction::Ignored
    }

    /// A [`Waker::wake`] arrived, on the loop's own thread.
    ///
    /// The scene is [`None`] when the window does not exist yet, which a wake
    /// sent before the first frame will find. The embedder is told anyway
    /// rather than the wake being dropped silently: a driver that waits to be
    /// released after each message would stall permanently on the one wake
    /// that arrived early.
    fn woken(&mut self, _scene: Option<Scene<'_>>) -> Reaction {
        Reaction::Ignored
    }

    /// One frame's costs, for an embedder that measures them.
    ///
    /// `present` is [`None`] when no frame reached the window — no presenter,
    /// or a presenter that declined this one because the drawable is
    /// zero-area, occluded or timed out. **None of those is a cheap frame; they
    /// are the absence of one**, and timing them is how a mean ends up
    /// describing how often the window was off-screen rather than what drawing
    /// costs.
    fn measured(&mut self, _tick: Duration, _present: Option<Duration>, _presenter: &str) {}

    /// One line of the loop's own diagnostics.
    ///
    /// The loop has things to say that only it can see — that it settled and
    /// parked, how long it stayed parked, that a redraw was forced and why —
    /// and a library that wrote them to stderr itself would be a library
    /// deciding an embedder's output format. Default: discarded.
    fn note(&mut self, _message: &str) {}
}

/// What the loop does after a paint attempt.
///
/// Named rather than inlined into `frame` so the decision can be tested; see
/// [`Host::after_paint`].
#[derive(Debug)]
enum AfterPaint {
    /// Carry on to the pacing decision.
    Continue,
    /// Drop the presenter and ask the embedder for another, then carry on.
    Rebind,
    /// Stop the loop, and report this as the failure that ended it.
    Fail(DesktopError),
}

/// What the loop was doing when it last parked, so waking can report what did
/// **not** happen while it was parked.
struct Parked {
    since: Instant,
    ticks: u64,
    presents: u64,
}

/// Opens a window, binds a presenter to it, and runs the frame loop until the
/// window is closed.
pub fn run<A: App>(app: A) -> Result<(), DesktopError> {
    let event_loop = EventLoop::<Wake>::with_user_event()
        .build()
        .map_err(|error| DesktopError::EventLoop(error.to_string()))?;
    // The starting mode. The first frame replaces it: `WaitUntil` while the
    // generation advances, `Wait` once it is steady.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut host = Host {
        app,
        window: None,
        presenter: None,
        arena: Arena::new(),
        live: None,
        extent: (0, 0),
        previous_frame: None,
        forced: None,
        ticks: 0,
        presents: 0,
        rebinds: 0,
        parked: None,
        failure: None,
    };
    host.app.started(Waker(event_loop.create_proxy()));
    event_loop
        .run_app(&mut host)
        .map_err(|error| DesktopError::EventLoop(error.to_string()))?;
    match host.failure {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

struct Host<A: App> {
    app: A,
    window: Option<Arc<Window>>,
    presenter: Option<Box<dyn Present>>,
    arena: Arena,
    live: Option<LiveScene>,
    /// The drawable extent the loop last acted on, in physical pixels. A
    /// `Resized` that repeats it changes nothing and is dropped before it costs
    /// a scene rebuild.
    extent: (u32, u32),
    /// When the previous frame ran. `None` means the frame clock is stopped: on
    /// the first frame, and across the loop's own wait.
    previous_frame: Option<Instant>,
    /// Why the next frame must paint whatever the generation says. Consumed by
    /// the frame that acts on it.
    forced: Option<&'static str>,
    /// How many frames have called `tick`, and how many of those reached a
    /// presenter. The gap between them is the idle skip, and it is reported
    /// rather than claimed.
    ///
    /// `presents` counts calls that returned, **not** frames that reached the
    /// window: a presenter answers [`Drawn::No`] for a zero-area drawable, an
    /// occluded surface or a timed-out acquire, and those are counted here.
    /// [`App::measured`] is where that distinction is kept, because a caller
    /// measuring frame cost has to exclude the ones that did not happen.
    ticks: u64,
    presents: u64,
    /// Consecutive presenter rebinds with no successful frame between them. See
    /// [`MAX_CONSECUTIVE_REBINDS`].
    rebinds: u32,
    parked: Option<Parked>,
    /// The first failure that stopped the loop. `ApplicationHandler`'s methods
    /// return nothing, so a failure is parked here and reported by [`run`]
    /// rather than printed and forgotten.
    failure: Option<DesktopError>,
}

impl<A: App> Host<A> {
    /// Records `failure` and asks the event loop to stop. The first failure
    /// wins: a later one is a consequence of the state the first left behind.
    fn fail(&mut self, event_loop: &ActiveEventLoop, failure: DesktopError) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        event_loop.exit();
    }

    /// Applies what the embedder asked for after an event or a wake.
    fn react(&mut self, event_loop: &ActiveEventLoop, reaction: Reaction) {
        match reaction {
            Reaction::Ignored => {}
            Reaction::Frame => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Reaction::Redraw => self.force("the embedder asked for a redraw"),
            Reaction::Rebuild => {
                let (width, height) = self.extent;
                if width == 0 || height == 0 {
                    // No drawable yet, so there is nothing to rebuild against.
                    // Whatever the embedder changed still stands; the first
                    // frame builds it.
                    return;
                }
                self.rebuild(width, height);
                self.force("the embedder rebuilt the scene");
            }
            Reaction::Rebind => self.rebind(event_loop),
        }
    }

    /// Drops the presenter and asks the embedder for another one.
    fn rebind(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        // Dropped **before** the incoming one is built. Both own a surface on
        // this one window, and holding two at once is the state neither
        // windowing backend is asked to support.
        self.presenter = None;
        match self.app.presenter(window) {
            Ok(presenter) => {
                self.presenter = Some(presenter);
                self.attached();
                // The incoming presenter adopted the window's extent at
                // construction, so nothing is re-solved and the scene keeps its
                // state. It has drawn nothing, though, and the generation is
                // unchanged — which is one of the cases the forced list exists
                // for.
                self.force("a presenter rebind");
            }
            Err(error) => self.fail(event_loop, DesktopError::Present(error)),
        }
    }

    /// Tells the embedder that the scene or the presenter has just changed.
    fn attached(&mut self) {
        let name = self
            .presenter
            .as_ref()
            .map_or("no presenter", |presenter| presenter.name())
            .to_owned();
        let extent = self.extent;
        // Borrowed field by field rather than through a helper: a method
        // handing back a `Scene` would borrow the whole of `self`, and the call
        // below needs `self.app` at the same time.
        let Some(live) = self.live.as_mut() else {
            return;
        };
        self.app.attached(
            Scene {
                live,
                arena: &mut self.arena,
                extent,
            },
            &name,
        );
    }

    /// One frame.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        // The frame clock, read once, here. `saturating_duration_since` rather
        // than subtraction because a monotonic clock is only guaranteed
        // non-decreasing, and a zero-length frame is a correct answer where a
        // panic is not.
        let now = Instant::now();
        let dt = match self.previous_frame {
            // Raw, and unclamped here. `LiveScene::tick` applies the ceiling,
            // so the rule has one statement rather than one per host (story
            // #810). The clock stays the host's, which is the split the
            // decision record's own title names.
            Some(previous) => now.saturating_duration_since(previous),
            // The clock was stopped: this is the first frame, or the first one
            // after the loop parked. No animation time passed during the loop's
            // own wait, because nothing was animating through it, so the frame
            // that ends the wait starts from zero rather than from however long
            // the window sat untouched. This is what makes the clamp a guard
            // against *external* stalls only, and it is a fact about this
            // loop's timeline rather than a policy, so it stays here.
            None => Duration::ZERO,
        };
        self.previous_frame = Some(now);

        let Some(live) = self.live.as_mut() else {
            return;
        };
        let before_tick = Instant::now();
        let generation = live.tick(dt.as_secs_f32(), &mut self.arena);
        let tick_took = before_tick.elapsed();
        self.ticks += 1;

        let advanced = live.advanced();
        let forced = self.forced.take();
        if !advanced && let Some(reason) = forced {
            // The generation could not report this one, which is the whole
            // reason the forced list exists. Reported so the path is visible
            // when it fires rather than only when it fails to.
            self.app.note(&format!("forced redraw — {reason}"));
        }
        if advanced || forced.is_some() {
            let painted = self.paint();
            match self.after_paint(painted, tick_took) {
                AfterPaint::Continue => {}
                // The recovery this crate has always had, finally reachable from
                // the failure it exists for (issue #818). `rebind` forces a
                // redraw of its own, so the next frame paints through the new
                // presenter — which is why this returns rather than falling
                // through to the pacing below.
                AfterPaint::Rebind => return self.rebind(event_loop),
                AfterPaint::Fail(error) => return self.fail(event_loop, error),
            }
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

    /// What the loop does with the outcome of a paint.
    ///
    /// Separated from the `winit` call that applies it so that it can be
    /// asserted without a display: [`ActiveEventLoop`] cannot be constructed
    /// outside a running application, so a test that reached for `frame` could
    /// not run at all. This is the branch story #834 added — the loop had none,
    /// and treated every present failure as fatal.
    fn after_paint(
        &mut self,
        painted: Result<Option<Duration>, PresentError>,
        tick_took: Duration,
    ) -> AfterPaint {
        let error = match painted {
            Ok(present_took) => {
                self.record_frame(tick_took, present_took);
                // A frame reached the window, so whatever was recovered from is
                // behind us: the next loss starts a fresh count.
                self.rebinds = 0;
                return AfterPaint::Continue;
            }
            Err(error) => error,
        };
        match recovery(&error) {
            Recovery::Rebind => {
                self.rebinds += 1;
                // A surface lost again immediately after each rebind is not
                // being recovered from. Every rebind builds a fresh
                // `wgpu::Instance`, adapter, device and pipeline set on this
                // thread, and the rebind path returns before the pacing below —
                // so without a bound an unrecoverable loss spins as fast as
                // winit will dispatch, blocking on an adapter request each time,
                // and `run` never returns.
                if self.rebinds > MAX_CONSECUTIVE_REBINDS {
                    self.app.note(&format!(
                        "the surface was lost again after each of {MAX_CONSECUTIVE_REBINDS} \
                         rebinds — giving up"
                    ));
                    return AfterPaint::Fail(DesktopError::Present(error));
                }
                // `paint` did not reach `mark_shown`, so the generation is still
                // unshown and the frame is not lost by being retried through the
                // new presenter.
                self.app
                    .note(&format!("the surface was lost — rebinding: {error}"));
                AfterPaint::Rebind
            }
            Recovery::Stop => AfterPaint::Fail(DesktopError::Present(error)),
        }
    }

    /// Draws the committed scene and posts it, reporting how long the present
    /// took.
    fn paint(&mut self) -> Result<Option<Duration>, PresentError> {
        let (Some(window), Some(presenter)) = (self.window.as_ref(), self.presenter.as_mut())
        else {
            return Ok(None);
        };
        // Tells the compositor a frame is about to be posted, so it can
        // schedule the next one; winit asks for it immediately before
        // presenting.
        window.pre_present_notify();
        let before = Instant::now();
        let drawn = presenter.present(self.arena.committed())?;
        let took = before.elapsed();
        if let Some(live) = self.live.as_mut() {
            live.mark_shown();
        }
        self.presents += 1;
        Ok(drawn.drew().then_some(took))
    }

    /// Hands one frame's costs to the embedder.
    fn record_frame(&mut self, tick: Duration, present: Option<Duration>) {
        let presenter = self
            .presenter
            .as_ref()
            .map_or("no presenter", |presenter| presenter.name())
            .to_owned();
        self.app.measured(tick, present, &presenter);
    }

    /// Stops the loop until an event arrives.
    fn park(&mut self, event_loop: &ActiveEventLoop, generation: u64) {
        event_loop.set_control_flow(ControlFlow::Wait);
        // The frame clock stops with the loop; see the `None` arm in `frame`.
        self.previous_frame = None;
        if self.parked.is_none() {
            let (ticks, presents) = (self.ticks, self.presents);
            self.app.note(&format!(
                "settled at generation {generation} after {ticks} ticks and {presents} presents \
                 — waiting for an event"
            ));
            self.parked = Some(Parked {
                since: Instant::now(),
                ticks,
                presents,
            });
        }
    }

    /// Reports what did **not** happen while the loop was parked, if it was.
    ///
    /// That report is the only evidence the idle-frame skip produces at run
    /// time, so it is taken on every path that can end a park rather than only
    /// on the common one.
    fn report_park(&mut self) {
        let Some(parked) = self.parked.take() else {
            return;
        };
        let (elapsed, ticks, presents) = (
            parked.since.elapsed().as_secs_f32(),
            self.ticks - parked.ticks,
            self.presents - parked.presents,
        );
        self.app.note(&format!(
            "woken after {elapsed:.2} s parked — {ticks} ticks and {presents} presents ran while \
             parked"
        ));
    }

    /// Makes the next frame paint and present whatever the generation says.
    fn force(&mut self, reason: &'static str) {
        self.forced = Some(reason);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// The resize path: reconfigure the presenter, rebuild for the new extent,
    /// and repaint.
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
            return self.fail(event_loop, DesktopError::Present(error));
        }

        // A zero dimension is a minimised window. There is nothing to solve for
        // and nothing to draw into; the next non-zero `Resized` rebuilds.
        if width > 0 && height > 0 {
            self.rebuild(width, height);
            self.app.note(&format!(
                "resized to {width}x{height} physical pixels — scene rebuilt"
            ));
        }
        self.force("a resize");
    }

    /// Builds the scene for `width` x `height` into a fresh arena.
    fn rebuild(&mut self, width: u32, height: u32) {
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
        let live = self.app.build(&mut self.arena, width, height);
        // Generations count from a new arena, so nothing this scene commits can
        // be compared against what the window showed of the last one. The gate
        // is `LiveScene`'s and a new one starts unshown, so replacing the scene
        // clears it rather than this loop remembering to (story #810).
        self.live = Some(live);
        self.attached();
    }
}

impl<A: App> ApplicationHandler<Wake> for Host<A> {
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

        let window = match event_loop.create_window(self.app.window()) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, DesktopError::Window(error.to_string())),
        };
        let presenter = match self.app.presenter(Arc::clone(&window)) {
            Ok(presenter) => presenter,
            Err(error) => return self.fail(event_loop, DesktopError::Present(error)),
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

        let rects = self.arena.committed().rects().len();
        self.app.note(&format!(
            "{painter} — {}x{} physical pixels, {rects} rects",
            size.width, size.height,
        ));
        self.app.note(&format!(
            "frame loop — dt clamped to {} ms, {} Hz while the generation advances, waiting for \
             an event while it is steady",
            (dashlang::MAX_FRAME_DELTA * 1000.0).round(),
            (1.0 / FRAME_INTERVAL.as_secs_f32()).round(),
        ));

        // The first frame is one of the cases the generation cannot report.
        self.force("the first frame");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        // Taken before the embedder runs, so that a wake which also ends the
        // scene's turn still reports the idle interval it woke from rather than
        // swallowing it.
        self.report_park();
        // Matched exhaustively rather than tested against one variant, so that
        // the next variant added to `Wake` is a compile error here rather than
        // silently treated as "run a frame".
        match event {
            Wake::Stop => {
                // Not a failure: `run` returns `Ok(())` unless something had
                // already gone wrong, so an embedder that stops its own loop
                // does not have to distinguish "I asked for this" from a real
                // error.
                self.app.note("the embedder asked the loop to stop");
                event_loop.exit();
                return;
            }
            Wake::Frame => {}
        }
        let extent = self.extent;
        let scene = self.live.as_mut().map(|live| Scene {
            live,
            arena: &mut self.arena,
            extent,
        });
        let reaction = self.app.woken(scene);
        self.react(event_loop, reaction);
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
                self.app
                    .note(&format!("scale factor is now {scale_factor}"));
                self.force("a scale-factor change");
            }
            // Not every platform preserves the contents of an occluded surface,
            // and the generation cannot report that they were lost.
            WindowEvent::Occluded(false) => self.force("re-exposure after occlusion"),
            WindowEvent::RedrawRequested => self.frame(event_loop),
            // Everything else is the embedder's, input included. A window event
            // already reaches a parked loop, so input needs no waker.
            other => {
                let extent = self.extent;
                let Some(live) = self.live.as_mut() else {
                    return;
                };
                let reaction = self.app.event(
                    &other,
                    Scene {
                        live,
                        arena: &mut self.arena,
                        extent,
                    },
                );
                self.react(event_loop, reaction);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dashscene_gpu::FrameError;

    use super::*;

    /// The smallest thing that satisfies [`App`]. It builds no scene, and none
    /// of these tests runs a frame — what is under test is the branch the loop
    /// takes on a present failure, which happens before any of the rest.
    struct Stub {
        notes: Vec<String>,
    }

    impl App for Stub {
        fn build(&mut self, arena: &mut Arena, _width: u32, _height: u32) -> LiveScene {
            dashlang::attach_live(arena, Box::new(dashscene_engine::TaffySolver::new()))
        }

        fn note(&mut self, message: &str) {
            self.notes.push(message.to_owned());
        }
    }

    /// A host with no window and no presenter.
    ///
    /// Both are `None` because neither can be built without a display, and
    /// neither is reached: [`Host::after_paint`] is handed the outcome of a
    /// paint rather than performing one, which is the whole reason it was split
    /// out of `frame`.
    fn host() -> Host<Stub> {
        Host {
            app: Stub { notes: Vec::new() },
            window: None,
            presenter: None,
            arena: Arena::new(),
            live: None,
            extent: (0, 0),
            previous_frame: None,
            forced: None,
            ticks: 0,
            presents: 0,
            rebinds: 0,
            parked: None,
            failure: None,
        }
    }

    /// The defect issue #818 filed: a recoverable loss ended the loop, and the
    /// rebind that answers it was unreachable.
    ///
    /// This asserts the loop's own branch rather than `recovery`'s
    /// classification — a test of the classification alone would pass even if
    /// the loop ignored it, which is exactly the state this story found.
    #[test]
    fn a_lost_surface_rebinds_rather_than_ending_the_loop() {
        let mut host = host();
        let after = host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
        assert!(
            matches!(after, AfterPaint::Rebind),
            "a lost surface must rebind the presenter, not end the loop: {after:?}"
        );
    }

    /// The recovery is reported, so a rebind is visible when it happens rather
    /// than only when it fails.
    #[test]
    fn the_rebind_is_reported_to_the_embedder() {
        let mut host = host();
        let _ = host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
        assert!(
            host.app.notes.iter().any(|note| note.contains("rebinding")),
            "the rebind went unreported: {:?}",
            host.app.notes
        );
    }

    /// The other half of the contract, and the one a permissive fix would
    /// break: a failure that rebinding cannot answer must still stop the loop.
    #[test]
    fn a_fatal_present_failure_still_ends_the_loop() {
        for error in [
            PresentError::Frame(FrameError::Outdated),
            PresentError::Frame(FrameError::Validation),
            PresentError::Surface("no adapter".to_owned()),
            PresentError::Extent {
                width: 40_000,
                height: 40_000,
                max: 16_384,
            },
        ] {
            let mut host = host();
            let after = host.after_paint(Err(error), Duration::ZERO);
            assert!(
                matches!(after, AfterPaint::Fail(_)),
                "this must end the loop, not rebind: {after:?}"
            );
        }
    }

    /// A frame that succeeded is not a decision at all.
    #[test]
    fn a_successful_paint_carries_on() {
        let mut host = host();
        let after = host.after_paint(Ok(Some(Duration::from_millis(2))), Duration::ZERO);
        assert!(matches!(after, AfterPaint::Continue), "{after:?}");
    }

    /// A surface lost again after every rebind is not being recovered from.
    ///
    /// Without the bound this is an unbounded loop that builds a fresh
    /// `wgpu::Instance`, adapter, device and pipeline set on the event-loop
    /// thread every iteration, and `run` never returns.
    #[test]
    fn a_surface_lost_after_every_rebind_eventually_gives_up() {
        let mut host = host();
        for attempt in 1..=MAX_CONSECUTIVE_REBINDS {
            let after =
                host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
            assert!(
                matches!(after, AfterPaint::Rebind),
                "attempt {attempt} should still rebind: {after:?}"
            );
        }
        let after = host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
        assert!(
            matches!(after, AfterPaint::Fail(_)),
            "the loop must give up after {MAX_CONSECUTIVE_REBINDS} rebinds: {after:?}"
        );
    }

    /// The bound is on *consecutive* rebinds, so a device that recovers and
    /// later fails again gets the full allowance a second time. A counter that
    /// never reset would turn a long-lived host into one that dies on its fourth
    /// unrelated driver reset.
    #[test]
    fn a_frame_between_losses_resets_the_allowance() {
        let mut host = host();
        for _ in 0..MAX_CONSECUTIVE_REBINDS {
            let _ = host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
        }
        let _ = host.after_paint(Ok(Some(Duration::from_millis(2))), Duration::ZERO);

        for attempt in 1..=MAX_CONSECUTIVE_REBINDS {
            let after =
                host.after_paint(Err(PresentError::Frame(FrameError::Lost)), Duration::ZERO);
            assert!(
                matches!(after, AfterPaint::Rebind),
                "attempt {attempt} after a good frame should rebind: {after:?}"
            );
        }
    }
}
