//! The startup-scaling criterion — the falsifiable form of R5 (story #598,
//! epic #594, guardrail G-20).
//!
//! R5 says cold-start cost is "proportional to what is shown, not to file size
//! (mmap + section discipline)", and `docs/specification/05-qualification.md`
//! makes this the **first v1 exit criterion**: "A scaling benchmark with a
//! small-root document and a many-frame corpus document asserts that cold-start
//! cost tracks the shown root, not the document size." Nothing had ever
//! measured it.
//!
//! `docs/decisions/startup-scaling-is-measured-by-a-counter.md` settles what is
//! measured and what it is measured over. The short form, with each decision
//! named where this file depends on it:
//!
//! - **D1 — cost is a count of bytes, not an elapsed time.** So there is no
//!   benchmark framework here, and no timing is asserted on. A byte count is
//!   exact and identical on every machine, where a timing ratio needs a
//!   threshold that drifts and cannot run on the two-core CI runners without
//!   flaking.
//! - **D2 — both reads and copies count.** `dashbuf::open_verified_with_cost`
//!   records the hash of each asset payload it resolves;
//!   `load_document_bound_with_cost` records the loader's copy out of it. Both
//!   happen to every payload on this path, and each alone makes cold start
//!   scale with file size.
//! - **D3 — the boundary is the load path, not the frame.** [`load_cost`] runs
//!   exactly the three steps `demo/src/document.rs` runs — open, the load gate,
//!   the replay into a committed arena — and stops. Nothing here selects a
//!   painter, so no painter's internal copies reach the number.
//! - **D4 — both documents show the same root, and the assertion is equality.**
//!   The ratio is reported, derived from the two counts.
//! - **D5 — the many-frame document is generated when the benchmark runs**,
//!   from a `dashc_wasm::Document` built in code, with payloads from
//!   `corpus/photo`. Nothing multi-megabyte enters git.
//! - **D6 — wall-clock and the machine are recorded and asserted on nothing.**
//!   [`report`] prints both; no assertion reads either.
//! - **D7 — it is demonstrated failing at the base commit**, not asserted to
//!   fail.
//!
//! # This test is expected to fail until stories #595, #596 and #597 land
//!
//! That is the point of writing it first, and epic #594's definition of done
//! requires the failure to be demonstrated by running it. A benchmark that has
//! only ever been seen passing is the `t2-check-has-no-teeth` shape v0.13 spent
//! an entire tier removing.
//!
//! It is therefore held out of the `sanity` and `regression` tiers and run by
//! name through `just scaling` (`.config/nextest.toml`, profile
//! `scaling`), so the gate `just build` runs stays green while the criterion is
//! openly red. When the slice closes it moves into `regression` like any other
//! test, and a regression in it fails a build.
//!
//! # Why the many-frame document's payloads are tiles and not repeats
//!
//! `dashc_wasm::Document::push_asset` deduplicates by content hash, so a
//! many-frame document that showed the same four corpus photos over and over
//! would compile to **four** asset entries and four blob sections, and would
//! read the same number of bytes as the small document. The criterion would
//! then pass at the base commit while measuring nothing — the uniform-fixture
//! trap in its plainest form. Every extra frame therefore carries a distinct
//! payload: a distinct 128x128 tile cut from a distinct place in a corpus
//! photo, re-encoded. [`the_many_frame_document_carries_one_payload_per_frame`]
//! is the guard that fails if that ever collapses.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dashbuf::cost::LoadCost;
use dashc_wasm::{Asset, AssetKind, Box2D, Document, Node, Paint, PaintEntry, compile};
use dashpaint::{FillSpec, ImageFill, ImageFormat, Mat23, ScaleMode};
use dashscene_core::{Arena, BoundPayload, load_document_bound_with_cost};

/// The photo the shown root displays, in both documents, byte for byte.
///
/// One of the four `corpus/photo` payloads, which are 512x512 crops of CC0
/// photographs (`corpus/photo/README.md`). It is the largest asset in either
/// document, so "the shown root's own payload" is a number that stands out from
/// the tiles around it.
const ROOT_PHOTO: &str = "corpus/photo/dawn-mountains.png";

/// The photos the many-frame document's tiles are cut from.
///
/// All four, so the extra frames are not one photograph's statistics repeated.
const TILE_PHOTOS: [&str; 4] = [
    "corpus/photo/interior-render.png",
    "corpus/photo/coast-forest.png",
    "corpus/photo/snowy-forest.png",
    "corpus/photo/dawn-mountains.png",
];

/// A tile's side, in pixels. Divides 512 exactly, so a 512x512 corpus photo
/// yields whole tiles with nothing left over.
const TILE: u32 = 128;

/// Frames the many-frame document carries beyond the shown root.
///
/// Sixty-four: sixteen tiles from each of the four photos, which is every tile
/// of every photo exactly once. Large enough that the failing ratio is
/// unmistakable rather than marginal, and small enough that the whole
/// measurement is a few seconds.
const EXTRA_FRAMES: usize = 64;

/// The repository root — two levels up from this crate (`goldens/tooling`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// A committed corpus payload, exactly as it sits in the tree.
fn corpus_payload(path: &str) -> Vec<u8> {
    std::fs::read(repo_root().join(path)).unwrap_or_else(|error| panic!("{path} reads: {error}"))
}

/// Decodes a corpus PNG to 8-bit RGBA.
///
/// The same decode `goldens/tooling/tests/derived_bank.rs` and
/// `crates/dashpack/tests/band_contract.rs` use, through the same crate.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("a readable PNG header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded frame")];
    let info = reader.next_frame(&mut buffer).expect("it decodes");
    buffer.truncate(info.buffer_size());
    let texels = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("a corpus photo is {other:?}; they are RGB or RGBA"),
    };
    (info.width, info.height, texels)
}

/// Encodes the [`TILE`]-sided tile whose top-left texel is `(x0, y0)` as a PNG.
///
/// The tile's pixels are the photograph's, so each one is a distinct payload
/// with a real photograph's entropy — a synthetic fill would compress to a few
/// hundred bytes and make the many-frame document smaller than the small one.
fn encode_tile(texels: &[u8], width: u32, x0: u32, y0: u32) -> Vec<u8> {
    let mut rows = Vec::with_capacity((TILE * TILE * 4) as usize);
    for y in y0..y0 + TILE {
        let start = ((y * width + x0) * 4) as usize;
        let end = start + (TILE * 4) as usize;
        rows.extend_from_slice(&texels[start..end]);
    }

    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, TILE, TILE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("the tile header writes");
    writer
        .write_image_data(&rows)
        .expect("the tile samples write");
    writer.finish().expect("the tile PNG finishes");
    out
}

/// The first `wanted` tiles of the photos in [`TILE_PHOTOS`], in a fixed order.
///
/// Each is a different part of a photograph, so no two share bytes and none
/// shares bytes with [`ROOT_PHOTO`].
fn tiles(wanted: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    if wanted == 0 {
        return out;
    }
    for path in TILE_PHOTOS {
        let (width, height, texels) = decode_png(&corpus_payload(path));
        for y0 in (0..height - height % TILE).step_by(TILE as usize) {
            for x0 in (0..width - width % TILE).step_by(TILE as usize) {
                out.push(encode_tile(&texels, width, x0, y0));
                if out.len() == wanted {
                    return out;
                }
            }
        }
    }
    panic!(
        "the corpus yields {} tiles of {TILE}px, and {wanted} frames were asked for",
        out.len()
    );
}

/// A root frame whose only paint is an image fill naming `asset`.
///
/// `parent: None` makes it a root, so a document with n of these is a document
/// with n top-level frames — the shape a Figma file with many artboards
/// compiles to, and the shape the criterion is stated over.
fn frame(name: &str, asset: u32, side: f32) -> Node {
    Node {
        name: Some(name.to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: side,
            height: side,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: Some(FillSpec::Image(ImageFill {
                    image: asset,
                    scale_mode: ScaleMode::Fill,
                    transform: Mat23::IDENTITY,
                    tile_scale: 1.0,
                })),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
                extra_fills: Vec::new(),
            },
            clip: false,
            shape_field: None,
            shadows: Vec::new(),
            blurs: Vec::new(),
        }),
        ..Node::default()
    }
}

/// A compiled `.dsb` carrying the shown root, plus `extra` further frames each
/// showing a distinct tile.
///
/// `extra == 0` is the small-root document and `extra == EXTRA_FRAMES` is the
/// many-frame one. One builder for both, so the shown root is the same subtree
/// in each by construction rather than by two definitions agreeing.
fn document(extra: usize) -> Vec<u8> {
    let root_payload = corpus_payload(ROOT_PHOTO);
    let (root_width, root_height, _) = decode_png(&root_payload);

    let mut doc = Document::new();
    let root_asset = doc.push_asset(Asset {
        format: ImageFormat::Png,
        kind: AssetKind::Image,
        bytes: root_payload,
        width: root_width,
        height: root_height,
    });
    doc.push(frame("shown-root", root_asset, root_width as f32));

    for (index, tile) in tiles(extra).into_iter().enumerate() {
        let asset = doc.push_asset(Asset {
            format: ImageFormat::Png,
            kind: AssetKind::Image,
            bytes: tile,
            width: TILE,
            height: TILE,
        });
        doc.push(frame(&format!("frame-{index}"), asset, TILE as f32));
    }

    compile(&doc).expect("the generated document compiles")
}

/// What one load of a `.dsb` cost, and how long it took.
struct Measured {
    cost: LoadCost,
    elapsed: std::time::Duration,
    /// Asset payload bytes the file carries, summed over its entries — the
    /// document's own size in assets, which is what R5 says the cost must
    /// *not* track.
    payload_bytes: u64,
}

/// Loads `file` and returns what it cost.
///
/// Three steps and no others: `dashbuf::open_verified` (the envelope, the
/// flatbuffers verifier and the payload binding), the referential load gate,
/// then the replay into a committed arena. D3 puts the boundary exactly here —
/// a painter's own copies are not a property of
/// loading.
fn load_cost(file: &[u8]) -> Measured {
    let cost = LoadCost::new();
    let started = Instant::now();

    let (document, payloads) =
        dashbuf::open_verified_with_cost(file, &cost).expect("the file opens");
    let report = dashscene_validator::validate_document(&document);
    assert!(
        !report.has_errors(),
        "the generated document loads: {report}"
    );
    let bound: Vec<BoundPayload<'_>> = payloads
        .iter()
        .map(|b| BoundPayload::canonical(b))
        .collect();
    let mut arena = Arena::new();
    load_document_bound_with_cost(&document, &bound, &mut arena, &cost);

    let elapsed = started.elapsed();
    let payload_bytes = payloads.iter().map(|p| p.len() as u64).sum();
    Measured {
        cost,
        elapsed,
        payload_bytes,
    }
}

/// Prints one document's numbers. D6: the wall clock and the machine are
/// recorded here and asserted on nowhere.
fn report(label: &str, measured: &Measured) {
    println!(
        "STARTUP SCALING — {label}: hashed {} B, copied {} B, total {} B \
         (the file's asset payloads are {} B); {:.1} ms on {} {}",
        measured.cost.hashed(),
        measured.cost.copied(),
        measured.cost.total(),
        measured.payload_bytes,
        measured.elapsed.as_secs_f64() * 1000.0,
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
}

/// The criterion. Showing one root must cost the same out of a one-frame
/// document and out of a sixty-five-frame one.
///
/// Two assertions, and they fail differently:
///
/// - The small document must read **at least** the shown root's own payload.
///   Without it, a load path that read nothing at all would satisfy the
///   equality below while making no asset resident, and the criterion would
///   pass by doing nothing. This is the assertion that keeps D3's boundary —
///   "a committed arena with the shown root's assets resident" — honest.
/// - The many-frame document must read **exactly** what the small one reads.
///   This is R5. It is what fails today, because this benchmark measures the
///   **owning** path: `dashbuf::open_verified` hashes every blob and
///   `load_document` copies every payload, both of them for every entry in the
///   file rather than for the frame being shown. Story #597 built the path that
///   is bounded by the shown root, and story #598's re-run is what moves this
///   measurement onto it
///   (`docs/decisions/verification-moves-from-open-to-touch.md` D9).
#[test]
fn cold_start_tracks_the_shown_root_not_the_document_size() {
    let small = load_cost(&document(0));
    let many = load_cost(&document(EXTRA_FRAMES));

    report("small-root document (1 frame)", &small);
    report(
        &format!("many-frame document ({} frames)", EXTRA_FRAMES + 1),
        &many,
    );
    println!(
        "STARTUP SCALING — ratio {:.2}x (criterion: 1.00x)",
        many.cost.total() as f64 / small.cost.total() as f64
    );

    let root_payload = corpus_payload(ROOT_PHOTO).len() as u64;
    assert!(
        small.cost.total() >= root_payload,
        "loading the small-root document read {} asset payload bytes, fewer than the shown root's \
         own payload ({root_payload} B): the shown root's asset was never made resident, so the \
         equality below would hold without anything being loaded",
        small.cost.total()
    );

    assert_eq!(
        many.cost.total(),
        small.cost.total(),
        "R5 (guardrail G-20): showing the same root cost {} asset payload bytes out of a \
         {}-frame document and {} out of a 1-frame one, a factor of {:.2}. Cold start tracks the \
         document's size rather than the shown root. This is epic #594's criterion and it is \
         expected to fail until stories #595, #596 and #597 land — see \
         docs/decisions/startup-scaling-is-measured-by-a-counter.md",
        many.cost.total(),
        EXTRA_FRAMES + 1,
        small.cost.total(),
        many.cost.total() as f64 / small.cost.total() as f64,
    );
}

/// What the pre-slice load path does to one payload: hashes it once, then
/// copies it once.
///
/// D2 counts both because each alone makes cold start scale with file size, so
/// a counter seeing only one cannot falsify the other — and the criterion above
/// cannot tell them apart, because dropping either recording scales both
/// documents equally and leaves the ratio unchanged. This is where each
/// recording site is pinned on its own: `dashbuf::open_verified_with_cost`
/// records a hash and no copy, and `load_document_bound_with_cost` records a
/// copy and no hash.
///
/// It is also the finding epic #594 was opened against, stated as a test: a
/// payload's bytes are read **twice** before anything is drawn, and the first
/// of those reads faults in every page of a file that was supposed to be
/// mapped. Stories #596 and #597 change both numbers, and this test is where
/// that change has to be made deliberately rather than silently.
#[test]
fn the_pre_slice_load_path_hashes_every_payload_and_copies_it_once() {
    let file = document(0);
    let root_payload = corpus_payload(ROOT_PHOTO).len() as u64;

    let opening = LoadCost::new();
    let (document, payloads) =
        dashbuf::open_verified_with_cost(&file, &opening).expect("the file opens");
    assert_eq!(
        opening.hashed(),
        root_payload,
        "opening the file hashes the whole payload, through blob_by_hash"
    );
    assert_eq!(opening.copied(), 0, "opening the file copies nothing");

    let loading = LoadCost::new();
    let bound: Vec<BoundPayload<'_>> = payloads
        .iter()
        .map(|b| BoundPayload::canonical(b))
        .collect();
    let mut arena = Arena::new();
    load_document_bound_with_cost(&document, &bound, &mut arena, &loading);
    assert_eq!(
        loading.copied(),
        root_payload,
        "the loader copies the whole payload into an owned ImageAsset"
    );
    assert_eq!(loading.hashed(), 0, "the loader hashes nothing");
}

/// The fixture guard: every extra frame must carry a payload of its own.
///
/// `Document::push_asset` deduplicates by content hash. If the tiles ever
/// stopped differing — one tile cut sixty-four times, a synthetic fill, a
/// stride bug that read the same rows — the many-frame document would compile
/// to one entry and the criterion above would pass while measuring nothing.
/// This fails first, and by name.
///
/// It also pins that the shown root's payload is byte-identical in the two
/// documents, which is what makes "the same root" true rather than assumed.
#[test]
fn the_many_frame_document_carries_one_payload_per_frame() {
    let small = document(0);
    let many = document(EXTRA_FRAMES);

    let (_, small_payloads) = dashbuf::open_verified(&small).expect("the small document opens");
    let (_, many_payloads) = dashbuf::open_verified(&many).expect("the many-frame document opens");

    assert_eq!(small_payloads.len(), 1, "the small document is one frame");
    assert_eq!(
        many_payloads.len(),
        EXTRA_FRAMES + 1,
        "the many-frame document must carry one distinct payload per frame: identical bytes \
         collapse to one asset entry, which would make the two documents the same size"
    );

    let mut seen: Vec<&[u8]> = Vec::new();
    for payload in &many_payloads {
        assert!(
            !seen.contains(payload),
            "two of the many-frame document's payloads are the same bytes"
        );
        seen.push(payload);
    }

    assert_eq!(
        small_payloads[0], many_payloads[0],
        "the shown root's payload must be the same bytes in both documents"
    );
}
