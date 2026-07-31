//! Maps `winit` input events onto `LiveScene` signal writes and one
//! variant-set cycle (story #573).
//!
//! Deliberately small, and not a general input system: the cursor's
//! horizontal position and two keys drive the placeholder scene's one named
//! signal (`crate::SWEEP`), and one more key cycles a variant set built
//! against a node the scene already has. Story #574's showcase scenes, and
//! any real input vocabulary they need, are separate work.
//!
//! # No wake proxy needed
//!
//! Story #572 named the wake mechanism for a producer outside the event
//! loop: a `winit` `EventLoopProxy`, because a parked loop only wakes for a
//! window event or a proxy message. Key and pointer input arrive as ordinary
//! `WindowEvent`s (`shell::Host::window_event` already receives them), so
//! they wake a parked loop on their own — no proxy is added here.
//!
//! # Why the variant set retargets an existing node, and never adds one
//!
//! The first version of this module added a dedicated node for the variant
//! swatch, in a `Txn` of its own. Running the demo (not reading the code)
//! surfaced two problems with that, in order:
//!
//! 1. `LiveScene` documents that a static tree's committed node count and
//!    DFS order are invariant for its whole lifetime — `refresh_cache`
//!    (`crates/dashlang/src/reactive.rs`) `debug_assert!`s it on every real
//!    solve. Adding a node in a commit of its own, after
//!    `Scene::build_live` had already captured its first cache, tripped
//!    that assertion the moment the placeholder's own pulse forced the next
//!    real solve. Committing the extra node *before* `build_live` instead
//!    avoids that — `build_live`'s own first cache capture, unconditional
//!    on an empty cache, then covers both — but:
//! 2. A node committed before `build_live` runs is necessarily an earlier
//!    root than the scene's own, and this codebase paints DFS order
//!    back-to-front: an earlier root is a *lower* layer, and the
//!    placeholder's own `backdrop` (opaque, full-window) is later. The
//!    swatch existed and never crashed anything, and was completely
//!    invisible — painted, then immediately painted over.
//!
//! Retargeting an existing node sidesteps both: [`crate::VARIANT_SWATCH`]
//! (the "band" bar `main.rs` already draws, unbound by any signal) keeps its
//! `NodeId`, its place in DFS order, and its role in the rect table exactly
//! as `build_live` committed it. [`InputState::attach`] only calls
//! `Txn::add_variant_set` and, to restore a rebuild's prior state,
//! `Txn::set_variant` — neither adds a node, so the node count and DFS order
//! `refresh_cache` checks never change, and nothing about paint order shifts
//! either.
//!
//! # Why the variant set only overrides `Fill`
//!
//! Beyond the node-count question above, `LiveScene`'s paint-only and
//! contained-write commits replay a private geometry cache
//! (`LiveScene`/`CachedSolver` in `crates/dashlang/src/reactive.rs`) rather
//! than asking the real solver, so a second producer that moved a node
//! behind its back would be reverted the next time that cache replays.
//! Restricting every variant member to a `Fill` override sidesteps the
//! question: paint is re-interned from the arena's own live variant-overlay
//! state on **every** commit, by whichever solver ran (`dashscene-core`'s
//! `Txn::commit_with`, gated on `Arena`'s `paint_dirty` set), never from a
//! cached copy. [`crate::VARIANT_SWATCH`]'s geometry is never touched, so
//! there is nothing for the cache to disagree with either way.

use dashlang::LiveScene;
use dashscene_core::{
    Arena, Color, LayoutSolver, NodeId, SolvedRect, VariantMember, VariantSetId, VariantValue,
};
use winit::keyboard::KeyCode;

use crate::{BAND_FILL, SWEEP, VARIANT_SWATCH};

/// The swatch's fill for each variant member, cycled in order by the
/// variant key. Member 0 is [`crate::BAND_FILL`] — the bar's own authored
/// color — so declaring the variant set does not itself change the first
/// frame; the other two are chosen to read as clearly distinct, so a press
/// is unmistakable.
const SWATCH_COLORS: [Color; 3] = [
    BAND_FILL,
    Color {
        r: 0.90,
        g: 0.20,
        b: 0.65,
        a: 1.0,
    },
    Color {
        r: 0.95,
        g: 0.70,
        b: 0.10,
        a: 1.0,
    },
];

/// Input state that survives a scene rebuild (a resize tears down and
/// rebuilds the arena — `shell::Host::rebuild`): the variant set the
/// *current* arena carries, and which member is active, so a rebuild's call
/// to [`InputState::attach`] can restore it rather than snapping back to
/// member 0.
pub struct InputState {
    /// `None` before the first [`InputState::attach`], and again if a scene
    /// does not declare [`crate::VARIANT_SWATCH`] — always `Some` for the
    /// placeholder scene, once a window exists.
    variant_set: Option<VariantSetId>,
    member: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            variant_set: None,
            member: 0,
        }
    }

    /// Declares the variant set against [`crate::VARIANT_SWATCH`] in a
    /// freshly (re)built arena, and restores whichever member was active
    /// before the rebuild.
    ///
    /// Called once per rebuild (`shell::Host::rebuild`), **after** the scene
    /// builder has committed: the node it retargets does not exist before
    /// that. See the module-level "why" for why this never adds a node of
    /// its own.
    pub fn attach(&mut self, arena: &mut Arena) {
        let Some(node) = find_node(arena, VARIANT_SWATCH) else {
            // The scene this host runs does not declare the node this story
            // retargets (story #574's showcase scenes, or story #575's
            // loaded `.dsb`, may not). The variant key becomes a no-op
            // rather than a panic — the same posture `crate::SWEEP` missing
            // already takes in `set_sweep`.
            self.variant_set = None;
            return;
        };

        let members = SWATCH_COLORS
            .iter()
            .map(|&color| VariantMember {
                name: None,
                overrides: vec![(node, VariantValue::Fill(color))],
            })
            .collect();

        let mut txn = arena.open();
        let set = txn.add_variant_set(members);
        if self.member != 0 {
            txn.set_variant(set, self.member);
        }
        // No node added, no rect moved: every node carries forward
        // unchanged. Declaring the variant set, and restoring a non-zero
        // member, are the only changes this commit makes, and both are read
        // fresh from the arena's own live state wherever paint is interned
        // — see the module doc's "why Fill only" section.
        txn.commit_with(&mut NoGeometryChange);
        self.variant_set = Some(set);
    }

    /// Handles one key already filtered to a fresh press (`shell::Host`
    /// checks `ElementState::Pressed` and `!repeat` before calling this).
    /// Returns whether it changed anything, so the caller knows whether to
    /// force a redraw.
    pub fn key(&mut self, arena: &mut Arena, live: &mut LiveScene, code: KeyCode) -> bool {
        match code {
            KeyCode::ArrowLeft => set_sweep(live, 0.0),
            KeyCode::ArrowRight => set_sweep(live, 1.0),
            KeyCode::Space => self.cycle_variant(arena),
            _ => false,
        }
    }

    /// Switches [`crate::VARIANT_SWATCH`] to the next member in
    /// [`SWATCH_COLORS`], wrapping around.
    fn cycle_variant(&mut self, arena: &mut Arena) -> bool {
        let Some(set) = self.variant_set else {
            // `attach` found no node to retarget for this scene.
            return false;
        };
        self.member = (self.member + 1) % SWATCH_COLORS.len();

        let mut txn = arena.open();
        txn.set_variant(set, self.member);
        // Reports nothing: every node carries forward unchanged. Correct
        // because this variant only overrides `Fill` (see the module-level
        // "why" above) — no rect needs to move for the new color to reach
        // the committed scene.
        txn.commit_with(&mut NoGeometryChange);
        true
    }
}

/// Drives `crate::SWEEP` from the cursor's horizontal position, normalized
/// to the drawable's width and clamped to the signal's authored `0.0..=1.0`
/// range. A free function, not a method: it needs no state beyond the
/// signal write itself.
///
/// `sweep` is smoothed everywhere the placeholder scene binds it
/// (`Spring::critically_damped`, `main.rs`), so this reaches the same
/// spring-scheduled path a scripted pulse does (P3: the write is the whole
/// of what this function does; `LiveScene::tick` is what moves anything).
pub fn cursor_moved(live: &mut LiveScene, x_physical: f64, width: u32) -> bool {
    if width == 0 {
        return false;
    }
    let normalized = (x_physical as f32 / width as f32).clamp(0.0, 1.0);
    set_sweep(live, normalized)
}

/// Writes `value` to `crate::SWEEP` if the current scene declares it.
/// `false` when it does not — story #575 replaces the placeholder scene
/// with a loaded `.dsb`, which may not declare a `sweep` signal at all, and
/// a dropped write is exactly what `LiveScene::signal_named` returning
/// `None` is for.
fn set_sweep(live: &mut LiveScene, value: f32) -> bool {
    match live.signal_named(SWEEP) {
        Some(signal) => {
            live.set(signal, value);
            true
        }
        None => false,
    }
}

/// Finds the node named `name` among the ones the current commit carries.
/// A plain linear scan over the committed rect table — this runs once per
/// scene (re)build, never per frame, and the placeholder scene commits on
/// the order of ten nodes.
fn find_node(arena: &Arena, name: &str) -> Option<NodeId> {
    let committed = arena.committed();
    (0..committed.rects().len() as u32)
        .map(|i| committed.node_of(i))
        .find(|&id| arena.name(id) == Some(name))
}

/// Reports nothing: every node carries forward unchanged. See the module
/// doc's "why Fill only" section for why this is correct for a variant
/// switch restricted to `Fill`.
struct NoGeometryChange;

impl LayoutSolver for NoGeometryChange {
    fn solve(&mut self, _arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashscene_core::PaintKind;

    /// The committed solid fill of the node named `name`. Test-only:
    /// production code here only ever writes a fill, never reads one back.
    fn committed_fill(arena: &Arena, name: &str) -> Option<Color> {
        let node = find_node(arena, name)?;
        let committed = arena.committed();
        let index = (0..committed.rects().len() as u32).find(|&i| committed.node_of(i) == node)?;
        match committed
            .paints()
            .resolve(committed.rects()[index as usize].paint)
            .fill
        {
            Some(PaintKind::Solid { color }) => Some(color),
            _ => None,
        }
    }

    /// The committed width of the node named `name`.
    fn committed_width(arena: &Arena, name: &str) -> f32 {
        let node = find_node(arena, name).expect("the placeholder scene names this node");
        let committed = arena.committed();
        let index = (0..committed.rects().len() as u32)
            .find(|&i| committed.node_of(i) == node)
            .expect("a found node has a rect");
        committed.rects()[index as usize].w
    }

    /// Exercises the whole variant path against a real scene built by
    /// `crate::placeholder_scene` — a real `Arena`, not a fake one, is the
    /// point: this is the path that failed twice while running the demo
    /// before it worked (see the module doc's "why" section). `attach`
    /// declares the set without changing the first frame, and each `Space`
    /// press advances to the next member, wrapping back to the start.
    #[test]
    fn cycling_the_variant_rotates_through_every_color_and_wraps() {
        let mut arena = Arena::new();
        let mut live = crate::placeholder_scene(&mut arena, 800, 600);
        let mut input = InputState::new();
        input.attach(&mut arena);

        assert_eq!(committed_fill(&arena, VARIANT_SWATCH), Some(BAND_FILL));

        assert!(input.key(&mut arena, &mut live, KeyCode::Space));
        assert_eq!(
            committed_fill(&arena, VARIANT_SWATCH),
            Some(SWATCH_COLORS[1])
        );

        assert!(input.key(&mut arena, &mut live, KeyCode::Space));
        assert_eq!(
            committed_fill(&arena, VARIANT_SWATCH),
            Some(SWATCH_COLORS[2])
        );

        assert!(input.key(&mut arena, &mut live, KeyCode::Space));
        assert_eq!(committed_fill(&arena, VARIANT_SWATCH), Some(BAND_FILL));
    }

    /// `shell::Host::rebuild`'s sequence: a fresh arena, the scene rebuilt
    /// into it, then `attach` again. It must restore whichever member was
    /// active rather than snapping back to member 0.
    #[test]
    fn attach_restores_the_active_member_across_a_rebuild() {
        let mut arena = Arena::new();
        let mut live = crate::placeholder_scene(&mut arena, 800, 600);
        let mut input = InputState::new();
        input.attach(&mut arena);
        input.key(&mut arena, &mut live, KeyCode::Space);

        let mut arena = Arena::new();
        let _live = crate::placeholder_scene(&mut arena, 900, 700);
        input.attach(&mut arena);

        assert_eq!(
            committed_fill(&arena, VARIANT_SWATCH),
            Some(SWATCH_COLORS[1])
        );
    }

    /// A key and the cursor both drive `crate::SWEEP` through the same
    /// smoothed binding the scripted pulse uses (`main.rs`'s `rule` node) —
    /// confirmed by ticking until the spring settles and reading the bound
    /// rect back, not by trusting that the write reached anything.
    #[test]
    fn arrow_keys_and_the_cursor_drive_the_bound_rect_to_its_target() {
        let width = 800;
        let margin = width as f32 / 16.0;
        let rule_width = width as f32 - 2.0 * margin;

        let mut arena = Arena::new();
        let mut live = crate::placeholder_scene(&mut arena, width, 600);
        let mut input = InputState::new();
        input.attach(&mut arena);

        assert!(input.key(&mut arena, &mut live, KeyCode::ArrowRight));
        for _ in 0..300 {
            live.tick(1.0 / 60.0, &mut arena);
        }
        let width_at_max = committed_width(&arena, "rule");
        assert!(
            (width_at_max - rule_width).abs() < 0.5,
            "expected ~{rule_width}, got {width_at_max}"
        );

        assert!(cursor_moved(&mut live, 0.0, width));
        for _ in 0..300 {
            live.tick(1.0 / 60.0, &mut arena);
        }
        let width_at_min = committed_width(&arena, "rule");
        let expected_min = rule_width / 8.0;
        assert!(
            (width_at_min - expected_min).abs() < 0.5,
            "expected ~{expected_min}, got {width_at_min}"
        );
    }
}
