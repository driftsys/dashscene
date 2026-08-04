//! The browser showcase host (v0.15, story #587).
//!
//! The web counterpart of `demo/`: a canvas instead of a window, the lean
//! painter instead of a choice of two, and `requestAnimationFrame` instead of
//! winit's event loop. What it draws is the same content — the
//! `corpus/showcase` scenes, and a compiled `.dsb` — because a second set would
//! be a second set that drifts.
//!
//! # Why a separate crate rather than a `cfg` arm in `demo`
//!
//! `demo` depends on `skia-safe` and `softbuffer`, and neither builds for
//! `wasm32-unknown-unknown`. Sharing one crate would mean a `cfg` on every
//! dependency line to reach a build that has no Skia painter in it at all, and
//! the painter selector — the thing story #585 added to `demo` — has one option
//! on the web. What is left is small enough that sharing it would cost more
//! than it saved.
//!
//! # What is not here
//!
//! No painter selection, no input, and no scene cycling. Those are `demo`'s,
//! and this host does not reimplement them.
//!
//! # WebGPU only
//!
//! `wgpu`'s WebGL2 backend allows **zero** storage buffers per shader stage
//! (`wgpu::Limits::downlevel_webgl2_defaults`), and this painter's whole design
//! is storage-buffer tables. There is no fallback to build without a second
//! shader variant expressing every table as a uniform buffer or a texture,
//! which is a redesign rather than a fallback. A browser without WebGPU is told
//! so and draws nothing.

// Compiled on every target, and off wasm reached only by their own tests —
// which is the point rather than an accident. These two modules are what this
// crate can be wrong about without a browser noticing: a query string that
// selects the wrong scene, and a `Content-Range` that yields the wrong file
// length. Keeping them outside the `wasm32` half is what makes them testable by
// `cargo test` at all, and the `allow` records that the caller, not the code, is
// what is absent.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod fetch;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod pulse;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
mod source;

#[cfg(target_arch = "wasm32")]
mod document;
#[cfg(target_arch = "wasm32")]
mod host;

use dashbuf::container::ContainerError;
use dashbuf::prefix::BindError;
use dashscene_gpu::RendererError;

/// The id of the canvas this host draws into.
///
/// Named rather than "the first canvas on the page": a page that grew a second
/// one would otherwise start drawing into whichever came first in the document.
const CANVAS_ID: &str = "dashscene";

/// Why the host could not run.
#[derive(Debug)]
pub enum HostError {
    /// There is no `window`, so this is not running in a page.
    NoWindow,
    /// The page carries no element with the expected id.
    NoCanvas,
    /// It carries one, and it is not a `<canvas>`.
    NotACanvas,
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
    Bind(BindError),
    /// The document does not pass the referential load gate.
    Gate(String),
    /// No scene carries that name; the second field is the ones that do.
    UnknownScene(String, String),
    /// The painter could not be built.
    Renderer(RendererError),
    /// A frame was not put on the canvas.
    Frame(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWindow => write!(f, "there is no window; this is not a page"),
            Self::NoCanvas => write!(f, "the page has no element with id {CANVAS_ID:?}"),
            Self::NotACanvas => write!(f, "the element with id {CANVAS_ID:?} is not a <canvas>"),
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
            Self::Bind(error) => write!(f, "{error}"),
            Self::Gate(report) => write!(f, "the document fails the load gate: {report}"),
            Self::UnknownScene(name, known) => {
                write!(f, "no scene named {name:?}; there are {known}")
            }
            Self::Renderer(error) => write!(f, "{error}"),
            Self::Frame(message) => write!(f, "the frame was not presented: {message}"),
        }
    }
}
