//! The load gate (story #15): `.dsb` referential integrity (issue #63),
//! append-only enum range, and the geometry-free paint-vocabulary rules
//! (issue #100).
//!
//! Every test here builds a document the flatbuffer verifier accepts —
//! that is the point. The verifier checks buffer structure, not
//! referential integrity, so each of these loads clean and would fail far
//! from the producing bug, at paint time.

use dashbuf::{
    Color, Document, DocumentArgs, Fill, Gradient, GradientArgs, GradientKind, GradientStop, Image,
    ImageArgs, ImageFill, ImageFillArgs, ImageFormat, NO_PAINT, Node, NodeArgs, Paint, PaintArgs,
    ScaleMode, SolidFill, SolidFillArgs, Stroke, StrokeAlign, StrokeArgs, TextStyle, TextStyleArgs,
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
