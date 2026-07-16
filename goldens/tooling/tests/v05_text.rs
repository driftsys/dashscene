//! The v0.5 Latin text golden (issue #30; `DESIGN_1.md` §7.2): static
//! Latin labels — including a hug-sized chip whose box is sized by the
//! measure callback (#29) — rendered as textured MSDF atlas quads through
//! the Skia reference painter.
//!
//! The whole path is exercised end-to-end: `dashscene-core` authors the
//! text nodes; `dashscene-engine`'s `TaffySolver::with_typesetter`
//! measures each hug node against the one `Typesetter`, so the committed
//! box is the shaped text's own size (#29); the same typesetter then
//! stages the positioned glyph runs at boundary B; and the painter draws
//! each glyph as one MSDF quad, sampled from the committed ASCII atlas
//! (#27) and resolved with the MSDF median-distance shader.
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): the font is
//! the committed corpus Noto Sans; the atlas is the committed,
//! R7-reproducible ASCII fixture (`corpus/atlas/ascii`) — no
//! `msdf-atlas-gen` at render time; shaping and line breaking are
//! deterministic. MSDF resolve is anti-aliased, so — like the gradient
//! goldens — the comparison is tolerance-based, not bit-exact.
//!
//! Regeneration and diff workflow: goldens/README.md.

use dashpaint::{AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, AxisSizing, LayoutMode, Prop, TextStyle};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{AMBER, INK, NAVY, NEAR_WHITE, load_atlas, origin_of, size_of};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// The committed, R7-reproducible ASCII atlas — the same font as `FONT`,
/// under the shared `corpus/atlas/` home (not a crate's private test
/// tree — debt #217). Reused rather than regenerated so the golden needs
/// no build tool, and so one atlas fixture stays the single reproducible
/// source of truth.
const ATLAS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");

/// Shapes `text` at `size` (single line — the labels are hug-sized) and
/// places every glyph in absolute document space by adding the node's
/// resolved box origin. The painter never moves anything (P2): the
/// positions cross boundary B already absolute.
fn text_run(
    ts: &mut Typesetter,
    atlas: AtlasIndex,
    origin: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
) -> GlyphRun {
    let laid = ts.layout(text, size, None);
    let mut glyphs = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            glyphs.push(GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + g.y,
            });
        }
    }
    GlyphRun {
        atlas,
        size,
        color,
        glyphs,
    }
}

#[test]
fn latin_text_and_a_hug_label_match_their_golden() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);

    let mut arena = Arena::new();

    // Author a 320x140 navy backdrop (mode None: children place by their
    // authored offset). A plain white heading, and a hug-sized amber chip
    // — the chip's rounded box is its own fill, so the hug box the measure
    // callback resolves is visible behind the text.
    let (heading, chip, chip_text) = {
        let mut solver = TaffySolver::with_typesetter(&mut ts);
        let mut txn = arena.open();

        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(320.0));
        txn.set_prop(root, Prop::Height(140.0));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));

        let heading = txn.add_node(Some(root), Some("heading"));
        txn.set_prop(heading, Prop::X(20.0));
        txn.set_prop(heading, Prop::Y(22.0));
        txn.set_prop(heading, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(heading, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(heading, Prop::Text("Hello dashscene".to_string()));
        txn.set_prop(
            heading,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: 28.0,
                weight: 400,
                color: NEAR_WHITE,
            }),
        );

        let chip_text = "88 mph";
        let chip = txn.add_node(Some(root), Some("chip"));
        txn.set_prop(chip, Prop::X(20.0));
        txn.set_prop(chip, Prop::Y(76.0));
        txn.set_prop(chip, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(chip, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(chip, Prop::Fill(AMBER));
        txn.set_prop(
            chip,
            Prop::Corners {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        txn.set_prop(chip, Prop::Text(chip_text.to_string()));
        txn.set_prop(
            chip,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: 44.0,
                weight: 400,
                color: INK,
            }),
        );

        txn.commit_with(&mut solver);
        (heading, chip, chip_text)
    };

    // The measure callback drove the hug sizing (#29): each box is the
    // shaped text's own size, not the authored (zero) default. Prove it by
    // matching the resolved box against the typesetter directly.
    let heading_expected = ts.layout("Hello dashscene", 28.0, None);
    let (hw, hh) = size_of(&arena, heading);
    assert!(
        (hw - heading_expected.width).abs() < 0.01 && hw > 1.0,
        "the heading hugged the shaped width ({hw} vs {})",
        heading_expected.width
    );
    assert!(
        (hh - heading_expected.height).abs() < 0.01 && hh > 1.0,
        "the heading hugged the shaped height ({hh} vs {})",
        heading_expected.height
    );
    let chip_expected = ts.layout(chip_text, 44.0, None);
    let (cw, ch) = size_of(&arena, chip);
    assert!(
        (cw - chip_expected.width).abs() < 0.01 && cw > 1.0,
        "the chip hugged the shaped width ({cw} vs {})",
        chip_expected.width
    );
    assert!((ch - chip_expected.height).abs() < 0.01);

    // Stage the positioned glyph runs at boundary B, sampling the atlas.
    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(load_atlas(ATLAS_DIR));
    glyphs.push_run(text_run(
        &mut ts,
        atlas,
        origin_of(&arena, heading),
        "Hello dashscene",
        28.0,
        NEAR_WHITE,
    ));
    glyphs.push_run(text_run(
        &mut ts,
        atlas,
        origin_of(&arena, chip),
        chip_text,
        44.0,
        INK,
    ));

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &glyphs,
        None,
    );

    // MSDF resolve is anti-aliased at every glyph edge, so cross-machine
    // coverage rounding shifts a fraction of edge pixels (the same effect
    // the gradient goldens absorb with a 1% budget, but text is denser in
    // edges). A 5% budget clears that headroom while staying far below any
    // real regression — a dropped or shifted glyph moves whole glyph
    // areas, several percent each.
    goldens::assert_matches_golden_within("v05-text-latin", &painter.png_bytes(), 0.05);
}

/// A layout-only sanity guard, independent of the golden image: the two
/// hug boxes do not overlap and both sit inside the backdrop, so the
/// picture the golden pins is the one the assertions describe.
#[test]
fn the_hug_boxes_are_laid_out_where_the_golden_expects() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();

    let (heading, chip) = {
        let mut solver = TaffySolver::with_typesetter(&mut ts);
        let mut txn = arena.open();
        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(320.0));
        txn.set_prop(root, Prop::Height(140.0));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));
        let heading = txn.add_node(Some(root), Some("heading"));
        txn.set_prop(heading, Prop::X(20.0));
        txn.set_prop(heading, Prop::Y(22.0));
        txn.set_prop(heading, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(heading, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(heading, Prop::Text("Hello dashscene".to_string()));
        txn.set_prop(
            heading,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: 28.0,
                weight: 400,
                color: NEAR_WHITE,
            }),
        );
        let chip = txn.add_node(Some(root), Some("chip"));
        txn.set_prop(chip, Prop::X(20.0));
        txn.set_prop(chip, Prop::Y(76.0));
        txn.set_prop(chip, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(chip, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(chip, Prop::Text("88 mph".to_string()));
        txn.set_prop(
            chip,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: 44.0,
                weight: 400,
                color: INK,
            }),
        );
        txn.commit_with(&mut solver);
        (heading, chip)
    };

    let (hx, hy) = origin_of(&arena, heading);
    assert_eq!((hx, hy), (20.0, 22.0), "heading placed at its offset");
    let (_, hh) = size_of(&arena, heading);
    let (cx, cy) = origin_of(&arena, chip);
    assert_eq!((cx, cy), (20.0, 76.0), "chip placed at its offset");
    let (cw, ch) = size_of(&arena, chip);
    assert!(hy + hh <= cy, "the heading sits above the chip");
    assert!(
        cx + cw <= 320.0 && cy + ch <= 140.0,
        "the chip fits the backdrop"
    );
}
