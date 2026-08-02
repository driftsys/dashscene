//! Layer 1 of epic #569's verification net: the packed instance buffer,
//! pinned bit-exactly, with no GPU.
//!
//! `docs/decisions/instance-buffer-contract.md` states why this is the widest
//! part of that net. The short form: R-T4 makes the instance buffer the whole
//! of the painter's frame, so the largest class of painter defect — a dropped
//! clip, a wrong paint row, a wrong draw order, a group applied to the wrong
//! set — is a data defect, and every runner can catch it.
//!
//! # The fixtures are hand-built boundary-B tables, not documents
//!
//! What is under test is the translation *from* boundary B, so boundary B is
//! the input. Driving these from a committed `.dsb` would put the compiler,
//! the solver and the typesetter upstream of the assertion, and a golden that
//! moves would no longer say which of them moved it.
//!
//! # Every field of every fixture is distinguishable
//!
//! Deliberately, and repeatedly checked below. The same defect — a range-offset
//! test passing against an implementation that ignored the offset, because the
//! only element sat at offset 0 — is recorded on issues #650, #651, #561, #688
//! and #699.
//!
//! It takes two separate properties to close, and the first version of this
//! fixture had only one of them. The shadow list of the node that has several
//! is ordered `Drop, Inner, Drop`, so a row taken from a *filtered* position
//! rather than from the entry's own list lands on the wrong shadow — that is
//! one. The other is that the entry's own range must not start at 0: two
//! entries here carry shadows and two carry strokes, so the ones the packer is
//! read against sit at a non-zero offset, and dropping `entry.shadows.offset`
//! or `entry.stroke.offset` moves a golden. Beyond that, no two rects share
//! geometry and no two clip regions share a range.

use std::path::{Path, PathBuf};

use dashpaint::{
    Blur, BlurKind, ClipBox, ClipIndex, ClipTable, Color, CornerRadii, EntryParts, FillSpec,
    GlyphRunTable, Gradient, GradientKind, GradientStop, GroupComposite, ImageAsset, ImageFill,
    ImageFormat, ImageTable, Mat23, PaintEntry, PaintIndex, PaintTable, Painter, RectEntry,
    ScaleMode, Shadow, ShadowKind, StopRange, Stroke, StrokeAlign, Vec2, VectorField,
};
use dashscene_gpu::{GpuPainter, InstanceKind};

/// Boundary B's tables for one frame, as a fixture hands them over.
struct Scene {
    rects: Vec<RectEntry>,
    paints: PaintTable,
    images: ImageTable,
    clips: ClipTable,
    groups: Vec<GroupComposite>,
}

impl Scene {
    /// Packs this scene through the painter's own `Painter::paint`, rather
    /// than by calling the packer directly: the trait is the seam the frame
    /// actually arrives on, and a packer reachable only by a test would not be
    /// the one a host drives.
    fn pack(&self) -> GpuPainter {
        let mut painter = GpuPainter::new();
        painter.paint(
            &self.rects,
            &self.paints,
            &self.images,
            &self.clips,
            &self.groups,
            &GlyphRunTable::new(),
            None,
        );
        painter
    }
}

fn color(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

fn corners(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> CornerRadii {
    CornerRadii {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

fn rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    paint: PaintIndex,
    clip: ClipIndex,
    opacity: f32,
) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip,
        opacity,
    }
}

fn shadow(kind: ShadowKind, blur: f32) -> Shadow {
    Shadow {
        kind,
        offset: Vec2 { x: blur, y: -blur },
        blur,
        spread: blur / 2.0,
        color: color(0.0, 0.0, blur / 100.0, 0.5),
    }
}

/// A baked-vector coverage mask, distinguishable by `seed` in every field.
fn field(seed: f32) -> VectorField {
    VectorField {
        image: 0,
        atlas_rect: [seed as u32, seed as u32 + 1, 16, 32],
        plane_bounds: [-seed, -seed - 1.0, seed, seed + 1.0],
        distance_range: seed,
    }
}

/// The scene that walks the paint vocabulary: one rect per construct, no two
/// alike.
fn vocabulary() -> Scene {
    let mut paints = PaintTable::new();
    let mut clips = ClipTable::new();
    let mut images = ImageTable::new();

    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: vec![1, 2, 3],
    });

    // Three clip regions, none of them the reserved unclipped one, and the
    // ranges deliberately not in rect order: the two-box region starts at box
    // offset 0 and the one-box region at offset 2, so a packer that read
    // `count` and assumed `offset` still produces a wrong range.
    let outer = ClipBox {
        x: 4.0,
        y: 8.0,
        w: 400.0,
        h: 300.0,
        corners: corners(1.0, 2.0, 3.0, 4.0),
    };
    let inner = ClipBox {
        x: 16.0,
        y: 24.0,
        w: 200.0,
        h: 150.0,
        corners: corners(5.0, 6.0, 7.0, 8.0),
    };
    let nested = clips.push(&[outer, inner]);
    let single = clips.push(&[inner]);

    // Fills, interned in an order that is not the order the rects use them in.
    let teal = paints.intern_fill(&FillSpec::Solid {
        color: color(0.0, 0.5, 0.5, 1.0),
    });
    let amber = paints.intern_fill(&FillSpec::Solid {
        color: color(0.9, 0.6, 0.1, 1.0),
    });
    let violet = paints.intern_fill(&FillSpec::Solid {
        color: color(0.4, 0.1, 0.8, 0.75),
    });
    let sweep = paints.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Angular,
            handle_origin: Vec2 { x: 0.1, y: 0.2 },
            handle_primary: Vec2 { x: 0.9, y: 0.3 },
            handle_secondary: Vec2 { x: 0.2, y: 0.8 },
            stops: StopRange::NONE,
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: color(1.0, 0.0, 0.0, 1.0),
            },
            GradientStop {
                offset: 0.75,
                color: color(0.0, 0.0, 1.0, 1.0),
            },
        ],
    });
    let photo = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: 0,
        scale_mode: ScaleMode::Crop,
        transform: Mat23 {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            tx: 0.25,
            ty: 0.5,
        },
        tile_scale: 1.5,
    }));

    // 0 — a layout-only container: no fill, no effects, nothing to draw.
    let container = paints.push(PaintEntry::default());
    // 1 — a solid fill with four different radii.
    let solid = paints.push(PaintEntry {
        fill: teal,
        corners: corners(2.0, 4.0, 6.0, 8.0),
        ..PaintEntry::default()
    });
    // 2 — a gradient fill.
    let gradient = paints.push(PaintEntry {
        fill: sweep,
        ..PaintEntry::default()
    });
    // 3 — an image fill.
    let image = paints.push(PaintEntry {
        fill: photo,
        ..PaintEntry::default()
    });
    // 4 — stacked fill layers over a solid base (story C1, debt #146), and one
    // inner shadow. The shadow is here so that the entry at rect 6 does not
    // start at shadow row 0: a fixture whose only shadowed entry sits at offset
    // 0 lets a packer that dropped `entry.shadows.offset` stay green, which is
    // the defect #650, #651 and #699 each hit in turn.
    let stacked = paints.push_with(
        PaintEntry {
            fill: amber,
            ..PaintEntry::default()
        },
        EntryParts {
            extra_fills: &[violet, sweep],
            shadows: &[shadow(ShadowKind::Inner, 2.0)],
            ..EntryParts::default()
        },
    );
    // 5 — a stroked solid that also stacks a layer, so the golden itself pins
    // the stroke as the last of the node's ink. A stroked node with no stacked
    // layer leaves that ordering to the order test alone, and a golden that
    // cannot express a defect is the `t2-check-has-no-teeth` shape v0.13 exists
    // to remove.
    let stroked = paints.push_with(
        PaintEntry {
            fill: violet,
            corners: corners(9.0, 0.0, 0.0, 0.0),
            ..PaintEntry::default()
        },
        EntryParts {
            extra_fills: &[photo],
            stroke: Some(Stroke {
                width: 3.0,
                align: StrokeAlign::Outside,
                color: color(0.1, 0.2, 0.3, 1.0),
            }),
            ..EntryParts::default()
        },
    );
    // 6 — three shadows in the document's own order, drop and inner
    // interleaved, so a row taken from the filtered position is wrong. Also
    // stroked, and stroked Outside: a drop shadow casts from the stroked
    // silhouette, so this is the node whose shadow bounds are grown. Its stroke
    // is the table's second, which is what keeps `entry.stroke.offset` from
    // being 0 everywhere the packer reads it.
    let shadowed = paints.push_with(
        PaintEntry {
            fill: teal,
            corners: corners(7.0, 0.0, 7.0, 0.0),
            ..PaintEntry::default()
        },
        EntryParts {
            stroke: Some(Stroke {
                width: 6.0,
                align: StrokeAlign::Outside,
                color: color(0.3, 0.3, 0.3, 1.0),
            }),
            shadows: &[
                shadow(ShadowKind::Drop, 12.0),
                shadow(ShadowKind::Inner, 5.0),
                shadow(ShadowKind::Drop, 30.0),
            ],
            ..EntryParts::default()
        },
    );
    // 7 — a backdrop blur under a solid fill, with a layer blur beside it that
    // this painter is documented not to draw.
    let frosted = paints.push_with(
        PaintEntry {
            fill: amber,
            corners: corners(20.0, 20.0, 20.0, 20.0),
            ..PaintEntry::default()
        },
        EntryParts {
            blurs: &[
                Blur {
                    kind: BlurKind::Layer,
                    radius: 7.0,
                },
                Blur {
                    kind: BlurKind::Backdrop,
                    radius: 24.0,
                },
            ],
            ..EntryParts::default()
        },
    );
    // 8 — a baked-vector node: the fill is masked by the coverage field, and
    // the stroke and stacked layers it also carries do not apply.
    let masked = paints.push_with(
        PaintEntry {
            fill: sweep,
            corners: corners(11.0, 12.0, 13.0, 14.0),
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(field(3.0)),
            extra_fills: &[teal],
            stroke: Some(Stroke {
                width: 1.0,
                align: StrokeAlign::Inside,
                color: color(1.0, 1.0, 1.0, 1.0),
            }),
            ..EntryParts::default()
        },
    );
    // 9 — the frosted panel of the hero: a masked node whose backdrop blur is
    // confined to the field's coverage rather than to its box.
    let masked_frost = paints.push_with(
        PaintEntry {
            fill: violet,
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(field(9.0)),
            blurs: &[Blur {
                kind: BlurKind::Backdrop,
                radius: 40.0,
            }],
            ..EntryParts::default()
        },
    );
    // 10 — a baked-vector node whose fill is an image. The reference painter
    // draws nothing for one ("an image-filled vector is not in the measured
    // census; it draws nothing rather than an unmasked rectangle"), so neither
    // does the packer.
    let masked_image = paints.push_with(
        PaintEntry {
            fill: photo,
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(field(5.0)),
            ..EntryParts::default()
        },
    );

    let rects = vec![
        rect(
            0.0,
            0.0,
            1024.0,
            768.0,
            container,
            ClipIndex::UNCLIPPED,
            1.0,
        ),
        rect(10.0, 11.0, 100.0, 50.0, solid, ClipIndex::UNCLIPPED, 1.0),
        rect(120.0, 13.0, 90.0, 60.0, gradient, nested, 1.0),
        rect(220.0, 15.0, 80.0, 70.0, image, single, 0.5),
        rect(310.0, 17.0, 70.0, 80.0, stacked, ClipIndex::UNCLIPPED, 1.0),
        rect(390.0, 19.0, 60.0, 90.0, stroked, nested, 0.25),
        rect(
            460.0,
            21.0,
            50.0,
            100.0,
            shadowed,
            ClipIndex::UNCLIPPED,
            1.0,
        ),
        rect(520.0, 23.0, 40.0, 110.0, frosted, single, 0.75),
        rect(570.0, 25.0, 30.0, 120.0, masked, ClipIndex::UNCLIPPED, 1.0),
        rect(610.0, 27.0, 20.0, 130.0, masked_frost, nested, 0.9),
        rect(650.0, 29.0, 15.0, 140.0, masked_image, single, 0.6),
    ];

    Scene {
        rects,
        paints,
        images,
        clips,
        groups: Vec::new(),
    }
}

/// Two nesting render-target groups over a run of rects, so an instance's
/// layer is the innermost group containing its rect and not the outermost, the
/// first, or the last.
fn groups() -> Scene {
    let mut paints = PaintTable::new();
    let mut clips = ClipTable::new();

    let a = paints.push_solid(color(1.0, 0.0, 0.0, 1.0));
    let b = paints.push_solid(color(0.0, 1.0, 0.0, 1.0));
    let c = paints.push_solid(color(0.0, 0.0, 1.0, 1.0));
    let boxed = clips.push(&[ClipBox {
        x: 2.0,
        y: 3.0,
        w: 500.0,
        h: 400.0,
        corners: corners(0.5, 1.5, 2.5, 3.5),
    }]);

    // 0 root, 1 outer group, 2 inside outer, 3 inner group, 4 inside inner,
    // 5 back inside outer only, 6 outside every group.
    let rects = vec![
        rect(0.0, 0.0, 600.0, 500.0, a, ClipIndex::UNCLIPPED, 1.0),
        rect(10.0, 10.0, 200.0, 200.0, b, ClipIndex::UNCLIPPED, 1.0),
        rect(20.0, 20.0, 60.0, 60.0, c, boxed, 1.0),
        rect(30.0, 30.0, 100.0, 100.0, a, ClipIndex::UNCLIPPED, 0.5),
        rect(40.0, 40.0, 50.0, 50.0, b, boxed, 1.0),
        rect(50.0, 50.0, 40.0, 40.0, c, ClipIndex::UNCLIPPED, 1.0),
        rect(300.0, 300.0, 80.0, 80.0, a, ClipIndex::UNCLIPPED, 1.0),
    ];

    Scene {
        rects,
        paints,
        images: ImageTable::new(),
        clips,
        groups: vec![
            GroupComposite {
                start: 1,
                end: 6,
                alpha: 0.8,
            },
            GroupComposite {
                start: 3,
                end: 5,
                alpha: 0.4,
            },
        ],
    }
}

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(format!("{name}.txt"))
}

/// Compares a dumped buffer against its committed golden, writing
/// `{name}.actual.txt` beside it on a mismatch and re-recording it under
/// `UPDATE_GOLDENS=1` — the same ergonomics as the image harness
/// (`goldens/README.md`), and the same rule: a golden is reviewed truth, so
/// read the diff before committing a re-recording.
fn assert_matches_golden(name: &str, dumped: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden has a directory"))
            .expect("the goldens directory is writable");
        std::fs::write(&path, dumped).expect("the golden is writable");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "layer-1 golden {} is missing ({error}); re-record it with UPDATE_GOLDENS=1 and \
             review the result before committing",
            path.display()
        )
    });
    if expected == dumped {
        return;
    }
    let actual = path.with_file_name(format!("{name}.actual.txt"));
    std::fs::write(&actual, dumped).expect("the actual dump is writable");
    let first = expected
        .lines()
        .zip(dumped.lines())
        .position(|(a, b)| a != b)
        .map(|line| {
            format!(
                "first differing line {}:\n  golden: {}\n  actual: {}",
                line + 1,
                expected.lines().nth(line).expect("the line exists"),
                dumped.lines().nth(line).expect("the line exists"),
            )
        })
        .unwrap_or_else(|| {
            format!(
                "the dumps agree line by line but differ in length ({} golden, {} actual)",
                expected.lines().count(),
                dumped.lines().count()
            )
        });
    panic!(
        "layer-1 golden {name} moved.\n{first}\ngolden: {}\nactual: {}",
        path.display(),
        actual.display()
    );
}

#[test]
fn the_paint_vocabulary_packs_to_its_golden() {
    assert_matches_golden("vocabulary", &vocabulary().pack().instances().dump());
}

#[test]
fn nesting_groups_pack_to_their_golden() {
    assert_matches_golden("groups", &groups().pack().instances().dump());
}

/// The spans cover the instances exactly, with no gap and no overlap — the
/// property R-T4's dirty-range upload rests on, and one a golden alone would
/// not state.
#[test]
fn the_spans_partition_the_buffer_in_rect_order() {
    for scene in [vocabulary(), groups()] {
        let painter = scene.pack();
        let buffer = painter.instances();
        assert_eq!(
            buffer.spans().len(),
            scene.rects.len(),
            "one span per rect, index-aligned with the rect table"
        );
        let mut next = 0u32;
        for (index, span) in buffer.spans().iter().enumerate() {
            assert_eq!(
                span.offset,
                next,
                "rect {index}'s span starts where rect {}'s ended",
                index.wrapping_sub(1)
            );
            next += span.count;
        }
        assert_eq!(
            next as usize,
            buffer.instances().len(),
            "the spans end at the end of the buffer"
        );
    }
}

/// A node whose only ink is a fill packs exactly one instance.
///
/// The inverse of the hazard story #578 hit twice: `entry ==
/// PaintEntry::default()` stopped being a "draws nothing" test when the
/// effects and then the fill left the entry, and a fill-only node compared
/// equal to the default would have been dropped with no diagnostic. Nothing
/// downstream of the packer can tell a dropped fill from a fill-less node, so
/// the count is asserted here.
#[test]
fn a_fill_only_node_packs_exactly_one_instance() {
    let mut paints = PaintTable::new();
    let paint = paints.push_solid(color(0.2, 0.4, 0.6, 1.0));
    let scene = Scene {
        rects: vec![rect(1.0, 2.0, 3.0, 4.0, paint, ClipIndex::UNCLIPPED, 1.0)],
        paints,
        images: ImageTable::new(),
        clips: ClipTable::new(),
        groups: Vec::new(),
    };
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(0);
    assert_eq!(instances.len(), 1, "one fill, one instance");
    assert_eq!(instances[0].kind, InstanceKind::Fill.as_u32());
}

/// A layout-only container packs no instance, and still has a span.
#[test]
fn a_fill_less_node_packs_nothing_but_keeps_its_span() {
    let painter = vocabulary().pack();
    assert!(
        painter.instances().rect_instances(0).is_empty(),
        "the root container has no ink"
    );
    assert_eq!(painter.instances().spans()[0].count, 0);
}

/// A shadow's row is its position in the entry's own list, not its position
/// among the shadows of its kind.
///
/// Rect 6 carries `Drop, Inner, Drop`. A packer that counted the filtered
/// positions would give the second drop shadow row `offset + 1`, which is the
/// inner shadow's parameters — the same shape of defect a filtered `enumerate`
/// produces, and one no golden reader would notice without this stated.
#[test]
fn a_shadow_row_is_its_position_in_the_entrys_own_list() {
    let scene = vocabulary();
    let entry = scene.paints.resolve(scene.rects[6].paint);
    let base = entry.shadows.offset;
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(6);

    let shadows: Vec<_> = instances
        .iter()
        .filter(|i| i.kind == InstanceKind::Shadow.as_u32())
        .collect();
    assert_eq!(shadows.len(), 3, "two drop shadows and one inner");
    assert_eq!(
        (shadows[0].row, shadows[1].row, shadows[2].row),
        (base, base + 2, base + 1),
        "the drop shadows are rows 0 and 2 of the entry's list and the inner shadow is row 1"
    );
    assert_eq!(
        scene.paints.all_shadows()[shadows[1].row as usize].blur,
        30.0,
        "the second drop shadow resolves to the 30-unit blur it was authored with"
    );
}

/// Every instance of a rect carries that rect's clip range, and the ranges are
/// distinguishable — the assertion that fails if the packer reads the region
/// at index 0 for every rect, which a scene whose only clip sat at offset 0
/// would not catch.
#[test]
fn each_rect_carries_its_own_clip_range() {
    let scene = vocabulary();
    let painter = scene.pack();
    let buffer = painter.instances();

    for (index, rect) in scene.rects.iter().enumerate() {
        let region = scene.clips.region(rect.clip);
        for instance in buffer.rect_instances(index as u32) {
            assert_eq!(
                (instance.clip_offset, instance.clip_count),
                (region.offset, region.count),
                "rect {index}'s instances carry its own clip region"
            );
        }
    }

    let seen: std::collections::BTreeSet<_> = buffer
        .instances()
        .iter()
        .map(|i| (i.clip_offset, i.clip_count))
        .collect();
    assert!(
        seen.len() >= 3,
        "the fixture must exercise more than one clip range, or an ignored index passes: {seen:?}"
    );
    assert!(
        seen.contains(&(2, 1)),
        "a region that does not start at box 0 must be exercised: {seen:?}"
    );
}

/// An instance's layer is the innermost group containing its rect.
#[test]
fn a_layer_is_the_innermost_enclosing_group() {
    let scene = groups();
    let painter = scene.pack();
    let buffer = painter.instances();
    let layers: Vec<u32> = (0..scene.rects.len())
        .map(|index| {
            let instances = buffer.rect_instances(index as u32);
            assert_eq!(instances.len(), 1, "every rect here has exactly one fill");
            instances[0].layer
        })
        .collect();
    assert_eq!(
        layers,
        vec![0, 1, 1, 2, 2, 1, 0],
        "0 is the canvas, 1 is the outer group and 2 the inner; rect 5 falls back to the outer \
         group and rect 6 to the canvas"
    );
}

/// A baked-vector node draws one masked fill and nothing else: its stacked
/// layers and its stroke do not apply, because the vector carries its outline
/// in the baked geometry.
#[test]
fn a_masked_node_packs_one_fill_and_no_stroke() {
    let scene = vocabulary();
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(8);
    assert_eq!(instances.len(), 1, "one masked fill");
    assert_eq!(instances[0].kind, InstanceKind::Fill.as_u32());
    let entry = scene.paints.resolve(scene.rects[8].paint);
    assert_eq!(
        instances[0].shape,
        entry.shape.offset + 1,
        "the coverage-mask row, biased by one so zero means none"
    );
    assert_eq!(
        scene.paints.all_shapes()[entry.shape.offset as usize].distance_range,
        3.0,
        "the row resolves to the field the node was authored with"
    );
}

/// A masked node's backdrop blur carries the coverage mask too — the frosted
/// panel of the hero, whose blur follows the field rather than the box.
#[test]
fn a_masked_backdrop_carries_the_coverage_mask() {
    let scene = vocabulary();
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(9);
    let backdrop = instances
        .iter()
        .find(|i| i.kind == InstanceKind::Backdrop.as_u32())
        .expect("the node carries a backdrop blur");
    let entry = scene.paints.resolve(scene.rects[9].paint);
    // The row itself, not merely "not none": this is the second field in the
    // table, so an implementation that always reported row 0 would satisfy a
    // non-zero check and mask the wrong shape.
    assert_eq!(backdrop.shape, entry.shape.offset + 1);
    assert_eq!(
        scene.paints.all_shapes()[entry.shape.offset as usize].distance_range,
        9.0,
        "the row resolves to this node's field, not to the other node's"
    );
    let fill = instances
        .iter()
        .find(|i| i.kind == InstanceKind::Fill.as_u32())
        .expect("the node carries a fill");
    assert_eq!(backdrop.shape, fill.shape, "one mask, both instances");
}

/// A `BlurKind::Layer` blur packs nothing, and its presence does not shift the
/// backdrop blur's row: node-local layer blur is budgeted at v1 and the
/// reference painter skips it by the same filter.
#[test]
fn a_layer_blur_packs_nothing_and_shifts_no_row() {
    let scene = vocabulary();
    let entry = scene.paints.resolve(scene.rects[7].paint);
    assert_eq!(
        scene.paints.blurs(entry).len(),
        2,
        "a layer blur and a backdrop blur"
    );
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(7);
    let backdrops: Vec<_> = instances
        .iter()
        .filter(|i| i.kind == InstanceKind::Backdrop.as_u32())
        .collect();
    assert_eq!(backdrops.len(), 1, "only the backdrop blur packs");
    assert_eq!(
        scene.paints.all_blurs()[backdrops[0].row as usize].radius,
        24.0,
        "the row resolves to the backdrop blur, not to the layer blur before it"
    );
}

/// The per-node order is the reference painter's: backdrop, drop shadows,
/// fill, stacked layers, stroke, inner shadows.
///
/// Stated over one node carrying all of them, because the order is what
/// decides how the parts composite and two painters that disagree on it
/// produce different pixels from the same document.
#[test]
fn one_node_packs_its_parts_in_the_reference_painters_order() {
    let mut paints = PaintTable::new();
    let base = paints.intern_fill(&FillSpec::Solid {
        color: color(0.1, 0.1, 0.1, 1.0),
    });
    let over = paints.intern_fill(&FillSpec::Solid {
        color: color(0.9, 0.9, 0.9, 0.5),
    });
    let paint = paints.push_with(
        PaintEntry {
            fill: base,
            corners: corners(1.0, 2.0, 3.0, 4.0),
            ..PaintEntry::default()
        },
        EntryParts {
            extra_fills: &[over],
            stroke: Some(Stroke {
                width: 2.0,
                align: StrokeAlign::Center,
                color: color(0.0, 0.0, 0.0, 1.0),
            }),
            shadows: &[
                shadow(ShadowKind::Inner, 4.0),
                shadow(ShadowKind::Drop, 8.0),
            ],
            blurs: &[Blur {
                kind: BlurKind::Backdrop,
                radius: 6.0,
            }],
            ..EntryParts::default()
        },
    );
    let scene = Scene {
        rects: vec![rect(5.0, 6.0, 7.0, 8.0, paint, ClipIndex::UNCLIPPED, 1.0)],
        paints,
        images: ImageTable::new(),
        clips: ClipTable::new(),
        groups: Vec::new(),
    };
    let painter = scene.pack();
    let order: Vec<u32> = painter
        .instances()
        .rect_instances(0)
        .iter()
        .map(|i| i.kind)
        .collect();
    assert_eq!(
        order,
        vec![
            InstanceKind::Backdrop.as_u32(),
            InstanceKind::Shadow.as_u32(),
            InstanceKind::Fill.as_u32(),
            InstanceKind::Fill.as_u32(),
            InstanceKind::Stroke.as_u32(),
            InstanceKind::Shadow.as_u32(),
        ],
        "backdrop, drop shadow, fill, stacked layer, stroke, inner shadow"
    );
}

/// Repacking is idempotent, and the buffer is reused rather than regrown — the
/// steady-state frame R-T4 bounds.
#[test]
fn repacking_the_same_frame_produces_the_same_buffer() {
    let scene = vocabulary();
    let mut painter = GpuPainter::new();
    let mut dumps = Vec::new();
    for _ in 0..3 {
        painter.paint(
            &scene.rects,
            &scene.paints,
            &scene.images,
            &scene.clips,
            &scene.groups,
            &GlyphRunTable::new(),
            None,
        );
        dumps.push(painter.instances().dump());
    }
    assert_eq!(dumps[0], dumps[1]);
    assert_eq!(dumps[1], dumps[2]);
    assert_eq!(painter.frames_painted(), 3);
}

/// No two rects of the vocabulary fixture share geometry, and no two of its
/// instances are identical.
///
/// The uniform-fixture guard, as a test rather than as a comment: a fixture
/// whose elements compare equal lets an implementation that confused them pass,
/// which is the defect issues #650, #651, #561, #688 and #699 each record.
#[test]
fn the_vocabulary_fixture_is_distinguishable_in_every_instance() {
    let painter = vocabulary().pack();
    let instances = painter.instances().instances();
    for (a, first) in instances.iter().enumerate() {
        for second in &instances[a + 1..] {
            assert_ne!(
                format!("{first:?}"),
                format!("{second:?}"),
                "two instances of the fixture are identical, so a packer that confused them \
                 would still pass"
            );
        }
    }
}

/// The struct is 64 bytes laid out in declaration order with no implicit
/// padding — the shape the story's rules for anything crossing a language seam
/// call for, and the one a consumer declaring its own copy has to match.
///
/// Every member's offset is pinned, not just the total size. Size and alignment
/// alone do not hold `#[repr(C)]`: `repr(Rust)` produces the same 64 bytes at
/// alignment 4 for these members while reordering them, so a build that lost
/// the attribute would pass a size check and hand a shader the wrong bytes.
/// This is the check `dashscene-unity`'s `improper_ctypes_definitions` gate
/// gives the boundary-B types, which `instance-buffer-contract.md` deliberately
/// does not put this type behind.
#[test]
fn the_instance_is_sixty_four_bytes_in_declaration_order() {
    use dashscene_gpu::Instance;
    use std::mem::offset_of;

    assert_eq!(size_of::<Instance>(), 64);
    assert_eq!(align_of::<Instance>(), 4);
    // Eight 4-byte scalars and two four-float vectors: the members account for
    // every byte, so nothing is padding rustc chose.
    assert_eq!(8 * 4 + 2 * 16, size_of::<Instance>());

    let measured = [
        ("kind", offset_of!(Instance, kind), 0),
        ("tag", offset_of!(Instance, tag), 4),
        ("row", offset_of!(Instance, row), 8),
        ("shape", offset_of!(Instance, shape), 12),
        ("clip_offset", offset_of!(Instance, clip_offset), 16),
        ("clip_count", offset_of!(Instance, clip_count), 20),
        ("layer", offset_of!(Instance, layer), 24),
        ("opacity", offset_of!(Instance, opacity), 28),
        ("bounds", offset_of!(Instance, bounds), 32),
        ("corners", offset_of!(Instance, corners), 48),
    ];
    for (name, at, expected) in measured {
        assert_eq!(
            at, expected,
            "{name} moved to offset {at}: a consumer's own declaration of this struct is now wrong"
        );
    }
    // Both four-float vectors sit at a 16-byte offset, which is what lets a
    // consumer bind the row as a storage-buffer element without repacking it.
    assert_eq!(offset_of!(Instance, bounds) % 16, 0);
    assert_eq!(offset_of!(Instance, corners) % 16, 0);

    let zero = Instance::default();
    assert_eq!(zero.layer, Instance::NONE);
    assert_eq!(zero.shape, Instance::NONE);
    assert_eq!(zero.opacity, 0.0, "what makes a zeroed instance inert");
}

/// A drop shadow casts from the node's *stroked* silhouette, not from its fill
/// box — an Outside stroke grows it by the full width, a Center stroke by half,
/// an Inside stroke not at all.
///
/// The one term of a drop shadow's geometry that no row this instance names
/// carries, so the packer resolves it into the instance's own bounds. Without
/// it the buffer cannot reproduce `dashscene-skia`, whose
/// `a_drop_shadow_casts_from_the_outside_stroke_silhouette` pins the same
/// geometry from the pixel side.
#[test]
fn a_drop_shadow_casts_from_the_stroked_silhouette() {
    let cases = [
        (StrokeAlign::Inside, 0.0),
        (StrokeAlign::Center, 2.0),
        (StrokeAlign::Outside, 4.0),
    ];
    for (align, outset) in cases {
        let mut paints = PaintTable::new();
        let fill = paints.intern_fill(&FillSpec::Solid {
            color: color(1.0, 0.0, 0.0, 1.0),
        });
        let paint = paints.push_with(
            PaintEntry {
                fill,
                // One sharp corner and one round one: a sharp corner must stay
                // sharp under the growth, which is what the reference painter's
                // `spread_corners` does.
                corners: corners(10.0, 0.0, 3.0, 0.0),
                ..PaintEntry::default()
            },
            EntryParts {
                stroke: Some(Stroke {
                    width: 4.0,
                    align,
                    color: color(0.0, 0.0, 0.0, 1.0),
                }),
                shadows: &[
                    shadow(ShadowKind::Drop, 8.0),
                    shadow(ShadowKind::Inner, 8.0),
                ],
                ..EntryParts::default()
            },
        );
        let scene = Scene {
            rects: vec![rect(
                16.0,
                20.0,
                100.0,
                60.0,
                paint,
                ClipIndex::UNCLIPPED,
                1.0,
            )],
            paints,
            images: ImageTable::new(),
            clips: ClipTable::new(),
            groups: Vec::new(),
        };
        let painter = scene.pack();
        let instances = painter.instances().rect_instances(0);

        let drop = instances[0];
        assert_eq!(drop.kind, InstanceKind::Shadow.as_u32());
        assert_eq!(
            drop.bounds,
            [
                16.0 - outset,
                20.0 - outset,
                100.0 + 2.0 * outset,
                60.0 + 2.0 * outset
            ],
            "a {align:?} stroke grows the drop shadow's silhouette by {outset}"
        );
        assert_eq!(
            drop.corners,
            [10.0 + outset, 0.0, 3.0 + outset, 0.0],
            "the radii follow the growth and a sharp corner stays sharp ({align:?})"
        );

        // The stroke instance and the inner shadow keep the node's own box: the
        // reference painter passes no outset to either.
        let stroke = instances[instances.len() - 2];
        assert_eq!(stroke.kind, InstanceKind::Stroke.as_u32());
        assert_eq!(stroke.bounds, [16.0, 20.0, 100.0, 60.0]);
        let inner = instances[instances.len() - 1];
        assert_eq!(inner.kind, InstanceKind::Shadow.as_u32());
        assert_eq!(
            inner.bounds,
            [16.0, 20.0, 100.0, 60.0],
            "an inner shadow is clipped to the node's own shape and takes no outset"
        );
        assert_eq!(inner.corners, [10.0, 0.0, 3.0, 0.0]);
    }
}

/// A baked-vector node whose fill is an image packs nothing, matching the
/// reference painter, which draws nothing rather than an unmasked rectangle.
#[test]
fn a_masked_node_with_an_image_fill_packs_nothing() {
    let scene = vocabulary();
    let entry = scene.paints.resolve(scene.rects[10].paint);
    assert!(
        scene.paints.shape(entry).is_some(),
        "the fixture node is masked"
    );
    assert_eq!(entry.fill.tag, dashpaint::PaintTag::Image);
    let painter = scene.pack();
    assert!(painter.instances().rect_instances(10).is_empty());
}

/// The `tag` an instance carries is the boundary-B enum's own value, not a
/// second table of numbers this crate keeps.
///
/// A hand-written copy would survive a variant reorder in `dashpaint` and
/// quietly change what the field means, while every golden stayed green — the
/// "two records of one fact can disagree" hazard the contract record invokes
/// against a depth field.
#[test]
fn a_tag_is_the_boundary_b_enums_own_discriminant() {
    let scene = vocabulary();
    let painter = scene.pack();
    for (index, rect) in scene.rects.iter().enumerate() {
        let entry = scene.paints.resolve(rect.paint);
        for instance in painter.instances().rect_instances(index as u32) {
            match InstanceKind::from_u32(instance.kind) {
                InstanceKind::Shadow => {
                    let shadow = scene.paints.all_shadows()[instance.row as usize];
                    assert_eq!(instance.tag, shadow.kind as u32);
                }
                InstanceKind::Backdrop => {
                    let blur = scene.paints.all_blurs()[instance.row as usize];
                    assert_eq!(instance.tag, blur.kind as u32);
                }
                InstanceKind::Fill => {
                    assert!(
                        matches!(
                            instance.tag,
                            t if t == dashpaint::PaintTag::Solid as u32
                                || t == dashpaint::PaintTag::Gradient as u32
                                || t == dashpaint::PaintTag::Image as u32
                        ),
                        "a fill instance names a fill kind, not {}",
                        instance.tag
                    );
                }
                InstanceKind::Stroke => {
                    assert_eq!(instance.tag, 0);
                    assert!(scene.paints.stroke(entry).is_some());
                }
            }
        }
    }
}
