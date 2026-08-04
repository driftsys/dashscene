//! The browser half of the host: the canvas, the adapter, and the frame loop.
//!
//! Gated on `wasm32` in one place rather than item by item, so that everything
//! this crate can be wrong about *without* a browser — the query string and the
//! `Content-Range` grammar — stays compiled and tested on the host platform.

use std::cell::RefCell;
use std::rc::Rc;

use dashlang::LiveScene;
use dashpaint::Painter;
use dashscene_core::Arena;
use dashscene_gpu::{Changes, GpuPainter, SurfaceRenderer};
use showcase::{SceneBuilder, ScenePulse};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::source::{self, Source};
use crate::{CANVAS_ID, HostError, document, pulse};

/// The largest animation step one frame may take, in seconds.
///
/// A backgrounded tab is throttled to a frame a second, or stopped entirely,
/// and the frame that ends that gap must not advance a whole second of
/// animation in one step. The native host clamps for the same reason.
const MAX_FRAME_DELTA: f64 = 0.1;

/// The entry point the page calls.
///
/// Returns immediately. Everything that matters happens in the future spawned
/// here, because acquiring an adapter and a device is asynchronous on the web
/// and cannot be waited for — `crates/dashscene-gpu/src/render.rs` records why
/// blocking on it deadlocks against the event loop that would resolve it.
#[wasm_bindgen(start)]
pub fn start() {
    install_panic_hook();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = run().await {
            report(&error);
        }
    });
}

/// Sets up, then hands the page to the frame loop.
async fn run() -> Result<(), HostError> {
    let window = web_sys::window().ok_or(HostError::NoWindow)?;
    let source = source::select(&window.location().search().unwrap_or_default());

    let canvas = canvas(&window)?;
    // The drawable is measured in device pixels, which is what a painter draws
    // and what a swapchain is configured with. CSS pixels are what the page
    // lays out in, and on a display with a scale factor the two differ — a host
    // that configured the surface in CSS pixels would draw a picture the
    // browser then rescaled, which is the softness this painter exists to
    // avoid.
    let (width, height) = drawable(&window, &canvas);
    canvas.set_width(width);
    canvas.set_height(height);

    let surface_canvas = canvas.clone();
    let renderer = SurfaceRenderer::for_canvas(canvas, width, height)
        .await
        .map_err(HostError::Renderer)?;
    let info = renderer.adapter_info();
    log(&format!(
        "dashscene-gpu ({}, {:?}, {:?}) on a {width}x{height} drawable",
        info.name,
        info.backend,
        renderer.format()
    ));

    let mut arena = Arena::new();
    let (live, scripted, builder) = match &source {
        Source::Document(url) => {
            log(&format!("document — {url}"));
            // A compiled document carries no signal, no binding rows and no
            // variant table — true of every `.dsb` in the tree (issue #617) —
            // so there is nothing for a pulse to drive. The no-op is the honest
            // value, exactly as `demo/src/main.rs` states it.
            // No builder: a loaded document carries its own resolved canvas
            // size, so a resize reconfigures the surface and leaves the picture
            // alone — the same answer `demo/src/document.rs` gives. Rebuilding
            // would also mean fetching again, which a frame callback cannot do.
            (
                document::load(url, &mut arena).await?,
                no_pulse as ScenePulse,
                None,
            )
        }
        Source::Showcase(name) => {
            let scene = showcase::by_name(name).ok_or_else(|| {
                HostError::UnknownScene(
                    name.clone(),
                    showcase::SCENES
                        .iter()
                        .map(|scene| scene.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            })?;
            log(&format!("scene {} — {}", scene.name, scene.summary));
            (
                (scene.build)(&mut arena, width, height),
                scene.pulse,
                Some(scene.build),
            )
        }
    };

    Host {
        arena,
        live,
        renderer,
        painter: GpuPainter::new(),
        shown: None,
        previous: None,
        scripted,
        started: None,
        pulses: 0,
        builder,
        canvas: surface_canvas,
        extent: (width, height),
        window: window.clone(),
    }
    .spin(&window);
    Ok(())
}

/// The frame loop's state.
struct Host {
    arena: Arena,
    live: LiveScene,
    renderer: SurfaceRenderer,
    painter: GpuPainter,
    /// The generation last put on the canvas. [`None`] until one has been.
    shown: Option<u64>,
    /// The previous frame's timestamp, for the animation delta.
    previous: Option<f64>,
    /// The scene's scripted signal change, applied on an interval.
    scripted: ScenePulse,
    /// The first frame's timestamp, which the pulse phase is measured from.
    started: Option<f64>,
    /// How many pulses have been applied.
    pulses: u64,
    /// How to rebuild the scene for a new extent, or [`None`] for a loaded
    /// document, which carries its own.
    builder: Option<SceneBuilder>,
    canvas: HtmlCanvasElement,
    window: web_sys::Window,
    /// The drawable the surface is currently configured for, in device pixels.
    extent: (u32, u32),
}

/// A scene with nothing to script.
fn no_pulse(_live: &mut LiveScene, _index: u64) {}

impl Host {
    /// Schedules the loop and returns; the browser drives it from here.
    fn spin(self, window: &web_sys::Window) {
        // The idiom `requestAnimationFrame` needs and Rust does not have: a
        // closure that reschedules itself. The handle is held inside its own
        // closure through an `Rc`, which is what keeps it alive after this
        // function returns.
        let host = Rc::new(RefCell::new(self));
        let holder: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
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

    /// One frame: advance time, and draw if anything moved.
    ///
    /// The rule the native host follows (`demo/src/shell.rs`): the generation
    /// says whether a frame is worth drawing.
    ///
    /// `shown` is recorded when `present` returns, which is **not** the same as
    /// the frame having been drawn — a zero extent, an occluded window or an
    /// acquire that timed out all return `Ok(())` without drawing. A host
    /// cannot tell, and does not try to: `Changes` in
    /// `crates/dashscene-gpu/src/render.rs` records that the generation travels
    /// with the dirty set precisely so the renderer catches a broken chain
    /// itself, applying ranges only when this frame is the immediate successor
    /// of the one on the device. Second-guessing it here would be a second
    /// mechanism for one job, and the weaker of the two.
    /// Reconfigures for the canvas's current size, when it has changed.
    ///
    /// Rebuilds the scene at the new extent, as the native host does: a
    /// showcase scene derives every offset from the drawable it is given, so
    /// the new arena is the picture for the new size. The pulse phase is
    /// re-applied so a resize resumes the script rather than snapping every
    /// signal back to its initial value.
    fn resize_if_needed(&mut self) -> Result<(), HostError> {
        let (width, height) = drawable(&self.window, &self.canvas);
        if (width, height) == self.extent {
            return Ok(());
        }
        self.extent = (width, height);
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.renderer
            .resize(width, height)
            .map_err(HostError::Renderer)?;

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
        (self.scripted)(&mut self.live, self.pulses);
        // The generation the old arena reached names nothing in this one.
        self.shown = None;
        Ok(())
    }

    fn frame(&mut self, timestamp: f64) -> Result<(), HostError> {
        let dt = match self.previous {
            // Milliseconds, from a clock that may be coarsened for privacy but
            // does not run backwards. The `max(0.0)` costs nothing and makes
            // that an assumption this host does not depend on.
            Some(previous) => ((timestamp - previous).max(0.0) / 1000.0).min(MAX_FRAME_DELTA),
            None => 0.0,
        };
        self.previous = Some(timestamp);

        // A canvas sized in CSS has no event of its own, and this host draws
        // every frame anyway, so the extent is re-measured here rather than
        // through a listener. Without it the backing store keeps its first size
        // while the element stretches, and the browser rescales a fixed picture
        // — which turns a circle into an ellipse and is exactly how this was
        // found.
        if let Err(error) = self.resize_if_needed() {
            return Err(error);
        }

        // The scene's own script, on its interval. Without this nothing ever
        // writes a signal, `tick` takes its idle early return after the first
        // commit, and the page draws one frame and parks — which looks exactly
        // like a working host with nothing to animate.
        let started = *self.started.get_or_insert(timestamp);
        let due = pulse::count_by(timestamp - started);
        if due != self.pulses {
            self.pulses = due;
            (self.scripted)(&mut self.live, due);
        }

        let generation = self.live.tick(dt as f32, &mut self.arena);
        if self.shown == Some(generation) {
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
            .map_err(|error| HostError::Frame(error.to_string()))?;
        self.shown = Some(generation);
        Ok(())
    }
}

fn request_frame(window: &web_sys::Window, closure: &Closure<dyn FnMut(f64)>) {
    let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
}

/// The canvas named by [`CANVAS_ID`].
fn canvas(window: &web_sys::Window) -> Result<HtmlCanvasElement, HostError> {
    window
        .document()
        .ok_or(HostError::NoWindow)?
        .get_element_by_id(CANVAS_ID)
        .ok_or(HostError::NoCanvas)?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| HostError::NotACanvas)
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
pub(crate) fn log(message: &str) {
    web_sys::console::log_1(&JsValue::from_str(&format!("dashscene: {message}")));
}

fn report(error: &HostError) {
    web_sys::console::error_1(&JsValue::from_str(&format!("dashscene: {error}")));
}

/// Sends a panic to the console instead of leaving it an unreachable trap.
///
/// Written out rather than taken from `console_error_panic_hook`, which is this
/// much behaviour and a dependency. Without it a panic in wasm surfaces as
/// "unreachable executed" with no message and no location, which is the least
/// useful failure a browser can show.
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&format!("dashscene panicked: {info}")));
    }));
}
