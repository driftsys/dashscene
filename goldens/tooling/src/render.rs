//! Load a committed `.dsb` and render it through the v0 Skia reference painter —
//! the public render entry point behind `just render` and the `render-dsb`
//! binary (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).
//!
//! This mirrors the test-only `render_fixture` in `tests/render_oracle.rs` (the
//! E7 design-source oracle) with one deliberate difference: it takes emitted
//! `.dsb` *bytes* directly rather than recompiling a fixture with an empty
//! images map, so an embedded image fill (`dashbuf` `Image { bytes }`) is present
//! in `scene.images()` and paints. The E7 oracle and its helpers are left
//! byte-identical (docs/wip design, E7 safety), so this module carries its own
//! copy of the font/atlas resource loaders rather than moving them out of the
//! live test file.

use dashbuf::root_as_document;
use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageAsset,
    ImageFormat, Painter,
};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, TextShape, Typesetter};

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);

/// The committed ASCII glyph-atlas fixture directory (Noto Sans, font 0).
pub const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
/// The committed Arabic glyph-atlas fixture directory (Noto Sans Arabic, font 1).
pub const ATLAS_ARABIC_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// The one coverage cascade every TEXT node is measured and staged through:
/// Noto Sans primary (font 0), Noto Sans Arabic fallback (font 1). The font
/// index a shaped glyph carries indexes both this cascade and the atlas list
/// built in the same order (`[ascii, arabic]`). A copy of the E7 oracle's
/// `oracle_typesetter`, kept here so the live oracle test file stays
/// byte-identical.
pub fn oracle_typesetter() -> Typesetter {
    let latin = Font::from_bytes(
        std::fs::read(FONT_LATIN).expect("corpus Latin font present"),
        0,
    )
    .expect("Noto Sans parses");
    let arabic = Font::from_bytes(
        std::fs::read(FONT_ARABIC).expect("corpus Arabic font present"),
        0,
    )
    .expect("Noto Sans Arabic parses");
    Typesetter::with_fonts(vec![latin, arabic])
}

/// Converts a committed build-time atlas fixture at `dir` into a boundary-B
/// [`Atlas`]: only glyphs that paint (bounded outlines) carry a quad, so an
/// empty-outline glyph (space) is dropped. A copy of the goldens `common`
/// helper, kept here so the E7 oracle test file stays byte-identical.
pub fn load_atlas(dir: &str) -> Atlas {
    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"));
    let m = &bundle.metrics;
    let glyphs = m
        .glyphs
        .iter()
        .filter_map(|g| {
            Some(AtlasGlyph {
                glyph_id: g.glyph_id,
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

/// The resolved box of a committed node: origin (x, y) and size (w, h).
fn box_of(arena: &Arena, node: NodeId) -> (f32, f32, f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.x, rect.y, rect.w, rect.h)
}

/// The `TextShape` for a node's text style (story #327, #341): the fixed line
/// height, letter spacing, horizontal alignment, and standard-ligatures-off
/// bit the stager lays the run out under.
fn text_shape(style: &dashscene_core::TextStyle) -> TextShape {
    TextShape {
        line_height_px: style.line_height_px,
        letter_spacing: style.letter_spacing,
        align: match style.text_align {
            dashscene_core::TextAlign::Left => dashscene_typeset::text::TextAlign::Left,
            dashscene_core::TextAlign::Center => dashscene_typeset::text::TextAlign::Center,
            dashscene_core::TextAlign::Right => dashscene_typeset::text::TextAlign::Right,
        },
        ligatures_off: style.ligatures_off,
    }
}

/// The stager's vertical alignment for a node's text style (story #327).
fn vertical_align(align: dashscene_core::TextAlignV) -> crate::VerticalAlign {
    match align {
        dashscene_core::TextAlignV::Top => crate::VerticalAlign::Top,
        dashscene_core::TextAlignV::Center => crate::VerticalAlign::Center,
        dashscene_core::TextAlignV::Bottom => crate::VerticalAlign::Bottom,
    }
}

/// Shapes `text` at `size` and places every glyph in absolute document space
/// (the painter moves nothing, P2: the node's box origin is added here),
/// splitting a new run wherever the cascade switched fonts so each run samples
/// the atlas of its own font. Unlike the E7 oracle's `text_runs` (default axes),
/// this honors the node's lowered text axes (story #327): it lays out under
/// `shape` (fixed line height, letter spacing, horizontal align) within the
/// resolved box width `box_size.0` — so horizontal alignment centers within the
/// box, and the line breaks match the box the engine measured — and offsets the
/// whole block down by the vertical alignment over `box_size.1`.
#[allow(clippy::too_many_arguments)]
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    origin: (f32, f32),
    box_size: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
    shape: TextShape,
    valign: crate::VerticalAlign,
) -> Vec<GlyphRun> {
    let (box_w, box_h) = box_size;
    let laid = ts.layout_with(text, size, Some(box_w), shape);
    // Vertical alignment is block placement, not paint (P2) and not a measured
    // extent (P1): shift every glyph down by the box's free space above the block.
    let voff = crate::vertical_offset(box_h, laid.height, valign);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + voff + g.y,
            };
            match runs.last_mut() {
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
                    atlas,
                    size,
                    color,
                    glyphs: vec![quad],
                    opacity: 1.0,
                }),
            }
        }
    }
    runs
}

/// Walks the committed arena and stages glyph runs for every TEXT node — one or
/// more runs per node, placed within the node's resolved box. A node is a text
/// leaf exactly when it carries both authored characters and a text style; its
/// style's `size`, `color`, and the lowered text axes (line height, letter
/// spacing, horizontal and vertical align) drive the run (story #327). Based on
/// the E7 oracle's `stage_text`, which stays on default axes.
fn stage_text(arena: &Arena, ts: &mut Typesetter, atlases: &[AtlasIndex]) -> Vec<GlyphRun> {
    fn walk(
        arena: &Arena,
        node: NodeId,
        ts: &mut Typesetter,
        atlases: &[AtlasIndex],
        out: &mut Vec<GlyphRun>,
    ) {
        if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
            let (x, y, w, h) = box_of(arena, node);
            out.extend(text_runs(
                ts,
                atlases,
                (x, y),
                (w, h),
                text,
                style.size,
                style.color,
                text_shape(style),
                vertical_align(style.text_align_v),
            ));
        }
        for &child in arena.children(node) {
            walk(arena, child, ts, atlases, out);
        }
    }
    let mut out = Vec::new();
    for &root in arena.roots() {
        walk(arena, root, ts, atlases, &mut out);
    }
    out
}

/// Loads a committed `.dsb`, re-solves it through the one typesetter-backed
/// `TaffySolver` (so TEXT nodes size to their shaped extent rather than
/// collapsing to 0x0), stages a glyph run for every TEXT node, and renders the
/// committed scene with the Skia reference painter — returning the PNG. The
/// canvas is sized to the root node's solved box (`scene.rects()[0]`).
///
/// Unlike the test-only `render_fixture`, this loads emitted `.dsb` bytes
/// directly, so embedded image-fill bytes the `.dsb` carries are present in
/// `scene.images()` and paint. Font resolution is the committed Noto cascade;
/// a Latin family the corpus does not provide renders in Noto Sans (a measured
/// fidelity gap, disclosed in `goldens/oracle/README.md`).
pub fn render_dsb(dsb: &[u8]) -> Vec<u8> {
    let document = root_as_document(dsb).expect("a valid .dsb buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit an empty transaction through a typesetter-backed solver
    // so a full solve runs the measure seam (the pattern the text goldens use).
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    // Stage glyph runs for every TEXT node. The atlases are pushed in the
    // cascade's font order (`[ascii, arabic]`), so the font index a shaped glyph
    // carries selects its atlas.
    let mut glyphs = GlyphRunTable::new();
    let ascii = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    for run in stage_text(&arena, &mut ts, &[ascii, arabic]) {
        glyphs.push_run(run);
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &glyphs,
        None,
    );
    painter.png_bytes()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dashc_wasm::compile_figma;
    use dashscene_validator::Profile;

    use super::render_dsb;

    /// A one-page Figma REST document whose root FRAME is `root`.
    fn document_json(root: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "document": {
                "name": "Document",
                "type": "DOCUMENT",
                "children": [{
                    "name": "Page 1",
                    "type": "CANVAS",
                    "children": [root],
                }],
            },
        })
    }

    #[test]
    fn the_stager_shifts_glyphs_for_center_and_vertical_center_alignment() {
        use dashpaint::{AtlasIndex, Color};
        use dashscene_typeset::text::{TextAlign, TextShape};

        use crate::VerticalAlign;

        use super::{oracle_typesetter, text_runs};

        let mut ts = oracle_typesetter();
        // The atlas index only tags a run; it does not affect placement, so a
        // bare index pair is enough (font 0 = Latin, font 1 = Arabic).
        let atlases = [AtlasIndex(0), AtlasIndex(1)];
        let text = "Hi";
        let size = 32.0;
        let black = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let origin = (0.0, 0.0);
        // A box much wider and taller than "Hi", so centering has slack.
        let box_size = (400.0, 200.0);

        let left = text_runs(
            &mut ts,
            &atlases,
            origin,
            box_size,
            text,
            size,
            black,
            TextShape::default(),
            VerticalAlign::Top,
        );
        let center = text_runs(
            &mut ts,
            &atlases,
            origin,
            box_size,
            text,
            size,
            black,
            TextShape {
                line_height_px: None,
                letter_spacing: 0.0,
                align: TextAlign::Center,
                ligatures_off: false,
            },
            VerticalAlign::Center,
        );

        let left_glyph = left[0].glyphs[0];
        let center_glyph = center[0].glyphs[0];
        assert!(
            center_glyph.x > left_glyph.x,
            "center alignment shifts the first glyph right within the box \
             (left {}, center {})",
            left_glyph.x,
            center_glyph.x
        );
        assert!(
            center_glyph.y > left_glyph.y,
            "vertical centering shifts the block down within the box \
             (left {}, center {})",
            left_glyph.y,
            center_glyph.y
        );
    }

    #[test]
    fn render_dsb_returns_a_png_of_the_root_box_size() {
        // A single 100x60 frame with one solid fill — no text, no image — is the
        // smallest scene that exercises load -> solve -> paint -> png. It has no
        // fixture on disk: it is compiled in-process into a `.dsb`, exactly the
        // bytes `render_dsb` consumes at runtime.
        let root = serde_json::json!({
            "name": "one-frame",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        });
        let json = document_json(root).to_string();
        let (dsb, report) = compile_figma(&json, Profile::Core, &BTreeMap::new())
            .expect("the one-frame fixture compiles");
        assert!(
            report.is_empty(),
            "the one-frame fixture lowers clean: {report}"
        );

        let png = render_dsb(&dsb);

        assert!(!png.is_empty(), "render_dsb returns non-empty PNG bytes");
        let data = skia_safe::Data::new_copy(&png);
        let image = skia_safe::images::deferred_from_encoded_data(data, None)
            .expect("the rendered bytes decode as a PNG");
        assert_eq!(
            (image.width(), image.height()),
            (100, 60),
            "the PNG is sized to the root frame's solved box"
        );
    }
}
