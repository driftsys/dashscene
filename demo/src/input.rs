//! Maps `winit` input onto the current scene's own signal and its own variant
//! switch (story #573).
//!
//! Deliberately small, and not a general input system: the pointer's horizontal
//! position and two keys drive one scalar signal, and one key runs one action.
//! Four bindings, and no vocabulary beyond them.
//!
//! # Nothing here names a scene
//!
//! This module refers to no scene, no node, no signal name and no colour. It is
//! handed a signal **name** and an optional **action**, both carried on
//! `showcase::Showcase` and passed through `shell::SceneEntry`, and it can drive
//! any scene that declares them — including a scene story #575 loads from a
//! `.dsb` rather than builds in Rust.
//!
//! That is the seam stories #573 and #625 were both blocked on, and it is why
//! this module is the second version of itself. The first bound
//! `crate::SWEEP`, `crate::BAND_FILL` and `crate::VARIANT_SWATCH` — constants
//! of a placeholder scene that lived in `demo/` — and it authored a variant set
//! itself, in the host, against a node it knew by name. Story #574 deleted the
//! placeholder, and the host authoring content is exactly what the `demo/` and
//! `corpus/showcase/` split exists to prevent.
//!
//! # No wake proxy is needed
//!
//! Story #572 named the wake mechanism for a producer outside the event loop: a
//! `winit` `EventLoopProxy`, because a parked loop wakes only for a window event
//! or a proxy message. Key and pointer input *are* window events
//! (`shell::Host::window_event` already receives them), so they wake a parked
//! loop on their own. No proxy is added here.
//!
//! # Two traps this module no longer has a way to fall into
//!
//! Recorded so they are not rediscovered. The first version of this story gave
//! the host its own node to recolour, added by a raw `Txn` against the arena
//! `LiveScene` owns. Running the demonstration — not reading the code —
//! surfaced two failures in order:
//!
//! 1. **Committing the new node after `Scene::build_live` panicked.**
//!    `LiveScene` requires a static tree's committed node count and DFS order
//!    to hold for its whole lifetime, and `refresh_cache`
//!    (`crates/dashlang/src/reactive.rs`) `debug_assert!`s it on every real
//!    solve. A node added afterwards tripped that assertion on the next solve.
//! 2. **Committing it before `build_live` made it invisible.** A node committed
//!    first is necessarily an earlier root, this codebase paints DFS order
//!    back-to-front, and the scene's own backdrop is opaque and fills the
//!    window. The node existed, cost a quad, and was painted over.
//!
//! Neither can arise now: the host adds no node at all. A scene declares its
//! variant set inside its own builder, before `build_live` has captured
//! anything, and `switch_variant` only switches an already-declared set —
//! `Txn::set_variant` adds no node and changes no DFS order.

use dashlang::LiveScene;
use dashscene_core::Arena;
use winit::keyboard::KeyCode;

use crate::shell::SceneAction;

/// Drives `signal` from the pointer's horizontal position, normalised to the
/// drawable's width and clamped to the `0.0..=1.0` range every showcase signal
/// is authored over.
///
/// Returns whether anything was written, so the caller knows whether to force a
/// redraw.
///
/// The write is the whole of what this does: `LiveScene::tick` is what moves
/// anything, on the loop's own thread, at the loop's own time (P3).
pub fn cursor_moved(live: &mut LiveScene, signal: &str, x_physical: f64, width: u32) -> bool {
    if width == 0 {
        // A minimised window: there is no width to normalise against, and no
        // frame to show the result in either.
        return false;
    }
    let normalised = (x_physical as f32 / width as f32).clamp(0.0, 1.0);
    set_signal(live, signal, normalised)
}

/// Handles one key already filtered to a fresh press — `shell::Host` checks
/// `ElementState::Pressed` and `!repeat` before calling this, so holding a key
/// neither floods the signal nor spins the variant.
///
/// Returns whether anything changed.
pub fn key(
    code: KeyCode,
    signal: &str,
    action: Option<SceneAction>,
    live: &mut LiveScene,
    arena: &mut Arena,
) -> bool {
    match code {
        // The two ends of the signal's range, so the same channel the pointer
        // scrubs can be driven to a known value without aiming.
        KeyCode::ArrowLeft => set_signal(live, signal, 0.0),
        KeyCode::ArrowRight => set_signal(live, signal, 1.0),
        KeyCode::Space => match action {
            Some(action) => {
                action(live, arena);
                true
            }
            // The scene declares no variant set. The key does nothing, rather
            // than the host inventing something for it to do.
            None => false,
        },
        _ => false,
    }
}

/// Writes `value` to the scene's named signal, or reports that the scene does
/// not declare it.
///
/// `false` rather than a panic because the name is the scene's to choose and
/// `LiveScene::signal_named` returning `None` is exactly the case a document
/// loaded from a `.dsb` (story #575) can present.
fn set_signal(live: &mut LiveScene, signal: &str, value: f32) -> bool {
    match live.signal_named(signal) {
        Some(handle) => {
            live.set(handle, value);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use dashscene_core::{Color, Fill, NodeId};
    use showcase::Showcase;

    /// The `layout` scene, which is the one that carries an action. Fetched
    /// through `showcase::by_name` rather than by index so the test breaks if
    /// the scene is renamed rather than if the list is reordered.
    fn layout_scene() -> &'static Showcase {
        showcase::by_name("layout").expect("the showcase carries a scene named `layout`")
    }

    /// A scene built into a fresh arena at a fixed extent, exactly as
    /// `shell::Host::rebuild` builds one. A real `Arena` and a real
    /// `LiveScene`, never a stand-in: the paths under test are the ones that
    /// only misbehave against the real thing.
    fn build(scene: &Showcase) -> (Arena, LiveScene) {
        let mut arena = Arena::new();
        let live = (scene.build)(&mut arena, 960, 600);
        (arena, live)
    }

    fn node_of(arena: &Arena, name: &str) -> NodeId {
        *showcase::vocabulary::nodes_by_name(arena)
            .get(name)
            .unwrap_or_else(|| panic!("the scene names a node {name:?}"))
    }

    /// The committed rect index of `name`, or `None` when the node carries no
    /// rect at all.
    fn rect_index(arena: &Arena, name: &str) -> Option<usize> {
        let node = node_of(arena, name);
        let committed = arena.committed();
        (0..committed.rects().len() as u32)
            .find(|&i| committed.node_of(i) == node)
            .map(|i| i as usize)
    }

    /// The committed width of `name`.
    fn committed_width(arena: &Arena, name: &str) -> f32 {
        let index = rect_index(arena, name).expect("the node has a committed rect");
        arena.committed().rects()[index].w
    }

    /// The committed solid fill of `name`.
    fn committed_fill(arena: &Arena, name: &str) -> Option<Color> {
        let index = rect_index(arena, name).expect("the node has a committed rect");
        let committed = arena.committed();
        let paints = committed.paints();
        let kind = paints.resolve(committed.rects()[index].paint).fill;
        match paints.fill(kind) {
            Fill::Solid(color) => Some(color),
            _ => None,
        }
    }

    /// Ticks until every spring in the scene has settled, then measures what
    /// `layout`'s signal actually drives.
    ///
    /// The signal binds the reflow row's gap, and a gap is observable only
    /// through where it puts the chips either side of it — so this is the
    /// distance between the first chip and the third. Ten simulated seconds is
    /// far past the 0.55 s spring response the scene authors.
    fn settled_chip_span(arena: &mut Arena, live: &mut LiveScene) -> f32 {
        for _ in 0..600 {
            live.tick(1.0 / 60.0, arena);
        }
        let a = rect_index(arena, "reflow-a").expect("reflow-a has a rect");
        let c = rect_index(arena, "reflow-c").expect("reflow-c has a rect");
        let committed = arena.committed();
        committed.rects()[c].x - committed.rects()[a].x
    }

    /// The whole point of the seam: the host writes to a signal it knows only
    /// by the name the scene handed it, and the value reaches the bound rect.
    ///
    /// Read back through the committed geometry after ticking the spring out,
    /// not by trusting that the write landed somewhere.
    #[test]
    fn an_arrow_key_drives_the_scenes_own_named_signal() {
        let scene = layout_scene();
        let (mut arena, mut live) = build(scene);

        assert!(key(
            KeyCode::ArrowRight,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let wide = settled_chip_span(&mut arena, &mut live);

        assert!(key(
            KeyCode::ArrowLeft,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let narrow = settled_chip_span(&mut arena, &mut live);

        assert!(
            wide > narrow + 1.0,
            "ArrowRight is the top of the signal's range and ArrowLeft the bottom, so the \
             row's chips have to sit further apart at the top: {wide} against {narrow}"
        );
    }

    /// The pointer drives the same signal over the drawable width, and its two
    /// ends land exactly where the two arrow keys do.
    ///
    /// The equality is the assertion that matters: it is what proves the
    /// pointer is normalised against the drawable rather than fed through in
    /// physical pixels.
    #[test]
    fn the_pointer_reaches_the_same_two_ends_as_the_arrow_keys() {
        let scene = layout_scene();

        let (mut arena, mut live) = build(scene);
        assert!(key(
            KeyCode::ArrowRight,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let by_key = settled_chip_span(&mut arena, &mut live);

        let (mut arena, mut live) = build(scene);
        assert!(cursor_moved(&mut live, scene.signal, 960.0, 960));
        let by_pointer = settled_chip_span(&mut arena, &mut live);

        assert_eq!(
            by_key, by_pointer,
            "the right edge of the drawable is the top of the signal's range, which is where \
             ArrowRight puts it"
        );
    }

    /// A pointer position outside the drawable clamps rather than pushing the
    /// signal past the `0.0..=1.0` range every scene authors its bindings over.
    ///
    /// `dashlang`'s `map_range` is documented as unclamped, so an unclamped
    /// pointer would extrapolate: at four times the window's width this scene's
    /// gap would come out several times what its own top-of-range value is.
    /// Both ends are checked against the arrow key that names the same end.
    #[test]
    fn a_pointer_position_outside_the_drawable_clamps() {
        let scene = layout_scene();

        let (mut arena, mut live) = build(scene);
        assert!(key(
            KeyCode::ArrowRight,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let top_of_range = settled_chip_span(&mut arena, &mut live);

        let (mut arena, mut live) = build(scene);
        assert!(cursor_moved(&mut live, scene.signal, 4000.0, 960));
        assert_eq!(settled_chip_span(&mut arena, &mut live), top_of_range);

        let (mut arena, mut live) = build(scene);
        assert!(key(
            KeyCode::ArrowLeft,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let bottom_of_range = settled_chip_span(&mut arena, &mut live);

        let (mut arena, mut live) = build(scene);
        assert!(cursor_moved(&mut live, scene.signal, -4000.0, 960));
        assert_eq!(settled_chip_span(&mut arena, &mut live), bottom_of_range);

        assert!(
            top_of_range > bottom_of_range + 1.0,
            "the two ends have to differ, or clamping to either proves nothing"
        );
    }

    /// A zero-width drawable — a minimised window — writes nothing rather than
    /// dividing by zero.
    #[test]
    fn a_zero_width_drawable_writes_nothing() {
        let scene = layout_scene();
        let (_arena, mut live) = build(scene);
        assert!(!cursor_moved(&mut live, scene.signal, 12.0, 0));
    }

    /// The variant switch, proven by what it commits.
    ///
    /// Three presses walk the set's three members and wrap. Every assertion
    /// reads the **committed** table back — the width the solver resolved and
    /// the fill the paint table interned — rather than the arena's staged
    /// state, because publishing is the thing that was missing before this
    /// seam existed.
    #[test]
    fn the_scenes_action_commits_a_real_variant_switch() {
        let scene = layout_scene();
        let (mut arena, mut live) = build(scene);
        let press = |arena: &mut Arena, live: &mut LiveScene| {
            assert!(key(KeyCode::Space, scene.signal, scene.action, live, arena));
        };

        let wide = committed_width(&arena, "reflow-d");
        let amber = committed_fill(&arena, "reflow-d");
        assert!(wide > 0.0);
        assert!(amber.is_some());

        // Member 1 overrides Width and Fill: the chip narrows and recolours in
        // one switch, and neither value was written by a `set_prop`.
        press(&mut arena, &mut live);
        let narrow = committed_width(&arena, "reflow-d");
        assert!(
            narrow < wide,
            "member 1 overrides Width downward: {narrow} against {wide}"
        );
        assert_ne!(
            committed_fill(&arena, "reflow-d"),
            amber,
            "member 1 overrides Fill"
        );

        // Member 2 overrides Visible: the chip leaves the laid-out set, which
        // is a topology change and not a resize.
        press(&mut arena, &mut live);
        let gone = committed_width(&arena, "reflow-d");
        assert!(
            gone < narrow,
            "member 2 hides the chip, so its rect collapses: {gone} against {narrow}"
        );

        // And back to the authored state.
        press(&mut arena, &mut live);
        assert_eq!(committed_width(&arena, "reflow-d"), wide);
        assert_eq!(committed_fill(&arena, "reflow-d"), amber);
    }

    /// The switch is the variant machinery and not the `Prop::Visible` path the
    /// scene already had.
    ///
    /// Two things separate them, and both are asserted: the scripted phase
    /// drives `reflow-b` through `Prop::Visible` and leaves the arena's active
    /// member at 0, while the action moves the active member and changes
    /// `reflow-d`, which no `Prop::Visible` write in this scene ever touches.
    #[test]
    fn the_action_is_a_variant_switch_and_not_the_visible_path() {
        let scene = layout_scene();
        let (mut arena, mut live) = build(scene);
        let set = showcase::layout::variant_set().expect("the built scene declared its set");

        // Run the scripted phase through a full cycle of its own topology
        // change. It is the `Prop::Visible` path, so no variant moves.
        for phase in 1..=4 {
            (scene.pulse)(&mut live, phase);
            for _ in 0..120 {
                live.tick(1.0 / 60.0, &mut arena);
            }
        }
        assert_eq!(
            arena.active_variant(set),
            0,
            "the scripted phase writes Prop::Visible and switches no variant"
        );
        let untouched = committed_width(&arena, "reflow-d");

        // The action moves the variant, and it is the variant that changes the
        // chip the scripted phase never reaches.
        assert!(key(
            KeyCode::Space,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        assert_eq!(arena.active_variant(set), 1);
        assert_ne!(committed_width(&arena, "reflow-d"), untouched);
    }

    /// The switch survives every later tick.
    ///
    /// `LiveScene` replays a retained rect cache on a tick that solves nothing,
    /// which would revert a second producer's geometry. It cannot here, because
    /// every signal in this scene drives a layout-affecting channel — but that
    /// is an argument, so this asserts it against a long run of real ticks and
    /// real scripted phases instead of trusting it.
    #[test]
    fn the_switch_survives_the_ticks_and_pulses_that_follow_it() {
        let scene = layout_scene();
        let (mut arena, mut live) = build(scene);
        let authored = committed_width(&arena, "reflow-d");

        assert!(key(
            KeyCode::Space,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
        let switched = committed_width(&arena, "reflow-d");
        // Without this the test would pass on a switch that never happened:
        // an unchanged width holds just as steadily as a changed one.
        assert_ne!(
            switched, authored,
            "the switch has to have moved the chip before its persistence means anything"
        );

        for phase in 1..=8 {
            (scene.pulse)(&mut live, phase);
            for _ in 0..150 {
                live.tick(1.0 / 60.0, &mut arena);
            }
            assert_eq!(
                committed_width(&arena, "reflow-d"),
                switched,
                "phase {phase} replayed a stale rect cache over the variant switch"
            );
        }
    }

    /// A scene that declares no action ignores the key rather than the host
    /// substituting something for it.
    #[test]
    fn a_scene_without_an_action_ignores_the_key() {
        let scene = showcase::by_name("typography").expect("the showcase carries `typography`");
        assert!(scene.action.is_none());
        let (mut arena, mut live) = build(scene);
        assert!(!key(
            KeyCode::Space,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
    }

    /// Every scene has to name a signal its own `LiveScene` actually declares,
    /// or the pointer and the arrow keys are dead in that scene and nothing
    /// says so.
    #[test]
    fn every_scene_names_a_signal_it_declares() {
        for scene in showcase::SCENES {
            let (_arena, live) = build(scene);
            assert!(
                live.signal_named(scene.signal).is_some(),
                "scene {:?} carries the signal name {:?}, which its own LiveScene does not \
                 declare",
                scene.name,
                scene.signal
            );
        }
    }

    /// An unbound key changes nothing, so the host does not force a redraw for
    /// every keystroke a person types at the window.
    #[test]
    fn an_unbound_key_changes_nothing() {
        let scene = layout_scene();
        let (mut arena, mut live) = build(scene);
        assert!(!key(
            KeyCode::KeyQ,
            scene.signal,
            scene.action,
            &mut live,
            &mut arena
        ));
    }
}
