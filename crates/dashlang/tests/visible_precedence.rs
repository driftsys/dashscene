//! A node may now carry both a static `visible(bool)` and a reactive
//! `visible_when(signal)`. `set_base_props` stages the static value
//! first and `build_live` then seeds bound props from their signal's
//! initial value, so the signal wins. That is the precedence every
//! bound scalar prop already has; this pins it, because the static
//! setter makes the collision reachable for the first time.

use dashlang::{Arena, LayoutMode, Scene, node};
use dashscene_engine::TaffySolver;

#[test]
fn a_visible_signal_wins_over_the_static_value() {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let shown = scene.signal(true);

    scene.roots([node("row")
        .mode(LayoutMode::Horizontal)
        .size(100.0, 20.0)
        .child(
            node("item")
                .size(40.0, 20.0)
                .visible(false)
                .visible_when(shown),
        )]);

    let _live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    // The signal's initial value is `true`, so the item lays out with a
    // real width even though the static setter said hidden.
    let item = arena.committed().rects()[1];
    assert_eq!(item.w, 40.0, "the signal's initial value wins");
}
