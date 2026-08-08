//! The canvas, the adapter, and the frame loop.
//!
//! Gated on `wasm32` in one place rather than item by item, so that the one
//! thing this crate can be wrong about *without* a browser — the
//! `Content-Range` grammar — stays compiled and tested on the host platform.

use std::cell::RefCell;
use std::rc::Rc;

use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_core::Arena;
use dashscene_gpu::{Changes, GpuPainter, SurfaceRenderer};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::WebError;

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
    renderer: SurfaceRenderer,
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
            renderer: surface.renderer,
            painter: GpuPainter::new(),
            previous: None,
            on_frame,
            started: None,
            elapsed: 0.0,
            builder,
            canvas: surface.canvas,
            window: surface.window,
            extent: surface.extent,
        }
    }

    /// Schedules the loop and returns; the browser drives it from here.
    ///
    /// Errors are not returned, because after this point there is no caller to
    /// return to — the browser owns the stack. They go to the console as
    /// errors; an embedder that wants them elsewhere passes a reporter to
    /// [`Host::spin_reporting`].
    pub fn spin(self) {
        self.spin_reporting(report_to_console);
    }

    /// [`spin`](Self::spin), with the embedder's own error reporting.
    ///
    /// The reporter may capture, for the same reason [`FrameHook`] may: routing
    /// a failure into an embedder's own state — a signal, a bound JavaScript
    /// callback, a telemetry client — needs something held from before the loop
    /// started. A function pointer would force the statics this crate avoids
    /// everywhere else.
    pub fn spin_reporting(self, report: impl Fn(&WebError) + 'static) {
        let window = self.window.clone();
        self.run_loop(&window, report);
    }

    fn run_loop(self, window: &web_sys::Window, report: impl Fn(&WebError) + 'static) {
        // The idiom `requestAnimationFrame` needs and Rust does not have: a
        // closure that reschedules itself. The handle is held inside its own
        // closure through an `Rc`, which is what keeps it alive after this
        // function returns.
        let host = Rc::new(RefCell::new(self));
        let holder: FrameClosure = Rc::new(RefCell::new(None));
        let next = holder.clone();
        let owner = window.clone();

        *holder.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
            match host.borrow_mut().frame(timestamp) {
                // Rescheduled only while frames are being produced. A failure
                // stops the loop rather than repeating itself sixty times a
                // second into the console.
                Ok(()) => {
                    if let Some(closure) = next.borrow().as_ref() {
                        request_frame(&owner, closure);
                    }
                }
                Err(error) => report(&error),
            }
        }));

        if let Some(closure) = holder.borrow().as_ref() {
            request_frame(window, closure);
        }
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
        self.renderer
            .resize(width, height)
            .map_err(WebError::Renderer)?;

        let Some(build) = self.builder else {
            // A loaded document keeps its own canvas size; only the surface
            // changed.
            return Ok(());
        };
        // The renderer is about to be handed frames from an arena that has
        // never existed before, and the incoming arena's generations start
        // again — nothing in the frames themselves says so.
        self.renderer.document_replaced();
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
        if !self.live.advanced() {
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
        self.renderer
            .present(
                self.painter.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                scene.glyphs(),
                Some(changes),
            )
            .map_err(|error| WebError::Frame(error.to_string()))?;
        self.live.mark_shown();
        Ok(())
    }
}

fn request_frame(window: &web_sys::Window, closure: &Closure<dyn FnMut(f64)>) {
    let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
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
/// loop stops the loop permanently, and a page that reports that as an
/// ordinary log line disappears from a console filtered to errors and from
/// anything scraping `console.error`.
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
