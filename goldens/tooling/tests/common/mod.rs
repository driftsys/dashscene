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
//! and the perceptual calibration both measure (issue #544).

#![allow(dead_code)]

pub mod manifest;
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
            bytes: bundle.image_png.clone(),
        },
        m.atlas.width,
        m.atlas.height,
        m.atlas.px_per_em,
        m.atlas.distance_range_px,
        glyphs,
    )
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

/// Decodes a PNG into unpremultiplied RGBA8888 — the golden comparison space
/// (`docs/decisions/golden-comparison-space.md`, the same space
/// `SkiaPainter::rgba_bytes` reads back). Shared by the text goldens'
/// sensitivity guards, which measure a differing-pixel count against a
/// deliberately broken render (#120: helpers shared across this directory's
/// binaries live here, not copied per file).
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
