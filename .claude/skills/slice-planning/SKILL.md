---
name: slice-planning
description: Use when planning a slice, opening or splitting epics and stories, placing an issue on a milestone, or running the phase-end plan revision in this repository — epic shape and the three reasons to split, which milestones are exceptions, the three unanchored-work sweeps, the rolling-debt population read, and the docs/features.md re-check.
---

# Slice planning


The v0 plan lives as GitHub issues on this repo: one milestone per
`docs/roadmap.md` slice, with an `epic`-labeled issue under it, broken into
`story`-labeled issues.

**One epic is the usual shape, not a rule.** Two reasons to split have
precedent, and they are different reasons:

- **By artifact territory**, so two sessions cannot regenerate the same golden.
  This is v0.13's three streams (#438, #439, #475) under its burn-down (#362),
  and it is binding rather than optional where it applies:
  `docs/decisions/debt-streams-own-artifact-classes.md` is accepted and says the
  split is drawn by what a branch owns.
- **By what gates the parts**, so one blocked half does not make the whole slice
  read as blocked. This is v0.13's #474, "the inputs and rulings this slice
  waits on", and v0.21's #1106 (**no owner decisions left since 2026-08-18** —
  three of them until 2026-08-17, then two) against #1107 (target hardware, met
  on 2026-08-17).
- **By MVP against the rest**, so a slice cannot be held open by optimization.
  This is v0.21's #1120, and it comes with a rule: an epic split off this way
  **declares that it does not gate the slice**, and what it still holds at the
  close moves out. Debt that would read the same if the slice had never happened
  does not belong on such an epic at all: route it by the standing rule — a
  quick item blocking nothing to the rolling-debt milestone, anything unlocking
  only with a v1 consumer to `v1`. That is where an **already-filed** issue goes
  at a phase close, and it is not the finding-triage rule below, which decides
  whether a finding becomes an issue in the first place. Under that rule a quick
  finding is fixed in the PR that found it rather than filed — unless it is
  blocked. So `v0.23` keeps receiving items by two routes: already-filed issues
  routed here at a phase close, and blocked findings filed from a review.

**A slice with more than one epic reaches its phase end when the _last of its
gating epics_ closes**, not the first, and not counting an epic declared
non-gating. `docs/roadmap.md`'s ritual section says the same.

Two milestones do not fit the shape above, and neither is a defect to go fix:
**v0.23** is a holding milestone rather than a slice and will never have an
epic, and **v0.9** has none because #47, its epic, carries the v0.14 milestone
(issue #1114). A slice that is opened but not yet planned has its milestone and
no epic, which is where issues surfaced by the previous slice are placed.
Stories are split so that independent stories can run in parallel; each story is
worked in its own git worktree, on the branch named in the story issue, and its
body lists what it depends on and what it blocks.

## Plan revision at the end of each phase

provisional by design. When a slice's epic closes (v0.1, v0.2, …) — the **last**
of them, on a slice carrying more than one — revise the remaining epics and
stories against what was learned before starting the next slice: update, split,
merge, or re-order the issues, and record scope-level changes as new or updated
records in `docs/decisions/`.

**An epic states the issue count it plans, and the revision that closes the
slice records the count it closed beside it**
(`docs/decisions/slices-are-planned-against-their-inflow.md`, 2026-08-18).
Neither number gates anything and neither is a target: v0.20 planned 13 and
closed 142, and the point of writing both down is that nothing predicted the gap
and nothing recorded it until afterwards. v0.21's three epics carry theirs in a
comment each, posted at that revision.

**The same revision sweeps for unanchored work at three levels** — an open issue
with no milestone, an open issue on a slice that no epic names, and an open
issue with no label that any listing returns. The second was added after nine
such issues were found on v0.21, the third for story #859, which an epic named
in prose and no query returned. An issue that is an exception on purpose is said
to be one. **Each level's scope and its command are stated once**, in
`docs/decisions/slices-are-planned-against-their-inflow.md`; do not restate them
here or in `docs/roadmap.md`, which is how the three copies of this rule drifted
apart six times while it was being written.

**And it reads the rolling-debt milestone as a population**, grouping its open
issues by subject and acting on the groups. Filed one at a time under that
milestone's one-pull-request-each rule, three things are invisible: duplicates
that later work already repaired, clusters that are one property stated N times,
and **items sized against the gate they came from rather than against the
milestone they landed on**. The first pass found one of each: #511 and #647,
already repaired by #1193 and #1186; #1033 and #1060, which make one statement
and both cite #925; and #1241, which that milestone's own half-day threshold
excludes and which moved to `v1`. It is not a re-verification pass — much of
that population asserts an absence, which only a mutation and a test run can
check.

`docs/roadmap.md`'s ritual section carries all three.

**Re-check `docs/features.md` in the same pass**, against the code rather than
against `docs/design/` or `docs/specification/`. It asserts, feature by feature,
what is built and what is not, and no test fails when one of those assertions
goes stale. Four review rounds on the pull request that introduced it found 35
factual errors, and the majority came from claims written out of this
repository's own design and specification records — four of which had themselves
drifted from the code (`04-figma-vocabulary-profile.md`'s letter-case row,
`typeset-latin.md`'s "deliberately absent" list, the v0.10 close's import-oracle
frame count, and the atlas record's byte-identity reading). The recurring
mistake is depth: confirming a capability exists without checking which branches
it does not cover, what the default path does, or whether any command reaches
it. That is what this re-check is for.

