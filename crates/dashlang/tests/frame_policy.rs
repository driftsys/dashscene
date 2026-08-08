//! The frame policy `LiveScene` owns: the delta clamp, and the generation
//! gate (story #810).
//!
//! Both rules used to live in each host, written twice.
//! `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`
//! argues the clamp; `docs/decisions/crate-name-map.md` records why it moved
//! here before stories #741 and #794 published a copy in each integration
//! crate. `demo/tests/host_policy_invariant.rs` is what keeps it moved; this
//! file is what says it works.

use dashlang::{Arena, Channel, LayoutMode, MAX_FRAME_DELTA, Scene, Signal, Spring, node};
use dashscene_engine::TaffySolver;

/// A scene whose "bar" node — rect index 1 — has its width smoothed by a
/// spring, so the only thing that moves it is the scheduler advancing.
///
/// A spring rather than a direct binding on purpose: a direct write lands
/// whole on the first tick whatever `dt` is, so it could not tell a clamped
/// step from an unclamped one.
fn spring_scene(arena: &mut Arena) -> (dashlang::LiveScene, Signal<f32>) {
    let mut scene = Scene::new();
    let target = scene.signal(0.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            node("bar")
                .mode(LayoutMode::None)
                .size(0.0, 12.0)
                .bind(Channel::Width, target)
                .smooth(Channel::Width, Spring::critically_damped(0.1)),
        )]);
    let live = scene.build_live(arena, Box::new(TaffySolver::new()));
    (live, target)
}

/// The bar's committed width.
fn bar_width(arena: &Arena) -> f32 {
    arena.committed().rects()[1].w
}

#[test]
fn a_delta_larger_than_the_clamp_advances_by_the_clamp() {
    // The stall the clamp exists for: a backgrounded tab or a dragged window
    // hands over a whole second, and the frame that ends the gap must advance
    // by MAX_FRAME_DELTA and no further.
    let mut stalled_arena = Arena::new();
    let (mut stalled, stalled_target) = spring_scene(&mut stalled_arena);
    let mut clamped_arena = Arena::new();
    let (mut clamped, clamped_target) = spring_scene(&mut clamped_arena);

    stalled.set(stalled_target, 100.0);
    clamped.set(clamped_target, 100.0);

    stalled.tick(1.0, &mut stalled_arena);
    clamped.tick(MAX_FRAME_DELTA, &mut clamped_arena);

    let stalled_width = bar_width(&stalled_arena);
    assert!(
        stalled_width < 100.0,
        "a clamped one-second step must still be mid-flight, not settled: {stalled_width}"
    );
    assert_eq!(
        stalled_width,
        bar_width(&clamped_arena),
        "a one-second delta must advance the scheduler exactly as far as a \
         MAX_FRAME_DELTA one — the clamp is what stops a stall arriving as a jump"
    );
}

#[test]
fn a_delta_under_the_clamp_is_passed_through_untouched() {
    // The other half, and the one a clamp written as `dt = MAX_FRAME_DELTA`
    // rather than as a minimum would break. An ordinary frame must not be
    // rounded up to the ceiling.
    let mut ordinary_arena = Arena::new();
    let (mut ordinary, ordinary_target) = spring_scene(&mut ordinary_arena);
    let mut ceiling_arena = Arena::new();
    let (mut ceiling, ceiling_target) = spring_scene(&mut ceiling_arena);

    ordinary.set(ordinary_target, 100.0);
    ceiling.set(ceiling_target, 100.0);

    ordinary.tick(0.016, &mut ordinary_arena);
    ceiling.tick(MAX_FRAME_DELTA, &mut ceiling_arena);

    assert!(
        bar_width(&ordinary_arena) < bar_width(&ceiling_arena),
        "a 16 ms frame must advance less than a 100 ms one: {} vs {}",
        bar_width(&ordinary_arena),
        bar_width(&ceiling_arena)
    );
}

#[test]
fn a_negative_delta_advances_nothing() {
    // A clock that appears to run backwards. The browser host guarded this
    // with a `.max(0.0)` of its own before the rule moved.
    let mut arena = Arena::new();
    let (mut live, target) = spring_scene(&mut arena);
    live.set(target, 100.0);
    live.tick(-1.0, &mut arena);
    assert_eq!(
        bar_width(&arena),
        0.0,
        "a negative delta must not run the scheduler backwards, or forwards"
    );
}

#[test]
fn a_nan_delta_advances_nothing_rather_than_panicking_the_scheduler() {
    // `f32::clamp` returns NaN for a NaN input, and
    // `dashcue::Scheduler::advance` opens with
    // `assert!(dt.is_finite() && dt >= 0.0)`. So the obvious spelling would
    // let one bad timestamp from a host panic the runtime rather than be
    // absorbed. `max` then `min` returns the non-NaN operand instead, so NaN
    // becomes `0.0`. This test is why the order is written the way it is: it
    // fails, by panic, against `clamp`.
    let mut arena = Arena::new();
    let (mut live, target) = spring_scene(&mut arena);
    live.set(target, 100.0);
    live.tick(f32::NAN, &mut arena);
    let width = bar_width(&arena);
    assert!(
        width.is_finite(),
        "a NaN delta must not reach the scheduler: width became {width}"
    );
    assert_eq!(width, 0.0, "and it must advance nothing");
}

#[test]
fn the_gate_reports_advanced_until_the_generation_is_marked_shown() {
    let mut arena = Arena::new();
    let (mut live, target) = spring_scene(&mut arena);

    assert!(
        live.advanced(),
        "a scene nobody has drawn yet always has something to show"
    );

    live.mark_shown();
    assert!(
        !live.advanced(),
        "marking the current generation shown closes the gate"
    );

    // An idle tick commits nothing and holds the generation steady (D4), so
    // the gate stays closed and the host skips the frame.
    live.tick(0.016, &mut arena);
    assert!(
        !live.advanced(),
        "an idle tick commits nothing, so there is still nothing new to show"
    );

    // A signal write moves the generation, and the gate opens again.
    live.set(target, 100.0);
    live.tick(0.016, &mut arena);
    assert!(
        live.advanced(),
        "a commit that moved the generation reopens the gate"
    );

    live.mark_shown();
    assert!(!live.advanced(), "and closes once that one is shown");
}

#[test]
fn a_rebuilt_scene_starts_with_the_gate_open() {
    // The rule that used to be each host's to remember. A rebuild makes a new
    // arena whose generations restart, so a `shown` carried across it would be
    // compared against a number from a different sequence. Holding the gate on
    // `LiveScene` makes that structural: the new scene simply starts unshown.
    let mut arena = Arena::new();
    let (mut live, _) = spring_scene(&mut arena);
    live.mark_shown();
    assert!(!live.advanced(), "the gate is closed before the rebuild");

    let mut rebuilt_arena = Arena::new();
    let (rebuilt, _) = spring_scene(&mut rebuilt_arena);
    assert!(
        rebuilt.advanced(),
        "a rebuilt scene has shown nothing, whatever the scene it replaced had shown"
    );
}
