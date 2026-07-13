# AGENTS.md — dashscene-staging

dashscene turns UI designed in Figma — or authored programmatically in
code — into pixels on screen, through one intermediate representation
(DSB, the `.dsb` document), one shared layout+text runtime, and
interchangeable paint backends (Skia reference, Unity product, a lean
native painter later).

**Read these two files before doing anything else in this repo:**

- `specs/DESIGN_1.md` — the seed architecture doc: goals, requirements,
  stack, document format, producers, painters, target-hardware rules,
  the v0/v1/v2 plan.
- `specs/SCOPE_DECISIONS.md` — everything decided since, in the order it
  was decided: repo strategy, the full crate-name map, the `.dsb`
  format decision, the Deno/wasm Figma importer split, Unity's deferred
  separate repo, and the driftsys house-style conventions this repo
  follows (§7).

Both are living documents. `SCOPE_DECISIONS.md` supersedes
`DESIGN_1.md` wherever the two disagree — update it, don't silently
diverge from it.

## Repo status

This is `driftsys/dashscene-staging`, a **private working repo**.
`driftsys/dashscene` itself stays public and untouched — it's reserved
as the project's future facade (docs, book, site) and holds the 12
originally-squatted crate names. Nothing here is public yet. When
there's a real version running, staging's content gets promoted into
`dashscene` — the exact mechanism (fresh push vs. history merge) is
intentionally undecided until that point (`SCOPE_DECISIONS.md` §1).

## Crates

13 crates in one Cargo workspace (`resolver = "3"`, `edition = "2024"`,
`license = "MIT"`). Full role-by-role mapping: `SCOPE_DECISIONS.md` §2.

    dashscene            umbrella / facade
    dashscene-core        semantic model — arena, node tree, layout+paint
                          tables — plus the staged-mutation producer API
                          (open/set_prop/set_variant/commit)
    dashscene-engine      Taffy solve, variants, FLIP, measure callback
    dashscene-typeset     bidi, shaping, glyph atlas pipeline
    dashscene-validator   profiles, diagnostics, waivers
    dashpaint              paint table + painter trait (boundary B)
    dashscene-skia        Skia reference painter (the whole v0 painter)
    dashcue                descriptive animation vocabulary + its runtime
                          scheduling (transitions, springs, keyframes,
                          FLIP specs) — lands at slice v0.4
    dashlang               Rust DSL skin + stress-corpus generator
    dashbuf                flatbuffer schema — the .dsb document format
    dashc                  compiler CLI; also builds to wasm32-unknown-unknown
                          for the Deno importer
    dashscene-unity        Rust-side FFI bindings only — the Unity/C# project
                          itself is a separate, not-yet-created repo
    dashscene-web          wasm/tiny-skia painter — parked

Plus `importers/figma/` (Deno/TypeScript — the Figma REST importer and
the `sharedPluginData` annotator plugin; calls `dashc.wasm` directly
rather than reimplementing lowering/validation, see
`SCOPE_DECISIONS.md` §4), `corpus/` (stress corpus + Figma fixture
captures), `goldens/` (CI golden images + diff tooling).

## Commands

    just build      assemble + full check (this is what CI runs)
    just test        cargo test --workspace
    just lint         clippy -D warnings, cargo fmt --check, dprint check, markdownlint
    just fmt          reformat everything in place
    just check        test + lint + audit
    just verify       commit-message lint over the branch range, then build — run before opening a PR
    just wasm         build dashc for wasm32-unknown-unknown
    just deno-check   just deno-test   just deno-fmt   — scoped to importers/figma/
    just book         serve the mdBook docs locally
    just install      ./bootstrap — installs git hooks, git-std, dprint, markdownlint-cli

Full recipe set: `justfile`. Conventions behind all of it — publish
order, `.git-std.toml` versioning, CI job breakdown, why dprint is
markdown-only — are in `SCOPE_DECISIONS.md` §7, sourced from
driftsys/git-std, driftsys/upskill, driftsys/markspec.

## Where to start

The v0.1 walking skeleton (`DESIGN_1.md` §11) is complete and on
`main`: the `dashbuf` schema, `dashscene-core`'s arena +
staged-mutation API, `dashpaint`'s painter trait + paint-table types,
the `dashscene-skia` CPU-raster painter, the `dashlang` builder DSL,
and the golden harness in `goldens/`. For as-built component status see
`docs/design/`; for the decisions behind it see `docs/decisions/`.

v0.2 — flex core (epic #7) is also complete and on `main`:
`dashscene-engine` solves every scene with Taffy as the sole solver,
core carries the flex vocabulary (H/V modes, hug/fill/fixed sizing,
gap/padding/alignment, min/max clamps) and the negative-gap lowering,
and four exact-match goldens pin the result.

Work now proceeds through the GitHub issues (see "Plan tracking"). The
current slice is v0.3 — basic paint + importer (epic #12): the
validator's named diagnostics, `dashc`'s minimal compile pipeline, and
the fixture-driven Deno importer.

Everything else — `dashscene-typeset` (text, v0.5/v0.6), `dashc`'s full
Figma-importing behavior (v0.7), `dashcue`'s animation vocabulary
(v0.4), `dashscene-validator`'s full profile enforcement (v0.7),
`dashscene-unity` / `dashscene-web` (v1+) — is out of scope until its
slice.

**Resolved (`SCOPE_DECISIONS.md` §9):** the staged-mutation contract
(`open`/`set_prop`/`set_variant`/`commit`) lives on the arena in
`dashscene-core` — DESIGN §4 defines it as a property of the arena, and
`commit` mechanically operates on state core owns (double buffer,
generation stamp, dirty set). `dashcue` is the descriptive animation
vocabulary and its scheduling only; the transition spec describing how
a `set_variant` animates is `dashcue` data referenced by the commit,
while the switch itself is core's. `dashlang` builds directly on
`dashscene-core`; `dashcue` doesn't enter the graph until v0.4.

## Plan tracking

The v0 plan lives as GitHub issues on this repo: one `epic`-labeled
issue and one milestone per `DESIGN_1.md` §11 slice (v0.1 … v0.9),
broken into `story`-labeled issues. Stories are split so that
independent stories can run in parallel; each story is worked in its
own git worktree, on the branch named in the story issue, and its body
lists what it depends on and what it blocks.

Story workflow — the definition of done for every story:

- `just build` green.
- Open the PR as a **draft**, then run `/code-review` on it (`--comment`
  posts the findings as inline PR comments). Capture every finding as a
  checklist in the PR description — never drop a finding silently.
- Fix all critical findings before marking the PR ready. For minor
  findings, file one `debt`-labeled issue each (linked to the story)
  instead of fixing them inline.
- Mark the PR ready for review only once CI is green, the review pass is
  complete, and all critical findings are resolved. A non-draft PR is a
  request to merge, so it must never carry an unreviewed diff
  (`docs/decisions/review-before-ready-not-before-open.md`).
- Merge only when the PR is out of draft and CI is green on the commit
  being merged. Marking a PR ready is a gate, not a promise: a later
  push, or a rebase onto a moved `main`, can turn it red again.

Merging a PR — how the branch lands on `main`:

- Shape the branch before you merge it, not at the merge button. Rebase
  onto the latest `main`, squash the branch's commits into one
  conventional commit, and force-push. The PR then carries exactly one
  commit, and it applies to `main` without conflict.
- Keep separate commits only when they are separately meaningful — for
  example a preparatory refactor and the behavior change that builds on
  it, each independently reviewable and revertable.
- Land the PR with a merge commit ("Create a merge commit"). The branch
  is already squashed, so `main` still reads as one change per PR, and
  the merge commit records which PR the change came from.
- Avoid "Rebase and merge". It replays each branch commit onto the
  current `main`, so a conflict already resolved on the branch can come
  back during the replay (this is what blocked PR #108). A merge commit
  integrates the branch as-is and does not re-raise resolved conflicts.
- All three merge methods stay enabled, and GitHub has no
  default-merge-method setting: the merge button preselects whichever
  method that person used last. Never rely on the preselection — name
  the method explicitly, `gh pr merge --merge`.

Plan revision at the end of each phase: story breakdowns for future
slices are provisional by design. When a slice's epic closes (v0.1,
v0.2, …), revise the remaining epics and stories against what was
learned before starting the next slice — update, split, merge, or
re-order the issues, and record scope-level changes in
`specs/SCOPE_DECISIONS.md`.

## Principles (DESIGN_1.md §3 — don't violate these)

- **P1** — the document carries intent, never results. No resolved
  x/y/w/h, no rasterized pixels, no glyph positions.
- **P2** — one solver, one typesetter; painters only color. A painter
  never measures, wraps, kerns, or moves anything.
- **P3** — producers mutate, the runtime owns time. Nothing
  producer-side executes inside the frame loop.
- **P4** — vocabulary is validated, never discovered. Every
  out-of-profile construct is a named diagnostic, never a silent drop.
- **P5** — Figma compatibility is a property of one producer. DSB is a
  schema-first IR with its own spec; no producer's limitations define
  the format.

<!-- git-std:bootstrap -->

## Post-clone setup

Run `./bootstrap` after `git clone` or `git worktree add`.
