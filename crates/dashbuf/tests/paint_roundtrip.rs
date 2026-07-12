//! v0.3 paint-vocabulary round trips (issue #13): every new paint kind
//! and field survives a build → finish → decode cycle, through the
//! document-level paint pool (`Document.paints` + `Node.paint_entry`).

use dashbuf::NO_PAINT;
use dashbuf::{
    Color, Document, DocumentArgs, Fill, Gradient, GradientArgs, GradientKind, GradientStop, Image,
    ImageArgs, ImageFill, ImageFillArgs, ImageFormat, Mat23, Node, NodeArgs, Paint, PaintArgs,
    ScaleMode, SolidFill, SolidFillArgs, Stroke, StrokeAlign, StrokeArgs, Vec2, root_as_document,
};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

fn half_blue() -> Color {
    Color::new(0.0, 0.0, 1.0, 0.5)
}

/// Finishes a document holding the given node, paint pool, and image
/// assets, and returns the serialized buffer bytes.
fn finish_document(
    mut builder: FlatBufferBuilder<'static>,
    node: WIPOffset<Node<'static>>,
    paints: &[WIPOffset<Paint<'static>>],
    images: &[WIPOffset<Image<'static>>],
) -> Vec<u8> {
    let nodes = builder.create_vector(&[node]);
    let images = (!images.is_empty()).then(|| builder.create_vector(images));
    let paints = (!paints.is_empty()).then(|| builder.create_vector(paints));
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            images,
            paints,
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

/// Decodes the buffer and resolves the single node's paint-pool entry.
fn single_node_paint(bytes: &[u8]) -> Paint<'_> {
    let document = root_as_document(bytes).expect("valid dashbuf document");
    let node = document.nodes().expect("nodes present").get(0);
    document
        .paints()
        .expect("paint pool present")
        .get(node.paint_entry() as usize)
}

#[test]
fn gradient_fill_round_trips_all_four_kinds() {
    for &kind in GradientKind::ENUM_VALUES {
        let mut builder = FlatBufferBuilder::new();
        let stops = builder.create_vector(&[
            GradientStop::new(0.0, &red()),
            GradientStop::new(1.0, &half_blue()),
        ]);
        let gradient = Gradient::create(
            &mut builder,
            &GradientArgs {
                kind,
                handle_origin: Some(&Vec2::new(0.0, 0.0)),
                handle_primary: Some(&Vec2::new(1.0, 0.0)),
                handle_secondary: Some(&Vec2::new(0.0, 1.0)),
                stops: Some(stops),
            },
        );
        let paint = Paint::create(
            &mut builder,
            &PaintArgs {
                fill_type: Fill::Gradient,
                fill: Some(gradient.as_union_value()),
                ..Default::default()
            },
        );
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                paint_entry: 0,
                ..Default::default()
            },
        );

        let bytes = finish_document(builder, node, &[paint], &[]);
        let paint = single_node_paint(&bytes);
        assert_eq!(paint.fill_type(), Fill::Gradient);
        let gradient = paint.fill_as_gradient().expect("gradient fill present");
        assert_eq!(gradient.kind(), kind);
        let origin = gradient.handle_origin();
        let primary = gradient.handle_primary();
        let secondary = gradient.handle_secondary();
        assert_eq!((origin.x(), origin.y()), (0.0, 0.0));
        assert_eq!((primary.x(), primary.y()), (1.0, 0.0));
        assert_eq!((secondary.x(), secondary.y()), (0.0, 1.0));
        let stops = gradient.stops();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops.get(0).offset(), 0.0);
        assert_eq!(stops.get(0).color().r(), 1.0);
        assert_eq!(stops.get(1).offset(), 1.0);
        assert_eq!(stops.get(1).color().a(), 0.5);
    }
}

#[test]
fn image_fill_round_trips_every_scale_mode() {
    for &scale_mode in ScaleMode::ENUM_VALUES {
        let mut builder = FlatBufferBuilder::new();
        // Two assets, and the fill references index 1: a non-default
        // index, so a fill whose `image` field never gets written
        // (flatbuffers default 0) fails the assertion below.
        let decoy_bytes = builder.create_vector(&[9u8]);
        let decoy = Image::create(
            &mut builder,
            &ImageArgs {
                format: ImageFormat::Png,
                bytes: Some(decoy_bytes),
            },
        );
        let real_bytes = builder.create_vector(&[1u8, 2, 3, 4]);
        let real = Image::create(
            &mut builder,
            &ImageArgs {
                format: ImageFormat::Png,
                bytes: Some(real_bytes),
            },
        );
        let transform = Mat23::new(1.0, 0.0, 0.0, 1.0, 0.25, 0.5);
        let image_fill = ImageFill::create(
            &mut builder,
            &ImageFillArgs {
                image: 1,
                scale_mode,
                transform: Some(&transform),
                tile_scale: 2.0,
            },
        );
        let paint = Paint::create(
            &mut builder,
            &PaintArgs {
                fill_type: Fill::ImageFill,
                fill: Some(image_fill.as_union_value()),
                ..Default::default()
            },
        );
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                paint_entry: 0,
                ..Default::default()
            },
        );

        let bytes = finish_document(builder, node, &[paint], &[decoy, real]);
        let document = root_as_document(&bytes).expect("valid dashbuf document");
        let paint = single_node_paint(&bytes);
        assert_eq!(paint.fill_type(), Fill::ImageFill);
        let fill = paint.fill_as_image_fill().expect("image fill present");
        assert_eq!(fill.image(), 1);
        assert_eq!(fill.scale_mode(), scale_mode);
        let transform = fill.transform().expect("transform present");
        assert_eq!(
            (transform.a(), transform.b(), transform.c(), transform.d()),
            (1.0, 0.0, 0.0, 1.0)
        );
        assert_eq!((transform.tx(), transform.ty()), (0.25, 0.5));
        assert_eq!(fill.tile_scale(), 2.0);
        let image = document
            .images()
            .expect("images present")
            .get(fill.image() as usize);
        assert_eq!(image.format(), ImageFormat::Png);
        assert_eq!(image.bytes().expect("bytes present").bytes(), [1, 2, 3, 4]);
    }
}

#[test]
fn stroke_round_trips_every_align() {
    for &align in StrokeAlign::ENUM_VALUES {
        let mut builder = FlatBufferBuilder::new();
        let stroke = Stroke::create(
            &mut builder,
            &StrokeArgs {
                width: 2.5,
                align,
                color: Some(&red()),
            },
        );
        let paint = Paint::create(
            &mut builder,
            &PaintArgs {
                stroke: Some(stroke),
                ..Default::default()
            },
        );
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                paint_entry: 0,
                ..Default::default()
            },
        );

        let bytes = finish_document(builder, node, &[paint], &[]);
        let stroke = single_node_paint(&bytes).stroke().expect("stroke present");
        assert_eq!(stroke.width(), 2.5);
        assert_eq!(stroke.align(), align);
        assert_eq!(stroke.color().r(), 1.0);
    }
}

#[test]
fn corners_and_clip_round_trip() {
    let mut builder = FlatBufferBuilder::new();
    let corners = dashbuf::CornerRadii::new(1.0, 2.0, 3.0, 4.0);
    let paint = Paint::create(
        &mut builder,
        &PaintArgs {
            corners: Some(&corners),
            clip: true,
            ..Default::default()
        },
    );
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            paint_entry: 0,
            ..Default::default()
        },
    );

    let bytes = finish_document(builder, node, &[paint], &[]);
    let paint = single_node_paint(&bytes);
    let corners = paint.corners().expect("corners present");
    assert_eq!(corners.top_left(), 1.0);
    assert_eq!(corners.top_right(), 2.0);
    assert_eq!(corners.bottom_right(), 3.0);
    assert_eq!(corners.bottom_left(), 4.0);
    assert!(paint.clip());
}

#[test]
fn absent_fields_read_back_as_defaults() {
    let mut builder = FlatBufferBuilder::new();
    let empty_paint = Paint::create(&mut builder, &PaintArgs::default());
    let node = Node::create(&mut builder, &NodeArgs::default());

    let bytes = finish_document(builder, node, &[empty_paint], &[]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let node = document.nodes().expect("nodes present").get(0);
    // A node that never set paint_entry carries the NO_PAINT sentinel.
    assert_eq!(node.paint_entry(), NO_PAINT);
    // An empty pool entry reads back as all-default.
    let paint = document.paints().expect("paints present").get(0);
    assert_eq!(paint.fill_type(), Fill::NONE);
    assert_eq!(paint.stroke(), None);
    assert_eq!(paint.corners(), None);
    assert!(!paint.clip());
}

#[test]
fn an_image_fill_without_transform_defaults_to_identity_semantics() {
    let mut builder = FlatBufferBuilder::new();
    let image_fill = ImageFill::create(
        &mut builder,
        &ImageFillArgs {
            image: 0,
            ..Default::default()
        },
    );
    let paint = Paint::create(
        &mut builder,
        &PaintArgs {
            fill_type: Fill::ImageFill,
            fill: Some(image_fill.as_union_value()),
            ..Default::default()
        },
    );
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            paint_entry: 0,
            ..Default::default()
        },
    );

    let bytes = finish_document(builder, node, &[paint], &[]);
    let fill = single_node_paint(&bytes)
        .fill_as_image_fill()
        .expect("image fill present");
    assert_eq!(fill.transform(), None);
    assert_eq!(fill.tile_scale(), 1.0);
}

#[test]
fn two_nodes_share_one_paint_pool_entry() {
    let mut builder = FlatBufferBuilder::new();
    let solid = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&red()),
        },
    );
    let paint = Paint::create(
        &mut builder,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            ..Default::default()
        },
    );
    let first = Node::create(
        &mut builder,
        &NodeArgs {
            paint_entry: 0,
            ..Default::default()
        },
    );
    let second = Node::create(
        &mut builder,
        &NodeArgs {
            paint_entry: 0,
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[first, second]);
    let paints = builder.create_vector(&[paint]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: Some(paints),
            ..Default::default()
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    let nodes = decoded.nodes().expect("nodes present");
    let paints = decoded.paints().expect("paints present");
    // Both nodes reference the one pooled entry — the dedup-style-pool
    // shape of DESIGN §5.
    assert_eq!(paints.len(), 1);
    for index in 0..nodes.len() {
        let entry = paints.get(nodes.get(index).paint_entry() as usize);
        assert_eq!(entry.fill_type(), Fill::SolidFill);
        let solid = entry.fill_as_solid_fill().expect("solid fill present");
        assert_eq!(solid.color().expect("color present").r(), 1.0);
    }
}

#[test]
fn pooled_solid_and_legacy_paint_coexist() {
    let mut builder = FlatBufferBuilder::new();
    let solid = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&red()),
        },
    );
    let paint = Paint::create(
        &mut builder,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            ..Default::default()
        },
    );
    let legacy = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&half_blue()),
        },
    );
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            paint: Some(legacy),
            paint_entry: 0,
            ..Default::default()
        },
    );

    let bytes = finish_document(builder, node, &[paint], &[]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let node = document.nodes().expect("nodes present").get(0);
    // The pooled entry is the one that counts (paint_entry supersedes
    // the legacy shorthand when set)…
    let entry = single_node_paint(&bytes);
    assert_eq!(entry.fill_type(), Fill::SolidFill);
    let solid = entry.fill_as_solid_fill().expect("solid fill present");
    assert_eq!(solid.color().expect("color present").r(), 1.0);
    // …while the legacy v0.1 shorthand still reads for v0.1 documents.
    let legacy = node.paint().expect("legacy paint present");
    assert_eq!(legacy.color().expect("color present").b(), 1.0);
}
