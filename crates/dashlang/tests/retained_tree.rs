//! A tick that replays the retained geometry still leaves the scene's own
//! retained Taffy tree describing the scene.
//!
//! `LiveScene` keeps one `Box<dyn LayoutSolver>` for the life of a scene and
//! that solver keeps Taffy's tree, restyling each commit's layout-dirty set
//! rather than rebuilding (issue #164). A contained scalar write does not run
//! that solver at all: `apply_scalar_write`'s `WriteClass::Patch` arm stages the
//! prop and patches the cached rect itself, and the tick commits through
//! `CachedSolver`, which replays the patched cache (debt #191, A1). The prop is
//! staged all the same, `Prop::Width` is `PropClass::Layout`, and so the commit
//! finds a non-empty layout-dirty set that no solver read.
//!
//! `corpus/showcase/tests/retained_tree.rs` makes the same statement over the
//! showcase scenes, by comparing committed rects against a fresh solve after
//! each scripted phase — and it passed on the `main` this file was written
//! against, where the defect below is live. So no showcase scene reaches it,
//! and the statement needs a scene built for it rather than a scene that
//! happens to have one. This file is that scene.

use dashlang::{Arena, Channel, LayoutMode, LiveScene, Scene, Signal, node};
use dashscene_core::{LayoutSolver, NodeId};
use dashscene_engine::TaffySolver;

/// One simulated frame at 60 Hz.
const DT: f32 = 1.0 / 60.0;

/// A three-deep passthrough column: `frame` → `mid` → `chip`.
///
/// Every node is `LayoutMode::None` and fixed-size, which is what makes
/// `chip` **ancestor-contained** — so a width write on it classifies as
/// `WriteClass::Patch` and takes the replay path.
///
/// `frame` binds `Channel::X`, and binds it on a node **with children**, so
/// `write_is_single_rect` refuses to call it contained and it classifies as
/// `WriteClass::Solve`. That is the reflow this file needs, and it has to
/// happen two levels above `chip`: `dashscene_engine`'s incremental solve
/// restyles every dirty node *and its children*, so a write on `mid` would
/// restyle `chip` as a side effect and repair the very staleness under test.
///
/// Built once and used by both scenes below, rather than written twice: what
/// the tests here turn on is *which* properties make `chip` ancestor-contained,
/// so a second copy is a second place to change them — and changing one leaves
/// a test silently taking the `Solve` arm while still passing.
fn column_root(width: Signal<f32>, shift: Signal<f32>) -> dashlang::Node {
    node("frame")
        .mode(LayoutMode::None)
        .size(400.0, 200.0)
        .bind(Channel::X, shift)
        .child(
            node("mid")
                .mode(LayoutMode::None)
                .at(10.0, 10.0)
                .size(300.0, 120.0)
                .child(
                    node("chip")
                        .mode(LayoutMode::None)
                        .at(5.0, 5.0)
                        .size(40.0, 20.0)
                        .bind(Channel::Width, width),
                ),
        )
}

fn column(arena: &mut Arena) -> (LiveScene, Signal<f32>, Signal<f32>) {
    let mut scene = Scene::new();
    let width = scene.signal(40.0f32);
    let shift = scene.signal(0.0f32);
    scene.roots([column_root(width, shift)]);
    let live = scene.build_live(arena, Box::new(TaffySolver::new()));
    (live, width, shift)
}

/// The scene's three nodes, found by walking rather than by index arithmetic.
fn nodes(arena: &Arena) -> (NodeId, NodeId, NodeId) {
    let frame = arena.roots()[0];
    let mid = arena.children(frame)[0];
    let chip = arena.children(mid)[0];
    (frame, mid, chip)
}

/// What a solver with no retained tree resolves `node` to, against the arena as
/// it stands — the answer the committed table has to agree with.
///
/// Fresh (`TaffySolver::new`, `state: None`) is the whole point: it builds its
/// tree from the arena now, so anything the scene's own solver failed to
/// restyle shows up as a disagreement rather than being reproduced.
fn resolved(arena: &Arena, node: NodeId) -> [f32; 4] {
    let mut fresh = TaffySolver::new();
    let (_, rect) = fresh
        .solve(arena)
        .into_iter()
        .find(|(id, _)| *id == node)
        .expect("a fresh solve reports every node of a scene this small");
    [rect.x, rect.y, rect.w, rect.h]
}

/// The committed rect for `node`.
fn published(arena: &Arena, node: NodeId) -> [f32; 4] {
    let committed = arena.committed();
    let row = committed
        .rect_index_of(node)
        .expect("every node of this scene is under the shown root");
    let entry = &committed.rects()[row as usize];
    [entry.x, entry.y, entry.w, entry.h]
}

#[test]
fn a_reflow_after_a_replay_tick_publishes_the_width_the_replay_staged() {
    let mut arena = Arena::new();
    let (mut live, width, shift) = column(&mut arena);
    let (frame, _mid, chip) = nodes(&arena);

    // Tick 1: a contained width write. No solve runs — the tick patches the
    // cached rect and replays it — so this only checks the patch reached the
    // table. Without it the reflow below could agree by the write never having
    // landed at all.
    live.set(width, 60.0);
    live.tick(DT, &mut arena);
    assert_eq!(
        published(&arena, chip)[2],
        60.0,
        "the contained write publishes through the patched cache",
    );

    // Tick 2: move the root. This is a `WriteClass::Solve` write, so the real
    // solver runs inside the commit, and the root's shifted origin makes the
    // read-back descend the whole subtree and re-emit `chip` out of the Taffy
    // tree — which is what turns a stale tree into a stale published rect.
    live.set(shift, 50.0);
    live.tick(DT, &mut arena);

    assert_eq!(
        published(&arena, frame),
        resolved(&arena, frame),
        "the root moved and its own rect must agree with a fresh solve",
    );
    assert_eq!(
        published(&arena, chip),
        resolved(&arena, chip),
        "the reflow re-emitted `chip` out of the retained tree; if tick 1's \
         layout-dirty set was drained without any solver reading it, that tree \
         still carries the build-time width and this publishes it",
    );
}

#[test]
fn a_replay_tick_leaves_its_layout_dirty_set_for_the_next_real_solve() {
    // The mechanism the test above observes through its output, stated where
    // it happens: a commit whose solver did not solve leaves the set in place.
    let mut arena = Arena::new();
    let (mut live, width, _shift) = column(&mut arena);
    let (_frame, _mid, chip) = nodes(&arena);

    live.set(width, 60.0);
    live.tick(DT, &mut arena);

    assert!(
        arena.layout_dirty().contains(&chip),
        "the replay commit ran no solver, so `chip` is still owed a restyle",
    );
}

#[test]
fn a_text_binding_tick_leaves_its_layout_dirty_set_too() {
    // The second in-tree instance of the same class, and the one a width test
    // cannot stand in for. A text binding stages `Prop::Text` and sets no
    // `layout_dirty` flag, so its tick takes the plain replay arm — and
    // `Prop::Text` is `PropClass::Layout`, because the string is what a
    // measuring solver sizes a hug node to. A drained commit here left the
    // retained tree's node context holding the previous string.
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let value = scene.signal(0.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 40.0)
        .child(
            node("label")
                .mode(LayoutMode::None)
                .size(160.0, 20.0)
                .bind_text(value.map(|v| format!("{v:.0} km/h"))),
        )]);
    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let frame = arena.roots()[0];
    let label = arena.children(frame)[0];

    live.set(value, 42.0);
    live.tick(DT, &mut arena);

    assert_eq!(
        arena.text(label),
        Some("42 km/h"),
        "the binding staged its string, so there is something to be owed",
    );
    assert!(
        arena.layout_dirty().contains(&label),
        "a text tick solves nothing either, so `label` is still owed a restyle",
    );
}

/// A scene solver that replays rects a producer resolved for itself and says
/// so — the shape `LayoutSolver::consumes_layout_dirty`'s documentation
/// sanctions, handed to `build_live` as the scene's own solver.
///
/// It reports every node every time, so it satisfies `commit_with` without
/// ever reading `Arena::layout_dirty`.
struct ReplayingScene {
    rects: Vec<(NodeId, dashscene_core::SolvedRect)>,
}

impl LayoutSolver for ReplayingScene {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, dashscene_core::SolvedRect)> {
        if self.rects.is_empty() {
            let mut all: Vec<NodeId> = arena.roots().to_vec();
            let mut i = 0;
            while i < all.len() {
                all.extend(arena.children(all[i]).iter().copied());
                i += 1;
            }
            self.rects = all
                .into_iter()
                .map(|id| {
                    let l = arena.layout(id);
                    (
                        id,
                        dashscene_core::SolvedRect {
                            x: l.x,
                            y: l.y,
                            w: 1.0,
                            h: 1.0,
                        },
                    )
                })
                .collect();
        }
        self.rects.clone()
    }

    fn consumes_layout_dirty(&self) -> bool {
        false
    }
}

#[test]
fn a_layout_dirty_tick_does_not_drain_behind_a_replaying_scene_solver() {
    // `LiveScene::tick`'s layout-dirty arm wraps the scene's solver in
    // `FlipOverlay`, which forwards `solve` — so it must forward this answer
    // too. Taking the trait's `true` default there is correct only while the
    // wrapped solver happens to consume, and drains a set nothing read the
    // moment it does not. That is issue #621's decorator trap on a third
    // method, and this is the test that refuses it.
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let width = scene.signal(40.0f32);
    let shift = scene.signal(0.0f32);
    scene.roots([column_root(width, shift)]);
    let mut live = scene.build_live(&mut arena, Box::new(ReplayingScene { rects: Vec::new() }));
    let (frame, _mid, _chip) = nodes(&arena);

    // A `WriteClass::Solve` write: this takes the `FlipOverlay` arm, the one
    // arm that runs the scene's solver inside the commit.
    live.set(shift, 50.0);
    live.tick(DT, &mut arena);

    assert!(
        arena.layout_dirty().contains(&frame),
        "the scene's solver reported that it read nothing, so the overlay \
         wrapping it must report the same and the set must survive",
    );
}

/// The same column, plus a second root carrying a variant set — enough to
/// reach `LiveScene::tick`'s **other** `CachedSolver` arm.
///
/// A tick with a staged switch and no solve-forcing write runs the real solver
/// at step 0, *before* the transaction opens, and then commits the reflowed
/// rects through `CachedSolver`. The solver did solve; it just solved something
/// older than what this commit drains.
fn column_and_shelf(arena: &mut Arena) -> (LiveScene, Signal<f32>, Signal<f32>) {
    let mut scene = Scene::new();
    let width = scene.signal(40.0f32);
    let shift = scene.signal(0.0f32);

    scene.roots([
        column_root(width, shift),
        node("shelf")
            .mode(LayoutMode::Horizontal)
            .size(200.0, 40.0)
            .children([
                node("left").size(40.0, 40.0),
                node("right").size(40.0, 40.0),
            ]),
    ]);

    let live = scene.build_live(arena, Box::new(TaffySolver::new()));
    (live, width, shift)
}

#[test]
fn a_switch_tick_carries_the_writes_staged_after_its_solve() {
    use dashscene_core::{VariantMember, VariantValue};

    let mut arena = Arena::new();
    let (mut live, width, shift) = column_and_shelf(&mut arena);
    let (_frame, _mid, chip) = nodes(&arena);

    // A set that widens the shelf's first chip, staged after the build
    // because the builder carries no variant vocabulary — the seam an
    // embedder stages a switch through.
    //
    // Staged and **not committed**, like the `set_variant` below it.
    // `switched_variants` reads `arena.variant_sets()` and
    // `arena.active_variant` live, so a commit buys nothing here — and a
    // commit through a solver of this test's own is precisely the second
    // producer this file exists to reason about. It would drain the arena's
    // layout-dirty set behind the scene's own solver, which is harmless only
    // while `add_variant_set` stages no layout intent: the day it stages any,
    // this test would go on passing while testing nothing.
    let set = {
        let shelf = arena.roots()[1];
        let left = arena.children(shelf)[0];
        let mut txn = arena.open();
        txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("wide".to_owned()),
                overrides: vec![(left, VariantValue::Width(120.0))],
            },
        ])
    };

    // Tick 1: the switch **and** a contained write, in that order. The switch
    // takes `tick`'s step-0 solve, which reads the arena as it stands then;
    // the width is staged afterwards, inside the transaction, and the commit
    // that follows replays the patched cache. So the set this commit would
    // drain is not the set that solve read.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
    }
    live.set(width, 60.0);
    live.tick(DT, &mut arena);
    assert_eq!(
        published(&arena, chip)[2],
        60.0,
        "the contained write publishes through the patched cache",
    );

    // Tick 2: the reflow that reads `chip` back out of the retained tree.
    live.set(shift, 50.0);
    live.tick(DT, &mut arena);

    assert_eq!(
        published(&arena, chip),
        resolved(&arena, chip),
        "a solver that solved *outside* the commit has not read the set the \
         commit is about to drain: reporting consumption on this arm discards \
         every prop staged after that solve",
    );
}
