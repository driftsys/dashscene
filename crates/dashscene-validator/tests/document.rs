//! The load gate (story #15): `.dsb` referential integrity (issue #63),
//! append-only enum range, and the geometry-free paint-vocabulary rules
//! (issue #100).
//!
//! Every test here builds a document the flatbuffer verifier accepts —
//! that is the point. The verifier checks buffer structure, not
//! referential integrity, so each of these loads clean and would fail far
//! from the producing bug, at paint time.

use dashbuf::{
    AssetEntry, AssetEntryArgs, Binding, BindingArgs, BindingChannel, BindingTransform, Color,
    CornerRadii, Document, DocumentArgs, Fill, FillLayer, FillLayerArgs, Gradient, GradientArgs,
    GradientKind, GradientStop, ImageFill, ImageFillArgs, ImageFormat, NO_PAINT, Node, NodeArgs,
    Paint, PaintArgs, ScaleMode, SignalDecl, SignalDeclArgs, SolidFill, SolidFillArgs, Stroke,
    StrokeAlign, StrokeArgs, TextStyle, TextStyleArgs, TransformScale, TransformScaleArgs,
    VariantMember, VariantMemberArgs, VariantOverride, VariantOverrideArgs, VariantPropValue,
    VariantSet, VariantSetArgs, VariantVisible, VariantVisibleArgs, VariantWidth, VariantWidthArgs,
    VariantX, VariantXArgs, Vec2, root_as_document,
};
use dashscene_validator::{Location, NodePath, Severity, rule, validate_document};
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

/// Since story #107 the document carries asset identity and metadata, never
/// bytes (P1 applied to assets) — `hash`/`width`/`height` stand in for the
/// old `Image.bytes` payload the pre-#107 pool carried.
#[derive(Clone)]
struct ImageSpec {
    hash: [u8; 32],
    format: ImageFormat,
    width: u32,
    height: u32,
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
    /// `Some((x, y, width, height))` writes a `FixedSizeLayout`; `None` omits
    /// the struct, which is what almost every test here wants. Added for issue
    /// #1048's authored-box rules.
    layout: Option<(f32, f32, f32, f32)>,
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

    /// `n` well-formed asset-table entries, each with a distinct filler hash
    /// — a document-level test does not care about the payload, only that
    /// each entry is self-consistent (a 32-byte hash, a non-zero extent).
    fn images(mut self, n: usize) -> Self {
        let start = self.images.len();
        self.images.extend((0..n).map(|i| ImageSpec {
            hash: [(start + i) as u8 + 7; 32],
            format: ImageFormat::Png,
            width: 4,
            height: 4,
        }));
        self
    }

    /// One asset-table entry whose intrinsic extent is zero on both axes.
    fn zero_extent_image(mut self) -> Self {
        self.images.push(ImageSpec {
            hash: [9u8; 32],
            format: ImageFormat::Png,
            width: 0,
            height: 0,
        });
        self
    }

    /// One well-formed asset carrying a container format this build does not
    /// know.
    fn image_with_format(mut self, format: ImageFormat) -> Self {
        self.images.push(ImageSpec {
            hash: [10u8; 32],
            format,
            width: 4,
            height: 4,
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

        let assets: Vec<WIPOffset<AssetEntry>> = self
            .images
            .iter()
            .map(|spec| {
                let hash = b.create_vector(&spec.hash);
                AssetEntry::create(
                    &mut b,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: spec.format,
                        width: spec.width,
                        height: spec.height,
                        kind: dashbuf::AssetKind::Image,
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
                        ..Default::default()
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
                let layout = spec
                    .layout
                    .map(|(x, y, w, h)| dashbuf::FixedSizeLayout::new(x, y, w, h));
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
                        layout: layout.as_ref(),
                        ..Default::default()
                    },
                )
            })
            .collect();

        let nodes = b.create_vector(&nodes);
        let paints = (!paints.is_empty()).then(|| b.create_vector(&paints));
        let assets = (!assets.is_empty()).then(|| b.create_vector(&assets));
        let strings = (!strings.is_empty()).then(|| b.create_vector(&strings));
        let text_styles = (!text_styles.is_empty()).then(|| b.create_vector(&text_styles));

        let doc = Document::create(
            &mut b,
            &DocumentArgs {
                nodes: Some(nodes),
                assets,
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

/// Issue #1048. The document carries the authored box, and the solver copies
/// both members of its origin straight into the rect a painter places the node
/// with — so a non-finite one arrives at boundary B with nothing having named
/// it. `validate_scene` carries the same rule over the resolved row, but this
/// is the gate with production callers.
#[test]
fn a_non_finite_authored_origin_is_named() {
    for (x, y) in [
        (f32::NAN, 0.0),
        (0.0, f32::NAN),
        (f32::INFINITY, 0.0),
        (0.0, f32::NEG_INFINITY),
    ] {
        let report = check(Doc::default().node(NodeSpec {
            name: "placed",
            layout: Some((x, y, 100.0, 50.0)),
            ..Default::default()
        }));
        assert!(
            report.has(rule::RECT_INVALID_ORIGIN),
            "authored origin ({x}, {y}) must be named:\n{report}"
        );
        assert!(report.has_errors());
    }
}

/// The authored extent, under the id the paint gate already raises for the
/// resolved one — the posture `geometry.corner-radius-invalid` takes across the
/// same two gates. A negative *origin* is ordinary and a negative *extent* is
/// not, which is the whole reason these are two rules.
#[test]
fn a_non_finite_or_negative_authored_extent_is_named() {
    for (w, h) in [
        (f32::NAN, 50.0),
        (100.0, f32::INFINITY),
        (-10.0, 50.0),
        (100.0, -0.5),
    ] {
        let report = check(Doc::default().node(NodeSpec {
            name: "sized",
            layout: Some((0.0, 0.0, w, h)),
            ..Default::default()
        }));
        assert!(
            report.has(rule::RECT_INVALID_EXTENT),
            "authored extent {w}x{h} must be named:\n{report}"
        );
    }
}

#[test]
fn an_ordinary_authored_box_is_clean() {
    // A negative origin places a node above and left of its parent's origin,
    // which is ordinary; a zero extent is a legal empty box. Without this the
    // two tests above would pass against a rule that fires on every layout.
    for (label, (x, y, w, h)) in [
        ("a negative origin", (-40.0, -12.5, 100.0, 50.0)),
        ("a zero extent", (0.0, 0.0, 0.0, 0.0)),
    ] {
        let report = check(Doc::default().node(NodeSpec {
            name: "ordinary",
            layout: Some((x, y, w, h)),
            ..Default::default()
        }));
        assert!(
            !report.has(rule::RECT_INVALID_ORIGIN) && !report.has(rule::RECT_INVALID_EXTENT),
            "{label} is an ordinary authored box:\n{report}"
        );
    }
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
fn a_gradient_with_no_stops_inside_a_stacked_fill_is_named() {
    // Story C1 (debt #146): a stacked layer's own vocabulary rules apply
    // exactly as the primary fill's — the false assurance issue #100 names
    // is not confined to `Paint.fill`. Built directly (not through
    // `Doc`/`PaintSpec`, which has no stacked-fill support) since this is
    // the one test that needs it.
    let mut b = FlatBufferBuilder::new();
    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&red()),
        },
    );
    let stops = b.create_vector::<GradientStop>(&[]);
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
    let layer = FillLayer::create(
        &mut b,
        &FillLayerArgs {
            fill_type: Fill::Gradient,
            fill: Some(gradient.as_union_value()),
        },
    );
    let extra_fills = b.create_vector(&[layer]);
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
    let name = b.create_string("stacked");
    let node = Node::create(
        &mut b,
        &NodeArgs {
            name: Some(name),
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

    let document = root_as_document(&bytes).expect("the flatbuffer verifier accepts this buffer");
    let report = dashscene_validator::validate_document(&document);
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
fn an_asset_with_zero_extent_is_named() {
    // Since story #107 the document carries asset identity and metadata,
    // never bytes (P1 applied to assets), so the document gate can no
    // longer see "no bytes" at all — the bytes live in a blob section this
    // gate does not read. That check now lives on the scene's `ImageTable`,
    // which does carry bytes once a payload is resolved
    // (`crates/dashscene-validator/tests/scene.rs`'s identically-named
    // `an_image_asset_with_no_bytes_is_named`, unaffected by this
    // migration).
    //
    // What the document gate checks instead is that the entry is
    // self-consistent before any payload is resident: a non-zero intrinsic
    // extent, so layout and first paint have something to measure against
    // rather than resolving to nothing.
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
            .zero_extent_image(),
    );
    assert!(report.has(rule::ASSET_ZERO_EXTENT), "{report}");
    assert_eq!(
        report.find(rule::ASSET_ZERO_EXTENT).unwrap().at,
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

/// A one-node document whose single variant member declares one motion
/// track on `channel`, with `frames` as its keyframe list (story #771).
///
/// A `Keyframes` spec in every case, because it is the only arm with a rule
/// of its own; `channel` is what the rect-channel rule reads.
fn document_with_one_transition_track(
    node: u32,
    channel: dashbuf::BindingChannel,
    frames: &[(f32, f32)],
) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let one = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[one]);

    let frames: Vec<dashbuf::Keyframe> = frames
        .iter()
        .map(|(t, value)| dashbuf::Keyframe::new(*t, *value))
        .collect();
    let frames = b.create_vector(&frames);
    let spec = dashbuf::KeyframesSpec::create(
        &mut b,
        &dashbuf::KeyframesSpecArgs {
            duration: 1.0,
            frames: Some(frames),
        },
    );
    let track = dashbuf::PropTransition::create(
        &mut b,
        &dashbuf::PropTransitionArgs {
            node,
            channel,
            spec_type: dashbuf::TransitionSpec::KeyframesSpec,
            spec: Some(spec.as_union_value()),
        },
    );
    let tracks = b.create_vector(&[track]);
    let transition = dashbuf::VariantTransition::create(
        &mut b,
        &dashbuf::VariantTransitionArgs {
            tracks: Some(tracks),
            stagger: 0.0,
        },
    );
    let member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            transition: Some(transition),
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

/// The same one-node document, with one track carrying a non-keyframe spec
/// built from raw parts, so a test can put an out-of-range byte or a
/// nonsense scalar where a well-formed producer never would.
fn document_with_one_spec(
    spec_type: dashbuf::TransitionSpec,
    spec: flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>,
    mut b: FlatBufferBuilder<'_>,
) -> Vec<u8> {
    let one = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[one]);
    let track = dashbuf::PropTransition::create(
        &mut b,
        &dashbuf::PropTransitionArgs {
            node: 0,
            channel: dashbuf::BindingChannel::X,
            spec_type,
            spec: Some(spec),
        },
    );
    let tracks = b.create_vector(&[track]);
    let transition = dashbuf::VariantTransition::create(
        &mut b,
        &dashbuf::VariantTransitionArgs {
            tracks: Some(tracks),
            stagger: 0.0,
        },
    );
    let member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            transition: Some(transition),
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

fn document_with_tween(easing: dashbuf::Easing, duration: f32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let spec = dashbuf::TweenSpec::create(&mut b, &dashbuf::TweenSpecArgs { duration, easing });
    document_with_one_spec(dashbuf::TransitionSpec::TweenSpec, spec.as_union_value(), b)
}

fn document_with_spring(stiffness: f32, damping_ratio: f32) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let spec = dashbuf::SpringSpec::create(
        &mut b,
        &dashbuf::SpringSpecArgs {
            stiffness,
            damping_ratio,
        },
    );
    document_with_one_spec(
        dashbuf::TransitionSpec::SpringSpec,
        spec.as_union_value(),
        b,
    )
}

#[test]
fn an_easing_this_build_does_not_know_is_named() {
    // The gap the review found: `check_enum!` covered the union tag and the
    // channel but never descended into `TweenSpec`, so this document passed
    // the gate with an empty report and then panicked `load_document`'s
    // `unreachable!` — whose message claims the gate rejected it (P4).
    let report = validate(&document_with_tween(dashbuf::Easing(99), 1.0));
    assert!(report.has(rule::UNKNOWN_ENUM), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_tween_with_no_duration_is_named() {
    // `dashcue::Scheduler::start` asserts a finite, positive duration.
    let report = validate(&document_with_tween(dashbuf::Easing::Linear, 0.0));
    assert!(report.has(rule::TRANSITION_SPEC_OUT_OF_RANGE), "{report}");
}

#[test]
fn an_undamped_spring_is_named() {
    // A spring with a zero damping ratio never comes to rest, which is why
    // `dashcue` rejects it by assertion.
    let report = validate(&document_with_spring(200.0, 0.0));
    assert!(report.has(rule::TRANSITION_SPEC_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_keyframe_on_the_interval_boundary_is_named() {
    // The endpoints (0, 0) and (1, 1) are implicit, so a frame sits strictly
    // inside (0, 1) — unchanged by issue #852.
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::X,
        &[(1.0, 0.5)],
    ));
    assert!(report.has(rule::KEYFRAME_T_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_keyframe_carrying_a_non_finite_value_is_named() {
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::X,
        &[(0.5, f32::NAN)],
    ));
    assert!(report.has(rule::KEYFRAME_VALUE_NOT_FINITE), "{report}");
}

#[test]
fn a_transition_track_on_an_already_bound_channel_is_named() {
    // Two writers on one channel. The runtime addresses a binding and a
    // motion track by the same packed `PropKey`, so one silently shadows the
    // other and the FLIP sample — an absolute resolved value — would be
    // written through the binding path and resolved against the parent a
    // second time. Refused by name rather than by precedence (P4).
    let mut b = FlatBufferBuilder::new();
    let one = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[one]);

    let signal = dashbuf::SignalDecl::create(&mut b, &dashbuf::SignalDeclArgs::default());
    let signals = b.create_vector(&[signal]);
    let binding = dashbuf::Binding::create(
        &mut b,
        &dashbuf::BindingArgs {
            signal: 0,
            node: 0,
            channel: dashbuf::BindingChannel::X,
            ..Default::default()
        },
    );
    let bindings = b.create_vector(&[binding]);

    let spec = dashbuf::TweenSpec::create(
        &mut b,
        &dashbuf::TweenSpecArgs {
            duration: 1.0,
            easing: dashbuf::Easing::Linear,
        },
    );
    let track = dashbuf::PropTransition::create(
        &mut b,
        &dashbuf::PropTransitionArgs {
            node: 0,
            channel: dashbuf::BindingChannel::X,
            spec_type: dashbuf::TransitionSpec::TweenSpec,
            spec: Some(spec.as_union_value()),
        },
    );
    let tracks = b.create_vector(&[track]);
    let transition = dashbuf::VariantTransition::create(
        &mut b,
        &dashbuf::VariantTransitionArgs {
            tracks: Some(tracks),
            stagger: 0.0,
        },
    );
    let member = VariantMember::create(
        &mut b,
        &VariantMemberArgs {
            transition: Some(transition),
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
    let variant_sets = b.create_vector(&[set]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            variant_sets: Some(variant_sets),
            signals: Some(signals),
            bindings: Some(bindings),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let bytes = b.finished_data().to_vec();

    let report = validate(&bytes);
    assert!(report.has(rule::TRANSITION_TRACK_ALSO_BOUND), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_well_formed_transition_track_produces_no_diagnostics() {
    // Including a step — two frames sharing a `t` are legal and are what
    // issue #852 ruled, so this also proves the repeat rule counts to three
    // rather than firing on the pair.
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::X,
        &[(0.4, 0.0), (0.4, 1.0)],
    ));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_transition_track_on_a_non_rect_channel_is_named() {
    // FLIP animates rects only, and both the engine and `dashlang` state
    // that as a panic — so a document carrying one has to be refused here or
    // it takes the frame loop down (P4).
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::Opacity,
        &[(0.5, 0.5)],
    ));
    assert!(report.has(rule::TRANSITION_CHANNEL_NOT_A_RECT), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_transition_track_node_past_the_node_array_is_named() {
    let report = validate(&document_with_one_transition_track(
        99,
        dashbuf::BindingChannel::X,
        &[(0.5, 0.5)],
    ));
    assert!(
        report.has(rule::TRANSITION_TRACK_NODE_OUT_OF_RANGE),
        "{report}"
    );
    assert!(report.has_errors());
}

#[test]
fn a_keyframe_list_that_goes_backwards_is_named() {
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::X,
        &[(0.6, 0.0), (0.2, 1.0)],
    ));
    assert!(report.has(rule::KEYFRAME_T_DECREASES), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_third_keyframe_sharing_one_t_is_named() {
    // Two are a step; sampling walks to the last frame at a given `t`, so a
    // third carries a value no sample could ever return
    // (`docs/decisions/a-step-is-a-pair-of-keyframes.md`).
    let report = validate(&document_with_one_transition_track(
        0,
        dashbuf::BindingChannel::X,
        &[(0.4, 0.0), (0.4, 0.5), (0.4, 1.0)],
    ));
    assert!(report.has(rule::KEYFRAME_T_REPEATS), "{report}");
    assert!(report.has_errors());
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

#[test]
fn a_variant_visible_override_produces_no_diagnostics() {
    // The v0.8 append (story #283): the load gate accepts the new
    // VariantVisible union arm the same way it accepts every other known
    // arm — a document that carries one on a valid node validates clean.
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);

    let visible = VariantVisible::create(&mut b, &VariantVisibleArgs { value: false });
    let override_ = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 0,
            value_type: VariantPropValue::VariantVisible,
            value: Some(visible.as_union_value()),
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
            ..Default::default()
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

    let report = validate(&bytes);
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_variant_override_value_this_build_does_not_know_is_named() {
    // The VariantPropValue union is append-only (story #283 appends
    // VariantVisible). The flatbuffer verifier accepts an unknown union
    // tag as long as it carries a payload, so without this gate a newer
    // document's override would reach the loader's overlay resolution and
    // be silently dropped (P4). The stand-in payload is a VariantX table —
    // what a future arm would look like to this reader.
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);

    let payload = VariantX::create(&mut b, &VariantXArgs { value: 1.0 });
    let override_ = VariantOverride::create(
        &mut b,
        &VariantOverrideArgs {
            node: 0,
            value_type: VariantPropValue(99),
            value: Some(payload.as_union_value()),
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
            ..Default::default()
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

    let report = validate(&bytes);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|d| d.rule == rule::UNKNOWN_ENUM && d.message.contains("VariantOverride.value")),
        "the unknown union tag is named, not defaulted:\n{report}"
    );
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
            ..Default::default()
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

// ---------------------------------------------------------------------
// TextStyle.size's numeric domain (issue #557). Each bad value gets its own
// test, named so a mutation that only breaks one of them is identifiable —
// the NaN case matters most, because a naive `size < MIN || size > MAX`
// range check does not catch it (`NaN` compares `false` on both sides).
// ---------------------------------------------------------------------

#[test]
fn a_text_style_size_of_nan_is_named() {
    let report = validate(&document_with_text_style_size(f32::NAN, false));
    assert!(report.has(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE), "{report}");
    // A text style is a pooled surface, so the diagnostic points at its pool
    // index, never a Node index that would resolve to an unrelated layer.
    assert_eq!(
        report.find(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE).unwrap().at,
        Location::TextStyle(0),
    );
}

#[test]
fn a_negative_text_style_size_is_named() {
    let report = validate(&document_with_text_style_size(-12.0, false));
    assert!(report.has(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE), "{report}");
}

#[test]
fn an_infinite_text_style_size_is_named() {
    let report = validate(&document_with_text_style_size(f32::INFINITY, false));
    assert!(report.has(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE), "{report}");
}

#[test]
fn a_text_style_size_of_zero_is_named() {
    // Zero has no meaning for an em size the way a zero corner radius (no
    // rounding) or a zero stroke width (no stroke) does, and nothing in the
    // corpus authors one — so the domain is strictly positive, not merely
    // non-negative.
    let report = validate(&document_with_text_style_size(0.0, false));
    assert!(report.has(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE), "{report}");
}

#[test]
fn text_style_sizes_above_zero_are_allowed() {
    for size in [0.001, 12.0, 14.0, 16.0, 48.0] {
        let report = validate(&document_with_text_style_size(size, false));
        assert!(
            !report.has(rule::TEXT_STYLE_SIZE_OUT_OF_RANGE),
            "size {size} is in range:\n{report}"
        );
    }
}

// ---------------------------------------------------------------------
// The MSDF em-size floor (`docs/decisions/q1-msdf-below-14px.md`, debt
// #373). The floor is checked against the smallest size the document can
// reach at runtime, so these documents carry the runtime constructs a
// `.dsb` can express — a binding through a scale transform, a variant
// override — alongside the text style, and pin that none of them moves the
// number.
// ---------------------------------------------------------------------

/// One text node and its style, at em size `size`. When `animated`, the
/// document also carries the two constructs that change a node's state at
/// runtime, both aimed at that same node: a signal bound to its `Width`
/// through a 0.8 scale transform, and a variant member overriding its
/// width. Neither reaches `TextStyle.size`, which is the point.
fn document_with_text_style_size(size: f32, animated: bool) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let text = b.create_string("hi");
    let strings = b.create_vector(&[text]);
    let family = b.create_string("Inter");
    let style = TextStyle::create(
        &mut b,
        &TextStyleArgs {
            family: Some(family),
            size,
            weight: 400,
            color: Some(&red()),
            ..Default::default()
        },
    );
    let text_styles = b.create_vector(&[style]);
    let node = Node::create(
        &mut b,
        &NodeArgs {
            text: 0,
            text_style: 0,
            ..Default::default()
        },
    );
    let nodes = b.create_vector(&[node]);

    let (signals, bindings, variant_sets) = if animated {
        let name = b.create_string("progress");
        let signal = SignalDecl::create(
            &mut b,
            &SignalDeclArgs {
                name: Some(name),
                initial: 1.0,
            },
        );
        let signals = b.create_vector(&[signal]);
        let scale = TransformScale::create(&mut b, &TransformScaleArgs { factor: 0.8 });
        let binding = Binding::create(
            &mut b,
            &BindingArgs {
                signal: 0,
                node: 0,
                channel: BindingChannel::Width,
                transform_type: BindingTransform::TransformScale,
                transform: Some(scale.as_union_value()),
            },
        );
        let bindings = b.create_vector(&[binding]);

        let width = VariantWidth::create(&mut b, &VariantWidthArgs { value: 10.0 });
        let override_ = VariantOverride::create(
            &mut b,
            &VariantOverrideArgs {
                node: 0,
                value_type: VariantPropValue::VariantWidth,
                value: Some(width.as_union_value()),
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
                ..Default::default()
            },
        );
        (Some(signals), Some(bindings), Some(b.create_vector(&[set])))
    } else {
        (None, None, None)
    };

    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            strings: Some(strings),
            text_styles: Some(text_styles),
            signals,
            bindings,
            variant_sets,
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

#[test]
fn a_text_style_under_the_msdf_floor_is_named() {
    let report = validate(&document_with_text_style_size(12.0, false));
    let diagnostic = report
        .find(rule::TEXT_STYLE_BELOW_MSDF_FLOOR)
        .unwrap_or_else(|| panic!("the floor is not diagnosed:\n{report}"));
    // A warning, not an error: the text renders, and a target that accepts
    // the degrade declares a waiver — which an error would forbid.
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert!(!report.has_errors(), "{report}");
    // A text style is a pooled surface, so the diagnostic points at its pool
    // index, never a Node index that would resolve to an unrelated layer.
    assert_eq!(diagnostic.at, Location::TextStyle(0));
    // The message names the size reached, not just the node.
    assert!(
        diagnostic.message.contains("12 px per em"),
        "the reached size is named: {diagnostic}"
    );
    // Nothing in this document scales the text, so the reached size *is* the
    // authored one and there is no construct to name.
    assert!(
        !diagnostic.message.contains("reached from"),
        "an unscaled style names no construct: {diagnostic}"
    );
    assert!(
        diagnostic.workaround().is_some(),
        "the floor is a design choice, so it carries a workaround: {diagnostic}"
    );
}

#[test]
fn a_text_style_at_the_msdf_floor_is_allowed() {
    // The spike measured MSDF as matching direct rasterization *at* 14 px per
    // em, so the floor is inclusive: 14 passes, and so does anything above.
    for size in [14.0, 16.0, 48.0] {
        let report = validate(&document_with_text_style_size(size, false));
        assert!(
            !report.has(rule::TEXT_STYLE_BELOW_MSDF_FLOOR),
            "{size} px per em is on or above the floor:\n{report}"
        );
    }
}

#[test]
fn a_document_whose_constructs_do_not_scale_text_is_diagnosed_at_its_authored_size() {
    // The ruling on debt #373 checks the floor against the smallest size the
    // document can reach, so a document carrying the runtime constructs a
    // `.dsb` can express — a binding through a 0.8 scale transform, a variant
    // override, both on the text node — must land exactly where the same
    // document without them lands. A scale transform scales the *signal*
    // feeding a channel, and no channel reaches `TextStyle.size`.
    let animated = validate(&document_with_text_style_size(16.0, true));
    assert!(
        !animated.has(rule::TEXT_STYLE_BELOW_MSDF_FLOOR),
        "a 16 px per em style is not dragged under the floor by constructs that do not scale \
         text:\n{animated}"
    );
    assert!(animated.is_empty(), "unexpected diagnostics:\n{animated}");

    let animated = validate(&document_with_text_style_size(12.0, true));
    let diagnostic = animated
        .find(rule::TEXT_STYLE_BELOW_MSDF_FLOOR)
        .unwrap_or_else(|| panic!("the floor is not diagnosed:\n{animated}"));
    assert!(
        diagnostic.message.contains("12 px per em") && !diagnostic.message.contains("reached from"),
        "the reached size is the authored one, and no construct is named: {diagnostic}"
    );
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

// ---------------------------------------------------------------------
// R4's containment check (issue #257,
// `docs/decisions/bindings-are-explicit-and-flat.md`). R4 requires a
// statically provable frame cost; a hug ancestor resizes with its content,
// so a layout write under one reflows past the bound node's own subtree and
// the cost is no longer bounded by that subtree.
// ---------------------------------------------------------------------

/// The axis sizing of one node in a chain: `None` writes no constraints
/// table at all, which leaves the schema's `Fixed` defaults on both axes.
type Sizing = Option<(dashbuf::AxisSizing, dashbuf::AxisSizing)>;

const FIXED: Sizing = None;

fn hug_h() -> Sizing {
    Some((dashbuf::AxisSizing::Hug, dashbuf::AxisSizing::Fixed))
}

fn hug_v() -> Sizing {
    Some((dashbuf::AxisSizing::Fixed, dashbuf::AxisSizing::Hug))
}

/// One document whose nodes form a chain — `chain[0]` is the root and every
/// later node is the child of the one before — carrying a single binding on
/// the last node. A chain is enough: containment is a property of the parent
/// walk, and siblings never enter it.
fn document_with_chain(chain: &[(&str, Sizing)], channel: dashbuf::BindingChannel) -> Vec<u8> {
    use dashbuf::{
        Binding, BindingArgs, BindingTransform, LayoutConstraints, LayoutConstraintsArgs, Node,
        NodeArgs, SignalDecl, SignalDeclArgs,
    };

    let mut b = FlatBufferBuilder::new();
    let mut nodes = Vec::new();
    for (i, (name, sizing)) in chain.iter().enumerate() {
        let name = b.create_string(name);
        let constraints = sizing.map(|(sizing_h, sizing_v)| {
            LayoutConstraints::create(
                &mut b,
                &LayoutConstraintsArgs {
                    sizing_h,
                    sizing_v,
                    ..Default::default()
                },
            )
        });
        nodes.push(Node::create(
            &mut b,
            &NodeArgs {
                name: Some(name),
                parent: if i == 0 {
                    dashbuf::NO_PARENT
                } else {
                    i as u32 - 1
                },
                constraints,
                ..Default::default()
            },
        ));
    }
    let bound = nodes.len() as u32 - 1;
    let nodes = b.create_vector(&nodes);

    let signal_name = b.create_string("size/width");
    let signal = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: Some(signal_name),
            initial: 1.0,
        },
    );
    let signals = b.create_vector(&[signal]);
    let row = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 0,
            node: bound,
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

/// The violating scene: a bound `Width` under a parent that hugs.
#[test]
fn a_layout_binding_under_a_hug_ancestor_is_named() {
    let bytes = document_with_chain(
        &[("root", FIXED), ("panel", hug_h()), ("bar", FIXED)],
        dashbuf::BindingChannel::Width,
    );
    let report = validate_bytes(&bytes);
    let diagnostic = report
        .find(rule::BINDING_REFLOW_NOT_CONTAINED)
        .unwrap_or_else(|| panic!("the uncontained binding is named:\n{report}"));
    assert_eq!(diagnostic.at, Location::Binding(0));
    // A cost bound, not a defect: the document renders correctly, so the
    // target that accepts the reflow declares a waiver — and only a warning
    // is ever waivable.
    assert_eq!(diagnostic.severity, Severity::Warning);
    // …and a warning is only waivable while its id is in `rule::ALL`; an
    // unregistered one is diagnosed as an unknown rule instead, which would
    // leave the strict gate no way to accept the cost.
    assert!(rule::is_known(rule::BINDING_REFLOW_NOT_CONTAINED));
    assert!(
        diagnostic.message.contains("/root/panel"),
        "the diagnostic names the ancestor to fix: {}",
        diagnostic.message
    );
}

/// The satisfying scene: the same chain with every ancestor fixed.
#[test]
fn a_layout_binding_with_no_hug_ancestor_is_contained() {
    let bytes = document_with_chain(
        &[("root", FIXED), ("panel", FIXED), ("bar", FIXED)],
        dashbuf::BindingChannel::Width,
    );
    let report = validate_bytes(&bytes);
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

/// A hug on the axis the channel does *not* name still breaks containment:
/// a width change becomes a height change wherever text rewraps or a `Wrap`
/// container relines, so the two axes are not independent and a per-axis
/// test would under-report.
#[test]
fn a_hug_on_the_other_axis_also_breaks_containment() {
    let bytes = document_with_chain(
        &[("root", FIXED), ("panel", hug_v()), ("bar", FIXED)],
        dashbuf::BindingChannel::Width,
    );
    let report = validate_bytes(&bytes);
    assert!(
        report.has(rule::BINDING_REFLOW_NOT_CONTAINED),
        "a vertical hug still escapes a width write:\n{report}"
    );
}

/// Containment is the whole parent chain, not the parent: a fixed parent
/// under a hug grandparent still lets the reflow out.
#[test]
fn a_hug_grandparent_is_found_by_the_ancestor_walk() {
    let bytes = document_with_chain(
        &[("root", hug_h()), ("panel", FIXED), ("bar", FIXED)],
        dashbuf::BindingChannel::Height,
    );
    let report = validate_bytes(&bytes);
    let diagnostic = report
        .find(rule::BINDING_REFLOW_NOT_CONTAINED)
        .unwrap_or_else(|| panic!("the walk does not stop at the parent:\n{report}"));
    assert!(
        diagnostic
            .message
            .contains("1 hug ancestor(s), the nearest being /root "),
        "one hug ancestor, and it is the root: {}",
        diagnostic.message
    );
}

/// The walk counts every hug ancestor and names the nearest one — the
/// nearest is the one an author changes to contain the write.
#[test]
fn every_hug_ancestor_is_counted_and_the_nearest_is_named() {
    let bytes = document_with_chain(
        &[("root", hug_h()), ("panel", hug_v()), ("bar", FIXED)],
        dashbuf::BindingChannel::Gap,
    );
    let report = validate_bytes(&bytes);
    let diagnostic = report
        .find(rule::BINDING_REFLOW_NOT_CONTAINED)
        .unwrap_or_else(|| panic!("the uncontained binding is named:\n{report}"));
    assert!(
        diagnostic
            .message
            .contains("2 hug ancestor(s), the nearest being /root/panel "),
        "both ancestors counted, the nearer one named: {}",
        diagnostic.message
    );
}

/// The bound node's own sizing is not an ancestor's. A node that hugs the
/// axis it binds has its write overridden by the solver rather than
/// escaping upward, which is a different problem from an uncontained
/// reflow.
#[test]
fn a_hug_on_the_bound_node_itself_is_not_an_ancestor() {
    let bytes = document_with_chain(
        &[("root", FIXED), ("bar", hug_h())],
        dashbuf::BindingChannel::Width,
    );
    let report = validate_bytes(&bytes);
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

/// A paint-only channel never reaches the solver, so no ancestor can make
/// its write reflow anything — the split `dashlang::reactive::classify`
/// already makes for `WriteClass::PaintOnly`.
#[test]
fn a_paint_only_binding_under_a_hug_ancestor_is_not_named() {
    for channel in [
        dashbuf::BindingChannel::FillR,
        dashbuf::BindingChannel::FillG,
        dashbuf::BindingChannel::FillB,
        dashbuf::BindingChannel::FillA,
        dashbuf::BindingChannel::Opacity,
    ] {
        let bytes = document_with_chain(
            &[("root", hug_h()), ("panel", hug_v()), ("bar", FIXED)],
            channel,
        );
        let report = validate_bytes(&bytes);
        assert!(
            !report.has(rule::BINDING_REFLOW_NOT_CONTAINED),
            "{channel:?} is paint-only:\n{report}"
        );
    }
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
fn a_span_running_past_the_declared_tracks_is_named() {
    // The anchor itself fits (column 1 of 2), but anchor 1 + span 2 = 3
    // runs past the two declared column tracks. The engine would grow an
    // implicit third column and solve differently from the authored grid,
    // so it is named rather than solved silently (story #264, D7).
    let report = validate(&grid_document(GridDoc {
        child_column: Some(1),
        child_spans: (1, 2),
        ..Default::default()
    }));
    assert!(report.has(rule::GRID_SPAN_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
    // The anchor fits, so the anchor rule does not also fire — the overrun
    // is attributed to the span, not the anchor.
    assert!(!report.has(rule::GRID_ANCHOR_OUT_OF_RANGE), "{report}");

    // A span that exactly reaches the last track is fine: anchor 0 + span 2
    // = 2 covers both columns.
    let report = validate(&grid_document(GridDoc {
        child_column: Some(0),
        child_spans: (1, 2),
        ..Default::default()
    }));
    assert!(!report.has(rule::GRID_SPAN_OUT_OF_RANGE), "{report}");
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

/// A node whose paint is `fill`, with one binding on `channel` pointed at it.
///
/// Separate from [`document_with_bindings`], which builds a bare node with no
/// paint at all: this rule is about the pairing of a binding with a fill, so
/// the fixture has to carry both (issue #667).
fn document_with_fill_and_binding(fill: Fill, channel: dashbuf::BindingChannel) -> Vec<u8> {
    use dashbuf::{
        Binding, BindingArgs, BindingTransform, GradientKind, GradientStop, Node, NodeArgs, Paint,
        PaintArgs, SignalDecl, SignalDeclArgs, SolidFill, SolidFillArgs, Vec2,
    };

    let mut b = FlatBufferBuilder::new();

    // Both union payloads are built either way. Only the one `fill` names is
    // referenced, and building the other costs nothing but keeps the two arms
    // from diverging in anything but the tag.
    let solid = SolidFill::create(
        &mut b,
        &SolidFillArgs {
            color: Some(&red()),
        },
    );
    let stops = b.create_vector::<GradientStop>(&[]);
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
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: fill,
            fill: Some(match fill {
                Fill::Gradient => gradient.as_union_value(),
                _ => solid.as_union_value(),
            }),
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

    let decl = SignalDecl::create(
        &mut b,
        &SignalDeclArgs {
            name: None,
            initial: 1.0,
        },
    );
    let signals = b.create_vector(&[decl]);

    let row = Binding::create(
        &mut b,
        &BindingArgs {
            signal: 0,
            node: 0,
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
            paints: Some(paints),
            signals: Some(signals),
            bindings: Some(bindings),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

/// The defect issue #667 reported, named rather than dropped.
///
/// A fill channel writes one component of a solid colour, so the runtime
/// stages a whole solid fill on every flush. A gradient has no such component
/// to write into, and the flush replaced it outright — measured on the
/// authored path as a linear gradient plus `FillA` at 0.5 committing as an
/// opaque black at half alpha, with no diagnostic at all.
#[test]
fn a_fill_binding_on_a_gradient_filled_node_is_an_error() {
    let bytes = document_with_fill_and_binding(Fill::Gradient, dashbuf::BindingChannel::FillA);
    let report = validate_bytes(&bytes);

    let Some(found) = report.find(rule::BINDING_FILL_CHANNEL_ON_NON_SOLID_FILL) else {
        panic!("the gradient and the fill binding collide:\n{report}");
    };
    assert_eq!(
        found.severity,
        dashscene_validator::Severity::Error,
        "the producer stated two opinions about the fill and one is discarded, so there is no \
         reading that honors both:\n{report}"
    );
    assert!(rule::is_known(rule::BINDING_FILL_CHANNEL_ON_NON_SOLID_FILL));
}

/// The legitimate pairing stays legitimate. This is the case the rule must not
/// catch: a fill channel writing into a solid fill is exactly what fill
/// channels are for, and it is what every showcase scene does.
#[test]
fn a_fill_binding_on_a_solid_filled_node_is_accepted() {
    let bytes = document_with_fill_and_binding(Fill::SolidFill, dashbuf::BindingChannel::FillA);
    let report = validate_bytes(&bytes);

    assert!(
        !report.has(rule::BINDING_FILL_CHANNEL_ON_NON_SOLID_FILL),
        "a solid fill is what a fill channel writes into:\n{report}"
    );
}

/// `Opacity` shares `channel_effect`'s `PaintOnly` classification with the
/// four fill channels but is its own prop and does not touch the fill, so it
/// does not collide with a gradient. Pinned because the obvious
/// implementation — reusing `PaintOnly` — would wrongly catch it.
#[test]
fn an_opacity_binding_on_a_gradient_filled_node_is_accepted() {
    let bytes = document_with_fill_and_binding(Fill::Gradient, dashbuf::BindingChannel::Opacity);
    let report = validate_bytes(&bytes);

    assert!(
        !report.has(rule::BINDING_FILL_CHANNEL_ON_NON_SOLID_FILL),
        "opacity is a separate prop and leaves the fill alone:\n{report}"
    );
}

// ---------------------------------------------------------------------
// Loop tracks (story #772). Every rule here exists because the runtime's
// contract is to panic on what it rejects, so the gate has to catch it.
// ---------------------------------------------------------------------

/// A one-node document carrying loop tracks, and optionally a binding row
/// and a variant transition track to collide with.
fn document_with_loops(
    tracks: &[(u32, dashbuf::BindingChannel, bool)],
    bind: Option<(u32, dashbuf::BindingChannel)>,
    animate: Option<(u32, dashbuf::BindingChannel)>,
) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let one = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[one]);

    let mut rows = Vec::new();
    for (node, channel, spring) in tracks {
        let (spec_type, spec) = if *spring {
            (
                dashbuf::TransitionSpec::SpringSpec,
                dashbuf::SpringSpec::create(
                    &mut b,
                    &dashbuf::SpringSpecArgs {
                        stiffness: 100.0,
                        damping_ratio: 1.0,
                    },
                )
                .as_union_value(),
            )
        } else {
            (
                dashbuf::TransitionSpec::TweenSpec,
                dashbuf::TweenSpec::create(
                    &mut b,
                    &dashbuf::TweenSpecArgs {
                        duration: 1.0,
                        easing: dashbuf::Easing::Linear,
                    },
                )
                .as_union_value(),
            )
        };
        rows.push(dashbuf::LoopTrack::create(
            &mut b,
            &dashbuf::LoopTrackArgs {
                node: *node,
                channel: *channel,
                from: 0.0,
                to: 1.0,
                spec_type,
                spec: Some(spec),
                phase_offset: 0.0,
            },
        ));
    }
    let loops = b.create_vector(&rows);

    let bindings = bind.map(|(node, channel)| {
        let row = dashbuf::Binding::create(
            &mut b,
            &dashbuf::BindingArgs {
                node,
                channel,
                ..Default::default()
            },
        );
        b.create_vector(&[row])
    });

    let variant_sets = animate.map(|(node, channel)| {
        let spec = dashbuf::TweenSpec::create(
            &mut b,
            &dashbuf::TweenSpecArgs {
                duration: 1.0,
                easing: dashbuf::Easing::Linear,
            },
        );
        let track = dashbuf::PropTransition::create(
            &mut b,
            &dashbuf::PropTransitionArgs {
                node,
                channel,
                spec_type: dashbuf::TransitionSpec::TweenSpec,
                spec: Some(spec.as_union_value()),
            },
        );
        let tracks = b.create_vector(&[track]);
        let transition = dashbuf::VariantTransition::create(
            &mut b,
            &dashbuf::VariantTransitionArgs {
                tracks: Some(tracks),
                stagger: 0.0,
            },
        );
        let member = VariantMember::create(
            &mut b,
            &VariantMemberArgs {
                transition: Some(transition),
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
        b.create_vector(&[set])
    });

    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            loops: Some(loops),
            bindings,
            variant_sets,
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

#[test]
fn a_well_formed_loop_track_produces_no_diagnostics() {
    let report = validate(&document_with_loops(
        &[(0, dashbuf::BindingChannel::Rotation, false)],
        None,
        None,
    ));
    assert!(report.is_empty(), "unexpected diagnostics:\n{report}");
}

#[test]
fn a_loop_track_carrying_a_spring_is_named() {
    // A spring has no duration, so it has no cycle to repeat.
    // `dashcue::Scheduler::start_loop` panics on one; the gate names it.
    let report = validate(&document_with_loops(
        &[(0, dashbuf::BindingChannel::Opacity, true)],
        None,
        None,
    ));
    assert!(report.has(rule::LOOP_SPEC_IS_A_SPRING), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_loop_track_on_a_layout_channel_is_named() {
    // The mirror of the transition rule: a transition animates rects, a
    // loop animates paint. A loop never settles, so one on a layout
    // channel would re-solve every frame for as long as the document is
    // loaded.
    let report = validate(&document_with_loops(
        &[(0, dashbuf::BindingChannel::Width, false)],
        None,
        None,
    ));
    assert!(report.has(rule::LOOP_CHANNEL_NOT_PAINT), "{report}");
    assert!(report.has_errors());
}

#[test]
fn a_loop_track_node_past_the_node_array_is_named() {
    let report = validate(&document_with_loops(
        &[(99, dashbuf::BindingChannel::Opacity, false)],
        None,
        None,
    ));
    assert!(report.has(rule::LOOP_NODE_OUT_OF_RANGE), "{report}");
    assert!(report.has_errors());
}

/// All three collisions, because a loop is the sole writer of its channel
/// and each of the three other writers reaches it by a different route.
#[test]
fn a_loop_sharing_a_channel_with_any_other_writer_is_named() {
    let binding = validate(&document_with_loops(
        &[(0, dashbuf::BindingChannel::Opacity, false)],
        Some((0, dashbuf::BindingChannel::Opacity)),
        None,
    ));
    assert!(
        binding.has(rule::LOOP_CHANNEL_HAS_ANOTHER_WRITER),
        "a binding row: {binding}"
    );

    let transition = validate(&document_with_loops(
        &[(0, dashbuf::BindingChannel::X, false)],
        None,
        Some((0, dashbuf::BindingChannel::X)),
    ));
    assert!(
        transition.has(rule::LOOP_CHANNEL_HAS_ANOTHER_WRITER),
        "a variant transition track: {transition}"
    );

    let second = validate(&document_with_loops(
        &[
            (0, dashbuf::BindingChannel::Opacity, false),
            (0, dashbuf::BindingChannel::Opacity, false),
        ],
        None,
        None,
    ));
    assert!(
        second.has(rule::LOOP_CHANNEL_HAS_ANOTHER_WRITER),
        "a second loop: {second}"
    );

    // And the first of a pair is accepted: the rule names the second, not
    // both, so a report cannot claim two defects for one collision.
    assert_eq!(
        second
            .diagnostics()
            .iter()
            .filter(|d| d.rule == rule::LOOP_CHANNEL_HAS_ANOTHER_WRITER)
            .count(),
        1,
        "one collision is one diagnostic: {second}"
    );
}

/// The same one-node document with one loop track, built from raw values so
/// a test can put a non-finite endpoint or a negative offset where a
/// well-formed producer never would.
fn document_with_one_loop_value(
    channel: dashbuf::BindingChannel,
    from: f32,
    to: f32,
    phase_offset: f32,
) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let one = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[one]);
    let spec = dashbuf::TweenSpec::create(
        &mut b,
        &dashbuf::TweenSpecArgs {
            duration: 1.0,
            easing: dashbuf::Easing::Linear,
        },
    );
    let track = dashbuf::LoopTrack::create(
        &mut b,
        &dashbuf::LoopTrackArgs {
            node: 0,
            channel,
            from,
            to,
            spec_type: dashbuf::TransitionSpec::TweenSpec,
            spec: Some(spec.as_union_value()),
            phase_offset,
        },
    );
    let loops = b.create_vector(&[track]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            loops: Some(loops),
            ..Default::default()
        },
    );
    b.finish(document, None);
    b.finished_data().to_vec()
}

/// Each of these stands in front of an assertion in
/// `dashcue::Scheduler::start_loop` that would take the frame loop down.
#[test]
fn a_loop_tracks_endpoints_and_offset_are_range_checked() {
    let channel = dashbuf::BindingChannel::Rotation;

    let not_finite = validate(&document_with_one_loop_value(channel, f32::NAN, 1.0, 0.0));
    assert!(
        not_finite.has(rule::LOOP_VALUE_OUT_OF_RANGE),
        "a non-finite endpoint: {not_finite}"
    );

    let overflowing = validate(&document_with_one_loop_value(
        channel,
        -f32::MAX,
        f32::MAX,
        0.0,
    ));
    assert!(
        overflowing.has(rule::LOOP_VALUE_OUT_OF_RANGE),
        "a span wider than f32 holds: {overflowing}"
    );

    let backwards = validate(&document_with_one_loop_value(channel, 0.0, 1.0, -0.5));
    assert!(
        backwards.has(rule::LOOP_VALUE_OUT_OF_RANGE),
        "a negative phase offset: {backwards}"
    );

    // And opacity, whose endpoints the arena clamps rather than refuses.
    let clamped = validate(&document_with_one_loop_value(
        dashbuf::BindingChannel::Opacity,
        0.0,
        5.0,
        0.0,
    ));
    assert!(
        clamped.has(rule::LOOP_VALUE_OUT_OF_RANGE),
        "an opacity endpoint outside [0, 1]: {clamped}"
    );

    // The well-formed case, so the four above are not passing on something
    // else the same document says.
    let fine = validate(&document_with_one_loop_value(
        channel,
        0.0,
        std::f32::consts::TAU,
        0.25,
    ));
    assert!(fine.is_empty(), "unexpected diagnostics:\n{fine}");
}

/// A gradient-filled node with a loop on a fill channel: the same defect
/// issue #667 reported for a binding row, reached through a different table.
///
/// The loop's first sample stages a whole solid fill, so the authored
/// gradient is gone from frame one — and nothing ends a loop, so it never
/// comes back.
#[test]
fn a_loop_on_a_fill_channel_of_a_gradient_filled_node_is_named() {
    let mut b = FlatBufferBuilder::new();
    let stops = b.create_vector::<dashbuf::GradientStop>(&[]);
    let gradient = Gradient::create(
        &mut b,
        &GradientArgs {
            kind: dashbuf::GradientKind::Linear,
            handle_origin: Some(&dashbuf::Vec2::new(0.0, 0.0)),
            handle_primary: Some(&dashbuf::Vec2::new(1.0, 0.0)),
            handle_secondary: Some(&dashbuf::Vec2::new(0.0, 1.0)),
            stops: Some(stops),
        },
    );
    let paint = Paint::create(
        &mut b,
        &PaintArgs {
            fill_type: Fill::Gradient,
            fill: Some(gradient.as_union_value()),
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
    let spec = dashbuf::TweenSpec::create(
        &mut b,
        &dashbuf::TweenSpecArgs {
            duration: 1.0,
            easing: dashbuf::Easing::Linear,
        },
    );
    let track = dashbuf::LoopTrack::create(
        &mut b,
        &dashbuf::LoopTrackArgs {
            node: 0,
            channel: dashbuf::BindingChannel::FillA,
            from: 0.2,
            to: 1.0,
            spec_type: dashbuf::TransitionSpec::TweenSpec,
            spec: Some(spec.as_union_value()),
            phase_offset: 0.0,
        },
    );
    let loops = b.create_vector(&[track]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            paints: Some(paints),
            loops: Some(loops),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let report = validate(b.finished_data());

    assert!(
        report.has(rule::LOOP_FILL_CHANNEL_ON_NON_SOLID_FILL),
        "{report}"
    );
    assert!(report.has_errors());
}

/// A variant member overriding the same paint prop a loop drives masks
/// every sample the loop writes, because the overlay resolves first.
#[test]
fn a_loop_on_a_channel_a_variant_member_overrides_is_named() {
    let mut b = FlatBufferBuilder::new();
    let node = Node::create(&mut b, &NodeArgs::default());
    let nodes = b.create_vector(&[node]);

    let rotation = dashbuf::VariantRotation::create(
        &mut b,
        &dashbuf::VariantRotationArgs {
            angle: 0.5,
            anchor_x: 0.0,
            anchor_y: 0.0,
        },
    );
    let over = dashbuf::VariantOverride::create(
        &mut b,
        &dashbuf::VariantOverrideArgs {
            node: 0,
            value_type: dashbuf::VariantPropValue::VariantRotation,
            value: Some(rotation.as_union_value()),
        },
    );
    let overrides = b.create_vector(&[over]);
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
    let variant_sets = b.create_vector(&[set]);

    let spec = dashbuf::TweenSpec::create(
        &mut b,
        &dashbuf::TweenSpecArgs {
            duration: 1.0,
            easing: dashbuf::Easing::Linear,
        },
    );
    let track = dashbuf::LoopTrack::create(
        &mut b,
        &dashbuf::LoopTrackArgs {
            node: 0,
            channel: dashbuf::BindingChannel::Rotation,
            from: 0.0,
            to: 1.0,
            spec_type: dashbuf::TransitionSpec::TweenSpec,
            spec: Some(spec.as_union_value()),
            phase_offset: 0.0,
        },
    );
    let loops = b.create_vector(&[track]);
    let document = Document::create(
        &mut b,
        &DocumentArgs {
            nodes: Some(nodes),
            variant_sets: Some(variant_sets),
            loops: Some(loops),
            ..Default::default()
        },
    );
    b.finish(document, None);
    let report = validate(b.finished_data());

    assert!(
        report.has(rule::LOOP_CHANNEL_OVERRIDDEN_BY_A_VARIANT),
        "{report}"
    );
    assert!(report.has_errors());
}
