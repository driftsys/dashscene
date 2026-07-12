# Design: flex goldens + corpus cases (story #11)

    story    #11 — flex goldens + corpus cases
    epic     #7 — v0.2 flex core (this is the epic's last open story)
    branch   story/flex-goldens
    depends  #9 (Taffy solve, merged), #6 (golden harness, merged)

## Goal

Pin the v0.2 flex vocabulary with golden images, and record the one v0.2
stress-corpus case DESIGN_1.md §11 E3 names. This is a test-only story:
no new public API, no schema change, no change to any crate's behavior.

## Context

Two facts about the as-built code shape this design.

**dashlang cannot author a flex scene.** Its builder exposes
`at`/`size`/`fill`/`child` only, and `Scene::build` calls `commit()` —
the `FixedSolver`, which ignores flex. Every existing golden renders
through that path. `docs/decisions/negative-gap-lowering.md` D3 (the
"negative-gap D3" below) recorded this and deferred it: giving dashlang a
flex vocabulary, and deciding how a dashlang scene reaches the engine
solver, is a separate concern. This story does not resolve it either —
see D1's rejected alternatives.

**`Fill` has no weights.** Core's vocabulary is
`AxisSizing::{Fixed, Hug, Fill}`, and `dashscene-engine` maps `Fill` to
`flex_grow = 1.0` with no authored weight anywhere in core, `dashbuf`,
or the engine. `Fill` siblings therefore always split free space
equally. The epic's scope list names "fill weights", but no such
vocabulary exists to golden.

## Scope

Four golden scenes and one corpus case:

- `goldens/tooling/tests/v02_flex.rs` — the four scenes, following
  `v03_families.rs`'s per-family shape.
- `goldens/tooling/Cargo.toml` — add `dashscene-engine` as a
  dev-dependency. The goldens crate has never needed it: nothing painted
  a flex-solved scene before.
- `goldens/images/v02-nesting.png`, `v02-sizing.png`,
  `v02-clamping.png`, `v02-alignment.png`.
- `corpus/dsl-generated/hug-in-fill.md`, plus its entry in that
  directory's README "Cases" list.

Out of scope, tracked as new issues rather than absorbed: authored fill
weights, and dashlang's flex builder vocabulary (negative-gap D3).

## D1 — Scenes are authored against core's `Txn`

Each scene is built with `Arena::open` / `add_node` / `set_prop` and
solved with `commit_with(&mut TaffySolver::new())`, then painted by
`SkiaPainter` — the same painter path every existing golden uses. The
goldens crate is the producer, exactly as `dashscene-engine`'s
`tests/solve.rs` already does.

Rejected — extending dashlang with the v0.2 flex props plus a
`Scene::build_with(arena, &mut dyn LayoutSolver)`: `LayoutSolver` is a
core trait, so this would keep dashlang engine-free and would settle
negative-gap D3. It is the larger change, and it adds public API to a
crate this story does not otherwise touch. #11's acceptance criteria are
about goldens being green, not about the DSL. Deferred to the story filed
for negative-gap D3, which is where corpus generator #46 needs it anyway.

Rejected — making dashlang depend on `dashscene-engine` so `build()`
solves with Taffy internally: breaks story #5's constraint that dashlang
depends only on `dashscene-core`, and forces Taffy on every dashlang
consumer.

## D2 — One golden per construct, not one combined scene

Four scenes, four images, each failing in isolation so a regression names
its own construct — DESIGN_1.md §8's bisect-by-construction.

Rejected — a single combined `v02-flex` scene: a solver regression fails
one opaque image without saying which construct broke, and adding a
construct later rewrites the whole golden.

Rejected — combined _and_ per-construct: this is the shape v0.3 landed
(#14's combined golden, then #18's per-family ones). The two overlapped
heavily enough that #18 had to be rescoped mid-story, because the
combined golden asserted nothing the isolated ones did not.

## D3 — Every scene solves to integer rects, so goldens are exact-match

The painter has anti-aliasing on (story #14, which closed the sub-pixel
policy issue #85 by turning it on). A fractional rect edge therefore
produces AA coverage pixels, which are deterministic per skia version but
not bit-identical across CPU architectures — that is why the v0.3 goldens
need `assert_matches_golden_within` with a 1–2 % tolerance.

Integer-aligned solid fills produce no AA. Every scene below is
dimensioned so that each solved rect lands on an integer, which lets all
four goldens use `assert_matches_golden` — exact, zero tolerance. That
is the strongest form of the assertion and leaves no tolerance budget to
erode as the suite grows.

This is a constraint on the scenes, not a discovered property: if a
future construct cannot be made integral, it uses
`assert_matches_golden_within` with the reason recorded at the call site.

## D4 — The scenes

### `v02-nesting` — 120×80

Root `Horizontal`, gap 10, padding 5 on every edge. Two `Vertical`
columns of 50×70, each holding two 50×30 children with gap 10.

    root (Horizontal, gap 10, padding 5, 120×80)
      ├── col-a (Vertical, gap 10, 50×70)
      │     ├── a0 (50×30)
      │     └── a1 (50×30)
      └── col-b (Vertical, gap 10, 50×70)
            ├── b0 (50×30)
            └── b1 (50×30)

Content fits exactly: 50 + 10 + 50 = 110 = 120 − (5 + 5), and
30 + 10 + 30 = 70 = 80 − (5 + 5). Pins H-inside-V nesting, and that gap
and padding compose down a level.

### `v02-sizing` — 120×60

Root `Horizontal`, gap 0, no padding. A `Hug` node followed by two `Fill`
siblings.

    root (Horizontal, 120×60)
      ├── hug (SizingH Hug, Horizontal, height 60)
      │     └── inner (30×60 fixed — determines hug's width)
      ├── fill-a (SizingH Fill, height 60)
      └── fill-b (SizingH Fill, height 60)

`hug` resolves to 30 wide from its content. Free space is 120 − 30 = 90,
split equally between the two `Fill` siblings: 45 each. Solved x: 0, 30,
75. This is the hug-in-fill corpus case and the equal-`Fill`-split case
in one image.

### `v02-clamping` — 120×60

Root `Vertical`, gap 0, two `Horizontal` rows of 120×30.

    root (Vertical, 120×60)
      ├── row-max (Horizontal, 120×30)
      │     ├── capped (SizingH Fill, MaxWidth 40)   → 40
      │     └── rest   (SizingH Fill)                → 80
      └── row-min (Horizontal, 120×30)
            ├── floored (SizingH Fill, MinWidth 100) → 100
            └── rest    (SizingH Fill)               → 20

Both rows would split 60/60 unclamped. Pins that a clamp beats the flex
distribution in both directions.

### `v02-alignment` — 160×80

Root `Vertical`, gap 0, four `Horizontal` rows of 160×20, each holding
two 30×10 children with gap 10.

    row 0   main Start          cross Start    padding (10, 2, 10, 2)
    row 1   main Center         cross Center
    row 2   main End            cross End
    row 3   main SpaceBetween   cross Center

Covers all four `MainAxisAlign` variants and all three `CrossAxisAlign`
variants. In the unpadded rows, main-axis free space is
160 − (30 + 10 + 30) = 90 and cross-axis free space is 20 − 10 = 10, so
every centered offset is a whole number (45 and 5). In row 0, padding
leaves 160 − 20 − 70 = 70 free with the content starting at x = 10.

## D5 — Each test asserts twice

1. **Solved rects**, read from the `CommittedScene` before painting. A
   solver regression then fails with an arithmetic message ("expected
   x = 65, got x = 70") rather than "pixels differ".
2. **The golden**, exact-match, as the pixel proof that the paint path
   renders what the solver produced.

This follows `v01.rs`, which pins its key properties (stacking order,
paint dedup) independently of the image file. The rect assertions make
the tests debuggable; the goldens make them regression-proof against the
painter.

## D6 — The corpus case is hug-in-fill, and only hug-in-fill

`corpus/dsl-generated/hug-in-fill.md`, shaped like the existing
`negative-gap.md`: the scene, the expected solved rects, and the
executable proof (the `v02-sizing` golden test).

DESIGN_1.md §11 E3 defines the stress corpus as edge cases — "wrap,
hug-in-fill, grid spans, bidi, variant-topology" — and hug-in-fill is the
only one v0.2 reaches. Wrap and grid spans are v0.8, bidi is v0.6,
variant topology is v0.4.

Rejected — a corpus entry per goldened construct: nesting, clamping, and
alignment are ordinary vocabulary rather than the edge cases E3
enumerates, so the corpus would start duplicating the golden set.

## Acceptance criteria

- The four goldens are committed and green on a clean checkout
  (`cargo test -p goldens`), each via exact-match `assert_matches_golden`.
- Each test asserts its solved rects before comparing the image.
- `corpus/dsl-generated/hug-in-fill.md` exists and its README lists it.
- `just build` green.
- Two issues filed: authored fill weights; dashlang flex vocabulary +
  `build_with` (negative-gap D3), noted as a dependency of corpus
  generator #46.

## Alternatives considered

Recorded inline under D1, D2, and D6.
