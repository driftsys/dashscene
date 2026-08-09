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
use dashscene_gpu::{GpuPainter, Renderer};

const W: u32 = 64;
const H: u32 = 48;

/// Where the red half ends and the blue half begins.
const SEAM: u32 = 32;

fn renderer() -> Renderer {
    Renderer::new().expect("layer 3 needs a device")
}

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

/// The unpremultiplied RGBA texel at (x, y).
fn texel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
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

    // A frame with no backdrop releases the three drawable-sized targets, and
    // the next frosted frame has to build them again. Both scenes have been
    // drawn already, so every other allocator is warm and this delta is the
    // blur targets and nothing else.
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
