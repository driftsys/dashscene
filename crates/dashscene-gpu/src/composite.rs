//! Render-target group opacity: the plan that turns one ordered instance
//! stream into the passes that draw it (story #583).
//!
//! # What a group is, and why it needs more than one pass
//!
//! A group whose painted rects **overlap** cannot be drawn by multiplying each
//! rect's alpha — where two members overlap, the lower one shows through the
//! upper. `docs/decisions/masks-and-group-opacity.md` resolves the
//! non-overlapping case into per-rect `opacity` at commit (the *free* path, and
//! the only path this painter needed before now) and leaves the overlapping one
//! as a `dashpaint::GroupComposite`: the subtree draws into an offscreen layer
//! at full alpha, and the layer composites at the group's alpha.
//!
//! The reference painter is the specification for what that means, and this is
//! its device-aligned transcription:
//!
//! - a layer is the **full target extent**, transparent-initialised. Not the
//!   group's bounds — `dashscene-skia`'s `offscreen_layer` states the reason,
//!   and it is one this painter shares: a group's ink reaches past its rect
//!   range through shadows and blurs, so a tight bound would have to be derived
//!   from the effects rather than from the geometry, and getting it wrong moves
//!   pixels. Story #584 added the shadows, and story 733 adds the blur.
//! - the composite is **one source-over draw of the layer at the origin,
//!   modulated by the group's alpha** — `blend_layer`'s counterpart. A 1:1
//!   pixel copy, so it samples by `textureLoad` and needs no sampler, no
//!   filtering and no antialiasing.
//! - groups **nest**, and a layer closes into whatever was open around it.
//!
//! # The plan is derived from the instance stream alone
//!
//! [`plan`] reads `Instance::layer` and `Layer::parent` and nothing else — not
//! the group ranges the packer walked to assign them. That is deliberate: the
//! ranges are boundary B's and the packer has already consumed them, so
//! re-deriving nesting here would be a second derivation of one fact, which is
//! the shape `docs/decisions/instance-buffer-contract.md` rejects. It also
//! makes this function total over any instance buffer, which is what lets layer
//! 1 pin it on a runner with no GPU.
//!
//! # Ordering
//!
//! A layer composites at the point its instances **end**, not after every
//! instance in the frame: the members that follow a group in slice order draw
//! *over* the composited group, and the reference painter closes its layer at
//! the same point (`open: Vec<OpenLayer>`, popped when the group's rect range
//! ends). A plan that deferred every composite to the end of the frame would
//! put every group on top, which is a picture no test comparing against the
//! reference would accept.

use std::ops::Range;

use crate::instance::{Instance, InstanceBuffer, Layer};

/// One step inside a pass, in the order it is encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Draw this range of the instance buffer into the pass's target.
    ///
    /// Still to be split by atlas: this partition is by *target*, and the
    /// atlas partition is independent of it. Both are ordered ranges over the
    /// same index space, so the encoder intersects them.
    Instances(Range<u32>),
    /// Blend the named layer's texture into the pass's target at that layer's
    /// alpha. The slot is an [`Instance::layer`] value — the layer index plus
    /// one — so it indexes `layers()` at `slot - 1`.
    Composite(u32),
}

/// One render pass: a target, whether it is being written for the first time,
/// and the steps drawn into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    /// The layer slot this pass draws into, or [`Instance::NONE`] for the
    /// frame's own target.
    pub target: u32,
    /// True when nothing has drawn into this target yet, so the pass loads by
    /// clearing. A later pass on the same target must **load**, or it discards
    /// what the earlier one drew.
    pub clear: bool,
    pub steps: Vec<Step>,
}

/// The passes one frame's instance stream draws as.
///
/// A frame with no layers is one pass over the whole buffer, which is what
/// every frame before story #583 was.
///
/// # Panics
///
/// Panics when an instance names a layer the buffer does not hold, or when the
/// parent chain of a layer cycles. Both are broken contracts between the packer
/// and this planner rather than frames to skip (P4), and both are silent
/// otherwise: a cycle would hang, and an out-of-range slot would draw a group
/// into a texture that does not exist.
pub fn plan(buffer: &InstanceBuffer) -> Vec<Pass> {
    let layers = buffer.layers();
    let total = buffer.instances().len() as u32;
    let mut passes: Vec<Pass> = Vec::new();
    // Indexed by slot, so slot 0 — the frame's own target — is element 0.
    let mut written = vec![false; layers.len() + 1];
    // The open layers, outermost first. The frame's target is not on it; an
    // empty stack means the frame's target is current.
    let mut stack: Vec<u32> = Vec::new();
    let mut run_start = 0u32;

    for (index, instance) in buffer.instances().iter().enumerate() {
        let index = index as u32;
        // The stack's top *is* the slot it stands for — every layer has one
        // parent, so equal slots mean equal chains. Comparing the slot rather
        // than the chain is what keeps `chain` off the per-instance path: it
        // runs once per layer change, not once per quad.
        if instance.layer == target_of(&stack) {
            continue;
        }
        let wanted = chain(instance.layer, layers);
        // Everything up to here belongs to the target that is current now.
        emit(
            &mut passes,
            &mut written,
            target_of(&stack),
            Step::Instances(run_start..index),
        );
        run_start = index;

        // Close the layers that are open but not wanted, innermost first. Each
        // composites into whatever encloses it, which is the stack's new top.
        while !starts_with(&wanted, &stack) {
            let closed = stack
                .pop()
                .expect("an empty stack is a prefix of every chain, so the loop has ended");
            emit(
                &mut passes,
                &mut written,
                target_of(&stack),
                Step::Composite(closed),
            );
        }
        // Open the rest. Nothing is emitted for them here: this instance is the
        // first of the innermost one, so the next range emitted starts at it and
        // every opened layer is written before it composites.
        stack.extend_from_slice(&wanted[stack.len()..]);
    }

    emit(
        &mut passes,
        &mut written,
        target_of(&stack),
        Step::Instances(run_start..total),
    );
    // Close whatever the last instance left open, innermost first.
    while let Some(closed) = stack.pop() {
        emit(
            &mut passes,
            &mut written,
            target_of(&stack),
            Step::Composite(closed),
        );
    }
    passes
}

/// The layer slot a stack's top names, or [`Instance::NONE`] for the frame's
/// own target.
fn target_of(stack: &[u32]) -> u32 {
    stack.last().copied().unwrap_or(Instance::NONE)
}

/// True when `stack` is a prefix of `wanted` — the condition for the open
/// layers all being ones the next instance wants.
fn starts_with(wanted: &[u32], stack: &[u32]) -> bool {
    wanted.len() >= stack.len() && wanted[..stack.len()] == *stack
}

/// The chain of layers enclosing `slot`, outermost first, empty for
/// [`Instance::NONE`].
///
/// # Panics
///
/// As [`plan`].
fn chain(slot: u32, layers: &[Layer]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut current = slot;
    while current != Instance::NONE {
        let row = layers.get(current as usize - 1).unwrap_or_else(|| {
            panic!(
                "an instance names layer {current} of a {}-layer buffer: a layer slot is valid \
                 only in the buffer that assigned it",
                layers.len(),
            )
        });
        out.push(current);
        // A chain longer than the table has revisited a layer, and walking it
        // to termination is not an option — the loop would not terminate.
        assert!(
            out.len() <= layers.len(),
            "layer {slot}'s parent chain is longer than the {}-layer table: the chain cycles",
            layers.len(),
        );
        current = row.parent;
    }
    out.reverse();
    out
}

/// Appends `step` to the pass drawing into `target`, starting a new pass when
/// the target changes.
///
/// An empty instance range is dropped rather than recorded: it encodes no work,
/// and recording it would start a pass — and therefore a clear — on a target
/// that has nothing to draw.
fn emit(passes: &mut Vec<Pass>, written: &mut [bool], target: u32, step: Step) {
    if let Step::Instances(range) = &step
        && range.is_empty()
    {
        return;
    }
    match passes.last_mut() {
        Some(pass) if pass.target == target => pass.steps.push(step),
        _ => {
            let clear = !written[target as usize];
            written[target as usize] = true;
            passes.push(Pass {
                target,
                clear,
                steps: vec![step],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer of one instance per rect, each carrying the given layer slot,
    /// over a layer table of the given parents.
    ///
    /// The instances differ in nothing but their layer, which is the only field
    /// `plan` reads — so a case that varies anything else would be varying an
    /// axis this function is blind to, and saying nothing.
    fn buffer(slots: &[u32], parents: &[u32]) -> InstanceBuffer {
        let mut out = InstanceBuffer::new();
        for (index, parent) in parents.iter().enumerate() {
            out.push_layer(
                index,
                Layer {
                    // Distinct alphas, so a plan that named the wrong slot
                    // could not be read as naming the right one by a renderer
                    // test built on this helper.
                    alpha: (index as f32 + 1.0) / 16.0,
                    parent: *parent,
                },
            );
        }
        for (index, slot) in slots.iter().enumerate() {
            out.begin_rect(index as u32);
            out.push(Instance {
                layer: *slot,
                ..Instance::default()
            });
        }
        out
    }

    fn instances(range: Range<u32>) -> Step {
        Step::Instances(range)
    }

    #[test]
    fn a_frame_with_no_layers_is_one_cleared_pass_over_the_whole_buffer() {
        let plan = plan(&buffer(&[0, 0, 0], &[]));
        assert_eq!(
            plan,
            vec![Pass {
                target: Instance::NONE,
                clear: true,
                steps: vec![instances(0..3)],
            }]
        );
    }

    /// The composite lands where the group's instances **end**, so the members
    /// after it draw over the composited group rather than under it.
    #[test]
    fn a_group_composites_before_the_instances_that_follow_it() {
        let plan = plan(&buffer(&[0, 1, 1, 0], &[Instance::NONE]));
        assert_eq!(
            plan,
            vec![
                Pass {
                    target: 0,
                    clear: true,
                    steps: vec![instances(0..1)],
                },
                Pass {
                    target: 1,
                    clear: true,
                    steps: vec![instances(1..3)],
                },
                Pass {
                    target: 0,
                    clear: false,
                    steps: vec![Step::Composite(1), instances(3..4)],
                },
            ]
        );
    }

    /// The second pass on a target must **load**. A plan that cleared it would
    /// discard everything drawn before the group — which is invisible in any
    /// frame whose group happens to start at instance 0.
    #[test]
    fn returning_to_a_target_loads_rather_than_clears() {
        let plan = plan(&buffer(&[0, 1, 0], &[Instance::NONE]));
        let base: Vec<bool> = plan
            .iter()
            .filter(|pass| pass.target == Instance::NONE)
            .map(|pass| pass.clear)
            .collect();
        assert_eq!(base, vec![true, false], "the base target clears once");
    }

    /// A group that opens at instance 0 leaves no base pass before it, and the
    /// base target's first pass is then the one holding the composite — so that
    /// pass still clears.
    #[test]
    fn a_group_at_the_start_of_the_frame_clears_the_target_it_composites_into() {
        let plan = plan(&buffer(&[1, 1, 0], &[Instance::NONE]));
        assert_eq!(
            plan,
            vec![
                Pass {
                    target: 1,
                    clear: true,
                    steps: vec![instances(0..2)],
                },
                Pass {
                    target: 0,
                    clear: true,
                    steps: vec![Step::Composite(1), instances(2..3)],
                },
            ]
        );
    }

    /// A group running to the last instance still composites — the close is
    /// driven by the end of the buffer, not only by a later instance.
    #[test]
    fn a_group_at_the_end_of_the_frame_still_composites() {
        let plan = plan(&buffer(&[0, 1, 1], &[Instance::NONE]));
        assert_eq!(
            plan.last(),
            Some(&Pass {
                target: 0,
                clear: false,
                steps: vec![Step::Composite(1)],
            })
        );
    }

    /// Nesting: the inner group composites into the **outer** one, and the
    /// outer into the frame's target. A planner that composited every layer
    /// into the frame's target would pass every single-group case above.
    #[test]
    fn a_nested_group_composites_into_the_group_enclosing_it() {
        let plan = plan(&buffer(&[1, 1, 2, 2, 1], &[Instance::NONE, 1]));
        assert_eq!(
            plan,
            vec![
                Pass {
                    target: 1,
                    clear: true,
                    steps: vec![instances(0..2)],
                },
                Pass {
                    target: 2,
                    clear: true,
                    steps: vec![instances(2..4)],
                },
                Pass {
                    target: 1,
                    clear: false,
                    steps: vec![Step::Composite(2), instances(4..5)],
                },
                Pass {
                    target: Instance::NONE,
                    clear: true,
                    steps: vec![Step::Composite(1)],
                },
            ]
        );
    }

    /// Two groups closing at once, innermost first. The order is the whole
    /// claim: compositing the outer one before the inner would blend a layer
    /// that has not received its child yet.
    #[test]
    fn two_layers_closing_together_close_innermost_first() {
        let plan = plan(&buffer(&[1, 2, 0], &[Instance::NONE, 1]));
        let composites: Vec<u32> = plan
            .iter()
            .flat_map(|pass| &pass.steps)
            .filter_map(|step| match step {
                Step::Composite(slot) => Some(*slot),
                Step::Instances(_) => None,
            })
            .collect();
        assert_eq!(composites, vec![2, 1]);
    }

    /// Siblings: each closes into the frame's target, and the second opens a
    /// second cleared layer rather than reusing the first's contents.
    #[test]
    fn sibling_groups_each_composite_into_the_target_around_them() {
        let plan = plan(&buffer(&[1, 0, 2], &[Instance::NONE, Instance::NONE]));
        assert_eq!(
            plan,
            vec![
                Pass {
                    target: 1,
                    clear: true,
                    steps: vec![instances(0..1)],
                },
                Pass {
                    target: Instance::NONE,
                    clear: true,
                    steps: vec![Step::Composite(1), instances(1..2)],
                },
                Pass {
                    target: 2,
                    clear: true,
                    steps: vec![instances(2..3)],
                },
                Pass {
                    target: Instance::NONE,
                    clear: false,
                    steps: vec![Step::Composite(2)],
                },
            ]
        );
    }

    /// Adjacent siblings with no instance between them: the first must close
    /// before the second opens, and neither may swallow the other's quads.
    #[test]
    fn adjacent_sibling_groups_do_not_merge() {
        let plan = plan(&buffer(&[1, 2], &[Instance::NONE, Instance::NONE]));
        assert_eq!(
            plan,
            vec![
                Pass {
                    target: 1,
                    clear: true,
                    steps: vec![instances(0..1)],
                },
                Pass {
                    target: Instance::NONE,
                    clear: true,
                    steps: vec![Step::Composite(1)],
                },
                Pass {
                    target: 2,
                    clear: true,
                    steps: vec![instances(1..2)],
                },
                Pass {
                    target: Instance::NONE,
                    clear: false,
                    steps: vec![Step::Composite(2)],
                },
            ]
        );
    }

    /// Every instance is drawn exactly once, whatever the nesting — the
    /// property no individual case above states, and the one a dropped or
    /// duplicated range breaks.
    #[test]
    fn the_ranges_partition_the_buffer_in_order() {
        for (slots, parents) in [
            (
                vec![0u32, 1, 1, 0, 2, 0],
                vec![Instance::NONE, Instance::NONE],
            ),
            (vec![1u32, 2, 2, 1, 0], vec![Instance::NONE, 1]),
            (
                vec![2u32, 1, 3, 1, 2],
                vec![Instance::NONE, Instance::NONE, 2],
            ),
        ] {
            let buffer = buffer(&slots, &parents);
            let mut next = 0u32;
            for step in plan(&buffer).iter().flat_map(|pass| &pass.steps) {
                if let Step::Instances(range) = step {
                    assert_eq!(range.start, next, "ranges are contiguous and in order");
                    assert!(!range.is_empty(), "an empty range encodes no work");
                    next = range.end;
                }
            }
            assert_eq!(next, slots.len() as u32, "every instance is drawn");
        }
    }

    /// Each instance's quads reach the layer its own `layer` field names, which
    /// is what "the group is applied to the right set" means.
    #[test]
    fn every_instance_is_drawn_into_the_layer_it_names() {
        let slots = [0u32, 1, 2, 2, 1, 0, 3];
        let buffer = buffer(&slots, &[Instance::NONE, 1, Instance::NONE]);
        for pass in plan(&buffer) {
            for step in &pass.steps {
                if let Step::Instances(range) = step {
                    for index in range.clone() {
                        assert_eq!(
                            slots[index as usize], pass.target,
                            "instance {index} drew into layer {} and names {}",
                            pass.target, slots[index as usize],
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "a layer slot is valid only in the buffer that assigned it")]
    fn an_instance_naming_a_layer_the_buffer_does_not_hold_is_a_named_failure() {
        plan(&buffer(&[1], &[]));
    }

    #[test]
    #[should_panic(expected = "the chain cycles")]
    fn a_parent_chain_that_cycles_is_a_named_failure() {
        plan(&buffer(&[1], &[2, 1]));
    }
}
