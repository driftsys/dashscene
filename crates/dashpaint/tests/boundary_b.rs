//! Boundary-B contract tests against hand-built fixtures (issues #3, #13):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, PaintEntry, PaintIndex, PaintKind,
    PaintTable, Painter, RectEntry, ScaleMode, Stroke, StrokeAlign, Vec2,
};

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

    let red = table.push(PaintEntry::solid(RED));
    let blue = table.push(PaintEntry::solid(HALF_BLUE));

    assert_eq!(red, PaintIndex(0));
    assert_eq!(blue, PaintIndex(1));
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(red), Some(&PaintEntry::solid(RED)));
    assert_eq!(table.get(blue), Some(&PaintEntry::solid(HALF_BLUE)));
}

#[test]
fn paint_table_get_past_the_end_returns_none() {
    let mut table = PaintTable::new();
    table.push(PaintEntry::solid(RED));

    assert_eq!(table.get(PaintIndex(1)), None);
    assert_eq!(table.get(PaintIndex(u32::MAX)), None);
}

#[test]
fn paint_table_resolve_returns_the_entry() {
    let mut table = PaintTable::new();
    let red = table.push(PaintEntry::solid(RED));

    assert_eq!(table.resolve(red), &PaintEntry::solid(RED));
}

#[test]
#[should_panic(expected = "paint index 1 out of range")]
fn paint_table_resolve_panics_on_an_out_of_range_index() {
    let mut table = PaintTable::new();
    table.push(PaintEntry::solid(RED));

    table.resolve(PaintIndex(1));
}

#[test]
fn paint_entry_solid_is_fill_only() {
    let entry = PaintEntry::solid(RED);

    assert_eq!(entry.fill, Some(PaintKind::Solid { color: RED }));
    assert_eq!(entry.stroke, None);
    assert_eq!(entry.corners, CornerRadii::default());
    assert!(!entry.clip);
}

#[test]
fn a_paint_less_entry_pushes_and_resolves() {
    let mut table = PaintTable::new();
    let index = table.push(PaintEntry::default());

    assert_eq!(table.resolve(index).fill, None);
}

#[test]
fn a_full_entry_round_trips_through_the_table() {
    let gradient = Gradient {
        kind: GradientKind::Radial,
        handle_origin: Vec2 { x: 0.5, y: 0.5 },
        handle_primary: Vec2 { x: 1.0, y: 0.5 },
        handle_secondary: Vec2 { x: 0.5, y: 1.0 },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: HALF_BLUE,
            },
        ],
    };
    let entry = PaintEntry {
        fill: Some(PaintKind::Gradient(gradient.clone())),
        stroke: Some(Stroke {
            width: 2.0,
            align: StrokeAlign::Inside,
            color: RED,
        }),
        corners: CornerRadii {
            top_left: 1.0,
            top_right: 2.0,
            bottom_right: 3.0,
            bottom_left: 4.0,
        },
        clip: true,
    };
    let mut table = PaintTable::new();
    let index = table.push(entry.clone());

    assert_eq!(table.resolve(index), &entry);
}

#[test]
fn an_image_fill_round_trips_through_the_table() {
    let entry = PaintEntry {
        fill: Some(PaintKind::Image {
            image: 7,
            scale_mode: ScaleMode::Crop,
        }),
        ..PaintEntry::default()
    };
    let mut table = PaintTable::new();
    let index = table.push(entry.clone());

    assert_eq!(table.resolve(index), &entry);
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
            match &paints.resolve(rect.paint).fill {
                Some(PaintKind::Solid { color }) => self.painted.push((*rect, *color)),
                other => panic!("fixture only paints solids, got {other:?}"),
            }
        }
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let red = paints.push(PaintEntry::solid(RED));
    let blue = paints.push(PaintEntry::solid(HALF_BLUE));
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

#[test]
fn paint_index_is_transparent_over_u32() {
    assert_eq!(std::mem::size_of::<PaintIndex>(), 4);
    assert_eq!(std::mem::size_of::<RectEntry>(), 20);
}
