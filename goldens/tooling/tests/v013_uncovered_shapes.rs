//! v0.13 Stream A (#475): end-to-end frames for behaviours that had none.
//! Each scene here exists because a landed fix changed real output and **no
//! committed artifact moved** — the shape appeared nowhere in the corpus, so
//! the whole golden and oracle suite stayed green either way (issues #501,
//! #495).
//!
//! - `v013-hug-negative-margin` — a Hug row over a Hug child with a negative
//!   main-axis margin (issue #270).
//! - `v013-baseline-hug-cross` — a HUG cross-axis `Baseline` row holding text
//!   (issue #322).
//! - `v013-mask-effect-bleed` — a maskee whose drop shadow reaches past its own
//!   box, which is where the two readings of the G-7 mask-bounds ruling produce
//!   different pixels (issue #495, ruling confirmed by #287/PR #494).
//! - `v013-text-clipped` — a clipping frame narrower than the text inside it,
//!   so the clip actually cuts glyph ink (issue #275). The one committed scene
//!   with a clipped text node, `v07-text-lowering`, has ink that lies inside
//!   its clip box and renders the same either way.
//! - `v013-text-in-group` — text inside an overlapping partial-opacity group,
//!   so the run has to composite into the group's offscreen layer (issue
//!   #274). No committed scene carried a glyph run and a `GroupComposite` at
//!   all; the combination was a named paint-gate warning instead.
//!
//! Every scene is authored through `dashscene-core`'s producer API, solved by
//! `dashscene-engine`'s `TaffySolver` and painted by the reference painter, so
//! a regression has to survive the producer surface, the solver, commit-time
//! resolution and paint to stay green. It does not cover Figma lowering or the
//! `.dsb` round trip — that needs a captured fixture, and authoring one is a
//! manual Figma step (`corpus/figma-fixtures/README.md`).
//!
//! **Sensitivity is demonstrated, not assumed** (the discipline
//! `goldens/tooling/tests/v08_shadows.rs` established). Two of the three
//! scenes are dimensioned so that reverting the fix changes the *canvas size*,
//! which fails on the dimension check before any tolerance is consulted; the
//! third renders a deliberately broken twin and asserts the differing-pixel
//! count sits far above the golden's budget.

use dashpaint::{Color, GlyphRunTable, ImageTable, Painter, Shadow, ShadowKind, Vec2};
use dashscene_core::{
    Arena, AxisSizing, CrossAxisAlign, LayoutMode, NodeId, Prop, TextAlign, TextAlignV, TextStyle,
    Txn,
};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{AMBER, NAVY, NEAR_WHITE, PANEL, decode_golden, decode_rgba, diff_vs, load_atlas};

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

const RED: Color = rgb(0.80, 0.16, 0.16);
const GREEN: Color = rgb(0.15, 0.65, 0.30);
const BLUE: Color = rgb(0.20, 0.40, 0.90);
const SHADOW_INK: Color = rgb(0.60, 0.62, 0.70);
/// The `#322` row's own fill. Light enough against `NAVY` that a reviewer can
/// see where the row's bottom edge lands relative to the text's descender,
/// which is the whole subject of that frame.
const SLATE: Color = rgb(0.28, 0.33, 0.46);

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const ATLAS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");

/// A fixed-size filled child.
fn boxed(txn: &mut Txn<'_>, parent: Option<NodeId>, w: f32, h: f32, color: Color) -> NodeId {
    let id = txn.add_node(parent, None);
    txn.set_prop(id, Prop::Width(w));
    txn.set_prop(id, Prop::Height(h));
    txn.set_prop(id, Prop::Fill(color));
    id
}

/// A fixed-size filled child placed at an authored offset (a `None`-mode
/// parent).
fn placed(txn: &mut Txn<'_>, parent: Option<NodeId>, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    let id = txn.add_node(parent, None);
    txn.set_prop(id, Prop::X(x));
    txn.set_prop(id, Prop::Y(y));
    txn.set_prop(id, Prop::Width(w));
    txn.set_prop(id, Prop::Height(h));
    id
}

/// Rect `(x, y, w, h)` of a committed node, by identity (debt #119).
fn rect_of(arena: &Arena, node: NodeId) -> (f32, f32, f32, f32) {
    let scene = arena.committed();
    let i = scene
        .rect_index_of(node)
        .expect("the node is committed in this generation") as usize;
    let r = scene.rects()[i];
    (r.x, r.y, r.w, r.h)
}

/// Paints the committed scene on a canvas sized to the root's solved rect.
///
/// The canvas takes the ceiling of a fractional root size, so a scene whose
/// hug size is font-derived still renders whole. The size is derived from the
/// solve rather than authored, which is what makes a layout regression fail on
/// the golden's dimension check.
fn render(arena: &Arena, glyphs: &GlyphRunTable) -> Vec<u8> {
    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w.ceil() as i32, root.h.ceil() as i32);
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

// ---------------------------------------------------------------------
// Issue #270 — a Hug row over a Hug child with a negative main-axis margin.
// ---------------------------------------------------------------------

/// Builds the #270 scene: a Hug-width column of three Hug-width rows, each
/// holding a fixed box followed by a Hug container whose own content is one
/// fixed box, offset by `margin` on the main axis.
///
/// The rows carry the margins the #270 reproduction table names — `0` as the
/// control, then the two negative cases the rebate divides and multiplies
/// back: `-1` (a sub-unit overlap) and `-16` (a wide one). A fourth row pins
/// the other direction, that the fix stays out of the pass it does not repair;
/// see [`fixed_parent_guard_row`].
fn hug_negative_margin_scene(arena: &mut Arena) -> Vec<NodeId> {
    let mut txn = arena.open();
    let column = txn.add_node(None, Some("column"));
    txn.set_prop(column, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(column, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(column, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(column, Prop::Gap(8.0));
    txn.set_prop(column, Prop::Fill(NAVY));

    let mut rows = Vec::new();
    // The control row is deliberately the narrowest of the three, so the
    // column's own hug width is set by a negative-margin row. Reverting the
    // #270 fix collapses those rows and the column's width falls to the
    // control's — a canvas-size change, which no pixel tolerance can absorb.
    for (sibling_w, child_w, margin, fill) in [
        (32.0, 32.0, 0.0, RED),
        (56.0, 56.0, -1.0, GREEN),
        (56.0, 56.0, -16.0, BLUE),
    ] {
        let row = txn.add_node(Some(column), None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(row, Prop::Height(24.0));
        txn.set_prop(row, Prop::Fill(PANEL));

        boxed(&mut txn, Some(row), sibling_w, 24.0, AMBER);

        // The Hug child: no authored main size for the #236 rebate to fold
        // the negative margin into, which is the whole of #270.
        let hug = txn.add_node(Some(row), None);
        txn.set_prop(hug, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(hug, Prop::Height(24.0));
        txn.set_prop(
            hug,
            Prop::Margin {
                left: margin,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            },
        );
        boxed(&mut txn, Some(hug), child_w, 24.0, fill);
        rows.push(row);
    }
    rows.push(fixed_parent_guard_row(&mut txn, column));
    txn.commit_with(&mut TaffySolver::new());
    rows
}

/// The guard row: the same negative-margin Hug child under a **fixed-width**
/// parent, where the #270 shrink factor must not apply.
///
/// #270 maps a negative-margin Hug child at `flex_shrink = 1`, which is only
/// correct inside taffy's intrinsic pass — a fixed-width parent has definite
/// free space for a shrink factor to act on, and a Hug child is not a
/// shrinkable one (P5: Figma's hug does not shrink). The gate is what confines
/// the fix to the pass it repairs, and this row is the picture of it: the Hug
/// child is a `Wrap` container whose max-content width is 56 and whose
/// min-content width is 32, so a shrink factor that reached it would take it to
/// 40 and re-wrap it into two lines instead of letting it overflow the row.
fn fixed_parent_guard_row(txn: &mut Txn<'_>, column: NodeId) -> NodeId {
    let row = txn.add_node(Some(column), None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Width(80.0));
    txn.set_prop(row, Prop::Height(24.0));
    txn.set_prop(row, Prop::Fill(PANEL));

    boxed(txn, Some(row), 56.0, 24.0, AMBER);

    let hug = txn.add_node(Some(row), None);
    txn.set_prop(hug, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(
        hug,
        Prop::Margin {
            left: -16.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        },
    );
    boxed(txn, Some(hug), 24.0, 12.0, GREEN);
    boxed(txn, Some(hug), 32.0, 12.0, BLUE);
    row
}

#[test]
fn a_hug_row_over_a_negative_margin_hug_child_matches_its_golden() {
    let mut arena = Arena::new();
    let rows = hug_negative_margin_scene(&mut arena);

    // The #270 reproduction table, as solved geometry: sibling + child +
    // margin. A row that mis-sums shows up here before the image does.
    assert_eq!(rect_of(&arena, rows[0]).2, 64.0, "control: 32 + 32 + 0");
    assert_eq!(rect_of(&arena, rows[1]).2, 111.0, "56 + 56 - 1");
    assert_eq!(rect_of(&arena, rows[2]).2, 96.0, "56 + 56 - 16");
    assert_eq!(rect_of(&arena, rows[3]).2, 80.0, "the guard row is fixed");

    // The guard row's Hug child keeps its max-content width and overflows,
    // rather than shrinking into the 16 units of negative free space.
    let guard_child = arena.children(rows[3])[1];
    assert_eq!(
        rect_of(&arena, guard_child),
        (40.0, 96.0, 56.0, 12.0),
        "a Hug child under a fixed parent never shrinks: 56 wide on one wrap \
         line, overflowing the 80-wide row",
    );

    let column = arena.committed().rects()[0];
    assert_eq!(
        (column.w, column.h),
        (111.0, 120.0),
        "the column hugs the widest row (111) and four 24-tall rows over \
         three 8 gaps",
    );

    goldens::assert_matches_golden(
        "v013-hug-negative-margin",
        &render(&arena, &GlyphRunTable::new()),
    );
}

// ---------------------------------------------------------------------
// Issue #322 — a HUG cross-axis Baseline row holding text.
// ---------------------------------------------------------------------

const RUN: &str = "Ag";
const RUN_SIZE: f32 = 40.0;

fn text_style(color: Color) -> TextStyle {
    TextStyle {
        family: "Noto Sans".to_string(),
        size: RUN_SIZE,
        weight: 400,
        color,
        line_height_px: None,
        letter_spacing: 0.0,
        text_align: TextAlign::Left,
        text_align_v: TextAlignV::Top,
        ligatures_off: false,
    }
}

/// Builds the #322 scene: a HUG-cross `Baseline` row holding a tall box, a text
/// run and a `Fill` cross-sized child, with a following sibling underneath, all
/// inside a HUG-height column.
///
/// Returns `(row, run, stretched, follower)`.
fn baseline_hug_cross_scene(
    arena: &mut Arena,
    typesetter: &mut Typesetter,
) -> (NodeId, NodeId, NodeId, NodeId) {
    let mut solver = TaffySolver::with_text(typesetter, vec![load_atlas(ATLAS_DIR)]);
    let mut txn = arena.open();

    let column = txn.add_node(None, Some("column"));
    txn.set_prop(column, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(column, Prop::Width(240.0));
    txn.set_prop(column, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(column, Prop::Fill(NAVY));

    let row = txn.add_node(Some(column), Some("row"));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(row, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(row, Prop::Gap(12.0));
    txn.set_prop(
        row,
        Prop::Padding {
            left: 8.0,
            top: 8.0,
            right: 8.0,
            bottom: 6.0,
        },
    );
    txn.set_prop(row, Prop::Fill(SLATE));

    // The tall non-text child sets the row's baseline: taffy takes a leaf's
    // baseline as its box bottom, so the text run's glyph baseline is pulled
    // down to 100 and the run's own descender then ends below the row's
    // box-bottom cross size. That overhang is what #322 grows the row for.
    boxed(&mut txn, Some(row), 40.0, 100.0, AMBER);

    let run = txn.add_node(Some(row), Some("run"));
    txn.set_prop(run, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(run, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(run, Prop::Text(RUN.to_string()));
    txn.set_prop(run, Prop::TextStyle(text_style(NEAR_WHITE)));

    // A `Fill` cross-sized child, which the #322 pass must leave out of the
    // baseline set: it maps to `align_self: STRETCH`, so taffy stretches it
    // over the row's content box instead of baseline-aligning it, and counting
    // its stretched height towards the floor would feed the row's own cross
    // size back into itself. It stretches to the *corrected* content height, so
    // its bottom edge is a second reading of the row's grown size.
    let stretched = txn.add_node(Some(row), Some("stretched"));
    txn.set_prop(stretched, Prop::Width(24.0));
    txn.set_prop(stretched, Prop::SizingV(AxisSizing::Fill));
    txn.set_prop(stretched, Prop::Fill(GREEN));

    // The following sibling: the row's corrected cross size has to move it,
    // which is what makes #322 a solve rather than a rect patch.
    let follower = boxed(&mut txn, Some(column), 240.0, 24.0, BLUE);

    txn.commit_with(&mut solver);
    (row, run, stretched, follower)
}

#[test]
fn a_hug_cross_baseline_row_with_text_matches_its_golden() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();
    let (row, run, stretched, follower) = baseline_hug_cross_scene(&mut arena, &mut ts);

    // The row's cross size is the glyph-aligned extent, not taffy's
    // box-bottom one: top padding + the tallest baseline + the run's
    // descender + bottom padding. The descender is font-derived, so the
    // expected numbers are read from the typesetter rather than hard-coded.
    let laid = ts.layout(RUN, RUN_SIZE, None);
    let first = laid.lines.first().expect("the run has one line");
    let descender = laid.height - first.baseline_y;
    let box_bottom_height = 8.0 + 100.0 + 6.0;
    let glyph_aligned_height = 8.0 + 100.0 + descender + 6.0;

    let (_, row_y, _, row_h) = rect_of(&arena, row);
    assert_eq!(row_y, 0.0, "the row is the column's first child");
    assert!(
        (row_h - glyph_aligned_height).abs() < 0.01,
        "the HUG row grew to its glyph-aligned extent ({row_h} vs \
         {glyph_aligned_height}); taffy's box-bottom size is \
         {box_bottom_height}",
    );
    assert!(
        descender > 4.0,
        "the run must overhang the box-bottom size by a visible margin \
         (descender {descender})",
    );

    // The run sits on the tall box's baseline, and its own bottom edge lands
    // on the row's content-box bottom — the row grew to exactly hold it.
    let (_, run_y, _, run_h) = rect_of(&arena, run);
    assert!(
        (run_y + run_h + 6.0 - row_h).abs() < 0.01,
        "the run's bottom edge sits on the row's content-box bottom",
    );

    // The `Fill` cross child is stretched over the row's content box, not
    // baseline-aligned — and it is stretched over the *corrected* box, which
    // is what says the floor went back through the solver rather than patching
    // the row's rect.
    let (_, stretched_y, _, stretched_h) = rect_of(&arena, stretched);
    assert!(
        (stretched_y - 8.0).abs() < 0.01 && (stretched_h - (row_h - 14.0)).abs() < 0.01,
        "the Fill cross child stretches over the corrected content box \
         (y {stretched_y}, h {stretched_h}, row {row_h})",
    );

    // The following sibling moved with the row.
    let (_, follower_y, _, _) = rect_of(&arena, follower);
    assert!(
        (follower_y - row_h).abs() < 0.01,
        "the follower starts where the corrected row ends",
    );

    let glyphs = arena.committed().glyphs();
    let png = render(&arena, glyphs);

    // The canvas is the column's own hug height, so reverting #322 renders a
    // shorter image and the golden fails on its dimension check — before any
    // budget applies. The budget below only absorbs cross-machine MSDF edge
    // jitter (`docs/decisions/golden-comparison-space.md`, "Text goldens").
    goldens::assert_matches_golden_max_pixels(
        "v013-baseline-hug-cross",
        &png,
        goldens::CROSS_ARCH_BUDGET_PX,
    );
}

/// Sensitivity guard (the #232/#235 lesson), the one this golden was missing
/// when its budget was retuned in story #671. The other six pixel-budget
/// goldens each carry one; this golden did not, so nothing asserted that its
/// budget could still distinguish a right render from a wrong one.
///
/// The dimension check above catches the regression this scene exists for —
/// reverting #322 renders a shorter canvas and fails before any budget
/// applies. That is a different property from the budget being tight enough
/// to see lost ink, which is what this asserts: the row's text dropped must
/// exceed the budget.
#[test]
fn dropping_the_baseline_run_exceeds_the_budget() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();
    let _ = baseline_hug_cross_scene(&mut arena, &mut ts);

    let empty = render(&arena, &GlyphRunTable::new());
    let differed = diff_vs(
        &decode_golden("v013-baseline-hug-cross"),
        &decode_rgba(&empty),
    );
    assert!(
        differed > goldens::CROSS_ARCH_BUDGET_PX,
        "dropping the baseline row's text must exceed the \
         {}px budget, differed by {differed}",
        goldens::CROSS_ARCH_BUDGET_PX,
    );
}

// ---------------------------------------------------------------------
// Issue #495 — mask bounds are mask ∩ maskee, and a maskee effect that
// reaches past the maskee's own box still shows inside the mask.
// ---------------------------------------------------------------------

const MASK_CANVAS_W: f32 = 192.0;
const MASK_CANVAS_H: f32 = 96.0;

/// Builds the #495 scene: two panels, each a mask larger than its maskee, and
/// a maskee whose hard-edged drop shadow reaches past the maskee's own box.
///
/// The landed reading chains the mask box onto the region the maskee already
/// had, and adds nothing else — so the shadow shows anywhere inside the mask.
/// The rejected reading intersects the *maskee's* box as well, which would cut
/// the shadow at the maskee edge; here that hides the shadow completely, since
/// what is left of it sits behind the maskee's own fill.
///
/// The left panel's parent does not clip, so the parent box legitimately never
/// enters the region. The right panel's does, and its box is the tighter bound
/// on x while the mask is the tighter bound on y — so both boxes are visible in
/// the picture, in the outermost-first order the region carries.
///
/// `intersect_maskee` builds the rejected reading instead, by shrinking each
/// mask to the maskee's own box. That is the sensitivity twin, not a code
/// path: it produces exactly the pixels the rejected reading would.
fn mask_effect_bleed_scene(arena: &mut Arena, intersect_maskee: bool) {
    let mut txn = arena.open();
    let bg = placed(&mut txn, None, 0.0, 0.0, MASK_CANVAS_W, MASK_CANVAS_H);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let shadow = |txn: &mut Txn<'_>, node: NodeId| {
        txn.set_prop(
            node,
            Prop::Shadows(vec![Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: 24.0, y: 24.0 },
                blur: 0.0,
                spread: 0.0,
                color: SHADOW_INK,
            }]),
        );
    };

    // One container per panel: a mask stencils the siblings that follow it
    // within the same parent, so the left panel's mask would otherwise reach
    // across the whole canvas and stencil the right panel too.
    let panel_l = placed(&mut txn, Some(bg), 0.0, 0.0, 96.0, MASK_CANVAS_H);
    let panel_r = placed(&mut txn, Some(bg), 96.0, 0.0, 96.0, MASK_CANVAS_H);

    // Left panel: a non-clipping parent.
    let mask_l = if intersect_maskee {
        placed(&mut txn, Some(panel_l), 24.0, 24.0, 32.0, 32.0)
    } else {
        placed(&mut txn, Some(panel_l), 8.0, 8.0, 80.0, 80.0)
    };
    // A fill a mask must never paint: a mask is a stencil, not paint.
    txn.set_prop(mask_l, Prop::Fill(RED));
    txn.set_prop(mask_l, Prop::Mask(true));
    let maskee_l = placed(&mut txn, Some(panel_l), 24.0, 24.0, 32.0, 32.0);
    txn.set_prop(maskee_l, Prop::Fill(AMBER));
    shadow(&mut txn, maskee_l);

    // Right panel: a clipping parent, tighter than the mask on x only. Its
    // offset is relative to `panel_r` at x = 96, so it spans (104, 8)-(160, 80)
    // in canvas coordinates.
    let clip_r = placed(&mut txn, Some(panel_r), 8.0, 8.0, 56.0, 72.0);
    txn.set_prop(clip_r, Prop::Fill(PANEL));
    txn.set_prop(clip_r, Prop::Clip(true));
    // Relative to `clip_r` in turn: the mask spans (108, 12)-(172, 68) and the
    // maskee (120, 20)-(152, 52) in canvas coordinates.
    let mask_r = if intersect_maskee {
        placed(&mut txn, Some(clip_r), 16.0, 12.0, 32.0, 32.0)
    } else {
        placed(&mut txn, Some(clip_r), 4.0, 4.0, 64.0, 56.0)
    };
    txn.set_prop(mask_r, Prop::Mask(true));
    let maskee_r = placed(&mut txn, Some(clip_r), 16.0, 12.0, 32.0, 32.0);
    txn.set_prop(maskee_r, Prop::Fill(GREEN));
    shadow(&mut txn, maskee_r);

    txn.commit();
}

#[test]
fn a_maskee_effect_reaching_past_its_box_matches_its_golden() {
    let mut arena = Arena::new();
    mask_effect_bleed_scene(&mut arena, false);

    let png = render(&arena, &GlyphRunTable::new());
    let pixels = decode_rgba(&png);
    let w = MASK_CANVAS_W as usize;
    let probe = |x: usize, y: usize| goldens::pixel(&pixels, w, x, y);
    let quantized = |c: Color| {
        let q = |v: f32| (v * 255.0).round() as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    };

    // The ruling, as pixels. The maskee's box is (24, 24)-(56, 56); its
    // shadow's is (48, 48)-(80, 80). At (68, 68) the shadow is outside the
    // maskee's box and inside the mask's — the rejected reading paints navy
    // there, the landed one paints the shadow.
    assert_eq!(
        probe(68, 68),
        quantized(SHADOW_INK),
        "the maskee's shadow shows past the maskee's own box, inside the mask",
    );
    // Past the mask's box (88) the shadow is stenciled away.
    assert_eq!(
        probe(84, 84),
        quantized(NAVY),
        "and it stops at the mask's box, not at the parent's",
    );
    assert_eq!(probe(40, 40), quantized(AMBER), "the maskee's own fill");
    assert_eq!(
        probe(12, 12),
        quantized(NAVY),
        "the mask paints nothing of its own",
    );

    // Right panel: the clipping parent bounds x at 160, the mask bounds y at
    // 68 — outermost first, both load-bearing.
    assert_eq!(
        probe(156, 64),
        quantized(SHADOW_INK),
        "inside both the parent clip and the mask",
    );
    assert_eq!(
        probe(164, 64),
        quantized(NAVY),
        "past the clipping parent's box on x",
    );
    assert_eq!(
        probe(156, 72),
        quantized(PANEL),
        "past the mask's box on y — the clipping parent still shows",
    );

    goldens::assert_matches_golden("v013-mask-effect-bleed", &png);
}

/// The demonstrated-sensitivity guard for the mask golden (the discipline
/// `v08_shadows.rs` set): the golden compares exactly, so this measures how
/// much the rejected reading would move rather than whether it fits a budget.
/// A count in the hundreds is what says the frame has teeth — issue #422 is
/// the standing reminder that a budget can be too wide to fail on a
/// bounded-area defect.
#[test]
fn the_rejected_mask_reading_moves_far_more_than_the_golden_can_absorb() {
    let mut rejected = Arena::new();
    mask_effect_bleed_scene(&mut rejected, true);
    let differing = diff_vs(
        &decode_rgba(&render(&rejected, &GlyphRunTable::new())),
        &decode_golden("v013-mask-effect-bleed"),
    );
    assert!(
        differing > 800,
        "the rejected mask-bounds reading must move far more than a \
         cross-machine jitter budget could hide (moved {differing} px)",
    );
}

/// The clipping frame's inner width — narrower than "Ag" at `RUN_SIZE`, so
/// the clip cuts the second glyph rather than merely bounding the text.
const CLIP_W: f32 = 44.0;
const CLIP_CANVAS_W: f32 = 120.0;
const CLIP_CANVAS_H: f32 = 80.0;

/// Builds the #275 scene: a clipping frame holding a text child whose ink is
/// wider than the frame, so the frame's clip region actually cuts glyphs.
///
/// `clip` is the frame's clip flag — `false` renders the broken twin the
/// sensitivity guard measures, which is also exactly how this scene rendered
/// before the painter honoured a run's clip.
fn text_clipped_scene(arena: &mut Arena, typesetter: &mut Typesetter, clip: bool) -> NodeId {
    let mut solver = TaffySolver::with_text(typesetter, vec![load_atlas(ATLAS_DIR)]);
    let mut txn = arena.open();

    let root = txn.add_node(None, Some("backdrop"));
    txn.set_prop(root, Prop::Width(CLIP_CANVAS_W));
    txn.set_prop(root, Prop::Height(CLIP_CANVAS_H));
    txn.set_prop(root, Prop::Mode(LayoutMode::None));
    txn.set_prop(root, Prop::Fill(NAVY));

    // The clipping frame: fixed and narrower than the text it holds.
    let frame = placed(&mut txn, Some(root), 16.0, 16.0, CLIP_W, 48.0);
    txn.set_prop(frame, Prop::Fill(PANEL));
    txn.set_prop(frame, Prop::Clip(clip));

    // The text: hug-sized, so it takes its own shaped width and overflows the
    // frame. A clipping ancestor is what turns that overflow into a cut.
    let label = txn.add_node(Some(frame), Some("label"));
    txn.set_prop(label, Prop::X(4.0));
    txn.set_prop(label, Prop::Y(2.0));
    txn.set_prop(label, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(label, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(label, Prop::Text(RUN.to_string()));
    txn.set_prop(label, Prop::TextStyle(text_style(NEAR_WHITE)));

    txn.commit_with(&mut solver);
    label
}

/// Issue #275: a glyph run is clipped to the region its anchor rect carries.
///
/// The corpus had no scene where a clip cuts glyph ink — `v07-text-lowering`
/// has a clip whose box contains its text, so it renders identically clipped
/// or not. That is why the whole suite stayed green while runs ignored the
/// clip table entirely, and why this frame exists.
#[test]
fn text_inside_a_clipping_frame_is_cut_by_it() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();
    let label = text_clipped_scene(&mut arena, &mut ts, true);

    // The fixture only tests anything if the text really does overflow: a
    // label narrower than the frame would render the same either way.
    let (_, _, label_w, _) = rect_of(&arena, label);
    assert!(
        label_w > CLIP_W,
        "fixture: the label ({label_w} px) must be wider than the {CLIP_W} px \
         frame, or the clip cuts nothing — widen the string if the font changed",
    );

    let png = render(&arena, arena.committed().glyphs());
    goldens::assert_matches_golden_max_pixels("v013-text-clipped", &png, 200);
}

/// The demonstrated-sensitivity guard: rendering the same scene with the
/// frame's clip flag off — which is precisely how the painter behaved before
/// issue #275 — must move far more than any jitter budget could hide.
#[test]
fn dropping_the_clip_flag_moves_far_more_than_the_golden_can_absorb() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut unclipped = Arena::new();
    text_clipped_scene(&mut unclipped, &mut ts, false);

    let differing = diff_vs(
        &decode_rgba(&render(&unclipped, unclipped.committed().glyphs())),
        &decode_golden("v013-text-clipped"),
    );
    assert!(
        differing > 200,
        "an unclipped run must move far more than the golden's budget \
         (moved {differing} px)",
    );
}

const GROUP_CANVAS_W: f32 = 120.0;
const GROUP_CANVAS_H: f32 = 72.0;
/// The group's alpha. Below 1, and over an overlapping subtree, so commit
/// resolves it to a `GroupComposite` (the render-target path) rather than
/// folding it per rect (the free path).
const GROUP_ALPHA: f32 = 0.5;

/// Builds the #274 scene: a render-target group holding two overlapping
/// boxes and a text label, so the label must composite *into* the group's
/// offscreen layer rather than draw over the composited result.
///
/// The overlap is what forces the render-target path: a non-overlapping
/// subtree takes the free path, where the alpha rides on each rect and text
/// was already correct (story #44).
fn text_in_render_target_group_scene(arena: &mut Arena, typesetter: &mut Typesetter) -> NodeId {
    let mut solver = TaffySolver::with_text(typesetter, vec![load_atlas(ATLAS_DIR)]);
    let mut txn = arena.open();

    let root = txn.add_node(None, Some("backdrop"));
    txn.set_prop(root, Prop::Width(GROUP_CANVAS_W));
    txn.set_prop(root, Prop::Height(GROUP_CANVAS_H));
    txn.set_prop(root, Prop::Mode(LayoutMode::None));
    txn.set_prop(root, Prop::Fill(NEAR_WHITE));

    let group = placed(
        &mut txn,
        Some(root),
        0.0,
        0.0,
        GROUP_CANVAS_W,
        GROUP_CANVAS_H,
    );
    txn.set_prop(group, Prop::Opacity(GROUP_ALPHA));

    // Two overlapping opaque boxes: the overlap is what sends this group
    // down the render-target path instead of the free one.
    let back = placed(&mut txn, Some(group), 12.0, 12.0, 64.0, 48.0);
    txn.set_prop(back, Prop::Fill(BLUE));
    let front = placed(&mut txn, Some(group), 44.0, 12.0, 64.0, 48.0);
    txn.set_prop(front, Prop::Fill(AMBER));

    // The label sits inside the group, over the boxes.
    let label = txn.add_node(Some(group), Some("label"));
    txn.set_prop(label, Prop::X(16.0));
    txn.set_prop(label, Prop::Y(14.0));
    txn.set_prop(label, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(label, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(label, Prop::Text(RUN.to_string()));
    txn.set_prop(label, Prop::TextStyle(text_style(RED)));

    txn.commit_with(&mut solver);
    label
}

/// Issue #274: a glyph run inside a render-target group composites into that
/// group's layer, not over the composited result.
///
/// The corpus had no scene carrying both a glyph run and a `GroupComposite`
/// at all — `paint.text-outside-group` warned about the combination and
/// nothing exercised it — so the whole suite stayed green either way. That is
/// why this frame exists.
#[test]
fn text_inside_a_render_target_group_composites_in_its_layer() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();
    text_in_render_target_group_scene(&mut arena, &mut ts);

    // The fixture only tests anything if commit actually took the
    // render-target path: the free path carries its alpha per rect and
    // produces no `GroupComposite` at all.
    let scene = arena.committed();
    assert_eq!(
        scene.groups().len(),
        1,
        "fixture: the overlapping subtree must resolve to one render-target \
         group, or this frame does not exercise issue #274",
    );
    assert!(
        !scene.glyphs().is_empty(),
        "fixture: the scene must carry glyph runs",
    );

    let png = render(&arena, scene.glyphs());

    // The distinguishing property, stated exactly rather than through the
    // image's tolerance: text composited *into* a 0.5 layer can never reach
    // the run's own full-strength colour, while text drawn over the
    // composited result inks it directly. So the scene must be visibly red
    // somewhere, and nowhere fully red.
    let rgba = decode_rgba(&png);
    let full_strength = [
        (RED.r * 255.0).round() as u8,
        (RED.g * 255.0).round() as u8,
        (RED.b * 255.0).round() as u8,
        255,
    ];
    let reddish = rgba
        .chunks_exact(4)
        .filter(|p| p[0] > u16::from(p[1]).saturating_add(40) as u8)
        .count();
    let exact = rgba
        .chunks_exact(4)
        .filter(|p| p[..] == full_strength[..])
        .count();
    assert!(reddish > 0, "the label is visible");
    assert_eq!(
        exact, 0,
        "no pixel may carry the run's full-strength colour: every text pixel \
         went through the group's {GROUP_ALPHA} layer",
    );

    goldens::assert_matches_golden_max_pixels("v013-text-in-group", &png, 200);
}

/// The demonstrated-sensitivity guard: text drawn at full strength over the
/// composited layer — which is exactly how the painter behaved before issue
/// #274 — must move far more than any jitter budget could hide.
///
/// The pre-#274 painter is reproduced by handing the painter an empty group
/// list, so the layer never opens and the run draws over flat colour. That
/// changes the boxes too, so this measures a lower bound on the text's own
/// contribution rather than isolating it; the golden above is what pins the
/// text pixel exactly.
#[test]
fn text_outside_the_group_layer_moves_far_more_than_the_golden_can_absorb() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    let mut ts = Typesetter::new(font);
    let mut arena = Arena::new();
    text_in_render_target_group_scene(&mut arena, &mut ts);
    let scene = arena.committed();

    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        &[],
        scene.glyphs(),
        None,
    );
    let differing = diff_vs(
        &decode_rgba(&painter.png_bytes()),
        &decode_golden("v013-text-in-group"),
    );
    assert!(
        differing > 200,
        "text escaping the group layer must move far more than the golden's \
         budget (moved {differing} px)",
    );
}
