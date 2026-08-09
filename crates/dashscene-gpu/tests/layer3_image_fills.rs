//! Layer 3 for image fills: the residency path puts a payload on the device and
//! the shader samples the right texels of it (story #581).
//!
//! # Still a gate on the pipeline, not a fidelity check
//!
//! The same line story #580 drew and epic #569 insists on. What these establish
//! is that a payload reached a texture, that the atlas rectangle resolved to the
//! payload it belongs to, and that each scale mode reads the region of the image
//! it is defined to read. How an image *looks* — filtering, resampling, colour —
//! is layer 4's, needs hardware, and is story #586's.
//!
//! # The fixtures are deliberately not uniform
//!
//! Every payload here is asymmetric in extent and distinct in every texel. A
//! square payload cannot fail a transposed extent; a payload of one colour
//! cannot fail a wrong atlas offset, a wrong scale mode, or a sampler reading
//! the neighbouring allocation. Both mistakes are the ones this code can
//! actually make, so no fixture is allowed to be blind to them.

use dashpaint::{
    ClipIndex, ClipTable, FillSpec, GlyphRunTable, ImageAsset, ImageFill, ImageFormat, ImageTable,
    Mat23, PaintEntry, PaintTable, Painter, RectEntry, ScaleMode, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};

const W: u32 = 64;
const H: u32 = 48;

/// A renderer, or a named failure — the reason `layer3_render_smoke` gives.
fn renderer() -> Renderer {
    Renderer::new().expect("layer 3 needs a device")
}

/// A baked RGBA payload whose texel at `(x, y)` is `colour(x, y)`.
///
/// Baked rather than encoded so the bytes in the test are the bytes in the
/// texture: a decode between the two would make a failure ambiguous between the
/// residency path and the decoder. The PNG path has its own test below.
fn payload(width: u32, height: u32, colour: impl Fn(u32, u32) -> [u8; 4]) -> ImageAsset {
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            bytes.extend_from_slice(&colour(x, y));
        }
    }
    ImageAsset {
        format: ImageFormat::Rgba8Unorm,
        bytes,
    }
}

/// A four-by-two payload with eight distinguishable texels.
///
/// Four wide and two high, so a transposed extent reads out of range or reads
/// the wrong row; and every texel distinct, so a sample that landed one texel
/// off is a different colour rather than the same one.
fn eight_texels() -> ImageAsset {
    payload(4, 2, |x, y| {
        [(x as u8 + 1) * 40, (y as u8 + 1) * 90, 20, 255]
    })
}

/// Renders one image-filled rect over the whole canvas and returns the pixels.
fn draw_fill(asset: ImageAsset, fill: ImageFill, x: f32, y: f32, w: f32, h: f32) -> Vec<u8> {
    let mut images = ImageTable::new();
    let width = 4;
    let height = 2;
    let index = match asset.format.is_encoded() {
        true => images.push(asset),
        false => {
            let (w, h) = extent_of(&asset, width, height);
            images.push_baked(asset, w, h)
        }
    };
    let mut paints = PaintTable::new();
    let interned = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: index,
        ..fill
    }));
    let paint = paints.push(PaintEntry {
        fill: interned,
        ..PaintEntry::default()
    });
    let clips = ClipTable::new();
    let rects = [RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
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
    renderer()
        .render(
            painter.instances(),
            &paints,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum")
}

/// The extent a baked payload of four bytes a texel covers, given the width the
/// caller built it at.
fn extent_of(asset: &ImageAsset, width: u32, height: u32) -> (u32, u32) {
    assert_eq!(
        asset.bytes.len() as u32,
        width * height * 4,
        "the fixture's bytes and its stated extent disagree"
    );
    (width, height)
}

/// The unpremultiplied RGBA texel at (x, y).
fn texel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// A channel comparison that tolerates the rounding of the round trip through a
/// texture and back out through `unpremultiply`, and nothing more.
fn near(actual: [u8; 4], expected: [u8; 4], what: &str) {
    for channel in 0..4 {
        let delta = actual[channel].abs_diff(expected[channel]);
        assert!(
            delta <= 2,
            "{what}: channel {channel} was {} and should be about {} (whole texel {actual:?} \
             against {expected:?})",
            actual[channel],
            expected[channel]
        );
    }
}

/// An image fill draws the payload's own texels, and which texel lands where
/// follows the image's own axes.
///
/// The load-bearing part is that *different* positions give *different*
/// colours, in the order the payload was built in. A residency path that
/// uploaded the payload but resolved the wrong atlas rectangle would draw one
/// flat colour, and every "is anything drawn at all" check would still pass.
#[test]
fn an_image_fill_draws_the_payloads_own_texels() {
    // The rect is 4:2, exactly the payload's aspect, so Fill scales it by 8 with
    // no cropping and each source texel is an 8x8 block of the canvas.
    let pixels = draw_fill(
        eight_texels(),
        ImageFill {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        },
        0.0,
        0.0,
        32.0,
        16.0,
    );

    for sx in 0..4u32 {
        for sy in 0..2u32 {
            let expected = [(sx as u8 + 1) * 40, (sy as u8 + 1) * 90, 20, 255];
            // The centre of the block this source texel covers.
            let at = texel(&pixels, sx * 8 + 4, sy * 8 + 4);
            near(at, expected, &format!("source texel ({sx}, {sy})"));
        }
    }
}

/// Two payloads in one atlas each draw their own texels.
///
/// The check the whole residency path lives or dies by, and the one a
/// single-payload fixture cannot make: the first allocation of an atlas starts
/// at the origin, so a slot that ignored its own rectangle draws the first
/// payload correctly and every later payload as the first one.
#[test]
fn two_payloads_in_one_atlas_each_draw_their_own_texels() {
    let mut images = ImageTable::new();
    let first = images.push_baked(
        payload(4, 2, |x, _| [(x as u8 + 1) * 40, 10, 10, 255]),
        4,
        2,
    );
    // A different extent as well as different texels, so a slot that carried
    // the first payload's rectangle draws the wrong size *and* the wrong
    // colours.
    let second = images.push_baked(
        payload(2, 4, |_, y| [10, (y as u8 + 1) * 50, 10, 255]),
        2,
        4,
    );

    let mut paints = PaintTable::new();
    let image_paint = |paints: &mut PaintTable, index: u32| {
        let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
            image: index,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        paints.push(PaintEntry {
            fill,
            ..PaintEntry::default()
        })
    };
    let left = image_paint(&mut paints, first);
    let right = image_paint(&mut paints, second);

    let clips = ClipTable::new();
    let rects = [
        RectEntry {
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 16.0,
            paint: left,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
        RectEntry {
            x: 32.0,
            y: 0.0,
            w: 16.0,
            h: 32.0,
            paint: right,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        },
    ];
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

    // The first payload varies along x and is flat in y; the second varies along
    // y and is flat in x. Each is checked where the other's pattern would be
    // visibly wrong.
    near(
        texel(&pixels, 4, 8),
        [40, 10, 10, 255],
        "first payload, x=0",
    );
    near(
        texel(&pixels, 28, 8),
        [160, 10, 10, 255],
        "first payload, x=3",
    );
    near(
        texel(&pixels, 40, 4),
        [10, 50, 10, 255],
        "second payload, y=0",
    );
    near(
        texel(&pixels, 40, 28),
        [10, 200, 10, 255],
        "second payload, y=3",
    );

    // Both landed in the one RGBA8 atlas, so the frame is one draw call.
    assert_eq!(
        renderer.last_draw_runs(),
        1,
        "two payloads of one texel format share an atlas and must not split the batch"
    );
    assert_eq!(
        renderer.evictions(),
        0,
        "two small payloads must fit an atlas without evicting anything"
    );
}

/// Fit contains the image in the box and paints nothing beside it; Fill covers
/// the box and paints everywhere.
///
/// The two modes differ only in `max` against `min`, which is one character —
/// so they are checked against each other rather than each on its own.
#[test]
fn fit_letterboxes_where_fill_covers() {
    // A 4:2 payload in a square box. Fit scales to the width and leaves a
    // quarter of the box empty above and below; Fill scales to the height and
    // covers all of it.
    let square = (8.0, 8.0, 32.0, 32.0);
    let fill_mode = |mode| ImageFill {
        image: 0,
        scale_mode: mode,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    };
    let fitted = draw_fill(
        eight_texels(),
        fill_mode(ScaleMode::Fit),
        square.0,
        square.1,
        square.2,
        square.3,
    );
    let filled = draw_fill(
        eight_texels(),
        fill_mode(ScaleMode::Fill),
        square.0,
        square.1,
        square.2,
        square.3,
    );

    // Near the top of the box: outside the fitted image, inside the filling one.
    assert_eq!(
        texel(&fitted, 24, 10)[3],
        0,
        "Fit must not paint the letterbox"
    );
    assert!(
        texel(&filled, 24, 10)[3] > 250,
        "Fill covers the whole box, including where Fit letterboxes"
    );
    // The centre is inside both, and is the same source texel in both.
    assert!(texel(&fitted, 24, 24)[3] > 250, "Fit paints its own middle");
    assert!(texel(&filled, 24, 24)[3] > 250, "Fill paints its middle");
}

/// Tile repeats the payload every `size * tile_scale` units from the box's own
/// origin.
#[test]
fn tile_repeats_the_payload_from_the_box_origin() {
    // A 4x2 payload at tile_scale 4 repeats every 16 units across and 8 down.
    let pixels = draw_fill(
        eight_texels(),
        ImageFill {
            image: 0,
            scale_mode: ScaleMode::Tile,
            transform: Mat23::IDENTITY,
            tile_scale: 4.0,
        },
        0.0,
        0.0,
        48.0,
        24.0,
    );
    // The same point of the first tile and of the second, across and down.
    let first = texel(&pixels, 2, 2);
    near(first, [40, 90, 20, 255], "the first tile's origin texel");
    near(texel(&pixels, 18, 2), first, "one tile across");
    near(texel(&pixels, 2, 10), first, "one tile down");
    // And a point that is a *different* source texel, so the repetition is not
    // being confirmed by a flat image.
    near(
        texel(&pixels, 14, 2),
        [160, 90, 20, 255],
        "the last column of the first tile",
    );
}

/// Crop maps the box's normalised coordinate through the fill's transform.
///
/// The transform halves the image's u axis, so the box shows the left half of
/// the payload stretched across it: the texel at the box's right edge is the
/// payload's *second* column rather than its fourth.
#[test]
fn crop_maps_the_box_through_the_fills_transform() {
    let pixels = draw_fill(
        eight_texels(),
        ImageFill {
            image: 0,
            scale_mode: ScaleMode::Crop,
            transform: Mat23 {
                a: 0.5,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            tile_scale: 1.0,
        },
        0.0,
        0.0,
        32.0,
        16.0,
    );
    // Across the box, u runs 0 to 0.5, so the four visible bands are source
    // columns 0 and 1 rather than 0 through 3.
    near(
        texel(&pixels, 2, 4),
        [40, 90, 20, 255],
        "column 0 at the left",
    );
    near(
        texel(&pixels, 30, 4),
        [80, 90, 20, 255],
        "column 1 at the right, where an identity transform would show column 3",
    );
}

/// The encoded half: a PNG payload is decoded and drawn.
///
/// The same 7x5 fixture `dashpaint`'s own tests use, so the extent this asserts
/// is the one the header states rather than one this test chose.
#[test]
fn a_png_payload_is_decoded_and_drawn() {
    const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");
    let mut images = ImageTable::new();
    let index = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    });
    assert_eq!(
        (images.resolve(index).width, images.resolve(index).height),
        (7, 5),
        "the fixture is the 7x5 payload these tests are written against"
    );

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
    let clips = ClipTable::new();
    let rects = [RectEntry {
        x: 8.0,
        y: 8.0,
        w: 28.0,
        h: 20.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
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

    // Opaque inside the box and untouched outside it. What the payload's colours
    // are is the decoder's business and `dashpaint`'s tests own the header; what
    // this establishes is that the decode reached a texture at all.
    assert!(
        texel(&pixels, 20, 18)[3] > 250,
        "the decoded payload is drawn inside the box"
    );
    assert_eq!(
        texel(&pixels, 2, 2)[3],
        0,
        "and nothing is drawn outside it"
    );
}

/// A table row the frame does not draw is never made resident.
///
/// Residency has to follow the frame, not the table: a document's asset table
/// is every image it *could* show, and what has to fit in VRAM is what it shows
/// now. Resolving the table instead does not merely waste memory — a document
/// with more assets than one atlas holds would refuse to draw two of them,
/// because the whole table would be the working set by construction.
///
/// The undrawn asset is one **no atlas could hold**, which is what gives this
/// test teeth. A table of three ordinary payloads would land in one atlas
/// either way and every counter would agree, so the fixture is built so that
/// resolving the table cannot merely be wasteful — it has to fail by name.
/// The payload is one texel past `ATLAS_EXTENT`, derived from the constant
/// rather than restated, so that changing the atlas budget cannot quietly leave
/// this fixture small enough to fit — which would remove the test's teeth while
/// it kept passing. At one texel high it costs eight kilobytes to build.
#[test]
fn a_table_row_the_frame_does_not_draw_is_not_made_resident() {
    let mut images = ImageTable::new();
    let drawn = images.push_baked(
        payload(4, 2, |x, _| [(x as u8 + 1) * 40, 10, 10, 255]),
        4,
        2,
    );
    // An asset no rect names, and that `ResidencyError::TooLarge` would refuse
    // if anything asked for it.
    let too_wide = dashscene_gpu::ATLAS_EXTENT + 1;
    images.push_baked(payload(too_wide, 1, |_, _| [1, 2, 3, 255]), too_wide, 1);
    // And an ordinary one, so the fill table is three rows and the drawn row is
    // not the last.
    images.push_baked(payload(8, 8, |_, _| [4, 5, 6, 255]), 8, 8);

    let mut paints = PaintTable::new();
    // Interned for every asset, so the *fill table* has three rows too — the
    // array `resolve_images` walks. Only one of them is drawn.
    let fills: Vec<_> = (0..3)
        .map(|index| {
            paints.intern_fill(&FillSpec::Image(ImageFill {
                image: index,
                scale_mode: ScaleMode::Fill,
                transform: Mat23::IDENTITY,
                tile_scale: 1.0,
            }))
        })
        .collect();
    let paint = paints.push(PaintEntry {
        fill: fills[drawn as usize],
        ..PaintEntry::default()
    });
    let clips = ClipTable::new();
    let rects = [RectEntry {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 16.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
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

    // The one drawn payload still draws its own texels.
    near(texel(&pixels, 4, 8), [40, 10, 10, 255], "the drawn payload");
    near(
        texel(&pixels, 28, 8),
        [160, 10, 10, 255],
        "the drawn payload",
    );
    assert_eq!(renderer.last_draw_runs(), 1);

    // A steady-state second frame allocates nothing, residency included.
    let after_first = renderer.allocations();
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
    near(texel(&pixels, 4, 8), [40, 10, 10, 255], "the second frame");
    assert_eq!(
        renderer.allocations(),
        after_first,
        "a steady-state frame allocates nothing, residency included"
    );
    assert_eq!(renderer.evictions(), 0);
}

/// This painter declares what it can be handed, and the declaration follows the
/// device rather than a fixed list.
#[test]
fn the_declaration_names_what_this_painter_can_actually_take() {
    let renderer = renderer();
    let painter = GpuPainter::on(&renderer);

    // The one container it links a decoder for, and the two it does not.
    assert!(painter.samples(ImageFormat::Png));
    assert!(!painter.samples(ImageFormat::Jpeg), "issue #718");
    assert!(!painter.samples(ImageFormat::Gif), "issue #718");
    // Uncompressed texels need no feature at all.
    assert!(painter.samples(ImageFormat::Rgba8Srgb));
    assert!(painter.samples(ImageFormat::Rgba8Unorm));
    // And the block formats follow the device, both ways — asserted against the
    // device's own answer rather than against a constant, because a test that
    // hard-coded either would pass on one runner and fail on the next.
    assert_eq!(
        painter.samples(ImageFormat::Astc6x6Srgb),
        renderer.samples_astc(),
        "the ASTC declaration must follow the device"
    );
    assert_eq!(
        painter.samples(ImageFormat::Astc12x12Unorm),
        renderer.samples_astc()
    );

    // A painter built without an adapter claims no block format, whatever this
    // machine can do: that is the safe direction, since a payload it did not
    // claim is never bound to it.
    let quiet = GpuPainter::new();
    assert!(quiet.samples(ImageFormat::Png));
    assert!(!quiet.samples(ImageFormat::Astc6x6Srgb));
}

/// A replaced document does not draw the old document's images.
///
/// The residency cache is keyed by the image table's own row, and a rebuilt
/// arena starts that table again from zero — so index 0 of the new table can
/// have the same format, offset and length as index 0 of the old one and name a
/// completely different picture. Nothing in the key can tell them apart; the
/// digest that can is a debug assertion, so a release build would draw the old
/// image and report nothing.
///
/// `Renderer::forget_uploaded` is the signal a host already sends for exactly
/// this (`Present::document_replaced`), and residency now clears with it. The
/// two payloads below are built to collide: same extent, same format, same
/// length, and therefore the same key — only their texels differ.
#[test]
fn a_replaced_document_does_not_draw_the_previous_documents_image() {
    let scene = |seed: u8| {
        let mut images = ImageTable::new();
        images.push_baked(payload(4, 2, |_, _| [seed, 10, 10, 255]), 4, 2);
        let mut paints = PaintTable::new();
        let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        let paint = paints.push(PaintEntry {
            fill,
            ..PaintEntry::default()
        });
        (images, paints, paint)
    };

    let mut renderer = renderer();
    let clips = ClipTable::new();
    let draw = |renderer: &mut Renderer, images: &ImageTable, paints: &PaintTable, paint| {
        let rects = [RectEntry {
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 16.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }];
        let mut painter = GpuPainter::new();
        painter.paint(
            &rects,
            paints,
            images,
            &ClipTable::new(),
            &[],
            &GlyphRunTable::new(),
            None,
        );
        renderer
            .render(
                painter.instances(),
                paints,
                images,
                &ClipTable::new(),
                &GlyphRunTable::new(),
                W,
                H,
            )
            .expect("the fixture extent is within any device's maximum")
    };
    let _ = &clips;

    let (first_images, first_paints, first_paint) = scene(40);
    let first = draw(&mut renderer, &first_images, &first_paints, first_paint);
    near(texel(&first, 4, 8), [40, 10, 10, 255], "the first document");

    // The second document's asset has the same key as the first's, and
    // different texels.
    let (second_images, second_paints, second_paint) = scene(200);
    assert_eq!(
        first_images.all_entries()[0],
        second_images.all_entries()[0],
        "the two payloads must share a residency key, or this test proves nothing"
    );

    renderer.forget_uploaded();
    let second = draw(&mut renderer, &second_images, &second_paints, second_paint);
    near(
        texel(&second, 4, 8),
        [200, 10, 10, 255],
        "the replaced document's own image, not the one the cache still held",
    );
}

/// A resident PNG is decoded once, however many frames draw it.
///
/// The cost this whole mechanism exists to remove, and the one a pixel
/// comparison cannot see: the picture is identical whether the payload is
/// decoded once or on every frame. It was decoded on every frame until review
/// caught it, because `TexelPayload::of` ran before the cache-hit check and its
/// result was used only by a debug assertion.
///
/// Story #581 exists because PNG decoding was 20.4 % of every frame, so a
/// residency path that still decodes every frame delivers none of it.
#[test]
fn a_resident_png_is_decoded_once_and_not_once_a_frame() {
    const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");
    let mut images = ImageTable::new();
    let index = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    });
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
    let clips = ClipTable::new();
    let rects = [RectEntry {
        x: 4.0,
        y: 4.0,
        w: 40.0,
        h: 24.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
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
    for frame in 0..5 {
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
        assert_eq!(
            renderer.decodes(),
            1,
            "frame {frame} brought the running decode count to {}; a resident payload is \
             decoded once",
            renderer.decodes()
        );
    }
}

/// Drawing an image costs the device objects residency allocates, and
/// `Renderer::allocations` counts them.
///
/// Differenced against a renderer drawing the same geometry with a solid fill,
/// because an absolute count cannot separate residency's objects from the frame
/// buffers'. Everything else about the two frames is identical — one rect, one
/// instance, the same extent — so the difference is exactly what the image path
/// added.
///
/// This exists because the obvious version of the check could not fail. Asserting
/// that a *steady-state* frame allocates nothing says nothing about residency:
/// the atlas is created on the first frame, so the second frame's delta is zero
/// whether or not residency is counted at all. Removing residency's term from
/// `Renderer::allocations` left every other test in this suite green.
#[test]
fn the_allocation_count_includes_the_atlas_residency_created() {
    let geometry = |paint| {
        [RectEntry {
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 16.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }]
    };

    // The solid arm: no image table, so residency never builds an atlas.
    let mut solid_paints = PaintTable::new();
    let solid = solid_paints.push_solid(dashpaint::Color {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    });
    let mut solid_painter = GpuPainter::new();
    solid_painter.paint(
        &geometry(solid),
        &solid_paints,
        &ImageTable::new(),
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let mut solid_renderer = renderer();
    solid_renderer
        .render(
            solid_painter.instances(),
            &solid_paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");

    // The image arm: same rect, same extent, one payload.
    let mut images = ImageTable::new();
    let index = images.push_baked(eight_texels(), 4, 2);
    let mut image_paints = PaintTable::new();
    let fill = image_paints.intern_fill(&FillSpec::Image(ImageFill {
        image: index,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let image_paint = image_paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let mut image_painter = GpuPainter::new();
    image_painter.paint(
        &geometry(image_paint),
        &image_paints,
        &images,
        &ClipTable::new(),
        &[],
        &GlyphRunTable::new(),
        None,
    );
    let mut image_renderer = renderer();
    image_renderer
        .render(
            image_painter.instances(),
            &image_paints,
            &images,
            &ClipTable::new(),
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum");

    // Four: the atlas texture and its view, plus one extra bind-group rebuild.
    // The image arm rebinds a second time when the atlas appears, and a rebind
    // rebuilds every group — the placeholder's and the atlas's — where the solid
    // arm only ever built the placeholder's one.
    let difference = image_renderer.allocations() - solid_renderer.allocations();
    assert_eq!(
        difference,
        4,
        "an image frame allocates the atlas texture, its view and the bind groups that name it, \
         and all of them must reach Renderer::allocations; the solid arm reported {} and the \
         image arm {}",
        solid_renderer.allocations(),
        image_renderer.allocations()
    );
}

/// An image whose payload has no bytes draws nothing, rather than garbage.
///
/// Boundary B stores such an asset at a zero extent rather than refusing it,
/// because `dashscene-validator`'s image.no-bytes rule is what names that case
/// (`dashpaint`'s `identified_extent`). The shader then divides by that extent
/// in every scale mode, and the Fill/Fit range guard cannot catch the result:
/// a NaN compares false against everything, so an unguarded division sends
/// non-finite coordinates to the sampler and draws whatever they land on.
#[test]
fn an_image_with_no_bytes_draws_nothing() {
    // Both halves, because they fail differently: an encoded payload with no
    // bytes panics in the decoder, and a baked one divides by zero in the
    // shader. Neither may reach either place.
    let mut images = ImageTable::new();
    let encoded = images.push(ImageAsset {
        format: ImageFormat::Png,
        bytes: Vec::new(),
    });
    let index = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: Vec::new(),
        },
        0,
        0,
    );
    for asset in [encoded, index] {
        assert_eq!(
            (images.resolve(asset).width, images.resolve(asset).height),
            (0, 0),
            "the fixtures are the zero-extent assets this test is about"
        );
    }

    let mut paints = PaintTable::new();
    let mut rects = Vec::new();
    for (offset, asset) in [encoded, index].into_iter().enumerate() {
        let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
            image: asset,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        let paint = paints.push(PaintEntry {
            fill,
            ..PaintEntry::default()
        });
        rects.push(RectEntry {
            x: 8.0 + offset as f32 * 20.0,
            y: 8.0,
            w: 16.0,
            h: 24.0,
            paint,
            clip: ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        });
    }
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

    // Every texel of the canvas, not a sample of it: a NaN coordinate can land
    // anywhere in the atlas, so a check at one point could miss what it drew.
    for y in 0..H {
        for x in 0..W {
            assert_eq!(
                texel(&pixels, x, y),
                [0, 0, 0, 0],
                "an asset with no bytes painted ({x}, {y})"
            );
        }
    }
}
