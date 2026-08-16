# One solver per live scene: a producer stages, the runtime commits

    status   accepted (story #950, 2026-08-16); extended by issue #1148,
             2026-08-16 — the drain is conditional
    scope    corpus/showcase and the LiveScene contract it demonstrates;
             Txn::commit_with's drain of Arena::layout_dirty and the
             LayoutSolver method that gates it; binds #164 (the retained
             Taffy tree), #771 (the staged-variant seam), #863
             (TaffySolver::owning) and #191 (the contained-write fast path)

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
solve through that solver patches the nodes the set named and leaves the rest.
(**Read this paragraph against the extension at the end of this record**, which
narrows "a commit" to a commit whose solver actually solved — issue #1148. What
follows is unchanged for the second-producer case it is about, and the extension
is where the qualifier is argued.) A commit through a _different_ solver takes a
dirty set the scene's solver will never see, and the scene's solver then patches
nothing and replays a tree that no longer describes the scene. `corpus/showcase`
had three such commits: `layout::paint` and `surfaces::paint` at build time, and
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

**Option 1.** `showcase::resources::solver` builds one `TaffySolver::owning` per
scene, the wrapper is deleted, and `layout::switch_variant` stages without
committing. The tick that follows publishes the switch.

Option 2 was probed and is correct — 35 of 35 `demo` tests green with no test
changes and no change in behaviour — and it was rejected on what it publishes,
not on what it does. The accessor is a supported way for a producer to commit a
variant switch while bypassing the runtime's variant path, and a switch
committed that way silently loses the transition its member declares. Story #771
built the seam so that a host stages and the runtime animates; an API whose only
use is to go around it is the wrong thing to add, in a crate that is published.

Option 3 is the general form and is not this story's. It is sound — the two
generations are enough to distinguish "another producer committed a layout
change" from "a paint-only commit happened", which a naive commit counter is not
— but it is a change to `dashscene-core` and `dashscene-engine` to tolerate a
pattern the architecture already says not to use, and it costs a rebuild when it
fires. Filed as issue #1104 rather than made here, carrying the detector's
design.

Option 3 was then **built and parked** on `story/v1-retained-solver-detector`
(commit `a956ea9c`, PR #1138, closed unmerged and kept as evidence). It is green
and it works; what it is not is what its name says. See the extension below,
which is the answer that made it wait.

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

**And a scene-level test cannot close that gap.** The only lever it has is
marking nodes dirty, and `dashscene-engine`'s incremental path restyles every
dirty node — and its children — from the arena before solving, so dirtying the
tree to force a re-emit repairs it instead of exposing it. That was built on
this branch and removed once mutation showed it changed no outcome. Comparing
the retained tree against the arena directly needs the accessor option 2 above
rejects. Issue #1118 carries the gap, and issue #1104's detector is what would
actually close it — from inside the engine, at the moment the dirty set is
consumed, rather than at whatever later commit happens to re-emit the node.

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

## Extension: the drain is conditional (issue #1148, 2026-08-16)

Everything above is about a **second producer** — a solver other than the
scene's committing geometry into the same arena. Issue #1104 was split out to
detect that, and building the detector measured something the record above did
not know: **the scene's own producer does it too, on its cheapest path, by
design.**

### What was found

`Txn::commit_with` drained `Arena::layout_dirty` unconditionally. That set is
not the commit's input; it is what a retained solver restyles its tree from
(#164), so draining it is a claim that the restyle happened. A commit whose
geometry came from anywhere else still made that claim.

`dashlang`'s contained-write path is exactly such a commit.
`apply_scalar_write`'s `WriteClass::Patch` arm patches one cached rect itself
and commits through the replaying `CachedSolver` without solving (#191, A1) —
while still staging a `Prop::Width`, which `prop_class` calls layout intent.
`Prop::Text` is layout intent too, so a `bind_text` frame does the same. Every
such tick discarded a restyle nobody had performed.

Measured on a three-deep passthrough column, `chip` ancestor-contained and
width-bound, the root X-bound so its write forces a solve:

| tick                               | published `chip` | fresh solve |
| ---------------------------------- | ---------------- | ----------- |
| 1 — contained width write, 40 → 60 | `w = 60`         | `w = 60`    |
| 2 — root moves, real solve runs    | **`w = 40`**     | `w = 60`    |

The root's shifted origin makes the read-back descend the whole subtree and
re-emit `chip` out of a tree whose `chip` style is still the build-time 40.
`crates/dashlang/tests/retained_tree.rs` is that scene, and it fails at tick 2
against the `main` this extension was written on.

The reflow has to be **two** levels above the patched node: `incremental`
restyles every dirty node _and its children_, so a write on the parent repairs
the staleness as a side effect. That is the same mechanism issue #1118 records
for why a scene-level test cannot probe a stale tree by marking nodes dirty.

### Why the detector is not the answer to this

`Arena::layout_commit_generation` plus a `LayoutSolver::committed` callback
answers "a commit drained a layout-dirty set without this solver solving". Read
against the code rather than against the intent, that is a description of
`dashlang`'s fast path, not of a second producer. Measured on the parked branch,
alternating a patch tick with a solve tick: **10 forced rebuilds over 10 pairs**
— a full Taffy rebuild on essentially every reflow following a patch tick, which
reverses #164's saving inside the frame loop.

Each rebuild was _correct_: the tree really was stale. Detect-and-rebuild is the
expensive answer to a condition that should not arise.

### The choice

**`LayoutSolver` gains a defaulted `consumes_layout_dirty(&self) -> bool`, and
`commit_with` drains the set only when it is `true`.**

- `true` is the default, and it is right for every solver that resolves geometry
  from the arena: the internal `FixedSolver` and `TaffySolver`.
- **A decorator forwarding `solve` must forward this too**, and that is not what
  the first draft of this said. It claimed `FlipOverlay` "inherits the correct
  answer by saying nothing", which is true only while the solver it wraps
  happens to consume — and `FlipOverlay` wraps whatever solver an embedder
  handed `attach_live`, including the replaying one this trait's own
  documentation sanctions. Taking the default there drains a set nothing read,
  on the one arm that is supposed to be the real solve. It is the decorator trap
  issue #621 caught on `atlases` and `stage_text`, on a third method, and the
  review on PR #1155 caught it here.
- `false` is for a solver that replays rects the producer resolved for itself.
  `CachedSolver` is the only one in the tree.
- A commit answering `false` leaves the set in place. The nodes stay owed a
  restyle until a solve performs one, and no rect is published from a style
  nothing wrote.

**Both of `CachedSolver`'s construction sites answer `false`, and the
variant-switch arm is not the exception it looks like.** That arm's real solve
runs at step 0 of `LiveScene::tick`, _before_ the transaction opens, so the set
it read is not the set the commit is about to drain — every prop steps 1 to 4
staged was added afterwards. Making that arm report consumption is the
plausible-looking refinement, and it re-opens the defect for exactly those
props: applying it (an `inner_solved` flag, set per construction site) fails
`a_switch_tick_carries_the_writes_staged_after_its_solve` while the other two
tests in that file still pass, so the three discriminate three different
mistakes rather than restating one.

### Alternatives considered

- **A second commit entry point** (`commit_replaying`) rather than a trait
  method. Same effect, no trait change — rejected because the fact belongs to
  the solver, not to the call: an embedder writing its own replay solver would
  have to know to reach for the other method, and picking the wrong one is
  silent.
- **Restyle instead of carrying** — a new `LayoutSolver` method the replay path
  calls, patching the retained tree without computing. It removes the growth
  question, and it puts per-node work (including `set_node_context`, which
  builds a text node's measure input) onto the path that exists to be cheap.
  Rejected: carrying costs one already-paid push and defers the restyle to a
  frame that was going to restyle anyway.
- **Fix it in `dashlang`** — remember the patched nodes and re-dirty them before
  the next real solve. Rejected on the same ground story #950 was re-labelled
  for: it removes the instance and leaves the class. A partial-report solver is
  a pattern `LayoutSolver`'s own documentation invites, and core would still
  drain behind one.

### What it costs, and what stays open

**The set is bounded against what is dirty, not against the document.** Carrying
is what makes unbounded growth possible: a scene doing nothing but contained
writes adds an entry per binding per frame and never drains. So a carrying
commit dedups — first occurrence wins, so the staged order survives — and the
predicate is `should_compact`'s, reused for the same reason and with the same
amortization: a small floor, and twice what the previous dedup left. A set more
than twice its own live content holds at least as many duplicates as distinct
entries, so each dedup at least halves it and its `O(set)` cost amortizes to a
constant per staged write. The frequent case costs one length compare and no
allocation.

**Bounding it by the node count instead was the first answer, and it is the one
the review rejected.** It is sound — a set longer than the document provably
holds duplicates — and it is useless at the size that matters: one animating
node in a 10,000-node scene would accumulate 10,000 copies of itself before
anything reclaimed them, and a reflow landing in that window would build
`incremental`'s set out of all 10,000 to recover the one node that changed. That
is a document-scaled cost per reflow, which is the property issue #164 exists to
keep and the one issue #1111 is separately bounding.
`a_carried_layout_dirty_set_stays_bounded_by_what_is_dirty_not_by_the_document`
is measured on that shape — 1,000 nodes, 4 of them dirty — and the node-count
predicate leaves 1,000 entries in it.

**A carried entry can name a node that was already restyled**, and that is
accepted rather than fixed. The switch arm carries the nodes its own step-0
solve read. Restyling is idempotent and is not itself a Taffy computation, so
the cost is extra `set_style` calls and never an extra solve — bounded, like the
set, by twice the live dirty content rather than by the scene.

**Carrying does not turn `incremental`'s early return into a computation** — not
inside `LiveScene::tick`, where every path that reaches the real solver puts
something in the set or changes the shown root on its own account: a
`WriteClass::Solve` write and a `Visible` flip both push, `Txn::set_variant`
pushes before step 0's solve, and a shown-root change skips that early return by
a test of its own. Outside the tick, an embedder committing through a real
solver after a run of replayed ones does compute where it used to return early —
which is the defect being fixed, not a regression: returning early there left
the tree describing a scene that had moved.

**Issue #1104 stays open, and stays as written.** A genuine second producer runs
a _real_ solver, which reports that it consumed the set and drains it; the
scene's own solver still never sees it. The two are complementary, and the order
matters: once the replay path stops draining, `layout_commit_generation` finally
means what #1104's body says it means, because `dashlang`'s fast path no longer
trips it. The parked branch is the design, not the implementation — its
predicate was measured against a `main` where the replay drained.

**Issue #1118's gap narrows and does not close.** What
`corpus/showcase/tests/retained_tree.rs` cannot see is a stale tree that has not
yet reached the committed table. The staleness this extension removes was one
source of that; a second producer is the other, and it is the one #1118 was
filed against. It stays open behind #1104.
