//! v0.1 exit criterion E6 in miniature: a document built in memory
//! survives a flatbuffer round trip byte-for-byte-equivalent in its
//! decoded fields (docs/specification/05-qualification.md).

use dashbuf::{
    Document, DocumentArgs, FixedSizeLayout, Node, NodeArgs, SolidFill, SolidFillArgs,
    root_as_document,
};
use flatbuffers::FlatBufferBuilder;

mod common;
use common::red;

#[test]
fn node_round_trips_through_a_document_buffer() {
    let mut builder = FlatBufferBuilder::new();

    let name = builder.create_string("root");
    let color = red();
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
        AxisSizing, CrossAxisAlign, EdgeInsets, GridTrack, GridTrackArgs, GridTrackSizing,
        LayoutConstraints, LayoutConstraintsArgs, LayoutContainer, LayoutContainerArgs, LayoutMode,
        MainAxisAlign,
    };

    let mut builder = FlatBufferBuilder::new();

    // v0.8 (story #43): one Fixed and one Fraction track per axis, each
    // at a value distinguishable from the other axis's.
    let track = |b: &mut FlatBufferBuilder<'static>, sizing, value| {
        GridTrack::create(b, &GridTrackArgs { sizing, value })
    };
    let row_fixed = track(&mut builder, GridTrackSizing::Fixed, 96.0);
    let row_flex = track(&mut builder, GridTrackSizing::Fraction, 2.0);
    let grid_rows = builder.create_vector(&[row_fixed, row_flex]);
    let col_flex = track(&mut builder, GridTrackSizing::Fraction, 1.0);
    let col_fixed = track(&mut builder, GridTrackSizing::Fixed, 160.0);
    let grid_columns = builder.create_vector(&[col_flex, col_fixed]);

    let padding = EdgeInsets::new(1.0, 2.0, 3.0, 4.0);
    let flex = LayoutContainer::create(
        &mut builder,
        &LayoutContainerArgs {
            mode: LayoutMode::Horizontal,
            gap: 8.0,
            padding: Some(&padding),
            main_align: MainAxisAlign::Center,
            cross_align: CrossAxisAlign::End,
            cross_gap: Some(6.0),
            grid_rows: Some(grid_rows),
            grid_columns: Some(grid_columns),
        },
    );
    let margin = EdgeInsets::new(-8.0, 0.0, 0.0, 0.0);
    let constraints = LayoutConstraints::create(
        &mut builder,
        &LayoutConstraintsArgs {
            sizing_h: AxisSizing::Hug,
            sizing_v: AxisSizing::Fill,
            min_width: Some(10.0),
            max_width: Some(100.0),
            min_height: Some(5.0),
            max_height: Some(50.0),
            margin: Some(&margin),
            grid_row: Some(1),
            grid_column: Some(2),
            grid_row_span: 2,
            grid_column_span: 3,
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
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: None,
            strings: None,
            text_styles: None,
            ..Default::default()
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
    assert_eq!(flex.cross_gap(), Some(6.0));
    let rows = flex.grid_rows().expect("row tracks present");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.get(0).sizing(), GridTrackSizing::Fixed);
    assert_eq!(rows.get(0).value(), 96.0);
    assert_eq!(rows.get(1).sizing(), GridTrackSizing::Fraction);
    assert_eq!(rows.get(1).value(), 2.0);
    let columns = flex.grid_columns().expect("column tracks present");
    assert_eq!(columns.len(), 2);
    assert_eq!(columns.get(0).sizing(), GridTrackSizing::Fraction);
    assert_eq!(columns.get(0).value(), 1.0);
    assert_eq!(columns.get(1).sizing(), GridTrackSizing::Fixed);
    assert_eq!(columns.get(1).value(), 160.0);

    let constraints = decoded_node.constraints().expect("constraints present");
    assert_eq!(constraints.sizing_h(), AxisSizing::Hug);
    assert_eq!(constraints.sizing_v(), AxisSizing::Fill);
    assert_eq!(constraints.min_width(), Some(10.0));
    assert_eq!(constraints.max_width(), Some(100.0));
    assert_eq!(constraints.min_height(), Some(5.0));
    assert_eq!(constraints.max_height(), Some(50.0));
    let margin = constraints.margin().expect("margin present");
    assert_eq!(
        (margin.left(), margin.top(), margin.right(), margin.bottom()),
        (-8.0, 0.0, 0.0, 0.0),
        "negative margin (a lowering target) round-trips"
    );
    assert_eq!(constraints.grid_row(), Some(1));
    assert_eq!(constraints.grid_column(), Some(2));
    assert_eq!(constraints.grid_row_span(), 2);
    assert_eq!(constraints.grid_column_span(), 3);
}

#[test]
fn v08_layout_fields_default_to_absent_and_the_new_enum_members_round_trip() {
    use dashbuf::{
        CrossAxisAlign, LayoutConstraints, LayoutConstraintsArgs, LayoutContainer,
        LayoutContainerArgs, LayoutMode,
    };

    // The new enum tail members (Wrap = 3, Grid = 4, Baseline = 3)
    // round-trip as themselves — a discriminant shift would decode them
    // as an older member.
    for mode in [LayoutMode::Wrap, LayoutMode::Grid] {
        let mut builder = FlatBufferBuilder::new();
        let flex = LayoutContainer::create(
            &mut builder,
            &LayoutContainerArgs {
                mode,
                cross_align: CrossAxisAlign::Baseline,
                ..Default::default()
            },
        );
        let constraints =
            LayoutConstraints::create(&mut builder, &LayoutConstraintsArgs::default());
        let node = Node::create(
            &mut builder,
            &NodeArgs {
                flex: Some(flex),
                constraints: Some(constraints),
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
        let decoded_node = decoded.nodes().unwrap().get(0);
        let flex = decoded_node.flex().expect("flex container present");
        assert_eq!(flex.mode(), mode);
        assert_eq!(flex.cross_align(), CrossAxisAlign::Baseline);

        // The v0.8 appends, unwritten, read back absent (or the span
        // default of 1) — absence of intent is not a value of intent (P1).
        assert_eq!(flex.cross_gap(), None);
        assert!(flex.grid_rows().is_none());
        assert!(flex.grid_columns().is_none());
        let constraints = decoded_node.constraints().expect("constraints present");
        assert_eq!(constraints.grid_row(), None);
        assert_eq!(constraints.grid_column(), None);
        assert_eq!(constraints.grid_row_span(), 1);
        assert_eq!(constraints.grid_column_span(), 1);
    }
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
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: None,
            strings: None,
            text_styles: None,
            ..Default::default()
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
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: None,
            strings: None,
            text_styles: None,
            ..Default::default()
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
    assert!(
        constraints.margin().is_none(),
        "absent margin = zero insets"
    );
}
