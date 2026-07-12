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

#[test]
fn flex_and_constraint_fields_round_trip() {
    use dashbuf::{
        AxisSizing, CrossAxisAlign, EdgeInsets, LayoutConstraints, LayoutConstraintsArgs,
        LayoutContainer, LayoutContainerArgs, LayoutMode, MainAxisAlign,
    };

    let mut builder = FlatBufferBuilder::new();

    let padding = EdgeInsets::new(1.0, 2.0, 3.0, 4.0);
    let flex = LayoutContainer::create(
        &mut builder,
        &LayoutContainerArgs {
            mode: LayoutMode::Horizontal,
            gap: 8.0,
            padding: Some(&padding),
            main_align: MainAxisAlign::Center,
            cross_align: CrossAxisAlign::End,
        },
    );
    let constraints = LayoutConstraints::create(
        &mut builder,
        &LayoutConstraintsArgs {
            sizing_h: AxisSizing::Hug,
            sizing_v: AxisSizing::Fill,
            min_width: Some(10.0),
            max_width: Some(100.0),
            min_height: Some(5.0),
            max_height: Some(50.0),
        },
    );
    let layout = FixedSizeLayout::new(0.0, 0.0, 20.0, 30.0);
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: None,
            parent: u32::MAX,
            layout: Some(&layout),
            paint: None,
            paint_entry: u32::MAX,
            text: u32::MAX,
            text_style: u32::MAX,
            flex: Some(flex),
            constraints: Some(constraints),
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            images: None,
            paints: None,
            strings: None,
            text_styles: None,
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    let decoded_node = decoded.nodes().unwrap().get(0);

    let flex = decoded_node.flex().expect("flex container present");
    assert_eq!(flex.mode(), LayoutMode::Horizontal);
    assert_eq!(flex.gap(), 8.0);
    let padding = flex.padding().expect("padding present");
    assert_eq!(
        (
            padding.left(),
            padding.top(),
            padding.right(),
            padding.bottom()
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(flex.main_align(), MainAxisAlign::Center);
    assert_eq!(flex.cross_align(), CrossAxisAlign::End);

    let constraints = decoded_node.constraints().expect("constraints present");
    assert_eq!(constraints.sizing_h(), AxisSizing::Hug);
    assert_eq!(constraints.sizing_v(), AxisSizing::Fill);
    assert_eq!(constraints.min_width(), Some(10.0));
    assert_eq!(constraints.max_width(), Some(100.0));
    assert_eq!(constraints.min_height(), Some(5.0));
    assert_eq!(constraints.max_height(), Some(50.0));
}

#[test]
fn a_node_without_flex_tables_reads_back_absent() {
    let mut builder = FlatBufferBuilder::new();

    let layout = FixedSizeLayout::new(0.0, 0.0, 1.0, 1.0);
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: None,
            parent: u32::MAX,
            layout: Some(&layout),
            paint: None,
            paint_entry: u32::MAX,
            text: u32::MAX,
            text_style: u32::MAX,
            flex: None,
            constraints: None,
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            images: None,
            paints: None,
            strings: None,
            text_styles: None,
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    let decoded_node = decoded.nodes().unwrap().get(0);
    assert!(decoded_node.flex().is_none());
    assert!(decoded_node.constraints().is_none());
}

#[test]
fn empty_flex_tables_read_back_the_schema_defaults() {
    use dashbuf::{
        AxisSizing, CrossAxisAlign, LayoutConstraints, LayoutConstraintsArgs, LayoutContainer,
        LayoutContainerArgs, LayoutMode, MainAxisAlign,
    };

    // A present-but-empty table is the second spelling of "all
    // defaults" (the first is an absent table); both must mean the
    // same thing to a future loader.
    let mut builder = FlatBufferBuilder::new();
    let flex = LayoutContainer::create(&mut builder, &LayoutContainerArgs::default());
    let constraints = LayoutConstraints::create(&mut builder, &LayoutConstraintsArgs::default());
    let layout = FixedSizeLayout::new(0.0, 0.0, 1.0, 1.0);
    let node = Node::create(
        &mut builder,
        &NodeArgs {
            name: None,
            parent: u32::MAX,
            layout: Some(&layout),
            paint: None,
            paint_entry: u32::MAX,
            text: u32::MAX,
            text_style: u32::MAX,
            flex: Some(flex),
            constraints: Some(constraints),
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            images: None,
            paints: None,
            strings: None,
            text_styles: None,
        },
    );
    builder.finish(document, None);

    let decoded = root_as_document(builder.finished_data()).expect("valid dashbuf document");
    let decoded_node = decoded.nodes().unwrap().get(0);

    let flex = decoded_node.flex().expect("container present");
    assert_eq!(flex.mode(), LayoutMode::None);
    assert_eq!(flex.gap(), 0.0);
    assert!(flex.padding().is_none(), "absent padding = zero insets");
    assert_eq!(flex.main_align(), MainAxisAlign::Start);
    assert_eq!(flex.cross_align(), CrossAxisAlign::Start);

    let constraints = decoded_node.constraints().expect("constraints present");
    assert_eq!(constraints.sizing_h(), AxisSizing::Fixed);
    assert_eq!(constraints.sizing_v(), AxisSizing::Fixed);
    assert_eq!(constraints.min_width(), None);
    assert_eq!(constraints.max_width(), None);
    assert_eq!(constraints.min_height(), None);
    assert_eq!(constraints.max_height(), None);
}
