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

    // Every node's interactions, read **once for the whole pass** (debt #1066).
    // Planning a set and naming a node both need them, and reading them twice
    // walked the same `interactions` array and allocated the same refusal
    // strings twice over.
    //
    // Not fewer reads — more. The gathering pass below needs every node, where
    // the naming loop read only the named ones; what this removes is the
    // *second* read of a node, not the first. It is affordable because a node
    // with no reactions reads into two empty `Vec`s, which allocate nothing, so
    // what the pass carries for the rest of the file is one empty struct per
    // node.
    //
    // Indexed by position in `nodes` rather than keyed on a node id, because
    // `rest::Node::id` is optional and a node without one would silently lose
    // its switches from whichever half used the key.
    let reads: Vec<Interactions> = nodes
        .iter()
        .map(|walked| prototype::read(walked.node))
        .collect();

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
    // alignment nothing checks, and resolution indexes one with a position
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
        let at = located(index_of_id, walked);
        // Neither half is named here. A set's findings need the switches that
        // resolved onto its members, which the gathering pass below answers, so
        // both halves are emitted together further down — in set order, which
        // is what keeps two sets' findings from interleaving by which half of
        // this pass produced them first.
        let state = match Plan::of(walked.node, &walked.path) {
            Ok(plan) => {
                plans.push(plan);
                SetState::Lowers(plans.len() - 1)
            }
            // A set with fewer than two members has nothing to lose and names
            // nothing, so no other diagnostic will report a switch into it.
            Err(None) => SetState::Silent,
            Err(Some(why)) => SetState::NamedItsLoss(why),
        };
        sets.push(Set { members, state, at });
    }

    // Member id to set, so resolving a switch is a lookup rather than a scan
    // over every set's member list for every node in the file.
    let set_of: BTreeMap<&str, usize> = sets
        .iter()
        .enumerate()
        .flat_map(|(index, set)| set.members.iter().map(move |id| (*id, index)))
        .collect();

    // Every `CHANGE_TO` in the file, gathered onto the host whose variant table
    // carries its transition (debt #1064). The destination decides the set and
    // the host chain decides who switches it, which is `hosting`'s rule; this
    // is the same rule read from the other end, once, so that `emit` is handed
    // a resolved table rather than re-deriving one.
    //
    // A switch travelling through a **member root** joins its set's default
    // table, which `emit` copies into every instance of that set. One
    // travelling through an **instance** overrides that default for that
    // instance alone. Before this, only an instance root's own reactions were
    // applied, so a `CHANGE_TO` on any deeper layer contributed no transition
    // at all and its tween was dropped in silence — the everyday shape of an
    // inner layer driving the enclosing instance's variant.
    //
    // Nothing here is gated on reachability. A set that no instance shows emits
    // no table, so gathering its switches costs a lookup and decides nothing,
    // and gating it would tie the table's contents to the fixed point below for
    // no gain.
    let mut set_default: BTreeMap<usize, Vec<&Switch>> = BTreeMap::new();
    let mut instance_own: BTreeMap<usize, Vec<&Switch>> = BTreeMap::new();
    // Whether **anything** that reaches a set declares a transition into it, at
    // either scope. A set's own finding needs this rather than the default
    // table alone: a transition declared only by a baked layer inside one
    // instance still animates that instance's switch, and reading `set_default`
    // for the answer left a set that differs on no rect channel silent while
    // dropping the tween — a silent drop (P4) whose population the widening
    // above enlarges from the instance root to every layer under it.
    let mut animates = vec![false; sets.len()];
    for (at, read) in reads.iter().enumerate() {
        for switch in &read.switches {
            let Some(index) = set_of.get(switch.destination.as_str()) else {
                continue;
            };
            let Hosting::LandsAt(host) = hosting(&nodes, at, &sets[*index]) else {
                continue;
            };
            animates[*index] |= switch.tween.is_some();
            if is_definition(nodes[host].node) {
                set_default.entry(*index).or_default().push(switch);
            } else {
                instance_own.entry(host).or_default().push(switch);
            }
        }
    }

    // Each set's default table and the destinations its members contended over.
    let defaults: Vec<DeclaredTweens> = sets
        .iter()
        .enumerate()
        .map(|(index, set)| match set.state {
            SetState::Lowers(plan) => declared_tweens(
                &plans[plan].members,
                set_default.get(&index).map_or(&[][..], Vec::as_slice),
            ),
            _ => (BTreeMap::new(), Vec::new()),
        })
        .collect();

    // Every set's own findings, in set order.
    for (index, set) in sets.iter().enumerate() {
        match &set.state {
            SetState::Lowers(plan) => {
                let (_, collisions) = &defaults[index];
                diagnostics.extend(plans[*plan].diagnostics(&set.at, animates[index], collisions));
            }
            SetState::NamedItsLoss(why) => diagnostics.push(Diagnostic {
                rule: rule::UNLOWERABLE_SET,
                severity: Severity::Warning,
                at: set.at.clone(),
                message: format!(
                    "{why}, so this component set lowers no variant table; its instances still \
                     paint the member they show",
                ),
            }),
            SetState::Silent => {}
        }
    }

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
    //
    // It is solved by a **worklist** rather than by rescanning (debt #1066).
    // Re-testing every node and rebuilding both sets from scratch each round
    // cost a file whose instantiation chain is C levels deep C+1 full scans and
    // C+1 fresh allocations, where each instance need only be visited when the
    // definition holding it first becomes reachable. C is 1 for almost every
    // real file, so this was never slow; the shape was what invited growth.
    //
    // The instances that paint directly seed it, and each member that becomes
    // switchable enqueues the instances inside it. An instance whose holding
    // definition carries no id can never be switched to, so it is seeded
    // nowhere — which is what `reaches_screen` says by answering `false` for it.
    let mut direct: Vec<usize> = Vec::new();
    let mut inside: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (at, walked) in nodes.iter().enumerate() {
        if walked.node.kind != "INSTANCE" {
            continue;
        }
        match walked.definition {
            None => direct.push(at),
            Some(definition) => {
                if let Some(id) = definition.id.as_deref() {
                    inside.entry(id).or_default().push(at);
                }
            }
        }
    }

    let mut shown: BTreeSet<&str> = BTreeSet::new();
    let mut switchable: BTreeSet<&str> = BTreeSet::new();
    let mut queue = direct;
    while let Some(at) = queue.pop() {
        let Some(component) = nodes[at].node.component_id.as_deref() else {
            continue;
        };
        if !shown.insert(component) {
            continue;
        }
        let Some(index) = set_of.get(component) else {
            continue;
        };
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
        if !matches!(sets[*index].state, SetState::Lowers(_)) {
            continue;
        }
        for member in &sets[*index].members {
            if switchable.insert(member) {
                queue.extend(inside.get(member).into_iter().flatten().copied());
            }
        }
    }

    // Every node's interactions, named here and only here — the switches
    // included, now that the sets above say where each one lands — and every
    // instance's variant table, emitted from the same read.
    //
    // The hosts whose variant table lowered, as positions in `nodes`: the
    // answer a switch needs about the host it travels through.
    let mut carrying: BTreeSet<usize> = BTreeSet::new();
    // One authored reaction's findings, by the layer it was authored on: where
    // the first copy landed in `diagnostics`, and how many copies there were
    // (debt #1056).
    let mut echoed: Echoed = BTreeMap::new();

    // **Emit first, name second** (debt #1141). Whether a *member root* carries
    // a transition is not a fact about its set having a plan — it is whether any
    // instance of that set actually shipped a `VariantSet`, and `emit` refuses
    // an instance whose baked geometry disagrees with the member it shows. A set
    // every instance of which was refused, or that no instance shows at all,
    // lowers no table anywhere, and calling its members' curves degrades claimed
    // a state change the document does not carry — issue #1017's defect on a
    // path that issue did not reach.
    //
    // That answer needs every instance walked, so it cannot be reached in the
    // naming loop, which meets member roots before the instances that show them.
    // The findings this pass produces are held per node and replayed in the
    // naming loop instead of being pushed here, so the diagnostic order is
    // exactly what one interleaved loop produced.
    let mut emitted: BTreeSet<usize> = BTreeSet::new();
    let mut pending: BTreeMap<usize, Vec<Pending<'_>>> = BTreeMap::new();
    for (at, walked) in nodes.iter().enumerate() {
        let node = walked.node;
        // The variant table **before** the switches are judged, because a set
        // that plans is not yet a table that ships: a switch called a degrade
        // before that answer is known would say "the switch lands in one frame"
        // beside "this instance lowers no variant table" — the contradiction
        // this pass exists to remove, one scope down from the set (issue #1017).
        //
        // An instance inside a definition lowers to no document node, so it can
        // carry no table at all: `emit` would fail to find an index for every
        // baked child and name the instance unlowerable, which is a finding
        // about a layer that never ships (issue #1018). An instance of a
        // standalone `COMPONENT`, of a set this file does not carry, or of one
        // no plan could be built for has no table to emit either; its switches
        // are judged below and the set's own loss is named above.
        if node.kind != "INSTANCE" || walked.definition.is_some() {
            continue;
        }
        let Some((plan, index, active)) = plan_of(&plans, &sets, &set_of, node) else {
            continue;
        };
        let location = || located(index_of_id, walked);
        // The set's default table, overridden by the switches that travel
        // through **this** instance — its own root's, and every one on a baked
        // layer below it whose destination is a member of this set (debt #1064).
        let (own, contended) = declared_tweens(
            &plan.members,
            instance_own.get(&at).map_or(&[][..], Vec::as_slice),
        );
        let mut tween = vec![None; plan.members.len()];
        for (destination, kept) in &defaults[index].0 {
            tween[*destination] = *kept;
        }
        for (destination, kept) in own {
            tween[destination] = kept;
        }
        match plan.emit(doc, node, active, &tween, index_of_id) {
            Ok(()) => {
                carrying.insert(at);
                emitted.insert(index);
                // Two layers of one instance declaring different transitions to
                // the same destination lose one of them, for the reason a set's
                // two members do: the document carries one transition per
                // destination. Widening what reaches this table is what created
                // the case, so naming it is what keeps the widening from being a
                // silent loss (P4, issue #976). Gated on the table having a
                // track, because a transition with none is not written at all
                // and the set has already said so.
                //
                // It goes through the same collapse every other echoed finding
                // does (debt #1056), keyed on the **member this instance shows**
                // rather than on the instance: the layers that disagree are the
                // master's, so every instance of that member echoes the same
                // contention, and reporting it once per instance would re-create
                // inside one pass the multiplicity that collapse exists to
                // remove.
                if plan.animatable() {
                    let source = plan.members[active].id.as_deref().unwrap_or(&walked.path);
                    let bucket = pending.entry(at).or_default();
                    for member in contended.iter().map(|m| plan.members[*m].name.as_str()) {
                        bucket.push(Pending {
                            echo: Some(source),
                            diagnostic: Diagnostic {
                                rule: rule::UNSUPPORTED_MOTION,
                                severity: Severity::Warning,
                                at: location(),
                                message: format!(
                                    "more than one layer of this instance declares a CHANGE_TO to \
                                     \"{member}\" with a different transition, and the document \
                                     carries one transition per destination, so only one of them \
                                     lowers",
                                ),
                            },
                        });
                    }
                }
            }
            // Per instance, and deliberately not collapsed: each refused
            // instance lost its own table, so the count is the finding.
            Err(why) => pending.entry(at).or_default().push(Pending {
                echo: None,
                diagnostic: Diagnostic {
                    rule: rule::UNLOWERABLE_SET,
                    severity: Severity::Warning,
                    at: location(),
                    message: format!(
                        "{why}, so this instance lowers no variant table; its baked subtree still \
                         paints",
                    ),
                },
            }),
        }
    }

    for (at, walked) in nodes.iter().enumerate() {
        // A **member root** carries the set's default transitions exactly where
        // some instance shipped a table for them to be copied into (debt
        // #1141). This is the reverse arm of a two-variant set — the ordinary
        // authoring shape — and an earlier draft keyed it on "is an instance"
        // alone and took its degrade away, while its successor keyed it on the
        // set having a plan and gave a degrade to sets that ship nothing.
        //
        // Recorded here rather than in a pass of its own: `emitted` is complete
        // before this loop begins, and `carrying` is read only for a switch's
        // **host**, which `paths` guarantees is the node itself or one of its
        // ancestors — so a pre-order walk has always recorded it by the time
        // anything resolving through it is reached.
        if is_definition(walked.node)
            && let Some(id) = walked.node.id.as_deref()
            && let Some(index) = set_of.get(id)
            && emitted.contains(index)
        {
            carrying.insert(at);
        }
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
        let node = walked.node;
        let read = &reads[at];
        let location = || located(index_of_id, walked);
        // The emit pass's findings for this node, replayed in node order so the
        // diagnostics read exactly as one interleaved loop produced them.
        for found in pending.remove(&at).unwrap_or_default() {
            match found.echo {
                Some(source) => report(&mut diagnostics, &mut echoed, source, found.diagnostic),
                None => diagnostics.push(found.diagnostic),
            }
        }

        if !names_here {
            continue;
        }
        if !read.switches.is_empty() || !read.unsupported.is_empty() {
            // Where each of this node's switches lands, and whether its
            // transition reached a table — both per switch, because the
            // destination decides the set and the host chain decides who
            // carries it (debt #1064, #1065).
            let resolved: Vec<(Landing<'_>, Reach)> = read
                .switches
                .iter()
                .map(|switch| {
                    // One walk of the host chain, answering both questions
                    // (debt #1142). `Reach` is unused by every arm but
                    // `Landing::Set`, whose three cases are the whole reason it
                    // is not a bool.
                    let unlanded = || (unlanded(&set_of, &components, &nodes, at), Reach::Named);
                    match set_of.get(switch.destination.as_str()) {
                        None => unlanded(),
                        Some(index) => {
                            let set = &sets[*index];
                            let lowers = matches!(set.state, SetState::Lowers(_));
                            match hosting(&nodes, at, set) {
                                Hosting::Elsewhere => unlanded(),
                                Hosting::LandsAt(host) if lowers && carrying.contains(&host) => {
                                    (Landing::Set(set), Reach::Table)
                                }
                                Hosting::LandsAt(_) => (Landing::Set(set), Reach::Named),
                                // A set that lowers nothing has already named
                                // that loss itself, so the absent host adds
                                // nothing to say; a set that lowers has not.
                                Hosting::LandsNowhere if lowers => {
                                    (Landing::Set(set), Reach::Nowhere)
                                }
                                Hosting::LandsNowhere => (Landing::Set(set), Reach::Named),
                            }
                        }
                    }
                })
                .collect();
            // One authored reaction is one finding, however many copies of the
            // layer carrying it reach the screen (debt #1056). Figma echoes a
            // component's interaction onto every instance verbatim, so a
            // mistake authored once inside a master arrived here once per
            // instance: a design-system file with fifty instances reported
            // fifty-one errors for one thing to fix.
            let source = authored_source(node, &walked.path);
            for diagnostic in interaction_diagnostics(read, &resolved, &location(), policy) {
                report(&mut diagnostics, &mut echoed, source, diagnostic);
            }
        }
    }

    // What the copies cost the reader, said on the one finding that survived
    // them rather than by repeating it. The count is deliberately on the
    // message and not on the `Location`: which node a finding sits at is
    // `dashscene-validator`'s vocabulary, and one producer's echo is not a
    // reason to widen it.
    for (index, copies) in echoed.into_values() {
        if copies > 1 {
            // "further copies of the same reaction", not "of this layer": the
            // usual source is one layer echoed onto many instances, but two
            // identical reactions on one node fold together here too, and
            // naming a layer would be wrong for that one.
            diagnostics[index].message.push_str(&format!(
                " (and {} further {} of the same reaction, not listed separately)",
                copies - 1,
                if copies == 2 { "copy" } else { "copies" },
            ));
        }
    }

    diagnostics
}

/// The layer a reaction was **authored** on, as a key for "these findings are
/// one finding".
///
/// An instance's baked children carry synthetic `I<instance>;<source>` ids —
/// the form `docs/specification/06-dashc-figma-lowering.md` pins — so the
/// authored layer is the last `;`-separated segment, and every copy of one
/// master layer answers with the same id. A node the walk gave no id falls back
/// to its own path, which is unique, so it collapses with nothing.
///
/// **This is deliberately not applied to `figma.unsupported`.** The two look
/// alike and are not: a prototype refusal names a *behaviour* and leaves the
/// node in the document, so reporting it once or fifty-one times produces
/// exactly the same bytes; `figma.unsupported` names a *box* and skips its
/// subtree, so fifty copies are fifty omissions and the multiplicity is the
/// finding. The specification draws that line itself — "Unlike
/// `figma.unsupported` it shall **not** skip the node: what has no lowering is
/// the behaviour, not the box" (`06-dashc-figma-lowering.md`, "Refusal" 10).
fn authored_source<'a>(node: &'a Node, path: &'a str) -> &'a str {
    // `rsplit` on a non-empty pattern always yields at least one item, so there
    // is no no-segment case to fall back from — only the no-id one above.
    node.id
        .as_deref()
        .map_or(path, |id| id.rsplit(';').next().unwrap_or(id))
}

/// One authored layer's findings: where the first copy landed in `diagnostics`,
/// and how many copies there were.
type Echoed<'a> = BTreeMap<(&'a str, &'static str, String), (usize, usize)>;

/// A finding the emit pass produced, held until the naming pass reaches its node
/// (debt #1141).
///
/// It exists because the two passes had to be split — whether a member root
/// carries a table needs every instance walked first — and splitting them would
/// otherwise have moved every instance-level finding ahead of every
/// interaction-level one.
struct Pending<'a> {
    /// The authored layer this collapses onto, or `None` for a finding that is
    /// genuinely per instance and must keep its count.
    echo: Option<&'a str>,
    diagnostic: Diagnostic,
}

/// Reports `diagnostic`, or folds it into an identical one already reported for
/// the same authored layer (debt #1056).
///
/// Every echoed finding goes through here, the per-instance contention warning
/// included: a diagnostic pushed straight onto `diagnostics` would keep the
/// multiplicity this exists to remove.
fn report<'a>(
    diagnostics: &mut Vec<Diagnostic>,
    echoed: &mut Echoed<'a>,
    source: &'a str,
    diagnostic: Diagnostic,
) {
    let key = (source, diagnostic.rule, diagnostic.message.clone());
    if let Some((_, copies)) = echoed.get_mut(&key) {
        *copies += 1;
    } else {
        echoed.insert(key, (diagnostics.len(), 1));
        diagnostics.push(diagnostic);
    }
}

/// Where one `CHANGE_TO` lands, as the **file** answers it.
///
/// Resolved per switch rather than per node, because the **destination** is what
/// decides which set is being switched (debt #1065). A node can carry two
/// switches that land in two different sets — a nested instance switching its
/// own variant with one reaction and its parent's with another is exactly that
/// shape — and a per-node answer cannot express it.
enum Landing<'a> {
    /// The destination is a member of a set one of this node's hosts belongs
    /// to, so the switch is expressible.
    ///
    /// **Whether it reaches a table is a separate question**, answered by
    /// [`Reach`]: a switch whose host is separated from it by a definition
    /// lands in this set and reaches no table, and saying so is what keeps the
    /// refusal message from asserting the destination is not a member when it
    /// is.
    Set(&'a Set<'a>),
    /// The **nearest** host shows a component this file does not contain — the
    /// ordinary shape of an instance of a published-library set the export left
    /// out.
    ///
    /// The nearest one and no other. Asking whether *any* enclosing host showed
    /// an absent component called a broken `destinationId` a missing library
    /// whenever the layer happened to sit inside a library instance, which
    /// downgrades an error that withholds the bytes to a fixed warning and
    /// describes a host the file does carry (issue #976).
    LibraryAbsent,
    /// The node belongs to at least one component set the file carries, and the
    /// destination is a member of none of the sets its hosts belong to: a
    /// `destinationId` the export closure trimmed, or one naming a member of an
    /// unrelated set.
    NotAMember,
    /// No set, and no missing library either: a plain frame, or an instance of
    /// a standalone local `COMPONENT`. A `CHANGE_TO` here resolves nowhere and
    /// never could.
    NoSet,
}

/// How far one `CHANGE_TO` that lands in a set actually got.
///
/// Three answers, not two, because "no table carries it" splits on whether
/// anything else says so. A host whose own table was refused already carries an
/// `UNLOWERABLE_SET` naming why; a switch with **no** qualifying host at all is
/// named nowhere else, and reporting it as a degrade — or not at all — is a
/// silent drop (P4).
enum Reach {
    /// The switch's transition reached a variant table.
    Table,
    /// A host could have carried it and did not: its own table was refused, or
    /// its set lowers none. `UNLOWERABLE_SET` has already named why, on the set
    /// or on that instance.
    Named,
    /// No host can carry it. A definition stands between this layer and the
    /// only host belonging to the destination's set, so the layer reaches the
    /// screen through a component that set does not switch — and nothing else
    /// in the pass will mention it.
    Nowhere,
}

/// One component set the file carries, as resolution needs it.
struct Set<'a> {
    members: Vec<&'a str>,
    state: SetState,
    /// Where this set's own findings are located. Carried so every set-level
    /// diagnostic is emitted from one place in set order — planning them and
    /// naming them are separated by the gathering pass, and pushing each half
    /// where it is computed would interleave two sets' findings by which half
    /// ran first.
    at: Location,
}

/// What a set does with a switch that reaches it, and which diagnostic reports
/// the loss where it does nothing.
enum SetState {
    /// A plan was built, so a variant table lowers and a switch into it ships.
    /// Carries that plan's index, which is what keeps `plans` from being a
    /// second collection to be kept in step with this one.
    Lowers(usize),
    /// No plan: the set names that loss itself under `UNLOWERABLE_SET`, and
    /// this carries the reason until the set-level diagnostics are emitted.
    NamedItsLoss(String),
    /// No plan and nothing named it: fewer than two members, so the set has no
    /// alternative state and never reports one.
    Silent,
}

/// Why a `CHANGE_TO` that did **not** land lowers nowhere.
///
/// Where it lands is [`hosting`]'s answer, not this one: the destination decides
/// the set (debt #1065), and the switch lands when some host — the node itself,
/// or an enclosing `INSTANCE` or definition — belongs to it. This is only the
/// other branch, and it reads the **nearest** host alone.
fn unlanded(
    set_of: &BTreeMap<&str, usize>,
    components: &BTreeSet<&str>,
    nodes: &[Walked<'_>],
    at: usize,
) -> Landing<'static> {
    // The **nearest** host is what explains why, and
    // only that one. Asking whether *any* host shows a component the file lacks
    // reported a broken `destinationId` on an instance of a set the file
    // carries as a missing library, whenever that instance happened to sit
    // inside an instance of one it does not — downgrading an Error that
    // withholds the bytes to a fixed warning, and printing a sentence untrue of
    // the host it is written at (issue #976).
    let host = hosts_of(nodes, at).next().map(|host| nodes[host].node);
    let shows = host.and_then(|host| host.component_id.as_deref());
    let component = shows.or_else(|| host.filter(|host| is_definition(host)).and_then(member_id));
    if component.is_some_and(|component| set_of.contains_key(component)) {
        return Landing::NotAMember;
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
/// for. Deliberately separate from [`hosting`]: whether the file carries a set
/// and whether this pass could lower it are different questions, and answering
/// the first with the second is what made an unlowerable set report itself as
/// absent.
///
/// It reaches the plan through the same `set_of` index resolution uses, so
/// neither the lookup nor the member list is derived twice.
fn plan_of<'p, 'a>(
    plans: &'p [Plan<'a>],
    sets: &[Set<'_>],
    set_of: &BTreeMap<&str, usize>,
    node: &Node,
) -> Option<(&'p Plan<'a>, usize, usize)> {
    let component = node.component_id.as_deref()?;
    let index = *set_of.get(component)?;
    let SetState::Lowers(plan) = sets[index].state else {
        return None;
    };
    let plan = &plans[plan];
    plan.member_of(component)
        .map(|active| (plan, index, active))
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
/// `resolved` runs parallel to `read.switches`: for each one, where it lands
/// ([`Landing`]) and how far it got ([`Reach`]). Both are what `prototype::read`
/// cannot work out on its own, and together they decide what the reader used to
/// guess from a `destinationId`'s presence: whether a refused curve is a degrade
/// or part of an omission (issue #1017), and which of the two omissions a switch
/// that lands nowhere earns (issue #1016). A set that plans does not guarantee a
/// table — `emit` refuses an instance whose geometry disagrees with the member
/// it shows — so `Reach` is what separates a curve that degrades a shipped state
/// change from one that names a loss, and both from a switch nothing else
/// reports.
///
/// Both are per **switch** rather than per node (debt #1064, #1065). One node
/// can declare two switches landing in two different sets, and a layer's switch
/// travels through whichever host belongs to its destination's set, so neither
/// question has a per-node answer.
fn interaction_diagnostics(
    read: &Interactions,
    resolved: &[(Landing<'_>, Reach)],
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

    for (switch, (landing, reach)) in read.switches.iter().zip(resolved) {
        let omission = match landing {
            // The destination is a member of a set one of this node's hosts
            // belongs to, so the switch is expressible — but only a set that
            // lowers a table actually ships it.
            Landing::Set(set) => {
                match set.state {
                    // It ships, so only its curve was ever at risk — unless
                    // this node is not the one that shipped it.
                    SetState::Lowers(_) if matches!(reach, Reach::Table) => None,
                    // No host of this layer belongs to the set at all, and
                    // nothing else in the pass will say so. The layer sits
                    // inside a component of its own, which reaches the screen
                    // independently of this set, so no switch into the set
                    // replaces it — the destination is real and unreachable
                    // from here. Naming it is what keeps the definition
                    // boundary in `hosting` from being a silent drop
                    // (P4): the boundary is right, and its refusal has to be
                    // said out loud.
                    SetState::Lowers(_) if matches!(reach, Reach::Nowhere) => Some((
                        by_policy,
                        format!(
                            "a CHANGE_TO names destination {}, a member of a component set this \
                             layer sits inside — but the layer belongs to a nested component of \
                             its own, which no switch into that set replaces, so the switch \
                             lowers nowhere",
                            switch.destination,
                        ),
                    )),
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
                    SetState::Lowers(_) | SetState::NamedItsLoss(_) => {
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
            // The file carries a set this node belongs to and the destination
            // is a member of none of them: a `destinationId` the export closure
            // trimmed, or one naming a member of a different set. That is a
            // broken file, and under `Strict` handing over its bytes ships a
            // button whose click does nothing (issue #976).
            Landing::NotAMember => Some((
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

/// Where a finding about this node is written: the document index it lowered
/// to, and the path the walk gave it.
///
/// One definition, because the emit pass and the naming pass both write findings
/// about the same node — and a copy in each is how the index fallback and the
/// path form would come to disagree between two findings on one layer.
fn located(index_of_id: &IndexOfId, walked: &Walked<'_>) -> Location {
    Location::Node(NodePath::new(
        index_of(index_of_id, walked.node),
        walked.path.clone(),
    ))
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
    /// This node's parent, as a position in the `paths` vector.
    ///
    /// What it is here for is the **host chain** — every `INSTANCE` or
    /// definition this node sits in, nearest first, including the node itself
    /// where it is one ([`hosts_of`]).
    ///
    /// Separate from `definition` because the two answer different questions
    /// and an `INSTANCE` splits them. Its baked children paint, so it is not a
    /// definition for `definition`'s purpose; but a `CHANGE_TO` on one of
    /// those children switches *its* variant, so a host is what a switch
    /// resolves through. Figma echoes a component's reaction onto the instance
    /// verbatim, so an inner layer driving the enclosing instance's variant —
    /// an everyday authoring shape — arrives as exactly that: a reaction on a
    /// baked child with no `componentId` and no definition above it.
    ///
    /// **The whole chain, not the nearest one** (debt #1065). A nested
    /// `INSTANCE` shows a set of its own, so stopping at the nearest host
    /// answers with *that* set for a chip switching the variant of the
    /// component it sits inside — a destination that is then not one of its
    /// members, which under `Strict` withheld a document that is correct.
    /// Which host a switch travels through is decided by its destination
    /// instead, in [`hosting`], and that needs the ones above the
    /// nearest.
    ///
    /// A parent link rather than a materialised chain: the chain is walked once
    /// per switch, which is rare, while storing one would allocate a `Vec` for
    /// every node in the file — an O(nodes x depth) cost on every compile, added
    /// inside the pass debt #1066 is about.
    ///
    /// Positions rather than `&Node` so a host can be asked whether its own
    /// variant table lowered, which is a fact about the pass and not about the
    /// node.
    parent: Option<usize>,
}

/// What one walk of a node's host chain says about a `CHANGE_TO` whose
/// destination is a member of `set`.
///
/// Both answers come from **one** walk (debt #1142). They are different
/// questions — whether the switch lands in this set at all, and which host's
/// table carries it — and asking them separately walked the same chain twice
/// for every switch, once to decide it lands and once for [`Reach`].
enum Hosting {
    /// No host of this node belongs to `set`, so the switch does not land in it
    /// and something else has to explain why.
    Elsewhere,
    /// The switch lands, and **no** host can carry it: a definition stands
    /// between this node and the nearest host that belongs to the set.
    LandsNowhere,
    /// The switch lands, and this host's table carries it — a position in the
    /// `paths` vector.
    LandsAt(usize),
}

/// The host a `CHANGE_TO` whose destination is a member of `set` travels
/// through: the nearest of the node's hosts that belongs to `set`, as a position
/// in the `paths` vector.
///
/// This is the resolution rule debt #1065 settled, and debt #1064 is what
/// "travels through" then means for the transition table. The **destination**
/// decides which set is being switched; the chain decides which host does the
/// switching. A layer switching the variant of the instance it belongs to and a
/// nested instance switching its _parent's_ are then one shape, answered by one
/// rule, where resolving by position alone could only ever answer one of them.
///
/// **A definition between the node and the host answers `None`.** A
/// definition's contents reach the screen only through an instance of *it*,
/// so a layer inside a master that sits within a member belongs to that
/// master's content and not to the member's — and its switch must not join
/// the enclosing set's table, which would let a reaction that never paints
/// set the transition every instance of that set ships (issue #1018).
///
/// An **instance** between the two is crossed freely, and that asymmetry is
/// the point: an instance's baked children do paint, as part of whatever
/// shows them, so a layer inside a nested instance switching the enclosing
/// component's variant is exactly the shape debt #1065 is about.
fn hosting(nodes: &[Walked<'_>], at: usize, set: &Set<'_>) -> Hosting {
    let mut crossed = false;
    for host in hosts_of(nodes, at) {
        let node = nodes[host].node;
        if belongs_to(node, set) {
            return if crossed {
                Hosting::LandsNowhere
            } else {
                Hosting::LandsAt(host)
            };
        }
        crossed |= is_definition(node);
    }
    Hosting::Elsewhere
}

/// Every `INSTANCE` or definition the node at `at` sits in, **nearest first**,
/// including that node itself where it is one.
fn hosts_of<'n>(nodes: &'n [Walked<'_>], at: usize) -> impl Iterator<Item = usize> + 'n {
    let mut next = Some(at);
    std::iter::from_fn(move || {
        while let Some(current) = next {
            next = nodes[current].parent;
            let node = nodes[current].node;
            if node.kind == "INSTANCE" || is_definition(node) {
                return Some(current);
            }
        }
        None
    })
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
}

/// Whether `node` is one of `set`'s members: an `INSTANCE` showing one, or the
/// member `COMPONENT` itself.
///
/// A `COMPONENT_SET` is a definition too, and deliberately matches nothing —
/// its own id names the set rather than a member, so it never absorbs a switch
/// that one of its members should carry.
fn belongs_to(node: &Node, set: &Set<'_>) -> bool {
    member_id(node).is_some_and(|id| set.members.contains(&id))
}

/// The member `COMPONENT` id this node belongs to, if it belongs to one at all:
/// an `INSTANCE`'s `componentId`, or a definition's own id.
fn member_id(node: &Node) -> Option<&str> {
    if is_definition(node) {
        node.id.as_deref()
    } else {
        node.component_id.as_deref()
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
            parent: None,
        })
        .rev()
        .collect();
    while let Some(walked) = stack.pop() {
        // This node's own position, which is where it is about to be pushed —
        // and so the parent link its children carry. Sound because nothing else
        // pushes to `out` between here and the push below.
        let at = out.len();
        let children: Vec<&Node> = walked.node.children.iter().collect();
        let segments = super::disambiguated_segments(&children);
        let definition = walked.owner();
        for (child, segment) in walked.node.children.iter().zip(segments).rev() {
            stack.push(Walked {
                node: child,
                path: format!("{}/{segment}", walked.path),
                definition,
                parent: Some(at),
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

        // The set's own default transitions are **not** read here. `apply`
        // gathers every switch in the file onto the host whose table carries
        // it, from the walk it already has, and this set's default table is
        // the switches that resolved onto its member roots (debt #1064,
        // #1066). Reading them here as well would walk the same members a
        // second time and allocate the same refusal strings twice, and it
        // would have to re-derive the resolution rule to do it — which is what
        // `diagnostics` below already says about naming them here.
        Ok(Self {
            members,
            props,
            differing,
        })
    }

    /// The member index a Figma component id names, if this set holds it.
    fn member_of(&self, id: &str) -> Option<usize> {
        position_of(&self.members, id)
    }

    /// Whether this set differs on any **rect** channel, and so whether a
    /// transition into it has anything to animate. A set that differs nowhere,
    /// or only on a channel a transition cannot carry, writes no transition at
    /// all — every member's is `None`, which is what "lands in one frame"
    /// means.
    ///
    /// One definition, because two callers ask: `diagnostics` decides whether a
    /// contended destination has lost anything beyond what it already said, and
    /// `apply` asks the same about an instance's own contention.
    fn animatable(&self) -> bool {
        self.differing
            .values()
            .flatten()
            .any(|prop| RECT_CHANNELS.iter().any(|(channel, _)| channel == prop))
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
    /// `animates` is whether any switch into this set declares a transition at
    /// all — a set whose switches declare none has no motion to lose, so it
    /// names none — and `collisions` the destinations more than one declaration
    /// named with transitions that disagree. Both are gathered by `apply` and
    /// passed in rather than read here (debt #1064, #1066).
    fn diagnostics(&self, at: &Location, animates: bool, collisions: &[usize]) -> Vec<Diagnostic> {
        let mut out = Vec::new();

        if !animates {
            return out;
        }
        let named: BTreeSet<Prop> = self
            .differing
            .values()
            .flatten()
            .filter(|prop| !RECT_CHANNELS.iter().any(|(channel, _)| channel == *prop))
            .copied()
            .collect();
        let rect = self.animatable();
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
        out.extend(collisions.iter().map(|member| {
            let member = self.members[*member].name.as_str();
            Diagnostic {
                rule: rule::UNSUPPORTED_MOTION,
                severity: Severity::Warning,
                at: at.clone(),
                // "more than one layer", not "more than one member": since debt
                // #1064 this table gathers every switch that resolves onto a
                // member root, so the two declarations that disagree can be two
                // layers inside one member as easily as two member roots.
                message: format!(
                    "more than one layer of this component set declares a CHANGE_TO to \
                     \"{member}\" with a different transition, and the document carries one \
                     transition per destination, so only one of them lowers",
                ),
            }
        }));
        out
    }

    /// Emits this set's `VariantSet` for one instance, or names why it cannot
    /// be expressed against that instance's baked subtree.
    /// `tween` is the transition each member is switched **to** with, already
    /// resolved by `apply`: the set's default table, overridden by the switches
    /// that travel through this instance. It arrives resolved rather than being
    /// folded here, because which switches those are is a question about the
    /// whole file's host chains and not about this instance's baked subtree
    /// (debt #1064).
    fn emit(
        &self,
        doc: &mut Document,
        instance: &Node,
        active: usize,
        tween: &[Option<(f32, Easing)>],
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

/// The transition each destination is switched to with, by member index.
///
/// A key is present exactly where something was declared, which is not the same
/// as its tween being `Some`: a switch that declares no transition, or one whose
/// curve had no `dashcue` spelling, lands in one frame **on purpose** and must
/// override a default rather than leave it standing.
type Declared = BTreeMap<usize, Option<(f32, Easing)>>;

/// [`Declared`] together with the members more than one declaration named with
/// transitions that disagree, **by member index**.
///
/// Indices rather than names so a caller can borrow the name from the file
/// rather than clone it, which is what lets a contention key an echo (debt
/// #1137) without allocating (debt #1142).
type DeclaredTweens = (Declared, Vec<usize>);

/// The transition each destination is declared with, and the destinations more
/// than one declaration named with transitions that disagree.
///
/// The document carries **one transition per destination**
/// (`docs/decisions/motion-is-document-data-keyed-on-the-destination.md`), so
/// where two declarations disagree only one of them lowers, and the contention
/// is named rather than riding silently on the order Figma lists them in (P4,
/// issue #976). Two declarations of the *same* transition lose nothing and name
/// nothing.
///
/// The later declaration wins, which is the direction `apply` already takes for
/// an instance's own reaction over its set's. What is named is that a
/// declaration was displaced at all, never which one survived.
///
/// One fold for both scopes. A set's default table folds the switches its
/// members declare; an instance's own table folds the ones its baked subtree
/// declares, over that default. They were one rule read at one scope until debt
/// #1064 widened the other, and writing the collision case twice is how the two
/// would drift.
///
fn declared_tweens(members: &[&Node], switches: &[&Switch]) -> DeclaredTweens {
    let mut declared: BTreeMap<usize, (Option<(f32, Easing)>, bool)> = BTreeMap::new();
    for switch in switches {
        if let Some(destination) = position_of(members, &switch.destination) {
            let entry = declared.entry(destination).or_insert((switch.tween, false));
            entry.1 |= entry.0 != switch.tween;
            entry.0 = switch.tween;
        }
    }
    let contended = declared
        .iter()
        .filter(|(_, (_, contended))| *contended)
        .map(|(destination, _)| *destination)
        .collect();
    let tweens = declared
        .into_iter()
        .map(|(destination, (kept, _))| (destination, kept))
        .collect();
    (tweens, contended)
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
