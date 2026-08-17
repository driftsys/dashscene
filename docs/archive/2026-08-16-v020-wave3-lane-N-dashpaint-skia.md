# Wave 3, lane N — dashpaint and the reference painter

Run this with **Opus**. Everything marked "Verified" was checked against
`origin/main` at `bfd776fb` on 2026-08-16. Everything marked "the issue claims"
was not — check it yourself.

Four other lane prompts already name this lane as the Phase 2 owner of
`crates/dashpaint/src/lib.rs` and `crates/dashscene-skia/src/lib.rs`, and three
of them deferred work to it. This prompt is written after those lanes ran, so it
carries what they found.

## Setup

    git worktree add <worktrees>/wt-lane-n-skia -b debt/v020-dashpaint-skia origin/main
    cd <worktrees>/wt-lane-n-skia
    ./bootstrap

Cold build — skia. Expect several minutes on the first `just test`.

## `main` has a MERGE QUEUE since 2026-08-16 — read this before you finish

`gh pr merge <n> --merge` **fails**: "The merge strategy for main is set by the
merge queue". `gh pr merge <n>` alone fails too, because repo-level auto-merge is
disabled. Enqueue with the mutation the merge button calls:

    ID=$(gh pr view <n> --json id --jq .id)
    gh api graphql -f query='mutation($id:ID!){ enqueuePullRequest(input:{pullRequestId:$id}){ mergeQueueEntry { position state } } }' -f id="$ID"

Queue config, verified: `merge_method: MERGE`, `min_entries_to_merge: 1`,
`min_entries_to_merge_wait_minutes: 5`, `grouping_strategy: ALLGREEN`. It builds
a merge group, re-runs `ci` on it, and merges. Your PR can land in a group with
another lane's — that happened on PR #1173 and both file sets were intact. Poll
`gh pr view <n> --json state` until it leaves `OPEN`.

## What you own

    #1074  Atlas::image stays pub, so a payload can be swapped under the checked extent
    #1045  ClipTable::push and ImageTable::push convert an offset and a count separately
    #1160  field_coverage's device-quad guard still runs after the atlas fetch
    #1186  field_effect recompiles its SkSL every paint()
    #1185  the lean painter's masked-fill quad is origin-cancelled and unguarded

Read each with `gh issue view <n>`. **#1160 and #1185 both carry a comment that
corrects their own body — read the comments, not only the bodies.** This
repository amends by comment.

## Verified symbol map

    crates/dashscene-skia/src/lib.rs
      ImageCache                   287     ImageCache::begin_frame    303
      field_effect (the local)     432     draw_vector_field         1117
      field_coverage              1263     draw_backdrop_blur_field  1557
      decode_image                1921
      a_coverage_field_that_draws_nothing_decodes_no_atlas   2554

    crates/dashpaint/src/lib.rs
      PaintTable::push             725     Atlas                     2639

**Line numbers move.** Cite symbols in anything you write.

## #1160 — the design is already done; do not redo it

A four-way design fan-out ran on 2026-08-16 and its conclusion is in the issue's
comment. The short form:

**Do not** thread `&mut ImageCache` into `field_coverage`, which is the shape the
issue body sketches. It works — a verifier applied it, compiled clean, ran 76 of
76 crate tests and measured the win — but it deletes both hoists issue #1044 put
in, which makes the `FIELD_MASK_SKSL` compile unconditional (that is #1186), and
it falsifies about forty lines of prose including a test assertion message.

**Do** extract the decidable part:

    fn field_quad(rect: &RectEntry, field: &VectorField) -> Option<Rect>

holding `field.draws()`, the `dest` construction and the device-quad guard.
`field_coverage` keeps its exact signature and opens with
`let dest = field_quad(rect, field)?;`. Both call sites keep their structure —
`field.draws()` becomes `field_quad(rect, field).is_some()`.

**Both #1044 hoists stay.** The consolidation the issue asks for comes from
putting the two guards in one named function, not from moving the ask back down
into the draw. That is why this design never has to argue against #1044.

No existing sentence becomes false, which is the reason this shape was chosen
over the other. The three doc sections about the two guards move onto
`field_quad`.

**The test.** `a_frosted_node_with_an_out_of_domain_origin_draws_nothing` is in
`tests/painter.rs` and `DECODE_CALLS` is a `cfg(test)` thread-local in
`src/lib.rs`, so that test **cannot** observe a fetch. The instrument is
`a_coverage_field_that_draws_nothing_decodes_no_atlas` in `src/lib.rs`'s own test
module: widen its loop to carry an origin, and add a row with a **sound** field
at `x: f32::MAX` expecting zero fetches. Verified: that row reads 1 on `main` and
0 after, so it falsifies rather than decorates.

## The mechanism four documents get wrong — do not repeat it

`3.0e38` and `f32::MAX` origins do **not** overflow to `+inf`. Measured:

    3.0e38f32 + 8.0 == 3.0e38        -> true
    (3.0e38 + 8.0) - (3.0e38 + 0.0)  -> exactly 0.0
    f32::MAX + 8.0 == f32::MAX       -> true

It is **cancellation** — the field extent falls below one ulp of the origin, so
both ends round to the same float and the width is exactly zero. The NaN-origin
route is real and unaffected.

The overflow claim is written in four places, and **two of them are code you will
touch**: the inline comment above the device-quad guard in `field_coverage`, and
the doc plus case label of `a_frosted_node_with_an_out_of_domain_origin_draws_nothing`.
Correct both in the same commit as #1160. The other two are the bodies of #1160
and #1048, each of which already carries a correcting comment.

**It bounds what #1048 can do**, which matters if you talk to lane L: the
collapse is a *ratio* of two operands, not a property of either — origin `1e8`
against an 8-unit field admits, origin `65536.0` against a 0.001-unit field
collapses. A finiteness rule over `RectEntry::x`/`y` covers the NaN route and not
this one, so the painters' local floors stay necessary after #1048 lands.

## #1185 — decide before you code, and the doc is the defect

`crates/dashscene-skia/src/lib.rs`'s `field_coverage` doc says "`dashscene-gpu`
has no such case". Verified false for one of the two gpu pipelines:
`crates/dashscene-gpu/src/shaders/paint.wgsl`'s vertex stage builds
`lo = inst.bounds.xy + field.plane.xy`, `hi = inst.bounds.xy + field.plane.zw`,
`quad = vec4f(lo, hi - lo)`, and `msdf_sample` computes
`t = (p - quad.xy) / quad.zw`. That is the same origin-cancelled extent, divided
by, unguarded.

Two halves, and they are not the same job:

- **The doc sentence is false today** and is in your file. Correct it whatever
  else you decide.
- **Whether the lean painter takes a floor of its own** is a decision. The shader
  has `quad.zw` and could refuse a non-positive extent the way it already gates
  on `params2.w`. `crates/dashscene-gpu/src/shaders/paint.wgsl` is **lane H's
  file**, and lane H is finished — so it is free, but say in your PR that you
  took it.

Neither `VectorField::draws` nor `field_draws` can see the node origin, so this
is **not** a predicate change. Do not try to solve it there.

## #1186 — the measurement is already taken

`Painter::paint` binds `field_effect` as a frame local while `images: ImageCache`
and `msdf: MsdfCache` are `SkiaPainter` fields. `MsdfCache::effect`'s own doc
states the invariant that makes holding a compiled effect across frames sound —
the shader is a constant, so no input can stale it — and `FIELD_MASK_SKSL` is
equally a constant.

Measured at about **30 us** per `paint()` call that draws a masked node, 200
iterations against release Skia. That is larger and more frequent than the cost
#1160 names, so do not let it ride along with #1160 — it is its own commit.

## #1074 and #1045 — the two dashpaint ones

Not investigated by lane H beyond reading them. #1074 will **rewrite call sites
in `crates/dashscene-gpu/`**, which four other prompts told their lanes to expect;
those lanes are finished, so the rebase they were warned about is yours to do
rather than theirs to absorb. #1045 is the shape issue #1014 corrected, one table
over — read #1014's fix first.

## Definition of done

1. `just test` between edits; `just build` green before pushing — quote its
   Summary line, do not paraphrase.
2. **`just wasm-lint`** if you touch anything with a wasm32 half. `dashpaint` has
   one; the skia crate does not build for wasm32 and is excluded.
3. Open the PR **as an ordinary PR, never a draft**. Run `/code-review <PR#> max`
   **while CI runs**. Capture every finding as a checklist; never drop one.
   Budget for volume: this territory's PRs have returned 9 to 13 findings each,
   and on lane H's last four PRs roughly half of every round were errors in the
   author's own new prose.
4. Fix all critical findings. **Review the fix round too.**
5. File each independent minor finding as `debt` on **v0.20**.
6. Write **`Refs #<n>`**. Before opening, grep the body and the commit message
   for `(clos|fix|resolv)[a-z]* *:? *#?[0-9]` — lane H put a closing keyword in
   front of the wrong number twice in two PRs, both times in the opening
   sentence of the PR body.
7. **Before merging** — `gh pr view <n> --json files`, and
   `git diff --stat origin/main...HEAD`. The three-dot diff is what catches a
   scripted doc edit that silently did not apply: `prim fmt` re-wraps
   paragraphs, so an anchor written from memory rather than from the file will
   not match and the script will still report success. **Verify a doc edit with
   `grep`, never by the write returning.**
8. Enqueue with the mutation at the top of this prompt. Poll until the PR leaves
   `OPEN`, then `git diff` your files against `origin/main` to confirm the queue
   landed them intact.

## Do not

- Do not repeat the overflow mechanism. It is cancellation; see above.
- Do not thread `&mut ImageCache` into `field_coverage` for #1160.
- Do not fold #1186 into #1160's commit.
- Do not try to fix #1185 in `VectorField::draws` — it cannot see the origin.
- Do not merge on a green `just verify` alone. It runs no test tier.
