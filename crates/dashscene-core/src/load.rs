//! Loading a `.dsb` document into the arena — the document→runtime path
//! (docs/design/dashbuf.md, docs/design/dashc.md).
//!
//! A `.dsb` is the serialized *intent* (P1), and the arena is the runtime's
//! intent model, so loading is a straight replay of the document's nodes
//! through the ordinary producer API: `add_node` + `set_prop` + `commit`. It
//! adds no semantics — a loaded scene is indistinguishable from the same
//! scene staged by hand, and a test pins exactly that.
//!
//! # This function assumes a validated document (P4)
//!
//! It does not re-check referential integrity, and it panics on an index
//! that misses — the same contract as `PaintTable::resolve` and the
//! `Painter` trait. The caller runs the gates first, and there are two of
//! them:
//!
//! ```text
//! // `dashbuf::open` runs the envelope check and binds the asset payloads.
//! let (doc, payloads) = dashbuf::open(file_bytes)?;  // container + verifier
//! let report = dashscene_validator::validate_document(&doc);  // load gate: references
//! if report.has_errors() { /* refuse; never load */ }
//! load_document(&doc, &payloads, &mut arena);    // safe iff the gate passed
//! ```
//!
//! `dashscene-validator` is published *after* `dashscene-core`, so this
//! crate cannot call it — which is exactly why the contract is stated here
//! rather than enforced here.

use dashbuf::{
    BindingTransform, Document, Fill, NO_FIELD, NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE,
    VariantPropValue,
};

use crate::arena::{
    Arena, AxisSizing, CrossAxisAlign, GridTrack, LayoutMode, MainAxisAlign, NodeId, Prop,
    TextAlign, TextAlignV, TextStyle, VariantMember, VariantValue,
};
use crate::bindings::{Channel, ScalarTransform, SignalId};
use crate::committed::{
    Blur, BlurKind, Color, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat, Mat23,
    PaintKind, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2, VectorField,
};

/// Replays a validated `.dsb` document into `arena` and commits it,
/// returning the commit's generation.
///
/// `payloads` binds the document's asset entries to their bytes, one per entry
/// in entry order. `dashbuf::open` produces exactly that from a `.dsb` file, and
/// the panic below fires if the two lengths disagree — a caller that bound the
/// wrong set would otherwise repaint nodes with another document's assets.
///
/// The document's nodes are appended to whatever the arena already holds —
/// the loader is a producer, not an owner, matching `dashlang::Scene::build`.
///
/// # Panics
///
/// On any index the document carries that does not resolve (a paint entry,
/// an image asset, a string, a text style, a parent). Those are precisely
/// what `dashscene_validator::validate_document` reports as errors, so a
/// panic here means the caller skipped the gate.
pub fn load_document(doc: &Document<'_>, payloads: &[&[u8]], arena: &mut Arena) -> u64 {
    let nodes = doc.nodes().unwrap_or_default();
    let paints = doc.paints().unwrap_or_default();
    let strings = doc.strings().unwrap_or_default();
    let text_styles = doc.text_styles().unwrap_or_default();

    let mut txn = arena.open();

    // Assets first: a paint entry's image fill references them by index, so
    // they must exist before any paint prop is staged.
    //
    // Since story #107 the document carries asset *identity and metadata*, not
    // bytes (P1 applied to assets). `payloads` is the caller's binding of each
    // entry's content hash to the bytes it names — resolved from the file's blob
    // sections, which `dashbuf::open` does for the ordinary case. The arena is
    // where the two rejoin.
    //
    // The document's indices are 0..n, but the arena may already hold assets
    // from an earlier load, so the document's index is NOT the arena's. Keep
    // the mapping and rewrite every image fill through it — assuming they
    // coincide would silently repaint one document's nodes with another
    // document's assets.
    let entries = doc.assets().unwrap_or_default();
    assert_eq!(
        payloads.len(),
        entries.len(),
        "the caller bound {} payloads for {} asset entries: every entry's hash resolves to \
         exactly one payload through the binding, and the null binding is the identity map \
         (docs/decisions/asset-model-content-addressed-blobs.md)",
        payloads.len(),
        entries.len()
    );
    let image_of: Vec<u32> = entries
        .iter()
        .zip(payloads)
        .map(|(entry, payload)| {
            txn.add_image(ImageAsset {
                format: image_format(entry.format()),
                bytes: payload.to_vec(),
            })
        })
        .collect();

    // The baked-vector pools (story B1). Each `VectorShape` resolves to a
    // flat, self-contained `VectorField` the painter samples: its atlas's
    // asset index is rewritten through `image_of` (the atlas PNG is an
    // ordinary asset-table entry), and the atlas's `distance_range` folds in
    // beside the shape's own rect and plane bounds. The atlas's `px_per_em`
    // does not — the painter derives scale from the rect/bounds ratio
    // directly, so carrying the bake resolution past load time would be
    // dead weight on the boundary-B struct (debt #358). A paint entry's
    // `shape_field` then indexes this vector directly. All indices are
    // validated upstream (P4), so a miss is a panic, matching the
    // paint/image resolution above.
    let vector_atlases = doc.vector_atlases().unwrap_or_default();
    let shape_of: Vec<VectorField> = doc
        .vector_shapes()
        .unwrap_or_default()
        .iter()
        .map(|shape| {
            let atlas = vector_atlases.get(shape.atlas() as usize);
            let rect = shape
                .atlas_rect()
                .expect("vector shape carries an atlas rect (validated upstream, P4)");
            let plane = shape
                .plane_bounds()
                .expect("vector shape carries plane bounds (validated upstream, P4)");
            VectorField {
                image: image_of[atlas.image() as usize],
                atlas_rect: [rect.x(), rect.y(), rect.width(), rect.height()],
                plane_bounds: [plane.left(), plane.top(), plane.right(), plane.bottom()],
                distance_range: atlas.distance_range(),
            }
        })
        .collect();

    // The node array is in DFS order, so a parent is always staged before
    // its children and `ids[parent]` is always populated by the time a child
    // reads it (the load gate's `node.parent-not-before-child` rule is what
    // makes this safe to assume).
    let mut ids: Vec<NodeId> = Vec::with_capacity(nodes.len());

    for node in nodes.iter() {
        let parent = match node.parent() {
            NO_PARENT => None,
            index => Some(ids[index as usize]),
        };
        let id = txn.add_node(parent, node.name());
        ids.push(id);

        if let Some(layout) = node.layout() {
            txn.set_prop(id, Prop::X(layout.x()));
            txn.set_prop(id, Prop::Y(layout.y()));
            txn.set_prop(id, Prop::Width(layout.width()));
            txn.set_prop(id, Prop::Height(layout.height()));
        }

        // `paint_entry` supersedes the v0.1 `paint` shorthand — the load
        // gate rejects a node that sets both (`paint.conflicting-representation`).
        if node.paint_entry() != NO_PAINT {
            let paint = paints.get(node.paint_entry() as usize);
            load_paint(&mut txn, id, &paint, &image_of, &shape_of);
        } else if let Some(solid) = node.paint()
            && let Some(color) = solid.color()
        {
            txn.set_prop(id, Prop::Fill(color_of(color)));
        }

        if node.text() != NO_TEXT {
            txn.set_prop(id, Prop::Text(strings.get(node.text() as usize).to_owned()));
        }
        if node.text_style() != NO_TEXT_STYLE {
            let style = text_styles.get(node.text_style() as usize);
            txn.set_prop(
                id,
                Prop::TextStyle(TextStyle {
                    family: style.family().to_owned(),
                    size: style.size(),
                    weight: style.weight(),
                    // Never defaulted. An absent color is `text.style-no-color`
                    // at the load gate, so it cannot reach here — and inventing
                    // one would be exactly the silent discovery P4 forbids.
                    color: color_of(style.color().expect(
                        "text style carries a color (validated upstream, P4: text.style-no-color)",
                    )),
                    // The four v0.9 axes (story #310). Absent reads back as the
                    // schema default (auto line height, zero spacing, Left/Top),
                    // so a pre-#310 document loads unchanged.
                    line_height_px: style.line_height_px(),
                    letter_spacing: style.letter_spacing(),
                    text_align: text_align_of(style.text_align()),
                    text_align_v: text_align_v_of(style.text_align_v()),
                    // Story #341: standard ligatures off. Absent reads back
                    // `false` (ligatures on, the pre-#341 default), so an
                    // older document loads unchanged.
                    ligatures_off: style.ligatures_off(),
                }),
            );
        }

        if let Some(flex) = node.flex() {
            txn.set_prop(id, Prop::Mode(layout_mode(flex.mode())));
            txn.set_prop(id, Prop::Gap(flex.gap()));
            if let Some(p) = flex.padding() {
                txn.set_prop(
                    id,
                    Prop::Padding {
                        left: p.left(),
                        top: p.top(),
                        right: p.right(),
                        bottom: p.bottom(),
                    },
                );
            }
            txn.set_prop(id, Prop::MainAlign(main_align(flex.main_align())));
            txn.set_prop(id, Prop::CrossAlign(cross_align(flex.cross_align())));
            // Absent cross gap means follows-`gap`, absent track lists
            // mean implicit auto tracks — absence of intent stages no
            // prop (P1), like min/max below.
            if let Some(v) = flex.cross_gap() {
                txn.set_prop(id, Prop::CrossGap(v));
            }
            if let Some(rows) = flex.grid_rows() {
                txn.set_prop(id, Prop::GridRows(rows.iter().map(grid_track).collect()));
            }
            if let Some(columns) = flex.grid_columns() {
                txn.set_prop(
                    id,
                    Prop::GridColumns(columns.iter().map(grid_track).collect()),
                );
            }
        }

        if let Some(c) = node.constraints() {
            txn.set_prop(id, Prop::SizingH(axis_sizing(c.sizing_h())));
            txn.set_prop(id, Prop::SizingV(axis_sizing(c.sizing_v())));
            // Absent min/max means unconstrained — absence of intent is not a
            // value of intent (P1), so an absent bound stages no prop at all
            // rather than a sentinel.
            if let Some(v) = c.min_width() {
                txn.set_prop(id, Prop::MinWidth(v));
            }
            if let Some(v) = c.max_width() {
                txn.set_prop(id, Prop::MaxWidth(v));
            }
            if let Some(v) = c.min_height() {
                txn.set_prop(id, Prop::MinHeight(v));
            }
            if let Some(v) = c.max_height() {
                txn.set_prop(id, Prop::MaxHeight(v));
            }
            if let Some(m) = c.margin() {
                txn.set_prop(
                    id,
                    Prop::Margin {
                        left: m.left(),
                        top: m.top(),
                        right: m.right(),
                        bottom: m.bottom(),
                    },
                );
            }
            // Grid placement (v0.8, story #43). An absent anchor is
            // auto-placement, so it stages no prop; the spans default
            // to 1 in the schema and in `Layout`, so replaying the
            // value unconditionally is a no-op for old documents.
            if let Some(v) = c.grid_row() {
                txn.set_prop(id, Prop::GridRow(v));
            }
            if let Some(v) = c.grid_column() {
                txn.set_prop(id, Prop::GridColumn(v));
            }
            txn.set_prop(id, Prop::GridRowSpan(c.grid_row_span()));
            txn.set_prop(id, Prop::GridColumnSpan(c.grid_column_span()));
        }

        // v0.8 masks + group opacity (story #44). Each stages only when it
        // differs from the arena default, the same absence-is-not-intent
        // rule as the min/max constraints above — a fully-opaque, unmasked,
        // visible node stages nothing.
        if node.opacity() != 1.0 {
            txn.set_prop(id, Prop::Opacity(node.opacity()));
        }
        if node.mask() {
            txn.set_prop(id, Prop::Mask(true));
        }
        if !node.visible() {
            txn.set_prop(id, Prop::Visible(false));
        }
    }

    // The variant table (v0.4, story #20) replays the same way: each
    // VariantSet becomes an add_variant_set call, and a document that
    // was authored (or last committed) mid-switch replays that switch
    // through set_variant rather than staying pinned to member 0.
    for set in doc.variant_sets().unwrap_or_default().iter() {
        let members = set
            .members()
            .unwrap_or_default()
            .iter()
            .map(|member| VariantMember {
                name: member.name().map(str::to_owned),
                overrides: member
                    .overrides()
                    .unwrap_or_default()
                    .iter()
                    .map(|o| (ids[o.node() as usize], variant_value(&o)))
                    .collect(),
            })
            .collect();
        let id = txn.add_variant_set(members);
        let active = set.active_member() as usize;
        if active != 0 {
            txn.set_variant(id, active);
        }
    }

    // The binding tables (v0.7, story #167) replay through the same
    // producer API: every declaration, then every row, in document
    // order. Indices resolve through this load's own mappings (`ids`,
    // `signal_ids`), never raw — the arena may already hold nodes and
    // signals from an earlier load.
    let signal_ids: Vec<SignalId> = doc
        .signals()
        .unwrap_or_default()
        .iter()
        .map(|signal| txn.declare_signal(signal.name(), signal.initial()))
        .collect();
    for row in doc.bindings().unwrap_or_default().iter() {
        txn.bind(
            ids[row.node() as usize],
            channel_of(row.channel()),
            signal_ids[row.signal() as usize],
            transform_of(&row),
        );
    }

    txn.commit()
}

/// One binding row's channel, converted from the wire enum. An unknown
/// value is `binding.unknown-channel` at the load gate, so it never
/// reaches here (the same posture as the layout enums below).
fn channel_of(channel: dashbuf::BindingChannel) -> Channel {
    Channel::from_code(channel.0).unwrap_or_else(|| {
        unreachable!("unknown BindingChannel {channel:?}: rejected by the load gate (P4)")
    })
}

/// One binding row's transform, converted from the `BindingTransform`
/// union. Union NONE is the identity transform by schema contract.
fn transform_of(row: &dashbuf::Binding<'_>) -> ScalarTransform {
    match row.transform_type() {
        BindingTransform::NONE => ScalarTransform::Identity,
        BindingTransform::TransformScale => ScalarTransform::Scale(
            row.transform_as_transform_scale()
                .expect("TransformScale present")
                .factor(),
        ),
        BindingTransform::TransformMapRange => {
            let m = row
                .transform_as_transform_map_range()
                .expect("TransformMapRange present");
            ScalarTransform::MapRange {
                in_lo: m.in_lo(),
                in_hi: m.in_hi(),
                out_lo: m.out_lo(),
                out_hi: m.out_hi(),
            }
        }
        BindingTransform::TransformClamp => {
            let c = row
                .transform_as_transform_clamp()
                .expect("TransformClamp present");
            ScalarTransform::Clamp {
                lo: c.lo(),
                hi: c.hi(),
            }
        }
        other => unreachable!("unknown BindingTransform {other:?}: rejected by the load gate (P4)"),
    }
}

/// One `VariantOverride`'s value, converted from the `VariantPropValue`
/// union to the arena's narrow `VariantValue` (the same five-prop slice
/// — docs/decisions/variant-set-flat-index.md).
fn variant_value(o: &dashbuf::VariantOverride<'_>) -> VariantValue {
    match o.value_type() {
        VariantPropValue::VariantX => {
            VariantValue::X(o.value_as_variant_x().expect("VariantX present").value())
        }
        VariantPropValue::VariantY => {
            VariantValue::Y(o.value_as_variant_y().expect("VariantY present").value())
        }
        VariantPropValue::VariantWidth => VariantValue::Width(
            o.value_as_variant_width()
                .expect("VariantWidth present")
                .value(),
        ),
        VariantPropValue::VariantHeight => VariantValue::Height(
            o.value_as_variant_height()
                .expect("VariantHeight present")
                .value(),
        ),
        VariantPropValue::VariantFill => VariantValue::Fill(color_of(
            o.value_as_variant_fill()
                .expect("VariantFill present")
                .color(),
        )),
        VariantPropValue::VariantVisible => VariantValue::Visible(
            o.value_as_variant_visible()
                .expect("VariantVisible present")
                .value(),
        ),
        other => unreachable!("unknown VariantPropValue {other:?}: rejected by the load gate (P4)"),
    }
}

/// The `PaintKind` a stacked-fill layer's `Fill` union resolves to (story C1,
/// debt #146), or `None` for `Fill::NONE` — a malformed layer (a stacked
/// layer with no fill has no meaning, unlike the primary `Paint.fill`, which
/// legitimately can be absent for a stroke-only entry), so it is dropped
/// here the same way `load_paint`'s own primary-fill match tolerates an
/// unrecognized value: this function assumes a validated document (P4),
/// same contract as the rest of this module.
fn fill_layer_kind_of(layer: &dashbuf::FillLayer<'_>, image_of: &[u32]) -> Option<PaintKind> {
    match layer.fill_type() {
        Fill::SolidFill => layer
            .fill_as_solid_fill()
            .and_then(|s| s.color())
            .map(|c| PaintKind::Solid { color: color_of(c) }),
        Fill::Gradient => layer.fill_as_gradient().map(|g| {
            let stops: Vec<GradientStop> = g
                .stops()
                .iter()
                .map(|s| GradientStop {
                    offset: s.offset(),
                    color: color_of(s.color()),
                })
                .collect();
            PaintKind::Gradient(Gradient::new(
                gradient_kind(g.kind()),
                vec2_of(g.handle_origin()),
                vec2_of(g.handle_primary()),
                vec2_of(g.handle_secondary()),
                &stops,
            ))
        }),
        Fill::ImageFill => layer.fill_as_image_fill().map(|f| PaintKind::Image {
            image: image_of[f.image() as usize],
            scale_mode: scale_mode(f.scale_mode()),
            transform: f.transform().map(mat23_of),
            tile_scale: f.tile_scale(),
        }),
        _ => None,
    }
}

/// One pool entry's fill, stroke, corners, clip, and baked-vector shape,
/// staged onto `id`.
fn load_paint(
    txn: &mut crate::arena::Txn<'_>,
    id: NodeId,
    paint: &dashbuf::Paint<'_>,
    image_of: &[u32],
    shape_of: &[VectorField],
) {
    match paint.fill_type() {
        Fill::SolidFill => {
            if let Some(solid) = paint.fill_as_solid_fill()
                && let Some(color) = solid.color()
            {
                txn.set_prop(id, Prop::Fill(color_of(color)));
            }
        }
        Fill::Gradient => {
            if let Some(g) = paint.fill_as_gradient() {
                let stops: Vec<GradientStop> = g
                    .stops()
                    .iter()
                    .map(|s| GradientStop {
                        offset: s.offset(),
                        color: color_of(s.color()),
                    })
                    .collect();
                txn.set_prop(
                    id,
                    Prop::FillWith(PaintKind::Gradient(Gradient::new(
                        gradient_kind(g.kind()),
                        vec2_of(g.handle_origin()),
                        vec2_of(g.handle_primary()),
                        vec2_of(g.handle_secondary()),
                        &stops,
                    ))),
                );
            }
        }
        Fill::ImageFill => {
            if let Some(f) = paint.fill_as_image_fill() {
                txn.set_prop(
                    id,
                    Prop::FillWith(PaintKind::Image {
                        // Through the mapping, never the document's own index.
                        image: image_of[f.image() as usize],
                        scale_mode: scale_mode(f.scale_mode()),
                        transform: f.transform().map(mat23_of),
                        tile_scale: f.tile_scale(),
                    }),
                );
            }
        }
        // A pool entry with no fill is a stroke-only or clip-only entry — a
        // legitimate shape, not a missing one.
        _ => {}
    }

    // Stacked fills (story C1, debt #146): every layer above the bottom
    // fill, in the same array order it paints. Absent (the pre-C1 default)
    // means a single fill, so an old document stages nothing here — matching
    // `PaintEntry::extra_fills`'s empty default.
    if let Some(layers) = paint.extra_fills()
        && !layers.is_empty()
    {
        txn.set_prop(
            id,
            Prop::ExtraFills(
                layers
                    .iter()
                    .filter_map(|layer| fill_layer_kind_of(&layer, image_of))
                    .collect(),
            ),
        );
    }

    if let Some(s) = paint.stroke() {
        // `Stroke.color` is `(required)` in the schema, so the accessor is
        // not an Option.
        txn.set_prop(
            id,
            Prop::Stroke(Stroke {
                width: s.width(),
                align: stroke_align(s.align()),
                color: color_of(s.color()),
            }),
        );
    }

    if let Some(c) = paint.corners() {
        txn.set_prop(
            id,
            Prop::Corners {
                top_left: c.top_left(),
                top_right: c.top_right(),
                bottom_right: c.bottom_right(),
                bottom_left: c.bottom_left(),
            },
        );
    }

    // v0.8 shadows (story #45). Absent means none; the prop replaces the
    // whole list, so an empty vector would clear it — set the prop only
    // when the document carries a non-empty list, matching the corners
    // and stroke omissions above.
    if let Some(shadows) = paint.shadows()
        && !shadows.is_empty()
    {
        txn.set_prop(
            id,
            Prop::Shadows(
                shadows
                    .iter()
                    .map(|s| Shadow {
                        kind: shadow_kind(s.kind()),
                        // An absent `offset` struct is a zero (centered)
                        // shadow — Figma always writes one, but the schema
                        // leaves the struct optional.
                        offset: s.offset().map_or(Vec2 { x: 0.0, y: 0.0 }, vec2_of),
                        blur: s.blur(),
                        spread: s.spread(),
                        // `Shadow.color` is `(required)`, so the accessor
                        // is not an Option (like `Stroke.color`).
                        color: color_of(s.color()),
                    })
                    .collect(),
            ),
        );
    }

    // v0.11 blurs (story #393). Same shape as the shadows above: absent means
    // none, and the prop replaces the whole list, so it is set only when the
    // document carries a non-empty one. Loading it matters even before a
    // painter draws it — a schema field the loader ignored would drop the
    // node's blur silently, which is exactly what P4 forbids.
    if let Some(blurs) = paint.blurs()
        && !blurs.is_empty()
    {
        txn.set_prop(
            id,
            Prop::Blurs(
                blurs
                    .iter()
                    .map(|b| Blur {
                        kind: blur_kind(b.kind()),
                        radius: b.radius(),
                    })
                    .collect(),
            ),
        );
    }

    // The document pools clip with the paint entry; the arena carries it as
    // node intent (issue #97). Two nodes sharing a style but differing in
    // clip therefore need two pool entries in the document, which is what
    // the emitter's pool key accounts for.
    if paint.clip() {
        txn.set_prop(id, Prop::Clip(true));
    }

    // The baked-vector shape channel (story B1). `NO_FIELD` is the implicit
    // parametric shape, so an old document (which carries no shape field)
    // stages nothing here and loads unchanged. A valid index resolves to the
    // pre-built `VectorField` and stages it as paint intent.
    if paint.shape_field() != NO_FIELD {
        txn.set_prop(id, Prop::ShapeField(shape_of[paint.shape_field() as usize]));
    }
}

fn color_of(c: &dashbuf::Color) -> Color {
    Color {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}

fn vec2_of(v: &dashbuf::Vec2) -> Vec2 {
    Vec2 { x: v.x(), y: v.y() }
}

fn mat23_of(m: &dashbuf::Mat23) -> Mat23 {
    Mat23 {
        a: m.a(),
        b: m.b(),
        c: m.c(),
        d: m.d(),
        tx: m.tx(),
        ty: m.ty(),
    }
}

// The enum maps are exhaustive over the values this build knows. A value it
// does not know is `vocabulary.unknown-enum` at the load gate, so it never
// reaches here — the wildcard arm exists because `flatc` models an
// append-only enum as a newtype over `u8`, which has no exhaustive match.

fn image_format(f: dashbuf::ImageFormat) -> ImageFormat {
    match f {
        dashbuf::ImageFormat::Png => ImageFormat::Png,
        dashbuf::ImageFormat::Jpeg => ImageFormat::Jpeg,
        dashbuf::ImageFormat::Gif => ImageFormat::Gif,
        other => unreachable!("unknown ImageFormat {other:?}: rejected by the load gate (P4)"),
    }
}

fn gradient_kind(k: dashbuf::GradientKind) -> GradientKind {
    match k {
        dashbuf::GradientKind::Linear => GradientKind::Linear,
        dashbuf::GradientKind::Radial => GradientKind::Radial,
        dashbuf::GradientKind::Angular => GradientKind::Angular,
        dashbuf::GradientKind::Diamond => GradientKind::Diamond,
        other => unreachable!("unknown GradientKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn scale_mode(m: dashbuf::ScaleMode) -> ScaleMode {
    match m {
        dashbuf::ScaleMode::Fill => ScaleMode::Fill,
        dashbuf::ScaleMode::Fit => ScaleMode::Fit,
        dashbuf::ScaleMode::Crop => ScaleMode::Crop,
        dashbuf::ScaleMode::Tile => ScaleMode::Tile,
        other => unreachable!("unknown ScaleMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn stroke_align(a: dashbuf::StrokeAlign) -> StrokeAlign {
    match a {
        dashbuf::StrokeAlign::Inside => StrokeAlign::Inside,
        dashbuf::StrokeAlign::Center => StrokeAlign::Center,
        dashbuf::StrokeAlign::Outside => StrokeAlign::Outside,
        other => unreachable!("unknown StrokeAlign {other:?}: rejected by the load gate (P4)"),
    }
}

/// The document's blur kind as the arena's (story #393).
///
/// The catch-all is `unreachable!`, not a coercion to `Backdrop`, and the
/// difference matters: the generated binding is a newtype over `u8` whose
/// verifier does no range check, so an out-of-range discriminant does reach
/// here. Coercing it would turn a future kind — a progressive blur, say —
/// into a backdrop blur, and `PaintEntry::samples_backdrop` would then impose
/// a painter ordering barrier for an effect that needs none. That is a silent
/// semantic substitution, which is exactly what P4 forbids. The load gate
/// range-checks `Blur.kind` before this runs, so reaching the arm means the
/// gate was bypassed. Same shape as [`shadow_kind`] below.
fn blur_kind(k: dashbuf::BlurKind) -> BlurKind {
    match k {
        dashbuf::BlurKind::Layer => BlurKind::Layer,
        dashbuf::BlurKind::Backdrop => BlurKind::Backdrop,
        other => unreachable!("unknown BlurKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn shadow_kind(k: dashbuf::ShadowKind) -> ShadowKind {
    match k {
        dashbuf::ShadowKind::Drop => ShadowKind::Drop,
        dashbuf::ShadowKind::Inner => ShadowKind::Inner,
        other => unreachable!("unknown ShadowKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn layout_mode(m: dashbuf::LayoutMode) -> LayoutMode {
    match m {
        dashbuf::LayoutMode::None => LayoutMode::None,
        dashbuf::LayoutMode::Horizontal => LayoutMode::Horizontal,
        dashbuf::LayoutMode::Vertical => LayoutMode::Vertical,
        dashbuf::LayoutMode::Wrap => LayoutMode::Wrap,
        dashbuf::LayoutMode::Grid => LayoutMode::Grid,
        other => unreachable!("unknown LayoutMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn main_align(a: dashbuf::MainAxisAlign) -> MainAxisAlign {
    match a {
        dashbuf::MainAxisAlign::Start => MainAxisAlign::Start,
        dashbuf::MainAxisAlign::Center => MainAxisAlign::Center,
        dashbuf::MainAxisAlign::End => MainAxisAlign::End,
        dashbuf::MainAxisAlign::SpaceBetween => MainAxisAlign::SpaceBetween,
        other => unreachable!("unknown MainAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn cross_align(a: dashbuf::CrossAxisAlign) -> CrossAxisAlign {
    match a {
        dashbuf::CrossAxisAlign::Start => CrossAxisAlign::Start,
        dashbuf::CrossAxisAlign::Center => CrossAxisAlign::Center,
        dashbuf::CrossAxisAlign::End => CrossAxisAlign::End,
        dashbuf::CrossAxisAlign::Baseline => CrossAxisAlign::Baseline,
        other => unreachable!("unknown CrossAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn text_align_of(a: dashbuf::TextAlign) -> TextAlign {
    match a {
        dashbuf::TextAlign::Left => TextAlign::Left,
        dashbuf::TextAlign::Center => TextAlign::Center,
        dashbuf::TextAlign::Right => TextAlign::Right,
        other => unreachable!("unknown TextAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn text_align_v_of(a: dashbuf::TextAlignV) -> TextAlignV {
    match a {
        dashbuf::TextAlignV::Top => TextAlignV::Top,
        dashbuf::TextAlignV::Center => TextAlignV::Center,
        dashbuf::TextAlignV::Bottom => TextAlignV::Bottom,
        other => unreachable!("unknown TextAlignV {other:?}: rejected by the load gate (P4)"),
    }
}

fn grid_track(t: dashbuf::GridTrack<'_>) -> GridTrack {
    match t.sizing() {
        dashbuf::GridTrackSizing::Fixed => GridTrack::Fixed(t.value()),
        dashbuf::GridTrackSizing::Fraction => GridTrack::Fraction(t.value()),
        other => unreachable!("unknown GridTrackSizing {other:?}: rejected by the load gate (P4)"),
    }
}

fn axis_sizing(s: dashbuf::AxisSizing) -> AxisSizing {
    match s {
        dashbuf::AxisSizing::Fixed => AxisSizing::Fixed,
        dashbuf::AxisSizing::Hug => AxisSizing::Hug,
        dashbuf::AxisSizing::Fill => AxisSizing::Fill,
        other => unreachable!("unknown AxisSizing {other:?}: rejected by the load gate (P4)"),
    }
}
