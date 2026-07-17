//! v0.4 variant-table round trips (issue #20): a `VariantSet`'s members
//! and their sparse overrides survive a build → finish → decode cycle
//! through `Document.variant_sets`
//! (docs/decisions/variant-set-flat-index.md).

use dashbuf::{
    Color, Document, DocumentArgs, Node, NodeArgs, VariantFill, VariantFillArgs, VariantHeight,
    VariantHeightArgs, VariantMember, VariantMemberArgs, VariantOverride, VariantOverrideArgs,
    VariantPropValue, VariantSet, VariantSetArgs, VariantVisible, VariantVisibleArgs, VariantWidth,
    VariantWidthArgs, VariantX, VariantXArgs, VariantY, VariantYArgs, root_as_document,
};
use flatbuffers::FlatBufferBuilder;

/// Finishes a document holding two bare nodes and the given variant
/// sets, and returns the serialized buffer bytes.
fn finish_document(
    mut builder: FlatBufferBuilder<'static>,
    variant_sets: Vec<flatbuffers::WIPOffset<VariantSet<'static>>>,
) -> Vec<u8> {
    let a = Node::create(&mut builder, &NodeArgs::default());
    let b = Node::create(&mut builder, &NodeArgs::default());
    let nodes = builder.create_vector(&[a, b]);
    let variant_sets = builder.create_vector(&variant_sets);
    let document = Document::create(
        &mut builder,
        &DocumentArgs {
            nodes: Some(nodes),
            variant_sets: Some(variant_sets),
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

#[test]
fn a_variant_set_round_trips_its_member_names_and_default_active_member() {
    let mut b = FlatBufferBuilder::new();
    let default_name = b.create_string("Default");
    let default_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            name: Some(default_name),
            overrides: None,
        },
    );
    let wide_name = b.create_string("Wide");
    let wide_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            name: Some(wide_name),
            overrides: None,
        },
    );
    let members = b.create_vector(&[default_member, wide_member]);
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            ..Default::default()
        },
    );

    let bytes = finish_document(b, vec![set]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let sets = document.variant_sets().expect("variant_sets present");
    assert_eq!(sets.len(), 1);
    let members = sets.get(0).members().expect("members present");
    assert_eq!(members.len(), 2);
    assert_eq!(members.get(0).name(), Some("Default"));
    assert_eq!(members.get(1).name(), Some("Wide"));
    // Written absent: defaults to member 0, never a sentinel.
    assert_eq!(sets.get(0).active_member(), 0);
}

#[test]
fn active_member_reads_back_when_set_to_a_non_default_value() {
    let mut b = FlatBufferBuilder::new();
    let m0 = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let m1 = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let members = b.create_vector(&[m0, m1]);
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            active_member: 1,
        },
    );

    let bytes = finish_document(b, vec![set]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    assert_eq!(document.variant_sets().unwrap().get(0).active_member(), 1);
}

#[test]
fn a_member_with_no_overrides_reads_back_empty() {
    let mut b = FlatBufferBuilder::new();
    let member = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let members = b.create_vector(&[member]);
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            ..Default::default()
        },
    );

    let bytes = finish_document(b, vec![set]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let member = document
        .variant_sets()
        .unwrap()
        .get(0)
        .members()
        .unwrap()
        .get(0);
    assert!(member.overrides().is_none_or(|o| o.is_empty()));
}

/// Every `VariantPropValue` union member round-trips through one
/// override, each targeting a distinct node index — proving the sparse
/// `(node, value)` pairing survives, not just the value.
#[test]
fn every_prop_value_kind_round_trips_through_one_override() {
    let mut b = FlatBufferBuilder::new();

    let x = VariantX::create(&mut b, &VariantXArgs { value: 11.0 });
    let y = VariantY::create(&mut b, &VariantYArgs { value: 22.0 });
    let width = VariantWidth::create(&mut b, &VariantWidthArgs { value: 33.0 });
    let height = VariantHeight::create(&mut b, &VariantHeightArgs { value: 44.0 });
    let fill = VariantFill::create(
        &mut b,
        &VariantFillArgs {
            color: Some(&Color::new(0.0, 1.0, 0.0, 1.0)),
        },
    );
    // `true` against the bool default of `false`, so a shifted union
    // discriminant reads the wrong arm and this override notices.
    let visible = VariantVisible::create(&mut b, &VariantVisibleArgs { value: true });

    let overrides = [
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 0,
                value_type: VariantPropValue::VariantX,
                value: Some(x.as_union_value()),
            },
        ),
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 1,
                value_type: VariantPropValue::VariantY,
                value: Some(y.as_union_value()),
            },
        ),
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 0,
                value_type: VariantPropValue::VariantWidth,
                value: Some(width.as_union_value()),
            },
        ),
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 1,
                value_type: VariantPropValue::VariantHeight,
                value: Some(height.as_union_value()),
            },
        ),
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 0,
                value_type: VariantPropValue::VariantFill,
                value: Some(fill.as_union_value()),
            },
        ),
        VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 1,
                value_type: VariantPropValue::VariantVisible,
                value: Some(visible.as_union_value()),
            },
        ),
    ];
    let overrides = b.create_vector(&overrides);
    let member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            overrides: Some(overrides),
            ..Default::default()
        },
    );
    let members = b.create_vector(&[member]);
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            ..Default::default()
        },
    );

    let bytes = finish_document(b, vec![set]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let overrides = document
        .variant_sets()
        .unwrap()
        .get(0)
        .members()
        .unwrap()
        .get(0)
        .overrides()
        .expect("overrides present");
    assert_eq!(overrides.len(), 6);

    let x = overrides.get(0);
    assert_eq!(x.node(), 0);
    assert_eq!(x.value_type(), VariantPropValue::VariantX);
    assert_eq!(x.value_as_variant_x().unwrap().value(), 11.0);

    let y = overrides.get(1);
    assert_eq!(y.node(), 1);
    assert_eq!(y.value_as_variant_y().unwrap().value(), 22.0);

    let width = overrides.get(2);
    assert_eq!(width.node(), 0);
    assert_eq!(width.value_as_variant_width().unwrap().value(), 33.0);

    let height = overrides.get(3);
    assert_eq!(height.node(), 1);
    assert_eq!(height.value_as_variant_height().unwrap().value(), 44.0);

    let fill = overrides.get(4);
    assert_eq!(fill.node(), 0);
    let color = fill.value_as_variant_fill().unwrap().color();
    assert_eq!(
        (color.r(), color.g(), color.b(), color.a()),
        (0.0, 1.0, 0.0, 1.0)
    );

    let visible = overrides.get(5);
    assert_eq!(visible.node(), 1);
    assert_eq!(visible.value_type(), VariantPropValue::VariantVisible);
    assert!(visible.value_as_variant_visible().unwrap().value());
}

#[test]
fn multiple_variant_sets_round_trip_independently() {
    let mut b = FlatBufferBuilder::new();

    let m0 = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let members_a = b.create_vector(&[m0]);
    let set_a = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members_a),
            ..Default::default()
        },
    );

    let m1 = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let m2 = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let members_b = b.create_vector(&[m1, m2]);
    let set_b = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members_b),
            active_member: 1,
        },
    );

    let bytes = finish_document(b, vec![set_a, set_b]);
    let document = root_as_document(&bytes).expect("valid dashbuf document");
    let sets = document.variant_sets().expect("variant_sets present");
    assert_eq!(sets.len(), 2);
    assert_eq!(sets.get(0).members().unwrap().len(), 1);
    assert_eq!(sets.get(0).active_member(), 0);
    assert_eq!(sets.get(1).members().unwrap().len(), 2);
    assert_eq!(sets.get(1).active_member(), 1);
}

#[test]
fn a_document_without_variant_sets_reads_back_absent() {
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
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

    let document = root_as_document(&bytes).expect("valid dashbuf document");
    assert!(document.variant_sets().is_none());
}
