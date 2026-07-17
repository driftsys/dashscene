//! `load_document`'s variant-table replay (story #20): "loading is a
//! straight replay of the document's nodes through the ordinary
//! producer API" (`docs/design/dashscene-core-arena.md`) extends to
//! `Document.variant_sets` — a loaded scene resolves the same rect/paint
//! tables a hand-staged `add_variant_set`/`set_variant` call would.

use dashbuf::{
    Document, DocumentArgs, FixedSizeLayout, Node, NodeArgs, VariantMember, VariantMemberArgs,
    VariantOverride, VariantOverrideArgs, VariantPropValue, VariantSet, VariantSetArgs,
    VariantWidth, VariantWidthArgs, root_as_document,
};
use dashscene_core::{Arena, load_document};
use flatbuffers::FlatBufferBuilder;

/// Two 10x10 nodes and one variant set whose only member overrides node
/// 1's width, plus `active_member` — parameterized so the same fixture
/// proves both "not yet switched" and "switched at load time."
fn document_bytes(active_member: u32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 10.0, 10.0);
    let a = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            ..Default::default()
        },
    );
    let node_b = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[a, node_b]);

    let default_member = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let width = VariantWidth::create(&mut b, &VariantWidthArgs { value: 99.0 });
    let width_override = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 1,
            value_type: VariantPropValue::VariantWidth,
            value: Some(width.as_union_value()),
        },
    );
    let overrides = b.create_vector(&[width_override]);
    let wide_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            overrides: Some(overrides),
            ..Default::default()
        },
    );
    let members = b.create_vector(&[default_member, wide_member]);
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            active_member,
        },
    );
    let variant_sets = b.create_vector(&[set]);

    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            variant_sets: Some(variant_sets),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

#[test]
fn a_loaded_document_resolves_with_its_default_active_member() {
    let bytes = document_bytes(0);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);

    assert_eq!(
        arena.committed().rects()[1].w,
        10.0,
        "base width, unswitched"
    );
}

#[test]
fn a_loaded_document_resolves_with_a_non_default_active_member() {
    let bytes = document_bytes(1);
    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);

    assert_eq!(
        arena.committed().rects()[1].w,
        99.0,
        "the document's own active_member selects the override at load time"
    );
}

#[test]
fn a_document_without_variant_sets_still_loads() {
    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 5.0, 5.0);
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);

    assert_eq!(arena.committed().rects().len(), 1);
    assert_eq!(arena.committed().rects()[0].w, 5.0);
}

/// The binding tables (story #167) replay through the same producer API:
/// a loaded document's signals and rows land in the arena tables exactly
/// as a hand-staged `declare_signal`/`bind` sequence would, with node
/// and signal indices resolved through this load's own mappings.
#[test]
fn a_loaded_document_replays_its_binding_tables() {
    use dashbuf::{
        Binding, BindingArgs, BindingChannel, BindingTransform, SignalDecl, SignalDeclArgs,
        TransformScale, TransformScaleArgs,
    };
    use dashscene_core::{Channel, ScalarTransform};

    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 10.0, 10.0);
    let a = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            ..Default::default()
        },
    );
    let child = Node::create(
        &mut b,
        &NodeArgs {
            parent: 0,
            layout: Some(&layout),
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[a, child]);

    let name = b.create_string("size/gap");
    let named = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: Some(name),
            initial: 16.0,
        },
    );
    let anonymous = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 2.0,
        },
    );
    let signals = b.create_vector(&[named, anonymous]);

    let scale = TransformScale::create(&mut b, &TransformScaleArgs { factor: 3.0 });
    let rows = [
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 0,
                channel: BindingChannel::Gap,
                transform_type: BindingTransform::NONE,
                transform: None,
            },
        ),
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 1,
                node: 1,
                channel: BindingChannel::FillA,
                transform_type: BindingTransform::TransformScale,
                transform: Some(scale.as_union_value()),
            },
        ),
    ];
    let bindings = b.create_vector(&rows);

    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            signals: Some(signals),
            bindings: Some(bindings),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    // Pre-seed the arena with one node and one signal, so the loader's
    // index mappings are exercised: the document's indices are not the
    // arena's.
    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        let seeded = txn.add_node(None, Some("pre-existing"));
        let _ = seeded;
        txn.declare_signal(Some("pre-existing"), 1.0);
        txn.commit();
    }

    let doc = root_as_document(&bytes).expect("valid document");
    load_document(&doc, &mut arena);

    let signals = arena.signals();
    assert_eq!(signals.len(), 3);
    assert_eq!(signals[1].name.as_deref(), Some("size/gap"));
    assert_eq!(signals[1].initial, 16.0);
    assert_eq!(signals[2].name, None);

    let rows = arena.bindings();
    assert_eq!(rows.len(), 2);
    // Node 0 of the document is arena node 1 (one pre-existing node).
    assert_eq!(rows[0].node.index(), 1);
    assert_eq!(rows[0].channel, Channel::Gap);
    assert_eq!(rows[0].signal.index(), 1);
    assert_eq!(rows[0].transform, ScalarTransform::Identity);
    assert_eq!(rows[1].node.index(), 2);
    assert_eq!(rows[1].channel, Channel::FillA);
    assert_eq!(rows[1].signal.index(), 2);
    assert_eq!(rows[1].transform, ScalarTransform::Scale(3.0));
}

/// The v0.8 layout fields (story #43) replay through the same producer
/// API: a loaded grid container's tracks, cross gap, and baseline
/// alignment — and a child's placement — land in the arena's layout
/// intent exactly as hand-staged props would.
#[test]
fn a_loaded_document_replays_its_v08_layout_fields() {
    use dashbuf::{
        CrossAxisAlign, GridTrack, GridTrackArgs, GridTrackSizing, LayoutConstraints,
        LayoutConstraintsArgs, LayoutContainer, LayoutContainerArgs, LayoutMode,
    };

    let mut b = FlatBufferBuilder::new();
    let row_track = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fixed,
            value: 96.0,
        },
    );
    let column_track = GridTrack::create(
        &mut b,
        &GridTrackArgs {
            sizing: GridTrackSizing::Fraction,
            value: 2.0,
        },
    );
    let grid_rows = b.create_vector(&[row_track]);
    let grid_columns = b.create_vector(&[column_track]);
    let flex = LayoutContainer::create(
        &mut b,
        &LayoutContainerArgs {
            mode: LayoutMode::Grid,
            gap: 12.0,
            cross_align: CrossAxisAlign::Baseline,
            cross_gap: Some(16.0),
            grid_rows: Some(grid_rows),
            grid_columns: Some(grid_columns),
            ..Default::default()
        },
    );
    let layout = FixedSizeLayout::new(0.0, 0.0, 100.0, 100.0);
    let container = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            flex: Some(flex),
            ..Default::default()
        },
    );
    let constraints = LayoutConstraints::create(
        &mut b,
        &LayoutConstraintsArgs {
            grid_row: Some(0),
            grid_column: Some(0),
            grid_row_span: 1,
            grid_column_span: 1,
            ..Default::default()
        },
    );
    let child = Node::create(
        &mut b,
        &NodeArgs {
            parent: 0,
            layout: Some(&layout),
            constraints: Some(constraints),
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[container, child]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);

    let root = arena.roots()[0];
    let container_layout = arena.layout(root);
    assert_eq!(container_layout.mode, dashscene_core::LayoutMode::Grid);
    assert_eq!(container_layout.cross_gap, Some(16.0));
    assert_eq!(
        container_layout.cross_align,
        dashscene_core::CrossAxisAlign::Baseline
    );
    let (rows, columns) = arena.grid_tracks(root);
    assert_eq!(rows, [dashscene_core::GridTrack::Fixed(96.0)]);
    assert_eq!(columns, [dashscene_core::GridTrack::Fraction(2.0)]);

    let child = arena.children(root)[0];
    let child_layout = arena.layout(child);
    assert_eq!(child_layout.grid_row, Some(0));
    assert_eq!(child_layout.grid_column, Some(0));
    assert_eq!(child_layout.grid_row_span, 1);
    assert_eq!(child_layout.grid_column_span, 1);
}
