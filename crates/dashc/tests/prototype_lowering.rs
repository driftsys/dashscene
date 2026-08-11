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

use dashc_wasm::figma::lower;
use dashc_wasm::{
    CompileError, Document, EmitPolicy, TransitionSpec, VariantValue, compile_figma,
    compile_figma_with_bindings_and_policy,
};
use dashscene_validator::{Profile, Severity};

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
    // Two of its fifteen `refused-*` nodes are navigation targets carrying no
    // interaction, and two more — `refused-scroll-animate` and
    // `refused-mouse-enter` — carry none either, because both
    // `setReactionsAsync` writes were refused when the capture was authored.
    // So eleven constructs are exercisable here, not fifteen; the README
    // already says a test asserting on the last two will find nothing.
    let error = compile_figma(REFUSED, Profile::Core, &BTreeMap::new())
        .expect_err("a refused prototype construct withholds the bytes under Strict (R6)");
    let CompileError::Diagnostics(found) = error else {
        panic!("the refusal arrives as diagnostics, not as a walk abort");
    };
    let report = format!("{found}");

    for construct in [
        "DISSOLVE",
        "PUSH",
        "CUSTOM_CUBIC_BEZIER",
        "EASE_OUT_BACK",
        "AFTER_TIMEOUT",
        "ON_KEY_DOWN",
        "URL",
        "SET_VARIABLE",
        "OVERLAY",
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
    // Seventeen, not ten: every finding survives one pass (debt #149), so
    // seven of the ten interaction-carrying nodes report two independent gaps
    // — a refused navigation beside a refused transition or trigger. The
    // number is pinned here because `corpus/figma-fixtures/README.md` and the
    // manifest both state it, and nothing else would notice it going stale.
    assert_eq!(errors.len(), 17, "{found}");
    let nodes: std::collections::BTreeSet<String> =
        errors.iter().map(|d| format!("{}", d.at)).collect();
    assert_eq!(nodes.len(), 10, "across ten nodes: {found}");
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
    let file: dashc_wasm::figma::rest::FigmaFile =
        serde_json::from_value(value).expect("the synthetic document parses");
    lower(&file, Profile::Core, &BTreeMap::new()).expect("the document lowers")
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

    let (_, diagnostics) = lower_json(doc_json);
    let named = diagnostics
        .iter()
        .find(|d| d.message.contains("9:99"))
        .unwrap_or_else(|| panic!("the dangling destination is named: {diagnostics:?}"));
    assert_eq!(named.rule, "figma.prototype.unsupported-motion");
    assert_eq!(named.severity, Severity::Warning);
}

#[test]
fn a_reaction_on_a_master_nothing_instantiates_is_not_a_finding() {
    // `Walk::visit` fires no diagnostic inside a definition, because nothing
    // in it paints. A refused reaction on a master no instance shows costs the
    // picture nothing either — and at error severity under Strict it would
    // withhold the whole document over a layer that never ships.
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
    boxed("1:30", "elsewhere", "FRAME", 0.0, 200.0, 100.0, 50.0)]));
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
