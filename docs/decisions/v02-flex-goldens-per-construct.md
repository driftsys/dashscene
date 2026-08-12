# v0.2 flex goldens are one per construct, closing epic #7

    status   accepted (story #11, 2026-07-13)
    scope    goldens/; epic #7 (v0.2 flex core) — closes the epic

## Context

Story #11 is epic #7's last open story: pin the v0.2 flex vocabulary — H/V
nesting, per-axis sizing (hug/fill), min/max clamps, main/cross alignment — with
golden images, plus `docs/specification/05-qualification.md`'s E3, one
v0.2-reachable stress-corpus case (hug-in-fill). Two facts about the as-built
code shaped the choice:

- `dashlang` has no flex vocabulary (`docs/decisions/negative-gap-lowering.md`
  D3): its builder exposes `at`/`size`/`fill`/`child` only, and `Scene::build`
  commits through the fixed solver, which ignores flex. Story #11 confirmed the
  deferral still holds and filed #118 (dashlang flex builder vocabulary +
  `Scene::build_with`) against it; #46 (the DSL-generated corpus generator)
  depends on #118. **Resolved 2026-07-15**: #118 added the vocabulary and
  `Scene::build_with`, and each of this story's four tests gained a DSL-built
  assertion alongside its hand-built one
  (`docs/decisions/dashlang-flex-vocabulary.md`) — the per-construct golden
  split this record chose is unchanged.
- Core's `AxisSizing::{Fixed, Hug, Fill}` carries no fill weight, and
  `dashscene-engine` maps every `Fill` to `flex_grow = 1.0`, so `Fill` siblings
  always split free space equally. The epic's scope list names "fill weights",
  but no such vocabulary exists anywhere to golden. Filed #117 to decide whether
  the dashscene document needs one at all — Figma's own auto-layout has no flex
  weight either.

## Options

1. One golden per construct: four scenes (nesting, sizing, clamping, alignment),
   each failing in isolation.
2. One combined scene covering all four constructs.
3. Combined plus per-construct — both 1 and 2.

## Choice

Option 1: `goldens/tooling/tests/v02_flex.rs`'s four tests —
`nesting_matches_its_golden`, `sizing_matches_its_golden`,
`clamping_matches_its_golden`, `alignment_matches_its_golden` — each with its
own checked-in PNG
(`goldens/images/v02-{nesting,sizing,clamping,alignment}.png`), authored against
`dashscene-core`'s `Txn` and solved with `dashscene-engine`'s `TaffySolver` via
`commit_with`, painted by the same `SkiaPainter` path every existing golden
uses.

## Why

- A combined scene (option 2) fails one opaque image without saying which
  construct broke, and adding a construct later rewrites the whole golden —
  against `docs/technotes/rendering-and-painters.md`'s bisect-by-construction.
- Combined-plus-per-construct (option 3) is the shape v0.3 landed (story #14's
  combined golden, then story #18's per-family set), and the two overlapped
  heavily enough that #18 needed a mid-story rescope. Story #11 does not repeat
  that.
- Every scene is dimensioned so each solved rect lands on an integer:
  integer-aligned solid fills carry no anti-aliased edges, so all four goldens
  use the exact-match `assert_matches_golden` rather than a tolerance (extends
  `docs/decisions/golden-comparison-space.md`). This binds future flex goldens
  too: a construct that cannot be made integral changes the scene's dimensions,
  not the comparison function.
- Each test asserts the solved rects, read from the committed scene, before
  comparing the image, so a solver regression fails with an arithmetic message
  rather than "pixels differ"; the golden then proves only that the paint path
  renders what the solver produced.
- Fill weights and dashlang's flex vocabulary stay out of scope, each with its
  own filed issue rather than a silent drop (#117, #118) — see the Context
  above.

## Corpus case

`docs/specification/05-qualification.md`'s E3 names wrap, hug-in-fill, grid
spans, bidi, and variant topology as the stress-corpus edge cases; of those,
only hug-in-fill is reachable at v0.2 (wrap and grid spans are v0.8, bidi is
v0.6, variant topology is v0.4). It lands as
`corpus/dsl-generated/hug-in-fill.md`, with `sizing_matches_its_golden` as its
executable proof — the same shape as the existing `negative-gap.md` entry. The
other three goldened constructs (nesting, clamping, and the rest of alignment)
are ordinary v0.2 vocabulary, not E3 edge cases, so they stay golden-only and
are not also filed as corpus entries.

## Trace

- Satisfies: issue #11 acceptance criteria.
- Closes epic #7's story list (v0.2 flex core) — story #11 was its last open
  story.
- Related decisions: `docs/decisions/flex-vocabulary-shape.md` (the vocabulary
  these goldens pin); `docs/decisions/negative-gap-lowering.md` D3 (the dashlang
  deferral, now #118); `docs/decisions/golden-comparison-space.md` (the
  exact-match extension); `docs/decisions/v03-paint-goldens-per-family.md` (the
  v0.3 granularity precedent this story chose not to repeat).
