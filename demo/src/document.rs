//! Which document this demonstration draws, when it is asked for one (story
//! #575).
//!
//! **The loading is not here.** `dashscene_desktop::Document` maps a file and
//! reads only the payloads the shown root draws, and
//! `dashscene_desktop::load_bytes` replays one already in memory; both were
//! extracted at story #794, because every windowed embedder that opens a `.dsb`
//! would otherwise write them. What is left is the demonstration's own: a
//! command-line flag, and a golden compiled into the binary so the host has
//! something to draw without a path.
//!
//! [`shell::SceneBuilder`]: crate::shell::SceneBuilder
//! [`shell::ScenePulse`]: crate::shell::ScenePulse

use std::path::PathBuf;
use std::sync::OnceLock;

use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_desktop::{DesktopError, Document};

/// `goldens/dsb/v03-paint.dsb`: `dashc::compile_figma`'s output for a real
/// Figma capture, `corpus/figma-fixtures/v03-paint.json` — a 960x680 paint
/// vocabulary swatch board (solid, gradient, and image fills, a stroke,
/// corner radii) that two CI suites already pin byte-for-byte
/// (`goldens/dsb/README.md`).
///
/// Picked over the other nine `goldens/dsb` fixtures because it is the
/// richest compiled document in the tree (14 nodes, 14 paint entries) and
/// one of only **two** carrying a real embedded image through the v0.11
/// asset table, so loading it exercises the payload-binding step that the
/// other eight never reach. The second is `v03-paint-hifi.dsb`, this
/// document's HiFi derivation, which carries the same picture as a derived
/// payload behind a manifest section — so it is the same document rather
/// than a second one to choose from.
///
/// `goldens/images/v03-paint.png` is **not** a picture of this document,
/// despite the name: `goldens/tooling/tests/v03_families.rs` names it a
/// hand-built boundary-B golden at 96x96, decoupled from any producer,
/// while this document's own root resolves to 960x680
/// (`crates/dashc/tests/figma_lowering.rs::the_fixture_compiles_loads_and_renders`
/// pins its 14-rect shape and that it rasterizes, with no pixel golden).
/// No committed `goldens/dsb` fixture has a wired, pixel-compared
/// end-to-end picture today (issue #616).
///
/// This is the committed golden byte for byte — not recompiled, not
/// modified — read at compile time so the host carries no runtime path
/// dependency on the working tree.
const DOCUMENT: &[u8] = include_bytes!("../../goldens/dsb/v03-paint.dsb");

/// The mapped file this demonstration was pointed at.
///
/// A `OnceLock` rather than a field on the scene entry because
/// [`crate::shell::SceneEntry`] holds `build` as a plain function pointer: the
/// registry lists scenes that need no configuration, and one that does cannot
/// carry it through that seam without widening it for every other scene. Set
/// once, in `main`, before the loop starts.
///
/// It is this demonstration's static and not the integration crate's:
/// `dashscene_desktop::Document` is an ordinary value an embedder holds
/// wherever it likes, and the arena keeps the mapping alive by itself.
static MAPPED: OnceLock<Document> = OnceLock::new();

/// What `--dsb` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `--dsb` was not given: this is showing showcase scenes.
    NotAsked,
    /// `--dsb` with no path — [`DOCUMENT`], embedded at compile time.
    Embedded,
    /// `--dsb <path>` — that file, mapped.
    Mapped(PathBuf),
}

/// Takes `--dsb` and its optional path off the front of `arguments`.
///
/// The flag is read in first position only, which is where it has always been
/// read: [`crate::painter::Choice::take`] has already removed `--painter` and
/// its value from anywhere in the list, so a document run is `--dsb` first
/// whatever order the two flags were typed in.
///
/// The path is optional, and absent is not an error. The embedded golden is
/// what this has drawn since story #575, and it exists so the host carries no
/// runtime path dependency on the working tree; taking that away to add a
/// mapping would trade one capability for another rather than adding one.
pub fn take(arguments: &mut Vec<String>) -> Source {
    if arguments.first().map(String::as_str) != Some("--dsb") {
        return Source::NotAsked;
    }
    arguments.remove(0);
    if arguments.is_empty() {
        Source::Embedded
    } else {
        Source::Mapped(PathBuf::from(arguments.remove(0)))
    }
}

/// Maps `path` and points [`scene`] at it for the rest of the process.
///
/// Mapped here, in `main`, rather than lazily inside [`scene`]: a path the host
/// cannot reach is the one failure a person can cause from the command line,
/// and this is the last place that can report it and exit rather than panic out
/// of the frame loop.
///
/// # Panics
///
/// If called twice. One run draws one document, and a second call would mean
/// two answers to which.
pub fn map_file(path: PathBuf) -> Result<(), DesktopError> {
    let document = Document::map(path)?;
    if MAPPED.set(document).is_err() {
        panic!("the document source is chosen once");
    }
    Ok(())
}

/// The fonts and atlases a loaded document is measured and drawn with.
///
/// **This is the whole of issue #863's fix on this host.** Every `.dsb` load
/// path built `TaffySolver::new()` — no typesetter, no atlases — so a document
/// containing text laid its text nodes out as empty leaves and drew no glyphs
/// at all, while this same demonstration drew text correctly for its own
/// scenes. The difference was never the document: it was that a scene built in
/// code brings its own solver, and a loaded one had nothing to bring.
///
/// The resources are the showcase's, unchanged and already assembled — the same
/// cascade and the same committed atlases its scenes draw with. That is the
/// point: this host always had them.
fn text() -> dashscene_desktop::TextResources {
    dashscene_desktop::TextResources::new(
        showcase::resources::new_typesetter(),
        showcase::resources::atlases(),
    )
}

/// Loads the selected document into `arena` and attaches a [`LiveScene`] to it.
///
/// `width` and `height` go unused: a loaded document already carries its own
/// resolved canvas size (P1 — the document is intent, and this document's
/// intent already includes concrete geometry from the Figma capture it was
/// compiled from), unlike a scene built in code, which derives every offset
/// from the drawable it is given. A resize therefore reloads the same picture
/// rather than rescaling it.
///
/// # Two load paths, and only one of them is bounded by what is shown
///
/// The embedded golden is a `&'static [u8]` with nothing to map, so it takes
/// the owning path. That path cannot be bounded by what is shown even in
/// principle: `load_document` copies every payload into an owned `ImageAsset`,
/// so every entry needs bytes.
///
/// A mapped file is bounded by the shown root, which is R5 — see
/// `dashscene_desktop::Document`, which holds both and states the difference.
///
/// # Panics
///
/// If the document fails to open or fails the load gate. This is a
/// `SceneBuilder`, which returns a `LiveScene` and has nowhere to report to;
/// `main` has already mapped the file, so the failure a person can cause from
/// the command line has been reported and exited before this runs. For the
/// embedded golden it never should, since it is frozen and pinned elsewhere.
pub fn scene(arena: &mut Arena, _width: u32, _height: u32) -> LiveScene {
    let loaded = match MAPPED.get() {
        // The first root, which is what this host showed before there was a
        // way to say otherwise. The showcase's own scenes are single-root and
        // `--dsb` takes a path rather than a root, so nothing here has a
        // second artboard to name yet (story #837).
        Some(mapped) => mapped.load(dashscene_desktop::ShownRoot::FIRST, Some(text()), arena),
        None => dashscene_desktop::load_bytes(DOCUMENT, Some(text()), arena),
    };
    loaded.unwrap_or_else(|error| panic!("the document does not load: {error}"))
}

/// The name the selected document is reported by.
pub fn name() -> String {
    MAPPED.get().map_or_else(
        || "goldens/dsb/v03-paint.dsb (embedded)".to_owned(),
        |mapped| mapped.path().display().to_string(),
    )
}

/// No-op: [`DOCUMENT`] carries no `dashcue` signal or binding rows, and no
/// variant table either — inspected while building story #575, and true of
/// every one of the ten committed `goldens/dsb` fixtures today (issue #617) —
/// so there is nothing for a pulse to drive.
///
/// The frame loop still ticks the attached scene every frame regardless: with
/// no live binding and no scheduler track, `LiveScene::tick` takes its
/// idle-frame early return after the first present, and the loop settles and
/// stops painting. That is story #575's other finding — the idle skip holds for
/// a loaded document exactly as it does for a scene built in code.
pub fn pulse(_live: &mut LiveScene, _index: u64) {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Source, take};

    fn arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|argument| (*argument).to_owned()).collect()
    }

    /// No flag, nothing taken. The list must reach scene selection exactly as
    /// it arrived, or a scene name would go missing.
    #[test]
    fn a_list_without_the_flag_is_left_alone() {
        let mut list = arguments(&["typography", "--all"]);
        assert_eq!(take(&mut list), Source::NotAsked);
        assert_eq!(list, arguments(&["typography", "--all"]));
    }

    /// The flag alone selects the embedded golden, and is removed.
    #[test]
    fn the_flag_with_no_path_selects_the_embedded_golden() {
        let mut list = arguments(&["--dsb"]);
        assert_eq!(take(&mut list), Source::Embedded);
        assert!(list.is_empty(), "the flag is taken out of the list");
    }

    /// The flag with a path selects that file, and both tokens are removed.
    ///
    /// The path is asserted by value rather than by count: a `take` that
    /// removed the right number of tokens and returned the wrong one would pass
    /// a count.
    #[test]
    fn the_flag_with_a_path_selects_that_file() {
        let mut list = arguments(&["--dsb", "corpus/showcase/typography.dsb", "--all"]);
        assert_eq!(
            take(&mut list),
            Source::Mapped(PathBuf::from("corpus/showcase/typography.dsb"))
        );
        assert_eq!(
            list,
            arguments(&["--all"]),
            "the flag and its path are both taken, and nothing else is"
        );
    }

    /// The flag is read in first position only. Elsewhere it is left for scene
    /// selection to refuse by name, which is what happened before story #575
    /// and is not something it changes.
    #[test]
    fn the_flag_is_read_in_first_position_only() {
        let mut list = arguments(&["typography", "--dsb"]);
        assert_eq!(take(&mut list), Source::NotAsked);
        assert_eq!(list, arguments(&["typography", "--dsb"]));
    }

    /// The embedded golden loads through the integration crate's owning path.
    ///
    /// It is the one assertion that says this demonstration's compiled-in
    /// document is still a document the loader accepts, and it runs without a
    /// window. `scene` itself is not called here: it reads the process-wide
    /// `MAPPED`, which `main` owns, so a test that used it would decide what
    /// every other test in this binary loads.
    #[test]
    fn the_embedded_golden_loads() {
        let mut arena = dashscene_core::Arena::new();
        dashscene_desktop::load_bytes(super::DOCUMENT, Some(super::text()), &mut arena)
            .expect("the embedded golden is a committed fixture and must load");
        assert!(
            !arena.committed().rects().is_empty(),
            "the golden draws something, or the demonstration would show an empty window"
        );
    }
}

#[cfg(test)]
mod text_reaches_a_loaded_document {
    /// **A loaded document draws its text**, which is issue #863 observed from
    /// the outside — and on **both** load paths, which is not one assertion
    /// twice.
    ///
    /// `Document::load` names a shown root before attaching, so
    /// `Arena::shown_roots()` yields one root; `load_bytes` names none, so it
    /// yields every root. `TaffySolver::stage_text` iterates that set, so the
    /// two paths differ in exactly the arena state staging keys on — and the
    /// mapped one is the path R5 is about and the one story #838's renumbering
    /// moved under. A positive assertion on only the owning path would leave
    /// `demo --dsb <a file with text>` free to draw nothing with the suite
    /// green.
    ///
    /// The `None` half is asserted beside each: it is the pre-#863 picture, and
    /// it is what says the fonts are the cause rather than the document. Both
    /// halves matter — a text node that measures as an empty leaf makes its
    /// siblings lay out around a box the design did not specify, so the damage
    /// was never confined to the missing glyphs.
    #[test]
    fn a_loaded_document_draws_text_on_both_paths_when_the_host_supplies_fonts() {
        const FIXTURE: &str = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../goldens/dsb/v07-text-hug-in-fill.dsb"
        );
        let bytes = std::fs::read(FIXTURE).expect("the committed text fixture is present");

        /// Glyph runs, and the text node's resolved size.
        fn measured(arena: &dashscene_core::Arena) -> (usize, f32, f32) {
            let scene = arena.committed();
            let row = (0..scene.rects().len() as u32)
                .find(|&row| arena.text(scene.node_of(row)).is_some())
                .expect("the fixture carries a text node");
            let rect = scene.rects()[row as usize];
            (scene.glyphs().runs().len(), rect.w, rect.h)
        }

        let owning = |text| {
            let mut arena = dashscene_core::Arena::new();
            dashscene_desktop::load_bytes(&bytes, text, &mut arena).expect("the fixture loads");
            measured(&arena)
        };
        let mapped = |text| {
            let document = dashscene_desktop::Document::map(std::path::Path::new(FIXTURE))
                .expect("the committed fixture maps");
            let mut arena = dashscene_core::Arena::new();
            document
                .load(dashscene_desktop::ShownRoot::FIRST, text, &mut arena)
                .expect("the fixture loads");
            measured(&arena)
        };

        for (path, with, without) in [
            ("load_bytes", owning(Some(super::text())), owning(None)),
            ("Document::load", mapped(Some(super::text())), mapped(None)),
        ] {
            let (runs, width, height) = with;
            assert!(
                runs > 0,
                "{path}: the host supplied a cascade and its atlases, so the document's text \
                 must reach the painter as glyph runs"
            );
            assert!(
                width > 1.0 && height > 1.0,
                "{path}: and the hug-sized text node must measure to its shaped size rather \
                 than collapse: {width} x {height}"
            );
            assert_eq!(
                without,
                (0, 0.0, 0.0),
                "{path}: and without them it is the pre-#863 picture — no glyphs, and a text \
                 node measured as an empty leaf"
            );
        }
    }
}
