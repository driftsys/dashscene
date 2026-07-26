# Driver prompt — open v0.12, and triage the debt backlog into three streams

Four tasks, in this order. The first is small and unblocks the rest; the last
three are planning, not code.

Read `AGENTS.md` first — its conventions override defaults. `main` is at
`57b0f88`. Epic #344 closed 2026-07-26 and its plan revision ran for the
**roadmap** (#423), but **not** for the issue graph: epic #345 is still marked
provisional and has no story issues. That is task 2.

## 1 — #411: `Cargo.lock` is gitignored, and the stated reason is false

`.gitignore` says:

    # Library workspace: Cargo.lock is not committed (dashc, the one binary
    # target, is still fully reproducible via pinned dependency versions).
    Cargo.lock

Only three direct dependencies are exactly pinned — `flatbuffers`, `skia-safe`,
`fdsm`. `taffy`, `rustybuzz`, `lyon`, `serde`, `blake3` and every transitive
dependency float. So the parenthetical does not hold.

Why it is first rather than filed with the rest: `goldens/dsb/*.dsb` are compared
byte for byte and `goldens/images/*.png` bit for bit, and v0.11 spent a whole
slice making a golden diff attributable to one named cause
(`docs/decisions/r7-survives-the-envelope-rebaseline.md`). Without a lock file a
transitive dependency moving produces a golden diff **indistinguishable from a
real regression**. v0.12 re-baselines goldens again when banks land, so the cost
of waiting lands inside the next slice.

Recommended: commit `Cargo.lock`, drop the ignore line, replace the false comment
with the real reason, and record it — this reverses a documented convention, so it
needs a decision record, not just a diff. The alternative (keep it ignored and
record why) is a legitimate answer if the reasoning holds; what is not acceptable
is leaving a false claim in the file.

Also fix the capture's now-wrong claim: `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`
names `Cargo.lock` as "the mechanism on the crate side" for reproducible banks.

## 2 — v0.12's story breakdown (epic #345)

Scope is settled and revised; do not re-derive it. Read, in order:

- `docs/roadmap.md`'s v0.12 section — revised at the v0.11 close, including the
  band-coverage constraint it added.
- `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` — **the ungardened
  half is this slice's input.** Its `status` line says which half is as-built and
  which is still live; do not re-garden the as-built half.
- `docs/technotes/2026-07-26-tolerance-band-coverage.md` and issue #422.

Break #345 into `story`-labeled issues, one per independently workable piece,
each naming its branch and what it depends on. Four things v0.11 learned that
should shape the breakdown rather than be rediscovered:

- **Sequence the structural format change so its diff is attributable.** Splitting
  "the file grows an envelope" from "the schema changes" turned a seven-golden
  rewrite into a mechanical check — each new file's section 0 equalled the whole
  of the old file. Cold-bank assembly re-baselines goldens again; split it the
  same way so the diff has one cause.
- **A one-instance fixture cannot fail.** `v03-paint` has one image, so every
  index in it is 0 and it could not fail a dedup, ordering, or wrong-index bug.
  The per-asset band oracle has the same hazard: one asset per class proves
  nothing about _escalation_, which is the whole mechanism.
- **Each band ships with the mutation that fails it.** That is #422's
  recommendation and the discipline the import-oracle frames adopted at this
  close — not a budget chosen in advance and never exercised.
- **#416 gets its second writer here.** The packer re-derives payloads, so an
  `AssetEntry`'s recorded format and extent can finally disagree with the payload
  it names. Deciding where an image header parser lives (`dashscene-validator`
  publishes before `dashc`) is a crate-boundary decision this slice should take
  with the packer's needs concrete — it will also want to _decode_, which is a
  different trust boundary.

`dashpack` is not among the 12 squatted crate names: register it in
`docs/decisions/crate-name-map.md` when the slice opens.

## 3 — v0.13 cluster triage, including the v0.9 and v0.10 strays

Milestone 14 has 51 open items, clustered by scope:

    11  dashscene-engine     7  (unscoped)      2  dashpaint
    11  dashscene-core       5  dashscene-typeset   2  dashlang
     8  dashcue              2  dashscene-skia   1 each  importers, repo, goldens

Plus strays that were never re-anchored: **v0.10 has 18 open and v0.9 has 5**,
both closed slices. Triage all of it as one set, not as three lists.

The dividing line is already recorded — `docs/decisions/pre-v1-hardening-slice.md`:
feature scope gated on a specific v1 consumer stays on v1, because it unlocks with
its consumer and is not burn-down-able early.

One constraint on how far this can go: **v0.13's scope keeps growing while v0.12
runs.** v0.11 filed three debt items in one slice. So pull clusters _forward_ into
parallel streams; do not declare v0.13 started, or the focused pass it exists for
runs against a moving list and needs a second pass anyway.

## 4 — the isolation protocol for three streams

Three streams, split by **what a branch owns**, not by which slice an item
belongs to:

- **Stream A — v0.12.** Owns `dashpack`, `dashbuf`'s format surface, `goldens/`,
  `dashscene-skia`'s profile preview, and every byte-exact artifact. **The only
  stream allowed to regenerate a golden.**
- **Stream B — `dashcue` + `dashscene-typeset` debt** (13 items). No overlap with A.
- **Stream C — `dashscene-engine` debt** (11 items). No overlap with A.

Hold back `dashscene-core` (11 items) until A's shape is visible — core's
commit/alloc cluster is where bank assembly may land. Hold back the `goldens`,
`dashpaint` and `dashscene-skia` debt (5 items) entirely: that is A's territory.

Why goldens serialize rather than merge: a regenerated binary golden does not
merge, it collides, and the second session cannot tell whether its regeneration is
correct without re-deriving the first's reasoning. Two concurrent re-baselines
would destroy the attribution property v0.11 built.

The protocol every stream gets, verbatim:

- One git worktree per story, `./bootstrap` after `git worktree add`. Never edit
  another stream's checkout.
- **Never `git reset --soft origin/main` to reshape a branch.** It moves HEAD
  without touching the working tree, so if `origin/main` has advanced your next
  commit silently reverts everything that landed in between — and `just verify`
  still passes, because a revert is self-consistent. This nearly shipped a revert
  of seven files during v0.11. Before pushing any reshaped branch, check
  `git diff --name-only origin/main HEAD` lists only files you meant to change.
- Rebase onto `origin/main` before opening each PR, and re-run `just verify`
  after — it is the merged state that has to be green, not the branch's own.
- CI cannot run (#263). Local `just build` / `just verify` is the gate.

One honest limit to state in the plan: **review is the throughput bottleneck, not
free crates.** Every one of v0.11's five stories had a real defect found in
review — a reader trusting the writer, three unvalidated header fields, decorative
compile-time assertions, a falsely-refused legal JPEG, a named diagnostic turned
into a panic, and a fixture-update path that had become silently destructive.
Three streams is where to stop.

## Workflow, non-negotiable

Draft PR → `/code-review` → every finding captured as a checklist in the PR
description → critical fixed, minor filed as one `debt` issue each → ready only
after review → merge with a merge commit. Conventional commits; **the scope is
mandatory and validated**. Prose is plain literal English. Garden `docs/wip/` and
archive this prompt verbatim when its work lands.
