//! v0.2 flex goldens (issue #11): one focused scene per construct —
//! nesting, sizing, clamping, alignment — so a regression implicates one
//! construct (docs/design/architecture.md, bisect-by-construction) rather than one
//! opaque combined image.
//!
//! Scenes are authored against dashscene-core's `Txn` and solved by
//! dashscene-engine's `TaffySolver`. Each test also builds the same
//! scene through `dashlang`'s flex vocabulary and `Scene::build_with`,
//! and asserts the two commits produce identical rects (issue #118;
//! `docs/decisions/negative-gap-lowering.md` D3 recorded the original
//! deferral).
//!
//! Every scene is dimensioned so that each solved rect lands on an
//! integer. Integer-aligned solid fills produce no anti-aliased edges,
//! so these goldens compare exactly — unlike the v0.3 paint goldens,
//! whose gradients and curves need a tolerance
//! (`docs/decisions/golden-comparison-space.md`).

use dashlang::{Node, anon, node, scene};
use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{
    Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, Txn,
};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

const NAVY: Color = rgb(0.05, 0.1, 0.2);
const RED: Color = rgb(0.8, 0.1, 0.1);
const GREEN: Color = rgb(0.1, 0.7, 0.2);
const GOLD: Color = rgb(0.9, 0.7, 0.1);
const BLUE: Color = rgb(0.2, 0.4, 0.9);

/// Adds a fixed-size filled child to `parent`.
fn boxed(txn: &mut Txn<'_>, parent: NodeId, w: f32, h: f32, color: Color) -> NodeId {
    let id = txn.add_node(Some(parent), None);
    txn.set_prop(id, Prop::Width(w));
    txn.set_prop(id, Prop::Height(h));
    txn.set_prop(id, Prop::Fill(color));
    id
}

/// Rect (x, y, w, h) of the DFS index `i` — the same index order
/// dashscene-engine's `tests/solve.rs` uses.
fn rect(arena: &Arena, i: usize) -> (f32, f32, f32, f32) {
    let r = arena.committed().rects()[i];
    (r.x, r.y, r.w, r.h)
}

/// Converts a solved dimension to the `i32` the painter's canvas takes.
/// Every scene in this file is dimensioned so its rects land on
/// integers (module doc), so `v.round()` never moves the value; the
/// assert catches a scene that ever violates that constraint instead of
/// letting `as i32` truncate a fractional value without a signal.
fn exact_dim(v: f32) -> i32 {
    assert_eq!(v, v.round(), "solved dimension {v} is not integral");
    v as i32
}

/// Paints the committed scene on a canvas sized to the root's solved
/// rect (rect index 0) and compares it against the exact-match golden
/// `name`.
fn render_and_compare(arena: &Arena, name: &str) {
    let (_, _, w, h) = rect(arena, 0);
    let scene = arena.committed();
    let mut painter = SkiaPainter::new(exact_dim(w), exact_dim(h));
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    goldens::assert_matches_golden(name, &painter.png_bytes());
}

/// Asserts a DSL-built scene commits to the same rects and paints as
/// its hand-built equivalent — the DSL-equals-hand-built pattern
/// `crates/dashlang/tests/builder.rs`'s `assert_same_output` already
/// established for v0.1, applied here to the four ported v0.2 flex
/// scenes.
fn assert_dsl_matches_hand_built(dsl: &Arena, hand: &Arena) {
    assert_eq!(dsl.committed().rects(), hand.committed().rects());
    assert_eq!(dsl.committed().paints(), hand.committed().paints());
}

#[test]
fn nesting_matches_its_golden() {
    // A 120×80 row of two 50×70 columns, gap 10, padding 5 — the
    // content fits the root exactly: 50 + 10 + 50 = 110 = 120 - (5 + 5).
    // Each column stacks two 50×30 cells with gap 10, which fills the
    // column's height exactly: 30 + 10 + 30 = 70.
    //
    // The cells cover their column edge to edge, so the column's own
    // fill shows only through the 10-high gap between them, and the
    // root's fill shows only through the padding. That is what makes
    // the nesting visible in the image.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(80.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Gap(10.0));
    txn.set_prop(
        root,
        Prop::Padding {
            left: 5.0,
            top: 5.0,
            right: 5.0,
            bottom: 5.0,
        },
    );
    txn.set_prop(root, Prop::Fill(NAVY));

    for (column_fill, cells) in [(RED, [GOLD, GREEN]), (BLUE, [GREEN, GOLD])] {
        let column = txn.add_node(Some(root), None);
        txn.set_prop(column, Prop::Width(50.0));
        txn.set_prop(column, Prop::Height(70.0));
        txn.set_prop(column, Prop::Mode(LayoutMode::Vertical));
        txn.set_prop(column, Prop::Gap(10.0));
        txn.set_prop(column, Prop::Fill(column_fill));
        for cell in cells {
            boxed(&mut txn, column, 50.0, 30.0, cell);
        }
    }
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 80.0), "root");
    assert_eq!(
        rect(&arena, 1),
        (5.0, 5.0, 50.0, 70.0),
        "first column at the padding origin"
    );
    assert_eq!(rect(&arena, 2), (5.0, 5.0, 50.0, 30.0), "its first cell");
    assert_eq!(
        rect(&arena, 3),
        (5.0, 45.0, 50.0, 30.0),
        "its second cell: 5 + 30 + 10"
    );
    assert_eq!(
        rect(&arena, 4),
        (65.0, 5.0, 50.0, 70.0),
        "second column: 5 + 50 + 10"
    );
    assert_eq!(rect(&arena, 5), (65.0, 5.0, 50.0, 30.0), "its first cell");
    assert_eq!(rect(&arena, 6), (65.0, 45.0, 50.0, 30.0), "its second cell");

    let mut dsl = Arena::new();
    let dsl_column = |fill: Color, cells: [Color; 2]| {
        node("column")
            .size(50.0, 70.0)
            .mode(LayoutMode::Vertical)
            .gap(10.0)
            .fill(fill)
            .children(
                cells
                    .into_iter()
                    .map(|cell| anon().size(50.0, 30.0).fill(cell)),
            )
    };
    scene([node("root")
        .size(120.0, 80.0)
        .mode(LayoutMode::Horizontal)
        .gap(10.0)
        .padding(5.0, 5.0, 5.0, 5.0)
        .fill(NAVY)
        .child(dsl_column(RED, [GOLD, GREEN]))
        .child(dsl_column(BLUE, [GREEN, GOLD]))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_dsl_matches_hand_built(&dsl, &arena);

    render_and_compare(&arena, "v02-nesting");
}

#[test]
fn sizing_matches_its_golden() {
    // A 120×60 row: a Hug node followed by two Fill siblings.
    //
    // The Hug node has no authored width — it takes its 30-wide child's
    // width. That leaves 120 - 30 = 90 of free space, which the two Fill
    // siblings split equally (45 each): core has no fill weight, and the
    // engine maps every Fill to flex_grow = 1.
    //
    // The Hug node's child is only 40 high against the node's 60, so the
    // node's own fill shows below it — otherwise the child would cover
    // the node exactly and the hug box would be invisible in the image.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(60.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Fill(NAVY));

    let hug = txn.add_node(Some(root), Some("hug"));
    txn.set_prop(hug, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::Height(60.0));
    txn.set_prop(hug, Prop::Fill(RED));
    boxed(&mut txn, hug, 30.0, 40.0, GOLD);

    for fill_color in [GREEN, BLUE] {
        let fill = txn.add_node(Some(root), None);
        txn.set_prop(fill, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(fill, Prop::Height(60.0));
        txn.set_prop(fill, Prop::Fill(fill_color));
    }
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 60.0), "root");
    assert_eq!(
        rect(&arena, 1),
        (0.0, 0.0, 30.0, 60.0),
        "hug takes its content's width"
    );
    assert_eq!(
        rect(&arena, 2),
        (0.0, 0.0, 30.0, 40.0),
        "the hug node's fixed child"
    );
    assert_eq!(
        rect(&arena, 3),
        (30.0, 0.0, 45.0, 60.0),
        "first Fill: (120 - 30) / 2"
    );
    assert_eq!(
        rect(&arena, 4),
        (75.0, 0.0, 45.0, 60.0),
        "second Fill: the equal split"
    );

    let mut dsl = Arena::new();
    scene([node("root")
        .size(120.0, 60.0)
        .mode(LayoutMode::Horizontal)
        .fill(NAVY)
        .child(
            node("hug")
                .mode(LayoutMode::Horizontal)
                .sizing_h(AxisSizing::Hug)
                .size(0.0, 60.0)
                .fill(RED)
                .child(anon().size(30.0, 40.0).fill(GOLD)),
        )
        .children([GREEN, BLUE].into_iter().map(|color| {
            anon()
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 60.0)
                .fill(color)
        }))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_dsl_matches_hand_built(&dsl, &arena);

    render_and_compare(&arena, "v02-sizing");
}

/// A 120×30 row of two Fill children, the first carrying `clamp`.
/// Unclamped the two would split 60/60, so the row shows exactly what
/// the clamp changed.
fn clamped_row(txn: &mut Txn<'_>, root: NodeId, clamp: Prop, first: Color, second: Color) {
    let row = txn.add_node(Some(root), None);
    txn.set_prop(row, Prop::Width(120.0));
    txn.set_prop(row, Prop::Height(30.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));

    let clamped = txn.add_node(Some(row), None);
    txn.set_prop(clamped, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(clamped, clamp);
    txn.set_prop(clamped, Prop::Height(30.0));
    txn.set_prop(clamped, Prop::Fill(first));

    let rest = txn.add_node(Some(row), None);
    txn.set_prop(rest, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(rest, Prop::Height(30.0));
    txn.set_prop(rest, Prop::Fill(second));
}

fn dsl_clamped_row(clamp: impl FnOnce(Node) -> Node, first: Color, second: Color) -> Node {
    node("row")
        .size(120.0, 30.0)
        .mode(LayoutMode::Horizontal)
        .child(clamp(
            anon()
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .fill(first),
        ))
        .child(
            anon()
                .sizing_h(AxisSizing::Fill)
                .size(0.0, 30.0)
                .fill(second),
        )
}

#[test]
fn clamping_matches_its_golden() {
    // A 120×60 column of two 120×30 rows. Both rows hold two Fill
    // children that would split 60/60; the clamp on the first child
    // moves the split in each direction, and the freed space goes to the
    // unclamped sibling:
    //   row one, MaxWidth 40  ->  40 / 80
    //   row two, MinWidth 100 -> 100 / 20
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(60.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(root, Prop::Fill(NAVY));

    clamped_row(&mut txn, root, Prop::MaxWidth(40.0), RED, GREEN);
    clamped_row(&mut txn, root, Prop::MinWidth(100.0), GOLD, BLUE);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 60.0), "root");
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 120.0, 30.0), "the max row");
    assert_eq!(
        rect(&arena, 2),
        (0.0, 0.0, 40.0, 30.0),
        "capped at MaxWidth 40"
    );
    assert_eq!(
        rect(&arena, 3),
        (40.0, 0.0, 80.0, 30.0),
        "its sibling takes the rest"
    );
    assert_eq!(rect(&arena, 4), (0.0, 30.0, 120.0, 30.0), "the min row");
    assert_eq!(
        rect(&arena, 5),
        (0.0, 30.0, 100.0, 30.0),
        "floored at MinWidth 100"
    );
    assert_eq!(
        rect(&arena, 6),
        (100.0, 30.0, 20.0, 30.0),
        "its sibling keeps only 20"
    );

    let mut dsl = Arena::new();
    scene([node("root")
        .size(120.0, 60.0)
        .mode(LayoutMode::Vertical)
        .fill(NAVY)
        .child(dsl_clamped_row(|n| n.max_width(40.0), RED, GREEN))
        .child(dsl_clamped_row(|n| n.min_width(100.0), GOLD, BLUE))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_dsl_matches_hand_built(&dsl, &arena);

    render_and_compare(&arena, "v02-clamping");
}

/// A 160×20 row holding two 30×10 children with gap 10, under the given
/// alignments and padding. Content is 30 + 10 + 30 = 70 wide.
fn align_row(
    txn: &mut Txn<'_>,
    root: NodeId,
    main: MainAxisAlign,
    cross: CrossAxisAlign,
    padding: Option<(f32, f32, f32, f32)>,
    colors: [Color; 2],
) {
    let row = txn.add_node(Some(root), None);
    txn.set_prop(row, Prop::Width(160.0));
    txn.set_prop(row, Prop::Height(20.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(10.0));
    if let Some((left, top, right, bottom)) = padding {
        txn.set_prop(
            row,
            Prop::Padding {
                left,
                top,
                right,
                bottom,
            },
        );
    }
    txn.set_prop(row, Prop::MainAlign(main));
    txn.set_prop(row, Prop::CrossAlign(cross));
    for color in colors {
        boxed(txn, row, 30.0, 10.0, color);
    }
}

fn dsl_align_row(
    main: MainAxisAlign,
    cross: CrossAxisAlign,
    padding: Option<(f32, f32, f32, f32)>,
    colors: [Color; 2],
) -> Node {
    let row = node("row")
        .size(160.0, 20.0)
        .mode(LayoutMode::Horizontal)
        .gap(10.0)
        .main_align(main)
        .cross_align(cross)
        .children(colors.into_iter().map(|c| anon().size(30.0, 10.0).fill(c)));
    match padding {
        Some((left, top, right, bottom)) => row.padding(left, top, right, bottom),
        None => row,
    }
}

#[test]
fn alignment_matches_its_golden() {
    // A 160×80 column of four 160×20 rows, one per alignment pairing.
    // The rows carry no fill, so the root's navy shows through and each
    // row's two children read as blocks against it.
    //
    // Main-axis free space in an unpadded row is 160 - 70 = 90 and
    // cross-axis free space is 20 - 10 = 10, so both centre offsets are
    // whole numbers (45 and 5).
    //
    //   row 0, y = 0    Start / Start, padding (10, 2, 10, 2)
    //   row 1, y = 20   Center / Center
    //   row 2, y = 40   End / End
    //   row 3, y = 60   SpaceBetween / Center
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(160.0));
    txn.set_prop(root, Prop::Height(80.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(root, Prop::Fill(NAVY));

    align_row(
        &mut txn,
        root,
        MainAxisAlign::Start,
        CrossAxisAlign::Start,
        Some((10.0, 2.0, 10.0, 2.0)),
        [RED, GOLD],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::Center,
        CrossAxisAlign::Center,
        None,
        [GREEN, BLUE],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::End,
        CrossAxisAlign::End,
        None,
        [GOLD, RED],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::SpaceBetween,
        CrossAxisAlign::Center,
        None,
        [BLUE, GREEN],
    );
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 160.0, 80.0), "root");

    // Start / Start, padded: content begins at the padding origin.
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 160.0, 20.0), "row 0");
    assert_eq!(
        rect(&arena, 2),
        (10.0, 2.0, 30.0, 10.0),
        "start, at the left padding"
    );
    assert_eq!(rect(&arena, 3), (50.0, 2.0, 30.0, 10.0), "10 + 30 + 10 gap");

    // Center / Center: 90 free on the main axis, 10 on the cross.
    assert_eq!(rect(&arena, 4), (0.0, 20.0, 160.0, 20.0), "row 1");
    assert_eq!(
        rect(&arena, 5),
        (45.0, 25.0, 30.0, 10.0),
        "centered: 90 / 2"
    );
    assert_eq!(
        rect(&arena, 6),
        (85.0, 25.0, 30.0, 10.0),
        "45 + 30 + 10 gap"
    );

    // End / End: content is flush with the right and bottom edges.
    assert_eq!(rect(&arena, 7), (0.0, 40.0, 160.0, 20.0), "row 2");
    assert_eq!(rect(&arena, 8), (90.0, 50.0, 30.0, 10.0), "end: 160 - 70");
    assert_eq!(
        rect(&arena, 9),
        (130.0, 50.0, 30.0, 10.0),
        "flush right: 160 - 30"
    );

    // SpaceBetween: the free space becomes the space between the two,
    // so the authored gap is subsumed by it.
    assert_eq!(rect(&arena, 10), (0.0, 60.0, 160.0, 20.0), "row 3");
    assert_eq!(rect(&arena, 11), (0.0, 65.0, 30.0, 10.0), "flush left");
    assert_eq!(rect(&arena, 12), (130.0, 65.0, 30.0, 10.0), "flush right");

    let mut dsl = Arena::new();
    scene([node("root")
        .size(160.0, 80.0)
        .mode(LayoutMode::Vertical)
        .fill(NAVY)
        .child(dsl_align_row(
            MainAxisAlign::Start,
            CrossAxisAlign::Start,
            Some((10.0, 2.0, 10.0, 2.0)),
            [RED, GOLD],
        ))
        .child(dsl_align_row(
            MainAxisAlign::Center,
            CrossAxisAlign::Center,
            None,
            [GREEN, BLUE],
        ))
        .child(dsl_align_row(
            MainAxisAlign::End,
            CrossAxisAlign::End,
            None,
            [GOLD, RED],
        ))
        .child(dsl_align_row(
            MainAxisAlign::SpaceBetween,
            CrossAxisAlign::Center,
            None,
            [BLUE, GREEN],
        ))])
    .build_with(&mut dsl, &mut TaffySolver::new());

    assert_dsl_matches_hand_built(&dsl, &arena);

    render_and_compare(&arena, "v02-alignment");
}
