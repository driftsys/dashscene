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
//!   are handed over while authored intent is being dropped (R6).
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
use crate::figma::rest::{FigmaFile, Node, Paint};

/// The diagnostic rules this producer assembles for the variant table and the
/// prototype interactions that animate it.
pub mod rule {
    /// A prototype interaction the document has no construct for at all — a
    /// trigger, action or navigation outside the vocabulary, **or** a
    /// `CHANGE_TO` naming a destination no set in the file carries, which
    /// lowers no switch at all (issue #976). Nothing about either reaches the
    /// document, so the severity follows the emit policy: an error that
    /// withholds the bytes under `EmitPolicy::Strict` (R6), a warning under
    /// `EmitPolicy::Partial`.
    pub const UNSUPPORTED_INTERACTION: &str = "figma.prototype.unsupported-interaction";
    /// A variant switch lowers, but the motion Figma declared for it does
    /// not: an easing with no `dashcue` spelling, a difference on a channel no
    /// transition can animate, or a second member declaring a different
    /// transition to a destination one already claimed. Always a warning — the
    /// state change ships and lands in one frame, or with the transition that
    /// won, which is what a member with no transition has always meant.
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

    // Which members something instantiates. A definition paints nothing, so a
    // reaction on a master no instance shows costs the picture nothing and is
    // named nowhere — the same reasoning `Walk::visit` uses when it fires no
    // finding at all inside a definition.
    let shown: BTreeSet<&str> = nodes
        .iter()
        .filter(|(node, _)| node.kind == "INSTANCE")
        .filter_map(|(node, _)| node.component_id.as_deref())
        .collect();

    // Every painting node's interactions, named here and only here.
    // Definitions are skipped for the reason above; a member's own reactions
    // are read again in `Plan::of`, where they become the set's defaults, and
    // named there only when no instance echoes them.
    for (node, path) in &nodes {
        if is_definition(node) {
            continue;
        }
        diagnostics.extend(interaction_diagnostics(
            &prototype::read(node),
            &Location::Node(NodePath::new(index_of(index_of_id, node), path.clone())),
            policy,
        ));
    }

    // The component sets, planned once each: the member trees, what differs
    // between them, and the transitions their own reactions declare. None of
    // that depends on an instance, which is what lets a set with no instance
    // still name what it could not lower — `refused-fill-diff` has no
    // instance and is the case every real Figma file hits.
    let mut plans: Vec<Plan<'_>> = Vec::new();
    for (node, path) in &nodes {
        if node.kind != "COMPONENT_SET" {
            continue;
        }
        let at = Location::Node(NodePath::new(index_of(index_of_id, node), path.clone()));
        match Plan::of(node, path) {
            Ok(plan) => {
                diagnostics.extend(plan.diagnostics(&shown, &at, policy));
                plans.push(plan);
            }
            Err(Some(why)) => diagnostics.push(Diagnostic {
                rule: rule::UNLOWERABLE_SET,
                severity: Severity::Warning,
                at,
                message: format!(
                    "{why}, so this component set lowers no variant table; its instances still \
                     paint the member they show",
                ),
            }),
            // A set with nothing to lose names nothing.
            Err(None) => {}
        }
    }

    for (node, path) in &nodes {
        if node.kind != "INSTANCE" {
            continue;
        }
        let at = || Location::Node(NodePath::new(index_of(index_of_id, node), path.clone()));
        let own = prototype::read(node).switches;

        // An instance of a standalone `COMPONENT`, or of a set this file does
        // not carry, has no alternative to switch to. That costs the picture
        // nothing — but a `CHANGE_TO` on it does not lower, and dropping one
        // in silence is what P4 forbids, so the switches are named below
        // whether or not a plan was found.
        let found = node.component_id.as_deref().and_then(|component| {
            plans
                .iter()
                .find_map(|plan| plan.member_of(component).map(|active| (plan, active)))
        });

        for switch in &own {
            let lands =
                found.is_some_and(|(plan, _)| plan.member_of(&switch.destination).is_some());
            if !lands {
                // An omission, not a degrade: no switch reaches the document
                // at all, so this follows the emit policy like every other
                // interaction with no lowering. Under `Strict` a file whose
                // export closure trimmed a `destinationId` used to compile
                // clean and ship a button whose click does nothing (issue
                // #976).
                diagnostics.push(Diagnostic {
                    rule: rule::UNSUPPORTED_INTERACTION,
                    severity: omission_severity(policy),
                    at: at(),
                    message: format!(
                        "a CHANGE_TO names destination {}, which is not a member of a component \
                         set this file carries, so the switch lowers nowhere",
                        switch.destination,
                    ),
                });
            }
        }

        let Some((plan, active)) = found else {
            continue;
        };
        if let Err(why) = plan.emit(doc, node, active, &own, index_of_id) {
            diagnostics.push(Diagnostic {
                rule: rule::UNLOWERABLE_SET,
                severity: Severity::Warning,
                at: at(),
                message: format!(
                    "{why}, so this instance lowers no variant table; its baked subtree still \
                     paints",
                ),
            });
        }
    }

    diagnostics
}

/// Whether this node is a component definition — resolved by the walk, never
/// painted, and therefore never diagnosed (story #242).
fn is_definition(node: &Node) -> bool {
    node.kind == "COMPONENT" || node.kind == "COMPONENT_SET"
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
fn interaction_diagnostics(
    read: &Interactions,
    at: &Location,
    policy: crate::EmitPolicy,
) -> Vec<Diagnostic> {
    let severity = omission_severity(policy);
    read.unsupported
        .iter()
        .map(|what| Diagnostic {
            rule: rule::UNSUPPORTED_INTERACTION,
            severity,
            at: at.clone(),
            message: format!("{what} is not in the document vocabulary"),
        })
        .chain(read.motion.iter().map(|what| Diagnostic {
            rule: rule::UNSUPPORTED_MOTION,
            severity: Severity::Warning,
            at: at.clone(),
            message: what.clone(),
        }))
        .collect()
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

/// Every node of the file in the walk's own order, each with the diagnostic
/// path the walk would give it.
///
/// Definitions are included, unlike the walk's traversal: a component set is
/// exactly what this module is here to read.
fn paths(file: &FigmaFile) -> Vec<(&Node, String)> {
    let roots = super::top_level_nodes(&file.document).unwrap_or_default();
    let mut out = Vec::new();
    let mut stack: Vec<(&Node, String)> = super::disambiguated_segments(&roots)
        .into_iter()
        .zip(roots)
        .map(|(segment, root)| (root, format!("/{segment}")))
        .rev()
        .collect();
    while let Some((node, path)) = stack.pop() {
        let children: Vec<&Node> = node.children.iter().collect();
        let segments = super::disambiguated_segments(&children);
        for (child, segment) in node.children.iter().zip(segments).rev() {
            stack.push((child, format!("{path}/{segment}")));
        }
        out.push((node, path));
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
    /// What each member's own reactions could not lower, kept per member so a
    /// finding can be named only when no instance echoes it.
    reactions: Vec<Interactions>,
}

impl<'a> Plan<'a> {
    /// Plans one component set, or says why it cannot be lowered — `None`
    /// where there is nothing to say, because nothing was lost.
    fn of(set: &'a Node, path: &str) -> Result<Self, Option<String>> {
        let _ = path;
        let members: Vec<&Node> = set
            .children
            .iter()
            .filter(|child| child.kind == "COMPONENT")
            .collect();
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
            reactions,
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

    /// Everything this set is responsible for naming: the props that differ
    /// on a channel no transition can animate, and its members' own reaction
    /// findings where no instance echoes them.
    fn diagnostics(
        &self,
        shown: &BTreeSet<&str>,
        at: &Location,
        policy: crate::EmitPolicy,
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();

        // A member's reaction is echoed verbatim onto every instance showing
        // it, and those instances name it themselves. What no instance shows
        // is named here instead — the reverse arm of a two-variant set is the
        // ordinary case, since an instance echoes only the member it is on.
        //
        // The question is per-set, not per-file (issue #976): a set nothing
        // instantiates paints nothing, so its members' refused reactions cost
        // the picture nothing and are named nowhere. Asking whether the *file*
        // holds any instance let an unrelated one — which every real Figma
        // file has — turn those reactions into findings, and under `Strict`
        // withhold the whole document over a layer that never ships.
        let instantiated = self
            .members
            .iter()
            .any(|member| member.id.as_deref().is_some_and(|id| shown.contains(id)));
        if instantiated {
            for (member, read) in self.members.iter().zip(&self.reactions) {
                let echoed = member.id.as_deref().is_some_and(|id| shown.contains(id));
                if !echoed {
                    out.extend(interaction_diagnostics(read, at, policy));
                }
            }
        }

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
    // The eleven fields deliberately not compared, and why:
    //
    // `id` and `name` are identity — the members are joined *by* name, and
    // their ids differ by construction. `children` are compared through the
    // name-path key set and this function's own per-key application, never
    // here, or a child's overridable prop would count as a structural
    // difference. `absolute_bounding_box`, `size`, `fills`, `visible` and
    // `rotation` are exactly the inputs to `Props`, which is the overridable
    // half. `relative_transform` is not compared for a narrower reason: the
    // only thing read off it is the turn `Node::turn` derives, which is a
    // `Props` input, and its other five components are read by nothing at
    // all. So two members differing only in the scale or translation their
    // matrices carry compare equal here — unchanged from before the field was
    // parsed, and a gap rather than a classification (issue #1019).
    // `interactions` and `component_id` are the prototype layer, which
    // differs between members by design.
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
        relative_transform: _,
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
