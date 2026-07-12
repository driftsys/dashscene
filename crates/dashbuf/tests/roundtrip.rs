//! v0.1 exit criterion E6 in miniature: a document built in memory
//! survives a flatbuffer round trip byte-for-byte-equivalent in its
//! decoded fields (DESIGN_1.md §11).

use dashbuf::{
    Color, Document, DocumentArgs, FixedSizeLayout, Node, NodeArgs, SolidFill, SolidFillArgs,
    root_as_document,
};
use flatbuffers::FlatBufferBuilder;

#[test]
fn node_round_trips_through_a_document_buffer() {
    let mut builder = FlatBufferBuilder::new();

    let name = builder.create_string("root");
    let color = Color::new(1.0, 0.0, 0.0, 1.0);
    let paint = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&color),
        },
    );
    let layout = FixedSizeLayout::new(8.0, 4.0, 100.0, 50.0);
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: Some(name),
            parent: u32::MAX,
            layout: Some(&layout),
            paint: Some(paint),
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    let decoded_nodes = decoded.nodes().expect("nodes vector present");
    assert_eq!(decoded_nodes.len(), 1);

    let decoded_node = decoded_nodes.get(0);
    assert_eq!(decoded_node.name(), Some("root"));
    assert_eq!(decoded_node.parent(), u32::MAX);

    let decoded_layout = decoded_node.layout().expect("layout present");
    assert_eq!(decoded_layout.x(), 8.0);
    assert_eq!(decoded_layout.y(), 4.0);
    assert_eq!(decoded_layout.width(), 100.0);
    assert_eq!(decoded_layout.height(), 50.0);

    let decoded_color = decoded_node
        .paint()
        .expect("paint present")
        .color()
        .expect("color present");
    assert_eq!(decoded_color.r(), 1.0);
    assert_eq!(decoded_color.g(), 0.0);
    assert_eq!(decoded_color.b(), 0.0);
    assert_eq!(decoded_color.a(), 1.0);
}

#[test]
fn a_root_node_reads_back_the_default_parent_sentinel() {
    let mut builder = FlatBufferBuilder::new();

    let layout = FixedSizeLayout::new(0.0, 0.0, 1.0, 1.0);
    let paint = SolidFill::create(&mut builder, &SolidFillArgs { color: None });
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: None,
            parent: u32::MAX,
            layout: Some(&layout),
            paint: Some(paint),
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    assert_eq!(decoded.nodes().unwrap().get(0).parent(), u32::MAX);
}
