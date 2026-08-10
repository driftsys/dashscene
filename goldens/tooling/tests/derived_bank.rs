//! The first derived bank: a real HiFi packing, assembled, loaded, and frozen
//! (story #434).
//!
//! `cold_bank_assembly.rs` is this file's other half. There, RAW is the
//! identity map and a reassembly must reproduce the committed file byte for
//! byte — nothing is derived, so nothing may move. Here the payloads are
//! packer output, so the file is a different file, and the question becomes
//! what stayed the same across the two.
//!
//! # What is asserted, and why each one is separate
//!
//! - **The HiFi file is frozen.** Byte-exact against a committed golden. What
//!   that is worth is measured below rather than asserted, because the first
//!   version of this comment overstated it.
//! - **The hot section did not move.** The document inside the HiFi file is the
//!   same bytes as the document inside the RAW golden, because an `AssetEntry`
//!   names a hash and never a section index
//!   (`docs/decisions/asset-model-content-addressed-blobs.md`). Recorded intent
//!   until #433 measured it against a stand-in bank; this measures it against a
//!   real packer.
//! - **It loads.** `dashbuf::open_verified` resolves each canonical hash through
//!   the
//!   derivation manifest to the KTX2 payload the packer produced. Assembling a
//!   file no reader can open is the failure mode
//!   `docs/decisions/derivation-manifest-section.md` exists to prevent, so the
//!   read side is asserted here and not assumed from the write side.
//!
//! # What the frozen file is actually worth, measured
//!
//! Story #431 predicted that a byte-exact fixture would be "the only thing"
//! that catches a silent compressor regression. Five mutations are recorded
//! against this golden and against `crates/dashpack/tests/band_contract.rs`,
//! whose recorded table pins each derived rung's KTX2 file. Issue #458
//! remeasured the first three and added the last two; the `digest` column of
//! that table arrived with the same issue, so "before" is what the recorded
//! table caught when it held lengths alone:
//!
//! | mutation | table, before | table, now | this golden |
//! | --- | --- | --- | --- |
//! | `ZSTD_LEVEL` 19 to 1 | caught | caught | caught |
//! | `PACK_QUALITY` Thorough to Fastest | caught | caught | **survives** |
//! | same-length byte change (`KTXorientation`) | **survives** | caught | caught |
//! | `Quality::Fastest` hard-coded at `pack`'s call site | caught | caught | **survives** |
//! | vendored `THOROUGH` mid-bandwidth tune limit 94 to 93 | caught | caught | **survives** |
//!
//! The fourth is the second regression arriving one line lower. It is measured
//! because two tests pin `PACK_QUALITY == Quality::Thorough` — one in
//! `band_contract.rs` and one in `profile_preview_weld.rs` — and both survive
//! it: pinning the constant is not the same as pinning what the encoder does.
//! The fifth is the same regression again, arriving inside the vendored encoder
//! rather than in our source, which is the shape a revendoring would take.
//!
//! So the prediction was half right, and the half that was wrong matters. The
//! recorded table catches a compressor regression too, because a level change
//! moves the length. What this golden still adds is that it is the sole check
//! over the *assembled file* — the section table, the manifest bytes, the page
//! alignment — none of which the packer's own tests touch. Byte identity over
//! the packer's KTX2 output is no longer unique to it: since #458 the recorded
//! table pins a BLAKE3 per derived row.
//!
//! # The limit of this fixture, stated rather than left to be discovered
//!
//! `v03-paint.dsb` is the only committed compiled document with an image, and
//! it has exactly one. Every asset index in it is 0, so **this file cannot fail
//! an ordering, deduplication, or wrong-index bug** — including one in the
//! manifest, whose rows would be in the right order with one row no matter what
//! the code did. `crates/dashbuf/tests/bank.rs` carries the multi-asset
//! manifest cases on hand-built documents for exactly that reason.
//!
//! The image is 16x16, and the reason the quality mutations survive here is
//! **not** the block count. Issue #458 measured it: at the 8x8 rung the packer
//! chooses, 16x16 is 2 by 2 blocks, and encoding it at `Quality::Thorough` and
//! at `Quality::Fastest` produces the same 64 bytes — 0 of 64 differ. The
//! earlier claim in this comment, that 16x16 is one block at every footprint,
//! was wrong: it is 16 blocks at 4x4, 9 at 6x6 and 4 at 8x8, 10x10 and 12x12.
//! What makes the asset effort-insensitive is its content. It is a small flat
//! figure, so every search effort converges on the same blocks at the rung that
//! ships. The two rungs where effort does change the bytes, 6x6 and 12x12, are
//! rejected by the band and never reach the file.
//!
//! That is why a golden over a merely *larger* asset was not the fix issue #458
//! settled on. Block count is not the property that matters; content complexity
//! is. `import-image-fill` at 380x380 has 4096 blocks at its 6x6 rung and 51570
//! of its 65536 payload bytes move between Thorough and Fastest, and it is
//! covered by digest in the recorded table at 64 hex characters rather than by
//! a second committed 21 KB payload.
//!
//! What this file carries that no hand-built document and no recorded number
//! can is a real packer's bytes over a real compiler's output, in the container
//! they actually ship in.

use std::path::{Path, PathBuf};

use dashbuf::AssetKind;
use dashpack::astc::Rgba8;
use dashpack::bank::{Asset, pack_bank};
use dashpack::profile::{Binding, Profile};

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The RAW golden this story derives from, and the HiFi golden it produces.
const RAW_GOLDEN: &str = "goldens/dsb/v03-paint.dsb";
const HIFI_GOLDEN: &str = "goldens/dsb/v03-paint-hifi.dsb";

/// Decodes a canonical PNG payload to the 8-bit RGBA the packer measures in.
///
/// The same decode `crates/dashpack/tests/band_contract.rs` uses, through the
/// same crate. It matters that it is the same one: the golden below is
/// byte-exact over the encoder's output, and the encoder's input is whatever
/// this returns.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .expect("the canonical payload has a readable PNG header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded frame")];
    let info = reader.next_frame(&mut buffer).expect("it decodes");
    buffer.truncate(info.buffer_size());
    let texels = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("the canonical payload is {other:?}; the corpus images are RGB or RGBA"),
    };
    (info.width, info.height, texels)
}

/// The RAW golden's ui section and its canonical payloads, with each payload
/// decoded ready for the packer.
struct Corpus {
    file: Vec<u8>,
    ui: Vec<u8>,
    canonical: Vec<Vec<u8>>,
    kinds: Vec<AssetKind>,
    decoded: Vec<(u32, u32, Vec<u8>)>,
}

fn corpus() -> Corpus {
    let file = std::fs::read(root().join(RAW_GOLDEN)).expect("the RAW golden is readable");
    let (document, payloads) = dashbuf::open_verified(&file).expect("the RAW golden opens");
    let ui = dashbuf::container::ui_document(&file)
        .expect("a ui section")
        .to_vec();

    let entries = document.assets().expect("v03-paint carries an asset table");
    assert!(
        !entries.is_empty(),
        "the fixture chosen for this test has no assets, so it would pack nothing",
    );

    let kinds = entries.iter().map(|entry| entry.kind()).collect();
    let canonical: Vec<Vec<u8>> = payloads.iter().map(|p| p.to_vec()).collect();
    let decoded = canonical.iter().map(|p| decode_png(p)).collect();
    Corpus {
        file,
        ui,
        canonical,
        kinds,
        decoded,
    }
}

impl Corpus {
    /// The assets, in entry order, as the packer takes them.
    fn assets(&self) -> Vec<Asset<'_>> {
        self.canonical
            .iter()
            .zip(&self.kinds)
            .zip(&self.decoded)
            .map(|((canonical, &kind), (width, height, texels))| Asset {
                canonical,
                kind,
                image: Rgba8::new(*width, *height, texels).expect("a decoded canonical payload"),
            })
            .collect()
    }
}

#[test]
fn a_hifi_bank_assembles_to_its_golden() {
    let corpus = corpus();
    let bank = pack_bank(Profile::HiFi, &corpus.assets()).expect("every asset packs under HiFi");
    let file = bank
        .assemble(&corpus.ui)
        .expect("the document and its HiFi bank assemble");

    let path = root().join(HIFI_GOLDEN);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::write(&path, &file).expect("the golden is writable");
        return;
    }
    let golden = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p goldens --test derived_bank` \
             to create it",
            path.display(),
        )
    });
    assert_eq!(
        file, golden,
        "the HiFi assembly of {RAW_GOLDEN} drifted. Unlike a RAW reassembly this file is \
         allowed to change — but only for a named cause: an encoder or compressor change, a \
         band change, or a container-layout change. Identify which before regenerating with \
         UPDATE_GOLDENS=1.",
    );
}

#[test]
fn the_hifi_file_loads_and_resolves_to_the_packed_payloads() {
    let corpus = corpus();
    let bank = pack_bank(Profile::HiFi, &corpus.assets()).expect("packs");
    let file = bank.assemble(&corpus.ui).expect("assembles");

    let (document, resident) = dashbuf::open_verified(&file).expect("the HiFi file opens");
    assert_eq!(
        document.assets().expect("assets").len(),
        corpus.canonical.len(),
    );

    // Every payload came back, and it is the packer's derivation rather than
    // the canonical bytes — which is the whole difference from a RAW file, and
    // the thing `blob_by_hash` alone could not have resolved.
    for (index, asset) in bank.assets.iter().enumerate() {
        assert_eq!(
            resident[index],
            asset.resident(),
            "asset {index} resolves to the payload HiFi bound it to",
        );
        assert_ne!(
            resident[index], corpus.canonical[index],
            "asset {index} resolved to its canonical bytes, so this file is not derived \
             and proves nothing about the manifest",
        );
        assert!(
            matches!(asset.binding, Binding::Derived(_)),
            "asset {index} is not derived under HiFi",
        );
    }
}

#[test]
fn the_hot_section_survives_a_real_packing() {
    // The property `docs/decisions/asset-model-content-addressed-blobs.md`
    // recorded and #433 could only measure against a stand-in bank. The
    // document is an *input* to assembly, so a packer that changed every cold
    // byte may not change one hot byte.
    let corpus = corpus();
    let bank = pack_bank(Profile::HiFi, &corpus.assets()).expect("packs");
    let file = bank.assemble(&corpus.ui).expect("assembles");

    assert_ne!(
        file, corpus.file,
        "the HiFi file equals the RAW golden, so nothing was derived",
    );
    assert_eq!(
        dashbuf::container::ui_document(&file).expect("a ui section"),
        dashbuf::container::ui_document(&corpus.file).expect("a ui section"),
        "the document is byte-identical across the RAW and HiFi assemblies",
    );
}

#[test]
fn the_manifest_is_what_makes_the_hifi_file_resolvable() {
    // The negative half of the story, and the reason a manifest exists at all:
    // strip it and the same file stops loading. Without this, every assertion
    // above would still pass if `open` had quietly fallen back to matching
    // canonical hashes against section hashes.
    let corpus = corpus();
    let bank = pack_bank(Profile::HiFi, &corpus.assets()).expect("packs");
    let file = bank.assemble(&corpus.ui).expect("assembles");

    let container = dashbuf::container::Container::parse(&file).expect("parses");
    assert!(
        container
            .bindings_manifest()
            .expect("a well-formed file")
            .is_some(),
        "a HiFi file carries a derivation manifest",
    );

    // Reassembling the same derived payloads as if they were canonical is what
    // a manifest-free writer would have produced.
    let without = dashbuf::bank::assemble(
        &corpus.ui,
        &dashbuf::bank::ColdBank::raw(bank.assets.iter().map(|a| a.resident())),
    );
    match without {
        // The payloads are not their own preimage, so no entry names them: the
        // writer refuses before a reader ever sees the file.
        Err(dashbuf::bank::AssembleError::Unbound { .. }) => {}
        other => panic!(
            "binding derived payloads to their own hashes should be unassemblable \
             against this document, got {other:?}",
        ),
    }
}

#[test]
fn the_packer_report_measures_what_the_profile_cost() {
    // The size analysis story #434 asks for, as an assertion rather than only
    // as printed output. The numbers themselves are recorded in
    // `docs/technotes/hifi-bank-size.md`.
    let corpus = corpus();
    let bank = pack_bank(Profile::HiFi, &corpus.assets()).expect("packs");
    let report = bank.report();

    assert_eq!(
        report.canonical_bytes(),
        corpus.canonical.iter().map(Vec::len).sum::<usize>()
    );
    assert_eq!(
        report.resident_bytes(),
        bank.assets
            .iter()
            .map(|a| a.resident().len())
            .sum::<usize>(),
    );
    // v03-paint's image is 256 texels in a 93-byte PNG, and packs to 4 blocks at
    // the 8x8 rung: 64 bytes of blocks, which Zstd stores in 33, under 216 bytes
    // of KTX2 framing. HiFi therefore makes it *larger*, because the framing has
    // a floor a 93-byte PNG does not. That is a real property of small assets
    // and not a defect — asserted so that a change which made it accidentally
    // smaller is noticed and explained rather than welcomed.
    assert!(
        report.resident_bytes() > report.canonical_bytes(),
        "HiFi is expected to cost more than canonical on this fixture, whose one image is \
         smaller than a single compressed block plus its container framing; got {} against {}",
        report.resident_bytes(),
        report.canonical_bytes(),
    );
    println!("{report}");
}
