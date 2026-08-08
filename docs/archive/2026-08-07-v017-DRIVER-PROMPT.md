# v0.17 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message. Read the
             2026-08-08 amendment first — it supersedes the body wherever
             the two disagree.
    written  2026-08-07, before the slice started. Nothing in it is as-built.
             Everything specific below was checked against `main` at 69a76bf,
             and re-confirmed at 91f6615: PR #801 landed in between and touched
             none of the paths cited here. Stale the moment a story lands.
    amended  2026-08-08, twice. First with the four rulings that unblocked the
             slice; then again, after five pull requests landed, with what the
             work found. The amendment is the current brief and the body below
             it is left unedited by design — rewriting it in place would lose
             the record of what was known when it was written, which is the
             only thing it is still good for. Refreshed against `main` at
             010ad51.
    empties  when epic #793 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment its
             work lands, and records nothing a design record should hold.
             Removing it from docs/wip/ and editing docs/wip/README.md are one
             commit, not two.

Drive v0.17 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the merge
method and the five principles, and it is authoritative over anything below.
This prompt adds only what is not in it.

## Amendment, 2026-08-08 — read this instead of the body

**Everything below this section was written before the slice started and is left
unedited**, because a driver prompt is a point-in-time brief and rewriting it in
place would destroy the record of what was known when. Where this amendment and
the body disagree, **this amendment is right**. The body is still worth reading
for the traps it lists, which have not gone stale.

`main` is at `010ad51`. **Three of the milestone's issues are closed and seven
are open** — but take those counts from
`gh issue list --milestone "v0.17 — embedding and integration"` rather than from
here, on the rule the body already gives.

### The four rulings that unblocked the slice

All taken by the owner on 2026-08-08. Each is recorded on its issue, and all
four are carried by epic #793.

- **#803 — desktop gets its own crate, `dashscene-desktop`.** Closed. The two
  hosts do not share a dependency set. **#792 and #794 are therefore parallel.**
- **The shared host policy lives in `dashlang`, on `LiveScene::tick`** — ruled
  with #803, and **already done** (story #810).
- **#795 — the first real published version is 0.2.0.** Every reserved name sits
  at 0.1.0. Nothing is published in this slice.
- **All 17 crates.io names are reserved.** The body flags `dashscene-desktop` as
  the unheld name; checking that found two more, `dashpack` and
  `dashpack-astcenc-sys`, both crates that already ship. All three were reserved
  2026-08-08, so the body's "time-sensitive" section is discharged.

### What has landed since

Five pull requests, in this order. **#798 is fixed and merged**, so the body's
"fix #798 first" section is discharged too.

| PR   | what                                                                | merged    |
| ---- | ------------------------------------------------------------------- | --------- |
| #805 | #798, the stale glyph-quad instance buffer                          | `1751432` |
| #809 | the #803 decision record in `crate-name-map.md`                     | `c398793` |
| #804 | this prompt and its ledger row                                      | `e480f83` |
| #811 | story #810 — the frame clamp and generation gate move to `dashlang` | `b9df1d1` |
| #812 | story #741 — `dashscene-web` becomes the web integration crate      | `010ad51` |

### What is left, in order

1. **#794 and #792, in parallel** — the two-crate ruling is what makes that
   possible.
2. **#795**, which also carries #776's remaining third (the payload number and
   the gate).
3. **#727 and #796.** #796 closes the epic and archives this file verbatim to
   `docs/archive/`.

### Findings the body does not have

**Fixing #795's Defect 2 with one entry makes it worse.** git-std's
`write_version` calls `.captures()`, not `captures_iter`, and splices exactly
one span (`crates/standard-version/src/regex_engine.rs`, doc comment at 71, call
at 85). One `[[version_files]]` entry rewrites **one match per file**, and the
root manifest holds 16 `version = "0.0.0"` occurrences — so a single entry moves
`dashscene`'s and leaves 15 at `^0.0.0`, behind a registry that now looks
covered. Verify any fix with an actual `git std bump --dry-run` and by reading
all 16 requirements, not by inspecting the config.

**The registry audit is done, and exactly one gap remains repo-wide.** Derived
from `ls crates/` rather than from any list: every crate is in `members`,
`[workspace.dependencies]`, `.git-std.toml` `scopes`, the `publish` recipe and
`AGENTS.md`. **Only `[[version_files]]` is short, and only by `dashpack` and
`dashpack-astcenc-sys`.** The full table is on issue #795. Also there: the root
`Cargo.toml` still claims, at **lines 61-63**, that the `publish` recipe does
not list `dashpack-astcenc-sys`. It does, at `justfile` line 194. (Issue #795's
own comment cites that as line 59, which is the wrong line — it lands on the
sentence about `dashpack`.) A comment asserting a gap that has been closed is how the next audit
gets told not to look.

**Adding a dependency can invert the publish order.** Story #741 made
`dashscene-web` depend on `dashscene-gpu`, and the recipe published it first —
correct while it was an empty placeholder, and a failure at the first real
publish. **#794 will do the same thing**: a new `dashscene-desktop` that depends
on `dashscene-gpu` and `dashscene-skia` has to be placed after both. Verify
topologically against all crates from `cargo metadata`, not by looking at the
one crate you changed.

**`just lint` now runs clippy for `wasm32` as well**, and that is new. It ran
for the host target only, so `crates/dashscene-web`'s `host.rs` and
`document.rs` — both gated on `wasm32` — had never been compiled by clippy at
all, and two errors were sitting in them. A published crate whose main logic is
never linted is what the second line prevents. **Any new `cfg`-gated crate needs
its own line**, which `dashscene-desktop` will not (it is a host-target crate),
but which a future mobile crate would.

**What #794 inherits from #741.** The two extractions are **not symmetric**, and
the body says so. `demo/src/present.rs` already has a `Present` trait with two
implementations; `demo-web` had none. But the API lessons transfer:

- The scene seam is a function pointer that already exists —
  `showcase::SceneBuilder` — so the crate declares the same shape and depends on
  `showcase` not at all.
- **The per-frame seam has to be a closure**, not a function pointer, for two
  reasons, and each one causes a real defect. An embedder must remember what it already wrote, or
  `tick` never takes its idle early return and the host never parks. And a hook
  that remembers writes nothing after a rebuild, into a new scene holding none
  of those writes — so `dashscene-web` carries a `FrameKind::{Continuing,
  Rebuilt}` to name that. Desktop rebuilds on resize the same way.
- **The error type cannot move whole.** `demo`'s error will mix integration
  failures with demonstration ones, exactly as `demo-web`'s did. Split it: a
  published crate cannot remove a variant.
- **Six debt issues came out of the slice so far** — #806, #807, #808 from
  #805, and #813, #814, #815 from #812. #813 is a breaking change to a
  `dashscene-web` enum, so it is cheapest **before** anything is published.

### What epic #793's definition of done actually asks for

Worth restating because it is easy to half-satisfy. **`demo-web` consuming the
crate is not the check** — that would pass with two of five pieces moved and
three left inline. The check is a **test or a lint naming the five** that fails
when one is in the wrong place. `demo/tests/integration_surface.rs` is that for
the web, and it fails in **both** directions. #794 needs its desktop equivalent.

Two of the five are now delegated rather than owned: story #810 put the
generation gate on `dashlang::LiveScene`, so an integration crate calls
`advanced`/`mark_shown` and restates nothing. A test naming the five should say
so, or it reads as unmet.

### Process, from three failures that cost real time

- **Commit before mutation testing.** `git checkout -- <path>` restores from the
  **index**, so on a branch with staged renames it silently discards every
  unstaged edit to that file. This destroyed a rewrite twice.
- **Never edit a worktree while a review subagent is working in it.** One agent
  mutation-tested in the same tree and restored with a checkout, taking the
  concurrent edits with it. Tell every review agent explicitly: read-only, copy
  to `/tmp` to experiment, no `git checkout/stash/restore/reset`.
- **`#N` at the start of a line is a Markdown heading.** MD018 fails the lint on
  it, in files and in issue bodies alike. Write "issue #N" or "story #N", which
  AGENTS.md asks for anyway.

### CI

**Still down for billing**, re-confirmed 2026-08-08 against run `31245904215` on
`main` — created that day, first job carrying the annotation verbatim. The body's
section on it applies in full, including that **every `failure` in this state
says nothing about the code**. Merge on local evidence and record the exception
on each pull request.

## Where the slice stands

**Take the open and closed counts from `gh issue list --milestone "v0.17 —
embedding and integration"` and `main`'s commit from `git log`, never from this
file.** The pair went wrong three times in v0.15 and twice in v0.16. What is
below was true at `91f6615` on 2026-08-07, when nine issues were open and none
closed.

| issue                          | state                                        |
| ------------------------------ | -------------------------------------------- |
| #803 desktop: one crate or two | **open — blocks everything.** Owner's ruling |
| #776 payload budget            | **ruled in two parts of three**, stays open  |
| #741 S17.1 web crate           | open, ruled yes, unbuilt                     |
| #792 S17.2 R5 on the web       | open                                         |
| #794 S17.3 desktop surface     | open, blocked on #803                        |
| #795 S17.4 publishable         | open, versioning half ruled 2026-08-07       |
| #727 S17.5 backend guide       | open                                         |
| #796 S17.6 the records         | open                                         |

Order, from the epic:

1. the two open questions — #803 and #776
2. #741
3. #792 and #794
4. #795
5. #727 and #796

**#792 and #794 are only parallel if #803 rules two crates.** If desktop lands
beside the web one, both stories edit `dashscene-web` and must sequence — #792
first, because it changes the load path #794 would wrap. Two stories in two
worktrees on one crate is how a merge conflict becomes a lost edit.

## What has already been ruled — do not re-open these

- **#741 — `dashscene-web` becomes the web integration crate.** Ruled
  2026-08-07. The crate is a registered, empty member, held by story #588 for
  exactly this. `demo-web` keeps the demonstration and consumes the crate.
- **#776 — the payload budget covers the runtime alone, not what a page
  downloads**, and the gate compares **raw bytes** with brotli reported beside
  it. A number written against fetched bytes moves downward when #792 lands,
  reading as an improvement no runtime change produced. The **number** and the
  **gate** are not ruled; #795 carries them.
- **#795 — the workspace versions together, not per crate.** Ruled 2026-08-07.
  What the first real version _is_ remains open.

## Two traps inside those rulings

**A library crate has no measurable size.** Dead-code elimination happens at
link time, and `dashscene-web` is 31 lines with no dependencies and a plain
`lib`. #741 gives the crate _boundary_ that defines "the runtime alone" and
still yields nothing to weigh. Measuring needs a minimal `cdylib` linking
`dashscene-web` and nothing else. Small — the tail of S17.1, or #795.

**The one measured payload number is the demo host and contains the compiler.**
1.37 MB brotli is `demo_web.wasm`, and `demo-web` → `showcase` → `dashc`. An
embedder loading a prebuilt `.dsb` links none of it. The 789 KB `dashc_wasm`
figure is **not subtractable** — it is a differently-linked build. And
`wasm-opt` appears in neither the `justfile` nor CI, so "post-`wasm-opt`" is not
a stage this repository produces: a gate has to name a pipeline stage, not only
an artifact.

## What #795 already owes, found while ruling its versioning half

Both were verified on 2026-08-07 and neither is in the story body. Do not
rediscover them.

- **`.git-std.toml`'s `[[version_files]]` covers 14 of 16 crates.** `dashpack`
  and `dashpack-astcenc-sys` are missing, so `git std bump` cannot move them and
  together-versioning silently breaks at the first bump. Issue #445 named this
  exact item with this exact consequence and **was closed as completed with it
  unfixed** — its sibling item in the same file, `scopes`, _was_ fixed at lines
  46-47.
- **The root `Cargo.toml` is not a `[[version_files]]` entry.** A bump moves the
  crate versions and leaves the 16 `[workspace.dependencies]` requirements at
  `version = "0.0.0"`. Publishing at 0.1.0 would emit crates requiring `^0.0.0`
  of their siblings, which does not match. **Every crate with an internal
  dependency would be broken on first release**, and no local build would notice:
  `path` wins locally and the version requirement is ignored.
- **A floor on the first real version.** `dashscene-gpu` is reserved on
  crates.io at **0.1.0**, and `crate-name-map.md` records the twelve originals at
  the same. Versioning together means the first published version must clear
  every placeholder — so 0.1.0 is not available.

## If #803 rules two crates, one thing is time-sensitive

**`dashscene-desktop` is not among the reserved crates.io names**, and no issue
mentions reserving it — not #803, not #794.
`docs/decisions/crate-name-map.md` records
`dashscene-gpu` being reserved 2026-08-01 as a standalone placeholder 0.1.0 —
`repository` pointing at the public `driftsys/dashscene`, not at this repo —
precisely because "a name can be squatted out from under the project while
nothing is published."

Story #794's eight registries can wait for the story. **The crates.io
reservation cannot**: it is the one part of this that a delay can lose outright.

## The five integration pieces, and where they live today

Epic #793's definition of done is that **none of these five lives in `demo/` or
`demo-web/` any more** — not that the demo consumes the new crate, which would
pass with two moved and three left inline. A test or a lint naming the five is
the deliverable; a reviewer's judgement is not.

| piece                        | web                         | desktop                 |
| ---------------------------- | --------------------------- | ----------------------- |
| surface handoff              | `for_canvas`, async adapter | window handle, blocking |
| the tick loop                | `requestAnimationFrame`     | winit + `ControlFlow`   |
| generation-and-`shown`       | `host.rs:135-136,270`       | `shell.rs:477-479`      |
| resize + `document_replaced` | `host.rs:227-234`           | `shell.rs`              |
| byte-range `.dsb` load       | fetch `Range`               | mapped, `prefix::Plan`  |

## Two duplications you will meet, and the shape behind them

**The generation-and-`shown` contract is written twice**, and
`demo-web/src/host.rs:190` documents its own rule by pointing at the other host:
_"The rule the native host follows (`demo/src/shell.rs`)"_.

**The `dt` clamp is written twice** — `MAX_FRAME_DELTA`, 100 ms in both,
`demo/src/shell.rs:141` and `demo-web/src/host.rs:25` — and the web one again
cites its sibling rather than the record that binds them,
`docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`
(issue #775, re-pointed 2026-08-07).

**That is the same failure mode twice, and it is the argument for this slice.**
Between two `publish = false` demos it is a wart. Between two **published**
integration crates it becomes a semver-bound agreement that nothing checks. So
two crates is not enough on its own: **the shared policy needs a home neither
crate owns**, or the duplication is merely promoted. `LiveScene::tick` in
`dashlang` already owns the generation the gate reads, and is the candidate.
Raise it rather than deciding it inside a story.

**The two extractions are not symmetric.** `demo/src/present.rs` already defines
a `Present` trait — `document_replaced()`, `present() -> Drawn` — with two
implementations. `demo-web` has no such trait and is hardcoded to the GPU
painter. Desktop has an abstraction web lacks, so #794 is not #741 again with a
different `cfg`.

## Fix #798 before #792 and #794

`dashscene-gpu`'s instance patch path writes a stale buffer on the typography
showcase scene. `t1-correctness`, unmilestoned, found by running the host rather
than by a test.

    cargo build -p demo && ./target/debug/demo typography --painter gpu

Panics about two seconds in. A `debug_assert!` catches it; **a release build
draws the wrong quad with nothing reporting it.**

It is on this slice's own subject: the generation-and-`shown` contract is one of
the five pieces moving, and `demo/src/present.rs` hands the GPU painter
`Some(Changes { rects: scene.dirty(), .. })` — so the code that would move is
the code that is currently wrong. Fixing it after the move splits its history
across two locations.

## Debt: nothing is scheduled here, and that is deliberate

No `debt` issue carries the v0.17 milestone. A full re-triage of all 78 open
debt issues is designed and pending, and its standing instruction is that **it
anchors nothing into v0.17** — this slice is already planned and scoped, and
adding to it mid-flight is the drift, not the fix.

**That does not exempt you from the merge check.** `AGENTS.md` requires
re-reading the milestone's _open_ issues before merging, not only the story's
own, because a slice's other sessions file against the work in flight
(#783 was filed twelve minutes after story #597's PR opened and twenty-six
before it merged).

## CI IS DOWN — READ THIS BEFORE ANYTHING ELSE

**Down again, for billing.** Confirmed 2026-08-07 against run `31199053993` on
`main`: `changes`, `dprint` and `fmt` fail with **zero steps** and every other
job is skipped behind them. The logs 404. The reason lives on one endpoint and
nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns, verbatim: _"The job was not started because recent account payments
have failed or your spending limit needs to be increased."_

**Query annotations before diagnosing anything else.** "This check has no steps"
is the UI's wording for a job that was never scheduled, not a config fault, and
the workflow file is valid. **Every `failure` in this state says nothing about
the code** — do not read the run history as evidence about a branch.

This has now happened twice. If it is fixed by the time you read this, re-run a
workflow on `main` and confirm `exit-gate` and `ci` go green before trusting any
run; the standing rule then returns to force. While it lasts, merge on local
evidence — `just build`, plus `just calibrate` when the diff touches the
`packer` filter — and **record the exception on each PR** rather than merging
silently.

## The loop, per story

1. Read the story issue and **every comment on it**. Three rulings in this slice
   live only in comments — #776's, #795's versioning half, and #775's re-point.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. **Check the scope against the code before writing any of it.** Every story in
   v0.16 was smaller or differently shaped than its body said.
4. Implement.
5. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it.** Note that `just lint` now also gates intra-doc links
   (`justfile:109`), which clippy does not resolve.
6. Open the PR **ready, never a draft**. Name the tiers you actually ran, and
   record the CI exception.
7. **Run `/code-review` and mean it** — the fan-out, not an author pass.
8. Capture **every** finding as a checklist in the PR description. Fix criticals
   inline; file one `debt` issue per minor finding.
9. Before merging, re-read the milestone's open issues. Then merge with
   `gh pr merge --merge`, delete the branch, remove the worktree, comment the
   outcome on the story, update memory.

## What this repo has already paid for — the traps most likely here

The general ones are in `AGENTS.md` and in the v0.15 and v0.16 prompts in
`docs/archive/`. These are the ones this slice's subject invites.

- **An issue's negative claim needs the same check as a positive one.** #775
  said "the only thing connecting them is a comment"; the decision record
  binding every host had been accepted **seven days before the issue was
  filed**. Wrong when filed, not stale. Debt issues assert absences, and an
  absence names nothing to go read. Compare the record's date against the
  issue's `createdAt`.
- **A claim can be true of the function it names and still name the wrong
  function.** A review agent told to verify every claim passed them all, because
  each was true of the thing it named. Ask whether the named thing is still on
  the path.
- **An issue closed as completed may have an item unfixed.** #445 enumerated
  seven registries, fixed one half of `.git-std.toml` and not the other, and
  closed. Re-derive a checklist rather than trusting its state.
- **A green `ci` does not mean the suite ran**, and in this billing state a red
  one does not mean it failed. Read the individual jobs.
- **Verifying against a design or specification record is not verifying.** Four
  of this repo's records have drifted from the code. Check the code.
- **Assert the drawn output, not the document.** Arena calls are intent; the
  painter reads `committed()`. Two tests once passed while the feature rendered
  nothing.

## Stop and ask, rather than deciding alone

This went three for three in v0.15 and three for three in v0.16.

- **#803 is unanswered.** It is the whole gate on S17.1, and its first half —
  one crate or two — is all that blocks. The name, if two, can settle inside
  #794.
- **The first real published version** is owner input, and #795 needs it.
- **Where the shared host policy lives**, if #803 rules two crates. Deciding
  that inside a story promotes a duplication into a published API.
- **A golden moves.** That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass. This slice's definition of done says
  **zero goldens moved**, and nothing here should change rendered output.

## Definition of done, from the epic

- **An embedder can draw a `.dsb` in a browser without copying code out of
  `demo-web`** — checked by none of the five pieces remaining there, not by
  `demo-web` consuming the crate.
- **The same for desktop**, in whatever shape #803 settles, with the same check
  rather than an assurance. Whatever an embedder must still write for itself is
  **named**, in a doc comment or a record.
- **R5 holds on the web target**, measured the way epic #594 measured it on
  native, and **demonstrated failing first** (#792).
- **Nothing is published.** This slice makes the crates publishable; the publish
  itself is a separate decision.
- **Zero goldens moved.**
