//! The smallest browser embedder that draws a `.dsb` (issue #776, story #795).
//!
//! **This exists to be weighed, not to be run.** The payload budget issue #776
//! opens with a measured 1.37 MB, and that number is `demo_web.wasm` — a host
//! that reaches `showcase` and through it `dashc`, the whole compiler. An
//! embedder loading a document that was compiled somewhere else links none of
//! that, so the figure overstates the runtime by an unknown margin, and the
//! 789 KB `dashc_wasm` build is not subtractable from it because it is a
//! differently-linked artifact.
//!
//! A library crate cannot answer the question either: dead-code elimination
//! happens at link time, so `dashscene-web` on its own has no size. Only a
//! linked artifact does. Hence this crate — one `cdylib`, three dashscene
//! dependencies, and the shortest code that reaches a drawn frame.
//!
//! # It is also a claim being checked
//!
//! `dashscene-web` documents **four** things an embedder still supplies: which
//! scene to draw and where it comes from, what happens each frame, the page, and
//! error reporting. `run` below is the smallest form of the first two, plus the
//! fourth in its least useful form.
//!
//! It is deliberately **less** than that list, and the gaps are the measurement
//! rather than an oversight:
//!
//! - **No `SceneBuilder`.** `Host::new` takes [`None`], which is the answer
//!   `demo-web` also gives for a loaded document: a `.dsb` carries its own
//!   resolved canvas size, so a resize reconfigures the surface and leaves the
//!   picture alone, and rebuilding would mean fetching again, which a frame
//!   callback cannot do.
//! - **No page.** This crate is a `.wasm` and nothing else; the HTML that loads
//!   it is not part of what an embedder downloads.
//! - **No run-time choice of document**, and no graceful handling of a missing
//!   canvas. Both are an embedder's, and adding them here would weigh them
//!   rather than the runtime.
//!
//! So the figure is a **floor**: the least an embedder can link and still draw.

//! # Gated on `wasm32`, like the host it mirrors
//!
//! `dashscene-web`'s API is `wasm32`-only — `Surface`, `Host` and
//! `load_document` do not exist off that target — so off wasm this crate is
//! empty. `demo-web` gates its own browser half the same way. There is nothing
//! here to test on the host platform, which is the point: everything this crate
//! contains is what the linker keeps.

#[cfg(target_arch = "wasm32")]
mod embedder {
    use dashlang::LiveScene;
    use dashscene_core::Arena;
    use dashscene_web::{
        FrameKind, Host, ShownRoot, Surface, WebError, install_panic_hook, load_document, log,
    };
    use wasm_bindgen::prelude::*;

    /// The document this draws. A fixed path, because choosing one is the
    /// embedder's job and parsing a query string would be measuring that choice.
    const DOCUMENT: &str = "scene.dsb";

    /// The canvas this draws into.
    const CANVAS: &str = "dashscene";

    #[wasm_bindgen(start)]
    pub fn start() {
        install_panic_hook();
        wasm_bindgen_futures::spawn_local(async {
            if let Err(error) = run().await {
                report(&error);
            }
        });
    }

    /// Attach, load, spin. The whole of what a minimal embedder writes.
    async fn run() -> Result<(), WebError> {
        let surface = Surface::attach(CANVAS).await?;
        let mut arena = Arena::new();
        let live = load_document(DOCUMENT, ShownRoot::FIRST, &mut arena).await?;
        // Detached: this page has one canvas for its whole life, so there is
        // nothing to stop the loop for. `detach` is the call that says so —
        // dropping the handle would stop the loop instead (story #834), and a
        // minimal embedder is exactly where that mistake would be silent.
        Host::new(surface, arena, live, Box::new(no_frame_hook), None)
            .spin()
            .detach();
        Ok(())
    }

    /// A document carries no signal, no binding row and no variant table — true of
    /// every `.dsb` in the tree (issue #617) — so there is nothing for a per-frame
    /// hook to drive. The honest value is a hook that writes nothing, which is also
    /// what keeps the idle skip working: writing a signal every frame would mark its
    /// binding dirty and the page would never park.
    fn no_frame_hook(_live: &mut LiveScene, _elapsed: f64, _kind: FrameKind) {}

    /// Where a failure goes. `dashscene-web` names error reporting as the
    /// embedder's, and this is the least an embedder can do with it: the
    /// crate's own console writer, which costs no further dependency.
    ///
    /// An earlier version built a `JsValue` and dropped it — reporting nothing,
    /// while this comment said otherwise.
    fn report(error: &WebError) {
        log(&format!("{error}"));
    }
}
