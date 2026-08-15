//! Figma's prototype interactions, read off a captured node (story #773).
//!
//! Figma's prototype model *is* `dashcue`'s model in one narrow place: a
//! reaction whose action changes an instance to another variant of its own
//! component set carries a duration and an easing, and that is exactly what a
//! `VariantTransition` says. Everything else Figma's prototype vocabulary can
//! express — navigating to another frame, opening an overlay, following a
//! URL, setting a variable, branching on one — has no construct in this
//! document at all.
//!
//! So this module answers one question per interaction: **is it a variant
//! switch, and if so how does it animate**. What is not a variant switch is
//! named and dropped, never lowered approximately (P4). The producer owns the
//! mapping and the validator owns the verdict (P5), which is why the names
//! this module returns are strings assembled here rather than
//! `dashscene_validator::Construct` variants — a prototype vocabulary in the
//! validator would make its enum a list of one producer's gaps.
//!
//! Every shape read here is pinned by `prototype-smart-animate.json` and
//! `prototype-refused.json` and written up in
//! `docs/technotes/figma-rest-shapes.md`. Two of that note's
//! findings are load-bearing here and invisible in the code that uses them:
//! the nested `duration` is in **seconds** (Figma's own published spec says
//! milliseconds, and is wrong), and the flat
//! `transitionNodeID`/`transitionDuration`/`transitionEasing` triple is never
//! read, because it cannot express a trigger, a navigation or a second
//! action, and it invents a transition where the interaction says there is
//! none.

use crate::document::Easing;
use crate::figma::rest::{Action, Node, Transition};

/// One lowered variant switch: which member it travels to, and how.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Switch {
    /// The Figma node id of the member `COMPONENT` the switch changes to —
    /// the destination the transition is keyed on
    /// (`docs/decisions/motion-is-document-data-keyed-on-the-destination.md`).
    pub(super) destination: String,
    /// The tween the switch animates with: seconds and a curve. `None` means
    /// the switch lands in one frame, which is what every document written
    /// before v0.18 says — either because the interaction carries no
    /// transition, or because the one it carries has no `dashcue` spelling
    /// and was named in `refused_motion` below rather than approximated.
    pub(super) tween: Option<(f32, Easing)>,
    /// The transition this switch declared and the vocabulary has no spelling
    /// for, named but not yet classified.
    ///
    /// It rides on the switch rather than being filed as a finding here
    /// because **this module cannot tell a degrade from an omission** (issue
    /// #1017): whether the curve's loss leaves a state change behind depends
    /// on whether `destination` resolves to a member of a set the file
    /// carries, and that is not known until `variants::apply` has walked the
    /// file. Deciding it here from the mere presence of a `destinationId`
    /// filed a refused curve as "the switch lands in one frame" for a switch
    /// that landed nowhere.
    pub(super) refused_motion: Option<String>,
}

/// What one node's interactions lower to.
#[derive(Debug, Default)]
pub(super) struct Interactions {
    /// The variant switches, in the order the payload declares them.
    pub(super) switches: Vec<Switch>,
    /// Interactions with no lowering at all — a trigger, action, navigation
    /// or transition kind outside the vocabulary. Nothing about them reaches
    /// the document.
    ///
    /// A refused *transition* reaches this list only where no switch was
    /// built at all: a refused trigger, a navigation other than `CHANGE_TO`,
    /// or a `CHANGE_TO` with no `destinationId`. Where a switch exists the
    /// curve rides on it, in [`Switch::refused_motion`], because only the
    /// caller knows whether that switch lands.
    pub(super) unsupported: Vec<String>,
}

/// Reads one node's `interactions`, sorting them into candidate switches and
/// refusals.
///
/// It classifies nothing that depends on resolution: a `CHANGE_TO`'s
/// destination is an id this module cannot look up, so a switch it returns is
/// a candidate and a curve it could not lower rides on that candidate
/// unclassified (issue #1017).
///
/// Reactions are read off the node being walked and are never resolved back
/// through the component set: REST reports a component's interaction on an
/// instance **verbatim**, so an instance that inherits its reactions already
/// carries them in full (`instance-inherited` pins this).
///
/// Every finding survives one pass (debt #149): an interaction whose trigger
/// and whose navigation are both outside the vocabulary reports both, because
/// they are two independent gaps and a reader who fixed one would meet the
/// other.
pub(super) fn read(node: &Node) -> Interactions {
    let mut found = Interactions::default();

    for interaction in &node.interactions {
        // Nothing in the document carries a trigger: a `VariantTransition` is
        // keyed on its destination member and the *host* decides when to
        // switch. `ON_CLICK` is the gesture that design assumes, so it is the
        // one trigger whose loss costs nothing. Any other trigger carries
        // information the destination key cannot hold — `AFTER_TIMEOUT` says
        // the switch happens by itself, `ON_KEY_DOWN` names an input — so it
        // is refused rather than quietly re-pointed at a click.
        let trigger = interaction.trigger.as_ref().map(|t| t.kind.as_str());
        let trigger_lowers = trigger == Some("ON_CLICK");
        if !trigger_lowers {
            found.unsupported.push(format!(
                "prototype trigger {} (only ON_CLICK is assumed; the document carries no trigger \
                 construct, so the host drives the switch)",
                trigger.unwrap_or("<absent>"),
            ));
        }

        // A refused trigger refuses the whole interaction, switch included.
        // `UNSUPPORTED_INTERACTION`'s contract is that nothing about it
        // reaches the document, and a switch that lowered under a trigger
        // this producer cannot honour would make that false: under
        // `Partial` the file would emit carrying a state change Figma
        // performs on a timer and dashscene performs on a click.
        for action in &interaction.actions {
            read_action(action, trigger_lowers, &mut found);
        }
    }

    found
}

/// One action's verdict, appended to `found`.
///
/// `trigger_lowers` is whether the interaction's trigger survived. It decides
/// nothing about which gaps are *named* — every finding survives one pass
/// either way (debt #149) — only whether a switch is **built** at all, and so
/// whether a refused curve travels on that switch or is part of an omission
/// this module can already see.
///
/// A switch built here is a *candidate*, never a promise: this module reads
/// one node and cannot resolve a `destinationId` against the file's component
/// sets, so whether the candidate reaches the document is `variants::apply`'s
/// answer and not this one (issue #1017).
fn read_action(action: &Action, trigger_lowers: bool, found: &mut Interactions) {
    // `CONDITIONAL` nests `Action[]` inside `conditionalBlocks` recursively;
    // the branches are deliberately not descended into. The document has no
    // condition construct, so a branch that lowered would run
    // unconditionally — a picture the designer never authored, which is worse
    // than the omission this names.
    if action.kind != "NODE" {
        found.unsupported.push(format!(
            "prototype action {} (the document carries no navigation, variable or condition \
             construct)",
            action.kind,
        ));
        return;
    }

    // `CHANGE_TO` is the one navigation with a lowering: it swaps an
    // instance's variant, which is `set_variant`. `NAVIGATE`, `OVERLAY` and
    // `SCROLL_TO` all move between *frames*, and this document is one scene
    // with no page model.
    let navigation = action.navigation.as_deref();
    let destination = match (navigation, action.destination_id.clone()) {
        (Some("CHANGE_TO"), Some(destination)) => Some(destination),
        (Some("CHANGE_TO"), None) => {
            found
                .unsupported
                .push("a CHANGE_TO action with no destinationId".to_string());
            None
        }
        (navigation, _) => {
            found.unsupported.push(format!(
                "prototype navigation {} (only CHANGE_TO lowers; the document has no \
                 frame-to-frame navigation)",
                navigation.unwrap_or("<absent>"),
            ));
            None
        }
    };

    // The transition is read whether or not a switch is built, because a
    // transition kind outside the vocabulary is a gap under *any* navigation
    // and every finding must survive one pass (debt #149) — otherwise a
    // designer who changed NAVIGATE to CHANGE_TO would meet the second
    // refusal only on the next compile.
    //
    // Where a switch is built the refusal travels **on** it, unclassified.
    // Where none is built there is nothing for it to travel on and no
    // resolution left to wait for: the trigger or the navigation has already
    // refused the whole interaction, so the curve is part of that omission
    // here.
    let builds_switch = trigger_lowers && destination.is_some();
    let mut refused_motion = None;
    let tween = match &action.transition {
        None => None,
        Some(transition) => {
            let (tween, refused) = read_transition(transition);
            if let Some(what) = refused {
                if builds_switch {
                    refused_motion = Some(what);
                } else {
                    found.unsupported.push(what);
                }
            }
            tween
        }
    };

    if let Some(destination) = destination.filter(|_| builds_switch) {
        found.switches.push(Switch {
            destination,
            tween,
            refused_motion,
        });
    }
}

/// The tween a transition lowers to, and the construct refused by name when
/// it lowers to none.
fn read_transition(transition: &Transition) -> (Option<(f32, Easing)>, Option<String>) {
    // Smart Animate interpolates whatever differs between the two variants,
    // which is what per-prop tracks express. `DISSOLVE` cross-fades the two
    // states as images and `PUSH`/`MOVE_IN`/`SLIDE_IN` translate whole frames
    // past each other; neither is a per-prop interpolation, and there is
    // nothing in the vocabulary to approximate them with.
    if transition.kind != "SMART_ANIMATE" {
        return (
            None,
            Some(format!(
                "prototype transition {} (only SMART_ANIMATE lowers, because only it interpolates \
                 per-prop)",
                transition.kind,
            )),
        );
    }

    let easing = transition.easing.as_ref().map(|e| e.kind.as_str());
    let easing = match easing {
        Some("LINEAR") => Easing::Linear,
        Some("EASE_IN") => Easing::EaseIn,
        Some("EASE_OUT") => Easing::EaseOut,
        // Figma's name for the symmetric curve is EASE_IN_AND_OUT; `dashcue`
        // calls the same curve EaseInOut.
        Some("EASE_IN_AND_OUT") => Easing::EaseInOut,
        // The four spring presets — GENTLE, QUICK, BOUNCY, SLOW — arrive as a
        // bare `{"type": …}` with no `easingFunctionSpring`, so REST supplies
        // none of the stiffness and damping a `dashcue` Spring needs. Mapping
        // one anyway would put numbers in the document that no producer
        // wrote and nothing can verify. Liftable the moment the four presets'
        // parameters are measured and recorded.
        //
        // CUSTOM_CUBIC_BEZIER *does* arrive with its four control points, and
        // is refused for the opposite reason: `dashcue` has four fixed cubics
        // and no arbitrary one, and `KeyframesSpec` samples progress rather
        // than declaring a curve, so there is nothing to lower the control
        // points into.
        other => {
            return (
                None,
                Some(format!(
                    "prototype easing {} (dashcue has four fixed cubics — Linear, EaseIn, \
                     EaseOut, EaseInOut — and no parameters for it in the payload)",
                    other.unwrap_or("<absent>"),
                )),
            );
        }
    };

    // Seconds, not milliseconds — see the module docs. A transition Figma
    // reports at or below zero has no span to travel, which is the same thing
    // as no transition and needs no separate name for it.
    let duration = transition.duration.unwrap_or(0.0);
    ((duration > 0.0).then_some((duration, easing)), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figma::rest::FigmaFile;

    const SMART_ANIMATE: &str =
        include_str!("../../../../corpus/figma-fixtures/prototype-smart-animate.json");
    const REFUSED: &str = include_str!("../../../../corpus/figma-fixtures/prototype-refused.json");

    fn find<'a>(file: &'a FigmaFile, name: &str) -> &'a Node {
        fn walk<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
            if node.name == name {
                return Some(node);
            }
            node.children.iter().find_map(|child| walk(child, name))
        }
        walk(&file.document, name).expect("the fixture has the node")
    }

    fn file(json: &str) -> FigmaFile {
        serde_json::from_str(json).expect("the fixture parses")
    }

    #[test]
    fn a_smart_animate_change_to_lowers_to_a_switch() {
        let f = file(SMART_ANIMATE);
        let read = read(find(&f, "state=rest"));
        assert_eq!(
            read.switches,
            vec![Switch {
                destination: "6:183".to_string(),
                tween: Some((0.3, Easing::EaseOut)),
                refused_motion: None,
            }],
        );
        assert!(read.unsupported.is_empty(), "{:?}", read.unsupported);
    }

    #[test]
    fn the_duration_is_read_in_seconds_not_milliseconds() {
        // The one finding that would have been invisible: `@figma/rest-api-spec`
        // documents the nested duration as milliseconds and it is seconds, so
        // a lowering that trusted the comment would divide by 1000 and animate
        // this in 0.3 ms. Both spellings are `number`, so only a value test
        // catches it — and the flat `transitionDuration: 300` sitting beside
        // it is what a wrong reading would produce.
        let f = file(SMART_ANIMATE);
        let (duration, _) = read(find(&f, "state=rest")).switches[0]
            .tween
            .expect("the arm carries a tween");
        assert!(
            (0.25..0.35).contains(&duration),
            "0.3 s must lower as 0.3, not 0.0003 and not 300: got {duration}",
        );
    }

    #[test]
    fn each_easing_arm_lowers_or_is_named() {
        let f = file(SMART_ANIMATE);
        for (name, expected) in [
            ("easing-linear", Some((0.05, Easing::Linear))),
            ("easing-ease-in-and-out", Some((0.15, Easing::EaseInOut))),
            ("easing-gentle", None),
            ("easing-quick", None),
            ("easing-bouncy", None),
            ("easing-slow", None),
        ] {
            let read = read(find(&f, name));
            assert_eq!(read.switches.len(), 1, "{name} carries one switch");
            assert_eq!(read.switches[0].tween, expected, "{name}");
            // A switch was built either way, so a refused easing must never
            // reach `unsupported`, which withholds the document under Strict.
            // It rides on the switch instead, and `variants::apply` decides
            // whether that is a degrade or part of an omission (issue #1017).
            assert!(
                read.unsupported.is_empty(),
                "{name}: {:?}",
                read.unsupported
            );
            assert_eq!(
                read.switches[0].refused_motion.is_none(),
                expected.is_some(),
                "{name} names its dropped curve exactly when it has one",
            );
        }
    }

    #[test]
    fn an_instance_echoes_its_inherited_reaction_in_full() {
        // REST reports a component's interaction on its instance verbatim, so
        // the lowering never has to resolve back through the component set.
        let f = file(SMART_ANIMATE);
        assert_eq!(
            read(find(&f, "instance-inherited")).switches,
            read(find(&f, "state=rest")).switches,
        );
    }

    #[test]
    fn every_refused_construct_is_named_by_its_own_node() {
        let f = file(REFUSED);
        for (node, expected) in [
            ("refused-dissolve", "DISSOLVE"),
            ("refused-push-left", "PUSH"),
            ("refused-custom-cubic-bezier", "CUSTOM_CUBIC_BEZIER"),
            ("refused-ease-out-back", "EASE_OUT_BACK"),
            ("refused-after-timeout", "AFTER_TIMEOUT"),
            ("refused-on-key-down", "ON_KEY_DOWN"),
            ("refused-url", "URL"),
            ("refused-set-variable", "SET_VARIABLE"),
            ("refused-overlay", "OVERLAY"),
            ("refused-conditional", "CONDITIONAL"),
        ] {
            let read = read(find(&f, node));
            let named: Vec<&String> = read
                .unsupported
                .iter()
                .chain(
                    read.switches
                        .iter()
                        .filter_map(|s| s.refused_motion.as_ref()),
                )
                .collect();
            assert!(
                named.iter().any(|m| m.contains(expected)),
                "{node} must name {expected}, got {named:?}",
            );
            assert!(
                read.switches.is_empty(),
                "{node} lowers no switch, got {:?}",
                read.switches,
            );
        }
    }

    #[test]
    fn a_navigate_action_is_refused_even_though_its_transition_is_smart_animate() {
        // `refused-ease-out-back` and `refused-custom-cubic-bezier` both carry
        // a SMART_ANIMATE transition under a NAVIGATE navigation. The
        // navigation is what decides: a frame-to-frame move has no lowering
        // whatever it animates with, so the refusal must be the hard kind that
        // withholds the document and not the motion degrade an easing earns.
        let f = file(REFUSED);
        let read = read(find(&f, "refused-ease-out-back"));
        assert!(
            read.unsupported.iter().any(|m| m.contains("NAVIGATE")),
            "{:?}",
            read.unsupported,
        );
        assert!(read.switches.is_empty());
    }

    #[test]
    fn the_flat_triple_is_never_read() {
        // `refused-on-key-down` carries `"transition": null` inside its action
        // and `transitionDuration: 300` outside it — a transition no author
        // wrote. A lowering that fell back to the flat fields would find one
        // here; reading `interactions` alone finds none.
        let f = file(REFUSED);
        let node = find(&f, "refused-on-key-down");
        assert!(
            node.interactions[0].actions[0].transition.is_none(),
            "the fixture's inner transition is null",
        );
        assert!(read(node).switches.is_empty());
    }

    #[test]
    fn a_refused_trigger_lowers_no_switch() {
        // The capture cannot exercise this: `refused-after-timeout` and
        // `refused-on-key-down` both carry NAVIGATE, so the navigation refuses
        // the switch before the trigger has to. Synthetic, and named as such —
        // the shape is Figma's documented `AfterTimeoutTrigger` beside the
        // CHANGE_TO the captures pin.
        //
        // What it guards: `unsupported`'s contract says nothing about the
        // interaction reaches the document. A switch that lowered here would
        // make that false under `EmitPolicy::Partial`, where the file emits
        // and would carry a state change Figma performs on a timer.
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "timed",
            "type": "INSTANCE",
            "interactions": [{
                "trigger": { "type": "AFTER_TIMEOUT", "timeout": 1.5 },
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
            }],
        }))
        .unwrap();

        let read = read(&node);
        assert!(
            read.switches.is_empty(),
            "a refused trigger refuses the whole interaction: {:?}",
            read.switches,
        );
        assert!(
            read.unsupported.iter().any(|m| m.contains("AFTER_TIMEOUT")),
            "{:?}",
            read.unsupported,
        );
    }

    #[test]
    fn a_refused_curve_under_a_refused_trigger_is_an_omission_not_a_degrade() {
        // The bucket matters: a degrade says "the switch lands in one frame",
        // which claims a switch. With no switch built at all there is nothing
        // for the curve to ride on and no resolution left to wait for, so it
        // is part of the omission here rather than deferred (issue #1017).
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "timed-spring",
            "type": "INSTANCE",
            "interactions": [{
                "trigger": { "type": "AFTER_TIMEOUT", "timeout": 1.5 },
                "actions": [{
                    "type": "NODE",
                    "destinationId": "1:6",
                    "navigation": "CHANGE_TO",
                    "transition": {
                        "type": "SMART_ANIMATE",
                        "easing": { "type": "GENTLE" },
                        "duration": 0.3,
                    },
                }],
            }],
        }))
        .unwrap();

        let read = read(&node);
        assert!(read.switches.is_empty(), "{:?}", read.switches);
        assert!(
            read.unsupported.iter().any(|m| m.contains("GENTLE")),
            "{:?}",
            read.unsupported
        );
    }

    #[test]
    fn a_node_with_no_interactions_lowers_nothing_and_names_nothing() {
        let f = file(REFUSED);
        // Both are navigation *targets* that the authoring command builds
        // without reactions, which is the property this test needs. Note what
        // that is not: Figma imposes no rule that a target carries no
        // interaction, so this holds by how the fixture is authored and not by
        // construction. `refused-scroll-animate` stood here until the
        // re-capture landed the `setReactionsAsync` write that had been
        // refused, at which point it gained one and stopped being an example of
        // the case — and a later re-capture can do the same to these two
        // (`corpus/figma-fixtures/README.md`).
        for name in ["refused-destination", "refused-overlay-target"] {
            let read = read(find(&f, name));
            assert!(read.switches.is_empty(), "{name}");
            assert!(read.unsupported.is_empty(), "{name}");
        }
    }
}
