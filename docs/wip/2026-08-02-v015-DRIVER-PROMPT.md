# v0.15 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    empties  when epic #569 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.15 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## Where things stand

`main` is at `0681bad`, CI green, no open PRs, one worktree (the primary, on
`main`). Epic #569 tracks the slice; `docs/roadmap.md` has the slice map.

Closed: #577 (the crate), #600 (the FFI gate), #671 (text-golden budgets).

**#578's flattening half is done** — six changes landed, across issues #650
and #651, #665, #670, #688 and #699. `PaintEntry` is `#[repr(C)]`, `Copy`,
64 bytes and
on `dashscene-unity`'s `extern "C"` surface. Read #578's body and its last
three comments before starting: they say what remains (the per-instance
struct, the packer, the layer-1 goldens), name two hazards, and offer a split.

Order from the epic: **#578**, then #579 and #580 in parallel; #585 depends
only on #580 and is the development loop the rest of the slice runs on, so
land it early; #581 → #582 and #580 → #583 → #584 can proceed in parallel.
Then #586 needs #582/#583/#584; #587 needs #585; #588 last. **#640 is a
prerequisite of #581, not a finding to surface during it** — read the comment
on it first.

## The loop, per story

1. Read the story issue and every comment on it. Several carry corrections
   that post-date the body.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. Implement. Use subagents for mechanical sweeps across crates — scope each
   to named directories, forbid workspace-wide builds, and require it to
   report rather than edit outside its scope.
4. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — read the filter, do not
   recall it.
5. Open the PR **ready, never a draft**. Name the tiers you actually ran.
6. Run `/code-review` on the PR.
7. **Capture every finding as a checklist in the PR description, including
   the ones scored below the tool's reporting threshold.** Fix criticals
   inline. For a minor finding, either fix it if it is a record your own
   change falsified, or file one `debt`-labeled issue linked to the story.
   Never drop a finding silently.
8. Merge only with the checklist complete and CI green **on the commit being
   merged** — `gh pr merge --merge`, named explicitly.
9. Delete the branch (`gh api -X DELETE /repos/{owner}/{repo}/git/refs/heads/<branch>`
   — a `git push --delete` runs the pre-push hook and takes minutes), remove
   the worktree, comment the outcome on the story, update memory.
10. Next story.

## Stop and ask, rather than deciding alone

- A story's scope turns out to be wrong, or its body describes work already
  done. Say so and propose a re-scope; do not silently redefine it.
- A golden moves. That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass.
- A decision that binds other stories (a band threshold, a format, an ABI
  shape). Write a `docs/decisions/` record and flag it.
- Layer 4 (#586) needs a GPU and a recorded adapter; it cannot be run in CI.

## What has actually cost time here

- **`cd` does not persist between commands.** Use `git -C <abs-path>` for
  every git call in a worktree. A backgrounded `cd` once ran a rebase against
  `main` instead of the branch.
- **A green summary is not a green build.** `just build` is four gates and
  only the first prints a summary; it has printed "1324 passed" and exited
  101. Capture `cmd > file 2>&1; echo "REAL EXIT: $?"` and read that line.
- **Check `git config --get remote.origin.url` before any fetch, reset or
  push.** A failed Skia binaries download rewrites `origin` to a Chromium
  mirror for a couple of minutes (debt #677), and with `fetch.prune` on, a
  fetch in that window deletes every remote-tracking ref.
- **Another session may be working in this repo at the same time.** Check
  `ps` for live `git` processes before mutating shared state; worktree and
  branch listings go stale.
- **Mutation-test every assertion you rely on**, and check the fixture is not
  uniform. A range-offset test passed against an implementation that ignored
  the offset entirely, because the only element sat at offset 0. This class
  has now been hit on #650, #651, #561, #688 and #699.
- **Measure before pinning a number.** Guessed layout sizes were wrong twice.
- **Do not enforce a vocabulary rule in the boundary crate.** A
  `MAX_GRADIENT_STOPS` assert in `dashpaint` made two validator diagnostics
  unreachable. P4 puts vocabulary rules in `dashscene-validator` as named
  diagnostics; the boundary crate refuses only API misuse.
- **`corpus/showcase/tests/migration.rs` compares two independent arenas.**
  Anything you flatten into a table needs resolving there in the same change
  (`docs/decisions/cross-arena-comparison-resolves-indices.md`). It has been
  caught five times.

## When the slice is done

Close epic #569 with a summary of what landed, revise the remaining epics and
stories against what v0.15 taught before v0.16 starts, and record scope-level
changes as `docs/decisions/` records — the phase-end ritual in `AGENTS.md`.
