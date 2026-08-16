//! Layer 3, the backdrop blur (story #733): does the frosted region hold a
//! blurred copy of what was composited beneath it, and does it hold it only
//! where the node's own shape reaches.
//!
//! Read every assertion here as "did the pipeline do this thing at all", not as
//! "is the falloff right". Nothing here compares against the reference painter;
//! the falloff is story #586's measurement, and the story's own body says
//! plainly that a per-pixel band cannot see a wide low-amplitude difference in
//! a blur anyway.
//!
//! # Every probe is placed where the two candidate answers differ
//!
//! That is not a stylistic preference in this crate. Story #584's corner probe
//! sat outside the drop shadow at the spread radius *and* at the unspread one,
//! so removing the spread survived the whole suite. The fixtures below are
//! built so that a sharp answer and a blurred answer disagree at the probe:
//! a hard colour seam runs down the middle of the target, and the panels
//! straddle it. Away from that seam every probe would read the same whatever
//! the shader did.

use dashpaint::{
    Blur, BlurKind, ClipBox, ClipIndex, ClipTable, Color, CornerRadii, EntryParts, GlyphRunTable,
    GroupComposite, ImageAsset, ImageFormat, ImageTable, PaintEntry, PaintTable, Painter,
    RectEntry, Vec2, VectorField,
};
use dashscene_gpu::{GpuPainter, ResidencyError};

mod common;
use common::{H, JPEG_FIXTURE, W, renderer, texel};

/// Where the red half ends and the blue half begins.
const SEAM: u32 = 32;

fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

/// An entry that fills with `color` and carries nothing else.
fn solid(paints: &mut PaintTable, color: Color) -> dashpaint::PaintIndex {
    let fill = paints.intern_fill(&dashpaint::FillSpec::Solid { color });
    paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    })
}

/// The same radius on all four corners.
fn all(radius: f32) -> CornerRadii {
    CornerRadii {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32, paint: dashpaint::PaintIndex) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }
}

/// A paint entry that draws no ink of its own and carries the given blurs.
///
/// No fill, deliberately: the panel's own colour would sit over the frosted
/// region and every probe below would be reading it instead of the blur.
fn frosted(paints: &mut PaintTable, blurs: &[Blur], corners: CornerRadii) -> dashpaint::PaintIndex {
    paints.push_with(
        PaintEntry {
            corners,
            ..PaintEntry::default()
        },
        EntryParts {
            blurs,
            ..EntryParts::default()
        },
    )
}

fn backdrop(radius: f32) -> Blur {
    Blur {
        kind: BlurKind::Backdrop,
        radius,
    }
}

fn draw(rects: &[RectEntry], paints: &PaintTable) -> Vec<u8> {
    draw_groups(rects, paints, &ClipTable::new(), &[])
}

fn draw_groups(
    rects: &[RectEntry],
    paints: &PaintTable,
    clips: &ClipTable,
    groups: &[GroupComposite],
) -> Vec<u8> {
    let mut painter = GpuPainter::new();
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        clips,
        groups,
        &GlyphRunTable::new(),
        None,
    );
    renderer()
        .render(
            painter.instances(),
            paints,
            &ImageTable::new(),
            clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum")
}

/// The two halves the panels are seen against: opaque red left of [`SEAM`],
/// opaque blue right of it, and a hard edge between them.
fn halves(paints: &mut PaintTable) -> Vec<RectEntry> {
    let red = solid(paints, rgba(1.0, 0.0, 0.0, 1.0));
    let blue = solid(paints, rgba(0.0, 0.0, 1.0, 1.0));
    vec![
        rect(0.0, 0.0, SEAM as f32, H as f32, red),
        rect(SEAM as f32, 0.0, (W - SEAM) as f32, H as f32, blue),
    ]
}

/// A backdrop blur replaces the region it covers with a blurred copy of what is
/// beneath it — the whole claim, at its simplest.
///
/// The probe is one texel left of the seam, where the sharp backdrop is pure
/// red and a blurred one has pulled blue across. A shader that drew nothing at
/// all leaves pure red here, which is exactly what this instance did from story
/// #578 until now.
#[test]
fn a_backdrop_replaces_its_region_with_a_blurred_copy_of_what_is_beneath() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    rects.push(rect(8.0, 8.0, 48.0, 32.0, panel));

    let pixels = draw(&rects, &paints);
    let inside = texel(&pixels, SEAM - 1, 24);
    assert!(
        inside[2] > 40,
        "one texel inside the panel and left of the seam should have blue pulled across it, \
         got {inside:?} — a pure red means the backdrop drew nothing",
    );
    assert!(
        inside[0] > 40,
        "and it should still be mostly its own side's red, got {inside:?}",
    );
}

/// Outside the panel the backdrop is untouched, and the probe for that is
/// **inside the panel's bounding box and outside its rounded shape**.
///
/// A probe simply away from the panel proves nothing: no quad reaches it, so it
/// would stay sharp however wrong the coverage was. The corner of a heavily
/// rounded panel is the one place the two answers differ — the quad covers it,
/// the shape does not — and it is the same trap story #584's inner shadow fell
/// into, where a probe two units out was discarded by the geometry before the
/// coverage ever ran.
///
/// Both probes take the **same x**, so the only thing that differs between them
/// is the shape coverage.
#[test]
fn the_frosted_region_stops_at_the_nodes_shape_and_not_at_its_box() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    // Narrow and centred on the seam, so its corners sit beside the seam rather
    // than far from it: at a corner the blur's window still reaches blue, so a
    // frosted corner and a sharp one differ.
    //
    // The radius is wide for the same reason the probes are placed where they
    // are. The corner the shape excludes is six or seven texels from the seam,
    // and at a sigma of 3.5 that is two sigma out — a real difference, but three
    // code points of it, which is a threshold measuring rounding rather than
    // coverage. At a radius of 20 the same texel sits inside the falloff.
    let panel = frosted(&mut paints, &[backdrop(20.0)], all(8.0));
    rects.push(rect(24.0, 8.0, 16.0, 32.0, panel));

    let pixels = draw(&rects, &paints);
    let corner = texel(&pixels, 25, 9);
    let middle = texel(&pixels, 25, 24);
    assert_eq!(
        corner,
        [255, 0, 0, 255],
        "the panel's rounded corner is outside its shape, so the backdrop there is untouched \
         red — got {corner:?}",
    );
    assert!(
        middle[2] > 40,
        "the same column at the panel's vertical middle is inside the shape and is frosted, \
         got {middle:?} — equal to the corner means the shape coverage was never applied",
    );
}

/// The kernel reads **past the node's own box**, which is what makes the
/// frosted region a copy of the real backdrop rather than of a cropped one.
///
/// `dashscene-skia` relies on the same property — Skia reads the halo from
/// outside the clip — and pins it with `the_backdrop_blur_reads_past_the_node_box`.
/// Here the panel sits entirely over the red half and stops short of the seam,
/// so a kernel confined to the panel would find nothing but red.
#[test]
fn the_kernel_reads_past_the_panels_own_box() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    // Right edge at the seam, so every texel of the panel is over red.
    rects.push(rect(8.0, 8.0, 24.0, 32.0, panel));

    let pixels = draw(&rects, &paints);
    let near_edge = texel(&pixels, SEAM - 2, 24);
    assert!(
        near_edge[2] > 20,
        "the panel stops at the seam and every texel under it is red, so blue at {:?} can only \
         have come from a tap outside the panel's own box — got {near_edge:?}",
        (SEAM - 2, 24),
    );
}

/// **Two backdrops, because one cannot falsify a row lookup.** They differ in
/// every field this painter reads of a blur — radius, position, size, corners
/// and the free-path alpha — so a renderer that resolved both from one row
/// still draws one of them wrong.
///
/// The discriminator is the radius: the narrow blur leaves a texel near the
/// seam much closer to its own side's colour than the wide one does.
#[test]
fn two_backdrops_each_resolve_their_own_row() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let sharp = frosted(&mut paints, &[backdrop(2.0)], CornerRadii::default());
    let soft = frosted(&mut paints, &[backdrop(16.0)], all(4.0));
    rects.push(rect(8.0, 4.0, 48.0, 16.0, sharp));
    let mut dim = rect(4.0, 26.0, 56.0, 18.0, soft);
    dim.opacity = 0.75;
    rects.push(dim);

    let pixels = draw(&rects, &paints);
    let narrow = texel(&pixels, SEAM - 3, 12);
    let wide = texel(&pixels, SEAM - 3, 34);
    assert!(
        narrow[0] > wide[0] + 20,
        "three texels left of the seam, the radius-2 panel should stay far closer to red than \
         the radius-16 one: narrow {narrow:?} against wide {wide:?} — equal values mean both \
         backdrops resolved the same blur row",
    );
    assert!(
        wide[2] > narrow[2] + 20,
        "and the wide one should have pulled far more blue across: narrow {narrow:?} against \
         wide {wide:?}",
    );
}

/// Each backdrop snapshots the target **at its own point in the stream**, not
/// once for the frame.
///
/// The second panel is drawn after an opaque fill has covered everything, so it
/// has nothing but that fill to blur and must come out its colour. A renderer
/// that took one snapshot before the first backdrop would blur the red and blue
/// halves instead, and the probe would carry them.
#[test]
fn the_second_backdrop_reads_what_the_first_one_and_everything_after_it_left() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let first = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    rects.push(rect(8.0, 8.0, 48.0, 32.0, first));
    let green = solid(&mut paints, rgba(0.0, 1.0, 0.0, 1.0));
    rects.push(rect(0.0, 0.0, W as f32, H as f32, green));
    let second = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    rects.push(rect(8.0, 8.0, 48.0, 32.0, second));

    let pixels = draw(&rects, &paints);
    let inside = texel(&pixels, SEAM, 24);
    assert_eq!(
        inside,
        [0, 255, 0, 255],
        "the second panel has only the green fill beneath it, so its blurred copy is green — \
         got {inside:?}, and any red or blue in it came from a snapshot taken before the green \
         was drawn",
    );
}

/// **A render-target group is a backdrop root**: a backdrop inside one samples
/// that group's layer and nothing further down.
///
/// `dashscene-skia` specifies this — the layer Skia filters is the innermost
/// open one — and the reason is in
/// `docs/decisions/backdrop-blur-is-core-vocabulary.md`: sampling through the
/// group would composite the backdrop twice, once directly and once inside the
/// group's own alpha.
///
/// The canvas holds red on the left and nothing on the right. The group holds
/// blue on the right and a panel across the seam. Sampling the group's layer
/// finds blue and transparency; sampling the canvas beneath it would find the
/// red, and the probe would carry red where the canvas has none.
#[test]
fn a_backdrop_inside_a_group_samples_that_groups_layer() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, rgba(1.0, 0.0, 0.0, 1.0));
    let blue = solid(&mut paints, rgba(0.0, 0.0, 1.0, 1.0));
    let panel = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    let rects = vec![
        rect(0.0, 0.0, SEAM as f32, H as f32, red),
        rect(SEAM as f32, 0.0, (W - SEAM) as f32, H as f32, blue),
        rect(8.0, 8.0, 48.0, 32.0, panel),
    ];
    // The group opens at the blue rect and closes after the panel, so the
    // canvas beneath it holds only the red half.
    let groups = [GroupComposite {
        start: 1,
        end: 3,
        alpha: 1.0,
    }];

    let pixels = draw_groups(&rects, &paints, &ClipTable::new(), &groups);
    let inside = texel(&pixels, SEAM + 4, 24);
    assert!(
        inside[0] < 20,
        "the group's layer holds no red at all, so a frosted texel inside the group must carry \
         none — got {inside:?}, and red in it means the blur sampled the canvas beneath the \
         group rather than the group's own layer",
    );
}

/// The kernel blurs **vertically as well as horizontally**.
///
/// Every other fixture in this file is stated over a vertical seam, so all of
/// them are satisfied by the horizontal pass alone — a renderer that ran the
/// first axis twice, or dropped the second, passes every one. That is the
/// uniform-symmetry trap this crate has hit before, and this is the axis it
/// leaves unfalsified: the seam here runs across rather than down.
#[test]
fn the_kernel_blurs_along_both_axes() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, rgba(1.0, 0.0, 0.0, 1.0));
    let blue = solid(&mut paints, rgba(0.0, 0.0, 1.0, 1.0));
    let panel = frosted(&mut paints, &[backdrop(16.0)], CornerRadii::default());
    let middle = (H / 2) as f32;
    let rects = vec![
        rect(0.0, 0.0, W as f32, middle, red),
        rect(0.0, middle, W as f32, H as f32 - middle, blue),
        rect(8.0, 4.0, 48.0, 40.0, panel),
    ];

    let pixels = draw(&rects, &paints);
    let above = texel(&pixels, 32, H / 2 - 2);
    assert!(
        above[2] > 40,
        "two texels above a horizontal seam, inside the panel, blue should have been pulled \
         upward — got {above:?}, and a pure red means only the horizontal axis ran",
    );
}

/// The vertical half of the kernel reads past the panel's box too, which is a
/// separate claim from the horizontal one and fails for a different reason.
///
/// The horizontal pass reads the snapshot, which is the whole target, so it
/// reaches past any box for free. The vertical pass reads the horizontal
/// pass's *output*, and a row that pass never wrote is transparent — so a
/// panel whose horizontal quad was not grown along the resolve's axis pulls in
/// transparency at its top and bottom edges instead of the backdrop beyond
/// them, and the frosted region darkens rather than frosting.
///
/// The panel's bottom edge sits exactly on a horizontal seam, so every texel
/// beneath it is red and any blue at the probe came from a row outside the box.
#[test]
fn the_vertical_half_reads_past_the_panels_own_box() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, rgba(1.0, 0.0, 0.0, 1.0));
    let blue = solid(&mut paints, rgba(0.0, 0.0, 1.0, 1.0));
    let panel = frosted(&mut paints, &[backdrop(16.0)], CornerRadii::default());
    let seam = (H / 2) as f32;
    let rects = vec![
        rect(0.0, 0.0, W as f32, seam, red),
        rect(0.0, seam, W as f32, H as f32 - seam, blue),
        // Bottom edge on the seam, so the panel covers red and nothing else.
        rect(8.0, 0.0, 48.0, seam, panel),
    ];

    let pixels = draw(&rects, &paints);
    let inside = texel(&pixels, 32, H / 2 - 2);
    assert!(
        inside[2] > 20,
        "the panel stops at the seam, so blue at this texel can only have come from a row the \
         horizontal pass wrote outside the panel's own box — got {inside:?}",
    );
    assert!(
        inside[3] > 250,
        "and it must not have pulled in transparency: alpha {} of 255",
        inside[3],
    );
}

/// Blurring a **uniform** field leaves it exactly as it was, at the target's
/// edge as much as in the middle.
///
/// This is what the tap clamp is for. `backdrop_blur_filter` uses
/// `TileMode::Clamp` and CSS's `backdrop-filter` specifies the same rule for
/// the same reason: past the target's edge there is no backdrop to read, and a
/// kernel that read zero there would darken every frosted node touching a
/// border — most visibly on a full-bleed panel, which is exactly what a frosted
/// navigation bar is.
///
/// The panels are flush against the target's edges and the field beneath them
/// is one colour, so the correct answer is that nothing changes anywhere. Any
/// darkening is a tap that fell off the target and was counted as transparent
/// black rather than clamped.
///
/// # The backdrop is partly transparent, and that is the second claim
///
/// A kernel whose weights do not sum to one scales everything it touches, alpha
/// included — and against an **opaque** backdrop that is undetectable, because
/// the readback divides the colour by the alpha and both moved by the same
/// factor. Every probe would read `[255, 0, 0, 255]` with the normaliser wrong
/// by any factor at all. A partially transparent field is what makes the alpha
/// channel carry the error instead of cancelling it.
///
/// The two panels take a wide radius and a **radius of 1**, because that is
/// where the discrete kernel and the continuous integral it is often normalised
/// by actually differ: at a sigma of 10.5 they agree to a fifth of a code
/// point, and at the sigma a radius of 1 maps to they are 4.6 % apart.
#[test]
fn blurring_a_uniform_backdrop_changes_nothing_even_at_the_targets_edge() {
    let mut paints = PaintTable::new();
    let half_red = solid(&mut paints, rgba(1.0, 0.0, 0.0, 0.5));
    let wide = frosted(&mut paints, &[backdrop(24.0)], CornerRadii::default());
    let narrow = frosted(&mut paints, &[backdrop(1.0)], CornerRadii::default());
    let rects = vec![
        rect(0.0, 0.0, W as f32, H as f32, half_red),
        rect(0.0, 0.0, 32.0, H as f32, wide),
        rect(32.0, 0.0, 32.0, H as f32, narrow),
    ];

    let pixels = draw(&rects, &paints);
    for (x, y) in [
        (0, 0),
        (0, 24),
        (1, 1),
        (16, 24),
        (30, 47),
        (33, 0),
        (48, 24),
        (63, 47),
    ] {
        assert_eq!(
            texel(&pixels, x, y),
            [255, 0, 0, 128],
            "a uniform backdrop blurs to itself, colour and alpha alike; texel {:?} moved",
            (x, y),
        );
    }
}

/// A free-path alpha below one **composites** the blurred copy over the sharp
/// original, where a full one **replaces** it.
///
/// `dashscene-skia`'s `backdrop_layer_paint` is where that discontinuity comes
/// from, and it is deliberate there: source-over is indistinguishable from
/// replacement for an opaque backdrop and wrong for a partially transparent
/// one, which is debt #405. A painter that ignored the alpha would draw the
/// same picture at every opacity — so this renders the same scene twice and
/// varies nothing but that one input, which is the only shape that can tell
/// them apart.
#[test]
fn the_free_path_alpha_decides_whether_the_copy_replaces_or_composites() {
    let frost = |opacity: f32| {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = frosted(&mut paints, &[backdrop(12.0)], CornerRadii::default());
        let mut node = rect(8.0, 8.0, 48.0, 32.0, panel);
        node.opacity = opacity;
        rects.push(node);
        texel(&draw(&rects, &paints), SEAM - 1, 24)
    };

    let replaced = frost(1.0);
    let composited = frost(0.5);
    assert!(
        replaced[2] > 40,
        "at full alpha the blurred copy replaces the region, got {replaced:?}",
    );
    assert!(
        composited[2] * 2 < replaced[2] + 12 && composited[2] * 2 + 12 > replaced[2],
        "at half alpha the copy composites over the sharp original, which halves how much blue \
         crossed the seam: {composited:?} against {replaced:?}",
    );
    assert!(
        composited[0] > replaced[0],
        "and leaves more of the original red standing: {composited:?} against {replaced:?}",
    );
}

/// The atlas the coverage-mask fixture lives in, and where its field sits
/// inside it — **not at the origin**, so a mapping that dropped the sub-rect's
/// offset draws the wrong texels rather than the right ones by luck. That is
/// the range-offset trap issues #650, #651, #561, #688 and #699 all record.
const MASK_ATLAS: u32 = 8;
const MASK_RECT: [u32; 4] = [3, 2, 4, 4];

/// A field that is **outside on its first column and inside on the other
/// three**, over [`MASK_RECT`].
///
/// Both halves of that are load-bearing, and one of them was not obvious.
///
/// The outside column is what makes the coverage falsifiable at all: a field
/// that were inside everywhere would be indistinguishable from no coverage
/// sampling, since the quad alone would be doing the work.
///
/// **The field reaching the right edge of its rectangle is what makes the
/// *quad* falsifiable**, and a first version of this fixture had it the other
/// way round and could not. `msdf_sample` clamps its coordinate into the
/// sub-rect, so a fragment past the field's quad reads whatever sits in the
/// edge column. With an *outside* edge column that read returns no coverage and
/// a painter taking its quad from the node's box still draws the right picture
/// — the mutation survived the whole file. With an *inside* one the same
/// painter frosts every texel from the field's quad to the box's far edge,
/// which is the defect the geometry is the only guard against.
fn mask_field() -> ImageAsset {
    let [rx, ry, rw, rh] = MASK_RECT;
    let mut bytes = Vec::with_capacity((MASK_ATLAS * MASK_ATLAS * 4) as usize);
    for y in 0..MASK_ATLAS {
        for x in 0..MASK_ATLAS {
            let inside_rect = x >= rx && x < rx + rw && y >= ry && y < ry + rh;
            // Derived from the rectangle rather than restated, so moving the
            // rectangle moves the field with it.
            let v = match x.checked_sub(rx) {
                Some(0) | None => 0,
                Some(_) if inside_rect => 255,
                _ => 0,
            };
            bytes.extend_from_slice(&[v, v, v, 255]);
        }
    }
    ImageAsset {
        format: ImageFormat::Rgba8Unorm,
        bytes,
    }
}

/// The masked-and-frosted node the two `field_draws` tests draw: the two halves
/// under a node whose fill **and** backdrop are both masked by `field`, over
/// `atlas`.
///
/// Shared because those two tests differ in what they measure and not in what
/// they draw (issue #1166). The geometry is
/// `a_masked_backdrop_follows_the_fields_outline_rather_than_its_box`'s, and it
/// is not arbitrary — the panel straddles [`SEAM`], which is what every probe in
/// this file depends on.
///
/// **That reference test still builds its own**, so this is three copies down to
/// two rather than to one. It draws a different node — `frosted`, with no fill —
/// so folding it in would mean a second parameter for the one thing the two
/// disagree about. Moving the panel there still moves it out from under these
/// two callers silently.
///
/// **The fill is deliberate, and it is the opposite of what [`frosted`] does.**
/// That helper omits one because a panel's own colour would sit over the frosted
/// region and every probe there would read it instead of the blur. Neither
/// caller here has a probe to protect, and the fill is what puts **both
/// consumers of the flag in one node**: the masked fill is the arm `paint.wgsl`
/// gates and the backdrop is the one `backdrop_mask` filters, so a field that
/// draws nothing has to be empty on both, and a residency fetch added to either
/// path is in this scene.
///
/// **The fill must stay a solid or a gradient.** `pack_rect` emits a fill
/// instance on the shape branch for those two only, so an image or pattern fill
/// here would silently leave the backdrop as the sole consumer carrying the
/// shape — and the degenerate test's `last_draw_runs() == 5` would need a
/// different number with nothing saying why.
///
/// The atlas index is taken rather than pushed because the caller owns the
/// [`ImageTable`] it also renders with. Both callers push the same baked payload
/// today; the parameter is what lets one of them stop.
fn masked_frosted_scene(atlas: u32, field: VectorField) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let green = paints.intern_fill(&dashpaint::FillSpec::Solid {
        color: rgba(0.0, 1.0, 0.0, 1.0),
    });
    let panel = paints.push_with(
        PaintEntry {
            fill: green,
            ..PaintEntry::default()
        },
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: atlas,
                ..field
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
    (rects, paints)
}

/// **A masked node's backdrop follows the field's outline, not its box.**
///
/// This is the arm the hero's own frosted panel takes — a Figma VECTOR carrying
/// `BACKGROUND_BLUR` — and `dashscene-skia` draws it through
/// `draw_backdrop_blur_field` rather than `draw_backdrop_blur_box`.
///
/// Three probes, and each one falsifies a different way of getting it wrong:
///
/// - inside the field's outline: frosted, or nothing was drawn at all;
/// - inside the field's quad but on its **outside** column: untouched, or the
///   coverage was never sampled and the quad alone was doing the work;
/// - inside the node's box and past the field's **quad**: untouched, or the
///   quad was taken from the box — and that one is not cosmetic, because
///   `msdf_sample` clamps into the sub-rect, so out there it reports the edge
///   column's coverage, and this field's edge column is *inside*. See
///   [`mask_field`] for why that detail is what makes this probe work at all.
#[test]
fn a_masked_backdrop_follows_the_fields_outline_rather_than_its_box() {
    let mut images = ImageTable::new();
    let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);

    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            // Wide enough that a texel two past the field's quad still sits
            // inside the falloff: at a sigma of 10.5 the probe at x = 46 is
            // 1.3 sigma from the seam, so a painter that frosted it would pull
            // tens of code points of red across and not one or two.
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: atlas,
                atlas_rect: MASK_RECT,
                // The field's quad is 20 units of a 36-unit box, so the box
                // reaches x 24..60 and the field only x 24..44. Everything from
                // 44 rightward is inside the box and outside the field.
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                // 20 device units over 4 atlas texels is five pixels a texel.
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));

    let mut painter = GpuPainter::new();
    let clips = ClipTable::new();
    painter.paint(
        &rects,
        &paints,
        &images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let pixels = renderer()
        .render(
            painter.instances(),
            &paints,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");

    // The field's four atlas columns map to five device units each: x 24..29 is
    // the outside column, 29..44 the inside ones, and 44..60 is inside the
    // node's box and past the field's quad entirely.
    let outside_column = texel(&pixels, 26, 24);
    let inside_field = texel(&pixels, 36, 24);
    let past_the_quad = texel(&pixels, 46, 24);
    assert!(
        inside_field[0] > 40,
        "inside the field and four texels right of the seam, red should have crossed — got \
         {inside_field:?}, and a pure blue means the masked arm drew nothing",
    );
    assert_eq!(
        outside_column,
        [255, 0, 0, 255],
        "the field's first column is outside its outline, so the backdrop there is untouched \
         red — got {outside_column:?}, which means the coverage was never sampled and the quad \
         alone was doing the work",
    );
    assert_eq!(
        past_the_quad,
        [0, 0, 255, 255],
        "past the field's quad but still inside the node's box, the backdrop is untouched — \
         got {past_the_quad:?}, which means the quad came from the node's box and \
         `msdf_sample`'s clamp reported this field's inside edge column out here",
    );
}

/// **A refused backdrop does not renumber the one behind it** (issues #972 and
/// #994).
///
/// `BlurTargets` builds one bind-group pair per backdrop it is prepared for,
/// each binding that backdrop's own coverage atlas, and `pass` indexes them by
/// position. So there are two numbers here — a backdrop's position in the plan
/// and its slot in that list — and everything turns on which one reaches
/// `pass`.
///
/// **They have been the same number and are not any more.** Under issue #972
/// `BlurTargets` was prepared for every _planned_ backdrop, so the two agreed
/// and a refused backdrop consumed a slot it never drew through; the defect was
/// a `resolved_backdrops += 1` made conditional, which moved every backdrop
/// behind a refused one onto the previous one's mask — the 1x1 placeholder
/// nothing ever writes. The second node's frost vanished with no refusal
/// recorded, which is a silent drop and the thing P4 forbids. Under issue #994
/// a refused backdrop is filtered out before `prepare`, so the two numbers
/// differ by every refusal ahead of it, and `PlannedBackdrop::slot` — assigned
/// by the step that does the filtering — is the one that binds.
///
/// This fixture is what tells them apart: refused first, so the live backdrop
/// sits at plan position 1 and slot 0. Taking the plan position instead now
/// indexes `bind_groups[2]` of a two-element list and panics, where before it
/// drew the wrong picture.
///
/// Two backdrops, because one cannot see an off-by-one in an index — and no
/// other test in this file draws two masked backdrops at all.
#[test]
fn a_refused_backdrop_does_not_renumber_the_one_behind_it() {
    let mut images = ImageTable::new();
    // Pushed first, so the refused node is also first in the plan.
    let refused = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });
    // Both axes, not the pair, for the reason its two siblings give.
    let extent = images.resolve(refused);
    assert!(
        extent.width != 0 && extent.height != 0,
        "the fixture must have an extent on both axes, or `resident_image` answers with the \
         no-bytes case before it ever reaches a decoder and nothing is refused — got {} x {}",
        extent.width,
        extent.height,
    );
    let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);

    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);

    // The refused one, below the live node and **with its top-left corner one
    // texel left of the seam**. A refused field zeroes the plane, so the quad
    // that grows from that corner is where the defect drew; putting the corner
    // beside the seam is what makes the sweep below able to fail at all. Placed
    // in a far corner — where this fixture first put it — the frost would be a
    // half-coverage copy of a uniform red neighbourhood, which is that same
    // red, and the sweep would pass with the defect present. The module
    // docstring says exactly this about probes and it applies to sweeps too.
    let dead = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: refused,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 8.0, 4.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(SEAM as f32 - 1.0, 44.0, 8.0, 4.0, dead));

    // And the live one — `a_masked_backdrop_follows_the_fields_outline_rather_than_its_box`'s
    // node exactly, so its three probes mean here what they mean there.
    let panel = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: atlas,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));

    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(
        &rects,
        &paints,
        &images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let mut renderer = renderer();
    let pixels = renderer
        .render(
            painter.instances(),
            &paints,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");

    // Which one was refused, not merely how many. A count alone would hold just
    // as well if the *panel's* baked atlas had been the refused one, and every
    // picture assertion below is written for the opposite pairing.
    let refusals = renderer.refusals();
    assert_eq!(
        refusals.len(),
        1,
        "only the first node's field is refused — got {refusals:?}",
    );
    assert_eq!(refusals[0].what, "a vector field's atlas");
    assert_eq!(
        refusals[0].row, refused,
        "the refused row is the JPEG the first node names, not the panel's baked atlas",
    );
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
    );

    // The live backdrop draws its own picture, unchanged by the refused one in
    // front of it. A renumbered slot binds the placeholder here and this probe
    // reads pure blue.
    let inside_field = texel(&pixels, 36, 24);
    assert!(
        inside_field[0] > 40,
        "the second backdrop frosts its own outline — got {inside_field:?}, and a pure blue means \
         it bound the refused node's bind group and sampled the placeholder",
    );
    assert_eq!(
        texel(&pixels, 26, 24),
        [255, 0, 0, 255],
        "its field's first column is still outside its outline",
    );
    assert_eq!(
        texel(&pixels, 46, 24),
        [0, 0, 255, 255],
        "and past its quad the backdrop is still untouched",
    );

    // And the refused one drew nothing. The band below the live node's box,
    // whole width: its quad reaches y 41 at most once the antialiasing width is
    // added, so everything from 42 down belongs to the refused node alone, and
    // its corner sits on the seam where a frost of either half is visible.
    for y in 42..H {
        for x in 0..W {
            let untouched = if x < SEAM {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                texel(&pixels, x, y),
                untouched,
                "the refused node frosted ({x}, {y})"
            );
        }
    }
}

/// **A backdrop whose coverage field was refused draws nothing at all** (issue
/// #972).
///
/// The masked arm of `fs_blur_resolve` reads four members of `GpuShape`, and
/// `GpuBlur::masked` used to say they were there whenever the instance named a
/// field — never whether that field resolved. A refused one leaves the row
/// zeroed, so the shader computed `msdf_coverage(sample, px_range = 0)`, which
/// is `0.5` for every sample it could possibly take, over a plane of no area
/// that `clamped_quad` then grows by the antialiasing width. The result was a
/// small square of half-strength frost at the node's top-left corner, on a
/// pipeline that writes with no blending: a plausible wrong picture on exactly
/// the path issues #718 and #720 exist to make quiet.
///
/// The whole canvas rather than a probe, for the reason the image-fill refusal
/// gives: the defect's own footprint is a couple of texels wide and sits where
/// no probe placed for the *drawn* case would look.
#[test]
fn a_refused_coverage_field_leaves_the_backdrop_untouched() {
    let mut images = ImageTable::new();
    let field = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });
    // Both axes, not the pair: `resident_image` returns early when *either* is
    // zero, so `!= (0, 0)` would pass a payload it still never offers a decoder.
    let extent = images.resolve(field);
    assert!(
        extent.width != 0 && extent.height != 0,
        "the fixture must have an extent on both axes, or `resident_image` answers with the \
         no-bytes case before it ever reaches a decoder and nothing is refused — got {} x {}",
        extent.width,
        extent.height,
    );

    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    // The fixture of `a_masked_backdrop_follows_the_fields_outline_rather_than_its_box`,
    // with one thing changed: the field's payload cannot be decoded. So that
    // test's picture is what this one would draw if the refusal were not
    // honoured, and it frosts a wide region straddling the seam.
    let panel = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: field,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));

    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(
        &rects,
        &paints,
        &images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let mut renderer = renderer();
    let pixels = renderer
        .render(
            painter.instances(),
            &paints,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");

    // Named, not silent — the half P4 is about.
    let refusals = renderer.refusals();
    assert_eq!(
        refusals.len(),
        1,
        "one refused payload is one refusal — got {refusals:?}",
    );
    assert_eq!(
        refusals[0].what, "a vector field's atlas",
        "the consumer is named, because an image fill and a field's atlas are the same table row \
         with very different symptoms",
    );
    assert_eq!(refusals[0].row, field);
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
    );

    // And nothing drawn: the two halves stand exactly as they were composited.
    for y in 0..H {
        for x in 0..W {
            let untouched = if x < SEAM {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                texel(&pixels, x, y),
                untouched,
                "a refused coverage field frosted ({x}, {y})"
            );
        }
    }
}

/// **A refused backdrop's blur row is still checked against the table that
/// assigned it** (issue #1022).
///
/// The row check is a *release* panic by design: it guards a caller that packs
/// against one `PaintTable` and renders against another, which is the mistake
/// that produces a plausible wrong picture rather than an absent one. It opened
/// `Renderer::resolve_backdrop` and ran for every planned backdrop, because the
/// refusal was decided further down that same function. Issue #994 moved the
/// refusal up to `backdrop_mask` and left the check below it, so whether a
/// packer/renderer mismatch was named at all had become conditional on an
/// unrelated residency outcome — named for a backdrop whose field resolved,
/// silent for one whose field did not.
///
/// **The refusal is what makes this a regression test**, and it is asserted
/// rather than assumed. The sibling case — a backdrop that draws, with the same
/// bad row — panicked throughout, so a test over one would have passed before
/// the fix and after it. If the JPEG ever stops being refused — a decoder linked
/// for it, a fixture swap, a payload whose extent makes `resident_image` answer
/// on the no-extent arm instead — this scene silently becomes that one, and the
/// same `names blur row 0 of 0` still arrives from the drawn path. So the frame
/// is drawn **first** against the table that holds the blur row, where it does
/// not panic, and the refusal is checked there by consumer and by error.
///
/// That check is load-bearing under `should_panic` only because the attribute
/// names its message: a failed assertion panics too, and with a different
/// message, so `expected` is what makes a broken premise a failure rather than
/// a pass.
///
/// The two tables are built by the same calls in the same order and differ in
/// one thing: the second holds no blur rows. So every other index the frame
/// resolves — the entry, the coverage field, the two halves' fills — still lines
/// up, and the row is the only thing that can panic.
#[test]
#[should_panic(expected = "names blur row 0 of 0")]
fn a_refused_backdrops_blur_row_is_still_checked_against_its_table() {
    let mut images = ImageTable::new();
    let field = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });

    /// The scene, over a table that holds `blurs` — the same rects and the same
    /// field either way.
    fn scene(field: u32, blurs: &[Blur]) -> (PaintTable, Vec<RectEntry>) {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = paints.push_with(
            PaintEntry::default(),
            EntryParts {
                blurs,
                shape: Some(VectorField {
                    image: field,
                    atlas_rect: MASK_RECT,
                    plane_bounds: [0.0, 0.0, 20.0, 32.0],
                    distance_range: 0.5,
                }),
                ..EntryParts::default()
            },
        );
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (paints, rects)
    }

    let (packed, rects) = scene(field, &[backdrop(24.0)]);
    let (rendered, _) = scene(field, &[]);
    assert!(
        rendered.all_blurs().is_empty(),
        "the render-side table must hold no blur row, or the instance's row 0 is valid and \
         nothing is being checked",
    );

    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(
        &rects,
        &packed,
        &images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let mut renderer = renderer();

    // The premise, proved rather than assumed, and against the table that holds
    // the blur row so that this frame reaches the end. A backdrop whose field
    // resolved would panic here too, from the drawn path, and this test would
    // then be asserting nothing #1022 is about.
    let _ = renderer
        .render(
            painter.instances(),
            &packed,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");
    let refusals = renderer.refusals();
    assert_eq!(
        refusals.len(),
        1,
        "the coverage field must be refused, or this scene is the drawn case — got {refusals:?}",
    );
    assert_eq!(refusals[0].what, "a vector field's atlas");
    assert_eq!(refusals[0].row, field);
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
        "refused for the reason this test needs — a no-extent payload never reaches a decoder and \
         would be a different scene",
    );

    // And now the same instances against the table that holds no blur row.
    let _ = renderer.render(
        painter.instances(),
        &rendered,
        &images,
        &clips,
        &GlyphRunTable::new(),
        W,
        H,
    );
}

/// **A degenerate coverage field draws nothing** — the painter's half of issue
/// #1021.
///
/// `field_draws` rejects a field with no quad or no atlas rectangle *before*
/// residency is asked, so the row keeps `GpuShape::default()` and both consumers
/// read the same zero a refusal leaves: the backdrop is not drawn and the masked
/// fill's coverage arm is gated shut. Nothing covered that, and the guard is not
/// cosmetic — every mapping in `gpu_shape` divides by the atlas rectangle, so
/// removing it feeds `msdf_coverage` a NaN range over a quad `clamped_quad` has
/// grown by the antialiasing width.
///
/// # What this falsifies, and what it only pins
///
/// Both rejections are here because they are independent conditions over
/// independent members, but they are **not equally falsifiable**, and the
/// difference was measured rather than reasoned about:
///
/// - Dropping the **quad** half of `field_draws` fails this test. A field with
///   no quad divides by nothing, keeps a finite range, and frosts a corner
///   patch — the same shape of wrong picture issue #972 was filed for.
/// - Dropping the **atlas-rectangle** half changes no texel either consumer
///   draws, on this adapter. `gpu_shape` divides by the rectangle, so the row
///   goes out with a NaN `half_uv` and an infinite `px_range`, and both arms
///   come back at zero coverage and discard. The guard stays because that is a
///   coincidence of how one adapter resolves a NaN sampler coordinate and not
///   a decision the shader states — the same argument `GpuMsdfRow::resolved`
///   makes against inferring absence from a value.
/// - Dropping the **finite-extent** half fails this test on each of the six
///   non-finite rows (issue #1034). Four of them carry an infinite bound, which
///   the ordering admits — `f32::INFINITY > 0.0` is true and so is `0.0 >
///   f32::NEG_INFINITY` — and two carry four finite bounds whose difference
///   overflows, which a predicate testing each bound admits as well. Before the
///   fix the field resolved, the backdrop was planned, and this frame took
///   **five** draws rather than one, measured. The reference painter did worse
///   with the same field — its backdrop **erased** the content beneath the node
///   — which is what made this a divergence and not only a wrong picture.
///
/// **The picture is not the only observable**, which is why that case is guarded
/// here rather than merely pinned. A field that resolves makes `backdrop_mask`
/// answer `Some`, so the backdrop is planned, `resolve_backdrop` encodes its two
/// passes, and the frame routes through `BlurTargets::base` and blits back. It
/// also brings the masked fill naming that field back into a range of its own,
/// which since issue #1024 it is otherwise left out of — **four** draws in all
/// that a degenerate field does not pay for. `Renderer::last_draw_runs` exists
/// for exactly this distinction and its own doc says so: a test asserting only
/// the picture cannot tell a frame that batched from one that did not.
///
/// **This is not the whole of #1021.** What that issue is about is that the drop
/// is nowhere *named* — a refused payload is recorded on `Renderer::refusals`
/// and a degenerate one reaches no diagnostic at any seam, which is what P4
/// forbids. Nothing is asserted about refusals here, deliberately: the seam that
/// issue argues for is `dashscene-validator`, which already names
/// `asset.image-no-bytes` for the sibling case and would settle both painters
/// at once where a check in `resolve_frame` names it for this one alone.
///
/// The atlas is a real baked payload, so residency is not what stops it.
#[test]
fn a_degenerate_coverage_field_draws_nothing() {
    // One device for every row. `Renderer::new` is an adapter request and a
    // device creation, the dominant cost of every test in this file.
    //
    // **The renderer does carry state between rows**, and the sound row below is
    // what puts it there: that row plans a backdrop, so `BlurTargets` allocates
    // and then holds its frame-wide targets across the idle rows that follow,
    // under the grace `TARGET_GRACE_FRAMES` names. Sharing is still sound —
    // each `render` call plans its own frame, the first pass on the target
    // clears, and what is held names nothing a later row reads — but the rows
    // are order-dependent, and the sound one is first on purpose: it is the
    // premise every row after it is read against.
    let mut renderer = renderer();
    for (what, atlas_rect, plane_bounds, draws) in [
        // **The sound row, and it is the premise the rest rest on** (issue
        // #1143). Without it the refusal check below cannot fail for any row:
        // `resolve_frame` reads `field_draws(field) && let Some(slot) =
        // self.resident_image(..)`, so a rejected field short-circuits before
        // residency is ever asked and `refusals()` is empty whatever the
        // payload is. That assertion passed against a zero-byte atlas, which is
        // exactly the case it claims to exclude.
        //
        // This row reaches residency through the same builder, so "no refusal"
        // becomes a statement about this atlas rather than about a predicate
        // that never asked. It also supplies the contrast the draw count is
        // read against: a field that resolves plans the backdrop and brings the
        // masked fill back into a range of its own.
        ("a sound field", MASK_RECT, [0.0, 0.0, 20.0, 32.0], true),
        (
            "no atlas rectangle",
            // Derived, so that moving `MASK_RECT` — which its own doc says is
            // deliberately off the origin and may be moved to re-test the
            // offset trap — moves this row with it and with the residency
            // test's, which derives the same way.
            [MASK_RECT[0], MASK_RECT[1], 0, 0],
            [0.0, 0.0, 20.0, 32.0],
            false,
        ),
        ("no quad", MASK_RECT, [0.0, 0.0, 0.0, 0.0], false),
        // The four positions an infinity passes the ordering in (issue #1034),
        // and so the four this frame drew before `field_draws` required finite
        // bounds. `left` and `top` take the negative one because `right > left`
        // is what they have to satisfy; `right` and `bottom` take the positive
        // one for the same reason mirrored. An infinity in the other position
        // fails the ordering and was already rejected, so it would pin nothing.
        (
            "an infinite left bound",
            MASK_RECT,
            [f32::NEG_INFINITY, 0.0, 20.0, 32.0],
            false,
        ),
        (
            "an infinite top bound",
            MASK_RECT,
            [0.0, f32::NEG_INFINITY, 20.0, 32.0],
            false,
        ),
        (
            "an infinite right bound",
            MASK_RECT,
            [0.0, 0.0, f32::INFINITY, 32.0],
            false,
        ),
        (
            "an infinite bottom bound",
            MASK_RECT,
            [0.0, 0.0, 20.0, f32::INFINITY],
            false,
        ),
        // Four finite bounds in the right order whose **difference** is not
        // finite: `3.0e38 - -3.0e38` is `6.0e38`, which overflows f32. This is
        // why `field_draws` tests the extent rather than the four bounds; a
        // version checking `is_finite` on each bound admits these two and
        // computes the same infinite `px_range` the rows above did.
        (
            "a plane quad whose width overflows",
            MASK_RECT,
            [-3.0e38, 0.0, 3.0e38, 32.0],
            false,
        ),
        (
            "a plane quad whose height overflows",
            MASK_RECT,
            [0.0, -3.0e38, 20.0, 3.0e38],
            false,
        ),
    ] {
        let mut images = ImageTable::new();
        let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);
        // The picture this would draw with the guard removed is not the
        // geometry test's: the fill `masked_frosted_scene` carries shades the
        // field's plane quad green at half coverage as well, over whatever
        // frost the backdrop leaves.
        let (rects, paints) = masked_frosted_scene(
            atlas,
            VectorField {
                // `image` is the helper's own argument; whatever is written
                // here is overwritten by it.
                image: 0,
                atlas_rect,
                plane_bounds,
                distance_range: 0.5,
            },
        );

        let clips = ClipTable::new();
        let mut painter = GpuPainter::new();
        painter.paint(
            &rects,
            &paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        let pixels = renderer
            .render(
                painter.instances(),
                &paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");

        // The premise, and it is the **sound** row that proves it: the atlas is
        // a real baked payload, so this scene is about degeneracy and not about
        // a refusal wearing its clothes. A degenerate row cannot prove that on
        // its own — `field_draws` short-circuits before residency is asked, so
        // this assertion holds for it whatever the payload is (issue #1143).
        // It asserts nothing about the *diagnostic* issue #1021 asks for.
        assert!(
            renderer.refusals().is_empty(),
            "the field's atlas must be resident, or this measures a refusal and not a field with \
             {what} — got {:?}",
            renderer.refusals(),
        );
        // And the draw count, which is what makes the atlas-rectangle case
        // falsifiable where its picture cannot be. A field that resolved would
        // plan the backdrop: two passes for the blur and one blit to put the
        // base back, against the single instance run this frame encodes.
        //
        // **One.** `composite::plan` breaks the pass at the backdrop instance
        // whether or not it draws, so there is a range either side of it — and
        // since issue #1024 the range after it is empty, because the masked
        // fill names the same unresolved field and is left out of every run.
        // So the two halves are the whole of what this frame submits.
        //
        // A resolved field adds four to that: the fill's own instance back in a
        // second range, the blur's two passes, and the base blit.
        //
        // The sound row is what says those four are real rather than a number
        // this test invented: it takes the same path and reports five.
        assert_eq!(
            renderer.last_draw_runs(),
            if draws { 5 } else { 1 },
            "a field with {what} must leave the frame at {} draws — a resolved field plans the \
             backdrop and draws the masked fill, and a rejected one leaves the one instance run \
             its two halves make. For a rejected field no texel of this fixture can show which \
             of the two happened, which is why the count is asserted at all",
            if draws { 5 } else { 1 },
        );
        if draws {
            continue;
        }

        // The whole canvas rather than a probe, for the reason the refusal tests
        // give: a NaN coordinate can land anywhere, so a check at one point
        // could miss what it drew.
        for y in 0..H {
            for x in 0..W {
                let untouched = if x < SEAM {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                assert_eq!(
                    texel(&pixels, x, y),
                    untouched,
                    "a field with {what} drew at ({x}, {y})"
                );
            }
        }
    }
}

/// **A frame whose only backdrop is refused allocates nothing** (issue #994).
///
/// The refusal used to be decided inside `Renderer::resolve_backdrop`, which
/// runs after every allocation that backdrop needed has been made: three
/// drawable-sized textures and their views — the largest per-frame allocation
/// this painter makes — two uniform buffers and two bind groups for a slot
/// nothing writes, plus routing the whole frame through `BlurTargets::base`
/// and a full-target blit to put it back. `backdrop_mask` now decides it where
/// `backdrop_masks` is built, which is before any of that.
///
/// # The comparison, and why it is three scenes rather than two
///
/// Equality alone is not falsifiable: a frame that allocates nothing equals a
/// frame that allocates nothing whether or not `Renderer::allocations` counts
/// `BlurTargets` at all, and that term has gone uncounted three times in this
/// crate — `a_frame_with_a_backdrop_allocates_and_a_steady_one_does_not` is the
/// test written for exactly that. So the live backdrop is here too, and it is
/// the one that must differ.
///
/// **The live one carries no coverage mask**, and that is deliberate rather
/// than incidental: it makes the JPEG the only payload any of the three scenes
/// names, so residency is identical across all of them and the delta below is
/// the blur targets and nothing else. A live *masked* backdrop would need a
/// baked atlas of its own, and making that resident is an allocation this
/// measurement would then have to exclude.
///
/// Every scene is drawn once before anything is measured, for the reason its
/// sibling gives: cold frames differ in instance count, and `Frame`'s own
/// buffers growing satisfies "allocated more" whatever the blur targets do.
#[test]
fn a_frame_whose_only_backdrop_is_refused_allocates_nothing() {
    let mut images = ImageTable::new();
    let field = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });
    // Both axes, not the pair, for the reason its two siblings give.
    let extent = images.resolve(field);
    assert!(
        extent.width != 0 && extent.height != 0,
        "the fixture must have an extent on both axes, or `resident_image` answers with the \
         no-bytes case before it ever reaches a decoder and nothing is refused — got {} x {}",
        extent.width,
        extent.height,
    );

    // Three scenes over one image table. `plain` holds no backdrop at all,
    // `refused` one confined to the undecodable field, and `live` one over the
    // node's own box — the only one of the three that draws.
    // A radius of zero emits no backdrop instance at all (`pack::frosts`), so
    // this is `live` with one number changed and nothing else.
    let plain = {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = frosted(&mut paints, &[backdrop(0.0)], CornerRadii::default());
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (rects, paints)
    };
    let refused = {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = paints.push_with(
            PaintEntry::default(),
            EntryParts {
                blurs: &[backdrop(24.0)],
                shape: Some(VectorField {
                    image: field,
                    atlas_rect: MASK_RECT,
                    plane_bounds: [0.0, 0.0, 20.0, 32.0],
                    distance_range: 0.5,
                }),
                ..EntryParts::default()
            },
        );
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (rects, paints)
    };
    let live = {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = frosted(&mut paints, &[backdrop(24.0)], CornerRadii::default());
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (rects, paints)
    };

    let clips = ClipTable::new();
    let mut renderer = renderer();
    // The refusals with the count, because `resolve_frame` clears them at the
    // head of every frame: read after a later draw they would be that frame's.
    let mut draw_once = |rects: &[RectEntry], paints: &PaintTable| {
        let mut painter = GpuPainter::new();
        painter.paint(
            rects,
            paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        (renderer.allocations(), renderer.refusals().to_vec())
    };

    draw_once(&live.0, &live.1);
    draw_once(&refused.0, &refused.1);
    draw_once(&plain.0, &plain.1);

    let (after_plain, _) = draw_once(&plain.0, &plain.1);
    let (after_refused, refusals) = draw_once(&refused.0, &refused.1);
    assert_eq!(
        after_refused, after_plain,
        "a frame whose only backdrop is refused must allocate exactly what a frame with no \
         backdrop allocates — {after_plain} then {after_refused}, so it is still building the \
         targets, the uniforms and the bind groups for a backdrop that encodes nothing",
    );

    // Named, not silent: dropping the backdrop earlier must not drop the
    // diagnostic with it. The refusal is recorded in `resolve_frame`, which
    // runs before any of this, and this is what says so.
    assert_eq!(
        refusals.len(),
        1,
        "one refused payload is one refusal — got {refusals:?}",
    );
    assert_eq!(refusals[0].what, "a vector field's atlas");
    assert_eq!(refusals[0].row, field);
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
    );

    let (after_live, _) = draw_once(&live.0, &live.1);
    assert!(
        after_live > after_refused,
        "a backdrop that draws still allocates its targets: {after_refused} then {after_live} \
         — equal means `Renderer::allocations` does not count `BlurTargets`, and the assertion \
         above would hold with the defect present",
    );
}

/// **A refusal that changes from frame to frame does not rebuild the blur
/// targets** (issue #1020).
///
/// Issue #994 filtered a refused backdrop out before `BlurTargets::prepare`,
/// which is the saving `a_frame_whose_only_backdrop_is_refused_allocates_nothing`
/// measures. Its cost is in the other direction: a frame that plans one backdrop
/// and refuses it now reaches the branch that released everything, so a refusal
/// that is not stable across frames paid three drawable-sized textures and their
/// views back on every change.
///
/// **This is reachable rather than hypothetical.**
/// `ResidencyError::FrameExceedsAtlas` is returned as a bare `Err` and is
/// deliberately not put in the permanent refusal memo, so it is decided per
/// frame from what else that frame made resident — a backdrop can therefore be
/// refused on one frame and drawn on the next, indefinitely. It is the **only**
/// arm of `ResidencyError` that escapes the memo: `NoDecoder`,
/// `UnsupportedFormat` and `TooLarge` all go through `Residency::refuse`, which
/// records them, so those three are stable and cannot oscillate. This fixture
/// uses `NoDecoder` because it is the one a test can produce deterministically,
/// and the branch of `prepare` it exercises is the same one.
///
/// # What is compared, and why one alternation is not enough
///
/// A single refused frame proves nothing: the targets survive
/// `TARGET_GRACE_FRAMES` of them, so the measurement has to straddle that
/// boundary. Coming back from **one** idle frame rebuilds only the per-backdrop
/// uniforms and bind groups; coming back from **two** rebuilds those and the
/// frame-wide targets underneath them. So the claim is that the first is
/// strictly cheaper than the second, which is false both when the grace is
/// removed — the two become equal — and when it is measured wrong.
///
/// Stated as an inequality rather than as two counts, because the counts are
/// `BlurTargets::prepare`'s own inventory and a test restating it would be the
/// second copy that record exists to avoid.
///
/// Every scene is drawn before anything is measured, for the reason its siblings
/// give: a cold frame's own buffers growing satisfies "allocated more" whatever
/// the blur targets do.
#[test]
fn an_alternating_refusal_does_not_rebuild_the_frame_wide_targets() {
    let mut images = ImageTable::new();
    let field = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });
    let extent = images.resolve(field);
    assert!(
        extent.width != 0 && extent.height != 0,
        "the fixture must have an extent on both axes, or `resident_image` answers with the \
         no-bytes case before it ever reaches a decoder and nothing is refused — got {} x {}",
        extent.width,
        extent.height,
    );

    // The live backdrop carries a **real coverage mask**, unlike the one in
    // `a_frame_whose_only_backdrop_is_refused_allocates_nothing`. That test
    // keeps the JPEG as the only payload so residency is identical across its
    // scenes; here the atlas is the point. `BlurTargets::bound_atlases` would
    // hold `[None]` on every frame of an unmasked fixture, so the interaction
    // this test is about — a bind group naming a real atlas view dropped on the
    // idle frame and rebuilt on return — would never be driven, and a change
    // that held the per-backdrop half across the grace would keep it green.
    //
    // Residency's own allocation is kept out of the measurement by drawing both
    // scenes in the warm-up below, which is what every allocation test in this
    // file does for `Frame`'s buffers.
    let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);

    // Two scenes over one image table.
    let refused = {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = paints.push_with(
            PaintEntry::default(),
            EntryParts {
                blurs: &[backdrop(24.0)],
                shape: Some(VectorField {
                    image: field,
                    atlas_rect: MASK_RECT,
                    plane_bounds: [0.0, 0.0, 20.0, 32.0],
                    distance_range: 0.5,
                }),
                ..EntryParts::default()
            },
        );
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (rects, paints)
    };
    let live = {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = paints.push_with(
            PaintEntry::default(),
            EntryParts {
                blurs: &[backdrop(24.0)],
                shape: Some(VectorField {
                    image: atlas,
                    atlas_rect: MASK_RECT,
                    plane_bounds: [0.0, 0.0, 20.0, 32.0],
                    distance_range: 0.5,
                }),
                ..EntryParts::default()
            },
        );
        rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
        (rects, paints)
    };

    let clips = ClipTable::new();
    let mut renderer = renderer();
    let mut draw_once = |rects: &[RectEntry], paints: &PaintTable| {
        let mut painter = GpuPainter::new();
        painter.paint(
            rects,
            paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        // The refusals with the count, because `resolve_frame` clears them at
        // the head of every frame: read after a later draw they would be that
        // frame's. The sibling test says the same.
        (renderer.allocations(), renderer.refusals().len())
    };

    // Warm both, and leave the renderer on a frame that drew. This is also what
    // takes the mask atlas out of the measurement: it is made resident on the
    // first live frame and stays touched by every later one.
    draw_once(&live.0, &live.1);
    draw_once(&refused.0, &refused.1);
    draw_once(&live.0, &live.1);
    draw_once(&refused.0, &refused.1);
    let (warm, live_refusals) = draw_once(&live.0, &live.1);
    assert_eq!(
        live_refusals, 0,
        "the live scene's mask must be resident, or both scenes are refused and the alternation \
         this measures never happens",
    );

    // One refused frame, then back. Inside the grace period.
    let (idle_once, refusals) = draw_once(&refused.0, &refused.1);
    assert_eq!(
        refusals, 1,
        "the refused scene must actually be refused, or it is a second live frame",
    );
    assert_eq!(
        idle_once, warm,
        "a refused frame allocates nothing itself — {warm} then {idle_once}",
    );
    let after_one = draw_once(&live.0, &live.1).0 - idle_once;

    // Two refused frames, then back. The second crosses the grace period, so
    // this rebuild includes the frame-wide targets the one above reused.
    let (before_two, _) = draw_once(&refused.0, &refused.1);
    let (released, _) = draw_once(&refused.0, &refused.1);
    assert_eq!(
        released, before_two,
        "releasing the targets frees rather than allocates — {before_two} then {released}",
    );
    let after_two = draw_once(&live.0, &live.1).0 - released;

    // The two halves, and each inequality pins one of them.
    //
    // **The frame-wide half is held**: coming back from one refused frame costs
    // less than coming back from two. Equal means it was released on the first
    // refused frame after all, which is the rebuild-on-every-change issue #1020
    // is about — and `FrameExceedsAtlas` can alternate on every frame
    // indefinitely.
    assert!(
        after_one < after_two,
        "coming back from one refused frame must cost less than coming back from two: {after_one} \
         against {after_two}.\n\n\
         **This test assumes `TARGET_GRACE_FRAMES` is 1**, which it cannot read — the \
         constant is private to the crate. If it was raised, both returns above are inside the \
         grace, both cost the same, and this fails saying nothing about the fix: add refused \
         frames to the second run until it straddles the new boundary. If it was removed, both \
         cost twelve, and that is the defect issue #1020 is about",
    );
    // **The per-backdrop half is not.** Each of those bind groups names this
    // scene's coverage atlas view, and the refused frame named no mask at all,
    // so they go on it and the return frame rebuilds them. Zero here means they
    // were held across a frame that named nothing they could be built from —
    // which is what keeps the grace clear of issue #1050, and which the
    // inequality above would accept.
    assert!(
        after_one > 0,
        "returning from a refused frame must rebuild the per-backdrop uniforms and bind groups: \
         {after_one}",
    );
}

/// **A frame whose backdrop is refused still clears its target** (issue #994).
///
/// A pass that resolves a backdrop clears the target itself, before the blur
/// snapshots it, and its render pass then loads — clearing twice would erase
/// the frosted region the pass exists to draw over. Filtering the refused
/// backdrop out also removed that clear, so the condition the render pass
/// reads had to become *what was encoded* rather than *what was planned*. Read
/// from the plan, a refused backdrop leaves the first pass on a target loading
/// a texture nothing cleared.
///
/// **The two frames are what makes it visible.** `Renderer::render` keeps its
/// offscreen texture across calls at one extent, so the second frame draws into
/// the first frame's pixels — and every other refusal test in this file paints
/// an opaque rect over the whole canvas, which hides a missing clear completely.
/// Here the second frame's only node has no fill of its own and its backdrop is
/// refused, so a correct painter writes nothing at all and the canvas must come
/// back transparent. Loading instead returns the first frame's red and blue.
#[test]
fn a_refused_backdrop_does_not_leave_the_previous_frame_on_the_target() {
    let mut images = ImageTable::new();
    let field = images.push(ImageAsset {
        format: ImageFormat::Jpeg,
        bytes: JPEG_FIXTURE.to_vec(),
    });
    // Both axes, not the pair, for the reason its three siblings give.
    let extent = images.resolve(field);
    assert!(
        extent.width != 0 && extent.height != 0,
        "the fixture must have an extent on both axes, or `resident_image` answers with the \
         no-bytes case before it ever reaches a decoder and nothing is refused — got {} x {}",
        extent.width,
        extent.height,
    );

    let clips = ClipTable::new();
    let mut renderer = renderer();
    let mut draw_once = |rects: &[RectEntry], paints: &PaintTable| {
        let mut painter = GpuPainter::new();
        painter.paint(
            rects,
            paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        let pixels = renderer
            .render(
                painter.instances(),
                paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        (pixels, renderer.allocations())
    };

    // The first frame fills the target, so anything left of it in the second is
    // the load this test is about.
    let mut ground = PaintTable::new();
    let (filled, after_first) = draw_once(&halves(&mut ground), &ground);
    assert_eq!(
        texel(&filled, 8, 24),
        [255, 0, 0, 255],
        "the first frame paints the whole target, or the second cannot inherit anything",
    );

    // The second draws one node: no fill of its own, and a backdrop confined to
    // a field that cannot be decoded. Nothing to draw, on a target that must
    // still be cleared.
    let mut paints = PaintTable::new();
    let panel = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: field,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    let (pixels, after_second) = draw_once(&[rect(24.0, 8.0, 36.0, 32.0, panel)], &paints);

    // **The premise, asserted rather than assumed.** This test can only see a
    // missing clear because the second frame draws into the texture the first
    // one left; `render_dirty` reuses its offscreen at one extent and charges
    // three allocations when it does not. wgpu zero-initialises a new texture,
    // so a rebuild here would make the sweep below pass against a target
    // nothing had to clear — the test would keep passing and stop testing.
    assert_eq!(
        after_second, after_first,
        "the second frame must draw into the first frame's texture: {after_first} then \
         {after_second} means `render` built a fresh offscreen, which arrives zeroed and makes \
         the sweep below unable to fail",
    );

    for y in 0..H {
        for x in 0..W {
            assert_eq!(
                texel(&pixels, x, y),
                [0, 0, 0, 0],
                "({x}, {y}) carries the previous frame: the pass loaded a target the refused \
                 backdrop no longer clears",
            );
        }
    }
}

/// A blur of no radius draws nothing, and `pack::frosts` is what makes that
/// true rather than the shader.
///
/// Not merely a waste to emit: the resolve pass composites its copy over the
/// original below full opacity, and a copy of the original composited over
/// itself is darker than the original. The reference painter's
/// `backdrop_blur_filter` returns no filter at all for a non-positive radius,
/// for the same reason.
#[test]
fn a_backdrop_of_no_radius_leaves_the_target_untouched() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = frosted(&mut paints, &[backdrop(0.0)], CornerRadii::default());
    let mut dim = rect(8.0, 8.0, 48.0, 32.0, panel);
    // Below one, so a resolve pass that ran would composite rather than replace
    // and would darken every texel it covered.
    dim.opacity = 0.5;
    rects.push(dim);

    let pixels = draw(&rects, &paints);
    assert_eq!(
        texel(&pixels, SEAM - 1, 24),
        [255, 0, 0, 255],
        "a zero-radius backdrop leaves the red half exactly as it was",
    );
    assert_eq!(
        texel(&pixels, SEAM, 24),
        [0, 0, 255, 255],
        "and the blue half with it",
    );
}

/// A frame that holds a backdrop allocates the targets it needs, and a second
/// identical frame allocates nothing.
///
/// **The first half is what makes `Renderer::allocations`'s new term
/// falsifiable at all.** This crate has had the same defect three times — the
/// residency textures went uncounted in story #581, the layer targets in #583,
/// and the blur targets in this one, each caught by review rather than by a
/// test. A term nothing makes non-zero cannot fail, so the assertion is a
/// *difference* between two scenes rather than an absolute number: one with a
/// backdrop, one identical but for the blur.
///
/// The second half is the steady-state claim R-T4 rests on: the targets are
/// held across frames, so drawing the same scene again reallocates nothing.
#[test]
fn a_frame_with_a_backdrop_allocates_and_a_steady_one_does_not() {
    let scene = |radius: f32| {
        let mut paints = PaintTable::new();
        let mut rects = halves(&mut paints);
        let panel = frosted(&mut paints, &[backdrop(radius)], CornerRadii::default());
        rects.push(rect(8.0, 8.0, 48.0, 32.0, panel));
        (rects, paints)
    };
    let clips = ClipTable::new();
    let mut renderer = renderer();
    let mut draw_once = |rects: &[RectEntry], paints: &PaintTable| {
        let mut painter = GpuPainter::new();
        painter.paint(
            rects,
            paints,
            &ImageTable::new(),
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                paints,
                &ImageTable::new(),
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("renders");
        renderer.allocations()
    };

    // A radius of zero emits no backdrop instance at all (`pack::frosts`), so
    // these two scenes differ in exactly one thing: whether the frame holds a
    // backdrop. Everything else — the rects, the paints, the extent — is equal.
    let (plain_rects, plain_paints) = scene(0.0);
    let (frost_rects, frost_paints) = scene(8.0);

    // Warm everything else up first, so that no later step is the first to
    // allocate a frame buffer, an atlas or an offscreen target. **This is what
    // makes the measurement blur-only.** Differencing a cold plain frame
    // against a cold frosted one does not: the two scenes hold different
    // instance counts, so `Frame`'s own buffers grow and rebind between them,
    // and that difference alone satisfies "allocated more" whether or not the
    // blur targets are counted at all. Deleting the `blurs` term survived
    // exactly that shape of test.
    draw_once(&frost_rects, &frost_paints);
    draw_once(&plain_rects, &plain_paints);
    draw_once(&frost_rects, &frost_paints);

    let steady = draw_once(&frost_rects, &frost_paints);
    assert_eq!(
        steady,
        draw_once(&frost_rects, &frost_paints),
        "a warm frame that holds a backdrop reallocates nothing",
    );

    // A frame with no backdrop releases the per-backdrop uniforms and bind
    // groups, and the next frosted frame has to build them again. **Not the
    // three drawable-sized targets**, which survive `TARGET_GRACE_FRAMES`
    // of such frames since issue #1020 — one plain frame is inside that, so
    // what `rebuilt` counts here is four objects and not twelve.
    // `an_alternating_refusal_does_not_rebuild_the_frame_wide_targets` is what
    // straddles the boundary; this test only needs the delta to be non-zero.
    // Both scenes have been drawn already, so every other allocator is warm and
    // this delta is the blur targets and nothing else.
    let released = draw_once(&plain_rects, &plain_paints);
    assert_eq!(
        released, steady,
        "dropping the backdrop frees targets rather than allocating any",
    );
    let rebuilt = draw_once(&frost_rects, &frost_paints);
    assert!(
        rebuilt > released,
        "a backdrop returning rebuilds the targets released above: {released} then {rebuilt} \
         — equal means `Renderer::allocations` does not count `BlurTargets`, which is the term \
         this test exists to make falsifiable",
    );
}

/// A backdrop is clipped exactly as the node's own fill is.
///
/// The clip cuts the panel in half down the seam, so the left half frosts and
/// the right half does not. Both probes are inside the panel and inside its
/// shape, so the clip is the only thing that separates them.
#[test]
fn a_backdrop_is_confined_to_the_clip_region_its_node_names() {
    let mut paints = PaintTable::new();
    let mut clips = ClipTable::new();
    let region = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: SEAM as f32,
        h: H as f32,
        corners: CornerRadii::default(),
    }]);
    let mut rects = halves(&mut paints);
    let panel = frosted(&mut paints, &[backdrop(8.0)], CornerRadii::default());
    let mut clipped = rect(8.0, 8.0, 48.0, 32.0, panel);
    clipped.clip = region;
    rects.push(clipped);

    let pixels = draw_groups(&rects, &paints, &clips, &[]);
    let inside_clip = texel(&pixels, SEAM - 1, 24);
    let outside_clip = texel(&pixels, SEAM + 1, 24);
    assert!(
        inside_clip[2] > 40,
        "left of the seam is inside the clip and frosts, got {inside_clip:?}",
    );
    assert_eq!(
        outside_clip,
        [0, 0, 255, 255],
        "right of the seam is outside the clip and keeps its untouched blue — got \
         {outside_clip:?}",
    );
}

/// A rotated frosted panel frosts a rotated region (story #832).
///
/// The backdrop blur is a **separate pipeline** from `paint.wgsl` — its own
/// vertex stage, its own quad, its own resolve — so the rotation the packer
/// stamps onto the backdrop instance reaches it only if that pipeline reads it.
/// Until story #832 it did not, and a rotated frosted panel rendered
/// byte-identically to an upright one while the reference painter turned it:
/// `dashscene-skia` calls `draw_backdrop_blur_box` inside the canvas rotation
/// it opens for the node.
///
/// That is the silent wrong picture `Painter::rotates` exists to prevent, so it
/// has to be false or fixed — this asserts it is fixed.
///
/// The panel is deliberately not square. A square frosted region turned about
/// its own centre covers a different area, but a *rotationally symmetric* one
/// would not, and this test would then pass against a pipeline that ignored the
/// term.
#[test]
fn a_rotated_backdrop_frosts_a_rotated_region() {
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = frosted(&mut paints, &[backdrop(6.0)], CornerRadii::default());

    // Straddling the seam, so the frosted region contains the hard red/blue
    // edge and turning it moves that edge's blurred image.
    let bar = |rotation: f32| RectEntry {
        x: 16.0,
        y: 16.0,
        w: 32.0,
        h: 10.0,
        paint: panel,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation,
        rotation_anchor: Vec2 { x: 16.0, y: 5.0 },
    };

    rects.push(bar(0.0));
    let upright = draw(&rects, &paints);

    rects.pop();
    rects.push(bar(std::f32::consts::FRAC_PI_4));
    let turned = draw(&rects, &paints);

    let differing = upright
        .chunks_exact(4)
        .zip(turned.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > 100,
        "turning a frosted panel a quarter turn changed only {differing} \
         pixels: the backdrop-blur pipeline is drawing the frosted region \
         unrotated while the node is rotated",
    );
}

// ---------------------------------------------------------------------------
// A replaced document does not leave a blur bind group naming the old atlas
// (issue #1050)
// ---------------------------------------------------------------------------

/// A field atlas one texel wider than `dashscene_gpu::ATLAS_EXTENT`, so
/// residency gives it a texture of its own rather than a rectangle in the shared
/// atlas (issue #720).
///
/// Written in full rather than as an intra-doc link: this is an integration test
/// crate, the constant is in scope only through its path, and neither gate would
/// catch the broken reference — `cargo doc` does not build test targets and
/// clippy does not resolve intra-doc links.
///
/// The width is what makes it dedicated; the height stays at [`MASK_ATLAS`], so
/// the payload is about 64 kB rather than the 16.8 MB a square one would be
/// (2049 x 2049 x 4 bytes). `inside` selects which columns of [`MASK_RECT`] are
/// inside the field, so two documents can carry visibly different masks at the
/// same extent.
fn wide_mask_field(inside: u32) -> ImageAsset {
    let [rx, ry, rw, rh] = MASK_RECT;
    let width = dashscene_gpu::ATLAS_EXTENT + 1;
    let mut bytes = Vec::with_capacity((width * MASK_ATLAS * 4) as usize);
    for y in 0..MASK_ATLAS {
        for x in 0..width {
            let inside_rect = x >= rx && x < rx + rw && y >= ry && y < ry + rh;
            let v = match x.checked_sub(rx) {
                Some(column) if inside_rect && column < inside => 255,
                _ => 0,
            };
            bytes.extend_from_slice(&[v, v, v, 255]);
        }
    }
    ImageAsset {
        format: ImageFormat::Rgba8Unorm,
        bytes,
    }
}

/// A scene of the two halves under a frosted panel masked by a dedicated field
/// atlas whose sub-rect is inside on its first `inside` columns.
fn wide_masked_backdrop(inside: u32) -> (Vec<RectEntry>, PaintTable, ImageTable) {
    let mut images = ImageTable::new();
    let atlas = images.push_baked(
        wide_mask_field(inside),
        dashscene_gpu::ATLAS_EXTENT + 1,
        MASK_ATLAS,
    );
    let mut paints = PaintTable::new();
    let mut rects = halves(&mut paints);
    let panel = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            blurs: &[backdrop(24.0)],
            shape: Some(VectorField {
                image: atlas,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    rects.push(rect(24.0, 8.0, 36.0, 32.0, panel));
    (rects, paints, images)
}

/// **Replacing the document must not leave a blur bind group naming the atlas
/// the old one used** (issue #1050).
///
/// `BlurTargets::bound_atlases` records an atlas **index** per drawn backdrop,
/// and that list is the whole key deciding whether the bind groups are rebuilt.
/// An index is meaningful only within one residency set:
/// `Residency::forget_resident` drops every dedicated texture, so the indices
/// after a dropped one shift down and a reused index can name a different
/// texture entirely.
///
/// # The arrangement, which is not the one the issue describes
///
/// That issue reasoned from a mask sitting in atlas 1 above a *dedicated* atlas
/// 0, replaced by a document whose mask lands in atlas 1 again. That shape is
/// not reachable: `atlas_for` keeps exactly one shared atlas per format and
/// returns the first, `forget_resident` retains it, and `push_atlas` appends —
/// so a retained shared atlas only ever moves **down**, and the mask that
/// follows it lands at a lower index than before, which the list comparison
/// sees.
///
/// What is reachable is simpler. Here the mask's own atlas is the dedicated one
/// and it is the only atlas in the set, so it is index 0 in both documents:
/// the old texture is dropped, the new one is created, and both frames record
/// `[Some(0)]`. Equal lists, different textures, and nothing rebuilt.
///
/// # Why the comparison is against a second renderer
///
/// A stale bind group is not visible in any counter — the groups exist either
/// way — so the observable is the picture, and the baseline has to be the same
/// document drawn by a renderer that never saw the first one. Comparing against
/// a constant would pin this fixture's blur rather than the property.
///
/// The two masks differ in how many columns of `MASK_RECT` are inside, so the
/// frosted region has a different width in each, and a blur reading the old
/// atlas frosts the old width.
#[test]
fn replacing_the_document_rebuilds_the_blur_bindings() {
    let clips = ClipTable::new();
    let draw = |renderer: &mut dashscene_gpu::Renderer,
                (rects, paints, images): &(Vec<RectEntry>, PaintTable, ImageTable)| {
        let mut painter = GpuPainter::new();
        painter.paint(
            rects,
            paints,
            images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                paints,
                images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum")
    };

    let first = wide_masked_backdrop(1);
    let second = wide_masked_backdrop(4);

    let mut reused = renderer();
    let before = draw(&mut reused, &first);
    let allocated_first = reused.allocations();
    reused.forget_uploaded();
    let after_replacement = draw(&mut reused, &second);

    // **The premise this test rests on, and it is not the fixture's width.**
    // What matters is that the field's atlas is *dedicated*, so that
    // `forget_resident` drops it and the replacement re-creates one at the same
    // index. A payload that fits the shared atlas is retained across the call,
    // the texture is then the same object either side of it, and the comparison
    // below would pass for a reason unrelated to this issue — which is what
    // raising `ATLAS_EXTENT`, or changing `usable_extent`'s rounding, would do
    // silently.
    //
    // The observable is the allocation counter: a dedicated texture and its view
    // are two allocations, and the two documents are identical in shape, so no
    // buffer grows between them and nothing else here allocates. Equal counts
    // mean the payload was retained, and then no index ever named two textures.
    assert!(
        reused.allocations() > allocated_first,
        "the replacement must build a new dedicated texture: {allocated_first} allocations \
         before and {} after means the payload landed in the shared atlas and was retained, so \
         the reused index this test is about never existed",
        reused.allocations(),
    );

    let mut clean = renderer();
    let expected = draw(&mut clean, &second);

    // **The third premise: the two documents draw differently**, or the
    // comparison below cannot fail however stale the binding is.
    assert_ne!(
        before, expected,
        "the two fixtures must frost different regions, or a bind group naming the first \
         document's atlas would draw the second document's picture anyway",
    );
    // Compared by first differing texel rather than by whole buffer: each frame
    // is W x H x 4 bytes, and a bare `assert_eq!` on two of them prints about a
    // hundred kilobytes of integers and names no pixel.
    let differs = after_replacement
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .position(|(a, b)| a != b);
    assert_eq!(
        differs.map(|i| (i as u32 % W, i as u32 / W)),
        None,
        "after `forget_uploaded` the frame must draw the new document, not blur through the \
         coverage atlas the old one left at the same index — first differing texel above",
    );
}

/// **A field that draws nothing makes no atlas resident** (issue #1159).
///
/// `VectorField::draws`' own doc states asking before fetching the atlas as
/// part of the predicate's contract: a field it rejects samples nothing, so
/// making its atlas resident is pure waste. `dashscene-skia`'s order is pinned
/// by its own decode counter (issue #1044); this is the same property one
/// painter over.
///
/// # Why nothing pinned it before
///
/// `resolve_frame` reads `field_draws(field) && let Some(slot) =
/// self.resident_image(..)`, and swapping those two operands left the whole
/// crate suite green — measured, 221 tests. The sibling
/// `a_degenerate_coverage_field_draws_nothing` cannot see it: with the order
/// flipped a degenerate field's atlas *is* made resident, but the payload is
/// sound so residency records no refusal, and the row still ends unresolved
/// because the predicate still gates the assignment. Every observable that test
/// has is blind to the order.
///
/// # The instrument, and why it is not the decode counter (issue #1165)
///
/// `Renderer::admissions` counts payloads **admitted to residency**, which is
/// the property this test is named for. `Renderer::decodes` is not, on two
/// counts that both apply on this path. It moves only for an *encoded* payload,
/// and the coverage fixture these tests draw is baked — so the first version had
/// to carry a PNG no other coverage fixture uses, leaving the payload kind they
/// actually draw masks with unpinned. And it is incremented *before* `allocate`
/// runs, so a payload that decoded and was then refused increments it while
/// nothing became resident.
///
/// Both are asserted below, and they answer different questions: the baked
/// fixture pins the fetch for the payload kind this file uses, and the decode
/// count staying at zero throughout says the baked path never took the encoded
/// one.
///
/// # The order of the cases is load-bearing
///
/// The rejected cases run **before** the sound one, and the **first** of them is
/// the one that discriminates: it runs on a renderer that has fetched nothing,
/// so a fetch would happen if the gate were removed. The second is weaker by
/// construction — both build the same `ImageTable`, so they share a
/// `PayloadKey`, and with the gate removed the first iteration admits the
/// payload and the second is a cache hit reading zero. It is kept because it
/// covers the predicate's other half, not because it could catch the ordering
/// on its own. Running either after the sound case would make it a cache hit
/// too.
///
/// The sound case is the positive control, and it is what says this path fetches
/// at all: a test asserting only zero passes against a painter that stopped
/// resolving fields entirely, which is the hole review found in the skia twin.
#[test]
fn a_field_that_draws_nothing_makes_no_atlas_resident() {
    let clips = ClipTable::new();
    let mut renderer = renderer();
    let mut draw_field = |field: VectorField| {
        let mut images = ImageTable::new();
        let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);
        let (rects, paints) = masked_frosted_scene(atlas, field);

        let before = (renderer.admissions(), renderer.decodes());
        let mut painter = GpuPainter::new();
        painter.paint(
            &rects,
            &paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                &paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        (
            renderer.admissions() - before.0,
            renderer.decodes() - before.1,
            renderer.last_draw_runs(),
        )
    };

    // The two rejected cases are **derived from** the sound one, each changing
    // one member, so "differs in the predicate's answer and in nothing else" is
    // structural rather than a claim in a comment.
    let sound = VectorField {
        image: 0,
        atlas_rect: MASK_RECT,
        plane_bounds: [0.0, 0.0, 20.0, 32.0],
        distance_range: 0.5,
    };
    let no_texels = VectorField {
        atlas_rect: [MASK_RECT[0], MASK_RECT[1], 0, 0],
        ..sound
    };
    let no_quad = VectorField {
        plane_bounds: [0.0, 0.0, 0.0, 0.0],
        ..sound
    };
    // The premise, so that a later change to the predicate fails here naming
    // itself rather than accusing `resolve_frame`'s operand order.
    assert!(
        sound.draws() && !no_texels.draws() && !no_quad.draws(),
        "the three fixtures must differ in exactly this predicate's answer",
    );

    for (what, field) in [("no atlas rectangle", no_texels), ("no quad", no_quad)] {
        let (admissions, decodes, _) = draw_field(field);
        assert_eq!(
            admissions, 0,
            "a field with {what} must admit nothing: `field_draws` is asked before \
             `resident_image`, so a fetch here is the payload being made available for a field \
             that samples none of it",
        );
        assert_eq!(
            decodes, 0,
            "and it must decode nothing either — the fixture is baked, so this stays zero \
             whatever the gate does, and a non-zero reading means the fixture stopped being the \
             payload kind this file draws masks with",
        );
    }

    // The positive control. `admissions` is incremented after `allocate` and
    // `upload` have both succeeded, so the count below already excludes a
    // payload that decoded and was then refused — that was the weakness of the
    // decode counter this replaced, not of this one. The refusal list and the
    // draw count are kept for a different reason: they say the row the frame
    // resolved is the one the picture is drawn from, so an admission that
    // resolved nothing would still fail here.
    let (admissions, decodes, runs) = draw_field(sound);
    assert_eq!(
        admissions, 1,
        "a sound field must make its atlas resident exactly once, or the zeros above hold \
         against a painter that resolves no field at all rather than against one that asks first",
    );
    assert_eq!(
        decodes, 0,
        "a baked payload is never decoded, so this counter cannot see the fetch above — which is \
         why `admissions` is the instrument and issue #1165 added it",
    );
    assert!(
        renderer.refusals().is_empty(),
        "the sound field's payload must have been made resident, not fetched and then refused — \
         got {:?}",
        renderer.refusals(),
    );
    assert_eq!(
        runs, 5,
        "a sound field resolves, so the frame plans the backdrop and draws the masked fill; one \
         draw would mean the fetch above bought nothing",
    );
}

/// **A payload is admitted once, however many frames draw it** (issue #1165).
///
/// `Residency::admissions` counts the insertion, so a residency **cache hit**
/// must not move it — that is the property separating an admission counter from
/// a call counter, and the reason the accessor's doc can say "steady state is
/// zero growth".
///
/// Nothing else reaches it. `resolve_frame` memoises per row through
/// `out.atlas_of_shape[row].is_none()`, so
/// `a_field_that_draws_nothing_makes_no_atlas_resident` calls
/// `Residency::resident` exactly once per payload and never takes the cache-hit
/// path at all: moving the increment above that early return leaves it green.
/// `Renderer::decodes` has this test one file over — a resident PNG decoded once
/// across five frames — and the new counter arrived without the equivalent.
///
/// The same scene four times, so every frame after the first is a hit on both
/// the frame's own memo and residency's.
#[test]
fn an_admitted_payload_is_counted_once_however_many_frames_draw_it() {
    let clips = ClipTable::new();
    let mut renderer = renderer();
    let mut images = ImageTable::new();
    let atlas = images.push_baked(mask_field(), MASK_ATLAS, MASK_ATLAS);
    let (rects, paints) = masked_frosted_scene(
        atlas,
        VectorField {
            image: 0,
            atlas_rect: MASK_RECT,
            plane_bounds: [0.0, 0.0, 20.0, 32.0],
            distance_range: 0.5,
        },
    );

    let mut draw = || {
        let mut painter = GpuPainter::new();
        painter.paint(
            &rects,
            &paints,
            &images,
            &clips,
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                &paints,
                &images,
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        renderer.admissions()
    };

    // The premise: this field draws, so the payload is admitted at all. A field
    // that drew nothing would satisfy every assertion below at zero.
    assert_eq!(
        draw(),
        1,
        "the first frame must admit the coverage atlas, or the equality below holds against a \
         scene that never fetched anything",
    );
    for frame in 2..=4 {
        assert_eq!(
            draw(),
            1,
            "frame {frame} re-admitted a payload residency already held: a cache hit returns \
             before the increment, and a counter that moved here would be counting calls rather \
             than admissions",
        );
    }
}
