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
use dashscene_core::{
    Arena, AxisSizing, LayoutMode, NodeId, Prop, TextAlign, TextAlignV, TextStyle,
};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{
    AMBER, INK, NAVY, NEAR_WHITE, decode_golden, diff_vs, load_atlas, origin_of, size_of,
};

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

const HEADING: &str = "Hello dashscene";
const HEADING_SIZE: f32 = 28.0;
const CHIP: &str = "88 mph";
const CHIP_SIZE: f32 = 44.0;

/// The golden's absolute-pixel budget (issue #233).
///
/// This golden compared against 5 % of its canvas until #233. At 320x140 that
/// is 2,240 px, while the whole inked text is only 3,763 px: erasing all the
/// text failed, but either single string vanishing did not — the heading alone
/// is 1,823 px and the chip's string 1,943 px, both under 2,240. A canvas
/// fraction is the wrong model for sparse content
/// (`docs/decisions/golden-comparison-space.md`, "Text goldens"), so this scene
/// moves to an absolute count sized to the ink, not to the canvas.
///
/// 1,200 px is two thirds of the smallest regression it must catch (1,823 px),
/// rounded down. Measured on this scene, on this branch, every number the
/// golden's own compare reported:
///
/// - the healthy render differs from the committed golden by 3 px. This golden
///   is not bit-exact against a fresh render, unlike the v0.6 Arabic one, which
///   is exactly 0 — a small drift the 5 % fraction had room to hide;
/// - the whole text inks 3,763 px of the 44,800-px canvas (8.40 %);
/// - dropping the heading differs by 1,823 px, dropping the chip's string by
///   1,943 px, both above this budget — proven by
///   `dropping_either_string_exceeds_the_budget`.
///
/// It leaves ample room for cross-machine anti-aliasing jitter: the one
/// cross-architecture difference this project has measured is 32 px (the v0.3
/// paint golden, `docs/decisions/golden-comparison-space.md`), so 1,200 px is
/// about 37x it. Per inked pixel it is tighter than either committed text
/// budget it sits beside — 0.32 px of tolerance per inked pixel, against
/// v07-text-fallback's 0.34 (500 over 1,491) — which is the direction #233 asks
/// for.
const BUDGET: usize = 1_200;

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
        opacity: 1.0,
    }
}

/// Authors the Latin screen and commits it through the one typesetter,
/// returning the arena and the two text nodes. Shared by the golden, the
/// layout guard, and the budget's sensitivity guard so all three pin the same
/// scene.
///
/// A 320x140 navy backdrop (mode None: children place by their authored
/// offset). A plain white heading, and a hug-sized amber chip — the chip's
/// rounded box is its own fill, so the hug box the measure callback resolves is
/// visible behind the text.
fn author_scene(ts: &mut Typesetter) -> (Arena, NodeId, NodeId) {
    let mut arena = Arena::new();
    let nodes = {
        let mut solver = TaffySolver::with_typesetter(ts);
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
        txn.set_prop(heading, Prop::Text(HEADING.to_string()));
        txn.set_prop(
            heading,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: HEADING_SIZE,
                weight: 400,
                color: NEAR_WHITE,
                line_height_px: None,
                letter_spacing: 0.0,
                text_align: TextAlign::Left,
                text_align_v: TextAlignV::Top,
                ligatures_off: false,
            }),
        );

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
        txn.set_prop(chip, Prop::Text(CHIP.to_string()));
        txn.set_prop(
            chip,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans".to_string(),
                size: CHIP_SIZE,
                weight: 400,
                color: INK,
                line_height_px: None,
                letter_spacing: 0.0,
                text_align: TextAlign::Left,
                text_align_v: TextAlignV::Top,
                ligatures_off: false,
            }),
        );

        txn.commit_with(&mut solver);
        (heading, chip)
    };
    (arena, nodes.0, nodes.1)
}

/// Stages the scene's glyph runs at boundary B, keeping the heading's run only
/// when `keep_heading` and the chip's only when `keep_chip`. Dropping one is
/// what the sensitivity guard renders; the golden keeps both.
fn staged(
    ts: &mut Typesetter,
    arena: &Arena,
    heading: NodeId,
    chip: NodeId,
    keep_heading: bool,
    keep_chip: bool,
) -> GlyphRunTable {
    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(load_atlas(ATLAS_DIR));
    if keep_heading {
        glyphs.push_run(text_run(
            ts,
            atlas,
            origin_of(arena, heading),
            HEADING,
            HEADING_SIZE,
            NEAR_WHITE,
        ));
    }
    if keep_chip {
        glyphs.push_run(text_run(
            ts,
            atlas,
            origin_of(arena, chip),
            CHIP,
            CHIP_SIZE,
            INK,
        ));
    }
    glyphs
}

/// Paints the committed scene with `glyphs` and returns the PNG bytes.
fn render(arena: &Arena, glyphs: &GlyphRunTable) -> Vec<u8> {
    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        glyphs,
        None,
    );
    painter.png_bytes()
}

#[test]
fn latin_text_and_a_hug_label_match_their_golden() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let (arena, heading, chip) = author_scene(&mut ts);

    // The measure callback drove the hug sizing (#29): each box is the
    // shaped text's own size, not the authored (zero) default. Prove it by
    // matching the resolved box against the typesetter directly.
    let heading_expected = ts.layout(HEADING, HEADING_SIZE, None);
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
    let chip_expected = ts.layout(CHIP, CHIP_SIZE, None);
    let (cw, ch) = size_of(&arena, chip);
    assert!(
        (cw - chip_expected.width).abs() < 0.01 && cw > 1.0,
        "the chip hugged the shaped width ({cw} vs {})",
        chip_expected.width
    );
    assert!((ch - chip_expected.height).abs() < 0.01);

    let glyphs = staged(&mut ts, &arena, heading, chip, true, true);
    let png = render(&arena, &glyphs);

    // Sparse text on a large canvas, so an absolute-pixel budget — not a
    // canvas fraction — is the right tolerance: a fraction wide enough to clear
    // cross-machine MSDF edge jitter reaches past a single string's whole
    // footprint, so one label vanishing passes
    // (`docs/decisions/golden-comparison-space.md`, "Text goldens"). `BUDGET`
    // is calibrated on this scene so either string vanishing fails, proven by
    // `dropping_either_string_exceeds_the_budget`.
    goldens::assert_matches_golden_max_pixels("v05-text-latin", &png, BUDGET);
}

/// The budget's sensitivity guard (issue #233): a partial text regression —
/// one of the scene's two strings vanishing — must exceed the budget.
///
/// This is the case the 5 % canvas fraction was blind to, and the reason this
/// golden moved to an absolute count. Each broken render is diffed against the
/// committed golden, so the number this asserts is the one the golden's own
/// compare would see.
#[test]
fn dropping_either_string_exceeds_the_budget() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let (arena, heading, chip) = author_scene(&mut ts);
    let golden = decode_golden("v05-text-latin");

    let broken = |ts: &mut Typesetter, keep_heading: bool, keep_chip: bool| {
        let glyphs = staged(ts, &arena, heading, chip, keep_heading, keep_chip);
        diff_vs(&golden, &common::decode_rgba(&render(&arena, &glyphs)))
    };

    let no_heading = broken(&mut ts, false, true);
    let no_chip = broken(&mut ts, true, false);
    assert!(
        no_heading > BUDGET,
        "the heading vanishing must exceed the {BUDGET}px budget, differed by {no_heading}"
    );
    assert!(
        no_chip > BUDGET,
        "the chip's string vanishing must exceed the {BUDGET}px budget, \
         differed by {no_chip}"
    );
}

/// A layout-only sanity guard, independent of the golden image: the two
/// hug boxes do not overlap and both sit inside the backdrop, so the
/// picture the golden pins is the one the assertions describe.
#[test]
fn the_hug_boxes_are_laid_out_where_the_golden_expects() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let (arena, heading, chip) = author_scene(&mut ts);

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
