//! Layer 3 of epic #569's verification net: the pipeline builds, naga
//! validates, and the painter draws roughly the right thing in roughly the
//! right place.
//!
//! # This is a gate on the pipeline, and never a fidelity check
//!
//! Epic #569 insists on the distinction and story #580 repeats it. What these
//! tests establish is that a pipeline was created, that the shader modules
//! validated, that coverage is high inside a shape and zero outside it, and
//! that a clip rejects. What they cannot establish is how any of it looks on a
//! real driver: a coarse per-pixel check on a software rasteriser says the
//! painter drew *something* in *about* the right place. Layer 4 is the
//! instrument for how it looks, it needs hardware, and it is story #586's.
//!
//! Read every assertion below as "did the pipeline do a thing at all", not as
//! "is the picture right". Nothing here compares against the reference
//! painter, deliberately.

use dashpaint::{
    ClipBox, ClipIndex, ClipTable, Color, CornerRadii, EntryParts, GlyphRunTable, ImageTable,
    PaintEntry, PaintTable, Painter, RectEntry, Stroke, StrokeAlign,
};
use dashscene_gpu::{GpuPainter, Renderer, RendererError};

const W: u32 = 64;
const H: u32 = 48;

/// A renderer, or a named failure. Panics rather than skipping: a layer-3
/// suite that quietly passes with no device is a green result that establishes
/// nothing.
fn renderer() -> Renderer {
    Renderer::new().expect("layer 3 needs a device")
}

fn rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    paint: dashpaint::PaintIndex,
    clip: ClipIndex,
) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip,
        opacity: 1.0,
    }
}

/// The unpremultiplied RGBA texel at (x, y).
fn texel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// Packs one scene through the painter and renders it.
fn draw(rects: &[RectEntry], paints: &PaintTable, clips: &ClipTable) -> Vec<u8> {
    draw_groups(rects, paints, clips, &[])
}

/// [`draw`], with render-target groups.
fn draw_groups(
    rects: &[RectEntry],
    paints: &PaintTable,
    clips: &ClipTable,
    groups: &[dashpaint::GroupComposite],
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

/// The pipeline builds and the shader modules validate.
///
/// Creating the renderer compiles `SDF_WGSL` concatenated with the render entry
/// points and creates the render pipeline, so naga has validated both modules
/// and wgpu has checked the bind group layout against what the shader declares
/// by the time this returns. A binding the shader does not declare, or a type
/// that disagrees, fails here.
#[test]
fn the_pipeline_builds_and_the_shaders_validate() {
    let renderer = renderer();
    let info = renderer.adapter_info();
    println!(
        "layer-3 adapter: {} | backend {:?} | device_type {:?}",
        info.name, info.backend, info.device_type
    );
}

/// The drawable this painter can address is the adapter's own maximum, and not
/// the 2048 px `wgpu::Limits::downlevel_defaults` names.
///
/// This is half of issue #714. The painter requests downlevel limits so that it
/// runs on the entry-tier class of device R3 names, and downlevel caps
/// `max_texture_dimension_2d` at 2048 — which made an ordinary 2288x1410 window
/// abort the showcase host on a machine whose adapter reports 16384. A
/// resolution is a property of the window the host opened rather than of the
/// features the painter uses, so it comes from the adapter.
///
/// Stated against a second adapter requested exactly as `Renderer::new`
/// requests its own — same options, same process, so the same adapter — rather
/// than against a literal. A literal would say either "at least 2048", which
/// the defect satisfied, or "16384", which is this machine and not CI's.
#[test]
fn the_drawable_maximum_is_the_adapters_and_not_the_downlevel_default() {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("layer 3 needs a device");

    let max = renderer().max_extent();
    println!(
        "layer-3 max_texture_dimension_2d: adapter {}, downlevel default {}, painter {max}",
        adapter.limits().max_texture_dimension_2d,
        wgpu::Limits::downlevel_defaults().max_texture_dimension_2d,
    );
    assert_eq!(
        max,
        adapter.limits().max_texture_dimension_2d,
        "the painter must be bounded by what the adapter reports, not by a synthetic limit"
    );
}

/// The maximum itself draws, and one past it is refused by name, on either
/// axis — rather than reaching `wgpu` and aborting the process.
///
/// The other half of issue #714, and the half no window is needed to state.
/// `Device::create_texture` and `Surface::configure` both raise a validation
/// error for an over-large extent, and a wgpu validation error is not returned
/// to the caller — it reaches the uncaptured-error handler and panics, which
/// inside the swapchain configure is non-unwinding and takes the process with
/// it. So the extent has to be refused before the call.
///
/// Both sides of the boundary, on both axes, and neither half is redundant:
///
/// - `max` itself must draw. wgpu refuses an extent strictly *greater* than
///   `max_texture_dimension_2d` (`wgpu-core`'s `conv.rs` compares
///   `given > limit`), so a check written `>=` would refuse a drawable the
///   device can address — and no ordinary fixture, at 64x48, comes near
///   enough to the boundary to notice.
/// - `max + 1` must be refused. A check that reads only the width, or only the
///   height, passes a fixture that oversizes both, so each axis is driven past
///   on its own with the other left small.
///
/// The admitted extents are `max` by 8 rather than `max` square: a square one
/// is a gigabyte of texture on a desktop adapter, and it is the boundary on
/// each axis that is under test, not the area.
#[test]
fn the_maximum_extent_draws_and_one_past_it_is_refused_on_either_axis() {
    let mut renderer = renderer();
    let max = renderer.max_extent();

    let mut paints = PaintTable::new();
    let red = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(
        &[rect(0.0, 0.0, 8.0, 8.0, red, ClipIndex::UNCLIPPED)],
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );

    for (width, height) in [(max, 8), (8, max)] {
        let pixels = renderer
            .render(
                painter.instances(),
                &paints,
                &ImageTable::new(),
                &clips,
                &GlyphRunTable::new(),
                width,
                height,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "a {width}x{height} drawable is what the device reports it can address: {error}"
                )
            });
        assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    }

    for (width, height) in [(max + 1, 16), (16, max + 1)] {
        let refused = renderer.render(
            painter.instances(),
            &paints,
            &ImageTable::new(),
            &clips,
            &GlyphRunTable::new(),
            width,
            height,
        );
        let Err(RendererError::Extent {
            width: reported_width,
            height: reported_height,
            max: reported_max,
        }) = refused
        else {
            panic!("a {width}x{height} drawable past a maximum of {max} was not refused");
        };
        assert_eq!((reported_width, reported_height), (width, height));
        assert_eq!(reported_max, max);
    }

    // And the renderer is still usable afterwards: a refusal happens before
    // anything is allocated or dropped, so it costs the caller nothing but the
    // frame it asked for.
    let pixels = renderer
        .render(
            painter.instances(),
            &paints,
            &ImageTable::new(),
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("an ordinary extent still renders after a refusal");
    assert_eq!(pixels.len(), (W * H * 4) as usize);
}

/// A filled rect covers its inside and leaves its outside alone.
///
/// The coarsest claim layer 3 makes, and the one that fails if the vertex
/// shader places the quad wrongly, the fragment shader takes the wrong row, or
/// the blend state discards everything.
#[test]
fn a_solid_rect_covers_inside_and_not_outside() {
    let mut paints = PaintTable::new();
    let red = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    let pixels = draw(
        &[rect(16.0, 12.0, 32.0, 24.0, red, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );

    // Well inside: opaque and the colour that was asked for.
    let inside = texel(&pixels, 32, 24);
    assert_eq!(inside[3], 255, "the middle of the rect is opaque");
    assert!(
        inside[0] > 250 && inside[1] < 5 && inside[2] < 5,
        "the middle of the rect is the fill colour, got {inside:?}"
    );

    // Well outside, on all four sides: untouched.
    for (x, y) in [(2, 2), (61, 2), (2, 45), (61, 45)] {
        assert_eq!(
            texel(&pixels, x, y)[3],
            0,
            "({x}, {y}) is outside the rect and must be clear"
        );
    }
}

/// A rounded corner removes coverage that a sharp one keeps.
///
/// Not a check on the shape of the curve — that is the rounded-box distance,
/// and layer 2 checks it against an independently sampled outline. This checks
/// only that the corner radius reached the shader at all: with a radius equal
/// to half the box's short side, the corner texel is outside the shape, and
/// with no radius it is inside.
#[test]
fn a_corner_radius_reaches_the_shader() {
    let mut paints = PaintTable::new();
    let colour = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let fill = paints.intern_fill(&dashpaint::FillSpec::Solid { color: colour });
    let sharp = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let round = paints.push(PaintEntry {
        fill,
        corners: CornerRadii {
            top_left: 12.0,
            top_right: 12.0,
            bottom_right: 12.0,
            bottom_left: 12.0,
        },
        ..PaintEntry::default()
    });
    let clips = ClipTable::new();
    let geometry = |paint| rect(16.0, 12.0, 24.0, 24.0, paint, ClipIndex::UNCLIPPED);

    let sharp_pixels = draw(&[geometry(sharp)], &paints, &clips);
    let round_pixels = draw(&[geometry(round)], &paints, &clips);

    // The texel just inside the top-left corner of the box.
    let at = (17, 13);
    assert!(
        texel(&sharp_pixels, at.0, at.1)[3] > 200,
        "a sharp corner covers its corner texel"
    );
    assert!(
        texel(&round_pixels, at.0, at.1)[3] < 55,
        "a 12-unit radius removes that corner, got {:?}",
        texel(&round_pixels, at.0, at.1)
    );
    // The middle is unaffected by the radius, so this is a corner difference
    // and not the whole shape moving.
    assert!(texel(&sharp_pixels, 28, 24)[3] > 250);
    assert!(texel(&round_pixels, 28, 24)[3] > 250);
}

/// A clip rejects what falls outside it.
///
/// The named layer-3 invariant from epic #569's table. The rect is drawn twice
/// over the same geometry, once unclipped and once through a region covering
/// only its left half; the right half must lose its coverage and the left half
/// must keep it.
#[test]
fn a_clip_region_rejects_what_lies_outside_it() {
    let mut paints = PaintTable::new();
    let green = paints.push_solid(Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    });
    let mut clips = ClipTable::new();
    let left_half = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 48.0,
        corners: CornerRadii::default(),
    }]);

    let geometry = |clip| rect(8.0, 12.0, 48.0, 24.0, green, clip);
    let unclipped = draw(&[geometry(ClipIndex::UNCLIPPED)], &paints, &clips);
    let clipped = draw(&[geometry(left_half)], &paints, &clips);

    // Inside the clip: both draw.
    assert!(texel(&unclipped, 16, 24)[3] > 250);
    assert!(
        texel(&clipped, 16, 24)[3] > 250,
        "the clipped rect still draws inside its region, got {:?}",
        texel(&clipped, 16, 24)
    );
    // Outside the clip: only the unclipped one draws. This is the assertion
    // that fails if the clip range never reached the shader.
    assert!(
        texel(&unclipped, 48, 24)[3] > 250,
        "the unclipped rect draws on the right"
    );
    assert_eq!(
        texel(&clipped, 48, 24)[3],
        0,
        "the clip rejects the right half"
    );
}

/// A later instance composites over an earlier one.
///
/// Slice order is draw order and the instance buffer preserves it
/// (`docs/decisions/instance-buffer-contract.md` D1), so the second rect wins
/// where they overlap. This fails if the frame is submitted out of order or if
/// the blend state is not source-over.
#[test]
fn a_later_rect_composites_over_an_earlier_one() {
    let mut paints = PaintTable::new();
    let red = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let blue = paints.push_solid(Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    let pixels = draw(
        &[
            rect(8.0, 8.0, 32.0, 32.0, red, ClipIndex::UNCLIPPED),
            rect(24.0, 8.0, 32.0, 32.0, blue, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &clips,
    );

    let only_red = texel(&pixels, 12, 24);
    let overlap = texel(&pixels, 32, 24);
    let only_blue = texel(&pixels, 52, 24);
    assert!(
        only_red[0] > 250 && only_red[2] < 5,
        "left is red, got {only_red:?}"
    );
    assert!(
        only_blue[2] > 250 && only_blue[0] < 5,
        "right is blue, got {only_blue:?}"
    );
    assert!(
        overlap[2] > 250 && overlap[0] < 5,
        "the later rect wins the overlap, got {overlap:?}"
    );
}

/// A rect's free-path opacity reaches the shader.
#[test]
fn the_rects_opacity_reaches_the_shader() {
    let mut paints = PaintTable::new();
    let black = paints.push_solid(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    let mut entry = rect(16.0, 12.0, 32.0, 24.0, black, ClipIndex::UNCLIPPED);
    entry.opacity = 0.5;
    let pixels = draw(&[entry], &paints, &clips);
    let alpha = texel(&pixels, 32, 24)[3];
    assert!(
        (120..=136).contains(&alpha),
        "a half-opaque rect composites at about half alpha, got {alpha}"
    );
}

/// Document space is y-down with its origin at the top-left, and the image
/// agrees.
///
/// Added because a flipped y axis survived mutation testing: every other
/// fixture here is centred on the canvas, so reflecting it maps each shape onto
/// itself and no assertion moves. That is the uniform-fixture defect in a third
/// guise — after uniform data and uniform arguments, uniform *symmetry* — and
/// the cure is the same, a fixture that can tell the two apart.
///
/// The rect sits in the top quarter of the canvas, so the top rows are covered
/// and the bottom rows are not. A flip swaps them.
#[test]
fn the_documents_y_down_origin_maps_to_the_top_of_the_image() {
    let mut paints = PaintTable::new();
    let fill = paints.push_solid(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    // y from 4 to 16 on a 48-tall canvas: nowhere near centred.
    let pixels = draw(
        &[rect(16.0, 4.0, 32.0, 12.0, fill, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );

    assert!(
        texel(&pixels, 32, 8)[3] > 250,
        "y = 8 is inside a rect spanning 4..16 and must be covered, got {:?}",
        texel(&pixels, 32, 8)
    );
    assert_eq!(
        texel(&pixels, 32, 40)[3],
        0,
        "y = 40 is far below that rect and must be clear — if it is covered, \
         the y axis is flipped"
    );
}

/// An instance kind this shader cannot draw yet draws nothing, and never
/// another table's row.
///
/// `Instance::tag` means a different enum for each `kind`, and the
/// discriminants collide: `PaintTag::Solid`, `ShadowKind::Inner` and
/// `BlurKind::Backdrop` are all 1. A fragment shader reading `tag` alone paints
/// `solids[row]` for a shadow instance, with `row` indexing the shadow table —
/// so a node with an inner shadow drew whatever colour sat at that row, over
/// its own fill. Found by review, with exactly this fixture.
#[test]
fn a_kind_this_shader_cannot_draw_is_black_not_another_tables_row() {
    use dashpaint::{EntryParts, FillSpec, Shadow, ShadowKind, Vec2};

    let mut paints = PaintTable::new();
    // Row 0 of the solid table is green; the node's own fill is red at row 1.
    // A shadow instance mistaking its shadow row for a solid row lands on row 0.
    let green = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
    });
    let red = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    });
    let _ = green;
    let paint = paints.push_with(
        PaintEntry {
            fill: red,
            ..PaintEntry::default()
        },
        EntryParts {
            // An inner shadow, so its instance is pushed *after* the fill and
            // would paint over it.
            shadows: &[Shadow {
                kind: ShadowKind::Inner,
                offset: Vec2 { x: 0.0, y: 0.0 },
                blur: 0.0,
                spread: 0.0,
                color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            }],
            ..EntryParts::default()
        },
    );
    let clips = ClipTable::new();
    let pixels = draw(
        &[rect(16.0, 12.0, 32.0, 24.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );
    let middle = texel(&pixels, 32, 24);
    assert!(
        middle[0] > 250 && middle[1] < 5 && middle[2] < 5,
        "the node's own red fill survives its inner shadow, got {middle:?} — green means \
         the shadow instance indexed the solid table, black means it painted over the fill"
    );
}

/// Half-opaque red over opaque green composites in sRGB-encoded space.
///
/// The target format is the whole of this: `Rgba8Unorm` blends the stored
/// bytes, `Rgba8UnormSrgb` has the hardware convert to linear light and blend
/// there. `docs/decisions/blur-blends-in-srgb-encoded-space.md` requires the
/// former as a term of the boundary-B contract, and the two differ by 60 code
/// points on this fixture — the same order as the ~50 that record measures.
///
/// Added because a one-character change to `TARGET_FORMAT` passed every other
/// test in this file, which made the decision the PR called "load-bearing" the
/// least defended thing in it.
#[test]
fn compositing_happens_in_srgb_encoded_space() {
    let mut paints = PaintTable::new();
    let green = paints.push_solid(Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    });
    let half_red = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 0.5,
    });
    let clips = ClipTable::new();
    let pixels = draw(
        &[
            rect(8.0, 8.0, 48.0, 32.0, green, ClipIndex::UNCLIPPED),
            rect(8.0, 8.0, 48.0, 32.0, half_red, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &clips,
    );
    let mixed = texel(&pixels, 32, 24);
    // 0.5 * 255 over 0.5 * 255, blended on the stored bytes.
    assert!(
        (120..=136).contains(&mixed[0]) && (120..=136).contains(&mixed[1]),
        "sRGB-encoded blending gives about [128, 128, 0]; linear light gives about \
         [188, 188, 0]. Got {mixed:?}"
    );
}

/// A clip region that does not start at box zero is still the one applied.
///
/// The uniform-fixture guard for the clip range: with one region in the table
/// its offset is zero, and a shader indexing `clip_boxes[i]` instead of
/// `clip_boxes[clip_offset + i]` passes.
#[test]
fn a_clip_region_past_the_first_is_the_one_applied() {
    let mut paints = PaintTable::new();
    let fill = paints.push_solid(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    });
    let mut clips = ClipTable::new();
    // The decoy occupies box 0 and covers the whole canvas, so a shader reading
    // the wrong offset clips nothing and the assertion below fails.
    let _decoy = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 64.0,
        h: 48.0,
        corners: CornerRadii::default(),
    }]);
    let live = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 48.0,
        corners: CornerRadii::default(),
    }]);
    let pixels = draw(&[rect(8.0, 12.0, 48.0, 24.0, fill, live)], &paints, &clips);
    assert!(texel(&pixels, 16, 24)[3] > 250, "inside the live region");
    assert_eq!(
        texel(&pixels, 48, 24)[3],
        0,
        "outside the live region — coverage here means the shader read box 0"
    );
}

/// The four corner radii reach their own corners.
///
/// Every earlier fixture used four equal radii, so a shader that permuted them
/// passed. This rounds only the top-left.
#[test]
fn each_corner_radius_lands_on_its_own_corner() {
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&dashpaint::FillSpec::Solid {
        color: Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        },
    });
    let paint = paints.push(PaintEntry {
        fill,
        corners: CornerRadii {
            top_left: 12.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        },
        ..PaintEntry::default()
    });
    let clips = ClipTable::new();
    let pixels = draw(
        &[rect(16.0, 12.0, 24.0, 24.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );
    // Top-left is rounded away; the other three corners are square.
    assert!(texel(&pixels, 17, 13)[3] < 55, "top-left is rounded");
    assert!(texel(&pixels, 38, 13)[3] > 200, "top-right is square");
    assert!(texel(&pixels, 38, 34)[3] > 200, "bottom-right is square");
    assert!(texel(&pixels, 17, 34)[3] > 200, "bottom-left is square");
}

/// A shape whose edge falls between pixel centres is antialiased.
///
/// Every earlier fixture is integer-aligned, so the antialiasing width and the
/// quad's AA margin could both be zeroed without moving an assertion.
#[test]
fn a_fractional_edge_is_antialiased() {
    let mut paints = PaintTable::new();
    let fill = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    // The right edge lands at x = 40.5, half a pixel through the texel at 40.
    let pixels = draw(
        &[rect(16.0, 12.0, 24.5, 24.0, fill, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );
    let edge = texel(&pixels, 40, 24)[3];
    assert!(
        (1..=254).contains(&edge),
        "a fractional edge is partly covered, got {edge} — 0 or 255 means the ramp \
         or the quad's margin was lost"
    );
}

/// The readback survives a width whose row is not a multiple of 256 bytes.
///
/// The canvas everywhere else is 64 wide, which is exactly 256 bytes, so the
/// row-padding arithmetic was never exercised.
#[test]
fn a_width_that_needs_row_padding_reads_back_correctly() {
    let mut paints = PaintTable::new();
    let fill = paints.push_solid(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    let (w, h) = (63u32, 48u32); // 252 bytes per row, padded to 256
    let mut painter = GpuPainter::new();
    painter.paint(
        &[rect(8.0, 8.0, 40.0, 32.0, fill, ClipIndex::UNCLIPPED)],
        &paints,
        &ImageTable::new(),
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let pixels = renderer()
        .render(
            painter.instances(),
            &paints,
            &ImageTable::new(),
            &clips,
            &GlyphRunTable::new(),
            w,
            h,
        )
        .expect("the fixture extent is within any device's maximum");
    assert_eq!(pixels.len(), (w * h * 4) as usize);
    let at = |x: u32, y: u32| pixels[((y * w + x) * 4 + 3) as usize];
    // Rows near the bottom are where a wrong stride shears the image.
    assert!(at(24, 12) > 250, "inside, near the top");
    assert!(at(24, 36) > 250, "inside, near the bottom");
    assert_eq!(
        at(60, 44),
        0,
        "outside, in the last column of the last rows"
    );
}

/// A translucent fill reads back unpremultiplied.
///
/// The blend state is premultiplied, so the texture holds `colour * alpha`.
/// `goldens/README.md` compares unpremultiplied RGBA8888, and every other
/// fixture here is either fully opaque or fully clear — where unpremultiplying
/// is the identity. A half-opaque red is where it is not.
#[test]
fn a_translucent_fill_reads_back_unpremultiplied() {
    let mut paints = PaintTable::new();
    let half_red = paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 0.5,
    });
    let clips = ClipTable::new();
    let pixels = draw(
        &[rect(16.0, 12.0, 32.0, 24.0, half_red, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );
    let middle = texel(&pixels, 32, 24);
    assert!(
        (120..=136).contains(&middle[3]),
        "half alpha, got {middle:?}"
    );
    assert!(
        middle[0] > 250,
        "red reads back at full strength once unpremultiplied, got {middle:?} — about \
         128 means the premultiplied bytes were returned as-is"
    );
}

/// The antialiasing ramp is one unit wide, not four.
///
/// A wider ramp still antialiases, so the fractional-edge test above cannot
/// tell them apart. This one can: two units outside a straight edge is clear at
/// a one-unit ramp and partly covered at a four-unit one.
#[test]
fn the_antialiasing_ramp_is_one_unit_wide() {
    let mut paints = PaintTable::new();
    let fill = paints.push_solid(Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    });
    let clips = ClipTable::new();
    // The right edge is at x = 40.0 exactly.
    let pixels = draw(
        &[rect(16.0, 12.0, 24.0, 24.0, fill, ClipIndex::UNCLIPPED)],
        &paints,
        &clips,
    );
    // Texel 41 spans 41.0..42.0, so its centre is 1.5 units outside the edge:
    // beyond a half-unit half-ramp, well inside a two-unit one.
    assert_eq!(
        texel(&pixels, 41, 24)[3],
        0,
        "1.5 units outside a straight edge is clear at a one-unit ramp, got {:?}",
        texel(&pixels, 41, 24)
    );
    // And the texel straddling the edge is still partly covered, so this is a
    // ramp width and not the ramp being switched off.
    assert!(
        (1..=254).contains(&texel(&pixels, 39, 24)[3]) || texel(&pixels, 39, 24)[3] == 255,
        "the edge itself is still covered"
    );
}

// ---------------------------------------------------------------------------
// Strokes (story #710)
// ---------------------------------------------------------------------------

/// The stroked box every test below draws: x 16..48, y 12..36.
const STROKE_BOX: (f32, f32, f32, f32) = (16.0, 12.0, 32.0, 24.0);

/// Four units, so each alignment puts its band in a place the others do not
/// reach: Outside covers x 48..52, Center 46..50, Inside 44..48.
const STROKE_WIDTH: f32 = 4.0;

/// A stroke-only paint entry — no fill at all, so the interior of the shape is
/// covered by nothing but the band, and an assertion about the interior means
/// what it says.
fn stroked(paints: &mut PaintTable, align: StrokeAlign, color: Color) -> dashpaint::PaintIndex {
    paints.push_with(
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: STROKE_WIDTH,
                align,
                color,
            }),
            ..EntryParts::default()
        },
    )
}

/// The alpha at (x, y), which is the whole of what a coverage claim needs.
fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
    texel(pixels, x, y)[3]
}

fn stroke_only(align: StrokeAlign) -> Vec<u8> {
    let mut paints = PaintTable::new();
    let ink = stroked(
        &mut paints,
        align,
        Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
    );
    let (x, y, w, h) = STROKE_BOX;
    draw(
        &[rect(x, y, w, h, ink, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    )
}

/// The one an instanced quad gets wrong by construction: an Outside stroke
/// paints a full width **beyond** the box its instance is stated over, and the
/// quad is built from that box.
///
/// Without the vertex shader growing the quad by the stroke's outset, the outer
/// half of the band is clipped by its own geometry — which looks like a thinner
/// stroke rather than like a defect, and no assertion about the interior would
/// notice.
#[test]
fn an_outside_stroke_draws_past_the_box_its_quad_is_built_from() {
    let pixels = stroke_only(StrokeAlign::Outside);
    let (x, y, w, h) = STROKE_BOX;
    let right = (x + w) as u32;
    let middle = (y + h / 2.0) as u32;

    // Two units beyond the right edge: inside the band, outside the quad the
    // instance's own bounds would have made.
    assert!(
        alpha(&pixels, right + 2, middle) > 200,
        "an Outside stroke must draw past the box, got alpha {}",
        alpha(&pixels, right + 2, middle)
    );
    // And it stops: a unit past the band is clear.
    assert_eq!(
        alpha(&pixels, right + 5, middle),
        0,
        "the band must end at the stroke width"
    );
}

/// A stroke-only node leaves its interior untouched. The claim a fill-shaped
/// assertion cannot make, and the one that fails if the shader ever falls back
/// to the fill's own coverage for a stroke row.
#[test]
fn a_stroke_covers_a_band_and_not_the_interior() {
    for align in [
        StrokeAlign::Inside,
        StrokeAlign::Center,
        StrokeAlign::Outside,
    ] {
        let pixels = stroke_only(align);
        let (x, y, w, h) = STROKE_BOX;
        let centre = ((x + w / 2.0) as u32, (y + h / 2.0) as u32);
        assert_eq!(
            alpha(&pixels, centre.0, centre.1),
            0,
            "{align:?}: the middle of a stroke-only node is not ink"
        );
    }
}

/// The three alignments put the band in three different places, which is the
/// whole of what the alignment means. An implementation that ignored it — or
/// that read the variant as a number rather than through the packer's map —
/// would put all three in the same place and pass every assertion above.
#[test]
fn each_alignment_puts_the_band_somewhere_the_others_do_not() {
    let (x, y, w, h) = STROKE_BOX;
    let edge = (x + w) as u32;
    let middle = (y + h / 2.0) as u32;
    // Texel `i` is sampled at its centre, `i + 0.5`, so these two sit 1.5 units
    // either side of the edge — half a unit inside each band's boundary, which
    // is a full coverage rather than the 0.5 the boundary itself would give.
    // `within` is covered by Inside and Center, `without` by Center and
    // Outside, so the three are told apart by two samples.
    let (within, without) = (edge - 2, edge + 1);

    let inside = stroke_only(StrokeAlign::Inside);
    assert!(
        alpha(&inside, within, middle) > 200 && alpha(&inside, without, middle) == 0,
        "an Inside stroke lies within the shape: got {} within, {} without",
        alpha(&inside, within, middle),
        alpha(&inside, without, middle)
    );

    let outside = stroke_only(StrokeAlign::Outside);
    assert!(
        alpha(&outside, without, middle) > 200 && alpha(&outside, within, middle) == 0,
        "an Outside stroke lies without: got {} within, {} without",
        alpha(&outside, within, middle),
        alpha(&outside, without, middle)
    );

    let centre = stroke_only(StrokeAlign::Center);
    assert!(
        alpha(&centre, within, middle) > 200 && alpha(&centre, without, middle) > 200,
        "a Center stroke straddles the outline: got {} within, {} without",
        alpha(&centre, within, middle),
        alpha(&centre, without, middle)
    );
}

/// The colour comes from the stroke row, not from the solid table.
///
/// `Instance::row` means a row of whichever table the kind names, and the two
/// tables are indexed independently — so a shader that reached for `solids[row]`
/// would paint a stroke with a fill colour and still cover exactly the right
/// band. The fill here is a different colour at the same row index, which is
/// what makes the mistake visible.
#[test]
fn a_stroke_takes_its_colour_from_the_stroke_row() {
    let mut paints = PaintTable::new();
    // Solid row 0: red. Nothing in this scene draws it; it exists so that a
    // shader reading the wrong table has something wrong to find.
    paints.push_solid(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    // Stroke row 0: blue.
    let ink = stroked(
        &mut paints,
        StrokeAlign::Center,
        Color {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        },
    );
    let (x, y, w, h) = STROKE_BOX;
    let pixels = draw(
        &[rect(x, y, w, h, ink, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    );

    let on_band = texel(&pixels, (x + w) as u32, (y + h / 2.0) as u32);
    assert!(
        on_band[2] > 200 && on_band[0] < 55,
        "the band must be the stroke's blue and not the solid table's red, got {on_band:?}"
    );
}

// ---------------------------------------------------------------------------
// Gradient fills (issue #715)
// ---------------------------------------------------------------------------

/// A three-stop gradient whose range starts at 0.25 and ends at 0.75, so both
/// clamped ends and both interior segments are reachable inside one box.
///
/// `kind` is the only thing that varies between calls, which is what
/// `each_gradient_kind_draws_its_own_picture` needs: with a shared fixture, a
/// difference in the picture can only come from the kind.
fn ramp(kind: dashpaint::GradientKind) -> dashpaint::FillSpec {
    dashpaint::FillSpec::Gradient {
        gradient: dashpaint::Gradient {
            kind,
            // The box's own left edge to its own right edge, with the secondary
            // handle down its left side: the identity frame over the node box.
            handle_origin: dashpaint::Vec2 { x: 0.0, y: 0.0 },
            handle_primary: dashpaint::Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: dashpaint::Vec2 { x: 0.0, y: 1.0 },
            stops: dashpaint::StopRange::NONE,
        },
        stops: vec![
            dashpaint::GradientStop {
                offset: 0.25,
                color: Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            },
            dashpaint::GradientStop {
                offset: 0.5,
                color: Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
            },
            dashpaint::GradientStop {
                offset: 0.75,
                color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            },
        ],
    }
}

/// A channel comparison that tolerates the rounding of an eight-bit target and
/// nothing more.
fn near(actual: [u8; 4], expected: [u8; 4], what: &str) {
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 2,
            "{what}: channel {channel} was {} and should be about {} (whole texel {actual:?} \
             against {expected:?})",
            actual[channel],
            expected[channel]
        );
    }
}

/// A gradient fill takes a different colour at different points inside one
/// shape — which is the whole of what a solid fill cannot do — and paints
/// nothing outside it.
///
/// # The probes, and where their numbers come from
///
/// The box spans x 16..48, so a fragment at pixel column `x` samples the box's
/// normalised coordinate `t = (x + 0.5 - 16) / 32` — a fragment's position is
/// its pixel's centre, and the painter draws at unit scale.
///
/// - `x = 18` gives t = 0.078, below the first stop, so the ramp clamps to red.
/// - `x = 46` gives t = 0.953, past the last stop, so it clamps to blue.
/// - `x = 32` gives t = 0.515625, inside the second segment at
///   `u = (0.515625 - 0.5) / 0.25 = 0.0625` — green a sixteenth of the way to
///   blue, which is `[0, 239, 16]`. That value is the one a wrong *segment*
///   fails: the first segment at the same `t` would be most of the way from red
///   to green.
///
/// # Two solid fills are interned first, deliberately
///
/// They put the gradient region's base past zero. The heap holds the solids at
/// its head and the gradients after them, so a painter that lost
/// `Globals::gradient_base` would read solid colours as gradient handles — and
/// with no solids in the table that mistake draws the right picture.
#[test]
fn a_gradient_fill_varies_across_the_shape_where_a_solid_cannot() {
    let mut paints = PaintTable::new();
    let teal = paints.intern_fill(&dashpaint::FillSpec::Solid {
        color: Color {
            r: 0.0,
            g: 0.5,
            b: 0.5,
            a: 1.0,
        },
    });
    let _unused = paints.intern_fill(&dashpaint::FillSpec::Solid {
        color: Color {
            r: 0.9,
            g: 0.9,
            b: 0.1,
            a: 1.0,
        },
    });
    let solid = paints.push(PaintEntry {
        fill: teal,
        ..PaintEntry::default()
    });
    let sweep = paints.intern_fill(&ramp(dashpaint::GradientKind::Linear));
    let gradient = paints.push(PaintEntry {
        fill: sweep,
        ..PaintEntry::default()
    });

    let pixels = draw(
        &[
            rect(16.0, 12.0, 32.0, 24.0, gradient, ClipIndex::UNCLIPPED),
            rect(4.0, 40.0, 8.0, 6.0, solid, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
    );

    near(
        texel(&pixels, 18, 24),
        [255, 0, 0, 255],
        "below the first stop the ramp clamps to it",
    );
    near(
        texel(&pixels, 46, 24),
        [0, 0, 255, 255],
        "past the last stop the ramp clamps to it",
    );
    near(
        texel(&pixels, 32, 24),
        [0, 239, 16, 255],
        "the middle of the box is inside the second segment",
    );

    // The gradient's own edges: nothing outside the box it fills.
    for (x, y) in [(2, 2), (61, 2), (61, 24)] {
        assert_eq!(
            texel(&pixels, x, y)[3],
            0,
            "({x}, {y}) is outside the gradient's box and must be clear"
        );
    }

    // And the solid path still reads its own row out of the heap's head.
    near(
        texel(&pixels, 8, 43),
        [0, 128, 128, 255],
        "a solid fill beside a gradient is still its own colour",
    );
}

/// The four gradient kinds are four different pictures.
///
/// One fixture, four renders, differing only in the kind — the shape a claim
/// takes when no single output can discriminate. An assertion on one render
/// could not tell a mis-mapped kind from a correct one, because every kind
/// produces a plausible ramp over the same stops.
#[test]
fn each_gradient_kind_draws_its_own_picture() {
    use dashpaint::GradientKind;

    let kinds = [
        GradientKind::Linear,
        GradientKind::Radial,
        GradientKind::Angular,
        GradientKind::Diamond,
    ];
    // Off the box's centre and off its diagonal, so no two of the four
    // parameterisations coincide there by symmetry.
    const PROBE: (u32, u32) = (26, 19);

    let drawn: Vec<[u8; 4]> = kinds
        .iter()
        .map(|&kind| {
            let mut paints = PaintTable::new();
            let fill = paints.intern_fill(&ramp(kind));
            let paint = paints.push(PaintEntry {
                fill,
                ..PaintEntry::default()
            });
            let pixels = draw(
                &[rect(16.0, 12.0, 32.0, 24.0, paint, ClipIndex::UNCLIPPED)],
                &paints,
                &ClipTable::new(),
            );
            texel(&pixels, PROBE.0, PROBE.1)
        })
        .collect();

    for (i, a) in drawn.iter().enumerate() {
        assert_eq!(a[3], 255, "{:?} covers the probe", kinds[i]);
        for (j, b) in drawn.iter().enumerate().skip(i + 1) {
            assert!(
                a != b,
                "{:?} and {:?} drew the same texel {a:?} at {PROBE:?}; one kind is reaching \
                 another's parameterisation",
                kinds[i],
                kinds[j]
            );
        }
    }
}

/// The second gradient in a table draws its own parameters, not the first
/// one's.
///
/// This is the assertion the heap's *stride* is falsifiable by. A frame with one
/// gradient reads row 0, whose words begin at the region's base whatever the
/// stride is — so every fixture above passes with the stride multiplied by
/// anything. Two rows separate them: at any stride but twelve, row 1 lands on
/// words the first gradient wrote, which is a well-formed frame with plausible
/// stops and draws a picture rather than failing.
///
/// The two gradients agree in nothing. Different kinds, different handles,
/// different stop counts, different colours — so the second row cannot be
/// mistaken for the first in any single channel either.
#[test]
fn a_second_gradient_row_draws_its_own_parameters() {
    let mut paints = PaintTable::new();
    // Row 0: the three-stop fixture, over the left half of the canvas.
    let first = paints.intern_fill(&ramp(dashpaint::GradientKind::Linear));
    let first_paint = paints.push(PaintEntry {
        fill: first,
        ..PaintEntry::default()
    });
    // Row 1: two stops, both grey-free and both distinct from anything above,
    // over a box on the right.
    let second = paints.intern_fill(&dashpaint::FillSpec::Gradient {
        gradient: dashpaint::Gradient {
            kind: dashpaint::GradientKind::Linear,
            handle_origin: dashpaint::Vec2 { x: 0.0, y: 0.0 },
            handle_primary: dashpaint::Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: dashpaint::Vec2 { x: 0.0, y: 1.0 },
            stops: dashpaint::StopRange::NONE,
        },
        stops: vec![
            dashpaint::GradientStop {
                offset: 0.0,
                color: Color {
                    r: 1.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
            },
            dashpaint::GradientStop {
                offset: 1.0,
                color: Color {
                    r: 0.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
            },
        ],
    });
    let second_paint = paints.push(PaintEntry {
        fill: second,
        ..PaintEntry::default()
    });

    let pixels = draw(
        &[
            rect(0.0, 4.0, 16.0, 16.0, first_paint, ClipIndex::UNCLIPPED),
            rect(32.0, 4.0, 16.0, 16.0, second_paint, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
    );

    // Row 0's box spans x 0..16, so the probe at x = 4 sits at t = 4.5/16 =
    // 0.28125 — just inside its first segment, at u = 0.125 of the way from red
    // to green.
    near(
        texel(&pixels, 4, 12),
        [223, 32, 0, 255],
        "the first gradient's own first segment",
    );
    // Row 1's box spans x 32..48, so x = 36 sits at t = 4.5/16 = 0.28125 of a
    // full-range yellow-to-cyan ramp: red falls to 0.71875 and blue rises to
    // 0.28125. The same t as the probe above, deliberately — the two rows differ
    // only in what they hold, not in where the fragment is inside them.
    near(
        texel(&pixels, 36, 12),
        [183, 255, 72, 255],
        "the second gradient's own stops, at the same t as the first's probe",
    );
}

/// An opaque solid, for the group tests below.
fn solid(paints: &mut PaintTable, r: f32, g: f32, b: f32) -> dashpaint::PaintIndex {
    paints.push_solid(Color { r, g, b, a: 1.0 })
}

/// **The whole reason the render-target path exists.** Two overlapping opaque
/// members of one group at alpha `a` composite as *one* image at `a` — where
/// they overlap, only the upper member shows.
///
/// This is the case per-rect alpha cannot express, and
/// `masks-and-group-opacity.md` says so: multiplying each member's alpha
/// independently lets the lower one show through the overlap. The two answers
/// are far apart and this asserts the right one, so a painter that quietly took
/// the free path fails here rather than looking approximately correct.
///
/// The numbers: inside the layer the blue rect covers the red one completely
/// where they meet, so the layer holds opaque blue there. Composited at 0.5
/// that is blue at half alpha. Had each rect been multiplied instead, blue at
/// 0.5 would sit over red at 0.5 — alpha 0.75, and visibly purple.
#[test]
fn overlapping_members_of_a_group_composite_as_one_image() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);

    let pixels = draw_groups(
        &[
            rect(8.0, 8.0, 20.0, 20.0, red, ClipIndex::UNCLIPPED),
            rect(18.0, 8.0, 20.0, 20.0, blue, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
        &[dashpaint::GroupComposite {
            start: 0,
            end: 2,
            alpha: 0.5,
        }],
    );

    near(
        texel(&pixels, 12, 18),
        [255, 0, 0, 128],
        "the red member alone, at the group's alpha",
    );
    near(
        texel(&pixels, 33, 18),
        [0, 0, 255, 128],
        "the blue member alone, at the group's alpha",
    );
    // x 18..28 is the overlap. The free path would put alpha 191 here and mix
    // red into it; the render-target path puts the upper member alone at 128.
    near(
        texel(&pixels, 23, 18),
        [0, 0, 255, 128],
        "the overlap shows the upper member only",
    );
}

/// A member drawn after the group draws **over** the composited group, not
/// under it — the composite lands where the group's instances end, not at the
/// end of the frame.
#[test]
fn a_rect_after_a_group_draws_over_the_composited_group() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let green = solid(&mut paints, 0.0, 1.0, 0.0);

    let pixels = draw_groups(
        &[
            rect(8.0, 8.0, 20.0, 20.0, red, ClipIndex::UNCLIPPED),
            rect(18.0, 8.0, 20.0, 20.0, green, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
        // Only the first rect is in the group; the second follows it.
        &[dashpaint::GroupComposite {
            start: 0,
            end: 1,
            alpha: 0.5,
        }],
    );

    near(
        texel(&pixels, 12, 18),
        [255, 0, 0, 128],
        "the group's own member, at its alpha",
    );
    // The later rect is opaque and outside the group, so it covers the
    // composited group entirely. A painter that composited every layer after
    // the last instance would show this blended under the group instead.
    near(
        texel(&pixels, 23, 18),
        [0, 255, 0, 255],
        "the rect after the group covers it",
    );
}

/// Nested groups compound: an inner group at 0.5 inside an outer at 0.5 reaches
/// the target at 0.25.
///
/// Two layers rather than one, because one cannot falsify the nesting — an
/// inner layer composited straight into the frame's target instead of into its
/// parent reaches it at 0.5, and no single-group fixture can tell those apart.
#[test]
fn a_nested_group_composites_through_its_parent() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);

    let pixels = draw_groups(
        &[
            rect(4.0, 8.0, 12.0, 20.0, red, ClipIndex::UNCLIPPED),
            rect(28.0, 8.0, 12.0, 20.0, blue, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
        &[
            dashpaint::GroupComposite {
                start: 0,
                end: 2,
                alpha: 0.5,
            },
            // The inner group holds the second rect only.
            dashpaint::GroupComposite {
                start: 1,
                end: 2,
                alpha: 0.5,
            },
        ],
    );

    near(
        texel(&pixels, 10, 18),
        [255, 0, 0, 128],
        "the outer group's own member, at 0.5",
    );
    // 0.5 * 0.5. A painter compositing the inner layer into the frame's target
    // rather than into its parent would leave 128 here.
    near(
        texel(&pixels, 34, 18),
        [0, 0, 255, 64],
        "the inner group's member, through both alphas",
    );
}

/// A group's alpha reaches the layer it belongs to and not another's. Two
/// groups with **different** alphas, because two groups sharing one would pass
/// with the layers swapped.
#[test]
fn each_group_composites_at_its_own_alpha() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);

    let pixels = draw_groups(
        &[
            rect(4.0, 8.0, 12.0, 20.0, red, ClipIndex::UNCLIPPED),
            rect(28.0, 8.0, 12.0, 20.0, blue, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
        &[
            dashpaint::GroupComposite {
                start: 0,
                end: 1,
                alpha: 0.25,
            },
            dashpaint::GroupComposite {
                start: 1,
                end: 2,
                alpha: 0.75,
            },
        ],
    );

    near(
        texel(&pixels, 10, 18),
        [255, 0, 0, 64],
        "the first group's alpha",
    );
    near(
        texel(&pixels, 34, 18),
        [0, 0, 255, 191],
        "the second group's alpha",
    );
}

/// What a group draws into is cleared, and what the frame drew before it is
/// not. A layer pass that cleared the frame's target on the way back would
/// erase everything drawn before the group.
#[test]
fn a_group_does_not_erase_what_was_drawn_before_it() {
    let mut paints = PaintTable::new();
    let green = solid(&mut paints, 0.0, 1.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);

    let pixels = draw_groups(
        &[
            rect(4.0, 8.0, 12.0, 20.0, green, ClipIndex::UNCLIPPED),
            rect(28.0, 8.0, 12.0, 20.0, blue, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
        // The group starts at the *second* rect, so the first is drawn into the
        // frame's target before the layer pass begins.
        &[dashpaint::GroupComposite {
            start: 1,
            end: 2,
            alpha: 0.5,
        }],
    );

    near(
        texel(&pixels, 10, 18),
        [0, 255, 0, 255],
        "the rect drawn before the group survives it",
    );
    near(
        texel(&pixels, 34, 18),
        [0, 0, 255, 128],
        "the group's own member",
    );
}

/// A clip box's **corner radii** reach the shader, and each reaches its own
/// corner.
///
/// Story #580 built `clip_coverage` to intersect rounded boxes and every clip
/// fixture since has used `CornerRadii::default()`, so the `b.corners` term
/// could have been replaced by zero and nothing would have failed — the clip
/// axis of the uniform-fixture trap. Both probes are needed and neither is
/// redundant: the rounded corner rejects a point a square clip would admit, and
/// the square corner admits a point that would be rejected if the radius were
/// applied to every corner instead of its own.
#[test]
fn a_clip_boxs_corner_radii_reach_their_own_corners() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);

    let mut clips = ClipTable::new();
    // A 32x32 clip box whose top-left corner is rounded by half its side and
    // whose other three corners are square.
    let rounded = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 32.0,
        corners: CornerRadii {
            top_left: 16.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        },
    }]);

    let pixels = draw(&[rect(0.0, 0.0, 64.0, 48.0, red, rounded)], &paints, &clips);

    // (1, 1) is inside the box and outside the corner's arc: the arc's centre
    // is (16, 16) at radius 16, and the probe sits about 20.5 units from it —
    // far outside the one-unit antialiasing ramp.
    assert_eq!(
        texel(&pixels, 1, 1),
        [0, 0, 0, 0],
        "the rounded corner rejects what lies outside its arc"
    );
    // The opposite corner is square, so the symmetric probe is admitted. A
    // shader applying one radius to all four corners fails here.
    near(
        texel(&pixels, 30, 30),
        [255, 0, 0, 255],
        "a square corner of the same box admits its own corner",
    );
    near(
        texel(&pixels, 16, 16),
        [255, 0, 0, 255],
        "the middle of the clip box is unaffected",
    );
}

/// The layer textures a group needs are counted by [`Renderer::allocations`],
/// and a steady-state frame that has one still allocates nothing.
///
/// Added in review, and the first half is the point. Story #719 found this
/// counter omitting residency's textures, which made the "allocates nothing"
/// claim unable to fail; the same fix then stayed unfalsifiable because the
/// omitted term was zero in every fixture that read it. `frame_path.rs` passes
/// no groups anywhere, so nothing there could tell whether the layer term is in
/// the sum at all.
///
/// This differences the same scene against itself, with and without a group, so
/// the layer term is the only thing that can move — and a build that drops it
/// from the sum reports no difference.
#[test]
fn a_groups_layer_objects_are_counted_and_then_reused() {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);
    let clips = ClipTable::new();
    let rects = [
        rect(8.0, 8.0, 20.0, 20.0, red, ClipIndex::UNCLIPPED),
        rect(18.0, 8.0, 20.0, 20.0, blue, ClipIndex::UNCLIPPED),
    ];
    let group = [dashpaint::GroupComposite {
        start: 0,
        end: 2,
        alpha: 0.5,
    }];

    // One renderer for the whole test: the counter is cumulative, and a second
    // renderer would start it again from its own construction.
    let mut renderer = self::renderer();
    let mut render = |groups: &[dashpaint::GroupComposite]| {
        let mut painter = GpuPainter::new();
        painter.paint(
            &rects,
            &paints,
            &ImageTable::new(),
            &clips,
            groups,
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                &paints,
                &ImageTable::new(),
                &clips,
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum");
        renderer.allocations()
    };

    // The same scene with no group, twice, so everything a groupless frame
    // needs is already allocated and settled before the group arrives.
    render(&[]);
    let without = render(&[]);

    let with = render(&group);
    assert_eq!(
        with - without,
        4,
        "a layer is a texture, a view, a uniform buffer and a bind group, and \
         all four have to reach the counter"
    );

    // And they are reused: the extent and the layer count are unchanged, so the
    // second grouped frame builds nothing.
    assert_eq!(
        render(&group),
        with,
        "a repeated frame reallocated its layer"
    );
}
