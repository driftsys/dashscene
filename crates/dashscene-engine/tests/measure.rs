//! Story #29 acceptance: the Taffy measure callback reads the
//! shaped-run cache so text drives hug sizing (docs/design/dashscene-engine.md).
//!
//! A hug-sized text node lays out to the width and height its
//! typesetter shapes — and to the exact numbers a direct
//! `Typesetter::layout` produces, because layout and paint read one
//! cache and so cannot disagree about a glyph's size.

use dashscene_core::{Arena, AxisSizing, Color, LayoutMode, Prop, TextStyle};
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

fn styled(txn: &mut dashscene_core::Txn<'_>, node: dashscene_core::NodeId, text: &str, size: f32) {
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

#[test]
fn a_hug_text_node_lays_out_to_its_shaped_width_and_height() {
    let text = "Hello";
    let size = 32.0;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(node, Prop::SizingV(AxisSizing::Hug));
    styled(&mut txn, node, text, size);

    let mut ts = typesetter();
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let rect = arena.committed().rects()[0];

    // The same cache, measured directly: an unconstrained hug node
    // imposes no wrap width, so the paragraph lays out on one line.
    let expected = ts.layout(text, size, None);
    assert!(expected.width > 0.0, "shaped text has a positive width");
    assert!(expected.height > 0.0, "one line has a positive height");
    assert_eq!(
        (rect.w, rect.h),
        (expected.width, expected.height),
        "the hug node's solved size is the typesetter's shaped size, bit for bit"
    );
}

#[test]
fn a_hug_column_hugs_to_its_text_child() {
    // The measured leaf feeds flex content sizing: a hug column with a
    // single text child sizes to that child, so text drives the hug of
    // the container around it, not just the leaf's own rect.
    let text = "Hello";
    let size = 32.0;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let col = txn.add_node(None, None);
    txn.set_prop(col, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(col, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(col, Prop::SizingV(AxisSizing::Hug));
    let label = txn.add_node(Some(col), None);
    txn.set_prop(label, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(label, Prop::SizingV(AxisSizing::Hug));
    styled(&mut txn, label, text, size);

    let mut ts = typesetter();
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let expected = ts.layout(text, size, None);
    let col_rect = arena.committed().rects()[0];
    let label_rect = arena.committed().rects()[1];
    assert_eq!(
        (label_rect.w, label_rect.h),
        (expected.width, expected.height),
        "the text leaf hugs its shaped run"
    );
    assert_eq!(
        (col_rect.w, col_rect.h),
        (expected.width, expected.height),
        "the column hugs to its text child"
    );
}

#[test]
fn a_width_constrained_text_node_wraps_and_grows_taller() {
    // A width that fits one word but not the whole string forces a
    // greedy wrap; the hug height then covers both lines. This drives
    // measure_text's known-width path — the fixed width is returned
    // unchanged and only the height comes from shaping.
    let text = "Hello world";
    let size = 32.0;

    // Shape once to choose a width between one word and the full line.
    let one_line = typesetter().layout(text, size, None);
    let wrap_width = one_line.width * 0.75;

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    txn.set_prop(node, Prop::SizingH(AxisSizing::Fixed));
    txn.set_prop(node, Prop::Width(wrap_width));
    txn.set_prop(node, Prop::SizingV(AxisSizing::Hug));
    styled(&mut txn, node, text, size);

    let mut ts = typesetter();
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    let expected = ts.layout(text, size, Some(wrap_width));
    assert!(
        expected.height > one_line.height,
        "the text wrapped to more than one line"
    );
    let rect = arena.committed().rects()[0];
    assert_eq!(
        rect.w, wrap_width,
        "the fixed width is unchanged by measure"
    );
    assert_eq!(
        rect.h, expected.height,
        "the hug height covers the wrapped lines"
    );
}
