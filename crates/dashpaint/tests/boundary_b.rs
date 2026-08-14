//! Boundary-B contract tests against hand-built fixtures (issues #3, #13):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, Blur, BlurKind, BlurRange, ClipBox, ClipIndex, ClipRegion,
    ClipTable, Color, CornerRadii, EntryParts, Fill, FillSpec, GlyphQuad, GlyphRange, GlyphRun,
    GlyphRunTable, Gradient, GradientKind, GradientStop, GroupComposite, ImageAsset, ImageFill,
    ImageFormat, ImageTable, Mat23, PaintEntry, PaintIndex, PaintKind, PaintTable, PaintTag,
    Painter, RectEntry, ScaleMode, Shadow, ShadowKind, ShadowRange, ShapeRange, StopRange, Stroke,
    StrokeAlign, StrokeRange, Vec2, VectorField,
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

    let red = table.push_solid(RED);
    let blue = table.push_solid(HALF_BLUE);

    assert_eq!(red, PaintIndex(0));
    assert_eq!(blue, PaintIndex(1));
    assert_eq!(table.len(), 2);
    // Compared through the fills the entries name rather than by rebuilding
    // an equal `PaintEntry`: a `PaintKind` is a row index, so an entry only
    // means anything against the table that interned it (story #578).
    assert_eq!(table.fill(table.resolve(red).fill), Fill::Solid(RED));
    assert_eq!(table.fill(table.resolve(blue).fill), Fill::Solid(HALF_BLUE));
}

#[test]
fn paint_table_get_past_the_end_returns_none() {
    let mut table = PaintTable::new();
    table.push_solid(RED);

    assert_eq!(table.get(PaintIndex(1)), None);
    assert_eq!(table.get(PaintIndex(u32::MAX)), None);
}

#[test]
fn paint_table_resolve_returns_the_entry() {
    let mut table = PaintTable::new();
    let red = table.push_solid(RED);

    assert_eq!(table.fill(table.resolve(red).fill), Fill::Solid(RED));
    assert_eq!(table.resolve(red).stroke, StrokeRange::NONE);
}

#[test]
#[should_panic(expected = "paint index 1 out of range")]
fn paint_table_resolve_panics_on_an_out_of_range_index() {
    let mut table = PaintTable::new();
    table.push_solid(RED);

    table.resolve(PaintIndex(1));
}

#[test]
fn push_solid_is_fill_only() {
    let mut table = PaintTable::new();
    let index = table.push_solid(RED);
    let entry = table.resolve(index);

    assert_eq!(table.fill(entry.fill), Fill::Solid(RED));
    assert_eq!(entry.stroke, StrokeRange::NONE);
    assert_eq!(entry.corners, CornerRadii::default());
    assert_eq!(entry.shadows, ShadowRange::NONE);
    assert_eq!(entry.blurs, BlurRange::NONE);
}

#[test]
fn a_paint_less_entry_pushes_and_resolves() {
    let mut table = PaintTable::new();
    let index = table.push(PaintEntry::default());

    // The paint-less entry: a tag rather than an absent value, so reading it
    // resolves like any other fill (story #578).
    assert_eq!(table.resolve(index).fill, PaintKind::NONE);
    assert_eq!(table.fill(table.resolve(index).fill), Fill::None);
}

#[test]
fn a_full_entry_round_trips_through_the_table() {
    let gradient = Gradient {
        kind: GradientKind::Radial,
        handle_origin: Vec2 { x: 0.5, y: 0.5 },
        handle_primary: Vec2 { x: 1.0, y: 0.5 },
        handle_secondary: Vec2 { x: 0.5, y: 1.0 },
        stops: StopRange::NONE,
    };
    let stops = vec![
        GradientStop {
            offset: 0.0,
            color: RED,
        },
        GradientStop {
            offset: 1.0,
            color: HALF_BLUE,
        },
    ];
    let mut table = PaintTable::new();
    let fill = table.intern_fill(&FillSpec::Gradient {
        gradient,
        stops: stops.clone(),
    });
    let stroke = Stroke {
        width: 2.0,
        align: StrokeAlign::Inside,
        color: RED,
    };
    let entry = PaintEntry {
        fill,
        corners: CornerRadii {
            top_left: 1.0,
            top_right: 2.0,
            bottom_right: 3.0,
            bottom_left: 4.0,
        },
        ..PaintEntry::default()
    };
    let shadow = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 4.0 },
        blur: 8.0,
        spread: 1.0,
        color: HALF_BLUE,
    };
    let index = table.push_with(
        entry,
        EntryParts {
            stroke: Some(stroke),
            shadows: &[shadow],
            ..EntryParts::default()
        },
    );

    // The entry round-trips with the range the table assigned, and the
    // shadow round-trips through the flat array that range names.
    let stored = *table.resolve(index);
    assert_eq!(stored.fill, entry.fill);
    assert_eq!(table.stroke(&stored), Some(&stroke));
    assert_eq!(stored.corners, entry.corners);
    assert_eq!(
        stored.shadows,
        ShadowRange {
            offset: 0,
            count: 1
        }
    );
    assert_eq!(table.shadows(&stored), &[shadow]);
    // The fill round-trips the same way: through the table that interned
    // it, with the stops resolved from the range it was given.
    match table.fill(stored.fill) {
        Fill::Gradient(view) => {
            assert_eq!(view.gradient.kind, GradientKind::Radial);
            assert_eq!(view.gradient.handle_secondary, Vec2 { x: 0.5, y: 1.0 });
            assert_eq!(view.stops, stops.as_slice());
        }
        other => panic!("expected a gradient fill, got {other:?}"),
    }
}

#[test]
fn an_image_fill_round_trips_through_the_table() {
    let image = ImageFill {
        image: 7,
        scale_mode: ScaleMode::Crop,
        // What the pre-#578 `transform: None` meant.
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    };
    let mut table = PaintTable::new();
    let fill = table.intern_fill(&FillSpec::Image(image));
    let index = table.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });

    assert_eq!(table.fill(fill), Fill::Image(&image));
    assert_eq!(table.resolve(index).fill, fill);
}

#[test]
fn image_table_pushes_and_resolves_assets() {
    let mut images = ImageTable::new();
    assert!(images.is_empty());

    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    };
    let index = images.push(asset.clone());

    assert_eq!(index, 0);
    assert_eq!(images.len(), 1);
    // Equal *including* the extent, which the table derived and the owned asset
    // reads back the same way — the two routes to an `ImageRef` agreeing is the
    // reason `as_ref` exists.
    assert_eq!(images.resolve(index), asset.as_ref());
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

/// A box distinguishable from [`SHARP`] in every field.
///
/// Needed because the flat-array tests below index into one shared box
/// array, and a fixture of identical boxes cannot tell a correct range from
/// one that reads the right *number* of boxes at the wrong offset — the two
/// slices compare equal. Mutating `ClipTable::view` to ignore `offset`
/// leaves an all-`SHARP` suite green.
const OTHER: ClipBox = ClipBox {
    x: 1.0,
    y: 2.0,
    w: 3.0,
    h: 4.0,
    corners: CornerRadii {
        top_left: 5.0,
        top_right: 6.0,
        bottom_right: 7.0,
        bottom_left: 8.0,
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

    let one = clips.push(&[SHARP]);
    let two = clips.push(&[SHARP, SHARP]);

    assert_eq!(one, ClipIndex(1));
    assert_eq!(two, ClipIndex(2));
    assert_eq!(clips.len(), 3);
    assert_eq!(clips.resolve(one).boxes(), &[SHARP]);
    assert_eq!(
        clips.get(two).map(|view| view.boxes()),
        Some(&[SHARP, SHARP][..])
    );
    assert_eq!(clips.get(ClipIndex(3)), None);
}

#[test]
fn a_region_with_boxes_is_not_unclipped() {
    assert!(ClipRegion::unclipped().is_unclipped());

    let mut clips = ClipTable::new();
    let one = clips.push(&[SHARP]);
    assert!(!clips.resolve(one).is_unclipped());
}

/// A region is a range into the table's one flat box array, and the ranges
/// of two regions pushed in order are adjacent and non-overlapping (story
/// #578). The upload path this shape exists for reads `all_boxes` directly,
/// so the ranges have to mean what they say against it.
#[test]
fn regions_are_adjacent_ranges_into_one_flat_box_array() {
    let mut clips = ClipTable::new();
    let one = clips.push(&[SHARP]);
    let two = clips.push(&[OTHER, SHARP]);

    assert_eq!(
        clips.region(ClipIndex::UNCLIPPED),
        ClipRegion {
            offset: 0,
            count: 0
        },
        "the reserved unclipped region names no boxes"
    );
    assert_eq!(
        clips.region(one),
        ClipRegion {
            offset: 0,
            count: 1
        }
    );
    assert_eq!(
        clips.region(two),
        ClipRegion {
            offset: 1,
            count: 2
        },
        "the second region starts where the first ended"
    );
    assert_eq!(clips.all_boxes(), &[SHARP, OTHER, SHARP]);

    // The boxes a view hands back are the slice its range names, read out of
    // the flat array rather than out of a Vec the region owns. Asserted
    // against the literal boxes rather than against a slice recomputed from
    // the same range this is checking, which would agree with an offset bug
    // by construction.
    assert_eq!(clips.resolve(one).boxes(), &[SHARP]);
    assert_eq!(
        clips.resolve(two).boxes(),
        &[OTHER, SHARP],
        "the second region starts at its own offset, not at the array's start"
    );
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
    /// The boxes each rect resolved to, in paint order. Stores the boxes
    /// rather than the `ClipRegion` range, because a range is meaningless
    /// away from the table it indexes and the painter is being asked what
    /// it was told to clip against (story #578).
    clipped: Vec<Vec<ClipBox>>,
    groups: Vec<GroupComposite>,
}

impl Painter for RecordingPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        _images: &ImageTable,
        clips: &ClipTable,
        groups: &[GroupComposite],
        _glyphs: &GlyphRunTable,
        _dirty: Option<&[u32]>,
    ) {
        for rect in rects {
            match Some(paints.fill(paints.resolve(rect.paint).fill)) {
                Some(Fill::Solid(color)) => self.painted.push((*rect, color)),
                other => panic!("fixture only paints solids, got {other:?}"),
            }
            self.clipped.push(clips.resolve(rect.clip).boxes().to_vec());
        }
        self.groups.extend_from_slice(groups);
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable, ClipTable) {
    let mut paints = PaintTable::new();
    let red = paints.push_solid(RED);
    let blue = paints.push_solid(HALF_BLUE);
    let mut clips = ClipTable::new();
    // The second rect sits inside the first, which clips it.
    let inside_first = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 50.0,
        corners: CornerRadii::default(),
    }]);
    let rects = vec![
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
        RectEntry {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            paint: blue,
            clip: inside_first,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
    ];
    (rects, paints, clips)
}

#[test]
fn painter_receives_rects_in_slice_order_with_resolved_colors() {
    let (rects, paints, clips) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}

#[test]
fn painter_resolves_each_rects_clip_region() {
    let (rects, paints, clips) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );

    assert!(
        painter.clipped[0].is_empty(),
        "the first rect resolves to no boxes"
    );
    assert_eq!(
        painter.clipped[1],
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
    dyn_painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );

    assert_eq!(
        painter.painted,
        vec![(rects[0], RED), (rects[1], HALF_BLUE)]
    );
}

#[test]
fn paint_index_is_transparent_over_u32() {
    assert_eq!(std::mem::size_of::<PaintIndex>(), 4);
    assert_eq!(std::mem::size_of::<ClipIndex>(), 4);
    // 28 until story #770, which added the rotation angle and its two
    // anchor components — twelve bytes, and the row is still four-byte
    // aligned with no padding. The number is pinned so that widening the
    // row every rect carries is a deliberate act with a reason recorded
    // beside it, which is what this assertion is for.
    assert_eq!(std::mem::size_of::<RectEntry>(), 40);
    assert_eq!(std::mem::align_of::<RectEntry>(), 4);
    assert_eq!(std::mem::size_of::<Color>(), 16);
    assert_eq!(std::mem::align_of::<Color>(), 4);
    assert_eq!(std::mem::size_of::<ClipBox>(), 32);
    assert_eq!(std::mem::align_of::<ClipBox>(), 4);
    // The flattened fill vocabulary (story #578): a tag and a row index,
    // which is what makes it uploadable and what a C# `[StructLayout]`
    // mirror has to agree with.
    assert_eq!(std::mem::size_of::<PaintKind>(), 8);
    assert_eq!(std::mem::align_of::<PaintKind>(), 4);
    assert_eq!(std::mem::size_of::<StopRange>(), 8);
    assert_eq!(std::mem::align_of::<StopRange>(), 4);
}

/// Two stop lists of *different* lengths, so a gradient's stops are read
/// from its own range rather than from the head of the flat array. A
/// fixture where both gradients carried the same number of stops would
/// pass against an implementation that ignored the offset entirely.
#[test]
fn each_gradient_reads_its_own_stops_from_the_flat_array() {
    let mut table = PaintTable::new();
    let two = vec![
        GradientStop {
            offset: 0.0,
            color: RED,
        },
        GradientStop {
            offset: 1.0,
            color: HALF_BLUE,
        },
    ];
    let three = vec![
        GradientStop {
            offset: 0.0,
            color: HALF_BLUE,
        },
        GradientStop {
            offset: 0.25,
            color: RED,
        },
        GradientStop {
            offset: 1.0,
            color: HALF_BLUE,
        },
    ];

    let first = table.intern_fill(&FillSpec::Gradient {
        gradient: linear_gradient(),
        stops: two.clone(),
    });
    let second = table.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Radial,
            ..linear_gradient()
        },
        stops: three.clone(),
    });

    assert_eq!(table.all_stops().len(), 5);
    match (table.fill(first), table.fill(second)) {
        (Fill::Gradient(a), Fill::Gradient(b)) => {
            assert_eq!(a.stops, two.as_slice());
            assert_eq!(b.stops, three.as_slice());
            assert_eq!(b.gradient.stops.offset, 2);
            assert_eq!(b.gradient.stops.count, 3);
        }
        other => panic!("expected two gradient fills, got {other:?}"),
    }
}

#[test]
fn interning_the_same_fill_twice_reuses_the_row() {
    let mut table = PaintTable::new();

    let first = table.intern_fill(&FillSpec::Solid { color: RED });
    let second = table.intern_fill(&FillSpec::Solid { color: RED });
    let other = table.intern_fill(&FillSpec::Solid { color: HALF_BLUE });

    assert_eq!(first, second);
    assert_ne!(first, other);
    assert_eq!(table.all_solids().len(), 2);
    assert_eq!(first.tag, PaintTag::Solid);
}

/// Dedup compares a gradient's stops, not the range naming them — a
/// candidate has no range yet, so comparing ranges would make every
/// gradient distinct and grow the table on every commit.
#[test]
fn gradient_dedup_compares_the_stops_themselves() {
    let mut table = PaintTable::new();
    let stops = vec![GradientStop {
        offset: 0.0,
        color: RED,
    }];
    let other_stops = vec![GradientStop {
        offset: 0.0,
        color: HALF_BLUE,
    }];

    let first = table.intern_fill(&FillSpec::Gradient {
        gradient: linear_gradient(),
        stops: stops.clone(),
    });
    let same = table.intern_fill(&FillSpec::Gradient {
        gradient: linear_gradient(),
        stops,
    });
    let differing = table.intern_fill(&FillSpec::Gradient {
        gradient: linear_gradient(),
        stops: other_stops,
    });

    assert_eq!(first, same);
    assert_ne!(first, differing);
    assert_eq!(table.all_gradients().len(), 2);
}

#[test]
#[should_panic(expected = "must arrive with StopRange::NONE")]
fn interning_a_gradient_that_already_names_a_range_is_refused() {
    let mut table = PaintTable::new();

    table.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            stops: StopRange {
                offset: 3,
                count: 2,
            },
            ..linear_gradient()
        },
        stops: vec![GradientStop {
            offset: 0.0,
            color: RED,
        }],
    });
}

/// A `PaintKind` names a row in the table that interned it. Pushing an
/// entry carrying one into a *different* table is the failure this refuses
/// by name — silently accepting it would paint whatever row happened to
/// sit at that index.
#[test]
#[should_panic(expected = "a fill index belongs to the table that interned it")]
fn an_entry_naming_a_fill_from_another_table_is_refused() {
    let mut interned_in = PaintTable::new();
    interned_in.push_solid(RED);
    interned_in.push_solid(HALF_BLUE);
    let stray = interned_in.resolve(PaintIndex(1)).fill;

    let mut other = PaintTable::new();
    other.push_solid(RED);
    other.push(PaintEntry {
        fill: stray,
        ..PaintEntry::default()
    });
}

/// What table compaction runs: a fill read out of one table and interned
/// into another has to come back as the same fill.
#[test]
fn a_fill_view_round_trips_through_its_spec() {
    let mut table = PaintTable::new();
    let stops = vec![
        GradientStop {
            offset: 0.0,
            color: RED,
        },
        GradientStop {
            offset: 1.0,
            color: HALF_BLUE,
        },
    ];
    let kind = table.intern_fill(&FillSpec::Gradient {
        gradient: linear_gradient(),
        stops: stops.clone(),
    });

    let mut rehomed = PaintTable::new();
    // A row ahead of it, so a re-homed fill that ignored the spec and kept
    // its old index would land on the wrong row.
    rehomed.push_solid(RED);
    let moved = rehomed.intern_fill(&table.fill(kind).to_spec().expect("a gradient fill"));

    assert_eq!(rehomed.fill(moved), table.fill(kind));
    match rehomed.fill(moved) {
        Fill::Gradient(view) => assert_eq!(view.stops, stops.as_slice()),
        other => panic!("expected a gradient fill, got {other:?}"),
    }
}

/// Two entries with *different* part counts, so each reads its own range
/// rather than the head of the array. A fixture where both carried the same
/// counts would pass against an implementation that ignored every offset.
#[test]
fn each_entry_reads_its_own_parts_from_the_flat_arrays() {
    let mut table = PaintTable::new();
    let red = table.intern_fill(&FillSpec::Solid { color: RED });
    let blue = table.intern_fill(&FillSpec::Solid { color: HALF_BLUE });
    let stroke = Stroke {
        width: 3.0,
        align: StrokeAlign::Outside,
        color: RED,
    };
    // A second, *different* stroke, so the second entry's stroke sits at a
    // non-zero offset. With only one stroke in the array an implementation
    // that ignored the offset entirely would still read the right one —
    // measured: that mutation survived the first version of this test.
    let other_stroke = Stroke {
        width: 1.0,
        align: StrokeAlign::Inside,
        color: HALF_BLUE,
    };
    // Two distinct coverage masks, for the same reason.
    let field = VectorField {
        image: 1,
        atlas_rect: [0, 0, 8, 8],
        plane_bounds: [0.0, 0.0, 1.0, 1.0],
        distance_range: 2.0,
    };
    let other_field = VectorField {
        image: 2,
        atlas_rect: [8, 0, 16, 8],
        plane_bounds: [0.0, 0.0, 0.5, 0.5],
        distance_range: 4.0,
    };

    let first = table.push_with(
        PaintEntry {
            fill: red,
            ..PaintEntry::default()
        },
        EntryParts {
            extra_fills: &[blue],
            stroke: Some(stroke),
            shape: Some(field),
            ..EntryParts::default()
        },
    );
    let second = table.push_with(
        PaintEntry {
            fill: blue,
            ..PaintEntry::default()
        },
        EntryParts {
            // Two layers where the first entry had one, and its own stroke:
            // every range on this entry starts at a different offset from
            // its neighbour's.
            extra_fills: &[red, blue],
            stroke: Some(other_stroke),
            shape: Some(other_field),
            ..EntryParts::default()
        },
    );

    let first = *table.resolve(first);
    let second = *table.resolve(second);
    assert_eq!(table.extra_fills(&first), &[blue]);
    assert_eq!(table.extra_fills(&second), &[red, blue]);
    assert_eq!(table.stroke(&first), Some(&stroke));
    assert_eq!(table.stroke(&second), Some(&other_stroke));
    assert_eq!(second.extra_fills.offset, 1);
    assert_eq!(second.stroke.offset, 1);
    assert_eq!(table.shape(&first), Some(&field));
    assert_eq!(table.shape(&second), Some(&other_field));
    assert_eq!(second.shape.offset, 1);
    assert_eq!(table.all_extra_fills().len(), 3);
    assert_eq!(table.all_strokes().len(), 2);
    assert_eq!(table.all_shapes().len(), 2);
}

/// The paint-less entry is a tag, not an absent value, so a painter's match
/// over the fill vocabulary stays exhaustive and cannot forget it.
#[test]
fn the_paint_less_entry_resolves_like_any_other_fill() {
    let mut table = PaintTable::new();
    let index = table.push(PaintEntry::default());
    let entry = *table.resolve(index);

    assert_eq!(entry.fill.tag, PaintTag::None);
    assert_eq!(table.fill(entry.fill), Fill::None);
    assert_eq!(table.fill(entry.fill).to_spec(), None);
    // And it names no row, so it cannot be caught by the cross-table check
    // the way a real fill can — there is nothing to be in range of.
    assert_eq!(table.all_solids().len(), 0);
}

#[test]
#[should_panic(expected = "the vocabulary is single-stroke")]
fn an_entry_naming_more_than_one_stroke_is_refused() {
    let mut table = PaintTable::new();
    let index = table.push_with(
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: 1.0,
                align: StrokeAlign::Center,
                color: RED,
            }),
            ..EntryParts::default()
        },
    );
    // Reaching a count above one takes a hand-built entry: `push_with`
    // assigns 0 or 1 from an `Option`. This is the arity the *read* refuses,
    // which is what a hand-built boundary-B input would hit.
    let mut entry = *table.resolve(index);
    entry.stroke.count = 2;
    table.stroke(&entry);
}

#[test]
#[should_panic(expected = "a layer with nothing to paint is a corrupt list")]
fn a_stacked_layer_naming_no_fill_is_refused() {
    let mut table = PaintTable::new();
    table.push_solid(RED);
    // A stacked layer exists to add ink. `PaintKind::NONE` there is a
    // corrupt list, not an empty one — and it is the entry's own `fill`, at
    // position 0, that is allowed to name nothing.
    table.push_with(
        PaintEntry::default(),
        EntryParts {
            extra_fills: &[PaintKind::NONE],
            ..EntryParts::default()
        },
    );
}

#[test]
#[should_panic(expected = "push_with assigns an entry's ranges")]
fn an_entry_arriving_with_a_range_already_assigned_is_refused() {
    let mut table = PaintTable::new();
    table.push_with(
        PaintEntry {
            shape: ShapeRange {
                offset: 7,
                count: 1,
            },
            ..PaintEntry::default()
        },
        EntryParts::default(),
    );
}

/// The handles every gradient fixture here shares; only kind and stops
/// vary, so a test that cares about stops is not also varying geometry.
fn linear_gradient() -> Gradient {
    Gradient {
        kind: GradientKind::Linear,
        handle_origin: Vec2 { x: 0.0, y: 0.0 },
        handle_primary: Vec2 { x: 1.0, y: 0.0 },
        handle_secondary: Vec2 { x: 0.0, y: 1.0 },
        stops: StopRange::NONE,
    }
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
        _groups: &[GroupComposite],
        _glyphs: &GlyphRunTable,
        dirty: Option<&[u32]>,
    ) {
        self.seen_dirty = dirty.map(<[u32]>::to_vec);
        self.seen_rects = rects.len();
    }
}

#[test]
fn the_dirty_set_crosses_boundary_b() {
    let mut paints = PaintTable::new();
    let paint = paints.push_solid(RED);
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];

    let mut painter = DirtyRecordingPainter::default();

    // A caller with a committed scene passes the set it produced.
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        Some(&[0]),
    );
    assert_eq!(painter.seen_dirty.as_deref(), Some(&[0u32][..]));
    assert_eq!(painter.seen_rects, 1);

    // A caller with hand-built tables has no dirty information.
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    assert_eq!(painter.seen_dirty, None);
}

/// The free-path group alpha rides on each rect, and the render-target
/// groups cross boundary B as their own slice — the two halves group
/// opacity resolves into (`docs/decisions/masks-and-group-opacity.md`).
#[test]
fn group_opacity_crosses_as_per_rect_alpha_and_a_group_slice() {
    let mut paints = PaintTable::new();
    let red = paints.push_solid(RED);
    let rects = vec![
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            // A free-path opacity node: its alpha is folded per-rect.
            opacity: 0.5,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
        RectEntry {
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
    ];
    // A render-target group over the first rect only, at alpha 0.25.
    let groups = [GroupComposite {
        start: 0,
        end: 1,
        alpha: 0.25,
    }];

    let mut painter = RecordingPainter::default();
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &groups,
        &GlyphRunTable::new(),
        None,
    );

    assert_eq!(painter.painted[0].0.opacity, 0.5);
    assert_eq!(painter.painted[1].0.opacity, 1.0);
    assert_eq!(painter.groups, groups);
}

/// A one-entry table over the half-blue fill, carrying one blur of `kind`.
///
/// The entry itself no longer says anything about the blur — a
/// `PaintEntry` names a range and the blur lives in the table's flat array
/// (story #578) — so there is nothing to build without a table, and the
/// helper hands back both.
fn blurred_table(kind: BlurKind) -> (PaintTable, PaintIndex) {
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Solid { color: HALF_BLUE });
    let index = paints.push_with(
        PaintEntry {
            fill,
            ..PaintEntry::default()
        },
        EntryParts {
            blurs: &[Blur { kind, radius: 16.0 }],
            ..EntryParts::default()
        },
    );
    (paints, index)
}

/// Whether an entry samples the backdrop is derived from its blurs
/// (`docs/decisions/backdrop-blur-is-core-vocabulary.md`), so an entry
/// that carries none — every entry written before v0.11 — samples
/// nothing, and a layer blur is node-local rather than backdrop-reading.
#[test]
fn only_a_backdrop_blur_makes_an_entry_sample_the_backdrop() {
    // The answer moved onto the table with the blurs it is derived from
    // (story #578), so it is asked of the table an entry came from.
    let mut plain = PaintTable::new();
    let empty = plain.push(PaintEntry::default());
    let solid = plain.push_solid(RED);
    assert!(!plain.samples_backdrop(plain.resolve(empty)));
    assert!(!plain.samples_backdrop(plain.resolve(solid)));

    let (layer, layer_index) = blurred_table(BlurKind::Layer);
    assert!(!layer.samples_backdrop(layer.resolve(layer_index)));

    let (backdrop, backdrop_index) = blurred_table(BlurKind::Backdrop);
    assert!(backdrop.samples_backdrop(backdrop.resolve(backdrop_index)));
}

/// Test double: records the rect indices that are ordering barriers. It
/// reads them from the paint table it is already handed — `Painter::paint`
/// grew no parameter for the backdrop contract, so the declaration
/// reaches every existing painter through the signature it already has.
#[derive(Default)]
struct BarrierRecordingPainter {
    barriers: Vec<usize>,
}

impl Painter for BarrierRecordingPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        _images: &ImageTable,
        _clips: &ClipTable,
        _groups: &[GroupComposite],
        _glyphs: &GlyphRunTable,
        _dirty: Option<&[u32]>,
    ) {
        for (index, rect) in rects.iter().enumerate() {
            if paints.samples_backdrop(paints.resolve(rect.paint)) {
                self.barriers.push(index);
            }
        }
    }
}

#[test]
fn a_backdrop_sampling_rect_crosses_boundary_b_as_an_ordering_barrier() {
    let mut paints = PaintTable::new();
    let plain = paints.push_solid(RED);
    // Through `push_with`: the blur that makes this entry a barrier
    // lives in the table's flat array, not on the entry (story #578).
    let frosted_fill = paints.intern_fill(&FillSpec::Solid { color: HALF_BLUE });
    let frosted = paints.push_with(
        PaintEntry {
            fill: frosted_fill,
            ..PaintEntry::default()
        },
        EntryParts {
            blurs: &[Blur {
                kind: BlurKind::Backdrop,
                radius: 16.0,
            }],
            ..EntryParts::default()
        },
    );
    let rects: Vec<RectEntry> = [plain, frosted, plain]
        .into_iter()
        .map(|paint| RectEntry {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        })
        .collect();

    let mut painter = BarrierRecordingPainter::default();
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );

    // Only the middle rect samples: rect 0 lies beneath it and must be
    // composited first, and rect 2 lies above it and is unconstrained.
    assert_eq!(painter.barriers, vec![1]);
}

// Debt #53: `Color` and `RectEntry` derive `PartialEq` over `f32` fields.
// The two tests below confirm, by running the comparison rather than by
// reasoning about the IEEE 754 standard, what that derive actually does
// with a NaN and with -0.0. Nothing in this crate depends on either
// behavior today; these tests exist so a future change that relies on
// this `PartialEq` (an equality-based dedup or dirty-diff) finds the
// actual semantics recorded and verified, not merely asserted in a doc
// comment.

#[test]
fn derived_partial_eq_is_not_reflexive_over_a_nan_field() {
    let with_nan = Color {
        r: f32::NAN,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    // Not `assert_ne!`, deliberately: the point is that a value does not
    // equal a bit-for-bit copy of itself, which is what `left == right`
    // over identical operands would normally confirm rather than refute.
    assert!(
        with_nan != with_nan,
        "IEEE 754 NaN != NaN, and a derived PartialEq inherits that: a \
         Color carrying a NaN channel is not equal to itself",
    );

    let rect_with_nan = RectEntry {
        x: f32::NAN,
        y: 0.0,
        w: 10.0,
        h: 10.0,
        paint: PaintIndex(0),
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    assert!(
        rect_with_nan != rect_with_nan,
        "the same non-reflexivity applies to RectEntry's f32 fields",
    );
}

#[test]
fn derived_partial_eq_treats_zero_and_negative_zero_as_equal() {
    let zero = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    let negative_zero = Color {
        r: -0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    // The bits genuinely differ...
    assert_ne!(
        zero.r.to_bits(),
        negative_zero.r.to_bits(),
        "0.0 and -0.0 must be different bit patterns, or this test proves nothing",
    );
    // ...but the derived PartialEq still calls them equal.
    assert_eq!(
        zero, negative_zero,
        "IEEE 754 0.0 == -0.0, and a derived PartialEq inherits that: two \
         Colors with differing bit patterns compare equal",
    );

    let rect_zero = RectEntry {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
        paint: PaintIndex(0),
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    let rect_negative_zero = RectEntry {
        x: -0.0,
        ..rect_zero
    };
    assert_ne!(rect_zero.x.to_bits(), rect_negative_zero.x.to_bits());
    assert_eq!(
        rect_zero, rect_negative_zero,
        "the same 0.0/-0.0 equality applies to RectEntry's f32 fields",
    );
}

/// A run naming no quads, for the flat-array tests below.
fn bare_run(rect: u32) -> GlyphRun {
    GlyphRun {
        rect,
        atlas: AtlasIndex(0),
        size: 16.0,
        color: Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        glyphs: GlyphRange::UNASSIGNED,
        opacity: 1.0,
    }
}

/// A quad distinguishable by its glyph id alone.
///
/// Takes the `u16` a font produces and widens it the way every producer does,
/// so both conversions stay ones the compiler checks as exact.
fn quad(glyph_id: u16) -> GlyphQuad {
    GlyphQuad {
        glyph_id: u32::from(glyph_id),
        x: f32::from(glyph_id),
        y: 0.0,
    }
}

/// Runs name adjacent, non-overlapping ranges into the table's one flat quad
/// array, and `quads` reads the slice a run's own range names (story #578).
///
/// Every quad carries a distinct glyph id, because a fixture of identical
/// quads cannot tell a correct range from one that reads the right *count* at
/// the wrong offset — the two slices compare equal. That is not hypothetical:
/// the same test written with a uniform fixture stayed green against exactly
/// that mutation when `ClipRegion` was flattened.
#[test]
fn runs_are_adjacent_ranges_into_one_flat_quad_array() {
    let mut glyphs = GlyphRunTable::new();
    glyphs.push_run(bare_run(0), &[quad(1)]);
    glyphs.push_run(bare_run(1), &[quad(2), quad(3)]);

    assert_eq!(
        glyphs.runs()[0].glyphs,
        GlyphRange {
            offset: 0,
            count: 1
        }
    );
    assert_eq!(
        glyphs.runs()[1].glyphs,
        GlyphRange {
            offset: 1,
            count: 2
        },
        "the second run starts where the first ended"
    );
    assert_eq!(glyphs.all_quads(), &[quad(1), quad(2), quad(3)]);

    assert_eq!(glyphs.quads(&glyphs.runs()[0]), &[quad(1)]);
    assert_eq!(
        glyphs.quads(&glyphs.runs()[1]),
        &[quad(2), quad(3)],
        "the second run reads from its own offset, not from the array's start"
    );
}

/// `push_run` refuses a run that already carries a range.
///
/// A caller cannot know where its quads will land in a table it has not
/// entered, so a range arriving at `push_run` is one that will be replaced —
/// and silently replacing it is how a producer comes to believe its own
/// offsets were used. The goldens harness re-homes runs between tables and is
/// the real caller that has to clear the range (P4).
#[test]
#[should_panic(expected = "push_run assigns a run's quad range")]
fn push_run_refuses_a_range_it_did_not_assign() {
    let mut glyphs = GlyphRunTable::new();
    glyphs.push_run(bare_run(0), &[quad(1)]);

    let mut smuggled = bare_run(1);
    smuggled.glyphs = GlyphRange {
        offset: 0,
        count: 1,
    };
    glyphs.push_run(smuggled, &[quad(2)]);
}

/// An empty table has an empty flat array, and a text-free scene pays nothing
/// for the flattening.
#[test]
fn an_empty_glyph_run_table_names_no_quads() {
    let glyphs = GlyphRunTable::new();
    assert!(glyphs.all_quads().is_empty());
    assert!(glyphs.runs().is_empty());
}

/// An atlas refuses a glyph id no font can produce, in every build profile
/// (issue #966).
///
/// `GlyphQuad` and `AtlasGlyph` widened their ids from `u16` to `u32` so that
/// neither struct carries padding it does not declare
/// (`docs/decisions/sub-word-members-widen-rather-than-pad.md`). The `u16` made
/// an out-of-domain id unrepresentable, and this replaces that on the
/// `AtlasGlyph` side only: such a row is one no font can name.
///
/// It is deliberately not the silent drop that record describes — a row this
/// constructor accepts is found by `Atlas::glyph` and paints, and what paints
/// nothing is a `GlyphQuad` the atlas has no row for. That side is unchecked
/// (issue #985).
///
/// This asserted a panic until issue #966. A `debug_assert!` compiles out, so
/// the guard held in no release build, and no tier here runs `cargo test
/// --release` — so a `should_panic` test could not tell the assertion from the
/// silent drop. The sibling invariant, sorted-unique ids, needs no such change:
/// `AtlasMetrics::from_bytes` refuses a blob that breaks it in every profile.
#[test]
fn an_atlas_refuses_a_glyph_id_no_font_can_produce() {
    let refused = Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
        64,
        64,
        16,
        2.0,
        vec![AtlasGlyph {
            glyph_id: u32::from(u16::MAX) + 1,
            plane_em: [0.0, 0.0, 1.0, 1.0],
            atlas_px: [0.0, 0.0, 8.0, 8.0],
        }],
    );
    assert_eq!(refused, Err(dashpaint::AtlasBuildError::GlyphIdAboveU16Max));
}

/// The largest glyph id a font can produce is accepted, so the check above
/// refuses an out-of-domain id rather than the top of the domain.
#[test]
fn an_atlas_accepts_the_largest_glyph_id_a_font_can_produce() {
    let atlas = Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
        64,
        64,
        16,
        2.0,
        vec![AtlasGlyph {
            glyph_id: u32::from(u16::MAX),
            plane_em: [0.0, 0.0, 1.0, 1.0],
            atlas_px: [0.0, 0.0, 8.0, 8.0],
        }],
    )
    .expect("u16::MAX is a glyph id OpenType can name");
    assert!(atlas.glyph(u32::from(u16::MAX)).is_some());
}

/// An atlas refuses a zero `px_per_em`, in every build profile (issue #724).
///
/// `Atlas::new`'s own doc carries what the value costs and why the refusal is a
/// `Result` rather than an assertion; this asserts the behaviour it describes.
/// The short version: a returned error runs in every profile, and no tier here
/// runs `cargo test --release`.
#[test]
fn an_atlas_refuses_a_zero_px_per_em() {
    let refused = Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
        64,
        64,
        0,
        2.0,
        Vec::new(),
    );
    assert_eq!(refused, Err(dashpaint::AtlasBuildError::ZeroPxPerEm));
}

/// The smallest legal `px_per_em` is accepted, so the check above refuses zero
/// rather than refusing a small atlas.
#[test]
fn an_atlas_accepts_the_smallest_non_zero_px_per_em() {
    let atlas = Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
        64,
        64,
        1,
        2.0,
        Vec::new(),
    )
    .expect("1 texel per em is a valid atlas scale");
    assert_eq!(atlas.px_per_em(), 1);
}

/// An atlas refuses a zero distance range (issue #964).
///
/// The other operand of the expression `px_per_em` was guarded in, and the one
/// that fails least visibly: `px_range` becomes 0, so `msdf_coverage` returns
/// `clamp(signed_distance * 0 + 0.5, 0, 1)` — exactly 0.5 for every sample — and
/// each glyph paints as a uniform half-alpha box. Text that is still text-shaped
/// and uniformly wrong is the plausible-wrong-picture class P4 forbids, not the
/// nothing-drawn class.
#[test]
fn an_atlas_refuses_a_zero_distance_range() {
    assert_eq!(
        atlas_with_distance_range(0.0),
        Err(dashpaint::AtlasBuildError::DistanceRangeOutOfDomain)
    );
}

/// An atlas refuses a negative distance range, which would invert coverage —
/// glyph interiors transparent and exteriors opaque.
#[test]
fn an_atlas_refuses_a_negative_distance_range() {
    assert_eq!(
        atlas_with_distance_range(-2.0),
        Err(dashpaint::AtlasBuildError::DistanceRangeOutOfDomain)
    );
}

/// An atlas refuses a distance range that is not finite.
///
/// Both non-finite values reach a wrong picture the same way the zero
/// `px_per_em` of issue #724 did, through the other operand: an infinity makes
/// `px_range` an infinity, so `clamp(inf + 0.5, 0, 1)` is a hard aliased edge
/// with an implementation-defined WGSL result at the sample whose median is
/// exactly 0.5, and a NaN reaches that same `clamp` directly. This is why the
/// domain is finite-and-positive rather than merely positive: an infinity is
/// positive.
#[test]
fn an_atlas_refuses_a_non_finite_distance_range() {
    for range in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            atlas_with_distance_range(range),
            Err(dashpaint::AtlasBuildError::DistanceRangeOutOfDomain),
            "a distance range of {range} is not finite"
        );
    }
}

/// A narrow distance range is accepted, so the checks above refuse an
/// out-of-domain value rather than a small one.
///
/// `f32::MIN_POSITIVE` is not the smallest value that passes — subnormals below
/// it are finite and greater than zero, and `f32::from_bits(1)` is accepted too.
/// The domain has one bound at each end and neither is a useful-range bound: a
/// range this narrow makes `px_range` small enough that coverage is 0.5
/// everywhere, which is the same picture the zero case is refused for. Choosing
/// the value where a range stops resolving an edge needs a number relating it to
/// the atlas extent, and no measurement in this repository supplies one — the
/// same reason there is no upper bound.
#[test]
fn an_atlas_accepts_a_narrow_distance_range() {
    let atlas = atlas_with_distance_range(f32::MIN_POSITIVE)
        .expect("a narrow distance range is out of no domain this constructor states");
    assert_eq!(atlas.distance_range_px(), f32::MIN_POSITIVE);
}

/// An otherwise-valid atlas carrying `distance_range_px`, so the four tests
/// above vary only the value under test.
fn atlas_with_distance_range(distance_range_px: f32) -> Result<Atlas, dashpaint::AtlasBuildError> {
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: Vec::new(),
        },
        64,
        64,
        16,
        distance_range_px,
        Vec::new(),
    )
}

// ---------------------------------------------------------------------------
// Baked texel payloads and the flattened image table (story #640)
// ---------------------------------------------------------------------------

/// Every format's discriminant survives the round trip through the stored row.
///
/// `ImageEntry::format` is a `u32` so the row is `#[repr(C)]`, which means the
/// enum and the number are two representations of one fact. This is the
/// assertion that they agree — for every variant, not for the one a fixture
/// happened to use.
#[test]
fn every_image_format_round_trips_through_its_discriminant() {
    let every = [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Gif,
        ImageFormat::Astc4x4Srgb,
        ImageFormat::Astc4x4Unorm,
        ImageFormat::Astc5x5Srgb,
        ImageFormat::Astc5x5Unorm,
        ImageFormat::Astc6x6Srgb,
        ImageFormat::Astc6x6Unorm,
        ImageFormat::Astc8x8Srgb,
        ImageFormat::Astc8x8Unorm,
        ImageFormat::Astc10x10Srgb,
        ImageFormat::Astc10x10Unorm,
        ImageFormat::Astc12x12Srgb,
        ImageFormat::Astc12x12Unorm,
        ImageFormat::Rgba8Srgb,
        ImageFormat::Rgba8Unorm,
    ];
    for format in every {
        assert_eq!(
            ImageFormat::from_u32(format.as_u32()),
            format,
            "{format:?} does not survive its own discriminant"
        );
    }
    // And the discriminants are distinct, which the round trip alone would not
    // catch if two variants shared one.
    let mut seen: Vec<u32> = every.iter().map(|f| f.as_u32()).collect();
    seen.sort_unstable();
    let count = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), count, "two image formats share a discriminant");
}

/// The encoded half and the baked half are exactly the two halves.
#[test]
fn only_the_source_encoded_containers_are_encoded() {
    assert!(ImageFormat::Png.is_encoded());
    assert!(ImageFormat::Jpeg.is_encoded());
    assert!(ImageFormat::Gif.is_encoded());
    assert!(!ImageFormat::Astc6x6Srgb.is_encoded());
    // Uncompressed is baked, not encoded: it is a ladder's terminal rung,
    // uploaded as texels rather than decoded.
    assert!(!ImageFormat::Rgba8Unorm.is_encoded());
    assert_eq!(ImageFormat::Rgba8Unorm.block(), None);
    assert_eq!(ImageFormat::Astc6x6Srgb.block(), Some((6, 6)));
    assert_eq!(ImageFormat::Png.block(), None);
}

/// The table stores one pool and hands back the right slice of it.
///
/// Three assets of **different lengths**, and the assertions are about the
/// second and third: a table that ignored the stored offset would return the
/// first asset's bytes for all three and pass any check stated over asset zero
/// alone.
///
/// Baked payloads throughout, so each length is the one its format and extent
/// require and no two are the same. The encoded push has its own test, because
/// it derives the extent rather than being told it (issue #716).
#[test]
fn a_flattened_table_returns_each_assets_own_bytes() {
    let mut images = ImageTable::new();
    // 2x1 RGBA8 is 8 bytes.
    let first = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: vec![1, 2, 3, 4, 5, 6, 7, 8],
        },
        2,
        1,
    );
    // 6x6 ASTC 6x6 is one 16-byte block.
    let second = images.push_baked(
        ImageAsset {
            format: ImageFormat::Astc6x6Srgb,
            bytes: (10..26).collect(),
        },
        6,
        6,
    );
    // 1x1 RGBA8 is 4 bytes.
    let third = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Srgb,
            bytes: vec![30, 31, 32, 33],
        },
        1,
        1,
    );

    assert_eq!(images.resolve(first).bytes, &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        images.resolve(second).bytes,
        (10u8..26).collect::<Vec<_>>().as_slice()
    );
    assert_eq!(images.resolve(third).bytes, &[30, 31, 32, 33]);
    assert_eq!(images.resolve(second).format, ImageFormat::Astc6x6Srgb);
    assert_eq!(images.resolve(third).format, ImageFormat::Rgba8Srgb);

    // The extent travels with the bytes, and differs per asset — a reader that
    // returned the first row's extent for all three would draw two of them at
    // the wrong size with no other symptom.
    assert_eq!(
        (images.resolve(first).width, images.resolve(first).height),
        (2, 1)
    );
    assert_eq!(
        (images.resolve(second).width, images.resolve(second).height),
        (6, 6)
    );
    assert_eq!(
        (images.resolve(third).width, images.resolve(third).height),
        (1, 1)
    );

    // The stored rows are what the FFI gate is stated over: fixed-width, and
    // partitioning the pool.
    let entries = images.all_entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].offset, 0);
    assert_eq!(entries[1].offset, 8);
    assert_eq!(entries[2].offset, 24);
}

/// An empty payload is a value, not a sentinel: it resolves to an empty slice
/// and the asset after it still finds its own bytes.
///
/// Stated over both halves since issue #716. An encoded payload that carries no
/// bytes carries no header either, and is stored at a zero extent rather than
/// refused — `dashscene-validator`'s `image.no-bytes` rule is what names it, and
/// it is handed a table that is already built, so a push that panicked would
/// replace a named diagnostic with a crash.
#[test]
fn a_zero_length_payload_is_an_ordinary_entry() {
    let mut images = ImageTable::new();
    let empty = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: Vec::new(),
    });
    let after = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: vec![9, 9, 9, 9],
        },
        1,
        1,
    );
    assert!(images.resolve(empty).bytes.is_empty());
    assert_eq!(
        (images.resolve(empty).width, images.resolve(empty).height),
        (0, 0),
        "a payload with no header states no extent"
    );
    assert_eq!(images.resolve(after).bytes, &[9, 9, 9, 9]);
    assert_eq!(
        images.all_entries()[1].offset,
        0,
        "an empty row consumes no pool"
    );

    // A baked payload can be zero-length too, and states the extent that makes
    // it so rather than being sized by its bytes.
    let mut baked = ImageTable::new();
    let nothing = baked.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: Vec::new(),
        },
        0,
        0,
    );
    assert!(baked.resolve(nothing).bytes.is_empty());
}

/// The PNG the extent tests are stated over. 7x5, so neither axis is the other
/// and neither is a multiple of any ASTC footprint — a transposed extent and a
/// square one both fail here, where a 1x1 fixture would accept either.
const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");

/// An encoded payload's extent comes from its own header, not from the caller
/// (issue #716).
///
/// The whole reason `ImageAsset` did not have to grow two fields, and the whole
/// reason 47 construction sites did not have to change: the table reads what
/// the bytes already say.
#[test]
fn an_encoded_payload_takes_its_extent_from_its_own_header() {
    let mut images = ImageTable::new();
    let index = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    });
    let asset = images.resolve(index);
    assert_eq!((asset.width, asset.height), (7, 5));
    // The row carries it too, which is what a non-Rust consumer reads.
    assert_eq!(
        (
            images.all_entries()[0].width,
            images.all_entries()[0].height
        ),
        (7, 5)
    );
    // And the same extent reaches a payload held outside a table.
    let owned = ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    };
    assert_eq!((owned.as_ref().width, owned.as_ref().height), (7, 5));
}

/// A baked payload states its extent, because its bytes cannot.
#[test]
fn a_baked_payload_carries_the_extent_it_was_pushed_with() {
    let mut images = ImageTable::new();
    // 7x5 at a 6x6 footprint is 2x1 blocks: the extent rounds up on both axes
    // and neither axis divides. A payload sized by truncating division would be
    // 16 bytes here and refused.
    let index = images.push_baked(
        ImageAsset {
            format: ImageFormat::Astc6x6Unorm,
            bytes: vec![0xAB; 32],
        },
        7,
        5,
    );
    let asset = images.resolve(index);
    assert_eq!((asset.width, asset.height), (7, 5));
    assert_eq!(asset.format, ImageFormat::Astc6x6Unorm);
}

/// The block count rounds up on each axis independently.
///
/// Stated over extents that are not multiples, and asymmetric ones, because a
/// formula that truncated, or that rounded one axis and not the other, agrees
/// with the correct one on every exact multiple.
#[test]
fn a_baked_payload_length_rounds_blocks_up_per_axis() {
    // Exact multiples: 2x2 blocks either way.
    assert_eq!(ImageFormat::Astc6x6Srgb.payload_len(12, 12), Some(4 * 16));
    // One texel over on each axis costs a whole further block on each.
    assert_eq!(ImageFormat::Astc6x6Srgb.payload_len(13, 13), Some(9 * 16));
    // Asymmetric, and neither axis divides: 2 blocks across, 1 down.
    assert_eq!(ImageFormat::Astc6x6Srgb.payload_len(7, 5), Some(2 * 16));
    // The transpose is a different number, so an implementation that swapped
    // the axes fails here.
    assert_eq!(ImageFormat::Astc6x6Srgb.payload_len(5, 7), Some(2 * 16));
    assert_eq!(ImageFormat::Astc12x12Srgb.payload_len(7, 25), Some(3 * 16));
    assert_eq!(ImageFormat::Astc12x12Srgb.payload_len(25, 7), Some(3 * 16));
    // The blockless baked half is four bytes a texel.
    assert_eq!(ImageFormat::Rgba8Srgb.payload_len(7, 5), Some(7 * 5 * 4));
    assert_eq!(ImageFormat::Rgba8Unorm.payload_len(0, 0), Some(0));
    // An encoded container's length is a property of its compression.
    assert_eq!(ImageFormat::Png.payload_len(7, 5), None);
    assert_eq!(ImageFormat::Jpeg.payload_len(7, 5), None);
    assert_eq!(ImageFormat::Gif.payload_len(7, 5), None);
}

/// A baked binding whose bytes are not the length its extent requires is
/// refused by name.
///
/// The baked half's version of what `identify` gives the encoded half for free.
/// Before the extent was carried, a binding could state any format beside any
/// bytes and nothing could tell; that half was closed by story #640. This is
/// the other half of the same disagreement.
#[test]
#[should_panic(expected = "describe different images")]
fn a_baked_payload_whose_length_contradicts_its_extent_is_refused() {
    let mut images = ImageTable::new();
    // 6x6 at a 6x6 footprint is one block, so 32 bytes is two images' worth.
    images.push_baked(
        ImageAsset {
            format: ImageFormat::Astc6x6Srgb,
            bytes: vec![0xAB; 32],
        },
        6,
        6,
    );
}

/// A non-empty encoded payload whose header does not parse panics, where an
/// empty one does not.
///
/// The two cases look alike — neither yields an extent — and they are handled
/// differently on purpose. Nothing owns a diagnostic for bytes that claim to be
/// a PNG and are not, so this is the broken-contract panic; the empty case has
/// a validator rule, so refusing it here would take that rule's job away.
#[test]
#[should_panic(expected = "header parses")]
fn a_corrupt_encoded_payload_is_refused_where_an_empty_one_is_stored() {
    ImageTable::new().push(ImageAsset {
        format: ImageFormat::Png,
        // The PNG signature, then nothing an IHDR can be read from.
        bytes: vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00],
    });
}

/// Each push refuses the other's half by name, so neither can be reached by
/// accident.
#[test]
fn each_push_refuses_the_other_half() {
    let encoded_through_baked = std::panic::catch_unwind(|| {
        ImageTable::new().push_baked(
            ImageAsset {
                format: ImageFormat::Png,
                bytes: SAMPLE_PNG.to_vec(),
            },
            7,
            5,
        )
    });
    assert!(
        encoded_through_baked.is_err(),
        "an encoded payload must not be pushed with a caller-stated extent: its header is the \
         one record of it"
    );

    let baked_through_encoded = std::panic::catch_unwind(|| {
        ImageTable::new().push(ImageAsset {
            format: ImageFormat::Astc6x6Srgb,
            bytes: vec![0xAB; 16],
        })
    });
    assert!(
        baked_through_encoded.is_err(),
        "a baked payload has no header, so a push that derives the extent must refuse it rather \
         than guess"
    );
}

/// A painter that says nothing about formats claims the source-encoded ones,
/// and a painter that overrides the declaration is believed.
///
/// This is the whole of how "can this painter use a baked payload" is answered,
/// and it has to be answered *before* a payload is bound: `Painter::paint`
/// returns nothing, so the question cannot be asked inside a frame.
#[test]
fn a_painter_declares_which_formats_it_can_use() {
    struct Quiet;
    struct Uploads;

    impl Painter for Quiet {
        fn paint(
            &mut self,
            _rects: &[RectEntry],
            _paints: &PaintTable,
            _images: &ImageTable,
            _clips: &ClipTable,
            _groups: &[GroupComposite],
            _glyphs: &GlyphRunTable,
            _dirty: Option<&[u32]>,
        ) {
        }
    }

    impl Painter for Uploads {
        /// A painter with a texture path says so. Story #581 is where the lean
        /// painter's own declaration stops being the default.
        fn samples(&self, format: ImageFormat) -> bool {
            format.is_encoded() || format.block().is_some()
        }

        fn paint(
            &mut self,
            _rects: &[RectEntry],
            _paints: &PaintTable,
            _images: &ImageTable,
            _clips: &ClipTable,
            _groups: &[GroupComposite],
            _glyphs: &GlyphRunTable,
            _dirty: Option<&[u32]>,
        ) {
        }
    }

    // The default is the conservative half, and conservative in the direction
    // that is safe: a painter that could upload a baked payload but did not say
    // so is handed an encoded one and decodes it.
    assert!(Quiet.samples(ImageFormat::Png));
    assert!(!Quiet.samples(ImageFormat::Astc6x6Srgb));
    assert!(!Quiet.samples(ImageFormat::Rgba8Unorm));

    assert!(Uploads.samples(ImageFormat::Png));
    assert!(Uploads.samples(ImageFormat::Astc6x6Srgb));
}
