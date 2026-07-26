//! The load gate: is this `.dsb` document internally consistent?
//!
//! Two failure classes live only here.
//!
//! **Referential integrity** (issue #63). The schema carries bare-`u32`
//! index fields — `Node.parent`, `Node.paint_entry`, `Node.text`,
//! `Node.text_style`, `ImageFill.image`. The flatbuffer verifier checks
//! buffer structure, not referential integrity, so an out-of-range index
//! loads clean and fails far from the producing bug, at paint time. A
//! solved scene cannot have this problem: every rect resolves to a pool
//! entry (`docs/decisions/boundary-b-unification.md`).
//!
//! **Unknown enum values.** The schema's enums are append-only, so a
//! reader built before an append receives the new value as a raw integer.
//! The schema's own comment makes this the load gate's job: "range-check
//! and emit a named diagnostic (P4/R6), never default silently."

use dashbuf::{
    AssetEntry, Document, Fill, FillLayer, NO_FIELD, NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE,
    Node, Paint, VariantSet,
};

use crate::paint::{
    check_corners, check_gradient_stops, check_image_index, check_shadow, check_stroke_width,
    error, warning,
};
use crate::{Location, NodePath, Report, rule};

/// The document's pool sizes, so the index rules cannot be handed the wrong
/// count. Four bare `usize` parameters would be positionally
/// interchangeable, and transposing two would silently validate an index
/// against the wrong pool.
struct PoolSizes {
    nodes: usize,
    paints: usize,
    assets: usize,
    strings: usize,
    text_styles: usize,
    /// Story B1: the baked-vector pools, so a paint entry's `shape_field` and
    /// a shape's `atlas` can be range-checked against them.
    vector_shapes: usize,
    vector_atlases: usize,
}

/// Range-checks one append-only enum field.
///
/// Takes the enum value *once*: `flatc` generates each enum as a newtype
/// over `u8` whose `variant_name()` is `None` for a value this build does
/// not know, and both halves are read here rather than by the caller. The
/// earlier shape passed `variant_name()` and the raw `u8` as two arguments,
/// which let a caller pair one field's name with another field's value —
/// it compiled, and checked the wrong field.
macro_rules! check_enum {
    ($report:expr, $at:expr, $field:literal, $value:expr) => {{
        let value = $value;
        if value.variant_name().is_none() {
            $report.push(error(
                rule::UNKNOWN_ENUM,
                $at,
                format!(
                    "{} carries value {}, which this build does not know; the schema's enums \
                     are append-only, so the document is newer than this reader",
                    $field, value.0
                ),
            ));
        }
    }};
}

/// Validates a `.dsb` document: referential integrity, enum range, and the
/// geometry-free paint-vocabulary rules.
///
/// Two things this deliberately does *not* do.
///
/// It takes no [`Profile`](crate::Profile). Every construct the schema can
/// express sits in docs/specification/04-figma-vocabulary-profile.md's NOW
/// band — including the v0.8 masks and group opacity (a mask is a shape
/// stencil, an opacity a node alpha; neither is profile-differentiated) —
/// so there is nothing here for a profile to differentiate. Out-of-profile
/// constructs are caught at the import gate ([`crate::triage`]), which is
/// the only place they exist: by the time a construct is in the schema, it
/// is in the vocabulary. The parameter returns when the effect vocabulary
/// lands (a blur or blend mode is profile:full-only), with a rule behind it
/// — an ignored one now would imply a check that does not happen.
///
/// It checks no geometry budgets. P1 says the document carries intent,
/// never results, so a `Hug`/`Fill` node has no box for a stroke width to
/// be measured against. Those run on the solved scene
/// ([`crate::validate_scene`]).
pub fn validate_document(doc: &Document<'_>) -> Report {
    let mut report = Report::default();

    let nodes = doc.nodes().unwrap_or_default();
    let paints = doc.paints().unwrap_or_default();
    let assets = doc.assets().unwrap_or_default();
    let vector_atlases = doc.vector_atlases().unwrap_or_default();
    let vector_shapes = doc.vector_shapes().unwrap_or_default();

    let sizes = PoolSizes {
        nodes: nodes.len(),
        paints: paints.len(),
        assets: assets.len(),
        strings: doc.strings().unwrap_or_default().len(),
        text_styles: doc.text_styles().unwrap_or_default().len(),
        vector_shapes: vector_shapes.len(),
        vector_atlases: vector_atlases.len(),
    };

    for (i, node) in nodes.iter().enumerate() {
        check_node_links(&mut report, &nodes, i as u32, &node, &sizes);
    }

    // An inert mask stencils nothing — it has no following sibling in its
    // parent, or it is a root (root masks are not applied). A likely
    // mistake, surfaced by name rather than silently doing nothing (story
    // #44 M13). Masks are rare, so the forward scan for a following sibling
    // is cheap.
    for (i, node) in nodes.iter().enumerate() {
        if !node.mask() {
            continue;
        }
        let parent = node.parent();
        let has_following_sibling = parent != NO_PARENT
            && nodes
                .iter()
                .skip(i + 1)
                .any(|later| later.parent() == parent);
        if !has_following_sibling {
            report.push(warning(
                rule::INERT_MASK,
                &Location::Node(node_path(&nodes, i as u32)),
                "this node is a mask but has no following sibling to stencil, so it masks \
                 nothing (a root mask is not applied either)"
                    .to_owned(),
            ));
        }
    }

    // A pool entry and an image asset are each shared by every node that
    // references them, so each is checked once, at its own index. Reporting
    // per referencing node would repeat one authoring mistake N times and
    // bury the rest of the report — which is why they carry a
    // `Location::PaintEntry` / `Location::ImageAsset` rather than a node
    // index that would resolve to an unrelated layer.
    for (i, paint) in paints.iter().enumerate() {
        check_paint_entry(&mut report, &paint, &Location::PaintEntry(i as u32), &sizes);
    }

    for (i, asset) in assets.iter().enumerate() {
        check_asset_entry(&mut report, &asset, &Location::ImageAsset(i as u32));
    }

    // Story B1: the baked-vector index chain. A shape names its atlas and an
    // atlas names its image, each a bare `u32` the loader resolves unchecked;
    // a dangling one is named here, at the pool entry that carries it (the
    // same per-pool posture as the paint and image checks above).
    for (i, shape) in vector_shapes.iter().enumerate() {
        let atlas = shape.atlas();
        if atlas as usize >= sizes.vector_atlases {
            report.push(error(
                rule::VECTOR_SHAPE_ATLAS_OUT_OF_RANGE,
                &Location::VectorShape(i as u32),
                format!(
                    "vector shape references atlas {atlas}, but the document carries {} vector \
                     atlases",
                    sizes.vector_atlases
                ),
            ));
        }
    }
    for (i, atlas) in vector_atlases.iter().enumerate() {
        let image = atlas.image();
        if image as usize >= sizes.assets {
            report.push(error(
                rule::VECTOR_ATLAS_IMAGE_OUT_OF_RANGE,
                &Location::VectorAtlas(i as u32),
                format!(
                    "vector atlas references asset {image}, but the asset table holds {} entries",
                    sizes.assets
                ),
            ));
        }
    }

    for (i, set) in doc.variant_sets().unwrap_or_default().iter().enumerate() {
        check_variant_set(&mut report, &set, i as u32, &sizes);
    }

    // The v0.7 binding tables (story #167). The loader resolves both
    // indices unchecked, so a dangling one is named here; the channel is
    // an append-only enum, range-checked like the layout enums; and a
    // duplicate signal name would make the runtime's by-name lookup
    // ambiguous.
    let signals = doc.signals().unwrap_or_default();
    let mut seen_names: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for (i, signal) in signals.iter().enumerate() {
        let Some(name) = signal.name() else { continue };
        if let Some(first) = seen_names.get(name) {
            report.push(error(
                rule::SIGNAL_NAME_DUPLICATE,
                &Location::Signal(i as u32),
                format!(
                    "signal declaration {i} carries the name \"{name}\", which declaration \
                     {first} already carries; a by-name lookup would resolve one and silently \
                     shadow the other"
                ),
            ));
        } else {
            seen_names.insert(name, i as u32);
        }
    }
    for (i, binding) in doc.bindings().unwrap_or_default().iter().enumerate() {
        let at = Location::Binding(i as u32);
        let signal = binding.signal();
        if signal as usize >= signals.len() {
            report.push(error(
                rule::BINDING_SIGNAL_OUT_OF_RANGE,
                &at,
                format!(
                    "binding references signal {signal}, but the document declares {} signals",
                    signals.len()
                ),
            ));
        }
        let node = binding.node();
        if node as usize >= sizes.nodes {
            report.push(error(
                rule::BINDING_NODE_OUT_OF_RANGE,
                &at,
                format!(
                    "binding references node {node}, but the document carries {} nodes",
                    sizes.nodes
                ),
            ));
        }
        check_enum!(report, &at, "Binding.channel", binding.channel());
        // The transform union tag is append-only like every enum (Format
        // joins at v0.8); the verifier accepts an unknown tag with a
        // payload, and the loader resolves the tag unchecked.
        check_enum!(report, &at, "Binding.transform", binding.transform_type());
    }

    // A text style's color is optional in the schema, so a producer can omit
    // it. Nothing downstream may invent one: the loader would have to pick a
    // default, and a silently-defaulted color is discovered vocabulary (P4).
    // The weight has a schema-pinned range (100..=900); font selection would
    // otherwise clamp it silently or pick an unintended face (issue #129).
    for (i, style) in doc.text_styles().unwrap_or_default().iter().enumerate() {
        // A text style is a pooled surface, so its diagnostics point at
        // `Location::TextStyle` — its pool index, never a `Node` index that
        // would resolve to an unrelated layer (the anti-collision contract on
        // `Location`). The value is a plain `u32` wrap, so no laziness is
        // needed for the clean path.
        let at = Location::TextStyle(i as u32);
        if style.color().is_none() {
            report.push(error(
                rule::TEXT_STYLE_NO_COLOR,
                &at,
                "text style carries no color; the schema makes it optional, but a consumer \
                 that defaults it has silently invented vocabulary (P4)"
                    .to_owned(),
            ));
        }
        let weight = style.weight();
        if !(100..=900).contains(&weight) {
            report.push(error(
                rule::TEXT_STYLE_WEIGHT_OUT_OF_RANGE,
                &at,
                format!(
                    "text style weight is {weight}; the schema pins it to the CSS scale, 100 to \
                     900 inclusive"
                ),
            ));
        }
    }

    report
}

/// One node's index fields and enum values.
///
/// `at` builds the node's name path on demand, only when a rule fires, and
/// memoizes it: a clean document pushes nothing, so it allocates no paths at
/// all (issue #127); a node that trips several rules walks its parent chain
/// once, not once per diagnostic. The earlier shape built every node's owned
/// path string up front, which the common clean case discarded unused.
fn check_node_links(
    report: &mut Report,
    nodes: &flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Node<'_>>>,
    index: u32,
    node: &Node<'_>,
    sizes: &PoolSizes,
) {
    let mut cached_path: Option<NodePath> = None;
    let mut at = || {
        Location::Node(
            cached_path
                .get_or_insert_with(|| node_path(nodes, index))
                .clone(),
        )
    };

    let parent = node.parent();
    if parent != NO_PARENT {
        if parent as usize >= sizes.nodes {
            report.push(error(
                rule::PARENT_OUT_OF_RANGE,
                &at(),
                format!(
                    "node references parent {parent}, but the document carries {} nodes",
                    sizes.nodes
                ),
            ));
        } else if parent >= index {
            // "Array order is DFS order" (schema): a parent is always
            // emitted before its children. A forward or self reference makes
            // every consumer that walks up the tree loop forever.
            report.push(error(
                rule::PARENT_NOT_BEFORE_CHILD,
                &at(),
                format!(
                    "node {index} references parent {parent}, which does not precede it; \
                     the node array is in DFS order, so a parent's index is always lower"
                ),
            ));
        }
    }

    let paint_entry = node.paint_entry();
    if paint_entry != NO_PAINT {
        if paint_entry as usize >= sizes.paints {
            report.push(error(
                rule::PAINT_ENTRY_OUT_OF_RANGE,
                &at(),
                format!(
                    "node references paint entry {paint_entry}, but the paint pool holds {} \
                     entries",
                    sizes.paints
                ),
            ));
        }
        if node.paint().is_some() {
            // The v0.1 walking-skeleton `paint` shorthand is superseded by
            // `paint_entry`. Writing both means the producer has two
            // opinions and one of them is silently discarded.
            report.push(error(
                rule::CONFLICTING_PAINT_REPRESENTATION,
                &at(),
                "node sets both the legacy `paint` shorthand and `paint_entry`; `paint_entry` \
                 supersedes `paint`, so the shorthand would be silently discarded"
                    .to_owned(),
            ));
        }
    }

    let text = node.text();
    if text != NO_TEXT && text as usize >= sizes.strings {
        report.push(error(
            rule::TEXT_STRING_OUT_OF_RANGE,
            &at(),
            format!(
                "node references string {text}, but the string pool holds {} entries",
                sizes.strings
            ),
        ));
    }

    let text_style = node.text_style();
    if text_style != NO_TEXT_STYLE && text_style as usize >= sizes.text_styles {
        report.push(error(
            rule::TEXT_STYLE_OUT_OF_RANGE,
            &at(),
            format!(
                "node references text style {text_style}, but the text-style pool holds {} \
                 entries",
                sizes.text_styles
            ),
        ));
    }

    if let Some(flex) = node.flex() {
        check_enum!(report, &at(), "LayoutContainer.mode", flex.mode());
        check_enum!(
            report,
            &at(),
            "LayoutContainer.main_align",
            flex.main_align()
        );
        check_enum!(
            report,
            &at(),
            "LayoutContainer.cross_align",
            flex.cross_align()
        );
        // The v0.8 grid track lists (story #43): each track's sizing is
        // an append-only enum like the ones above, and its value has a
        // pinned numeric domain like `weight` and stroke width.
        for (axis, tracks) in [("row", flex.grid_rows()), ("column", flex.grid_columns())] {
            for (i, track) in tracks.unwrap_or_default().iter().enumerate() {
                check_enum!(report, &at(), "GridTrack.sizing", track.sizing());
                check_grid_track(report, &at(), axis, i, &track);
            }
        }
        // A Fraction track divides free space, and an axis the container
        // hugs has none — the track, and everything anchored to it,
        // silently collapses to zero (story #43, review finding R7). The
        // constraints table holds the sizing; absent means Fixed.
        if flex.mode() == dashbuf::LayoutMode::Grid
            && let Some(constraints) = node.constraints()
        {
            let hugs = |sizing| sizing == dashbuf::AxisSizing::Hug;
            let has_fraction = |tracks: Option<flatbuffers::Vector<'_, _>>| {
                tracks
                    .unwrap_or_default()
                    .iter()
                    .any(|t: dashbuf::GridTrack| t.sizing() == dashbuf::GridTrackSizing::Fraction)
            };
            for (axis, sizing, tracks) in [
                ("vertical", constraints.sizing_v(), flex.grid_rows()),
                ("horizontal", constraints.sizing_h(), flex.grid_columns()),
            ] {
                if hugs(sizing) && has_fraction(tracks) {
                    report.push(error(
                        rule::GRID_FRACTION_TRACK_UNDER_HUG,
                        &at(),
                        format!(
                            "the grid hugs its {axis} axis but declares a Fraction track on \
                             it; a fraction divides free space, and a hug axis has none, so \
                             the track would silently collapse to zero"
                        ),
                    ));
                }
            }
        }
    }

    if let Some(constraints) = node.constraints() {
        check_enum!(
            report,
            &at(),
            "LayoutConstraints.sizing_h",
            constraints.sizing_h()
        );
        check_enum!(
            report,
            &at(),
            "LayoutConstraints.sizing_v",
            constraints.sizing_v()
        );
        // The v0.8 grid placement (story #43): spans span at least one
        // track, and anchors stay inside the parent's declared track
        // list — or, with no declared list, inside the solver's i16
        // line range (review findings R5/R6).
        for (axis, span) in [
            ("row", constraints.grid_row_span()),
            ("column", constraints.grid_column_span()),
        ] {
            if span == 0 {
                report.push(error(
                    rule::GRID_SPAN_ZERO,
                    &at(),
                    format!("grid {axis} span is 0; spanning no tracks has no meaning"),
                ));
            }
        }
        let parent_flex = match node.parent() {
            NO_PARENT => None,
            parent if (parent as usize) < sizes.nodes && parent < index => {
                nodes.get(parent as usize).flex()
            }
            // A dangling or forward parent is already named above; the
            // anchor check falls back to the undeclared-tracks bound.
            _ => None,
        };
        for (axis, anchor, span, tracks) in [
            (
                "row",
                constraints.grid_row(),
                constraints.grid_row_span(),
                parent_flex.and_then(|f| f.grid_rows()),
            ),
            (
                "column",
                constraints.grid_column(),
                constraints.grid_column_span(),
                parent_flex.and_then(|f| f.grid_columns()),
            ),
        ] {
            let Some(anchor) = anchor else { continue };
            match tracks {
                Some(tracks) if !tracks.is_empty() => {
                    let count = tracks.len();
                    if anchor as usize >= count {
                        report.push(error(
                            rule::GRID_ANCHOR_OUT_OF_RANGE,
                            &at(),
                            format!(
                                "grid {axis} anchor is {anchor}, but the parent declares \
                                 {count} {axis} tracks"
                            ),
                        ));
                    } else if anchor as usize + span as usize > count {
                        // The anchor fits but the spanned range runs off
                        // the end. A span of 0 is separately named above,
                        // and it cannot overflow (anchor + 0 <= count), so
                        // this fires only for a genuine overrun (D7).
                        report.push(error(
                            rule::GRID_SPAN_OUT_OF_RANGE,
                            &at(),
                            format!(
                                "grid {axis} anchor {anchor} plus span {span} runs past the \
                                 parent's {count} {axis} tracks"
                            ),
                        ));
                    }
                }
                // No declared track list (implicit auto tracks, or no
                // grid parent at all): bound the anchor so its 1-based
                // line index fits the solver's i16 lines.
                _ => {
                    if anchor > (i16::MAX as u16) - 1 {
                        report.push(error(
                            rule::GRID_ANCHOR_OUT_OF_RANGE,
                            &at(),
                            format!(
                                "grid {axis} anchor is {anchor}; without a declared track \
                                 list the anchor is bounded at 32766, the largest value \
                                 whose 1-based line index fits the solver's i16 lines"
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Node opacity has a schema-pinned domain (finite, 0..=1). The loader
    // clamps a stray value silently and a non-finite one reads back as
    // fully opaque (`NaN < 1.0` is false), so the load gate names it here —
    // the same posture as the text-style weight range (story #44 M7).
    let opacity = node.opacity();
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        report.push(error(
            rule::NODE_OPACITY_OUT_OF_RANGE,
            &at(),
            format!("node opacity is {opacity}; the schema pins it to the range 0.0 to 1.0"),
        ));
    }
}

/// One grid track's numeric domain: a `Fixed` value must be finite and
/// non-negative (a length), a `Fraction` weight finite and positive (a
/// zero or NaN weight makes the free-space division meaningless). Story
/// #43, review finding R6 — the same posture as `check_stroke_width`.
fn check_grid_track(
    report: &mut Report,
    at: &Location,
    axis: &str,
    index: usize,
    track: &dashbuf::GridTrack<'_>,
) {
    let value = track.value();
    let invalid = match track.sizing() {
        dashbuf::GridTrackSizing::Fixed => !value.is_finite() || value < 0.0,
        dashbuf::GridTrackSizing::Fraction => !value.is_finite() || value <= 0.0,
        // An unknown sizing is already named by check_enum; its value
        // has no domain to check.
        _ => false,
    };
    if invalid {
        let sizing = track.sizing().variant_name().unwrap_or("unknown");
        report.push(error(
            rule::GRID_TRACK_INVALID_VALUE,
            at,
            format!(
                "{axis} track {index} is {sizing}({value}); a Fixed track must be finite and \
                 non-negative, a Fraction weight finite and positive"
            ),
        ));
    }
}

/// A flatbuffer table carrying one `Fill` union — `Paint` (the primary fill)
/// and `FillLayer` (a stacked layer, story C1, debt #146) generate the same
/// three accessors, so `check_fill` validates either through this rather
/// than duplicating the rule per layer.
trait FillUnion<'a> {
    fn fill_type(&self) -> Fill;
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>>;
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>>;
}

impl<'a> FillUnion<'a> for Paint<'a> {
    fn fill_type(&self) -> Fill {
        Paint::fill_type(self)
    }
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>> {
        Paint::fill_as_gradient(self)
    }
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>> {
        Paint::fill_as_image_fill(self)
    }
}

impl<'a> FillUnion<'a> for FillLayer<'a> {
    fn fill_type(&self) -> Fill {
        FillLayer::fill_type(self)
    }
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>> {
        FillLayer::fill_as_gradient(self)
    }
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>> {
        FillLayer::fill_as_image_fill(self)
    }
}

/// One `Fill` union's vocabulary rules: its own enum range, a gradient's
/// stops, an image fill's scale mode and asset index. Shared by the primary
/// `Paint.fill` and every stacked `FillLayer` in `Paint.extra_fills` (story
/// C1) — a layer is not exempt from the same rules just because it sits in
/// a stack.
fn check_fill<'a>(
    report: &mut Report,
    at: &Location,
    fill: &impl FillUnion<'a>,
    sizes: &PoolSizes,
) {
    check_enum!(report, at, "Paint.fill", fill.fill_type());

    if fill.fill_type() == Fill::Gradient
        && let Some(gradient) = fill.fill_as_gradient()
    {
        check_enum!(report, at, "Gradient.kind", gradient.kind());
        // `stops` is `(required)`, so the accessor is not an Option — but
        // required mandates presence, not non-emptiness, which is exactly
        // the false assurance issue #100 names.
        let offsets: Vec<f32> = gradient.stops().iter().map(|s| s.offset()).collect();
        check_gradient_stops(report, at, &offsets);
    }

    if fill.fill_type() == Fill::ImageFill
        && let Some(image_fill) = fill.fill_as_image_fill()
    {
        check_enum!(report, at, "ImageFill.scale_mode", image_fill.scale_mode());
        check_image_index(report, at, image_fill.image(), sizes.assets);
    }
}

/// One paint-pool entry: its fill union (and any stacked fills over it), its
/// stroke, and its image reference.
fn check_paint_entry(report: &mut Report, paint: &Paint<'_>, at: &Location, sizes: &PoolSizes) {
    check_fill(report, at, paint, sizes);

    // Stacked fills (story C1, debt #146): each layer's own vocabulary rules,
    // the same posture as the shadows loop below — one field label for every
    // layer, `at` naming the paint entry rather than the individual layer.
    for layer in paint.extra_fills().unwrap_or_default().iter() {
        check_fill(report, at, &layer, sizes);
    }

    if let Some(stroke) = paint.stroke() {
        check_enum!(report, at, "Stroke.align", stroke.align());
        check_stroke_width(report, at, stroke.width());
    }

    if let Some(corners) = paint.corners() {
        check_corners(
            report,
            at,
            [
                corners.top_left(),
                corners.top_right(),
                corners.bottom_right(),
                corners.bottom_left(),
            ],
        );
    }

    // Story B1: the shape channel. NO_FIELD is the implicit parametric shape;
    // a valid index selects a baked field the loader resolves unchecked, so a
    // dangling one is named here (the same posture as `paint_entry`).
    let shape_field = paint.shape_field();
    if shape_field != NO_FIELD && shape_field as usize >= sizes.vector_shapes {
        report.push(error(
            rule::SHAPE_FIELD_OUT_OF_RANGE,
            at,
            format!(
                "paint entry references vector shape {shape_field}, but the document carries {} \
                 vector shapes",
                sizes.vector_shapes
            ),
        ));
    }

    // v0.11 blurs (story #393): the kind is range-checked like every other
    // append-only enum, and the radius has the same pinned numeric domain as a
    // shadow's blur. The enum check is load-bearing beyond tidiness — the
    // generated binding is a newtype over `u8` whose verifier does no range
    // check, so without this an out-of-range kind would reach
    // `dashscene-core`'s `blur_kind` and be refused there by `unreachable!`
    // instead of being reported here (P4: a named diagnostic at the gate).
    for (i, blur) in paint.blurs().unwrap_or_default().iter().enumerate() {
        check_enum!(report, at, "Blur.kind", blur.kind());
        let radius = blur.radius();
        if !radius.is_finite() || radius < 0.0 {
            report.push(error(
                rule::BLUR_INVALID_RADIUS,
                at,
                format!(
                    "blur {i} has radius {radius}; a blur radius must be finite and non-negative"
                ),
            ));
        }
    }

    // v0.8 shadows (story #45): the kind is an append-only enum, range-checked
    // like the layout enums, and the offset/blur/spread/color have a pinned
    // numeric domain like corners and stroke width.
    for (i, shadow) in paint.shadows().unwrap_or_default().iter().enumerate() {
        check_enum!(report, at, "Shadow.kind", shadow.kind());
        let offset = shadow.offset().map_or([0.0, 0.0], |o| [o.x(), o.y()]);
        let color = shadow.color();
        check_shadow(
            report,
            at,
            i,
            offset,
            shadow.blur(),
            shadow.spread(),
            [color.r(), color.g(), color.b(), color.a()],
        );
    }
}

/// One image asset: its container format and its bytes.
///
/// The painter decodes an asset behind an `expect` documented as "validated
/// upstream (P4)". Reading the asset table only for its length — which is
/// all the index rules need — would leave that `expect` with no upstream.
/// One `AssetEntry` (story #107): the identity and the metadata, since the
/// payload is not in this buffer.
///
/// The bytes a document names live in a blob section, and this gate does not
/// see the file — only the document. So what it can check is that the entry is
/// self-consistent: a 32-byte hash to resolve through the binding, a format
/// this build recognizes, and a non-zero extent for layout to use before the
/// payload is resident. Whether the payload the hash names actually agrees
/// with that recorded format and extent needs the payload, so it lives in
/// [`validate_asset_payloads`] — the load gate's other half (story #437,
/// closing debt #416).
fn check_asset_entry(report: &mut Report, asset: &AssetEntry<'_>, at: &Location) {
    check_enum!(report, at, "AssetEntry.format", asset.format());

    let hash_len = asset.hash().len();
    if hash_len != 32 {
        report.push(error(
            rule::ASSET_HASH_LENGTH,
            at,
            format!("asset hash is {hash_len} bytes; a BLAKE3-256 digest is 32"),
        ));
    }

    if asset.width() == 0 || asset.height() == 0 {
        report.push(error(
            rule::ASSET_ZERO_EXTENT,
            at,
            format!(
                "asset records an intrinsic extent of {}x{}; layout before the payload is                  resident would resolve to nothing",
                asset.width(),
                asset.height()
            ),
        ));
    }
}

/// Maps a document's recorded container format onto the one
/// [`dashpaint::image_id::identify`] answers in, or `None` for a value this
/// build does not know.
///
/// `dashbuf`'s enums are append-only, so a document produced by a newer
/// writer can carry a format number this reader has no variant for. That is
/// not the same as a wrong format, and the caller treats the two differently.
///
/// The `None` arm must stay reserved for values outside the schema's own enum.
/// `flatc` generates `ImageFormat` as a newtype over `u8` rather than a Rust
/// enum, so this match is not checked for exhaustiveness — when the schema
/// appends a format, nothing here fails to compile, and every asset of that
/// format would be skipped by this gate *and* pass `check_enum!`, which only
/// fires on values the generated enum does not name. Two gates would go quiet
/// at once, which is the silent drop P4 forbids, so
/// `every_schema_image_format_maps_to_a_paint_format` fails at the append
/// instead.
fn as_paint_format(format: dashbuf::ImageFormat) -> Option<dashpaint::ImageFormat> {
    match format {
        dashbuf::ImageFormat::Png => Some(dashpaint::ImageFormat::Png),
        dashbuf::ImageFormat::Jpeg => Some(dashpaint::ImageFormat::Jpeg),
        dashbuf::ImageFormat::Gif => Some(dashpaint::ImageFormat::Gif),
        _ => None,
    }
}

/// The load gate's second half: does the payload each `AssetEntry` names
/// agree with what the entry records about it?
///
/// An `AssetEntry` says a payload is a PNG of 512x512 and names it by content
/// hash; the payload itself lives in its own blob section. That is two places
/// describing one asset, and until now nothing checked that they agree
/// (debt #416). `dashc` derived both halves from a single header parse, so
/// they could not disagree — the packer is the second writer, it re-derives
/// payloads, and the rule earns its place the moment two independent code
/// paths can produce the pair.
///
/// Three things can be wrong, and each is a named diagnostic rather than a
/// silent pass or a panic (P4): the payload parses as no image this build
/// knows ([`rule::ASSET_PAYLOAD_UNREADABLE`]), its signature names a
/// different container than the entry does ([`rule::ASSET_FORMAT_MISMATCH`]),
/// or its header reports a different intrinsic extent
/// ([`rule::ASSET_EXTENT_MISMATCH`]). Each matters downstream: a painter
/// dispatches its decoder on the recorded format, and layout runs on the
/// recorded extent before the payload is resident, so a lie in either is
/// discovered as a wrong picture rather than as a bad document.
///
/// The `hash` needs no rule here. `dashbuf::open` resolves each entry through
/// the null binding and verifies the blob against its recorded content hash,
/// so a payload that does not match its hash never reaches this function.
///
/// # What this deliberately does not do
///
/// It never decodes. [`dashpaint::image_id::identify`] reads container
/// headers with bounds-checked slicing and returns; entropy coding and pixel
/// reconstruction stay out of every crate a producer links
/// (`docs/decisions/dashc-identifies-images-never-decodes.md`,
/// `docs/decisions/image-header-parser-lives-in-dashpaint.md`). So this gate
/// answers "does the header agree", never "do the pixels decode" — a payload
/// truncated after its header passes here and fails in the painter, which is
/// the correct division: only a decoder can find that, and a decoder is the
/// component the target-hardware rules keep out of the trusted path.
///
/// An entry whose recorded format this build does not recognize is skipped
/// rather than judged. The schema's enums are append-only, so such a document
/// is newer than this reader, and its payload may be a container this build
/// cannot identify either — calling that "unreadable" would report a stale
/// reader as a broken file. [`validate_document`] already names it, as
/// `rule::UNKNOWN_ENUM`, which is the honest diagnosis.
///
/// # Pairing
///
/// `payloads` must be in entry order, one per entry — exactly what
/// `dashbuf::open` returns. Fewer payloads than entries is named once, at the
/// first entry that has none, and the entries past it go unchecked; it means
/// the caller paired a document with the wrong payload list, so repeating it
/// per entry would bury the rest of the report. Surplus payloads are ignored
/// without a diagnostic: a payload that no entry names describes nothing in
/// the document, so there is no document defect to report, and P4 is about
/// vocabulary the document carries.
pub fn validate_asset_payloads(doc: &Document<'_>, payloads: &[&[u8]]) -> Report {
    let mut report = Report::default();
    let assets = doc.assets().unwrap_or_default();

    if payloads.len() < assets.len() {
        report.push(error(
            rule::ASSET_PAYLOAD_MISSING,
            &Location::ImageAsset(payloads.len() as u32),
            format!(
                "the document carries {} asset entries but {} payload(s) were supplied; \
                 entries from index {} on were not checked against their bytes",
                assets.len(),
                payloads.len(),
                payloads.len()
            ),
        ));
    }

    // `zip` stops at the shorter side, so a short `payloads` never indexes
    // past its end — the count above is the diagnosis, not a bounds check.
    for (i, (entry, payload)) in assets.iter().zip(payloads.iter()).enumerate() {
        let at = Location::ImageAsset(i as u32);

        let Some(recorded_format) = as_paint_format(entry.format()) else {
            continue;
        };

        let header = match dashpaint::image_id::identify(payload) {
            Ok(header) => header,
            Err(e) => {
                report.push(error(
                    rule::ASSET_PAYLOAD_UNREADABLE,
                    &at,
                    format!(
                        "asset records {recorded_format:?}, but its {} byte payload could not \
                         be read as an image header: {e}",
                        payload.len()
                    ),
                ));
                continue;
            }
        };

        // Format and extent are reported independently. A payload that is the
        // wrong container still has a real extent, and knowing both facts is
        // what tells a producer whether it swapped two assets or mis-recorded
        // one.
        if header.format != recorded_format {
            report.push(error(
                rule::ASSET_FORMAT_MISMATCH,
                &at,
                format!(
                    "asset records format {recorded_format:?}, but its payload's own signature \
                     is {:?}; a painter dispatches its decoder on the recorded format",
                    header.format
                ),
            ));
        }

        if header.width != entry.width() || header.height != entry.height() {
            report.push(error(
                rule::ASSET_EXTENT_MISMATCH,
                &at,
                format!(
                    "asset records an intrinsic extent of {}x{}, but its payload's header \
                     reports {}x{}; layout uses the recorded extent before the payload is \
                     resident",
                    entry.width(),
                    entry.height(),
                    header.width,
                    header.height
                ),
            ));
        }
    }

    report
}

/// One variant set (v0.4, issue #20): the active-member index, and every
/// member's overrides.
///
/// `VariantOverride.value` is `(required)` in the schema, so the union
/// cannot be absent by construction — a producer that omits it never
/// reaches this function, and foreign bytes that try are already rejected
/// by the flatbuffer verifier, before `validate_document` runs. `check_enum!`
/// still runs on `value_type()` for the same reason it runs on every other
/// union/enum here: presence is not the same guarantee as "a member this
/// build recognizes."
fn check_variant_set(report: &mut Report, set: &VariantSet<'_>, index: u32, sizes: &PoolSizes) {
    let at = Location::VariantSet(index);
    let members = set.members().unwrap_or_default();

    if members.is_empty() {
        report.push(error(
            rule::VARIANT_SET_NO_MEMBERS,
            &at,
            "variant set has no members; there is nothing for set_variant to select".to_owned(),
        ));
        return;
    }

    let active_member = set.active_member();
    if active_member as usize >= members.len() {
        report.push(error(
            rule::VARIANT_ACTIVE_MEMBER_OUT_OF_RANGE,
            &at,
            format!(
                "variant set's active_member is {active_member}, but it carries {} members",
                members.len()
            ),
        ));
    }

    for member in members.iter() {
        for override_ in member.overrides().unwrap_or_default().iter() {
            check_enum!(report, &at, "VariantOverride.value", override_.value_type());

            let node = override_.node();
            if node as usize >= sizes.nodes {
                report.push(error(
                    rule::VARIANT_OVERRIDE_NODE_OUT_OF_RANGE,
                    &at,
                    format!(
                        "variant override references node {node}, but the document carries {} \
                         nodes",
                        sizes.nodes
                    ),
                ));
            }
        }
    }
}

/// One node's slash-joined name path, walked up the parent chain on demand.
///
/// Built only when a diagnostic actually points at this node, so a clean
/// document allocates no paths at all (issue #127). The earlier shape
/// memoized every node's path in a forward pass before any rule ran, which
/// the common clean case discarded.
///
/// Safe against a malformed parent link by construction: a parent index
/// that is not strictly lower than the child's is treated as a root here,
/// and reported separately by `node.parent-not-before-child`. Each followed
/// link strictly decreases the index, so the walk terminates on any input.
fn node_path(
    nodes: &flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Node<'_>>>,
    index: u32,
) -> NodePath {
    let mut segments: Vec<String> = Vec::new();
    let mut i = index as usize;
    loop {
        let node = nodes.get(i);
        let segment = node
            .name()
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("#{i}"), str::to_owned);
        segments.push(segment);
        let parent = node.parent();
        if parent != NO_PARENT && (parent as usize) < i {
            i = parent as usize;
        } else {
            break;
        }
    }
    segments.reverse();
    NodePath::new(index, format!("/{}", segments.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every format the schema names must map to a `dashpaint` one.
    ///
    /// This is the guard on [`as_paint_format`]'s `_ => None` arm. `flatc`
    /// generates `ImageFormat` as a newtype over `u8`, so appending `Webp` to
    /// the schema would not make that match fail to compile — it would fall to
    /// `None`, and `validate_asset_payloads` would skip every WebP asset while
    /// `check_enum!` stayed quiet too, because the generated enum *does* name
    /// the value. Two gates going silent at once is the failure this catches,
    /// at the moment a human is editing the schema and can decide.
    #[test]
    fn every_schema_image_format_maps_to_a_paint_format() {
        for &format in dashbuf::ImageFormat::ENUM_VALUES {
            assert!(
                as_paint_format(format).is_some(),
                "the schema names {format:?}, but as_paint_format has no arm for it, so \
                 validate_asset_payloads would silently skip every asset of that format \
                 (P4). Add the arm — and the dashpaint::ImageFormat variant if it is \
                 missing too."
            );
        }
    }
}
