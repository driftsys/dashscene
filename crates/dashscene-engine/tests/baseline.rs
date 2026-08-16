//! #272: a TEXT leaf in a `CrossAxisAlign::Baseline` row aligns on its
//! shaped first-line baseline (the ascent), not its box bottom.
//!
//! Taffy's high-level measure API reports no baseline for a leaf, so Taffy
//! falls back to the box bottom (`baseline.unwrap_or(height)`); a mixed-size
//! row then aligns box bottoms, and the shorter runs sit a descender too low.
//! Figma's baseline auto-layout aligns the glyph baselines. The engine
//! corrects text leaves after the solve, using the typesetter's first-line
//! `baseline_y`. The render oracle's v08-baseline frame measures this against
//! Figma directly.

use dashscene_core::{
    Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, TextAlign,
    TextAlignV, TextStyle,
};
use dashscene_engine::TaffySolver;
use dashscene_typeset::text::{Font, Typesetter};

/// The committed corpus fixture font (corpus/fonts/noto-sans/).
const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

fn typesetter() -> Typesetter {
    let data = std::fs::read(FONT).expect("corpus fixture font present");
    Typesetter::new(Font::from_bytes(data, 0).expect("corpus font loads"))
}

fn styled(txn: &mut dashscene_core::Txn<'_>, node: NodeId, text: &str, size: f32) {
    txn.set_prop(node, Prop::Text(text.to_string()));
    txn.set_prop(
        node,
        Prop::TextStyle(TextStyle {
            family: "Noto Sans".to_string(),
            size,
            weight: 400,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            line_height_px: None,
            letter_spacing: 0.0,
            text_align: TextAlign::Left,
            text_align_v: TextAlignV::Top,
            ligatures_off: false,
        }),
    );
}

/// A hug-sized text leaf under `parent`.
fn text_leaf(txn: &mut dashscene_core::Txn<'_>, parent: NodeId, text: &str, size: f32) -> NodeId {
    let n = txn.add_node(Some(parent), None);
    txn.set_prop(n, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(n, Prop::SizingV(AxisSizing::Hug));
    styled(txn, n, text, size);
    n
}

/// (x, y, w, h) of the node at creation index `i`.
fn rect_at(arena: &Arena, i: usize) -> (f32, f32, f32, f32) {
    let r = arena.committed().rects()[i];
    (r.x, r.y, r.w, r.h)
}

/// A fixed-size non-text box under `parent`.
fn box_leaf(txn: &mut dashscene_core::Txn<'_>, parent: NodeId, w: f32, h: f32) -> NodeId {
    let n = txn.add_node(Some(parent), None);
    txn.set_prop(n, Prop::Width(w));
    txn.set_prop(n, Prop::Height(h));
    n
}

/// The #322 row's asymmetric cross-axis padding, so a grown cross size
/// pins both padding terms rather than only their sum.
const PAD_TOP: f32 = 8.0;
const PAD_BOTTOM: f32 = 6.0;

/// A HUG-cross-sized `Baseline` row holding one tall non-text box and one
/// text run — the #322 shape. Returns `(row, text run)`.
fn hug_baseline_row(
    txn: &mut dashscene_core::Txn<'_>,
    parent: Option<NodeId>,
    box_h: f32,
    text_size: f32,
) -> (NodeId, NodeId) {
    let row = txn.add_node(parent, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
    txn.set_prop(
        row,
        Prop::Padding {
            left: 0.0,
            top: PAD_TOP,
            right: 0.0,
            bottom: PAD_BOTTOM,
        },
    );
    box_leaf(txn, row, 40.0, box_h);
    let text = text_leaf(txn, row, "LARGE", text_size);
    (row, text)
}

#[test]
fn a_hug_baseline_row_grows_to_hold_its_glyph_aligned_children() {
    // Issue #322, the residual of #272's baseline correction. Taffy gives
    // a baseline-less leaf its box bottom as its baseline, so the row's
    // HUG cross size is sized from box bottoms: 100, the tall box. The
    // #272 pass then drops the text run so its GLYPH baseline meets the
    // box bottom, which pushes the run's own box bottom a descender past
    // the row's height — the run clips. A HUG row must hold the children
    // the correction placed in it.
    let mut ts = typesetter();
    let laid = ts.layout("LARGE", 40.0, None);
    let text_h = laid.height;
    let text_baseline = laid.lines[0].baseline_y;
    let descender = text_h - text_baseline;
    assert!(
        descender > 0.5,
        "the run's box bottom sits below its baseline, by {descender}"
    );
    let box_h = 100.0_f32;
    assert!(
        box_h > text_baseline,
        "the box bottom is the tallest baseline in the row"
    );

    let mut arena = Arena::new();
    let mut txn = arena.open();
    hug_baseline_row(&mut txn, None, box_h, 40.0); // row 0, box 1, text 2
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let eps = 0.01;
    let (_, _, _, row_h) = rect_at(&arena, 0);
    let (_, box_y, _, _) = rect_at(&arena, 1);
    let (_, text_y, _, _) = rect_at(&arena, 2);

    // The tallest baseline anchors at the content top.
    assert!(
        (box_y - PAD_TOP).abs() < eps,
        "the tallest baseline sits at the content top, got {box_y}"
    );
    // The two baselines still meet: the box bottom is the tallest.
    assert!(
        ((box_y + box_h) - (text_y + text_baseline)).abs() < eps,
        "baselines off the line: {} vs {}",
        box_y + box_h,
        text_y + text_baseline
    );
    // And the row now holds both boxes: its height is the text run's
    // bottom — a descender below the tallest baseline — plus its own
    // cross-axis padding on both sides.
    let want = PAD_TOP + box_h + descender + PAD_BOTTOM;
    assert!(
        (row_h - want).abs() < eps,
        "hug row height {row_h}, want {want} (pad {PAD_TOP} + box {box_h} + \
         descender {descender} + pad {PAD_BOTTOM})"
    );
    assert!(
        text_y + text_h + PAD_BOTTOM <= row_h + eps,
        "the run's bottom {} overflows the hug row's content box",
        text_y + text_h
    );
}

#[test]
fn a_grown_hug_baseline_row_re_places_its_following_siblings() {
    // The grown cross size must go back through the solver, not be
    // patched onto the row's rect: everything placed after the row in its
    // parent's flow moves by the growth, and the parent's own HUG height
    // grows with it. A post-solve height patch would leave both behind.
    let mut ts = typesetter();
    let laid = ts.layout("LARGE", 40.0, None);
    let descender = laid.height - laid.lines[0].baseline_y;
    let box_h = 100.0_f32;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let column = txn.add_node(None, None); // 0
    txn.set_prop(column, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(column, Prop::MainAlign(MainAxisAlign::Start));
    txn.set_prop(column, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(column, Prop::SizingV(AxisSizing::Hug));
    hug_baseline_row(&mut txn, Some(column), box_h, 40.0); // row 1, box 2, text 3
    let after = box_leaf(&mut txn, column, 40.0, 20.0); // 4
    let _ = after;
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let eps = 0.01;
    let (_, _, _, column_h) = rect_at(&arena, 0);
    let (_, _, _, row_h) = rect_at(&arena, 1);
    let (_, after_y, _, _) = rect_at(&arena, 4);

    let want_row = PAD_TOP + box_h + descender + PAD_BOTTOM;
    assert!(
        (row_h - want_row).abs() < eps,
        "row height {row_h}, want {want_row}"
    );
    assert!(
        (after_y - row_h).abs() < eps,
        "the sibling after the row sits at {after_y}, want the row's bottom {row_h}"
    );
    assert!(
        (column_h - (row_h + 20.0)).abs() < eps,
        "the hug column's height {column_h}, want {}",
        row_h + 20.0
    );
}

#[test]
fn a_fixed_height_baseline_row_keeps_its_authored_height() {
    // The #322 growth is HUG-only: an authored cross size is the author's
    // decision, and a run that overflows it clips exactly as it did.
    let mut ts = typesetter();
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None);
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(row, Prop::Height(100.0));
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
    box_leaf(&mut txn, row, 40.0, 100.0);
    text_leaf(&mut txn, row, "LARGE", 40.0);
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    assert_eq!(rect_at(&arena, 0).3, 100.0, "the authored height stands");
}

#[test]
fn a_hug_baseline_row_re_shrinks_when_its_text_does_on_a_retained_solver() {
    // The cross-size floor lives on the retained tree, so an incremental
    // solve must recompute it rather than carry the previous frame's. A
    // smaller run has a smaller descender, so the row must give the extra
    // room back.
    let mut ts = typesetter();
    let big = ts.layout("LARGE", 40.0, None);
    let big_descender = big.height - big.lines[0].baseline_y;
    let small = ts.layout("LARGE", 12.0, None);
    let small_descender = small.height - small.lines[0].baseline_y;
    assert!(big_descender > small_descender);
    let box_h = 100.0_f32;

    let mut arena = Arena::new();
    let mut solver = TaffySolver::with_typesetter(&mut ts);
    let text = {
        let mut txn = arena.open();
        let (_, text) = hug_baseline_row(&mut txn, None, box_h, 40.0);
        txn.commit_with(&mut solver);
        text
    };
    let eps = 0.01;
    let pad = PAD_TOP + PAD_BOTTOM;
    assert!(
        (rect_at(&arena, 0).3 - (pad + box_h + big_descender)).abs() < eps,
        "grown for the 40px run"
    );

    let mut txn = arena.open();
    styled(&mut txn, text, "LARGE", 12.0);
    txn.commit_with(&mut solver);
    assert!(
        (rect_at(&arena, 0).3 - (pad + box_h + small_descender)).abs() < eps,
        "the row height {} must fall back to {}",
        rect_at(&arena, 0).3,
        pad + box_h + small_descender
    );
}

#[test]
fn a_row_that_stops_needing_a_cross_floor_has_it_removed() {
    // The floor is injected into the retained tree's style, and a row is
    // restyled only when the row itself is dirty. Here the ROW never
    // changes: its only text child takes a `Fill` cross size, which makes
    // it a stretched child rather than a baseline-aligned one, so the row
    // stops being a baseline TEXT row at all. Its floor must come off, or
    // it keeps a height nothing in the scene asks for.
    let mut ts = typesetter();
    let laid = ts.layout("LARGE", 40.0, None);
    let descender = laid.height - laid.lines[0].baseline_y;
    assert!(descender > 0.5);
    let box_h = 100.0_f32;

    let mut arena = Arena::new();
    let mut solver = TaffySolver::with_typesetter(&mut ts);
    let text = {
        let mut txn = arena.open();
        let (_, text) = hug_baseline_row(&mut txn, None, box_h, 40.0);
        txn.commit_with(&mut solver);
        text
    };
    let eps = 0.01;
    let floored = PAD_TOP + box_h + descender + PAD_BOTTOM;
    assert!(
        (rect_at(&arena, 0).3 - floored).abs() < eps,
        "the row is floored at {floored} to start with, got {}",
        rect_at(&arena, 0).3
    );

    let mut txn = arena.open();
    txn.set_prop(text, Prop::SizingV(AxisSizing::Fill));
    txn.commit_with(&mut solver);

    // No baseline-aligned text child is left, so no correction and no
    // floor: the row hugs the tall box plus its own padding.
    let unfloored = PAD_TOP + box_h + PAD_BOTTOM;
    assert!(
        (rect_at(&arena, 0).3 - unfloored).abs() < eps,
        "the row height {} must fall back to {unfloored}",
        rect_at(&arena, 0).3
    );
}

#[test]
fn a_hug_baseline_rows_floor_never_beats_an_authored_min_height() {
    // The #322 floor replaces the row's Taffy min cross size, so it has to
    // carry the authored one: an authored min above the glyph-aligned
    // extent must still win, exactly as it does for a row that needs no
    // floor at all.
    let mut ts = typesetter();
    let laid = ts.layout("LARGE", 40.0, None);
    let descender = laid.height - laid.lines[0].baseline_y;
    let box_h = 100.0_f32;
    let grown = PAD_TOP + box_h + descender + PAD_BOTTOM;
    let authored_min = grown + 40.0;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let (row, _) = hug_baseline_row(&mut txn, None, box_h, 40.0);
    txn.set_prop(row, Prop::MinHeight(authored_min));
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    assert!(
        (rect_at(&arena, 0).3 - authored_min).abs() < 0.01,
        "the authored min height {authored_min} must beat the {grown} floor, got {}",
        rect_at(&arena, 0).3
    );
}

#[test]
fn a_fill_cross_child_of_a_baseline_row_stretches_instead_of_aligning() {
    // A `Fill` cross-sized child maps to `align_self: STRETCH`, and taffy
    // excludes a stretched item from baseline alignment. The #272
    // correction must exclude it too: re-placing it on a baseline would
    // move a child taffy stretched over the whole content box, and
    // counting its stretched height towards the #322 floor would feed the
    // row's own cross size back into itself.
    let mut ts = typesetter();
    let laid = ts.layout("LARGE", 40.0, None);
    let descender = laid.height - laid.lines[0].baseline_y;
    let box_h = 100.0_f32;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let (row, _) = hug_baseline_row(&mut txn, None, box_h, 40.0); // 0, 1, 2
    let filler = txn.add_node(Some(row), None); // 3
    txn.set_prop(filler, Prop::Width(10.0));
    txn.set_prop(filler, Prop::SizingV(AxisSizing::Fill));
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let eps = 0.01;
    let (_, _, _, row_h) = rect_at(&arena, 0);
    let (_, filler_y, _, filler_h) = rect_at(&arena, 3);
    let want_row = PAD_TOP + box_h + descender + PAD_BOTTOM;
    assert!(
        (row_h - want_row).abs() < eps,
        "the stretched child must not change the row height: {row_h}, want {want_row}"
    );
    assert!(
        (filler_y - PAD_TOP).abs() < eps,
        "the stretched child stays at the content top, got {filler_y}"
    );
    assert!(
        (filler_h - (row_h - PAD_TOP - PAD_BOTTOM)).abs() < eps,
        "the stretched child fills the content box: {filler_h}, want {}",
        row_h - PAD_TOP - PAD_BOTTOM
    );
}

#[test]
fn a_mixed_size_text_baseline_row_aligns_on_glyph_baselines() {
    // The v08-baseline oracle frame in miniature: a fixed HORIZONTAL row,
    // padding 24, gap 16, counter-axis BASELINE, with three Noto Sans runs
    // at 12/24/40. Every run's first-line glyph baseline must land on one
    // line — content_top (padding.top) + the tallest run's ascent — not on
    // the aligned box bottoms Taffy gives baseline-less leaves (#272).
    let mut ts = typesetter();
    let b_small = ts.layout("small", 12.0, None).lines[0].baseline_y;
    let b_medium = ts.layout("medium", 24.0, None).lines[0].baseline_y;
    let b_large = ts.layout("LARGE", 40.0, None).lines[0].baseline_y;
    // The tallest run has the largest ascent; it anchors the baseline line.
    assert!(
        b_large > b_medium && b_medium > b_small,
        "the first-line ascent grows with the render size"
    );

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let row = txn.add_node(None, None); // index 0
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Width(380.0));
    txn.set_prop(row, Prop::Height(120.0));
    txn.set_prop(row, Prop::Gap(16.0));
    txn.set_prop(
        row,
        Prop::Padding {
            left: 24.0,
            top: 24.0,
            right: 24.0,
            bottom: 24.0,
        },
    );
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
    let _small = text_leaf(&mut txn, row, "small", 12.0); // index 1
    let _medium = text_leaf(&mut txn, row, "medium", 24.0); // index 2
    let _large = text_leaf(&mut txn, row, "LARGE", 40.0); // index 3
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let content_top = 24.0_f32;
    let base = content_top + b_large; // the one glyph-baseline line
    let eps = 0.01;

    let (_, y_small, _, _) = rect_at(&arena, 1);
    let (_, y_medium, _, _) = rect_at(&arena, 2);
    let (_, y_large, _, _) = rect_at(&arena, 3);

    // The tallest run sits at the content top; the shorter runs drop by the
    // ascent difference so all three glyph baselines meet on one line.
    assert!(
        (y_large - content_top).abs() < eps,
        "the tallest run's box top is the content top, got {y_large}"
    );
    assert!(
        (y_large + b_large - base).abs() < eps,
        "large run baseline off the line: {} vs {base}",
        y_large + b_large
    );
    assert!(
        (y_medium + b_medium - base).abs() < eps,
        "medium run baseline off the line: {} vs {base}",
        y_medium + b_medium
    );
    assert!(
        (y_small + b_small - base).abs() < eps,
        "small run baseline off the line: {} vs {base}",
        y_small + b_small
    );
}

/// **The #322 baseline pass follows the shown root too** (story #838).
///
/// It was the one place in the solve that story missed. `baseline_pass` walked
/// `arena.roots()` and re-solved over every Taffy root, so on a document with
/// more than one artboard it did two wrong things at once, and neither is
/// visible to the per-frame band — that harness runs `TaffySolver::new()`,
/// which returns from `baseline_pass` before any of this.
///
/// It read `tree.layout()` for nodes no `compute_all` had computed, which is a
/// zeroed layout, and shaped their text against it — inventing a cross-size
/// floor from a row that was never solved. And its re-solve then computed
/// **every** root in the document, which is the per-frame cost the story
/// exists to remove, restored in full on exactly the documents that carry
/// text.
///
/// **How it fails, unconfined**: `baseline_pass`'s own
/// `debug_assert_eq!(settled, wanted)` fires. The floors collected from zeroed
/// layouts are not the floors a real solve of those rows produces, so the pass
/// does not converge — which is the correctness half, and it arrives before the
/// counter assertions below. Those bound the cost half, and stand for a release
/// build where the assertion is compiled out and the only symptom left is a
/// solve per artboard on every frame.
#[test]
fn the_baseline_pass_solves_the_shown_root_and_not_the_document() {
    let mut ts = typesetter();
    let mut arena = Arena::new();

    // Two artboards, each a HUG baseline row with a tall box and a text run —
    // the #322 shape, which is what makes the floor pass run at all. Different
    // text sizes, so the two rows want different floors and an unshown one
    // cannot be mistaken for the shown one's.
    let mut txn = arena.open();
    // A row added with no parent is a root, so this is the shown root itself —
    // `Txn::show_root` names it by node (issue #943).
    let (first_row, _) = hug_baseline_row(&mut txn, None, 60.0, 18.0);
    let (_, _) = hug_baseline_row(&mut txn, None, 60.0, 34.0);
    txn.show_root(Some(first_row));
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let mut solver = TaffySolver::with_typesetter(&mut ts);
    let mut txn = arena.open();
    txn.set_prop(first_row, Prop::X(1.0));
    txn.commit_with(&mut solver);
    let after_first = solver.solves();

    // A structural rebuild computes the shown root, and the floor pass may
    // re-solve it once to settle its own cross size. Two computations bound
    // that, and neither of them is per-root: an unconfined pass computes both
    // artboards on the re-solve and reports three.
    assert!(
        after_first <= 2,
        "a two-artboard document showing one artboard must not cost a solve per artboard \
         through the baseline pass: it ran {after_first}"
    );

    // And a settled frame stays settled. An invented floor for the unshown row
    // never matches what a real solve of it would produce, so the pass would
    // re-enter its re-solve arm every frame rather than converging.
    let mut txn = arena.open();
    txn.set_prop(first_row, Prop::X(2.0));
    txn.commit_with(&mut solver);
    assert!(
        solver.solves() - after_first <= 2,
        "a later layout frame must cost no more than the first: it ran {}",
        solver.solves() - after_first
    );
    assert_eq!(
        arena.committed().rects().len(),
        3,
        "the shown artboard's row, its box and its text run — and nothing of the other"
    );
}

/// **An owning solver measures through the typesetter it holds, and keeps its
/// retained tree.** Both halves, because either alone is satisfied by a solver
/// this test is not about (story #863).
///
/// `dashlang::attach_live` keeps a `Box<dyn LayoutSolver>` for the life of the
/// scene, so the solver in it is `'static` and cannot borrow a typesetter. Two
/// shapes answer that and only one is correct here:
///
/// - **The measurement half** fails for `TaffySolver::new()`, which holds no
///   typesetter at all: a hug-sized text node then measures as an empty leaf.
///   That is issue #863 itself, and without this assertion the test passes with
///   `owning` replaced by `new` — which it did, until a review said so.
/// - **The retained half** fails for a wrapper that owns the typesetter and
///   builds a fresh `TaffySolver` inside every call. Every solve starts with
///   `state: None`, so Taffy's tree is rebuilt per frame — issue #164's whole
///   saving, paid back per frame, on the path a loaded document takes.
///   `corpus/showcase` carried the last such wrapper and no longer does (issue
///   #950), so this assertion is now the only thing in the tree standing
///   between `owning` and that shape coming back.
///
/// The atlas list is empty on purpose: staging glyphs needs one and measuring
/// does not, so this isolates the measure seam. That the *glyphs* also arrive
/// is asserted where a real cascade exists — `demo`'s
/// `a_loaded_document_draws_text_when_the_host_supplies_fonts`.
#[test]
fn an_owning_solver_measures_through_its_typesetter_and_retains_its_tree() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(root, Prop::SizingV(AxisSizing::Hug));
    let text = text_leaf(&mut txn, root, "LARGE", 18.0);
    txn.commit();

    let mut solver = TaffySolver::owning(dashscene_engine::TextResources::new(
        typesetter(),
        std::sync::Arc::new(Vec::new()),
    ));

    let mut txn = arena.open();
    txn.set_prop(text, Prop::X(1.0));
    txn.commit_with(&mut solver);
    let built = solver.solves();
    assert!(built >= 1, "the first commit builds the tree: {built}");

    let (_, _, w, h) = rect_at(&arena, 1);
    assert!(
        w > 1.0 && h > 1.0,
        "the hug-sized text node measured through the held typesetter, rather than as the \
         empty leaf a solver with no typesetter produces: {w} x {h}"
    );

    // A paint-only commit. The retained tree is not recomputed, so the counter
    // must not move — the assertion a per-call solver fails.
    let mut txn = arena.open();
    txn.set_prop(
        text,
        Prop::Fill(Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    txn.commit_with(&mut solver);
    assert_eq!(
        solver.solves(),
        built,
        "a paint-only commit through a retained tree solves nothing; a solver that rebuilt \
         per call would report another computation here"
    );
}

/// A row that stops being baseline-aligned loses its children's corrections on
/// the **next** solve of a retained solver.
///
/// The corrections live in a retained, stamped table since issue #1153 — dense
/// and allocated once at rebuild, where a fresh `FxHashMap` per solve made this
/// property free. Nothing carries one solve's entries out of it now except a
/// stamp that is bumped rather than a table that is cleared, so this is what
/// says the bump happens: without it the run keeps a baseline offset for a row
/// that no longer has a baseline, and every later frame draws it a descender too
/// low.
///
/// **The row is FIXED height on purpose.** A HUG row records a #322 cross floor,
/// and a solve whose floors differ from the last one's re-solves and starts a
/// second collection — which bumps the stamp on its own and hides the missing
/// per-solve bump entirely. A fixed row records no floor either time, so the
/// pass takes its early return and the per-solve bump is the only one there is.
/// Verified by mutation, and the mutation has to be the right one: `insert`
/// indexes the slots `begin` sizes, so **deleting** the call makes the first
/// collection panic on an empty table and every shape fails alike. The mutation
/// that isolates the bump keeps the sizing and drops the stamp
/// (`offsets.slots.resize(arena.node_count(), (0, 0.0))` in its place). Under
/// that one a HUG twin of this test passes and this fails, with the run at
/// 57.24 where 0 is expected.
///
/// Asserted on the committed rect rather than on the arena's intent, because
/// the correction is a property of the solve and never of the document.
#[test]
fn a_row_that_stops_being_baseline_aligned_drops_its_corrections() {
    let mut ts = typesetter();
    let mut arena = Arena::new();
    let mut solver = TaffySolver::with_typesetter(&mut ts);

    let row = {
        let mut txn = arena.open();
        let row = txn.add_node(None, None);
        txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
        txn.set_prop(row, Prop::SizingH(AxisSizing::Hug));
        // Stated rather than left to `AxisSizing`'s default, because the whole
        // fixture rests on it: see above.
        txn.set_prop(row, Prop::SizingV(AxisSizing::Fixed));
        txn.set_prop(row, Prop::Height(160.0));
        txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
        box_leaf(&mut txn, row, 40.0, 100.0);
        text_leaf(&mut txn, row, "LARGE", 40.0);
        txn.commit_with(&mut solver);
        row
    };

    // And asserted, not only set: a row that had become HUG would have grown
    // past its authored height to the #322 floor, and would then take the
    // second-collection path this fixture exists to avoid.
    assert_eq!(
        rect_at(&arena, 0).3,
        160.0,
        "the row must stay at its authored height, or it is not the fixed shape this pins"
    );

    // The text run is creation index 2 — the row is 0 and the tall box is 1.
    // The box is the tallest baseline, so the run is pushed down by the gap
    // between the two.
    let corrected = rect_at(&arena, 2).1;
    assert!(
        corrected > 1.0,
        "the run must start out baseline-corrected, and sat at {corrected}"
    );

    let mut txn = arena.open();
    txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Start));
    txn.commit_with(&mut solver);

    let uncorrected = rect_at(&arena, 2).1;
    assert!(
        uncorrected.abs() < 0.01,
        "a `Start` row with no padding places its children at 0, and the run sat at \
         {uncorrected} — the previous solve's baseline correction, still applied"
    );
}
