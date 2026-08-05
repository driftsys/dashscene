//! Loads a compiled `.dsb` into the host, as the same [`shell::SceneBuilder`]
//! / [`shell::ScenePulse`] pair [`crate::placeholder_scene`] builds by hand
//! (story #575, epic #568).
//!
//! `dashscene_core::load`'s own doc comment states the read contract: run
//! `dashbuf::open` (the envelope plus the flatbuffers verifier — it calls
//! `root_as_document` internally, since a `.dsb` has been a sectioned
//! container since v0.11), then `dashscene_validator::validate_document`
//! (the referential load gate), then `dashscene_core::load_document` (the
//! replay through the ordinary producer API: `add_node` / `set_prop` /
//! `commit`). [`dashlang::attach_live`] is the loader-side counterpart of
//! `Scene::build_live` — it builds a [`LiveScene`] from the binding tables an
//! arena already carries, rather than from a freshly authored one — so a
//! loaded document drives the same [`LiveScene::tick`] the placeholder scene
//! does. Nothing here is a second path through the frame loop.
//!
//! [`shell::SceneBuilder`]: crate::shell::SceneBuilder
//! [`shell::ScenePulse`]: crate::shell::ScenePulse

use std::path::PathBuf;
use std::sync::OnceLock;

use dashbuf::map::MappedFile;
use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_engine::TaffySolver;

/// `goldens/dsb/v03-paint.dsb`: `dashc::compile_figma`'s output for a real
/// Figma capture, `corpus/figma-fixtures/v03-paint.json` — a 960x680 paint
/// vocabulary swatch board (solid, gradient, and image fills, a stroke,
/// corner radii) that two CI suites already pin byte-for-byte
/// (`goldens/dsb/README.md`).
///
/// Picked over the other nine `goldens/dsb` fixtures because it is the
/// richest compiled document in the tree (14 nodes, 14 paint entries) and
/// one of only **two** carrying a real embedded image through the v0.11
/// asset table, so loading it exercises the payload-binding step in
/// `dashscene_core::load_document` that the other eight never reach. The
/// second is `v03-paint-hifi.dsb`, this document's HiFi derivation, which
/// carries the same picture as a derived payload behind a manifest section
/// — so it is the same document rather than a second one to choose from.
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

/// The mapped file this host was pointed at, and the path it was named by.
///
/// A `OnceLock` rather than a field on the scene entry because
/// [`crate::shell::SceneEntry`] holds `build` as a plain function pointer: the
/// registry lists scenes that need no configuration, and one that does cannot
/// carry it through that seam without widening it for every other scene. Set
/// once, in `main`, before the loop starts.
///
/// The mapping is held for the life of the process rather than per call.
/// [`scene`] runs again on every resize, so remapping each time would be waste
/// — but it also has to be held. Today `dashscene_core::load_document` copies
/// each payload out, so the mapping could be dropped after the load; **story
/// #596 removes that copy**, after which the arena's image table points into
/// this region and dropping it would leave the painter reading unmapped memory.
/// Holding it here is what makes that change a no-op for this host
/// (`docs/decisions/assets-borrow-from-the-mapping.md`).
static MAPPED: OnceLock<Mapped> = OnceLock::new();

/// The mapping and the path it was named by, which is what a failure message
/// has to say.
struct Mapped {
    path: PathBuf,
    file: MappedFile,
}

/// What `--dsb` asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `--dsb` was not given: this host is showing showcase scenes.
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
/// what this host has drawn since story #575, and it exists so the host carries
/// no runtime path dependency on the working tree; taking that away to add a
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
/// and this is the last place that can report it and exit rather than panic
/// out of the frame loop.
///
/// # Panics
///
/// If called twice. One run draws one document, and a second call would mean
/// two answers to which.
pub fn map_file(path: PathBuf) -> std::io::Result<()> {
    let file = MappedFile::open(&path)?;
    if MAPPED.set(Mapped { path, file }).is_err() {
        panic!("the document source is chosen once");
    }
    Ok(())
}

/// The bytes [`scene`] loads: the mapping when `--dsb <path>` named one, and
/// the embedded golden otherwise.
fn bytes() -> &'static [u8] {
    MAPPED.get().map_or(DOCUMENT, |mapped| mapped.file.bytes())
}

/// How the loaded document is named in a failure, so a panic says which file it
/// was reading.
fn source_name() -> String {
    MAPPED.get().map_or_else(
        || "goldens/dsb/v03-paint.dsb (embedded)".to_owned(),
        |mapped| mapped.path.display().to_string(),
    )
}

/// Loads the selected document into `arena` and attaches a [`LiveScene`] to it.
///
/// `width` and `height` go unused: a loaded document already carries its own
/// resolved canvas size (P1 — the document is intent, and this document's
/// intent already includes concrete geometry from the Figma capture it was
/// compiled from), unlike [`crate::placeholder_scene`], which derives every
/// offset from the drawable it is given. A resize therefore reloads the same
/// picture rather than rescaling it.
///
/// The three steps are `dashbuf::open`, the referential load gate, then the
/// replay — unchanged by the mapping, which is the whole claim `dashbuf::map`
/// makes: `open` takes a `&[u8]` and does not care where it came from.
///
/// # Panics
///
/// If the document fails to open or fails the load gate. For the embedded
/// golden that never should, since it is frozen and pinned elsewhere; for a
/// file named on the command line it is the ordinary way to be told the file is
/// not a `.dsb`, and a panic is louder than silently painting nothing.
pub fn scene(arena: &mut Arena, _width: u32, _height: u32) -> LiveScene {
    let name = source_name();
    let (document, payloads) =
        dashbuf::open(bytes()).unwrap_or_else(|error| panic!("{name} does not open: {error}"));
    let report = dashscene_validator::validate_document(&document);
    assert!(
        !report.has_errors(),
        "{name} fails the load gate: {report:?}"
    );
    dashscene_core::load_document(&document, &payloads, arena);
    dashlang::attach_live(arena, Box::new(TaffySolver::new()))
}

/// No-op: `DOCUMENT` carries no `dashcue` signal or binding rows, and no
/// variant table either — inspected while building this story, and true of
/// every one of the ten committed `goldens/dsb` fixtures today (issue #617)
/// — so there is nothing for a pulse to drive.
///
/// The frame loop still ticks the attached scene every frame regardless:
/// with no live binding and no scheduler track, `LiveScene::tick` takes its
/// idle-frame early return after the first present, and the loop settles and
/// stops painting. That is this story's other finding — the idle skip holds
/// for a loaded document exactly as it does for the placeholder scene.
pub fn pulse(_live: &mut LiveScene, _index: u64) {}

#[cfg(test)]
mod tests {
    use super::{Source, take};
    use std::path::PathBuf;

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
    /// selection to refuse by name, which is what happened before this story
    /// and is not something it changes.
    #[test]
    fn the_flag_is_read_in_first_position_only() {
        let mut list = arguments(&["typography", "--dsb"]);
        assert_eq!(take(&mut list), Source::NotAsked);
        assert_eq!(list, arguments(&["typography", "--dsb"]));
    }
}
