//! The sorting key's arithmetic, modelled in `f32`.
//!
//! **This is a model, and it does not run `BrgPainter`.** Nothing in CI
//! compiles that file — the reason `sorting_keys.rs` beside this one is a text
//! scan — so the executable half of the claim has to be reproduced rather than
//! called. What is modelled is two lines of `OnPerformCulling`: the step, and
//! `sortAt = sortBase - sortDir * ((commandCount - 1 - command) * sortStep)`
//! evaluated in `f32` the way a C# `float` evaluates it. If those lines change
//! shape, `sorting_keys.rs` fails; if their ARITHMETIC stops holding, this file
//! fails.
//!
//! **Why it exists.** Issue #1389's repair rests on two numeric properties —
//! that consecutive keys land on distinct `float` values, and that no key
//! reaches the camera — and a review found both pinned by nothing at all. The
//! step had been written as `Math.Min` of a precision floor and a fold cap,
//! which reads as "obey both bounds" and in fact obeys the smaller one: at a
//! document far from the world origin the cap wins and rounds every key onto
//! one float. Every command then carries an identical key,
//! `BatchRendererGroup` regroups by material, and the defect returns with no
//! diagnostic. A text scan cannot see that; this can.
//!
//! **The two properties are no longer in tension**, which is the point of the
//! shape the keys now take. They are laid out BEHIND the sheet and run back
//! toward it, so distance from the camera falls with the command index at any
//! span — the fold a cap existed to prevent cannot occur, and the floor is left
//! as the only bound.

/// The shipped step: a floor, relative to the larger of the viewing distance
/// and the base point's own magnitude, never below one unit's worth.
fn step(distance: f32, base_magnitude: f32) -> f32 {
    distance.max(base_magnitude).max(1.0) * 1e-5
}

/// The step as it was written before the repair — the floor and the fold cap
/// combined with `Math.Min`, which takes whichever is SMALLER.
fn step_before_the_repair(distance: f32, base_magnitude: f32, commands: i32) -> f32 {
    step(distance, base_magnitude).min(distance * 0.25 / commands.max(1) as f32)
}

/// Every command's key, on the axis the keys are laid out along, in `f32`.
///
/// The sheet sits `base` from the world origin with the camera `distance`
/// beyond it, and the keys run backwards from the sheet: command `c` sits
/// `(commands - 1 - c)` steps on the far side, so command 0 is farthest.
fn keys(base: f32, step: f32, commands: i32) -> Vec<f32> {
    (0..commands)
        .map(|c| base - (commands - 1 - c) as f32 * step)
        .collect()
}

/// The keys as an earlier version built them — walking TOWARD the camera.
fn keys_toward_the_camera(base: f32, step: f32, commands: i32) -> Vec<f32> {
    (0..commands).map(|c| base + c as f32 * step).collect()
}

fn distinct(keys: &[f32]) -> usize {
    let mut bits: Vec<u32> = keys.iter().map(|k| k.to_bits()).collect();
    bits.sort_unstable();
    bits.dedup();
    bits.len()
}

/// Distance from a camera sitting `distance` beyond `base` on the same axis.
fn from_camera(key: f32, base: f32, distance: f32) -> f32 {
    (base + distance - key).abs()
}

/// The placement the review measured: the old step tied most of the keys.
///
/// **This is the test that would have caught it**, written from the review's own
/// numbers — a document 10000 units from the world origin, a camera half a unit
/// away, 256 draw commands. One `f32` step at 10000 is about 9.8e-4; the cap
/// gave 4.9e-4, which is below it.
#[test]
fn the_pre_repair_step_collapsed_keys_at_a_far_placement() {
    let (distance, base, commands) = (0.5f32, 10_000.0f32, 256);

    let before = step_before_the_repair(distance, base, commands);
    let collapsed = distinct(&keys(base, before, commands));
    assert!(
        collapsed < commands as usize,
        "the pre-repair step of {before} kept {collapsed} of {commands} keys distinct at \
         {base} units from the origin, so this placement no longer reproduces the defect \
         and the assertion below proves nothing by contrast."
    );

    let after = step(distance, base);
    let kept = distinct(&keys(base, after, commands));
    assert_eq!(
        kept, commands as usize,
        "the shipped step of {after} keeps only {kept} of {commands} keys distinct at {base} \
         units from the origin. Keys that tie carry no order at all, and BatchRendererGroup \
         then regroups the commands by material — issue #1389, returning with no diagnostic."
    );
}

/// The floor holds across the placements a host can reach.
#[test]
fn the_step_keeps_every_key_distinct_wherever_the_document_sits() {
    for base in [0.0f32, 1.0, 100.0, 10_000.0, 1.0e6] {
        for distance in [0.001f32, 0.5, 3.0, 400.0, 10_000.0] {
            for commands in [1i32, 11, 256, 1024, 10_000] {
                let s = step(distance, base);
                let kept = distinct(&keys(base, s, commands));
                assert_eq!(
                    kept, commands as usize,
                    "at base {base}, distance {distance}, {commands} commands, the step {s} \
                     keeps only {kept} keys distinct."
                );
            }
        }
    }
}

/// No key reaches the camera, at any span — which is why there is no cap.
///
/// **Unconditional, and that is the whole change.** The earlier layout walked
/// the keys toward the camera, where distance is `|distance - c * step|` and
/// the rank folds back once the span passes the viewing distance; a cap held
/// the span short of it, and that cap was what fought the precision floor.
/// Running the keys backwards from the sheet makes distance
/// `distance + (commands - 1 - c) * step`, which falls in `c` whatever the span
/// is.
#[test]
fn distance_from_the_camera_falls_with_the_command_index_at_any_span() {
    for base in [0.0f32, 100.0, 10_000.0] {
        for distance in [0.001f32, 0.5, 400.0] {
            // Spans far past the viewing distance, which is what folded before.
            for commands in [1i32, 11, 256, 10_000, 100_000] {
                let s = step(distance, base);
                let laid = keys(base, s, commands);
                let mut previous = f32::INFINITY;
                for (c, key) in laid.iter().enumerate() {
                    let d = from_camera(*key, base, distance);
                    assert!(
                        d <= previous,
                        "at base {base}, distance {distance}, {commands} commands, command {c} \
                         sits {d} from the camera against {previous} for the one before it — \
                         the rank folded."
                    );
                    previous = d;
                }
            }
        }
    }
}

/// The new layout ranks the commands exactly as the old one did where the old
/// one was sound.
///
/// **So this is a repair, not a re-specification.** Every measurement in
/// `docs/technotes/batch-renderer-group.md` §5b was taken with the keys walking
/// toward the camera, at spans far short of the viewing distance; if the two
/// layouts disagreed about the ORDER there, those measurements would not carry
/// over.
#[test]
fn the_layout_ranks_commands_the_same_way_the_old_one_did_where_it_held() {
    for base in [0.0f32, 100.0, 10_000.0] {
        for distance in [3.0f32, 400.0] {
            for commands in [2i32, 11, 256] {
                let s = step(distance, base);
                // Only where the old layout was sound. Past the viewing
                // distance it folded, which is the whole reason it was
                // replaced, so there is no order there to agree with.
                if (commands - 1) as f32 * s >= distance {
                    continue;
                }

                let now: Vec<usize> = order_by_distance(&keys(base, s, commands), base, distance);
                let before: Vec<usize> =
                    order_by_distance(&keys_toward_the_camera(base, s, commands), base, distance);
                assert_eq!(
                    now, before,
                    "at base {base}, distance {distance}, {commands} commands the two layouts \
                     rank the commands differently, so §5b's measurements would not carry over."
                );
            }
        }
    }
}

/// Command indices ordered farthest-from-camera first.
fn order_by_distance(keys: &[f32], base: f32, distance: f32) -> Vec<usize> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|a, b| {
        from_camera(keys[*b], base, distance)
            .partial_cmp(&from_camera(keys[*a], base, distance))
            .expect("the model produces no NaN")
    });
    order
}

/// One command per instance (issue #1401) raises the command count to the
/// instance count — this is not, by itself, the harder case for the floor.
///
/// **What this test actually adds.** `step()` does not depend on the command
/// count, and distinctness at a higher count is not a harder bar to clear
/// than at a lower one: every additional key runs farther from `base`, where
/// the float ULP falls rather than grows, so a document that keeps its keys
/// distinct at 256 commands is not thereby placed in doubt at 10,000. What
/// this test contributes instead is the strict `<` on distance from the
/// camera — the sibling sweep above asserts `<=`, which a tie would also
/// satisfy — and a record that the floor is exercised at the instance-scale
/// span issue #1401 raised the command count to: 10,000 commands at the
/// review's hostile placement — a document 10,000 units from the origin, a
/// camera half a unit away — spans 1,000 units behind the sheet.
#[test]
fn the_floor_keeps_instance_scale_command_counts_distinct() {
    let (distance, base, commands) = (0.5f32, 10_000.0f32, 10_000);
    let s = step(distance, base);
    let k = keys(base, s, commands);
    assert_eq!(distinct(&k), commands as usize);
    for c in 1..commands as usize {
        assert!(
            from_camera(k[c], base, distance) < from_camera(k[c - 1], base, distance),
            "distance from the camera must fall with the command index; \
             it does not between commands {} and {}",
            c - 1,
            c
        );
    }
}
