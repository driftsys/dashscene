# Wave 3, lane H — dashscene-gpu

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `4faeeda2` on 2026-08-16. Everything marked "the issue claims"
was not — check it yourself.

**Do not start until Phase 0 (#1046, the doc-link gate) has merged.** It changes
what `just lint` checks in every crate, including yours.

## Setup

    git worktree add <worktrees>/wt-lane-h-gpu -b debt/v020-gpu-wave3 origin/main
    cd <worktrees>/wt-lane-h-gpu
    ./bootstrap

Cold build — skia and wgpu. Expect several minutes on the first `just test`.

## What you own

Seven issues, all in `crates/dashscene-gpu`:

    #1021  a degenerate coverage field draws nothing and is named by no diagnostic
    #1034  field_draws admits an infinite plane_bounds; both painters then compute an infinite px_range
    #1040  the WGSL gate test justifies its comment strip with a claim that is false
    #1043  the same test counts its gates and cannot see where the call sits
    #1041  the draw_runs fixture builds a Resolved the resolve_frame invariant forbids
    #1050  BlurTargets survives forget_uploaded, so a bind group can outlive the atlas index it names
    #1055  LayerTargets::prepare has the release-and-rebuild thrash #1020 fixed for BlurTargets

Read each with `gh issue view <n>`.

## Verified symbol map — `crates/dashscene-gpu/src/render.rs` unless named

    Renderer::resolve_frame       render.rs:2362
    Renderer::refuse              render.rs:2679   (a second, unrelated `refuse`
                                                    lives in residency.rs:712 —
                                                    do not confuse them)
    Renderer::forget_uploaded     render.rs:1454
    field_draws                   render.rs:3040
    draw_runs                     render.rs:3114
    GpuImage                      render.rs:160
    LayerTargets                  render.rs:3312
    BlurTargets                   render.rs:3715
    BlurTargets::bound_atlases    render.rs:3795
    Residency::forget_resident    residency.rs:517

**Line numbers move.** #1040's body cites `paint.wgsl` lines 772/790 and 773/793;
on current `main` the sites are **787/805 and 788/808**. Verify positions
yourself and cite symbols in anything you write.

## #1040 — the nuance that makes it look wrong

The test `both_msdf_arms_gate_on_the_row_the_frame_resolved`
(`render.rs:5363`) strips WGSL comments before counting, and justifies that with
"both gates sit under long explanatory blocks that quote the condition".

**Two comments in `paint.wgsl` do mention `params2.w`** — at lines 309 and 382.
A loose grep finds them and concludes the issue is mistaken. It is not: the test
counts the full literal `in.params2.w != 0.0`, and **neither comment contains
it**. Verified. The issue's claim holds; the comment strip is justified by
something that is not true.

**#1040 and #1043 are the same test and should be one PR.** #1043 is the deeper
half: counting cannot see structure, so an emptied gate —

    if in.params2.w != 0.0 {
    }
    shape = msdf_coverage(msdf_sample(...), in.params2.z);

— leaves the counts equal and the test passes with both gates inert. Fixing the
justification without fixing the blindness closes the smaller half of one defect.

## #1021 and #1034 are the same predicate from two sides

`field_draws` is the gate. #1034 says the predicate itself is wrong —

    right > left && bottom > top && field.atlas_rect[2] > 0 && field.atlas_rect[3] > 0

rejects a NaN (every comparison against one is false) and **admits an infinity**.
#1021 says that whatever `field_draws` rejects is dropped with **no diagnostic**:
the row keeps `GpuShape::default()`, `Renderer::refuse` is never called, and P4
wants every out-of-profile construct named rather than silently dropped.

**#1034 reaches `dashscene-skia`.** Issue #1000 restated `field_draws` in
`field_coverage` so the two painters agree, so changing the predicate here and
not there re-opens the divergence #1000 closed. **`crates/dashscene-skia/src/lib.rs`
is lane N's file in Phase 2** — coordinate before editing it, or state in your PR
that the skia half is left for lane N and file it.

## #1050 — the mechanism, verified

`Renderer::forget_uploaded` resets uploaded state, the residency set and the
refusals, and does **not** touch `self.blurs`. `Residency::forget_resident`
retains shared atlases but does `self.atlases.retain(|atlas| !atlas.dedicated)`,
so a dedicated texture — what an oversized payload gets since issue #720 — is
dropped and **every index after it shifts down**. `BlurTargets::bound_atlases`
records an atlas *index*, so its bind groups can then name the wrong atlas.

`render.rs:3781` already carries a doc paragraph about exactly this shift. Read
it before designing the fix; it may already state the invariant you need.

## What to measure rather than argue

- **#1055's thrash needs two frames** with the render-target group appearing and
  disappearing. A single frame proves nothing. #1020's grace period is the
  pattern to copy — read what it actually does before assuming symmetry.
- **A gate whose deletion changes no rendered texel is a real result**, not a
  failed test. It happened on PR #1009: the whole layer-3 suite passed with the
  `KIND_TEXT` gate removed. If that is what you find, a **source** assertion is
  the only automated guard — and #1043 is precisely about a source assertion that
  does not work. Do not write another one of the same shape.
- **A refusal test that paints an opaque rect over the whole canvas cannot see a
  missing clear.** `Renderer::render` reuses its offscreen across calls at one
  extent, so a second frame over the first is how that becomes visible — and the
  reuse itself must be asserted (`allocations()` unchanged), or a rebuilt texture
  arrives zeroed and the sweep passes vacuously.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just wasm-painter` and `just wasm-lint`** — this crate has a wasm32 half the
   host clippy pass cannot see, and a blocking wait on the web path deadlocks.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review` **while
   CI runs**. Capture every finding as a checklist; never drop one. Budget for
   volume: this crate's PRs have returned 8 to 26 findings each, and PR #1037
   needed three rounds.
4. Fix all critical findings. **Review the fix round too** — on PR #989 four of
   eighteen findings came from that pass alone, and one was a regression the
   first fix introduced.
5. File each independent minor finding as `debt` on **v0.20**.
6. Write **`Refs #<n>`**. A closing keyword fires from commit messages that land
   on `main`, matches mid-sentence, takes only the first number, and a negated
   sentence matches as well as a positive one.
7. **Before merging** — `gh pr view <n> --json files`. Any path outside
   `crates/dashscene-gpu/` is a stray, and a stray is how a merge reverts another
   lane: PR #1037 reverted PR #1038 across seven files it never edited, and
   PR #1063 exists only to restore them.
8. **After merging** — `git diff --stat <previous-merge-sha> origin/main -- <that
   PR's files>`. An empty diff is the pass.
9. Rebase, squash to one conventional commit, force-push, wait for `ci` green on
   the commit being merged, then `gh pr merge --merge`.
10. After merging, `gh issue view <n> --json state` for every issue your commits
    named.

## Do not

- Do not edit `crates/dashpaint/src/lib.rs` — **lane N** owns it in Phase 2, and
  #1074 makes `Atlas::image` private, which will rewrite call sites in your file.
  Expect that rebase; do not pre-empt it.
- Do not edit `crates/dashscene-skia/src/lib.rs` without saying so — see #1034
  above.
- Do not merge on a green `just verify` alone. It runs no test tier.
