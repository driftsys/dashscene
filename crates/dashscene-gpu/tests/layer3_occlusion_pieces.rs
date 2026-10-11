//! Layer 3 for the occlusion pieces (story #1450): a scene drawn from its
//! pieces is the same picture as the scene drawn from whole rects.
//!
//! The commit cuts each rect into the pieces no later opaque core covers
//! (`dashscene_core::occlusion`), and the lean painter draws each piece's quad
//! while shading it against the rect's whole box. The pixels a piece leaves out
//! are under a later opaque core, so every pixel of the frame is the frame
//! drawn with an empty piece slice. Each fixture below draws both and compares
//! the whole frame.
//!
//! # Within one code point, and why not byte for byte
//!
//! The first draft compared bytes and failed on 48 pixels of the top row of
//! the first fixture, each by one code point of premultiplied alpha (128
//! against 127). A piece's quad is a different pair of triangles from the
//! whole box's, so the rasteriser interpolates `local` across a different
//! span and rounds it differently. A pixel whose coverage is exactly one half
//! — its centre half a pixel inside an edge, where the ramp crosses 127.5 —
//! then rounds to either side. So the comparison is in premultiplied RGBA and
//! allows one code point per channel. The defects this file exists for are
//! far outside that: two pieces overlapping on a half-transparent rect raise
//! alpha from 127 to about 191, and a gap between two pieces drops it to 0.
//!
//! Every rect that is cut is **translucent**. An opaque rect drawn twice over
//! the same pixel gives the same pixel, so a fixture whose cut rect was opaque
//! could not see two pieces overlapping. A translucent one blends twice where
//! two pieces overlap, which is the defect the vertex stage's grow bits exist
//! to prevent (`Instance::extent`), and it shows a gap where two pieces fail
//! to meet.
//!
//! Each fixture also checks that the pass really cut the rect, so that a
//! comparison between two whole-rect frames cannot pass in its place.

use dashpaint::{
    ClipIndex, ClipTable, Color, CornerRadii, FillSpec, GlyphRunTable, Gradient, GradientKind,
    GradientStop, ImageAsset, ImageFill, ImageFormat, ImageTable, Mat23, PaintEntry, PaintIndex,
    PaintTable, Painter, Piece, RectEntry, ScaleMode, StopRange, Vec2,
};
use dashscene_core::{Arena, occlusion};
use dashscene_gpu::GpuPainter;

mod common;
use common::{H, W, renderer};

fn rect(x: f32, y: f32, w: f32, h: f32, paint: PaintIndex) -> RectEntry {
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

/// A solid-filled entry with `corners`.
fn solid(paints: &mut PaintTable, color: Color, corners: CornerRadii) -> PaintIndex {
    let fill = paints.intern_fill(&FillSpec::Solid { color });
    paints.push(PaintEntry {
        fill,
        corners,
        ..PaintEntry::default()
    })
}

const SHARP: CornerRadii = CornerRadii {
    top_left: 0.0,
    top_right: 0.0,
    bottom_right: 0.0,
    bottom_left: 0.0,
};

/// Half-transparent blue: the rect that is cut.
const TRANSLUCENT: Color = Color {
    r: 0.2,
    g: 0.4,
    b: 0.9,
    a: 0.5,
};

/// Opaque orange: the core that cuts it.
const OPAQUE: Color = Color {
    r: 0.9,
    g: 0.5,
    b: 0.1,
    a: 1.0,
};

/// The pieces the commit would cut `rects` into, with the arena's default
/// antialiasing band — the pass `CommittedScene::pieces` runs, called directly
/// on hand-built tables.
fn pieces_of(rects: &[RectEntry], paints: &PaintTable, clips: &ClipTable) -> Vec<Piece> {
    let cores = occlusion::cores(rects, paints, clips, &[], Arena::DEFAULT_AA_BAND);
    let barriers = occlusion::barriers(rects, paints);
    occlusion::pieces(rects, &cores, &barriers, |index| {
        occlusion::occludable(paints.resolve(rects[index].paint), paints)
    })
}

/// Packs and draws one scene with `pieces`, returning the pixels.
fn draw(
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    pieces: &[Piece],
) -> Vec<u8> {
    let clips = ClipTable::new();
    let mut painter = GpuPainter::new();
    painter.paint(
        rects,
        paints,
        images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        pieces,
        None,
    );
    renderer()
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
}

/// One unpremultiplied RGBA8 pixel, premultiplied again and rounded.
fn premultiplied(pixel: &[u8]) -> [i32; 4] {
    let alpha = f32::from(pixel[3]);
    let channel = |value: u8| (f32::from(value) * alpha / 255.0).round() as i32;
    [
        channel(pixel[0]),
        channel(pixel[1]),
        channel(pixel[2]),
        i32::from(pixel[3]),
    ]
}

/// One differing pixel: `(x, y, left, right)`, in unpremultiplied RGBA.
type Difference = (u32, u32, [u8; 4], [u8; 4]);

/// The pixels that differ by more than one premultiplied code point on any
/// channel between two frames, as `(x, y, left, right)` in unpremultiplied
/// RGBA, at most a handful, for a failure message that says where.
fn differences(left: &[u8], right: &[u8]) -> (usize, Vec<Difference>) {
    let mut count = 0;
    let mut first = Vec::new();
    for (i, (a, b)) in left.chunks(4).zip(right.chunks(4)).enumerate() {
        let (pa, pb) = (premultiplied(a), premultiplied(b));
        if pa.iter().zip(pb).any(|(x, y)| (x - y).abs() > 1) {
            count += 1;
            if first.len() < 6 {
                let i = i as u32;
                first.push((
                    i % W,
                    i / W,
                    [a[0], a[1], a[2], a[3]],
                    [b[0], b[1], b[2], b[3]],
                ));
            }
        }
    }
    (count, first)
}

/// Draws `rects` from their pieces and whole, checks the pass cut rect 0 into
/// `expected` pieces, and asserts the two frames agree within one
/// premultiplied code point on every pixel.
fn assert_pieces_draw_the_whole_picture(
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    expected: usize,
) {
    let pieces = pieces_of(rects, paints, &ClipTable::new());
    let cut = pieces.iter().filter(|piece| piece.rect == 0).count();
    assert_eq!(
        cut, expected,
        "the pass cut rect 0 into {cut} pieces, so this fixture does not test what it says: \
         {pieces:?}"
    );

    let from_pieces = draw(rects, paints, images, &pieces);
    let whole = draw(rects, paints, images, &[]);
    let (count, first) = differences(&from_pieces, &whole);
    assert_eq!(
        count, 0,
        "{count} pixels differ by more than one premultiplied code point between the frame \
         drawn from pieces and the frame drawn whole; \
         the first, as (x, y, pieces, whole): {first:?}"
    );
}

/// A translucent rect under a later opaque core, cut into four pieces, draws
/// the same frame as the whole rect: outside the core, where the four pieces
/// meet each other and the antialiased edge of the box, and inside the core,
/// which covers it.
#[test]
fn a_translucent_rect_cut_by_a_core_draws_what_the_whole_rect_draws() {
    let mut paints = PaintTable::new();
    let under = solid(&mut paints, TRANSLUCENT, SHARP);
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(8.0, 8.0, 48.0, 32.0, under),
        rect(20.0, 16.0, 24.0, 16.0, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &ImageTable::new(), 4);
}

/// The same, with a core whose edges — and so every cut edge of the pieces
/// beneath it — fall between pixel centres. Two pieces that meet on such an
/// edge must leave no pixel uncovered and cover none twice.
#[test]
fn pieces_meeting_on_a_non_integral_edge_leave_no_seam() {
    let mut paints = PaintTable::new();
    let under = solid(&mut paints, TRANSLUCENT, SHARP);
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(7.6, 8.3, 48.2, 31.9, under),
        rect(20.3, 16.7, 23.4, 15.2, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &ImageTable::new(), 4);
}

/// A rounded translucent rect cut by a core keeps its rounded corners and its
/// antialiased outer edge: each piece is shaded against the whole box, so the
/// signed distance at a corner is the box's own.
#[test]
fn a_rounded_rect_cut_by_a_core_keeps_its_corners() {
    let mut paints = PaintTable::new();
    let under = solid(
        &mut paints,
        TRANSLUCENT,
        CornerRadii {
            top_left: 10.0,
            top_right: 6.0,
            bottom_right: 12.0,
            bottom_left: 4.0,
        },
    );
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(5.5, 6.25, 52.0, 36.0, under),
        rect(22.0, 18.0, 20.0, 12.0, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &ImageTable::new(), 4);
}

/// A core that crosses the cut rect's right edge leaves three pieces: a strip
/// above, a strip below and a piece to the left. The two strips' cut edges run
/// into the box's right edge, which is grown there, so each strip's quad has
/// one cut edge and one grown edge meeting at a corner. The box's right edge
/// is at 55.2, so the centre of pixel column 55 lies 0.3 outside it, inside
/// the antialiasing ramp: a strip whose right edge did not grow would leave
/// that column empty. (A first version had the edge at 55.8, where the column
/// past it is beyond the ramp, and it passed with the right grow bit never
/// set.)
#[test]
fn a_core_crossing_the_box_edge_leaves_three_pieces_with_grown_ends() {
    let mut paints = PaintTable::new();
    let under = solid(&mut paints, TRANSLUCENT, SHARP);
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(7.6, 8.3, 47.6, 31.9, under),
        rect(40.3, 16.0, 24.0, 16.0, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &ImageTable::new(), 3);
}

/// A cut gradient fill is the whole box's gradient: each piece evaluates the
/// ramp against `bounds`, so the colour at a pixel does not depend on which
/// piece's quad covers it. A piece restricts only the quad a rect is drawn
/// over; every fill is still evaluated against the rect's whole box.
#[test]
fn a_cut_gradient_fill_keeps_the_whole_box_ramp() {
    let mut paints = PaintTable::new();
    let stop = |offset: f32, r: f32, b: f32| GradientStop {
        offset,
        color: Color {
            r,
            g: 0.3,
            b,
            a: 0.6,
        },
    };
    let fill = paints.intern_fill(&FillSpec::Gradient {
        gradient: Gradient {
            kind: GradientKind::Linear,
            handle_origin: Vec2 { x: 0.0, y: 0.0 },
            handle_primary: Vec2 { x: 1.0, y: 1.0 },
            handle_secondary: Vec2 { x: -1.0, y: 1.0 },
            stops: StopRange::NONE,
        },
        stops: vec![stop(0.0, 1.0, 0.0), stop(1.0, 0.0, 1.0)],
    });
    let under = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(6.5, 6.25, 50.0, 34.0, under),
        rect(20.0, 16.0, 24.0, 14.0, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &ImageTable::new(), 4);
}

/// A cut image fill samples the whole box's image: each piece maps its texels
/// against `bounds`, never against its own quad.
///
/// A baked 4x3 payload whose twelve texels all differ, so a piece that mapped
/// the image onto its own quad would draw a different texel at most pixels.
/// The corpus PNG the other layer-3 binaries share is one colour, and a first
/// version of this fixture on it passed a mutation that mapped the image onto
/// the quad.
///
/// **No texel boundary and no box edge falls on a pixel centre.** Sampling is
/// nearest, so a pixel centre exactly on a boundary between two texels takes
/// either texel depending on the last bit of the interpolated position, and a
/// piece's quad interpolates it differently from the whole box's (see the
/// module documentation). A second version of this fixture, with the box at
/// `(6.5, 6.25)`, put the scaled image's left edge and a texel row boundary
/// on pixel centres and differed on 49 pixels for that reason alone. Under
/// `ScaleMode::Fill` the 4x3 image is drawn at 12.5 px per texel from an
/// origin of `(6.3, 4.35)`, so every boundary here is at least 0.15 px from a
/// pixel centre.
#[test]
fn a_cut_image_fill_keeps_the_whole_box_mapping() {
    let (width, height) = (4_u32, 3_u32);
    let mut bytes = Vec::new();
    for y in 0..height {
        for x in 0..width {
            bytes.extend_from_slice(&[(x as u8 + 1) * 50, (y as u8 + 1) * 70, 30, 255]);
        }
    }
    let mut images = ImageTable::new();
    let image = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes,
        },
        width,
        height,
    );
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let under = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let core = solid(&mut paints, OPAQUE, SHARP);
    let rects = [
        rect(6.3, 6.1, 50.0, 34.0, under),
        rect(20.0, 16.0, 24.0, 14.0, core),
    ];
    assert_pieces_draw_the_whole_picture(&rects, &paints, &images, 4);
}
