//! Prototype lowering (story #773): Figma's component sets into the variant
//! table, and the reactions that animate a switch into `VariantTransition`.
//!
//!     Figma REST JSON → lower → Document → emit → validate → .dsb
//!
//! Two captures are the whole input. `prototype-smart-animate.json` is the
//! half that maps: a two-variant set whose members differ in rect props only,
//! `ON_CLICK` → `NODE`/`CHANGE_TO` → `SMART_ANIMATE` on both members, and one
//! instance per easing arm. `prototype-refused.json` is the half that does
//! not, one construct per node so a diagnostic bisects to a name.
//!
//! The unit tests in `figma/prototype.rs` cover the reaction reader itself —
//! what one interaction lowers to. This binary covers what reaches the
//! *document*: the variant sets, the overrides they carry against the
//! instance's own node indices, which member each transition is keyed on, and
//! which constructs withhold the bytes.

use std::collections::BTreeMap;

use dashc_wasm::figma::{lower, lower_with_bindings_and_policy};
use dashc_wasm::{
    CompileError, Document, Easing, EmitPolicy, TransitionSpec, VariantValue, compile_figma,
    compile_figma_with_bindings_and_policy,
};
use dashscene_validator::{Location, Profile, Severity};

mod common;
use common::{node, parse};

const SMART_ANIMATE: &str =
    include_str!("../../../corpus/figma-fixtures/prototype-smart-animate.json");
const REFUSED: &str = include_str!("../../../corpus/figma-fixtures/prototype-refused.json");

/// The lowered document and its diagnostics, for a capture that lowers.
fn lowered(json: &str) -> (Document, Vec<dashscene_validator::Diagnostic>) {
    lower(&parse(json), Profile::Core, &BTreeMap::new()).expect("the capture lowers")
}

/// One instance's variant set. Sets are emitted in document order, so set
/// `i` belongs to the `i`th `INSTANCE` — `instance-inherited`, `easing-linear`,
/// `easing-ease-in-and-out`, then the four spring presets, then
/// `easing-custom-spring`.
fn set_of(doc: &Document, index: usize) -> &dashc_wasm::VariantSet {
    &doc.variant_sets[index]
}

/// The first lowered node of a given name, which for this fixture is
/// `instance-inherited`'s — the same instance `set_of(doc, 0)` belongs to.
/// Every instance bakes children of the same three names, so the pairing has
/// to be stated rather than assumed.
fn first_instance_child<'a>(doc: &'a Document, name: &str) -> (u32, &'a dashc_wasm::Node) {
    node(doc, name)
}

#[test]
fn each_instance_of_a_component_set_lowers_its_own_variant_set() {
    // A `VariantOverride` names a document node index and a definition lowers
    // to no document node, so the table cannot hang off the component set. It
    // hangs off each instance instead — eight of them here, one per easing
    // arm plus the inheriting one.
    let (doc, _) = lowered(SMART_ANIMATE);
    assert_eq!(
        doc.variant_sets.len(),
        8,
        "one variant set per INSTANCE of the set, and the fixture carries eight",
    );
    for set in &doc.variant_sets {
        assert_eq!(
            set.members
                .iter()
                .filter_map(|m| m.name.clone())
                .collect::<Vec<_>>(),
            vec!["state=rest", "state=active"],
            "the members are the set's own COMPONENTs, in declaration order",
        );
        // Every instance's `componentId` is `1:2` — `state=rest` — so the
        // member it shows is the one a loaded document first commits.
        assert_eq!(set.active_member, 0, "componentId names state=rest");
    }
}

#[test]
fn a_member_overrides_exactly_the_props_that_differ_from_the_active_one() {
    // The fan-out the fixture was authored for: one Figma interaction, four
    // tracks across three children. `bar` differs in Width, `dot` in X,
    // `panel` in Y and Height — and nothing else differs anywhere.
    //
    // The override values are checked against the *document's* own base
    // values, not only against each other: an override is a jump from what
    // the document already carries, so an override list that agreed with
    // itself and disagreed with the rect table would animate to the wrong
    // picture and this assertion is what sees it.
    let (doc, _) = lowered(SMART_ANIMATE);
    let (bar, bar_node) = first_instance_child(&doc, "bar");
    let (dot, dot_node) = first_instance_child(&doc, "dot");
    let (panel, panel_node) = first_instance_child(&doc, "panel");
    assert_eq!(
        (bar_node.box2d.width, dot_node.box2d.x),
        (64.0, 16.0),
        "the base is the rest member's authored geometry",
    );
    assert_eq!((panel_node.box2d.y, panel_node.box2d.height), (96.0, 32.0),);

    let set = set_of(&doc, 0);
    assert_eq!(
        set.members[0].overrides,
        vec![],
        "the active member is the document's own state, so it overrides nothing",
    );
    let overrides: Vec<(u32, VariantValue)> = set.members[1]
        .overrides
        .iter()
        .map(|o| (o.node, o.value))
        .collect();
    assert_eq!(
        overrides,
        vec![
            (bar, VariantValue::Width(288.0)),
            (dot, VariantValue::X(280.0)),
            (panel, VariantValue::Y(88.0)),
            (panel, VariantValue::Height(76.0)),
        ],
        "one override per differing prop, against the instance's own node indices",
    );
}

#[test]
fn the_transition_is_keyed_on_the_member_the_switch_travels_to() {
    // `state=rest`'s reaction targets `state=active` with EASE_OUT / 0.3 s and
    // `state=active`'s targets rest with EASE_IN / 0.2 s, so the two members
    // carry *different* specs. That asymmetry is the whole reason story #771
    // keyed the transition on the destination rather than on the set.
    let (doc, _) = lowered(SMART_ANIMATE);
    let set = set_of(&doc, 0);

    let to_rest = set.members[0]
        .transition
        .as_ref()
        .expect("rest is a destination");
    let to_active = set.members[1]
        .transition
        .as_ref()
        .expect("active is a destination");
    assert_eq!(
        to_rest.tracks[0].spec,
        TransitionSpec::Tween {
            duration: 0.2,
            easing: dashc_wasm::Easing::EaseIn,
        },
    );
    assert_eq!(
        to_active.tracks[0].spec,
        TransitionSpec::Tween {
            duration: 0.3,
            easing: dashc_wasm::Easing::EaseOut,
        },
    );
    // Figma has no stagger, so this producer always writes zero.
    assert_eq!((to_rest.stagger, to_active.stagger), (0.0, 0.0));
}

#[test]
fn every_track_names_a_rect_channel_of_a_node_the_document_carries() {
    // Both directions carry the same four tracks: the track list is the union
    // of what the members override, so a switch *back* to the active member
    // animates exactly the props the other one was overriding. A per-member
    // list would have left the return trip with nothing to animate, because
    // the active member overrides nothing.
    use dashc_wasm::BindingChannel::{Height, Width, X, Y};
    let (doc, _) = lowered(SMART_ANIMATE);
    let (bar, _) = first_instance_child(&doc, "bar");
    let (dot, _) = first_instance_child(&doc, "dot");
    let (panel, _) = first_instance_child(&doc, "panel");
    let expected = vec![(bar, Width), (dot, X), (panel, Y), (panel, Height)];

    for member in &set_of(&doc, 0).members {
        let transition = member.transition.as_ref().expect("both members animate");
        assert_eq!(
            transition
                .tracks
                .iter()
                .map(|t| (t.node, t.channel))
                .collect::<Vec<_>>(),
            expected,
            "{:?} animates the same four channels in either direction",
            member.name,
        );
    }
}

#[test]
fn an_instances_own_reaction_overrides_the_sets_default() {
    // `easing-linear` overrides only the arm it declares — the switch *to*
    // `state=active`. The switch back keeps the set's own EASE_IN / 0.2,
    // which is what "a reaction overrides the member's default rather than
    // replacing the field" means (story #771).
    let (doc, _) = lowered(SMART_ANIMATE);
    // Instance order is document order: instance-inherited, easing-linear, …
    let set = set_of(&doc, 1);
    assert_eq!(
        set.members[1]
            .transition
            .as_ref()
            .expect("active animates")
            .tracks[0]
            .spec,
        TransitionSpec::Tween {
            duration: 0.05,
            easing: dashc_wasm::Easing::Linear,
        },
        "the instance's own LINEAR / 0.05 wins over the set's EASE_OUT / 0.3",
    );
    assert_eq!(
        set.members[0]
            .transition
            .as_ref()
            .expect("rest animates")
            .tracks[0]
            .spec,
        TransitionSpec::Tween {
            duration: 0.2,
            easing: dashc_wasm::Easing::EaseIn,
        },
        "the arm it did not declare keeps the set's default",
    );
}

#[test]
fn a_spring_preset_is_named_and_its_switch_lands_in_one_frame() {
    // GENTLE, QUICK, BOUNCY and SLOW arrive as a bare `{"type": …}` with no
    // `easingFunctionSpring`, so REST supplies none of the stiffness and
    // damping a `dashcue` Spring needs. Lowering one would put numbers in the
    // document that no producer wrote.
    //
    // It is a warning and not an error on purpose: the switch still lowers,
    // and making it an error would stop this whole fixture emitting — which
    // is the one thing `corpus/figma-fixtures/README.md` split the two
    // captures to prevent.
    let (doc, diagnostics) = lowered(SMART_ANIMATE);
    for preset in ["GENTLE", "QUICK", "BOUNCY", "SLOW"] {
        let named = diagnostics.iter().find(|d| d.message.contains(preset));
        let named = named.unwrap_or_else(|| panic!("{preset} must be named"));
        assert_eq!(named.rule, "figma.prototype.unsupported-motion");
        assert_eq!(named.severity, Severity::Warning, "{preset}");
    }

    // Sets 3..7 are the four preset instances. Their `state=active` carries
    // no transition — the state change ships and lands in one frame — while
    // the switch back keeps the set's own default.
    for index in 3..7 {
        let set = set_of(&doc, index);
        assert!(
            set.members[1].transition.is_none(),
            "set {index}: a refused curve lowers no transition, rather than the wrong one",
        );
        assert!(
            !set.members[1].overrides.is_empty(),
            "set {index}: the switch itself still lowers",
        );
    }

    let (bytes, report) = compile_figma(SMART_ANIMATE, Profile::Core, &BTreeMap::new())
        .expect("the mapping fixture still emits, which is why the refusals are warnings");
    assert!(!bytes.is_empty());
    assert!(!report.has_errors(), "{report}");
}

#[test]
fn the_refused_capture_withholds_the_bytes_and_names_every_construct() {
    // The diagnostic fixture's contract (`corpus/figma-fixtures/README.md`):
    // under R6 it must emit no `.dsb`, and each node names what it holds.
    //
    // Four of its sixteen `refused-*` nodes carry no interaction, so twelve
    // do: `refused-destination` and `refused-overlay-target` are the two
    // navigation targets, `refused-mouse-enter`'s `setReactionsAsync` write is
    // still refused, and `refused-fill-diff` is a `COMPONENT_SET` — the
    // variant-diff case, asserted by its own test below rather than here.
    // The list below is thirteen construct *names* drawn from those twelve
    // cells, because `refused-scroll-animate` contributes two — its transition
    // and its navigation. They are grouped by what the diagnostic calls each
    // one, which is not always what the authoring command calls it: `OVERLAY`
    // and `SCROLL_TO` arrive as the `navigation` of a `NODE` action.
    let error = compile_figma(REFUSED, Profile::Core, &BTreeMap::new())
        .expect_err("a refused prototype construct withholds the bytes under Strict (R6)");
    let CompileError::Diagnostics(found) = error else {
        panic!("the refusal arrives as diagnostics, not as a walk abort");
    };
    let report = format!("{found}");

    for construct in [
        // transitions
        "DISSOLVE",
        "PUSH",
        "SCROLL_ANIMATE",
        // easings
        "CUSTOM_CUBIC_BEZIER",
        "EASE_OUT_BACK",
        // triggers
        "AFTER_TIMEOUT",
        "MOUSE_DOWN",
        "ON_KEY_DOWN",
        // navigations
        "SCROLL_TO",
        "OVERLAY",
        // actions
        "URL",
        "SET_VARIABLE",
        "CONDITIONAL",
    ] {
        assert!(
            report.contains(construct),
            "{construct} must be named rather than dropped (P4)",
        );
    }
    let errors: Vec<&dashscene_validator::Diagnostic> = found
        .diagnostics()
        .iter()
        .filter(|d| {
            d.rule == "figma.prototype.unsupported-interaction" && d.severity == Severity::Error
        })
        .collect();
    assert!(
        !errors.is_empty(),
        "an interaction with no lowering is the omission class, which withholds the bytes",
    );
    // Twenty-one, not twelve: every finding survives one pass (debt #149), so
    // nine of the twelve interaction-carrying nodes report two independent gaps
    // — a refused navigation beside a refused transition or trigger. The
    // number is pinned here because `corpus/figma-fixtures/README.md` and the
    // manifest both state it, and nothing else would notice it going stale.
    //
    // It was seventeen across ten until the re-capture, which landed the
    // `SCROLL_ANIMATE` write that had been refused and added a
    // `refused-mouse-down` cell.
    assert_eq!(errors.len(), 21, "{found}");
    let nodes: std::collections::BTreeSet<String> =
        errors.iter().map(|d| format!("{}", d.at)).collect();
    assert_eq!(nodes.len(), 12, "across twelve nodes: {found}");
}

#[test]
fn a_fill_only_variant_diff_is_named_rather_than_animated() {
    // The eleventh construct, and the one every real Figma file hits: a valid
    // SMART_ANIMATE over two members that differ in FILL only. The fill
    // difference is expressible as a `VariantFill` override; the *track* is
    // not, because commit resolves a node's paint from the variant overlay
    // ahead of its staged value, so a paint sample is masked by the member it
    // travels towards (issue #891).
    //
    // `refused-fill-diff` carries no INSTANCE, which is why the member diff
    // is computed at the component set: an instance-only path would leave
    // this construct unexercised.
    let (_, diagnostics) = lowered(REFUSED);
    let named: Vec<&dashscene_validator::Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-motion")
        .collect();
    assert!(
        named.iter().any(|d| d.message.contains("differ in fill")),
        "the fill difference is named: {named:?}",
    );
    assert!(
        named.iter().any(|d| d.message.contains("no rect channel")),
        "and so is the consequence — the transition has nothing left to animate: {named:?}",
    );
    for diagnostic in named {
        assert_eq!(
            diagnostic.severity,
            Severity::Warning,
            "a motion degrade never withholds bytes",
        );
    }
}

#[test]
fn the_partial_policy_downgrades_an_interaction_refusal() {
    // The same policy split `figma.unsupported` has (story S0-impl): under
    // `Partial` the omission is a warning and the document emits. Nothing is
    // skipped either way — an interaction is not paint, so the node keeps
    // painting and only the behaviour is dropped.
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        REFUSED,
        Profile::Core,
        &BTreeMap::new(),
        &[],
        EmitPolicy::Partial,
    )
    .expect("Partial emits the document with the gap named");
    assert!(!bytes.is_empty());
    assert!(!report.has_errors(), "{report}");
    assert!(report.has("figma.prototype.unsupported-interaction"));

    let (doc, _) = lowered(REFUSED);
    assert!(
        doc.nodes
            .iter()
            .any(|n| n.name.as_deref() == Some("refused-dissolve")),
        "the refused node still paints: what has no lowering is its behaviour, not its box",
    );
}

#[test]
fn the_capture_emits_and_reloads_byte_identically() {
    // R7. The variant table is built from `BTreeMap` traversals and
    // declaration order throughout, so one input yields one byte string —
    // which a table assembled from a `HashMap` would not.
    let (first, _) = compile_figma(SMART_ANIMATE, Profile::Core, &BTreeMap::new())
        .expect("the mapping fixture emits");
    let (second, _) = compile_figma(SMART_ANIMATE, Profile::Core, &BTreeMap::new())
        .expect("the mapping fixture emits");
    assert_eq!(first, second, "emission is byte-reproducible (R7)");
}

// ---------------------------------------------------------------------------
// Synthetic sets.
//
// The two captures pin what Figma really sends, and between them they exercise
// exactly one shape: a rect-only diff with an instance, and a fill-only diff
// with none. Everything below is a case no captured file carries — a non-rect
// override actually reaching the document, the paths that refuse an instance,
// and the three shapes a real design file hits that the fixtures do not. Each
// is built from the field shapes the captures pin, and says so.
// ---------------------------------------------------------------------------

/// A one-page document whose canvas holds `top`.
fn document_with(top: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "document": {
            "name": "Document",
            "type": "DOCUMENT",
            "children": [{ "name": "Page 1", "type": "CANVAS", "children": top }],
        },
    })
}

fn lower_json(value: serde_json::Value) -> (Document, Vec<dashscene_validator::Diagnostic>) {
    lower_json_with_policy(value, EmitPolicy::Strict)
}

/// [`lower_json`] under a chosen emit policy, which is what separates an
/// omission — withheld under `Strict` — from a degrade that is a warning
/// either way.
fn lower_json_with_policy(
    value: serde_json::Value,
    policy: EmitPolicy,
) -> (Document, Vec<dashscene_validator::Diagnostic>) {
    let file: dashc_wasm::figma::rest::FigmaFile =
        serde_json::from_value(value).expect("the synthetic document parses");
    lower_with_bindings_and_policy(&file, Profile::Core, &BTreeMap::new(), &[], policy)
        .expect("the document lowers")
}

fn solid(r: f64, g: f64, b: f64) -> serde_json::Value {
    serde_json::json!([{
        "type": "SOLID",
        "blendMode": "NORMAL",
        "color": { "r": r, "g": g, "b": b, "a": 1.0 },
    }])
}

/// A frame-like node at a page-absolute box.
fn boxed(id: &str, name: &str, kind: &str, x: f64, y: f64, w: f64, h: f64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "type": kind,
        "absoluteBoundingBox": { "x": x, "y": y, "width": w, "height": h },
        "size": { "x": w, "y": h },
        "fills": solid(0.5, 0.5, 0.5),
    })
}

/// A two-member set and one instance of the first member, built from `child`
/// pairs — `(rest, active)` nodes sharing a name. `instance` is the instance's
/// own baked subtree, page-offset by 200 on y.
fn set_with(
    rest: Vec<serde_json::Value>,
    active: Vec<serde_json::Value>,
    baked: Vec<serde_json::Value>,
) -> serde_json::Value {
    let member = |id: &str, name: &str, children: Vec<serde_json::Value>| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::Value::Array(children);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::Value::Array(baked);
    document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", rest),
                member("1:6", "state=active", active),
            ],
        },
        instance,
    ]))
}

#[test]
fn a_visibility_difference_lowers_as_an_override_and_is_named_as_unanimatable() {
    // The claim "overrides cover the whole VariantValue vocabulary; tracks
    // cover the four rect channels only" has no captured case: the fill-diff
    // fixture carries no instance, so no capture ever emits a non-rect
    // override. This is that case, and it is the only test that would notice
    // if `Prop::Visible` stopped reaching `override_value`.
    let mut hidden = boxed("1:7", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
    hidden["visible"] = serde_json::json!(false);
    let (doc, diagnostics) = lower_json(set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![hidden],
        vec![boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)],
    ));

    let (bar, _) = node(&doc, "bar");
    assert_eq!(doc.variant_sets.len(), 1);
    assert_eq!(
        doc.variant_sets[0].members[1].overrides,
        vec![dashc_wasm::VariantOverride {
            node: bar,
            value: VariantValue::Visible(false),
        }],
        "a visibility difference lowers as an override",
    );
    // No transition is declared on this set, so nothing names the motion —
    // there is none to lose.
    assert!(
        diagnostics.is_empty(),
        "a set with no reaction declares no motion: {diagnostics:?}",
    );
}

#[test]
fn a_rotation_difference_lowers_as_an_override_and_is_named_when_a_switch_animates() {
    // The other half of the same claim, plus the naming path: with a reaction
    // present, a non-rect difference earns exactly one motion diagnostic.
    // Figma reports a rotated node's bounding box as the bounds of the
    // *rotated* shape (`rest.rs`), so a node whose own origin is still
    // (10, 10) reports a box that starts 9.5885 above it and is 27.14 on a
    // side — the axis-aligned bounds of a 20x20 square turned by -0.5 rad.
    // Giving it the unrotated box instead would move the node as well as turn
    // it, and the test would be measuring the fixture rather than the diff.
    let mut turned = boxed(
        "1:7",
        "bar",
        "FRAME",
        10.0,
        0.41148922791594,
        27.140162,
        27.140162,
    );
    turned["rotation"] = serde_json::json!(-0.5);
    turned["size"] = serde_json::json!({ "x": 20.0, "y": 20.0 });
    let mut doc_json = set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![turned],
        vec![boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)],
    );
    // `state=rest`'s own reaction, in the shape prototype-smart-animate pins.
    doc_json["document"]["children"][0]["children"][0]["children"][0]["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:6",
            "navigation": "CHANGE_TO",
            "transition": {
                "type": "SMART_ANIMATE",
                "easing": { "type": "EASE_OUT" },
                "duration": 0.3,
            },
        }],
    }]);

    let (doc, diagnostics) = lower_json(doc_json);
    assert!(
        matches!(
            doc.variant_sets[0].members[1].overrides[..],
            [dashc_wasm::VariantOverride {
                value: VariantValue::Rotation { .. },
                ..
            }]
        ),
        "a rotation difference lowers as an override: {:?}",
        doc.variant_sets[0].members[1].overrides,
    );
    assert!(
        doc.variant_sets[0].members[1].transition.is_none(),
        "and no track carries it, so the switch lands in one frame",
    );
    let named: Vec<&str> = diagnostics.iter().map(|d| d.rule).collect();
    assert_eq!(
        named,
        vec![
            "figma.prototype.unsupported-motion",
            "figma.prototype.unsupported-motion",
        ],
        "the rotation, and that nothing rect-shaped is left to animate: {diagnostics:?}",
    );
    assert!(
        diagnostics[0].message.contains("rotation"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_text_colour_difference_is_refused_rather_than_lowered_as_a_paint_override() {
    // A TEXT node's fill lowers into its *style*, not into a `PaintEntry`, so
    // a `VariantFill` on one would have the commit walk paint a solid
    // rectangle over the label's box instead of recolouring a glyph. A button
    // whose label turns white on press is ordinary authoring, so this is the
    // shape a real file reaches first.
    let label = |id: &str, r: f64, g: f64, b: f64| {
        let mut node = boxed(id, "label", "TEXT", 10.0, 10.0, 20.0, 20.0);
        node["characters"] = serde_json::json!("Go");
        node["style"] = serde_json::json!({
            "fontFamily": "Inter",
            "fontSize": 16.0,
            "textAutoResize": "NONE",
        });
        node["fills"] = solid(r, g, b);
        node["layoutSizingHorizontal"] = serde_json::json!("FIXED");
        node["layoutSizingVertical"] = serde_json::json!("FIXED");
        node
    };
    let (doc, diagnostics) = lower_json(set_with(
        vec![label("1:3", 0.0, 0.0, 0.0)],
        vec![label("1:7", 1.0, 1.0, 1.0)],
        vec![{
            let mut baked = label("I1:14;1:3", 0.0, 0.0, 0.0);
            baked["absoluteBoundingBox"] = serde_json::json!({
                "x": 10.0, "y": 210.0, "width": 20.0, "height": 20.0,
            });
            baked
        }],
    ));

    assert!(
        doc.variant_sets.is_empty(),
        "no variant table, rather than one that paints over the label",
    );
    let named = &diagnostics[0];
    assert_eq!(named.rule, "figma.variants.unlowerable-set");
    assert!(
        named.message.contains("text colour"),
        "named for what it is: {named:?}",
    );
}

#[test]
fn a_fill_difference_no_single_solid_expresses_is_refused_rather_than_dropped() {
    // `VariantFill` carries one colour. Two members differing in gradient, in
    // image, or in the number of stacked paints all resolve to "no single
    // solid" — and a diff that only compared the resolved colour would see
    // `None` on both sides, report no difference, and emit a variant set that
    // silently leaves the wrong paint on screen (P4).
    // **Both** sides must be non-solid for this to bite. A solid against a
    // gradient resolves to `Some(colour)` against `None`, which even a
    // colour-only comparison sees; two *different* gradients resolve to
    // `None` against `None`, which is the pair that used to report no
    // difference at all. Mutating the fix back to a colour comparison is what
    // showed the first draft of this test was passing for the wrong reason.
    let gradient = |from: [f64; 3], to: [f64; 3]| {
        serde_json::json!([{
            "type": "GRADIENT_LINEAR",
            "blendMode": "NORMAL",
            "gradientHandlePositions": [
                { "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 },
            ],
            "gradientStops": [
                { "position": 0.0, "color": { "r": from[0], "g": from[1], "b": from[2], "a": 1.0 } },
                { "position": 1.0, "color": { "r": to[0], "g": to[1], "b": to[2], "a": 1.0 } },
            ],
        }])
    };
    let mut cool = boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
    cool["fills"] = gradient([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let mut warm = boxed("1:7", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
    warm["fills"] = gradient([0.0, 1.0, 0.0], [1.0, 1.0, 0.0]);
    let mut baked = boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0);
    baked["fills"] = gradient([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    let (doc, diagnostics) = lower_json(set_with(vec![cool], vec![warm], vec![baked]));

    assert!(doc.variant_sets.is_empty());
    assert_eq!(diagnostics[0].rule, "figma.variants.unlowerable-set");
    assert!(
        diagnostics[0]
            .message
            .contains("no single solid colour expresses"),
        "{:?}",
        diagnostics[0],
    );
}

#[test]
fn a_name_containing_a_slash_still_resolves_its_own_parent() {
    // `Icon/Chevron` is an everyday Figma name. Deriving a node's parent by
    // splitting its name path on `/` resolves it to a node that does not
    // exist, which reports (0, 0) for a child that is really at (10, 10) — a
    // false refusal, or worse a false *pass* that hides a real difference
    // where the offset happens to be zero.
    let child = |id: &str, x: f64, y: f64| boxed(id, "Icon/Chevron", "FRAME", x, y, 20.0, 20.0);
    let (doc, diagnostics) = lower_json(set_with(
        vec![child("1:3", 10.0, 10.0)],
        vec![child("1:7", 40.0, 10.0)],
        vec![child("I1:14;1:3", 10.0, 210.0)],
    ));

    assert!(
        diagnostics.is_empty(),
        "the parent resolves, so the base matches and nothing is refused: {diagnostics:?}",
    );
    let (icon, icon_node) = node(&doc, "Icon/Chevron");
    assert_eq!(icon_node.box2d.x, 10.0, "the walk's own value");
    assert_eq!(
        doc.variant_sets[0].members[1].overrides,
        vec![dashc_wasm::VariantOverride {
            node: icon,
            value: VariantValue::X(40.0),
        }],
        "and the override is computed against it, not against a phantom root",
    );
}

#[test]
fn an_instance_inside_auto_layout_still_lowers_its_variant_table() {
    // The instance's own extent is its parent's to decide — a FILL axis
    // lowers as 0 — while the member root sits on the canvas and authors 100.
    // Those two numbers are never equal and never should be, so verifying a
    // prop no member overrides is what would refuse every instance in a flex
    // row. Only the props an override will actually carry are checked.
    let mut instanced = set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        vec![boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)],
    );
    let top = instanced["document"]["children"][0]["children"]
        .as_array_mut()
        .expect("the canvas holds an array");
    let mut instance = top.remove(1);
    instance["layoutSizingHorizontal"] = serde_json::json!("FILL");
    // The instance's child keeps its authored box: the row lays out the
    // instance, not the instance's own children.
    let mut row = boxed("1:20", "row", "FRAME", 0.0, 200.0, 100.0, 50.0);
    row["layoutMode"] = serde_json::json!("HORIZONTAL");
    row["children"] = serde_json::json!([instance]);
    top.push(row);

    let (doc, diagnostics) = lower_json(instanced);
    assert!(
        diagnostics.is_empty(),
        "a Fill instance still switches: {diagnostics:?}",
    );
    assert_eq!(doc.variant_sets.len(), 1);
    assert_eq!(doc.variant_sets[0].members[1].overrides.len(), 1);
}

#[test]
fn an_instance_override_the_variant_table_cannot_express_is_refused_by_name() {
    // The other side of the same check. Here the members *do* differ in the
    // prop, so an override is due — and the instance carries its own value for
    // it, so overrides computed against the member's base would animate to a
    // picture the document does not hold.
    let (doc, diagnostics) = lower_json(set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        // The baked child sits at x = 25, not the member's 10.
        vec![boxed("I1:14;1:3", "bar", "FRAME", 25.0, 210.0, 20.0, 20.0)],
    ));

    assert!(doc.variant_sets.is_empty());
    assert_eq!(diagnostics[0].rule, "figma.variants.unlowerable-set");
    assert!(
        diagnostics[0].message.contains("instance-level override"),
        "{:?}",
        diagnostics[0],
    );
}

#[test]
fn an_instance_missing_a_member_child_is_refused_by_name() {
    let (doc, diagnostics) = lower_json(set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        vec![],
    ));

    assert!(doc.variant_sets.is_empty());
    assert!(
        diagnostics[0]
            .message
            .contains("no \"bar\" to match member"),
        "{:?}",
        diagnostics[0],
    );
}

#[test]
fn two_siblings_sharing_a_name_make_the_set_unlowerable() {
    // Names are the join key across members and instances, so a duplicate has
    // no unambiguous target — binding the override to whichever came first
    // would be a coin toss between two real nodes.
    let (doc, diagnostics) = lower_json(set_with(
        vec![
            boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0),
            boxed("1:4", "bar", "FRAME", 10.0, 30.0, 20.0, 20.0),
        ],
        vec![
            boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0),
            boxed("1:8", "bar", "FRAME", 40.0, 30.0, 20.0, 20.0),
        ],
        vec![
            boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0),
            boxed("I1:14;1:4", "bar", "FRAME", 10.0, 230.0, 20.0, 20.0),
        ],
    ));

    assert!(doc.variant_sets.is_empty());
    assert_eq!(diagnostics[0].rule, "figma.variants.unlowerable-set");
    assert!(
        diagnostics[0].message.contains("share the name path"),
        "{:?}",
        diagnostics[0],
    );
}

#[test]
fn a_change_to_naming_no_member_of_the_set_is_named() {
    // A `destinationId` the export closure trimmed, or one naming a component
    // in another set. The switch reads as valid and then lands nowhere, so
    // without this it is dropped in silence (P4).
    let mut doc_json = set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        vec![boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)],
    );
    doc_json["document"]["children"][0]["children"][1]["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "9:99",
            "navigation": "CHANGE_TO",
            "transition": {
                "type": "SMART_ANIMATE",
                "easing": { "type": "EASE_OUT" },
                "duration": 0.3,
            },
        }],
    }]);

    // An omission, not a degrade (issue #976). The switch lowers nowhere, so
    // nothing about the interaction reaches the document — which is the
    // description of `unsupported-interaction`, and the reason `Strict`
    // withholds the bytes rather than shipping a button whose click does
    // nothing. It carried `unsupported-motion` at a hard-coded warning until
    // this, so a file whose export closure trimmed a `destinationId` compiled
    // clean.
    let (_, diagnostics) = lower_json(doc_json.clone());
    let named = diagnostics
        .iter()
        .find(|d| d.message.contains("9:99"))
        .unwrap_or_else(|| panic!("the dangling destination is named: {diagnostics:?}"));
    assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
    assert_eq!(named.severity, Severity::Error);

    // And it follows the policy rather than being pinned to either end of it:
    // under `Partial` the same omission is a warning and the bytes go out.
    let (_, relaxed) = lower_json_with_policy(doc_json, EmitPolicy::Partial);
    let named = relaxed
        .iter()
        .find(|d| d.message.contains("9:99"))
        .unwrap_or_else(|| panic!("the dangling destination is named: {relaxed:?}"));
    assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
    assert_eq!(named.severity, Severity::Warning);
}

#[test]
fn a_reaction_on_a_master_nothing_instantiates_is_not_a_finding() {
    // `Walk::visit` fires no diagnostic inside a definition, because nothing
    // in it paints. A refused reaction on a master no instance shows costs the
    // picture nothing either — and at error severity under Strict it would
    // withhold the whole document over a layer that never ships.
    //
    // `elsewhere` is an INSTANCE of a component outside this set, not a plain
    // frame: the rule is per-master, so an unrelated instance anywhere in the
    // file must not turn this set's reactions into findings. That is the state
    // every real Figma file is in, and a file-global guard passed this test
    // only while nothing else in the tree was instantiated — the
    // uniform-fixture trap (issue #976).
    let mut elsewhere = boxed("1:30", "elsewhere", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    elsewhere["componentId"] = serde_json::json!("7:77");
    let mut doc_json = document_with(serde_json::json!([{
        "id": "1:10",
        "name": "set",
        "type": "COMPONENT_SET",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
        "children": [
            boxed("1:2", "state=rest", "COMPONENT", 0.0, 0.0, 100.0, 50.0),
            boxed("1:6", "state=active", "COMPONENT", 0.0, 0.0, 100.0, 50.0),
        ],
    },
    elsewhere]));
    doc_json["document"]["children"][0]["children"][0]["children"][0]["interactions"] = serde_json::json!([{
        "trigger": { "type": "AFTER_TIMEOUT", "timeout": 1.5 },
        "actions": [{ "type": "URL", "url": "https://example.com" }],
    }]);

    let (_, diagnostics) = lower_json(doc_json);
    assert!(
        diagnostics.is_empty(),
        "a definition nothing instantiates names nothing: {diagnostics:?}",
    );
}

#[test]
fn a_contended_destination_is_not_named_where_the_set_animates_nothing() {
    // A set whose members differ on no rect channel writes no transition at
    // all — every member's `transition` is `None`, because a transition with
    // no track animates nothing. So a contended destination has lost nothing
    // beyond what the no-rect-channel finding already says, and naming both
    // would have one set report that a transition lowers and that none does
    // (issue #976).
    //
    // The members are identical here, so nothing differs anywhere and the
    // track list is empty for the strongest reason available.
    let switch_to_done = |duration: f64| {
        serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:20",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": duration,
                },
            }],
        }])
    };
    let member = |id: &str, name: &str, to: Option<f64>| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        if let Some(duration) = to {
            node["interactions"] = switch_to_done(duration);
        }
        node
    };
    // A definition paints nothing, so the set needs a painting sibling or the
    // document is `figma.no-content`. A plain FRAME, not an INSTANCE, so
    // nothing instantiates this set and its members' reactions stay unnamed.
    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", Some(0.3)),
                member("1:6", "state=active", Some(0.6)),
                member("1:20", "state=done", None),
            ],
        },
        boxed("1:30", "elsewhere", "FRAME", 0.0, 200.0, 100.0, 50.0),
    ])));

    let [only] = &diagnostics[..] else {
        panic!("a set that animates nothing names that, and nothing else: {diagnostics:?}");
    };
    assert!(
        only.message.contains("differ on no rect channel"),
        "the one finding is that there is nothing to animate: {only:?}",
    );
}

#[test]
fn two_members_declaring_a_different_transition_to_one_destination_are_named() {
    // The schema keys a transition on its destination
    // (`docs/decisions/motion-is-document-data-keyed-on-the-destination.md`),
    // so where two members declare a `CHANGE_TO` to the same member with
    // different tweens, only one of them lowers. Which one is not a fact
    // about the set: `emit` applies an instance's own echoed reaction over
    // the set's table afterwards, so two instances of one set can ship
    // different transitions to the same destination. Naming the contention is
    // what keeps that from being silent (P4, issue #976).
    //
    // Three members, because the realistic shape is two states that both
    // animate into a third. The child `bar` sits at a different x in each, so
    // the switch has a rect channel to animate and the set earns no other
    // motion finding.
    let switch_to_done = |duration: f64| {
        serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:20",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": duration,
                },
            }],
        }])
    };
    let member = |id: &str, name: &str, x: f64, to: Option<f64>| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([boxed(
            &format!("{id}-bar"),
            "bar",
            "FRAME",
            x,
            10.0,
            20.0,
            20.0
        )]);
        if let Some(duration) = to {
            node["interactions"] = switch_to_done(duration);
        }
        node
    };
    // Two instances, one on each declaring member. Figma echoes a member's
    // reaction verbatim onto every instance showing it, so an instance
    // carrying none is a shape no real file has — the uniform-fixture trap,
    // and the one that would hide `emit`'s override entirely.
    let instance = |id: &str, component: &str, y: f64, duration: f64| {
        let mut node = boxed(id, "card", "INSTANCE", 0.0, y, 100.0, 50.0);
        node["componentId"] = serde_json::json!(component);
        node["interactions"] = switch_to_done(duration);
        node["children"] = serde_json::json!([boxed(
            &format!("I{id};bar"),
            "bar",
            "FRAME",
            if component == "1:2" { 10.0 } else { 30.0 },
            y + 10.0,
            20.0,
            20.0
        )]);
        node
    };
    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", 10.0, Some(0.3)),
                member("1:6", "state=active", 30.0, Some(0.6)),
                member("1:20", "state=done", 50.0, None),
            ],
        },
        instance("1:14", "1:2", 200.0, 0.3),
        instance("1:15", "1:6", 400.0, 0.6),
    ])));

    let [named] = &diagnostics[..] else {
        panic!("the contention is the set's only finding: {diagnostics:?}");
    };
    assert_eq!(named.rule, "figma.prototype.unsupported-motion");
    assert_eq!(
        named.severity,
        Severity::Warning,
        "one of the two transitions still lowers, so this degrades rather than omits",
    );
    assert!(
        named.message.contains("state=done"),
        "the diagnostic names the destination whose transition was contended: {named:?}",
    );
    // The scope noun, asserted where it is meant rather than as a side effect.
    // Both sentences now come from one writer picking between
    // `Contention::AcrossTheSet` and `WithinAnInstance`, so the arm is no
    // longer a literal visible in a diff of this line.
    //
    // Swapping it does already fail
    // `a_contention_echoed_onto_two_instances_is_reported_once` — measured —
    // because that fixture's members collide at set level too and its filter
    // on "of this instance" then admits one finding too many. What it reports
    // is a count in a test about echoing, which sends the diagnosis at the
    // collapse. This says which noun was wrong.
    assert!(
        named
            .message
            .contains("more than one layer of this component set declares"),
        "and says the contention is the set's, not one instance's: {named:?}",
    );

    // And the diagnostic is right not to say which one survives: each
    // instance ships the transition its own echoed reaction declares, so the
    // two disagree on the same destination of the same set.
    let to_done = |set: usize| {
        doc.variant_sets[set].members[2]
            .transition
            .as_ref()
            .expect("the destination carries a transition")
            .tracks[0]
            .spec
            .clone()
    };
    assert_eq!(
        (to_done(0), to_done(1)),
        (
            TransitionSpec::Tween {
                duration: 0.3,
                easing: dashc_wasm::Easing::EaseOut,
            },
            TransitionSpec::Tween {
                duration: 0.6,
                easing: dashc_wasm::Easing::EaseOut,
            },
        ),
        "an instance's own reaction overrides the set's table, so no member \
         order decides what ships",
    );
}

// ---------------------------------------------------------------------------
// The PR #1010 review inflow: issues #1016, #1017, #1018 and #1019. Each is a
// population no capture carries, and each is built from the field shapes the
// captures pin — `corpus/figma-fixtures/xfile-consumer.json` for the
// cross-file instance below, `prototype-refused.json` for the refused curves.
// ---------------------------------------------------------------------------

#[test]
fn a_change_to_on_an_instance_of_a_set_the_file_does_not_carry_is_a_warning() {
    // Issue #1016. The instance shape is `xfile-consumer.json`'s: a
    // `componentId` naming no node in the file, which is what every instance
    // of a published-library component set looks like. That capture carries no
    // reaction, so this adds one.
    //
    // A warning in **both** policies, which is where this differs from the
    // dangling destination below. The argument is the neighbouring severity:
    // `figma.variants.unlowerable-set` is a warning in both policies for a set
    // the file *carries* and cannot express, because refusing would withhold a
    // document that renders correctly — so a set the export never included,
    // which loses the same variant table and paints the same baked subtree,
    // cannot earn the harsher answer. `figma-component-lowering.md`
    // ("Severity") carries it in full. Between PR #1010 and this, prototyping
    // on a library instance refused the file under `Strict`.
    let mut instance = boxed("1:6", "xfile-chip", "INSTANCE", 0.0, 0.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:4");
    instance["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:5",
            "navigation": "CHANGE_TO",
            "transition": {
                "type": "SMART_ANIMATE",
                "easing": { "type": "EASE_OUT" },
                "duration": 0.3,
            },
        }],
    }]);
    let doc_json = document_with(serde_json::json!([instance]));

    for policy in [EmitPolicy::Strict, EmitPolicy::Partial] {
        let (_, diagnostics) = lower_json_with_policy(doc_json.clone(), policy);
        let named = diagnostics
            .iter()
            .find(|d| d.message.contains("1:5"))
            .unwrap_or_else(|| panic!("{policy:?}: the switch is named: {diagnostics:?}"));
        assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
        assert_eq!(
            named.severity,
            Severity::Warning,
            "{policy:?}: a set the file never carried is a degrade, not an omission \
             that withholds the bytes: {named:?}",
        );
    }
}

#[test]
fn a_refused_curve_on_a_switch_that_lands_nowhere_is_never_called_a_degrade() {
    // Issue #1017. `read_action` classified a `CHANGE_TO` from the presence of
    // its `destinationId`, and the destination is not resolved until
    // `variants::apply` has walked the file — so one action produced two
    // findings that contradicted each other: a warning saying "the switch
    // lands in one frame" beside the omission saying it lowers nowhere.
    //
    // The switch here lands nowhere and its DISSOLVE has no `dashcue`
    // spelling, which is the pair that collided.
    let mut doc_json = set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        vec![boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)],
    );
    doc_json["document"]["children"][0]["children"][1]["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "9:99",
            "navigation": "CHANGE_TO",
            "transition": {
                "type": "DISSOLVE",
                "easing": { "type": "EASE_OUT" },
                "duration": 0.3,
            },
        }],
    }]);

    // Both policies, because "carries the omission's rule and its severity" is
    // a claim about both ends of the policy and a curve pinned to either one
    // would satisfy a Strict-only assertion.
    for (policy, expected) in [
        (EmitPolicy::Strict, Severity::Error),
        (EmitPolicy::Partial, Severity::Warning),
    ] {
        let (_, diagnostics) = lower_json_with_policy(doc_json.clone(), policy);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.message.contains("lands in one frame")),
            "{policy:?}: no finding may claim a state change the document does not carry: \
             {diagnostics:?}",
        );
        // Both halves survive one pass (debt #149): the destination and the
        // curve are two independent gaps, and both are part of one omission.
        let curve = diagnostics
            .iter()
            .find(|d| d.message.contains("DISSOLVE"))
            .unwrap_or_else(|| {
                panic!("{policy:?}: the refused curve is still named: {diagnostics:?}")
            });
        assert_eq!(curve.rule, "figma.prototype.unsupported-interaction");
        assert_eq!(
            curve.severity, expected,
            "{policy:?}: the curve carries the omission's severity, not a degrade's fixed warning",
        );
        assert!(
            diagnostics.iter().any(|d| d.message.contains("9:99")),
            "{policy:?}: the dangling destination is named too: {diagnostics:?}",
        );
    }
}

#[test]
fn an_instance_inside_a_master_nothing_instantiates_shows_nothing() {
    // Issue #1018, and it needs two levels: an `INSTANCE` of the set, nested
    // inside a `COMPONENT` master that is itself never instantiated. A test
    // with a single definition proves nothing, because `is_definition` already
    // matches that node.
    //
    // `shown` collected `component_id` from every `INSTANCE` `paths` returned,
    // and `paths` includes definitions and everything under them — so the
    // nested instance made the set read as instantiated, and its other
    // member's refused reaction became a finding. The walk skips that whole
    // subtree: measured on this fixture, the document lowers **one** node, the
    // painting frame, and the three errors this used to raise were all about
    // layers that never reach it.
    let member = |id: &str, name: &str, refused: bool| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([boxed(
            &format!("{id}c"),
            "bar",
            "FRAME",
            10.0,
            10.0,
            20.0,
            20.0
        )]);
        if refused {
            node["interactions"] = serde_json::json!([{
                "trigger": { "type": "ON_HOVER" },
                "actions": [{ "type": "NODE", "destinationId": "1:2", "navigation": "CHANGE_TO" }],
            }]);
        }
        node
    };
    let mut nested = boxed("2:2", "chip", "INSTANCE", 0.0, 400.0, 100.0, 50.0);
    nested["componentId"] = serde_json::json!("1:2");
    nested["children"] =
        serde_json::json!([boxed("I2:2;1:2c", "bar", "FRAME", 10.0, 410.0, 20.0, 20.0)]);
    // The nested instance's own reaction is refused too — the neighbouring
    // half of the same gap. `is_definition` matches the master and not its
    // descendants, so this was named for the same wrong reason.
    nested["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_KEY_DOWN" },
        "actions": [{ "type": "URL", "url": "https://example.com" }],
    }]);
    let mut master = boxed("2:1", "master", "COMPONENT", 0.0, 400.0, 100.0, 50.0);
    master["children"] = serde_json::json!([nested]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", false), member("1:6", "state=active", true)],
        },
        master,
        // A definition paints nothing, so the document needs a painting
        // sibling or `figma.no-content` fires ahead of everything this pins.
        boxed("3:1", "canvas-frame", "FRAME", 0.0, 600.0, 100.0, 50.0),
    ])));

    assert_eq!(
        doc.nodes.len(),
        1,
        "only the painting frame lowers; both definitions and the whole subtree \
         under the master resolve without painting",
    );
    assert!(
        diagnostics.is_empty(),
        "nothing under a master no instance shows may be named, at either level: \
         {diagnostics:?}",
    );
}

#[test]
fn members_differing_only_in_which_way_their_matrix_faces_are_refused() {
    // Issue #1019. `relative_transform` reaches `Props` through `Node::turn`
    // alone, so five of its six components were classified as an overridable
    // input while being read by nothing.
    //
    // Only one of the five is actually lost, and which one is measured rather
    // than argued. A **scale** moves `absoluteBoundingBox` with it — that is
    // what Figma sends — so it already lowers as `Width`/`Height` overrides
    // and comparing it here would refuse a set that renders. The
    // **handedness** is carried by nothing at all: `matrix_turn` reads a
    // negative determinant as `0.0` on purpose, and a flip leaves an
    // axis-aligned box unchanged, so a mirrored member lowered as identical to
    // an upright one.
    let with_matrix = |id: &str, m: serde_json::Value, w: f64, h: f64| {
        let mut node = boxed(id, "bar", "FRAME", 10.0, 10.0, w, h);
        node["relativeTransform"] = m;
        node
    };
    let upright = || serde_json::json!([[1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]);

    // A mirror: the determinant is negative, and nothing else differs.
    let (_, diagnostics) = lower_json(set_with(
        vec![with_matrix("1:3", upright(), 20.0, 20.0)],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[-1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]),
            20.0,
            20.0,
        )],
        vec![with_matrix("I1:14;1:3", upright(), 20.0, 20.0)],
    ));
    let named = diagnostics
        .iter()
        .find(|d| d.message.contains("mirroring"))
        .unwrap_or_else(|| panic!("a mirror difference is named: {diagnostics:?}"));
    assert_eq!(named.rule, "figma.variants.unlowerable-set");
    assert_eq!(
        named.severity,
        Severity::Warning,
        "an unlowerable set is a degrade in both policies: the instance's baked \
         subtree still paints",
    );

    // Two mirrors about **different axes** are both mirrors, so a single
    // handedness bit calls them equal — and the half-turn between them (a flip
    // about x composed with a flip about y) ships in silence. A mirror also
    // makes `matrix_turn` return `0.0`, so a mirrored member's angle is not
    // carried either: where either matrix mirrors, the whole linear part is
    // uncarried and must match exactly.
    let (_, diagnostics) = lower_json(set_with(
        vec![with_matrix(
            "1:3",
            serde_json::json!([[-1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]),
            20.0,
            20.0,
        )],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[1.0, 0.0, 10.0], [0.0, -1.0, 10.0]]),
            20.0,
            20.0,
        )],
        vec![with_matrix("I1:14;1:3", upright(), 20.0, 20.0)],
    ));
    assert!(
        diagnostics.iter().any(|d| d.message.contains("mirroring")),
        "a flip about x and a flip about y differ by a half-turn: {diagnostics:?}",
    );

    // A **half-turn** is not a mirror, and the off-diagonal cannot tell them
    // apart: `[[-1, 0], [0, 1]]` and `[[-1, 0], [0, -1]]` both have zero
    // off-diagonals and only the second is a rotation. That confusion is what
    // issue #878 was about, so the boundary is asserted from the other side
    // too: a member turned 180 degrees is not refused.
    //
    // What that guards is the naive fix — comparing the matrices themselves,
    // or their off-diagonals, would refuse this pair. That a rotation
    // difference then lowers as a `Rotation` override is
    // `a_rotation_difference_lowers_as_an_override_and_is_named_when_a_switch_animates`'s
    // claim, and it computes the rotated bounding box that needs — not
    // repeated here, where the box would be measuring the fixture.
    let (_, diagnostics) = lower_json(set_with(
        vec![with_matrix("1:3", upright(), 20.0, 20.0)],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[-1.0, 0.0, 10.0], [0.0, -1.0, 10.0]]),
            20.0,
            20.0,
        )],
        vec![with_matrix("I1:14;1:3", upright(), 20.0, 20.0)],
    ));
    assert!(
        !diagnostics.iter().any(|d| d.message.contains("mirroring")),
        "a half-turn has a positive determinant and is a rotation: {diagnostics:?}",
    );

    // Two members mirrored the **same** way but scaled differently still
    // lower: the scale bakes into the bounding box for a mirrored member
    // exactly as it does for an upright one, so it is carried, and comparing
    // the raw linear part would refuse a set that renders. What the comparison
    // keeps is the orientation, with the magnitude divided out.
    // The **members** mirror; the instance's baked copy does not, as it does
    // not in the two cases above. A member lives inside a `COMPONENT`, which
    // the walk skips whole, so its matrix reaches only this comparison. A baked
    // child paints, so since debt #1047 a mirrored one is refused by name and
    // the instance would then lower no `bar` for the table to point at — which
    // would move what this case measures from the member comparison to that
    // refusal. Keeping the baked copy upright is what holds it on the
    // comparison.
    let (doc, diagnostics) = lower_json(set_with(
        vec![with_matrix(
            "1:3",
            serde_json::json!([[-1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]),
            20.0,
            20.0,
        )],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[-2.0, 0.0, 10.0], [0.0, 2.0, 10.0]]),
            40.0,
            40.0,
        )],
        vec![with_matrix("I1:14;1:3", upright(), 20.0, 20.0)],
    ));
    assert!(
        !diagnostics.iter().any(|d| d.message.contains("mirroring")),
        "both mirror the same way; only the scale differs, and the box carries it: \
         {diagnostics:?}",
    );
    assert_eq!(
        set_of(&doc, 0).members[1]
            .overrides
            .iter()
            .map(|o| o.value)
            .collect::<Vec<_>>(),
        vec![VariantValue::Width(40.0), VariantValue::Height(40.0)],
        "and it lowers as the overrides that carry it",
    );

    // And the scale a bounding box carries still lowers, rather than being
    // refused by a comparison that cannot tell the two apart. This is the
    // regression the narrower rule exists to avoid.
    let (doc, diagnostics) = lower_json(set_with(
        vec![with_matrix("1:3", upright(), 20.0, 20.0)],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[2.0, 0.0, 10.0], [0.0, 2.0, 10.0]]),
            40.0,
            40.0,
        )],
        vec![with_matrix("I1:14;1:3", upright(), 20.0, 20.0)],
    ));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        set_of(&doc, 0).members[1]
            .overrides
            .iter()
            .map(|o| o.value)
            .collect::<Vec<_>>(),
        vec![VariantValue::Width(40.0), VariantValue::Height(40.0)],
        "the scale is carried by the box the walk already lowers",
    );
}

// --- The review round on PR #1039: three regressions the first fix introduced.

#[test]
fn an_instance_of_a_set_the_file_carries_but_cannot_lower_is_not_reported_as_absent() {
    // Resolving "does this file carry a set for this node" off the *plans*
    // conflated two different answers: no set here, and a set no plan could be
    // built for. A set whose members differ in corner radius is unlowerable,
    // so no plan is pushed — and an instance of it then reported that the file
    // carried no component set for it, on the line below the finding that
    // named that very set, and took the published-library warning with it.
    //
    // Resolution is a question about the file, so it is answered from the node
    // tree. The switch's loss is `UNLOWERABLE_SET`'s to name in full: reporting
    // it twice would have one set say both that a switch lowers and that none
    // does, which is what `a_contended_destination_is_not_named_where_the_set_animates_nothing`
    // guards on the neighbouring path.
    let member = |id: &str, name: &str, radius: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["cornerRadius"] = serde_json::json!(radius);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        // A real member of the set, not a dangling id.
        "actions": [{ "type": "NODE", "destinationId": "1:6", "navigation": "CHANGE_TO" }],
    }]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 4.0), member("1:6", "state=active", 12.0)],
        },
        instance,
    ])));

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("carries no component set")),
        "the file plainly carries the set; only the plan failed: {diagnostics:?}",
    );
    assert_eq!(
        diagnostics.iter().map(|d| d.rule).collect::<Vec<_>>(),
        vec!["figma.variants.unlowerable-set"],
        "the set's own finding names the whole loss: {diagnostics:?}",
    );
}

#[test]
fn a_reaction_on_a_child_of_a_member_no_instance_echoes_is_still_named() {
    // Skipping definition subtrees for issue #1018 removed the only pass that
    // reached a member's *children*, and `Plan::diagnostics` compensated at the
    // member root alone — so a refused reaction one layer inside a member no
    // instance echoes was named nowhere. It paints the moment a switch to that
    // member fires, which is the whole reason non-echoed members are named at
    // all.
    let member = |id: &str, name: &str, refused: bool| {
        let mut child = boxed(&format!("{id}c"), "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
        if refused {
            child["interactions"] = serde_json::json!([{
                "trigger": { "type": "AFTER_TIMEOUT", "timeout": 1.5 },
                "actions": [{ "type": "URL", "url": "https://example.com" }],
            }]);
        }
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] =
        serde_json::json!([boxed("I1:14;1:2c", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", false), member("1:6", "state=active", true)],
        },
        instance,
    ])));

    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("AFTER_TIMEOUT")),
        "a refusal inside a non-echoed member is named at that node: {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("URL")),
        "and every finding survives one pass: {diagnostics:?}",
    );
}

#[test]
fn a_switch_inside_a_member_is_judged_against_the_set_that_owns_it() {
    // Naming a non-echoed member's subtree has to resolve each node the way
    // the rest of the file does. Judging the whole subtree against the
    // *enclosing* set reported a nested instance of another set as naming a
    // destination that is not a member — an Error under Strict for a switch
    // that is perfectly valid. And descending into a nested master named
    // reactions inside a definition nothing instantiates, which is the case
    // issue #1018 removed everywhere else.
    // Both members carry the same two children by name, or the set is a
    // topology change, no plan is built, and the subtree this is about is
    // never reached at all.
    let chip = |id: &str, y: f64, reaction: bool| {
        let mut node = boxed(id, "chip", "INSTANCE", 10.0, y, 20.0, 20.0);
        node["componentId"] = serde_json::json!("2:2");
        if reaction {
            node["interactions"] = serde_json::json!([{
                "trigger": { "type": "ON_CLICK" },
                // A real member of the *other* set, 2:10.
                "actions": [{ "type": "NODE", "destinationId": "2:6", "navigation": "CHANGE_TO" }],
            }]);
        }
        node
    };
    let buried = |id: &str, inner_id: &str, y: f64, reaction: bool| {
        let mut inner = boxed(inner_id, "inner", "FRAME", 10.0, y, 20.0, 20.0);
        if reaction {
            inner["interactions"] = serde_json::json!([{
                "trigger": { "type": "ON_KEY_DOWN" },
                "actions": [{ "type": "URL", "url": "https://example.com" }],
            }]);
        }
        let mut master = boxed(id, "buried", "COMPONENT", 10.0, y, 20.0, 20.0);
        master["children"] = serde_json::json!([inner]);
        master
    };

    let mut echoed = boxed("1:2", "state=rest", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    echoed["children"] =
        serde_json::json!([chip("1:20", 10.0, false), buried("3:1", "3:2", 40.0, false)]);
    let mut not_echoed = boxed("1:6", "state=active", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    not_echoed["children"] =
        serde_json::json!([chip("1:60", 10.0, true), buried("3:3", "3:4", 40.0, true)]);

    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([
        chip("I1:14;1:20", 210.0, false),
        buried("I1:14;3:1", "I1:14;3:2", 240.0, false),
    ]);

    let other_set = serde_json::json!({
        "id": "2:10",
        "name": "chips",
        "type": "COMPONENT_SET",
        "absoluteBoundingBox": { "x": 0.0, "y": 400.0, "width": 100.0, "height": 50.0 },
        "children": [
            boxed("2:2", "chip=off", "COMPONENT", 0.0, 400.0, 100.0, 50.0),
            boxed("2:6", "chip=on", "COMPONENT", 0.0, 400.0, 100.0, 50.0),
        ],
    });

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [echoed, not_echoed],
        },
        other_set,
        instance,
    ])));

    assert!(
        !diagnostics.iter().any(|d| d.message.contains("2:6")),
        "the nested instance switches within its own set, which the file carries: {diagnostics:?}",
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("ON_KEY_DOWN")),
        "a master buried inside a member is still a definition nothing instantiates: \
         {diagnostics:?}",
    );
}

#[test]
fn a_switch_into_a_set_that_lowers_no_table_is_not_called_a_degrade() {
    // The set is carried and the destination is one of its members, so the
    // switch reads as expressible — but the members differ in corner radius,
    // no plan is built, and no variant table is emitted. Calling the refused
    // curve a degrade there claims a state change nothing carries, which is
    // the same defect reached through the other path. `UNLOWERABLE_SET` has
    // already named the whole loss.
    let member = |id: &str, name: &str, radius: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["cornerRadius"] = serde_json::json!(radius);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:6",
            "navigation": "CHANGE_TO",
            "transition": { "type": "DISSOLVE", "easing": { "type": "EASE_OUT" }, "duration": 0.3 },
        }],
    }]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 4.0), member("1:6", "state=active", 12.0)],
        },
        instance,
    ])));

    assert!(
        doc.variant_sets.is_empty(),
        "the set lowers no table at all, so no switch ships",
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("lands in one frame")),
        "nothing may claim a switch the document does not carry: {diagnostics:?}",
    );
    // The curve is still named, at the same degrade severity the set's own
    // finding carries. Dropping it would be a silent drop (P4) and would
    // surface for the first time on the compile after the corner radius is
    // fixed, which is the second-compile failure debt #149 exists to prevent.
    let curve = diagnostics
        .iter()
        .find(|d| d.message.contains("DISSOLVE"))
        .unwrap_or_else(|| panic!("the refused curve is named: {diagnostics:?}"));
    assert_eq!(curve.rule, "figma.prototype.unsupported-motion");
    assert_eq!(
        curve.severity,
        Severity::Warning,
        "an unlowerable set stays a degrade end to end: {curve:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "figma.variants.unlowerable-set"),
        "and the set names its own loss: {diagnostics:?}",
    );
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "none of it withholds the document: {diagnostics:?}",
    );
}

#[test]
fn a_change_to_on_an_instance_of_a_standalone_local_component_is_not_the_library_case() {
    // The library exemption is for a `componentId` naming a component the file
    // does **not** contain. A standalone local `COMPONENT` is present in full:
    // there is simply no set, so the switch can never resolve and the file is
    // broken rather than under-exported. Keying the exemption on "no set
    // found" swallowed this and let it compile clean under Strict.
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{ "type": "NODE", "destinationId": "1:6", "navigation": "CHANGE_TO" }],
    }]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        boxed("1:2", "card", "COMPONENT", 0.0, 0.0, 100.0, 50.0),
        instance,
    ])));

    let named = diagnostics
        .iter()
        .find(|d| d.message.contains("1:6"))
        .unwrap_or_else(|| panic!("the switch is named: {diagnostics:?}"));
    assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
    assert_eq!(
        named.severity,
        Severity::Error,
        "the component is right there; no library is missing: {named:?}",
    );
}

#[test]
fn a_reaction_echoed_onto_a_baked_child_resolves_through_its_instance() {
    // The everyday authoring shape: an inner layer drives the enclosing
    // instance's variant. REST reports a component's interaction on the
    // instance verbatim, so the same `CHANGE_TO` arrives twice — once on the
    // master's `knob`, which resolves through the definition above it, and
    // once on the instance's baked `knob`, which has no `componentId` and no
    // definition above it at all.
    //
    // Resolving only through definitions classified the baked copy as
    // belonging to no component set, which made an ordinary file an error
    // under Strict while the identical reaction on the master stayed silent.
    let knob = |id: &str, y: f64| {
        let mut node = boxed(id, "knob", "FRAME", 10.0, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{ "type": "NODE", "destinationId": "1:6", "navigation": "CHANGE_TO" }],
        }]);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([knob("I1:14;1:3", 210.0)]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0)),
                member("1:6", "state=active", knob("1:7", 10.0)),
            ],
        },
        instance,
    ])));

    assert!(
        diagnostics.is_empty(),
        "the switch names a member of the set the instance shows, from both copies \
         of the layer that declares it: {diagnostics:?}",
    );
}

#[test]
fn a_member_reaction_is_not_named_where_the_set_lowers_no_table() {
    // A set that lowers no variant table can switch to nothing, so a member no
    // instance echoes never reaches the screen and its refused reaction costs
    // the picture exactly what a master nothing instantiates costs it. Naming
    // it made an ordinary hover-driven set — the most common interactive
    // component shape there is — refuse the file under Strict.
    let member = |id: &str, name: &str, radius: f64, refused: bool| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["cornerRadius"] = serde_json::json!(radius);
        if refused {
            node["interactions"] = serde_json::json!([{
                "trigger": { "type": "ON_HOVER" },
                "actions": [{ "type": "NODE", "destinationId": "1:2", "navigation": "CHANGE_TO" }],
            }]);
        }
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", 4.0, false),
                member("1:6", "state=active", 12.0, true),
            ],
        },
        instance,
    ])));

    assert!(
        !diagnostics.iter().any(|d| d.message.contains("ON_HOVER")),
        "no switch can reach that member, so its reaction ships nowhere: {diagnostics:?}",
    );
    assert_eq!(
        diagnostics.iter().map(|d| d.rule).collect::<Vec<_>>(),
        vec!["figma.variants.unlowerable-set"],
        "only the set's own loss is named: {diagnostics:?}",
    );
}

#[test]
fn an_instance_inside_a_member_still_shows_the_set_it_names() {
    // What is shown and what a switch can reach define each other. An instance
    // sitting inside a *member* reaches the screen exactly when a switch to
    // that member does, and it then shows its own set — whose members become
    // reachable in turn.
    //
    // Answering the two in sequence stops one level short: excluding every
    // instance inside a definition from `shown`, while naming a non-echoed
    // member's whole subtree, left the inner set reading as instantiated by
    // nothing, and its own members' refusals named nowhere (P4).
    let chip = |id: &str, y: f64| {
        let mut node = boxed(id, "chip", "INSTANCE", 10.0, y, 20.0, 20.0);
        node["componentId"] = serde_json::json!("2:2");
        node
    };
    let outer = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let inner = |id: &str, name: &str, refused: bool| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 400.0, 20.0, 20.0);
        if refused {
            node["interactions"] = serde_json::json!([{
                "trigger": { "type": "ON_KEY_DOWN" },
                "actions": [{ "type": "URL", "url": "https://example.com" }],
            }]);
        }
        node
    };
    // The instance bakes **no** chip of its own. That is what isolates the
    // question: the only instances of the inner set are the two sitting inside
    // the outer set's members, so if neither counts as showing it, nothing
    // does. (A baked chip here would reach `shown` directly, through the
    // instance rather than through a member, and the fixture would pass
    // without ever asking the question.) The missing child costs an
    // `unlowerable-set` warning, which is not what this asserts on.
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "outer",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                outer("1:2", "state=rest", chip("1:20", 10.0)),
                outer("1:6", "state=active", chip("1:60", 10.0)),
            ],
        },
        {
            "id": "2:10",
            "name": "inner",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 400.0, "width": 20.0, "height": 20.0 },
            "children": [inner("2:2", "chip=off", false), inner("2:6", "chip=on", true)],
        },
        instance,
    ])));

    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("ON_KEY_DOWN")),
        "the inner set is instantiated by a chip one member deep, so the member no \
         chip echoes is named: {diagnostics:?}",
    );
}

#[test]
fn a_change_to_on_a_node_belonging_to_no_component_set_follows_the_policy() {
    // The third arm of the resolution, and the one with no instance in it: a
    // plain frame carrying a `CHANGE_TO`. There is no variant to switch to and
    // no library missing, so it takes the policy's severity rather than the
    // exemption's fixed warning — and it is pinned here because nothing else
    // asserts which of the two a hostless node lands in.
    let mut frame = boxed("1:2", "loose", "FRAME", 0.0, 0.0, 100.0, 50.0);
    frame["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{ "type": "NODE", "destinationId": "9:99", "navigation": "CHANGE_TO" }],
    }]);
    let doc_json = document_with(serde_json::json!([frame]));

    for (policy, expected) in [
        (EmitPolicy::Strict, Severity::Error),
        (EmitPolicy::Partial, Severity::Warning),
    ] {
        let (_, diagnostics) = lower_json_with_policy(doc_json.clone(), policy);
        let named = diagnostics
            .iter()
            .find(|d| d.message.contains("9:99"))
            .unwrap_or_else(|| panic!("{policy:?}: the switch is named: {diagnostics:?}"));
        assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
        assert_eq!(named.severity, expected, "{policy:?}: {named:?}");
    }
}

#[test]
fn a_change_to_into_a_single_member_set_is_an_omission_nothing_else_reports() {
    // The third state a set can be in. A set of one member has no alternative
    // to switch to, so rule 12 has it name nothing at all — which means no
    // other diagnostic speaks for a `CHANGE_TO` into it, and this arm is the
    // only place that reports the loss. It is pinned here because the arm is
    // reachable from any layer, not only from an instance root, and nothing
    // else asserts which severity it takes.
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{ "type": "NODE", "destinationId": "1:2", "navigation": "CHANGE_TO" }],
    }]);
    let doc_json = document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [boxed("1:2", "state=rest", "COMPONENT", 0.0, 0.0, 100.0, 50.0)],
        },
        instance,
    ]));

    for (policy, expected) in [
        (EmitPolicy::Strict, Severity::Error),
        (EmitPolicy::Partial, Severity::Warning),
    ] {
        let (_, diagnostics) = lower_json_with_policy(doc_json.clone(), policy);
        let named = diagnostics
            .iter()
            .find(|d| d.message.contains("no second member"))
            .unwrap_or_else(|| panic!("{policy:?}: the switch is named: {diagnostics:?}"));
        assert_eq!(named.rule, "figma.prototype.unsupported-interaction");
        assert_eq!(named.severity, expected, "{policy:?}: {named:?}");
    }
}

#[test]
fn a_switch_on_an_instance_whose_own_table_is_refused_is_not_called_a_degrade() {
    // A set can plan and still ship no table for a *particular* instance:
    // `emit` refuses one whose own geometry disagrees with the member it
    // shows. Judging the switch before that answer is known put "the switch
    // lands in one frame" beside "this instance lowers no variant table" —
    // the same contradiction one scope down from the set.
    let mut turned = boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0);
    turned["fills"] = solid(0.1, 0.2, 0.3);
    let mut doc_json = set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![turned],
        // The instance's own bar sits where the member it shows does not,
        // which is the instance-level override `emit` refuses.
        vec![boxed("I1:14;1:3", "bar", "FRAME", 33.0, 210.0, 20.0, 20.0)],
    );
    doc_json["document"]["children"][0]["children"][1]["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:6",
            "navigation": "CHANGE_TO",
            "transition": { "type": "DISSOLVE", "easing": { "type": "EASE_OUT" }, "duration": 0.3 },
        }],
    }]);

    let (doc, diagnostics) = lower_json(doc_json);
    assert!(
        doc.variant_sets.is_empty(),
        "the instance's own table is refused, so no switch ships: {diagnostics:?}",
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("lands in one frame")),
        "nothing may claim a switch this instance does not carry: {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("DISSOLVE")),
        "and the curve is still named: {diagnostics:?}",
    );
}

#[test]
fn a_switch_on_a_baked_child_of_a_refused_instance_is_not_called_a_degrade() {
    // The same claim as the test above, from the node that first got it
    // wrong. Whether a switch ships was tracked per *node* and defaulted to
    // true, so every layer other than the instance root whose table was
    // emitted still reported "the switch lands in one frame" — including a
    // baked child, whose switches reach no table at all (debt #1064).
    let knob = |id: &str, x: f64, y: f64| {
        let mut node = boxed(id, "bar", "FRAME", x, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "DISSOLVE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": 0.3,
                },
            }],
        }]);
        node
    };
    let (doc, diagnostics) = lower_json(set_with(
        vec![boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)],
        vec![boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)],
        // At x 33 where the member it shows authors 10 — the instance-level
        // override `emit` refuses, so this instance ships no table.
        vec![knob("I1:14;1:3", 33.0, 210.0)],
    ));

    assert!(doc.variant_sets.is_empty(), "{diagnostics:?}");
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("lands in one frame")),
        "a baked child carries no transition into any table: {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("DISSOLVE")),
        "and the curve is still named: {diagnostics:?}",
    );
}

#[test]
fn a_refused_curve_on_a_member_no_instance_echoes_is_still_a_degrade() {
    // The reverse arm of a two-variant set, and the one shape no other test
    // covered: a member no instance shows, of a set that lowers, declaring a
    // `CHANGE_TO` whose curve the vocabulary refuses.
    //
    // Its switch **does** ship — `Plan::of` folds a member's own reaction into
    // the set's default tween table and `emit` copies that table into every
    // instance's `VariantSet` — so the loss is the curve alone, which is the
    // definition of a degrade. Keying "does this switch ship" on "is this node
    // an instance" took that away and reported the switch as reaching nothing.
    let mut member = boxed("1:6", "state=active", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    member["children"] = serde_json::json!([boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)]);
    member["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:2",
            "navigation": "CHANGE_TO",
            "transition": { "type": "DISSOLVE", "easing": { "type": "EASE_OUT" }, "duration": 0.3 },
        }],
    }]);
    let mut rest = boxed("1:2", "state=rest", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    rest["children"] = serde_json::json!([boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)]);
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] =
        serde_json::json!([boxed("I1:14;1:3", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [rest, member],
        },
        instance,
    ])));

    assert_eq!(doc.variant_sets.len(), 1, "the set lowers: {diagnostics:?}");
    let curve = diagnostics
        .iter()
        .find(|d| d.message.contains("DISSOLVE"))
        .unwrap_or_else(|| panic!("the refused curve is named: {diagnostics:?}"));
    assert_eq!(curve.rule, "figma.prototype.unsupported-motion");
    assert!(
        curve.message.contains("lands in one frame"),
        "the switch ships through the set's default table, so only the curve \
         was lost: {curve:?}",
    );
}

#[test]
fn members_differing_in_whether_their_transform_has_area_are_refused_by_that_name() {
    // A collapsed matrix is neither upright nor mirrored, and `matrix_turn`
    // reports `0.0` for it too — so its orientation is uncarried for the same
    // reason a mirror's is, and the set is refused. What it must not say is
    // that the two differ in mirroring, because neither mirrors.
    let with_matrix = |id: &str, m: serde_json::Value| {
        let mut node = boxed(id, "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
        node["relativeTransform"] = m;
        node
    };
    let (_, diagnostics) = lower_json(set_with(
        vec![with_matrix(
            "1:3",
            serde_json::json!([[1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]),
        )],
        vec![with_matrix(
            "1:7",
            serde_json::json!([[0.0, 0.0, 10.0], [0.0, 0.0, 10.0]]),
        )],
        vec![with_matrix(
            "I1:14;1:3",
            serde_json::json!([[1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]),
        )],
    ));
    let named = diagnostics
        .iter()
        .find(|d| d.rule == "figma.variants.unlowerable-set")
        .unwrap_or_else(|| panic!("the collapsed matrix is named: {diagnostics:?}"));
    assert!(
        named.message.contains("any area at all"),
        "and it names the difference that is there, not a mirror: {named:?}",
    );
}

// --- Debt #1064 and #1065: which set a CHANGE_TO resolves against, and which
// host's table carries the transition it declares.

/// A two-member set whose `knob` child sits at a different x per member, so the
/// set differs on a rect channel and a transition into it has a track to
/// animate. `reaction` decorates a node with an `ON_CLICK` `CHANGE_TO` to
/// `1:6` — `state=active` — carrying a Smart Animate curve.
fn smart_animate_to_active() -> serde_json::Value {
    serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:6",
            "navigation": "CHANGE_TO",
            "transition": {
                "type": "SMART_ANIMATE",
                "easing": { "type": "EASE_OUT" },
                "duration": 0.4,
            },
        }],
    }])
}

#[test]
fn a_switch_on_a_baked_layer_carries_its_transition_into_the_instances_table() {
    // Debt #1064. `emit` applied the switches of the instance **root** alone,
    // so a `CHANGE_TO` on any deeper layer contributed no transition and its
    // tween was dropped — with no diagnostic, because the pass then reported
    // the switch as having lost nothing. The everyday shape is exactly this
    // one: an inner `knob` layer driving the enclosing instance's variant.
    let knob = |id: &str, x: f64, y: f64| {
        let mut node = boxed(id, "knob", "FRAME", x, y, 20.0, 20.0);
        node["interactions"] = smart_animate_to_active();
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([knob("I1:14;1:3", 10.0, 210.0)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0, 10.0)),
                member("1:6", "state=active", knob("1:7", 60.0, 10.0)),
            ],
        },
        instance,
    ])));

    assert!(
        diagnostics.is_empty(),
        "the switch ships and so does its curve, so nothing is lost to name: {diagnostics:?}",
    );
    let transition = set_of(&doc, 0).members[1]
        .transition
        .as_ref()
        .expect("the tween the baked layer declared reaches the destination's member");
    assert_eq!(
        transition
            .tracks
            .iter()
            .map(|track| track.spec.clone())
            .collect::<Vec<_>>(),
        vec![TransitionSpec::Tween {
            duration: 0.4,
            easing: Easing::EaseOut,
        }],
        "with the duration and easing it was authored with",
    );
}

#[test]
fn a_nested_instance_switching_its_parents_variant_resolves_through_the_parent() {
    // Debt #1065. A `CHANGE_TO` used to resolve against the set of the nearest
    // node that shows one, which is right for a layer switching its own
    // instance's variant and wrong for a nested `INSTANCE` switching its
    // parent's: the chip shows a set of its own, so resolution stopped there,
    // answered with that set, and reported a destination that is not one of its
    // members — an Error that withheld the whole document under Strict.
    //
    // The destination decides the set now, so the chip resolves past itself to
    // the host that owns `1:6`. Two levels, which is the population: a fixture
    // with a single instance cannot exercise it.
    let chip = |id: &str, x: f64, y: f64| {
        let mut node = boxed(id, "chip", "INSTANCE", x, y, 20.0, 20.0);
        node["componentId"] = serde_json::json!("2:2");
        node["children"] = serde_json::json!([]);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            // 1:6 is `state=active` of the ENCLOSING set, not a member of 2:10.
            "actions": [{ "type": "NODE", "destinationId": "1:6", "navigation": "CHANGE_TO" }],
        }]);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let tone = |id: &str, name: &str| {
        let mut node = boxed(id, name, "COMPONENT", 300.0, 0.0, 20.0, 20.0);
        node["children"] = serde_json::json!([]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([chip("I1:14;1:3", 10.0, 210.0)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "2:10",
            "name": "chipset",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 300.0, "y": 0.0, "width": 40.0, "height": 20.0 },
            "children": [tone("2:2", "tone=plain"), tone("2:6", "tone=loud")],
        },
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", chip("1:3", 10.0, 10.0)),
                member("1:6", "state=active", chip("1:7", 60.0, 10.0)),
            ],
        },
        instance,
    ])));

    assert!(
        diagnostics.is_empty(),
        "the chip switches the variant of the component it sits inside, which is an \
         ordinary authoring shape and lowers: {diagnostics:?}",
    );
    assert_eq!(
        doc.variant_sets.len(),
        2,
        "both sets still lower their own table",
    );
}

#[test]
fn a_switch_on_a_nested_instance_still_resolves_its_own_set_first() {
    // The other side of the rule, and what keeps it from being "walk up until
    // something matches": where the destination is a member of the nested
    // instance's **own** set, the nested instance is the nearest host that owns
    // it, so it is the one that switches. Only the destination decides which
    // set; the chain decides who, nearest first.
    let chip = |id: &str, x: f64, y: f64| {
        let mut node = boxed(id, "chip", "INSTANCE", x, y, 20.0, 20.0);
        node["componentId"] = serde_json::json!("2:2");
        node["children"] = serde_json::json!([]);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            // 2:6 is a member of the chip's OWN set.
            "actions": [{ "type": "NODE", "destinationId": "2:6", "navigation": "CHANGE_TO" }],
        }]);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let tone = |id: &str, name: &str| {
        let mut node = boxed(id, name, "COMPONENT", 300.0, 0.0, 20.0, 20.0);
        node["children"] = serde_json::json!([]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([chip("I1:14;1:3", 10.0, 210.0)]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "2:10",
            "name": "chipset",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 300.0, "y": 0.0, "width": 40.0, "height": 20.0 },
            "children": [tone("2:2", "tone=plain"), tone("2:6", "tone=loud")],
        },
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", chip("1:3", 10.0, 10.0)),
                member("1:6", "state=active", chip("1:7", 60.0, 10.0)),
            ],
        },
        instance,
    ])));

    assert!(
        diagnostics.is_empty(),
        "a chip switching its own variant is the case that already worked, and still \
         resolves against its own set: {diagnostics:?}",
    );
}

#[test]
fn two_layers_of_one_instance_contending_over_a_destination_are_named() {
    // Widening what reaches an instance's table (debt #1064) created the
    // collision case a set's two members already had: the document carries one
    // transition per destination, so where two baked layers declare a different
    // one to the same member only one lowers. Naming it is what keeps the
    // widening from being a silent loss (P4, issue #976).
    let knob = |id: &str, name: &str, x: f64, y: f64, duration: f64| {
        let mut node = boxed(id, name, "FRAME", x, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": duration,
                },
            }],
        }]);
        node
    };
    let member = |id: &str, name: &str, x: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([
            knob(&format!("{id}:a"), "knob", x, 10.0, 0.4),
            knob(&format!("{id}:b"), "dial", 10.0, 10.0, 0.9),
        ]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([
        knob("I1:14;1:2:a", "knob", 10.0, 210.0, 0.4),
        knob("I1:14;1:2:b", "dial", 10.0, 210.0, 0.9),
    ]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 10.0), member("1:6", "state=active", 60.0)],
        },
        instance,
    ])));

    let contended: Vec<&str> = diagnostics
        .iter()
        // "layer of this instance", not "declares a CHANGE_TO": the set-level
        // collision message matches the looser string too, so this test passed
        // with the per-instance warning deleted outright.
        .filter(|d| d.message.contains("layer of this instance declares"))
        .map(|d| d.rule)
        .collect();
    assert!(
        !contended.is_empty(),
        "the contention is named rather than riding on declaration order: {diagnostics:?}",
    );
    assert!(
        contended
            .iter()
            .all(|rule| *rule == "figma.prototype.unsupported-motion"),
        "and it is a degrade — the switch itself still ships: {diagnostics:?}",
    );
}

#[test]
fn a_baked_layers_own_reaction_overrides_the_sets_default_at_depth() {
    // The other half of debt #1064, and the one the master-side fixture above
    // cannot reach. A reaction on a layer inside the **master** resolves through
    // the member root, so it joins the set's *default* table; a reaction on the
    // instance's own baked copy of that layer resolves through the **instance**,
    // so it overrides that default for this instance alone.
    //
    // `an_instances_own_reaction_overrides_the_sets_default` pins the same rule
    // one scope up, on the instance root. Restricting the gather back to the
    // root leaves this instance shipping the master's 0.4 rather than its own
    // 0.9, with nothing named.
    let knob = |id: &str, x: f64, y: f64, duration: f64| {
        let mut node = boxed(id, "knob", "FRAME", x, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": duration,
                },
            }],
        }]);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    // The instance's own baked copy declares a different curve from the master's.
    instance["children"] = serde_json::json!([knob("I1:14;1:3", 10.0, 210.0, 0.9)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0, 10.0, 0.4)),
                member("1:6", "state=active", knob("1:7", 60.0, 10.0, 0.4)),
            ],
        },
        instance,
    ])));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let transition = set_of(&doc, 0).members[1]
        .transition
        .as_ref()
        .expect("the destination's member carries a transition");
    assert_eq!(
        transition
            .tracks
            .iter()
            .map(|track| track.spec.clone())
            .collect::<Vec<_>>(),
        vec![TransitionSpec::Tween {
            duration: 0.9,
            easing: Easing::EaseOut,
        }],
        "this instance ships its own baked layer's curve, not the set's default",
    );
}

#[test]
fn a_master_inner_layers_reaction_reaches_the_sets_default_table() {
    // The master side of debt #1064, isolated: only the layer inside the
    // **master** declares the switch, and the instance's baked copy carries no
    // reaction at all.
    //
    // That this file can exist is the point. `apply` states that REST echoing a
    // reaction from a layer *inside* a master onto the instance's copy of that
    // layer is inference from the baked subtree being the resolved content, and
    // not a measured fact — no capture pins an interaction below a member root.
    // Where the echo does not happen, the set's default table is the only thing
    // left carrying what the designer authored, and reading that table off the
    // member **roots** alone dropped it in silence: the echoed member is not
    // named either, so nothing would have reported the loss (P4).
    let knob = |id: &str, x: f64, y: f64, reacts: bool| {
        let mut node = boxed(id, "knob", "FRAME", x, y, 20.0, 20.0);
        if reacts {
            node["interactions"] = smart_animate_to_active();
        }
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([knob("I1:14;1:3", 10.0, 210.0, false)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0, 10.0, true)),
                member("1:6", "state=active", knob("1:7", 60.0, 10.0, false)),
            ],
        },
        instance,
    ])));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let transition = set_of(&doc, 0).members[1]
        .transition
        .as_ref()
        .expect("the set's default table carries what a layer inside the master declared");
    assert_eq!(
        transition
            .tracks
            .iter()
            .map(|track| track.spec.clone())
            .collect::<Vec<_>>(),
        vec![TransitionSpec::Tween {
            duration: 0.4,
            easing: Easing::EaseOut,
        }],
    );
}

// --- Debt #1056: one authored reaction is one finding.

#[test]
fn one_authored_reaction_inside_a_master_is_one_finding_however_many_instances_show_it() {
    // Debt #1056. Figma echoes a component's interaction onto every instance
    // verbatim, so REST hands `dashc` one copy per instance plus the master's
    // own, and every copy was a separate resolved finding: a design-system file
    // with fifty instances of one master reported fifty-one errors for one
    // authored mistake.
    //
    // The copies are collapsed onto the layer the reaction was authored on —
    // the `<source>` half of the synthetic `I<instance>;<source>` id — and the
    // count is said on the survivor. The two members' own `knob`s are two
    // authored layers and stay two findings; what collapses is one layer's
    // echoes.
    let knob = |id: &str, y: f64| {
        let mut node = boxed(id, "knob", "FRAME", 10.0, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{ "type": "NODE", "destinationId": "9:99", "navigation": "CHANGE_TO" }],
        }]);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let instance = |id: &str, y: f64, baked: &str| {
        let mut node = boxed(id, "card", "INSTANCE", 0.0, y, 100.0, 50.0);
        node["componentId"] = serde_json::json!("1:2");
        node["children"] = serde_json::json!([knob(baked, y + 10.0)]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0)),
                member("1:6", "state=active", knob("1:7", 10.0)),
            ],
        },
        // Both show `state=rest`, so both bake a copy of layer `1:3`.
        instance("1:14", 200.0, "I1:14;1:3"),
        instance("1:15", 300.0, "I1:15;1:3"),
    ])));

    let refusals: Vec<(&str, &str)> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-interaction")
        .map(|d| {
            let Location::Node(at) = &d.at else {
                panic!("located at a node");
            };
            (at.path.as_str(), d.message.as_str())
        })
        .collect();
    assert_eq!(
        refusals.len(),
        2,
        "two authored layers carry the mistake, not three copies of it: {diagnostics:?}",
    );
    assert_eq!(
        refusals[0].0, "/set/state=active/knob",
        "the member no instance echoes is its own authored layer",
    );
    assert!(
        !refusals[0].1.contains("further cop"),
        "and it has no echoes, so nothing is appended: {:?}",
        refusals[0],
    );
    assert_eq!(
        refusals[1].0, "/card (1:14)/knob",
        "the echoed layer is reported at the first copy the walk reached",
    );
    assert!(
        refusals[1]
            .1
            .contains("(and 1 further copy of the same reaction, not listed separately)"),
        "and it says what the collapse cost the reader: {:?}",
        refusals[1],
    );
}

#[test]
fn a_refused_construct_echoed_onto_two_instances_stays_two_findings() {
    // The line debt #1056's rule is drawn on, asserted from the other side.
    // `figma.unsupported` is **not** collapsed: it names a box and skips the
    // node's subtree, so two instances echoing one refused layer are two
    // omissions from the document and the multiplicity is the finding. A
    // prototype refusal names a behaviour and leaves the node in place, so its
    // copies produce identical bytes and collapse.
    //
    // `06-dashc-figma-lowering.md` ("Refusal" 10) is where that difference is
    // stated: "Unlike `figma.unsupported` it shall not skip the node: what has
    // no lowering is the behaviour, not the box".
    let knob = |id: &str, y: f64| {
        let mut node = boxed(id, "knob", "FRAME", 10.0, y, 20.0, 20.0);
        // A construct the walk refuses by name, on the layer itself.
        node["strokesIncludedInLayout"] = serde_json::json!(true);
        node
    };
    let member = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let instance = |id: &str, y: f64, baked: &str| {
        let mut node = boxed(id, "card", "INSTANCE", 0.0, y, 100.0, 50.0);
        node["componentId"] = serde_json::json!("1:2");
        node["children"] = serde_json::json!([knob(baked, y + 10.0)]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", knob("1:3", 10.0)),
                member("1:6", "state=active", knob("1:7", 10.0)),
            ],
        },
        instance("1:14", 200.0, "I1:14;1:3"),
        instance("1:15", 300.0, "I1:15;1:3"),
    ])));

    let omitted: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.unsupported")
        .map(|d| {
            let Location::Node(at) = &d.at else {
                panic!("located at a node");
            };
            at.path.as_str()
        })
        .collect();
    assert_eq!(
        omitted,
        vec!["/card (1:14)/knob", "/card (1:15)/knob"],
        "each instance loses its own copy of the layer, and each loss is named: {diagnostics:?}",
    );
}

#[test]
fn a_reaction_inside_a_nested_master_does_not_reach_the_enclosing_sets_table() {
    // The boundary on debt #1064's widening, and a defect the widening
    // introduced before this pinned it. A layer inside a master that sits
    // *within* a member resolves upward to that member root — but a master's
    // contents reach the screen only through an instance of that master, and
    // nothing instantiates this one (issue #1018). Letting its `CHANGE_TO` join
    // the enclosing set's default table would have a reaction that never paints
    // set the transition every instance of that set ships.
    //
    // A definition between the layer and the host is what stops it. An
    // **instance** between them does not, which is
    // `a_nested_instance_switching_its_parents_variant_resolves_through_the_parent`'s
    // claim — an instance's baked children do paint.
    //
    // The leak is measured through the set's own finding rather than through an
    // emitted table, because it needs no instance: the members here are
    // identical, so they differ on no rect channel, and a set that declares a
    // transition at all then says the transition has nothing to animate. Where
    // nothing declares one, the set is silent. That is the difference this
    // asserts.
    let buried = |id: &str, inner_id: &str| {
        let mut inner = boxed(inner_id, "inner", "FRAME", 10.0, 10.0, 20.0, 20.0);
        inner["interactions"] = smart_animate_to_active();
        let mut master = boxed(id, "buried", "COMPONENT", 10.0, 10.0, 20.0, 20.0);
        master["children"] = serde_json::json!([inner]);
        master
    };
    let member = |id: &str, name: &str, nested: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([nested]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", buried("3:1", "3:2")),
                member("1:6", "state=active", buried("3:3", "3:4")),
            ],
        },
        // A definition paints nothing, so the document needs a painting sibling
        // or `figma.no-content` is what this would measure. A plain FRAME, so
        // nothing instantiates the set and no table is emitted either.
        boxed("1:30", "elsewhere", "FRAME", 0.0, 200.0, 100.0, 50.0),
    ])));

    assert!(
        diagnostics.is_empty(),
        "nothing that paints declares a transition into this set, so the set has no \
         motion to lose and names none: {diagnostics:?}",
    );
}

#[test]
fn a_mirrored_member_baked_into_its_instance_refuses_that_instances_table() {
    // What a real export of a mirrored member looks like, and the case
    // `members_differing_only_in_which_way_their_matrix_faces_are_refused` stops
    // covering once its baked copies are kept upright. Figma bakes the resolved
    // content of the active member into the instance, so a member that mirrors
    // gives the instance a mirrored baked child — which since debt #1047 is
    // refused by name, leaving the variant table with no node to point at.
    //
    // Both findings are asserted, because the pair is the behaviour: the
    // refusal is an error that withholds the bytes under Strict, and the lost
    // table is the warning that says the baked subtree still paints.
    let mirrored = |id: &str| {
        let mut node = boxed(id, "bar", "FRAME", 10.0, 10.0, 20.0, 20.0);
        node["relativeTransform"] = serde_json::json!([[-1.0, 0.0, 10.0], [0.0, 1.0, 10.0]]);
        node
    };

    let (_, diagnostics) = lower_json(set_with(
        vec![mirrored("1:3")],
        vec![mirrored("1:7")],
        vec![mirrored("I1:14;1:3")],
    ));

    let refused: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.unsupported")
        .map(|d| {
            let Location::Node(at) = &d.at else {
                panic!("located at a node");
            };
            at.path.as_str()
        })
        .collect();
    assert_eq!(
        refused,
        vec!["/card/bar"],
        "the instance's baked copy is the one that paints, so it is the one refused — \
         the members sit inside a COMPONENT, which the walk skips whole: {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "figma.variants.unlowerable-set"
                && d.message.contains("lowered to no document node")),
        "and the table cannot name a node that never lowered: {diagnostics:?}",
    );
}

// --- The review round on this PR: three defects the fixes above introduced.

#[test]
fn a_contention_echoed_onto_two_instances_is_reported_once() {
    // The per-instance contention warning is a finding this PR added, and it
    // arrives through the same echo the rest of debt #1056 is about: the two
    // layers that disagree are the master's, so every instance of that member
    // carries the same contention. Reporting it per instance would re-create,
    // inside the pass that removes the multiplicity, exactly the multiplicity it
    // removes.
    let knob = |id: &str, name: &str, x: f64, y: f64, duration: f64| {
        let mut node = boxed(id, name, "FRAME", x, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "EASE_OUT" },
                    "duration": duration,
                },
            }],
        }]);
        node
    };
    let member = |id: &str, name: &str, x: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([
            knob(&format!("{id}:a"), "knob", x, 10.0, 0.4),
            knob(&format!("{id}:b"), "dial", 10.0, 10.0, 0.9),
        ]);
        node
    };
    let instance = |id: &str, y: f64| {
        let mut node = boxed(id, "card", "INSTANCE", 0.0, y, 100.0, 50.0);
        node["componentId"] = serde_json::json!("1:2");
        node["children"] = serde_json::json!([
            knob(&format!("I{id};1:2:a"), "knob", 10.0, y + 10.0, 0.4),
            knob(&format!("I{id};1:2:b"), "dial", 10.0, y + 10.0, 0.9),
        ]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 10.0), member("1:6", "state=active", 60.0)],
        },
        instance("1:14", 200.0),
        instance("1:15", 300.0),
    ])));

    let per_instance: Vec<(&str, &str)> = diagnostics
        .iter()
        .filter(|d| d.message.contains("layer of this instance declares"))
        .map(|d| {
            let Location::Node(at) = &d.at else {
                panic!("located at a node");
            };
            (at.path.as_str(), d.message.as_str())
        })
        .collect();
    assert_eq!(
        per_instance.len(),
        1,
        "one authored contention, one finding, whatever it is echoed onto: {diagnostics:?}",
    );
    assert!(
        per_instance[0]
            .1
            .contains("1 further copy of the same reaction"),
        "and it says how many instances it stands for: {:?}",
        per_instance[0],
    );
    // The emit pass files this finding under a node index and the naming pass
    // replays it there, building the `Location` itself rather than carrying one
    // (debt #1142). That is only right while the two passes walk `nodes` in the
    // same order, and nothing else asserts it. A change that dropped the finding
    // instead is caught by the count above; what only this line catches is the
    // two passes disagreeing on which node an index means, which reports every
    // contention at another node's path with the count still 1.
    assert_eq!(
        per_instance[0].0, "/card (1:14)",
        "and it is located at the instance the emit pass filed it under",
    );
}

#[test]
fn a_broken_destination_inside_a_library_instance_is_still_an_error() {
    // Issue #976's contract, from inside a published-library instance. The
    // nearest host — the inner instance — shows a member of a set this file
    // *carries*, so the file can judge the destination and does: it is not a
    // member, the switch lowers nowhere, and under Strict the bytes are
    // withheld rather than shipping a button whose click does nothing.
    //
    // Asking whether *any* enclosing host showed an absent component answered
    // "missing library" here, which is a fixed warning — and said "this file
    // does not contain the component whose instance this layer belongs to" of a
    // layer whose instance the file does contain.
    let mut inner = boxed("9:2", "chip", "INSTANCE", 10.0, 210.0, 20.0, 20.0);
    inner["componentId"] = serde_json::json!("1:2");
    inner["children"] = serde_json::json!([]);
    inner["interactions"] = serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{ "type": "NODE", "destinationId": "7:7", "navigation": "CHANGE_TO" }],
    }]);
    // The outer instance shows a component this file does not carry.
    let mut outer = boxed("9:1", "library-card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    outer["componentId"] = serde_json::json!("8:8");
    outer["children"] = serde_json::json!([inner]);
    let member = |id: &str, name: &str| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest"), member("1:6", "state=active")],
        },
        outer,
    ])));

    let broken: Vec<(&str, Severity)> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-interaction")
        .map(|d| (d.message.as_str(), d.severity))
        .collect();
    let [(message, severity)] = broken[..] else {
        panic!("one interaction refusal: {diagnostics:?}");
    };
    assert_eq!(
        severity,
        Severity::Error,
        "a destination the file can judge and finds broken withholds the bytes under \
         Strict: {diagnostics:?}",
    );
    assert!(
        message.contains("is not a member of the component set this layer belongs to"),
        "and the message is about the set the nearest host belongs to, which the file \
         does carry: {message:?}",
    );
}

#[test]
fn a_transition_declared_only_by_a_baked_layer_is_named_where_it_animates_nothing() {
    // A set differing on no rect channel writes no transition at all, so a
    // declared one lands in one frame — and that loss is named. Reading only
    // the set's default table for "does anything animate here" left the loss
    // silent whenever the declaration came from a baked layer instead of a
    // member root, which is the population debt #1064's widening enlarges from
    // the instance root to every layer beneath it (P4).
    let knob = |id: &str, y: f64, reacts: bool| {
        let mut node = boxed(id, "knob", "FRAME", 10.0, y, 20.0, 20.0);
        if reacts {
            node["interactions"] = smart_animate_to_active();
        }
        node
    };
    // The members differ in FILL only, so the set lowers and has no rect channel.
    let member = |id: &str, name: &str, grey: f64, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["fills"] = solid(grey, grey, grey);
        node["children"] = serde_json::json!([child]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["fills"] = solid(0.2, 0.2, 0.2);
    // Only the instance's baked layer declares the transition.
    instance["children"] = serde_json::json!([knob("I1:14;1:3", 210.0, true)]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", 0.2, knob("1:3", 10.0, false)),
                member("1:6", "state=active", 0.8, knob("1:7", 10.0, false)),
            ],
        },
        instance,
    ])));

    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("differ on no rect channel")),
        "the declared transition has nothing to animate, and that is said rather than \
         dropped: {diagnostics:?}",
    );
}

#[test]
fn a_switch_that_no_host_can_carry_is_named_rather_than_dropped() {
    // The other half of the definition boundary, and a silent drop the first
    // fix round introduced. A layer inside a nested master resolves to a set it
    // sits inside, and `hosting` refuses the member root because a
    // definition stands between them — correctly, since that master reaches the
    // screen independently of the set. But refusing left the switch with no
    // table and no diagnostic: the branch handling "no table carries this"
    // assumes something else already named why, which is true for an instance
    // whose table was refused and false here.
    //
    // The shape: set S carries a nested component **set** inside each member.
    // A layer inside the nested member that no instance echoes is named, and
    // its CHANGE_TO points at S — which nothing in its own ancestry can switch.
    let inner = |id: &str| {
        let mut node = boxed(id, "inner", "FRAME", 10.0, 10.0, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{ "type": "NODE", "destinationId": "1:6", "navigation": "CHANGE_TO" }],
        }]);
        node
    };
    let tone = |id: &str, name: &str, child: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 10.0, 10.0, 20.0, 20.0);
        node["children"] = serde_json::json!([child]);
        node
    };
    let nested_set = |set_id: &str, plain: &str, loud: &str, a: &str, b: &str| {
        serde_json::json!({
            "id": set_id,
            "name": "nested-set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 10.0, "y": 10.0, "width": 40.0, "height": 20.0 },
            "children": [tone(plain, "tone=plain", inner(a)), tone(loud, "tone=loud", inner(b))],
        })
    };
    let member = |id: &str, name: &str, nested: serde_json::Value| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([nested]);
        node
    };
    // An instance of the nested set's FIRST member, so its members become
    // switchable and the second member — which nothing echoes — is named.
    let mut chip = boxed("3:20", "chip", "INSTANCE", 0.0, 400.0, 20.0, 20.0);
    chip["componentId"] = serde_json::json!("3:1");
    chip["children"] = serde_json::json!([inner("I3:20;3:2")]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [
                member("1:2", "state=rest", nested_set("3:10", "3:1", "3:3", "3:2", "3:4")),
                member("1:6", "state=active", nested_set("4:10", "4:1", "4:3", "4:2", "4:4")),
            ],
        },
        chip,
    ])));

    let named: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-interaction")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        named
            .iter()
            .any(|m| m.contains("no switch into that set replaces")),
        "the boundary refuses the host, and says so — refusing in silence is the drop \
         P4 forbids: {diagnostics:?}",
    );
}

// --- Debt #1141 and #1137: what a member root carries, and what keys a collapse.

/// An `ON_CLICK` `CHANGE_TO` to `1:6` whose transition is a spring preset —
/// real vocabulary Figma authors and `dashcue` has no spelling for, so the
/// switch ships and its curve is refused. That refusal is what makes a
/// carries-or-not answer visible: it is reported as a degrade where a table
/// carries the switch, and as a loss where none does.
fn refused_curve_to_active() -> serde_json::Value {
    serde_json::json!([{
        "trigger": { "type": "ON_CLICK" },
        "actions": [{
            "type": "NODE",
            "destinationId": "1:6",
            "navigation": "CHANGE_TO",
            "transition": { "type": "SMART_ANIMATE", "easing": { "type": "GENTLE" }, "duration": 0.3 },
        }],
    }])
}

#[test]
fn a_member_of_a_set_no_instance_could_emit_is_not_given_a_degrade() {
    // Debt #1141. A member root was treated as carrying the set's default
    // transitions on the strength of the set having a *plan*. A plan is not a
    // table: `emit` is per instance and refuses one whose baked geometry
    // disagrees with the member it shows. Where every instance is refused —
    // here, the only one — `doc.variant_sets` is empty, and calling the
    // member's refused curve a degrade claimed a state change no table carries.
    // That is issue #1017's defect on a path that issue did not reach.
    //
    // `a_refused_curve_on_a_member_no_instance_echoes_is_still_a_degrade` is the
    // neighbouring case and still passes: there an instance *does* emit, so the
    // member's transition really is copied into a shipped table.
    let mut member = boxed("1:6", "state=active", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    member["children"] = serde_json::json!([boxed("1:7", "bar", "FRAME", 40.0, 10.0, 20.0, 20.0)]);
    member["interactions"] = refused_curve_to_active();
    let mut rest = boxed("1:2", "state=rest", "COMPONENT", 0.0, 0.0, 100.0, 50.0);
    rest["children"] = serde_json::json!([boxed("1:3", "bar", "FRAME", 10.0, 10.0, 20.0, 20.0)]);
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    // The baked bar sits at x = 25 where the member it shows authors 10, which
    // is an instance-level override the variant table cannot express — so this
    // instance, the set's only one, emits nothing.
    instance["children"] =
        serde_json::json!([boxed("I1:14;1:3", "bar", "FRAME", 25.0, 210.0, 20.0, 20.0)]);

    let (doc, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [rest, member],
        },
        instance,
    ])));

    assert!(
        doc.variant_sets.is_empty(),
        "the only instance was refused, so no variant table ships at all",
    );
    let motion: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-motion")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        motion
            .iter()
            .any(|m| m.contains("no variant table carries the switch it would animate")),
        "the member's curve lost a switch that ships nowhere: {diagnostics:?}",
    );
    assert!(
        !motion.iter().any(|m| m.contains("lands in one frame")),
        "and it must not be called a degrade, which would claim a state change no \
         table carries: {diagnostics:?}",
    );
}

#[test]
fn two_different_refused_curves_on_one_layer_are_both_reported() {
    // Debt #149 — every finding survives one pass — at the scope debt #1056's
    // collapse operates in. Two reactions on one layer to the same destination,
    // refusing two different constructs, are two losses: a designer who fixes
    // the DISSOLVE would otherwise meet the spring refusal only on the next
    // compile, which is what P4 forbids.
    //
    // The collapse is keyed on the rendered message, which distinguishes them
    // because the construct's own name is in it. A key built from the
    // destination and the finding's kind — without the construct — folds the
    // second onto the first and prints it nowhere. That was tried and reverted;
    // this is what refuses it.
    let react = |curve: serde_json::Value| {
        serde_json::json!({
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                "transition": curve,
            }],
        })
    };
    let mut knob = boxed("I1:14;1:2:bar", "bar", "FRAME", 10.0, 210.0, 20.0, 20.0);
    knob["interactions"] = serde_json::json!([
        react(serde_json::json!({
            "type": "DISSOLVE", "easing": { "type": "EASE_OUT" }, "duration": 0.3,
        })),
        react(serde_json::json!({
            "type": "SMART_ANIMATE", "easing": { "type": "GENTLE" }, "duration": 0.3,
        })),
    ]);
    let member = |id: &str, name: &str, x: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([boxed(
            &format!("{id}:bar"),
            "bar",
            "FRAME",
            x,
            10.0,
            20.0,
            20.0
        )]);
        node
    };
    let mut instance = boxed("1:14", "card", "INSTANCE", 0.0, 200.0, 100.0, 50.0);
    instance["componentId"] = serde_json::json!("1:2");
    instance["children"] = serde_json::json!([knob]);

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 10.0), member("1:6", "state=active", 40.0)],
        },
        instance,
    ])));

    let motion: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-motion")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        motion.iter().any(|m| m.contains("DISSOLVE")),
        "the refused transition kind is named: {diagnostics:?}",
    );
    assert!(
        motion.iter().any(|m| m.contains("GENTLE")),
        "and so is the refused easing, rather than being folded onto the first and \
         printed nowhere: {diagnostics:?}",
    );
}

#[test]
fn a_refused_curve_echoed_onto_two_instances_is_one_finding() {
    // Debt #1137's guard, for the message shape that had none.
    //
    // The collapse is keyed on the rendered message, so it holds only while
    // every copy of one authored reaction renders the same message. That is a
    // property of the message text, and nothing in the type system keeps it —
    // so it needs a test per shape that can echo, not one.
    //
    // `one_authored_reaction_inside_a_master_is_one_finding_however_many_instances_show_it`
    // pins the `figma.prototype.unsupported-interaction` omission shape. This
    // pins the `figma.prototype.unsupported-motion` degrade shape, which was
    // measured to be unguarded: adding the node path to "the switch lands in
    // one frame" left the whole suite green.
    let knob = |id: &str, y: f64| {
        let mut node = boxed(id, "bar", "FRAME", 10.0, y, 20.0, 20.0);
        node["interactions"] = serde_json::json!([{
            "trigger": { "type": "ON_CLICK" },
            "actions": [{
                "type": "NODE",
                "destinationId": "1:6",
                "navigation": "CHANGE_TO",
                // A spring preset: the switch ships and its curve is refused,
                // which is what makes this the degrade shape.
                "transition": {
                    "type": "SMART_ANIMATE",
                    "easing": { "type": "GENTLE" },
                    "duration": 0.3,
                },
            }],
        }]);
        node
    };
    let member = |id: &str, name: &str, x: f64| {
        let mut node = boxed(id, name, "COMPONENT", 0.0, 0.0, 100.0, 50.0);
        node["children"] = serde_json::json!([boxed(
            &format!("{id}:bar"),
            "bar",
            "FRAME",
            x,
            10.0,
            20.0,
            20.0
        )]);
        node
    };
    let instance = |id: &str, y: f64| {
        let mut node = boxed(id, "card", "INSTANCE", 0.0, y, 100.0, 50.0);
        node["componentId"] = serde_json::json!("1:2");
        node["children"] = serde_json::json!([knob(&format!("I{id};1:2:bar"), y + 10.0)]);
        node
    };

    let (_, diagnostics) = lower_json(document_with(serde_json::json!([
        {
            "id": "1:10",
            "name": "set",
            "type": "COMPONENT_SET",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
            "children": [member("1:2", "state=rest", 10.0), member("1:6", "state=active", 40.0)],
        },
        instance("1:14", 200.0),
        instance("1:15", 300.0),
    ])));

    let motion: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.rule == "figma.prototype.unsupported-motion")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        motion.len(),
        1,
        "one authored curve, one finding, whatever it is echoed onto: {diagnostics:?}",
    );
    assert!(
        motion[0].contains("1 further copy of the same reaction"),
        "and it says how many instances it stands for — which is what fails first \
         if any part of this message ever varies per copy: {:?}",
        motion[0],
    );
}
