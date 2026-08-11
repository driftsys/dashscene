# v0.18 driver prompt — drive the slice to completion, one story at a time

    status   SUPERSEDED on 2026-08-09 by
             `2026-08-09-v018-DRIVER-PROMPT.md`, which covers the slice from
             issue #617 onward. Hand a session that one, not this. This is kept
             as written and marked rather than deleted, the same way its own
             spent gate section is: its story #770 material is now as-built,
             and its gate records why the slice could start at all.
             Both archive together when epic #769 closes.
    written  2026-08-08, before the slice started. Nothing in it is as-built.
             Everything specific below was checked against `main` at e5b6846,
             the merge of story #727. Stale the moment a story lands, and one
             section was stale within the hour: v0.17 closed the same evening.
    empties  when epic #769 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment its
             work lands, and records nothing a design record should hold.
             Removing it from docs/wip/ and editing docs/wip/README.md are one
             commit, not two.

Drive v0.18 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the merge
method and the five principles, and it is authoritative over anything below.
This prompt adds only what is not in it.

## The gate — discharged on 2026-08-08, hours after this was written

**This section is already spent. It is kept because the reasoning still says
why the slice may start, and deleting it would hide how narrow the margin was.**
Story #796 closed at 21:42 and epic #793 at 21:43 on 2026-08-08, roughly a
minute after the pull request carrying this file was opened. **v0.17 is closed
and v0.18 may start.** Confirm that from `gh issue view 793` rather than from
this paragraph.

**v0.18 has no technical dependency on v0.17.** The roadmap says so directly:
this slice touches `dashbuf`, `dashscene-core`, `dashcue`, `dashscene-engine`
and both painters, and none of that is on v0.17's packaging path or v0.16's
loading path. No file in it collided with what v0.17 was editing.

The condition was procedural. `AGENTS.md` requires the phase-end revision
before the next slice starts, and **story #796 was that revision** — it closed
v0.17 in `docs/roadmap.md` and confirmed v0.19's shape.

**What this slice still owes before a story starts is its own planning
session.** The phase-end revision that closed v0.17 is not the same act as
revising v0.18's breakdown against it, and `AGENTS.md` asks for both: _"revise
the remaining epics and stories against what was learned before starting the
next slice"_. The milestone is still named "(provisional)" and the epic body
still says its breakdown is. That is the first task, not story #770.

**Half of the epic's own standing block is already stale.** Epic #769 opens by
saying its number and placement are unsettled and that nothing should start
ahead of the owed v0.15/v0.16 revision. That revision happened on 2026-08-07
and **confirmed both**: the packaging half stayed v0.17, the mobile half became
v0.19, and v0.18 kept its number and its position
(`docs/roadmap.md`, the v0.18 entry). What is still provisional is the **story
breakdown**, as every epic's is before its own planning session. Whoever opens
the slice should edit the epic body to say that, rather than leaving a block in
place that reads as broader than it is.

## Where the slice stands

**Take the open and closed counts from
`gh issue list --milestone "v0.18 — animation vocabulary (provisional)"` and
`main`'s commit from `git log`, never from this file.** The pair went wrong
three times in v0.15 and twice in v0.16.

At e5b6846 the milestone holds six open issues and none closed.

| issue | state                                                      |
| ----- | ---------------------------------------------------------- |
| #769  | the epic                                                   |
| #770  | rotation channel — **the lead**                            |
| #771  | variant transitions serialize; blocked on #617             |
| #772  | loop tracks; depends on #770                               |
| #773  | read Figma's prototype reactions; **has no fixture** today |
| #774  | static SVG import — in the milestone, outside the epic     |

Order, from the epic: **#770 first**, then #771 and #773 (whose capture-and-pin
half can start ahead of #771), then #772, which needs #770 for its canonical
case. Story #774's placement is a revision decision and is deliberately outside
the epic — do not fold it in without one.

## What is already ruled — do not re-derive these

- **The Lottie triage is settled, and ThorVG is chosen.**
  `docs/technotes/runtime-content.md` §4-§6 fixed the three-bucket split
  (spec / sprite-sheet / runtime vector) and chose ThorVG — about 150 KB, MIT,
  native Lottie. **That note never mentions Vello**; the comparison against
  Vello is in `docs/wip/2026-08-07-animated-content-import.md`, which reaffirms
  ThorVG for this role. Read both before reopening the choice, and cite the
  right one. The three captures dated 2026-08-07 in `docs/wip/` **extend** the
  note rather than replacing it. Re-deriving the design space from scratch is
  the most expensive mistake available here.
  One note against it: its advice to run ThorVG's GL backend on the painter's
  context predates the wgpu painter landing at v0.15.
- **Binding expressions as embedded wasm are rejected**, on P1, P4, P3, payload
  size, determinism and security, with the reasoning recorded in
  `docs/wip/2026-08-07-motion-in-the-document.md`. The counter-proposal, if
  more computed power is wanted, is to widen the declarative transform union —
  `Scale | MapRange | Clamp` today — rather than to embed a virtual machine.
- **`dashbuf` must not depend on `dashcue`.** Confirmed at e5b6846: `dashcue`
  is a dependency of `dashscene-engine` and `dashlang`, and a dev-dependency of
  `goldens/tooling`. `dashbuf` is not among them, and §9's direction is that
  consumers depend on `dashcue` and never the reverse. The schema mirrors the
  vocabulary as tables and the loader constructs `dashcue` types from them.
- **`PropKey` cannot be stored.** It is opaque and caller-encoded; the packing
  math is `dashscene_core::prop_key`. The document stores `(node, channel)` and
  the loader packs it, which is what `Binding` already does.

## What checking the issues against the code found

All checked on 2026-08-08 at e5b6846. None of it is in the issue bodies.

### `Prop` has 37 variants, not 34 — and the obvious command says 34

Epic #769's gap table and story #770's table both say **34**. The count is
**37**. `docs/wip/2026-08-07-motion-in-the-document.md` and `docs/roadmap.md`
say 37 and are right.

The three the count loses are `Corners`, `Padding` and `Margin` — the only
struct-like variants — because their brace is preceded by a space, so a pattern
anchoring the delimiter directly to the variant name skips them and returns 34
without reporting a gap. This is the same failure `docs/wip/README.md` already
records against these captures: a variant count derived twice with the same
flawed command, where the repeat read as confirmation.

Derive it over the enum's line range and inspect the names, not only the total:

    awk 'NR>311 && NR<453 && /^    [A-Z]/' crates/dashscene-core/src/arena.rs

**Correct the two issue bodies rather than carrying the number forward.**

### Story #770 is not a vocabulary-only change

Both the capture and the issue say the gap is "vocabulary, not rendering" and
that the painter "can very likely rotate today". **Neither painter has a
rotation term.** There is no `rotat`, `cos(`, `sin(` or `mat2x2` in
`crates/dashscene-gpu/src/shaders/sdf.wgsl` or `paint.wgsl`, and
`dashscene-skia`'s only matrix concatenation is at `lib.rs:1841` — the
`ScaleMode::Crop` image-fill local matrix, which is the same `Mat23` the
capture already identified as an image crop rather than a node transform.

So the story's third acceptance criterion — both painters rotate, with the
shared SDF math single-sourced per `R-T5` — is real work in two painters and
in the shared layer, on top of the four-place vocabulary append. Scope it that
way from the start. `docs/design/dashscene-gpu.md` describes what R-T5
single-sourcing means in practice.

### Story #773 has no fixture, and its premise is not observable here

The issue says the importer "already fetches and discards" Figma's `reactions`.
**No code and no fixture in this repository mentions `reactions`** — not in
`importers/figma/`, not in any Rust source, not in any captured fixture. The
only occurrences are prose: `docs/roadmap.md`, and two of the 2026-08-07
captures. Check it with `git grep -i reactions`, which returns four lines and
none of them under `importers/` or `corpus/`.

What the captures do carry is `prototypeStartNodeID: null` and
`"interactions": []`, in 30 of the 32 fixtures, and **no `interactions` array
anywhere is non-empty**. Nothing strips the field: `importers/figma/src/trim.ts`
removes whole subtrees by annotator role and name prefix, not by key allowlist.
So the claim is true in principle — a reaction on a surviving node would reach
the closure — and there has never been one to discard.

**The story's first task is therefore to author a Figma file with a prototype
interaction and capture it**, which needs the PAT and
`importers/figma/plugins/fixture-author/`, and is not named in the story body.
Until that fixture exists, the capture-and-pin half has nothing to pin.

### Story #771 depends on two milestones and one unscheduled issue

Its stated dependency, issue #617 — no committed `.dsb` fixture carries a
variant table — is **open and unmilestoned**. Its decision has to cover
issue #255, which is **open in the v1 milestone**. So before this story can be tested
end to end, one unscheduled issue has to be built and one v1 issue has to
follow a decision taken here. Raise the sequencing rather than discovering it
at the acceptance criteria. Related: issue #626, `dashlang`'s `smooth` accepts
only a `Spring`, so tween and keyframe specs are unreachable from an authored
scene.

### The two append points, confirmed

`BindingChannel` holds exactly ten arms (`dashbuf.fbs:690-702`): `X, Y, Width,
Height, Gap, FillR, FillG, FillB, FillA, Opacity`. `VariantPropValue` holds
exactly six (`dashbuf.fbs:631-639`) and carries its own comment saying arms are
appended at the tail so existing discriminants are kept (R7). Both are the
append-only shape the stories assume. The frozen
`tests/fixtures/v0_5_document.dsb` round-trip is what proves an append stayed
one.

## CI IS DOWN — READ THIS BEFORE ANYTHING ELSE

**Down for billing, still.** Re-confirmed 2026-08-08 against run
`31274098879` on `main`: `changes`, `dprint` and `fmt` fail with **zero steps**
and every other job is skipped behind them. The reason lives on one endpoint
and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns, verbatim: _"The job was not started because recent account payments
have failed or your spending limit needs to be increased."_

**Query annotations before diagnosing anything else.** "This check has no steps"
is the UI's wording for a job that was never scheduled, not a config fault, and
the workflow file is valid. **Every `failure` in this state says nothing about
the code.** While it lasts, merge on local evidence — `just build`, plus
`just calibrate` when the diff touches the `packer` filter — and **record the
exception on each pull request** rather than merging silently. If it is fixed by
the time you read this, re-run a workflow on `main` and confirm `exit-gate` and
`ci` go green before trusting any run.

## The loop, per story

1. Read the story issue and **every comment on it**. Rulings in this repository
   routinely live only in comments.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. **Check the scope against the code before writing any of it.** Every story
   in v0.16 was smaller or differently shaped than its body said, and three of
   the five findings above are this slice's version of the same thing.
4. Implement.
5. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it.** `just lint` also gates intra-doc links, which clippy does not
   resolve.
6. Open the pull request **ready, never a draft**. Name the tiers you actually
   ran, and record the CI exception.
7. **Run `/code-review` and mean it** — the fan-out, not an author pass.
8. Capture **every** finding as a checklist in the pull request description. Fix
   criticals inline; file one `debt` issue per minor finding.
9. Before merging, re-read the milestone's open issues. Then merge with
   `gh pr merge --merge`, delete the branch, remove the worktree, comment the
   outcome on the story, update memory.

## The traps this slice's subject invites

The general ones are in `AGENTS.md` and in the v0.16 and v0.17 prompts in
`docs/archive/`. These are the ones a vocabulary slice is most likely to hit.

- **Assert the drawn output, not the document.** Arena calls are intent; the
  painter reads `committed()`. Two tests once passed while the feature rendered
  nothing. Story #770's own acceptance criteria say a rotation of zero must not
  be what makes the golden pass — that is this trap, named.
- **A green mutation means the fixture is wrong, not that the code is right.**
  A rotation applied to a rotationally symmetric shape changes no pixel. Choose
  the fixture so the mutation has somewhere to show.
- **Commit before mutation testing.** `git checkout -- <path>` restores from the
  index and silently discards unstaged edits. This has destroyed work twice.
- **A claim can be true of the thing it names and still name the wrong thing.**
  Ask whether the named function is still on the path.
- **An issue closed as completed may have an item unfixed.** That is exactly why
  rotation is untracked: issue #143 carried four items, landed three, and closed.
- **A golden moves.** That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass.

## Stop and ask, rather than deciding alone

All three questions raised this way in v0.15 needed the owner's answer, and so
did all three in v0.16. The capture's
open questions are owner input, not story-internal choices:

- **Which shape does the rotation channel take** — a bare `f32` angle, or an
  angle with an anchor point. Figma, SVG's `rotate(a cx cy)` and Lottie all
  carry an anchor.
- **Does rotation imply scale and skew?** They are absent for the same reason
  and are reached for by the same importers. Adding one channel three times is
  worse than adding three once; a full 2×3 transform on every node is larger
  than any single importer needs.
- **Does rotation perturb layout, or is it paint-only?** Story #770 requires
  this to be recorded in `docs/decisions/` rather than left implicit. The
  capture's recommendation is paint-only, as `Opacity` already is.
- **What binds a `VariantTransition` to a switch** — per variant set, per
  variant, or per interaction. Figma's model is per interaction, which is the
  level its `reactions` payload is keyed at.
- **Where a loop track's phase comes from**, and what ends one. Document load
  and node-visible are different features.
- **Whether story #774 belongs in this slice at all.** It is in the milestone
  and outside the epic, which is a state that should not survive the slice's
  planning session in either direction.

## Definition of done, from the epic

- **Motion is data in the document**: a `.dsb` carries a transition, and a
  document loaded from a file animates without Rust written against `dashlang`.
- **A node can rotate**, in the document, through both painters, proved by a
  golden that a mutation to the rotation term fails.
- **Whether rotation perturbs layout is recorded** in `docs/decisions/`, and so
  is the decision issue #255 names, covering both the binding and the variant
  side.
- **The append stayed an append**: the frozen `tests/fixtures/v0_5_document.dsb`
  still round-trips (R7).
- **Zero goldens moved** except the ones a new rotation fixture adds.
