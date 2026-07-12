# Negative gap lowers to child margins, in a shared core pass

    status   accepted (story #10, 2026-07-12)
    scope    dashbuf schema, dashscene-core, dashscene-engine; binds
             #46 (stress corpus) and the future dashc importer

## Context

Figma auto-layout allows a negative item spacing so children overlap;
CSS and Taffy `gap` cannot go negative (DESIGN_1.md §5 lists "negative
gap → margins" as a Figma≠CSS lowering). Story #10 implements that
lowering as a step both producer paths share — the DSL/commit path
now, `dashc` when the importer enters (v0.3).

## D1 — Margin vocabulary is the lowering target

The acceptance criterion compares a negative-gap scene against "the
equivalent margin-based scene", which requires a margin vocabulary
that did not exist. Added child-side, matching padding's shape:
`dashbuf` `LayoutConstraints.margin: EdgeInsets`, `dashscene-core`
`Layout.margin` with `Prop::Margin { left, top, right, bottom }`,
`dashscene-engine` `taffy::Style::margin`.

Rejected — expressing the margin-based equivalent with authored `x`
offsets: under a flex parent authored x/y are ignored (the solver owns
placement, story #8), so a negative child margin is the only
CSS-native way to pull a flex child back over its neighbor.

## D2 — The lowering is `Txn::lower_negative_gaps` in core

For every container with mode `Horizontal`/`Vertical` and `gap < 0`:
set `gap` to `0`, and add the negative gap to the leading main-axis
margin of each child after the first (`margin.left` for `Horizontal`,
`margin.top` for `Vertical`). Positive and zero gaps are untouched;
the pass adds to an existing child margin rather than replacing it,
and is idempotent.

Placed in `dashscene-core` because it is a pure intent→intent
transform on state core owns, and core is depended on by both the DSL
path (`dashlang → core`) and the future `dashc` (`dashc → core`) — so
both share it without linking the engine or Taffy. A `Txn` method (not
a direct `Arena` mutation) so it stays in the staged-mutation contract
(P3): the rewrite publishes with the commit.

Rejected — lowering inside `TaffySolver`'s style mapping: `dashc`
emits a `.dsb` and never runs the solver, so a solver-internal
lowering could not be shared with it, and the document would carry an
un-lowered negative gap Taffy cannot solve. DESIGN §5 places these
lowerings before the CSS-shaped representation.

Rejected — automatic lowering inside `commit`/`commit_with`: makes
every commit pay for a tree scan and hides a semantic transform inside
a mechanical operation. Lowering is an explicit producer pass.

## D3 — "DSL scene" is realized through the core commit path

The acceptance criterion says "a DSL scene with negative gap." Today
that cannot mean `dashlang::Scene::build`: `dashlang` depends only on
`dashscene-core` (story #5) with no engine dependency, and its `build`
uses `commit()` (the `FixedSolver`), which ignores flex — a negative
gap would have no visible effect. Solving a flex scene needs the
engine's `TaffySolver` via `commit_with`. So the acceptance test
builds both scenes through `dashscene-core`'s `Txn` (the shared
producer surface `dashlang` is a thin skin over) and solves them with
`TaffySolver`. Giving `dashlang` a flex builder vocabulary — and
deciding how a `dashlang` scene reaches the engine solver — is a
separate concern, deliberately out of scope here.

## D4 — Corpus case

No DSL-generated corpus harness exists yet — the stress-corpus
generator is #46 (v0.8) and will be `dashlang`-generated, which
depends on D3's deferred `dashlang` flex vocabulary. The negative-gap
construct therefore lands now as the executable acceptance test in
`dashscene-engine`'s tests plus a documented corpus entry
(`corpus/dsl-generated/negative-gap.md`), so #46's generator has the
case captured. No orphan runner is created.
