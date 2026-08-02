//! `load_document`'s variant-table replay (story #20): "loading is a
//! straight replay of the document's nodes through the ordinary
//! producer API" (`docs/design/dashscene-core-arena.md`) extends to
//! `Document.variant_sets` — a loaded scene resolves the same rect/paint
//! tables a hand-staged `add_variant_set`/`set_variant` call would.

use dashbuf::{
    Color, Document, DocumentArgs, Fill, FixedSizeLayout, Node, NodeArgs, Paint, PaintArgs, Shadow,
    ShadowArgs, ShadowKind, SolidFill, SolidFillArgs, VariantMember, VariantMemberArgs,
    VariantOverride, VariantOverrideArgs, VariantPropValue, VariantSet, VariantSetArgs,
    VariantVisible, VariantVisibleArgs, VariantWidth, VariantWidthArgs, Vec2, root_as_document,
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
    load_document(&doc, &[], &mut arena);

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
    load_document(&doc, &[], &mut arena);

    assert_eq!(
        arena.committed().rects()[1].w,
        99.0,
        "the document's own active_member selects the override at load time"
    );
}

/// The v0.8 `VariantVisible` override (story #283) replays through
/// `load_document` like every other variant value: a document whose
/// active member hides a child must load without panicking, and the
/// loaded overlay must carry that visibility onto the child's layout.
///
/// Before the `variant_value` `VariantVisible` arm existed, this
/// document passed the load gate (the validator's own
/// `a_variant_visible_override_produces_no_diagnostics` proves it
/// validates clean) yet panicked here on the `unreachable!` wildcard —
/// the two-gate contract's exact gap.
#[test]
fn a_loaded_document_replays_a_variant_visible_override() {
    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 10.0, 10.0);
    // Node 0 is the container; node 1 is its child.
    let container = Node::create(
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
    let nodes = b.create_vector(&[container, child]);

    let default_member = VariantMember::create(&mut b, &VariantMemberArgs::default());
    let hidden = VariantVisible::create(&mut b, &VariantVisibleArgs { value: false });
    let visible_override = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 1,
            value_type: VariantPropValue::VariantVisible,
            value: Some(hidden.as_union_value()),
        },
    );
    let overrides = b.create_vector(&[visible_override]);
    let hidden_member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            overrides: Some(overrides),
            ..Default::default()
        },
    );
    let members = b.create_vector(&[default_member, hidden_member]);
    // active_member 1 selects the override at load time.
    let set = VariantSet::create(
        &mut b,
        &VariantSetArgs {
            members: Some(members),
            active_member: 1,
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
    let bytes = b.finished_data().to_vec();

    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &[], &mut arena);

    let root = arena.roots()[0];
    let child = arena.children(root)[0];
    assert!(
        !arena.layout(child).visible,
        "the active member's VariantVisible(false) override hid the child at load time"
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
    load_document(&doc, &[], &mut arena);

    assert_eq!(arena.committed().rects().len(), 1);
    assert_eq!(arena.committed().rects()[0].w, 5.0);
}

/// The v0.8 masks + group-opacity node fields (story #44) replay through
/// `set_prop` like any other intent: a loaded document's opacity, mask,
/// and visibility reach the arena's intent model.
#[test]
fn a_loaded_document_replays_masks_and_opacity() {
    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 5.0, 5.0);
    // A group (opacity 0.5), a mask child, and a hidden child.
    let group = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            opacity: 0.5,
            ..Default::default()
        },
    );
    let mask = Node::create(
        &mut b,
        &NodeArgs {
            parent: 0,
            layout: Some(&layout),
            mask: true,
            ..Default::default()
        },
    );
    let hidden = Node::create(
        &mut b,
        &NodeArgs {
            parent: 0,
            layout: Some(&layout),
            visible: false,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[group, mask, hidden]);
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
    load_document(&doc, &[], &mut arena);

    let roots = arena.roots();
    let group_id = roots[0];
    let children: Vec<_> = arena.children(group_id).to_vec();
    assert_eq!(arena.opacity(group_id), 0.5);
    assert!(
        arena.is_mask(children[0]),
        "the mask child loaded as a mask"
    );
    assert!(
        !arena.layout(children[1]).visible,
        "the hidden child loaded as not visible"
    );
}

/// A shadow on a document paint entry replays through `load_document`'s
/// `load_paint` into the committed `PaintEntry.shadows` (story #45). The
/// frozen r7 fixture decodes a shadow only through raw dashbuf accessors;
/// this exercises the `.dsb` → core seam that stages `Prop::Shadows`.
#[test]
fn a_loaded_document_replays_its_shadows() {
    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 10.0, 10.0);

    // Paint entry 0: a solid fill plus an inner shadow, every field
    // non-default so a mis-staged field is visible.
    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
        },
    );
    let shadow = Shadow::create(
        &mut b,
        &ShadowArgs {
            kind: ShadowKind::Inner,
            offset: Some(&Vec2::new(3.0, -4.0)),
            blur: 6.0,
            spread: 2.0,
            color: Some(&Color::new(0.1, 0.2, 0.3, 0.5)),
        },
    );
    let shadows = b.create_vector(&[shadow]);
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            shadows: Some(shadows),
            ..Default::default()
        },
    );
    let paints = b.create_vector(&[paint]);

    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            paint_entry: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
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

    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &[], &mut arena);

    let scene = arena.committed();
    let entry = scene.paints().resolve(scene.rects()[0].paint);
    assert_eq!(
        scene.paints().shadows(entry),
        &[dashpaint::Shadow {
            kind: dashpaint::ShadowKind::Inner,
            offset: dashpaint::Vec2 { x: 3.0, y: -4.0 },
            blur: 6.0,
            spread: 2.0,
            color: dashpaint::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.5,
            },
        }],
        "the document's shadow replayed onto the committed paint entry"
    );
}

/// A stacked fill on a document paint entry replays through `load_document`'s
/// `load_paint` into the committed `PaintEntry.extra_fills` (story C1, debt
/// #146) — the same `.dsb` -> core seam the shadows test exercises, now for
/// `Prop::ExtraFills`.
#[test]
fn a_loaded_document_replays_its_stacked_fills() {
    use dashbuf::{FillLayer, FillLayerArgs, Gradient, GradientArgs, GradientKind, GradientStop};

    let mut b = FlatBufferBuilder::new();
    let layout = FixedSizeLayout::new(0.0, 0.0, 10.0, 10.0);

    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 0.0, 0.0, 1.0)),
        },
    );
    let stops = b.create_vector(&[
        GradientStop::new(0.0, &Color::new(0.0, 1.0, 0.0, 1.0)),
        GradientStop::new(1.0, &Color::new(0.0, 0.0, 1.0, 0.55)),
    ]);
    let gradient = Gradient::create(
        &mut b,
        &GradientArgs {
            kind: GradientKind::Linear,
            handle_origin: Some(&Vec2::new(0.0, 0.0)),
            handle_primary: Some(&Vec2::new(1.0, 0.0)),
            handle_secondary: Some(&Vec2::new(0.0, 1.0)),
            stops: Some(stops),
        },
    );
    let top_layer = FillLayer::create(
        &mut b,
        &FillLayerArgs {
            fill_type: Fill::Gradient,
            fill: Some(gradient.as_union_value()),
        },
    );
    let extra_fills = b.create_vector(&[top_layer]);
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            extra_fills: Some(extra_fills),
            ..Default::default()
        },
    );
    let paints = b.create_vector(&[paint]);

    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            paint_entry: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
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

    let doc = root_as_document(&bytes).expect("valid dashbuf document");
    let mut arena = Arena::new();
    load_document(&doc, &[], &mut arena);

    let scene = arena.committed();
    let paints = scene.paints();
    let entry = paints.resolve(scene.rects()[0].paint);
    // Read through the table: an entry names its fills by row index since
    // story #578, so the assertion resolves them rather than rebuilding an
    // equal fill value.
    assert_eq!(
        entry.fill.map(|kind| paints.fill(kind)),
        Some(dashpaint::Fill::Solid(dashpaint::Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        })),
        "the bottom fill replays exactly as a single fill always has"
    );
    let stacked: Vec<_> = entry
        .extra_fills
        .iter()
        .map(|&kind| paints.fill(kind))
        .collect();
    let [dashpaint::Fill::Gradient(overlay)] = stacked.as_slice() else {
        panic!("expected one stacked gradient layer, got {stacked:?}");
    };
    assert_eq!(overlay.gradient.kind, dashpaint::GradientKind::Linear);
    assert_eq!(
        overlay.gradient.handle_origin,
        dashpaint::Vec2 { x: 0.0, y: 0.0 }
    );
    assert_eq!(
        overlay.gradient.handle_primary,
        dashpaint::Vec2 { x: 1.0, y: 0.0 }
    );
    assert_eq!(
        overlay.gradient.handle_secondary,
        dashpaint::Vec2 { x: 0.0, y: 1.0 }
    );
    assert_eq!(
        overlay.stops,
        [
            dashpaint::GradientStop {
                offset: 0.0,
                color: dashpaint::Color {
                    r: 0.0,
                    g: 1.0,
                    b: 0.0,
                    a: 1.0,
                },
            },
            dashpaint::GradientStop {
                offset: 1.0,
                color: dashpaint::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.55,
                },
            },
        ],
        "the document's stacked fill replayed onto the committed paint entry"
    );
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
    load_document(&doc, &[], &mut arena);

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
    load_document(&doc, &[], &mut arena);

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

/// Story #310: the four widened text-style axes — a fixed line height, letter
/// spacing, and horizontal/vertical alignment — replay through
/// `load_document` into the arena's `TextStyle`, the seam the runtime reads.
#[test]
fn the_text_style_metrics_and_alignment_reach_the_arena() {
    use dashbuf::{TextStyle, TextStyleArgs};
    use dashscene_core::{TextAlign, TextAlignV};

    let mut b = FlatBufferBuilder::new();
    let hi = b.create_string("Hi");
    let strings = b.create_vector(&[hi]);
    let family = b.create_string("Inter");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            size: 16.0,
            weight: 400,
            color: Some(&Color::new(0.1, 0.2, 0.3, 1.0)),
            line_height_px: Some(30.0),
            letter_spacing: 2.5,
            text_align: dashbuf::TextAlign::Center,
            text_align_v: dashbuf::TextAlignV::Bottom,
            ligatures_off: false,
        },
    );
    let text_styles = b.create_vector(&[style]);
    let layout = FixedSizeLayout::new(0.0, 0.0, 40.0, 20.0);
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            text: 0,
            text_style: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            strings: Some(strings),
            text_styles: Some(text_styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();

    let document = root_as_document(&bytes).expect("verifies");
    let mut arena = Arena::new();
    load_document(&document, &[], &mut arena);

    let root = arena.roots()[0];
    let s = arena.text_style(root).expect("the style reached the arena");
    assert_eq!(s.line_height_px, Some(30.0));
    assert_eq!(s.letter_spacing, 2.5);
    assert_eq!(s.text_align, TextAlign::Center);
    assert_eq!(s.text_align_v, TextAlignV::Bottom);
}

/// Story #341: the standard-ligatures-off bit replays through `load_document`
/// into the arena's `TextStyle`, independently of the story #310 axes.
#[test]
fn ligatures_off_reaches_the_arena() {
    use dashbuf::{TextStyle, TextStyleArgs};

    let mut b = FlatBufferBuilder::new();
    let hi = b.create_string("Hi");
    let strings = b.create_vector(&[hi]);
    let family = b.create_string("Inter");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            size: 16.0,
            weight: 400,
            color: Some(&Color::new(0.1, 0.2, 0.3, 1.0)),
            ligatures_off: true,
            ..Default::default()
        },
    );
    let text_styles = b.create_vector(&[style]);
    let layout = FixedSizeLayout::new(0.0, 0.0, 40.0, 20.0);
    let node = Node::create(
        &mut b,
        &NodeArgs {
            layout: Some(&layout),
            text: 0,
            text_style: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);
    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            strings: Some(strings),
            text_styles: Some(text_styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();

    let document = root_as_document(&bytes).expect("verifies");
    let mut arena = Arena::new();
    load_document(&document, &[], &mut arena);

    let root = arena.roots()[0];
    let s = arena.text_style(root).expect("the style reached the arena");
    assert!(s.ligatures_off);
}
