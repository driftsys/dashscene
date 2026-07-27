//! Mid-flight retarget (R4) and variant-transition stagger tests
//! (issue #21): dashcue's public API only.

use dashcue::{Easing, PropKey, PropTransition, Scheduler, TransitionSpec, VariantTransition};

const K: PropKey = PropKey(1);
const STEP: f32 = 1.0 / 120.0;

fn linear_tween(duration: f32) -> TransitionSpec {
    TransitionSpec::Tween {
        duration,
        easing: Easing::Linear,
    }
}

fn spring() -> TransitionSpec {
    TransitionSpec::Spring {
        stiffness: 100.0,
        damping_ratio: 1.0,
    }
}

#[test]
fn tween_retarget_restarts_from_the_current_sample_and_ignores_from() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(50.0));

    s.start(K, 999.0, 0.0, linear_tween(1.0), 0.0); // `from` ignored
    assert_eq!(s.sample(K), Some(50.0)); // continuous at the retarget
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(25.0)); // halfway from 50 toward 0
}

#[test]
fn retarget_accepts_a_non_finite_from_because_it_is_ignored() {
    // The retarget path discards the caller-supplied `from` — the live
    // track's current sample wins — so a placeholder must not be rejected
    // there (issue #71).
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);

    s.start(K, f32::NAN, 0.0, linear_tween(1.0), 0.0);
    assert_eq!(s.sample(K), Some(50.0)); // continuous at the retarget
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(25.0));
}

#[test]
#[should_panic(expected = "span must be finite")]
fn retarget_panics_when_the_new_target_overflows_the_span() {
    // The span is measured from the live sample the retarget starts from,
    // not from the ignored `from` argument (issue #70).
    let mut s = Scheduler::new();
    s.start(K, 0.0, -3e38, linear_tween(1.0), 0.0);
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(-1.5e38));

    s.start(K, 0.0, 3e38, linear_tween(1.0), 0.0);
}

#[test]
fn spring_retarget_keeps_position_and_velocity() {
    // A: launch toward 100, then retarget to 0 mid-flight.
    let mut a = Scheduler::new();
    a.start(K, 0.0, 100.0, spring(), 0.0);
    for _ in 0..60 {
        a.advance(STEP);
    }
    let mid = a.sample(K).unwrap();
    a.start(K, 999.0, 0.0, spring(), 0.0);
    assert_eq!(a.sample(K), Some(mid)); // position carried

    // B: a fresh spring at the same position with zero velocity.
    let mut b = Scheduler::new();
    b.start(K, mid, 0.0, spring(), 0.0);

    a.advance(STEP);
    b.advance(STEP);
    // Both accelerate toward 0, but A still carries its old upward
    // velocity, so after one step A sits above B — that difference is
    // exactly the carried velocity times the step.
    assert!(a.sample(K).unwrap() > b.sample(K).unwrap());
    assert!(b.sample(K).unwrap() < mid); // B starts from rest: straight down
}

#[test]
fn tween_to_spring_retarget_hands_off_zero_velocity() {
    let mut s = Scheduler::new();
    s.start(K, 0.0, 100.0, linear_tween(1.0), 0.0);
    s.advance(0.5);

    s.start(K, 999.0, 50.0, spring(), 0.0); // already at 50, no velocity
    s.advance(STEP);
    assert_eq!(s.sample(K), Some(50.0)); // at rest on the target: no motion
    s.advance(0.0);
    assert!(s.is_empty()); // and the rest thresholds finished it
}

#[test]
fn retarget_during_the_delay_rearms_from_the_held_sample() {
    let mut s = Scheduler::new();
    s.start(K, 10.0, 100.0, linear_tween(1.0), 1.0);
    s.advance(0.5); // still delayed, holding at `from`
    assert_eq!(s.sample(K), Some(10.0));

    s.start(K, 77.0, 200.0, linear_tween(1.0), 0.0); // `from` ignored
    assert_eq!(s.sample(K), Some(10.0));
    s.advance(0.5);
    assert_eq!(s.sample(K), Some(105.0)); // halfway from 10 toward 200
}

#[test]
fn variant_transition_staggers_tracks_by_declaration_order() {
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: PropKey(1),
                spec: linear_tween(1.0),
            },
            PropTransition {
                prop: PropKey(2),
                spec: linear_tween(1.0),
            },
            PropTransition {
                prop: PropKey(3),
                spec: linear_tween(1.0),
            },
        ],
        stagger: 0.25,
    };
    let mut s = Scheduler::new();
    s.start_transition(&transition, |_| Some((0.0, 100.0)));
    assert_eq!(s.len(), 3);

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(1)), Some(25.0)); // 0.25 into its tween
    assert_eq!(s.sample(PropKey(2)), Some(0.0)); // delay just consumed
    assert_eq!(s.sample(PropKey(3)), Some(0.0)); // 0.25 of delay left

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(1)), Some(50.0));
    assert_eq!(s.sample(PropKey(2)), Some(25.0));
    assert_eq!(s.sample(PropKey(3)), Some(0.0));

    s.advance(0.25);
    assert_eq!(s.sample(PropKey(3)), Some(25.0));
}

// ---------------------------------------------------------------------
// A binding may decline a track (issue #74): without that, a prop the
// caller considers unchanged still starts a constant-value track that
// stays live for the spec's whole duration and retargets any concurrent
// start on that key.
// ---------------------------------------------------------------------
#[test]
fn start_transition_skips_a_track_its_binding_declines() {
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: PropKey(1),
                spec: linear_tween(1.0),
            },
            PropTransition {
                prop: PropKey(2),
                spec: linear_tween(1.0),
            },
        ],
        stagger: 0.0,
    };
    let mut s = Scheduler::new();
    s.start_transition(&transition, |prop| {
        (prop == PropKey(2)).then_some((0.0, 100.0))
    });

    assert_eq!(s.len(), 1);
    assert_eq!(
        s.sample(PropKey(1)),
        None,
        "the declined track never starts"
    );
    assert_eq!(s.sample(PropKey(2)), Some(0.0));
}

#[test]
fn a_declined_track_leaves_the_later_tracks_on_their_declared_stagger() {
    // The delay is `stagger * declaration index`, so declining the first
    // track must not pull the second one forward.
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: PropKey(1),
                spec: linear_tween(1.0),
            },
            PropTransition {
                prop: PropKey(2),
                spec: linear_tween(1.0),
            },
        ],
        stagger: 0.25,
    };
    let mut s = Scheduler::new();
    s.start_transition(&transition, |prop| {
        (prop == PropKey(2)).then_some((0.0, 100.0))
    });

    s.advance(0.25); // exactly the second track's declared delay
    assert_eq!(s.sample(PropKey(2)), Some(0.0));
    s.advance(0.25);
    assert_eq!(s.sample(PropKey(2)), Some(25.0));
}

#[test]
#[should_panic(expected = "duplicate prop key")]
fn start_transition_panics_on_a_duplicate_prop_key() {
    // Two tracks for one prop: the second `start` would take the retarget
    // path and drop the first track's spec and stagger delay with no
    // diagnostic (P4, issue #69).
    let transition = VariantTransition {
        tracks: vec![
            PropTransition {
                prop: K,
                spec: linear_tween(1.0),
            },
            PropTransition {
                prop: K,
                spec: spring(),
            },
        ],
        stagger: 0.0,
    };
    Scheduler::new().start_transition(&transition, |_| Some((0.0, 100.0)));
}

#[test]
#[should_panic(expected = "stagger")]
fn start_transition_panics_on_a_negative_stagger() {
    let transition = VariantTransition {
        tracks: vec![],
        stagger: -0.1,
    };
    Scheduler::new().start_transition(&transition, |_| Some((0.0, 1.0)));
}
