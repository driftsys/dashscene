# Driver prompt — lane D: dashscene-gpu, the PR #1009 review inflow

Run this with **Opus**. Everything below marked "Verified" was checked against
the tree on 2026-08-15 with `origin/main` at `557179b`. Everything marked "the
issue claims" was not — check it yourself before acting on it.

## Setup

    git worktree add <worktrees>/wt-lane-d-gpu -b debt/v020-gpu-refusal-inflow origin/main
    cd <worktrees>/wt-lane-d-gpu
    ./bootstrap

This worktree pays a cold build (skia and wgpu). Expect several minutes on the
first `just test`.

## What you own

Ten issues, all in `crates/dashscene-gpu`:

    #995   layer-3 tests have no shared module
    #1020  a backdrop refusal that changes between frames rebuilds the blur targets
    #1021  a degenerate coverage field draws nothing and is named by no diagnostic
    #1022  the backdrop blur-row contract panic no longer covers a backdrop that draws nothing
    #1023  a refused image fill is emptied by a sentinel
    #1024  a refused glyph atlas still rasterizes one quad per glyph, every frame
    #1025  an empty frame does not clear, and Renderer::draw's doc says it does
    #1026  bound_atlases keys the blur bind-group rebuild per slot, doc claims per backdrop
    #1027  GpuShape and GpuGlyphRun hand-copy an identical 32-byte tail
    #1028  BlurTargets::prepare allocates a Vec per frame only to compare it

Nine of the ten were filed by the `/code-review` fan-out on PR #1009 (issues
Issues #993, #994). Read each with `gh issue view <n>` before editing. They are
detailed and mostly correct; they are not always right about their own subject
— see "The trap" below.

**This lane is not splittable.** Verified: `resolve_frame`, `resolve_backdrop`,
`refuse`, `refusals`, `draw_runs`, `draw`, `render`, `backdrop_mask`,
`field_draws`, `atlas_of`, `resident_image`, `GpuShape`, `GpuGlyphRun`,
`GpuImage` and `BlurTargets` are **all in `crates/dashscene-gpu/src/render.rs`**
— one file. Two parallel sessions would collide on every hunk. Run it as one
lane with several sequential PRs.

## Verified facts — do not re-derive

Symbol locations, `origin/main` at `557179b`, all in
`crates/dashscene-gpu/src/render.rs` unless named otherwise:

- `Renderer::resolve_frame` — the residency resolve, `atlas_of_shape` and
  `atlas_of_run` are built here.
- `Renderer::resolve_backdrop` — the two packer-contract checks #1022 is about.
- `Renderer::refuse` — the only writer of the refusal list. There is a second,
  unrelated `refuse` in `crates/dashscene-gpu/src/residency.rs`; do not confuse
  them.
- `Renderer::refusals` and `Renderer::refusals_seen`. `SurfaceRenderer` has its
  own forwarding `refusals` in `crates/dashscene-gpu/src/surface.rs` — PR #1009's
  review found the surface path had *not* forwarded an accessor before, so
  **check both when you add one**.
- `draw_runs(buffer, resolved)` — a free function, the subject of #1024.
- `backdrop_mask(instance, resolved) -> Option<Option<GpuShape>>` — a free
  function, the double `Option` is what #1021 and #1026 both turn on.
- `field_draws(field: &dashpaint::VectorField) -> bool`.
- `BlurTargets::bound_atlases: Vec<Option<u32>>`, and its two uses: the
  comparison and the assignment. #1028 is the `collect()` above that comparison.
- `composite::plan(buffer) -> Vec<Pass>` is in
  `crates/dashscene-gpu/src/composite.rs`, not `render.rs`. That is #1025's
  subject.
- `ResidencyError::FrameExceedsAtlas` and `ResidencyError::NoDecoder` are in
  `crates/dashscene-gpu/src/residency.rs`. #1020's argument rests on
  `FrameExceedsAtlas` being the non-memoized arm — verify that before you accept
  the oscillation premise.

Verified for #1027: `GpuGlyphRun` is `{ color, uv, half_uv, px_range, resolved }`
and `GpuShape` is `{ plane, uv, half_uv, px_range, resolved }`. The tail from
offset 16 is identical, and both carry a doc paragraph explaining the `half_uv`
before `px_range` ordering and the WGSL alignment reason for it. The issue's
claim is accurate.

Verified for #995: `crates/dashscene-gpu/tests/common/` **does not exist**.
`include_bytes!` appears 3 times in `layer3_image_fills.rs` and once each in
`layer3_backdrop_blur.rs` and `layer3_text_and_fields.rs` — five occurrences in
three files. The issue says "the same path in three files"; check which of the
five are the same path before writing the shared constant.

**There is precedent in this workspace**: `crates/dashc/tests/common/mod.rs`
exists and is shared across that crate's twelve integration-test binaries. Copy
its shape rather than inventing one.

## The trap — the issues are not the last word on themselves

On PR #989 the issue's own prescribed fix was **worse than the defect it
described**: clearing `GpuBlur::masked` for a refused field would have frosted
the node's whole box where a corner patch did. It was caught by measuring, not
by reasoning. Two of your ten prescribe a fix in a "What closing it looks like"
section. Treat those as a hypothesis to test, not an instruction.

Note that #1026 says the current behaviour **is correct** and asks only that
the reason be stated. Do not "fix" it into a rebuild-per-frame.

## Suggested PR sequencing — one file, so serialise

You decide, but this ordering keeps the hunks apart:

1. **The P4 diagnostic group** — #1021, #1022, #1023. All three are "a case
   that draws nothing and names itself nowhere". P4 is the principle they are
   measured against: every out-of-profile construct is a named diagnostic,
   never a silent drop.
2. **The per-frame cost group** — #1024, #1028, #1020. All three cost budget on
   the constrained path; R-T4 bounds a steady-state frame to a dirty-range
   upload and a submission.
3. **The doc/contract group** — #1025, #1026.
4. **The refactors, last** — #1027 (shared tail; it moves offsets, so it
   conflicts with everything above) and #995 (tests only).

If you find a cheaper grouping, take it — but say in the PR body which issues
each PR closes and which it does not.

## What to measure rather than argue

- #1024 says a refused glyph atlas still submits one instance per glyph. Count
  the instances, do not infer them from the range arithmetic.
- #1025 says an empty frame presents an unwritten drawable. `Renderer::render`
  asserts the buffer is non-empty, so **no offscreen test can reach this** —
  whatever you write to hold it has to go through the surface path or through
  `draw` directly.
- #1020's oscillation needs two frames with different refusal outcomes. A single
  frame proves nothing.
- Deleting a gate that changes no rendered texel is a real outcome, not a failed
  test. It happened on PR #1009: the whole layer-3 suite passed with the
  `KIND_TEXT` gate removed. If that is what you find, a **source** assertion is
  the only automated guard — the repo has precedent in
  `the_gradient_kinds_are_distinct_and_match_the_shader`. Write it over
  comment-stripped source, match whole statements rather than substrings, and
  count gates against calls rather than against a literal.
- A refusal test that paints an opaque rect over the whole canvas cannot see a
  missing `clear`. `Renderer::render` reuses its offscreen across calls at one
  extent, so a second frame over the first is how that becomes visible — and the
  reuse itself has to be asserted (`allocations()` unchanged), or a rebuilt
  texture arrives zeroed and the sweep passes vacuously.

## Definition of done

1. `just test` between edits. `just build` green before pushing — quote its
   Summary line, do not paraphrase it.
2. `just wasm-painter` and `just wasm-lint` for anything you touch in this
   crate: `dashscene-gpu` has a wasm32 half that the host clippy pass cannot
   see, and a blocking wait on the web path deadlocks.
3. Push. **`just verify` may fail on the secrets gate for reasons that are not
   yours** — worktrees share one object store, so the scan sees every unpushed
   commit on this machine. Check what it names before assuming your change is at
   fault; issue #987 is about exactly this gate.
4. Open the PR **as an ordinary PR, never a draft** — `/code-review` declines a
   draft.
5. Run `/code-review` on the PR **while CI runs, not after**. Capture every
   finding as a checklist in the PR description. Never drop one silently.
   Budget for it: `/code-review` at `max` returned 15 findings plus a 6-finding
   gap sweep on ~680 lines of PR #1009 that had already been mutation-tested
   eight ways.
6. Fix all critical findings. File each minor one as its own `debt`-labeled
   issue linked to this work, **on the v0.20 milestone** — debt with no
   milestone is invisible at every slice close.
7. **When a critical finding changes the implementation, review the fix too.**
   On PR #989 four of eighteen findings came from that second pass alone, and
   one of them was a regression the first fix introduced.
8. In prose and commit messages write **`Refs #<n>`**. A closing keyword fires
   from commit messages that land on `main`, matches mid-sentence, takes only
   the first number, and **a negated sentence matches just as well as a positive
   one**. Put the one intended closing line on its own line at the end.
9. Before merging: `gh issue list --milestone "v0.20 — hardening: the critical
   findings and the Android recovery path" --state open` and read it.
10. Rebase onto the latest `main`, squash to one conventional commit,
    force-push, wait for `ci` green **on the commit you are merging**, then
    `gh pr merge --merge`. Merging is strictly serial here: `main` requires an
    up-to-date branch and auto-merge is disabled, so every other lane's branch
    goes `BEHIND` when you land.
11. After merging, `gh issue view <n> --json state` for every issue your commits
    named, not only those in the PR body.
12. **Before merging, read your own PR's file list** — `gh pr view <n> --json files`.
    Any path outside `crates/dashscene-gpu/` is a stray, and a stray is how a
    merge reverts another lane.
13. **After merging, check the previous lane's work is still on `main`:**

        git diff --stat <previous-merge-sha> origin/main -- <that PR's files>

    An empty diff is the pass. **This has failed twice.** PR #1037 — this lane's
    own first PR — reverted PR #1038 across seven `dashpaint`, `dashscene-skia`
    and `dashscene-validator` files it never edited, and `main` was missing work
    that four issues read as `CLOSED` for 90 minutes (restored by PR #1063).
    Earlier, PR #978 dropped PR #961's `justfile` recipe the same way (#991).
    CI is green through this: the older content still compiles and still passes
    its own tests, because the reverted lane's tests went with it.

## Do not

- Do not touch any file outside `crates/dashscene-gpu/` and the decision records
  you are correcting. Lane F owns `crates/dashpaint/src/lib.rs`; a signature
  change that reaches it must wait for that lane or be coordinated.
- Do not take **#960** — despite its title naming a GPU device, its body is the
  Android `surfaceDestroyed` deadlock. That is lane G's.
- Do not take #1000 (skia's `field_coverage`) — it is the painter-divergence
  twin of your work and belongs to lane F, which owns the seam.
- Do not merge on a green `just verify` alone. It runs no test tier.
