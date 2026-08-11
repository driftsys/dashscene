# v0.18 driver prompt, second half — from #617 onward

    status   live; hand this to a session as its first message. It supersedes
             `2026-08-08-v018-DRIVER-PROMPT.md`, which is kept and marked
             rather than deleted — its gate section records why the slice could
             start, and its story #770 sections are now as-built.
    written  2026-08-09, after three stories closed; **revised the same day**
             once the owner ruled on the two plan questions it opened (see
             "Where the slice stands"). Everything specific below was checked
             against `main` at 27fabf8. Stale the moment a story lands.
    empties  when epic #769 closes. Archive both prompts verbatim to
             docs/archive/ rather than gardening them — a driver prompt is
             spent the moment its work lands, and records nothing a design
             record should hold. Removing them from docs/wip/ and editing
             docs/wip/README.md are one commit, not two.

Read `AGENTS.md` first. It holds the story workflow, the test tiers, the merge
method and the five principles, and it is authoritative over anything below.

## Where the slice stands

**Take the counts from `gh issue list --milestone "v0.18 — animation
vocabulary"` and `main`'s commit from `git log`, never from this file.** The
pair went wrong three times in v0.15 and twice in v0.16.

At 27fabf8 the milestone holds **six open and three closed**, after the owner
ruled on both plan questions this prompt opened.

| issue | state                                                                  |
| ----- | ---------------------------------------------------------------------- |
| #769  | the epic                                                               |
| #770  | **closed** — the rotation vocabulary, lowering and reference painter   |
| #832  | **closed** — the lean painter draws rotation                           |
| #852  | **closed** — a step is a pair of keyframes                             |
| #617  | **milestoned in**, and is story #771's first half                      |
| #771  | variant transitions serialize — **build this next**                    |
| #772  | loop tracks; unblocked, carries two design questions of its own        |
| #773  | Figma prototype reactions; **needs a fixture only a human can author** |
| #845  | debt — a rotation does not compose down the tree                       |

**#774 and #848 left the milestone** on 2026-08-09 and are deliberately
unmilestoned: they are the seed of the SVG track, and the capture's proposal to
place it at v0.21 is an input to the phase-end revision rather than a decision.
Slice numbering is settled at that revision, so they are "not v0.18" rather
than "v0.21". Comments on both record it, and the answered configuration
questions travel with them.

**Story #771 is no longer what to build next — it is in review.** Pull
request #865 builds all three of its parts and closes issue #617 by name; the epic's
"a document loaded from a file animates" line is met and proved by a test. It
is one open review finding away from mergeable, and
`2026-08-09-FINISH-771-DRIVER-PROMPT.md` carries exactly that remainder. Hand
a session **that** prompt, not this one, until #865 merges.

This file stays the guide for the rest of the slice — the rulings above, the
traps below, and stories #772 and #773. It is revised properly when #865
lands, because that is when #771's own traps are final and #772 becomes next.

## What is already ruled — do not re-derive these

- **Rotation is paint-only, with an explicit anchor**
  (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`). The
  angle is radians, **y-down and clockwise-positive**, and Figma's `rotation`
  lowers with no sign conversion at all. The anchor is a point in the node's
  own space, canonically `(0, 0)`.
- **A rotation does not compose down the tree.** `Prop::Rotation` is per-node
  paint intent: the commit walk resolves every box absolutely and a clip region
  is axis-aligned, so nothing carries a parent's turn onto a descendant. A
  rotated Figma node **with children** is refused by name. Issue #845 holds
  whether the document should gain a composing transform; it is a real
  question, not a bug.
- **A step is a pair of keyframes**
  (`docs/decisions/a-step-is-a-pair-of-keyframes.md`). `Keyframe.t` is
  non-decreasing, two frames sharing a `t` are a step, and a third is a named
  producer error. `TransitionSpec` keeps its three arms — **there is no fourth
  variant for #771 to serialize**.
- **A discrete change of a bool prop is unreachable, and that is decided.**
  Every animatable channel is scalar; `Prop::Visible` is reachable only as an
  instantaneous `VariantVisible` override. SVG's `visibility` maps onto a step
  on `Opacity`; `display` needs a **channel**, which no curve vocabulary
  supplies. Recorded in the step decision's "What this does not close".
- **The SVG dependency questions are answered** (three comments on #774,
  checked against crates.io on 2026-08-09). `usvg`/`resvg`/`kurbo` are
  **Apache-2.0 OR MIT**, not MPL-2.0 — the Slint licence concern does not
  apply. `usvg` with `default-features = false` drops nine dependencies in one
  line, including `fontdb`, which is what keeps the wasm32 build small and
  routes `<text>` to `dashscene-typeset`. The stroker is
  `usvg::tiny_skia_path::PathStroker`, re-exported by `usvg` itself, so
  **issue #848 adds no dependency #774 was not already taking**.
- **v0.20 and v0.21 do not exist.** The capture proposes v0.20 for Unity and
  v0.21 for the SVG track, and says plainly that slice numbering is settled at
  a phase-end revision. Treat it as an input to that revision, not a decision.

## Issue #617 is story #771's first half — ruled 2026-08-09

**#617 is milestoned into v0.18 and folded into #771**, which now builds in
three parts. It is **left open**, not closed as "covered": an issue closed
while one of its items is unbuilt is how issue #143 lost the rotation channel,
which is the whole reason story #770 existed. It closes on #771's own pull
request, by name, when the emitter and the fixture land.

What #617 says: all ten committed `goldens/dsb` fixtures report zero signals,
zero bindings and zero variant sets, so a loaded document drives `attach_live`,
seeds one commit, and then has nothing left to drive. It is not a loader
defect — `dashc`'s Figma path resolves an `INSTANCE` to its one active subtree
at compile time, so a static REST export has no switchable set to preserve.

**What #617 does not say, and what changes its size** — found while building
story #770 and verified again at cf52ca3:

    grep -n "VariantVisible\|variant_sets" crates/dashc/src/emit.rs

returns nothing. **`dashc` has no variant-table emitter at all.** The variant
table exists in the schema and in `dashscene-core`'s loader, exercised only by
hand-built flatbuffers in tests (`crates/dashscene-core/tests/load.rs` is the
pattern). So #617 is not "author a fixture" — it is "teach the emitter to write
variant sets, then author a fixture", and that emitter is the same one that
story #771 needs for its motion rows.

So story #771 builds in this order:

1. **`dashc` emits variant sets** — the table, its members, their overrides,
   from an authored `Document`.
2. **A committed `.dsb` under `goldens/dsb/` carries one**, so a loaded
   document exercises commit and FLIP end to end. This is #617's deliverable
   and the epic's "a document loaded from a file animates" turns on it.
   Authored, never imported.
3. **The motion rows** #771 was filed for.

## Story #771's other blockers, both checked on 2026-08-09

- **#255 — open, in the v1 milestone.** Smoothing specs do not serialize;
  document bindings are always direct writes. #771's decision has to cover it,
  so a v1 issue follows a choice taken here. Raise the sequencing rather than
  meeting it at the acceptance criteria.
- **#626** — `dashlang`'s `smooth` accepts only a `Spring`, so tween and
  keyframe specs are unreachable from an authored scene. That is also how a
  step would be authored, so it touches the #852 ruling directly.

## Stop and ask, rather than deciding alone

All three questions raised this way in v0.15 needed the owner's answer, and so
did all three in v0.16. Six were raised in this slice and all six were answered.
**Two were answered on 2026-08-09** and are now recorded above rather than
open: issue #617 folds into #771, and the SVG track leaves the milestone.

These remain open, and both belong **inside** their story — research them and
come back with evidence, the way stories #832 and #852 did, rather than opening
with them:

- **Where does a loop track's phase come from, and what ends one?** (#772.)
  Document load and node-visible are different features.
- **What binds a `VariantTransition` to a switch** — per variant set, per
  variant, or per interaction? Figma's model is per interaction, which is the
  level its `reactions` payload is keyed at. (#771, #773.)

## CI IS DOWN FOR BILLING — READ THIS BEFORE DIAGNOSING ANYTHING

Still down at cf52ca3, and the owner has confirmed it will **not** be fixed.
`changes`, `dprint` and `fmt` fail with **zero steps** and every other job
skips behind them. The reason lives on one endpoint and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns "The job was not started because recent account payments have failed
or your spending limit needs to be increased." **Every `failure` in this state
says nothing about the code.** Red CI is neither information nor a blocker.
Merge on local evidence — `just build`, plus `just calibrate` when the diff
touches any path in the `packer` filter — and **record the exception on each
pull request** rather than merging silently.

## The loop, per story

1. Read the story issue and **every comment on it**. Rulings in this repository
   routinely live only in comments — #774's three dependency answers are there
   and nowhere else.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. **Check the scope against the code before writing any of it.** Every story
   in v0.16 was smaller or differently shaped than its body said, and all three
   in this slice's first half were too: #770 was half-built already, #832's
   central trade-off did not exist, and #852's stated cost was not real.
4. Implement.
5. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it.** `just lint` also gates intra-doc links, which clippy does not
   resolve.
6. Open the pull request **ready, never a draft**. Name the tiers actually run,
   and record the CI exception.
7. **Run `/code-review` and mean it** — the fan-out, not an author pass.
8. Capture **every** finding as a checklist in the pull request description.
   Fix criticals inline; file one `debt` issue per minor finding.
9. Before merging, re-read the milestone's open issues. Then merge with
   `gh pr merge --merge`, delete the branch, remove the worktree, comment the
   outcome on the story, update memory.

## The traps this slice's first half actually hit

Not a general list — these each cost real time between 2026-08-08 and
2026-08-09.

- **The fan-out finds what an inline pass does not.** Across three pull
  requests it returned sixteen findings; the author pass had found **none** of
  them. Two were reachable silent-wrong-picture bugs: a clip that turned with
  the node, and a backdrop-blur pipeline that ignored rotation entirely while
  the capability declared support.
- **`paint.wgsl` is not the whole painter.** `KIND_BACKDROP` appears there only
  as a constant; the blur draws from `blur.wgsl` through its own uniform, and
  `composite.rs` is a third path. After wiring a per-node property, `grep -c`
  it in all three and ask whether zero is right.
- **Commit before mutation testing.** `git checkout -- <path>` restores from
  the index and silently discards unstaged edits. It destroyed a finished
  shader fix on 2026-08-09, one file, recovered by hand. This is the third
  time it has bitten.
- **`git push` hangs forever on the credential prompt.** Use
  `git -c credential.helper='!gh auth git-credential' push`. A hung push may
  still have succeeded — check `git rev-parse origin/<branch>` before retrying,
  because `--force-with-lease` will then refuse for the right reason and read
  like a failure.
- **`rustfmt` joins a line continuation inside a string literal into literal
  spaces.** A panic message shipped with twenty-six of them; neither
  `cargo fmt --check` nor clippy sees it, and a `should_panic` test
  substring-matches the start. Read the assembled message, not the source.
- **WGSL aligns `vec2f` to eight bytes and Rust aligns it to four.** A row
  starting with a `vec4f` also has its array stride rounded to a multiple of
  16, so a member added anywhere costs the same 16 bytes. Assert
  `offset % 8 == 0` and pin every offset, not only the size.
- **Verify a limit against the pinned crate.** The field is
  `max_inter_stage_shader_variables` (15 in `downlevel_defaults`, counted in
  locations) and `max_storage_buffers_per_shader_stage` (4). Both were needed
  and both were nearly written from memory.
- **A golden that passes can still be stale.** Sweep the whole package with
  `UPDATE_GOLDENS=1` and check `git status`; never `UPDATE_GOLDENS=1` to make a
  failing test pass.

## A concurrent session works this repository

It filed #848 and #852 and landed the SVG capture while this slice's first half
was in flight. **Re-read the milestone before pressing merge, not only at the
start** — #852 was filed and ruled inside a single day. Check
`git config --get remote.origin.url` before any fetch, reset or push.

## Definition of done, from the epic

- **Motion is data in the document**: a `.dsb` carries a transition, and a
  document loaded from a file animates without Rust written against `dashlang`.
  **This is story #771's, in all three of its parts**, and it is the criterion
  the slice cannot close without.
- **A node can rotate**, in the document, through both painters, proved by a
  golden that a mutation to the rotation term fails. **Met** (#770, #832).
- **Whether rotation perturbs layout is recorded** in `docs/decisions/`, and so
  is the decision issue #255 names, covering both the binding and the variant
  side. **Half met**: rotation is recorded; #255's is #771's.
- **The append stayed an append**: the frozen `tests/fixtures/v0_5_document.dsb`
  still round-trips (R7). **Met** and asserted.
- **Zero goldens moved** except the ones a new rotation fixture adds. **Met**,
  swept twice.
