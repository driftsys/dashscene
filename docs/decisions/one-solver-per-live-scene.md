# One solver per live scene: a producer stages, the runtime commits

    status   accepted (story #950, 2026-08-16; extended by #1104 and
             #1118, same day)
    scope    corpus/showcase and the LiveScene contract it demonstrates,
             plus the detector in dashscene-core and dashscene-engine that
             makes the contract checkable; binds #164 (the retained Taffy
             tree), #771 (the staged-variant seam) and #863
             (TaffySolver::owning)

## Context

`TaffySolver` keeps Taffy's tree between solves and patches it from the arena's
layout-dirty set rather than rebuilding it (#164). Story #863 added
`TaffySolver::owning`, which holds its typesetter and so can live inside the
`Box<dyn LayoutSolver>` a `LiveScene` keeps for the life of a scene — the one
place a retained tree could not previously reach, because `TaffySolver` borrows
its typesetter and a `'static` box cannot carry a borrow.

`corpus/showcase` answered the same problem the other way and kept doing so: a
wrapper owning the typesetter, constructing a fresh `TaffySolver` inside every
call. Every solve started with no tree and rebuilt it. Three documents named
that wrapper as the shape `owning` replaced — `TaffySolver::owning`'s doc
comment, the `Text` enum's, and `measure-callback-typesetter-seam.md` — and a
reader following any of them found it still in use, on the demonstration all
three hosts run.

Replacing it is not the mechanical substitution it looks like. It fails `demo`'s
`the_switch_survives_the_ticks_and_pulses_that_follow_it`, and the reason is the
subject of this record.

**A retained tree is correct only while one solver sees every commit into the
arena, in order.** A commit consumes the arena's layout-dirty set; the next
solve through that solver patches the nodes the set named and leaves the rest. A
commit through a _different_ solver takes a dirty set the scene's solver will
never see, and the scene's solver then patches nothing and replays a tree that
no longer describes the scene. `corpus/showcase` had three such commits:
`layout::paint` and `surfaces::paint` at build time, and
`layout::switch_variant` on every press of the variant key. Only the third
stages layout intent — a `Txn::set_variant` marks every node its members
override — so only the third was load-bearing, and it is the one the failing
test caught.

The rebuild-per-solve wrapper was therefore doing **correctness work**, not only
paying a cost. That is the assumption issue #950 was written on and it was
wrong; the issue was re-labelled from `debt` to `story` on 2026-08-15 because of
it.

The failure is quiet in a way worth stating. The commit succeeds, every node has
a rect, and the stale value is not even visible on the ticks that follow the
switch: an incremental solve over a consumed dirty set reports **nothing**, and
a commit told nothing carries the previous rects forward, which are the right
ones. The wrong rect reaches the table only once something else dirties the row
and the readback descends far enough to re-emit the node out of the tree — and
it is repaired by the phase after that. Measured against this defect, the chip's
committed width was wrong after one scripted phase and right again after the
next.

## Options

1. **Stage the switch and let the tick publish it.** `switch_variant` stages
   `Txn::set_variant` and does not commit. `LiveScene::tick` already detects a
   staged switch, re-solves through its own solver and binds the member's
   declared transition (#771) — `switched_variants` exists for exactly this and
   found nothing while the showcase committed first.
2. **Commit through the scene's own solver.** Add a `LiveScene` accessor
   returning `&mut dyn LayoutSolver` and route all three out-of-band commits
   through it, so each scene has exactly one solver and every dirty set arrives
   in order.
3. **Detect the missed commit in the engine.** Have the arena record the
   generation of the last commit that consumed a non-empty layout-dirty set,
   have the solver record the generation its answer landed in, and rebuild — or
   trip a `debug_assert` — when the first has moved past the second.
4. **Leave it.** Keep the wrapper, keep the per-solve rebuild, and close the
   issue as not worth its cost.

## Choice

**Options 1 and 3.** Option 1 was this story's: `showcase::resources::solver`
builds one `TaffySolver::owning` per scene, the wrapper is deleted, and
`layout::switch_variant` stages without committing, so the tick that follows
publishes the switch. Option 3 followed immediately as issues #1104 and #1118,
and is what makes the rule checkable rather than merely stated; it is described
under its own number below.

Option 2 was probed and is correct — 35 of 35 `demo` tests green with no test
changes and no change in behaviour — and it was rejected on what it publishes,
not on what it does. The accessor is a supported way for a producer to commit a
variant switch while bypassing the runtime's variant path, and a switch
committed that way silently loses the transition its member declares. Story #771
built the seam so that a host stages and the runtime animates; an API whose only
use is to go around it is the wrong thing to add, in a crate that is published.

Option 3 is the general form. It was filed as issue #1104 rather than made in
this story, and it **landed immediately after** — so this record's Choice is
option 1 _and_ option 3, and the paragraphs below that describe the gap option 3
closes are kept as the reasoning that produced it rather than as a current
reading.

What it turned out to need, beyond the sketch in the issue: the arena knowing a
layout-consuming commit happened is only half. It cannot say **whose** commit it
was, and a solver guessing "my answer lands in the next generation" is wrong for
a solve that is not inside a commit — `LiveScene::tick` performs one of those on
every variant switch. So `LayoutSolver` gained a defaulted
`committed(generation)` that `Txn::commit_with` calls on the solver it was
handed, and the comparison is between that and
`Arena::layout_commit_generation`. A decorating solver has to forward it,
exactly as it must forward `atlases` and `stage_text` (issue #621); `dashlang`'s
two wrappers do.

Option 4 was rejected on the tree rather than the clock: see **Consequences**.

## Consequences

**The switch is published one tick later, and looks the same.** It is not one
frame slower to appear. `demo` is the only host that reaches the action —
`demo-web` and `demo-android` run the scripted pulse and take no key at all —
and its key handler returns `Reaction::Redraw`, which `dashscene-desktop` turns
into a forced frame, which ticks before it presents. Three `demo` tests read the
committed table straight after the key press and now tick first:
`the_scenes_action_commits_a_real_variant_switch`,
`the_action_is_a_variant_switch_and_not_the_visible_path` and
`the_switch_survives_the_ticks_and_pulses_that_follow_it`. The active member
still moves the moment the key returns, because a staged mutation reads back
immediately (P3); it is the committed geometry that waits.

**Exactly one tick, because this scene declares no transition.** No member of
the chip's set carries a `Txn::set_variant_transition`, and a member with none
starts no track and lands whole. The record this story replaces predicted that
moving to the seam would make the switch "animate through its declared
transition"; that is not what happens, because there is no declared transition
to animate through.

**A declared transition would now animate, and would not have before.** Probed
in both directions with a 0.5 s tween on member 1's width: through the seam the
chip eases 144 to 57.6 over 30 ticks at 60 Hz; with the old out-of-band commit
in place the same declaration snapped in one tick, because the commit had
already landed the after layout and `start_variant_flip` then saw `from == to`
on every track and declined each one. So this is a capability the showcase
gained, one line of scene authoring away, and not only a cost.

**The two build-time commits stay, and are checked rather than proved.**
`layout::paint` and `surfaces::paint` still commit through a throwaway solver:
the nodes do not exist until `build_live` has written them, so their staging
cannot join that commit. Both stage paint intent and arena metadata only — the
variant-set declaration and the image fills — so neither consumes a layout-dirty
set. `corpus/showcase/tests/retained_tree.rs` checks the consequence: it
compares every scene's committed rects against a from-scratch solve at build
time, after every scripted phase and after every variant press. It checks after
**every** phase rather than at the end of a run, because the disagreement is
transient — one phase wide — and a check placed after a batch of them passes
straight over it.

**That test is a check and not a proof, and this record claimed otherwise until
the review measured it.** A stale tree reaches the committed table only once
some later commit's readback descends to the affected node; a second producer
that committed through a full solve of its own published correct rects, so
nothing is wrong to see until something re-emits that node. A `Prop::Width`
write added out of band to `layout::paint` fails the test, and the same write in
`surfaces::paint` passes it, because `layout`'s scripted phases reflow the row
its node sits in and `surfaces`' phases never reach the tile.

**And a scene-level test could not close that gap.** The only lever it has is
marking nodes dirty, and `dashscene-engine`'s incremental path restyles every
dirty node — and its children — from the arena before solving, so dirtying the
tree to force a re-emit repairs it instead of exposing it. That was built on
this branch and removed once mutation showed it changed no outcome.

**Option 3 closed it, and changed what the comparison is for.** Since #1104 a
missed commit no longer produces a wrong picture at all: the solver detects it
and rebuilds, so the published table is correct and only
`LiveScene::forced_rebuilds` moves. Restoring `layout::switch_variant`'s
out-of-band commit now fails the counter test alone, where before #1104 it
failed the rect comparison at three nodes. Both tests are kept and they pin
different things — the counter, that nothing but a scene's own solver commits
geometry into its arena; the comparison, that the published table matches what
the intent solves to, which would catch a defect in the incremental patch or
readback that no counter would notice.

**The counter is a diagnostic and not a panic.** Every integration crate reaches
`TaffySolver` through `boxed`, so an embedder can arrive at a missed commit, and
taking a host down over something the runtime absorbs correctly would be the
wrong trade. `LiveScene` forwards the count read-only: it is the whole of what a
scene exposes about its solver, because handing out the solver itself is what
option 2 above rejects, and a counter cannot be misused that way.

**The saving, which is not the reason.** Measured on the parked attempt
(`debt/v019-showcase-solver`, commit `28c52227`) rather than re-taken here:
1,200 ticks with a layout-affecting write each, `--release`, macOS aarch64,
median tick — `layout` 0.021 ms to 0.006 ms, `typography` 0.037 ms to 0.022 ms,
`surfaces` unchanged at 0.010 ms. Roughly half the tick, and invisible at frame
level against a paint the showcase README measures in milliseconds. The reason
to make the change is that one answer to a problem beats two, and three
documents were pointing at the wrong one.

The `surfaces` row is **unexplained, and the explanation recorded beside it is
wrong.** The parked measurement attributed the flat number to that scene's pulse
driving paint channels; it does not — `sweep` binds `Channel::Width` on the
header and `Channel::X` on the frost panel, and `corpus/showcase/README.md` says
so two sections earlier, where the header's width is named as the thing it
animates. Both channels force a solve, so `surfaces` was paying a rebuild per
tick like the other two and its number should have moved. Left as an open
observation rather than reasoned into an answer: whoever re-takes these numbers
should start there.

**What a scene may now assume.** Nothing but a scene's own solver commits
geometry into its arena. The paragraph in `corpus/showcase/README.md` that
justified the old arrangement — every signal in `layout` drives a
layout-affecting channel, so no tick could replay a stale rect cache over the
switch — was sound about `LiveScene`'s rect cache and one level too shallow: it
did not cover the Taffy tree underneath it. Both are now covered by construction
rather than by that argument.
