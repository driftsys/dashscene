//! Boundary-B contract tests against hand-built fixtures (issue #3):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{Color, PaintKind, PaintTable, Painter, RectEntry};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const HALF_BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 0.5,
};

#[test]
fn paint_table_push_returns_sequential_indices_and_get_resolves_them() {
    let mut table = PaintTable::new();
    assert!(table.is_empty());

    let red = table.push(PaintKind::Solid { color: RED });
    let blue = table.push(PaintKind::Solid { color: HALF_BLUE });

    assert_eq!(red, 0);
    assert_eq!(blue, 1);
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(red), Some(&PaintKind::Solid { color: RED }));
    assert_eq!(
        table.get(blue),
        Some(&PaintKind::Solid { color: HALF_BLUE })
    );
}

#[test]
fn paint_table_get_past_the_end_returns_none() {
    let mut table = PaintTable::new();
    table.push(PaintKind::Solid { color: RED });

    assert_eq!(table.get(1), None);
    assert_eq!(table.get(u32::MAX), None);
}

#[test]
fn paint_table_resolve_returns_the_entry() {
    let mut table = PaintTable::new();
    let red = table.push(PaintKind::Solid { color: RED });

    assert_eq!(table.resolve(red), &PaintKind::Solid { color: RED });
}

#[test]
#[should_panic(expected = "paint index 1 out of range")]
fn paint_table_resolve_panics_on_an_out_of_range_index() {
    let mut table = PaintTable::new();
    table.push(PaintKind::Solid { color: RED });

    table.resolve(1);
}

/// Test double: resolves each rect's paint index and records what a real
/// painter would color. A painter only colors (P2) — so recording
/// (rect, resolved color) pairs is a complete observation of the contract.
#[derive(Default)]
struct RecordingPainter {
    painted: Vec<(RectEntry, Color)>,
}

impl Painter for RecordingPainter {
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable) {
        for rect in rects {
            let PaintKind::Solid { color } = paints.resolve(rect.paint);
            self.painted.push((*rect, *color));
        }
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let red = paints.push(PaintKind::Solid { color: RED });
    let blue = paints.push(PaintKind::Solid { color: HALF_BLUE });
    let rects = vec![
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            paint: red,
        },
        RectEntry {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            paint: blue,
        },
    ];
    (rects, paints)
}

#[test]
fn painter_receives_rects_in_slice_order_with_resolved_colors() {
    let (rects, paints) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(&rects, &paints);

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}

#[test]
fn painter_trait_is_object_safe() {
    let (rects, paints) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    let dyn_painter: &mut dyn Painter = &mut painter;
    dyn_painter.paint(&rects, &paints);

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}
