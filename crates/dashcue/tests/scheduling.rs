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
fn samples_iterates_live_tracks_in_start_order() {
    let mut s = Scheduler::new();
    s.start(PropKey(2), 0.0, 1.0, linear_tween(1.0), 0.0);
    s.start(PropKey(1), 5.0, 6.0, linear_tween(1.0), 0.0);

    s.advance(0.5);
    let got: Vec<(PropKey, f32)> = s.samples().collect();
    assert_eq!(got, vec![(PropKey(2), 0.5), (PropKey(1), 5.5)]);
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
#[should_panic(expected = "strictly increasing")]
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
