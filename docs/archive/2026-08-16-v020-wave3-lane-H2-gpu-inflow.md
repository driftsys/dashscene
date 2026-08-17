# Lane H2 — dashscene-gpu, the inflow after lane H closed

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `291fbcbc` on 2026-08-16.

**Why this lane exists:** lane H finished and its crate is free. These two issues
arrived after it closed and have nobody. No other running lane touches
`crates/dashscene-gpu`, so this starts immediately with no coordination.

## Setup

    git worktree add <worktrees>/wt-lane-h2-gpu -b debt/v020-gpu-inflow origin/main
    cd <worktrees>/wt-lane-h2-gpu
    ./bootstrap

Cold build — skia and wgpu.

## What you own

    #1149  check_extent admits a zero, so a surface with no pixels is accepted
    #1185  the masked-fill quad is origin-cancelled and unguarded, and skia's doc says it cannot be

Read both with `gh issue view <n>`.

## #1149 — this is the half of #1094 that could not be done then

Verified: `Renderer::check_extent` is at **`crates/dashscene-gpu/src/render.rs:1416`**
and tests only

    width > self.max_extent || height > self.max_extent

so **a zero on either axis is accepted**. `Renderer::max_extent` is at
`render.rs:653`.

The issue names three callers that hand an extent to `wgpu` after passing
through it — `SurfaceRenderer::new_async` (`surface.rs:284`),
`SurfaceRenderer::resize` (`surface.rs:390`) and `Renderer::render`. **Verify
that list yourself**: `Renderer::new_async` also exists at `render.rs:867`, and
the two `new_async` are different functions in different types. An earlier
session confused exactly this kind of pair.

**Context you should have.** The android half of #1094 landed already: the first
attach was taking up a 0x0 extent, and `machine.rs` now refuses it. This issue is
the defence in depth behind that — the guard at the layer that hands the extent
to `wgpu`. So a fix here should hold even for a caller that has no android-side
machine, and the android-side test is not the one that proves it.

## #1185 — read the skia doc it contradicts before you touch anything

`crates/dashscene-skia/src/lib.rs`'s `field_coverage` documents its device-quad
guard and asserts the lean painter needs no equivalent:

> `dashscene-gpu` has no such case — it hands `plane_bounds` to the shader and
> derives its scale from `right - left` directly — so this second guard is this
> painter's and not a restatement of anything.

The issue's finding is that **this is true of one gpu pipeline and false of the
other**: `gpu_shape` (`render.rs:2871`) does derive `px_range` from
`right - left` directly, and the masked-fill path does not.

Two consequences you must handle rather than pick between:

- The **code** gap in `dashscene-gpu` — yours.
- The **skia doc** that now states something false. `crates/dashscene-skia/src/lib.rs`
  is not this lane's file and issue **#1160** is already open against
  `field_coverage`. **Do not edit it.** Either coordinate with whoever holds
  #1160, or state in your PR body that the skia doc is left false and say which
  sentence — do not leave it implied.

This is the third round of the same painter-divergence family (#1000, #1034,
#1144, #1160). Before writing a predicate, check whether
`dashpaint::VectorField::draws` — the one method both painters call since
issue #1144 — is the thing to reuse rather than restate. A fourth copy is the
mechanism every one of those issues was filed for.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just wasm-painter` and `just wasm-lint`** — this crate has a wasm32 half
   the host clippy pass cannot see.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one.
4. **The finding-triage rule changed on 2026-08-16 — do not use the old one.**
   Findings are **fixed in the pull request that found them**. File one as `debt`
   only when (a) the fix cannot be made here — blocked on hardware, on a missing
   dependency, on a v1 consumer, or on an owner ruling — or (b) it is not
   critical, is over half a day, and names no correctness defect. **This PR
   closes `debt` issues, so under (b) you may file only a nice-to-have — a
   finding that names no defect at all.** A finding you judge incorrect is
   rejected on the checklist with the reasoning. Record fixed / rejected / filed
   against each item; a ticked box alone does not say which.
   (`docs/decisions/review-before-ready-not-before-open.md`.)
5. **Review every change made after the review pass** — over what changed, not a
   second full pass.
6. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
7. **Before merging** — `gh pr view <n> --json files`. Anything outside
   `crates/dashscene-gpu/` is a stray.
8. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`; an empty diff is the pass. This has failed twice on this repo,
   and the second repair was itself only half done (issue #1168).
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
   the commit being merged, then `gh pr merge --merge`. **Enqueue only once `ci`
   is green**: with checks still running `gh pr merge` silently enables
   auto-merge instead, which merges later with nobody reading the checklist.
10. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashscene-skia/src/lib.rs` — see #1185 above.
- Do not edit `crates/dashpaint/` — Phase 2 owns it (#1045, #1074).
- Do not merge on a green `just verify` alone. It runs no test tier.
