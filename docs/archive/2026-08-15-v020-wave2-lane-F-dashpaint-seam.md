# Driver prompt — lane F: the dashpaint seam and what sits either side of it

Run this with **Opus**. Everything below marked "Verified" was checked against
the tree on 2026-08-15 with `origin/main` at `557179b`. Everything marked "the
issue claims" was not — check it yourself before acting on it.

## Setup

    git worktree add <worktrees>/wt-lane-f-seam -b debt/v020-dashpaint-seam-rest origin/main
    cd <worktrees>/wt-lane-f-seam
    ./bootstrap

This worktree pays a cold build (skia and wgpu).

## What you own

Five issues. Four are what PR #1005's own sweep and review left behind; one is
the validator rule that PR argued for and did not write.

    #1012  push_with grows five arrays before push_entry's panics       dashpaint
    #1014  push_run's u32::MAX contract checks the lengths separately   dashpaint
    #1000  skia's field_coverage divides by atlas_rect where the GPU painter refuses
    #1002  no validator rule for a .dsb's distance_range               validator
    #1001  Atlas::width and Atlas::height are pub and unchecked         dashpaint + both painters

Read each with `gh issue view <n>` before editing.

**Read `docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md`
first.** It is the record PR #1005 wrote, it is what these five are the tail of,
and it states in full what the checks do and do not cover. Three of your five
are cases that record names as uncovered.

## The ordering that matters — #1001 lands LAST

**Verified conflict.** `Atlas::width` and `Atlas::height` are read at six sites:

    crates/dashscene-gpu/src/render.rs:2394   if atlas.width == 0 || atlas.height == 0
    crates/dashscene-gpu/src/render.rs:2408   width: atlas.width,
    crates/dashscene-gpu/src/render.rs:2409   height: atlas.height,
    crates/dashscene-gpu/src/render.rs:2655   uv[2] / atlas.width as f32
    crates/dashscene-gpu/src/pack.rs:228      let height = atlas.height as f32;
    crates/dashscene-skia/src/lib.rs:1015     let height = atlas.height as f32;

Making those fields private — which is what #1001 asks for, and what PR #983 did
to `px_per_em` for the same reason — rewrites four lines of
`crates/dashscene-gpu/src/render.rs`. **Lane D owns that file and is rewriting
it heavily.**

So: do **#1012, #1014, #1000, #1002 first**, in whatever grouping suits. Hold
**#1001** until lane D has merged, then rebase onto the new `main` and do it.
If lane D is still open when everything else of yours is done, say so in your
final PR and leave #1001 on the milestone rather than forcing the conflict.

## Verified facts — do not re-derive

All line numbers `origin/main` at `557179b`.

- `PaintTable::push_with` — `crates/dashpaint/src/lib.rs:2178`.
  `push_entry` — 2241. `check_fills` — 2259. `PaintTable::span` — 1910.
  `push_run` — 2906. All in the one file, which is **yours alone this wave**.
- The production caller chain #1012 describes is real: `intern_paint` at
  `crates/dashscene-core/src/arena.rs:2906`, `intern_fill` at
  `crates/dashpaint/src/lib.rs:1820`, `compact_paints` at
  `crates/dashscene-core/src/arena.rs:2990`. If your fix reaches `arena.rs`,
  note that no other Wave 2 lane owns it — but check for an open PR before you
  assume that.
- `field_coverage` — `crates/dashscene-skia/src/lib.rs:1164`. This is #1000's
  subject and it is skia's, not the GPU painter's.
- `field_draws` and `gpu_shape` are in `crates/dashscene-gpu/src/render.rs`
  (2787 and 2677). **Read them, do not edit them** — #1000's fix belongs in
  skia, so that the reference painter stops disagreeing with the lean one. Lane
  D owns that file.
- `VECTOR_ATLAS_IMAGE_OUT_OF_RANGE = "vector.atlas-image-out-of-range"` is at
  `crates/dashscene-validator/src/lib.rs:265`. The loop #1002 wants a rule
  beside is `for (i, atlas) in vector_atlases.iter().enumerate()` at
  **`crates/dashscene-validator/src/document.rs:171`**. Tests for this area are
  in `crates/dashscene-validator/tests/vector.rs`.
- **`validate_document` has real production callers** — `crates/dashc/src/lib.rs:166`,
  `crates/dashc/src/main.rs:119`, `crates/dashscene-desktop/src/document.rs:144`,
  `crates/dashscene-desktop/src/lib.rs:124`, `crates/dashscene-ffi/src/lib.rs:322`.
  So a rule you add there **does** fire in production. Do not confuse this with
  `validate_scene`, which is a different entry point and is called from
  `crates/dashscene-validator/tests/scene.rs` and nowhere else — that one is the
  gate with no production caller, and #1002's body says so about the wrong one
  if you read it quickly.
- A new validator rule can change what `dashc` emits under `EmitPolicy::Strict`.
  **Lane E is working in `crates/dashc/`.** Your rule may break their tests and
  theirs may break yours; neither of you edits the other's files, but say in the
  PR body if you saw a `dashc` test change.

## What the record already settled — do not re-litigate

From `docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md`:

- The check sits at the **table push**, not on the type, and not as a `Result`.
  `Arena::commit` returns `u64` and is the only production caller of both
  pushes, so a `Result` would make every one of 77 call sites across six
  packages `.expect()`.
- It is spelled `if { panic!() }` rather than `assert!`, deliberately: **no test
  tier runs `--release`**, so `should_panic` cannot tell an `assert!` from a
  `debug_assert!`, and `assert!` has a debug-only spelling that could be
  weakened later. Keep that spelling for anything you add.
- A `GlyphQuad` constructor is **unavailable**, not merely declined: it needs
  private fields, and `neither_glyph_type_carries_padding` reads
  `offset_of!(GlyphQuad, glyph_id)` from `dashscene-unity`, another crate.
  Private fields delete that assertion. `VectorField` is different — a
  constructor is available there and was declined on cost. If #1001 pushes you
  toward private fields on `Atlas`, **check first whether any cross-crate
  `offset_of!` reads them**, the same way.

## What to measure rather than argue

- #1000 says skia produces an infinity where the GPU painter draws nothing.
  Build the field — sound `plane_bounds`, `atlas_rect: [0, 0, 0, 0]` — and show
  the two painters disagree before you change either. A painter divergence is
  the thing goldens exist to catch, so check whether a golden moves.
- #1014 is practically unreachable (`u32::MAX` quads is about 48 GB at 12 bytes
  each). It is filed as a contract claim the code does not honour, not as a live
  bug. **Do not build a 48 GB test.** Test `span`'s arithmetic directly, or
  restructure so the sum is what is checked.
- #1012's orphan rows persist behind an `Arc` until a later `compact_paints`
  rebuilds the table. `crates/dashscene-ffi/src/lib.rs` catches the unwind and
  returns `DsStatus::Panic`, and the host keeps the same runtime — so the
  observable is "a later commit sees rows no entry names", not a crash.
- #1001 has **no live divide-by-zero**: `resolve_frame` already skips the run,
  and skia reads `atlas.height` only in a subtraction. It is debt because the
  invariant is stated in one painter and nowhere else. Say that plainly in the
  PR body rather than implying you fixed a crash.

## Definition of done

1. `just test` between edits. `just build` green before pushing — quote its
   Summary line, do not paraphrase it.
2. `just wasm-painter` and `just wasm-lint` if your diff reaches
   `crates/dashscene-gpu/` at all (it will, for #1001's call sites).
3. **`just calibrate` if your diff touches any path in the `packer` filter.**
   The filter is defined in the `changes` job of `.github/workflows/ci.yml` and
   enumerated with a reason per entry in `docs/decisions/test-tiers.md`. Read it
   there — the list has drifted three times as a partial copy.
4. Push. **`just verify` may fail on the secrets gate for reasons that are not
   yours** — worktrees share one object store. Issue #987 is about that gate.
5. Open the PR **as an ordinary PR, never a draft**.
6. Run `/code-review` on the PR **while CI runs, not after**. Capture every
   finding as a checklist in the PR description. Never drop one silently.
   On PR #1005 the review returned ~16 findings the branch's own hand sweep had
   not — and the two worst were in the PR's **own prose**, not in the code.
7. Fix all critical findings. File each minor one as its own `debt`-labeled
   issue linked to this work, **on the v0.20 milestone**.
8. **When a critical finding changes the implementation, review the fix too.**
9. In prose and commit messages write **`Refs #<n>`**. A closing keyword fires
   from commit messages that land on `main`, matches mid-sentence, takes only
   the first number, and a negated sentence matches just as well as a positive
   one.
10. Before merging: `gh issue list --milestone "v0.20 — hardening: the critical
    findings and the Android recovery path" --state open` and read it.
11. Rebase onto the latest `main`, squash to one conventional commit,
    force-push, wait for `ci` green **on the commit you are merging**, then
    `gh pr merge --merge`. Merging is strictly serial.
12. After merging, `gh issue view <n> --json state` for every issue your commits
    named, not only those in the PR body.

## Do not

- **Do not edit `crates/dashscene-gpu/src/render.rs` for anything but #1001's
  mechanical call sites, and not until lane D has merged.**
- Do not edit `crates/dashc/` — lane E owns it.
- Do not restate what the seam record already says. If your change makes any
  sentence in `docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md`
  false, **edit that record in the same PR** — a behaviour change falsifies the
  records that describe the old behaviour, and eleven of PR #1009's fifteen
  findings were exactly that.
- Do not merge on a green `just verify` alone. It runs no test tier.
