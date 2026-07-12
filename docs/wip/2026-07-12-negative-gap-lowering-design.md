# negative-gap lowering (Figma≠CSS) — design

    story    #10 (epic #7, v0.2 flex core)
    branch   story/negative-gap-lowering
    date     2026-07-12
    status   working memory — garden before the PR lands

## Goal

Lower negative gap to margins (DESIGN_1.md §5 — a Figma≠CSS lowering).
Figma auto-layout allows a negative item spacing (children overlap);
CSS/Taffy `gap` cannot go negative. The lowering rewrites a negative
container gap into per-child margins that Taffy solves to the same
overlap. Implemented as a shared step both producer paths use: the
DSL/commit path now, `dashc` when the importer enters (v0.3).

Acceptance (issue #10): a scene with negative gap produces the same
rect table as the equivalent margin-based scene; `just build` green;
a corpus case for the negative-gap construct.

## Decisions (alternatives considered)

### D1 — Margin vocabulary (the lowering target)

The "equivalent margin-based scene" the acceptance criterion compares
against is not expressible today — there is no margin vocabulary. Add
it, child-side, matching padding's shape:

- `dashbuf`: `margin: EdgeInsets` on `LayoutConstraints` (appended
  after `max_height`; additive, R7).
- `dashscene-core`: `Layout.margin: EdgeInsets`, and one granular
  `Prop::Margin { left, top, right, bottom }` (the staged API's grain
  — one property per call).
- `dashscene-engine`: `Layout.margin` maps to `taffy::Style::margin`
  (a `Rect<LengthPercentageAuto>`; negative margins are legal in CSS
  and Taffy, which is exactly why they express overlap).

Rejected — expressing the margin-based equivalent with authored `x`
offsets (`Prop::X`): under a flex parent authored x/y are ignored (the
solver owns placement, story #8), so the equivalent scene is
inexpressible without a real margin. Margin is the only CSS-native way
to pull a flex child back over its neighbor.

### D2 — The lowering: `Txn::lower_negative_gaps`

- **Chosen:** a staged operation on the arena's layout intent
  (`dashscene-core`). For every container node whose mode is
  `Horizontal`/`Vertical` and whose `gap` is negative: set its `gap`
  to `0`, and add the (negative) gap to the leading main-axis margin
  of each child after the first in document order — `margin.left` for
  `Horizontal`, `margin.top` for `Vertical`. Positive and zero gaps
  are untouched (positive gap is CSS-native). Adding to (not
  replacing) the child's margin keeps an author's own margin intact.
  The pass is idempotent: after it runs no negative gaps remain, so a
  second run is a no-op.
- Placed in `dashscene-core` because it is a pure intent→intent
  transform on state core owns, and core is depended on by both the
  DSL path (`dashlang → core`) and the future `dashc` (`dashc →
  core`) — so both share it without linking the engine or Taffy. It
  is a `Txn` method (not a direct `Arena` mutation) so it stays inside
  the staged-mutation contract (P3): the rewrite publishes with the
  commit like any other staged change.
- Rejected — lowering inside `TaffySolver`'s style mapping (transient,
  never stored): `dashc` emits a `.dsb` document and never runs the
  solver, so a solver-internal lowering could not be shared with it,
  and the document would carry an un-lowered negative gap Taffy cannot
  solve. DESIGN §5 places these lowerings in the compiler, before the
  CSS-shaped representation.
- Rejected — automatic lowering inside `commit`/`commit_with`: makes
  every commit pay for a tree scan, and hides a semantic transform
  inside a mechanical operation. Lowering is an explicit producer pass
  (DESIGN §5: "lowerings happen in scdc"), so the producer calls it
  deliberately.

### D3 — "DSL scene" is realized through the core commit path

The acceptance criterion says "a DSL scene with negative gap." Today
that phrase cannot mean `dashlang::Scene::build`: `dashlang` depends
only on `dashscene-core` (story #5) with no engine dependency, and its
`build` uses `commit()` (the `FixedSolver`), which ignores flex
entirely — a negative gap would have no visible effect. Solving a flex
scene needs the engine's `TaffySolver` via `commit_with`.

So the acceptance test builds both scenes through `dashscene-core`'s
`Txn` (the shared producer/commit surface `dashlang` is a thin skin
over) and solves them with `TaffySolver`. Giving `dashlang` a flex
builder vocabulary — and deciding how a `dashlang` scene reaches the
engine solver — is a separate concern (a future `dashlang`-flex
story), deliberately out of scope here.

### D4 — Corpus case

No DSL-generated corpus harness exists yet — the stress-corpus
generator is #46 (v0.8), and it will be `dashlang`-generated, which
depends on D3's deferred `dashlang` flex vocabulary. The negative-gap
construct therefore lands now as (a) the executable acceptance test in
`dashscene-engine`'s tests, and (b) a documented corpus entry under
`corpus/dsl-generated/` describing the scene and expected overlap, so
#46's generator has the case captured. No orphan runner is created.

## File impact

    crates/dashbuf/schema/dashbuf.fbs        margin on LayoutConstraints
    crates/dashbuf/tests/roundtrip.rs        margin round-trips
    crates/dashscene-core/src/arena.rs       Layout.margin, Prop::Margin,
                                             Txn::lower_negative_gaps
    crates/dashscene-core/src/lib.rs         re-exports (unchanged set)
    crates/dashscene-core/tests/arena.rs     margin prop; lowering unit
    crates/dashscene-engine/src/lib.rs       margin -> taffy Style
    crates/dashscene-engine/tests/solve.rs   acceptance (A == B)
    corpus/dsl-generated/README.md           corpus purpose + the case
    corpus/dsl-generated/negative-gap.md     the documented case

## Testing

1. dashbuf: a `LayoutConstraints` carrying `margin (1,2,3,4)` round-
   trips; an absent margin reads back zero insets.
2. core: `Prop::Margin` sets `Layout.margin` and reads back via
   `Arena::layout`; default is zero insets.
3. core (lowering unit, on intent): a Horizontal container gap −8 with
   three children lowers to gap 0, child[0].margin unchanged,
   child[1].margin.left and child[2].margin.left each −8; a Vertical
   container lowers on `margin.top`; a positive gap is untouched; a
   pre-existing child margin is added to, not replaced; the pass is
   idempotent.
4. engine acceptance (A == B): scene A (container gap −8, three fixed
   children) lowered then solved equals scene B (container gap 0,
   children[1..] `margin.left −8`) solved — identical rect tables,
   with the expected 8-unit overlaps.
5. engine: a Vertical negative-gap column overlaps on the main axis;
   authored margins still solve without the lowering (margin is a
   real, standalone vocabulary, not only a lowering artifact).
