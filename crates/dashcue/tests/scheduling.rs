//! Scheduler tests (issue #21): fixed time steps, hand-computed
//! expectations — dashcue's public API only.

use dashcue::{Easing, Keyframe, PropKey, Scheduler, TransitionSpec};

const K: PropKey = PropKey(1);

fn linear_tween(duration: f32) -> TransitionSpec {
    TransitionSpec::Tween {
        duration,
        easing: Easing::Linear,
    }
}

#[test]
fn tween_advances_deterministically_with_a_fixed_step() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);

    assert_eq!(s.sample(K), Some(0.0)); // live before any advance
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(25.0));
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(50.0));
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(75.0));
}

// ---------------------------------------------------------------------
// A track that finished this frame lingers until the next `advance`
// sweeps it: `is_settled` reports idle (no live track) even while
// `is_empty` does not.
// ---------------------------------------------------------------------
#[test]
fn a_finished_track_is_settled_before_the_next_advance_sweeps_it() {
    let mut s = Scheduler::new();
    assert!(s.is_settled(), "an empty scheduler is settled");

    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    assert!(!s.is_settled(), "a live track is not settled");
    assert!(!s.is_empty());

    // Advance past the tween's duration: the track finishes on this call but
    // is not swept until the next advance.
    s.advance(1.5);
    assert_eq!(s.sample(K), Some(100.0), "the tween snapped to its target");
    assert!(
        !s.is_empty(),
        "the finished track lingers until the next advance"
    );
    assert!(
        s.is_settled(),
        "but the scheduler is settled — no live track"
    );

    // The next advance sweeps it.
    s.advance(0.0);
    assert!(s.is_empty(), "swept");
    assert!(s.is_settled());
}

#[test]
fn eased_tween_samples_the_easing_polynomial() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Tween {
            duration: 1.0,
            easing: Easing::EaseInOut,
        },
        0.0,
    );

    s.advance(0.25);
    assert_eq!(s.sample(K), Some(6.25)); // 4 * 0.25^3 * 100
}

#[test]
fn finished_track_samples_exactly_to_then_the_next_advance_drops_it() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);

    s.advance(1.5); // overshoots the duration
    assert_eq!(s.sample(K), Some(100.0)); // exact `to`, still sampleable
    assert_eq!(s.len(), 1);

    s.advance(0.0); // next frame: finished track is dropped first
    assert_eq!(s.sample(K), None);
    assert!(s.is_empty());
}

#[test]
fn delayed_track_holds_at_from_until_the_delay_elapses() {
    let mut s = Scheduler::new();
    s.start(K, 10.0, 20.0, linear_tween(1.0), 0.5);

    s.advance(0.25);
    assert_eq!(s.sample(K), Some(10.0)); // still inside the delay
    s.advance(0.5); // 0.25 left of delay, then 0.25 of track time
    assert_eq!(s.sample(K), Some(12.5));
}

#[test]
#[should_panic(expected = "span must be finite")]
fn start_panics_on_a_from_to_span_wider_than_f32_holds() {
    // Both endpoints are finite and pass their own checks, but `to - from`
    // overflows to infinity, so every mid-flight sample would be infinite
    // or NaN until the finish frame snapped to `to` (issue #70).
    let mut s = Scheduler::new();
    s.start(K, -3e38, 3e38, linear_tween(1.0), 0.0);
}

#[test]
fn samples_iterates_live_tracks_in_start_order() {
    let mut s = Scheduler::new();
    s.start(PropKey(2), 0.0, 1.0, linear_tween(1.0), 0.0);
    s.start(PropKey(1), 5.0, 6.0, linear_tween(1.0), 0.0);

    s.advance(0.5);
    let got: Vec<(PropKey, f32)> = s.samples().collect();
    assert_eq!(got, vec![(PropKey(2), 0.5), (PropKey(1), 5.5)]);
}

// ---------------------------------------------------------------------
// Start order is a guarantee, not an artifact of the current storage
// (issue #77): a retarget re-enters at the back, and consumers
// (`dashlang`'s reactive drive, the engine's FLIP frame output) read
// `samples()` in that order. Any future storage change must preserve it.
// ---------------------------------------------------------------------
#[test]
fn a_retargeted_track_re_enters_samples_at_the_back() {
    let mut s = Scheduler::new();
    s.start(PropKey(1), 0.0, 100.0, linear_tween(1.0), 0.0);
    s.start(PropKey(2), 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);
    let keys: Vec<PropKey> = s.samples().map(|(key, _)| key).collect();
    assert_eq!(keys, vec![PropKey(1), PropKey(2)], "start order");

    s.start(PropKey(1), 0.0, 0.0, linear_tween(1.0), 0.0);
    let keys: Vec<PropKey> = s.samples().map(|(key, _)| key).collect();
    assert_eq!(
        keys,
        vec![PropKey(2), PropKey(1)],
        "the retargeted track re-enters at the back"
    );
}

const STEP: f32 = 1.0 / 120.0;

fn critical_spring() -> TransitionSpec {
    TransitionSpec::Spring {
        stiffness: 100.0,
        damping_ratio: 1.0,
    }
}

#[test]
fn spring_converges_to_the_target_and_finishes() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);

    let mut steps = 0;
    while !s.is_empty() {
        s.advance(STEP);
        steps += 1;
        assert!(steps < 10_000, "spring never reached rest");
    }
    // The finishing frame sampled exactly `to` before the drop:
    // rerun and stop on the finishing frame.
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);
    for _ in 0..steps - 1 {
        s.advance(STEP);
    }
    assert_eq!(s.sample(K), Some(100.0));
}

#[test]
fn spring_advance_is_bit_deterministic_across_runs() {
    let run = || {
        let mut s = Scheduler::new();
        s.start(K, 0.0, 100.0, critical_spring(), 0.0);
        let mut samples = Vec::new();
        for _ in 0..240 {
            s.advance(STEP);
            samples.extend(s.sample(K).map(f32::to_bits));
        }
        samples
    };
    assert_eq!(run(), run());
}

#[test]
fn spring_moves_monotonically_toward_the_target_when_critically_damped() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, critical_spring(), 0.0);

    let mut previous = 0.0;
    for _ in 0..240 {
        s.advance(STEP);
        let Some(now) = s.sample(K) else { break };
        assert!(
            now >= previous,
            "critically damped spring moved away from the target"
        );
        assert!(
            now <= 100.0 + 1.0,
            "critically damped spring exceeded the target"
        );
        previous = now;
    }
}

#[test]
fn spring_survives_a_frame_hitch_without_diverging() {
    // Stiffness 1500 is Compose's default spring stiffness — the
    // vocabulary is calibrated against it — and dt = 0.1 is one
    // dropped-frame hitch at 10 fps. Without substepping, one Euler
    // step of that size diverges (velocity += 10^5 * 0.1).
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Spring {
            stiffness: 1500.0,
            damping_ratio: 1.0,
        },
        0.0,
    );

    s.advance(0.1);
    let after = s.sample(K).unwrap();
    assert!(
        after.is_finite() && (0.0..=200.0).contains(&after),
        "spring diverged across a frame hitch: {after}"
    );

    let mut steps = 0;
    while !s.is_empty() {
        s.advance(0.1);
        steps += 1;
        assert!(steps < 10_000, "spring never reached rest after a hitch");
    }
}

#[test]
fn large_magnitude_spring_settles_and_finishes() {
    // A FLIP-scale layout delta (0 -> 1e5) must settle in bounded time.
    // Absolute rest thresholds never trip at this magnitude — the f32
    // ulp of `to` exceeds REST_DELTA — so the pre-#68 spring froze an
    // ulp short of `to` and advanced forever. The magnitude-scaled
    // thresholds settle it and snap it to exactly `to`.
    let to = 1e5;
    let mut s = Scheduler::new();
    s.start(K, 0.0, to, critical_spring(), 0.0);

    let mut steps = 0;
    while !s.is_empty() {
        s.advance(STEP);
        steps += 1;
        assert!(steps < 10_000, "large-magnitude spring never reached rest");
    }
    // Rerun and stop on the finishing frame: it sampled exactly `to`.
    let mut s = Scheduler::new();
    s.start(K, 0.0, to, critical_spring(), 0.0);
    for _ in 0..steps - 1 {
        s.advance(STEP);
    }
    assert_eq!(s.sample(K), Some(to));
}

// ---------------------------------------------------------------------
// The spring's two rest gates are dimensionally consistent (issue #214):
// the velocity gate is sized against the spring's characteristic velocity
// (omega * magnitude), not against the position magnitude. Sized against
// the position magnitude it becomes the binding condition and holds a
// large-magnitude track open past the point where the position gate is
// already satisfied — the stiffer the spring, the longer the overhang.
// ---------------------------------------------------------------------
#[test]
fn the_position_gate_binds_a_large_magnitude_spring() {
    // The documented position gate (docs/design/dashcue.md, "Finishing")
    // is |value - to| < max(REST_DELTA, REST_REL * scale) with
    // scale = max(|to - from|, |to|); from 0 to 1e5 the relative term
    // governs and the gate is 1.0.
    const TO: f32 = 1e5;
    const POSITION_GATE: f32 = 1e-5 * TO;

    for stiffness in [100.0, 400.0, 1500.0, 10_000.0] {
        let mut s = Scheduler::new();
        s.start(
            K,
            0.0,
            TO,
            TransitionSpec::Spring {
                stiffness,
                damping_ratio: 1.0,
            },
            0.0,
        );

        let mut steps = 0;
        let mut entered_gate = None;
        while !s.is_settled() {
            s.advance(STEP);
            steps += 1;
            assert!(steps < 10_000, "spring never reached rest at {stiffness}");
            if entered_gate.is_none() && (s.sample(K).unwrap() - TO).abs() < POSITION_GATE {
                entered_gate = Some(steps);
            }
        }
        assert_eq!(
            entered_gate,
            Some(steps),
            "at stiffness {stiffness} the spring finished at step {steps}, later than the step \
             on which it entered the position gate — the velocity gate is the binding condition"
        );
    }
}

#[test]
fn keyframes_interpolate_through_declared_frames_including_overshoot() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![Keyframe { t: 0.5, value: 1.5 }],
        },
        0.0,
    );

    s.advance(0.25); // between (0,0) and (0.5,1.5): progress 0.75
    assert_eq!(s.sample(K), Some(75.0));
    s.advance(0.25); // at the declared frame: progress 1.5 (overshoot)
    assert_eq!(s.sample(K), Some(150.0));
    s.advance(0.25); // between (0.5,1.5) and (1,1): progress 1.25
    assert_eq!(s.sample(K), Some(125.0));
    s.advance(0.25); // done: exactly `to`
    assert_eq!(s.sample(K), Some(100.0));
}

#[test]
fn keyframes_with_no_declared_frames_degrade_to_linear() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![],
        },
        0.0,
    );

    s.advance(0.5);
    assert_eq!(s.sample(K), Some(50.0));
}

#[test]
#[should_panic(expected = "non-decreasing")]
fn start_panics_on_unsorted_keyframes() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        1.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![
                Keyframe { t: 0.6, value: 0.5 },
                Keyframe { t: 0.4, value: 0.9 },
            ],
        },
        0.0,
    );
}

#[test]
#[should_panic(expected = "dt")]
fn advance_panics_on_a_negative_dt() {
    let mut s = Scheduler::new();
    s.advance(-0.1);
}

#[test]
#[should_panic(expected = "from must be finite")]
fn a_fresh_start_panics_on_a_non_finite_from() {
    // The caller-supplied `from` is only meaningful on a fresh start, and
    // there it must be finite (issue #71).
    let mut s = Scheduler::new();
    s.start(K, f32::NAN, 1.0, linear_tween(1.0), 0.0);
}

#[test]
#[should_panic(expected = "duration")]
fn start_panics_on_a_non_positive_tween_duration() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 1.0, linear_tween(0.0), 0.0);
}

#[test]
#[should_panic(expected = "damping_ratio")]
fn start_panics_on_a_zero_damping_ratio() {
    // An undamped spring oscillates forever and never finishes (#72):
    // validation rejects damping_ratio == 0.
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        1.0,
        TransitionSpec::Spring {
            stiffness: 100.0,
            damping_ratio: 0.0,
        },
        0.0,
    );
}

/// A pair of keyframes at the same `t` is a step, and the scheduler samples it
/// as one (issue #852, `docs/decisions/a-step-is-a-pair-of-keyframes.md`).
///
/// The span is 0 -> 100 so the sampled value reads directly as a percentage of
/// the step: 0 before the flip, 100 after it.
#[test]
fn two_keyframes_at_one_t_are_a_step() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![
                Keyframe { t: 0.4, value: 0.0 },
                Keyframe { t: 0.4, value: 1.0 },
            ],
        },
        0.0,
    );

    s.advance(0.2);
    assert_eq!(s.sample(K), Some(0.0), "held before the step");
    s.advance(0.1);
    assert_eq!(s.sample(K), Some(0.0), "still held just before it");
    s.advance(0.1);
    assert_eq!(s.sample(K), Some(100.0), "flipped at the step");
    s.advance(0.2);
    assert_eq!(s.sample(K), Some(100.0), "held after it");
}

/// A multi-step sequence, which is what `calcMode="discrete"` with several
/// values is: two flips, held between them.
#[test]
fn four_keyframes_are_two_steps() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![
                Keyframe { t: 0.3, value: 0.0 },
                Keyframe { t: 0.3, value: 0.5 },
                Keyframe { t: 0.7, value: 0.5 },
                Keyframe { t: 0.7, value: 1.0 },
            ],
        },
        0.0,
    );

    s.advance(0.1);
    assert_eq!(s.sample(K), Some(0.0));
    s.advance(0.4);
    assert_eq!(s.sample(K), Some(50.0), "the middle step");
    s.advance(0.4);
    assert_eq!(s.sample(K), Some(100.0), "the last step");
}

/// Three frames at one `t` is a producer error, not a step.
///
/// Sampling walks to the last frame at a given `t`, so the middle one carries
/// a value no sample can return. Named rather than ignored (P4).
#[test]
#[should_panic(expected = "at most two keyframes may share a t")]
fn three_keyframes_at_one_t_are_refused() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        1.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![
                Keyframe { t: 0.4, value: 0.0 },
                Keyframe { t: 0.4, value: 0.5 },
                Keyframe { t: 0.4, value: 1.0 },
            ],
        },
        0.0,
    );
}

/// The open interval is unchanged by #852: a frame at 0 or 1 restates an
/// endpoint that is already implicit.
#[test]
#[should_panic(expected = "strictly inside (0, 1)")]
fn a_keyframe_at_zero_is_still_refused() {
    let mut s = Scheduler::new();
    s.start(
        K,
        0.0,
        1.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![Keyframe { t: 0.0, value: 1.0 }],
        },
        0.0,
    );
}

// ---------------------------------------------------------------------
// Loop tracks (story #772): the ambient class. A loop repeats its curve
// indefinitely and never finishes, so it never settles.
// ---------------------------------------------------------------------

#[test]
fn a_loop_repeats_its_curve_and_never_finishes() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 0.0);

    assert_eq!(s.sample(K), Some(0.0), "live before any advance");
    s.advance(0.25);
    assert_eq!(s.sample(K), Some(25.0));
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(75.0));

    // Past the duration the cycle wraps rather than snapping to `to` and
    // finishing: 0.75 + 0.5 = 1.25, which is 0.25 into the second cycle.
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(25.0), "the second cycle, not the target");
    assert!(!s.is_settled(), "a loop is never settled");

    // And it is never swept, however many cycles run.
    for _ in 0..100 {
        s.advance(0.1);
    }
    assert!(!s.is_empty(), "a loop track is never swept");
    assert!(!s.is_settled());
}

/// A whole number of cycles returns to the start of the curve, not to its
/// end — the wrap is exact, so a loop cannot drift over a long session.
#[test]
fn a_whole_number_of_cycles_returns_to_the_start_of_the_curve() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    for _ in 0..40 {
        s.advance(0.25);
    }
    assert_eq!(s.sample(K), Some(0.0), "ten exact cycles land back at 0");
}

/// The phase offset is what staggers a row of skeleton bars: the track
/// starts that far into its own cycle rather than holding at `from`,
/// which is what `delay` does for a one-shot.
#[test]
fn a_phase_offset_starts_the_loop_partway_through_its_cycle() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 0.25);
    assert_eq!(s.sample(K), Some(25.0), "seeded a quarter into the cycle");
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(75.0));
}

/// An offset of a whole cycle or more is the same phase as its remainder,
/// so a producer cannot push a track arbitrarily far into the future.
#[test]
fn a_phase_offset_beyond_one_cycle_wraps() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 3.25);
    assert_eq!(s.sample(K), Some(25.0));
}

/// A spring carries velocity and has no duration, so it has no cycle to
/// repeat. Refused by name rather than looped on some invented period
/// (P4) — the shape #852 chose for a third keyframe sharing a `t`.
#[test]
#[should_panic(expected = "a spring has no duration")]
fn a_looping_spring_is_refused_by_name() {
    let mut s = Scheduler::new();
    s.start_loop(
        K,
        0.0,
        100.0,
        TransitionSpec::Spring {
            stiffness: 100.0,
            damping_ratio: 1.0,
        },
        0.0,
    );
}

/// A keyframes curve loops on the same path as a tween — the shared timed
/// path (#75), so the wrap is written once.
#[test]
fn a_keyframes_loop_wraps_on_the_same_path_as_a_tween() {
    let mut s = Scheduler::new();
    s.start_loop(
        K,
        0.0,
        100.0,
        TransitionSpec::Keyframes {
            duration: 1.0,
            frames: vec![Keyframe {
                t: 0.5,
                value: 0.25,
            }],
        },
        0.0,
    );
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(25.0), "the declared frame at t = 0.5");
    s.advance(1.0);
    assert_eq!(s.sample(K), Some(25.0), "the same point one cycle later");
    assert!(!s.is_settled());
}

/// Starting a transition on a key a loop holds is refused by name, not
/// resolved by the retarget path (story #772).
///
/// `start`'s retarget removes the live track, so a transition landing on a
/// looping key would end the loop with no diagnostic — which contradicts the
/// ruling that nothing ends a loop. The load gate refuses the document that
/// could cause it; this is the backstop, and it had no test until review
/// asked for one.
#[test]
#[should_panic(expected = "carries a loop track")]
fn starting_a_transition_on_a_looping_key_is_refused() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.start(K, 0.0, 50.0, linear_tween(1.0), 0.0);
}

/// And the reverse: a second loop on one key is refused too, so a loop is
/// the sole writer of its channel in the scheduler as well as in the gate.
#[test]
#[should_panic(expected = "already carries a track")]
fn a_second_loop_on_one_key_is_refused() {
    let mut s = Scheduler::new();
    s.start_loop(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.start_loop(K, 0.0, 50.0, linear_tween(1.0), 0.0);
}
