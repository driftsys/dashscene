# Lane P — the unowned inflow: engine, ffi, and the CI pipeline

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `291fbcbc` on 2026-08-16.

**Why this lane exists:** these four sit in crates and files no Wave 3 lane
covers. They are not leftovers from a lane — they have never had an owner.

## Setup

    git worktree add <worktrees>/wt-lane-p -b debt/v020-unowned-inflow origin/main
    cd <worktrees>/wt-lane-p
    ./bootstrap

## What you own

    #1153  the baseline map puts a hash probe on the per-frame readback path   dashscene-engine
    #1183  the byte-taking loads leave a stale LiveScene paired with a fresh arena   dashscene-ffi
    #1171  the merge-group range is derived twice, with different dot semantics  .github/workflows/ci.yml
    #1181  the merge queue requires allow_auto_merge, and nothing records that   docs + repo settings

**These are three unrelated areas.** Do them as separate PRs; there is no shared
design question. #1171 and #1181 are both CI and can share one.

## Verified territory

    baseline_pass / cross_offset   crates/dashscene-engine/src/lib.rs:940 and :1076
    load_into                      crates/dashscene-ffi/src/lib.rs:457
    load_mapped_into               crates/dashscene-ffi/src/lib.rs:497

**#1153 is `dashscene-engine`, not `dashscene-core`** — the issue does not say
which crate, and the obvious guess is wrong. Verified: `baseline_pass` is called
twice in `crates/dashscene-engine/src/lib.rs`, at lines 940 and 1076. Both call
sites matter; a fix at one is half a fix.

## #1153 — a cost that moved, not a cost that appeared

PR #1150 made `baseline_pass`'s `cross_offset` a sparse `FxHashMap<NodeId, f32>`
where it had been `vec![None; node_count]`. **That was the right change** — the
map is the honest model, a baseline offset exists only for children of a
baseline-aligned text row, and an empty map allocates nothing, which is what took
the per-frame band's byte term to 0.

The finding is that it **moved** a cost rather than removing one: a hash probe
now sits on the per-frame readback path. So:

- **Do not revert to the vector.** That undoes issue #1111's measured win.
- The question is whether the probe can be avoided on the path that reads, not
  whether the map was a mistake.
- **`goldens/tooling/tests/per_frame_scaling.rs` is the instrument**, and it is
  **lane L's file** (issues #997, #1015, #1146). Read it, do not edit it. If your
  change needs a new term there, say so in the PR and let lane L add it.

## #1183 — the asymmetry is the finding

`load_mapped_into` (`ffi/src/lib.rs:497`) clears `runtime.scene` in the same
breath as it replaces the arena, with a comment explaining why: a caught panic
between that point and the reassignment would leave a runtime holding a **new
arena** and the **previous document's `LiveScene`**.

`load_into` (`ffi/src/lib.rs:457`) — the byte-taking path — does not.

Found while writing `docs/design/c-abi.md`: the record described the mapped
loader's fix and the description does not hold for the other two loaders.

**So there are two artifacts to fix, and the doc is not optional.** If you fix
the code and leave `docs/design/c-abi.md` describing a property only one loader
has, you have reproduced the defect that filed this issue. Note also that
**#1190** is open against that same file ("duplicates the crate's module docs and
states…") — check whether it is being worked before editing it.

`crates/dashscene-ffi/` has a `c-abi` gate: **`just check`** compiles the
committed header from C and checks the two halves agree. It needs a C toolchain.

## #1171 and #1181 — the CI pair

**#1171** — `ci.yml`'s `changes` job derives the batch's range in two independent
places: `base`/`ref` on the `dorny/paths-filter` step, and `BASE`/`HEAD` on the
code-detector step. They disagree in form: with a SHA `base`,
`dorny/paths-filter@v3` runs `git diff base..head` — **two dots** — while the
detector runs `is-code-change "$BASE...$HEAD"` — **three dots**, under a 13-line
comment explaining why two dots is wrong.

**Be careful how you test this.** A broken merge-group expression **reports green
over the wrong range — it does not go silent**: a wrong field name yields an
empty string, `scripts/is-code-change` fails closed to `true`, and the suite runs
against a degraded range and passes. Verify by running the script with an empty
`BASE` rather than by reading a green run.

**#1181** — enabling the `merge_queue` rule is not sufficient. GitHub implements
"merge when ready" through auto-merge, so with `allow_auto_merge: false` every
enqueue fails:

    $ gh pr merge 1179 --merge
    ! The merge strategy for main is set by the merge queue
    GraphQL: Auto merge is not allowed for this repository (enablePullRequestAutoMerge)

Measured on PR #1179. **This is a record-keeping issue** — the setting is a
repository property that nothing in the repo documents, so the next person to
configure a queue rediscovers it. The fix is durable prose, most likely in
`docs/decisions/review-before-ready-not-before-open.md`, which already carries the
ruleset's parameters.

**Do not change repository settings as part of this** unless the owner asks —
record what is required and why.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just check`** if you touch `crates/dashscene-ffi/`, for the `c-abi` gate.
3. For any `ci.yml` change: `gh workflow run ci --ref <your branch>` forces the
   path-filtered gates on, so a filter change can be measured before it merges.
4. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
5. **The finding-triage rule changed on 2026-08-16 — do not use the old one.**
   Findings are **fixed in the pull request that found them**. File one as `debt`
   only when (a) the fix cannot be made here — blocked on hardware, on a missing
   dependency, on a v1 consumer, or on an owner ruling — or (b) it is not
   critical, is over half a day, and names no correctness defect. **This PR
   closes `debt` issues, so under (b) you may file only a nice-to-have.** A
   finding you judge incorrect is rejected on the checklist with the reasoning.
   Record fixed / rejected / filed against each item.
   (`docs/decisions/review-before-ready-not-before-open.md`.)
6. **Review every change made after the review pass.**
7. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
8. **Before merging** — `gh pr view <n> --json files`. Your paths are
   `crates/dashscene-engine/`, `crates/dashscene-ffi/`, `.github/workflows/ci.yml`
   and `docs/decisions/`. Anything else is a stray.
9. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass.
10. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
    the commit being merged, then `gh pr merge --merge`. **Enqueue only once `ci`
    is green** — with checks still running, `gh pr merge` silently enables
    auto-merge instead, which merges later with nobody reading the checklist.
    That is #1181's own subject; you of all lanes should get it right.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `goldens/tooling/tests/per_frame_scaling.rs` — **lane L** owns it.
- Do not edit `crates/dashscene-gpu/` or `crates/dashc/` — lanes H2 and I2.
- Do not change repository settings for #1181 without the owner's say-so.
- Do not merge on a green `just verify` alone. It runs no test tier.
