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
//! `commit`). That is [`load_embedded`]'s path, and since story #596 it is
//! not the only one here: [`load_mapped`] reads the envelope with
//! `dashbuf::prefix` instead, because only that reader hands back a payload's
//! byte range, which is what an image table pointing into the mapping needs.
//! The gate and the replay are the same in both; see [`scene`] for why the
//! readers differ. [`dashlang::attach_live`] is the loader-side counterpart of
//! `Scene::build_live` — it builds a [`LiveScene`] from the binding tables an
//! arena already carries, rather than from a freshly authored one — so a
//! loaded document drives the same [`LiveScene::tick`] the placeholder scene
//! does. Nothing here is a second path through the frame loop.
//!
//! [`shell::SceneBuilder`]: crate::shell::SceneBuilder
//! [`shell::ScenePulse`]: crate::shell::ScenePulse

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use dashbuf::map::MappedFile;
use dashbuf::prefix::{self, Envelope, MIN_PREFIX, PrefixError};
use dashlang::LiveScene;
use dashscene_core::{Arena, MappedPayload, Region};
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
    /// Behind an `Arc` so the image table can point into the same mapping the
    /// host holds. `Arc<MappedFile>` coerces to `Arc<dyn Region>`, which is the
    /// handle `docs/decisions/assets-borrow-from-the-mapping.md` D1 chose over
    /// a borrow — one refcount, and no lifetime on any boundary-B type.
    file: Arc<MappedFile>,
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
    let file = Arc::new(MappedFile::open(&path)?);
    if MAPPED.set(Mapped { path, file }).is_err() {
        panic!("the document source is chosen once");
    }
    Ok(())
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
/// # Two load paths, and the mapped one is not `dashbuf::open`
///
/// The embedded golden is a `&'static [u8]` with nothing to borrow from, so it
/// takes `dashbuf::open` and the loader copies each payload, as it always has.
///
/// A mapped file takes the **prefix reader** instead — `Envelope::read`, then
/// `prefix::plan` — because that is the only reader that hands back a payload's
/// byte *range*, and a range is what an image table pointing into the mapping
/// needs. `dashbuf::open` returns slices, and recovering an offset by
/// subtracting one slice's pointer from another's is refused by
/// `docs/decisions/assets-borrow-from-the-mapping.md` D6 as arithmetic that is
/// correct until someone passes a slice from elsewhere.
///
/// The reader is the browser host's, used natively — `demo-web/src/document.rs`
/// drives the same three calls over fetched ranges. Over a mapping every range
/// is already in hand, so the rounds that are network requests there are slices
/// here.
///
/// # Panics
///
/// If the document fails to open or fails the load gate. For the embedded
/// golden that never should, since it is frozen and pinned elsewhere; for a
/// file named on the command line it is the ordinary way to be told the file is
/// not a `.dsb`, and a panic is louder than silently painting nothing.
pub fn scene(arena: &mut Arena, _width: u32, _height: u32) -> LiveScene {
    match MAPPED.get() {
        Some(mapped) => load_mapped(mapped, arena),
        None => load_embedded(arena),
    }
}

/// The embedded golden: `dashbuf::open` over bytes that borrow from nothing,
/// and the loader's owning path.
fn load_embedded(arena: &mut Arena) -> LiveScene {
    let name = source_name();
    let (document, payloads) =
        dashbuf::open(DOCUMENT).unwrap_or_else(|error| panic!("{name} does not open: {error}"));
    gate(&document, &name);
    dashscene_core::load_document(&document, &payloads, arena);
    dashlang::attach_live(arena, Box::new(TaffySolver::new()))
}

/// A mapped file: the prefix reader for the ranges, then the loader's mapped
/// path, which copies no payload byte at all.
fn load_mapped(mapped: &'static Mapped, arena: &mut Arena) -> LiveScene {
    let name = mapped.path.display().to_string();
    let file = mapped.file.bytes();
    let file_len = file.len() as u64;

    // At most two answers, by `Envelope::read`'s contract: the header, then the
    // table whose length the header states. Bounded rather than looped on
    // trust, exactly as the browser host bounds it — the difference is that
    // here a "fetch" is a slice of memory already mapped.
    let mut need = MIN_PREFIX.min(file.len());
    let mut envelope = None;
    for _ in 0..2 {
        match Envelope::read(&file[..need], file_len) {
            Ok(read) => {
                envelope = Some(read);
                break;
            }
            Err(PrefixError::NeedMore { need: more }) => {
                need = more.min(file.len());
            }
            Err(PrefixError::Malformed(error)) => panic!("{name} is not a .dsb: {error}"),
        }
    }
    let envelope = envelope.unwrap_or_else(|| panic!("{name}: the envelope never resolved"));

    let hot = &file[..envelope.hot_len() as usize];
    let plan = prefix::plan(&envelope, hot).unwrap_or_else(|e| panic!("{name} does not open: {e}"));
    let document = plan.document();
    gate(&document, &name);

    // One `MappedPayload` per asset entry, in entry order — which is exactly
    // what `Plan::wanted` returns, undeduplicated, so no reordering or
    // expansion is needed here.
    //
    // Bound as **canonical**, and refused when that would be a lie. A file with
    // a derivation manifest carries the rung a profile selected, not the
    // payload the document names, and binding it as canonical tags a KTX2 as a
    // `Png` — the mistake issue #640 exists to prevent. The owning path finds
    // that out by parsing the payload's header; the mapped path reads no header
    // at all, by design, so nothing downstream would catch it. This host ships
    // no profile and has no way to name a rung, so the honest answer is to
    // refuse the file rather than draw the wrong thing.
    // Verification, unchanged from the owning path. `Plan::bind` hashes every
    // payload against the section table, which is the promise `dashbuf::open`
    // keeps by hashing before it returns bytes; a prefix load keeps it here or
    // nowhere. Its return value — the same payloads as slices — is not used,
    // because the loader takes ranges; what is wanted is the check.
    //
    // This is the eager verification story #597 moves to touch time. Until it
    // does, a mapped load still faults every payload in, so this story removes
    // the copies and moves no number on story #598's criterion.
    let resident: Vec<&[u8]> = plan
        .wanted()
        .iter()
        .map(|want| &file[want.range.start as usize..want.range.end as usize])
        .collect();
    plan.bind(&resident).unwrap_or_else(|error| {
        panic!("{name} carries a payload that fails its own hash: {error}")
    });

    let entries = document.assets().unwrap_or_default();
    let payloads: Vec<MappedPayload> = plan
        .wanted()
        .iter()
        .zip(entries.iter())
        .map(|(want, entry)| {
            assert_eq!(
                want.hash,
                entry.hash().bytes(),
                "{name} binds a derived payload through its derivation manifest, and this host \
                 has no quality profile to name the rung with: it can map a RAW file only \
                 (issue #640)"
            );
            MappedPayload::canonical(want.range.clone())
        })
        .collect();

    // The region the table points into is this same mapping, shared rather
    // than opened again: `MappedFile` is `Send + Sync` and satisfies `Region`
    // through its `AsRef<[u8]>`, so the handle costs one refcount and no
    // lifetime anywhere. `scene` runs again on every resize, and each run hands
    // the table another reference to the one mapping made in `main`.
    let region: Arc<dyn Region> = mapped.file.clone();
    dashscene_core::load_document_mapped(&document, region, &payloads, arena);
    dashlang::attach_live(arena, Box::new(TaffySolver::new()))
}

/// The referential load gate, run before either replay.
fn gate(document: &dashbuf::Document<'_>, name: &str) {
    let report = dashscene_validator::validate_document(document);
    assert!(
        !report.has_errors(),
        "{name} fails the load gate: {report:?}"
    );
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
    use std::path::PathBuf;
    use std::sync::Arc;

    use dashbuf::map::MappedFile;

    use super::{Mapped, Source, load_mapped, take};

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

    /// A mapped file that binds a **derived** payload is refused by name.
    ///
    /// The guard exists because story #596 would otherwise have turned a loud
    /// failure into a silently wrong picture: the owning path parses the
    /// payload's header and panics when a KTX2 arrives where the entry says
    /// `Png`, and the mapped path reads no header at all. `v03-paint-hifi.dsb`
    /// is the one committed fixture that carries a derivation manifest, so it
    /// is the only fixture this can be shown with.
    #[test]
    #[should_panic(expected = "no quality profile to name the rung")]
    fn a_mapped_file_that_binds_a_derived_payload_is_refused() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../goldens/dsb/v03-paint-hifi.dsb");
        let file = Arc::new(MappedFile::open(&path).expect("the fixture maps"));
        // Leaked rather than stored in `MAPPED`: the static is set once per
        // process and `main` owns that, so a test that wrote to it would decide
        // what every other test in this binary loads.
        let mapped: &'static Mapped = Box::leak(Box::new(Mapped { path, file }));

        let mut arena = dashscene_core::Arena::new();
        load_mapped(mapped, &mut arena);
    }
}
