//! The v0.7 binding tables (story #167), one focused test per construct,
//! matching `paint_roundtrip.rs`'s style: signal declarations (named and
//! anonymous), binding rows across every `BindingChannel`, each
//! `BindingTransform` union member, the union-NONE identity default, and
//! two rows sharing one declaration.

use dashbuf::{
    Binding, BindingArgs, BindingChannel, BindingTransform, Document, DocumentArgs, SignalDecl,
    SignalDeclArgs, TransformClamp, TransformClampArgs, TransformMapRange, TransformMapRangeArgs,
    TransformScale, TransformScaleArgs, root_as_document,
};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

fn document_with(
    b: &mut FlatBufferBuilder<'static>,
    signals: Vec<WIPOffset<SignalDecl<'static>>>,
    bindings: Vec<WIPOffset<Binding<'static>>>,
) -> Vec<u8> {
    let signals = b.create_vector(&signals);
    let bindings = b.create_vector(&bindings);
    let document = Document::create(
        b,
        &DocumentArgs {
            signals: Some(signals),
            bindings: Some(bindings),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

#[test]
fn named_and_anonymous_signal_declarations_round_trip() {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("color/accent.r");
    let named = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: Some(name),
            initial: 0.13,
        },
    );
    let anonymous = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: -3.5,
        },
    );
    let bytes = document_with(&mut b, vec![named, anonymous], Vec::new());

    let doc = root_as_document(&bytes).expect("valid document");
    let signals = doc.signals().expect("signals present");
    assert_eq!(signals.len(), 2);
    assert_eq!(signals.get(0).name(), Some("color/accent.r"));
    assert_eq!(signals.get(0).initial(), 0.13);
    assert_eq!(signals.get(1).name(), None);
    assert_eq!(signals.get(1).initial(), -3.5);
}

#[test]
fn every_binding_channel_round_trips() {
    // Iterates the generated ENUM_VALUES, so a future channel cannot be
    // silently missed here (the paint_roundtrip precedent for enums).
    let mut b = FlatBufferBuilder::new();
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 0.0,
        },
    );
    let bindings: Vec<_> = BindingChannel::ENUM_VALUES
        .iter()
        .enumerate()
        .map(|(node, &channel)| {
            Binding::create(
                &mut b,
                &BindingArgs {
                    signal: 0,
                    node: node as u32,
                    channel,
                    transform_type: BindingTransform::NONE,
                    transform: None,
                },
            )
        })
        .collect();
    let count = bindings.len();
    let bytes = document_with(&mut b, vec![signal], bindings);

    let doc = root_as_document(&bytes).expect("valid document");
    let rows = doc.bindings().expect("bindings present");
    assert_eq!(rows.len(), count);
    for (index, &channel) in BindingChannel::ENUM_VALUES.iter().enumerate() {
        assert_eq!(rows.get(index).channel(), channel);
        assert_eq!(rows.get(index).node(), index as u32);
    }
}

#[test]
fn an_absent_transform_reads_back_as_the_identity_default() {
    let mut b = FlatBufferBuilder::new();
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 1.0,
        },
    );
    let row = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 0,
            node: 0,
            channel: BindingChannel::Width,
            transform_type: BindingTransform::NONE,
            transform: None,
        },
    );
    let bytes = document_with(&mut b, vec![signal], vec![row]);

    let doc = root_as_document(&bytes).expect("valid document");
    let row = doc.bindings().expect("bindings present").get(0);
    assert_eq!(row.transform_type(), BindingTransform::NONE);
}

#[test]
fn every_transform_union_member_round_trips() {
    let mut b = FlatBufferBuilder::new();
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 1.0,
        },
    );

    let scale = TransformScale::create(&mut b, &TransformScaleArgs { factor: 2.5 });
    let map_range = TransformMapRange::create(
        &mut b,
        &TransformMapRangeArgs {
            in_lo: 0.0,
            in_hi: 10.0,
            out_lo: 5.0,
            out_hi: 105.0,
        },
    );
    let clamp = TransformClamp::create(&mut b, &TransformClampArgs { lo: -1.0, hi: 1.0 });

    let rows = vec![
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 0,
                channel: BindingChannel::X,
                transform_type: BindingTransform::TransformScale,
                transform: Some(scale.as_union_value()),
            },
        ),
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 1,
                channel: BindingChannel::Y,
                transform_type: BindingTransform::TransformMapRange,
                transform: Some(map_range.as_union_value()),
            },
        ),
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 2,
                channel: BindingChannel::Height,
                transform_type: BindingTransform::TransformClamp,
                transform: Some(clamp.as_union_value()),
            },
        ),
    ];
    let bytes = document_with(&mut b, vec![signal], rows);

    let doc = root_as_document(&bytes).expect("valid document");
    let rows = doc.bindings().expect("bindings present");

    let scale = rows.get(0);
    assert_eq!(scale.transform_type(), BindingTransform::TransformScale);
    assert_eq!(
        scale
            .transform_as_transform_scale()
            .expect("TransformScale present")
            .factor(),
        2.5
    );

    let map_range = rows.get(1);
    assert_eq!(
        map_range.transform_type(),
        BindingTransform::TransformMapRange
    );
    let m = map_range
        .transform_as_transform_map_range()
        .expect("TransformMapRange present");
    assert_eq!(
        (m.in_lo(), m.in_hi(), m.out_lo(), m.out_hi()),
        (0.0, 10.0, 5.0, 105.0)
    );

    let clamp = rows.get(2);
    assert_eq!(clamp.transform_type(), BindingTransform::TransformClamp);
    let c = clamp
        .transform_as_transform_clamp()
        .expect("TransformClamp present");
    assert_eq!((c.lo(), c.hi()), (-1.0, 1.0));
}

#[test]
fn two_rows_can_share_one_signal_declaration() {
    let mut b = FlatBufferBuilder::new();
    let name = b.create_string("speed");
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: Some(name),
            initial: 40.0,
        },
    );
    let rows = vec![
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 3,
                channel: BindingChannel::Width,
                transform_type: BindingTransform::NONE,
                transform: None,
            },
        ),
        Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 7,
                channel: BindingChannel::Gap,
                transform_type: BindingTransform::NONE,
                transform: None,
            },
        ),
    ];
    let bytes = document_with(&mut b, vec![signal], rows);

    let doc = root_as_document(&bytes).expect("valid document");
    let rows = doc.bindings().expect("bindings present");
    assert_eq!(rows.get(0).signal(), 0);
    assert_eq!(rows.get(1).signal(), 0);
    assert_eq!(doc.signals().expect("signals present").len(), 1);
}

#[test]
fn a_document_without_binding_tables_reads_back_absent() {
    let mut b = FlatBufferBuilder::new();
    let document = Document::create(&mut b, &DocumentArgs::default());
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    let doc = root_as_document(&bytes).expect("valid document");
    assert!(doc.signals().is_none());
    assert!(doc.bindings().is_none());
}
