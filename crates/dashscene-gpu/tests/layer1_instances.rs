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
    Atlas, AtlasGlyph, AtlasIndex, Blur, BlurKind, ClipBox, ClipIndex, ClipTable, Color,
    CornerRadii, EntryParts, FillSpec, GlyphQuad, GlyphRange, GlyphRun, GlyphRunTable, Gradient,
    GradientKind, GradientStop, GroupComposite, ImageAsset, ImageFill, ImageFormat, ImageTable,
    Mat23, PaintEntry, PaintIndex, PaintTable, Painter, RectEntry, ScaleMode, Shadow, ShadowKind,
    StopRange, Stroke, StrokeAlign, Vec2, VectorField,
};
use dashscene_gpu::{GpuPainter, InstanceKind};

/// A 7x5 PNG, the same fixture `dashpaint`'s own tests use. An image asset
/// needs real bytes since issue #716, because the table reads the extent out of
/// the payload's header rather than being told it.
const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");

/// Boundary B's tables for one frame, as a fixture hands them over.
struct Scene {
    rects: Vec<RectEntry>,
    paints: PaintTable,
    images: ImageTable,
    clips: ClipTable,
    groups: Vec<GroupComposite>,
    glyphs: GlyphRunTable,
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
            &self.glyphs,
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
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
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

    // A real payload, because `ImageTable::push` reads the extent out of the
    // header (issue #716). 7x5 — neither square nor a multiple of any block
    // footprint, so a consumer that transposed the extent or rounded it fails
    // rather than agreeing by symmetry.
    images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
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
        glyphs: GlyphRunTable::new(),
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
        glyphs: GlyphRunTable::new(),
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

/// One atlas glyph, asymmetric in all eight numbers.
///
/// `plane_em` is `[left, bottom, right, top]` in ems, y-up from the baseline,
/// and `atlas_px` is `[left, bottom, right, top]` in texels, bottom-left origin.
/// No two of the eight agree, and `bottom` is negative on one glyph, so a
/// packer that swapped a pair, dropped the y flip, or reused one component
/// produces a different number rather than the same one.
fn atlas_glyph(glyph_id: u32, plane_em: [f32; 4], atlas_px: [f32; 4]) -> AtlasGlyph {
    AtlasGlyph {
        glyph_id,
        plane_em,
        atlas_px,
    }
}

/// An atlas `height` texels tall, holding glyphs 11 and 23.
///
/// Two atlases in the text fixture, and **they differ in height** — which is
/// what the `atlas_px` y flip is stated against. An atlas of one height cannot
/// catch a flip taken against a constant, against the other atlas, or against
/// the width.
///
/// Glyph 17 is deliberately absent from both: it is the empty-outline case a
/// space produces, and it must draw nothing rather than an empty quad.
fn text_atlas(width: u32, height: u32, px_per_em: u16, range: f32) -> Atlas {
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: SAMPLE_PNG.to_vec(),
        },
        width,
        height,
        px_per_em,
        range,
        vec![
            atlas_glyph(11, [0.1, -0.2, 0.65, 0.75], [3.0, 5.0, 19.0, 29.0]),
            atlas_glyph(23, [0.05, 0.15, 0.45, 0.5], [21.0, 7.0, 33.0, 27.0]),
        ],
    )
}

fn glyph_quad(glyph_id: u32, x: f32, y: f32) -> GlyphQuad {
    GlyphQuad { glyph_id, x, y }
}

/// The scene `groups()` builds, with four positioned glyph runs over it.
///
/// Anchored so that no axis of the mapping is uniform:
///
/// - **rect 2** is clipped and inside the outer group; **rect 4** is clipped and
///   inside the *inner* group; **rect 6** is unclipped and inside no group. A
///   run that took its clip or its layer from the wrong place lands on a
///   different number in at least one of the three.
/// - **two runs share rect 4**, so a cursor that advanced once per rect drops
///   one.
/// - The two atlases differ in height, in `px_per_em` and in `distance_range_px`;
///   the runs differ in size, colour and opacity.
/// - No run is anchored to rect 0, so an implementation that ignored the anchor
///   and appended every run to the first rect moves the golden.
fn text() -> Scene {
    let mut scene = groups();

    // A second clip region, so that the one the anchored rects carry sits at a
    // **non-zero** box offset.
    //
    // `groups()` pushes one region, which starts at box 0 — and a run that
    // dropped `ClipRegion::offset` entirely would then still produce the right
    // number. That is the range-offset trap issues #650, #651, #561, #688 and
    // #699 record, and mutation testing is what caught this instance of it: the
    // dropped-offset mutation survived a first version of this fixture while
    // every other one died.
    let deep = scene.clips.push(&[
        ClipBox {
            x: 6.0,
            y: 7.0,
            w: 300.0,
            h: 250.0,
            corners: corners(9.0, 10.0, 11.0, 12.0),
        },
        ClipBox {
            x: 12.0,
            y: 14.0,
            w: 120.0,
            h: 90.0,
            corners: corners(13.0, 14.0, 15.0, 16.0),
        },
    ]);
    scene.rects[2].clip = deep;
    scene.rects[4].clip = deep;

    let mut glyphs = GlyphRunTable::new();
    let tall = glyphs.push_atlas(text_atlas(64, 48, 32, 4.0));
    let short = glyphs.push_atlas(text_atlas(40, 25, 20, 3.0));

    let mut run =
        |atlas: AtlasIndex, rect: u32, size: f32, c: Color, opacity: f32, quads: &[GlyphQuad]| {
            glyphs.push_run(
                GlyphRun {
                    rect,
                    atlas,
                    size,
                    color: c,
                    glyphs: GlyphRange::UNASSIGNED,
                    opacity,
                },
                quads,
            );
        };

    // Glyph 17 sits between the two placed glyphs of the first run: it is
    // absent from the atlas, so the run packs two instances and not three, and
    // the second is at the third quad's position.
    run(
        tall,
        2,
        24.0,
        color(0.9, 0.2, 0.3, 1.0),
        0.75,
        &[
            glyph_quad(11, 100.0, 200.0),
            glyph_quad(17, 130.0, 200.0),
            glyph_quad(23, 152.0, 206.0),
        ],
    );
    run(
        short,
        4,
        13.0,
        color(0.1, 0.7, 0.4, 0.5),
        1.0,
        &[glyph_quad(23, 40.0, 61.0)],
    );
    run(
        tall,
        4,
        18.0,
        color(0.2, 0.2, 0.9, 1.0),
        0.25,
        &[glyph_quad(11, 47.0, 72.0)],
    );
    run(
        short,
        6,
        31.0,
        color(1.0, 1.0, 0.0, 0.8),
        1.0,
        &[glyph_quad(23, 310.0, 330.0), glyph_quad(11, 325.0, 330.0)],
    );

    scene.glyphs = glyphs;
    scene
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

#[test]
fn positioned_glyph_runs_pack_to_their_golden() {
    assert_matches_golden("text", &text().pack().instances().dump());
}

/// A glyph's quad is its pen position plus the atlas glyph's `plane_em` scaled
/// by the run's size, with the y-up-to-y-down flip applied.
///
/// Stated over the numbers rather than left to the golden, because a golden
/// says the output did not change and this says the output is right. The two
/// catch different mistakes: a golden re-recorded against a wrong
/// implementation is green forever.
#[test]
fn a_glyph_quad_is_its_pen_position_plus_the_scaled_plane_bounds() {
    let scene = text();
    let painter = scene.pack();
    // Rect 2's first glyph: quad 11 at (100, 200), atlas 0, size 24.
    let glyph = painter.instances().rect_instances(2)[1];
    assert_eq!(glyph.kind, InstanceKind::Text.as_u32());
    // plane_em is [0.1, -0.2, 0.65, 0.75]: left 0.1, bottom -0.2, right 0.65,
    // top 0.75. y-up from the baseline, so the quad's top is `y - top * size`
    // and its height is `(top - bottom) * size`.
    assert_eq!(
        glyph.bounds,
        [
            100.0 + 0.1 * 24.0,
            200.0 - 0.75 * 24.0,
            (0.65 - 0.1) * 24.0,
            (0.75 - -0.2) * 24.0,
        ],
        "the quad is the pen position plus plane_em scaled by the run's size"
    );
    // The height is not the width, and the origin is above the baseline: both
    // are false for a packer that dropped the flip.
    assert!(
        glyph.bounds[1] < 200.0,
        "the glyph's top is above its baseline"
    );
    assert_ne!(glyph.bounds[2], glyph.bounds[3]);
}

/// A glyph's `corners` is its atlas rectangle, flipped from `atlas_px`'s
/// bottom-left origin to a top-left one **against its own atlas's height**.
///
/// The two atlases are 48 and 25 texels tall, so a flip taken against a
/// constant, against the other atlas, or against the width lands on a different
/// row for at least one of them.
#[test]
fn a_glyphs_corners_are_its_atlas_rectangle_flipped_against_its_own_atlas() {
    let scene = text();
    let painter = scene.pack();
    // Rect 2's first glyph is glyph 11 of the 48-tall atlas:
    // atlas_px [3, 5, 19, 29] -> [x 3, y 48 - 29, w 16, h 24].
    assert_eq!(
        painter.instances().rect_instances(2)[1].corners,
        [3.0, 48.0 - 29.0, 16.0, 24.0]
    );
    // Rect 4's first glyph is glyph 23 of the 25-tall atlas:
    // atlas_px [21, 7, 33, 27] -> [x 21, y 25 - 27, w 12, h 20]. The y is
    // negative, which is what an atlas shorter than its own glyph's top edge
    // produces — a fixture value chosen so that a flip against the *other*
    // atlas's height gives a positive number and fails here.
    assert_eq!(
        painter.instances().rect_instances(4)[1].corners,
        [21.0, 25.0 - 27.0, 12.0, 20.0]
    );
}

/// A glyph id the atlas has no quad for produces no instance.
///
/// The empty-outline case — a space — and `dashpaint::Atlas::glyph`'s own
/// contract. The run places three quads and packs two, and the second packed
/// one is the *third* quad, so a packer that skipped the wrong one fails.
#[test]
fn a_glyph_the_atlas_does_not_place_packs_nothing() {
    let scene = text();
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(2);
    assert_eq!(instances.len(), 3, "one solid fill and two of three glyphs");
    assert_eq!(instances[2].bounds[0], 152.0 + 0.05 * 24.0);
}

/// A run's glyphs take the anchor rect's clip region and its group layer.
///
/// Both are derived from the anchor rather than carried on the run
/// (`docs/decisions/glyph-runs-cross-boundary-b.md`), so this is the one place
/// the derivation is checked. Rect 2 is inside the outer group, rect 4 inside
/// the inner one and rect 6 inside neither, and rect 6 is also unclipped — so
/// no two of the three agree on both.
#[test]
fn a_runs_glyphs_take_the_anchor_rects_clip_and_layer() {
    let scene = text();
    let painter = scene.pack();
    for rect in [2u32, 4, 6] {
        let instances = painter.instances().rect_instances(rect);
        let fill = instances[0];
        for glyph in &instances[1..] {
            assert_eq!(glyph.kind, InstanceKind::Text.as_u32());
            assert_eq!(
                (glyph.clip_offset, glyph.clip_count),
                (fill.clip_offset, fill.clip_count),
                "rect {rect}'s glyphs must sit in the same clip region as its own ink"
            );
            assert_eq!(
                glyph.layer, fill.layer,
                "rect {rect}'s glyphs must composite into the same layer as its own ink"
            );
        }
    }
    // And the three anchors genuinely differ, so the assertion above is not
    // comparing a constant against itself.
    let layer = |rect: u32| painter.instances().rect_instances(rect)[0].layer;
    assert_eq!((layer(2), layer(4), layer(6)), (1, 2, 0));
    let clip = |rect: u32| {
        let ink = painter.instances().rect_instances(rect)[0];
        (ink.clip_offset, ink.clip_count)
    };
    assert_eq!((clip(2), clip(6)), ((1, 2), (0, 0)));
    // And the offset the anchored rects carry is not zero, which is what makes
    // the assertion above able to fail for an implementation that dropped it.
    assert_ne!(
        clip(2).0,
        0,
        "the fixture's clip region starts past box zero"
    );
}

/// A rect with two runs draws both, in table order, after its own ink.
#[test]
fn two_runs_on_one_rect_both_pack_after_the_rects_ink() {
    let scene = text();
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(4);
    assert_eq!(
        instances.len(),
        3,
        "one fill and one glyph from each of two runs"
    );
    assert_eq!(instances[0].kind, InstanceKind::FillSolid.as_u32());
    // Run 1 then run 2: the rows are the runs' own table indices, in order.
    assert_eq!(instances[1].row, 1);
    assert_eq!(instances[2].row, 2);
    // And they are different runs, not one run twice.
    assert_ne!(instances[1].bounds, instances[2].bounds);
}

/// A run's `opacity` reaches its glyphs, and it is the run's own rather than
/// the anchor rect's.
///
/// The two are equal in every scene commit builds — the field is derivable and
/// kept only until that fold-in lands — so the fixture makes them differ, which
/// is the only way to say which one the packer read.
#[test]
fn a_glyph_carries_its_runs_own_opacity() {
    let scene = text();
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(4);
    assert_eq!(instances[0].opacity, 1.0, "rect 4's own opacity");
    assert_eq!(instances[1].opacity, 1.0, "run 1's opacity");
    assert_eq!(
        instances[2].opacity, 0.25,
        "run 2's opacity, not the rect's"
    );
}

/// A run anchored past the rect table is named rather than silently dropped.
///
/// The cursor cannot report it from inside the walk — it simply never matches —
/// so the check is at the end, and this is what says the check is there. The
/// reference painter asserts the same thing (P4).
#[test]
#[should_panic(expected = "is anchored to rect")]
fn a_run_anchored_past_the_rect_table_is_named() {
    let mut scene = groups();
    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(text_atlas(64, 48, 32, 4.0));
    glyphs.push_run(
        GlyphRun {
            rect: 99,
            atlas,
            size: 12.0,
            color: color(1.0, 1.0, 1.0, 1.0),
            glyphs: GlyphRange::UNASSIGNED,
            opacity: 1.0,
        },
        &[glyph_quad(11, 0.0, 0.0)],
    );
    scene.glyphs = glyphs;
    scene.pack();
}

/// The spans cover the instances exactly, with no gap and no overlap — the
/// property R-T4's dirty-range upload rests on, and one a golden alone would
/// not state.
#[test]
fn the_spans_partition_the_buffer_in_rect_order() {
    for scene in [vocabulary(), groups(), text()] {
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
        glyphs: GlyphRunTable::new(),
    };
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(0);
    assert_eq!(instances.len(), 1, "one fill, one instance");
    assert_eq!(instances[0].kind, InstanceKind::FillSolid.as_u32());
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
        .filter(|i| InstanceKind::from_u32(i.kind).is_shadow())
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
    // A gradient, not a solid: the merged kind carries the fill vocabulary, so
    // this now says which fill rather than only that there is one.
    assert_eq!(instances[0].kind, InstanceKind::FillGradient.as_u32());
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
        .find(|i| i.kind == InstanceKind::FillSolid.as_u32())
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
        glyphs: GlyphRunTable::new(),
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
            InstanceKind::ShadowDrop.as_u32(),
            InstanceKind::FillSolid.as_u32(),
            InstanceKind::FillSolid.as_u32(),
            InstanceKind::Stroke.as_u32(),
            InstanceKind::ShadowInner.as_u32(),
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
    assert_eq!(2 * 16 + 8 * 4, size_of::<Instance>());

    let measured = [
        ("bounds", offset_of!(Instance, bounds), 0),
        ("corners", offset_of!(Instance, corners), 16),
        ("kind", offset_of!(Instance, kind), 32),
        ("row", offset_of!(Instance, row), 36),
        ("shape", offset_of!(Instance, shape), 40),
        ("clip_offset", offset_of!(Instance, clip_offset), 44),
        ("clip_count", offset_of!(Instance, clip_count), 48),
        ("layer", offset_of!(Instance, layer), 52),
        ("opacity", offset_of!(Instance, opacity), 56),
        ("outset", offset_of!(Instance, outset), 60),
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
    // A shader's array stride is the struct's size rounded up to its alignment,
    // and a four-float member makes that 16. Sixty-four exactly, so both sides
    // agree on where element `n` begins — without the declared pad the Rust
    // type would be 60 and every element after the first would be misread.
    assert_eq!(size_of::<Instance>() % 16, 0);

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
            glyphs: GlyphRunTable::new(),
        };
        let painter = scene.pack();
        let instances = painter.instances().rect_instances(0);

        let drop = instances[0];
        assert_eq!(drop.kind, InstanceKind::ShadowDrop.as_u32());
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
        assert_eq!(inner.kind, InstanceKind::ShadowInner.as_u32());
        assert_eq!(
            inner.bounds,
            [16.0, 20.0, 100.0, 60.0],
            "an inner shadow is clipped to the node's own shape and takes no outset"
        );
        assert_eq!(inner.corners, [10.0, 0.0, 3.0, 0.0]);
    }
}

/// One node's shadows and its stroke, so a test can state what the packer
/// wrote for each without rebuilding a scene per claim.
///
/// `shadows` are pushed in the order given, and every value differs from every
/// other so that a claim about one cannot be satisfied by another's number.
fn shadowed_scene(stroke: Option<Stroke>, shadows: &[Shadow], opacity: f32) -> Scene {
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Solid {
        color: color(1.0, 0.0, 0.0, 1.0),
    });
    let paint = paints.push_with(
        PaintEntry {
            fill,
            corners: corners(10.0, 0.0, 3.0, 0.0),
            ..PaintEntry::default()
        },
        EntryParts {
            stroke,
            shadows,
            ..EntryParts::default()
        },
    );
    Scene {
        rects: vec![rect(
            16.0,
            20.0,
            100.0,
            60.0,
            paint,
            ClipIndex::UNCLIPPED,
            opacity,
        )],
        paints,
        images: ImageTable::new(),
        clips: ClipTable::new(),
        groups: Vec::new(),
        glyphs: GlyphRunTable::new(),
    }
}

/// A stroke's quad grows by the outset its alignment gives, and the packer is
/// what says so.
///
/// The vertex stage computed this from the stroke row until story #584 and
/// cannot any more: the paint heap a shadow's parameters live in is bound to the
/// fragment stage alone, so the growth had to become a value both kinds carry.
/// This is the stroke half of that move, and it is stated over the same three
/// alignments `dashscene-skia`'s `stroke_outset` is.
#[test]
fn a_strokes_outset_is_the_reach_its_alignment_gives() {
    let cases = [
        (StrokeAlign::Inside, 0.0),
        (StrokeAlign::Center, 2.0),
        (StrokeAlign::Outside, 4.0),
    ];
    for (align, expected) in cases {
        let scene = shadowed_scene(
            Some(Stroke {
                width: 4.0,
                align,
                color: color(0.0, 0.0, 0.0, 1.0),
            }),
            &[],
            1.0,
        );
        let painter = scene.pack();
        let instances = painter.instances().rect_instances(0);
        let stroke = instances
            .iter()
            .find(|i| i.kind == InstanceKind::Stroke.as_u32())
            .expect("the node is stroked");
        assert_eq!(
            stroke.outset, expected,
            "a {align:?} stroke of width 4 reaches {expected} past the fill box"
        );
    }
}

/// A drop shadow's quad grows by its spread, its blur's support and its offset
/// together; an inner shadow's does not grow at all.
///
/// Stated over two drop shadows that agree in nothing, because one cannot
/// falsify a sum: a single fixture whose spread, offset and blur all took the
/// same value would read the same at every weighting of the three.
///
/// The blur term is three sigma, and sigma is the authored radius through
/// `dashpaint::BLUR_SIGMA_PER_RADIUS` — the number `blurred_rounded_box` in
/// `shaders/sdf.wgsl` integrates over, so the quad and the coverage agree about
/// where the shadow ends.
#[test]
fn a_drop_shadows_outset_covers_its_spread_its_blur_and_its_offset() {
    let sigma = dashpaint::BLUR_SIGMA_PER_RADIUS;
    let cases = [
        (
            Shadow {
                kind: ShadowKind::Drop,
                offset: Vec2 { x: 6.0, y: -2.0 },
                blur: 8.0,
                spread: 3.0,
                color: color(0.0, 0.0, 0.0, 0.5),
            },
            3.0 + 3.0 * 8.0 * sigma + 6.0,
        ),
        (
            Shadow {
                kind: ShadowKind::Drop,
                // The larger axis wins, and it is y here: a quad grows
                // symmetrically, so the displacement it has to cover is the
                // furthest of the two.
                offset: Vec2 { x: -1.0, y: 9.0 },
                blur: 2.0,
                spread: 0.5,
                color: color(0.0, 0.0, 0.0, 0.5),
            },
            0.5 + 3.0 * 2.0 * sigma + 9.0,
        ),
    ];
    for (shadow, expected) in cases {
        let scene = shadowed_scene(None, &[shadow], 1.0);
        let painter = scene.pack();
        let drop = painter.instances().rect_instances(0)[0];
        assert_eq!(drop.kind, InstanceKind::ShadowDrop.as_u32());
        assert_eq!(
            drop.outset, expected,
            "spread {} + three sigma of blur {} + offset",
            shadow.spread, shadow.blur
        );
    }

    // An inner shadow reaches nowhere: it is clipped to the node's own shape,
    // whatever its offset and blur are, and this one's are the larger of the
    // two above.
    let scene = shadowed_scene(
        None,
        &[Shadow {
            kind: ShadowKind::Inner,
            offset: Vec2 { x: 6.0, y: -2.0 },
            blur: 8.0,
            spread: 3.0,
            color: color(0.0, 0.0, 0.0, 0.5),
        }],
        1.0,
    );
    let painter = scene.pack();
    let inner = painter.instances().rect_instances(0)[1];
    assert_eq!(inner.kind, InstanceKind::ShadowInner.as_u32());
    assert_eq!(inner.outset, 0.0);
}

/// A spread negative enough to collapse the shadow leaves the quad at the
/// instance's own bounds rather than shrinking it.
///
/// The floor is not decoration: a negative outset would make the vertex stage
/// build a quad *inside* `bounds`, and the shadow would be clipped by geometry
/// that is smaller than the node it belongs to.
#[test]
fn a_shadow_that_collapses_does_not_shrink_its_own_quad() {
    let scene = shadowed_scene(
        None,
        &[Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 0.0, y: 0.0 },
            blur: 0.0,
            spread: -40.0,
            color: color(0.0, 0.0, 0.0, 1.0),
        }],
        1.0,
    );
    let painter = scene.pack();
    assert_eq!(painter.instances().rect_instances(0)[0].outset, 0.0);
}

/// A shadow that inks nothing produces no instance at all — issue #285,
/// implemented natively rather than reproduced and filed again.
///
/// Two ways to reach zero effective alpha, and both are the issue's own: a
/// fully transparent shadow colour, and a node whose free-path alpha is zero.
/// The rows of the shadows that *do* ink are asserted alongside, because the
/// hazard of skipping an instance is that it shifts what the ones after it
/// name — and a row here is the shadow's position in its entry's own list, not
/// its position among the instances that were emitted.
#[test]
fn a_shadow_with_no_effective_alpha_emits_no_instance() {
    let visible = |kind: ShadowKind, alpha: f32| Shadow {
        kind,
        offset: Vec2 { x: 1.0, y: 2.0 },
        blur: 4.0,
        spread: 0.0,
        color: color(0.0, 0.0, 0.0, alpha),
    };
    let scene = shadowed_scene(
        None,
        &[
            visible(ShadowKind::Drop, 0.0),
            visible(ShadowKind::Drop, 0.75),
            visible(ShadowKind::Inner, 0.0),
            visible(ShadowKind::Inner, 0.5),
        ],
        1.0,
    );
    let painter = scene.pack();
    let instances = painter.instances().rect_instances(0);
    let shadows: Vec<(u32, u32)> = instances
        .iter()
        .filter(|i| InstanceKind::from_u32(i.kind).is_shadow())
        .map(|i| (i.kind, i.row))
        .collect();
    assert_eq!(
        shadows,
        vec![
            (InstanceKind::ShadowDrop.as_u32(), 1),
            (InstanceKind::ShadowInner.as_u32(), 3),
        ],
        "only the two that ink are packed, each still naming its own row"
    );

    // And the same node at zero opacity packs neither, however opaque the
    // shadow colours are.
    let scene = shadowed_scene(
        None,
        &[
            visible(ShadowKind::Drop, 1.0),
            visible(ShadowKind::Inner, 1.0),
        ],
        0.0,
    );
    let painter = scene.pack();
    assert!(
        painter
            .instances()
            .rect_instances(0)
            .iter()
            .all(|i| !InstanceKind::from_u32(i.kind).is_shadow()),
        "a node at zero opacity casts no shadow"
    );
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

/// Each instance's kind names the table its `row` indexes.
///
/// The replacement for a test that checked `tag` against `dashpaint`'s own
/// discriminant. There is no separate tag now — `kind` carries the sub-kind —
/// so a shadow cannot be read as a fill by forgetting to check something. What
/// is left to check is that the packer put each row in the table its kind
/// names.
#[test]
fn each_kind_names_the_table_its_row_indexes() {
    for scene in [vocabulary(), text()] {
        each_kind_names_its_table(&scene);
    }
}

fn each_kind_names_its_table(scene: &Scene) {
    let painter = scene.pack();
    for (index, rect) in scene.rects.iter().enumerate() {
        let entry = scene.paints.resolve(rect.paint);
        for instance in painter.instances().rect_instances(index as u32) {
            let row = instance.row as usize;
            match InstanceKind::from_u32(instance.kind) {
                InstanceKind::ShadowDrop => {
                    assert_eq!(scene.paints.all_shadows()[row].kind, ShadowKind::Drop)
                }
                InstanceKind::ShadowInner => {
                    assert_eq!(scene.paints.all_shadows()[row].kind, ShadowKind::Inner)
                }
                InstanceKind::Backdrop => {
                    assert_eq!(scene.paints.all_blurs()[row].kind, BlurKind::Backdrop)
                }
                InstanceKind::FillSolid => assert!(row < scene.paints.all_solids().len()),
                InstanceKind::FillGradient => assert!(row < scene.paints.all_gradients().len()),
                InstanceKind::FillImage => assert!(row < scene.paints.all_images().len()),
                InstanceKind::Stroke => {
                    assert!(row < scene.paints.all_strokes().len());
                    assert!(scene.paints.stroke(entry).is_some());
                }
                InstanceKind::Text => {
                    assert!(row < scene.glyphs.runs().len());
                    assert_eq!(
                        scene.glyphs.runs()[row].rect,
                        index as u32,
                        "a glyph instance sits in the span of the rect its run is anchored to"
                    );
                }
            }
        }
    }
}

/// No two kinds share a discriminant, and only the kinds that draw past their
/// own bounds carry an outset.
///
/// The first is the property the merge exists for: `kind` and `tag` used to be
/// separate fields whose values collided — `PaintTag::Solid`,
/// `ShadowKind::Inner` and `BlurKind::Backdrop` are all 1 — and story #580's
/// fragment shader read the tag without the kind and painted a shadow from the
/// solid table.
///
/// The second was "every packed instance's pad is zero" until story #584 gave
/// that word a meaning. The claim it becomes is stronger: a fill, a glyph, a
/// backdrop and an **inner** shadow all draw inside the box their instance is
/// stated over, so an outset on any of them would grow a quad for ink that does
/// not exist. Only a stroke and a drop shadow may carry one, and neither may
/// carry a negative one — a quad smaller than the instance's own bounds is not
/// what the member means.
#[test]
fn kinds_are_distinct_and_only_the_kinds_that_draw_outside_carry_an_outset() {
    let kinds = [
        InstanceKind::ShadowDrop,
        InstanceKind::ShadowInner,
        InstanceKind::Backdrop,
        InstanceKind::FillSolid,
        InstanceKind::FillGradient,
        InstanceKind::FillImage,
        InstanceKind::Stroke,
        InstanceKind::Text,
    ];
    let values: std::collections::BTreeSet<u32> = kinds.iter().map(|k| k.as_u32()).collect();
    assert_eq!(values.len(), kinds.len(), "two kinds share a value");
    for kind in kinds {
        assert_eq!(InstanceKind::from_u32(kind.as_u32()), kind);
    }

    let mut outside = 0;
    for scene in [vocabulary(), groups(), text()] {
        let painter = scene.pack();
        for instance in painter.instances().instances() {
            let kind = InstanceKind::from_u32(instance.kind);
            assert!(
                instance.outset >= 0.0,
                "{kind:?} carries a negative outset, which would shrink its quad"
            );
            if matches!(kind, InstanceKind::Stroke | InstanceKind::ShadowDrop) {
                outside += usize::from(instance.outset > 0.0);
                continue;
            }
            assert_eq!(
                instance.outset, 0.0,
                "{kind:?} draws inside its own bounds and must not grow its quad"
            );
        }
    }
    assert!(
        outside > 0,
        "no instance in three scenes carries an outset, so the assertion above proved nothing"
    );
}

/// A group starting past the rect table is named rather than silently dropped.
///
/// Added in review. The forward cursor cannot report it from inside the walk —
/// it simply never matches — so the check is at the end, exactly as the glyph
/// run's is. It matters more than a symmetry argument: the layer table is
/// index-aligned with the group slice, so a group that is never opened leaves
/// every later layer recorded at the wrong index, and the painter then
/// composites groups into each other's layers.
#[test]
#[should_panic(expected = "starts at rect")]
fn a_group_starting_past_the_rect_table_is_named() {
    let mut scene = groups();
    scene.groups.push(GroupComposite {
        start: 99,
        end: 100,
        alpha: 0.5,
    });
    scene.pack();
}
