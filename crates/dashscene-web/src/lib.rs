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
//!    asynchronously. What was acquired is readable as types rather than only
//!    as the line `describe` formats — `Surface::adapter_info` and
//!    `Surface::format`, added at story #835 (issue #815).
//! 2. **The `requestAnimationFrame` loop** — `Host::spin`. It **survives a lost
//!    device**, rebuilding the surface against the same canvas and carrying on
//!    (`recovery`), and it **can be stopped** — `spin` hands back a
//!    `LoopHandle` whose `Drop` ends it. Both were gaps until story #834: a
//!    recoverable context loss froze the page until reload, and a loop, once
//!    started, held the arena, the painter and the renderer until the page went
//!    away (issues #813 and #814).
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
//! - **When the loop ends.** `spin` hands back a `LoopHandle`; holding it for
//!   as long as the canvas is mounted, and dropping it when that view goes
//!   away, is the embedder's. A full-page host calls `LoopHandle::detach` and
//!   never thinks about it again.
//! - **Error reporting.** `WebError` implements `Display`; where that text
//!   goes is the embedder's decision. What it does **not** have to decide is
//!   whether a failure was survivable — the loop has already acted, and
//!   `recovery::recovery` is the same classification it used.
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
use dashscene_gpu::{FrameError, RendererError};
use dashscene_validator::Report;

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

// Outside the `wasm32` half for the reason `fetch` and `shown` are: it is the
// loop's own decision — whether a failed frame ends the loop — and a decision
// compiled only for the browser is one no `cargo test` can assert. Story #834
// exists because that decision was wrong in both integration crates and nothing
// could have caught it.
pub mod recovery;

#[cfg(target_arch = "wasm32")]
mod document;
#[cfg(target_arch = "wasm32")]
mod host;

pub use recovery::Recovery;

/// What an embedder has to name to use `Surface::adapter_info` and
/// `Surface::format`, re-exported from `dashscene-gpu` so that naming one does
/// not oblige it to declare a `wgpu` dependency and keep the version in step
/// with this crate's.
///
/// [`AdapterInfo`] and [`TextureFormat`] are the two the accessors return.
/// [`Backend`] and [`DeviceType`] are the field types a caller branches on —
/// `dashscene-gpu`'s copy of this re-export records which field type is
/// deliberately absent and why.
///
/// Not gated on `wasm32`, unlike everything below: the types exist on every
/// target even where the surface that returns them does not, and a re-export
/// that came and went with the target would be one more thing an embedder had
/// to know.
pub use dashscene_gpu::{AdapterInfo, Backend, DeviceType, TextureFormat};

/// Which root `load_document` bounds its fetch by, re-exported from `dashbuf`
/// for the reason the block above gives: naming one must not oblige an embedder
/// to declare a dependency on the format crate (story #837).
///
/// Not gated on `wasm32`, for the same reason the block above is not — and
/// `load_document` is named here in backticks rather than linked for the
/// opposite reason: it **is** gated, so an intra-doc link to it does not resolve
/// on a host build, and intra-doc links are a lint gate here.
pub use dashbuf::prefetch::ShownRoot;

/// What a load needs to measure and draw text, re-exported for the same reason
/// [`ShownRoot`] is — so an embedder can name the parameter without declaring a
/// dependency on the crate that defines it (story #863).
///
/// **Since issue #992 it can also build one.** Naming was as far as this went,
/// and that left the parameter unconstructible through the facade:
/// [`TextResources::from_faces`] resolved, and calling it did not, because its
/// argument is a `Vec` of [`FaceBytes`] and that did not cross. [`AtlasBytes`]
/// is needed by any face carrying a sheet — `FaceBytes::atlas` is an `Option`,
/// so a measure-only cascade never names it — so carrying one without the other
/// would have reached only that half.
///
/// [`TextResources::from_faces`] takes the bytes — one [`FaceBytes`] per face:
/// its font file, the family and CSS weight it stands for, its index within a
/// collection, and the committed sheet its glyphs sample as an [`AtlasBytes`].
/// It returns the cascade and the atlas list from one walk, with the font-slot
/// order enforced rather than assumed, which is why it is one call and not two
/// (story #947). Nothing bakes a sheet at run time ([`AtlasBytes`] says why), so
/// a host arrives with a committed PNG and the metrics blob beside it or its
/// text is measured and never drawn.
///
/// [`TextResources::new`] takes the pair already assembled, and is not the
/// lesser route. `corpus/showcase/src/resources.rs` is the worked example and
/// uses **both**: the two halves have different lifetimes there — a
/// [`Typesetter`] is per-scene, because it is not [`Clone`] and the solver
/// shaping with it needs it exclusively, while the atlas set is converted once
/// for the process behind a `LazyLock` — so it calls `from_faces` from two
/// places, keeps `typesetter` from one and `atlases` from the other, and pairs
/// them with `new`. Read its own doc for why those cannot be one call.
///
/// Every type that shape names is re-exported here, [`TextResourcesError`]
/// included. What is **not** carried is the vocabulary to build a [`Typesetter`]
/// or an [`Atlas`] from scratch — a cascade of faces, families and weights, or a
/// sheet with its own metrics — so an embedder assembling either from parts
/// still depends on `dashscene-typeset` and `dashpaint` directly.
///
/// Not gated on `wasm32`, for the reason the two blocks above are not.
pub use dashscene_engine::{AtlasBytes, FaceBytes, TextResources, TextResourcesError};

/// The committed sheet a staged run samples, and [`TextResources`]'s second
/// half. Re-exported for the reason [`TextResources`] gives — rustdoc lists
/// these alphabetically, so that block is not necessarily above this one.
pub use dashpaint::Atlas;

/// The cascade text is shaped and measured through, and [`TextResources`]'s
/// first half. Re-exported for the reason [`TextResources`] gives.
pub use dashscene_typeset::text::Typesetter;

#[cfg(target_arch = "wasm32")]
pub use document::load_document;
#[cfg(target_arch = "wasm32")]
pub use host::{
    FrameHook, FrameKind, Host, LoopHandle, SceneBuilder, Surface, install_panic_hook, log,
};

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
    /// The document has no root at the ordinal the embedder asked to show, so
    /// there is nothing to show and no subtree to bound the load by. Carries the
    /// url, as every other variant that can name what failed does.
    ///
    /// `roots` is how many the document does carry, which is what separates a
    /// document with no roots at all from an ordinal past the end of one that
    /// has some. `dashscene_desktop::DesktopError::NoSuchRoot` is the same
    /// variant on the other host, deliberately (story #837).
    NoSuchRoot {
        url: String,
        ordinal: u32,
        roots: u32,
    },
    /// The document does not pass the referential load gate.
    ///
    /// Carries the validator's own [`Report`] rather than a formatted string,
    /// so an embedder can count diagnostics, filter by severity, or render its
    /// own message. It was `format!("{report:?}")` until story #834; every
    /// sibling variant wraps its underlying error type, and this one stringified
    /// a type that is public and structured (issue #813).
    Gate(Report),
    /// The painter could not be built.
    Renderer(RendererError),
    /// A frame was not put on the canvas.
    ///
    /// Carries [`FrameError`] rather than a formatted string, because the loop
    /// has to branch on it: [`FrameError::is_recoverable`] says whether the
    /// remedy is to rebuild the surface and carry on. Flattening it here is what
    /// made a recoverable context loss permanent (issue #813).
    Frame(FrameError),
    /// The surface was lost again immediately after each of `attempts`
    /// rebuilds, so the loop stopped rather than rebuilding a device forever.
    ///
    /// A recovery that works is followed by a frame, and a frame resets the
    /// count — so reaching this means the device is not coming back: a removed
    /// GPU, or a driver reset that did not recover.
    Unrecoverable { attempts: u32 },
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
            Self::NoSuchRoot {
                url,
                ordinal,
                roots: 0,
            } => write!(
                f,
                "{url} carries no root node (root {ordinal} was asked for)"
            ),
            Self::NoSuchRoot {
                url,
                ordinal,
                roots,
            } => write!(
                f,
                "{url} carries {roots} root{}, and root {ordinal} was asked for",
                if *roots == 1 { "" } else { "s" }
            ),
            // Not `{report}`. `Report`'s own `Display` is one `writeln!` per
            // diagnostic, so interpolating it puts embedded newlines and a
            // trailing one inside an error message — which every sibling
            // variant here is a single line of, and which a console line or a
            // log record is expected to be. The structure is on the variant for
            // an embedder that wants it; this is the one-line rendering.
            Self::Gate(report) => {
                write!(f, "the document fails the load gate: {}", one_line(report))
            }
            Self::Renderer(error) => write!(f, "{error}"),
            Self::Frame(error) => write!(f, "the frame was not presented: {error}"),
            Self::Unrecoverable { attempts } => write!(
                f,
                "the surface was lost again after each of {attempts} rebuilds, so the loop stopped"
            ),
        }
    }
}

impl std::error::Error for WebError {}

/// A validator [`Report`] as one line.
///
/// `Report`'s `Display` writes a line per diagnostic and ends with a newline,
/// which is right for a terminal report and wrong inside an error message. Both
/// integration crates render it this way so a gate failure reads the same in a
/// browser console and in a shell (story #834).
pub(crate) fn one_line(report: &Report) -> String {
    report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::WebError;

    /// Both `NoSuchRoot` renderings, and the singular — the browser's copy of
    /// `dashscene_desktop`'s test of the same variant, deliberately, because the
    /// two messages are meant to read alike and nothing else holds them to it.
    #[test]
    fn no_such_root_names_both_numbers_and_counts_in_the_singular() {
        let rendered = |ordinal, roots| {
            WebError::NoSuchRoot {
                url: "/panel.dsb".to_owned(),
                ordinal,
                roots,
            }
            .to_string()
        };

        assert_eq!(
            rendered(1, 2),
            "/panel.dsb carries 2 roots, and root 1 was asked for"
        );
        assert_eq!(
            rendered(1, 1),
            "/panel.dsb carries 1 root, and root 1 was asked for"
        );
        assert_eq!(
            rendered(0, 0),
            "/panel.dsb carries no root node (root 0 was asked for)"
        );
    }
}
