# Negative gap lowers to child margins, in a shared core pass

    status   accepted (story #10, 2026-07-12)
    scope    dashbuf schema, dashscene-core, dashscene-engine; binds
             #46 (stress corpus) and the future dashc importer

## Context

Figma auto-layout allows a negative item spacing so children overlap;
CSS `gap` cannot go negative (`docs/design/dashbuf.md` lists "negative gap →
margins" as a Figma≠CSS lowering). Story #10 implements that lowering
as a step both producer paths share — the DSL/commit path now, `dashc`
when the importer enters (v0.3).

The lowering exists to keep the **document** CSS-representable, not to
make the scene solvable. Taffy is not the constraint: it takes `gap` as
a raw length and applies a negative one arithmetically, so an un-lowered
negative-gap scene solves to the same rects as the lowered one. Measured
2026-07-12 across a horizontal row of fixed children, a vertical column
of fixed children, and a horizontal row of `Fill` children — in the
`Fill` case the negative gap returns its absolute value to free space,
the same behavior as the lowered margins (see "Known property" below).

Nothing may depend on that. It is outside CSS `gap` semantics, which
forbid a negative value; Taffy simply does not validate it. `dashc`
emits a `.dsb` that never reaches Taffy, and P5 makes the dashscene document a
schema-first IR whose vocabulary is the lowering target — so the negative gap
must be gone by the time the document is written, whatever any one solver
happens to tolerate.

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
un-lowered negative gap that no CSS-shaped consumer may be asked to
read. `docs/design/architecture.md` places these lowerings before the
CSS-shaped representation.

Rejected — automatic lowering inside `commit`/`commit_with`: makes
every commit pay for a tree scan and hides a semantic transform inside
a mechanical operation. Lowering is an explicit producer pass.

**Where the lowering suite lives (revisit trigger).**
`docs/design/dashbuf.md` and
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md` place all four
Figma≠CSS lowerings (negative gap, canvas stacking, strokes-in-layout,
scale-to-inset) in the compiler (`dashc`). Negative gap is the first to be
implemented; the others are importer scope (v0.3+). It lands as a
single `dashscene-core` `Txn` method — the shared building block
`dashc` calls — rather than a dedicated lowering module, because
abstracting a lowering _suite_ around one member would be premature.
When the second lowering lands, revisit: if lowerings accumulate as
core `Txn` methods, extract them into a dedicated lowering module (or
a `dashc`-side pass that reuses these primitives) so the runtime
arena's staged-mutation API does not accrete a compiler pass suite.
Recorded here so that revisit is deliberate, not forgotten.

**Trigger checked at story #139; it did not fire.** The Figma lowering
asked whether `lower_negative_gaps` should move into a `dashc` lowering
module, and it stays in core's `Txn`. The trigger fires on auto-layout,
and the v0.3 lowering refuses auto-layout outright — `Document` cannot
express flex in the first place (debt #140), so there is no second
lowering yet and no negative gap to lower. Moving the pass now would be
speculative: it operates on the arena, while a document-side pass would
have to operate on `Document`. Revisit again when the flex lowering lands.
See `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`.

**Trigger re-checked at story #140 (2026-07-16); the second site exists
and the pass still does not move.** The flex lowering applies the same
rewrite inside `dashc`'s walk — the walk builds a `Document`, not an
arena, so core's `Txn` method cannot serve it, and the rewrite needs only
the sibling order the walk already has (a dedicated lowering module would
add a second tree pass for one rule). The two sites share the rule by
statement, not by code: gap to zero, the gap onto the leading main-axis
margin of each in-flow child after the first. Extract a shared module only
if a third site appears. See `docs/decisions/figma-flex-lowering.md` D3.

**Known property (CSS margin semantics).** The lowered margins behave
as CSS/Taffy margins: a negative margin on a `Fill` child returns its
absolute value to the container's free space, so `Fill` siblings grow.
The acceptance criterion (a negative-gap scene equals the equivalent
margin-based scene) holds by construction — both sides use the same
margins — but whether this matches Figma's own negative-gap behavior
for `Fill` children is a fidelity question with no real Figma file to
check against at v0.2. Verifying it against a captured fixture is
deferred to the importer slice (tracked as a `debt` issue). For fixed
and hug children the overlap is exactly the authored gap.

**Fidelity verified at story #140 (debt #105), with two findings.**
For fixed-size children the lowering is exact: the captured
`lowering-negative-gap.json` (five 56-wide children, `itemSpacing: -16`),
lowered and solved through the engine, lands every child on Figma's own
`absoluteBoundingBox`
(`crates/dashc/tests/flex_lowering.rs::the_negative_gap_fixture_solves_to_figmas_captured_rects`).
The `Fill`-children variant stays unverifiable: capturing it needs a
manual authoring step in the fixture-author plugin, and no captured
fixture carries `Fill` under a negative gap — the question above stays
open. The verification also surfaced a runtime gap this lowering's
output exposes: Taffy 0.12's intrinsic (hug) sizing mis-sums children
with negative margins, so a hug-sized container over a lowered negative
gap solves to a collapsed main-axis size (engine debt #236).

**Where the lowering is verified.** At the intent level, in
`dashscene-core`'s arena tests — the rewrite (gap zeroed, leading
margins added, at every depth), idempotence, NaN, and additivity. It
cannot be verified through solved rects: because Taffy applies a raw
negative gap (see Context), a rect-level test passes whether or not the
pass ran. `dashscene-engine`'s `lowered_margins_compose_through_nesting`
therefore pins the other half — that the lowering's _output_ (nested
negative margins) is faithfully solved — and says so in its own comment.
Any future rect-level assertion about this lowering must not be read as
evidence that the lowering ran.

**Margin is flex-flow vocabulary.** It applies only to a child in a
flex (`Horizontal`/`Vertical`) parent's flow. A margin authored on a
root, or on a child of a mode-`None` (passthrough) parent, is inert —
placement there is the authored offset — so the `TaffySolver` agrees
with `commit()`'s fixed resolution (which ignores margin), preserving
the mode-`None` equivalence guarantee (story #9).

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

Story #11 (the v0.2 flex goldens) hit the same gap and authored its
four scenes the same way — directly against `Txn`, solved with
`TaffySolver` — rather than through `dashlang`. It filed #118 to
build the deferred `dashlang` flex vocabulary and a
`Scene::build_with(arena, solver)` entry point; #46 (the
DSL-generated stress corpus) depends on #118.

**Resolved by #118 (2026-07-15).** `dashlang` now has the v0.2 flex
vocabulary and `Scene::build_with`; the four `v02_flex.rs` goldens
gained a DSL-built assertion alongside their existing hand-built one.
See `docs/decisions/dashlang-flex-vocabulary.md`.

## D4 — Corpus case

No DSL-generated corpus harness exists yet — the stress-corpus
generator is #46 (v0.8) and will be `dashlang`-generated, which
depends on D3's deferred `dashlang` flex vocabulary. The negative-gap
construct therefore lands now as the executable acceptance test in
`dashscene-engine`'s tests plus a documented corpus entry
(`corpus/dsl-generated/negative-gap.md`), so #46's generator has the
case captured. No orphan runner is created.
