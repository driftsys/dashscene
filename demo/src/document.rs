//! Loads a compiled `.dsb` into the host, as the same [`shell::SceneBuilder`]
//! / [`shell::ScenePulse`] pair [`crate::placeholder_scene`] builds by hand
//! (story #575, epic #568).
//!
//! `dashscene_core::load`'s own doc comment states the read contract: run
//! `dashbuf::open_verified` (the envelope plus the flatbuffers verifier — it
//! calls `root_as_document` internally, since a `.dsb` has been a sectioned
//! container since v0.11), then `dashscene_validator::validate_document`
//! (the referential load gate), then `dashscene_core::load_document` (the
//! replay through the ordinary producer API: `add_node` / `set_prop` /
//! `commit`). That is [`load_embedded`]'s path, and it is not the only one
//! here: [`load_mapped`] reads through `dashbuf::open`, which hands back where
//! each payload lies rather than the payload, makes the shown root's assets
//! resident through a `BlobResidency`, and leaves the rest of the file cold. The
//! gate and the replay are the same in both; see [`scene`] for why the readers
//! differ. [`dashlang::attach_live`] is the loader-side counterpart of
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
use dashbuf::residency::BlobResidency;
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
/// # Two load paths, and only one of them is bounded by what is shown
///
/// The embedded golden is a `&'static [u8]` with nothing to borrow from and no
/// pages to fault, so it takes `dashbuf::open_verified` and the loader copies
/// each payload, as it always has. That path cannot be bounded by what is
/// shown even in principle: `load_document` copies every payload into an owned
/// `ImageAsset`, so every entry needs bytes.
///
/// A mapped file takes `dashbuf::open`, which resolves each asset entry to
/// where its payload lies and reads none of them. The shown root's assets are
/// then made resident one at a time through a `BlobResidency`, and
/// `load_document_mapped` binds ranges rather than bytes — so the cost of
/// opening this file tracks the root being drawn rather than the file's size,
/// which is R5 (`docs/decisions/verification-moves-from-open-to-touch.md`).
///
/// Until story #597 this path read the envelope with `dashbuf::prefix`, because
/// at the time that was the only reader handing back a payload's byte *range*.
/// `dashbuf::open` returns ranges now, so the native host reads through the
/// strict reader again — which is the home
/// `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`
/// gives it, since bounds-checking a section against a full-length mapping
/// costs nothing and touches no page. `prefix` stays what that record says it
/// is: the reader for a host holding only a prefix, which is
/// `demo-web/src/document.rs`.
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

/// The embedded golden: `dashbuf::open_verified` over bytes that borrow from
/// nothing, and the loader's owning path.
fn load_embedded(arena: &mut Arena) -> LiveScene {
    let name = source_name();
    let (document, payloads) = dashbuf::open_verified(DOCUMENT)
        .unwrap_or_else(|error| panic!("{name} does not open: {error}"));
    gate(&document, &name);
    dashscene_core::load_document(&document, &payloads, arena);
    dashlang::attach_live(arena, Box::new(TaffySolver::new()))
}

/// A mapped file: `dashbuf::open` for the ranges, a `BlobResidency` for the shown
/// root's payloads, then the loader's mapped path, which copies no payload byte
/// at all.
fn load_mapped(mapped: &'static Mapped, arena: &mut Arena) -> LiveScene {
    let name = mapped.path.display().to_string();
    let file = mapped.file.bytes();

    // Reads the envelope, every structured section and the binding, and stops
    // at where each payload lies. No blob page is faulted in by this call.
    let (document, wanted) =
        dashbuf::open(file).unwrap_or_else(|error| panic!("{name} does not open: {error}"));
    gate(&document, &name);

    // Bound as **canonical**, and refused when that would be a lie. A file with
    // a derivation manifest carries the rung a profile selected, not the
    // payload the document names, and binding it as canonical tags a KTX2 as a
    // `Png` — the mistake issue #640 exists to prevent. The owning path finds
    // that out by parsing the payload's header; the mapped path reads no header
    // at all, by design, so nothing downstream would catch it. This host ships
    // no profile and has no way to name a rung, so the honest answer is to
    // refuse the file rather than draw the wrong thing.
    let entries = document.assets().unwrap_or_default();
    for (want, entry) in wanted.iter().zip(entries.iter()) {
        assert_eq!(
            want.hash,
            entry.hash().bytes(),
            "{name} binds a derived payload through its derivation manifest, and this host \
             has no quality profile to name the rung with: it can map a RAW file only \
             (issue #640)"
        );
    }

    // The prefetch, and the whole of what this host reads out of the file's
    // cold half: the assets the shown root's subtree draws, proven one at a
    // time. Everything else stays cold, which is what makes cold start track
    // the root being drawn rather than the file's size (R5, D4).
    //
    // **A row bound below whose payload was not touched is not proven.** The
    // image table takes one row per asset entry, so a many-frame document's
    // other frames are ranges into this mapping that nothing has hashed.
    // Nothing draws them this slice — a frame whose payload is not ready needs
    // the placeholder field that has no producer, which stays in v1 (D6) — and
    // debt #779 carries the gap.
    let residency = BlobResidency::new();
    let shown = dashbuf::prefetch::first_root(&document)
        .unwrap_or_else(|| panic!("{name} carries no root node"));
    for index in dashbuf::prefetch::assets_of_root(&document, shown) {
        let want = &wanted[index as usize];
        let bytes = &file[want.range.start as usize..want.range.end as usize];
        residency.touch(want, bytes).unwrap_or_else(|error| {
            panic!("{name} carries a payload that fails its own hash: {error}")
        });
    }

    // One `MappedPayload` per asset entry, in entry order — which is exactly
    // the order `dashbuf::open` returns its `Wanted`s in, undeduplicated, so no
    // reordering or expansion is needed here.
    let payloads: Vec<MappedPayload> = wanted
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
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

    /// A two-root `.dsb`, RAW, with `corrupt`'s payload one byte wrong.
    ///
    /// No committed fixture is this shape: every `goldens/dsb` document has one
    /// root, and over a one-root document "the shown root's assets" and "every
    /// asset in the file" are the same set — so nothing built from one can tell
    /// a prefetch bounded by the shown root from a prefetch of the whole table.
    /// Two roots, one payload each, is the smallest document that can.
    ///
    /// Each root's paint is an image fill naming its own asset, so root A's
    /// subtree reaches asset 0 and nothing else.
    fn two_root_document(corrupt: usize) -> Vec<u8> {
        use dashbuf::{
            AssetEntry, AssetEntryArgs, AssetKind, Document, DocumentArgs, Fill, ImageFill,
            ImageFillArgs, ImageFormat, NO_PARENT, Node, NodeArgs, Paint, PaintArgs,
        };
        use flatbuffers::FlatBufferBuilder;

        // Distinct bytes and distinct lengths, so a swapped pair is visible.
        let payloads = [vec![0xA1u8; 64], vec![0xB2u8; 96]];
        let mut builder = FlatBufferBuilder::new();

        let entries: Vec<_> = payloads
            .iter()
            .map(|payload| {
                let hash = builder.create_vector(blake3::hash(payload).as_bytes());
                AssetEntry::create(
                    &mut builder,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: ImageFormat::Png,
                        kind: AssetKind::Image,
                        width: 8,
                        height: 8,
                    },
                )
            })
            .collect();
        let assets = builder.create_vector(&entries);

        let paints: Vec<_> = [0u32, 1]
            .into_iter()
            .map(|image| {
                let fill = ImageFill::create(
                    &mut builder,
                    &ImageFillArgs {
                        image,
                        ..Default::default()
                    },
                );
                Paint::create(
                    &mut builder,
                    &PaintArgs {
                        fill_type: Fill::ImageFill,
                        fill: Some(fill.as_union_value()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let paints = builder.create_vector(&paints);

        let nodes: Vec<_> = [0u32, 1]
            .into_iter()
            .map(|paint_entry| {
                Node::create(
                    &mut builder,
                    &NodeArgs {
                        parent: NO_PARENT,
                        paint_entry,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let nodes = builder.create_vector(&nodes);

        let document = Document::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                paints: Some(paints),
                assets: Some(assets),
                ..Default::default()
            },
        );
        builder.finish(document, None);

        let bank = dashbuf::bank::ColdBank::raw(payloads.iter().map(Vec::as_slice));
        let mut file =
            dashbuf::bank::assemble(builder.finished_data(), &bank).expect("the fixture assembles");

        // The blob sections are in asset-entry order for a RAW assembly, and the
        // section table is left untouched — so the file still records what each
        // payload should hash to, and only a read of the bytes can notice.
        let container = dashbuf::container::Container::parse(&file).expect("the fixture parses");
        let blobs: Vec<_> = container
            .sections()
            .filter(|entry| entry.kind == dashbuf::container::SectionKind::Blob as u16)
            .collect();
        assert_eq!(blobs.len(), payloads.len(), "one blob per payload");
        let at = blobs[corrupt].offset as usize;
        file[at] ^= 0xFF;
        file
    }

    /// Maps `bytes` and loads them as the host would, keeping the temporary
    /// directory alive for the call.
    fn load_bytes_mapped(bytes: &[u8]) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("fixture.dsb");
        std::fs::write(&path, bytes).expect("the fixture writes");

        let file = Arc::new(MappedFile::open(&path).expect("the fixture maps"));
        // Leaked for the same reason the tests below leak: `MAPPED` is set once
        // per process and `main` owns that.
        let mapped: &'static Mapped = Box::leak(Box::new(Mapped { path, file }));

        let mut arena = dashscene_core::Arena::new();
        load_mapped(mapped, &mut arena);
    }

    /// The frame nobody is showing is never read, even when its payload is
    /// wrong.
    ///
    /// This is R5 stated as a behaviour rather than as a count, and it is the
    /// one assertion that fails if the host ever touches its whole asset table
    /// instead of the shown root's set — the change that would put the
    /// startup-scaling criterion back where epic #594 found it. Its partner
    /// below corrupts root A's payload in the same fixture and requires the
    /// panic, so neither can pass by the host reading nothing at all.
    #[test]
    fn a_mapped_load_leaves_the_frame_nobody_shows_cold() {
        load_bytes_mapped(&two_root_document(1));
    }

    /// The same fixture with the **shown** root's payload corrupted is refused.
    #[test]
    #[should_panic(expected = "fails its own hash")]
    fn a_mapped_load_refuses_the_shown_roots_corrupted_payload() {
        load_bytes_mapped(&two_root_document(0));
    }

    /// The shown root's payload is made resident, and a corrupted one is
    /// refused by name.
    ///
    /// This is the only thing that says the prefetch above is wired at all.
    /// Every other assertion about this host passes with the touch loop
    /// removed: `dashbuf::open` reads no payload, so a corrupted file opens,
    /// the load gate is about references rather than bytes, and
    /// `load_document_mapped` binds ranges without reading them. A host that
    /// prefetched nothing would draw a corrupted picture in silence, which is
    /// exactly what moving verification off the reader risks — so the check is
    /// that the payload the shown root draws is proven before the load.
    ///
    /// `v03-paint.dsb` is one of the two committed fixtures carrying a payload,
    /// and its single root draws it, so its whole asset table is the shown
    /// root's prefetch set.
    #[test]
    #[should_panic(expected = "fails its own hash")]
    fn a_mapped_file_whose_shown_payload_is_corrupted_is_refused() {
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../goldens/dsb/v03-paint.dsb");
        let mut bytes = std::fs::read(&source).expect("the fixture reads");

        // One byte of the payload itself. The section table is left alone, so
        // it still records what the payload should hash to and nothing before
        // the touch can notice: the root hash covers the table, not the file.
        let container = dashbuf::container::Container::parse(&bytes).expect("the fixture parses");
        let blob = container
            .sections()
            .find(|entry| entry.kind == dashbuf::container::SectionKind::Blob as u16)
            .expect("v03-paint.dsb carries a payload");
        bytes[blob.offset as usize] ^= 0xFF;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("corrupted.dsb");
        std::fs::write(&path, &bytes).expect("the corrupted fixture writes");

        let file = Arc::new(MappedFile::open(&path).expect("the corrupted fixture maps"));
        // Leaked for the same reason the test below leaks: `MAPPED` is set once
        // per process and `main` owns that.
        let mapped: &'static Mapped = Box::leak(Box::new(Mapped { path, file }));

        let mut arena = dashscene_core::Arena::new();
        load_mapped(mapped, &mut arena);
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
