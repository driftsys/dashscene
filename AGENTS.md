# AGENTS.md — dashscene-staging

dashscene turns UI designed in Figma — or authored programmatically in
code — into pixels on screen, through one intermediate representation
(SCD, `.dsb`), one shared layout+text runtime, and interchangeable paint
backends (Skia reference, Unity product, a lean native painter later).

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

No implementation exists yet beyond crate stubs. The v0.1 walking
skeleton (`DESIGN_1.md` §11) is the entry point, in this order:

1. `dashbuf` — minimal schema: node tree, a fixed-size layout mode (no
   Taffy yet), a solid-fill paint kind.
2. `dashscene-core` — the in-memory arena mirroring that schema, plus
   enough of a mutation API to build a scene by hand.
3. `dashpaint` — the painter trait (boundary B) and the solid-fill
   paint table entry.
4. `dashscene-skia` — first `Painter` impl, CPU raster only (this is
   what makes goldens deterministic).
5. `dashlang` — minimal builder DSL to construct a test scene without
   hand-writing flatbuffer bytes.
6. Golden harness in `goldens/` — renders the DSL-built scene through
   the Skia painter, diffs against a checked-in PNG.

Everything else — `dashscene-engine` (Taffy, v0.2), `dashscene-typeset`
(text, v0.5/v0.6), `dashc`'s real Figma-importing behavior (v0.3
minimal, v0.7 full), `dashcue`'s animation vocabulary (v0.4),
`dashscene-validator`'s real profile enforcement, `dashscene-unity` /
`dashscene-web` (v1+) — is out of scope until its slice.

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
- Run `/code-review` on the story's diff before opening the PR, and
  capture every finding as a checklist in the PR description — never
  drop a finding silently.
- Fix all critical findings before merging. For minor findings, file
  one `debt`-labeled issue each (linked to the story) instead of
  fixing them inline.
- Merge only when CI is green, the review pass is complete, and all
  critical findings are resolved.

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
- **P5** — Figma compatibility is a property of one producer. SCD is a
  schema-first IR with its own spec; no producer's limitations define
  the format.

<!-- git-std:bootstrap -->

## Post-clone setup

Run `./bootstrap` after `git clone` or `git worktree add`.
