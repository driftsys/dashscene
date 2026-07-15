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
    Document, Fill, Image, NO_PAINT, NO_PARENT, NO_TEXT, NO_TEXT_STYLE, Node, Paint, VariantSet,
};

use crate::paint::{
    check_gradient_stops, check_image_bytes, check_image_index, check_stroke_width, error,
};
use crate::{Location, NodePath, Report, rule};

/// The document's pool sizes, so the index rules cannot be handed the wrong
/// count. Four bare `usize` parameters would be positionally
/// interchangeable, and transposing two would silently validate an index
/// against the wrong pool.
struct PoolSizes {
    nodes: usize,
    paints: usize,
    images: usize,
    strings: usize,
    text_styles: usize,
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
/// It takes no [`Profile`](crate::Profile). Every construct the v0.3
/// schema can express sits in docs/specification/04-figma-vocabulary-profile.md's NOW band — the schema has no
/// blur, no blend mode, no mask — so there is nothing here for a profile
/// to differentiate. Out-of-profile constructs are caught at the import
/// gate ([`crate::triage`]), which is the only place they exist: by the
/// time a construct is in the schema, it is in the vocabulary. The
/// parameter returns when the effect vocabulary lands at v0.8, with a rule
/// behind it — an ignored one now would imply a check that does not
/// happen.
///
/// It checks no geometry budgets. P1 says the document carries intent,
/// never results, so a `Hug`/`Fill` node has no box for a stroke width to
/// be measured against. Those run on the solved scene
/// ([`crate::validate_scene`]).
pub fn validate_document(doc: &Document<'_>) -> Report {
    let mut report = Report::default();

    let nodes = doc.nodes().unwrap_or_default();
    let paints = doc.paints().unwrap_or_default();
    let images = doc.images().unwrap_or_default();

    let sizes = PoolSizes {
        nodes: nodes.len(),
        paints: paints.len(),
        images: images.len(),
        strings: doc.strings().unwrap_or_default().len(),
        text_styles: doc.text_styles().unwrap_or_default().len(),
    };

    let paths = node_paths(&nodes);
    for (i, node) in nodes.iter().enumerate() {
        check_node_links(&mut report, &node, &paths[i], &sizes);
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

    for (i, image) in images.iter().enumerate() {
        check_image_asset(&mut report, &image, &Location::ImageAsset(i as u32));
    }

    for (i, set) in doc.variant_sets().unwrap_or_default().iter().enumerate() {
        check_variant_set(&mut report, &set, i as u32, &sizes);
    }

    // A text style's color is optional in the schema, so a producer can omit
    // it. Nothing downstream may invent one: the loader would have to pick a
    // default, and a silently-defaulted color is discovered vocabulary (P4).
    for (i, style) in doc.text_styles().unwrap_or_default().iter().enumerate() {
        if style.color().is_none() {
            let at = Location::Node(NodePath::new(i as u32, format!("<text style #{i}>")));
            report.push(error(
                rule::TEXT_STYLE_NO_COLOR,
                &at,
                "text style carries no color; the schema makes it optional, but a consumer \
                 that defaults it has silently invented vocabulary (P4)"
                    .to_owned(),
            ));
        }
    }

    report
}

/// One node's index fields and enum values.
fn check_node_links(report: &mut Report, node: &Node<'_>, path: &NodePath, sizes: &PoolSizes) {
    let at = Location::Node(path.clone());
    let index = path.index;

    let parent = node.parent();
    if parent != NO_PARENT {
        if parent as usize >= sizes.nodes {
            report.push(error(
                rule::PARENT_OUT_OF_RANGE,
                &at,
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
                &at,
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
                &at,
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
                &at,
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
            &at,
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
            &at,
            format!(
                "node references text style {text_style}, but the text-style pool holds {} \
                 entries",
                sizes.text_styles
            ),
        ));
    }

    if let Some(flex) = node.flex() {
        check_enum!(report, &at, "LayoutContainer.mode", flex.mode());
        check_enum!(report, &at, "LayoutContainer.main_align", flex.main_align());
        check_enum!(
            report,
            &at,
            "LayoutContainer.cross_align",
            flex.cross_align()
        );
    }

    if let Some(constraints) = node.constraints() {
        check_enum!(
            report,
            &at,
            "LayoutConstraints.sizing_h",
            constraints.sizing_h()
        );
        check_enum!(
            report,
            &at,
            "LayoutConstraints.sizing_v",
            constraints.sizing_v()
        );
    }
}

/// One paint-pool entry: its fill union, its gradient stops, its stroke, and
/// its image reference.
fn check_paint_entry(report: &mut Report, paint: &Paint<'_>, at: &Location, sizes: &PoolSizes) {
    check_enum!(report, at, "Paint.fill", paint.fill_type());

    if paint.fill_type() == Fill::Gradient
        && let Some(gradient) = paint.fill_as_gradient()
    {
        check_enum!(report, at, "Gradient.kind", gradient.kind());
        // `stops` is `(required)`, so the accessor is not an Option — but
        // required mandates presence, not non-emptiness, which is exactly
        // the false assurance issue #100 names.
        let offsets: Vec<f32> = gradient.stops().iter().map(|s| s.offset()).collect();
        check_gradient_stops(report, at, &offsets);
    }

    if paint.fill_type() == Fill::ImageFill
        && let Some(image_fill) = paint.fill_as_image_fill()
    {
        check_enum!(report, at, "ImageFill.scale_mode", image_fill.scale_mode());
        check_image_index(report, at, image_fill.image(), sizes.images);
    }

    if let Some(stroke) = paint.stroke() {
        check_enum!(report, at, "Stroke.align", stroke.align());
        check_stroke_width(report, at, stroke.width());
    }
}

/// One image asset: its container format and its bytes.
///
/// The painter decodes an asset behind an `expect` documented as "validated
/// upstream (P4)". Reading the asset table only for its length — which is
/// all the index rules need — would leave that `expect` with no upstream.
fn check_image_asset(report: &mut Report, image: &Image<'_>, at: &Location) {
    check_enum!(report, at, "Image.format", image.format());
    check_image_bytes(report, at, image.bytes().map_or(0, |bytes| bytes.len()));
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

/// Every node's slash-joined name path, memoized in one forward pass.
///
/// Safe against a malformed parent link by construction: a parent index
/// that is not strictly lower than the child's is treated as a root here,
/// and reported separately by `node.parent-not-before-child`. So this walk
/// terminates on any input.
fn node_paths(
    nodes: &flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Node<'_>>>,
) -> Vec<NodePath> {
    let mut paths: Vec<NodePath> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let index = i as u32;
        let segment = node
            .name()
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("#{index}"), str::to_owned);
        let parent = node.parent();
        let path = if parent != NO_PARENT && (parent as usize) < i {
            format!("{}/{segment}", paths[parent as usize].path)
        } else {
            format!("/{segment}")
        };
        paths.push(NodePath::new(index, path));
    }
    paths
}
