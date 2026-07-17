//! v0.8 layout-fidelity goldens (story #43): one focused scene per new
//! construct — wrap, grid with spans, baseline — the same
//! per-construct split as the v0.2 flex goldens
//! (`docs/decisions/v02-flex-goldens-per-construct.md`), so a
//! regression implicates one construct.
//!
//! Every scene is dimensioned so that each solved rect lands on an
//! integer; integer-aligned solid fills produce no anti-aliased edges,
//! so these goldens compare exactly, extending the v0.2 rule. Scenes
//! are hand-built only: `dashlang` has no wrap/grid vocabulary yet, so
//! there is no DSL side to compare (unlike `v02_flex.rs`, whose DSL
//! assertions arrived with issue #118's vocabulary).

use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{
    Arena, AxisSizing, Color, CrossAxisAlign, GridTrack, LayoutMode, NodeId, Prop, Txn,
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

/// Rect (x, y, w, h) of the DFS index `i`.
fn rect(arena: &Arena, i: usize) -> (f32, f32, f32, f32) {
    let r = arena.committed().rects()[i];
    (r.x, r.y, r.w, r.h)
}

/// Converts a solved dimension to the `i32` the painter's canvas takes,
/// asserting the scene kept its integer-dimensioning promise.
fn exact_dim(v: f32) -> i32 {
    assert_eq!(v, v.round(), "solved dimension {v} is not integral");
    v as i32
}

/// Paints the committed scene on a canvas sized to the root's solved
/// rect and compares it against the exact-match golden `name`.
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

#[test]
fn wrap_matches_its_golden() {
    // A fixed-width (200), hug-height wrap row, padding 10, gap 10,
    // cross gap 20. The inner width is 180: 80 + 10 + 60 = 150 fits,
    // + 10 + 70 does not, so the row breaks into [80, 60] and [70, 50].
    // The hug height is 10 + 30 + 20 + 30 + 10 = 100. The distinct
    // cross gap (20) is what the image shows against the main gap (10).
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Mode(LayoutMode::Wrap));
    txn.set_prop(root, Prop::Width(200.0));
    txn.set_prop(root, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(root, Prop::Gap(10.0));
    txn.set_prop(root, Prop::CrossGap(20.0));
    txn.set_prop(
        root,
        Prop::Padding {
            left: 10.0,
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
        },
    );
    txn.set_prop(root, Prop::Fill(NAVY));
    for (w, color) in [(80.0, RED), (60.0, GOLD), (70.0, GREEN), (50.0, BLUE)] {
        boxed(&mut txn, root, w, 30.0, color);
    }
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 200.0, 100.0), "hug root");
    assert_eq!(rect(&arena, 1), (10.0, 10.0, 80.0, 30.0), "line 1");
    assert_eq!(rect(&arena, 2), (100.0, 10.0, 60.0, 30.0), "80 + 10 gap");
    assert_eq!(rect(&arena, 3), (10.0, 60.0, 70.0, 30.0), "wrapped line 2");
    assert_eq!(rect(&arena, 4), (90.0, 60.0, 50.0, 30.0), "70 + 10 gap");

    render_and_compare(&arena, "v08-wrap");
}

#[test]
fn grid_spans_match_their_golden() {
    // A fixed 200×160 grid, padding 10, both gaps 10, columns
    // [60px, 1fr, 1fr] and rows [40px, 1fr, 1fr]: the fraction columns
    // take (200 − 20 − 20 − 60) / 2 = 50 and the fraction rows
    // (160 − 20 − 20 − 40) / 2 = 40. A header spans all three columns,
    // a tall cell spans two rows, a footer spans two columns, and a
    // fixed 30×20 box sits at its cell origin instead of stretching.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Mode(LayoutMode::Grid));
    txn.set_prop(root, Prop::Width(200.0));
    txn.set_prop(root, Prop::Height(160.0));
    txn.set_prop(root, Prop::Gap(10.0));
    txn.set_prop(root, Prop::CrossGap(10.0));
    txn.set_prop(
        root,
        Prop::Padding {
            left: 10.0,
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
        },
    );
    txn.set_prop(root, Prop::Fill(NAVY));
    txn.set_prop(
        root,
        Prop::GridColumns(vec![
            GridTrack::Fixed(60.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ]),
    );
    txn.set_prop(
        root,
        Prop::GridRows(vec![
            GridTrack::Fixed(40.0),
            GridTrack::Fraction(1.0),
            GridTrack::Fraction(1.0),
        ]),
    );

    let cell = |txn: &mut Txn<'_>, row: u16, column: u16, color: Color| {
        let id = txn.add_node(Some(root), None);
        txn.set_prop(id, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(id, Prop::SizingV(AxisSizing::Fill));
        txn.set_prop(id, Prop::GridRow(row));
        txn.set_prop(id, Prop::GridColumn(column));
        txn.set_prop(id, Prop::Fill(color));
        id
    };
    let header = cell(&mut txn, 0, 0, RED);
    txn.set_prop(header, Prop::GridColumnSpan(3));
    let tall = cell(&mut txn, 1, 0, GOLD);
    txn.set_prop(tall, Prop::GridRowSpan(2));
    cell(&mut txn, 1, 1, GREEN);
    let footer = cell(&mut txn, 2, 1, BLUE);
    txn.set_prop(footer, Prop::GridColumnSpan(2));
    let fixed = boxed(&mut txn, root, 30.0, 20.0, GREEN);
    txn.set_prop(fixed, Prop::GridRow(1));
    txn.set_prop(fixed, Prop::GridColumn(2));
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 200.0, 160.0), "grid root");
    assert_eq!(rect(&arena, 1), (10.0, 10.0, 180.0, 40.0), "header spans 3");
    assert_eq!(
        rect(&arena, 2),
        (10.0, 60.0, 60.0, 90.0),
        "tall spans 2 rows"
    );
    assert_eq!(rect(&arena, 3), (80.0, 60.0, 50.0, 40.0), "plain fill cell");
    assert_eq!(
        rect(&arena, 4),
        (80.0, 110.0, 110.0, 40.0),
        "footer spans 2 columns"
    );
    assert_eq!(
        rect(&arena, 5),
        (140.0, 60.0, 30.0, 20.0),
        "fixed box at its cell origin"
    );

    render_and_compare(&arena, "v08-grid-spans");
}

#[test]
fn baseline_matches_its_golden() {
    // A fixed 140×60 row, gap 10, baseline-aligned. Leaf baselines are
    // bottom edges, so the three mixed-height boxes align their bottoms
    // at the tallest child's: y = 48 − height, i.e. 28, 0, 16.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Width(140.0));
    txn.set_prop(root, Prop::Height(60.0));
    txn.set_prop(root, Prop::Gap(10.0));
    txn.set_prop(root, Prop::CrossAlign(CrossAxisAlign::Baseline));
    txn.set_prop(root, Prop::Fill(NAVY));
    boxed(&mut txn, root, 30.0, 20.0, RED);
    boxed(&mut txn, root, 40.0, 48.0, GOLD);
    boxed(&mut txn, root, 30.0, 32.0, GREEN);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 140.0, 60.0), "root");
    assert_eq!(rect(&arena, 1), (0.0, 28.0, 30.0, 20.0), "short box");
    assert_eq!(rect(&arena, 2), (40.0, 0.0, 40.0, 48.0), "tall box");
    assert_eq!(rect(&arena, 3), (90.0, 16.0, 30.0, 32.0), "middle box");

    render_and_compare(&arena, "v08-baseline");
}
