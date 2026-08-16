//! Every scene's retained Taffy tree still describes the scene after a run.
//!
//! Each scene hands its `LiveScene` one solver and that solver keeps Taffy's
//! tree, patching it from each commit's layout-dirty set rather than rebuilding
//! it (issue #950). A commit **consumes** that dirty set, so the tree is only
//! correct while nothing else commits geometry into the same arena: a second
//! solver's commit takes a dirty set the scene's solver never sees, and the
//! scene's solver then patches nothing and replays a tree that no longer
//! describes the scene.
//!
//! That failure is silent. The commit succeeds, every rect has a value, and the
//! only symptom is a rect holding the answer to a question nobody is asking any
//! more — which is why this compares rather than trusts. Each scene is driven
//! through its scripted phases and, where it has one, its action; then a
//! **fresh** solver — `state: None`, so its first solve rebuilds from the arena
//! — solves the same arena, and every rect it reports has to match the one the
//! scene committed.
//!
//! # Two tests, because the comparison cannot see the condition
//!
//! `no_scene_makes_its_solver_rebuild_from_a_missed_commit` asks the condition
//! directly. `LiveScene::forced_rebuilds` counts the solves where the scene's
//! solver found the arena's layout-commit generation ahead of the last one it
//! took part in (issue #1104), which is true at the solve that discovers it,
//! whatever any scene happens to draw. That is the test that holds for a second
//! producer added later, on any node.
//!
//! `every_scenes_retained_tree_agrees_with_a_fresh_solve` compares the published
//! rects, and on its own it is **positional**. A stale tree reaches the
//! committed table only once a readback descends to the affected node, so before
//! issue #1104 a `Prop::Width` write committed out of band failed it in
//! `layout::paint` and passed in `surfaces::paint` — the same defect, opposite
//! verdicts, because `layout`'s scripted phases reflow the row its node sits in
//! and `surfaces`' never reach the tile. Issue #1118 was that gap; the counter
//! closes it.
//!
//! A test cannot force the tree to give itself away either. The only lever it
//! has is marking nodes dirty, and `dashscene_engine`'s incremental path
//! restyles every dirty node — and its children — from the arena before solving,
//! so dirtying the tree to force a re-emit *repairs* it instead of exposing it.
//! That was tried here and removed.
//!
//! # What each one is worth now
//!
//! Since issue #1104 a missed commit **no longer produces a wrong picture**: the
//! solver detects it and rebuilds, so the table is right and only the counter
//! moves. Restoring `layout::switch_variant`'s out-of-band commit — the defect
//! issue #950 removed — now fails the counter test alone, where before #1104 it
//! failed this comparison at three nodes.
//!
//! So the comparison is no longer what guards against a second producer. What it
//! still guards is everything else that could put the published table out of
//! step with what the intent solves to — a defect in the incremental patch or in
//! the readback itself, which no counter would notice and which would reach a
//! screen.
//!
//! See `docs/decisions/one-solver-per-live-scene.md`.

use dashlang::LiveScene;
use dashscene_core::{Arena, LayoutSolver};
use dashscene_engine::TaffySolver;
use dashscene_typeset::text::Typesetter;
use showcase::{SCENES, Showcase, resources};

/// The drawable every scene here is built for. Any size does, as long as the
/// scene and the re-solve agree on it — the solve is over the arena, which
/// already holds whatever the build resolved this to.
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

/// Long enough for the scenes' springs to settle: they author a 0.55 s
/// response and this is four simulated seconds, so the comparison below is
/// against a resting layout rather than against a sample mid-flight.
const TICKS_PER_PHASE: usize = 240;

/// One node the committed table and a fresh solve disagree about: its index,
/// the published `[x, y, w, h]`, and the re-solved one.
type Disagreement = (usize, [f32; 4], [f32; 4]);

/// Solves `arena` from scratch and reports every node whose rect disagrees with
/// the committed table, with how many nodes were compared to reach that.
///
/// The fresh solver is the point: a solver with no retained tree builds one from
/// the arena as it stands now and reports every node, so anything the scene's own
/// solver failed to patch shows up here as a disagreement.
///
/// `typesetter` is lent rather than built here, and that is the whole reason
/// this takes a parameter. Freshness is `state: None` on the solver, not a new
/// cascade: `resources::solver` would re-parse Inter Regular, Inter SemiBold and
/// Noto Sans Arabic on each of this test's 30 checks, about 90 font parses for
/// trees that are identical either way. Measured over three runs of the built
/// binary, that is 0.41 s against 0.26 s here.
///
/// Compared exactly rather than within a tolerance. Both answers come from the
/// same Taffy computation over the same intent, and the committed table is an
/// f32 passthrough of a solve with Taffy's rounding disabled (R7), so equal is
/// what "the tree is current" means — a tolerance would admit a tree that had
/// drifted by less than it.
fn disagreements(arena: &Arena, typesetter: &mut Typesetter) -> (Vec<Disagreement>, usize) {
    let mut fresh = TaffySolver::with_text(typesetter, resources::atlases());
    let solved = fresh.solve(arena);
    let committed = arena.committed();
    let mut out = Vec::new();
    let mut compared = 0;
    for (node, rect) in solved {
        // A node the commit resolved no rect for is not a disagreement: the
        // solve covers the shown roots' subtrees and so does the table, but a
        // scene that confined its traversal would legitimately have fewer rows
        // than this solver reports. None of the three do today, so this skips
        // nothing — it is here so that one which did would not fail as a
        // mismatch when it is really a narrower scene.
        let Some(index) = committed.rect_index_of(node) else {
            continue;
        };
        let entry = &committed.rects()[index as usize];
        let published = [entry.x, entry.y, entry.w, entry.h];
        let resolved = [rect.x, rect.y, rect.w, rect.h];
        compared += 1;
        if published != resolved {
            out.push((node.index(), published, resolved));
        }
    }
    (out, compared)
}

/// Fails if the committed table disagrees with a fresh solve, naming `where`.
///
/// The compared count is asserted as well as the disagreement count. Every
/// assertion here is that a list is **empty**, and a comparison that examined no
/// nodes produces an empty list too — so without this the whole test could go
/// green having established nothing, which is the failure mode a guard is least
/// likely to notice about itself.
fn assert_agrees(scene: &Showcase, arena: &Arena, typesetter: &mut Typesetter, when: &str) {
    let (found, compared) = disagreements(arena, typesetter);
    assert!(
        compared > 0,
        "`{}` compared no nodes at all {when}: the solve and the committed table share no node, \
         so an empty disagreement list says nothing",
        scene.name,
    );
    assert!(
        found.is_empty(),
        "`{}` disagrees with a fresh solve {when}, at {} of {compared} node(s) — (node, \
         committed, re-solved): {found:?}",
        scene.name,
        found.len(),
    );
}

/// Drives one scene through all four of its scripted phases, ticking each to
/// rest and checking **after every phase**.
///
/// Checking inside the loop rather than after it is what makes this able to
/// fail, and it is not a matter of precision. A stale retained tree is
/// **transient**: measured against the defect this story removed, the chip's
/// committed width was wrong after phase 1 and right again after phase 2, so a
/// single check at the end of the four phases sees a scene that agrees and
/// reports nothing. The same shape is why `demo`'s
/// `the_switch_survives_the_ticks_and_pulses_that_follow_it` asserts inside its
/// own phase loop.
fn run_phases(
    scene: &Showcase,
    arena: &mut Arena,
    live: &mut LiveScene,
    typesetter: &mut Typesetter,
    when: &str,
) {
    for phase in 1..=4 {
        (scene.pulse)(live, phase);
        for _ in 0..TICKS_PER_PHASE {
            live.tick(1.0 / 60.0, arena);
        }
        assert_agrees(
            scene,
            arena,
            typesetter,
            &format!("{when}, after scripted phase {phase}"),
        );
    }
}

#[test]
fn no_scene_drifts_from_a_fresh_solve_or_makes_its_solver_rebuild() {
    // One cascade for the whole run, lent to every check. See `disagreements`.
    let mut typesetter = resources::new_typesetter();

    for scene in SCENES {
        let mut arena = Arena::new();
        let mut live = (scene.build)(&mut arena, WIDTH, HEIGHT);

        // Build time first, before any tick. This is a floor and not a proof
        // about the two build-time passes: `layout::paint` and
        // `surfaces::paint` each commit through a fresh solver whose first
        // solve is a full rebuild, so whatever they staged, the rects they
        // publish are correct and a from-scratch solve immediately afterwards
        // necessarily agrees. What it does catch is a build that published
        // geometry no solve produced. Whether those passes stage layout intent
        // is answered by the phases below, and only for a node the phases
        // reach — see this file's header.
        assert_agrees(scene, &arena, &mut typesetter, "straight out of its build");

        run_phases(
            scene,
            &mut arena,
            &mut live,
            &mut typesetter,
            "before any variant switch",
        );

        // Then the action, on the one scene that has one, and the phases again
        // over the switched members — the switch changes which member's
        // overrides the solve reads, so a phase after it exercises a different
        // layout than the phases before it did.
        //
        // Pressed until the set wraps back to member 0 rather than a fixed
        // number of times, so a member added to the scene widens this run
        // instead of going untested. The bound is a runaway guard, not the
        // count.
        if let Some(action) = scene.action {
            let sets: Vec<_> = arena.variant_sets().collect();
            assert!(
                !sets.is_empty(),
                "`{}` declares an action, so it declares the set that action switches",
                scene.name,
            );
            let mut presses = 0;
            loop {
                action(&mut live, &mut arena);
                for _ in 0..TICKS_PER_PHASE {
                    live.tick(1.0 / 60.0, &mut arena);
                }
                presses += 1;
                assert_agrees(
                    scene,
                    &arena,
                    &mut typesetter,
                    &format!("after variant press {presses}"),
                );

                // The scripted phases after every press, and not only once at
                // the end. Ticking after a switch is not enough to expose a
                // stale tree on its own: those ticks re-solve, an incremental
                // solve over a dirty set the switch's own commit already
                // consumed reports **nothing**, and a commit told nothing
                // carries the previous rects forward — which are the right
                // ones. The stale value reaches the table only once something
                // else dirties the row and the readback descends far enough to
                // re-emit the chip out of the tree. A scripted phase is that
                // something: `spread` binds the row's gap, `show_middle` its
                // middle chip.
                run_phases(
                    scene,
                    &mut arena,
                    &mut live,
                    &mut typesetter,
                    &format!("after variant press {presses}"),
                );

                if sets.iter().all(|&set| arena.active_variant(set) == 0) {
                    break;
                }
                assert!(
                    presses < 64,
                    "`{}`'s action did not walk its variant sets back to member 0 in {presses} \
                     presses",
                    scene.name,
                );
            }
        }

        // The second assertion, over the same run. `forced_rebuilds` is
        // cumulative, so one read here covers every solve above — including the
        // build, whose out-of-band passes cost nothing until the first tick
        // solves and so cannot be read for at the top.
        //
        // This is the half that does not depend on any scene reaching any node,
        // and it is why the comparisons above are no longer the whole guard.
        assert_eq!(
            live.forced_rebuilds(),
            0,
            "`{}` made its solver rebuild after a missed commit: something other than the \
             scene's own solver committed geometry into its arena \
             (docs/decisions/one-solver-per-live-scene.md)",
            scene.name,
        );
    }
}
