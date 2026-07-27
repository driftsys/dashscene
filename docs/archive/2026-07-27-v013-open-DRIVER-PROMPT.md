# Driver prompt — close v0.12, revise the plan, and open v0.13

You are opening **v0.13, the pre-v1 hardening slice**, in
`driftsys/dashscene-staging`. Read `AGENTS.md` first — its conventions override
your defaults. `main` is at `d70eb52`.

**Do not start burning down debt before task 1 and 2 are done.** v0.13 exists to
give accumulated debt a focused pass; running it against an unrevised list is
the failure the phase-end rule exists to prevent, and this backlog has already
grown 51 to 75 while v0.12 ran.

## Read these before touching anything

- `docs/roadmap.md` — the slice map. v0.12's section is closed-shaped but the
  epic is still open; v0.13's is marked provisional.
- `gh issue view 345` — v0.12's epic, with its nine stories and what each
  learned.
- `gh issue view 362` — v0.13's epic, written at the 2026-07-19 triage against
  a backlog that has since changed.
- `docs/decisions/pre-v1-hardening-slice.md` — the dividing line this slice was
  cut on: feature scope gated on a specific v1 consumer stays on v1.

## 1 — Close epic #345, and run the plan revision it triggers

Closing a slice epic is a deliberate act in this repo, not a consequence of the
last story merging. **Confirm with the repository owner before closing it** if
they have not already said to.

All nine stories are merged and #453 was closed as unnecessary. What remains on
the milestone is three inherited debt items (#286, #307, #357) — decide whether
they follow v0.12 or move.

Then run the revision the close triggers: revise the remaining epics and stories
against what v0.12 learned, and record scope-level changes as new or updated
records in `docs/decisions/`. That is the same rule that produced v0.12's own
breakdown.

## 2 — Re-triage v0.13 before opening it

The backlog moved under the epic. **75 open items**, against the 54 epic #362
was written for:

    12 dashscene-core     6 importers        2 dashpaint      1 repo
    11 dashscene-engine   5 dashscene-typeset 2 dashlang       1 oracle
     8 dashcue            5 dashscene-skia    2 goldens        1 docs
                          3 dashc                              1 validator

Three things changed since #362 was written, and the epic body does not know
about any of them:

- **22 strays were re-anchored here** from the closed v0.9 and v0.10
  milestones, as one set rather than three lists.
- **Two items left for v1** — #460 (text residency) and #470 (the pairwise
  closure limit) moved out with the script-coverage scope
  (`docs/roadmap.md`, v1 section).
- **v0.12's own reviews filed the rest**, including #445, #446, #447, #449,
  #452, #455, #457, #458.

## 3 — #462 probably does not belong in a debt sweep

Read it before triaging it. **No memory budget and no target display resolution
exist in `docs/specification/`**, while `dashpack` is designed so that a profile
exceeding the target budget is a validator error rather than a silent quality
cut. That loop has no number in it: **a profile that cannot fail is not a
contract**.

It is a specification gap rather than code debt, and it now gates real content —
one 1920x1080 background is 8.29 MB resident, and a 90-frame welcome sequence is
746 MB uncompressed against 47 MB at ASTC 8x8. It may deserve its own slot
rather than a line in a burn-down.

## 4 — The stream protocol needs rebuilding for this slice

v0.12 ran three streams split by **what a branch owns**: A was epic #345 and the
only stream allowed to regenerate a golden, B was #438 (`dashcue` +
`dashscene-typeset`), C was #439 (`dashscene-engine`). Both B and C are still
open and their items are still on this milestone.

That split was drawn against v0.12's territory. Redraw it against v0.13's, and
keep the rule that produced it: **one stream owns every byte-exact artifact**,
because a regenerated binary golden collides rather than merges and the second
session cannot tell whether its regeneration is correct without re-deriving the
first's reasoning.

`dashscene-core` (12 items) was held back in v0.12 pending the packer's shape.
That shape is now known, so it can be released — but it is also the largest
cluster, and its commit and allocation items touch the seam bank assembly landed
on.

## Things that will bite you

**A green suite is not evidence that nothing moved.** Assert golden integrity
directly with `git hash-object` per file. v0.12 held zero golden movement across
nine stories by checking rather than assuming, and the one time a rename moved a
rendered measurement, the oracle caught it because the number was recorded.

**Review is the throughput bottleneck, not free crates.** Every one of v0.12's
nine stories had a real defect found in review. Three streams is where to stop.

**Never `git reset --soft origin/main` to reshape a branch.** It moves HEAD
without touching the working tree, so if `origin/main` advanced your next commit
silently reverts everything in between — and `just verify` still passes, because
a revert is self-consistent. Before every push, check `git diff --name-only
origin/main HEAD`.

**Six registries enumerate crates.** If a story adds one, all six must follow or
things break silently: `Cargo.toml` (members, publish order, workspace deps),
`.git-std.toml` (**without it, commits scoped to the new crate are rejected**),
the `justfile` `publish` recipe, `AGENTS.md`, `docs/design/architecture.md`,
`docs/technotes/glossary.md`. Story #430 updated one and #448 had to repair it.

## If you dispatch subagents

**Tell them to review inline and not to spawn review subagents.** Four of
v0.12's seven story agents delegated their review and then stalled waiting on
children that had already terminated; one had its worktree mutated underneath it
by its own reviewer, producing a false build failure.

**Tell them to mutation-test.** Break what a check is supposed to catch and
confirm a named test goes red. It found a real defect at every step of v0.12:
six undetectable checks in #437, a vacuous assertion in #431, a
green-when-broken threshold in #433, an overstated doc comment in #434.

Also tell them: squash to one commit, use `Closes #N` rather than `Refs #N` so
GitHub closes the story, and rebase onto `origin/main` before opening the PR
because main moves under them.

## Workflow, non-negotiable

Draft PR → `/code-review` → every finding captured as a checklist in the PR
description → critical fixed, minor filed as one `debt` issue each → ready only
after review → merge with a merge commit (`gh pr merge --merge`, never rebase).
Conventional commits; **the scope is mandatory and validated**. `git commit
--amend` on a clean tree trips a stash trap — amend with `--no-verify`.

**CI cannot run** — every GitHub Actions job fails in 1-4 s with no steps, the
billing block tracked as #263. Local `just build` / `just verify` is the gate.
Verify the 1-4 s no-steps signature rather than assuming it.

Prose everywhere — comments, commit messages, docs, PR bodies — is plain literal
English, no idioms.

## Garden this prompt when its work lands

Archive it verbatim to `docs/archive/`, as the seven now there were, and update
`docs/wip/README.md`'s count. `docs/wip/` currently holds six files, all
accounted for in that README.
