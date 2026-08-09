//! Emitting a [`Document`] as `.dsb` bytes.
//!
//! R7: same input → byte-identical document. Hashing, signing, and CI all
//! depend on it, so nothing here may depend on iteration order of a hash
//! map, on addresses, or on anything else that varies between runs. The one
//! place that could is the paint pool, and it is interned in **first-use DFS
//! order** — the same rule `dashscene-core`'s commit uses, so a document and
//! the scene it loads into agree on pool order too.

use std::collections::HashMap;

use dashbuf::{
    AssetEntry, AssetEntryArgs, AtlasRect, Binding as FbBinding, BindingArgs as FbBindingArgs,
    Blur as FbBlur, BlurArgs, BlurKind as FbBlurKind, Color, CornerRadii, Document as FbDocument,
    DocumentArgs as FbDocumentArgs, EdgeInsets as FbEdgeInsets, FillLayer as FbFillLayer,
    FillLayerArgs as FbFillLayerArgs, FixedSizeLayout, Gradient, GradientArgs, GradientStop,
    GridTrack as FbGridTrack, GridTrackArgs as FbGridTrackArgs, ImageFill, ImageFillArgs,
    Keyframe as FbKeyframe, KeyframesSpec, KeyframesSpecArgs,
    LayoutConstraints as FbLayoutConstraints, LayoutConstraintsArgs,
    LayoutContainer as FbLayoutContainer, LayoutContainerArgs, LoopTrack as FbLoopTrack,
    LoopTrackArgs, Mat23, NO_FIELD, NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE, Node as FbNode,
    NodeArgs as FbNodeArgs, Paint as BufPaint, PaintArgs, PlaneBounds,
    PropTransition as FbPropTransition, PropTransitionArgs, Shadow as FbShadow, ShadowArgs,
    ShadowKind as FbShadowKind, SignalDecl as FbSignalDecl, SignalDeclArgs as FbSignalDeclArgs,
    SolidFill, SolidFillArgs, SpringSpec, SpringSpecArgs, Stroke, StrokeArgs,
    TextStyle as FbTextStyle, TextStyleArgs, TransformClamp, TransformClampArgs, TransformMapRange,
    TransformMapRangeArgs, TransformScale, TransformScaleArgs, TweenSpec, TweenSpecArgs,
    VariantFill as FbVariantFill, VariantFillArgs, VariantHeight as FbVariantHeight,
    VariantHeightArgs, VariantMember as FbVariantMember, VariantMemberArgs,
    VariantOverride as FbVariantOverride, VariantOverrideArgs,
    VariantRotation as FbVariantRotation, VariantRotationArgs, VariantSet as FbVariantSet,
    VariantSetArgs, VariantTransition as FbVariantTransition, VariantTransitionArgs,
    VariantVisible as FbVariantVisible, VariantVisibleArgs, VariantWidth as FbVariantWidth,
    VariantWidthArgs, VariantX as FbVariantX, VariantXArgs, VariantY as FbVariantY, VariantYArgs,
    Vec2, VectorAtlas as FbVectorAtlas, VectorAtlasArgs, VectorShape as FbVectorShape,
    VectorShapeArgs,
};
use dashpaint::{BlurKind, FillSpec, ShadowKind};
use flatbuffers::{FlatBufferBuilder, UnionWIPOffset, WIPOffset};

use crate::document::{
    Asset, AssetKind, AxisSizing, Binding, BindingChannel, BindingTransform, CrossAxisAlign,
    Document, Easing, EdgeInsets, GridTrack, LayoutMode, LoopTrack, MainAxisAlign, Node, Paint,
    PaintEntry, PropTransition, SignalDecl, TextAlign, TextAlignV, TextStyle, TransitionSpec,
    VariantMember, VariantOverride, VariantSet, VariantTransition, VariantValue, VectorAtlas,
    VectorShape,
};

/// Serializes a document to `.dsb` bytes.
///
/// Deterministic: the same [`Document`] always produces the same bytes (R7).
pub fn emit(doc: &Document) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();

    // The paint pool, interned in first-use DFS order. A `Vec` of keys, not
    // a HashMap iteration, decides the pool's order — a HashMap's order is
    // unspecified and would make the bytes vary between runs, breaking R7.
    let mut pool: Vec<&Paint> = Vec::new();
    let mut pool_of: HashMap<PaintKey, u32> = HashMap::new();
    let mut entry_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());

    // The string and text-style pools intern in the same first-use DFS order,
    // for the same R7 reason: two text nodes sharing a string or a style
    // share one pool entry, and the order is the node walk's, never a hash
    // map's. `docs/design/dashbuf.md`: "Dedup is the producer's job; the pool
    // makes it representable."
    let mut strings: Vec<&str> = Vec::new();
    let mut string_of_pool: HashMap<&str, u32> = HashMap::new();
    let mut string_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());
    let mut styles: Vec<&TextStyle> = Vec::new();
    let mut style_of_pool: HashMap<TextStyleKey, u32> = HashMap::new();
    let mut style_of: Vec<Option<u32>> = Vec::with_capacity(doc.nodes.len());

    for node in &doc.nodes {
        entry_of.push(node.paint.as_ref().map(|paint| {
            let key = paint_key(paint);
            *pool_of.entry(key).or_insert_with(|| {
                let index = u32::try_from(pool.len()).expect("paint pool exceeds u32::MAX");
                pool.push(paint);
                index
            })
        }));
        string_of.push(node.text.as_deref().map(|text| {
            *string_of_pool.entry(text).or_insert_with(|| {
                let index = u32::try_from(strings.len()).expect("string pool exceeds u32::MAX");
                strings.push(text);
                index
            })
        }));
        style_of.push(node.text_style.as_ref().map(|style| {
            let key = text_style_key(style);
            *style_of_pool.entry(key).or_insert_with(|| {
                let index = u32::try_from(styles.len()).expect("text-style pool exceeds u32::MAX");
                styles.push(style);
                index
            })
        }));
    }

    let assets: Vec<WIPOffset<AssetEntry>> =
        doc.assets.iter().map(|a| build_asset(&mut b, a)).collect();
    let paints: Vec<WIPOffset<BufPaint>> = pool.iter().map(|p| build_paint(&mut b, p)).collect();
    let string_offsets: Vec<WIPOffset<&str>> = strings.iter().map(|s| b.create_string(s)).collect();
    let style_offsets: Vec<WIPOffset<FbTextStyle>> =
        styles.iter().map(|s| build_text_style(&mut b, s)).collect();
    let nodes: Vec<WIPOffset<FbNode>> = doc
        .nodes
        .iter()
        .zip(&entry_of)
        .zip(string_of.iter().zip(&style_of))
        .map(|((node, entry), (text, style))| build_node(&mut b, node, *entry, *text, *style))
        .collect();

    // The variant table (v0.4, story #20). Empty for a document with no
    // switchable states, so the create_vector below is skipped and every
    // document authored before this emitter existed emits byte-identically
    // (R7) — the same absence rule the baked-vector pools follow.
    let variant_set_offsets: Vec<WIPOffset<FbVariantSet>> = doc
        .variant_sets
        .iter()
        .map(|set| build_variant_set(&mut b, set))
        .collect();
    let signal_offsets: Vec<WIPOffset<FbSignalDecl>> = doc
        .signals
        .iter()
        .map(|s| build_signal(&mut b, s))
        .collect();
    let binding_offsets: Vec<WIPOffset<FbBinding>> = doc
        .bindings
        .iter()
        .map(|row| build_binding(&mut b, row))
        .collect();
    // The baked-vector pools (story B1). Both are empty for a document with
    // no vectors, so the create_vector below is skipped and a pre-B1 document
    // emits byte-identically (R7).
    let vector_atlas_offsets: Vec<WIPOffset<FbVectorAtlas>> = doc
        .vector_atlases
        .iter()
        .map(|a| build_vector_atlas(&mut b, a))
        .collect();
    let vector_shape_offsets: Vec<WIPOffset<FbVectorShape>> = doc
        .vector_shapes
        .iter()
        .map(|s| build_vector_shape(&mut b, s))
        .collect();
    // The ambient animations (story #772). Empty for a document with no
    // loop, so the create_vector below is skipped and every document
    // authored before this field emits byte-identically (R7).
    let loop_offsets: Vec<WIPOffset<FbLoopTrack>> = doc
        .loops
        .iter()
        .map(|track| build_loop_track(&mut b, track))
        .collect();

    let nodes = b.create_vector(&nodes);
    let assets = (!assets.is_empty()).then(|| b.create_vector(&assets));
    let paints = (!paints.is_empty()).then(|| b.create_vector(&paints));
    let strings = (!string_offsets.is_empty()).then(|| b.create_vector(&string_offsets));
    let text_styles = (!style_offsets.is_empty()).then(|| b.create_vector(&style_offsets));
    let variant_sets =
        (!variant_set_offsets.is_empty()).then(|| b.create_vector(&variant_set_offsets));
    let signals = (!signal_offsets.is_empty()).then(|| b.create_vector(&signal_offsets));
    let bindings = (!binding_offsets.is_empty()).then(|| b.create_vector(&binding_offsets));
    let vector_atlases =
        (!vector_atlas_offsets.is_empty()).then(|| b.create_vector(&vector_atlas_offsets));
    let vector_shapes =
        (!vector_shape_offsets.is_empty()).then(|| b.create_vector(&vector_shape_offsets));
    let loops = (!loop_offsets.is_empty()).then(|| b.create_vector(&loop_offsets));

    let document = FbDocument::create(
        &mut b,
        &FbDocumentArgs {
            nodes: Some(nodes),
            assets,
            paints,
            strings,
            text_styles,
            variant_sets,
            signals,
            bindings,
            vector_atlases,
            vector_shapes,
            loops,
            // No `..Default::default()`: every field is named, so clippy's
            // needless_update refuses the rest pattern. The next schema
            // append restores it.
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

/// Builds one variant set and everything under it (v0.4, story #20).
///
/// Member order is the authored order — the emitter does not sort or dedup
/// — because selection is a flat index into that list
/// (`docs/decisions/variant-set-flat-index.md`): reordering here would
/// silently repoint `active_member` and every `set_variant` call a runtime
/// makes. An empty member list emits as the schema's absent vector rather
/// than an empty one, and `dashscene_validator`'s load gate names it
/// (`variant set has no members`) rather than this emitter refusing it —
/// one gate, not two that could disagree.
fn build_variant_set<'a>(
    b: &mut FlatBufferBuilder<'a>,
    set: &VariantSet,
) -> WIPOffset<FbVariantSet<'a>> {
    let members: Vec<WIPOffset<FbVariantMember>> = set
        .members
        .iter()
        .map(|member| build_variant_member(b, member))
        .collect();
    let members = (!members.is_empty()).then(|| b.create_vector(&members));
    FbVariantSet::create(
        b,
        &VariantSetArgs {
            members,
            active_member: set.active_member,
        },
    )
}

/// Builds one variant member. An unnamed member writes no `name` slot: the
/// schema's string is optional, and a producer that has no name for a state
/// (a hand-authored set, an SVG one) carries none rather than an empty one.
fn build_variant_member<'a>(
    b: &mut FlatBufferBuilder<'a>,
    member: &VariantMember,
) -> WIPOffset<FbVariantMember<'a>> {
    // Both children are created before the member table is started, which is
    // what flatbuffers requires: a table under construction may not have
    // another object built inside it.
    let name = member.name.as_deref().map(|name| b.create_string(name));
    let overrides: Vec<WIPOffset<FbVariantOverride>> = member
        .overrides
        .iter()
        .map(|entry| build_variant_override(b, entry))
        .collect();
    let overrides = (!overrides.is_empty()).then(|| b.create_vector(&overrides));
    let transition = member
        .transition
        .as_ref()
        .map(|transition| build_variant_transition(b, transition));
    FbVariantMember::create(
        b,
        &VariantMemberArgs {
            name,
            overrides,
            transition,
        },
    )
}

/// Builds one variant transition and the per-prop tracks under it (story
/// #771). Track order is the authored order: a track's stagger delay is
/// `stagger * i` over its declared index, so reordering here would re-time
/// every track after the first.
fn build_variant_transition<'a>(
    b: &mut FlatBufferBuilder<'a>,
    transition: &VariantTransition,
) -> WIPOffset<FbVariantTransition<'a>> {
    let tracks: Vec<WIPOffset<FbPropTransition>> = transition
        .tracks
        .iter()
        .map(|track| build_prop_transition(b, track))
        .collect();
    let tracks = (!tracks.is_empty()).then(|| b.create_vector(&tracks));
    FbVariantTransition::create(
        b,
        &VariantTransitionArgs {
            tracks,
            stagger: transition.stagger,
        },
    )
}

/// Builds one track: the `(node, channel)` pair the loader packs into a
/// `PropKey`, and the curve it travels.
fn build_prop_transition<'a>(
    b: &mut FlatBufferBuilder<'a>,
    track: &PropTransition,
) -> WIPOffset<FbPropTransition<'a>> {
    let (spec_type, spec) = build_spec(b, &track.spec);
    FbPropTransition::create(
        b,
        &PropTransitionArgs {
            node: track.node,
            channel: channel_of(track.channel),
            spec_type,
            spec: Some(spec),
        },
    )
}

/// Builds one ambient animation (story #772): the same `(node, channel)`
/// pair a transition track carries, its own authored endpoints, and where
/// in its cycle it starts.
fn build_loop_track<'a>(
    b: &mut FlatBufferBuilder<'a>,
    track: &LoopTrack,
) -> WIPOffset<FbLoopTrack<'a>> {
    let (spec_type, spec) = build_spec(b, &track.spec);
    FbLoopTrack::create(
        b,
        &LoopTrackArgs {
            node: track.node,
            channel: channel_of(track.channel),
            from: track.from,
            to: track.to,
            spec_type,
            spec: Some(spec),
            phase_offset: track.phase_offset,
        },
    )
}

/// Builds the `TransitionSpec` union, returning the discriminant and the
/// union value to store beside it.
///
/// Shared by the two tables that carry the union — a variant transition's
/// track and a loop track — so a fourth arm cannot land in one emitter and
/// not the other. The union's children are built before the table that
/// owns them, which is what a flatbuffer builder requires.
fn build_spec<'a>(
    b: &mut FlatBufferBuilder<'a>,
    spec: &TransitionSpec,
) -> (dashbuf::TransitionSpec, WIPOffset<UnionWIPOffset>) {
    match spec {
        TransitionSpec::Tween { duration, easing } => (
            dashbuf::TransitionSpec::TweenSpec,
            TweenSpec::create(
                b,
                &TweenSpecArgs {
                    duration: *duration,
                    easing: easing_of(*easing),
                },
            )
            .as_union_value(),
        ),
        TransitionSpec::Spring {
            stiffness,
            damping_ratio,
        } => (
            dashbuf::TransitionSpec::SpringSpec,
            SpringSpec::create(
                b,
                &SpringSpecArgs {
                    stiffness: *stiffness,
                    damping_ratio: *damping_ratio,
                },
            )
            .as_union_value(),
        ),
        TransitionSpec::Keyframes { duration, frames } => {
            // `Keyframe` is an inline struct, so the frame list is a vector
            // of values rather than of offsets — built before the table that
            // owns it, like every other child here.
            let frames: Vec<FbKeyframe> = frames
                .iter()
                .map(|frame| FbKeyframe::new(frame.t, frame.value))
                .collect();
            let frames = (!frames.is_empty()).then(|| b.create_vector(&frames));
            (
                dashbuf::TransitionSpec::KeyframesSpec,
                KeyframesSpec::create(
                    b,
                    &KeyframesSpecArgs {
                        duration: *duration,
                        frames,
                    },
                )
                .as_union_value(),
            )
        }
    }
}

fn easing_of(easing: Easing) -> dashbuf::Easing {
    match easing {
        Easing::Linear => dashbuf::Easing::Linear,
        Easing::EaseIn => dashbuf::Easing::EaseIn,
        Easing::EaseOut => dashbuf::Easing::EaseOut,
        Easing::EaseInOut => dashbuf::Easing::EaseInOut,
    }
}

/// Builds one variant override — the `(node, prop, value)` triple, with the
/// value as the schema's `VariantPropValue` union arm.
///
/// `value` is required by the schema, so every arm produces one: an
/// override naming no prop has no meaning, and the load gate rejects it the
/// same way it rejects a `VariantFill` with no colour.
fn build_variant_override<'a>(
    b: &mut FlatBufferBuilder<'a>,
    entry: &VariantOverride,
) -> WIPOffset<FbVariantOverride<'a>> {
    let (value_type, value) = match entry.value {
        VariantValue::X(value) => (
            dashbuf::VariantPropValue::VariantX,
            FbVariantX::create(b, &VariantXArgs { value }).as_union_value(),
        ),
        VariantValue::Y(value) => (
            dashbuf::VariantPropValue::VariantY,
            FbVariantY::create(b, &VariantYArgs { value }).as_union_value(),
        ),
        VariantValue::Width(value) => (
            dashbuf::VariantPropValue::VariantWidth,
            FbVariantWidth::create(b, &VariantWidthArgs { value }).as_union_value(),
        ),
        VariantValue::Height(value) => (
            dashbuf::VariantPropValue::VariantHeight,
            FbVariantHeight::create(b, &VariantHeightArgs { value }).as_union_value(),
        ),
        VariantValue::Fill(color) => (
            dashbuf::VariantPropValue::VariantFill,
            FbVariantFill::create(
                b,
                &VariantFillArgs {
                    color: Some(&color_of(color)),
                },
            )
            .as_union_value(),
        ),
        VariantValue::Visible(value) => (
            dashbuf::VariantPropValue::VariantVisible,
            FbVariantVisible::create(b, &VariantVisibleArgs { value }).as_union_value(),
        ),
        VariantValue::Rotation {
            angle,
            anchor: (anchor_x, anchor_y),
        } => (
            dashbuf::VariantPropValue::VariantRotation,
            FbVariantRotation::create(
                b,
                &VariantRotationArgs {
                    angle,
                    anchor_x,
                    anchor_y,
                },
            )
            .as_union_value(),
        ),
    };
    FbVariantOverride::create(
        b,
        &VariantOverrideArgs {
            node: entry.node,
            value_type,
            value: Some(value),
        },
    )
}

fn build_signal<'a>(
    b: &mut FlatBufferBuilder<'a>,
    signal: &SignalDecl,
) -> WIPOffset<FbSignalDecl<'a>> {
    let name = b.create_string(&signal.name);
    FbSignalDecl::create(
        b,
        &FbSignalDeclArgs {
            name: Some(name),
            initial: signal.initial,
        },
    )
}

/// Builds one binding row. Identity is the union-NONE default, so the
/// common Figma-authored row costs no transform table.
///
/// # Panics
///
/// Panics on a `Custom` transform: a closure does not serialize, and the
/// gate in [`crate::compile`] names it (`binding.custom-transform`)
/// before this emitter ever runs — reaching here means the caller
/// skipped the gate (validated upstream, P4).
fn build_binding<'a>(b: &mut FlatBufferBuilder<'a>, row: &Binding) -> WIPOffset<FbBinding<'a>> {
    let (transform_type, transform) = match row.transform {
        BindingTransform::Identity => (dashbuf::BindingTransform::NONE, None),
        BindingTransform::Scale(factor) => (
            dashbuf::BindingTransform::TransformScale,
            Some(TransformScale::create(b, &TransformScaleArgs { factor }).as_union_value()),
        ),
        BindingTransform::MapRange {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => (
            dashbuf::BindingTransform::TransformMapRange,
            Some(
                TransformMapRange::create(
                    b,
                    &TransformMapRangeArgs {
                        in_lo,
                        in_hi,
                        out_lo,
                        out_hi,
                    },
                )
                .as_union_value(),
            ),
        ),
        BindingTransform::Clamp { lo, hi } => (
            dashbuf::BindingTransform::TransformClamp,
            Some(TransformClamp::create(b, &TransformClampArgs { lo, hi }).as_union_value()),
        ),
        BindingTransform::Custom(id) => panic!(
            "binding carries Custom transform (closure {id}); a closure does not serialize, and \
             compile's binding.custom-transform gate refuses it before emission (P4)"
        ),
    };
    FbBinding::create(
        b,
        &FbBindingArgs {
            signal: row.signal,
            node: row.node,
            channel: channel_of(row.channel),
            transform_type,
            transform,
        },
    )
}

fn channel_of(channel: BindingChannel) -> dashbuf::BindingChannel {
    match channel {
        BindingChannel::X => dashbuf::BindingChannel::X,
        BindingChannel::Y => dashbuf::BindingChannel::Y,
        BindingChannel::Width => dashbuf::BindingChannel::Width,
        BindingChannel::Height => dashbuf::BindingChannel::Height,
        BindingChannel::Gap => dashbuf::BindingChannel::Gap,
        BindingChannel::FillR => dashbuf::BindingChannel::FillR,
        BindingChannel::FillG => dashbuf::BindingChannel::FillG,
        BindingChannel::FillB => dashbuf::BindingChannel::FillB,
        BindingChannel::FillA => dashbuf::BindingChannel::FillA,
        BindingChannel::Opacity => dashbuf::BindingChannel::Opacity,
        BindingChannel::Rotation => dashbuf::BindingChannel::Rotation,
        BindingChannel::RotationAnchorX => dashbuf::BindingChannel::RotationAnchorX,
        BindingChannel::RotationAnchorY => dashbuf::BindingChannel::RotationAnchorY,
    }
}

fn build_node<'a>(
    b: &mut FlatBufferBuilder<'a>,
    node: &Node,
    paint_entry: Option<u32>,
    text: Option<u32>,
    text_style: Option<u32>,
) -> WIPOffset<FbNode<'a>> {
    let name = node.name.as_deref().map(|n| b.create_string(n));

    // The flex tables. Absent stays absent — `container: None` is the
    // schema's mode-`None` state and `constraints: None` its fully-default
    // state — so a fixed-layout document emits the same bytes it did
    // before the flex vocabulary was carried (R7: the frozen goldens
    // hold). `container` is read by reference: the v0.8 grid track lists
    // make `LayoutContainer` non-`Copy`.
    let flex = node.container.as_ref().map(|c| {
        // The grid track vectors go in before the container table, since
        // both borrow the builder. Empty lists (a non-grid container)
        // write the schema's absent field, so a pre-v0.8 document is
        // byte-identical (R7).
        let grid_rows = build_grid_tracks(b, &c.grid_rows);
        let grid_columns = build_grid_tracks(b, &c.grid_columns);
        let padding = insets(c.padding);
        FbLayoutContainer::create(
            b,
            &LayoutContainerArgs {
                mode: match c.mode {
                    LayoutMode::Horizontal => dashbuf::LayoutMode::Horizontal,
                    LayoutMode::Vertical => dashbuf::LayoutMode::Vertical,
                    LayoutMode::Wrap => dashbuf::LayoutMode::Wrap,
                    LayoutMode::Grid => dashbuf::LayoutMode::Grid,
                },
                gap: c.gap,
                padding: (c.padding != EdgeInsets::default()).then_some(&padding),
                main_align: match c.main_align {
                    MainAxisAlign::Start => dashbuf::MainAxisAlign::Start,
                    MainAxisAlign::Center => dashbuf::MainAxisAlign::Center,
                    MainAxisAlign::End => dashbuf::MainAxisAlign::End,
                    MainAxisAlign::SpaceBetween => dashbuf::MainAxisAlign::SpaceBetween,
                },
                cross_align: match c.cross_align {
                    CrossAxisAlign::Start => dashbuf::CrossAxisAlign::Start,
                    CrossAxisAlign::Center => dashbuf::CrossAxisAlign::Center,
                    CrossAxisAlign::End => dashbuf::CrossAxisAlign::End,
                    CrossAxisAlign::Baseline => dashbuf::CrossAxisAlign::Baseline,
                },
                // v0.8 schema appends (story #43, lowered at story #264).
                // Absent `cross_gap` and empty track lists stay absent.
                cross_gap: c.cross_gap,
                grid_rows,
                grid_columns,
            },
        )
    });
    let constraints = node.constraints.map(|c| {
        let margin = insets(c.margin);
        FbLayoutConstraints::create(
            b,
            &LayoutConstraintsArgs {
                sizing_h: axis_sizing(c.sizing_h),
                sizing_v: axis_sizing(c.sizing_v),
                min_width: c.min_width,
                max_width: c.max_width,
                min_height: c.min_height,
                max_height: c.max_height,
                margin: (c.margin != EdgeInsets::default()).then_some(&margin),
                // v0.8 grid placement (story #43, lowered at story #264).
                // An absent anchor stays absent (auto-placement); unit
                // spans equal the schema default and are omitted.
                grid_row: c.grid_row,
                grid_column: c.grid_column,
                grid_row_span: c.grid_row_span,
                grid_column_span: c.grid_column_span,
            },
        )
    });

    FbNode::create(
        b,
        &FbNodeArgs {
            name,
            parent: node.parent.unwrap_or(NO_PARENT),
            layout: Some(&FixedSizeLayout::new(
                node.box2d.x,
                node.box2d.y,
                node.box2d.width,
                node.box2d.height,
            )),
            // The v0.1 `paint` shorthand is never written: `paint_entry`
            // supersedes it, and writing both is a producer error the load
            // gate names (`paint.conflicting-representation`).
            paint_entry: paint_entry.unwrap_or(NO_PAINT),
            // A non-text node leaves both at the sentinel — the schema
            // default, so it is omitted from the buffer and a text-free
            // document emits the bytes it did before this vocabulary (R7).
            text: text.unwrap_or(NO_TEXT),
            text_style: text_style.unwrap_or(NO_TEXT_STYLE),
            flex,
            constraints,
            // v0.8 (story #44). Each equals its schema default for an
            // opaque, unmasked, visible node, so flatc omits it and a
            // pre-v0.8 document emits the same bytes (R7).
            opacity: node.opacity,
            mask: node.mask,
            visible: node.visible,
            // v0.18 (story #770). All three equal their schema default for
            // an unrotated node, so flatc omits them and a document with no
            // rotation emits the bytes it did before this vocabulary (R7).
            // The anchor is written whenever it is non-zero even at a zero
            // angle: it is the node's stated turning point, and dropping it
            // would silently re-anchor a later binding that drives only the
            // angle.
            rotation: node.rotation,
            rotation_anchor_x: node.rotation_anchor.0,
            rotation_anchor_y: node.rotation_anchor.1,
            ..Default::default()
        },
    )
}

/// Builds one `TextStyle` pool entry. The color is always written: the
/// lowering never emits a color-less style (a text node with no solid fill is
/// refused at the walk), and the loader treats an absent color as a producer
/// error (`text.style-no-color`, P4). The four v0.9 axes (story #310) and
/// #341's `ligatures_off` write their value; each equals its schema default
/// for a plain style (auto line height, zero spacing, Left/Top, ligatures
/// on), so flatc omits it and a pre-#310 document emits byte-identically (R7).
fn build_text_style<'a>(
    b: &mut FlatBufferBuilder<'a>,
    style: &TextStyle,
) -> WIPOffset<FbTextStyle<'a>> {
    let family = b.create_string(&style.family);
    FbTextStyle::create(
        b,
        &TextStyleArgs {
            family: Some(family),
            size: style.size,
            weight: style.weight,
            color: Some(&color_of(style.color)),
            line_height_px: style.line_height_px,
            letter_spacing: style.letter_spacing,
            text_align: text_align_of(style.text_align),
            text_align_v: text_align_v_of(style.text_align_v),
            ligatures_off: style.ligatures_off,
        },
    )
}

fn text_align_of(align: TextAlign) -> dashbuf::TextAlign {
    match align {
        TextAlign::Left => dashbuf::TextAlign::Left,
        TextAlign::Center => dashbuf::TextAlign::Center,
        TextAlign::Right => dashbuf::TextAlign::Right,
    }
}

fn text_align_v_of(align: TextAlignV) -> dashbuf::TextAlignV {
    match align {
        TextAlignV::Top => dashbuf::TextAlignV::Top,
        TextAlignV::Center => dashbuf::TextAlignV::Center,
        TextAlignV::Bottom => dashbuf::TextAlignV::Bottom,
    }
}

fn axis_sizing(sizing: AxisSizing) -> dashbuf::AxisSizing {
    match sizing {
        AxisSizing::Fixed => dashbuf::AxisSizing::Fixed,
        AxisSizing::Hug => dashbuf::AxisSizing::Hug,
        AxisSizing::Fill => dashbuf::AxisSizing::Fill,
    }
}

fn insets(e: EdgeInsets) -> FbEdgeInsets {
    FbEdgeInsets::new(e.left, e.top, e.right, e.bottom)
}

/// Builds a grid track vector, or `None` for an empty list — the schema's
/// absent field, so a non-grid container writes no track vector (R7). Each
/// track is a `GridTrack` table (sizing + value), so the vector is a vector
/// of offsets, built before the enclosing `LayoutContainer` since both
/// borrow the builder.
fn build_grid_tracks<'a>(
    b: &mut FlatBufferBuilder<'a>,
    tracks: &[GridTrack],
) -> Option<WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<FbGridTrack<'a>>>>> {
    if tracks.is_empty() {
        return None;
    }
    let offsets: Vec<WIPOffset<FbGridTrack>> = tracks
        .iter()
        .map(|track| {
            let (sizing, value) = match *track {
                GridTrack::Fixed(v) => (dashbuf::GridTrackSizing::Fixed, v),
                GridTrack::Fraction(v) => (dashbuf::GridTrackSizing::Fraction, v),
            };
            FbGridTrack::create(b, &FbGridTrackArgs { sizing, value })
        })
        .collect();
    Some(b.create_vector(&offsets))
}

/// Builds one `AssetEntry`: the payload's identity and the metadata a runtime
/// needs before the payload is resident. The bytes themselves are not here —
/// they leave as a blob section of the file's envelope (story #107).
fn build_asset<'a>(b: &mut FlatBufferBuilder<'a>, asset: &Asset) -> WIPOffset<AssetEntry<'a>> {
    let hash = b.create_vector(&asset.hash());
    AssetEntry::create(
        b,
        &AssetEntryArgs {
            hash: Some(hash),
            // A document records the **canonical** payload's format and
            // nothing else. `dashpack` derives baked payloads beside the
            // document and never rewrites it
            // (`docs/decisions/asset-model-content-addressed-blobs.md`), so a
            // baked format arriving here would mean a derivation had been
            // written back into the source of truth. Refused by name rather
            // than mapped to something close: `dashbuf::ImageFormat` has no
            // baked variant precisely because the document must not carry one.
            format: match asset.format {
                dashpaint::ImageFormat::Png => dashbuf::ImageFormat::Png,
                dashpaint::ImageFormat::Jpeg => dashbuf::ImageFormat::Jpeg,
                dashpaint::ImageFormat::Gif => dashbuf::ImageFormat::Gif,
                baked => panic!(
                    "asset {:?} is {baked:?}, a baked payload: a document carries canonical \
                     payloads only, and a derivation is selected at load time",
                    asset.hash()
                ),
            },
            // `Image` is the schema default, so an image asset writes no slot
            // for this field and the committed `.dsb` bytes are unmoved by its
            // arrival (story #432).
            kind: match asset.kind {
                AssetKind::Image => dashbuf::AssetKind::Image,
                AssetKind::DistanceField => dashbuf::AssetKind::DistanceField,
            },
            width: asset.width,
            height: asset.height,
        },
    )
}

/// Builds one `VectorAtlas` pool entry (story B1).
fn build_vector_atlas<'a>(
    b: &mut FlatBufferBuilder<'a>,
    atlas: &VectorAtlas,
) -> WIPOffset<FbVectorAtlas<'a>> {
    FbVectorAtlas::create(
        b,
        &VectorAtlasArgs {
            image: atlas.image,
            px_per_em: atlas.px_per_em,
            distance_range: atlas.distance_range,
        },
    )
}

/// Builds one `VectorShape` pool entry (story B1). `AtlasRect` and
/// `PlaneBounds` are inline structs, set on the args by reference.
fn build_vector_shape<'a>(
    b: &mut FlatBufferBuilder<'a>,
    shape: &VectorShape,
) -> WIPOffset<FbVectorShape<'a>> {
    let [x, y, width, height] = shape.atlas_rect;
    let [left, top, right, bottom] = shape.plane_bounds;
    FbVectorShape::create(
        b,
        &VectorShapeArgs {
            atlas: shape.atlas,
            atlas_rect: Some(&AtlasRect::new(x, y, width, height)),
            plane_bounds: Some(&PlaneBounds::new(left, top, right, bottom)),
        },
    )
}

/// Builds one fill union value. `Paint.fill` and each stacked `FillLayer.fill`
/// (story C1, debt #146) are the same union in the same shape, so
/// `build_paint` shares this rather than duplicating the match per layer.
fn build_fill<'a>(
    b: &mut FlatBufferBuilder<'a>,
    spec: &FillSpec,
) -> (dashbuf::Fill, WIPOffset<UnionWIPOffset>) {
    match spec {
        FillSpec::Solid { color } => {
            let solid = SolidFill::create(
                b,
                &SolidFillArgs {
                    color: Some(&color_of(*color)),
                },
            );
            (dashbuf::Fill::SolidFill, solid.as_union_value())
        }
        FillSpec::Gradient { gradient, stops } => {
            let fb_stops: Vec<GradientStop> = stops
                .iter()
                .map(|s| GradientStop::new(s.offset, &color_of(s.color)))
                .collect();
            let fb_stops = b.create_vector(&fb_stops);
            let fb_gradient = Gradient::create(
                b,
                &GradientArgs {
                    kind: match gradient.kind {
                        dashpaint::GradientKind::Linear => dashbuf::GradientKind::Linear,
                        dashpaint::GradientKind::Radial => dashbuf::GradientKind::Radial,
                        dashpaint::GradientKind::Angular => dashbuf::GradientKind::Angular,
                        dashpaint::GradientKind::Diamond => dashbuf::GradientKind::Diamond,
                    },
                    handle_origin: Some(&vec2_of(gradient.handle_origin)),
                    handle_primary: Some(&vec2_of(gradient.handle_primary)),
                    handle_secondary: Some(&vec2_of(gradient.handle_secondary)),
                    stops: Some(fb_stops),
                },
            );
            (dashbuf::Fill::Gradient, fb_gradient.as_union_value())
        }
        FillSpec::Image(image_fill) => {
            let dashpaint::ImageFill {
                image,
                scale_mode,
                transform,
                tile_scale,
            } = image_fill;
            // `Mat23::IDENTITY` is what an absent `transform` means since
            // story #578 removed `ImageFill::transform`'s `Option`; write
            // the field only when it differs, matching the bytes the old
            // `Option` produced (R7).
            let transform = (*transform != dashpaint::Mat23::IDENTITY).then(|| mat23_of(transform));
            let fb_image_fill = ImageFill::create(
                b,
                &ImageFillArgs {
                    image: *image,
                    scale_mode: match scale_mode {
                        dashpaint::ScaleMode::Fill => dashbuf::ScaleMode::Fill,
                        dashpaint::ScaleMode::Fit => dashbuf::ScaleMode::Fit,
                        dashpaint::ScaleMode::Crop => dashbuf::ScaleMode::Crop,
                        dashpaint::ScaleMode::Tile => dashbuf::ScaleMode::Tile,
                    },
                    transform: transform.as_ref(),
                    tile_scale: *tile_scale,
                },
            );
            (dashbuf::Fill::ImageFill, fb_image_fill.as_union_value())
        }
    }
}

fn build_paint<'a>(b: &mut FlatBufferBuilder<'a>, paint: &Paint) -> WIPOffset<BufPaint<'a>> {
    let entry = &paint.entry;

    let (fill_type, fill) = match &entry.fill {
        None => (dashbuf::Fill::NONE, None),
        Some(kind) => {
            let (fill_type, fill) = build_fill(b, kind);
            (fill_type, Some(fill))
        }
    };

    // Stacked fills (story C1, debt #146): each layer built before the
    // vector, and the vector before the enclosing Paint — the standard
    // flatbuffer nesting order, like `shadows` below. Absent (an empty list)
    // omits the field, so a single-fill entry round-trips identically (R7).
    let extra_fills = (!entry.extra_fills.is_empty()).then(|| {
        let layers: Vec<_> = entry
            .extra_fills
            .iter()
            .map(|kind| {
                let (fill_type, fill) = build_fill(b, kind);
                FbFillLayer::create(
                    b,
                    &FbFillLayerArgs {
                        fill_type,
                        fill: Some(fill),
                    },
                )
            })
            .collect();
        b.create_vector(&layers)
    });

    let stroke = entry.stroke.as_ref().map(|s| {
        Stroke::create(
            b,
            &StrokeArgs {
                width: s.width,
                align: match s.align {
                    dashpaint::StrokeAlign::Inside => dashbuf::StrokeAlign::Inside,
                    dashpaint::StrokeAlign::Center => dashbuf::StrokeAlign::Center,
                    dashpaint::StrokeAlign::Outside => dashbuf::StrokeAlign::Outside,
                },
                color: Some(&color_of(s.color)),
            },
        )
    });

    // Sharp corners are the overwhelmingly common case, and `Paint.corners`
    // is optional — so an all-zero radii struct is 16 bytes of zeros in every
    // sharp-cornered entry. Omit it; the loader already reads an absent
    // `corners` as the default, so this round-trips identically.
    let corners = (entry.corners != dashpaint::CornerRadii::default()).then(|| {
        CornerRadii::new(
            entry.corners.top_left,
            entry.corners.top_right,
            entry.corners.bottom_right,
            entry.corners.bottom_left,
        )
    });

    // Shadows (story #45). Each Shadow table is built before the vector, and
    // the vector before the enclosing Paint — the standard flatbuffer nesting
    // order. Absent (an empty list) omits the field, so a shadow-less entry
    // round-trips identically (R7).
    let shadows = (!paint.shadows.is_empty()).then(|| {
        let shadows: Vec<_> = paint
            .shadows
            .iter()
            .map(|s| {
                FbShadow::create(
                    b,
                    &ShadowArgs {
                        kind: match s.kind {
                            ShadowKind::Drop => FbShadowKind::Drop,
                            ShadowKind::Inner => FbShadowKind::Inner,
                        },
                        offset: Some(&vec2_of(s.offset)),
                        blur: s.blur,
                        spread: s.spread,
                        color: Some(&color_of(s.color)),
                    },
                )
            })
            .collect();
        b.create_vector(&shadows)
    });

    // Blurs (story #393). Same nesting order and same absent-is-empty rule as
    // `shadows` above, so a blur-less entry round-trips byte-identically and
    // no committed `.dsb` fixture changes (R7).
    let blurs = (!paint.blurs.is_empty()).then(|| {
        let blurs: Vec<_> = paint
            .blurs
            .iter()
            .map(|bl| {
                FbBlur::create(
                    b,
                    &BlurArgs {
                        kind: match bl.kind {
                            BlurKind::Layer => FbBlurKind::Layer,
                            BlurKind::Backdrop => FbBlurKind::Backdrop,
                        },
                        radius: bl.radius,
                    },
                )
            })
            .collect();
        b.create_vector(&blurs)
    });

    BufPaint::create(
        b,
        &PaintArgs {
            fill_type,
            fill,
            stroke,
            corners: corners.as_ref(),
            clip: paint.clip,
            shadows,
            // Story B1: a VECTOR node lowers to a baked shape index here; a
            // non-vector entry carries `None`, so the sentinel keeps the
            // output byte-identical (R7).
            shape_field: paint.shape_field.unwrap_or(NO_FIELD),
            extra_fills,
            blurs,
        },
    )
}

fn color_of(c: dashpaint::Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

fn vec2_of(v: dashpaint::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

fn mat23_of(m: &dashpaint::Mat23) -> Mat23 {
    Mat23::new(m.a, m.b, m.c, m.d, m.tx, m.ty)
}

/// The pool's interning key: a canonical bit encoding of the paint entry
/// plus the clip flag.
///
/// `f32`s go in by bit pattern, not by value: `f32` is not `Eq`/`Hash`, and
/// NaN is not equal to itself, so a value-keyed pool would emit a fresh
/// entry for every NaN and break R7's byte-reproducibility.
///
/// The clip flag is part of the key because the schema pools clip with the
/// paint entry while the arena carries it per node — two nodes with the same
/// style but different clip are two document entries. The shape index (story
/// B1) is in the key for the same reason: two entries with the same fill but
/// different baked shapes — or a shape vs. the parametric box — must not
/// collapse to one pool entry.
type PaintKey = (Vec<u32>, bool, Option<u32>);

fn paint_key(paint: &Paint) -> PaintKey {
    (
        entry_bits(&paint.entry, &paint.shadows, &paint.blurs),
        paint.clip,
        paint.shape_field,
    )
}

/// The text-style pool's interning key. The `f32` size, color, line height,
/// and letter spacing go in by bit pattern for the same reason the paint key's
/// do (`f32` is not `Eq`/`Hash`, and a value key would mint a fresh entry per
/// NaN, breaking R7's byte-reproducibility). The key covers every axis the
/// style carries (story #310, #341): two styles differing only in, say,
/// alignment or `ligatures_off` must be two distinct pool entries, never
/// collapse to one.
type TextStyleKey = (String, u32, u16, [u32; 4], Option<u32>, u32, u32, u32, bool);

fn text_style_key(style: &TextStyle) -> TextStyleKey {
    (
        style.family.clone(),
        style.size.to_bits(),
        style.weight,
        color_bits(style.color),
        style.line_height_px.map(f32::to_bits),
        style.letter_spacing.to_bits(),
        style.text_align as u32,
        style.text_align_v as u32,
        style.ligatures_off,
    )
}

/// The interning key's bits for one fill kind — the tag plus its payload.
/// Shared by `entry_bits` for the primary fill and every stacked layer
/// (story C1, debt #146), so a solid tags `1` no matter which slot it fills.
fn fill_kind_bits(spec: &FillSpec) -> Vec<u32> {
    let mut key = Vec::new();
    match spec {
        FillSpec::Solid { color } => {
            key.push(1);
            key.extend(color_bits(*color));
        }
        FillSpec::Gradient { gradient, stops } => {
            key.push(2);
            key.push(gradient.kind as u32);
            key.extend([
                gradient.handle_origin.x.to_bits(),
                gradient.handle_origin.y.to_bits(),
            ]);
            key.extend([
                gradient.handle_primary.x.to_bits(),
                gradient.handle_primary.y.to_bits(),
            ]);
            key.extend([
                gradient.handle_secondary.x.to_bits(),
                gradient.handle_secondary.y.to_bits(),
            ]);
            key.push(stops.len() as u32);
            for s in stops {
                key.push(s.offset.to_bits());
                key.extend(color_bits(s.color));
            }
        }
        FillSpec::Image(image_fill) => {
            let dashpaint::ImageFill {
                image,
                scale_mode,
                transform,
                tile_scale,
            } = image_fill;
            key.push(3);
            key.push(*image);
            key.push(*scale_mode as u32);
            key.push(tile_scale.to_bits());
            // `Mat23::IDENTITY` is what an absent transform means since
            // story #578 (see `build_fill`); key it the same way so a
            // pool hit tracks a real byte match, not just an in-memory one.
            if *transform == dashpaint::Mat23::IDENTITY {
                key.push(0);
            } else {
                key.push(1);
                key.extend([
                    transform.a.to_bits(),
                    transform.b.to_bits(),
                    transform.c.to_bits(),
                    transform.d.to_bits(),
                    transform.tx.to_bits(),
                    transform.ty.to_bits(),
                ]);
            }
        }
    }
    key
}

fn entry_bits(
    entry: &PaintEntry,
    shadows: &[dashpaint::Shadow],
    blurs: &[dashpaint::Blur],
) -> Vec<u32> {
    let mut key = Vec::new();
    match &entry.fill {
        None => key.push(0),
        Some(kind) => key.extend(fill_kind_bits(kind)),
    }
    // Stacked fills (story C1, debt #146): two entries sharing the same
    // bottom fill but a different stack (or no stack at all) must not
    // collapse to one pool entry, so the count and each layer's bits join
    // the key — the same "count then each entry's bits" shape `shadows`
    // below uses.
    key.push(entry.extra_fills.len() as u32);
    for kind in &entry.extra_fills {
        key.extend(fill_kind_bits(kind));
    }
    match &entry.stroke {
        None => key.push(0),
        Some(s) => {
            key.push(1);
            key.push(s.width.to_bits());
            key.push(s.align as u32);
            key.extend(color_bits(s.color));
        }
    }
    key.extend([
        entry.corners.top_left.to_bits(),
        entry.corners.top_right.to_bits(),
        entry.corners.bottom_right.to_bits(),
        entry.corners.bottom_left.to_bits(),
    ]);
    key.push(shadows.len() as u32);
    for s in shadows {
        key.push(s.kind as u32);
        key.extend([s.offset.x.to_bits(), s.offset.y.to_bits()]);
        key.push(s.blur.to_bits());
        key.push(s.spread.to_bits());
        key.extend(color_bits(s.color));
    }
    // Blurs join the key on the same "count then each entry's bits" shape
    // (story #393). Two nodes that differ only in their blur must not share a
    // pool entry, and appending here leaves a blur-less entry's key unchanged.
    key.push(blurs.len() as u32);
    for bl in blurs {
        key.push(bl.kind as u32);
        key.push(bl.radius.to_bits());
    }
    key
}

fn color_bits(c: dashpaint::Color) -> [u32; 4] {
    [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]
}
