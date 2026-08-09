//! The canvas, the adapter, and the frame loop.
//!
//! Gated on `wasm32` in one place rather than item by item, so that the one
//! thing this crate can be wrong about *without* a browser — the
//! `Content-Range` grammar — stays compiled and tested on the host platform.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_core::Arena;
use dashscene_gpu::{Changes, GpuPainter, SurfaceRenderer};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;

use crate::WebError;
use crate::recovery::{Recovery, recovery};

/// Builds the scene for a drawable of this size.
///
/// Called again on every resize, because a scene built in code derives its
/// offsets from the extent it was given. A loaded document carries its own
/// resolved size and passes [`None`] instead.
pub type SceneBuilder = fn(&mut Arena, u32, u32) -> LiveScene;

/// Called once per frame, with the seconds elapsed since the first one.
///
/// This is where an embedder writes its signals. The host does not know what
/// they are: it hands over the clock and the scene, and whatever the hook
/// writes is picked up by the `tick` that follows in the same frame.
///
/// A closure rather than a function pointer, because an embedder needs to keep
/// state between frames — at minimum, what it has already written. Writing the
/// same signal every frame is not free: it marks the binding dirty, so `tick`
/// never takes its idle early return and the page never parks.
pub type FrameHook = Box<dyn FnMut(&mut LiveScene, f64, FrameKind)>;

/// The self-rescheduling `requestAnimationFrame` closure, held inside its own
/// `Rc` so that it outlives the call that scheduled it.
type FrameClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

/// How many consecutive surface rebuilds the loop will attempt before giving up.
///
/// A recovery that works is followed by a frame, and a frame resets the count —
/// so this bounds a surface that is being lost *repeatedly*, which is what a
/// removed GPU or an unrecoverable driver reset looks like. Without a bound the
/// loop asks for a new adapter, device and pipeline set every time the old one
/// fails, forever, at whatever rate the browser will schedule it.
///
/// Three rather than one, because a single loss during a driver reset is exactly
/// the case worth recovering from and the second attempt often succeeds.
const MAX_CONSECUTIVE_RECOVERIES: u32 = 3;

/// What the frame closure and its [`LoopHandle`] both reach.
struct LoopState {
    /// False once the loop has been asked to stop. Read at the top of every
    /// frame and again before rescheduling.
    running: Cell<bool>,
    /// The `requestAnimationFrame` id that has been scheduled and has not yet
    /// fired, so that stopping can cancel it.
    ///
    /// This is what makes dropping the frame `Closure` safe. A callback the
    /// browser has already registered points at that closure through a
    /// `wasm-bindgen` shim, and invoking a shim whose closure has been dropped
    /// throws "closure invoked recursively or after being dropped" — an uncaught
    /// error on the ordinary unmount path. Cancelling first means the browser
    /// has nothing left to invoke.
    pending: Cell<Option<i32>>,
    /// Consecutive recoveries with no successful frame between them. See
    /// [`MAX_CONSECUTIVE_RECOVERIES`].
    recoveries: Cell<u32>,
}

/// Why [`FrameHook`] is being called.
///
/// This exists because of a trap an embedder would otherwise have to discover.
/// A hook that tracks what it has already applied — which it must, see above —
/// would decline to write anything after a resize, because the elapsed time has
/// not changed. But the scene it is writing into is a **new** one, rebuilt for
/// the new extent, holding none of those writes. The picture would silently
/// revert to its initial state on the first resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// An ordinary frame. The scene holds whatever the hook last wrote.
    Continuing,
    /// The scene was just rebuilt for a new extent and holds nothing the hook
    /// wrote. Anything tracked as already applied has to be applied again.
    Rebuilt,
}

/// A canvas, sized in device pixels, with a painter attached to it.
///
/// Built before the scene, because a scene built in code needs the extent to
/// build *for*. That ordering is the reason this is a separate step rather
/// than something [`Host::new`] does: an embedder attaches the surface, reads
/// [`Surface::extent`], builds its scene at that size, and then hands both to
/// the host.
pub struct Surface {
    renderer: SurfaceRenderer,
    canvas: HtmlCanvasElement,
    window: web_sys::Window,
    extent: (u32, u32),
}

impl Surface {
    /// Finds the canvas with this id, sizes its drawable, and acquires an
    /// adapter for it.
    ///
    /// Asynchronous because acquiring an adapter and a device is, on the web,
    /// and cannot be waited for — `crates/dashscene-gpu/src/render.rs` records
    /// why blocking on it deadlocks against the event loop that would resolve
    /// it. An embedder calls this from `wasm_bindgen_futures::spawn_local`.
    ///
    /// The canvas is named rather than taken as "the first canvas on the
    /// page": a page that grew a second one would otherwise start drawing into
    /// whichever came first in the document.
    pub async fn attach(canvas_id: &str) -> Result<Self, WebError> {
        let window = web_sys::window().ok_or(WebError::NoWindow)?;
        let canvas = canvas(&window, canvas_id)?;
        // The drawable is measured in device pixels, which is what a painter
        // draws and what a swapchain is configured with. CSS pixels are what
        // the page lays out in, and on a display with a scale factor the two
        // differ — a host that configured the surface in CSS pixels would draw
        // a picture the browser then rescaled, which is the softness this
        // painter exists to avoid.
        let (width, height) = drawable(&window, &canvas);
        canvas.set_width(width);
        canvas.set_height(height);

        let surface_canvas = canvas.clone();
        let renderer = SurfaceRenderer::for_canvas(canvas, width, height)
            .await
            .map_err(WebError::Renderer)?;
        Ok(Self {
            renderer,
            canvas: surface_canvas,
            window,
            extent: (width, height),
        })
    }

    /// The drawable's size in device pixels. Build the scene for this, not for
    /// the canvas's CSS size.
    pub fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// The adapter and format actually acquired, for an embedder that wants to
    /// report them. `demo-web` logs this line.
    pub fn describe(&self) -> String {
        let info = self.renderer.adapter_info();
        let (width, height) = self.extent;
        format!(
            "dashscene-gpu ({}, {:?}, {:?}) on a {width}x{height} drawable",
            info.name,
            info.backend,
            self.renderer.format()
        )
    }
}

/// The frame loop's state.
pub struct Host {
    arena: Arena,
    live: LiveScene,
    /// [`None`] only while a lost surface is being rebuilt, between
    /// [`Host::release_surface`] and [`Host::adopt`]. The loop does not schedule
    /// a frame through that window, so no frame runs without one.
    renderer: Option<SurfaceRenderer>,
    painter: GpuPainter,
    /// The previous frame's timestamp, for the animation delta.
    previous: Option<f64>,
    /// The embedder's per-frame callback.
    on_frame: FrameHook,
    /// The first frame's timestamp, which elapsed time is measured from.
    started: Option<f64>,
    /// Seconds since the first frame, as last handed to [`FrameHook`]. Held so
    /// a rebuild can re-establish the embedder's state at the same point on
    /// its own timeline rather than at zero.
    elapsed: f64,
    /// How to rebuild the scene for a new extent, or [`None`] for a loaded
    /// document, which carries its own.
    builder: Option<SceneBuilder>,
    canvas: HtmlCanvasElement,
    window: web_sys::Window,
    /// The drawable the surface is currently configured for, in device pixels.
    extent: (u32, u32),
    /// Why the next frame must paint whatever the generation says. Consumed by
    /// the frame that acts on it.
    ///
    /// The native host has carried one since story #572; this loop had no need
    /// of one until a surface could be rebuilt underneath it. A rebuilt surface
    /// is the case the generation cannot report: the scene has not changed, so
    /// `advanced()` is false, and the new device has drawn nothing — without
    /// this the canvas would stay blank until something else happened to move
    /// the scene.
    forced: Option<&'static str>,
}

impl Host {
    /// Takes an attached surface and a scene already built for its extent.
    ///
    /// `builder` rebuilds that scene when the canvas changes size. Pass
    /// [`None`] for a loaded document, which carries its own resolved size: a
    /// resize then reconfigures the surface and leaves the picture alone, which
    /// is the same answer the native host gives. Rebuilding a document would
    /// also mean fetching it again, which a frame callback cannot do.
    pub fn new(
        surface: Surface,
        arena: Arena,
        live: LiveScene,
        on_frame: FrameHook,
        builder: Option<SceneBuilder>,
    ) -> Self {
        Self {
            arena,
            live,
            renderer: Some(surface.renderer),
            painter: GpuPainter::new(),
            previous: None,
            on_frame,
            started: None,
            elapsed: 0.0,
            builder,
            canvas: surface.canvas,
            window: surface.window,
            extent: surface.extent,
            forced: None,
        }
    }

    /// Schedules the loop and returns a handle that stops it; the browser drives
    /// it from here.
    ///
    /// Errors are not returned, because after this point there is no caller to
    /// return to — the browser owns the stack. They go to the console as
    /// errors; an embedder that wants them elsewhere passes a reporter to
    /// [`Host::spin_reporting`].
    ///
    /// **Dropping the returned [`LoopHandle`] stops the loop.** An embedder that
    /// wants the loop to run until the page goes away says so with
    /// [`LoopHandle::detach`]; see that type for why the default is this way
    /// round.
    #[must_use = "dropping the LoopHandle stops the loop; call detach() to run until the page goes \
                  away"]
    pub fn spin(self) -> LoopHandle {
        self.spin_reporting(report_to_console)
    }

    /// [`spin`](Self::spin), with the embedder's own error reporting.
    ///
    /// The reporter may capture, for the same reason [`FrameHook`] may: routing
    /// a failure into an embedder's own state — a signal, a bound JavaScript
    /// callback, a telemetry client — needs something held from before the loop
    /// started. A function pointer would force the statics this crate avoids
    /// everywhere else.
    ///
    /// It is called for a **recoverable** failure too, immediately before the
    /// surface is rebuilt. A context loss that the loop rode out is still
    /// something an embedder counting failures wants to see, and the alternative
    /// — reporting only what killed the loop — would make the recovery
    /// invisible exactly when it is worth knowing about.
    #[must_use = "dropping the LoopHandle stops the loop; call detach() to run until the page goes \
                  away"]
    pub fn spin_reporting(self, report: impl Fn(&WebError) + 'static) -> LoopHandle {
        let window = self.window.clone();
        self.run_loop(&window, report)
    }

    fn run_loop(
        self,
        window: &web_sys::Window,
        report: impl Fn(&WebError) + 'static,
    ) -> LoopHandle {
        // The idiom `requestAnimationFrame` needs and Rust does not have: a
        // closure that reschedules itself. The handle is held inside its own
        // closure through an `Rc`, which is what keeps it alive after this
        // function returns.
        let host = Rc::new(RefCell::new(self));
        let holder: FrameClosure = Rc::new(RefCell::new(None));
        let next = holder.clone();
        let owner = window.clone();
        let state = Rc::new(LoopState {
            running: Cell::new(true),
            pending: Cell::new(None),
            recoveries: Cell::new(0),
        });
        let live = state.clone();
        // `Rc` because both the frame closure and the future that rebuilds the
        // surface report through it, and `impl Fn` is not `Clone`.
        let report = Rc::new(report);
        // The future that rebuilds the surface outlives the frame it was
        // scheduled from, and must not be what keeps the host alive: a handle
        // dropped while that rebuild is in flight has to free the arena, the
        // painter and the renderer regardless. It upgrades this **after** its
        // await, never before, which is what makes that true.
        let weak = Rc::downgrade(&host);

        *holder.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
            // This id has fired and can no longer be cancelled.
            live.pending.set(None);
            // The handle was dropped, or `stop` was called, between this frame
            // being scheduled and it running.
            if !live.running.get() {
                return;
            }
            match host.borrow_mut().frame(timestamp) {
                Ok(()) => {
                    // A frame reached the canvas, so whatever was recovered
                    // from is behind us: the next loss starts a fresh count
                    // rather than inheriting an old one.
                    live.recoveries.set(0);
                    // Checked again because the embedder's `FrameHook` runs
                    // inside `frame`, and `LoopHandle::stop` is documented as
                    // safe to call from it. Rescheduling here would restart a
                    // loop that was just stopped.
                    if live.running.get()
                        && let Some(closure) = next.borrow().as_ref()
                    {
                        request_frame(&owner, closure, &live);
                    }
                }
                Err(error) => {
                    report(&error);
                    match recovery(&error) {
                        // The remedy `dashscene_gpu::FrameError::Lost` names.
                        // Asynchronous, because acquiring an adapter is — which
                        // is why it cannot happen inline here and why the loop
                        // stops scheduling until it lands.
                        Recovery::Rebuild => rebuild_surface(
                            weak.clone(),
                            next.clone(),
                            owner.clone(),
                            live.clone(),
                            report.clone(),
                        ),
                        // Repeating the failure sixty times a second into the
                        // console is not a recovery. The loop is finished, so
                        // it releases the host rather than sitting on a device
                        // and an arena for the life of the page — the leak
                        // `LoopHandle` prevents on the embedder's path, and
                        // which the loop's own path has to prevent too.
                        Recovery::Stop => {
                            live.running.set(false);
                            release(next.clone());
                        }
                    }
                }
            }
        }));

        if let Some(closure) = holder.borrow().as_ref() {
            request_frame(window, closure, &state);
        }
        LoopHandle {
            state,
            holder,
            window: window.clone(),
            detached: false,
        }
    }

    /// Gives up the current surface, before a replacement is built for the same
    /// canvas.
    ///
    /// Ordered this way because the native crate's rebind states the rule and
    /// follows it: "Both own a surface on this one window, and holding two at
    /// once is the state neither windowing backend is asked to support."
    /// `canvas.getContext("webgpu")` hands back the *same* `GPUCanvasContext`
    /// object every time, so two `wgpu::Surface`s built from one canvas would be
    /// two configurations of one context — and would hold two devices and two
    /// full sets of atlas and instance buffers at the moment the GPU has just
    /// failed, which is the worst moment to ask for them.
    ///
    /// The loop stops scheduling while this is the state, so no frame runs
    /// without a renderer.
    fn release_surface(&mut self) {
        self.renderer = None;
    }

    /// Adopts a surface built against the same canvas, after a context loss.
    ///
    /// `document_replaced` is deliberately **not** called. It exists to clear
    /// what a renderer holds about the previous document, and this renderer is
    /// new: `uploaded_generation` starts `None`, so the first frame through it
    /// takes the whole-write path rather than patching ranges into a buffer the
    /// device never received. Calling it would be a second mechanism for a state
    /// the constructor already establishes.
    fn adopt(&mut self, renderer: SurfaceRenderer) {
        self.renderer = Some(renderer);
        // The scene did not change, so the generation cannot ask for this frame.
        // Without it the new device would hold an empty swapchain until
        // something else moved the scene — which, on a settled page, is never.
        self.forced = Some("the surface was rebuilt after a context loss");
    }

    /// Reconfigures for the canvas's current size, when it has changed.
    ///
    /// Rebuilds the scene at the new extent, as the native host does: a scene
    /// built in code derives every offset from the drawable it is given, so the
    /// new arena is the picture for the new size. [`FrameHook`] is called again
    /// at the same elapsed time, so a resize resumes the embedder's state
    /// rather than snapping every signal back to its initial value.
    fn resize_if_needed(&mut self) -> Result<(), WebError> {
        let (width, height) = drawable(&self.window, &self.canvas);
        if (width, height) == self.extent {
            return Ok(());
        }
        self.extent = (width, height);
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        // The extent is recorded whether or not there is a surface to configure
        // for it, so a rebuild that lands after a resize builds for the size the
        // canvas is now rather than the size it was when the device was lost.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.resize(width, height).map_err(WebError::Renderer)?;
        }

        let Some(build) = self.builder else {
            // A loaded document keeps its own canvas size; only the surface
            // changed.
            return Ok(());
        };
        // The renderer is about to be handed frames from an arena that has
        // never existed before, and the incoming arena's generations start
        // again — nothing in the frames themselves says so.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.document_replaced();
        }
        self.arena = Arena::new();
        self.live = build(&mut self.arena, width, height);
        // The new scene holds nothing the hook wrote into the old one, and the
        // elapsed time has not moved, so a hook that tracks what it applied
        // would write nothing at all. `Rebuilt` is what tells it otherwise.
        (self.on_frame)(&mut self.live, self.elapsed, FrameKind::Rebuilt);
        // The generation the old arena reached names nothing in this one. The
        // gate belongs to `LiveScene` and the one built above starts unshown,
        // so replacing the scene clears it rather than this host remembering
        // to (story #810).
        Ok(())
    }

    /// One frame: advance time, and draw if anything moved.
    ///
    /// The rule is `LiveScene`'s, not this host's: the generation says whether
    /// a frame is worth drawing, and `advanced` answers it. Both hosts asked
    /// the question with a `shown` of their own until story #810 gave it one
    /// owner — this comment used to cite the *other host* for its rule
    /// rather than the record, which is the shape a duplicated contract takes
    /// (issue #775). The record that governs it is
    /// `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
    ///
    /// The generation is marked shown when `present` returns, which is **not**
    /// the same as the frame having been drawn — a zero extent, an occluded
    /// window or an acquire that timed out all return `Ok(())` without
    /// drawing. A host cannot tell, and does not try to: `Changes` in
    /// `crates/dashscene-gpu/src/render.rs` records that the generation travels
    /// with the dirty set precisely so the renderer catches a broken chain
    /// itself, applying ranges only when this frame is the immediate successor
    /// of the one on the device. Second-guessing it here would be a second
    /// mechanism for one job, and the weaker of the two.
    fn frame(&mut self, timestamp: f64) -> Result<(), WebError> {
        let dt = match self.previous {
            // Milliseconds to seconds, from a clock that may be coarsened
            // for privacy but does not run backwards. Raw from there:
            // `LiveScene::tick` applies both the ceiling and the floor — including the negative
            // case this host used to guard with a `max(0.0)` of its own —
            // so the rule has one statement rather than one per host
            // (story #810).
            Some(previous) => (timestamp - previous) / 1000.0,
            None => 0.0,
        };
        self.previous = Some(timestamp);
        // The embedder's own timeline, in seconds, held so a rebuild mid-run
        // can re-establish its state at the same point rather than at zero.
        let started = *self.started.get_or_insert(timestamp);
        self.elapsed = (timestamp - started) / 1000.0;

        // A canvas sized in CSS has no event of its own, and this host draws
        // every frame anyway, so the extent is re-measured here rather than
        // through a listener. Without it the backing store keeps its first size
        // while the element stretches, and the browser rescales a fixed picture
        // — which turns a circle into an ellipse and is exactly how this was
        // found.
        self.resize_if_needed()?;

        // The embedder's per-frame write. Without something here nothing ever
        // writes a signal, `tick` takes its idle early return after the first
        // commit, and the page draws one frame and parks — which looks exactly
        // like a working host with nothing to animate. That was one of the two
        // defects in this loop's first cut, and no test caught it.
        //
        // Called every frame rather than when the host decides something is
        // due: what counts as due is the embedder's, and a hook handed the
        // clock every frame can answer it. `demo-web` counts showcase pulses.
        (self.on_frame)(&mut self.live, self.elapsed, FrameKind::Continuing);

        self.live.tick(dt as f32, &mut self.arena);
        // Taken whether or not it is acted on, so a forced frame forces exactly
        // one — the same rule the native loop's `forced` follows.
        let forced = self.forced.take();
        if !self.live.advanced() && forced.is_none() {
            return Ok(());
        }

        let scene = self.arena.committed();
        let changes = Changes {
            rects: scene.dirty(),
            generation: scene.generation(),
        };
        // The painter takes the dirty rects; the renderer takes them *and* the
        // generation they were reported against. The generation travels so that
        // a frame the device declined breaks the chain by arithmetic rather
        // than by anyone remembering to say so — the same call the native host
        // makes, for the same reason.
        self.painter.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            Some(changes.rects),
        );
        // A frame between `release_surface` and `adopt`. The loop schedules
        // none through that window, so reaching here means something else asked
        // for a frame; there is nothing to draw into and nothing to report.
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        renderer
            .present(
                self.painter.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                scene.glyphs(),
                Some(changes),
            )
            .map_err(WebError::Frame)?;
        self.live.mark_shown();
        Ok(())
    }
}

/// Rebuilds the surface against the same canvas, and resumes the loop.
///
/// The recovery `dashscene_gpu::FrameError::Lost` names: "rebuild the
/// presenter, which is the recovery it already has". On the web that is
/// `SurfaceRenderer::for_canvas`, which is asynchronous — acquiring an adapter
/// and a device cannot be waited for on the browser's main thread
/// (`crates/dashscene-gpu/src/render.rs` records why blocking on it deadlocks).
/// So the loop stops scheduling, this future runs, and the loop resumes when it
/// lands.
///
/// The scene, the arena and the embedder's own state are untouched. Only the
/// device is new.
fn rebuild_surface(
    host: Weak<RefCell<Host>>,
    holder: FrameClosure,
    window: web_sys::Window,
    state: Rc<LoopState>,
    report: Rc<impl Fn(&WebError) + 'static>,
) {
    // A surface lost over and over is not being recovered from. Bounded here
    // rather than inside the future, so the count is spent before an adapter is
    // asked for rather than after.
    let attempt = state.recoveries.get() + 1;
    state.recoveries.set(attempt);
    if attempt > MAX_CONSECUTIVE_RECOVERIES {
        report(&WebError::Unrecoverable {
            attempts: MAX_CONSECUTIVE_RECOVERIES,
        });
        state.running.set(false);
        release(holder);
        return;
    }

    // Read out what the rebuild needs, and give up the old surface, **before**
    // anything is awaited: two surfaces on one canvas context is the state the
    // native crate's rebind documents as unsupported, and a `RefCell` borrow
    // must not be held across an await in any case — the frame closure borrows
    // the same cell.
    let (canvas, width, height) = {
        let Some(host) = host.upgrade() else {
            return;
        };
        let mut borrowed = host.borrow_mut();
        let (width, height) = borrowed.extent;
        let canvas = borrowed.canvas.clone();
        borrowed.release_surface();
        (canvas, width, height)
    };

    spawn_local(async move {
        match SurfaceRenderer::for_canvas(canvas, width, height).await {
            Ok(renderer) => {
                // Upgraded **after** the await, never before. Holding a strong
                // reference across it would make this future the thing keeping
                // the host alive, so a handle dropped while the adapter request
                // was in flight would not free the arena, the painter or the
                // renderer — and if the request never settled, would not free
                // them at all.
                let Some(host) = host.upgrade() else {
                    return;
                };
                // The loop was stopped while the adapter request was in flight.
                // Installing a device on a canvas that is being torn down is
                // worse than doing nothing.
                if !state.running.get() {
                    return;
                }
                host.borrow_mut().adopt(renderer);
                if let Some(closure) = holder.borrow().as_ref() {
                    request_frame(&window, closure, &state);
                }
            }
            // The canvas could not give another device. Reported and not
            // retried: a loop that rebuilds on every failed rebuild is a loop
            // asking the same question forever. The loop is finished, so it
            // releases the host rather than sitting on an arena for the life of
            // the page.
            Err(error) => {
                report(&WebError::Renderer(error));
                state.running.set(false);
                release(holder);
            }
        }
    });
}

/// Drops the frame closure, and the host it holds, on a microtask.
///
/// Deferred because this is reached from inside the frame closure, and a closure
/// cannot drop itself while the browser is executing it. By the time a microtask
/// runs, the callback has returned.
///
/// Safe without cancelling a `requestAnimationFrame` only because every caller
/// reaches this from a path that did not reschedule one.
/// [`LoopHandle::stop`] cancels, because it can be called at any time.
fn release(holder: FrameClosure) {
    spawn_local(async move {
        *holder.borrow_mut() = None;
    });
}

/// Stops the frame loop.
///
/// Returned by [`Host::spin`]. **Dropping it stops the loop**, which is the way
/// round an embedder needs: mounting and unmounting a canvas is ordinary — a
/// single-page application routing away, a component unmounting, a modal
/// closing — and the failure that shape prevents is the loop going on ticking
/// against a canvas that is no longer on the page, holding the arena, the
/// painter and the renderer alive with it (issue #814).
///
/// The other way round — a loop that runs until something explicitly stops it —
/// makes the leak the default and the correct behaviour the thing an embedder
/// has to know to ask for. This way the embedder that wants a loop outliving
/// its handle says so, in one call, and [`Host::spin`] is `#[must_use]` so
/// ignoring the return value is a warning rather than a silent stop.
pub struct LoopHandle {
    state: Rc<LoopState>,
    /// The self-rescheduling closure. Cleared when the loop stops, which is what
    /// breaks the `Rc` cycle holding the host.
    holder: FrameClosure,
    /// The window the pending frame was requested from, so that stopping can
    /// cancel it.
    window: web_sys::Window,
    detached: bool,
}

impl LoopHandle {
    /// Stops the loop. Idempotent, and safe to call from inside a
    /// [`FrameHook`].
    ///
    /// No further frame is requested. The host — and the arena, the painter and
    /// the renderer it owns — is freed on a microtask rather than here, because
    /// this may be called from inside the frame closure itself and a closure
    /// cannot drop itself while the browser is executing it.
    ///
    /// **The already-scheduled frame is cancelled first**, and that ordering is
    /// the whole of why this is safe. Every successful frame schedules the next
    /// one before it returns, so at almost any moment there is a
    /// `requestAnimationFrame` registered against the closure below. Dropping
    /// the closure while the browser still holds that registration means the
    /// browser invokes a `wasm-bindgen` shim whose closure is gone, which throws
    /// "closure invoked recursively or after being dropped" — on the ordinary
    /// unmount path this type exists for. Cancelling leaves nothing to invoke.
    pub fn stop(&self) {
        self.state.running.set(false);
        if let Some(pending) = self.state.pending.take() {
            // Failure here means the id was already spent, which is the state
            // this is trying to reach.
            let _ = self.window.cancel_animation_frame(pending);
        }
        let holder = self.holder.clone();
        spawn_local(async move {
            // Dropping the `Closure` drops the host with it. By the time a
            // microtask runs, the frame callback this may have been called from
            // has returned.
            *holder.borrow_mut() = None;
        });
    }

    /// Runs until the page goes away, and gives up the ability to stop.
    ///
    /// What a full-page demonstration wants, and what `demo-web` calls. Named
    /// rather than implied, so the loop that outlives its handle is a decision
    /// in the source rather than a dropped value.
    pub fn detach(mut self) {
        self.detached = true;
    }
}

impl Drop for LoopHandle {
    fn drop(&mut self) {
        if !self.detached {
            self.stop();
        }
    }
}

/// Schedules the next frame, recording the id so that stopping can cancel it.
///
/// The id is what makes the frame closure safe to drop; see [`LoopState::pending`].
fn request_frame(window: &web_sys::Window, closure: &Closure<dyn FnMut(f64)>, state: &LoopState) {
    match window.request_animation_frame(closure.as_ref().unchecked_ref()) {
        Ok(pending) => state.pending.set(Some(pending)),
        // The browser refused to schedule. Nothing is pending, which is what
        // the `None` records; the loop has no frame coming and no way to ask
        // for one, so it is over.
        Err(_) => state.pending.set(None),
    }
}

/// The canvas the embedder named.
fn canvas(window: &web_sys::Window, id: &str) -> Result<HtmlCanvasElement, WebError> {
    window
        .document()
        .ok_or(WebError::NoWindow)?
        .get_element_by_id(id)
        .ok_or_else(|| WebError::NoCanvas(id.to_string()))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| WebError::NotACanvas(id.to_string()))
}

/// The canvas's size in device pixels, which is what the drawable must be.
fn drawable(window: &web_sys::Window, canvas: &HtmlCanvasElement) -> (u32, u32) {
    let scale = window.device_pixel_ratio().max(1.0);
    let area = canvas.get_bounding_client_rect();
    // At least one pixel on each axis. A canvas the page has not laid out
    // measures zero, and a zero-extent drawable is a swapchain that configures
    // nothing.
    let width = ((area.width() * scale).round() as u32).max(1);
    let height = ((area.height() * scale).round() as u32).max(1);
    (width, height)
}

/// Writes one line to the browser console.
///
/// Public because an embedder reporting what it loaded should sound like the
/// integration reporting what it attached, rather than inventing a second
/// prefix.
pub fn log(message: &str) {
    web_sys::console::log_1(&JsValue::from_str(&format!("dashscene: {message}")));
}

/// Writes one line to the browser console, as an **error**.
///
/// The default reporter, and the severity matters: a failure inside the frame
/// loop either ends it or costs a device rebuild, and a page that reports either
/// as an ordinary log line disappears from a console filtered to errors and from
/// anything scraping `console.error`.
///
/// It is deliberately the same severity for both. A recovered context loss is
/// still the page having lost its device, and a reporter that graded it down
/// would be deciding for the embedder that the recovery made it uninteresting —
/// which is the judgement `Host::spin_reporting` exists to hand over.
fn report_to_console(error: &WebError) {
    web_sys::console::error_1(&JsValue::from_str(&format!("dashscene: {error}")));
}

/// Sends a panic to the console instead of leaving it an unreachable trap.
///
/// Written out rather than taken from `console_error_panic_hook`, which is this
/// much behaviour and a dependency. Without it a panic in wasm surfaces as
/// "unreachable executed" with no message and no location, which is the least
/// useful failure a browser can show.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&format!("dashscene panicked: {info}")));
    }));
}
