//! Scheduler tests (issue #21): fixed time steps, hand-computed
//! expectations — dashcue's public API only.

use dashcue::{Easing, PropKey, Scheduler, TransitionSpec};

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
