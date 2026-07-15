//! Boundary-B contract tests against hand-built fixtures (issues #3, #13):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{
    ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CornerRadii, Gradient, GradientKind,
    GradientStop, ImageAsset, ImageFormat, ImageTable, PaintEntry, PaintIndex, PaintKind,
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
            transform: None,
            tile_scale: 1.0,
        }),
        ..PaintEntry::default()
    };
    let mut table = PaintTable::new();
    let index = table.push(entry.clone());

    assert_eq!(table.resolve(index), &entry);
}

#[test]
fn image_table_pushes_and_resolves_assets() {
    let mut images = ImageTable::new();
    assert!(images.is_empty());

    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: vec![1, 2, 3],
    };
    let index = images.push(asset.clone());

    assert_eq!(index, 0);
    assert_eq!(images.len(), 1);
    assert_eq!(images.resolve(index), &asset);
    assert_eq!(images.get(1), None);
}

#[test]
#[should_panic(expected = "image index 3 out of range")]
fn image_table_resolve_panics_on_an_out_of_range_index() {
    ImageTable::new().resolve(3);
}

const SHARP: ClipBox = ClipBox {
    x: 0.0,
    y: 0.0,
    w: 10.0,
    h: 10.0,
    corners: CornerRadii {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    },
};

#[test]
fn a_new_clip_table_reserves_the_unclipped_region_at_index_zero() {
    let clips = ClipTable::new();

    assert_eq!(clips.len(), 1);
    assert_eq!(ClipIndex::UNCLIPPED, ClipIndex(0));
    assert!(clips.resolve(ClipIndex::UNCLIPPED).is_unclipped());
    assert!(clips.resolve(ClipIndex::UNCLIPPED).boxes().is_empty());
}

#[test]
fn clip_table_push_returns_sequential_indices_and_get_resolves_them() {
    let mut clips = ClipTable::new();

    let one = clips.push(ClipRegion::new(vec![SHARP]));
    let two = clips.push(ClipRegion::new(vec![SHARP, SHARP]));

    assert_eq!(one, ClipIndex(1));
    assert_eq!(two, ClipIndex(2));
    assert_eq!(clips.len(), 3);
    assert_eq!(clips.resolve(one).boxes(), &[SHARP]);
    assert_eq!(
        clips.get(two).map(ClipRegion::boxes),
        Some(&[SHARP, SHARP][..])
    );
    assert_eq!(clips.get(ClipIndex(3)), None);
}

#[test]
fn a_region_with_boxes_is_not_unclipped() {
    assert!(ClipRegion::unclipped().is_unclipped());
    assert!(!ClipRegion::new(vec![SHARP]).is_unclipped());
}

#[test]
#[should_panic(expected = "clip index 4 out of range")]
fn clip_table_resolve_panics_on_an_out_of_range_index() {
    ClipTable::new().resolve(ClipIndex(4));
}

/// Test double: resolves each rect's paint index and clip index, and
/// records what a real painter would color and clip against. A painter
/// only colors (P2) — so recording (rect, resolved color) pairs plus the
/// resolved region is a complete observation of the contract.
#[derive(Default)]
struct RecordingPainter {
    painted: Vec<(RectEntry, Color)>,
    clipped: Vec<ClipRegion>,
}

impl Painter for RecordingPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        _images: &ImageTable,
        clips: &ClipTable,
        _dirty: Option<&[u32]>,
    ) {
        for rect in rects {
            match &paints.resolve(rect.paint).fill {
                Some(PaintKind::Solid { color }) => self.painted.push((*rect, *color)),
                other => panic!("fixture only paints solids, got {other:?}"),
            }
            self.clipped.push(clips.resolve(rect.clip).clone());
        }
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable, ClipTable) {
    let mut paints = PaintTable::new();
    let red = paints.push(PaintEntry::solid(RED));
    let blue = paints.push(PaintEntry::solid(HALF_BLUE));
    let mut clips = ClipTable::new();
    // The second rect sits inside the first, which clips it.
    let inside_first = clips.push(ClipRegion::new(vec![ClipBox {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 50.0,
        corners: CornerRadii::default(),
    }]));
    let rects = vec![
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
        },
        RectEntry {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            paint: blue,
            clip: inside_first,
        },
    ];
    (rects, paints, clips)
}

#[test]
fn painter_receives_rects_in_slice_order_with_resolved_colors() {
    let (rects, paints, clips) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(&rects, &paints, &ImageTable::new(), &clips, None);

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}

#[test]
fn painter_resolves_each_rects_clip_region() {
    let (rects, paints, clips) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(&rects, &paints, &ImageTable::new(), &clips, None);

    assert!(painter.clipped[0].is_unclipped());
    assert_eq!(
        painter.clipped[1].boxes(),
        &[ClipBox {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            corners: CornerRadii::default(),
        }]
    );
}

#[test]
fn painter_trait_is_object_safe() {
    let (rects, paints, clips) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    let dyn_painter: &mut dyn Painter = &mut painter;
    dyn_painter.paint(&rects, &paints, &ImageTable::new(), &clips, None);

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}

#[test]
fn paint_index_is_transparent_over_u32() {
    assert_eq!(std::mem::size_of::<PaintIndex>(), 4);
    assert_eq!(std::mem::size_of::<ClipIndex>(), 4);
    assert_eq!(std::mem::size_of::<RectEntry>(), 24);
    assert_eq!(std::mem::align_of::<RectEntry>(), 4);
    assert_eq!(std::mem::size_of::<Color>(), 16);
    assert_eq!(std::mem::align_of::<Color>(), 4);
    assert_eq!(std::mem::size_of::<ClipBox>(), 32);
    assert_eq!(std::mem::align_of::<ClipBox>(), 4);
}

/// The dirty set is advisory, but it must reach the painter. A painter
/// that wants to honour R-T4 cannot do so if boundary B does not carry
/// the set (`docs/decisions/dirty-set-advisory-across-boundary-b.md`).
#[derive(Default)]
struct DirtyRecordingPainter {
    seen_dirty: Option<Vec<u32>>,
    seen_rects: usize,
}

impl Painter for DirtyRecordingPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        _paints: &PaintTable,
        _images: &ImageTable,
        _clips: &ClipTable,
        dirty: Option<&[u32]>,
    ) {
        self.seen_dirty = dirty.map(<[u32]>::to_vec);
        self.seen_rects = rects.len();
    }
}

#[test]
fn the_dirty_set_crosses_boundary_b() {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry::solid(RED));
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
    }];

    let mut painter = DirtyRecordingPainter::default();

    // A caller with a committed scene passes the set it produced.
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        Some(&[0]),
    );
    assert_eq!(painter.seen_dirty.as_deref(), Some(&[0u32][..]));
    assert_eq!(painter.seen_rects, 1);

    // A caller with hand-built tables has no dirty information.
    painter.paint(&rects, &paints, &ImageTable::new(), &ClipTable::new(), None);
    assert_eq!(painter.seen_dirty, None);
}
