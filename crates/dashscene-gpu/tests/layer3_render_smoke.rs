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
    PaintEntry, PaintTable, Painter, RectEntry, Stroke, StrokeAlign, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer, RendererError};

mod common;
use common::{H, W, renderer, texel};

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
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }
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
    .expect(
        "layer 3 needs a wgpu adapter and found none. On a runner this means no software device \
         is available; the test job installs mesa-vulkan-drivers.",
    );

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
    let group = one_group();
    let mut renderer = self::renderer();
    let mut render = layer_allocation_probe();

    // The same scene with no group, twice, so everything a groupless frame
    // needs is already allocated and settled before the group arrives.
    render(&mut renderer, &[]);
    let without = render(&mut renderer, &[]);

    let with = render(&mut renderer, &group);
    assert_eq!(
        with - without,
        4,
        "a layer is a texture, a view, a uniform buffer and a bind group, and \
         all four have to reach the counter"
    );

    // And they are reused: the extent and the layer count are unchanged, so the
    // second grouped frame builds nothing.
    assert_eq!(
        render(&mut renderer, &group),
        with,
        "a repeated frame reallocated its layer"
    );
}

/// The two-rect scene both layer-allocation tests measure, and a probe that
/// renders it and reports [`Renderer::allocations`].
///
/// Shared because the two tests differ only in the sequence of frames they
/// render; a second copy of the fixture would let them drift into measuring
/// different scenes. **One renderer per probe**: the counter is cumulative, so a
/// second renderer would start it again from its own construction.
fn layer_allocation_probe() -> impl FnMut(&mut Renderer, &[dashpaint::GroupComposite]) -> u64 {
    let mut paints = PaintTable::new();
    let red = solid(&mut paints, 1.0, 0.0, 0.0);
    let blue = solid(&mut paints, 0.0, 0.0, 1.0);
    let clips = ClipTable::new();
    let rects = [
        rect(8.0, 8.0, 20.0, 20.0, red, ClipIndex::UNCLIPPED),
        rect(18.0, 8.0, 20.0, 20.0, blue, ClipIndex::UNCLIPPED),
    ];
    move |renderer: &mut Renderer, groups: &[dashpaint::GroupComposite]| {
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
    }
}

/// The group both layer-allocation tests raise and drop.
fn one_group() -> [dashpaint::GroupComposite; 1] {
    [dashpaint::GroupComposite {
        start: 0,
        end: 2,
        alpha: 0.5,
    }]
}

/// **A render-target group that comes and goes on alternate frames does not
/// rebuild its layer, and two idle frames still release it** (issue #1055).
///
/// `BlurTargets` had this shape and issue #1020 closed it with a grace period.
/// `LayerTargets` had the same release with no grace, so a fading overlay or a
/// group whose only node is toggled released and rebuilt a drawable-sized
/// texture, its view, a uniform buffer and a bind group **per layer, on every
/// change**. Measured before the fix: four allocations for one layer, 31
/// against 27.
///
/// # Both halves, because a hold that never releases would pass one of them
///
/// The grace has two edges and this straddles both, the way
/// `an_alternating_refusal_does_not_rebuild_the_frame_wide_targets` does for
/// the blur half:
///
/// - A gap of **one** frame costs nothing. That is the fix.
/// - A gap of **two** releases, and the next group frame pays to rebuild. That
///   is what keeps a scene that has genuinely stopped compositing from holding
///   a drawable-sized texture per layer for the life of the renderer.
///
/// A test that stopped at one idle frame would pass with `Idle::Expired`
/// deleted, or with `TARGET_GRACE_FRAMES` raised to any value at all.
///
/// **This test assumes `TARGET_GRACE_FRAMES` is 1**, which it cannot read — the
/// constant is private to the crate. Raising it makes the second half fail, and
/// the failure is this sentence.
///
/// # What may safely be held, which is the question #1055 asks
///
/// Everything here. `LayerTargets` holds a texture, its view, its own alpha
/// uniform and a bind group naming those two — and **nothing it holds names a
/// resource it does not own**. That is the difference from `BlurTargets`, whose
/// per-backdrop bind groups name a coverage atlas view belonging to the
/// residency set, which is why that type releases in two steps and this one does
/// not need to. The alphas are rewritten every frame a layer is prepared, and
/// each layer pass clears its attachment, so neither a stale opacity nor stale
/// texels can survive the hold.
#[test]
fn a_group_that_comes_and_goes_does_not_rebuild_its_layer() {
    let group = one_group();
    let mut renderer = self::renderer();
    let mut render = layer_allocation_probe();

    // Warm both shapes, so that no later step is the first to allocate an
    // offscreen, a frame buffer or the layer objects themselves.
    render(&mut renderer, &[]);
    render(&mut renderer, &group);
    render(&mut renderer, &[]);
    let settled = render(&mut renderer, &group);

    // The premise: with the group held steady nothing moves, so any delta below
    // is the alternation and not the frame.
    assert_eq!(
        render(&mut renderer, &group),
        settled,
        "a repeated grouped frame must reallocate nothing, or the round trip below measures          something other than the group coming and going",
    );

    // **One idle frame: the hold.** `allocations` is cumulative, so equality
    // here says only that nothing was built — which is the whole claim, since
    // the frame that drops the group never allocated even before the fix. What
    // makes it discriminating is the frame after it.
    let held = render(&mut renderer, &[]);
    assert_eq!(
        held, settled,
        "the frame that drops the group must build nothing",
    );
    assert_eq!(
        render(&mut renderer, &group),
        settled,
        "a group returning after one idle frame must reuse the layer objects held for it: four          per layer — a drawable-sized texture, its view, a uniform buffer and a bind group —          rebuilt on every change is the cost issue #1020 removed for backdrops",
    );

    // **Two idle frames: the release.** Without this half the grace could hold
    // forever and everything above would still pass.
    render(&mut renderer, &[]);
    let released = render(&mut renderer, &[]);
    assert_eq!(
        released, settled,
        "releasing must free rather than allocate",
    );
    assert!(
        render(&mut renderer, &group) > released,
        "after two idle frames the layer objects must have been released, so the next grouped          frame rebuilds them. Equal means they are held past the grace — a drawable-sized          texture per layer kept for a scene that has stopped compositing",
    );
}

/// A node casting `shadows`, with an opaque fill of `fill`.
fn shadowed(
    paints: &mut PaintTable,
    fill: [f32; 3],
    shadows: &[dashpaint::Shadow],
) -> dashpaint::PaintIndex {
    let index = solid(paints, fill[0], fill[1], fill[2]);
    let entry = *paints.resolve(index);
    paints.push_with(
        entry,
        EntryParts {
            shadows,
            ..EntryParts::default()
        },
    )
}

/// One shadow, stated in full so that a fixture never inherits a default it
/// then depends on.
fn shadow(
    kind: dashpaint::ShadowKind,
    offset: (f32, f32),
    blur: f32,
    spread: f32,
    colour: [f32; 4],
) -> dashpaint::Shadow {
    dashpaint::Shadow {
        kind,
        offset: dashpaint::Vec2 {
            x: offset.0,
            y: offset.1,
        },
        blur,
        spread,
        color: Color {
            r: colour[0],
            g: colour[1],
            b: colour[2],
            a: colour[3],
        },
    }
}

/// A drop shadow lands where its offset puts it, behind the node, and nowhere
/// else.
///
/// Blur zero, so the edge is hard and the assertions are about *placement*
/// rather than about falloff — `blurred_rounded_box` degenerates to the
/// unblurred shape at sigma zero and the fixture takes that branch deliberately.
///
/// The offset is asymmetric — right and down by different amounts — because a
/// fixture displaced equally on both axes cannot tell the two components apart,
/// and one that reused the same number for the offset and the box's size could
/// not tell a displacement from a growth.
#[test]
fn a_drop_shadow_lands_at_its_offset_behind_the_node() {
    let mut paints = PaintTable::new();
    let paint = shadowed(
        &mut paints,
        [1.0, 0.0, 0.0],
        &[shadow(
            dashpaint::ShadowKind::Drop,
            (6.0, 10.0),
            0.0,
            0.0,
            [0.0, 0.0, 1.0, 1.0],
        )],
    );
    // The node's box is x 10..30, y 8..24; the shadow's is x 16..36, y 18..34.
    let pixels = draw(
        &[rect(10.0, 8.0, 20.0, 16.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    );

    near(
        texel(&pixels, 33, 30),
        [0, 0, 255, 255],
        "the shadow alone, past the node's box on both axes",
    );
    near(
        texel(&pixels, 20, 15),
        [255, 0, 0, 255],
        "the node's own fill, over the shadow rather than under it",
    );
    near(
        texel(&pixels, 12, 10),
        [255, 0, 0, 255],
        "the corner the shadow was displaced away from is the fill's alone",
    );
    near(
        texel(&pixels, 5, 4),
        [0, 0, 0, 0],
        "outside both boxes nothing is drawn",
    );
    near(
        texel(&pixels, 33, 12),
        [0, 0, 0, 0],
        "and past the shadow's own box on one axis only, likewise",
    );
}

/// A blurred drop shadow ramps rather than stepping, and it stops where the
/// shader's own support ends.
///
/// The claim is the falloff's *shape* at gate resolution: saturated well inside
/// the shadow, about half at its edge — a Gaussian's value at the boundary of
/// the shape it blurs — and nothing at all past three sigma, which is the
/// window `blurred_rounded_box` integrates over and the reach the packer sizes
/// the quad by. **That last probe is what a clipped quad fails**: an instance
/// whose outset did not cover the blur draws a hard cut where this expects a
/// smooth zero, and the cut is inside the picture rather than at its edge.
#[test]
fn a_blurred_drop_shadow_falls_off_and_stops_at_three_sigma() {
    let blur = 8.0;
    let sigma = blur * dashpaint::BLUR_SIGMA_PER_RADIUS;
    let mut paints = PaintTable::new();
    let paint = shadowed(
        &mut paints,
        [1.0, 1.0, 1.0],
        &[shadow(
            dashpaint::ShadowKind::Drop,
            (0.0, 0.0),
            blur,
            0.0,
            [0.0, 0.0, 0.0, 1.0],
        )],
    );
    // A small node, so the shadow's falloff has room on every side: box x
    // 24..40, y 16..32, centred in the 64x48 target.
    let pixels = draw(
        &[rect(24.0, 16.0, 16.0, 16.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    );

    // Straight out from the box's left edge at its vertical centre, so the
    // corner radii and the two axes' falloffs do not interact.
    let left_edge = 24.0f32;
    let alpha_at = |x: u32| texel(&pixels, x, 24)[3];

    let edge = alpha_at(left_edge as u32 - 1);
    assert!(
        (100..=155).contains(&edge),
        "a Gaussian's value at the edge of the shape it blurs is about half, and this was {edge}"
    );
    let further_out = alpha_at(20);
    let deeper_in = alpha_at(27);
    assert!(
        further_out < edge,
        "four units out the shadow has faded further than at the edge: {further_out} against {edge}"
    );
    assert!(
        deeper_in > edge,
        "and three units in it is denser: {deeper_in} against {edge}"
    );
    // The support's own boundary, derived from the constant rather than
    // restated: three sigma out from the box's edge is where
    // `blurred_rounded_box`'s window closes. A literal here would keep passing
    // if the mapping changed and would stop proving anything.
    let support = 3.0 * sigma;
    let past = (left_edge - support - 1.0).floor() as u32;
    let within = (left_edge - support + 2.0).ceil() as u32;
    assert_eq!(
        alpha_at(past),
        0,
        "past three sigma ({past} px) the shader's own window is empty"
    );
    assert!(
        alpha_at(within) > 0,
        "and inside it ({within} px) the shadow is still being drawn, so the probe above \
         is a boundary rather than a region the quad never reached"
    );
}

/// An inner shadow draws inside the node's shape and nowhere outside it, on the
/// side its offset came from.
///
/// The complement of the drop-shadow case, and the one that would go unnoticed
/// if the two shared a coverage: an inner shadow displaced by +x darkens the
/// node's **left** edge, because the lit hole moved right and what is left
/// behind is the shadow.
#[test]
fn an_inner_shadow_draws_inside_the_node_only() {
    let mut paints = PaintTable::new();
    let paint = shadowed(
        &mut paints,
        [1.0, 1.0, 1.0],
        &[shadow(
            dashpaint::ShadowKind::Inner,
            (6.0, 0.0),
            0.0,
            0.0,
            [0.0, 0.0, 1.0, 1.0],
        )],
    );
    // Box x 16..48, y 12..36.
    let pixels = draw(
        &[rect(16.0, 12.0, 32.0, 24.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    );

    near(
        texel(&pixels, 18, 24),
        [0, 0, 255, 255],
        "the band the hole's displacement left behind, inside the node's left edge",
    );
    near(
        texel(&pixels, 30, 24),
        [255, 255, 255, 255],
        "the lit interior, where the hole is",
    );
    // **Half a unit outside the box, not two.** The quad an inner shadow is
    // drawn on is its own bounds plus the antialiasing margin and nothing more,
    // so a probe further out than that is discarded by the geometry whatever the
    // coverage says — and a shadow that had forgotten to clip itself to the
    // node's shape would still pass it. Pixel 15 is inside the quad and outside
    // the shape, which is the only place the clip is observable at all.
    near(
        texel(&pixels, 15, 24),
        [0, 0, 0, 0],
        "an inner shadow is clipped to the node's shape, so it paints nothing \
         just outside it",
    );
    near(
        texel(&pixels, 50, 24),
        [0, 0, 0, 0],
        "and nothing further out either, on the side its offset points at",
    );
}

/// An inner shadow's spread insets its lit hole, so the band thickens on every
/// side rather than on the side an offset points at.
///
/// The offset case above cannot state this: its shadow has no spread, so the
/// hole's *size* is never exercised and a painter that added the spread where it
/// should subtract it draws the same picture. Here the offset is zero and the
/// spread is the only term, which is the other half of the same arithmetic.
#[test]
fn an_inner_shadows_spread_thickens_the_band_on_every_side() {
    let mut paints = PaintTable::new();
    let paint = shadowed(
        &mut paints,
        [1.0, 1.0, 1.0],
        &[shadow(
            dashpaint::ShadowKind::Inner,
            (0.0, 0.0),
            0.0,
            6.0,
            [0.0, 0.0, 1.0, 1.0],
        )],
    );
    // Box x 16..48, y 12..36; the hole insets to x 22..42, y 18..30.
    let pixels = draw(
        &[rect(16.0, 12.0, 32.0, 24.0, paint, ClipIndex::UNCLIPPED)],
        &paints,
        &ClipTable::new(),
    );

    for (x, y, side) in [
        (19, 24, "left"),
        (45, 24, "right"),
        (32, 15, "top"),
        (32, 33, "bottom"),
    ] {
        near(
            texel(&pixels, x, y),
            [0, 0, 255, 255],
            &format!("the spread band on the {side} side"),
        );
    }
    near(
        texel(&pixels, 32, 24),
        [255, 255, 255, 255],
        "and the hole the spread left in the middle",
    );
}

/// Two shadows in one frame draw their own rows.
///
/// **The pixel-side statement of the stride**, and the reason it is worth
/// making twice: `paint_heap`'s unit tests read the words the CPU wrote, and
/// this reads what the fragment stage found at `shadow_base + row *
/// SHADOW_WORDS`. A base or a stride that disagreed with the writer would give
/// the second shadow the first's colour and geometry — a well-formed picture,
/// which no coverage assertion catches.
///
/// The two shadows differ in colour *and* in offset, so a frame that read one
/// row for both fails on where the ink is as well as on what colour it is.
///
/// **A gradient is interned first, deliberately.** Without one the frame's
/// gradient region is empty, so `shadow_base` and `gradient_base` hold the same
/// number and a uniform that carried either would draw the same picture. That
/// is the uniform-environment trap: the fixture would be varied in every field
/// the shadow rows hold and still blind to which base found them.
#[test]
fn two_shadows_in_one_frame_draw_their_own_rows() {
    let mut paints = PaintTable::new();
    paints.intern_fill(&ramp(dashpaint::GradientKind::Linear));
    let first = shadowed(
        &mut paints,
        [1.0, 1.0, 1.0],
        &[shadow(
            dashpaint::ShadowKind::Drop,
            (5.0, 0.0),
            0.0,
            0.0,
            [1.0, 0.0, 0.0, 1.0],
        )],
    );
    let second = shadowed(
        &mut paints,
        [1.0, 1.0, 1.0],
        &[shadow(
            dashpaint::ShadowKind::Drop,
            (0.0, 5.0),
            0.0,
            0.0,
            [0.0, 0.0, 1.0, 1.0],
        )],
    );
    let pixels = draw(
        &[
            rect(4.0, 4.0, 12.0, 12.0, first, ClipIndex::UNCLIPPED),
            rect(36.0, 4.0, 12.0, 12.0, second, ClipIndex::UNCLIPPED),
        ],
        &paints,
        &ClipTable::new(),
    );

    near(
        texel(&pixels, 19, 10),
        [255, 0, 0, 255],
        "the first node's shadow: red, and displaced along x",
    );
    near(
        texel(&pixels, 40, 19),
        [0, 0, 255, 255],
        "the second node's shadow: blue, and displaced along y — its own row",
    );
    near(
        texel(&pixels, 40, 2),
        [0, 0, 0, 0],
        "the second shadow did not take the first's x displacement",
    );
}

/// A shadow's spread grows the shape it casts, and a negative spread shrinks it.
///
/// Stated over a hard-edged shadow at no offset, so the only thing that can move
/// the boundary is the spread itself.
///
/// The node carries **one rounded corner and three sharp ones**, which is what
/// makes the corner rule falsifiable: a spread grows a rounded corner's radius
/// and leaves a sharp corner sharp (CSS's rule, and the reference painter's
/// `spread_corners`). A fixture with four sharp corners would draw the same
/// picture whatever the corner arithmetic did, and a fixture with four equal
/// radii could not tell a per-corner adjustment from a global one.
#[test]
fn a_shadows_spread_grows_the_shape_it_casts() {
    let cast = |spread: f32| {
        let mut paints = PaintTable::new();
        let index = solid(&mut paints, 1.0, 1.0, 1.0);
        let mut entry = *paints.resolve(index);
        entry.corners = CornerRadii {
            top_left: 8.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        };
        let paint = paints.push_with(
            entry,
            EntryParts {
                shadows: &[shadow(
                    dashpaint::ShadowKind::Drop,
                    (0.0, 0.0),
                    0.0,
                    spread,
                    [0.0, 0.0, 1.0, 1.0],
                )],
                ..EntryParts::default()
            },
        );
        // Box x 24..40, y 16..32.
        draw(
            &[rect(24.0, 16.0, 16.0, 16.0, paint, ClipIndex::UNCLIPPED)],
            &paints,
            &ClipTable::new(),
        )
    };

    // Four units left of the box's edge: empty at no spread, shadow at +6.
    let none = cast(0.0);
    near(
        texel(&none, 20, 24),
        [0, 0, 0, 0],
        "with no spread the shadow ends at the node's own edge",
    );
    let grown = cast(6.0);
    near(
        texel(&grown, 20, 24),
        [0, 0, 255, 255],
        "a spread of 6 puts the shadow's edge 6 units out",
    );
    // The corners, at the spread shape's own top-left and top-right. The box
    // grows to x 18..46, y 10..38, so the rounded corner's radius grows from 8
    // to 14 and the sharp one stays at 0.
    //
    // **The probe is chosen to separate the two radii, not merely to be outside
    // the shape.** A sample far into the corner is outside at 8 *and* at 14, and
    // says nothing: it survived exactly that mutation. Pixel (21, 13) is 14.85
    // units from the grown corner's centre and 6.36 from the unspread one, so it
    // is outside the correct shape and inside the one a painter draws if it
    // hands the shadow the node's own radii.
    near(
        texel(&grown, 21, 13),
        [0, 0, 0, 0],
        "the rounded corner grew with the spread; at the node's own radius this \
         sample would be inside the shadow",
    );
    near(
        texel(&grown, 45, 11),
        [0, 0, 255, 255],
        "and the sharp corner stayed sharp — a spread that rounded it would cut \
         this sample away",
    );
    // A negative spread pulls the shadow inside the node, where the node's own
    // opaque fill covers it — so the visible statement is at the node's edge.
    let shrunk = cast(-6.0);
    near(
        texel(&shrunk, 25, 24),
        [255, 255, 255, 255],
        "a spread of -6 pulls the shadow's edge inside the node, which covers it",
    );
}

/// A rotated node draws turned, and a rotation of zero is not what makes that
/// pass (story #832).
///
/// Read as layer 3 always is: "did the pipeline turn the quad at all", never
/// "is the picture right". What it can establish, and what no earlier layer
/// can, is that the vertex stage's rotation term reaches the rasteriser — the
/// packer's rows are asserted bit-exact in layer 1, and a shader that ignored
/// them would still pass every one of those.
///
/// The fixture is a 40 x 10 bar, deliberately not square: a rotation of a
/// rotationally symmetric shape changes no pixel, so a square here would pass
/// against a shader that dropped the term.
#[test]
fn a_rotated_quad_covers_different_pixels_than_an_upright_one() {
    let mut paints = PaintTable::new();
    let solid = paints.push_solid(Color {
        r: 0.9,
        g: 0.2,
        b: 0.1,
        a: 1.0,
    });

    // Centred in the frame and turned about its own centre, so the turned bar
    // stays inside the canvas and the two silhouettes overlap in the middle —
    // the difference is at the ends, which is where a dropped term shows.
    let bar = |rotation: f32| RectEntry {
        x: 12.0,
        y: 19.0,
        w: 40.0,
        h: 10.0,
        paint: solid,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation,
        rotation_anchor: Vec2 { x: 20.0, y: 5.0 },
    };

    let upright = draw(&[bar(0.0)], &paints, &ClipTable::new());
    let turned = draw(
        &[bar(std::f32::consts::FRAC_PI_4)],
        &paints,
        &ClipTable::new(),
    );

    let differing = upright
        .chunks_exact(4)
        .zip(turned.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > 200,
        "a quarter turn of a 40 x 10 bar changed only {differing} pixels: the \
         vertex stage is not applying the rotation term",
    );

    // The bar is 40 wide and 10 tall about its centre at (32, 24), so upright
    // it covers the far left of that row and turned it cannot: at 45 degrees
    // its half-length reaches about 14 pixels along each axis, not 20.
    let far_left = texel(&upright, 13, 24);
    assert!(far_left[3] > 200, "the upright bar covers its own left end");
    let same_point_turned = texel(&turned, 13, 24);
    assert!(
        same_point_turned[3] < 64,
        "the turned bar still covers (13, 24), which is past the reach of its \
         own half-length at 45 degrees: got alpha {}",
        same_point_turned[3],
    );
}

/// A clip does not turn with the node it clips (story #832).
///
/// The reference painter applies an ancestor's clip first and the node's
/// rotation inside it, so the clip stays axis-aligned in document space while
/// the node turns. The shader reaches the same place by testing the clip
/// against the rotated position rather than against the node's own frame —
/// testing it against `local` would rotate the clip along with the node, which
/// is a plausible picture and a wrong one.
#[test]
fn a_clip_does_not_turn_with_the_node_it_clips() {
    let mut paints = PaintTable::new();
    let solid = paints.push_solid(Color {
        r: 0.1,
        g: 0.7,
        b: 0.3,
        a: 1.0,
    });

    // A clip covering the frame's top half only. A node turning inside it must
    // still be cut along y = 24 — a horizontal edge — however far it turns.
    let mut clips = ClipTable::new();
    let clip = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: 24.0,
        corners: CornerRadii::default(),
    }]);

    let turned = RectEntry {
        x: 12.0,
        y: 19.0,
        w: 40.0,
        h: 10.0,
        paint: solid,
        clip,
        opacity: 1.0,
        rotation: std::f32::consts::FRAC_PI_4,
        rotation_anchor: Vec2 { x: 20.0, y: 5.0 },
    };

    let pixels = draw(&[turned], &paints, &clips);

    // Nothing survives below the clip's own horizontal edge, at any column.
    for x in 0..W {
        for y in 25..H {
            assert_eq!(
                texel(&pixels, x, y)[3],
                0,
                "ink at ({x}, {y}) is below the clip's edge at y = 24: the clip \
                 turned with the node instead of staying axis-aligned",
            );
        }
    }
    // And the node does still draw inside the clip, so the assertion above is
    // not passing on an empty frame.
    let inside = (0..24)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| texel(&pixels, x, y)[3] > 200)
        .count();
    assert!(
        inside > 50,
        "the clipped rotated node drew almost nothing ({inside} covered \
         pixels), so the clip assertion above proves nothing",
    );
}
