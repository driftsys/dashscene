//! The reactive layer: signals, bindings, transforms, and the
//! per-frame flush (issue #166; docs/wip/2026-07-13-reactive-bindings-spec.md,
//! decisions D1–D4 and D8; docs/archive/2026-07-14-scope-decisions.md §23).
//!
//! A producer declares **signals** on a [`Scene`], **binds** signal
//! values to node props through a declarative **transform** vocabulary,
//! and then drives a live scene at 60 Hz through one commit per
//! [`LiveScene::tick`]. `dashscene-core` is unchanged: the whole layer
//! lives here, because a signal's value is a *result* (P1 keeps results
//! out of the document) and the reactive graph sits on no crate boundary
//! (D1). `dashlang` depending on both `dashscene-core` and `dashcue` is
//! what keeps core clear of the animation crate (SCOPE §9), by
//! construction.
//!
//! The design's load-bearing simplifications, honoured here:
//!
//! - **A binding connects data to one prop on one node — never two
//!   nodes** (the Non-goals section). "When the list grows, the panel
//!   below it moves" is layout, resolved by the solver, not a signal
//!   edge (P2).
//! - **Bindings are explicit and declarative, never implicitly tracked**
//!   (D2). Each binding names its source signal, its target
//!   [`Channel`], and a [`Transform`]. The graph is a flat table known
//!   before anything runs, so a write is statically classifiable as
//!   layout-affecting or paint-only — which is what lets a contained
//!   scalar write skip the solve (A1).
//! - **Signals are push-on-flush, never pull-on-paint** (P3). A signal's
//!   value is read only during [`LiveScene::tick`]'s flush, never by the
//!   renderer.
//! - **Bindings resolve to a target at build**, so a producer never
//!   handles a [`dashscene_core::NodeId`] and cannot hold a stale one.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use dashcue::{PropKey, Scheduler, TransitionSpec};
use dashscene_core::{
    Arena, Atlas, AxisSizing, Color, FillSpec, Layout, LayoutMode, LayoutSolver, NodeId, Prop,
    ScalarTransform, SignalId, SolvedRect, StagedRun, VariantSetId,
};

use crate::{Node, Scene};

/// The scheduler key for one binding: core's one packing (debt #208),
/// wrapped in `dashcue`'s typed key. The same math the engine's FLIP
/// uses, so one `(node, channel)` yields one key everywhere.
fn prop_key(node: NodeId, channel: Channel) -> PropKey {
    PropKey(dashscene_core::prop_key(node, channel))
}

mod private {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for bool {}
}

// The channel vocabulary is the document's, in `dashscene-core`, since
// story #167 (the full §23 set — X/Y/Width/Height, `Gap`, and the four
// `Fill` channels; debt #201). Re-exported so the authoring surface
// keeps one import path.
pub use dashscene_core::Channel;

/// The core `Prop` a non-fill channel writes. Fill channels never route
/// here: a fill write goes through the per-node fill shadow (see
/// [`LiveScene::tick`]), because one channel writes one component of a
/// four-component color.
fn prop_for(channel: Channel, v: f32) -> Prop {
    match channel {
        Channel::X => Prop::X(v),
        Channel::Y => Prop::Y(v),
        Channel::Width => Prop::Width(v),
        Channel::Height => Prop::Height(v),
        Channel::Gap => Prop::Gap(v),
        // Opacity is a single scalar prop, so it routes here like geometry
        // rather than through the four-component fill shadow — but it is
        // paint-only, so [`classify`] keeps it off the solve (debt #253).
        Channel::Opacity => Prop::Opacity(v),
        Channel::FillR | Channel::FillG | Channel::FillB | Channel::FillA => {
            unreachable!("fill channels write through the fill shadow, never prop_for")
        }
        // Three channels address one three-component prop, so they cannot
        // stage directly: writing the angle alone would have to invent an
        // anchor, and writing an anchor component alone would have to invent
        // an angle. They write through the node's rotation shadow, exactly
        // as the four fill channels write through its fill shadow.
        Channel::Rotation | Channel::RotationAnchorX | Channel::RotationAnchorY => {
            unreachable!("rotation channels write through the rotation shadow, never prop_for")
        }
    }
}

/// Whether `channel` addresses one component of the node's rotation.
fn is_rotation(channel: Channel) -> bool {
    matches!(
        channel,
        Channel::Rotation | Channel::RotationAnchorX | Channel::RotationAnchorY
    )
}

/// One component of a node's rotation shadow, selected by a rotation
/// channel: `[angle, anchor_x, anchor_y]`.
fn rotation_component(rotation: &mut [f32; 3], channel: Channel) -> &mut f32 {
    match channel {
        Channel::Rotation => &mut rotation[0],
        Channel::RotationAnchorX => &mut rotation[1],
        Channel::RotationAnchorY => &mut rotation[2],
        _ => unreachable!("rotation_component takes only a rotation channel"),
    }
}

/// The `Prop` a rotation shadow stages.
fn rotation_prop(rotation: [f32; 3]) -> Prop {
    Prop::Rotation {
        angle: rotation[0],
        anchor: (rotation[1], rotation[2]),
    }
}

/// Whether `channel` addresses one component of the node's fill color.
fn is_fill(channel: Channel) -> bool {
    matches!(
        channel,
        Channel::FillR | Channel::FillG | Channel::FillB | Channel::FillA
    )
}

/// One component of `color`, selected by a fill channel.
fn fill_component(color: &mut Color, channel: Channel) -> &mut f32 {
    match channel {
        Channel::FillR => &mut color.r,
        Channel::FillG => &mut color.g,
        Channel::FillB => &mut color.b,
        Channel::FillA => &mut color.a,
        other => unreachable!("{other:?} is not a fill channel"),
    }
}

/// The initial authored value of `channel` — the datum a spring smooths
/// *from* before any signal changes. Geometry and gap come from the
/// layout; a fill channel reads its component of the authored solid
/// fill, or `0.0` for an unfilled node.
fn initial_channel_value(
    layout: &Layout,
    fill: Option<Color>,
    rotation: [f32; 3],
    channel: Channel,
) -> f32 {
    match channel {
        Channel::X => layout.x,
        Channel::Y => layout.y,
        Channel::Width => layout.width,
        Channel::Height => layout.height,
        Channel::Gap => layout.gap,
        // This function only runs for a channel that has a binding, and
        // `seed_scalar` overwrites every bound channel's `last_applied`
        // right after, from the signal's own initial value — the same
        // precedence `visible`/`visible_when` already has
        // (`crates/dashlang/tests/visible_precedence.rs`). So this
        // fallback is never read as a spring's actual starting point,
        // whether or not the node also authors `Node::opacity`; `1.0`
        // just matches the arena's own default (`Arena::opacity`).
        Channel::Opacity => 1.0,
        // The node's authored rotation, read back per component. Without
        // these arms the wildcard below treats a rotation channel as a fill
        // channel and `fill_component` panics — this match has no
        // exhaustiveness check to catch that, which is why the arms are
        // spelled out rather than left to the fallback (story #770).
        //
        // `rotation` carries the same `[angle, anchor_x, anchor_y]` order the
        // rotation shadow uses.
        Channel::Rotation => rotation[0],
        Channel::RotationAnchorX => rotation[1],
        Channel::RotationAnchorY => rotation[2],
        _ => {
            let mut color = fill.unwrap_or(TRANSPARENT);
            *fill_component(&mut color, channel)
        }
    }
}

/// The fill a bound-but-unfilled node's shadow starts from. Fully
/// transparent, so an author who binds only some fill channels sees the
/// unbound components stay at zero rather than an invented color.
const TRANSPARENT: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// How one scalar binding's write reaches the committed output — decided
/// at build, never per frame (the flat table is what makes a write
/// statically classifiable, `docs/decisions/bindings-are-explicit-and-flat.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum WriteClass {
    /// A contained rect write: patch one cached rect, skip the solve (A1).
    Patch,
    /// A layout-affecting write that escapes one rect: force the solve.
    Solve,
    /// A fill write: paint-only, no patch and no solve
    /// (`docs/decisions/visible-is-layout-opacity-is-paint.md`).
    PaintOnly,
}

/// Classifies one channel's write. `contained` and the rect containment
/// rule are the caller's (`write_is_single_rect`); `Gap` always solves —
/// a gap redistributes the container's children by definition.
fn classify(
    channel: Channel,
    contained: bool,
    has_children: bool,
    passthrough: bool,
) -> WriteClass {
    // Fill and opacity are both paint-only (they never reflow anything —
    // `docs/decisions/visible-is-layout-opacity-is-paint.md`, debt #253).
    if is_fill(channel) || channel == Channel::Opacity || is_rotation(channel) {
        return WriteClass::PaintOnly;
    }
    if channel == Channel::Gap {
        return WriteClass::Solve;
    }
    if contained && write_is_single_rect(channel, has_children, passthrough) {
        WriteClass::Patch
    } else {
        WriteClass::Solve
    }
}

/// A stable index into a [`ClosureId`]-keyed table. A `Custom` transform
/// stores only this id, not the closure, so the [`Transform`] enum stays
/// plain data that a future `.dsb` schema could serialise — the closure
/// itself lives in a `dashlang`-only side table and a compiled `Custom`
/// binding is a named diagnostic rather than a silent drop (D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosureId(u32);

/// A declarative numeric-to-text format — the one text transform that is
/// not a closure, so it survives into a serialised binding table (D8).
/// Renders `prefix` + the value at `decimals` fixed places + `suffix`.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatSpec {
    pub prefix: String,
    pub decimals: u8,
    pub suffix: String,
}

impl FormatSpec {
    pub fn new(prefix: impl Into<String>, decimals: u8, suffix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            decimals,
            suffix: suffix.into(),
        }
    }

    fn apply(&self, v: f32) -> String {
        format!(
            "{}{:.*}{}",
            self.prefix, self.decimals as usize, v, self.suffix
        )
    }
}

/// The bounded, declarative transform vocabulary (D8). Everything a
/// designer can express in Figma lives in the non-`Custom` subset, so
/// the validator can reason about it and it can serialise to `.dsb` at
/// v0.7. `Custom` is the `dashlang`-only escape hatch: an arbitrary Rust
/// closure, referenced by [`ClosureId`] because closures do not
/// serialise.
///
/// A scalar-channel binding uses `Identity` / `Scale` / `MapRange` /
/// `Clamp` / `Custom`; a text binding uses `Format` / `Custom`. The
/// binding's target kind selects which closure table a `Custom` id
/// indexes.
#[derive(Clone, Debug, PartialEq)]
pub enum Transform {
    Identity,
    Scale(f32),
    MapRange {
        in_lo: f32,
        in_hi: f32,
        out_lo: f32,
        out_hi: f32,
    },
    Clamp {
        lo: f32,
        hi: f32,
    },
    Format(FormatSpec),
    Custom(ClosureId),
}

fn eval_scalar(transform: &Transform, closures: &[Box<dyn Fn(f32) -> f32>], v: f32) -> f32 {
    match transform {
        Transform::Custom(id) => closures[id.0 as usize](v),
        // A scalar binding never carries a Format transform (the builder
        // only produces Format through the text path); return the input
        // unchanged rather than panicking inside the frame loop.
        Transform::Format(_) => v,
        // The declarative subset shares core's one implementation of the
        // transform math, so a live binding and a loaded document binding
        // cannot compute differently.
        declarative => to_core(declarative)
            .expect("every non-Custom, non-Format transform is declarative")
            .apply(v),
    }
}

/// The core (serializable) form of a declarative transform, or `None`
/// for the two forms that never enter the document tables: `Custom` (a
/// closure does not serialize — D8) and `Format` (text-only).
fn to_core(transform: &Transform) -> Option<ScalarTransform> {
    match *transform {
        Transform::Identity => Some(ScalarTransform::Identity),
        Transform::Scale(factor) => Some(ScalarTransform::Scale(factor)),
        Transform::MapRange {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => Some(ScalarTransform::MapRange {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        }),
        Transform::Clamp { lo, hi } => Some(ScalarTransform::Clamp { lo, hi }),
        Transform::Format(_) | Transform::Custom(_) => None,
    }
}

/// The `dashlang` form of a core (loaded-document) transform.
fn from_core(transform: ScalarTransform) -> Transform {
    match transform {
        ScalarTransform::Identity => Transform::Identity,
        ScalarTransform::Scale(factor) => Transform::Scale(factor),
        ScalarTransform::MapRange {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        } => Transform::MapRange {
            in_lo,
            in_hi,
            out_lo,
            out_hi,
        },
        ScalarTransform::Clamp { lo, hi } => Transform::Clamp { lo, hi },
    }
}

fn eval_text(transform: &Transform, closures: &[Box<dyn Fn(f32) -> String>], v: f32) -> String {
    match transform {
        Transform::Format(spec) => spec.apply(v),
        Transform::Custom(id) => closures[id.0 as usize](v),
        // A text binding never carries a scalar transform.
        _ => String::new(),
    }
}

/// A per-prop spring, following `dashcue`'s stiffness + damping-ratio
/// model so a Compose `SpringSpec` maps onto it as data. `smooth` binds
/// one to a channel: the signal sets the spring's target, and the
/// scheduler drives the actual value (D4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping_ratio: f32,
}

impl Spring {
    pub fn new(stiffness: f32, damping_ratio: f32) -> Self {
        Self {
            stiffness,
            damping_ratio,
        }
    }

    /// A critically-damped spring (`damping_ratio = 1`) whose natural
    /// period is `response` seconds — the fastest approach with no
    /// overshoot. Stiffness follows from the period: `ω = 2π / response`,
    /// `stiffness = ω²`.
    pub fn critically_damped(response: f32) -> Self {
        let omega = std::f32::consts::TAU / response;
        Self {
            stiffness: omega * omega,
            damping_ratio: 1.0,
        }
    }

    fn spec(self) -> TransitionSpec {
        TransitionSpec::Spring {
            stiffness: self.stiffness,
            damping_ratio: self.damping_ratio,
        }
    }
}

/// A typed handle to a signal owned by a [`Scene`] / [`LiveScene`].
/// Signals cannot be free-standing values: two nodes must be able to
/// bind the same one, and identity needs an owner, which is the builder
/// (D2, "The authoring surface"). `T` is `f32` or `bool`.
pub struct Signal<T> {
    id: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T> std::fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signal").field("id", &self.id).finish()
    }
}

/// The value type of a signal — `f32` or `bool`. Sealed: the reactive
/// layer stores exactly these two scalar spaces.
pub trait SignalValue: private::Sealed + Copy {
    #[doc(hidden)]
    fn declare(scene: &mut Scene, initial: Self) -> u32;
    #[doc(hidden)]
    fn store(live: &mut LiveScene, id: u32, value: Self);
}

impl SignalValue for f32 {
    fn declare(scene: &mut Scene, initial: Self) -> u32 {
        let id = scene.scalar_inits.len() as u32;
        scene.scalar_inits.push(initial);
        scene.scalar_names.push(None);
        id
    }

    fn store(live: &mut LiveScene, id: u32, value: Self) {
        // A set to the current value is a no-op: skip it so the next tick
        // does not re-evaluate the signal's bindings (and possibly re-solve)
        // for a value that did not change (P3). A NaN set stays dirty, which
        // is safe.
        if live.scalars[id as usize] == value {
            return;
        }
        live.scalars[id as usize] = value;
        live.scalar_dirty[id as usize] = true;
    }
}

impl SignalValue for bool {
    fn declare(scene: &mut Scene, initial: Self) -> u32 {
        let id = scene.bool_inits.len() as u32;
        scene.bool_inits.push(initial);
        id
    }

    fn store(live: &mut LiveScene, id: u32, value: Self) {
        if live.bools[id as usize] == value {
            return;
        }
        live.bools[id as usize] = value;
        live.bool_dirty[id as usize] = true;
    }
}

/// A closure applied to a scalar signal, produced by [`Signal::map`].
/// `U` is `f32` for a scalar-channel binding or `String` for a text
/// binding; `From` lowers it to a [`ScalarExpr`] or [`TextExpr`] at the
/// `bind` / `bind_text` call.
pub struct Mapped<U> {
    signal: u32,
    f: Box<dyn Fn(f32) -> U>,
}

impl<U> std::fmt::Debug for Mapped<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mapped")
            .field("signal", &self.signal)
            .finish_non_exhaustive()
    }
}

impl Signal<f32> {
    /// The `Custom` escape hatch: an arbitrary closure over the signal's
    /// value. The return type picks the binding kind — `f32` binds to a
    /// scalar channel, `String` to text.
    pub fn map<U, F: Fn(f32) -> U + 'static>(self, f: F) -> Mapped<U> {
        Mapped {
            signal: self.id,
            f: Box::new(f),
        }
    }

    /// A declarative scale: the channel takes `value * factor`.
    pub fn scale(self, factor: f32) -> ScalarExpr {
        ScalarExpr {
            signal: self.id,
            kind: ScalarKind::Data(Transform::Scale(factor)),
        }
    }

    /// A declarative linear remap from `[in_lo, in_hi]` to
    /// `[out_lo, out_hi]` (unclamped — compose with [`Signal::clamp`]).
    pub fn map_range(self, in_lo: f32, in_hi: f32, out_lo: f32, out_hi: f32) -> ScalarExpr {
        ScalarExpr {
            signal: self.id,
            kind: ScalarKind::Data(Transform::MapRange {
                in_lo,
                in_hi,
                out_lo,
                out_hi,
            }),
        }
    }

    /// A declarative clamp to `[lo, hi]`.
    pub fn clamp(self, lo: f32, hi: f32) -> ScalarExpr {
        ScalarExpr {
            signal: self.id,
            kind: ScalarKind::Data(Transform::Clamp { lo, hi }),
        }
    }

    /// A declarative numeric-to-text format for [`Node::bind_text`].
    pub fn format(self, spec: FormatSpec) -> TextExpr {
        TextExpr {
            signal: self.id,
            kind: TextKind::Format(spec),
        }
    }
}

/// A scalar-channel binding expression: a source signal plus a transform
/// to `f32`. Built via [`Signal::scale`] / [`Signal::map_range`] /
/// [`Signal::clamp`] / [`Signal::map`], or `From<Signal<f32>>` for the
/// identity transform.
pub struct ScalarExpr {
    signal: u32,
    kind: ScalarKind,
}

enum ScalarKind {
    Data(Transform),
    Custom(Box<dyn Fn(f32) -> f32>),
}

impl std::fmt::Debug for ScalarExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScalarExpr")
            .field("signal", &self.signal)
            .finish_non_exhaustive()
    }
}

impl From<Signal<f32>> for ScalarExpr {
    fn from(signal: Signal<f32>) -> Self {
        ScalarExpr {
            signal: signal.id,
            kind: ScalarKind::Data(Transform::Identity),
        }
    }
}

impl From<Mapped<f32>> for ScalarExpr {
    fn from(mapped: Mapped<f32>) -> Self {
        ScalarExpr {
            signal: mapped.signal,
            kind: ScalarKind::Custom(mapped.f),
        }
    }
}

/// A text binding expression: a source signal plus a transform to
/// `String`. Built via [`Signal::format`] or [`Signal::map`].
pub struct TextExpr {
    signal: u32,
    kind: TextKind,
}

enum TextKind {
    Format(FormatSpec),
    Custom(Box<dyn Fn(f32) -> String>),
}

impl std::fmt::Debug for TextExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextExpr")
            .field("signal", &self.signal)
            .finish_non_exhaustive()
    }
}

impl From<Mapped<String>> for TextExpr {
    fn from(mapped: Mapped<String>) -> Self {
        TextExpr {
            signal: mapped.signal,
            kind: TextKind::Custom(mapped.f),
        }
    }
}

impl Node {
    /// Bind a scalar channel to a signal expression. Resolved to a
    /// target at build, so the producer never names a `NodeId`.
    pub fn bind(mut self, channel: Channel, expr: impl Into<ScalarExpr>) -> Self {
        self.scalar_bindings.push((channel, expr.into()));
        self
    }

    /// Smooth a bound channel through a spring: the bound signal sets the
    /// spring's target, and the scheduler drives the value (D4). Inert
    /// unless the same channel also carries a [`Node::bind`].
    pub fn smooth(mut self, channel: Channel, spring: Spring) -> Self {
        self.smoothing.push((channel, spring));
        self
    }

    /// Bind the node's text content to a signal expression.
    pub fn bind_text(mut self, expr: impl Into<TextExpr>) -> Self {
        self.text_binding = Some(expr.into());
        self
    }

    /// Bind the node's visibility to a boolean signal. `Visible` is
    /// layout-affecting: a flip reflows siblings (D7).
    pub fn visible_when(mut self, signal: Signal<bool>) -> Self {
        self.visible_binding = Some(signal.id);
        self
    }
}

/// A binding resolved to its target at build. `node` is the write
/// target; `key` addresses the scheduler track when `smoothing` is set.
struct ScalarBinding {
    node: NodeId,
    parent: Option<NodeId>,
    channel: Channel,
    signal: u32,
    transform: Transform,
    /// How this binding's write reaches the committed output — patch one
    /// cached rect (A1), force a solve, or paint-only. Decided at build:
    /// a rect channel patches when the node is ancestor-contained (no
    /// upward escape) and the write moves no descendant
    /// ([`write_is_single_rect`]); `Gap` always solves; fill channels
    /// are always paint-only.
    class: WriteClass,
    smoothing: Option<TransitionSpec>,
    key: PropKey,
    /// The last value driven to the channel — the spring's `from` when a
    /// fresh track starts.
    last_applied: f32,
}

struct TextBinding {
    node: NodeId,
    signal: u32,
    transform: Transform,
}

struct VisibleBinding {
    node: NodeId,
    signal: u32,
}

/// One cached solved rect patched in place for a contained write.
#[derive(Clone, Copy)]
enum Patch {
    X(f32),
    Y(f32),
    W(f32),
    H(f32),
}

/// Whether a write to `channel` changes only the node's own rect (the
/// node is already known to be ancestor-contained), so the commit can
/// patch one cached rect instead of solving. A size change never moves a
/// passthrough (mode-`None`) node's absolutely-placed children, but it
/// redistributes a flex node's children; a position change moves every
/// child. So a non-leaf X/Y write, or a non-leaf non-passthrough size
/// write, is not single-rect and must re-solve.
fn write_is_single_rect(channel: Channel, has_children: bool, passthrough: bool) -> bool {
    match channel {
        Channel::X | Channel::Y => !has_children,
        Channel::Width | Channel::Height => !has_children || passthrough,
        // Only rect channels reach here ([`classify`] routes Gap and the
        // fill channels before asking).
        other => unreachable!("{other:?} is not a rect channel"),
    }
}

fn resolve_patch(
    channel: Channel,
    v: f32,
    parent: Option<NodeId>,
    cached: &[(NodeId, SolvedRect)],
    index: &HashMap<NodeId, usize>,
) -> Patch {
    // A contained write moves no ancestor, so the parent's absolute
    // origin is still whatever the last solve put in the cache.
    let parent_origin = |axis: fn(&SolvedRect) -> f32| -> f32 {
        parent.map_or(0.0, |p| axis(&cached[index[&p]].1))
    };
    match channel {
        Channel::Width => Patch::W(v),
        Channel::Height => Patch::H(v),
        Channel::X => Patch::X(parent_origin(|r| r.x) + v),
        Channel::Y => Patch::Y(parent_origin(|r| r.y) + v),
        // Only WriteClass::Patch reaches here, and only rect channels
        // classify as Patch.
        other => unreachable!("{other:?} never classifies as a patchable rect write"),
    }
}

fn apply_patch(rect: &mut SolvedRect, patch: Patch) {
    match patch {
        Patch::X(x) => rect.x = x,
        Patch::Y(y) => rect.y = y,
        Patch::W(w) => rect.w = w,
        Patch::H(h) => rect.h = h,
    }
}

/// Drives one resolved scalar value into the arena, by the binding's
/// write class: a fill channel updates the node's fill shadow and stages
/// the whole color (paint-only — no patch, no solve); a contained rect
/// channel stages the prop and patches the cached rect (A1); anything
/// else stages the prop and forces the solve. Shared by the direct flush
/// and the scheduler drain so a smoothed and an unsmoothed write cannot
/// disagree.
#[expect(
    clippy::too_many_arguments,
    reason = "one call site shape: the tick loop's split borrows of LiveScene"
)]
fn apply_scalar_write(
    txn: &mut dashscene_core::Txn<'_>,
    b: &mut ScalarBinding,
    v: f32,
    fill_shadow: &mut HashMap<NodeId, Color>,
    rotation_shadow: &mut HashMap<NodeId, [f32; 3]>,
    cached_solve: &[(NodeId, SolvedRect)],
    cached_index: &HashMap<NodeId, usize>,
    patches: &mut Vec<(usize, Patch)>,
    layout_dirty: &mut bool,
) {
    match b.class {
        // A fill channel writes one component through the node's shadow;
        // opacity is a single scalar prop, so it stages directly. Both are
        // paint-only — no patch, no solve.
        WriteClass::PaintOnly if is_fill(b.channel) => {
            let color = fill_shadow
                .get_mut(&b.node)
                .expect("every fill-bound node has a seeded shadow");
            *fill_component(color, b.channel) = v;
            txn.set_prop(b.node, Prop::Fill(*color));
        }
        // A rotation channel writes one of three components through the
        // node's rotation shadow, for the reason `prop_for` gives: the
        // other two must survive the write rather than be invented.
        WriteClass::PaintOnly if is_rotation(b.channel) => {
            let rotation = rotation_shadow
                .get_mut(&b.node)
                .expect("every rotation-bound node has a seeded shadow");
            *rotation_component(rotation, b.channel) = v;
            txn.set_prop(b.node, rotation_prop(*rotation));
        }
        WriteClass::PaintOnly => {
            txn.set_prop(b.node, prop_for(b.channel, v));
        }
        WriteClass::Patch => {
            txn.set_prop(b.node, prop_for(b.channel, v));
            // A node under an unshown root has no committed rect, so there is
            // no row to patch (story #838). The intent is staged above and
            // takes effect if that root is ever shown; a patch is an overlay
            // on a solved rect and this node has none. Indexing here rather
            // than asking would panic in the frame loop on any document whose
            // unshown artboards carry a bound node — which is every
            // multi-artboard Figma file with a variable on the second board.
            let Some(&idx) = cached_index.get(&b.node) else {
                b.last_applied = v;
                return;
            };
            let patch = resolve_patch(b.channel, v, b.parent, cached_solve, cached_index);
            patches.push((idx, patch));
        }
        WriteClass::Solve => {
            txn.set_prop(b.node, prop_for(b.channel, v));
            *layout_dirty = true;
        }
    }
    b.last_applied = v;
}

/// A [`LayoutSolver`] that reports only the rects a contained write
/// patched this tick — not the whole retained cache. `commit_with`'s own
/// carry-forward (core issue #164: "a solver may report only the nodes
/// that moved... leaves the rest... to carry forward from the previous
/// commit") supplies every other node's rect unchanged, so publishing a
/// contained-scalar change never touches, let alone allocates, the whole
/// retained geometry (debt #191). Feeding it to `commit_with` never invokes
/// the real solver's `solve`, which is how a contained write performs no
/// layout solve (A1). Core is unchanged: the "no solve" decision lives
/// entirely in `dashlang`.
///
/// # The text halves are forwarded, and only they (issue #621)
///
/// `solve` is replaced; [`LayoutSolver::atlases`] and
/// [`LayoutSolver::stage_text`] are handed straight to `inner`. Taking
/// their defaults instead is what issue #621 was: both default to nothing,
/// `Txn::commit_with` rebuilds the glyph-run table from whatever the solver
/// stages and carries nothing forward, so **every glyph run disappeared on
/// a paint-only commit** and came back only on the next frame that solved.
/// A scene whose only animation is a changing string blanked that string
/// the moment it changed, which is the plainest use of the reactive text
/// binding there is.
///
/// Forwarding them costs no layout solve, which is why it does not argue
/// with the paragraph above. `atlases` is an `Arc::clone` of a build
/// artifact. `stage_text` shapes each text node against the `geometry`
/// closure `commit_with` supplies — the rects this commit just published,
/// which for this solver are the retained cache with the tick's patches
/// applied — and reads no Taffy tree. `FlipOverlay`, directly below, made
/// the same call for the same reason and says so.
struct CachedSolver<'a> {
    /// This tick's changed rects only, built fresh by `patched_rects` for
    /// one `commit_with` call.
    rects: Vec<(NodeId, SolvedRect)>,
    /// The scene's real solver, for the two halves that are forwarded
    /// rather than replaced.
    inner: &'a mut dyn LayoutSolver,
}

impl LayoutSolver for CachedSolver<'_> {
    fn solve(&mut self, _arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        // Moved, not cloned: `self.rects` is already this call's answer,
        // built for exactly one `solve`.
        std::mem::take(&mut self.rects)
    }

    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        self.inner.atlases()
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        self.inner.stage_text(arena, geometry)
    }
}

/// A [`LayoutSolver`] that runs the real solve and then writes this tick's
/// FLIP state back over its answer (story #771, finding 3 on PR #865).
///
/// A layout-dirty tick has to re-solve inside the commit, and the two
/// reasons pull in the same direction: the solved layout is authoritative
/// for everything the tick's other writes moved, and glyph runs are staged
/// by the commit **only** when the real solver runs inside it
/// (`corpus/showcase/tests/badge.rs` states that coupling and fails
/// without it). But the solve answers with the switch's *destination*
/// layout, and a switch that publishes its destination has landed in one
/// frame — which is the behaviour a declared transition replaces.
///
/// So the solve does not move; this wraps it. Two things are written back
/// over its answer, and both are needed because a solver may report only
/// the nodes that moved (core issue #164) and the retained solver reports
/// a node **once**: a second solve in the same tick, after the switch's
/// own solve in step 0, returns nothing for the nodes the switch moved,
/// and `commit_with` would then carry their pre-switch rects forward.
struct FlipOverlay<'a> {
    inner: &'a mut dyn LayoutSolver,
    /// The rects the switch's own solve moved, from step 0 — including the
    /// nodes carrying no track, which no sample would report.
    reflowed: &'a [(NodeId, SolvedRect)],
    /// This frame's FLIP samples, one per animating channel. Applied per
    /// channel rather than as a whole rect, so a node whose size the solve
    /// changed this tick keeps that and travels on the animating axis only.
    samples: &'a [(NodeId, Patch)],
    /// The retained cache, as the base for a sampled node that neither the
    /// solve nor the switch reported — every frame of a transition after
    /// the one that started it.
    cached_solve: &'a [(NodeId, SolvedRect)],
    cached_index: &'a HashMap<NodeId, usize>,
}

impl LayoutSolver for FlipOverlay<'_> {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        let mut rects = self.inner.solve(arena);

        // The switch's rects, and only where the solve is silent: a node
        // this tick's other writes also moved was reported just now, and
        // that answer accounts for both changes.
        for (node, rect) in self.reflowed {
            if !rects.iter().any(|(n, _)| n == node) {
                rects.push((*node, *rect));
            }
        }

        // Then the samples. A node absent from both is appended from the
        // cache, which is what carries a transition through a tick that
        // reflowed for an unrelated reason.
        for (node, patch) in self.samples {
            match rects.iter_mut().find(|(n, _)| n == node) {
                Some((_, rect)) => apply_patch(rect, *patch),
                // A node the cache does not hold is one under an unshown root
                // (story #838): the committed table it is built from covers
                // the shown root's subtree only. There is no base rect to
                // apply the sample to and nothing draws the node, so the
                // sample is dropped rather than indexed for.
                None => {
                    if let Some(&at) = self.cached_index.get(node) {
                        let mut rect = self.cached_solve[at].1;
                        apply_patch(&mut rect, *patch);
                        rects.push((*node, rect));
                    }
                }
            }
        }

        rects
    }

    // The other two halves of the seam are forwarded, never defaulted.
    // Both carry a default that stages nothing, so a wrapper that omits
    // them compiles and silently commits a scene with no text at all:
    // omitting them here cost the badge scene every glyph run it had
    // (`corpus/showcase/tests/badge.rs`), with nothing but that test to
    // say so. `geometry` already resolves to the rects `solve` returned
    // above, so a run on a travelling node is placed against the sample
    // this frame published rather than its destination.

    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        self.inner.atlases()
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        self.inner.stage_text(arena, geometry)
    }
}

/// Applies every patch to the retained cache in place, then returns just
/// the entries this tick changed — the size of a tick's dirty writes, not
/// the size of the scene (debt #191). A node touched by more than one
/// patch (e.g. both `X` and `Width` bound to signals dirty in the same
/// tick) reports once, with every one of its patches already applied, so
/// `commit_with` never sees the same [`NodeId`] twice (P4).
fn patched_rects(
    cached_solve: &mut [(NodeId, SolvedRect)],
    patches: &[(usize, Patch)],
) -> Vec<(NodeId, SolvedRect)> {
    for (i, patch) in patches {
        apply_patch(&mut cached_solve[*i].1, *patch);
    }
    let mut changed: Vec<usize> = patches.iter().map(|(i, _)| *i).collect();
    changed.sort_unstable();
    changed.dedup();
    changed.into_iter().map(|i| cached_solve[i]).collect()
}

/// One node's value on one rect channel, in a solved layout (story #771).
/// `None` when the layout does not place that node.
fn channel_value(rects: &[(NodeId, SolvedRect)], node: NodeId, channel: Channel) -> Option<f32> {
    let rect = rects.iter().find(|(n, _)| *n == node).map(|(_, r)| r)?;
    Some(match channel {
        Channel::X => rect.x,
        Channel::Y => rect.y,
        Channel::Width => rect.w,
        Channel::Height => rect.h,
        // Only rect channels reach here: the load gate refuses a motion
        // track naming any other channel, because FLIP animates rects only.
        other => unreachable!("{other:?} is not a rect channel"),
    })
}

/// A FLIP sample as a cache patch. The sample is an absolute resolved
/// value — the track was bound from one solved rect to another — so it
/// replaces the channel rather than being resolved against a parent, which
/// is what separates it from a binding write.
fn flip_patch(channel: Channel, value: f32) -> Patch {
    match channel {
        Channel::X => Patch::X(value),
        Channel::Y => Patch::Y(value),
        Channel::Width => Patch::W(value),
        Channel::Height => Patch::H(value),
        other => unreachable!("{other:?} is not a rect channel"),
    }
}

/// A declared curve as `dashcue`'s (story #771).
///
/// `dashscene-core` mirrors the vocabulary rather than depending on
/// `dashcue`, so a loaded transition arrives as core's type and is
/// converted here — the crate that depends on both, and the one that owns
/// the scheduler these specs are started on.
fn flip_spec(spec: &dashscene_core::TransitionSpec) -> TransitionSpec {
    match spec {
        dashscene_core::TransitionSpec::Tween { duration, easing } => TransitionSpec::Tween {
            duration: *duration,
            easing: match easing {
                dashscene_core::Easing::Linear => dashcue::Easing::Linear,
                dashscene_core::Easing::EaseIn => dashcue::Easing::EaseIn,
                dashscene_core::Easing::EaseOut => dashcue::Easing::EaseOut,
                dashscene_core::Easing::EaseInOut => dashcue::Easing::EaseInOut,
            },
        },
        dashscene_core::TransitionSpec::Spring {
            stiffness,
            damping_ratio,
        } => TransitionSpec::Spring {
            stiffness: *stiffness,
            damping_ratio: *damping_ratio,
        },
        dashscene_core::TransitionSpec::Keyframes { duration, frames } => {
            TransitionSpec::Keyframes {
                duration: *duration,
                frames: frames
                    .iter()
                    .map(|frame| dashcue::Keyframe {
                        t: frame.t,
                        value: frame.value,
                    })
                    .collect(),
            }
        }
    }
}

/// Accumulates resolved bindings while the node tree is staged.
#[derive(Default)]
struct BuildCtx {
    scalar: Vec<ScalarBinding>,
    text: Vec<TextBinding>,
    visible: Vec<VisibleBinding>,
    scalar_closures: Vec<Box<dyn Fn(f32) -> f32>>,
    text_closures: Vec<Box<dyn Fn(f32) -> String>>,
    /// The core `SignalId` of each `dashlang` scalar signal, in
    /// declaration order — the ids `stage_live` stages binding rows
    /// against (story #167: the binding table is a document construct,
    /// so `build_live` records every declarative binding in the arena).
    core_signals: Vec<SignalId>,
    /// Per-node authored rotation for the rotation-channel shadow, seeded
    /// for every rotation-bound node so a binding driving one of the three
    /// components keeps the other two: `[angle, anchor_x, anchor_y]`.
    rotation_shadow: HashMap<NodeId, [f32; 3]>,
    /// Per-node authored fill for the fill-channel shadow, seeded for
    /// every node that binds a fill channel.
    fill_shadow: HashMap<NodeId, Color>,
}

/// A live, bindable scene: signal state, the resolved binding tables,
/// the `dashcue` scheduler, and the retained geometry that lets a
/// contained write skip the solve. Produced by [`Scene::build_live`],
/// advanced by [`LiveScene::tick`].
///
/// A `LiveScene` assumes it solely owns the committed geometry of its
/// arena between ticks: a no-solve tick replays the whole retained cache
/// ([`LiveScene::tick`]), so a second producer that mutates the same
/// arena's nodes between ticks would be overwritten by the cached
/// geometry. Give a live scene its own arena.
/// The largest animation step one frame may take, in seconds.
///
/// A stalled host must not hand the scheduler the whole stall as one step: a
/// backgrounded browser tab is throttled to a frame a second or stopped
/// outright, and a native window can be dragged, resized or paused for as long
/// as someone holds the mouse. The frame that ends such a gap advances by this
/// much and no more.
///
/// **100 ms is a choice, not a derivation.** It has to sit above ordinary
/// hitches or it fires in normal operation, and nothing distinguishes it from
/// Unity's 333 ms without a frame budget this project does not have. What the
/// binding rule fixes is that every host clamps at the *same* value and that
/// the value is configured rather than inherited from an engine default —
/// `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
///
/// # Why it lives here and not in a host
///
/// It was written twice, in two different units:
/// `Duration::from_millis(100)` in the native host and `f64 = 0.1` in the
/// browser one, so holding the two in step already required a unit conversion
/// that nothing performed. Between two `publish = false` demonstrations that is
/// a minor flaw; stories #741 and #794 turn those hosts into two *published*
/// integration crates, at which point it becomes a semver-bound agreement that
/// nothing checks. Story #810 moved it here, and
/// `demo/tests/host_policy_invariant.rs` is what keeps it here.
///
/// The clock stays with the host, which is the other half of that record's
/// title. A host decides when its own clock is stopped — the first frame, and
/// the frames that end a parked loop, both of which start from zero — because
/// that is a fact about the host's timeline. What it no longer decides is how
/// large a step is too large.
pub const MAX_FRAME_DELTA: f32 = 0.1;

pub struct LiveScene {
    solver: Box<dyn LayoutSolver>,
    scheduler: Scheduler,
    scalars: Vec<f32>,
    bools: Vec<bool>,
    scalar_dirty: Vec<bool>,
    bool_dirty: Vec<bool>,
    scalar_bindings: Vec<ScalarBinding>,
    text_bindings: Vec<TextBinding>,
    visible_bindings: Vec<VisibleBinding>,
    scalar_closures: Vec<Box<dyn Fn(f32) -> f32>>,
    text_closures: Vec<Box<dyn Fn(f32) -> String>>,
    /// Scheduler key (`PropKey.0`) → index into `scalar_bindings`, for
    /// the smoothed bindings only.
    key_index: HashMap<u64, usize>,
    /// The rows a declared loop track writes through (story #772), and the
    /// scheduler key of each, parallel to `scalar_bindings`/`key_index`.
    ///
    /// A loop is a `ScalarBinding` whose value comes from the scheduler
    /// rather than from a signal, which is what lets it reuse
    /// `apply_scalar_write` — and with it the fill and rotation shadows,
    /// so a loop driving one fill component keeps the other three. Held
    /// apart from `scalar_bindings` because the flush in step 1 walks that
    /// table by signal dirtiness, and a loop has no signal to be dirty.
    loop_bindings: Vec<ScalarBinding>,
    loop_index: HashMap<u64, usize>,
    /// The last solved geometry, in DFS/committed order.
    cached_solve: Vec<(NodeId, SolvedRect)>,
    cached_index: HashMap<NodeId, usize>,
    /// The active member of every variant set in the arena, as of the end
    /// of the last tick (story #771).
    ///
    /// A `set_variant` is staged on the **arena**, which this scene does not
    /// own — an embedder reaches it through the host's scene seam, and
    /// nothing tells this type that it happened. Comparing against this
    /// snapshot each tick is the detection, and it is why a loaded document
    /// can animate at all: without it a switch would be absorbed by the idle
    /// early return below, which is exactly what issue #617 observed.
    variant_active: Vec<usize>,
    /// The rect channels a variant switch is currently animating, as
    /// `PropKey.0` → the node and channel a sample patches.
    ///
    /// These tracks live on the same `scheduler` the smoothed bindings use,
    /// so one `advance` drains both and one idle test covers both. The map
    /// is what tells the drain loop which kind of write a sampled key is:
    /// a key in `key_index` is a binding, a key here is a FLIP track.
    flip_tracks: HashMap<u64, (NodeId, Channel)>,
    /// Runtime lookup name → scalar signal id, for the named signals
    /// (story #167): `Scene::signal_named` declarations, or a loaded
    /// document's signal names ([`attach_live`]).
    names: HashMap<String, u32>,
    /// The current rotation of every node with a rotation-channel binding,
    /// as `[angle, anchor_x, anchor_y]`. The counterpart of `fill_shadow`
    /// below, for the same reason: three channels address one
    /// three-component prop.
    rotation_shadow: HashMap<NodeId, [f32; 3]>,
    /// The current fill color of every node with a fill-channel binding.
    /// One channel writes one component; the shadow is what makes the
    /// other three keep their values across that write.
    fill_shadow: HashMap<NodeId, Color>,
    generation: u64,
    /// The generation a host has reported as shown, through
    /// [`LiveScene::mark_shown`]. `None` before the first one.
    ///
    /// This is host-facing state held here rather than in each host, for the
    /// same reason [`MAX_FRAME_DELTA`] is: it was written twice, and stories
    /// #741 and #794 would have published both copies. Holding it beside
    /// `generation` also makes one rule structural that was previously a thing
    /// each host had to remember — a rebuild produces a new `LiveScene` whose
    /// generations count from a new arena, and this field starts `None` with
    /// it, so no host can forget to clear a `shown` that no longer means
    /// anything.
    shown: Option<u64>,
}

impl LiveScene {
    /// Every variant set whose active member changed since the last tick,
    /// as `(set, new member)`, updating the snapshot as it goes (story
    /// #771).
    ///
    /// A set added to the arena since the last tick counts as switched only
    /// if its active member is not 0, which is the same rule the loader
    /// applies when replaying a document's own `active_member`: member 0 is
    /// the state the base node values already express, so adopting it
    /// animates nothing.
    fn switched_variants(&mut self, arena: &Arena) -> Vec<(VariantSetId, usize)> {
        let mut switched = Vec::new();
        for (i, set) in arena.variant_sets().enumerate() {
            let active = arena.active_variant(set);
            match self.variant_active.get_mut(i) {
                Some(previous) if *previous == active => {}
                Some(previous) => {
                    *previous = active;
                    switched.push((set, active));
                }
                None => {
                    self.variant_active.push(active);
                    if active != 0 {
                        switched.push((set, active));
                    }
                }
            }
        }
        switched
    }

    /// Binds the transition declared for `member` from the `before` layout
    /// to the `after` one, onto this scene's scheduler (story #771).
    ///
    /// A member with no declared transition starts no track, which lands the
    /// switch in one frame — the behaviour every scene had before v0.18, and
    /// what a document that carries no motion rows still gets.
    ///
    /// A track whose before and after values are equal starts no track
    /// either (debt #487): it has nothing to animate, and declining it
    /// leaves every other track's stagger delay where it was, because a
    /// delay is computed from the track's declared index and not from how
    /// many started.
    fn start_variant_flip(
        &mut self,
        arena: &Arena,
        set: VariantSetId,
        member: usize,
        before: &[(NodeId, SolvedRect)],
        after: &[(NodeId, SolvedRect)],
    ) {
        let Some(declared) = arena.variant_transition(set, member) else {
            return;
        };
        for (i, track) in declared.tracks.iter().enumerate() {
            let (Some(from), Some(to)) = (
                channel_value(before, track.node, track.channel),
                channel_value(after, track.node, track.channel),
            ) else {
                // A track naming a node absent from either layout. The load
                // gate rejects an out-of-range node, so this is a node the
                // solver did not place — a hidden one — and it has no rect
                // to travel between.
                continue;
            };
            if from == to {
                continue;
            }
            let key = PropKey(dashscene_core::prop_key(track.node, track.channel));
            self.scheduler.start(
                key,
                from,
                to,
                flip_spec(&track.spec),
                declared.stagger * i as f32,
            );
            self.flip_tracks.insert(key.0, (track.node, track.channel));
        }
    }

    /// Push a new value into a signal. Marks the signal's bindings for
    /// the next flush; nothing is recomputed until [`LiveScene::tick`]
    /// (push-on-flush, P3).
    pub fn set<T: SignalValue>(&mut self, signal: Signal<T>, value: T) {
        T::store(self, signal.id, value);
    }

    /// The scalar signal declared under `name` — a `Scene::signal_named`
    /// declaration, or a loaded document's signal (story #167: a Figma
    /// variable's mode-qualified name). `None` when no signal carries the
    /// name.
    pub fn signal_named(&self, name: &str) -> Option<Signal<f32>> {
        self.names.get(name).map(|&id| Signal {
            id,
            marker: PhantomData,
        })
    }

    /// The generation of the most recent commit.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the committed generation has moved since the last
    /// [`mark_shown`](Self::mark_shown) — that is, whether this frame is worth
    /// drawing.
    ///
    /// `true` before the first `mark_shown`, because a scene nobody has drawn
    /// yet always has something to show.
    ///
    /// # Why the gate lives here
    ///
    /// Both hosts held an `Option<u64>` of their own and compared it against
    /// the value [`tick`](Self::tick) had just returned. That is one rule
    /// written twice, and stories #741 and #794 would have published a copy in
    /// each integration crate (story #810,
    /// `docs/decisions/crate-name-map.md`). Holding it beside `generation`
    /// also removes the step a host used to have to remember: a rebuild makes
    /// a new `LiveScene` over a new arena whose generations restart, and the
    /// gate starts clear with it rather than needing to be cleared.
    ///
    /// # What it does not mean
    ///
    /// It answers "has anything changed", not "was the last frame drawn". A
    /// host records a generation as shown when its present call returns, and a
    /// present can return `Ok` without drawing — a zero extent, an occluded
    /// window, or an acquire that timed out. Nothing here tries to detect
    /// that, and a host should not either: the generation travels with the
    /// dirty set to the renderer precisely so a broken chain is caught by
    /// arithmetic rather than by anyone remembering to report it
    /// (`crates/dashscene-gpu/src/render.rs`).
    pub fn advanced(&self) -> bool {
        self.shown != Some(self.generation)
    }

    /// Record the committed generation as shown, so [`advanced`](Self::advanced)
    /// reports `false` until the next one that moves it.
    pub fn mark_shown(&mut self) {
        self.shown = Some(self.generation);
    }

    /// Advance one frame: open one `Txn`, flush every dirty binding,
    /// advance the scheduler and write each live track, then commit (D4).
    /// A frame that changed only paint props or only contained scalars
    /// commits through the retained geometry and never calls the real
    /// solver; a frame that reflowed re-solves and refreshes the cache.
    pub fn tick(&mut self, dt: f32, arena: &mut Arena) -> u64 {
        // The delta is clamped here, not by the caller (story #810). A host
        // hands over the raw interval its own clock measured and this decides
        // how much of it the scheduler may advance, so the rule has one
        // statement and no host can hold a copy that drifts.
        //
        // `max` then `min` rather than `clamp`, deliberately: `f32::clamp`
        // returns NaN for a NaN input, and `dashcue::Scheduler::advance`
        // opens with `assert!(dt.is_finite() && dt >= 0.0)`. So a NaN delta
        // reaching it takes the frame loop down rather than being absorbed —
        // one bad timestamp from a host would panic the runtime. `f32::max`
        // returns the non-NaN operand instead, so NaN becomes `0.0` and the
        // frame is treated as taking no time.
        // `crates/dashlang/tests/frame_policy.rs` pins that difference, and
        // swapping this for `clamp` fails it by that panic.
        //
        // The lower bound also absorbs a negative interval, which the browser
        // host guarded with a `max(0.0)` of its own before the rule moved.
        #[allow(clippy::manual_clamp)]
        let dt = dt.max(0.0).min(MAX_FRAME_DELTA);

        // A variant switch staged on the arena since the last tick (story
        // #771). Detected before the idle test, because a switch changes no
        // signal and starts no track by itself: without this the frame would
        // take the early return below and the switch would never be seen,
        // which is what issue #617 measured against every committed fixture.
        let switched = self.switched_variants(arena);

        // Idle frame: no signal changed and no track is still live — a track
        // that finished but has not yet been swept produces no sample, so
        // `is_settled` (not `is_empty`) is the right test. A commit would only
        // churn the generation while nothing moved (D4), so skip it and hold
        // the generation steady, keeping it a meaningful "something changed"
        // signal for a downstream consumer.
        // A shown root staged since the last tick, detected the same way and
        // for the same reason as the variant switch above (story #838).
        // `Txn` has no `Drop` that reverts, so `show_root` leaves the arena
        // changed and uncommitted exactly as `set_variant` used to — and a
        // change of shown root moves no signal and starts no track, so
        // without this the idle return below would swallow it and the host
        // would go on painting the artboard it was already showing, forever.
        // That is issue #617's shape, on the state this story added.
        let shown_root_staged = arena.shown_root() != arena.committed().shown_root();

        if switched.is_empty()
            && !shown_root_staged
            && self.scheduler.is_settled()
            && !self.scalar_dirty.iter().any(|&d| d)
            && !self.bool_dirty.iter().any(|&d| d)
        {
            return self.generation;
        }

        let mut layout_dirty = false;
        let mut patches: Vec<(usize, Patch)> = Vec::new();
        // The same FLIP samples as `patches`, keyed by node rather than by
        // cache index: a layout-dirty tick commits through the real solver
        // and overlays them on its answer, where a cache index means
        // nothing (`FlipOverlay`).
        let mut flip_samples: Vec<(NodeId, Patch)> = Vec::new();

        // 0. A variant switch: re-solve for the layout the new members
        //    produce, bind the declared transition from the old layout to it,
        //    and adopt it as the cache the samples below patch.
        //
        //    The solve is called directly rather than through a commit
        //    because the two are needed at different moments: the *after*
        //    rects are what a FLIP track's `to` is, and the commit at the end
        //    of this tick must publish the first **sample**, not the after
        //    layout — committing the after layout here would land the switch
        //    in one frame, which is the behaviour this replaces. A staged
        //    `set_variant` is visible to the solver already (P3), so one
        //    solve answers it.
        let mut switch_moved: Vec<(NodeId, SolvedRect)> = Vec::new();
        if !switched.is_empty() {
            // **One** solve for every set switched since the last tick, not
            // one per set. `Txn::set_variant` is visible to the solver the
            // moment it is staged, so the first solve already reflects all of
            // them; a second would see `from == to` for the next set's tracks
            // and decline every one, leaving every set but the first to snap.
            switch_moved = self.solver.solve(arena);

            let before = std::mem::take(&mut self.cached_solve);
            let mut index = std::mem::take(&mut self.cached_index);
            let mut after = before.clone();

            // Merge, never replace. `LayoutSolver::solve` is allowed to
            // return only the nodes whose rect changed, and `TaffySolver`
            // does exactly that — so assigning its result would drop every
            // unmoved node from the cache permanently, and the next contained
            // write to one would panic on `cached_index`. `Txn::commit_with`
            // carries the rest forward, and this cache exists to mirror what
            // it carries.
            for (node, rect) in &switch_moved {
                match index.get(node) {
                    Some(&i) => after[i].1 = *rect,
                    None => {
                        index.insert(*node, after.len());
                        after.push((*node, *rect));
                    }
                }
            }

            for (set, member) in &switched {
                self.start_variant_flip(arena, *set, *member, &before, &after);
            }
            self.cached_solve = after;
            self.cached_index = index;
        }

        let mut txn = arena.open();

        // 1. Scalar bindings whose source signal changed. A smoothed
        //    binding sets the spring's target; the scheduler drains
        //    below. A direct binding writes now: a fill channel through
        //    the fill shadow (paint-only), a contained rect channel as a
        //    cache patch, anything else forces a solve.
        for b in &mut self.scalar_bindings {
            if !self.scalar_dirty[b.signal as usize] {
                continue;
            }
            let raw = self.scalars[b.signal as usize];
            let v = eval_scalar(&b.transform, &self.scalar_closures, raw);
            if let Some(spec) = &b.smoothing {
                let from = self.scheduler.sample(b.key).unwrap_or(b.last_applied);
                self.scheduler.start(b.key, from, v, spec.clone(), 0.0);
            } else {
                apply_scalar_write(
                    &mut txn,
                    b,
                    v,
                    &mut self.fill_shadow,
                    &mut self.rotation_shadow,
                    &self.cached_solve,
                    &self.cached_index,
                    &mut patches,
                    &mut layout_dirty,
                );
            }
        }

        // 2. Text bindings. Text inside a fixed box is paint-only: it
        //    never forces a solve (A2). A hug text node that reflows on
        //    text change needs the measure callback and is future work.
        for b in &self.text_bindings {
            if !self.scalar_dirty[b.signal as usize] {
                continue;
            }
            let raw = self.scalars[b.signal as usize];
            let s = eval_text(&b.transform, &self.text_closures, raw);
            txn.set_prop(b.node, Prop::Text(s));
        }

        // 3. Visibility bindings. `Visible` is layout-affecting (D7): a
        //    flip reflows siblings, so it always forces a solve (A3, A4).
        for b in &self.visible_bindings {
            if !self.bool_dirty[b.signal as usize] {
                continue;
            }
            txn.set_prop(b.node, Prop::Visible(self.bools[b.signal as usize]));
            layout_dirty = true;
        }

        // 4. Advance the scheduler and write every live track. In-flight
        //    springs keep producing values after their signal stopped
        //    changing, so this drains every frame, independent of the
        //    dirty set.
        self.scheduler.advance(dt);
        let samples: Vec<(PropKey, f32)> = self.scheduler.samples().collect();
        for (key, value) in samples {
            // Two kinds of track share this scheduler. A key in `key_index`
            // is a smoothed binding and writes through the binding path; a
            // key in `flip_tracks` is a variant FLIP and patches one rect
            // channel directly, because its `from`/`to` were bound from
            // resolved rects and the sample is already absolute.
            if let Some(&bi) = self.key_index.get(&key.0) {
                apply_scalar_write(
                    &mut txn,
                    &mut self.scalar_bindings[bi],
                    value,
                    &mut self.fill_shadow,
                    &mut self.rotation_shadow,
                    &self.cached_solve,
                    &self.cached_index,
                    &mut patches,
                    &mut layout_dirty,
                );
            } else if let Some(&li) = self.loop_index.get(&key.0) {
                // A declared loop (story #772), written through the same
                // path a smoothed binding takes — its row carries the write
                // class and the shadows, so one fill component loops
                // without inventing the other three. The gate holds it to a
                // paint channel, so this never patches a rect and never
                // sets `layout_dirty`: a track that never settles still
                // commits through the retained-geometry replay.
                apply_scalar_write(
                    &mut txn,
                    &mut self.loop_bindings[li],
                    value,
                    &mut self.fill_shadow,
                    &mut self.rotation_shadow,
                    &self.cached_solve,
                    &self.cached_index,
                    &mut patches,
                    &mut layout_dirty,
                );
            } else if let Some(&(node, channel)) = self.flip_tracks.get(&key.0) {
                let idx = self.cached_index[&node];
                let patch = flip_patch(channel, value);
                patches.push((idx, patch));
                flip_samples.push((node, patch));
            } else {
                unreachable!(
                    "scheduler key {key:?} is not a smoothed binding, a declared loop or a \
                     FLIP track; every track this scene starts is registered in one of the \
                     three — `key_index`, `loop_index` or `flip_tracks`"
                );
            }
        }

        // 5. One commit. A reflow re-solves and refreshes the cache; a
        //    contained/paint-only frame patches the cache and replays it,
        //    so the real solver is never called.
        //
        //    A change of shown root takes the reflow arm whatever the writes
        //    did (story #838). The cached arms replay `cached_solve`, which
        //    holds the rects of the root that *was* shown, so the newly shown
        //    subtree would reach `commit_with` with no rect for any of its
        //    nodes — which it refuses by name (P4). Only the real solver has
        //    the answer, and `TaffySolver` already reads the whole new subtree
        //    back rather than pruning it for exactly this call.
        let layout_dirty = layout_dirty || shown_root_staged;
        let generation = if layout_dirty {
            // The real solver runs inside the commit, so the layout is
            // authoritative and glyph runs stage; this frame's switch and
            // samples are written back over its answer, so a transition
            // sharing a tick with a reflow still travels (`FlipOverlay`).
            let mut overlay = FlipOverlay {
                inner: &mut *self.solver,
                reflowed: &switch_moved,
                samples: &flip_samples,
                cached_solve: &self.cached_solve,
                cached_index: &self.cached_index,
            };
            txn.commit_with(&mut overlay)
        } else if !switch_moved.is_empty() {
            // The switch tick publishes two sets at once: every node the
            // reflow moved — including the ones carrying no track, which
            // patches alone would not report — and every node this frame's
            // FLIP sample patched. The union, not the whole scene: the
            // commit carries the rest forward.
            for (i, patch) in &patches {
                apply_patch(&mut self.cached_solve[*i].1, *patch);
            }
            let mut changed: Vec<usize> = patches.iter().map(|(i, _)| *i).collect();
            changed.extend(
                switch_moved
                    .iter()
                    .filter_map(|(node, _)| self.cached_index.get(node).copied()),
            );
            changed.sort_unstable();
            changed.dedup();
            let mut cached = CachedSolver {
                rects: changed.into_iter().map(|i| self.cached_solve[i]).collect(),
                inner: &mut *self.solver,
            };
            txn.commit_with(&mut cached)
        } else {
            let mut cached = CachedSolver {
                rects: patched_rects(&mut self.cached_solve, &patches),
                inner: &mut *self.solver,
            };
            txn.commit_with(&mut cached)
        };

        // `cached_index` is rebuilt with the cache, not built once. Before
        // story #838 it could be built once: a reflow changes geometry and
        // never the committed order, so a NodeId kept its row for the life of
        // the scene. A renumbering breaks exactly that — the committed table
        // is a different subtree and every row names a different node — and a
        // stale index then patches the wrong rect, silently in a release
        // build. `shown_root_staged` above is what routes a renumbering here:
        // it forces this arm, so there is no second condition to test.
        if layout_dirty {
            refresh_cache(arena, &mut self.cached_solve);
            self.cached_index = self
                .cached_solve
                .iter()
                .enumerate()
                .map(|(i, (id, _))| (*id, i))
                .collect();
        }

        // Sweep finished FLIP tracks. `dashcue` drops a track when it
        // settles, so a key with no sample left is done; keeping the map in
        // step with the scheduler is what stops it growing with the number
        // of switches a session makes (R4).
        let scheduler = &self.scheduler;
        self.flip_tracks
            .retain(|key, _| scheduler.sample(PropKey(*key)).is_some());

        for d in &mut self.scalar_dirty {
            *d = false;
        }
        for d in &mut self.bool_dirty {
            *d = false;
        }

        self.generation = generation;
        generation
    }
}

/// Stages one binding's seed value — the build/attach-time analogue of
/// [`apply_scalar_write`], before any cache or patch list exists. A fill
/// channel writes through the shadow; everything else stages its prop.
fn seed_scalar(
    txn: &mut dashscene_core::Txn<'_>,
    b: &mut ScalarBinding,
    v: f32,
    fill_shadow: &mut HashMap<NodeId, Color>,
    rotation_shadow: &mut HashMap<NodeId, [f32; 3]>,
) {
    if is_fill(b.channel) {
        let color = fill_shadow
            .get_mut(&b.node)
            .expect("every fill-bound node has a seeded shadow");
        *fill_component(color, b.channel) = v;
        txn.set_prop(b.node, Prop::Fill(*color));
    } else if is_rotation(b.channel) {
        let rotation = rotation_shadow
            .get_mut(&b.node)
            .expect("every rotation-bound node has a seeded shadow");
        *rotation_component(rotation, b.channel) = v;
        txn.set_prop(b.node, rotation_prop(*rotation));
    } else {
        txn.set_prop(b.node, prop_for(b.channel, v));
    }
    b.last_applied = v;
}

/// Stages each loop's first sample, so the commit that publishes the
/// attached scene already shows the phase the track starts at (story
/// #772).
///
/// Without this a track with a `phase_offset` is live in the scheduler but
/// absent from the committed output until the first tick, so a host that
/// presents before it ticks draws the authored value for one frame and then
/// jumps — visible as a row of skeleton bars snapping into step.
fn seed_loops(
    txn: &mut dashscene_core::Txn<'_>,
    rows: &mut [ScalarBinding],
    scheduler: &Scheduler,
    fill_shadow: &mut HashMap<NodeId, Color>,
    rotation_shadow: &mut HashMap<NodeId, [f32; 3]>,
) {
    for b in rows {
        let v = scheduler
            .sample(b.key)
            .expect("a loop track is live from the moment it starts");
        seed_scalar(txn, b, v, fill_shadow, rotation_shadow);
    }
}

/// Builds a [`LiveScene`] from the binding tables an arena already
/// carries — the loader-side entry point (story #167): load a `.dsb`
/// with `dashscene_core::load_document`, then attach. The document's
/// signal declarations become live signals, addressable by name
/// ([`LiveScene::signal_named`] — a Figma variable's mode-qualified
/// name), and its binding rows become live scalar bindings driven by
/// [`LiveScene::tick`], exactly as if the scene had been authored with
/// `Scene::build_live`. One mechanism, two producers.
///
/// Seeds every bound channel from its signal's initial value and commits
/// once through `solver`, which the live scene then owns for reflows —
/// the same first-commit contract as `Scene::build_live`. Bindings whose
/// signal a producer never sets simply hold their seeded values.
///
/// # Panics
///
/// Panics if a binding row references a signal or node the arena does
/// not hold — impossible for tables staged through `Txn::bind`, which
/// validates both.
pub fn attach_live(arena: &mut Arena, mut solver: Box<dyn LayoutSolver>) -> LiveScene {
    let signals: Vec<dashscene_core::SignalDecl> = arena.signals().to_vec();
    let rows: Vec<dashscene_core::Binding> = arena.bindings().to_vec();

    let mut names: HashMap<String, u32> = HashMap::new();
    for (id, decl) in signals.iter().enumerate() {
        if let Some(name) = &decl.name {
            names.insert(name.clone(), id as u32);
        }
    }

    let mut fill_shadow: HashMap<NodeId, Color> = HashMap::new();
    let mut rotation_shadow: HashMap<NodeId, [f32; 3]> = HashMap::new();
    let mut bindings: Vec<ScalarBinding> = Vec::with_capacity(rows.len());
    for row in &rows {
        let node = row.node;
        let layout = arena.layout(node);
        let has_children = !arena.children(node).is_empty();
        let passthrough = layout.mode == LayoutMode::None;
        let fill = match arena.fill(node) {
            Some(FillSpec::Solid { color }) => Some(*color),
            _ => None,
        };
        if is_fill(row.channel) {
            fill_shadow
                .entry(node)
                .or_insert(fill.unwrap_or(TRANSPARENT));
        }
        // Seeded from the node's authored rotation, so a binding that drives
        // only the angle keeps the anchor the document stated, and one that
        // drives only an anchor component keeps the authored angle.
        if is_rotation(row.channel) {
            let (angle, anchor) = arena.rotation(node);
            rotation_shadow
                .entry(node)
                .or_insert([angle, anchor.0, anchor.1]);
        }
        bindings.push(ScalarBinding {
            node,
            parent: arena.parent(node),
            channel: row.channel,
            signal: row.signal.index() as u32,
            transform: from_core(row.transform),
            class: classify(
                row.channel,
                ancestor_contained(arena, node),
                has_children,
                passthrough,
            ),
            smoothing: None,
            key: prop_key(node, row.channel),
            last_applied: {
                let (angle, anchor) = arena.rotation(node);
                initial_channel_value(&layout, fill, [angle, anchor.0, anchor.1], row.channel)
            },
        });
    }

    let scalars: Vec<f32> = signals.iter().map(|s| s.initial).collect();

    // Seed every bound channel from its signal's initial value and
    // commit once through the solver — the loaded literals already agree
    // with the initials for a document the importer produced, and the
    // seed makes that an invariant for any producer.
    // The loops the document declares (story #772), started before the seed
    // commit so their phase is staged into it. A track with a `phase_offset`
    // begins partway through its cycle, and a host that attaches, presents
    // once and only then ticks would otherwise draw its authored value for
    // one frame and jump.
    let mut scheduler = Scheduler::new();
    let (mut loop_bindings, loop_index) = attach_loops(
        arena,
        &mut scheduler,
        &mut fill_shadow,
        &mut rotation_shadow,
    );

    let generation = {
        let mut txn = arena.open();
        for b in &mut bindings {
            let raw = scalars[b.signal as usize];
            let v = to_core(&b.transform)
                .expect("attached bindings carry declarative transforms only")
                .apply(raw);
            seed_scalar(&mut txn, b, v, &mut fill_shadow, &mut rotation_shadow);
        }
        seed_loops(
            &mut txn,
            &mut loop_bindings,
            &scheduler,
            &mut fill_shadow,
            &mut rotation_shadow,
        );
        txn.commit_with(&mut *solver)
    };

    let mut cached_solve = Vec::new();
    refresh_cache(arena, &mut cached_solve);
    let cached_index = cached_solve
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();

    let scalar_dirty = vec![false; scalars.len()];

    LiveScene {
        solver,
        scheduler,
        // The variant snapshot starts at each set's current active member, so
        // the state a document arrived in is the baseline rather than a switch
        // to animate on the first tick (story #771).
        variant_active: arena
            .variant_sets()
            .map(|s| arena.active_variant(s))
            .collect(),
        flip_tracks: HashMap::new(),
        scalars,
        bools: Vec::new(),
        scalar_dirty,
        bool_dirty: Vec::new(),
        scalar_bindings: bindings,
        text_bindings: Vec::new(),
        visible_bindings: Vec::new(),
        scalar_closures: Vec::new(),
        text_closures: Vec::new(),
        key_index: HashMap::new(),
        loop_bindings,
        loop_index,
        cached_solve,
        cached_index,
        names,
        fill_shadow,
        rotation_shadow,
        generation,
        shown: None,
    }
}

/// Starts one scheduler track per loop the arena declares, and returns the
/// rows the drain writes their samples through (story #772).
///
/// Shared by both construction paths on purpose. `attach_live` and
/// `stage_live` are two separate ways into a `LiveScene`, and wiring a new
/// channel into one of them only is a mistake this crate has already made
/// once — the loaded path worked and the builder panicked. A loop declared
/// on the arena is driven whichever way the scene was made.
///
/// The shadows are seeded exactly as a binding row's are, and for the same
/// reason: a loop driving one fill component must keep the other three, and
/// one driving the rotation angle must keep the authored anchor.
fn attach_loops(
    arena: &Arena,
    scheduler: &mut Scheduler,
    fill_shadow: &mut HashMap<NodeId, Color>,
    rotation_shadow: &mut HashMap<NodeId, [f32; 3]>,
) -> (Vec<ScalarBinding>, HashMap<u64, usize>) {
    let mut rows: Vec<ScalarBinding> = Vec::new();
    let mut index: HashMap<u64, usize> = HashMap::new();

    for track in arena.loop_tracks() {
        let node = track.node;
        // A loop animates paint only. The document gate refuses anything
        // else by name, but `Txn::add_loop_track` deliberately does not —
        // a hand-built arena is not held to a document rule — so a producer
        // staging a layout channel directly would reach `classify` and get
        // `WriteClass::Solve`. A loop never settles, so that would put the
        // real solver in the frame loop for as long as the document is
        // loaded, with no diagnostic. Named here instead, at the same place
        // every other cross-crate contract violation is.
        let class = classify(track.channel, false, false, false);
        assert!(
            matches!(class, WriteClass::PaintOnly),
            "loop track on {node:?} names channel {:?}, which is layout-affecting; \
             a loop animates paint channels only, because a track that never \
             settles would otherwise re-solve every frame",
            track.channel,
        );

        let fill = match arena.fill(node) {
            Some(FillSpec::Solid { color }) => Some(*color),
            _ => None,
        };
        // Read once and shared by the shadow seed and the initial value
        // below — `Arena::rotation` resolves the variant overlay, so it is
        // not a field read.
        let (angle, anchor) = arena.rotation(node);
        let rotation = [angle, anchor.0, anchor.1];
        if is_fill(track.channel) {
            fill_shadow
                .entry(node)
                .or_insert(fill.unwrap_or(TRANSPARENT));
        }
        if is_rotation(track.channel) {
            rotation_shadow.entry(node).or_insert(rotation);
        }

        let key = prop_key(node, track.channel);
        // The load gate refuses a loop that shares a channel with any other
        // writer, so this is the only track on `key` and `start_loop`'s own
        // assertion is the backstop rather than the check.
        scheduler.start_loop(
            key,
            track.from,
            track.to,
            flip_spec(&track.spec),
            track.phase_offset,
        );
        index.insert(key.0, rows.len());
        rows.push(ScalarBinding {
            node,
            parent: arena.parent(node),
            channel: track.channel,
            // No signal drives a loop. The field is never read for these
            // rows: step 1 walks `scalar_bindings`, not this table, and the
            // drain reaches them by scheduler key.
            signal: u32::MAX,
            transform: Transform::Identity,
            // A loop animates paint only (the load gate refuses anything
            // else), so this classifies as `PaintOnly` and the sample never
            // patches a rect or forces a solve. That is what keeps a
            // never-settling track off the solver.
            class,
            smoothing: None,
            key,
            // Only the paint arms are reachable — the assertion above holds
            // the channel to them — so the layout `initial_channel_value`
            // would read is never needed, and building one per track would
            // resolve the variant overlay for a value always discarded.
            last_applied: initial_channel_value(&Layout::default(), fill, rotation, track.channel),
        });
    }

    (rows, index)
}

/// Whether every ancestor of `node` keeps a size change inside `node`'s
/// own subtree — the same containment rule `stage_live` propagates
/// top-down (an ancestor contains iff it is a passthrough, non-hug
/// parent), computed bottom-up for a loaded arena.
fn ancestor_contained(arena: &Arena, node: NodeId) -> bool {
    let mut at = arena.parent(node);
    while let Some(ancestor) = at {
        let layout = arena.layout(ancestor);
        let hug = layout.sizing_h == AxisSizing::Hug || layout.sizing_v == AxisSizing::Hug;
        if layout.mode != LayoutMode::None || hug {
            return false;
        }
        at = arena.parent(ancestor);
    }
    true
}

/// Rebuild the retained geometry from the committed buffer after a real
/// solve. The DFS order is invariant for a static tree, so the
/// node→index map does not change.
fn refresh_cache(arena: &Arena, cached: &mut Vec<(NodeId, SolvedRect)>) {
    let committed = arena.committed();
    let rebuilt: Vec<(NodeId, SolvedRect)> = committed
        .rects()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                committed.node_of(i as u32),
                SolvedRect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: r.h,
                },
            )
        })
        .collect();
    // A static tree's committed order is invariant across reflows: a reflow
    // changes geometry, never the node count or DFS order (a `Visible` flip
    // keeps the node with a degenerate rect). `cached_index` is rebuilt only
    // beside this call, so a shape change here would silently misalign every
    // patch. Guard it, skipping the first call before any prior cache exists.
    //
    // A **renumbering** is the one legitimate shape change (story #838): the
    // shown root moved, so the committed table is a different subtree by
    // design. The caller rebuilds `cached_index` on exactly that condition,
    // which is why it is excused here rather than asserted through.
    debug_assert!(
        cached.is_empty()
            || committed.renumbered()
            || (cached.len() == rebuilt.len()
                && cached.iter().zip(&rebuilt).all(|(a, b)| a.0 == b.0)),
        "refresh_cache: committed node count or order changed across a reflow"
    );
    *cached = rebuilt;
}

impl Scene {
    /// Declare a signal with an initial value, returning a handle two or
    /// more nodes can bind. `T` is `f32` or `bool`.
    pub fn signal<T: SignalValue>(&mut self, initial: T) -> Signal<T> {
        let id = T::declare(self, initial);
        Signal {
            id,
            marker: PhantomData,
        }
    }

    /// Declare a named scalar signal (story #167). The name is the
    /// runtime lookup key ([`LiveScene::signal_named`]) and is staged
    /// into the document binding table, so a `dashlang` scene and an
    /// imported Figma document expose signals the same way — a Figma
    /// variable's mode-qualified name is exactly such a name.
    ///
    /// # Panics
    ///
    /// Panics when `name` is already declared on this scene: the by-name
    /// lookup would silently shadow one declaration, which is the exact
    /// condition the load gate names `signal.name-duplicate` on a
    /// serialized document — the authoring path refuses it the same way,
    /// by name (P4).
    pub fn signal_named(&mut self, name: &str, initial: f32) -> Signal<f32> {
        if self.scalar_names.iter().flatten().any(|n| n == name) {
            panic!(
                "signal_named({name:?}) is already declared on this scene: a by-name lookup \
                 would silently shadow one declaration (the load gate names the same condition \
                 signal.name-duplicate)"
            );
        }
        let signal = self.signal(initial);
        self.scalar_names[signal.id as usize] = Some(name.to_owned());
        signal
    }

    /// Set (replacing) the scene's roots. The builder form the reactive
    /// authoring surface uses: declare signals, then set the roots that
    /// bind them.
    pub fn roots(&mut self, roots: impl IntoIterator<Item = Node>) -> &mut Self {
        self.roots = roots.into_iter().collect();
        self
    }

    /// Build into `arena` and return a [`LiveScene`] the producer drives
    /// per frame. Assigns `NodeId`s, resolves every declared binding to
    /// its target, seeds each bound prop from its signal's initial value,
    /// and commits once through `solver` — which the live scene then owns
    /// and reuses for every reflow.
    ///
    /// Distinct from [`Scene::build`], which returns only a
    /// [`crate::Built`] generation: a live scene needs to retain the
    /// solver, the scheduler, and the binding tables, so it is its own
    /// entry point rather than a change to `build`'s return type
    /// (docs/decisions/dashlang-flex-vocabulary.md D3).
    pub fn build_live(self, arena: &mut Arena, mut solver: Box<dyn LayoutSolver>) -> LiveScene {
        let mut ctx = BuildCtx::default();

        // The same loop startup the loaded path runs, before the txn opens
        // because it needs `&Arena`. The builder has no loop vocabulary of
        // its own, so this finds rows only when a producer staged them on
        // the arena directly — but it is wired here rather than left out,
        // because a channel wired into one of the two paths and not the
        // other is a mistake this crate has already made once.
        let mut scheduler = Scheduler::new();
        let (mut loop_bindings, loop_index) = attach_loops(
            arena,
            &mut scheduler,
            &mut ctx.fill_shadow,
            &mut ctx.rotation_shadow,
        );

        let generation = {
            let mut txn = arena.open();
            // Every scalar signal is declared in the arena first, in
            // declaration order, so a `dashlang` signal id and its core
            // `SignalId` coincide and the staged binding rows (story
            // #167) reference the right declaration. Bool signals stay
            // `dashlang`-only: the serialized vocabulary is scalar.
            for (name, initial) in self.scalar_names.iter().zip(&self.scalar_inits) {
                ctx.core_signals
                    .push(txn.declare_signal(name.as_deref(), *initial));
            }
            for root in self.roots {
                stage_live(&mut txn, None, root, true, &mut ctx);
            }

            // Seed each bound prop from its signal's initial value, so
            // the first committed scene is consistent with the signals.
            for b in &mut ctx.scalar {
                let raw = self.scalar_inits[b.signal as usize];
                let v = eval_scalar(&b.transform, &ctx.scalar_closures, raw);
                seed_scalar(
                    &mut txn,
                    b,
                    v,
                    &mut ctx.fill_shadow,
                    &mut ctx.rotation_shadow,
                );
            }
            for b in &ctx.text {
                let raw = self.scalar_inits[b.signal as usize];
                let s = eval_text(&b.transform, &ctx.text_closures, raw);
                txn.set_prop(b.node, Prop::Text(s));
            }
            for b in &ctx.visible {
                txn.set_prop(b.node, Prop::Visible(self.bool_inits[b.signal as usize]));
            }
            seed_loops(
                &mut txn,
                &mut loop_bindings,
                &scheduler,
                &mut ctx.fill_shadow,
                &mut ctx.rotation_shadow,
            );

            txn.commit_with(&mut *solver)
        };

        let mut cached_solve = Vec::new();
        refresh_cache(arena, &mut cached_solve);
        let cached_index = cached_solve
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();

        let mut key_index = HashMap::new();
        for (i, b) in ctx.scalar.iter().enumerate() {
            if b.smoothing.is_some() {
                key_index.insert(b.key.0, i);
            }
        }

        let scalars = self.scalar_inits;
        let bools = self.bool_inits;
        let scalar_dirty = vec![false; scalars.len()];
        let bool_dirty = vec![false; bools.len()];
        let names = self
            .scalar_names
            .into_iter()
            .enumerate()
            .filter_map(|(id, name)| name.map(|n| (n, id as u32)))
            .collect();

        LiveScene {
            solver,
            scheduler,
            // The variant snapshot starts at each set's current active member, so
            // the state a document arrived in is the baseline rather than a switch
            // to animate on the first tick (story #771).
            variant_active: arena
                .variant_sets()
                .map(|s| arena.active_variant(s))
                .collect(),
            flip_tracks: HashMap::new(),
            scalars,
            bools,
            scalar_dirty,
            bool_dirty,
            scalar_bindings: ctx.scalar,
            text_bindings: ctx.text,
            visible_bindings: ctx.visible,
            scalar_closures: ctx.scalar_closures,
            text_closures: ctx.text_closures,
            key_index,
            loop_bindings,
            loop_index,
            cached_solve,
            cached_index,
            names,
            fill_shadow: ctx.fill_shadow,
            rotation_shadow: ctx.rotation_shadow,
            generation,
            shown: None,
        }
    }
}

/// Stage one node and its subtree, resolving its bindings to `NodeId`
/// targets. Consumes the node so a `Custom` transform's closure moves
/// into the closure table (closures are not `Clone`).
///
/// `contained` is whether a size change on this node stays in its own
/// subtree: true for a root (roots are independent islands), and
/// propagated to children only through a passthrough, non-hug parent
/// (the design's containment rule — every ancestor to the root is fixed
/// or fill).
fn stage_live(
    txn: &mut dashscene_core::Txn<'_>,
    parent: Option<NodeId>,
    node: Node,
    contained: bool,
    ctx: &mut BuildCtx,
) {
    let id = txn.add_node(parent, node.name.as_deref());
    crate::set_base_props(txn, id, &node);

    let Node {
        name,
        layout,
        fill,
        fill_with,
        rotation,
        scalar_bindings,
        smoothing,
        text_binding,
        visible_binding,
        children,
        ..
    } = node;

    // The node's authored rotation as the shadow's three components. The
    // builder's `None` is the arena default, unrotated.
    let rotation = rotation.map_or([0.0, 0.0, 0.0], |(angle, anchor)| {
        [angle, anchor.0, anchor.1]
    });

    let has_children = !children.is_empty();
    let passthrough = layout.mode == LayoutMode::None;

    // A smoothing spec with no binding on the same channel would be
    // silently inert — the spring has no signal to take targets from.
    // Named at build, never dropped (P4, debt #194).
    for (channel, _) in &smoothing {
        if !scalar_bindings.iter().any(|(c, _)| c == channel) {
            panic!(
                "smooth({channel:?}) on node {name:?} has no matching bind({channel:?}, ...): \
                 the spring would be silently inert (debt #194)"
            );
        }
    }

    // A `fill_with` paint and a fill-channel binding on the same node
    // cannot both survive: the binding drives one component of a solid
    // color through the node's fill shadow, and every write it makes is a
    // `Prop::Fill`, which replaces the node's whole fill slot. The
    // authored gradient or image fill would be gone from the first
    // committed frame, with nothing reporting it. Named at build, the
    // same way an inert spring is (P4).
    if fill_with.is_some()
        && let Some((channel, _)) = scalar_bindings.iter().find(|(c, _)| is_fill(*c))
    {
        panic!(
            "fill_with(...) and bind({channel:?}, ...) on node {name:?} cannot be combined: \
             a fill-channel binding writes one component of a solid color and stages the \
             whole color as Prop::Fill, which would silently replace the authored gradient \
             or image fill. Bind the fill channels of a solid fill(...) instead, or drop \
             the fill-channel binding and keep fill_with(...)"
        );
    }

    for (channel, expr) in scalar_bindings {
        let ScalarExpr { signal, kind } = expr;
        let transform = match kind {
            ScalarKind::Data(t) => t,
            ScalarKind::Custom(f) => {
                let closure_id = ClosureId(ctx.scalar_closures.len() as u32);
                ctx.scalar_closures.push(f);
                Transform::Custom(closure_id)
            }
        };
        // The binding table is a document construct (story #167): every
        // declarative binding is staged into the arena as a row, so a
        // dashlang scene and a loaded document expose one table. A
        // `Custom` transform stays dashlang-only — its closure does not
        // serialize (D8) — so it lives in the live tables but never in
        // the arena's.
        if let Some(core_transform) = to_core(&transform) {
            txn.bind(
                id,
                channel,
                ctx.core_signals[signal as usize],
                core_transform,
            );
        }
        // The shadow seeds from the solid `fill` only, and never from
        // `fill_with`: a gradient or an image has no four-component color
        // for the channels to address, so there is nothing to merge a
        // component write into. Preferring one over the other — dropping
        // the binding, or dropping the paint — would be exactly the silent
        // loss P4 forbids, so the combination is refused by name above
        // instead, and this seed only ever sees a node whose authored fill
        // is solid or absent.
        if is_fill(channel) {
            ctx.fill_shadow
                .entry(id)
                .or_insert(fill.unwrap_or(TRANSPARENT));
        }
        // The rotation counterpart, and for the same reason: three channels
        // address one three-component prop, so a binding driving one of them
        // needs the other two to survive the write rather than be invented.
        // `attach_live` seeds this from the loaded node; here the scene is
        // being staged, so it seeds from what the builder authored.
        //
        // Without this seed `seed_scalar`'s `expect` fires on the first
        // rotation-bound node (story #770).
        if is_rotation(channel) {
            ctx.rotation_shadow.entry(id).or_insert(rotation);
        }
        let smoothing_spec = smoothing
            .iter()
            .find(|(c, _)| *c == channel)
            .map(|(_, spring)| spring.spec());
        let key = prop_key(id, channel);
        ctx.scalar.push(ScalarBinding {
            node: id,
            parent,
            channel,
            signal,
            transform,
            class: classify(channel, contained, has_children, passthrough),
            smoothing: smoothing_spec,
            key,
            last_applied: initial_channel_value(&layout, fill, rotation, channel),
        });
    }

    if let Some(expr) = text_binding {
        let TextExpr { signal, kind } = expr;
        let transform = match kind {
            TextKind::Format(spec) => Transform::Format(spec),
            TextKind::Custom(f) => {
                let closure_id = ClosureId(ctx.text_closures.len() as u32);
                ctx.text_closures.push(f);
                Transform::Custom(closure_id)
            }
        };
        ctx.text.push(TextBinding {
            node: id,
            signal,
            transform,
        });
    }

    if let Some(signal) = visible_binding {
        ctx.visible.push(VisibleBinding { node: id, signal });
    }

    let hug = layout.sizing_h == AxisSizing::Hug || layout.sizing_v == AxisSizing::Hug;
    let child_contained = contained && layout.mode == LayoutMode::None && !hug;
    for child in children {
        stage_live(txn, Some(id), child, child_contained, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Debt #191: the no-solve path used to publish a contained write by
    /// building `CachedSolver { rects: self.cached_solve.clone() }` — an
    /// O(retained-node-count) allocation and copy every tick, regardless
    /// of how many rects the tick actually touched. `patched_rects` is
    /// the replacement: its output is sized to the tick's patch count,
    /// never to the retained cache it reads from.
    ///
    /// A retained cache of 50,000 rects stands in for a large scene; one
    /// patch, to node 0, is this tick's only write. Before the fix (a
    /// full clone of `cached_solve`), `changed.len()` would be 50,000, not
    /// 1 — this assertion is what a reverted fix fails.
    #[test]
    fn patched_rects_is_bounded_by_the_patch_count_not_the_cache_size() {
        const RETAINED_NODE_COUNT: usize = 50_000;

        let mut arena = Arena::new();
        let ids: Vec<NodeId> = {
            let mut txn = arena.open();
            let ids = (0..RETAINED_NODE_COUNT)
                .map(|_| txn.add_node(None, None))
                .collect();
            txn.commit();
            ids
        };
        let seed = SolvedRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let mut cached_solve: Vec<(NodeId, SolvedRect)> =
            ids.iter().map(|&id| (id, seed)).collect();

        let patches = vec![(0usize, Patch::W(42.0))];
        let changed = patched_rects(&mut cached_solve, &patches);

        assert_eq!(
            changed.len(),
            1,
            "one patch must report one changed rect, not the whole retained cache"
        );
        assert!(
            changed.capacity() < RETAINED_NODE_COUNT,
            "the delta must not allocate proportional to the retained node count \
             (capacity {}, retained {RETAINED_NODE_COUNT})",
            changed.capacity()
        );
        assert_eq!(cached_solve[0].1.w, 42.0, "the patch is applied in place");
        assert_eq!(changed[0].1.w, 42.0, "the reported rect reflects the patch");
    }

    /// Two different channels on the same node (e.g. `X` and `Width`)
    /// both classify as `Patch` and can both be dirty in the same tick.
    /// Both patches must land on the retained cache, and the node must be
    /// reported exactly once — `Txn::commit_with` panics on a duplicate
    /// `NodeId` (P4).
    #[test]
    fn patched_rects_dedupes_multiple_patches_to_the_same_node() {
        let mut arena = Arena::new();
        let ids: Vec<NodeId> = {
            let mut txn = arena.open();
            let ids: Vec<NodeId> = (0..3).map(|_| txn.add_node(None, None)).collect();
            txn.commit();
            ids
        };
        let seed = SolvedRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let mut cached_solve: Vec<(NodeId, SolvedRect)> =
            ids.iter().map(|&id| (id, seed)).collect();

        // Both patches target index 1 (a distinct field each).
        let patches = vec![(1usize, Patch::X(7.0)), (1usize, Patch::W(9.0))];
        let changed = patched_rects(&mut cached_solve, &patches);

        assert_eq!(changed.len(), 1, "one node reported once, not twice");
        assert_eq!(changed[0].0, ids[1]);
        assert_eq!(
            (changed[0].1.x, changed[0].1.w),
            (7.0, 9.0),
            "both patches applied"
        );
    }
}
