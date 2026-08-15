//! The variant table, lowered from Figma's component sets (story #773).
//!
//! Story #242 lowered an `INSTANCE` from its baked subtree and left
//! `COMPONENT`/`COMPONENT_SET` resolving-but-not-painting, deferring the
//! variant table by name: "emitting the variant table from the Figma lowering
//! is its own story with its own overridable-prop mapping"
//! (`docs/decisions/figma-component-lowering.md`). This is that story, and it
//! arrived as story #773's blocker rather than on its own: a
//! `VariantTransition` nests on a `VariantMember`, so a lowered prototype
//! interaction had nowhere to land.
//!
//! # One variant set per instance
//!
//! A `VariantOverride` names a **document node index**, and a definition
//! lowers to no document node — so the variant table cannot be expressed
//! against the component set. It is expressed against each `INSTANCE` of that
//! set instead: one `VariantSet` per instance, its `active_member` taken from
//! the instance's `componentId`, and its overrides pointing at that
//! instance's own baked children. Components keep not painting, so story
//! #242's rule survives untouched.
//!
//! The instance's children carry the **same names** as the member's, which is
//! what joins the two trees. Names, not ids: an instance's baked children get
//! synthetic `I<instance>;<source>` ids, and the ids differ per instance while
//! the names do not.
//!
//! # What the members' differences lower to, and what they do not
//!
//! Figma's Smart Animate interpolates whatever differs between two variants,
//! so the tracks come from **diffing the members** — one Figma interaction
//! fans out across all of them. Two limits shape that diff, and both are
//! named rather than approximated (P4):
//!
//! - **Only a difference a `VariantValue` can express lowers at all.** The
//!   members are otherwise compared field by field, and a difference anywhere
//!   else — a child only one member has, a different corner radius, a
//!   different auto-layout mode — makes the whole set unlowerable. The
//!   comparison destructures `rest::Node`, so a field added to the REST
//!   subset later cannot be forgotten here: it fails to compile until it is
//!   classified.
//! - **Only a rect difference can be animated.** A fill, visibility or
//!   rotation difference lowers as an *override* — the switch carries it —
//!   but not as a *track*, because commit resolves a node's paint from the
//!   variant overlay ahead of its staged value, so every sample of a paint
//!   transition is masked by the member it travels towards (issue #891,
//!   measured and recorded in
//!   `docs/decisions/motion-is-document-data-keyed-on-the-destination.md`).
//!
//! # Severity: an omission, or a degrade
//!
//! One of the three rules below follows the emit policy the way
//! `figma.unsupported` does — an error under `Strict`, a warning under
//! `Partial`. **Unlike `figma.unsupported` it does not skip the node**: what has
//! no lowering is the behaviour, not the box
//! (`docs/decisions/figma-component-lowering.md`, and
//! `docs/specification/06-dashc-figma-lowering.md` states it as a `shall`). The
//! other two never follow the policy, and the split is what each one costs:
//!
//! - An **interaction with no lowering** is an omission. Nothing about it
//!   reaches the document, and under `Strict` the contract is that no bytes
//!   are handed over while authored intent is being dropped (R6). One
//!   population under that rule is a warning in both policies: a `CHANGE_TO`
//!   on a node whose own component set the file does not carry at all, which
//!   is the ordinary shape of an instance of a published-library set and
//!   which loses exactly what an unlowerable set below loses (issue #1016).
//!   `interaction_diagnostics` states that argument where the split is made.
//! - A **motion degrade** and an **unlowerable set** leave the picture
//!   exactly as it is. A switch that lands in one frame is what every
//!   document written before v0.18 says, and a set that emits no variant
//!   table is what *every* Figma import said before this story — so refusing
//!   the file over either would withhold a document that renders correctly,
//!   and would stop `lowering-variant-topology.json` compiling at all. That
//!   is `figma/bindings.rs`'s posture, for its reason: "the picture is right;
//!   only the live binding is not carried yet."

use std::collections::{BTreeMap, BTreeSet};

use dashpaint::Color;
use dashscene_validator::{Diagnostic, Location, NodePath, Severity};

use crate::document::{
    AxisSizing, BindingChannel, Document, Easing, PropTransition, TransitionSpec, VariantMember,
    VariantOverride, VariantSet, VariantTransition, VariantValue,
};
use crate::figma::bindings::IndexOfId;
use crate::figma::prototype::{self, Interactions, Switch};
use crate::figma::rest::{FigmaFile, Node, Paint, carries_its_angle, has_area, same_orientation};

/// The diagnostic rules this producer assembles for the variant table and the
/// prototype interactions that animate it.
pub mod rule {
    /// A prototype interaction the document has no construct for at all — a
    /// trigger, action or navigation outside the vocabulary, **or** a
    /// `CHANGE_TO` naming a destination that is not a member of the component
    /// set the node shows, which lowers no switch at all (issue #976). Nothing
    /// about either reaches the document, so the severity follows the emit
    /// policy: an error that withholds the bytes under `EmitPolicy::Strict`
    /// (R6), a warning under `EmitPolicy::Partial`.
    ///
    /// **One population under this rule is a fixed warning in both policies**
    /// (issue #1016): a `CHANGE_TO` on a node whose own component set the file
    /// carries nowhere, which is the ordinary shape of an instance of a
    /// published-library set. `interaction_diagnostics` states the argument
    /// where the split is made.
    pub const UNSUPPORTED_INTERACTION: &str = "figma.prototype.unsupported-interaction";
    /// The motion Figma declared for a variant switch does not lower: an
    /// easing with no `dashcue` spelling, a difference on a channel no
    /// transition can animate, or a second member declaring a different
    /// transition to a destination one already claimed. **Always a warning**,
    /// which is the property this rule carries and the reason the split
    /// matters — the picture is never at stake here.
    ///
    /// Usually the state change ships and lands in one frame, or with the
    /// transition that won, which is what a member with no transition has
    /// always meant. One population is a warning without a switch behind it:
    /// a curve on a switch into a set that lowers no variant table at all,
    /// where `UNLOWERABLE_SET` names the switch's own loss and this names the
    /// curve's, at the same severity, rather than dropping it (P4, debt #149).
    /// It never claims the switch lands, and `interaction_diagnostics` writes
    /// a message that says so.
    pub const UNSUPPORTED_MOTION: &str = "figma.prototype.unsupported-motion";
    /// A component set whose members differ in a way no `VariantOverride` can
    /// express, so no `VariantSet` is emitted for it. Always a warning: the
    /// document is exactly what it was before this story taught the lowering
    /// to emit variant sets at all, and the instance's baked subtree still
    /// paints.
    pub const UNLOWERABLE_SET: &str = "figma.variants.unlowerable-set";
}

/// Emits `Document.variant_sets` from the file's component sets and attaches
/// the prototype transitions that animate them, returning every diagnostic
/// the two earned.
///
/// Runs after the walk, for the same reason `bindings::apply` does: an
/// override names a document node index, so every node has to have landed
/// first. `index_of_id` is the walk's own Figma-id → document-index map.
pub(super) fn apply(
    doc: &mut Document,
    file: &FigmaFile,
    index_of_id: &IndexOfId,
    policy: crate::EmitPolicy,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let nodes = paths(file);

    // Every component this file contains, and the member ids of every set it
    // contains — both read off the file, never off what planning achieved.
    //
    // Reading resolution off `plans` conflated "no set here" with "a set I
    // could not plan", so an instance of a set refused for a corner-radius
    // difference reported that the file carried no set for it, on the line
    // below the finding that named that very set.
    let components: BTreeSet<&str> = nodes
        .iter()
        .filter(|walked| walked.node.kind == "COMPONENT")
        .filter_map(|walked| walked.node.id.as_deref())
        .collect();

    // The component sets, planned once each: the member trees, what differs
    // between them, and the transitions their own reactions declare. None of
    // that depends on an instance, which is what lets a set with no instance
    // still name what it could not lower — `refused-fill-diff` has no
    // instance and is the case every real Figma file hits, and it is why this
    // is not gated on the set's own depth either: an instance elsewhere can
    // show a member of a set that sits inside a definition, and gating the
    // naming would drop that instance's lost table in silence.
    //
    // Planned **before** any interaction is named, because a switch's motion
    // is only a degrade where the switch reaches the document, and naming it
    // first is what had the reader decide from a `destinationId`'s presence
    // instead (issue #1017).
    //
    // The member ids travel **with** each set's state rather than beside it:
    // two parallel vectors filled by two loops over the same filter is an
    // alignment nothing checks, and `landing` indexes one with a position
    // found in the other.
    let mut plans: Vec<Plan<'_>> = Vec::new();
    let mut sets: Vec<Set<'_>> = Vec::new();
    for walked in &nodes {
        if walked.node.kind != "COMPONENT_SET" {
            continue;
        }
        let members = members_of(walked.node)
            .into_iter()
            .filter_map(|member| member.id.as_deref())
            .collect();
        let at = Location::Node(NodePath::new(
            index_of(index_of_id, walked.node),
            walked.path.clone(),
        ));
        let state = match Plan::of(walked.node, &walked.path) {
            Ok(plan) => {
                diagnostics.extend(plan.diagnostics(&at));
                plans.push(plan);
                SetState::Lowers(plans.len() - 1)
            }
            // A set with fewer than two members has nothing to lose and names
            // nothing, so no other diagnostic will report a switch into it.
            Err(None) => SetState::Silent,
            Err(Some(why)) => {
                diagnostics.push(Diagnostic {
                    rule: rule::UNLOWERABLE_SET,
                    severity: Severity::Warning,
                    at,
                    message: format!(
                        "{why}, so this component set lowers no variant table; its instances \
                         still paint the member they show",
                    ),
                });
                SetState::NamedItsLoss
            }
        };
        sets.push(Set { members, state });
    }

    // Member id to set, so resolving a switch is a lookup rather than a scan
    // over every set's member list for every node in the file.
    let set_of: BTreeMap<&str, usize> = sets
        .iter()
        .enumerate()
        .flat_map(|(index, set)| set.members.iter().map(move |id| (*id, index)))
        .collect();

    // Which members something instantiates, and which layers a switch can put
    // on screen.
    //
    // A definition paints nothing, so a reaction on a master no instance shows
    // costs the picture nothing and is named nowhere — the same reasoning
    // `Walk::visit` uses when it fires no finding at all inside a definition.
    // An `INSTANCE` inside such a master does not count as showing anything
    // either (issue #1018): the walk skips that subtree whole.
    //
    // But the two answers define each other, so neither is a single pass. An
    // instance sitting inside a **member** is reachable exactly when a switch
    // to that member is, and it then shows its own set, whose members become
    // reachable in turn. Answering "what is shown" first and "what is
    // reachable" second stops one level short, and a set whose only instance
    // sits one member deep loses its members' findings in silence. So this is
    // the least fixed point: start from the instances that paint directly and
    // grow until nothing new is reachable. It terminates because `shown` only
    // ever grows — `reaches_screen` is monotone in `switchable`, `switchable`
    // is monotone in `shown` — so every round either adds a component id or is
    // the one that ends the loop, and the ids come from a finite file.
    let mut shown = BTreeSet::new();
    let mut switchable = BTreeSet::new();
    loop {
        let grown: BTreeSet<&str> = nodes
            .iter()
            .filter(|walked| walked.node.kind == "INSTANCE" && walked.reaches_screen(&switchable))
            .filter_map(|walked| walked.node.component_id.as_deref())
            .collect();
        if grown == shown {
            break;
        }
        shown = grown;
        // Every member of a set that something instantiates **and that lowers
        // a variant table**: a set whose table never lowers can switch to
        // nothing, so its members cost the picture what a master no instance
        // shows costs it — nothing.
        //
        // Two review rounds argued that second clause in opposite directions,
        // so the ruling is written down rather than left to the next one.
        // Against it: a refusal on such a member is named nowhere today and
        // appears for the first time once the set is repaired, which resembles
        // the second-compile failure debt #149 forbids. For it: debt #149 is
        // about one finding hiding behind another **on a construct that
        // ships**, and nothing here ships — no switch can reach that member
        // while its set lowers no table, so its subtree never paints. Naming
        // it would be an error under `Strict` withholding a document over a
        // layer that cannot appear, which is the cost issue #1018 exists to
        // remove. The refused-curve branch in `interaction_diagnostics` reads
        // the other way for the same reason, not against it: that curve sits
        // on a node that does paint.
        switchable = sets
            .iter()
            .filter(|set| {
                matches!(set.state, SetState::Lowers(_))
                    && set.members.iter().any(|id| shown.contains(id))
            })
            .flat_map(|set| set.members.iter().copied())
            .collect();
    }

    // Every node's interactions, named here and only here — the switches
    // included, now that the sets above say where each one lands — and every
    // instance's variant table, emitted from the same read.
    for walked in &nodes {
        // What reaches the screen, and so what is worth naming. A node outside
        // every definition paints. A node inside one reaches the screen only
        // through an instance, so it is named exactly where a switch could
        // bring it there and nothing else will: a member of an instantiated
        // set that no instance echoes. An echoed member is named on the baked
        // copy the instance carries, and a definition nothing instantiates
        // paints nowhere at all (issue #1018) — including a nested master
        // inside a member, whose own `owner` is that master and not the
        // member.
        let names_here = walked.reaches_screen(&switchable)
            && match walked.owner() {
                None => true,
                // An echoed member is named on the baked copy every instance
                // showing it carries, so naming it here as well would report
                // one authored reaction twice.
                //
                // **The captures pin that echo at a member root and nowhere
                // deeper.** Every interaction in `corpus/figma-fixtures/` sits
                // on a member `COMPONENT` root or on an `INSTANCE` root, so
                // that REST also echoes a reaction from a layer *inside* a
                // master onto the instance's copy of that layer is inference
                // from the baked subtree being the resolved content, not a
                // measured fact. If it turned out false, a refusal on such a
                // layer would be named nowhere — which is why it is written
                // here rather than left implied.
                Some(definition) => definition
                    .id
                    .as_deref()
                    .is_some_and(|id| !shown.contains(id)),
            };
        if !names_here {
            continue;
        }
        let node = walked.node;
        let read = prototype::read(node);
        let at = || {
            Location::Node(NodePath::new(
                index_of(index_of_id, node),
                walked.path.clone(),
            ))
        };
        // The variant table **before** the switches are judged, because a set
        // that plans is not yet a table that ships: `emit` refuses an instance
        // whose own geometry disagrees with the member it shows, and a switch
        // called a degrade before that answer is known would say "the switch
        // lands in one frame" beside "this instance lowers no variant table"
        // — the contradiction this pass exists to remove, one scope down from
        // the set (issue #1017).
        //
        // An instance inside a definition lowers to no document node, so it
        // can carry no table at all: `emit` would fail to find an index for
        // every baked child and name the instance unlowerable, which is a
        // finding about a layer that never ships (issue #1018). An instance of
        // a standalone `COMPONENT`, of a set this file does not carry, or of
        // one no plan could be built for has no table to emit either; its
        // switches are judged below and the set's own loss is named above.
        // Whether **this node's own** switches reach a variant table, which is
        // what separates a curve that degrades a shipped state change from one
        // that names a loss. Two nodes earn it, and the rest do not:
        //
        // - an `INSTANCE` whose table `emit` accepted, because `emit` applies
        //   that instance's own switches over the set's defaults;
        // - a **member root** of a set that lowers, because `Plan::of` folds a
        //   member's own reaction into the set's default tween table, which
        //   `emit` then copies into every instance's `VariantSet`. This is the
        //   reverse arm of a two-variant set — the ordinary authoring shape —
        //   and an earlier draft keyed the flag on "is an instance" alone and
        //   took its degrade away.
        //
        // Everything else carries no transition anywhere: `emit` applies only
        // the instance root's switches (debt #1064), and a node inside a
        // definition lowers to no document node at all.
        let mut carries_a_switch = false;
        if node.kind == "INSTANCE"
            && walked.definition.is_none()
            && let Some((plan, active)) = plan_of(&plans, &sets, &set_of, node)
        {
            match plan.emit(doc, node, active, &read.switches, index_of_id) {
                Ok(()) => carries_a_switch = true,
                Err(why) => diagnostics.push(Diagnostic {
                    rule: rule::UNLOWERABLE_SET,
                    severity: Severity::Warning,
                    at: at(),
                    message: format!(
                        "{why}, so this instance lowers no variant table; its baked subtree still \
                         paints",
                    ),
                }),
            }
        }
        if is_definition(node)
            && let Some(id) = node.id.as_deref()
            && let Some(index) = set_of.get(id)
            && matches!(sets[*index].state, SetState::Lowers(_))
        {
            carries_a_switch = true;
        }

        if !read.switches.is_empty() || !read.unsupported.is_empty() {
            diagnostics.extend(interaction_diagnostics(
                &read,
                landing(&sets, &set_of, &components, walked),
                carries_a_switch,
                &at(),
                policy,
            ));
        }
    }

    diagnostics
}

/// Where a node's `CHANGE_TO` switches can land, as the **file** answers it.
enum Landing<'a> {
    /// The set those switches travel within.
    Set(&'a Set<'a>),
    /// The node shows a component this file does not contain — the ordinary
    /// shape of an instance of a published-library set the export left out.
    LibraryAbsent,
    /// No set, and no missing library either: a plain frame, or an instance of
    /// a standalone local `COMPONENT`. A `CHANGE_TO` here resolves nowhere and
    /// never could.
    NoSet,
}

/// One component set the file carries, as resolution needs it.
struct Set<'a> {
    members: Vec<&'a str>,
    state: SetState,
}

/// What a set does with a switch that reaches it, and which diagnostic reports
/// the loss where it does nothing.
enum SetState {
    /// A plan was built, so a variant table lowers and a switch into it ships.
    /// Carries that plan's index, which is what keeps `plans` from being a
    /// second collection to be kept in step with this one.
    Lowers(usize),
    /// No plan, and the set named that loss itself under `UNLOWERABLE_SET`.
    NamedItsLoss,
    /// No plan and nothing named it: fewer than two members, so the set has no
    /// alternative state and never reports one.
    Silent,
}

/// Which set a node's switches are judged against.
///
/// The node's own `componentId` decides where it has one. Otherwise the switch
/// belongs to whatever shows the node — the nearest enclosing `INSTANCE` or
/// definition — because a `CHANGE_TO` on a layer inside a component switches
/// the variant of the instance that layer is part of. Both halves are needed:
/// the master's copy of that layer resolves through the definition, and the
/// instance's baked copy of the same layer, which REST echoes the reaction
/// onto verbatim, resolves through the instance.
fn landing<'s>(
    sets: &'s [Set<'s>],
    set_of: &BTreeMap<&str, usize>,
    components: &BTreeSet<&str>,
    walked: &Walked<'_>,
) -> Landing<'s> {
    let host = walked.switch_host();
    let shows = host.and_then(|host| host.component_id.as_deref());
    let component = shows.or_else(|| {
        host.filter(|host| is_definition(host))
            .and_then(|host| host.id.as_deref())
    });
    if let Some(index) = component.and_then(|component| set_of.get(component)) {
        return Landing::Set(&sets[*index]);
    }
    // Only a `componentId` naming a component the file does not contain is the
    // missing-library case. An instance of a standalone local `COMPONENT` is
    // not: the file carries the component, there is simply no set, so a
    // `CHANGE_TO` on it is broken rather than un-exported.
    match shows {
        Some(component) if !components.contains(component) => Landing::LibraryAbsent,
        _ => Landing::NoSet,
    }
}

/// The *plan* for the set `node` instantiates, and the member it shows — what
/// emitting a variant table needs, and `None` for a set no plan could be built
/// for. Deliberately separate from [`landing`]: whether the file carries a set
/// and whether this pass could lower it are different questions, and answering
/// the first with the second is what made an unlowerable set report itself as
/// absent.
///
/// It reaches the plan through the same `set_of` index [`landing`] uses, so
/// neither the lookup nor the member list is derived twice.
fn plan_of<'p, 'a>(
    plans: &'p [Plan<'a>],
    sets: &[Set<'_>],
    set_of: &BTreeMap<&str, usize>,
    node: &Node,
) -> Option<(&'p Plan<'a>, usize)> {
    let component = node.component_id.as_deref()?;
    let set = &sets[*set_of.get(component)?];
    let SetState::Lowers(plan) = set.state else {
        return None;
    };
    let plan = &plans[plan];
    plan.member_of(component).map(|active| (plan, active))
}

/// Whether this node is a component definition — resolved by the walk, never
/// painted, and therefore never diagnosed (story #242).
fn is_definition(node: &Node) -> bool {
    node.kind == "COMPONENT" || node.kind == "COMPONENT_SET"
}

/// A component set's members, in declaration order — the order `active_member`
/// and every `set_variant` index count in.
///
/// One definition, because two callers ask: `apply` resolves a switch against
/// it and `Plan::of` builds the member trees from it. Two copies of the rule
/// would let a switch be classified as landing while `emit` finds no member of
/// that name, with neither path naming the loss.
fn members_of(set: &Node) -> Vec<&Node> {
    set.children
        .iter()
        .filter(|child| child.kind == "COMPONENT")
        .collect()
}

/// The severity an **omission** carries under `policy`: nothing about the
/// construct reaches the document, so `Strict` withholds the bytes rather than
/// handing over a picture missing what the designer authored (R6).
fn omission_severity(policy: crate::EmitPolicy) -> Severity {
    match policy {
        crate::EmitPolicy::Strict => Severity::Error,
        crate::EmitPolicy::Partial => Severity::Warning,
    }
}

/// One node's read interactions, as diagnostics at `at`.
///
/// `landing` is where this node's switches can reach, as the file answers it.
/// It is what `prototype::read` cannot work out on its own, and it decides two
/// things the reader used to guess from a `destinationId`'s presence: whether
/// a refused curve is a degrade or part of an omission (issue #1017), and
/// which of the two omissions a switch that lands nowhere earns (issue #1016).
/// `carries_a_switch` is whether **this node's own** switches reached a
/// variant table in the document. A set that plans does not guarantee it —
/// `emit` refuses an instance whose geometry disagrees with the member it
/// shows — and neither does being inside one, because only an instance root's
/// switches are applied to the table at all (debt #1064). It is what separates
/// a curve that degrades a shipped state change from one that names a loss.
fn interaction_diagnostics(
    read: &Interactions,
    landing: Landing<'_>,
    carries_a_switch: bool,
    at: &Location,
    policy: crate::EmitPolicy,
) -> Vec<Diagnostic> {
    let by_policy = omission_severity(policy);
    let mut out: Vec<Diagnostic> = read
        .unsupported
        .iter()
        .map(|what| Diagnostic {
            rule: rule::UNSUPPORTED_INTERACTION,
            severity: by_policy,
            at: at.clone(),
            message: format!("{what} is not in the document vocabulary"),
        })
        .collect();

    for switch in &read.switches {
        let omission = match landing {
            // The destination is a member of the set this node shows, so the
            // switch is expressible — but only a set that lowers a table
            // actually ships it.
            Landing::Set(set) if set.members.contains(&switch.destination.as_str()) => {
                match set.state {
                    // It ships, so only its curve was ever at risk — unless
                    // this node is not the one that shipped it.
                    SetState::Lowers(_) if carries_a_switch => None,
                    // No switch reaches the document from here, and something
                    // else has already named why: `UNLOWERABLE_SET`, on the
                    // set or on this instance. The curve must not be called a
                    // degrade — "the switch lands in one frame" would claim a
                    // state change no table carries, which is issue #1017's
                    // defect on the neighbouring path — but it must still be
                    // named, because every finding survives one pass (debt
                    // #149) and a curve dropped here would surface for the
                    // first time on the compile after the set is repaired. It
                    // takes the warning that neighbour carries, so the whole
                    // loss stays a degrade end to end.
                    SetState::Lowers(_) | SetState::NamedItsLoss => {
                        if let Some(what) = &switch.refused_motion {
                            // The message names no vehicle. This branch fires
                            // for a set that lowers nothing and for a layer
                            // whose switches reach no table, and an earlier
                            // draft that picked between "this instance" and
                            // "this component set" put the wrong noun on both
                            // — on a member root, which is no instance, and on
                            // a baked child of an instance whose table shipped
                            // perfectly well. What is true in every case is
                            // the thing worth saying.
                            out.push(Diagnostic {
                                rule: rule::UNSUPPORTED_MOTION,
                                severity: Severity::Warning,
                                at: at.clone(),
                                message: format!(
                                    "{what}, and no variant table carries the switch it would \
                                     animate, so nothing is left for it to degrade",
                                ),
                            });
                        }
                        continue;
                    }
                    // Nothing named it: a set of fewer than two members has no
                    // alternative state, so a `CHANGE_TO` into it is an
                    // omission like any other and this is the only place that
                    // will say so.
                    SetState::Silent => Some((
                        by_policy,
                        format!(
                            "a CHANGE_TO names destination {}, whose component set has no second \
                             member to switch to, so the switch lowers nowhere",
                            switch.destination,
                        ),
                    )),
                }
            }
            // The file carries the set this node shows and the destination is
            // not one of its members: a `destinationId` the export closure
            // trimmed, or one naming a member of a different set. That is a
            // broken file, and under `Strict` handing over its bytes ships a
            // button whose click does nothing (issue #976).
            Landing::Set(_) => Some((
                by_policy,
                format!(
                    "a CHANGE_TO names destination {}, which is not a member of the component set \
                     this layer belongs to, so the switch lowers nowhere",
                    switch.destination,
                ),
            )),
            // No set, and no missing library either — a plain frame, or an
            // instance of a standalone local `COMPONENT`. There is no variant
            // to switch to and the file is present in full, so this is a
            // broken switch rather than an export that left a library out, and
            // it takes the same severity as one.
            Landing::NoSet => Some((
                by_policy,
                format!(
                    "a CHANGE_TO names destination {}, and this layer belongs to no component set, \
                     so the switch lowers nowhere",
                    switch.destination,
                ),
            )),
            // The node shows a component the file does not contain — the
            // ordinary shape of an instance of a published-library component
            // set, which every real Figma file is full of. A **warning in
            // both policies**
            // (issue #1016), on the neighbouring severity rather than on a new
            // judgement: `UNLOWERABLE_SET` is a warning in both policies for a
            // set the file *carries* and this pass cannot express, because
            // refusing would withhold a document that renders correctly. A set
            // the export never included loses the same variant table and its
            // baked subtree paints the same, so making the absent set the error
            // and the present-but-unlowerable one the warning inverts the two.
            // `figma-component-lowering.md` ("Severity") carries the argument
            // in full, including what the closure independently decided about
            // the same population.
            //
            // What stays at the policy's severity is a construct with no
            // lowering **anywhere** — a refused trigger, action or navigation
            // above. Those are vocabulary gaps whatever file carries the set;
            // this one is a set the export did not include.
            Landing::LibraryAbsent => Some((
                Severity::Warning,
                format!(
                    "a CHANGE_TO names destination {}, and this file does not contain the component \
                     whose instance this layer belongs to, so the switch lowers nowhere",
                    switch.destination,
                ),
            )),
        };

        match omission {
            None => {
                if let Some(what) = &switch.refused_motion {
                    out.push(Diagnostic {
                        rule: rule::UNSUPPORTED_MOTION,
                        severity: Severity::Warning,
                        at: at.clone(),
                        message: format!("{what}; the switch lands in one frame"),
                    });
                }
            }
            Some((earned, message)) => {
                out.push(Diagnostic {
                    rule: rule::UNSUPPORTED_INTERACTION,
                    severity: earned,
                    at: at.clone(),
                    message,
                });
                // The curve is part of that omission rather than a degrade:
                // "the switch lands in one frame" claims a state change the
                // document does not carry (issue #1017). It is still named,
                // because every finding survives one pass (debt #149).
                //
                // It takes the omission's severity rather than the policy's,
                // and the reading that says otherwise is worth answering: a
                // `DISSOLVE` has no `dashcue` spelling whatever file carries
                // the set, so it looks like a vocabulary gap that should be an
                // error under `Strict`. What makes it not one here is that
                // there is no switch for it to animate — the curve's loss
                // costs nothing beyond the omission already named. Giving it
                // the policy's severity would also make a library instance
                // withhold the document again whenever its switch declared a
                // curve, which is the whole of what issue #1016 is about.
                if let Some(what) = &switch.refused_motion {
                    out.push(Diagnostic {
                        rule: rule::UNSUPPORTED_INTERACTION,
                        severity: earned,
                        at: at.clone(),
                        message: format!("{what}, and the switch it would animate lowers nowhere"),
                    });
                }
            }
        }
    }

    out
}

/// The document index a Figma node lowered to, or `0` when it lowered to none
/// — a definition, or a node the walk skipped. The index is the advisory half
/// of a diagnostic location and the path is the stable half, the same
/// convention `Walk::unsupported_at` uses for a node that never landed.
fn index_of(index_of_id: &IndexOfId, node: &Node) -> u32 {
    node.id
        .as_ref()
        .and_then(|id| index_of_id.get(id))
        .map_or(0, |lowered| lowered.index)
}

/// One node of the file, as this pass sees it.
struct Walked<'a> {
    node: &'a Node,
    /// The diagnostic path the walk would give it.
    path: String,
    /// The nearest **ancestor** that is a component definition, if any — not
    /// this node itself, which `is_definition` answers and [`Walked::owner`]
    /// folds in.
    ///
    /// Carried because `is_definition` alone matches a definition node and
    /// none of its descendants, so everything one layer inside a master was
    /// treated as if it shipped (issue #1018). It is the node rather than a
    /// flag because whether a definition's contents can ever reach the screen
    /// is a question about *that* definition — whether an instance shows it —
    /// and a bool cannot say which one to ask about.
    definition: Option<&'a Node>,
    /// The nearest ancestor that is an `INSTANCE` **or** a definition —
    /// whichever comes first.
    ///
    /// Separate from `definition` because the two answer different questions
    /// and an `INSTANCE` splits them. Its baked children paint, so it is not a
    /// definition for `definition`'s purpose; but a `CHANGE_TO` on one of
    /// those children switches *its* variant, so it is what a switch resolves
    /// against. Figma echoes a component's reaction onto the instance
    /// verbatim, so an inner layer driving the enclosing instance's variant —
    /// an everyday authoring shape — arrives as exactly that: a reaction on a
    /// baked child with no `componentId` and no definition above it.
    host: Option<&'a Node>,
}

impl<'a> Walked<'a> {
    /// The definition this node's contents belong to: itself when it is one,
    /// otherwise the nearest ancestor that is.
    ///
    /// `None` means the node paints directly. `Some(d)` means it reaches the
    /// screen only through an instance of `d`.
    fn owner(&self) -> Option<&'a Node> {
        if is_definition(self.node) {
            Some(self.node)
        } else {
            self.definition
        }
    }

    /// Whether this node can reach the screen at all, given the members a
    /// switch can put there.
    ///
    /// Outside every definition it paints directly. Inside one it paints only
    /// where a switch reaches the definition holding it — which is what makes
    /// this and `switchable` mutually recursive, and why `apply` solves them
    /// together rather than in sequence.
    fn reaches_screen(&self, switchable: &BTreeSet<&str>) -> bool {
        match self.owner() {
            None => true,
            Some(definition) => definition
                .id
                .as_deref()
                .is_some_and(|id| switchable.contains(id)),
        }
    }

    /// The node whose component set this node's `CHANGE_TO` switches travel
    /// within: itself when it is an instance or a definition, otherwise the
    /// nearest ancestor that is either.
    fn switch_host(&self) -> Option<&'a Node> {
        if self.node.kind == "INSTANCE" || is_definition(self.node) {
            Some(self.node)
        } else {
            self.host
        }
    }
}

/// Every node of the file in the walk's own order, each with the diagnostic
/// path the walk would give it and whether it sits inside a definition.
///
/// Definitions are included, unlike the walk's traversal: a component set is
/// exactly what this module is here to read. What their descendants cost is
/// the flag above rather than their absence, because the three loops that read
/// this want different answers — the sets themselves are planned, and nothing
/// under one is named or emitted.
fn paths(file: &FigmaFile) -> Vec<Walked<'_>> {
    let roots = super::top_level_nodes(&file.document).unwrap_or_default();
    let mut out = Vec::new();
    let mut stack: Vec<Walked<'_>> = super::disambiguated_segments(&roots)
        .into_iter()
        .zip(roots)
        .map(|(segment, root)| Walked {
            node: root,
            path: format!("/{segment}"),
            definition: None,
            host: None,
        })
        .rev()
        .collect();
    while let Some(walked) = stack.pop() {
        let children: Vec<&Node> = walked.node.children.iter().collect();
        let segments = super::disambiguated_segments(&children);
        let (definition, host) = (walked.owner(), walked.switch_host());
        for (child, segment) in walked.node.children.iter().zip(segments).rev() {
            stack.push(Walked {
                node: child,
                path: format!("{}/{segment}", walked.path),
                definition,
                host,
            });
        }
        out.push(walked);
    }
    out
}

/// The four rect channels a variant transition may animate, in the order
/// their tracks are declared. Every other difference lowers as an override
/// and not as a track.
const RECT_CHANNELS: [(Prop, BindingChannel); 4] = [
    (Prop::X, BindingChannel::X),
    (Prop::Y, BindingChannel::Y),
    (Prop::Width, BindingChannel::Width),
    (Prop::Height, BindingChannel::Height),
];

/// One overridable prop, as a key for "what differs where".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prop {
    X,
    Y,
    Width,
    Height,
    Fill,
    Visible,
    Rotation,
}

impl Prop {
    /// The word a diagnostic uses for this prop.
    fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Width => "width",
            Self::Height => "height",
            Self::Fill => "fill",
            Self::Visible => "visibility",
            Self::Rotation => "rotation",
        }
    }
}

/// One node's overridable props, computed the way the walk computes them.
#[derive(Debug, Clone, PartialEq)]
struct Props<'a> {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// The node's fills **as authored**. Compared raw rather than through
    /// `solid`, so a difference no override can carry — a gradient against a
    /// solid, two `imageRef`s, a stacked list — is still seen. Seeing it is
    /// what lets it be refused by name instead of dropped in silence (P4);
    /// `Plan::of` does the refusing.
    fills: &'a [Paint],
    /// The single visible solid fill an override could carry, folded with its
    /// paint opacity exactly as `color_of` folds it. `None` for no visible
    /// fill, several of them, or a fill that is not solid.
    solid: Option<Color>,
    /// Whether a `VariantFill` can address this node's colour at all. A
    /// `TEXT` node's fill lowers into its **style**, not into a `PaintEntry`,
    /// so an override would have the commit walk paint a solid rectangle over
    /// the label's box rather than recolour a glyph.
    paints: bool,
    visible: bool,
    rotation: f32,
}

impl<'a> Props<'a> {
    /// This node's props, given its parent — which decides both the origin
    /// they are relative to and whether a solver owns their placement.
    ///
    /// The rules that decide whether a number is authored intent or solver
    /// output (P1) are **not** re-derived here: `constraints_of` and
    /// `text_sizing` are the walk's own, called directly, so a per-axis sizing
    /// rule cannot drift between the two. What is replicated is the geometry
    /// arithmetic, and `emit` checks its result against what the walk actually
    /// lowered before trusting any of it.
    fn of(node: &'a Node, parent: Option<&Node>) -> Self {
        let bbox = node.absolute_bounding_box;
        // The walk's own source for a turn, not `rotation` alone (issue
        // #878), so a member's rotation prop cannot drift from the angle the
        // walk lowered.
        let rotation = node.turn();
        let (own_w, own_h) = match node.size {
            Some(size) if rotation != 0.0 => (size.x, size.y),
            _ => bbox.map_or((0.0, 0.0), |b| (b.width, b.height)),
        };
        let (dx, dy) = super::rotated_bounds_offset(rotation, own_w, own_h);
        // Figma spells "no auto-layout" as an absent mode or `NONE`.
        let in_flow =
            parent.is_some_and(|p| p.layout_mode.as_deref().is_some_and(|mode| mode != "NONE"));
        let (x, y) = match (bbox, parent.and_then(|p| p.absolute_bounding_box), in_flow) {
            (Some(b), Some(parent), false) => (b.x - dx - parent.x, b.y - dy - parent.y),
            // A root has no parent to be relative to, and a node the solver
            // places has no authored position at all.
            _ => (0.0, 0.0),
        };

        // The walk's own per-axis sizing, including the `textAutoResize`
        // reconciliation a free-standing label needs — without it every TEXT
        // node outside auto-layout would report an extent the document does
        // not carry. The blockers this collects are the walk's to report: a
        // node carrying one never lowered, so `emit` finds no document index
        // for it and refuses the set by name.
        let mut ignored = Vec::new();
        let constraints = super::constraints_of(node, None, &mut ignored);
        let constraints = if node.kind == "TEXT" {
            super::text_sizing(node, constraints)
        } else {
            constraints
        };
        let sizing = constraints.unwrap_or_default();

        Self {
            x,
            y,
            width: if sizing.sizing_h == AxisSizing::Fixed {
                own_w
            } else {
                0.0
            },
            height: if sizing.sizing_v == AxisSizing::Fixed {
                own_h
            } else {
                0.0
            },
            fills: &node.fills,
            solid: solid_fill(node),
            paints: node.kind != "TEXT",
            visible: node.visible != Some(false),
            rotation,
        }
    }

    /// The props that differ between two members' matching nodes.
    fn diff(&self, other: &Self) -> Vec<Prop> {
        [
            (Prop::X, self.x != other.x),
            (Prop::Y, self.y != other.y),
            (Prop::Width, self.width != other.width),
            (Prop::Height, self.height != other.height),
            (Prop::Fill, self.fills != other.fills),
            (Prop::Visible, self.visible != other.visible),
            (Prop::Rotation, self.rotation != other.rotation),
        ]
        .into_iter()
        .filter_map(|(prop, differs)| differs.then_some(prop))
        .collect()
    }

    /// Why this prop's difference cannot be carried by an override, if it
    /// cannot. Only `Fill` has a reason: the other six are scalars the
    /// vocabulary always expresses.
    fn unexpressible(&self, prop: Prop) -> Option<&'static str> {
        match prop {
            Prop::Fill if !self.paints => Some(
                "their text colour, which lowers into the node's style and not into a paint \
                 entry, so a variant fill override would paint over the label rather than \
                 recolour it",
            ),
            Prop::Fill if self.solid.is_none() => Some(
                "a fill no single solid colour expresses (a gradient, an image, stacked paints, \
                 or none at all), which `VariantFill` cannot carry",
            ),
            _ => None,
        }
    }

    /// This prop's value as the override that carries it.
    ///
    /// Infallible: `Plan::of` refuses a set whose differing fill is not a
    /// single solid on a painting node, so by the time this runs `solid` is
    /// `Some` wherever `Prop::Fill` reaches it.
    fn override_value(&self, prop: Prop) -> VariantValue {
        match prop {
            Prop::X => VariantValue::X(self.x),
            Prop::Y => VariantValue::Y(self.y),
            Prop::Width => VariantValue::Width(self.width),
            Prop::Height => VariantValue::Height(self.height),
            Prop::Fill => VariantValue::Fill(
                self.solid
                    .expect("a differing fill was checked expressible in Plan::of"),
            ),
            Prop::Visible => VariantValue::Visible(self.visible),
            Prop::Rotation => VariantValue::Rotation {
                angle: self.rotation,
                // Figma turns a node about its own local origin, which is the
                // row this producer gives for every rotation
                // (`rotation-is-paint-only-and-anchored-explicitly.md`).
                anchor: (0.0, 0.0),
            },
        }
    }

    /// This prop's value as the walk lowered it, for the four rect props.
    /// `None` for the three that carry no replicated arithmetic and so have
    /// nothing to check.
    fn rect_value(&self, prop: Prop) -> Option<f32> {
        match prop {
            Prop::X => Some(self.x),
            Prop::Y => Some(self.y),
            Prop::Width => Some(self.width),
            Prop::Height => Some(self.height),
            _ => None,
        }
    }
}

/// A node's single visible solid fill, folded with its paint opacity the way
/// the walk folds it. `None` for no visible fill, several of them, or a fill
/// that is not solid.
fn solid_fill(node: &Node) -> Option<Color> {
    match super::single_visible_paint(&node.fills) {
        super::OnePaint::One(paint) if paint.kind == "SOLID" => {
            Some(super::color_of(paint.color?, paint.opacity))
        }
        _ => None,
    }
}

/// One node of a member's tree, and the parent whose box its position is
/// relative to.
///
/// The parent is carried rather than derived from the name path: a Figma name
/// may contain a slash (`Icon/Chevron` is an everyday convention), so
/// splitting the key would resolve to a node that does not exist — silently
/// reporting `(0, 0)` and, where the real offset happens to be `(0, 0)`,
/// hiding a genuine difference instead of refusing.
struct Entry<'a> {
    node: &'a Node,
    parent: Option<&'a Node>,
}

/// One component set, planned: its members, what differs between them, and
/// the transitions their own reactions declare.
struct Plan<'a> {
    /// The member `COMPONENT`s in declaration order — the order
    /// `active_member` and every `set_variant` index count in.
    members: Vec<&'a Node>,
    /// Each member's props, keyed by name path, `""` being the member root.
    /// Every member carries the same key set, checked when the plan is built.
    props: Vec<BTreeMap<String, Props<'a>>>,
    /// Which props differ anywhere in the set, by key — the union across
    /// members, which is what makes one track list serve a switch in either
    /// direction.
    differing: BTreeMap<String, Vec<Prop>>,
    /// The tween a switch **to** member `i` animates with, as the set's own
    /// reactions declare it. An instance's own reaction overrides this.
    tween: Vec<Option<(f32, Easing)>>,
    /// The members more than one declaration named as a destination with
    /// transitions that disagree, by name — the transitions `tween` above
    /// could not all carry (issue #976).
    collisions: Vec<String>,
}

impl<'a> Plan<'a> {
    /// Plans one component set, or says why it cannot be lowered — `None`
    /// where there is nothing to say, because nothing was lost.
    fn of(set: &'a Node, path: &str) -> Result<Self, Option<String>> {
        let _ = path;
        let members = members_of(set);
        // A set with one member has no alternative state, so it emits no
        // variant table and names nothing: there is no switch to lose, which
        // is the same reason an instance of a standalone `COMPONENT` is
        // passed over in silence. Naming it would put a diagnostic on every
        // synthetic single-variant set in the tree for a loss that is not one.
        if members.len() < 2 {
            return Err(None);
        }

        let trees: Vec<BTreeMap<String, Entry<'a>>> = members
            .iter()
            .map(|member| tree_of(member))
            .collect::<Result<_, _>>()
            .map_err(Some)?;

        // Every member must carry the same nodes. A member with a child the
        // others do not have is Figma's "topology change", and no override
        // adds or removes a node — `VariantVisible` can hide one that exists,
        // never conjure one that does not.
        let keys: BTreeSet<&String> = trees[0].keys().collect();
        for (member, tree) in members.iter().zip(&trees).skip(1) {
            let other: BTreeSet<&String> = tree.keys().collect();
            if let Some(missing) = keys.symmetric_difference(&other).next() {
                return Err(Some(format!(
                    "member \"{}\" and member \"{}\" differ in their child nodes (\"{}\" is in one \
                     and not the other), which no variant override can express",
                    members[0].name,
                    member.name,
                    show(missing),
                )));
            }
        }

        // Everything a variant override cannot carry must be equal. The
        // comparison destructures `rest::Node`, so this stays exhaustive
        // against the REST subset the walk reads as that subset grows.
        for key in &keys {
            let first = trees[0][*key].node;
            for (member, tree) in members.iter().zip(&trees).skip(1) {
                if let Some(field) = differs_beyond_overrides(first, tree[*key].node) {
                    return Err(Some(format!(
                        "members \"{}\" and \"{}\" differ in {field} on \"{}\", which no variant \
                         override can express",
                        members[0].name,
                        member.name,
                        show(key),
                    )));
                }
            }
        }

        let props: Vec<BTreeMap<String, Props<'a>>> = trees
            .iter()
            .map(|tree| {
                tree.iter()
                    .map(|(key, entry)| (key.clone(), Props::of(entry.node, entry.parent)))
                    .collect()
            })
            .collect();

        let mut differing: BTreeMap<String, Vec<Prop>> = BTreeMap::new();
        for key in &keys {
            let mut union: BTreeSet<Prop> = BTreeSet::new();
            for other in props.iter().skip(1) {
                union.extend(props[0][*key].diff(&other[*key]));
            }
            // A difference an override cannot carry refuses the set rather
            // than lowering the props around it: half a switch is a picture
            // the designer never authored, which is what P4 forbids more
            // firmly than it forbids an omission.
            for prop in &union {
                for (member, member_props) in members.iter().zip(&props) {
                    if let Some(why) = member_props[*key].unexpressible(*prop) {
                        return Err(Some(format!(
                            "members \"{}\" and \"{}\" differ in {why} on \"{}\"",
                            members[0].name,
                            member.name,
                            show(key),
                        )));
                    }
                }
            }
            if !union.is_empty() {
                differing.insert((*key).clone(), union.into_iter().collect());
            }
        }

        // The set's own default transitions: a member's reaction names the
        // member it changes to, and that destination is the key the schema
        // stores the transition under. Collected per destination first,
        // because two members may name the same one.
        // Only one declaration per destination can be kept, so each entry
        // carries the tween that survived and whether anything it displaced
        // disagreed with it. Two members declaring the *same* transition lose
        // nothing and name nothing.
        let reactions: Vec<Interactions> = members.iter().map(|m| prototype::read(m)).collect();
        let mut declared: BTreeMap<usize, (Option<(f32, Easing)>, bool)> = BTreeMap::new();
        for read in &reactions {
            for switch in &read.switches {
                if let Some(destination) = position_of(&members, &switch.destination) {
                    let entry = declared.entry(destination).or_insert((switch.tween, false));
                    entry.1 |= entry.0 != switch.tween;
                    entry.0 = switch.tween;
                }
            }
        }
        // The later declaration wins, which is the direction `emit` already
        // takes for an instance's own reaction over the set's. What is named
        // is that a declaration was displaced at all, never which one
        // survived: `emit` applies the instance's own echoed reaction over
        // this table afterwards, so the transition an instance ships is not
        // this member order's answer (P4, issue #976).
        let mut tween = vec![None; members.len()];
        let mut collisions = Vec::new();
        for (destination, (kept, contended)) in declared {
            if contended {
                collisions.push(members[destination].name.clone());
            }
            tween[destination] = kept;
        }

        Ok(Self {
            members,
            props,
            differing,
            tween,
            collisions,
        })
    }

    /// The member index a Figma component id names, if this set holds it.
    fn member_of(&self, id: &str) -> Option<usize> {
        position_of(&self.members, id)
    }

    /// Whether any switch into this set animates at all. A set whose members
    /// declare no transition has no motion to lose, so it names none.
    fn animates(&self) -> bool {
        self.tween.iter().any(Option::is_some)
    }

    /// What this set is responsible for naming: the props that differ on a
    /// channel no transition can animate, and a destination two members
    /// declared different transitions to.
    ///
    /// **Its members' own reactions are not named here.** `apply` names every
    /// node of the file against one rule — a node inside a definition is named
    /// where a switch could bring it on screen and nothing else will — and
    /// that rule covers a member root and every layer under it alike. Naming
    /// them here as well needed this method to re-walk the member subtrees
    /// with resolution rules of its own, which is a second derivation of facts
    /// `apply` already has: it judged a nested instance of *another* set
    /// against this one's members, descended into nested masters `apply`
    /// skips, and located every finding at the set rather than at the node.
    fn diagnostics(&self, at: &Location) -> Vec<Diagnostic> {
        let mut out = Vec::new();

        if !self.animates() {
            return out;
        }
        let mut named: BTreeSet<Prop> = BTreeSet::new();
        let mut rect = false;
        for props in self.differing.values() {
            for prop in props {
                if RECT_CHANNELS.iter().any(|(p, _)| p == prop) {
                    rect = true;
                } else {
                    named.insert(*prop);
                }
            }
        }
        out.extend(named.into_iter().map(|prop| Diagnostic {
            rule: rule::UNSUPPORTED_MOTION,
            severity: Severity::Warning,
            at: at.clone(),
            message: format!(
                "the members differ in {}, which Smart Animate interpolates and a variant \
                 transition cannot: only the four rect channels have a seam a runtime can write \
                 over (issue #891). The override ships, so the switch changes it in one frame",
                prop.name(),
            ),
        }));
        if !rect {
            out.push(Diagnostic {
                rule: rule::UNSUPPORTED_MOTION,
                severity: Severity::Warning,
                at: at.clone(),
                message: "the members differ on no rect channel, so the declared transition has \
                          nothing to animate and the switch lands in one frame"
                    .to_string(),
            });
            // Nothing this set declares reaches the document, so a contended
            // destination has lost nothing beyond what the line above already
            // says. Naming both would have the set report that a transition
            // ships and that none does (issue #976).
            return out;
        }

        // The document carries one transition per destination, so where two
        // members declared a different one to the same member, one of them
        // does not lower. Naming it is what keeps that loss from riding
        // silently on the order Figma happens to list the members in.
        //
        // Which one survives is deliberately not stated: `emit` applies the
        // instance's own echoed reaction over this table, so an instance
        // sitting on the *earlier* member ships that member's transition
        // rather than the one this table kept (issue #976).
        out.extend(self.collisions.iter().map(|member| Diagnostic {
            rule: rule::UNSUPPORTED_MOTION,
            severity: Severity::Warning,
            at: at.clone(),
            message: format!(
                "more than one member declares a CHANGE_TO to \"{member}\" with a different \
                 transition, and the document carries one transition per destination, so only \
                 one of them lowers",
            ),
        }));
        out
    }

    /// Emits this set's `VariantSet` for one instance, or names why it cannot
    /// be expressed against that instance's baked subtree.
    fn emit(
        &self,
        doc: &mut Document,
        instance: &Node,
        active: usize,
        own: &[Switch],
        index_of_id: &IndexOfId,
    ) -> Result<(), String> {
        // The instance's baked subtree, joined to the members by name.
        let baked = tree_of(instance)?;
        let mut index: BTreeMap<&String, u32> = BTreeMap::new();
        for key in self.props[active].keys() {
            let entry = baked.get(key).ok_or_else(|| {
                format!(
                    "the instance has no \"{}\" to match member \"{}\"",
                    show(key),
                    self.members[active].name,
                )
            })?;
            let lowered = entry
                .node
                .id
                .as_ref()
                .and_then(|id| index_of_id.get(id))
                .ok_or_else(|| {
                    format!(
                        "the instance's \"{}\" lowered to no document node",
                        show(key),
                    )
                })?;
            index.insert(key, lowered.index);
        }

        // The safety net under the geometry arithmetic `Props::of`
        // replicates, applied to **exactly the props an override will carry**
        // and no others. A prop the members agree on gets no override and no
        // track, so its base is never read and a disagreement there says
        // nothing — checking it anyway is what would refuse every instance
        // inside an auto-layout parent, whose own extent is its parent's to
        // decide and never matches a member root sitting on the canvas.
        //
        // Where a prop *is* overridden, the active member is the one whose
        // value the document already carries, so a disagreement means either
        // the replication is wrong or the instance carries its own override —
        // and an override computed against a base that is not the document's
        // would animate to the wrong picture.
        for (key, props) in &self.differing {
            let node = &doc.nodes[index[key] as usize];
            for prop in props {
                let Some(authored) = self.props[active][key].rect_value(*prop) else {
                    continue;
                };
                let lowered = match prop {
                    Prop::X => node.box2d.x,
                    Prop::Y => node.box2d.y,
                    Prop::Width => node.box2d.width,
                    Prop::Height => node.box2d.height,
                    _ => continue,
                };
                if lowered != authored {
                    return Err(format!(
                        "the instance's \"{}\" lowered {} {lowered}, and the member it shows \
                         (\"{}\") authors {authored} — an instance-level override the variant \
                         table cannot express",
                        show(key),
                        prop.name(),
                        self.members[active].name,
                    ));
                }
            }
        }

        // A track per differing rect channel, in key then channel order, so
        // one input yields one track list (R7). The list is the union over
        // the set rather than one member's own overrides: a switch back to
        // the active member animates exactly the props the others override,
        // and the active member overrides nothing.
        let mut tracks = Vec::new();
        for (key, props) in &self.differing {
            for (prop, channel) in RECT_CHANNELS {
                if props.contains(&prop) {
                    tracks.push((index[key], channel));
                }
            }
        }

        // The instance's own reactions override the set's defaults, member by
        // member, rather than replacing the whole table — which is the shape
        // story #771 predicted when it keyed the transition on the
        // destination.
        let mut tween = self.tween.clone();
        for switch in own {
            if let Some(destination) = self.member_of(&switch.destination) {
                tween[destination] = switch.tween;
            }
        }

        let mut members = Vec::with_capacity(self.members.len());
        for (position, member) in self.members.iter().enumerate() {
            // The active member is the document's own state, so it overrides
            // nothing: switching back to it restores the base, including any
            // instance-level value the base carries.
            let mut overrides = Vec::new();
            if position != active {
                for (key, props) in &self.differing {
                    let own = self.props[active][key].diff(&self.props[position][key]);
                    for prop in props.iter().filter(|prop| own.contains(prop)) {
                        overrides.push(VariantOverride {
                            node: index[key],
                            value: self.props[position][key].override_value(*prop),
                        });
                    }
                }
            }
            members.push(VariantMember {
                name: Some(member.name.clone()),
                overrides,
                // A transition with no track animates nothing, so it is not
                // written at all — the member then reads exactly as a
                // pre-v0.18 one, which is what "lands in one frame" means.
                transition: tween[position].filter(|_| !tracks.is_empty()).map(
                    |(duration, easing)| VariantTransition {
                        tracks: tracks
                            .iter()
                            .map(|(node, channel)| PropTransition {
                                node: *node,
                                channel: *channel,
                                spec: TransitionSpec::Tween { duration, easing },
                            })
                            .collect(),
                        // Figma has no stagger, so this producer always
                        // writes zero.
                        stagger: 0.0,
                    },
                ),
            });
        }

        doc.variant_sets.push(VariantSet {
            members,
            active_member: u32::try_from(active).expect("a member index fits u32"),
        });
        Ok(())
    }
}

/// The member index a Figma component id names, within `members`.
fn position_of(members: &[&Node], id: &str) -> Option<usize> {
    members.iter().position(|m| m.id.as_deref() == Some(id))
}

/// A name path as a diagnostic writes it — the root reads as `/`.
fn show(key: &str) -> &str {
    if key.is_empty() { "/" } else { key }
}

/// One member's (or instance's) subtree, keyed by the name path relative to
/// its root — `""` for the root itself, `"bar"` for a child, `"panel/label"`
/// for a grandchild — each with the parent its box is relative to.
///
/// Names are the join key across members and instances, so two siblings
/// sharing one make the set unlowerable rather than silently binding an
/// override to whichever came first.
fn tree_of(root: &Node) -> Result<BTreeMap<String, Entry<'_>>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![(root, None, String::new())];
    while let Some((node, parent, key)) = stack.pop() {
        if out.insert(key.clone(), Entry { node, parent }).is_some() {
            return Err(format!(
                "two nodes under \"{}\" share the name path \"{key}\", so an override cannot name \
                 one of them",
                root.name,
            ));
        }
        for child in &node.children {
            let child_key = if key.is_empty() {
                child.name.clone()
            } else {
                format!("{key}/{}", child.name)
            };
            stack.push((child, Some(node), child_key));
        }
    }
    Ok(out)
}

/// The first field in which two members' matching nodes differ **beyond**
/// what a `VariantOverride` can express, named for the diagnostic.
///
/// Both sides are destructured rather than field-matched, so a field added to
/// `rest::Node` later fails to compile here until it is classified as
/// overridable or compared. That is the guarantee the mirrored-vocabulary
/// enum maps in `dashlang` get from exhaustive matches, and it is why this is
/// written the long way. The comparisons then run as a short-circuiting
/// sequence rather than as one array: `Vec<Paint>`, `Vec<Effect>` and
/// `serde_json::Map` are not cheap, and this runs once per node per member
/// pair.
fn differs_beyond_overrides(a: &Node, b: &Node) -> Option<&'static str> {
    // The ten fields deliberately not compared, and why:
    //
    // `id` and `name` are identity — the members are joined *by* name, and
    // their ids differ by construction. `children` are compared through the
    // name-path key set and this function's own per-key application, never
    // here, or a child's overridable prop would count as a structural
    // difference. `absolute_bounding_box`, `size`, `fills`, `visible` and
    // `rotation` are exactly the inputs to `Props`, which is the overridable
    // half. `interactions` and `component_id` are the prototype layer, which
    // differs between members by design.
    //
    // `relative_transform` is the one field split across both halves, and
    // issue #1019 is why it is compared below for one component only:
    //
    // - its **turn** is a `Props` input, through `Node::turn`;
    // - its **translation** and its **scale magnitude** reach `Props` through
    //   `absolute_bounding_box`, which `Props::of` derives `x`/`y`/`width`/
    //   `height` from and which `Props::diff` then compares. Not the field
    //   itself — it is bound `_` here — and not on every axis: an in-flow
    //   child's `x`/`y` are zero because the solver owns them, and an extent
    //   is zero on any axis that is not `Fixed`, which is exactly where a
    //   translation or a scale would not be authored intent to begin with
    //   (P1). Measured rather than assumed: two members whose matrices scale
    //   by 1 and by 2 with the bounding box following — the payload Figma
    //   actually sends, because a scale bakes into the box — lower today as
    //   `Width`/`Height` overrides and draw correctly. Comparing the magnitude
    //   here would refuse a set that renders, which is the capability
    //   regression `figma-component-lowering.md` declines to trade a working
    //   file for;
    // - a **mirrored** matrix is carried by nothing at all. `matrix_turn`
    //   reads a negative determinant as `0.0` deliberately, a flip leaves an
    //   axis-aligned bounding box unchanged, and no `VariantValue` spells a
    //   mirror — so two members differing by a flip lowered as identical and
    //   the flip went out in silence, which is what P4 forbids. Because that
    //   `0.0` also discards the mirrored node's *angle*, a mirror makes the
    //   whole linear part uncarried rather than one bit of it.
    //
    // Skew is the remaining component and is unreachable from this producer:
    // Figma cannot author one
    // (`rotation-is-paint-only-and-anchored-explicitly.md`).
    let Node {
        id: _,
        name: _,
        kind,
        children: _,
        fills: _,
        strokes,
        fill_geometry,
        stroke_geometry,
        effects,
        stroke_weight,
        stroke_align,
        complex_stroke_properties,
        stroke_dashes,
        corner_radius,
        rectangle_corner_radii,
        corner_smoothing,
        arc_data,
        clips_content,
        blend_mode,
        opacity,
        visible: _,
        layout_mode,
        layout_wrap,
        item_spacing,
        padding_left,
        padding_right,
        padding_top,
        padding_bottom,
        primary_axis_align_items,
        counter_axis_align_items,
        counter_axis_spacing,
        counter_axis_align_content,
        layout_sizing_horizontal,
        layout_sizing_vertical,
        min_width,
        max_width,
        min_height,
        max_height,
        grid_row_gap,
        grid_column_gap,
        grid_columns_sizing,
        grid_rows_sizing,
        grid_row_anchor_index,
        grid_column_anchor_index,
        grid_row_span,
        grid_column_span,
        layout_positioning,
        strokes_included_in_layout,
        item_reverse_z_index,
        absolute_bounding_box: _,
        rotation: _,
        relative_transform,
        size: _,
        is_mask,
        mask_type,
        section_contents_hidden,
        characters,
        style,
        style_override_table,
        component_id: _,
        interactions: _,
    } = a;

    macro_rules! differs {
        ($what:literal, $field:expr, $other:expr) => {
            if *$field != $other {
                return Some($what);
            }
        };
    }

    differs!("node type", kind, b.kind);
    // The part of the matrix nothing downstream can see.
    //
    // Where **both** matrices carry their angle, nothing here needs
    // comparing: the angle reaches `Props` through `Node::turn`, and the scale
    // reaches it through the box. Where either does not — a mirror, or a
    // collapsed matrix — `matrix_turn` returns `0.0` for it, so not even the
    // angle survives and the *orientation*, the handedness together with the
    // angle, is carried by nothing.
    //
    // A single handedness bit is not enough: a flip about x and a flip about y
    // are both mirrors, and the half-turn between them would ship in silence.
    // The whole linear part is too much: a mirrored member scaled against
    // another mirrored member differs there, and that difference *is* carried,
    // because the scale bakes into the bounding box exactly as it does for an
    // unmirrored pair. So the comparison divides the magnitude out and keeps
    // what is left.
    //
    // An absent matrix is the identity, which does not mirror, so the two
    // spellings of "upright" compare equal rather than making a set
    // unlowerable over a field Figma omits.
    if !(carries_its_angle(*relative_transform) && carries_its_angle(b.relative_transform))
        && !same_orientation(*relative_transform, b.relative_transform)
    {
        // A matrix with no area is not a mirror, and saying so would name the
        // wrong difference. `Handedness` already tells the two apart.
        return Some(
            match (
                has_area(*relative_transform),
                has_area(b.relative_transform),
            ) {
                (true, true) => "their mirroring",
                _ => "whether their transform has any area at all",
            },
        );
    }
    differs!("their strokes", strokes, b.strokes);
    // Geometry counts only on a `VECTOR`, which is the only kind the walk
    // reads it for. On a frame Figma emits the rendered outline of the node's
    // own box — `bar` at 64 wide and `bar` at 288 wide carry different path
    // strings for no reason but their width — so comparing it anywhere else
    // would count a *result* (P1) as a structural difference and make every
    // rect-differing set unlowerable. On a vector the geometry is the
    // authored shape, and no override swaps one baked distance field for
    // another.
    if kind == "VECTOR" {
        differs!("their vector geometry", fill_geometry, b.fill_geometry);
        differs!(
            "their vector stroke geometry",
            stroke_geometry,
            b.stroke_geometry
        );
    }
    differs!("their effects", effects, b.effects);
    differs!("stroke weight", stroke_weight, b.stroke_weight);
    differs!("stroke alignment", stroke_align, b.stroke_align);
    differs!(
        "stroke type",
        complex_stroke_properties,
        b.complex_stroke_properties
    );
    differs!("their stroke dashes", stroke_dashes, b.stroke_dashes);
    differs!("corner radius", corner_radius, b.corner_radius);
    differs!(
        "corner radius",
        rectangle_corner_radii,
        b.rectangle_corner_radii
    );
    differs!("corner smoothing", corner_smoothing, b.corner_smoothing);
    differs!("their arc parameters", arc_data, b.arc_data);
    differs!("clip", clips_content, b.clips_content);
    differs!("blend mode", blend_mode, b.blend_mode);
    differs!("opacity", opacity, b.opacity);
    differs!("auto-layout mode", layout_mode, b.layout_mode);
    differs!("wrap mode", layout_wrap, b.layout_wrap);
    differs!("gap", item_spacing, b.item_spacing);
    differs!("padding", padding_left, b.padding_left);
    differs!("padding", padding_right, b.padding_right);
    differs!("padding", padding_top, b.padding_top);
    differs!("padding", padding_bottom, b.padding_bottom);
    differs!(
        "main-axis alignment",
        primary_axis_align_items,
        b.primary_axis_align_items
    );
    differs!(
        "cross-axis alignment",
        counter_axis_align_items,
        b.counter_axis_align_items
    );
    differs!(
        "wrap line gap",
        counter_axis_spacing,
        b.counter_axis_spacing
    );
    differs!(
        "wrap line distribution",
        counter_axis_align_content,
        b.counter_axis_align_content
    );
    differs!(
        "horizontal sizing",
        layout_sizing_horizontal,
        b.layout_sizing_horizontal
    );
    differs!(
        "vertical sizing",
        layout_sizing_vertical,
        b.layout_sizing_vertical
    );
    differs!("minimum width", min_width, b.min_width);
    differs!("maximum width", max_width, b.max_width);
    differs!("minimum height", min_height, b.min_height);
    differs!("maximum height", max_height, b.max_height);
    differs!("grid row gap", grid_row_gap, b.grid_row_gap);
    differs!("grid column gap", grid_column_gap, b.grid_column_gap);
    differs!(
        "grid column tracks",
        grid_columns_sizing,
        b.grid_columns_sizing
    );
    differs!("grid row tracks", grid_rows_sizing, b.grid_rows_sizing);
    differs!(
        "grid placement",
        grid_row_anchor_index,
        b.grid_row_anchor_index
    );
    differs!(
        "grid placement",
        grid_column_anchor_index,
        b.grid_column_anchor_index
    );
    differs!("grid span", grid_row_span, b.grid_row_span);
    differs!("grid span", grid_column_span, b.grid_column_span);
    differs!(
        "layout positioning",
        layout_positioning,
        b.layout_positioning
    );
    differs!(
        "strokes in layout",
        strokes_included_in_layout,
        b.strokes_included_in_layout
    );
    differs!(
        "child paint order",
        item_reverse_z_index,
        b.item_reverse_z_index
    );
    differs!("mask membership", is_mask, b.is_mask);
    differs!("mask type", mask_type, b.mask_type);
    differs!(
        "hidden section contents",
        section_contents_hidden,
        b.section_contents_hidden
    );
    differs!("their text", characters, b.characters);
    differs!("their text style", style, b.style);
    differs!(
        "their per-character styles",
        style_override_table,
        b.style_override_table
    );

    None
}
