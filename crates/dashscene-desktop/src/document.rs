//! Loading a compiled `.dsb` from a file, bounded by the root that is shown
//! (story #575, extracted at story #794).
//!
//! The file is **mapped** rather than read. `dashbuf::open` resolves each asset
//! entry to where its payload lies and reads none of them; the shown root's
//! assets are then made resident one at a time through a `BlobResidency`, and
//! `load_document_mapped` binds ranges rather than bytes — so the cost of
//! opening a file tracks the root being drawn rather than the file's size,
//! which is R5 (`docs/decisions/verification-moves-from-open-to-touch.md`).
//!
//! `dashbuf::open` is the strict reader, and it is the right one here for the
//! reason `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`
//! gives: bounds-checking a section against a full-length mapping costs nothing
//! and touches no page. `dashbuf::prefix` is the reader for a host holding only
//! a prefix, which on the desktop is nothing — it is what `dashscene-web` reads
//! a fetched byte range through.
//!
//! [`dashlang::attach_live`] is the loader-side counterpart of
//! `Scene::build_live` — it builds a [`LiveScene`] from the binding tables an
//! arena already carries, rather than from a freshly authored one — so a loaded
//! document drives the same `LiveScene::tick` a scene built in code does.
//! Nothing here is a second path through the frame loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashbuf::map::MappedFile;
use dashbuf::prefetch::ShownRoot;
use dashbuf::residency::BlobResidency;
use dashlang::LiveScene;
use dashscene_core::{Arena, MappedPayload, Region};

use crate::DesktopError;

/// A mapped `.dsb`, ready to be loaded into an arena.
///
/// Held by the embedder rather than by a static, and held for as long as the
/// picture is on screen. [`Document::load`] runs again whenever the loop
/// rebuilds, so remapping each time would be waste — but it also **has** to be
/// held: `load_document_mapped` copies no payload byte, so the arena's image
/// table points into this mapping and dropping it would leave the painter
/// reading unmapped memory
/// (`docs/decisions/assets-borrow-from-the-mapping.md`).
///
/// The arena holds its own reference for exactly that reason, so a `Document`
/// dropped while an arena built from it is still alive is safe — it is the last
/// reference that unmaps, not this one.
pub struct Document {
    path: PathBuf,
    /// Behind an `Arc` so the image table can point into the same mapping this
    /// holds. `Arc<MappedFile>` coerces to `Arc<dyn Region>`, which is the
    /// handle `docs/decisions/assets-borrow-from-the-mapping.md` D1 chose over
    /// a borrow — one refcount, and no lifetime on any boundary-B type.
    file: Arc<MappedFile>,
}

impl Document {
    /// Maps `path`.
    ///
    /// Mapping is separated from loading because a path the process cannot
    /// reach is the failure an embedder can report before it opens a window,
    /// and [`Document::load`] runs inside the loop where there is no longer
    /// anywhere useful to report to.
    pub fn map(path: impl AsRef<Path>) -> Result<Self, DesktopError> {
        let path = path.as_ref().to_path_buf();
        let file = MappedFile::open(&path).map_err(|error| DesktopError::Map {
            path: path.display().to_string(),
            error,
        })?;
        Ok(Self {
            path,
            file: Arc::new(file),
        })
    }

    /// The path this was mapped from, which is what a failure message has to
    /// name.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replays the document into `arena` and attaches a [`LiveScene`] to it,
    /// bounding what it reads by the root `shown_root` names.
    ///
    /// **`shown_root` is a parameter rather than state on this handle**, because
    /// `load` runs again on every rebuild and the embedder is what knows which
    /// artboard it is showing. A field here would have to be kept in step with
    /// that anyway, and would make "which root is on screen" answerable in two
    /// places (story #837).
    ///
    /// It bounds the **load** and nothing below it. Every root is still solved,
    /// committed and painted; story #838 is what changes that. So passing a
    /// different `shown_root` today changes which payloads are made resident and
    /// which are left cold, and does not change the picture.
    ///
    /// The extent is not a parameter: a loaded document already carries its own
    /// resolved canvas size (P1 — the document is intent, and a document
    /// compiled from a Figma capture has concrete geometry in that intent),
    /// unlike a scene built in code, which derives every offset from the
    /// drawable it is given. A resize therefore reloads the same picture rather
    /// than rescaling it, which is why [`crate::App::build`] is free to ignore
    /// the extent it is handed for this case.
    ///
    /// # Errors
    ///
    /// Every error **this function returns** is raised before the document is
    /// replayed into `arena`, so a failed load leaves `arena` exactly as it was
    /// and an embedder may reuse it. That is not true of the panics below.
    ///
    /// # Panics
    ///
    /// This can panic rather than return, and the conditions belong to the
    /// crates under it rather than to this signature — no attempt is made here
    /// to enumerate them, because the set is theirs to change. Two are worth
    /// naming because an embedder meets them:
    ///
    /// - `Txn::use_mapped_pool` refuses an arena whose image table already holds
    ///   rows, **whatever put them there** — an earlier mapped load, or images
    ///   the embedder staged itself. A table is owned or mapped and never both
    ///   (`docs/decisions/assets-borrow-from-the-mapping.md` D1). Passing a
    ///   fresh [`Arena`] avoids it, which is what [`crate::App`] does on every
    ///   rebuild.
    /// - A mapped payload lying past 4 GiB into the file exceeds what an image
    ///   row's `u32` offset can name.
    ///
    /// **A panic here unwinds on this target.** An embedder catching one holds
    /// an arena in a state this function does not specify — how far the replay
    /// reached depends on which condition fired — so discard it rather than draw
    /// it.
    pub fn load(
        &self,
        shown_root: ShownRoot,
        text: Option<crate::TextResources>,
        arena: &mut Arena,
    ) -> Result<LiveScene, DesktopError> {
        let name = self.path.display().to_string();
        let bytes = self.file.bytes();

        // Reads the envelope, every structured section and the binding, and
        // stops at where each payload lies. No blob page is faulted in by this
        // call.
        let (document, wanted) = dashbuf::open(bytes).map_err(DesktopError::Open)?;

        let report = dashscene_validator::validate_document(&document);
        if report.has_errors() {
            return Err(DesktopError::Gate { path: name, report });
        }

        // Bound as **canonical**, and refused when that would be a lie. A file
        // with a derivation manifest carries the rung a profile selected, not
        // the payload the document names, and binding it as canonical tags a
        // KTX2 as a `Png` — the mistake issue #640 exists to prevent. The
        // owning path finds that out by parsing the payload's header; this path
        // reads no header at all, by design, so nothing downstream would catch
        // it. This crate ships no profile and has no way to name a rung, so the
        // honest answer is to refuse the file rather than draw the wrong thing.
        //
        // The comparison itself is `dashscene-core`'s, because `dashscene-web`
        // and `dashscene-ffi` make the same one and each names its own source
        // in its own error type.
        if dashscene_core::first_derived_payload(&document, &wanted).is_some() {
            return Err(DesktopError::Derived { path: name });
        }

        // The prefetch, and the whole of what this reads out of the file's cold
        // half: the assets the shown root's subtree draws, proven one at a
        // time. Everything else stays cold, which is what makes cold start
        // track the root being drawn rather than the file's size (R5, D4).
        //
        // **A row bound below whose payload was not touched is not proven**, and
        // that is still true: the image table takes one row per asset entry, so
        // a many-frame document's other frames are ranges into this mapping
        // that nothing has hashed.
        //
        // What changed at story #838 is that nothing can **reach** them. The
        // traversal, the solve and the paint follow the shown root named below,
        // so a row no rect references is a row no painter resolves — which is
        // what debt #779 was waiting for, and it closes that debt rather than
        // narrowing it.
        //
        // The coupling is worth naming, because it is the way it comes back:
        // the rows are safe **because** the traversal is confined. A load that
        // named no root — `Txn::show_root(None)` is still every root — would
        // bind the same unverified rows and paint them.
        let residency = BlobResidency::new();
        let root = dashbuf::prefetch::resolve(&document, shown_root).ok_or_else(move || {
            DesktopError::NoSuchRoot {
                path: name,
                ordinal: shown_root.ordinal(),
                roots: dashbuf::prefetch::root_count(&document),
            }
        })?;
        for index in dashbuf::prefetch::assets_of_root(&document, root) {
            let want = &wanted[index as usize];
            let payload = &bytes[want.range.start as usize..want.range.end as usize];
            residency
                .touch(want, payload)
                .map_err(DesktopError::Payload)?;
        }

        // One `MappedPayload` per asset entry, in entry order — which is
        // exactly the order `dashbuf::open` returns its `Wanted`s in,
        // undeduplicated, so no reordering or expansion is needed here.
        let payloads: Vec<MappedPayload> = wanted
            .iter()
            .map(|want| MappedPayload::canonical(want.range.clone()))
            .collect();

        // How many roots the arena held *before* this load, so the document
        // ordinal can be turned into the arena node it actually named. The load
        // appends to whatever the arena already holds, so the two ordinals agree
        // only when it held nothing (issue #943).
        //
        // **Not two mapped documents**, whatever issue #943's text says: this
        // path calls `Txn::use_mapped_pool`, which refuses an arena whose image
        // table already holds rows. What reaches here is an arena holding nodes
        // but no images — and on the *owned* `load_document` path, any two
        // documents.
        let roots_before = arena.roots().len();
        // The region the table points into is this same mapping, shared rather
        // than opened again: `MappedFile` is `Send + Sync` and satisfies
        // `Region` through its `AsRef<[u8]>`, so the handle costs one refcount
        // and no lifetime anywhere. This runs again on every rebuild, and each
        // run hands the table another reference to the one mapping.
        let region: Arc<dyn Region> = self.file.clone();
        dashscene_core::load_document_mapped(&document, region, &payloads, arena);
        // The runtime's half of the bound the prefetch above took. The load
        // replays every root — a document is every artboard it carries, and
        // dropping one at load would make the file unreadable rather than
        // unshown — and this confines what is solved, committed and painted to
        // the one being shown (story #838, issue #822).
        //
        // The ordinal correction, the commit, and the argument for a named
        // panic over a typed error are all `show_appended_root`'s own
        // documentation now — `dashscene-web` and `dashscene-ffi` make this
        // same call, and the reasoning belongs where all three can read it.
        dashscene_core::show_appended_root(
            &document,
            shown_root,
            roots_before,
            &self.path.display(),
            arena,
        );
        Ok(dashlang::attach_live(
            arena,
            dashscene_engine::TaffySolver::boxed(text),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Document, ShownRoot};
    use crate::DesktopError;

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
            AssetEntry, AssetEntryArgs, AssetKind, Document as Doc, DocumentArgs, Fill, ImageFill,
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

        let document = Doc::create(
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

        // The blob sections are in asset-entry order for a RAW assembly, and
        // the section table is left untouched — so the file still records what
        // each payload should hash to, and only a read of the bytes can notice.
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

    /// The failure, or a panic naming what should have failed.
    ///
    /// `Result::expect_err` needs `Debug` on the success type, and neither
    /// `LiveScene` nor [`Document`] carries one — a mapping handle and a live
    /// scene have nothing useful to print, and deriving it to satisfy a test
    /// would put it in the published API.
    fn refusal<T>(result: Result<T, DesktopError>, expected: &str) -> DesktopError {
        match result {
            Ok(_) => panic!("{expected}, and it was accepted"),
            Err(error) => error,
        }
    }

    /// Maps `bytes` and loads them showing `shown_root`, keeping the temporary
    /// directory alive for the call.
    fn load_bytes_showing(bytes: &[u8], shown_root: ShownRoot) -> Result<(), DesktopError> {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("fixture.dsb");
        std::fs::write(&path, bytes).expect("the fixture writes");

        let document = Document::map(&path)?;
        let mut arena = dashscene_core::Arena::new();
        document.load(shown_root, None, &mut arena).map(|_| ())
    }

    /// [`load_bytes_showing`] with the first root, which is what every test
    /// written before story #837 meant.
    fn load_bytes(bytes: &[u8]) -> Result<(), DesktopError> {
        load_bytes_showing(bytes, ShownRoot::FIRST)
    }

    /// The frame nobody is showing is never read, even when its payload is
    /// wrong.
    ///
    /// This is R5 stated as a behaviour rather than as a count, and it is the
    /// one assertion that fails if this ever touches the whole asset table
    /// instead of the shown root's set — the change that would put the
    /// startup-scaling criterion back where epic #594 found it. Its partner
    /// below corrupts root A's payload in the same fixture and requires the
    /// refusal, so neither can pass by reading nothing at all.
    #[test]
    fn a_mapped_load_leaves_the_frame_nobody_shows_cold() {
        load_bytes(&two_root_document(1)).expect("the unshown root's payload is never read");
    }

    /// The same fixture with the **shown** root's payload corrupted is refused.
    #[test]
    fn a_mapped_load_refuses_the_shown_roots_corrupted_payload() {
        let error = load_bytes(&two_root_document(0)).expect_err("a corrupted payload is refused");
        assert!(
            matches!(error, DesktopError::Payload(_)),
            "the shown root's corrupted payload must be reported as a payload mismatch, not as \
             {error}"
        );
    }

    /// Showing the **second** root reads the second root's payload and leaves
    /// the first cold — the two assertions above with the roles exchanged
    /// (story #837).
    ///
    /// This pair is what says a [`ShownRoot`] is read rather than accepted and
    /// ignored, and it says it in the only way that cannot be satisfied by
    /// reading nothing: the corruption that must be tolerated and the
    /// corruption that must be refused swap places with the ordinal, so a
    /// loader that read every payload fails the first and one that read none
    /// fails the second. Nothing in the fixture changes between the four tests.
    ///
    /// It is also this crate's answer to story #837's "a test showing a document
    /// with more than one root loading bounded by a root that is not root 0",
    /// and the reason that story could be built without authoring a fixture:
    /// `two_root_document` already existed for the R5 pair above, and its two
    /// roots carry distinct payloads of distinct lengths.
    #[test]
    fn showing_the_second_root_leaves_the_first_roots_payload_cold() {
        load_bytes_showing(&two_root_document(0), ShownRoot::nth(1))
            .expect("the unshown first root's payload is never read");
    }

    /// **A [`ShownRoot`] is a document ordinal, and a load into an arena that
    /// already holds roots is where that stops being the arena's ordinal too.**
    ///
    /// The load appends — "the document's nodes are appended to whatever the
    /// arena already holds" is what `dashscene_core::load` promises — so with one
    /// root already present, this document's root 1 is the arena's root **2**.
    ///
    /// Before issue #943 the ordinal was handed to `Txn::show_root` unchanged,
    /// which indexed the *arena's* roots with it: the prefetch read this
    /// document's second payload and the traversal then confined itself to the
    /// arena's root 1 — this document's **first** root. The wrong artboard,
    /// solved, committed and painted, with no diagnostic — the failure mode P4
    /// forbids.
    ///
    /// **A node authored in code is the prefix, not a second `.dsb`.** Issue #943
    /// describes composing two documents, and on the owned `load_document` path
    /// that is exactly reachable; on *this* path it is not, because
    /// `Txn::use_mapped_pool` refuses an arena whose image table already holds
    /// rows — a table is owned or mapped and never both
    /// (`docs/decisions/assets-borrow-from-the-mapping.md` D1). An authored
    /// prefix adds no image rows, so it composes here, and it produces the same
    /// non-zero root offset the conversion has to survive.
    ///
    /// The assertion is over which node holds a rect row rather than over
    /// `shown_root()` alone, because the row is what a painter reads.
    #[test]
    fn a_load_into_an_arena_that_already_holds_roots_shows_the_documents_own_root() {
        let mut arena = dashscene_core::Arena::new();
        let mut txn = arena.open();
        txn.add_node(None, Some("authored"));
        txn.commit();

        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("fixture.dsb");
        std::fs::write(&path, two_root_document(0)).expect("the fixture writes");
        Document::map(&path)
            .expect("the fixture maps")
            .load(ShownRoot::nth(1), None, &mut arena)
            .expect("the unshown first root's payload is never read");

        let roots = arena.roots().to_vec();
        assert_eq!(
            roots.len(),
            3,
            "the authored root plus the document's two, which is the whole premise"
        );

        let scene = arena.committed();
        assert_eq!(
            scene.shown_root(),
            Some(roots[2]),
            "the load named its own document's root 1, which is the arena's root 2"
        );
        assert_eq!(
            scene.rect_index_of(roots[2]),
            Some(0),
            "and that root is the one the commit resolved a rect for"
        );
        assert_eq!(
            scene.rect_index_of(roots[1]),
            None,
            "the document's *first* root draws nothing: it is the artboard the ordinal \
             conflation would have shown instead"
        );
    }

    /// The same fixture with the **second** root's payload corrupted, shown, is
    /// refused.
    #[test]
    fn showing_the_second_root_refuses_its_own_corrupted_payload() {
        let error = load_bytes_showing(&two_root_document(1), ShownRoot::nth(1))
            .expect_err("a corrupted payload is refused");
        assert!(
            matches!(error, DesktopError::Payload(_)),
            "the second root's corrupted payload must be reported as a payload mismatch, not as \
             {error}"
        );
    }

    /// An ordinal past the document's last root is refused by name, with the
    /// count the embedder should have asked within.
    ///
    /// The two-root fixture has roots 0 and 1, so anything from 2 up is not
    /// there. A loader that clamped to the last root, or fell back to the
    /// first, would draw a picture the embedder did not ask for and report
    /// nothing — which is the failure P4's "never a silent drop" rules out one
    /// layer up.
    ///
    /// **Root 7, not root 2.** The obvious ordinal to ask for is one past the
    /// end, and it makes the two numbers equal — at which point transposing
    /// them where the error is *built* passes this test, the sibling tests and
    /// the two rendering tests in `crate`, which construct the variant by hand
    /// and so pin only the formatting. A review's mutation pass found exactly
    /// that. Seven against two separates them, and the uniform-fixture trap it
    /// closes is the one where the **arguments** are uniform rather than the
    /// data.
    #[test]
    fn an_ordinal_past_the_last_root_is_refused_by_name() {
        let error = load_bytes_showing(&two_root_document(0), ShownRoot::nth(7))
            .expect_err("a root that is not there is refused");
        assert!(
            matches!(
                error,
                DesktopError::NoSuchRoot {
                    ordinal: 7,
                    roots: 2,
                    ..
                }
            ),
            "expected a NoSuchRoot naming the ordinal asked for and the count the document \
             carries, in that order, got {error}"
        );
    }

    /// The shown root's payload is made resident, and a corrupted one is
    /// refused by name.
    ///
    /// This and `a_mapped_load_refuses_the_shown_roots_corrupted_payload` are
    /// the two that say the prefetch above is wired at all — the other over a
    /// two-root fixture, this one over a committed file. Every assertion that
    /// is not about a corrupted payload passes with the touch loop removed:
    /// `dashbuf::open` reads no payload, so a corrupted file opens, the load
    /// gate is about references rather than bytes, and `load_document_mapped`
    /// binds ranges without reading them. A loader that prefetched nothing
    /// would draw a corrupted picture in silence, which is exactly what moving
    /// verification off the reader risks.
    ///
    /// `v03-paint.dsb` is one of the two committed fixtures carrying a payload,
    /// and its single root draws it, so its whole asset table is the shown
    /// root's prefetch set.
    #[test]
    fn a_mapped_file_whose_shown_payload_is_corrupted_is_refused() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../goldens/dsb/v03-paint.dsb");
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

        let error = load_bytes(&bytes).expect_err("a corrupted payload is refused");
        assert!(
            matches!(error, DesktopError::Payload(_)),
            "expected a payload mismatch, got {error}"
        );
    }

    /// A mapped file that binds a **derived** payload is refused by name.
    ///
    /// The guard exists because story #596 would otherwise have turned a loud
    /// failure into a silently wrong picture: the owning path parses the
    /// payload's header and fails when a KTX2 arrives where the entry says
    /// `Png`, and this path reads no header at all. `v03-paint-hifi.dsb` is the
    /// one committed fixture that carries a derivation manifest, so it is the
    /// only fixture this can be shown with.
    #[test]
    fn a_mapped_file_that_binds_a_derived_payload_is_refused() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../goldens/dsb/v03-paint-hifi.dsb");
        let document = Document::map(&path).expect("the fixture maps");
        let mut arena = dashscene_core::Arena::new();

        let error = refusal(
            document.load(ShownRoot::FIRST, None, &mut arena),
            "a derived payload must be refused",
        );
        assert!(
            matches!(error, DesktopError::Derived { .. }),
            "expected a derived-payload refusal, got {error}"
        );
    }

    /// A path that does not exist is reported before anything is opened.
    #[test]
    fn a_missing_file_is_reported_with_its_path() {
        let error = refusal(
            Document::map("no/such/file.dsb"),
            "a missing file must not map",
        );
        assert!(
            error.to_string().contains("no/such/file.dsb"),
            "the failure must name the path it could not map, and said {error}"
        );
    }

    /// **What `None` costs, on the mapped path, stated as a measurement.**
    ///
    /// `None` is a supported argument and this test is not a tripwire — it
    /// cannot fail when text is wired in, because wiring text in means passing
    /// `Some`. What it pins is the *price* of the other choice: no glyph run
    /// reaches the painter, and the hug-sized text node measures as an empty
    /// leaf, so its siblings lay out around a box the design did not specify.
    /// Before story #863 that was every load's behaviour and nothing said so.
    ///
    /// **The document is not at fault, and the first assertion is what says
    /// so.** The text arrives and `Arena::text` reads it back; the solve is
    /// what drops it. A test that only counted glyph runs would pass equally
    /// well against a fixture carrying no text at all.
    ///
    /// That `Some` produces the opposite on this same path is asserted where a
    /// real cascade exists — `demo`'s
    /// `a_loaded_document_draws_text_on_both_paths_when_the_host_supplies_fonts`,
    /// which needs fonts and atlases this crate has no way to build.
    #[test]
    fn a_mapped_load_without_text_resources_draws_no_glyphs_and_collapses_its_text() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../goldens/dsb/v07-text-hug-in-fill.dsb"
        ))
        .expect("the committed text fixture is present");
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("text.dsb");
        std::fs::write(&path, &bytes).expect("the fixture writes");

        let document = Document::map(&path).expect("the fixture maps");
        let mut arena = dashscene_core::Arena::new();
        document
            .load(ShownRoot::FIRST, None, &mut arena)
            .expect("the fixture loads");

        let scene = arena.committed();
        let text_row = (0..scene.rects().len() as u32)
            .find(|&row| arena.text(scene.node_of(row)).is_some())
            .expect("the fixture carries a text node");
        assert_eq!(
            arena.text(scene.node_of(text_row)),
            Some("hug inside fill"),
            "the document carries the text, so the solve is what drops it rather than the load"
        );
        assert!(
            scene.glyphs().runs().is_empty(),
            "no glyph run reaches a painter, because the solver this path builds has no atlas set"
        );
        let rect = scene.rects()[text_row as usize];
        assert_eq!(
            (rect.w, rect.h),
            (0.0, 0.0),
            "and the hug-sized text node measured as an empty leaf, so its siblings lay out \
             around a box the design did not specify"
        );
    }
}
