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
use dashbuf::residency::BlobResidency;
use dashlang::LiveScene;
use dashscene_core::{Arena, MappedPayload, Region};
use dashscene_engine::TaffySolver;

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

    /// Replays the document into `arena` and attaches a [`LiveScene`] to it.
    ///
    /// The extent is not a parameter: a loaded document already carries its own
    /// resolved canvas size (P1 — the document is intent, and a document
    /// compiled from a Figma capture has concrete geometry in that intent),
    /// unlike a scene built in code, which derives every offset from the
    /// drawable it is given. A resize therefore reloads the same picture rather
    /// than rescaling it, which is why [`crate::App::build`] is free to ignore
    /// the extent it is handed for this case.
    pub fn load(&self, arena: &mut Arena) -> Result<LiveScene, DesktopError> {
        let name = self.path.display().to_string();
        let bytes = self.file.bytes();

        // Reads the envelope, every structured section and the binding, and
        // stops at where each payload lies. No blob page is faulted in by this
        // call.
        let (document, wanted) = dashbuf::open(bytes).map_err(DesktopError::Open)?;

        let report = dashscene_validator::validate_document(&document);
        if report.has_errors() {
            return Err(DesktopError::Gate {
                path: name,
                report: format!("{report:?}"),
            });
        }

        // Bound as **canonical**, and refused when that would be a lie. A file
        // with a derivation manifest carries the rung a profile selected, not
        // the payload the document names, and binding it as canonical tags a
        // KTX2 as a `Png` — the mistake issue #640 exists to prevent. The
        // owning path finds that out by parsing the payload's header; this path
        // reads no header at all, by design, so nothing downstream would catch
        // it. This crate ships no profile and has no way to name a rung, so the
        // honest answer is to refuse the file rather than draw the wrong thing.
        let entries = document.assets().unwrap_or_default();
        for (want, entry) in wanted.iter().zip(entries.iter()) {
            if want.hash != entry.hash().bytes() {
                return Err(DesktopError::Derived { path: name });
            }
        }

        // The prefetch, and the whole of what this reads out of the file's cold
        // half: the assets the shown root's subtree draws, proven one at a
        // time. Everything else stays cold, which is what makes cold start
        // track the root being drawn rather than the file's size (R5, D4).
        //
        // **A row bound below whose payload was not touched is not proven.**
        // The image table takes one row per asset entry, so a many-frame
        // document's other frames are ranges into this mapping that nothing has
        // hashed. Nothing draws them — a frame whose payload is not ready needs
        // the placeholder field that has no producer, which stays in v1 (D6) —
        // and debt #779 carries the gap.
        let residency = BlobResidency::new();
        let shown = dashbuf::prefetch::first_root(&document)
            .ok_or_else(|| DesktopError::NoRoot { path: name.clone() })?;
        for index in dashbuf::prefetch::assets_of_root(&document, shown) {
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

        // The region the table points into is this same mapping, shared rather
        // than opened again: `MappedFile` is `Send + Sync` and satisfies
        // `Region` through its `AsRef<[u8]>`, so the handle costs one refcount
        // and no lifetime anywhere. This runs again on every rebuild, and each
        // run hands the table another reference to the one mapping.
        let region: Arc<dyn Region> = self.file.clone();
        dashscene_core::load_document_mapped(&document, region, &payloads, arena);
        Ok(dashlang::attach_live(arena, Box::new(TaffySolver::new())))
    }
}

#[cfg(test)]
mod tests {
    use super::Document;
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

    /// Maps `bytes` and loads them, keeping the temporary directory alive for
    /// the call.
    fn load_bytes(bytes: &[u8]) -> Result<(), DesktopError> {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("fixture.dsb");
        std::fs::write(&path, bytes).expect("the fixture writes");

        let document = Document::map(&path)?;
        let mut arena = dashscene_core::Arena::new();
        document.load(&mut arena).map(|_| ())
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
            document.load(&mut arena),
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
}
