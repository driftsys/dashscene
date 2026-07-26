//! Story B1 (#340) — baked vector shapes render through the whole stack.
//!
//! A real MSDF field baked by `dashc`'s `vector_field` generator (B1.1) is
//! carried in a hand-built `.dsb` (the B1.2 schema), loaded into the arena
//! (`dashscene-core`, B1.3), and painted by the Skia reference painter (B1.3).
//! This is the "vectors actually render" checkpoint: the shape's silhouette
//! appears, a hole reads through as transparent (the field masks — a plain box
//! would fill it), a gradient composes under the mask, and the parametric
//! (no-field) path is unchanged.
//!
//! Not an oracle frame: it never touches the frozen E7 `manifest.json` /
//! `render_oracle` surface. It renders a synthetic document and asserts the
//! composition directly.

use dashbuf::{
    AssetEntry, AssetEntryArgs, AssetKind, AtlasRect, Color, Document, DocumentArgs, Fill,
    FixedSizeLayout, Gradient, GradientArgs, GradientKind, GradientStop, ImageFormat, Node,
    NodeArgs, Paint, PaintArgs, PlaneBounds, SolidFill, SolidFillArgs, Vec2, VectorAtlas,
    VectorAtlasArgs, VectorShape, VectorShapeArgs, root_as_document,
};
use dashc_wasm::figma::vector_field::{VectorAtlasBaker, VectorPath, WindingRule};
use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_skia::SkiaPainter;
use flatbuffers::FlatBufferBuilder;

/// A 40×40 solid square (counter-clockwise, y-down), NONZERO.
const SQUARE: &str = "M 0 0 L 40 0 L 40 40 L 0 40 Z";
/// The same square with a centered 20×20 hole (the inner subpath), filled
/// EVENODD so the hole reads through.
const SQUARE_WITH_HOLE: &str = "M 0 0 L 40 0 L 40 40 L 0 40 Z M 10 10 L 10 30 L 30 30 L 30 10 Z";

const SURFACE: i32 = 100;
/// The vector node's box origin; the field renders at `origin + plane_bounds`.
const ORIGIN: f32 = 20.0;
const BOX: f32 = 40.0;

/// One packed field baked by the real generator, plus the placement the schema
/// needs.
struct Baked {
    png: Vec<u8>,
    width: u32,
    height: u32,
    px_per_em: f32,
    distance_range: f32,
    atlas_rect: [u32; 4],
    plane_bounds: [f32; 4],
}

fn bake(path: &str, winding: WindingRule) -> Baked {
    let mut baker = VectorAtlasBaker::new();
    let shape = baker
        .add(&VectorPath { path, winding })
        .expect("the path bakes");
    let out = baker.finish().expect("the atlas packs");
    let placement = &out.shapes[shape as usize];
    let r = placement.atlas_rect;
    let p = placement.plane_bounds;
    Baked {
        png: out.image_png,
        width: out.width,
        height: out.height,
        px_per_em: out.px_per_em as f32,
        distance_range: out.distance_range as f32,
        atlas_rect: [r.x, r.y, r.width, r.height],
        plane_bounds: [p.left as f32, p.top as f32, p.right as f32, p.bottom as f32],
    }
}

/// Which fill masks the baked field.
enum TestFill {
    /// Opaque red.
    Solid,
    /// A horizontal red→blue linear gradient.
    Gradient,
}

/// Builds a one-node `.dsb`: a vector node whose paint entry's `fill` is masked
/// by the baked field.
fn vector_dsb(baked: &Baked, fill: TestFill) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();

    // Since story #107 the document carries asset identity and metadata, never
    // bytes: the hash is a filler (the caller hands the real PNG bytes to
    // `load_document` directly, bypassing the hash-resolution `dashbuf::open`
    // does for a real file), and the extent is the baked atlas's own.
    let hash = b.create_vector(&[7u8; 32]);
    let image = AssetEntry::create(
        &mut b,
        &AssetEntryArgs {
            hash: Some(hash),
            format: ImageFormat::Png,
            width: baked.width,
            height: baked.height,
            // The atlas is a baked MSDF field, not a picture.
            kind: AssetKind::DistanceField,
        },
    );
    let atlas = VectorAtlas::create(
        &mut b,
        &VectorAtlasArgs {
            image: 0,
            px_per_em: baked.px_per_em,
            distance_range: baked.distance_range,
        },
    );
    let [rx, ry, rw, rh] = baked.atlas_rect;
    let [pl, pt, pr, pb] = baked.plane_bounds;
    let shape = VectorShape::create(
        &mut b,
        &VectorShapeArgs {
            atlas: 0,
            atlas_rect: Some(&AtlasRect::new(rx, ry, rw, rh)),
            plane_bounds: Some(&PlaneBounds::new(pl, pt, pr, pb)),
        },
    );

    let (fill_type, fill) = match fill {
        TestFill::Solid => {
            let solid = SolidFill::create(
                &mut b,
                &SolidFillArgs {
                    color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
                },
            );
            (Fill::SolidFill, solid.as_union_value())
        }
        TestFill::Gradient => {
            let stops = b.create_vector(&[
                GradientStop::new(0.0, &Color::new(1.0, 0.0, 0.0, 1.0)),
                GradientStop::new(1.0, &Color::new(0.0, 0.0, 1.0, 1.0)),
            ]);
            let gradient = Gradient::create(
                &mut b,
                &GradientArgs {
                    kind: GradientKind::Linear,
                    handle_origin: Some(&Vec2::new(0.0, 0.5)),
                    handle_primary: Some(&Vec2::new(1.0, 0.5)),
                    handle_secondary: Some(&Vec2::new(0.0, 1.0)),
                    stops: Some(stops),
                },
            );
            (Fill::Gradient, gradient.as_union_value())
        }
    };
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type,
            fill: Some(fill),
            shape_field: 0,
            ..Default::default()
        },
    );
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&FixedSizeLayout::new(ORIGIN, ORIGIN, BOX, BOX)),
            paint_entry: 0,
            ..Default::default()
        },
    );

    let nodes = b.create_vector(&[node]);
    let assets = b.create_vector(&[image]);
    let paints = b.create_vector(&[paint]);
    let vector_atlases = b.create_vector(&[atlas]);
    let vector_shapes = b.create_vector(&[shape]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            assets: Some(assets),
            paints: Some(paints),
            vector_atlases: Some(vector_atlases),
            vector_shapes: Some(vector_shapes),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

/// Renders a `.dsb` at [`SURFACE`]×[`SURFACE`] and returns unpremultiplied
/// RGBA8888 rows. `payloads` binds the document's asset entries to their
/// bytes, one per entry in entry order — this document is a hand-built
/// section payload, not a `dashbuf::open`-read file, so the binding is done
/// by hand.
fn render(bytes: &[u8], payloads: &[&[u8]]) -> Vec<u8> {
    let doc = root_as_document(bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, payloads, &mut arena);
    let scene = arena.committed();
    let mut painter = SkiaPainter::new(SURFACE, SURFACE);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    painter.rgba_bytes()
}

/// The RGBA of the pixel at (x, y).
fn px(rgba: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * SURFACE + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn a_solid_vector_renders_its_silhouette_with_the_hole_masked_out() {
    // A red square with a hole, EVENODD. The ring paints red; the hole reads
    // through as transparent — which a plain box fill could never do, so this
    // proves the field, not the box, shapes the ink.
    let baked = bake(SQUARE_WITH_HOLE, WindingRule::EvenOdd);
    let dsb = vector_dsb(&baked, TestFill::Solid);
    let rgba = render(&dsb, &[baked.png.as_slice()]);

    // A point in the left wall of the ring (shape ~(3, 20)) is opaque red.
    let ring = px(&rgba, 23, 40);
    assert!(
        ring[0] > 200 && ring[1] < 60 && ring[2] < 60 && ring[3] > 200,
        "the ring must paint opaque red, got {ring:?}"
    );
    // The hole centre (shape (20, 20)) reads through — the field masks it out.
    let hole = px(&rgba, 40, 40);
    assert!(
        hole[3] < 40,
        "the hole must be transparent (a plain box would fill it), got {hole:?}"
    );
    // Well outside the shape is transparent (the padded quad reads coverage 0).
    let outside = px(&rgba, 92, 92);
    assert!(
        outside[3] < 40,
        "outside the shape must be transparent, got {outside:?}"
    );
}

#[test]
fn a_gradient_vector_renders_the_gradient_masked_by_the_field() {
    // A horizontal red→blue linear gradient under the square field. The
    // gradient shows only inside the shape, and it varies across the box —
    // proving the field masks the gradient rather than replacing it (the
    // hero's 12 gradient-filled vectors depend on exactly this).
    let baked = bake(SQUARE, WindingRule::NonZero);
    let dsb = vector_dsb(&baked, TestFill::Gradient);
    let rgba = render(&dsb, &[baked.png.as_slice()]);

    // Inside the square: opaque, and red on the left, blue on the right.
    let left = px(&rgba, 28, 40);
    let right = px(&rgba, 52, 40);
    assert!(
        left[3] > 200 && right[3] > 200,
        "the gradient must be opaque inside the shape, got left {left:?} right {right:?}"
    );
    assert!(
        left[0] > right[0] && right[2] > left[2],
        "the gradient must vary across the box (red left, blue right), got left {left:?} right {right:?}"
    );
    // Outside the shape the gradient is masked away.
    let outside = px(&rgba, 92, 92);
    assert!(
        outside[3] < 40,
        "the gradient must be masked to nothing outside the shape, got {outside:?}"
    );
}

#[test]
fn a_parametric_paint_without_a_field_fills_its_whole_box() {
    // The NO_FIELD (parametric) path is unchanged: a solid paint with no shape
    // channel fills its entire box, exactly as before B1. (The frozen golden
    // suites are the byte-identical guard; this is a focused regression check
    // that the field branch did not disturb the box-fill path.)
    let mut b = FlatBufferBuilder::new();
    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
        },
    );
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            // No shape channel: the sentinel default.
            ..Default::default()
        },
    );
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&FixedSizeLayout::new(ORIGIN, ORIGIN, BOX, BOX)),
            paint_entry: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let paints = b.create_vector(&[paint]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: Some(paints),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    let rgba = render(&bytes, &[]);
    // Every corner of the box is filled — no shape carves it.
    for (x, y) in [(22, 22), (57, 22), (22, 57), (57, 57), (40, 40)] {
        let p = px(&rgba, x, y);
        assert!(
            p[0] > 200 && p[3] > 200,
            "the whole box must fill red at ({x}, {y}), got {p:?}"
        );
    }
    // An empty table means no ImageTable was needed; assert the scene carried
    // none, so this really is the parametric path.
    let doc = root_as_document(&bytes).expect("valid document");
    let mut arena = Arena::new();
    load_document(&doc, &[], &mut arena);
    assert_eq!(
        arena.committed().images().len(),
        ImageTable::new().len(),
        "a parametric document carries no image atlas"
    );
}
