//! Shared fixtures for the golden test binaries under this directory
//! (debt #120): the scene colour palette and the boundary-B staging
//! helpers the text goldens share. Each test binary that declares
//! `mod common;` compiles its own copy, so a helper one binary does not
//! use is still used by another — hence the `dead_code` allowance.
//!
//! The cross-crate half of #120 (helpers shared with `dashscene-engine`'s
//! own tests) stays out: an integration-test crate cannot share code
//! without a dev-only crate, and `goldens/tooling/src/lib.rs` cannot host
//! scene helpers without taking `dashscene-core`/`dashscene-skia` as real
//! dependencies. This module fixes only the within-directory half.
//!
//! [`manifest`] is the second shared home: the design-source oracle harness
//! both oracle binaries walk their manifests with (debt #338). [`stress`] is
//! the third: the generated block-stress payload the profile-preview oracle
//! and the perceptual calibration both measure (issue #544). [`many_root`] is
//! the fourth: the sixty-five-root document the load criterion and the
//! per-frame criterion are both stated over (story #836).
//!
//! # Why the three generators stay declared here (issue #932)
//!
//! [`manifest`], [`many_root`] and [`stress`] are unrelated to each other, and
//! every binary declaring `mod common;` compiles all three whether it names
//! them or not. Eighteen binaries declare it and **eleven name none of the
//! three**. Issue #932 offered two shapes — a `#[path]` include per fixture in
//! only the binaries that use it, or a split into a sibling `fixtures/`
//! directory — and asked for the cost to be measured before either was
//! chosen, saying that under a second the honest answer is to write the reason
//! down here and close it.
//!
//! Measured 2026-08-16 on macOS aarch64, on an otherwise idle machine: the
//! serial (`-j1`) rebuild of exactly those eleven binaries, with the three
//! `pub mod` lines present and removed, seven runs each. **3.13 s against
//! 2.82 s at the medians, 3.10 s against 2.76 s at the minima — a difference
//! of about 0.31 s across all eleven**, or roughly 29 ms per binary.
//! Re-derive it by timing
//!
//!     cargo build -j1 -p goldens --test <each of the eleven>
//!
//! both ways, touching this file before each run so the binaries really
//! rebuild — without the `touch` cargo answers from cache and both sides
//! measure nothing. Take it on an idle machine: measured while another
//! checkout was running its own gates, the same comparison returned 4.2 to
//! 9.7 s and no usable difference.
//!
//! That is well under the threshold the issue set itself, so the idiom stays.
//!
//! **What the number is, stated carefully.** It is the whole marginal cost of
//! declaring the three modules in a binary — codegen *and* the link work that
//! follows from it, since with the three lines removed none of the eleven
//! still references `dashc_wasm` or `serde_json`/`goldens::oracle`. What it is
//! *not* is a saving in dependency compilation: those are dev-dependencies of
//! this package and are built once for it whichever shape this module takes.
//!
//! It also describes the tree as this branch leaves it, which is the tree the
//! decision applies to. [`decode_png`] below now keeps `png` in every binary
//! that declares `mod common;`, where before this branch `png` reached one
//! only through [`many_root`] — so the same comparison taken on the parent
//! commit would have found a slightly larger difference.
//!
//! **This measures binaries that already declare `mod common;`, and says
//! nothing about one newly declaring it.** That is a much larger number:
//! adding `mod common;` to `derived_bank.rs` — tried on this branch, for
//! issue #929, and reverted — took its serial rebuild from 0.43 s to 0.77 s
//! on its own, because it then links the whole golden harness rather than
//! `dashbuf` + `dashpack` + `png`. One binary joining costs about what all
//! eleven above pay together, and `derived_bank` is in the sanity tier that
//! runs before every commit. Issue #1090 carries that.

#![allow(dead_code)]

pub mod manifest;
pub mod many_root;
pub mod stress;

use dashpaint::{
    Atlas, AtlasGlyph, ClipIndex, ClipTable, Color, GlyphRange, GlyphRun, GlyphRunTable,
    ImageAsset, ImageFormat, ImageTable, PaintTable, Painter, RectEntry, Vec2,
};
use dashscene_core::{Arena, NodeId, TextAlign, TextAlignV, TextStyle};
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;

/// An opaque colour from its RGB channels.
pub const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

pub const NAVY: Color = rgb(0.05, 0.07, 0.12);
pub const PANEL: Color = rgb(0.12, 0.16, 0.24);
pub const NEAR_WHITE: Color = rgb(0.92, 0.94, 0.98);
pub const AMBER: Color = rgb(0.98, 0.78, 0.20);
pub const INK: Color = rgb(0.08, 0.09, 0.13);

/// Converts a committed build-time atlas fixture at `dir` into the
/// boundary-B [`Atlas`]: only glyphs that paint (bounded outlines) carry
/// a quad, so an empty-outline glyph (space) is dropped, and the
/// sorted-by-glyph-id order the metrics blob guarantees is preserved.
///
/// `dir` is a committed `corpus/atlas/<charset>` fixture; regenerate it
/// with the ignored `regenerate_committed_*_fixture` tests in
/// `crates/dashscene-typeset/tests/atlas_pipeline.rs`.
pub fn load_atlas(dir: &str) -> Atlas {
    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"));
    let m = &bundle.metrics;
    let glyphs = m
        .glyphs
        .iter()
        .filter_map(|g| {
            Some(AtlasGlyph {
                glyph_id: u32::from(g.glyph_id),
                plane_em: g.plane_em?,
                atlas_px: g.atlas_px?,
            })
        })
        .collect();
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            // Moved, not copied: the only borrow still outstanding is `m`, a
            // disjoint field of the same bundle (issue #967).
            bytes: bundle.image_png,
        },
        m.atlas.width,
        m.atlas.height,
        m.atlas.px_per_em,
        m.atlas.distance_range_px,
        glyphs,
    )
    .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"))
}

/// A 4×4 checker `ImageAsset`, rendered through the painter itself — the
/// shared image fixture the paint goldens fill with (debt #103). `dark` is
/// the darker of the two checker squares; the lighter square is fixed, so
/// each caller reproduces its own committed golden byte-for-byte (the two
/// former copies had diverged only on `dark`).
pub fn checker_asset(dark: Color) -> ImageAsset {
    let mut painter = SkiaPainter::new(4, 4);
    let mut paints = PaintTable::new();
    let dark = paints.push_solid(dark);
    let light = paints.push_solid(rgb(0.9, 0.85, 0.7));
    let mut rects = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            rects.push(RectEntry {
                x: x as f32,
                y: y as f32,
                w: 1.0,
                h: 1.0,
                paint: if (x + y) % 2 == 0 { dark } else { light },
                clip: ClipIndex::UNCLIPPED,
                opacity: 1.0,
                rotation: 0.0,
                rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
            });
        }
    }
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    ImageAsset {
        format: ImageFormat::Png,
        bytes: painter.png_bytes(),
    }
}

/// A `TextStyle` at `size`/`color` in `family`, with every other axis at its
/// authoring default: weight 400, no explicit line height, no letter
/// spacing, left/top alignment, ligatures on. This is the shape every text
/// golden scene authors repeatedly (debt #354); mirrors
/// `dashscene-engine`'s own `tests/measure.rs::styled()`, widened for
/// family and colour since the goldens mix fonts and inking colours where
/// the engine tests do not.
pub fn text_style(family: &str, size: f32, color: Color) -> TextStyle {
    TextStyle {
        family: family.to_string(),
        size,
        weight: 400,
        color,
        line_height_px: None,
        letter_spacing: 0.0,
        text_align: TextAlign::Left,
        text_align_v: TextAlignV::Top,
        ligatures_off: false,
    }
}

/// A copy of `table` keeping only the runs `keep` accepts, with the atlas set
/// — and therefore every `GlyphRun::atlas` index — unchanged.
///
/// This is how a text golden's sensitivity guard drops part of the scene's
/// ink now that runs come from commit rather than from the test: the scene is
/// staged once, for real, and the guard renders a subset of it. Previously
/// each guard re-staged by hand, which let the broken render drift from the
/// one the golden itself drew.
pub fn runs_where(table: &GlyphRunTable, keep: impl Fn(&GlyphRun) -> bool) -> GlyphRunTable {
    let mut out = GlyphRunTable::new();
    for atlas in table.atlases() {
        out.push_atlas(atlas.clone());
    }
    for run in table.runs().iter().filter(|r| keep(r)) {
        // The run's range names quads in `table`, not in `out`. Clearing it
        // is what says so: `push_run` refuses a range it did not assign, and
        // re-homing a run into another table is exactly the case that rule
        // exists for (story #578).
        let mut rehomed = *run;
        rehomed.glyphs = GlyphRange::UNASSIGNED;
        out.push_run(rehomed, table.quads(run));
    }
    out
}

/// A copy of `table` keeping only the runs anchored to one of `keep` — the
/// per-text-node case of [`runs_where`].
pub fn runs_anchored_to(table: &GlyphRunTable, keep: &[u32]) -> GlyphRunTable {
    runs_where(table, |run| keep.contains(&run.rect))
}

/// The rect-table index of a committed node — the value its glyph runs carry
/// as their anchor.
pub fn rect_index_of(arena: &Arena, node: NodeId) -> u32 {
    arena
        .committed()
        .rect_index_of(node)
        .expect("the node is committed")
}

/// The resolved box origin of a committed node.
pub fn origin_of(arena: &Arena, node: NodeId) -> (f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.x, rect.y)
}

/// The resolved box size of a committed node.
pub fn size_of(arena: &Arena, node: NodeId) -> (f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.w, rect.h)
}

/// Decodes a PNG to 8-bit RGBA through the `png` crate, widening an RGB
/// source to opaque RGBA.
///
/// `what` names the payload in each panic below, so a failure says which
/// input was bad. It reads as the subject of the sentence — "the canonical
/// payload has a readable PNG header" — so pass a noun phrase, not a path.
///
/// # Which decoder, and why it is not [`decode_rgba`]
///
/// The `png` crate, deliberately, and not Skia. `perceptual_calibration.rs`'s
/// scores are read beside `crates/dashpack/tests/band_contract.rs`'s band
/// fractions, which decodes the same way; a second decoder's disagreement
/// would sit inside every comparison, indistinguishable from codec error.
/// That is the reason this helper exists in this form, and it binds that
/// caller — `many_root`'s use is ordinary decoding and would tolerate either.
/// [`decode_rgba`] is not interchangeable with this: it goes through Skia to
/// land in the golden *comparison* space
/// (`docs/decisions/golden-comparison-space.md`), which is what a rendered
/// frame is compared in. This one is the packer's input space.
///
/// # Panics
///
/// Naming `what`: if `bytes` is not a readable PNG, if it does not fit in
/// memory, if it decodes as neither RGB nor RGBA, or if it decodes at other
/// than eight bits per sample.
///
/// That last check is here because no `png::Transformations` is set, so a
/// sixteen-bit source would arrive as `ColorType::Rgb` at six bytes per pixel
/// and the widening below would read `chunks_exact(3)` across sample halves —
/// wrong texels, no panic, and a caller slicing the oversized buffer stays in
/// bounds. Both copies this replaced had that hole. It is not a claim that
/// everything else is rejected: an 8-bit RGB source carrying `tRNS`
/// transparency still decodes here with its alpha forced opaque. A caller
/// needing either case sets `png::Transformations` itself, as
/// `goldens/tooling/tests/lean_painter_baked_assets.rs` does (issue #1090).
pub fn decode_png(bytes: &[u8], what: &str) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|e| panic!("{what} has a readable PNG header: {e}"));
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .unwrap_or_else(|| panic!("{what} fits in memory"))
    ];
    let info = reader
        .next_frame(&mut buffer)
        .unwrap_or_else(|e| panic!("{what} decodes: {e}"));
    // Colour type first, then depth. An indexed or grayscale source is
    // rejected for being indexed or grayscale, which is the useful thing to
    // say about it; checking depth first would answer a 4-bit palette with a
    // sentence about sample width.
    assert!(
        matches!(info.color_type, png::ColorType::Rgb | png::ColorType::Rgba),
        "{what} is {:?}; it must be RGB or RGBA",
        info.color_type
    );
    assert_eq!(
        info.bit_depth,
        png::BitDepth::Eight,
        "{what} decodes at {:?}; this helper reads one byte per sample and \
         sets no png::Transformations, so any other depth would be widened \
         across sample boundaries",
        info.bit_depth
    );
    buffer.truncate(info.buffer_size());
    let texels = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("{what} is {other:?}; it must be RGB or RGBA"),
    };
    (info.width, info.height, texels)
}

/// Decodes a PNG into unpremultiplied RGBA8888 — the golden comparison space
/// (`docs/decisions/golden-comparison-space.md`, the same space
/// `SkiaPainter::rgba_bytes` reads back). Shared by the text goldens'
/// sensitivity guards, which measure a differing-pixel count against a
/// deliberately broken render (#120: helpers shared across this directory's
/// binaries live here, not copied per file).
///
/// Not the decoder a canonical payload goes through — see [`decode_png`],
/// which states the split.
pub fn decode_rgba(png_bytes: &[u8]) -> Vec<u8> {
    use skia_safe::{AlphaType, ColorType, Data, ImageInfo, images};
    let img = images::deferred_from_encoded_data(Data::new_copy(png_bytes), None)
        .expect("the PNG decodes");
    let (w, h) = (img.width(), img.height());
    let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut px = vec![0u8; (w * h * 4) as usize];
    img.read_pixels(
        &info,
        &mut px,
        (w * 4) as usize,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    );
    px
}

/// The committed golden `{name}.png` decoded into the RGBA8888 comparison
/// space.
pub fn decode_golden(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../images")
        .join(format!("{name}.png"));
    decode_rgba(
        &std::fs::read(&path)
            .unwrap_or_else(|e| panic!("committed golden {} present: {e}", path.display())),
    )
}

/// The count of differing pixels between two RGBA8888 buffers.
pub fn diff_vs(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}
