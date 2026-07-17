//! The load gate (story #15): `.dsb` referential integrity (issue #63),
//! append-only enum range, and the geometry-free paint-vocabulary rules
//! (issue #100).
//!
//! Every test here builds a document the flatbuffer verifier accepts —
//! that is the point. The verifier checks buffer structure, not
//! referential integrity, so each of these loads clean and would fail far
//! from the producing bug, at paint time.

use dashbuf::{
    Color, CornerRadii, Document, DocumentArgs, Fill, Gradient, GradientArgs, GradientKind,
    GradientStop, Image, ImageArgs, ImageFill, ImageFillArgs, ImageFormat, NO_PAINT, Node,
    NodeArgs, Paint, PaintArgs, ScaleMode, SolidFill, SolidFillArgs, Stroke, StrokeAlign,
    StrokeArgs, TextStyle, TextStyleArgs, VariantMember, VariantMemberArgs, VariantOverride,
    VariantOverrideArgs, VariantPropValue, VariantSet, VariantSetArgs, VariantX, VariantXArgs,
    Vec2, root_as_document,
};
use dashscene_validator::{Location, NodePath, rule, validate_document};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

fn red() -> Color {
    Color::new(1.0, 0.0, 0.0, 1.0)
}

/// A document builder that keeps each test to the one field it is about.
#[derive(Default)]
struct Doc {
    nodes: Vec<NodeSpec>,
    paints: Vec<PaintSpec>,
    images: Vec<ImageSpec>,
    strings: usize,
    text_styles: usize,
}

#[derive(Clone)]
struct ImageSpec {
    format: ImageFormat,
    byte_count: usize,
}

#[derive(Default, Clone)]
struct NodeSpec {
    name: &'static str,
    parent: Option<u32>,
    paint_entry: Option<u32>,
    legacy_paint: bool,
    text: Option<u32>,
    text_style: Option<u32>,
    /// `None` leaves the schema default (1.0); `Some` sets it, so a test
    /// can drive an out-of-range value (story #44).
    opacity: Option<f32>,
    /// Marks the node a mask (story #44).
    mask: bool,
}

#[derive(Clone)]
enum PaintSpec {
    Solid,
    Gradient { kind: GradientKind, stops: Vec<f32> },
    Image { index: u32, scale_mode: ScaleMode },
    Stroke { width: f32, align: StrokeAlign },
}

impl Doc {
    fn node(mut self, spec: NodeSpec) -> Self {
        self.nodes.push(spec);
        self
    }

    fn paint(mut self, spec: PaintSpec) -> Self {
        self.paints.push(spec);
        self
    }

    /// `n` well-formed image assets.
    fn images(mut self, n: usize) -> Self {
        self.images.extend((0..n).map(|_| ImageSpec {
            format: ImageFormat::Png,
            byte_count: 1,
        }));
        self
    }

    /// One asset whose `bytes` vector is present but empty.
    fn empty_image(mut self) -> Self {
        self.images.push(ImageSpec {
            format: ImageFormat::Png,
            byte_count: 0,
        });
        self
    }

    /// One well-formed asset carrying a container format this build does not
    /// know.
    fn image_with_format(mut self, format: ImageFormat) -> Self {
        self.images.push(ImageSpec {
            format,
            byte_count: 1,
        });
        self
    }

    fn strings(mut self, n: usize) -> Self {
        self.strings = n;
        self
    }

    fn text_styles(mut self, n: usize) -> Self {
        self.text_styles = n;
        self
    }

    fn build(self) -> Vec<u8> {
        let mut b = FlatBufferBuilder::new();

        let paints: Vec<WIPOffset<Paint>> = self
            .paints
            .iter()
            .map(|spec| build_paint(&mut b, spec))
            .collect();

        let images: Vec<WIPOffset<Image>> = self
            .images
            .iter()
            .map(|spec| {
                let bytes = b.create_vector(&vec![0u8; spec.byte_count]);
                Image::create(
                    &mut b,
                    &ImageArgs {
                        format: spec.format,
                        bytes: Some(bytes),
                    },
                )
            })
            .collect();

        let strings: Vec<WIPOffset<&str>> =
            (0..self.strings).map(|_| b.create_string("hi")).collect();

        let text_styles: Vec<WIPOffset<TextStyle>> = (0..self.text_styles)
            .map(|_| {
                let family = b.create_string("Inter");
                TextStyle::create(
                    &mut b,
                    &TextStyleArgs {
                        family: Some(family),
                        size: 16.0,
                        weight: 400,
                        color: Some(&red()),
                    },
                )
            })
            .collect();

        let nodes: Vec<WIPOffset<Node>> = self
            .nodes
            .iter()
            .map(|spec| {
                let name = b.create_string(spec.name);
                let legacy = spec.legacy_paint.then(|| {
                    SolidFill::create(
                        &mut b,
                        &SolidFillArgs {
                            color: Some(&red()),
                        },
                    )
                });
                Node::create(
                    &mut b,
                    &NodeArgs {
                        name: Some(name),
                        parent: spec.parent.unwrap_or(dashbuf::NO_PARENT),
                        paint: legacy,
                        paint_entry: spec.paint_entry.unwrap_or(NO_PAINT),
                        text: spec.text.unwrap_or(dashbuf::NO_TEXT),
                        text_style: spec.text_style.unwrap_or(dashbuf::NO_TEXT_STYLE),
                        opacity: spec.opacity.unwrap_or(1.0),
                        mask: spec.mask,
                        ..Default::default()
                    },
                )
            })
            .collect();

        let nodes = b.create_vector(&nodes);
        let paints = (!paints.is_empty()).then(|| b.create_vector(&paints));
        let images = (!images.is_empty()).then(|| b.create_vector(&images));
        let strings = (!strings.is_empty()).then(|| b.create_vector(&strings));
        let text_styles = (!text_styles.is_empty()).then(|| b.create_vector(&text_styles));

        let doc = Document::create(
            &mut b,
            &DocumentArgs {
                nodes: Some(nodes),
                images,
                paints,
                strings,
                text_styles,
                ..Default::default()
            },
        );
        b.finish(doc, None);
        b.finished_data().to_vec()
    }
}

fn build_paint<'a>(b: &mut FlatBufferBuilder<'a>, spec: &PaintSpec) -> WIPOffset<Paint<'a>> {
    match spec {
        PaintSpec::Solid => {
            let fill = SolidFill::create(
                b,
                &SolidFillArgs {
                    color: Some(&red()),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::SolidFill,
                    fill: Some(fill.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Gradient { kind, stops } => {
            let stops: Vec<GradientStop> = stops
                .iter()
                .map(|&offset| GradientStop::new(offset, &red()))
                .collect();
            let stops = b.create_vector(&stops);
            let gradient = Gradient::create(
                b,
                &GradientArgs {
                    kind: *kind,
                    handle_origin: Some(&Vec2::new(0.0, 0.0)),
                    handle_primary: Some(&Vec2::new(1.0, 0.0)),
                    handle_secondary: Some(&Vec2::new(0.0, 1.0)),
                    stops: Some(stops),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::Gradient,
                    fill: Some(gradient.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Image { index, scale_mode } => {
            let image_fill = ImageFill::create(
                b,
                &ImageFillArgs {
                    image: *index,
                    scale_mode: *scale_mode,
                    transform: None,
                    tile_scale: 1.0,
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    fill_type: Fill::ImageFill,
                    fill: Some(image_fill.as_union_value()),
                    ..Default::default()
                },
            )
        }
        PaintSpec::Stroke { width, align } => {
            let stroke = Stroke::create(
                b,
                &StrokeArgs {
                    width: *width,
                    align: *align,
                    color: Some(&red()),
                },
            );
            Paint::create(
                b,
                &PaintArgs {
                    stroke: Some(stroke),
                    ..Default::default()
                },
            )
        }
    }
}

fn named(name: &'static str) -> NodeSpec {
    NodeSpec {
        name,
        ..Default::default()
    }
}

/// Validates the built bytes. Asserts the flatbuffer verifier is happy
/// first — a test that accidentally builds a structurally invalid buffer
/// would prove nothing about referential integrity.
fn check(doc: Doc) -> dashscene_validator::Report {
    let bytes = doc.build();
    let document = root_as_document(&bytes).expect("the flatbuffer verifier accepts this buffer");
    validate_document(&document)
}

#[test]
fn a_well_formed_document_produces_no_diagnostics() {
    let report = check(
        Doc::default()
            .node(named("root"))
            .node(NodeSpec {
                name: "card",
                parent: Some(0),
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Solid),
    );
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_node_opacity_out_of_range_is_named() {
    // Story #44 M7: a non-finite or out-of-[0,1] opacity is named at the
    // load gate rather than silently clamped by the loader.
    for bad in [2.0, -0.5, f32::NAN, f32::INFINITY] {
        let report = check(Doc::default().node(NodeSpec {
            name: "translucent",
            opacity: Some(bad),
            ..Default::default()
        }));
        assert!(
            report.has(rule::NODE_OPACITY_OUT_OF_RANGE),
            "opacity {bad} must be named:\n{report}"
        );
    }
    // The in-range endpoints pass.
    for good in [0.0, 0.5, 1.0] {
        let report = check(Doc::default().node(NodeSpec {
            name: "ok",
            opacity: Some(good),
            ..Default::default()
        }));
        assert!(
            !report.has(rule::NODE_OPACITY_OUT_OF_RANGE),
            "opacity {good} is in range:\n{report}"
        );
    }
}

#[test]
fn a_mask_with_a_following_sibling_is_not_inert_but_one_without_is() {
    // Story #44 M13: a mask that stencils a following sibling is fine; a
    // mask with none (a root mask, or the last child) is named inert.
    let masking = check(
        Doc::default()
            .node(named("parent"))
            .node(NodeSpec {
                name: "mask",
                parent: Some(0),
                mask: true,
                ..Default::default()
            })
            .node(NodeSpec {
                name: "content",
                parent: Some(0),
                ..Default::default()
            }),
    );
    assert!(
        !masking.has(rule::INERT_MASK),
        "a masking mask is fine:\n{masking}"
    );

    // The mask is the last child — nothing follows it.
    let inert = check(Doc::default().node(named("parent")).node(NodeSpec {
        name: "mask",
        parent: Some(0),
        mask: true,
        ..Default::default()
    }));
    assert!(
        inert.has(rule::INERT_MASK),
        "a trailing mask is inert:\n{inert}"
    );

    // A root mask is not applied, so it is inert too.
    let root_mask = check(Doc::default().node(NodeSpec {
        name: "root-mask",
        mask: true,
        ..Default::default()
    }));
    assert!(
        root_mask.has(rule::INERT_MASK),
        "a root mask is inert:\n{root_mask}"
    );
}

#[test]
fn parent_index_past_the_node_array_is_named() {
    let report = check(Doc::default().node(named("root")).node(NodeSpec {
        name: "orphan",
        parent: Some(99),
        ..Default::default()
    }));
    assert!(report.has(rule::PARENT_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_parent_that_does_not_precede_its_child_is_named() {
    // The node array is in DFS order, so a parent's index is always lower.
    // A forward reference is a cycle waiting to happen in every consumer
    // that walks up the tree.
    let report = check(Doc::default().node(NodeSpec {
        name: "self-parented",
        parent: Some(0),
        ..Default::default()
    }));
    assert!(report.has(rule::PARENT_NOT_BEFORE_CHILD), "{report}");
}

#[test]
fn paint_entry_past_the_pool_is_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "root",
                paint_entry: Some(4),
                ..Default::default()
            })
            .paint(PaintSpec::Solid),
    );
    assert!(report.has(rule::PAINT_ENTRY_OUT_OF_RANGE), "{report}");
}

#[test]
fn setting_both_the_legacy_paint_and_paint_entry_is_named() {
    // `paint_entry` supersedes the v0.1 `paint` shorthand, so writing both
    // means one of the producer's two opinions is silently discarded.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "root",
                paint_entry: Some(0),
                legacy_paint: true,
                ..Default::default()
            })
            .paint(PaintSpec::Solid),
    );
    assert!(
        report.has(rule::CONFLICTING_PAINT_REPRESENTATION),
        "{report}"
    );
}

#[test]
fn text_and_text_style_indices_past_their_pools_are_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "label",
                text: Some(3),
                text_style: Some(3),
                ..Default::default()
            })
            .strings(1)
            .text_styles(1),
    );
    assert!(report.has(rule::TEXT_STRING_OUT_OF_RANGE), "{report}");
    assert!(report.has(rule::TEXT_STYLE_OUT_OF_RANGE), "{report}");
}

#[test]
fn an_image_fill_past_the_asset_table_is_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "photo",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Image {
                index: 7,
                scale_mode: ScaleMode::Fill,
            })
            .images(1),
    );
    assert!(report.has(rule::IMAGE_OUT_OF_RANGE), "{report}");
}

#[test]
fn an_enum_value_this_build_does_not_know_is_named() {
    // The schema's enums are append-only: a v0.3 reader handed a v0.8
    // document gets the new value back as a raw integer. The schema's own
    // comment makes range-checking the load gate's job — "never default
    // silently".
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "future",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind(9),
                stops: vec![0.0, 1.0],
            }),
    );
    assert!(report.has(rule::UNKNOWN_ENUM), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_gradient_with_no_stops_is_named() {
    // `stops` is flatbuffer-`(required)`, which mandates presence, not
    // non-emptiness — the false assurance issue #100 names. The painter
    // reaches `.first().expect(..)` and panics.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "empty",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![],
            }),
    );
    assert!(report.has(rule::GRADIENT_NO_STOPS), "{report}");
}

#[test]
fn a_gradient_over_the_stop_budget_is_named() {
    let stops: Vec<f32> = (0..=dashscene_validator::MAX_GRADIENT_STOPS)
        .map(|i| i as f32 / dashscene_validator::MAX_GRADIENT_STOPS as f32)
        .collect();
    assert_eq!(stops.len(), dashscene_validator::MAX_GRADIENT_STOPS + 1);
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "busy",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops,
            }),
    );
    assert!(report.has(rule::GRADIENT_STOP_BUDGET), "{report}");
}

#[test]
fn a_gradient_stop_offset_outside_zero_to_one_is_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "skewed",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![0.0, 1.5],
            }),
    );
    assert!(report.has(rule::GRADIENT_STOP_OFFSET_INVALID), "{report}");
}

#[test]
fn a_non_finite_gradient_stop_offset_is_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "nan",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![0.0, f32::NAN],
            }),
    );
    assert!(report.has(rule::GRADIENT_STOP_OFFSET_INVALID), "{report}");
}

#[test]
fn a_negative_stroke_width_is_named() {
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "inverted",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Stroke {
                width: -2.0,
                align: StrokeAlign::Center,
            }),
    );
    assert!(report.has(rule::STROKE_INVALID_WIDTH), "{report}");
}

#[test]
fn a_node_diagnostic_carries_the_nodes_name_path() {
    // "{rule id, node path, severity}" (docs/design/architecture.md) — the path is what
    // sends a designer to the right layer, so it has to be the name chain,
    // not just an index.
    let report = check(
        Doc::default()
            .node(named("screen"))
            .node(NodeSpec {
                name: "card",
                parent: Some(0),
                ..Default::default()
            })
            .node(NodeSpec {
                name: "badge",
                parent: Some(1),
                paint_entry: Some(9),
                ..Default::default()
            }),
    );
    let diagnostic = report
        .find(rule::PAINT_ENTRY_OUT_OF_RANGE)
        .expect("the dangling paint entry is reported");
    assert_eq!(
        diagnostic.at,
        Location::Node(NodePath::new(2, "/screen/card/badge"))
    );
}

#[test]
fn a_pool_diagnostic_points_at_the_pool_entry_not_a_node() {
    // A pooled entry's index is a POOL index. Reporting it as a node index
    // would send a consumer that resolves diagnostics to layers — an editor
    // jumping to it, issue #41's waiver machinery keying on it — to an
    // unrelated node that happens to share the number.
    let report = check(
        Doc::default()
            // Three nodes, so node index 1 exists and is a different thing
            // from paint-pool index 1.
            .node(named("screen"))
            .node(NodeSpec {
                name: "innocent",
                parent: Some(0),
                ..Default::default()
            })
            .node(NodeSpec {
                name: "gradient-holder",
                parent: Some(0),
                paint_entry: Some(1),
                ..Default::default()
            })
            .paint(PaintSpec::Solid)
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![],
            }),
    );
    let diagnostic = report
        .find(rule::GRADIENT_NO_STOPS)
        .expect("the empty gradient is reported");
    assert_eq!(diagnostic.at, Location::PaintEntry(1));
    assert_eq!(diagnostic.at.to_string(), "<paint pool #1>");
}

#[test]
fn an_image_asset_with_no_bytes_is_named() {
    // The painter decodes an asset behind `.expect("image asset decodes
    // (validated upstream, P4)")`. Reading the asset table only for its
    // length — which is all the index rules need — would leave that expect
    // with no upstream.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "photo",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Image {
                index: 0,
                scale_mode: ScaleMode::Fill,
            })
            .empty_image(),
    );
    assert!(report.has(rule::IMAGE_NO_BYTES), "{report}");
    assert_eq!(
        report.find(rule::IMAGE_NO_BYTES).unwrap().at,
        Location::ImageAsset(0)
    );
}

#[test]
fn an_image_container_format_this_build_does_not_know_is_named() {
    // `Image.format` is one of the schema's append-only enums, so it needs
    // the same range check as the rest — otherwise a v0.8 container format
    // reaches the painter's decode and panics.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "photo",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Image {
                index: 0,
                scale_mode: ScaleMode::Fill,
            })
            .image_with_format(ImageFormat(7)),
    );
    assert!(report.has(rule::UNKNOWN_ENUM), "{report}");
    assert_eq!(
        report.find(rule::UNKNOWN_ENUM).unwrap().at,
        Location::ImageAsset(0)
    );
}

#[test]
fn gradient_stops_that_run_backwards_are_named() {
    // Each offset is individually inside 0..=1, so no range rule catches
    // this. The painter hands the offsets to Skia as a `positions` array,
    // which must be monotonically increasing — unordered stops rasterize
    // unpredictably and differ between painters.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "shuffled",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![0.0, 0.8, 0.3, 1.0],
            }),
    );
    assert!(report.has(rule::GRADIENT_STOP_ORDER), "{report}");
}

#[test]
fn gradient_stops_that_repeat_an_offset_are_allowed() {
    // A hard color stop is authored as two stops at the same offset. The
    // ramp is still monotonically increasing, so it must not be a
    // diagnostic.
    let report = check(
        Doc::default()
            .node(NodeSpec {
                name: "hard-stop",
                paint_entry: Some(0),
                ..Default::default()
            })
            .paint(PaintSpec::Gradient {
                kind: GradientKind::Linear,
                stops: vec![0.0, 0.5, 0.5, 1.0],
            }),
    );
    assert!(!report.has(rule::GRADIENT_STOP_ORDER), "{report}");
    assert!(report.is_empty(), "{report}");
}

// ---------------------------------------------------------------------
// The v0.4 variant table (issue #20). `Doc`'s builder DSL is v0.1-v0.3
// vocabulary only; these build the flatbuffer directly rather than widen
// it for one feature's tests.
// ---------------------------------------------------------------------

/// One node, one `VariantSet` whose one member overrides `node`'s `X` —
/// the well-formed shape every malformed-input test below perturbs.
fn document_with_one_variant_override(node: u32, active_member: u32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let a = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[a]);

    let x = VariantX::create(&mut b, &VariantXArgs { value: 1.0 });
    let override_ = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node,
            value_type: VariantPropValue::VariantX,
            value: Some(x.as_union_value()),
        },
    );
    let overrides = b.create_vector(&[override_]);
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

fn validate(bytes: &[u8]) -> dashscene_validator::Report {
    let document = root_as_document(bytes).expect("the flatbuffer verifier accepts this buffer");
    validate_document(&document)
}

#[test]
fn a_well_formed_variant_set_produces_no_diagnostics() {
    let report = validate(&document_with_one_variant_override(0, 0));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_variant_override_node_past_the_node_array_is_named() {
    let report = validate(&document_with_one_variant_override(99, 0));
    assert!(
        report.has(rule::VARIANT_OVERRIDE_NODE_OUT_OF_RANGE),
        "{report}"
    );
    assert!(report.has_errors());
}

#[test]
fn an_active_member_past_the_member_list_is_named() {
    let report = validate(&document_with_one_variant_override(0, 7));
    assert!(
        report.has(rule::VARIANT_ACTIVE_MEMBER_OUT_OF_RANGE),
        "{report}"
    );
    assert!(report.has_errors());
}

#[test]
fn a_variant_set_with_no_members_is_named() {
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);
    let set = VariantSet::create(&mut b, &VariantSetArgs::default());
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

    let report = validate(&bytes);
    assert!(report.has(rule::VARIANT_SET_NO_MEMBERS), "{report}");
    assert!(report.has_errors());
}

// ---------------------------------------------------------------------
// Text-style weight range (issue #129) and paint corner radii (issue
// #128). `Doc`'s DSL always writes weight 400 and sharp corners, so these
// build the flatbuffer directly to reach one out-of-spec value.
// ---------------------------------------------------------------------

/// One node and one text style whose weight is `weight`, everything else
/// well-formed. The load gate iterates the style pool independent of any
/// reference, so no node need point at the style.
fn document_with_text_style_weight(weight: u16) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);
    let family = b.create_string("Inter");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            size: 16.0,
            weight,
            color: Some(&red()),
        },
    );
    let text_styles = b.create_vector(&[style]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            text_styles: Some(text_styles),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

#[test]
fn a_text_style_weight_below_100_is_named() {
    // The schema pins weight to the CSS scale, 100..=900. Font selection
    // would otherwise clamp it silently or pick an unintended face — the
    // silent vocabulary drop P4 forbids.
    let report = validate(&document_with_text_style_weight(50));
    assert!(report.has(rule::TEXT_STYLE_WEIGHT_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
    // A text style is a pooled surface: its diagnostic points at
    // Location::TextStyle (its pool index), never a Node index that would
    // resolve to an unrelated layer (issue #41 review).
    assert_eq!(
        report
            .find(rule::TEXT_STYLE_WEIGHT_OUT_OF_RANGE)
            .unwrap()
            .at,
        Location::TextStyle(0),
    );
}

#[test]
fn a_text_style_weight_above_900_is_named() {
    let report = validate(&document_with_text_style_weight(1234));
    assert!(report.has(rule::TEXT_STYLE_WEIGHT_OUT_OF_RANGE), "{report}");
}

#[test]
fn text_style_weights_at_the_boundaries_are_allowed() {
    // The range is inclusive, so the two endpoints must not be diagnosed.
    for weight in [100, 900] {
        let report = validate(&document_with_text_style_weight(weight));
        assert!(
            !report.has(rule::TEXT_STYLE_WEIGHT_OUT_OF_RANGE),
            "weight {weight} is in range:\n{report}"
        );
    }
}

/// One node painted by a paint entry whose corner radii are `radii`
/// (`[top_left, top_right, bottom_right, bottom_left]`).
fn document_with_paint_corners(radii: [f32; 4]) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let fill = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&red()),
        },
    );
    let corners = CornerRadii::new(radii[0], radii[1], radii[2], radii[3]);
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(fill.as_union_value()),
            corners: Some(&corners),
            ..Default::default()
        },
    );
    let paints = b.create_vector(&[paint]);
    let node = Node::create(
        &mut b,
        &NodeArgs {
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
    b.finished_data().to_vec()
}

#[test]
fn a_negative_paint_corner_radius_is_named() {
    // The painter's `RRect::new_rect_radii` does not clamp a negative radius
    // to zero, and the same radius is copied into every ClipBox of a clipping
    // node's subtree — so a negative radius clips the whole subtree wrongly
    // (issue #128). Corners are geometry-free authored intent, so the load
    // gate catches it — the only gate compile_figma runs.
    let report = validate(&document_with_paint_corners([-4.0, 0.0, 0.0, 0.0]));
    assert!(report.has(rule::CORNER_RADIUS_INVALID), "{report}");
    assert!(report.has_errors());
    assert_eq!(
        report.find(rule::CORNER_RADIUS_INVALID).unwrap().at,
        Location::PaintEntry(0),
    );
}

#[test]
fn a_non_finite_paint_corner_radius_is_named() {
    let report = validate(&document_with_paint_corners([0.0, f32::NAN, 0.0, 0.0]));
    assert!(report.has(rule::CORNER_RADIUS_INVALID), "{report}");
}

#[test]
fn well_formed_paint_corners_are_allowed() {
    // Ordinary rounded corners must not be diagnosed.
    let report = validate(&document_with_paint_corners([8.0, 8.0, 8.0, 8.0]));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

// ---------------------------------------------------------------------
// The v0.7 binding tables (story #167): dangling indices, the channel
// range check, and duplicate signal names. Built with a local raw
// builder — the tables need no node specs beyond a bare node.
// ---------------------------------------------------------------------

/// One document with `signal_names` declarations and one binding row.
fn document_with_bindings(
    signal_names: &[Option<&str>],
    signal: u32,
    node: u32,
    channel: dashbuf::BindingChannel,
) -> Vec<u8> {
    use dashbuf::{
        Binding, BindingArgs, BindingTransform, Document, DocumentArgs, Node, NodeArgs, SignalDecl,
        SignalDeclArgs,
    };

    let mut b = FlatBufferBuilder::new();
    let bare = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[bare]);

    let decls: Vec<_> = signal_names
        .iter()
        .map(|name| {
            let name = name.map(|n| b.create_string(n));
            SignalDecl::create(&mut b, &SignalDeclArgs { name, initial: 1.0 })
        })
        .collect();
    let signals = b.create_vector(&decls);

    let row = Binding::create(
        &mut b,
        &BindingArgs {
            signal,
            node,
            channel,
            transform_type: BindingTransform::NONE,
            transform: None,
        },
    );
    let bindings = b.create_vector(&[row]);

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
    b.finished_data().to_vec()
}

fn validate_bytes(bytes: &[u8]) -> dashscene_validator::Report {
    let document = root_as_document(bytes).expect("the flatbuffer verifier accepts this buffer");
    validate_document(&document)
}

#[test]
fn a_well_formed_binding_table_produces_no_diagnostics() {
    let bytes = document_with_bindings(&[Some("size/gap")], 0, 0, dashbuf::BindingChannel::Gap);
    let report = validate_bytes(&bytes);
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_binding_signal_past_the_declarations_is_named() {
    let bytes = document_with_bindings(&[Some("a")], 7, 0, dashbuf::BindingChannel::X);
    let report = validate_bytes(&bytes);
    let diagnostic = report
        .diagnostics()
        .iter()
        .find(|d| d.rule == rule::BINDING_SIGNAL_OUT_OF_RANGE)
        .expect("the dangling signal is named");
    assert_eq!(diagnostic.at, Location::Binding(0));
}

#[test]
fn a_binding_node_past_the_node_array_is_named() {
    let bytes = document_with_bindings(&[Some("a")], 0, 9, dashbuf::BindingChannel::X);
    let report = validate_bytes(&bytes);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.rule == rule::BINDING_NODE_OUT_OF_RANGE)
    );
}

#[test]
fn a_binding_channel_this_build_does_not_know_is_named() {
    // The enum is append-only: a newer document can carry a channel this
    // build has no variant for. Range-checked, never defaulted (P4).
    let bytes = document_with_bindings(&[Some("a")], 0, 0, dashbuf::BindingChannel(200));
    let report = validate_bytes(&bytes);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.rule == rule::UNKNOWN_ENUM && d.message.contains("Binding.channel"))
    );
}

#[test]
fn a_duplicate_signal_name_is_named_and_anonymous_signals_are_not() {
    let bytes = document_with_bindings(
        &[Some("size/gap"), None, None, Some("size/gap")],
        0,
        0,
        dashbuf::BindingChannel::Gap,
    );
    let report = validate_bytes(&bytes);
    let duplicates: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule::SIGNAL_NAME_DUPLICATE)
        .collect();
    assert_eq!(duplicates.len(), 1, "two anonymous signals do not collide");
    assert_eq!(duplicates[0].at, Location::Signal(3));
}

#[test]
fn a_binding_transform_this_build_does_not_know_is_named() {
    // The BindingTransform union is append-only (Format joins at v0.8,
    // per the schema comment), and the flatbuffer verifier accepts an
    // unknown union tag as long as it carries a payload — so without
    // this gate the loader's transform_of would panic on a document from
    // a newer producer.
    use dashbuf::{
        Binding, BindingArgs, BindingChannel, BindingTransform, Document, DocumentArgs, Node,
        NodeArgs, SignalDecl, SignalDeclArgs, TransformScale, TransformScaleArgs,
    };

    let mut b = FlatBufferBuilder::new();
    let bare = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[bare]);
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 1.0,
        },
    );
    let signals = b.create_vector(&[signal]);
    // A stand-in payload for the tag this build does not know — what a
    // v0.8 Format table would look like to a v0.7 reader.
    let payload = TransformScale::create(&mut b, &TransformScaleArgs { factor: 1.0 });
    let row = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 0,
            node: 0,
            channel: BindingChannel::Gap,
            transform_type: BindingTransform(9),
            transform: Some(payload.as_union_value()),
        },
    );
    let bindings = b.create_vector(&[row]);
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

    let report = validate_bytes(&bytes);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.rule == rule::UNKNOWN_ENUM && d.message.contains("Binding.transform")),
        "the unknown union tag is named, not defaulted:\n{report}"
    );
}

// ---------------------------------------------------------------------------
// The v0.8 grid vocabulary (story #43, review findings R5–R7).
// ---------------------------------------------------------------------------

/// A grid container (declared row/column tracks, optional explicit
/// sizing) with one anchored child — each test varies the one field it
/// is about.
struct GridDoc {
    rows: Vec<(dashbuf::GridTrackSizing, f32)>,
    columns: Vec<(dashbuf::GridTrackSizing, f32)>,
    /// The container's (sizing_h, sizing_v); `None` writes no
    /// constraints table (Fixed defaults).
    container_sizing: Option<(dashbuf::AxisSizing, dashbuf::AxisSizing)>,
    child_row: Option<u16>,
    child_column: Option<u16>,
    child_spans: (u16, u16),
}

impl Default for GridDoc {
    fn default() -> Self {
        Self {
            rows: vec![(dashbuf::GridTrackSizing::Fixed, 40.0)],
            columns: vec![
                (dashbuf::GridTrackSizing::Fixed, 60.0),
                (dashbuf::GridTrackSizing::Fraction, 1.0),
            ],
            container_sizing: None,
            child_row: Some(0),
            child_column: Some(1),
            child_spans: (1, 1),
        }
    }
}

fn grid_document(spec: GridDoc) -> Vec<u8> {
    use dashbuf::{
        GridTrack, GridTrackArgs, LayoutConstraints, LayoutConstraintsArgs, LayoutContainer,
        LayoutContainerArgs, LayoutMode,
    };

    let mut b = FlatBufferBuilder::new();
    let build_tracks = |b: &mut FlatBufferBuilder<'static>,
                        tracks: &[(dashbuf::GridTrackSizing, f32)]| {
        let tracks: Vec<_> = tracks
            .iter()
            .map(|&(sizing, value)| GridTrack::create(b, &GridTrackArgs { sizing, value }))
            .collect();
        b.create_vector(&tracks)
    };
    let rows = build_tracks(&mut b, &spec.rows);
    let columns = build_tracks(&mut b, &spec.columns);
    let flex = LayoutContainer::create(
        &mut b,
        &LayoutContainerArgs {
            mode: LayoutMode::Grid,
            grid_rows: Some(rows),
            grid_columns: Some(columns),
            ..Default::default()
        },
    );
    let container_constraints = spec.container_sizing.map(|(sizing_h, sizing_v)| {
        LayoutConstraints::create(
            &mut b,
            &LayoutConstraintsArgs {
                sizing_h,
                sizing_v,
                ..Default::default()
            },
        )
    });
    let container = Node::create(
        &mut b,
        &NodeArgs {
            flex: Some(flex),
            constraints: container_constraints,
            ..Default::default()
        },
    );
    let child_constraints = LayoutConstraints::create(
        &mut b,
        &LayoutConstraintsArgs {
            grid_row: spec.child_row,
            grid_column: spec.child_column,
            grid_row_span: spec.child_spans.0,
            grid_column_span: spec.child_spans.1,
            ..Default::default()
        },
    );
    let child = Node::create(
        &mut b,
        &NodeArgs {
            parent: 0,
            constraints: Some(child_constraints),
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
    b.finished_data().to_vec()
}

#[test]
fn a_well_formed_grid_produces_no_diagnostics() {
    let report = validate(&grid_document(GridDoc::default()));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn invalid_grid_track_values_are_named() {
    // The same numeric-domain posture as weight and stroke width: a
    // Fixed track is a length (finite, non-negative), a Fraction weight
    // divides free space (finite, positive).
    for (sizing, value) in [
        (dashbuf::GridTrackSizing::Fixed, -50.0),
        (dashbuf::GridTrackSizing::Fixed, f32::NAN),
        (dashbuf::GridTrackSizing::Fraction, 0.0),
        (dashbuf::GridTrackSizing::Fraction, f32::NAN),
        (dashbuf::GridTrackSizing::Fraction, -1.0),
    ] {
        let report = validate(&grid_document(GridDoc {
            rows: vec![(sizing, value)],
            ..Default::default()
        }));
        assert!(
            report.has(rule::GRID_TRACK_INVALID_VALUE),
            "{sizing:?}({value}): {report}"
        );
        assert!(report.has_errors());
    }
}

#[test]
fn a_grid_span_of_zero_is_named() {
    let report = validate(&grid_document(GridDoc {
        child_spans: (0, 1),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_SPAN_ZERO), "{report}");
    assert!(report.has_errors());
}

#[test]
fn an_anchor_past_the_declared_tracks_is_named() {
    // The default container declares one row track; anchoring the child
    // at row 1 names the overrun.
    let report = validate(&grid_document(GridDoc {
        child_row: Some(1),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_ANCHOR_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

#[test]
fn an_anchor_past_the_i16_line_range_is_named_without_declared_tracks() {
    // With no declared track list there is no count to bound against,
    // so the bound is the solver's i16 line range: 32766 is the largest
    // 0-based anchor whose 1-based line still fits.
    let report = validate(&grid_document(GridDoc {
        rows: vec![],
        columns: vec![],
        child_row: Some(32767),
        child_column: Some(32766),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_ANCHOR_OUT_OF_RANGE), "{report}");
    // Only the row anchor overruns; the column anchor sits exactly at
    // the bound.
    assert_eq!(report.diagnostics().len(), 1, "{report}");
}

#[test]
fn a_fraction_track_on_a_hug_axis_is_named() {
    // A fraction divides free space and a hug axis has none: the track
    // (and everything anchored to it) silently collapses to zero, so
    // the construct is diagnosed rather than solved to nothing (P4).
    let report = validate(&grid_document(GridDoc {
        container_sizing: Some((dashbuf::AxisSizing::Hug, dashbuf::AxisSizing::Fixed)),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_FRACTION_TRACK_UNDER_HUG), "{report}");
    assert!(report.has_errors());

    // The transpose: hug vertical + fraction row.
    let report = validate(&grid_document(GridDoc {
        rows: vec![(dashbuf::GridTrackSizing::Fraction, 1.0)],
        container_sizing: Some((dashbuf::AxisSizing::Fixed, dashbuf::AxisSizing::Hug)),
        child_row: Some(0),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_FRACTION_TRACK_UNDER_HUG), "{report}");

    // Hug on an axis whose tracks are all Fixed is fine.
    let report = validate(&grid_document(GridDoc {
        container_sizing: Some((dashbuf::AxisSizing::Fixed, dashbuf::AxisSizing::Hug)),
        ..Default::default()
    }));
    assert!(!report.has(rule::GRID_FRACTION_TRACK_UNDER_HUG), "{report}");
}
