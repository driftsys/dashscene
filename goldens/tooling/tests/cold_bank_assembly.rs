//! A RAW assembly moves zero bytes (story #433).
//!
//! Every other golden `.dsb` assertion in this repo recompiles a Figma fixture
//! and compares the result with the committed bytes, so it measures the whole
//! compiler. This one measures only the assembly step, and it measures it
//! against the committed bytes themselves: take a golden apart into the ui
//! section and its payloads, put it back together through
//! `dashbuf::bank::assemble` under a RAW bank, and require the result to equal
//! the file it came from, byte for byte.
//!
//! The two checks answer different questions and one does not imply the other.
//! A compiler change that altered the lowering *and* an assembly change that
//! compensated for it would leave the recompile assertion green; this one takes
//! the committed ui section as given, so it cannot be compensated for. In the
//! other direction, this one would not notice a lowering change at all.
//!
//! # Why this is an equality and not a re-baseline
//!
//! RAW is the null binding — the identity map — so the resident payload *is*
//! the canonical payload (`docs/decisions/asset-model-content-addressed-blobs.md`).
//! There is nothing for a RAW assembly to derive, so there is nothing for it to
//! move. A failure here is a bug in the assembly path, not a golden that needs
//! regenerating, and it is the one assertion that says so directly.
//!
//! # What this test cannot catch
//!
//! Six of the seven goldens have no assets at all, and the seventh has exactly
//! one (`goldens/dsb/README.md`), so every asset index in the corpus is 0. This
//! test cannot fail an ordering, deduplication, or wrong-index bug — there is
//! no second asset for an index to be wrong about. `crates/dashbuf/tests/bank.rs`
//! carries the multi-asset cases on hand-built documents for that reason. What
//! this test does carry that no hand-built document can is the committed bytes.

use std::path::{Path, PathBuf};

use dashbuf::bank::{ColdBank, assemble};
use dashbuf::container::{Container, SectionKind};

/// Every committed golden `.dsb`, by path.
fn goldens() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dsb");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "dsb"))
        .collect();
    // Directory order is not defined, and a failure message that names a
    // different golden each run is harder to act on.
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no goldens found in {} — this test would pass vacuously",
        root.display(),
    );
    paths
}

#[test]
fn every_golden_reassembles_to_itself_under_a_raw_bank() {
    for path in goldens() {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let committed = std::fs::read(&path).expect("the golden is readable");

        // Take it apart. `open` runs the null binding: each asset entry's
        // canonical hash resolved to the blob section carrying it, verified.
        let (_, payloads) =
            dashbuf::open(&committed).unwrap_or_else(|e| panic!("{name} does not open: {e}"));
        let ui = dashbuf::container::ui_document(&committed).expect("a ui section");

        // Put it back together. The payloads are canonical by definition here —
        // they came out of a RAW file — so the identity map is the right bank.
        let reassembled = assemble(ui, &ColdBank::raw(payloads.iter().copied()))
            .unwrap_or_else(|e| panic!("{name} does not reassemble: {e}"));

        assert_eq!(
            reassembled.len(),
            committed.len(),
            "{name}: a RAW reassembly changed the file length, from {} to {} bytes. \
             RAW is the identity map, so there is nothing for it to move: this is an \
             assembly bug, not a golden to regenerate.",
            committed.len(),
            reassembled.len(),
        );
        let first_difference = reassembled.iter().zip(&committed).position(|(a, b)| a != b);
        assert_eq!(
            first_difference, None,
            "{name}: a RAW reassembly changed the bytes, first at offset {:?}. \
             RAW is the identity map, so there is nothing for it to move: this is an \
             assembly bug, not a golden to regenerate.",
            first_difference,
        );
    }
}

#[test]
fn the_corpus_still_holds_exactly_one_asset_bearing_golden() {
    // Two things at once, both cheap. It pins the section counts
    // `goldens/dsb/README.md` states, so a golden that silently grew or lost a
    // blob section is a failure rather than a documentation drift. And it is
    // the standing measurement behind the caveat in this file's header: the day
    // a second asset-bearing golden lands, this test fails, and whoever updates
    // it is told that the multi-asset gap it describes has just narrowed.
    let mut with_assets = Vec::new();
    for path in goldens() {
        let name = path
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(&path).expect("the golden is readable");
        let container = Container::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Counted by kind rather than as `len() - 1`, so a golden that grew a
        // second structured section is not miscounted as an asset.
        let blobs = container
            .sections()
            .filter(|entry| entry.kind == SectionKind::Blob as u16)
            .count();
        if blobs > 0 {
            with_assets.push((name, blobs));
        }
    }
    assert_eq!(
        with_assets,
        vec![("v03-paint.dsb".to_owned(), 1)],
        "the committed corpus's asset-bearing goldens changed. If a fixture gained an \
         image, update this expectation and the note in goldens/dsb/README.md.",
    );
}
