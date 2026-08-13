//! Desktop integration for dashscene: what a windowed embedder must have and
//! would otherwise write for itself (story #794).
//!
//! The desktop counterpart of `dashscene-web`, and the same five pieces in
//! `winit` terms rather than canvas terms:
//!
//! 1. **The window-to-surface handoff** — [`GpuPresenter::new`], over
//!    `dashscene_gpu::SurfaceRenderer`. Blocking, where the browser's is
//!    asynchronous. What was acquired is readable as types rather than only as
//!    the line [`Present::name`] carries — [`GpuPresenter::adapter_info`] and
//!    [`GpuPresenter::format`] for an embedder holding the presenter, and
//!    [`Attached::adapter`] for one that took the default and holds a
//!    `Box<dyn Present>`. Story #835 (issue #819) added the first pair;
//!    issue #902 added the second route, without which the ordinary embedder
//!    had the string and nothing else.
//! 2. **The frame loop** — [`run`], on `winit`'s event loop, pacing itself at
//!    60 Hz while the generation advances and parking on an event wait while it
//!    is steady. It **survives a lost surface**, rebinding the presenter and
//!    carrying on ([`recovery`]), and it **can be stopped from another thread**
//!    ([`Waker::stop`]). Both were gaps until story #834: a recoverable loss
//!    called `event_loop.exit()` and ended the process, and an embedder that did
//!    not own the window's lifetime had no way to end the loop at all (issues
//!    #818 and #820).
//! 3. **The generation gate** — which frames are worth drawing. *Delegated*
//!    rather than held: story #810 moved it to `dashlang::LiveScene::advanced`
//!    and `mark_shown`, so this crate and the browser one read one rule. The
//!    loop calls it; nothing here restates it.
//! 4. **Rebuilding on resize**, and reporting [`Present::document_replaced`] —
//!    because a new arena's generations restart and nothing in the frames
//!    themselves says so.
//! 5. **The `.dsb` load** — [`Document`] for a file, which is **mapped** and
//!    bounded by the root that is shown, and [`load_bytes`] for a document
//!    already in memory.
//!
//! # What an embedder still writes
//!
//! Named here rather than left as whatever did not fit, which is what epic
//! #793 asks of both integration crates.
//!
//! - **[`App::build`] — what to draw.** This crate has no scene registry and
//!   no opinion about content. An embedder builds a scene into the arena it is
//!   handed, or loads a document into it, and returns the `LiveScene` the loop
//!   ticks.
//! - **Input.** Every window event the loop does not own — which is every
//!   input event — arrives at [`App::event`] untouched, along with the scene
//!   and the drawable extent. Mapping a pointer position onto a signal, or a
//!   key onto a variant switch, is the embedder's: this crate holds no signal
//!   name, no node name and no variant set.
//! - **Anything driven from off the loop's thread.** [`App::started`] hands
//!   over a [`Waker`], because a parked loop cannot otherwise be reached. What
//!   sends it — a timer, a scripted sequence, a data feed — is the embedder's,
//!   and so is deciding when to end the loop with [`Waker::stop`]. What stays
//!   `winit`'s and cannot be handed over: [`run`] owns the calling thread until
//!   the loop ends, so the stop is a message rather than a returned handle.
//! - **Where the diagnostics go.** [`App::note`] receives the loop's own lines;
//!   a library that wrote them to stderr itself would be deciding an embedder's
//!   output format. Default: discarded.
//! - **Which painter.** [`App::presenter`] defaults to the lean painter, and
//!   overriding it is how a second one is selected at run time — see
//!   [`present`] for why only one implementation ships here.
//! - **Error reporting.** [`DesktopError`] implements `Display`; where that
//!   text goes is the embedder's decision. What it does **not** have to decide
//!   is whether a failure was survivable — the loop has already acted, and
//!   [`recovery::recovery`] is the same classification it used.

use std::io;

use dashbuf::residency::PayloadMismatch;
use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_engine::TaffySolver;
use dashscene_validator::Report;

mod document;
mod host;
pub mod present;
pub mod recovery;

pub use document::Document;
pub use host::{App, Attached, FRAME_INTERVAL, Reaction, Scene, Waker, run};
pub use present::{
    AdapterDetails, AdapterInfo, Backend, DeviceType, Drawn, GpuPresenter, Present, PresentError,
    TextureFormat,
};
pub use recovery::Recovery;

/// Which root [`Document::load`] bounds its read by, re-exported from `dashbuf`
/// so that naming one does not oblige an embedder to declare a dependency on
/// the format crate and keep its version in step with this one's — the same
/// reason the `dashscene-gpu` types above are re-exported (story #837).
pub use dashbuf::prefetch::ShownRoot;

/// What a load needs to measure and draw text, re-exported for the same reason
/// [`ShownRoot`] is: an embedder should be able to *name* the parameter without
/// declaring a dependency on the crate that defines it.
///
/// Naming is as far as the re-export goes. **Building** a [`Typesetter`] means
/// assembling a cascade — faces, families and weights — and an [`Atlas`] is a
/// committed sheet with its own metrics, so an embedder doing real text work
/// depends on `dashscene-typeset` and `dashpaint` directly.
/// `corpus/showcase/src/resources.rs` is the worked example, and it is about
/// eighty lines.
pub use dashpaint::Atlas;
pub use dashscene_engine::TextResources;
pub use dashscene_typeset::text::Typesetter;

/// Replays a document already in memory, through the **owning** load path.
///
/// [`Document`] is the path to prefer for a file: it maps, and it reads only
/// the payloads the shown root draws, so the cost of opening tracks the root
/// being drawn rather than the file's size (R5). This one cannot be bounded
/// that way even in principle — `dashscene_core::load_document` copies every
/// payload into an owned `ImageAsset`, so it needs bytes for every asset entry
/// whether or not anything draws them.
///
/// It is here for the case that has no file to map: a document compiled into
/// the binary, or one that arrived over a channel that yielded bytes rather
/// than a path. An embedder holding a path should not use it.
pub fn load_bytes(
    bytes: &[u8],
    text: Option<TextResources>,
    arena: &mut Arena,
) -> Result<LiveScene, DesktopError> {
    let (document, payloads) = dashbuf::open_verified(bytes).map_err(DesktopError::Open)?;
    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        return Err(DesktopError::Gate {
            path: "<in memory>".to_owned(),
            report,
        });
    }
    dashscene_core::load_document(&document, &payloads, arena);
    Ok(dashlang::attach_live(arena, TaffySolver::boxed(text)))
}

/// Why the integration could not run.
///
/// Every variant is something the *integration* can hit. An embedder's own
/// failures — no scene by that name, a command line it cannot parse — are the
/// embedder's to model. That split is deliberate: this type is a semver
/// commitment the moment the crate is published, and a variant naming a scene
/// registry this crate does not have would be one it could never remove.
///
/// **Modelling them does not have to mean wrapping this type.** `demo-web`
/// wraps `WebError` in a `DemoError` of its own; `demo` returns this type
/// unwrapped from `shell::run` and reports its own failures into an exit code
/// before the loop starts, so they never reach an error type at all. Both
/// honour the split.
#[derive(Debug)]
pub enum DesktopError {
    /// The file could not be mapped, and this is the path that was tried.
    Map { path: String, error: io::Error },
    /// The envelope, the document, or an asset's binding.
    Open(dashbuf::OpenError),
    /// The document does not pass the referential load gate.
    ///
    /// Carries the validator's own [`Report`] rather than a formatted string, so
    /// an embedder can count diagnostics, filter by severity, or render its own
    /// message. It was `format!("{report:?}")` until story #834; every sibling
    /// variant wraps its underlying error type, and this one stringified a type
    /// that is public and structured (issue #818).
    Gate { path: String, report: Report },
    /// A payload is not the one the file names.
    Payload(PayloadMismatch),
    /// The file binds a **derived** payload through a derivation manifest, and
    /// this crate has no quality profile to name the rung with. Binding it as
    /// canonical would tag a KTX2 as whatever format the entry claims, which is
    /// the mistake issue #640 exists to prevent.
    Derived { path: String },
    /// The document has no root at the ordinal the embedder asked to show, so
    /// there is nothing to show and no subtree to bound the load by.
    ///
    /// `roots` is how many the document does carry, which is what separates the
    /// two ways this happens — a document with no roots at all, and an ordinal
    /// past the end of one that has some. One variant rather than two, because
    /// an embedder's recovery is the same either way and the count is what tells
    /// it which mistake it made (story #837).
    NoSuchRoot {
        path: String,
        ordinal: u32,
        roots: u32,
    },
    /// A frame was not put on the window.
    Present(PresentError),
    /// The window could not be created.
    Window(String),
    /// The event loop could not be built, or ended with a failure.
    EventLoop(String),
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Map { path, error } => write!(f, "{path} cannot be mapped: {error}"),
            Self::Open(error) => write!(f, "{error}"),
            // Not `{report}`. `Report`'s own `Display` is one `writeln!` per
            // diagnostic, so interpolating it puts embedded newlines and a
            // trailing one inside an error message — which every sibling
            // variant here is a single line of. The structure is on the variant
            // for an embedder that wants it; this is the one-line rendering,
            // and it matches what `dashscene-web` prints for the same failure.
            Self::Gate { path, report } => {
                write!(f, "{path} fails the load gate: {}", one_line(report))
            }
            Self::Payload(error) => write!(f, "{error}"),
            Self::Derived { path } => write!(
                f,
                "{path} binds a derived payload through its derivation manifest, and this crate \
                 has no quality profile to name the rung with: it can map a RAW file only \
                 (issue #640)"
            ),
            Self::NoSuchRoot {
                path,
                ordinal,
                roots: 0,
            } => write!(
                f,
                "{path} carries no root node (root {ordinal} was asked for)"
            ),
            Self::NoSuchRoot {
                path,
                ordinal,
                roots,
            } => write!(
                f,
                "{path} carries {roots} root{}, and root {ordinal} was asked for",
                if *roots == 1 { "" } else { "s" }
            ),
            Self::Present(error) => write!(f, "{error}"),
            Self::Window(message) => write!(f, "the window could not be created: {message}"),
            Self::EventLoop(message) => write!(f, "the event loop: {message}"),
        }
    }
}

/// A validator [`Report`] as one line.
///
/// `Report`'s `Display` writes a line per diagnostic and ends with a newline,
/// which is right for a terminal report and wrong inside an error message. Both
/// integration crates render it this way so a gate failure reads the same in a
/// shell and in a browser console (story #834).
fn one_line(report: &Report) -> String {
    report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

impl std::error::Error for DesktopError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Map { error, .. } => Some(error),
            Self::Open(error) => Some(error),
            Self::Payload(error) => Some(error),
            Self::Present(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopError;

    /// Both `NoSuchRoot` renderings, and the singular.
    ///
    /// Rendered rather than matched. `document::tests`'s
    /// `an_ordinal_past_the_last_root_is_refused_by_name` asserts the variant
    /// with `matches!`, which never formats it — so transposing `{ordinal}` and
    /// `{roots}` in either arm would pass every other test in this crate while
    /// publishing a message that names the two numbers the wrong way round. The
    /// count is the whole reason D6 of
    /// `docs/decisions/the-shown-root-is-named-by-ordinal.md` gives for one
    /// variant instead of two, so it is worth a test that reads it.
    ///
    /// The zero-root arm is reachable from nothing else here: the load gate
    /// refuses a document with no nodes, so only a constructed value renders it.
    #[test]
    fn no_such_root_names_both_numbers_and_counts_in_the_singular() {
        let rendered = |ordinal, roots| {
            DesktopError::NoSuchRoot {
                path: "panel.dsb".to_owned(),
                ordinal,
                roots,
            }
            .to_string()
        };

        assert_eq!(
            rendered(1, 2),
            "panel.dsb carries 2 roots, and root 1 was asked for"
        );
        assert_eq!(
            rendered(1, 1),
            "panel.dsb carries 1 root, and root 1 was asked for",
            "every committed goldens/dsb fixture has exactly one root, so this is the commonest \
             way to reach this error and it must not read `1 roots`"
        );
        assert_eq!(
            rendered(0, 0),
            "panel.dsb carries no root node (root 0 was asked for)"
        );
    }
}
