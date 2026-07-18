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
    Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, NodeId, Prop, TextStyle,
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
