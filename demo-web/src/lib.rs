//! The browser showcase host (v0.15, story #587; the integration half extracted
//! at story #741).
//!
//! The web counterpart of `demo/`: a canvas instead of a window, the lean
//! painter instead of a choice of two, and `requestAnimationFrame` instead of
//! winit's event loop. What it draws is the same content — the
//! `corpus/showcase` scenes, and a compiled `.dsb` — because a second set would
//! be a second set that drifts.
//!
//! # What is left here, and why that is the point
//!
//! The canvas-to-surface handoff, the frame loop, the generation gate,
//! rebuilding on resize, and the byte-range `.dsb` load are **not here any
//! more**. They are `dashscene-web`, because every browser embedder would
//! otherwise write them, and two of the five were wrong in this host's first
//! cut with no test catching either.
//!
//! What remains is the demonstration, and it is exactly what an embedder writes
//! for itself: which scene to draw and where it comes from (`source`), when the
//! showcase's scripted signal change is applied (`pulse`), and the page.
//!
//! # Why a separate crate rather than a `cfg` arm in `demo`
//!
//! `demo` depends on `skia-safe` and `softbuffer`, and neither builds for
//! `wasm32-unknown-unknown`. Sharing one crate would mean a `cfg` on every
//! dependency line to reach a build that has no Skia painter in it at all, and
//! the painter selector — the thing story #585 added to `demo` — has one option
//! on the web.
//!
//! # What is not here
//!
//! No painter selection, no input, and no scene cycling. Those are `demo`'s,
//! and this host does not reimplement them.

// Compiled on every target, and off wasm reached only by their own tests —
// which is the point rather than an accident. These two modules are what this
// host can be wrong about without a browser noticing: a query string that
// selects the wrong scene, and when a pulse falls due. Keeping them outside the
// `wasm32` half is what makes them testable by `cargo test` at all, and the
// `allow` records that the caller, not the code, is what is absent.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod pulse;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod source;

/// The id of the canvas this host draws into.
///
/// Named rather than "the first canvas on the page": a page that grew a second
/// one would otherwise start drawing into whichever came first in the document.
/// `dashscene_web::Surface::attach` takes it as a parameter, because which
/// canvas an embedder owns is the embedder's to say.
///
/// Reached only from the `wasm32` half, like the modules above, so the same
/// `allow` records that the caller rather than the constant is what is absent
/// off that target.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const CANVAS_ID: &str = "dashscene";

/// Why this host could not run.
///
/// Two shapes, deliberately. `Web` is whatever the integration reported and
/// this host cannot improve on. `UnknownScene` is this host's own: it names the
/// scene registry `dashscene-web` does not have and must not grow a variant
/// for, since a published crate cannot remove one.
#[cfg(target_arch = "wasm32")]
#[derive(Debug)]
enum DemoError {
    /// The integration surface failed.
    Web(dashscene_web::WebError),
    /// No scene carries that name; the second field is the ones that do.
    UnknownScene(String, String),
}

#[cfg(target_arch = "wasm32")]
impl std::fmt::Display for DemoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Web(error) => write!(f, "{error}"),
            Self::UnknownScene(name, known) => {
                write!(f, "no scene named {name:?}; there are {known}")
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<dashscene_web::WebError> for DemoError {
    fn from(error: dashscene_web::WebError) -> Self {
        Self::Web(error)
    }
}

#[cfg(target_arch = "wasm32")]
mod page {
    use dashlang::LiveScene;
    use dashscene_core::Arena;
    use dashscene_web::{
        FrameHook, FrameKind, Host, SceneBuilder, Surface, install_panic_hook, log,
    };
    use wasm_bindgen::prelude::*;

    use crate::source::{self, Source};
    use crate::{CANVAS_ID, DemoError, pulse};

    /// The entry point the page calls.
    ///
    /// Returns immediately. Everything that matters happens in the future
    /// spawned here, because acquiring an adapter is asynchronous on the web
    /// and cannot be waited for.
    #[wasm_bindgen(start)]
    pub fn start() {
        install_panic_hook();
        wasm_bindgen_futures::spawn_local(async {
            if let Err(error) = run().await {
                web_sys::console::error_1(&JsValue::from_str(&format!("dashscene: {error}")));
            }
        });
    }

    /// Chooses what to draw, builds it, and hands it to the integration.
    async fn run() -> Result<(), DemoError> {
        // The surface first, because a scene built in code needs the extent to
        // build *for*.
        let surface = Surface::attach(CANVAS_ID).await?;
        log(&surface.describe());
        let (width, height) = surface.extent();

        let window = web_sys::window().ok_or(dashscene_web::WebError::NoWindow)?;
        let source = source::select(&window.location().search().unwrap_or_default());

        let mut arena = Arena::new();
        let (live, builder, scripted): (LiveScene, Option<SceneBuilder>, showcase::ScenePulse) =
            match &source {
                Source::Document(url) => {
                    log(&format!("document — {url}"));
                    // A compiled document carries no signal, no binding rows
                    // and no variant table — true of every `.dsb` in the tree
                    // (issue #617) — so there is nothing for a pulse to drive.
                    // The no-op is the honest value, exactly as
                    // `demo/src/main.rs` states it.
                    //
                    // No builder either: a loaded document carries its own
                    // resolved canvas size, so a resize reconfigures the
                    // surface and leaves the picture alone — the same answer
                    // `demo/src/document.rs` gives. Rebuilding would also mean
                    // fetching again, which a frame callback cannot do.
                    (
                        // The first root: this host takes a url, not a root,
                        // so it has no second artboard to name (story #837).
                        dashscene_web::load_document(
                            url,
                            dashscene_web::ShownRoot::FIRST,
                            // The showcase's own cascade and atlases, which
                            // this host already carries for its own scenes.
                            // Without them a document containing text lays its
                            // text nodes out as empty leaves and draws no
                            // glyphs — issue #863, and the whole difference
                            // between a scene built in code and one loaded.
                            Some(dashscene_web::TextResources::new(
                                showcase::resources::new_typesetter(),
                                showcase::resources::atlases(),
                            )),
                            &mut arena,
                        )
                        .await?,
                        None,
                        no_pulse as showcase::ScenePulse,
                    )
                }
                Source::Showcase(name) => {
                    let scene = showcase::by_name(name).ok_or_else(|| {
                        DemoError::UnknownScene(
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
                        Some(scene.build),
                        scene.pulse,
                    )
                }
            };

        // Detached: this is a full-page demonstration, and the page going away
        // is the only end it has. An embedder that mounts and unmounts a canvas
        // keeps the handle instead and drops it — which is the case story #834
        // added it for, and the reason `spin` is `#[must_use]` (issue #814).
        Host::new(surface, arena, live, scripted_pulse(scripted), builder)
            .spin()
            .detach();
        Ok(())
    }

    /// The showcase's scripted signal change, on its interval.
    ///
    /// This is the demonstration's whole per-frame job, and it is what
    /// `dashscene-web` deliberately does not know about: what counts as "due"
    /// belongs to whoever owns the signals. A product host would drive its own
    /// state here instead.
    ///
    /// **It has to remember what it applied.** Writing the pulse every frame
    /// would mark its bindings dirty every frame, so `tick` would never take
    /// its idle early return and the page would never park — which is the
    /// behaviour the generation gate exists to produce. That is why
    /// `FrameHook` is a closure and not a function pointer.
    ///
    /// **And it has to apply it again after a rebuild.** A resize builds a new
    /// scene holding none of these writes, at the same elapsed time, so the
    /// change test below would decline to write anything and the picture would
    /// revert to its initial state. `FrameKind::Rebuilt` is what says so.
    fn scripted_pulse(scripted: showcase::ScenePulse) -> FrameHook {
        // Starts at 0, not `None`, so the first frame writes nothing. `due` is
        // 0 on that frame, and a scene's build already put it in its index-0
        // state, so writing it again would be redundant — and would be a
        // visible flash for any future scene whose `pulse(_, 0)` does not match
        // its own build-time default. The host this replaced held the same
        // value in a `pulses: u64` starting at 0, for the same reason.
        let mut applied: u64 = 0;
        Box::new(move |live, elapsed, kind| {
            let due = pulse::count_by(elapsed * 1000.0);
            if applied == due && kind == FrameKind::Continuing {
                return;
            }
            applied = due;
            scripted(live, due);
        })
    }

    /// A scene with nothing to script.
    fn no_pulse(_live: &mut LiveScene, _index: u64) {}
}
