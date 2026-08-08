//! Web integration for dashscene: what a browser embedder must have and would
//! otherwise write for itself (story #741).
//!
//! Five pieces, and the argument for shipping them rather than demonstrating
//! them is that **two of the five were wrong in the browser host's first cut
//! and no test caught either** — both were found by running it in a browser.
//! The loop never drove the scene's pulse, and the host never followed the
//! canvas on resize.
//!
//! 1. **The canvas-to-surface handoff** — `Surface::attach`. Finding the
//!    canvas, measuring the drawable in device pixels, and acquiring an adapter
//!    asynchronously.
//! 2. **The `requestAnimationFrame` loop** — `Host::spin`.
//! 3. **The generation gate** — which frames are worth drawing. This one is
//!    *delegated* rather than held: story #810 moved it to
//!    `dashlang::LiveScene::advanced` and `mark_shown`, so both this crate and
//!    the native host read one rule. The loop calls it; nothing here restates
//!    it.
//! 4. **Rebuilding on resize**, re-applying the scene's phase, and reporting
//!    `document_replaced` — because a new arena's generations restart and
//!    nothing in the frames themselves says so.
//! 5. **The byte-range `.dsb` load** — `load_document`, over
//!    `dashbuf::prefix`, so a document is read without pulling the whole file
//!    into linear memory first, and **bounded by the root that is shown**: the
//!    payloads that root's subtree draws are the only ones fetched, and the
//!    rest of the file is never requested. That is R5 on this target, and
//!    story #792 is where it started holding — see the `shown` module for how a browser
//!    turns out to have a region after all.
//!
//! # What an embedder still writes
//!
//! Named here rather than left as whatever did not fit, which is what epic
//! #793 asks of both integration crates.
//!
//! - **Which scene to draw, and where it comes from.** This crate takes a
//!   `LiveScene` that is already built and a `SceneBuilder` to rebuild it
//!   at a new extent. Choosing between a document URL and a scene built in
//!   code — and parsing a query string to decide — is the embedder's.
//! - **What happens each frame.** `FrameHook` is called with the seconds
//!   elapsed since the first frame, and is where an embedder writes its
//!   signals. `demo-web` uses it to run the showcase's scripted pulse; a
//!   product host would drive its own state there.
//! - **The page.** The canvas element, its CSS, and how the module is loaded.
//! - **Error reporting.** `WebError` implements `Display`; where that text
//!   goes is the embedder's decision.
//!
//! # WebGPU only
//!
//! `wgpu`'s WebGL2 backend allows **zero** storage buffers per shader stage
//! (`wgpu::Limits::downlevel_webgl2_defaults`), and this painter's whole design
//! is storage-buffer tables. There is no fallback without a second shader
//! variant expressing every table as a uniform buffer or a texture, which is a
//! redesign rather than a fallback. A browser without WebGPU is told so and
//! draws nothing.

use dashbuf::container::ContainerError;
use dashbuf::residency::PayloadMismatch;
use dashscene_gpu::RendererError;

// Compiled on every target, and off wasm reached only by their own tests —
// which is the point rather than an accident. This module is what the crate
// can be wrong about without a browser noticing: a `Content-Range` that yields
// the wrong file length. Keeping it outside the `wasm32` half is what makes it
// testable by `cargo test` at all, and the `allow` records that the caller,
// not the code, is what is absent.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod fetch;

// Compiled on every target, for the same reason `fetch` is, and it is the
// larger half of what this crate can be wrong about without a browser
// noticing: which payloads a load reads. R5 is a property of that set, and
// keeping the decision outside the `wasm32` half is what lets `cargo test`
// assert it at all (story #792).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod shown;

#[cfg(target_arch = "wasm32")]
mod document;
#[cfg(target_arch = "wasm32")]
mod host;

#[cfg(target_arch = "wasm32")]
pub use document::load_document;
#[cfg(target_arch = "wasm32")]
pub use host::{FrameHook, FrameKind, Host, SceneBuilder, Surface, install_panic_hook, log};

/// Why the integration could not run.
///
/// Every variant is something the *integration* can hit. An embedder's own
/// failures — no scene by that name, a query string it cannot parse — are the
/// embedder's to model; `demo-web` wraps this in an enum that adds its own.
/// That split is deliberate: this type is a semver commitment the moment the
/// crate is published, and a variant naming a scene registry this crate does
/// not have would be one it could never remove.
#[derive(Debug)]
pub enum WebError {
    /// There is no `window`, so this is not running in a page.
    NoWindow,
    /// The page carries no element with the id the host was given.
    NoCanvas(String),
    /// It carries one, and it is not a `<canvas>`.
    NotACanvas(String),
    /// `fetch` did not resolve to a `Response`.
    NotAResponse,
    /// The browser refused something, and this is what it said.
    Js(String),
    /// The server answered, and not with success.
    Http { url: String, status: u16 },
    /// The server honoured the range and did not say how long the file is, so
    /// there is no length to bound the envelope with.
    NoTotal(String),
    /// The file is shorter than its own section table describes.
    ShortFile,
    /// The envelope is malformed.
    Envelope(ContainerError),
    /// The envelope reader asked for a prefix it had already been given.
    EnvelopeUnreachable,
    /// The document, its manifest, or an asset's binding.
    Open(dashbuf::OpenError),
    /// A fetched payload is not the one the file names.
    Payload(PayloadMismatch),
    /// The server returned a different number of payload bytes than the ranges
    /// asked for, so the region does not hold what the payload table says it
    /// does.
    ///
    /// Detectable only because the layout is computed before anything is
    /// fetched: the host knows what it asked for (story #792).
    ShortPayloads { asked: u64, got: u64 },
    /// The file binds a **derived** payload through a derivation manifest, and
    /// this crate has no quality profile to name the rung with. Binding it as
    /// canonical would tag the bytes with whatever format the entry claims,
    /// which is the mistake issue #640 exists to prevent.
    Derived(String),
    /// The document carries no root node, so there is nothing to show and no
    /// subtree to bound the load by. Carries the url, as every other variant
    /// that can name what failed does.
    NoRoot(String),
    /// The document does not pass the referential load gate.
    Gate(String),
    /// The painter could not be built.
    Renderer(RendererError),
    /// A frame was not put on the canvas.
    Frame(String),
}

impl std::fmt::Display for WebError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWindow => write!(f, "there is no window; this is not a page"),
            Self::NoCanvas(id) => write!(f, "the page has no element with id {id:?}"),
            Self::NotACanvas(id) => write!(f, "the element with id {id:?} is not a <canvas>"),
            Self::NotAResponse => write!(f, "fetch did not resolve to a Response"),
            Self::Js(message) => write!(f, "{message}"),
            Self::Http { url, status } => write!(f, "{url} answered {status}"),
            Self::NoTotal(url) => write!(
                f,
                "{url} sent a partial response with no Content-Range total, so the \
                 file's length is unknown"
            ),
            Self::ShortFile => write!(f, "the file is shorter than its section table describes"),
            Self::Envelope(error) => write!(f, "{error}"),
            Self::EnvelopeUnreachable => {
                write!(f, "the envelope reader asked for a prefix it already had")
            }
            Self::Open(error) => write!(f, "{error}"),
            Self::Payload(error) => write!(f, "{error}"),
            Self::ShortPayloads { asked, got } => write!(
                f,
                "the payload ranges asked for {asked} bytes and {got} arrived, so the region \
                 does not hold what the payload table names"
            ),
            Self::Derived(url) => write!(
                f,
                "{url} binds a derived payload through its derivation manifest, and this crate \
                 has no quality profile to name the rung with: it can load a RAW file only \
                 (issue #640)"
            ),
            Self::NoRoot(url) => write!(f, "{url} carries no root node"),
            Self::Gate(report) => write!(f, "the document fails the load gate: {report}"),
            Self::Renderer(error) => write!(f, "{error}"),
            Self::Frame(message) => write!(f, "the frame was not presented: {message}"),
        }
    }
}

impl std::error::Error for WebError {}
