//! Layer 3 for text and baked vector fields: a glyph run reaches the atlas and
//! the shader resolves the right texels of it, and a coverage mask confines its
//! node's fill (story #582).
//!
//! # Still a gate on the pipeline, not a fidelity check
//!
//! The line story #580 drew and epic #569 insists on. What these establish is
//! that a glyph's quad landed where the run placed it, that it sampled its own
//! rectangle of its own atlas, that the run's colour and alpha reached the
//! pixel, and that a masked node draws the field's silhouette rather than its
//! box. How text *looks* — the MSDF resolve against a real font at a real size —
//! is layer 4's, needs hardware, and is story #586's.
//!
//! # The atlases are baked, and every quadrant of them is different
//!
//! Baked rather than PNG so the bytes in the test are the bytes in the texture:
//! a decode between the two would make a failure ambiguous between the residency
//! path and the decoder, which the PNG arm of `layer3_image_fills` already
//! covers. Baked also exercises more of this story's own code — a baked
//! payload's extent comes from what the caller states, and for a glyph atlas
//! that is `Atlas::width`/`Atlas::height` rather than a header.
//!
//! The fixture is asymmetric on every axis this code can get wrong. Two glyphs
//! sit in **opposite quadrants** of one atlas, so a rectangle taken from the
//! wrong glyph reads texels that are entirely outside the field. Each glyph's
//! own quadrant is half inside and half outside, and the two are split on
//! **different axes** — one left/right, one top/bottom — so a run that sampled
//! the other glyph's rectangle, or transposed a coordinate, draws a different
//! picture rather than the same one. `atlas_px` has a bottom-left origin and
//! the instance carries a top-left one, so a dropped flip reads a zero
//! quadrant.

use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, ClipIndex, ClipTable, Color, EntryParts, FillSpec, GlyphQuad,
    GlyphRange, GlyphRun, GlyphRunTable, ImageAsset, ImageFill, ImageFormat, ImageTable, Mat23,
    PaintEntry, PaintTable, Painter, RectEntry, ScaleMode, Vec2, VectorField,
};
use dashscene_gpu::{GpuPainter, ResidencyError};

mod common;
use common::{H, JPEG_FIXTURE, W, renderer, texel};

/// The atlas is eight texels square and its glyphs are four.
const ATLAS: u32 = 8;

// Both fixtures carry the same value in all three colour channels, so `median3`
// is that value and a fixture states a coverage rather than a channel
// arrangement. What is under test here is where a sample lands, not how three
// channels reconcile; that is `sdf.wgsl`'s own function and layer 2 measures it.

/// The value a texel carries when it is neither fully inside nor fully outside.
///
/// `153 / 255` is `0.6`, so the resolve's signed distance is `0.1` and the
/// coverage it produces is `0.1 * px_range + 0.5` — a number that **changes
/// with `px_range`**. Every other texel in these fixtures saturates, and a
/// saturated probe cannot fail a wrong range: it is `1.0` for any range above
/// one and `0.0` for any range at all. This is the texel that pins it.
const GRADED: u8 = 153;

/// What a `GRADED` texel resolves to at this fixture's range, as an alpha.
///
/// `px_range` is `distance_range * size / px_per_em` for a glyph and
/// `distance_range * quad_width / atlas_width` for a field; both fixtures are
/// built so that it is `2.5`, giving `0.1 * 2.5 + 0.5 = 0.75`.
const GRADED_ALPHA: u8 = 191;

/// The glyph fixture's field: three quadrants, split on different axes.
///
/// - The **top-left** quadrant is glyph 11's, and its left half is inside.
/// - The **bottom-right** quadrant is glyph 23's, and its bottom half is inside.
/// - The **top-right** quadrant is glyph 31's, and it is uniformly [`GRADED`].
/// - The bottom-left quadrant is entirely outside, so any sample that strays
///   into it draws nothing at all.
fn glyph_field() -> ImageAsset {
    let mut bytes = Vec::with_capacity((ATLAS * ATLAS * 4) as usize);
    for y in 0..ATLAS {
        for x in 0..ATLAS {
            let v = if x >= 4 && y < 4 {
                GRADED
            } else if (x < 4 && y < 4 && x < 2) || (x >= 4 && y >= 4 && y >= 6) {
                255
            } else {
                0
            };
            bytes.extend_from_slice(&[v, v, v, 255]);
        }
    }
    ImageAsset {
        format: ImageFormat::Rgba8Unorm,
        bytes,
    }
}

/// An atlas over [`glyph_field`]: three glyphs one em square, and one with no
/// plane quad at all.
///
/// `plane_em` is `[left, bottom, right, top]` y-up from the baseline, so a
/// one-em square glyph is `[0, 0, 1, 1]` and its quad's top sits a full size
/// above the pen. `atlas_px` is `[left, bottom, right, top]` with a bottom-left
/// origin: glyph 11's `[0, 4, 4, 8]` is the **top** half of the image and glyph
/// 23's `[4, 0, 8, 4]` is the bottom half.
fn glyph_atlas() -> Atlas {
    Atlas::new(
        glyph_field(),
        ATLAS,
        ATLAS,
        // One em is four texels — the size a glyph occupies here — so a run at
        // size 20 renders each texel five device pixels wide.
        4,
        // Chosen so that a run at size 20 resolves `px_range` to 2.5. The
        // saturated texels are still saturated at that range, and the `GRADED`
        // ones land at a value only this range produces — which is what makes
        // the range itself falsifiable.
        0.5,
        vec![
            AtlasGlyph {
                glyph_id: 11,
                plane_em: [0.0, 0.0, 1.0, 1.0],
                atlas_px: [0.0, 4.0, 4.0, 8.0],
            },
            AtlasGlyph {
                glyph_id: 23,
                plane_em: [0.0, 0.0, 1.0, 1.0],
                atlas_px: [4.0, 0.0, 8.0, 4.0],
            },
            // The graded quadrant, at the image's top-right — which is
            // `atlas_px`'s bottom-left `[4, 4, 8, 8]`.
            AtlasGlyph {
                glyph_id: 31,
                plane_em: [0.0, 0.0, 1.0, 1.0],
                atlas_px: [4.0, 4.0, 8.0, 8.0],
            },
            // Glyph 31's twin with **no plane quad at all** (issue #1185).
            // `Atlas::new` does not refuse it, so `pack_run` emits an
            // instance whose `bounds` extent is `(right - left) * size` — zero —
            // and `msdf_sample` divides by it. It samples the same uniformly
            // graded quadrant, so if it draws anything at all it draws a
            // definite alpha rather than an accident of which edge the clamp
            // took. Named by only one test; the others address ids by number.
            AtlasGlyph {
                glyph_id: 41,
                plane_em: [0.0, 0.0, 0.0, 0.0],
                atlas_px: [4.0, 4.0, 8.0, 8.0],
            },
        ],
    )
    .expect("a test atlas states a non-zero px_per_em")
}

/// A scene of one unpainted rect with `runs` anchored to it, rendered.
///
/// The rect covers the canvas and draws nothing of its own, so every pixel the
/// result carries came from a glyph.
fn draw_runs(runs: &[(GlyphRun, Vec<GlyphQuad>)]) -> (Vec<u8>, usize) {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry::default());
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: H as f32,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];

    let mut glyphs = GlyphRunTable::new();
    glyphs.push_atlas(glyph_atlas());
    for (run, quads) in runs {
        glyphs.push_run(*run, quads);
    }

    let clips = ClipTable::new();
    let images = ImageTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    let mut renderer = renderer();
    let pixels = renderer
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum");
    (pixels, renderer.last_draw_runs())
}

/// One run of one glyph, at `size`, with its pen at `(x, y)`.
fn one_glyph(
    glyph_id: u32,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
    opacity: f32,
) -> (GlyphRun, Vec<GlyphQuad>) {
    (
        GlyphRun {
            rect: 0,
            atlas: AtlasIndex(0),
            size,
            color,
            glyphs: GlyphRange::UNASSIGNED,
            opacity,
        },
        vec![GlyphQuad { glyph_id, x, y }],
    )
}

/// A glyph draws the run's colour where its own field is inside, and nothing
/// where it is outside.
///
/// Glyph 11's quadrant is inside on its **left** half. The quad is twenty units
/// square with its pen at `(10, 30)`, so it covers `x` 10..30 and `y` 10..30,
/// and each atlas texel is five device pixels wide. The probes sit on texel
/// centres rather than on the boundary between two: what happens at the
/// boundary is the resolve's ramp, which layer 2 measures.
#[test]
fn a_glyph_paints_its_runs_colour_where_its_own_field_is_inside() {
    let ink = Color {
        r: 0.2,
        g: 0.6,
        b: 1.0,
        a: 1.0,
    };
    let (pixels, runs) = draw_runs(&[one_glyph(11, 10.0, 30.0, 20.0, ink, 1.0)]);

    // Inside: the first and second texel columns of the glyph, at device x 12
    // and 17.
    for x in [12, 17] {
        let [r, g, b, a] = texel(&pixels, x, 20);
        assert_eq!(a, 255, "the glyph's left half is opaque at x {x}");
        assert!(
            r.abs_diff(51) <= 2 && g.abs_diff(153) <= 2 && b == 255,
            "the pixel is the run's own colour at x {x}: {:?}",
            [r, g, b, a]
        );
    }
    // Outside: the third and fourth texel columns, at device x 22 and 27.
    for x in [22, 27] {
        assert_eq!(
            texel(&pixels, x, 20)[3],
            0,
            "the glyph's right half is outside its field, at x {x}"
        );
    }
    // And nothing outside the quad at all.
    assert_eq!(texel(&pixels, 5, 20)[3], 0, "left of the glyph's quad");
    assert_eq!(texel(&pixels, 12, 5)[3], 0, "above the glyph's quad");
    assert_eq!(texel(&pixels, 12, 35)[3], 0, "below the glyph's quad");
    assert_eq!(runs, 1, "one atlas is one draw call");
}

/// The other glyph of the same atlas reads its own rectangle, and that rectangle
/// is split on the other axis.
///
/// This is the assertion the fixture's opposite quadrants exist for. Glyph 23's
/// quadrant is inside on its **bottom** half, so a run that sampled glyph 11's
/// rectangle would paint a left half; one that sampled a quadrant neither glyph
/// owns would paint nothing; and one that dropped `atlas_px`'s bottom-left
/// origin would read the top half of the image, which is entirely outside for
/// these columns.
#[test]
fn the_other_glyph_of_one_atlas_reads_its_own_rectangle() {
    let ink = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    let (pixels, _) = draw_runs(&[one_glyph(23, 20.0, 30.0, 20.0, ink, 1.0)]);

    // The quad covers x 20..40 and y 10..30, five device pixels per texel.
    // Inside: the bottom two texel rows, at device y 22 and 27.
    for y in [22, 27] {
        assert_eq!(
            texel(&pixels, 30, y)[3],
            255,
            "glyph 23's bottom half is inside its field, at y {y}"
        );
    }
    // Outside: the top two texel rows, at device y 12 and 17.
    for y in [12, 17] {
        assert_eq!(
            texel(&pixels, 30, y)[3],
            0,
            "glyph 23's top half is outside its field, at y {y}"
        );
    }
    // The split is horizontal, not vertical: both halves of the *width* agree,
    // which is what tells this glyph's rectangle from the other's.
    assert_eq!(texel(&pixels, 22, 27)[3], 255, "left of the bottom half");
    assert_eq!(texel(&pixels, 37, 27)[3], 255, "right of the bottom half");
}

/// A run's `opacity` multiplies its fill's alpha.
///
/// The free-path group alpha, which the reference painter folds into the colour
/// before it packs its uniforms. Here it rides on `Instance::opacity` and the
/// shader multiplies it into the coverage, which is the same product.
#[test]
fn a_runs_opacity_multiplies_its_fills_alpha() {
    let ink = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 0.8,
    };
    let (pixels, _) = draw_runs(&[one_glyph(11, 10.0, 30.0, 20.0, ink, 0.5)]);
    let alpha = texel(&pixels, 12, 20)[3];
    // 0.8 * 0.5 = 0.4 of 255, within the rounding an eight-bit target allows.
    assert!(
        alpha.abs_diff(102) <= 2,
        "the run's alpha and its opacity multiply: {alpha}"
    );
}

/// Two runs of different sizes each resolve their own screen-pixel range, and
/// the larger one covers more pixels.
///
/// The one property `px_range` being per run rather than per frame is stated
/// over: a painter that took one run's range for both would still draw both
/// glyphs, and only the edge would be wrong.
#[test]
fn two_runs_at_different_sizes_each_draw_their_own_quad() {
    let ink = Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    let (pixels, _) = draw_runs(&[
        one_glyph(11, 4.0, 20.0, 16.0, ink, 1.0),
        one_glyph(11, 36.0, 44.0, 8.0, ink, 1.0),
    ]);
    // The first quad covers x 4..20, y 4..20; its left half is inside.
    assert_eq!(texel(&pixels, 6, 12)[3], 255, "the large run's left half");
    assert_eq!(texel(&pixels, 18, 12)[3], 0, "the large run's right half");
    // The second covers x 36..44, y 36..44 — a quarter of the area.
    assert_eq!(texel(&pixels, 37, 40)[3], 255, "the small run's left half");
    assert_eq!(texel(&pixels, 43, 40)[3], 0, "the small run's right half");
}

// ---------------------------------------------------------------------------
// Baked vector fields
// ---------------------------------------------------------------------------

/// Where the mask fixture's field sits inside its atlas: **not at the origin**.
///
/// `VectorField::atlas_rect` is `[x, y, w, h]` with a **top-left** origin,
/// unlike a glyph's `atlas_px`. A field at `[0, 0, ...]` cannot fail an
/// implementation that drops the origin term, which is the range-offset trap
/// issues #650, #651, #561, #688 and #699 all record — and which a first
/// version of this fixture walked into, caught by mutation rather than by
/// review. Both coordinates are non-zero and they differ from each other, so a
/// transposed origin fails too.
const MASK_RECT: [u32; 4] = [3, 2, 4, 4];

/// The mask fixture's field, over [`MASK_RECT`]: two inside columns, one
/// [`GRADED`], one outside.
///
/// Everything outside that rectangle is outside the field, so a mask that read
/// the wrong sub-rect draws nothing at all.
fn mask_field() -> ImageAsset {
    let [rx, ry, rw, rh] = MASK_RECT;
    let mut bytes = Vec::with_capacity((ATLAS * ATLAS * 4) as usize);
    for y in 0..ATLAS {
        for x in 0..ATLAS {
            let inside_rect = x >= rx && x < rx + rw && y >= ry && y < ry + rh;
            // Derived from the rectangle rather than restated, so that moving
            // it moves the field with it and the fixture cannot come apart.
            let v = match x.checked_sub(rx) {
                Some(0 | 1) if inside_rect => 255,
                Some(2) if inside_rect => GRADED,
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

/// A node whose solid fill is masked by a coverage field narrower than its own
/// box, rendered.
fn draw_masked_node() -> Vec<u8> {
    let mut images = ImageTable::new();
    let atlas = images.push_baked(mask_field(), ATLAS, ATLAS);

    let mut paints = PaintTable::new();
    let ink = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 1.0,
            g: 0.5,
            b: 0.0,
            a: 1.0,
        },
    });
    let paint = paints.push_with(
        PaintEntry {
            fill: ink,
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(VectorField {
                image: atlas,
                atlas_rect: MASK_RECT,
                // The field's quad is **narrower than the node's box** — 20
                // units of a 40-unit rect — which is what makes "the mask
                // confines the fill" a falsifiable claim. A painter that drew
                // the parametric box would cover the whole 40.
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                // 20 device units over 4 atlas texels is five pixels a texel,
                // so this resolves `px_range` to 2.5 — the same number the
                // glyph fixture reaches, and what `GRADED_ALPHA` is stated at.
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );

    let rects = vec![RectEntry {
        x: 8.0,
        y: 8.0,
        w: 40.0,
        h: 32.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
    let clips = ClipTable::new();
    let glyphs = GlyphRunTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    renderer()
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum")
}

/// A masked node draws the field's silhouette, and nothing outside it.
///
/// Three claims, and the third is the one this story exists for. Before it, a
/// masked solid fill drew as an ordinary rounded rectangle over the node's whole
/// box: the packer set `Instance::shape` and the shader never read it. So the
/// probe at x 40 — inside the node's box, outside the field's quad — is what
/// separates a painter that applies the mask from one that ignores it.
#[test]
fn a_coverage_mask_confines_its_nodes_fill_to_the_fields_quad() {
    let pixels = draw_masked_node();

    // The field's quad is x 8..28, y 8..40, at five device pixels per texel
    // across and eight down. Its left half is inside.
    for x in [10, 15] {
        let [r, g, b, a] = texel(&pixels, x, 24);
        assert_eq!(a, 255, "the field's left half is opaque at x {x}");
        assert!(
            r == 255 && g.abs_diff(128) <= 2 && b == 0,
            "the pixel is the node's own fill at x {x}: {:?}",
            [r, g, b, a]
        );
    }
    // Its third column is graded, and the alpha it lands on is a function of
    // the field's screen-pixel range — the one probe here that a wrong range
    // changes. Every other one saturates.
    let graded = texel(&pixels, 20, 24)[3];
    assert!(
        graded.abs_diff(GRADED_ALPHA) <= 2,
        "the field's graded column resolves at its own px_range: {graded}"
    );
    // Its fourth is outside.
    assert_eq!(
        texel(&pixels, 25, 24)[3],
        0,
        "the field's last column is outside it"
    );
    // And past the field's quad entirely, while still inside the node's box.
    // This is the probe that fails against a painter which draws the box.
    for x in [32, 40] {
        assert_eq!(
            texel(&pixels, x, 24)[3],
            0,
            "x {x} is inside the node's 40-unit box and outside the field's 20-unit quad"
        );
    }
}

/// The mask's quad is placed relative to the node's origin, not to the canvas.
///
/// `plane_bounds` is node-box-relative by
/// `docs/decisions/baked-vector-msdf-field.md`, so moving the node moves the
/// coverage with it. A painter that read the plane as absolute would draw the
/// same pixels for both nodes.
#[test]
fn the_masks_quad_follows_its_nodes_origin() {
    let pixels = draw_masked_node();
    // The node's origin is (8, 8), so the field starts there and not at zero.
    assert_eq!(
        texel(&pixels, 10, 24)[3],
        255,
        "just inside the node's origin"
    );
    assert_eq!(texel(&pixels, 3, 24)[3], 0, "left of the node's origin");
    assert_eq!(texel(&pixels, 10, 4)[3], 0, "above the node's origin");
    // The quad is 32 units tall from y 8, so y 42 is past its bottom.
    assert_eq!(texel(&pixels, 10, 44)[3], 0, "below the field's quad");
}

/// A glyph whose field is neither inside nor outside resolves at its run's own
/// screen-pixel range.
///
/// The only probe in this file that a wrong `px_range` moves. Every other texel
/// saturates: `clamp` puts a fully-inside sample at 1 for any range above one
/// and a fully-outside sample at 0 for any range at all, so a painter that took
/// the range from the wrong atlas, dropped the run's size, or divided by the
/// wrong `px_per_em` would pass every one of them.
#[test]
fn a_graded_sample_resolves_at_the_runs_own_screen_pixel_range() {
    let ink = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    let (pixels, _) = draw_runs(&[one_glyph(31, 10.0, 30.0, 20.0, ink, 1.0)]);
    // px_range = distance_range_px * size / px_per_em = 0.5 * 20 / 4 = 2.5, and
    // the quadrant is uniformly GRADED, so every probe inside the quad agrees.
    for (x, y) in [(12, 20), (22, 15), (27, 27)] {
        let alpha = texel(&pixels, x, y)[3];
        assert!(
            alpha.abs_diff(GRADED_ALPHA) <= 2,
            "a graded texel resolves to {GRADED_ALPHA}, not {alpha}, at ({x}, {y})"
        );
    }
}

/// A glyph atlas and an image payload of the same shape are two residency
/// entries, not one.
///
/// # The collision this exists for
///
/// A glyph atlas does not live in the `ImageTable` — `dashpaint::Atlas` owns its
/// payload, because a run's glyph ids are meaningless without the atlas that
/// places them. So there are two index spaces, and index 0 of each is an
/// ordinary value in both. Everything else `PayloadKey` carries is the format,
/// the pool offset and the length, and this fixture makes all three agree: two
/// eight-by-eight `Rgba8Unorm` payloads are 256 bytes each at offset 0.
///
/// The image fill is packed before the glyph, so without a discriminator the
/// glyph asks for a key the image already holds, gets the image's rectangle back
/// and samples the photograph as a distance field. The image payload's colour
/// channels are all zero, which resolves as *fully outside*, so the glyph would
/// draw nothing at all — a picture missing its text, with no error anywhere in a
/// release build. In a debug build the residency digest catches it first, which
/// is what that assertion is for.
#[test]
fn a_glyph_atlas_and_an_image_row_of_the_same_shape_do_not_collide() {
    // A payload the same size and format as the glyph atlas, and black: its
    // colour channels are zero, so read as a distance field it is entirely
    // outside.
    let mut opaque_black = Vec::with_capacity((ATLAS * ATLAS * 4) as usize);
    for _ in 0..ATLAS * ATLAS {
        opaque_black.extend_from_slice(&[0, 0, 0, 255]);
    }
    let mut images = ImageTable::new();
    let index = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: opaque_black,
        },
        ATLAS,
        ATLAS,
    );
    assert_eq!(index, 0, "the image row and the atlas share an index");

    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: index,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let paint = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: H as f32,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];

    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(glyph_atlas());
    assert_eq!(atlas.0, 0, "the atlas and the image row share an index");
    let (run, quads) = one_glyph(
        11,
        10.0,
        30.0,
        20.0,
        Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
        1.0,
    );
    glyphs.push_run(run, &quads);

    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    let pixels = renderer()
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum");

    // The glyph still reads its own atlas: green where its field is inside.
    let [r, g, b, a] = texel(&pixels, 12, 20);
    assert_eq!(
        [r, g, b, a],
        [0, 255, 0, 255],
        "the glyph must sample its own atlas, not the image row that keys the same"
    );
    // And where its field is outside, the image beneath it shows through as the
    // black it is — so the probe above is not simply everything being green.
    assert_eq!(
        texel(&pixels, 27, 20),
        [0, 0, 0, 255],
        "the image fill is still drawn, and the glyph's right half is outside its field"
    );
}

/// A node whose **gradient** fill is masked by the same coverage field, drawn
/// over the same 40x32 box at (8, 8).
///
/// The gradient's handles are the identity frame over the node's box, so `t` is
/// the box's own normalised x. Two stops spanning the full range, so every
/// probe below is inside the single segment and its colour is a linear function
/// of `t` alone.
fn draw_masked_gradient_node() -> Vec<u8> {
    let mut images = ImageTable::new();
    let atlas = images.push_baked(mask_field(), ATLAS, ATLAS);

    let mut paints = PaintTable::new();
    let ink = paints.intern_fill(&FillSpec::Gradient {
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
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            },
            dashpaint::GradientStop {
                offset: 1.0,
                color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            },
        ],
    });
    let paint = paints.push_with(
        PaintEntry {
            fill: ink,
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(VectorField {
                image: atlas,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );

    let rects = vec![RectEntry {
        x: 8.0,
        y: 8.0,
        w: 40.0,
        h: 32.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
    let clips = ClipTable::new();
    let glyphs = GlyphRunTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    renderer()
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum")
}

/// A masked gradient fill draws, and its frame is the **node's box** rather than
/// the field's quad.
///
/// This is the one combination that needed both halves of the slice: story #582
/// resolved the mask and its coverage, and issue #715 the colour it modulates.
/// Until #715 it drew nothing at all.
///
/// # Why the frame is the falsifiable part
///
/// The node's box is 40 units wide and the field's quad is 20, so the two frames
/// disagree by a factor of two — and a masked instance's *quad* is the field's,
/// which makes taking the frame from the quad the natural mistake. At x = 10 the
/// fragment sits at 10.5, so the box's own `t` is `(10.5 - 8) / 40 = 0.0625` and
/// the ramp gives `[239, 0, 16]`; the field's quad would give `t = 0.125` and
/// `[223, 0, 32]`, sixteen code points away on two channels. `dashscene-skia`
/// builds its gradient frame from the entry's box for a masked node exactly as
/// it does for an unmasked one, so this is the reference painter's answer and
/// not a choice made here.
#[test]
fn a_masked_gradient_takes_its_frame_from_the_node_box_and_its_coverage_from_the_field() {
    let pixels = draw_masked_gradient_node();

    // Inside the field, at two columns whose colours differ — which is what a
    // solid fill could not produce and what says the ramp reached the shader.
    for (x, expected) in [(10u32, [239u8, 0, 16]), (15, [207, 0, 48])] {
        let [r, g, b, a] = texel(&pixels, x, 24);
        assert_eq!(a, 255, "the field's left half is opaque at x {x}");
        for (channel, (got, want)) in [r, g, b].into_iter().zip(expected).enumerate() {
            assert!(
                got.abs_diff(want) <= 2,
                "at x {x} channel {channel} was {got} and should be about {want} \
                 (whole texel {:?}) — a frame taken from the field's quad instead of the \
                 node's box reads twice as far along the ramp",
                [r, g, b, a]
            );
        }
    }

    // Outside the field's quad but inside the node's box: the mask still
    // confines the fill, which a gradient must not have loosened.
    assert_eq!(
        texel(&pixels, 40, 24)[3],
        0,
        "x 40 is inside the node's box and outside the field's quad"
    );
    assert_eq!(texel(&pixels, 2, 24)[3], 0, "x 2 is outside both");
}

/// **A refused glyph atlas draws nothing and names itself** (issue #973).
///
/// The refusal path had one end-to-end test and it was over an image fill, which
/// reaches neither of the other two consumers' shader code. A glyph atlas is the
/// one the widening was written for: one sheet for a whole script at a whole
/// weight, so a CJK coverage set is exactly the payload that outgrows a device,
/// where an oversized photograph has to be authored deliberately.
///
/// What the whole-canvas sweep pins is that the run does not paint. **Since
/// issue #993 that is a mechanism**: `GpuMsdfRow::resolved` is zero on the
/// row a refusal leaves, the vertex stage carries it into `params2.w`, and the
/// `KIND_TEXT` arm leaves the coverage at zero without it — so the fragment is
/// discarded before any colour is read.
///
/// Until then it was an outcome, supported by **three** unrelated things and none
/// of them about refusal:
///
/// 1. The resolved row stays at `GpuGlyphRun::default()`, so `px_range` is zero
///    and `msdf_coverage(sample, 0)` is exactly `0.5` for any sample. The
///    coverage clears the `cover <= 0.0` discard, so the fragment **was**
///    shaded.
/// 2. The same default leaves `run.color` at zero alpha, and the `KIND_TEXT`
///    arm has no `discard` of its own — unlike the image-fill arm beside it —
///    so what the fragment returned was `vec4f(0, 0, 0, 0)`.
/// 3. Writing that changes nothing only because the pipeline blends
///    premultiplied.
///
/// The ground rect is opaque for the third: over a transparent canvas the sweep
/// compares `[0, 0, 0, 0]` against `[0, 0, 0, 0]` and cannot see a blend state
/// that stopped treating a zero write as a no-op. **The gate retired that
/// reason** — a refused run is discarded and reaches no blender at all, so the
/// opaque ground no longer guards the blend state on this path. It is kept
/// because it costs nothing and because it is what makes the sweep able to
/// distinguish a drawn glyph from an absent one at all.
///
/// **This test cannot fail on the gate alone**, and that is why the mechanism
/// has one: deleting it restores the coincidence above and changes no rendered
/// texel, measured. `render::tests::both_msdf_arms_gate_on_the_row_the_frame_resolved`
/// is what fails there. What this test does catch is the pairing — with an
/// opaque colour written beside the refusal, the gate is the only thing left
/// keeping the canvas clean, which is the mutation issue #993 measured at
/// `[255, 255, 255, 128]`.
#[test]
fn a_refused_glyph_atlas_draws_nothing_and_names_itself() {
    // The passing fixture's atlas, with one thing changed: its payload is a
    // container this painter cannot decode. The extent is stated by the
    // `Atlas`, not read from the payload, so `resolve_frame`'s zero-extent arm
    // does not answer first and the refusal is what is left.
    let atlas = Atlas::new(
        ImageAsset {
            format: ImageFormat::Jpeg,
            bytes: JPEG_FIXTURE.to_vec(),
        },
        ATLAS,
        ATLAS,
        4,
        0.5,
        vec![AtlasGlyph {
            glyph_id: 11,
            plane_em: [0.0, 0.0, 1.0, 1.0],
            atlas_px: [0.0, 4.0, 4.0, 8.0],
        }],
    )
    .expect("a test atlas states a non-zero px_per_em");

    // **Opaque, not the empty entry the drawn glyph tests use.** Over a
    // transparent canvas every texel is already `[0, 0, 0, 0]`, so a sweep
    // cannot tell a run that was never drawn from one that wrote premultiplied
    // zero over nothing — and the second of those is what actually happens
    // here, because the refused row's coverage is `0.5` and only its zero alpha
    // makes the write a no-op. That the write disappears is a property of the
    // pipeline's premultiplied-alpha blend, which is a third thing holding this
    // outcome up and which no transparent canvas can see.
    let mut paints = PaintTable::new();
    let ground = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 0.0,
            g: 0.5,
            b: 0.25,
            a: 1.0,
        },
    });
    let paint = paints.push(PaintEntry {
        fill: ground,
        ..PaintEntry::default()
    });
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: H as f32,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];

    let mut glyphs = GlyphRunTable::new();
    glyphs.push_atlas(atlas);
    // Two runs naming the one refused atlas, because one refused payload must be
    // one refusal: `resolve_frame`'s memo arrays record only what *resolved*, so
    // a refused atlas stays unresolved and every further run asks again. With a
    // single run this test would pass whether or not the dedup existed.
    let ink = Color {
        r: 0.2,
        g: 0.6,
        b: 1.0,
        a: 1.0,
    };
    for (run, quads) in [
        one_glyph(11, 10.0, 30.0, 20.0, ink, 1.0),
        one_glyph(11, 34.0, 30.0, 20.0, ink, 1.0),
    ] {
        glyphs.push_run(run, &quads);
    }

    let clips = ClipTable::new();
    let images = ImageTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    let mut renderer = renderer();
    let pixels = renderer
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum");

    // Named, not silent — the half P4 is about.
    let refusals = renderer.refusals();
    assert_eq!(
        refusals.len(),
        1,
        "two runs name one refused atlas, so one refusal — got {refusals:?}",
    );
    assert_eq!(
        refusals[0].what, "a glyph atlas",
        "the consumer is named: a glyph atlas and an image fill are refused by the same call and \
         a host can do nothing with either without knowing which it was",
    );
    assert_eq!(refusals[0].row, 0, "the atlas row, not the run's");
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
    );
    assert_eq!(renderer.refusals_seen(), 1);

    // And nothing drawn, rather than a plausible wrong picture.
    // The ground, everywhere: the runs left it exactly as the rect drew it.
    let ground = texel(&pixels, 0, 0);
    assert_eq!(
        ground,
        [0, 128, 64, 255],
        "the ground rect is what every texel should still be",
    );
    for y in 0..H {
        for x in 0..W {
            assert_eq!(
                texel(&pixels, x, y),
                ground,
                "a refused glyph atlas painted ({x}, {y})"
            );
        }
    }
}

/// **A masked fill whose coverage field was refused draws nothing** (issues
/// #972, #973).
///
/// The fill half of the same defect the backdrop carries. `paint.wgsl`'s vertex
/// stage says a zeroed field row leaves a quad with no area, and that is true of
/// the quad and not of what is drawn: the quad is then grown by the
/// antialiasing width, and the fragment stage's masked arm computed
/// `msdf_coverage(sample, px_range = 0)`, which is `0.5` whatever the sample
/// was. The coverage survived the `discard` and the node's own colour landed at
/// half alpha in a small square at its top-left corner.
///
/// **The gate is what empties it, on this arm and on the text arm alike.** Until
/// issue #993 that was only true here, and a glyph run reached the same half
/// coverage and painted nothing for an unrelated reason — its zero-alpha
/// default. `a_refused_glyph_atlas_draws_nothing_and_names_itself` above is
/// where that is written up; both arms now read `params2.w`.
#[test]
fn a_refused_coverage_field_draws_no_masked_fill() {
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
    // `draw_masked_node`'s fixture with one thing changed: the field's payload
    // cannot be decoded. So that test's picture — an opaque orange silhouette —
    // is what this one would draw if the refusal were not honoured.
    let ink = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 1.0,
            g: 0.5,
            b: 0.0,
            a: 1.0,
        },
    });
    let paint = paints.push_with(
        PaintEntry {
            fill: ink,
            ..PaintEntry::default()
        },
        EntryParts {
            shape: Some(VectorField {
                image: field,
                atlas_rect: MASK_RECT,
                plane_bounds: [0.0, 0.0, 20.0, 32.0],
                distance_range: 0.5,
            }),
            ..EntryParts::default()
        },
    );
    // Two nodes naming the one refused field, because one refused payload must
    // be one refusal. `resolve_frame`'s memo array records only what *resolved*,
    // so a refused field stays unresolved and the second node asks again; the
    // dedup that answers is keyed on the consumer and the row, and this is the
    // only test that exercises it for a field rather than an image fill. With a
    // single node the count below would hold whether or not it existed.
    let rects: Vec<RectEntry> = (0..2)
        .map(|i| RectEntry {
            x: 8.0,
            y: 8.0 + i as f32 * 4.0,
            w: 40.0,
            h: 32.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        })
        .collect();

    let clips = ClipTable::new();
    let glyphs = GlyphRunTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(&rects, &paints, &images, &clips, &[], &glyphs, None);
    let mut renderer = renderer();
    let pixels = renderer
        .render(painter.instances(), &paints, &images, &clips, &glyphs, W, H)
        .expect("the fixture extent is within any device's maximum");

    let refusals = renderer.refusals();
    assert_eq!(
        refusals.len(),
        1,
        "two nodes name one refused field, so one refusal — got {refusals:?}",
    );
    assert_eq!(refusals[0].what, "a vector field's atlas");
    assert_eq!(refusals[0].row, field);
    assert_eq!(
        refusals[0].error,
        ResidencyError::NoDecoder {
            format: ImageFormat::Jpeg
        },
    );

    for y in 0..H {
        for x in 0..W {
            assert_eq!(
                texel(&pixels, x, y),
                [0, 0, 0, 0],
                "a refused coverage field painted ({x}, {y})"
            );
        }
    }
}

/// **A glyph whose quad has no area draws nothing** (issue #1185).
///
/// `msdf_sample` divides by the quad's extent, and until #1185 nothing refused
/// a zero one. `pack_run` builds a glyph's `bounds` extent as
/// `(right - left) * size` from the atlas row's own `plane_em`, and `Atlas::new`
/// does not refuse a row whose plane quad has no area — so `t` divided by zero,
/// the clamp took the sub-rect's edge, and the margin around a zero-area quad
/// was shaded at whatever coverage that edge resolved to. The run's colour in a
/// small square where the glyph is not.
///
/// **Both ways in one test**, which is what makes the zero mean anything: the
/// same run at the same pen with glyph 31, whose plane quad is one em square
/// over the same uniformly graded texels, draws. A test asserting only the
/// empty frame would pass against a painter that stopped drawing text.
///
/// # This is the reachable half of #1185, not the one it was filed for
///
/// The issue is about the **masked** arm, whose quad is `hi - lo` with the node
/// origin added to both ends, and that route is not observable on this
/// machine's adapter. Measured against `paint.wgsl` on Metal, with a node at
/// `x = 32.0` and a `1e-6`-unit plane: had the shader evaluated
/// `(32.0 + 1e-6) - (32.0 + 0.0)` in `f32` the extent would be `0.0` on x and
/// one ulp of 16 — `1.9073486e-6` — on y, and it is exactly `1e-6` on both, so
/// the backend reassociates the origin out of the subtraction and no
/// cancellation occurs. The guard is in the vertex stage for both arms because
/// the property should hold in the source rather than depend on that; what
/// **this** test can reach is the arm whose zero comes from the CPU.
#[test]
fn a_glyph_whose_quad_has_no_area_draws_nothing() {
    let ink = Color {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    // Same pen, same size, same texels — one em of plane quad apart.
    let at_pen = |glyph_id| draw_runs(&[one_glyph(glyph_id, 30.0, 20.0, 20.0, ink, 1.0)]).0;

    let empty = at_pen(41);
    let painted: Vec<(u32, u32)> = (0..H)
        .flat_map(|y| (0..W).map(move |x| (x, y)))
        .filter(|&(x, y)| texel(&empty, x, y)[3] != 0)
        .collect();
    assert!(
        painted.is_empty(),
        "a glyph with no plane quad painted {} pixels, the first at {:?}",
        painted.len(),
        painted.first()
    );

    // Glyph 31 is its twin over the same texels with a one-em plane quad. Its
    // device quad is x 30..50, y 0..20, uniformly graded, so it resolves at the
    // same alpha every other graded probe in this file does.
    let drawn = at_pen(31);
    assert_eq!(
        texel(&drawn, 35, 10)[3],
        GRADED_ALPHA,
        "a glyph with a one-em plane quad must still draw, or the assertion above says nothing"
    );
}

/// **A refused glyph run is submitted as no instances at all** (issue #1024),
/// and that is visible from outside the crate as a split draw.
///
/// The correctness fix for a refused atlas was `GpuMsdfRow::resolved` (issue
/// #993), which makes the fragment stage discard. The quads were still drawn:
/// `draw_runs` emitted ranges covering the whole buffer and dropped none, so a
/// text node whose atlas was refused still submitted one instance per glyph —
/// the vertex stage, the rasterizer, `rounded_box_sdf`, the gate,
/// `clip_coverage` and a `discard`, per fragment, on every frame it was drawn.
/// `Residency` memoizes the refusal, so it never recovered.
///
/// # Why the scene has a rect on each side
///
/// **The instance count is not observable from outside this crate, and the
/// draw count is.** A refused run at the *end* of the buffer changes neither —
/// the range simply stops earlier, and one draw is still one draw. Put it
/// between two drawn stretches and the gap splits what was one range into two,
/// so `Renderer::last_draw_runs` goes from one to two.
///
/// That is the trade, stated rather than hidden: **one more draw call, against
/// one instance per glyph**. `resolve_frame`'s own doc names the likeliest
/// refusal as a CJK coverage set — one sheet for a whole script at a whole
/// weight — which is also the case with the most glyphs, so the population that
/// costs the most is the population most likely to be refused.
///
/// `a_refused_glyph_run_is_in_no_range_at_all` is the unit half, and it counts
/// the instances directly rather than inferring them from the draw split.
#[test]
fn a_refused_glyph_run_splits_the_frame_rather_than_drawing_its_quads() {
    let mut paints = PaintTable::new();
    let ground = paints.intern_fill(&FillSpec::Solid {
        color: Color {
            r: 0.0,
            g: 0.5,
            b: 0.25,
            a: 1.0,
        },
    });
    let paint = paints.push(PaintEntry {
        fill: ground,
        ..PaintEntry::default()
    });
    let rect_at = |y: f32| RectEntry {
        x: 0.0,
        y,
        w: W as f32,
        h: 16.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    };
    // Rect 0 carries the glyphs, rect 1 follows them in the buffer. So the
    // instance order is fill, glyph, glyph, fill — and the refused pair is
    // between two drawn stretches rather than at an end.
    let rects = vec![rect_at(0.0), rect_at(24.0)];

    let build = |bytes: Vec<u8>, format: ImageFormat| {
        let mut glyphs = GlyphRunTable::new();
        glyphs.push_atlas(
            Atlas::new(
                ImageAsset { format, bytes },
                ATLAS,
                ATLAS,
                4,
                0.5,
                vec![AtlasGlyph {
                    glyph_id: 11,
                    plane_em: [0.0, 0.0, 1.0, 1.0],
                    atlas_px: [0.0, 4.0, 4.0, 8.0],
                }],
            )
            .expect("a test atlas states a non-zero px_per_em"),
        );
        let ink = Color {
            r: 0.2,
            g: 0.6,
            b: 1.0,
            a: 1.0,
        };
        for (run, quads) in [
            one_glyph(11, 10.0, 4.0, 12.0, ink, 1.0),
            one_glyph(11, 34.0, 4.0, 12.0, ink, 1.0),
        ] {
            glyphs.push_run(run, &quads);
        }
        glyphs
    };

    let clips = ClipTable::new();
    let images = ImageTable::new();
    // One device for both cases, for the reason
    // `a_degenerate_coverage_field_draws_nothing` gives: `Renderer::new` is an
    // adapter request and a device creation, the dominant cost of every test in
    // this file. Sharing it is safe here because the two atlases differ in
    // `ImageFormat`, which `PayloadKey::atlas` keys on — so the baked one being
    // resident does not answer for the JPEG, and the refusal below is a real one
    // rather than a cache hit.
    let mut renderer = renderer();
    let mut draw = |glyphs: &GlyphRunTable| {
        let mut painter = GpuPainter::new();
        painter.paint(&rects, &paints, &images, &clips, &[], glyphs, None);
        let pixels = renderer
            .render(painter.instances(), &paints, &images, &clips, glyphs, W, H)
            .expect("the fixture extent is within any device's maximum");
        (renderer.last_draw_runs(), renderer.refusals().len(), pixels)
    };

    // The same scene twice, differing only in whether the atlas can be decoded.
    // The baked one is the control: it says the split below is the refusal and
    // not the shape of the fixture.
    let (drawn_calls, drawn_refusals, _) =
        draw(&build(glyph_field().bytes, ImageFormat::Rgba8Unorm));
    assert_eq!(drawn_refusals, 0, "the baked atlas must not be refused");
    assert_eq!(
        drawn_calls, 1,
        "a resolved run is one range over the whole buffer, so the frame is one draw",
    );

    let (refused_calls, refused_refusals, pixels) =
        draw(&build(JPEG_FIXTURE.to_vec(), ImageFormat::Jpeg));
    assert_eq!(
        refused_refusals, 1,
        "two runs name one refused atlas, so one refusal",
    );
    assert_eq!(
        refused_calls, 2,
        "the refused glyphs must be in no range, which splits the frame's one range in two — \
         one draw means their quads are still being submitted and discarded per fragment",
    );

    // **Both rects still draw**, which the draw count alone cannot say. The
    // failure this change could introduce is the opposite of the one it fixes:
    // `Resolved::draws` answering `false` for an instance that does draw leaves
    // the same two ranges and takes that instance's ink out of the picture.
    // Every sibling refusal test in this file probes the canvas, and this is
    // why.
    let ground = [0, 128, 64, 255];
    for (what, y) in [
        ("the rect before the glyphs", 8),
        ("the rect after them", 30),
    ] {
        let seen = texel(&pixels, W / 2, y);
        assert!(
            seen.iter().zip(ground).all(|(a, b)| a.abs_diff(b) <= 1),
            "{what} must still be drawn — got {seen:?} against {ground:?}, so an instance that \
             draws was excluded from every range",
        );
    }
}
