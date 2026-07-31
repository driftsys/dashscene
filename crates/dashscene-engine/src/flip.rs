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
//!
//! Visibility across a variant switch (story #283, a named limit): a
//! `set_variant` that toggles a child's `Visible` makes it appear or
//! disappear. This FLIP path animates rect channels only (X/Y/Width/Height)
//! and binds each track from the node's before and after rects — a hidden
//! node's rect is degenerate, and there is no visibility or opacity channel,
//! so the appearing/disappearing node itself is not tweened; it pops. Its
//! reflowing siblings, present in both rect slices, animate normally. Fading
//! an appearing node is descriptive-animation work for a later slice (an
//! opacity channel), not this FLIP path — no new FLIP machinery is added here.

use dashcue::{PropKey, Scheduler, VariantTransition};
use dashscene_core::{Channel, NodeId, SolvedRect};
use rustc_hash::{FxHashMap, FxHashSet};

/// The four rect channels, in packing order — the fixed set a rect
/// decomposes into. A rect animates as one `dashcue` track per channel
/// (`docs/design/dashcue.md`: "a multi-channel prop ... animates as one
/// track per channel"). The channel vocabulary itself is
/// `dashscene_core::Channel` — the document binding vocabulary — so a
/// serialized binding row, a reactive binding, and a FLIP track all
/// address a prop the same way (debt #208).
const RECT_CHANNELS: [Channel; 4] = [Channel::X, Channel::Y, Channel::Width, Channel::Height];

/// The `dashcue` prop key for one channel of one node — core's one
/// packing (`dashscene_core::prop_key`: node slot in the high bits, the
/// [`Channel`] wire code in the low eight; debt #208), exposed as the
/// typed `dashcue::PropKey` FLIP tracks carry. The math lives in core,
/// beside `Channel`, because core cannot depend on `dashcue` and every
/// consumer (this engine, `dashlang`'s reactive layer) already depends
/// on core — so one `(node, channel)` yields one `u64` everywhere.
pub fn prop_key(node: NodeId, channel: Channel) -> PropKey {
    PropKey(dashscene_core::prop_key(node, channel))
}

/// Decodes a packed [`PropKey`] back to its node slot and channel —
/// core's canonical decoder (`dashscene_core::decode_prop_key`), over the
/// typed key (debts #207/#208). Returns `None` when the key was not built
/// by [`prop_key`]; the caller names the failure (P4) rather than
/// mis-binding it.
pub fn decode_prop_key(key: PropKey) -> Option<(u32, Channel)> {
    dashscene_core::decode_prop_key(key.0)
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
    /// Scratch: this frame's live channel keys, rebuilt by
    /// [`advance`](VariantFlip::advance) from [`Scheduler::samples`] — one
    /// O(live tracks) pass — so pruning `targets` is an O(1) set lookup per
    /// rect channel per node instead of a linear scan of the scheduler's
    /// tracks repeated per node per channel (issue #205). Cleared and
    /// reused rather than replaced, so a steady-state animated set costs no
    /// allocation once its capacity settles (R4).
    live_scratch: FxHashSet<PropKey>,
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
    /// A declared channel whose before and after values are equal starts no
    /// track (debt #487): it has nothing to animate, and a declined track
    /// leaves every other track's stagger delay where it was. A node whose
    /// every declared channel sits still therefore does not enter the
    /// animated set at all — see [`sampled_rects`](VariantFlip::sampled_rects)
    /// for what that set is.
    ///
    /// # Panics
    ///
    /// Panics if a transition track's prop key is not engine-packed — it
    /// does not decode through [`decode_prop_key`], or it names a channel
    /// that is not a rect channel (FLIP animates rects only) — or if it
    /// names a `(node, channel)` whose node is absent from `before` or
    /// `after`. Each is a broken contract between the producer's declared
    /// tracks and this engine (house rule: cross-crate contract
    /// violations panic; debt #207). A raw key that happens to decode to
    /// a rect channel of a node present in both slices is
    /// indistinguishable from a legitimate one and is not detected.
    pub fn start(
        &mut self,
        before: &[(NodeId, SolvedRect)],
        after: &[(NodeId, SolvedRect)],
        transition: &VariantTransition,
    ) {
        for track in &transition.tracks {
            match decode_prop_key(track.prop) {
                None => panic!(
                    "FLIP track {:?} is not an engine-packed prop key; build track keys with \
                     dashscene_engine::prop_key",
                    track.prop
                ),
                Some((_, channel)) if !RECT_CHANNELS.contains(&channel) => panic!(
                    "FLIP track {:?} names channel {channel:?}, which is not a rect channel; \
                     FLIP animates rects only (X, Y, Width, Height)",
                    track.prop
                ),
                Some(_) => {}
            }
        }
        let before_val = channel_values(before);
        let after_val = channel_values(after);
        self.scheduler.start_transition(transition, |key| {
            let from = before_val.get(&key).copied().unwrap_or_else(|| {
                panic!("FLIP track {key:?} names a node absent from the before rects")
            });
            let to = after_val.get(&key).copied().unwrap_or_else(|| {
                panic!("FLIP track {key:?} names a node absent from the after rects")
            });
            // A channel whose resolved value did not change has nothing to
            // animate, so it starts no track (debt #487). `dashcue` lets a
            // binding decline a track (#74) and computes a track's stagger
            // delay from its declared index, so declining one does not
            // move any other track's delay.
            //
            // The contract this rests on: `targets` — and with it
            // `sample` and `sampled_rects` — is the set of nodes that are
            // *animating*, not the set the transition *declared*. That is
            // already what `advance` enforces (it drops a node the moment
            // its last channel finishes) and what a node with no declared
            // track already gets (it is absent, and pops). A node whose
            // every declared channel sits still is animating nothing and
            // is absent by the same rule. Its consumers are unaffected:
            // `compose` fills a channel with no live track from the after
            // rect, so an absent node and a node sampled on every unmoved
            // channel yield the same rect.
            (from != to).then_some((from, to))
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
    ///
    /// Builds `live_scratch` once per call from [`Scheduler::samples`]
    /// (issue #205): the naive approach — probe [`Scheduler::sample`] once
    /// per rect channel per target — costs a linear scan of the scheduler's
    /// tracks per probe, so pruning `targets` that way is
    /// O(targets · channels · live tracks). Indexing the live keys once
    /// makes each node's liveness check an O(1) set lookup instead, so the
    /// whole prune is O(targets + live tracks).
    pub fn advance(&mut self, dt: f32) {
        self.scheduler.advance(dt);
        self.live_scratch.clear();
        for (key, _value) in self.scheduler.samples() {
            #[cfg(test)]
            probe_cost::record(1);
            self.live_scratch.insert(key);
        }
        self.targets
            .retain(|(node, _)| node_is_live_in(&self.live_scratch, *node));
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
    ///
    /// The set is the nodes that are **animating**, never the nodes a
    /// transition **declared** (debt #487). A node with no declared track is
    /// absent and pops; a node whose declared channels have all finished is
    /// dropped by [`advance`](VariantFlip::advance); a node whose every
    /// declared channel starts and ends at the same value never enters. In
    /// every one of those cases the node's rect is its after rect, which is
    /// what a consumer that composes this over the after layout already has.
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
        for channel in RECT_CHANNELS {
            if let Some(value) = self.scheduler.sample(prop_key(node, channel)) {
                *channel_mut(&mut rect, channel) = value;
            }
        }
        rect
    }
}

/// Whether `node` has any live channel track in `scheduler` — probed
/// directly against the scheduler, one [`Scheduler::sample`] call per rect
/// channel. Still used by [`VariantFlip::is_live`] (`start`'s per-switch
/// bookkeeping, not `advance`'s per-frame prune — issue #205 is scoped to
/// `advance`). Each `Scheduler::sample` call is a linear scan of the
/// scheduler's tracks (`dashcue::Scheduler`, "storage is a linearly scanned
/// Vec"), so this is O(live tracks) per call — fine at `start`'s call
/// frequency (once per switch, not once per frame).
fn node_is_live(scheduler: &Scheduler, node: NodeId) -> bool {
    RECT_CHANNELS.iter().any(|&channel| {
        #[cfg(test)]
        probe_cost::record(scheduler.len());
        scheduler.sample(prop_key(node, channel)).is_some()
    })
}

/// Whether `node` has any rect channel key present in `live` — the same
/// "is any rect channel live" test [`node_is_live`] performs, against a
/// prebuilt index (issue #205's fix) instead of probing the scheduler
/// directly: each check here is O(1), where `node_is_live`'s is O(live
/// tracks).
fn node_is_live_in(live: &FxHashSet<PropKey>, node: NodeId) -> bool {
    RECT_CHANNELS.iter().any(|&channel| {
        #[cfg(test)]
        probe_cost::record(1);
        live.contains(&prop_key(node, channel))
    })
}

/// Test-only cost accounting for issue #205: a deterministic stand-in for
/// wall-clock cost, since call *count* alone doesn't distinguish the fix —
/// both the old and new code make O(targets · channels) calls per
/// `advance`. What differs is what each call costs, so each primitive is
/// weighted by its own documented cost: a [`Scheduler::sample`] probe by
/// `scheduler.len()` (the scan it performs), a [`Scheduler::samples`] item
/// or an `FxHashSet::contains` probe by 1 (O(1) work each). Summed across
/// one `advance` call, this total is the quantity issue #205 names:
/// O(targets · channels · live tracks) through `node_is_live`, O(targets +
/// live tracks) through `node_is_live_in`.
#[cfg(test)]
mod probe_cost {
    use std::cell::Cell;

    thread_local! {
        static COST: Cell<usize> = const { Cell::new(0) };
    }

    pub fn reset() {
        COST.with(|c| c.set(0));
    }

    pub fn record(units: usize) {
        COST.with(|c| c.set(c.get() + units));
    }

    pub fn get() -> usize {
        COST.with(|c| c.get())
    }
}

/// Index each rect's four channels by their packed [`PropKey`], the lookup
/// the binding closure resolves `from`/`to` through.
fn channel_values(rects: &[(NodeId, SolvedRect)]) -> FxHashMap<PropKey, f32> {
    let mut map = FxHashMap::default();
    for &(node, rect) in rects {
        for channel in RECT_CHANNELS {
            map.insert(prop_key(node, channel), channel_of(&rect, channel));
        }
    }
    map
}

fn channel_of(rect: &SolvedRect, channel: Channel) -> f32 {
    match channel {
        Channel::X => rect.x,
        Channel::Y => rect.y,
        Channel::Width => rect.w,
        Channel::Height => rect.h,
        // Only RECT_CHANNELS reach here.
        other => unreachable!("{other:?} is not a rect channel"),
    }
}

fn channel_mut(rect: &mut SolvedRect, channel: Channel) -> &mut f32 {
    match channel {
        Channel::X => &mut rect.x,
        Channel::Y => &mut rect.y,
        Channel::Width => &mut rect.w,
        Channel::Height => &mut rect.h,
        // Only RECT_CHANNELS reach here.
        other => unreachable!("{other:?} is not a rect channel"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashcue::{Easing, PropTransition, TransitionSpec};
    use dashscene_core::Arena;

    fn rect(x: f32) -> SolvedRect {
        SolvedRect {
            x,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        }
    }

    /// `n` nodes, each with all four rect channels animating on a live
    /// one-second linear tween — `advance`'s worst case, the shape issue
    /// #205 is about. Every `dt` used against the returned flip stays far
    /// short of the tween's duration, so nothing finishes and every node
    /// survives one `advance` call — the two sizes below are therefore an
    /// apples-to-apples comparison of the same shape of surviving work,
    /// scaled by node count alone.
    fn animating_flip(n: usize) -> (Arena, VariantFlip) {
        let mut arena = Arena::new();
        let nodes: Vec<NodeId> = {
            let mut txn = arena.open();
            let nodes = (0..n).map(|_| txn.add_node(None, None)).collect();
            txn.commit();
            nodes
        };

        let before: Vec<_> = nodes.iter().map(|&node| (node, rect(0.0))).collect();
        let after: Vec<_> = nodes.iter().map(|&node| (node, rect(100.0))).collect();
        let tracks = nodes
            .iter()
            .flat_map(|&node| {
                RECT_CHANNELS.iter().map(move |&channel| PropTransition {
                    prop: prop_key(node, channel),
                    spec: TransitionSpec::Tween {
                        duration: 1.0,
                        easing: Easing::Linear,
                    },
                })
            })
            .collect();
        let transition = VariantTransition {
            tracks,
            stagger: 0.0,
        };

        let mut flip = VariantFlip::new();
        flip.start(&before, &after, &transition);
        (arena, flip)
    }

    /// Issue #205: `advance`'s prune must cost O(animating nodes), not
    /// O(animating nodes² · channels). `probe_cost` weighs each primitive
    /// by its own documented cost, not by call count — call count alone is
    /// O(N) either way; the quadratic blowup is in what each call costs.
    /// Growing N by 4x should grow a linear cost ~4x and a quadratic cost
    /// ~16x — comfortably far apart to tell apart with one fixed
    /// threshold, without timing anything.
    ///
    /// Falsifiable: reverting `advance` to probe `node_is_live` per target
    /// (the pre-fix shape) makes this fail, because `probe_cost` then
    /// records `scheduler.len()` per node instead of O(1) per node — see
    /// the PR description for the recorded failing run.
    #[test]
    fn advance_prunes_at_cost_linear_in_animating_nodes() {
        const N: usize = 200;
        const SCALE: usize = 4;

        let (_arena_small, mut small) = animating_flip(N);
        probe_cost::reset();
        small.advance(0.01);
        let small_cost = probe_cost::get();

        let (_arena_big, mut big) = animating_flip(N * SCALE);
        probe_cost::reset();
        big.advance(0.01);
        let big_cost = probe_cost::get();

        // Confirms the two runs compare the same shape of work: dt is tiny
        // against the 1-second tween, so nothing finished and every node
        // is still live (and counted) in both.
        assert_eq!(small.targets.len(), N, "no node finished in the small run");
        assert_eq!(
            big.targets.len(),
            N * SCALE,
            "no node finished in the big run"
        );

        let ratio = big_cost as f64 / small_cost as f64;
        let quadratic_ratio = (SCALE * SCALE) as f64;
        assert!(
            ratio < quadratic_ratio / 2.0,
            "cost ratio {ratio:.1} for a {SCALE}x node increase reads as O(N^2) \
             (expected close to {SCALE}x for O(N), not close to {quadratic_ratio}x); \
             small_cost={small_cost} (N={N}), big_cost={big_cost} (N={})",
            N * SCALE,
        );
    }

    /// The other half of the fix's contract: `advance` must prune exactly
    /// the nodes whose every channel finished, not merely prune cheaply. A
    /// mixed scene — one node finishes within `dt`, one does not — is the
    /// case a single-node test can't tell apart from "dropped everything"
    /// or "dropped nothing".
    #[test]
    fn advance_prunes_exactly_the_nodes_whose_every_channel_finished() {
        let mut arena = Arena::new();
        let (short, long) = {
            let mut txn = arena.open();
            let short = txn.add_node(None, None);
            let long = txn.add_node(None, None);
            txn.commit();
            (short, long)
        };

        let transition = VariantTransition {
            tracks: vec![
                PropTransition {
                    prop: prop_key(short, Channel::X),
                    spec: TransitionSpec::Tween {
                        duration: 0.5,
                        easing: Easing::Linear,
                    },
                },
                PropTransition {
                    prop: prop_key(long, Channel::X),
                    spec: TransitionSpec::Tween {
                        duration: 2.0,
                        easing: Easing::Linear,
                    },
                },
            ],
            stagger: 0.0,
        };

        let mut flip = VariantFlip::new();
        flip.start(
            &[(short, rect(0.0)), (long, rect(0.0))],
            &[(short, rect(10.0)), (long, rect(10.0))],
            &transition,
        );

        // The frame `short` finishes on: `Scheduler::advance`'s doc
        // guarantees a track that finishes this step still samples until
        // the *next* advance, so both nodes are still in the animated set.
        flip.advance(0.5);
        let live: Vec<NodeId> = flip.sampled_rects().map(|(n, _)| n).collect();
        assert_eq!(
            live,
            vec![short, long],
            "both present the frame short finishes on"
        );

        // The following advance drops exactly `short` — `long` still has a
        // live track and survives.
        flip.advance(0.1);
        let live: Vec<NodeId> = flip.sampled_rects().map(|(n, _)| n).collect();
        assert_eq!(
            live,
            vec![long],
            "short is pruned once its track has actually finished; long, still \
             running, is not"
        );
    }
}
