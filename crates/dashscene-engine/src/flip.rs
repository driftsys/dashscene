//! Minimal FLIP for variant-switch layout deltas
//! (`docs/archive/2026-07-14-design-1-seed.md` §6.3, "variant transition
//! ... FLIP for layout deltas").
//!
//! FLIP — First, Last, Invert, Play — animates a layout change by measuring
//! the geometry before it (First) and after it (Last), then playing each
//! moved node from its old rect to its new one. The layout change here is a
//! `set_variant` switch: the retained [`TaffySolver`](crate::TaffySolver)
//! gives the before and after rects cheaply (issue #164), and the declared
//! [`VariantTransition`] says how each moved node's rect travels.
//!
//! The seam (`docs/design/dashcue.md`, "The seam"): `dashcue` carries no
//! resolved values (P1), so a transition spec cannot name absolute targets.
//! At commit time this engine knows each animated prop's old and new
//! resolved values and binds them onto the declared transition, handing the
//! scheduler concrete `(key, from, to, spec, delay)` tracks. A rect is a
//! multi-channel prop, so it animates as one track per channel
//! ([`Channel`]).
//!
//! Interruptibility and bounded cost (R4): a second [`VariantFlip::start`]
//! mid-flight retargets the live tracks through the scheduler's existing
//! retarget rule — the new track resumes from the current sample, a
//! spring-to-spring retarget keeps its velocity, and nothing snaps. Each
//! frame's cost is `O(animated nodes)` with no per-frame allocation and no
//! state that grows with animation history.

use dashcue::{PropKey, Scheduler, VariantTransition};
use dashscene_core::{NodeId, SolvedRect};
use rustc_hash::FxHashMap;

/// One channel of a node's rect. A rect animates as one `dashcue` track per
/// channel (`docs/design/dashcue.md`: "a multi-channel prop ... animates as
/// one track per channel"). The discriminants are the low bits of the
/// packed [`PropKey`]; keep them stable so a producer and this engine agree
/// on the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    X = 0,
    Y = 1,
    W = 2,
    H = 3,
}

/// The four channels, in packing order — the fixed set a rect decomposes
/// into.
const CHANNELS: [Channel; 4] = [Channel::X, Channel::Y, Channel::W, Channel::H];

/// The `dashcue` prop key for one node's one rect channel. `dashscene-engine`
/// owns this packing (`docs/design/dashcue.md`: "the engine packs node index
/// and channel into [the `PropKey`]"): the node's arena slot in the high
/// bits, the [`Channel`] discriminant in the low two. A producer declares a
/// variant transition's tracks with keys built here, so [`VariantFlip::start`]
/// binds each track's `from`/`to` from the before/after rects.
pub fn prop_key(node: NodeId, channel: Channel) -> PropKey {
    PropKey(((node.index() as u64) << 2) | channel as u64)
}

/// Drives the minimal FLIP for a variant switch: bind the declared
/// [`VariantTransition`] onto `dashcue`'s scheduler from the before/after
/// rects, advance it once per frame, and sample each moved node's current
/// rect.
///
/// The runtime owns one `VariantFlip` alongside its solver, the same way it
/// owns one [`Scheduler`]. See the module docs for the FLIP and retarget
/// contracts.
///
/// Not `Debug`: `dashcue`'s [`Scheduler`] is not `Debug`, and this holds one.
#[derive(Default)]
pub struct VariantFlip {
    scheduler: Scheduler,
    /// The after (target) rect of every node with a live track, in the order
    /// the animation started them. [`VariantFlip::sample`] overlays each
    /// channel's live sample on this target; a channel with no live track
    /// holds its after value. A `Vec` (not a hash map) keeps iteration order
    /// deterministic so #23 samples reproducibly, and a retarget updates a
    /// node's target in place rather than reordering it.
    targets: Vec<(NodeId, SolvedRect)>,
}

impl VariantFlip {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start — or retarget — the FLIP for a variant switch.
    ///
    /// `before` and `after` are the resolved rects of the animated nodes on
    /// each side of the switch (the retained solver's readbacks, issue #164);
    /// `after` must contain every node the transition animates. `transition`'s
    /// tracks name the `(node, channel)` props to animate, each track's prop
    /// key built by [`prop_key`]; the track's spec and the transition's
    /// stagger drive it. Binding resolves each track's `(from, to)` from
    /// `before`/`after` — the resolved values that never live in the
    /// vocabulary (P1).
    ///
    /// A track already live (an interrupting switch) retargets through the
    /// scheduler: the caller-supplied `from` is ignored and the track resumes
    /// from its current sample, so nothing snaps.
    ///
    /// # Panics
    ///
    /// Panics if a transition track names a `(node, channel)` whose node is
    /// absent from `before` or `after` — a broken contract between the
    /// producer's declared tracks and the captured rects (house rule:
    /// cross-crate contract violations panic).
    pub fn start(
        &mut self,
        before: &[(NodeId, SolvedRect)],
        after: &[(NodeId, SolvedRect)],
        transition: &VariantTransition,
    ) {
        let before_val = channel_values(before);
        let after_val = channel_values(after);
        self.scheduler.start_transition(transition, |key| {
            let from = before_val.get(&key).copied().unwrap_or_else(|| {
                panic!("FLIP track {key:?} names a node absent from the before rects")
            });
            let to = after_val.get(&key).copied().unwrap_or_else(|| {
                panic!("FLIP track {key:?} names a node absent from the after rects")
            });
            (from, to)
        });
        // Record each animated node's target rect so `sample` can rebuild a
        // full rect from the per-channel samples. `start_transition` has just
        // made every declared track live, so a node with any live channel is
        // one this switch animates; a retarget updates its target in place.
        for &(node, rect) in after {
            if !self.is_live(node) {
                continue;
            }
            match self.targets.iter_mut().find(|(n, _)| *n == node) {
                Some(slot) => slot.1 = rect,
                None => self.targets.push((node, rect)),
            }
        }
    }

    /// Advance every live track by `dt` seconds — the runtime clock's step,
    /// never a clock this reads (P3) — then drop any node whose channels have
    /// all finished. Deterministic under a fixed step (`dashcue`'s IEEE-754
    /// determinism), which is what the E5 goldens sample.
    pub fn advance(&mut self, dt: f32) {
        self.scheduler.advance(dt);
        let scheduler = &self.scheduler;
        self.targets
            .retain(|(node, _)| node_is_live(scheduler, *node));
    }

    /// The current animated rect of `node`, or `None` when it is not
    /// animating. Each channel takes its live track's sample; a channel whose
    /// track has finished — or was never animated — holds the after value.
    pub fn sample(&self, node: NodeId) -> Option<SolvedRect> {
        let &(_, target) = self.targets.iter().find(|(n, _)| *n == node)?;
        Some(self.compose(node, target))
    }

    /// Every animating node's current rect, in start order — the frame output
    /// #23 samples at t = 0 / 0.5 / 1.
    pub fn sampled_rects(&self) -> impl Iterator<Item = (NodeId, SolvedRect)> + '_ {
        self.targets
            .iter()
            .map(|&(node, target)| (node, self.compose(node, target)))
    }

    /// Whether any node is still animating.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    fn is_live(&self, node: NodeId) -> bool {
        node_is_live(&self.scheduler, node)
    }

    /// Overlay `node`'s live channel samples on its `target` rect.
    fn compose(&self, node: NodeId, target: SolvedRect) -> SolvedRect {
        let mut rect = target;
        for channel in CHANNELS {
            if let Some(value) = self.scheduler.sample(prop_key(node, channel)) {
                *channel_mut(&mut rect, channel) = value;
            }
        }
        rect
    }
}

/// Whether `node` has any live channel track in `scheduler`.
fn node_is_live(scheduler: &Scheduler, node: NodeId) -> bool {
    CHANNELS
        .iter()
        .any(|&channel| scheduler.sample(prop_key(node, channel)).is_some())
}

/// Index each rect's four channels by their packed [`PropKey`], the lookup
/// the binding closure resolves `from`/`to` through.
fn channel_values(rects: &[(NodeId, SolvedRect)]) -> FxHashMap<PropKey, f32> {
    let mut map = FxHashMap::default();
    for &(node, rect) in rects {
        for channel in CHANNELS {
            map.insert(prop_key(node, channel), channel_of(&rect, channel));
        }
    }
    map
}

fn channel_of(rect: &SolvedRect, channel: Channel) -> f32 {
    match channel {
        Channel::X => rect.x,
        Channel::Y => rect.y,
        Channel::W => rect.w,
        Channel::H => rect.h,
    }
}

fn channel_mut(rect: &mut SolvedRect, channel: Channel) -> &mut f32 {
    match channel {
        Channel::X => &mut rect.x,
        Channel::Y => &mut rect.y,
        Channel::W => &mut rect.w,
        Channel::H => &mut rect.h,
    }
}
