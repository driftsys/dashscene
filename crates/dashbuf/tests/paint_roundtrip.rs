//! v0.3 paint-vocabulary round trips (issue #13): every new paint kind
//! and field survives a build → finish → decode cycle.

use dashbuf::{
    Color, CornerRadii, Document, DocumentArgs, Fill, Gradient, GradientArgs, GradientKind,
    GradientStop, Image, ImageArgs, ImageFill, ImageFillArgs, ImageFormat, Node, NodeArgs,
    ScaleMode, SolidFill, SolidFillArgs, Stroke, StrokeArgs, Vec2, root_as_document,
};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

fn half_blue() -> Color {
    Color::new(0.0, 0.0, 1.0, 0.5)
}

/// Wraps a single finished node in a document and hands back the
/// decoded bytes.
fn finish_single_node_document(
    mut builder: FlatBufferBuilder<'static>,
    node: WIPOffset<Node<'static>>,
) -> Vec<u8> {
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            images: None,
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

fn single_node(bytes: &[u8]) -> Node<'_> {
    root_as_document(bytes)
        .expect("valid dashbuf document")
        .nodes()
        .expect("nodes vector present")
        .get(0)
}

#[test]
fn gradient_fill_round_trips_all_four_kinds() {
    for kind in [
        GradientKind::Linear,
        GradientKind::Radial,
        GradientKind::Angular,
        GradientKind::Diamond,
    ] {
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
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                fill_type: Fill::Gradient,
                fill: Some(gradient.as_union_value()),
                ..Default::default()
            },
        );

        let bytes = finish_single_node_document(builder, node);
        let decoded = single_node(&bytes);
        assert_eq!(decoded.fill_type(), Fill::Gradient);
        let gradient = decoded.fill_as_gradient().expect("gradient fill present");
        assert_eq!(gradient.kind(), kind);
        let origin = gradient.handle_origin().expect("origin present");
        let primary = gradient.handle_primary().expect("primary present");
        let secondary = gradient.handle_secondary().expect("secondary present");
        assert_eq!((origin.x(), origin.y()), (0.0, 0.0));
        assert_eq!((primary.x(), primary.y()), (1.0, 0.0));
        assert_eq!((secondary.x(), secondary.y()), (0.0, 1.0));
        let stops = gradient.stops().expect("stops present");
        assert_eq!(stops.len(), 2);
        assert_eq!(stops.get(0).offset(), 0.0);
        assert_eq!(stops.get(0).color().r(), 1.0);
        assert_eq!(stops.get(1).offset(), 1.0);
        assert_eq!(stops.get(1).color().a(), 0.5);
    }
}

#[test]
fn image_fill_round_trips_every_scale_mode() {
    for scale_mode in [
        ScaleMode::Fill,
        ScaleMode::Fit,
        ScaleMode::Crop,
        ScaleMode::Tile,
    ] {
        let mut builder = FlatBufferBuilder::new();
        let bytes_vector = builder.create_vector(&[1u8, 2, 3, 4]);
        let image = Image::create(
            &mut builder,
            &ImageArgs {
                format: ImageFormat::Png,
                bytes: Some(bytes_vector),
            },
        );
        let images = builder.create_vector(&[image]);
        let image_fill = ImageFill::create(
            &mut builder,
            &ImageFillArgs {
                image: 0,
                scale_mode,
            },
        );
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                fill_type: Fill::ImageFill,
                fill: Some(image_fill.as_union_value()),
                ..Default::default()
            },
        );
        let nodes = builder.create_vector(&[node]);
        let document = Document::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                images: Some(images),
            },
        );
        builder.finish(document, None);

        let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
        let node = decoded.nodes().expect("nodes present").get(0);
        assert_eq!(node.fill_type(), Fill::ImageFill);
        let fill = node.fill_as_image_fill().expect("image fill present");
        assert_eq!(fill.image(), 0);
        assert_eq!(fill.scale_mode(), scale_mode);
        let image = decoded.images().expect("images present").get(0);
        assert_eq!(image.format(), ImageFormat::Png);
        assert_eq!(image.bytes().expect("bytes present").bytes(), [1, 2, 3, 4]);
    }
}

#[test]
fn stroke_round_trips_every_align() {
    for align in [
        dashbuf::StrokeAlign::Inside,
        dashbuf::StrokeAlign::Center,
        dashbuf::StrokeAlign::Outside,
    ] {
        let mut builder = FlatBufferBuilder::new();
        let stroke = Stroke::create(
            &mut builder,
            &StrokeArgs {
                width: 2.5,
                align,
                color: Some(&red()),
            },
        );
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                stroke: Some(stroke),
                ..Default::default()
            },
        );

        let bytes = finish_single_node_document(builder, node);
        let stroke = single_node(&bytes).stroke().expect("stroke present");
        assert_eq!(stroke.width(), 2.5);
        assert_eq!(stroke.align(), align);
        assert_eq!(stroke.color().expect("color present").r(), 1.0);
    }
}

#[test]
fn corners_and_clip_round_trip() {
    let mut builder = FlatBufferBuilder::new();
    let corners = CornerRadii::new(1.0, 2.0, 3.0, 4.0);
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            corners: Some(&corners),
            clip: true,
            ..Default::default()
        },
    );

    let bytes = finish_single_node_document(builder, node);
    let decoded = single_node(&bytes);
    let corners = decoded.corners().expect("corners present");
    assert_eq!(corners.top_left(), 1.0);
    assert_eq!(corners.top_right(), 2.0);
    assert_eq!(corners.bottom_right(), 3.0);
    assert_eq!(corners.bottom_left(), 4.0);
    assert!(decoded.clip());
}

#[test]
fn absent_corners_and_clip_read_back_as_defaults() {
    let mut builder = FlatBufferBuilder::new();
    let node = Node::create(&mut builder, &NodeArgs::default());

    let bytes = finish_single_node_document(builder, node);
    let decoded = single_node(&bytes);
    assert_eq!(decoded.corners(), None);
    assert!(!decoded.clip());
    assert_eq!(decoded.fill_type(), Fill::NONE);
}

#[test]
fn fill_union_discriminates_solid_and_legacy_paint_still_reads() {
    let mut builder = FlatBufferBuilder::new();
    let solid = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&red()),
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
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            ..Default::default()
        },
    );

    let bytes = finish_single_node_document(builder, node);
    let decoded = single_node(&bytes);
    assert_eq!(decoded.fill_type(), Fill::SolidFill);
    let solid = decoded.fill_as_solid_fill().expect("solid fill present");
    assert_eq!(solid.color().expect("color present").r(), 1.0);
    // The legacy v0.1 shorthand still reads alongside the union.
    let legacy = decoded.paint().expect("legacy paint present");
    assert_eq!(legacy.color().expect("color present").b(), 1.0);
    assert_eq!(decoded.fill_as_gradient(), None);
    assert_eq!(decoded.fill_as_image_fill(), None);
}
