//! Boundary-B contract tests against hand-built fixtures (issues #3, #13):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{
    AtlasIndex, Blur, BlurKind, ClipBox, ClipIndex, ClipRegion, ClipTable, Color, CornerRadii,
    GlyphQuad, GlyphRange, GlyphRun, GlyphRunTable, Gradient, GradientKind, GradientStop,
    GroupComposite, ImageAsset, ImageFormat, ImageTable, PaintEntry, PaintIndex, PaintKind,
    PaintTable, Painter, RectEntry, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2,
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
        shadows: vec![Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 0.0, y: 4.0 },
            blur: 8.0,
            spread: 1.0,
            color: HALF_BLUE,
        }],
        shape: None,
        extra_fills: Vec::new(),
        blurs: Vec::new(),
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
            match &paints.resolve(rect.paint).fill {
                Some(PaintKind::Solid { color }) => self.painted.push((*rect, *color)),
                other => panic!("fixture only paints solids, got {other:?}"),
            }
            self.clipped.push(clips.resolve(rect.clip).boxes().to_vec());
        }
        self.groups.extend_from_slice(groups);
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable, ClipTable) {
    let mut paints = PaintTable::new();
    let red = paints.push(PaintEntry::solid(RED));
    let blue = paints.push(PaintEntry::solid(HALF_BLUE));
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
        },
        RectEntry {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            paint: blue,
            clip: inside_first,
            opacity: 1.0,
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
    assert_eq!(std::mem::size_of::<RectEntry>(), 28);
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
    let paint = paints.push(PaintEntry::solid(RED));
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
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
    let red = paints.push(PaintEntry::solid(RED));
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
        },
        RectEntry {
            x: 20.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            paint: red,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
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

/// A blurred entry over the half-blue fill, for the backdrop cases below.
fn blurred(kind: BlurKind) -> PaintEntry {
    PaintEntry {
        blurs: vec![Blur { kind, radius: 16.0 }],
        ..PaintEntry::solid(HALF_BLUE)
    }
}

/// Whether an entry samples the backdrop is derived from its blurs
/// (`docs/decisions/backdrop-blur-is-core-vocabulary.md`), so an entry
/// that carries none — every entry written before v0.11 — samples
/// nothing, and a layer blur is node-local rather than backdrop-reading.
#[test]
fn only_a_backdrop_blur_makes_an_entry_sample_the_backdrop() {
    assert!(!PaintEntry::default().samples_backdrop());
    assert!(!PaintEntry::solid(RED).samples_backdrop());
    assert!(!blurred(BlurKind::Layer).samples_backdrop());
    assert!(blurred(BlurKind::Backdrop).samples_backdrop());
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
            if paints.resolve(rect.paint).samples_backdrop() {
                self.barriers.push(index);
            }
        }
    }
}

#[test]
fn a_backdrop_sampling_rect_crosses_boundary_b_as_an_ordering_barrier() {
    let mut paints = PaintTable::new();
    let plain = paints.push(PaintEntry::solid(RED));
    let frosted = paints.push(blurred(BlurKind::Backdrop));
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
fn quad(glyph_id: u16) -> GlyphQuad {
    GlyphQuad {
        glyph_id,
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
