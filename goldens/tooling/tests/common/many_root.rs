//! The many-root document the two scaling criteria are stated over — one
//! builder, two measurements.
//!
//! It was `startup_scaling.rs`'s alone until story #836. That criterion
//! measures the **load**; the per-frame one in `per_frame_scaling.rs` measures
//! what a frame costs once the document is loaded. Both are stated over the
//! same shape — sixty-five top-level frames, the shape a Figma file with many
//! artboards compiles to — and story #836 was told to reuse this document
//! rather than author a second one, because two generators of "the many-root
//! document" would be two things that have to agree with nothing holding them
//! to it (the reason [`super::stress`] gives for its own move).
//!
//! Nothing here changed in the move. The numbers `startup_scaling.rs` records
//! are measured over the same bytes: [`document`] is the same function, and
//! `the_many_frame_document_carries_one_payload_per_frame` is what says so on
//! every run.
//!
//! # Why the extra frames carry tiles and not repeats
//!
//! `dashc_wasm::Document::push_asset` deduplicates by content hash, so a
//! many-frame document that showed the same four corpus photos over and over
//! would compile to **four** asset entries and four blob sections, and would
//! read the same number of bytes as the small document. The load criterion
//! would then pass at the base commit while measuring nothing — the
//! uniform-fixture trap in its plainest form. Every extra frame therefore
//! carries a distinct payload: a distinct 128x128 tile cut from a distinct
//! place in a corpus photo, re-encoded.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use dashc_wasm::{Asset, AssetKind, Box2D, Document, Node, Paint, PaintEntry, compile};
use dashpaint::{FillSpec, ImageFill, ImageFormat, Mat23, ScaleMode};

/// The photo the shown root displays, in both documents, byte for byte.
///
/// One of the four `corpus/photo` payloads, which are 512x512 crops of CC0
/// photographs (`corpus/photo/README.md`). It is the largest asset in either
/// document, so "the shown root's own payload" is a number that stands out from
/// the tiles around it.
pub const ROOT_PHOTO: &str = "corpus/photo/dawn-mountains.png";

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
///
/// It is also the ceiling: [`tiles`] panics past it, because the corpus yields
/// no sixty-fifth distinct tile. A measurement that wants a *different* root
/// count asks for fewer, never more (`per_frame_scaling.rs`'s scaling guard
/// does exactly that).
pub const EXTRA_FRAMES: usize = 64;

/// A committed corpus payload, exactly as it sits in the tree.
///
/// The repository root comes from [`super::manifest::repo_root`] rather than
/// from a second `CARGO_MANIFEST_DIR` walk of this module's own: one definition
/// of "two levels up from `goldens/tooling`" per module tree, so a directory
/// move has one place to be found.
pub fn corpus_payload(path: &str) -> Vec<u8> {
    std::fs::read(super::manifest::repo_root().join(path))
        .unwrap_or_else(|error| panic!("{path} reads: {error}"))
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
        let (width, height, texels) =
            super::decode_png(&corpus_payload(path), &format!("the corpus photo {path}"));
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
///
/// Every frame is a leaf, so the document's node count is `extra + 1` and each
/// root is one node — which is what lets `per_frame_scaling.rs` read a rect
/// table's row count as a root count.
///
/// # Memoised per size, and what that is worth (issue #930)
///
/// [`build`] decodes four 512x512 corpus photos and encodes `extra` 128x128
/// tiles, about 2.5 s at `EXTRA_FRAMES` on macOS aarch64. The two binaries
/// stated over this document ask for it **ten** times between them:
/// `per_frame_scaling.rs` five (two at `0`, three at `EXTRA_FRAMES`) and
/// `startup_scaling.rs` five (three at `0`, two at `EXTRA_FRAMES`). This turns
/// those ten builds into two per process.
///
/// **Re-derive rather than trust that split**, because it has already been
/// wrong twice in consecutive stories: it read four-and-five while story #944's
/// byte term had taken `per_frame_scaling.rs` to six, and issue #1119 then took
/// it to five.
///
/// ```text
/// grep -c "load(" goldens/tooling/tests/per_frame_scaling.rs
/// grep -cE "[^_]document\(" goldens/tooling/tests/startup_scaling.rs
/// ```
///
/// Both need the `fn load` definition and the comment lines discounted, which
/// is why the numbers above are stated as a split rather than as one total.
///
/// **What it buys is CPU, not elapsed time — measure before claiming
/// otherwise.** `cargo test` runs a binary's tests on parallel threads, so the
/// redundant builds were already overlapping on separate cores. Measured on
/// macOS aarch64, three runs each, `cargo test -p goldens --test <binary>`:
///
/// | binary              | wall before | wall after | CPU before | CPU after |
/// |---------------------|-------------|------------|------------|-----------|
/// | `per_frame_scaling` | 2.87 s      | 2.83 s     | 8.00 s     | 2.69 s    |
/// | `startup_scaling`   | 2.79 s      | 2.74 s     | 5.40 s     | 2.69 s    |
///
/// So the wall clock does not move on a machine with cores to spare, and
/// about 8 s of CPU across the two goes away. Elapsed time does move where
/// the builds cannot overlap: `per_frame_scaling` under `--test-threads=1`
/// went from 7.96 s to 2.81 s.
///
/// **The two callers are the CI steps that run these binaries under
/// `cargo test`**, one process per *binary*, in `.github/workflows/ci.yml`:
///
/// - `startup-scaling criterion (R5 / G-20)`
/// - `per-frame scaling criterion (#822's per-frame half)`
///
/// The tiers gain nothing: `just test`, `just test-regression` and
/// `just build` run under nextest, one process per test, and each test asks
/// for at most one document of each size. They pay a little instead — the
/// returned `Vec` is now a clone of the cached one rather than the buffer
/// `build` moved out, so a nextest process holds about 2 MB twice.
///
/// The cache is a map of per-size cells rather than one lock held across
/// [`build`], so a `document(0)` caller does not queue behind a 65-root build
/// it shares nothing with. Same-size callers do wait, which is the point.
///
/// Two consequences worth knowing when reading this file's numbers. A test
/// that waits on another test's build counts that wait in its own elapsed
/// time, and a test that arrives after the build counts none of it — so the
/// figure `per_frame_scaling.rs`'s criterion prints varies with scheduling
/// order under `cargo test`. Nothing asserts on it (D6). And under
/// `cargo test` the binary's tests now read one build's output rather than
/// each generating their own, so they no longer cross-check a
/// non-deterministic builder; nextest still builds independently, which is
/// where that guard now lives.
pub fn document(extra: usize) -> Vec<u8> {
    // The map's lock is released before `build` runs, so a panic in `build`
    // cannot poison it; `into_inner` is here only so an unrelated poisoning
    // could not turn every later call into a second failure.
    static BUILT: Mutex<BTreeMap<usize, Arc<OnceLock<Vec<u8>>>>> = Mutex::new(BTreeMap::new());

    let cell = {
        let mut built = BUILT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(built.entry(extra).or_default())
    };
    cell.get_or_init(|| build(extra)).clone()
}

/// Compiles the document [`document`] memoises.
fn build(extra: usize) -> Vec<u8> {
    let root_payload = corpus_payload(ROOT_PHOTO);
    let (root_width, root_height, _) = super::decode_png(&root_payload, "the shown root's photo");

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
