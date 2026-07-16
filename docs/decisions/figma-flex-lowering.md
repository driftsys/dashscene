# The Figma flex lowering carries per-axis intent and refuses what diverges

    status   accepted (story #140, 2026-07-16)
    scope    crates/dashc (the figma module and the document model)
    binds    #37/#159/#160 (every later widening walks this walk), the v0.8
             wrap/grid/baseline work, and debt #236

## Context

Story #140 lowers Figma auto-layout (`layoutMode: HORIZONTAL`/`VERTICAL`)
into the flex vocabulary the `.dsb` schema has carried since v0.2
(`docs/decisions/flex-vocabulary-shape.md`). The schema needed no change:
`dashc`'s document model gains mirrors of the two optional tables
(`LayoutContainer`, `LayoutConstraints`), and `emit` writes them — absent
stays absent, so a fixed-layout document emits byte-identically to before
(the frozen goldens hold, R7).

The constraint that shapes everything here is
`docs/decisions/figma-auto-layout-refused-on-two-grounds.md`'s reason two:
inside an auto-layout frame, `absoluteBoundingBox` is Figma's solver output,
never authored intent.

## D1 — `layoutSizingHorizontal`/`layoutSizingVertical` are the sizing source

Figma encodes sizing twice: the modern per-node, per-axis
`layoutSizingHorizontal`/`layoutSizingVertical` (`FIXED`/`HUG`/`FILL`) and
the older container-side `primaryAxisSizingMode`/`counterAxisSizingMode`
plus child-side `layoutGrow`/`layoutAlign`. Every captured fixture carries
both, and the modern pair is exactly `AxisSizing` per axis — no
axis-relative reshuffling. The lowering reads the modern pair only; the
older encoding carries no information the modern one does not, so leaving
it unread is not a drop.

## D2 — What is not intent lowers as zero, per axis

A `Fixed` axis's extent is authored: it lowers from the captured box. A
`Hug` or `Fill` axis's extent, and a flex child's x/y, are solver output:
they lower as `0.0` — the value the runtime solver ignores — never as the
captured numbers, which would render correctly at exactly one size (P1).
An authored offset still lowers under a mode-`None` parent, where placement
is the offset (the v0.3 behavior, unchanged).

## D3 — The negative-gap lowering runs in the walk

`docs/decisions/negative-gap-lowering.md` requires the document to carry no
negative gap. `dashc` cannot reuse core's `Txn::lower_negative_gaps` — the
walk builds a `Document`, not an arena — so the walk applies the same
rewrite at the source: gap to zero, the gap onto the leading main-axis
margin (`left` in a row, `top` in a column) of every in-flow child after
the first. The rewrite lives in the one pass that also knows sibling order,
so no second tree walk exists. See that record's "revisit trigger" note for
why this is a second site rather than a shared module.

## D4 — `SPACE_BETWEEN` zeroes the authored gap

Figma ignores `itemSpacing` under `primaryAxisAlignItems: SPACE_BETWEEN` —
the solver owns the spacing — while CSS adds `gap` to the distributed
space. The two disagree, so the authored value lowers as zero and only the
alignment carries. No capture pins `SPACE_BETWEEN`; the value set is
Figma's documented enum, and the synthetic test states so.

## D5 — What diverges or cannot be solved is refused by name

Refusals are error diagnostics
(`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`), and
each names its construct:

- **`GRID`**, **`layoutWrap: WRAP`**, **`counterAxisAlignItems: BASELINE`**
  — the runtime solves none of them until v0.8 (`docs/roadmap.md`, layout
  fidelity; the schema's enum members append there). Lowering grid or wrap
  onto a single flex line, or baseline onto `Start`, would move every
  child. The refusing node's subtree is skipped: its children's boxes are
  that solver's output (P1).
- **A `Fill` child on an axis its parent hugs** — Figma resolves the
  sizing cycle from the child's stored size (solver state P1 forbids
  reading); a CSS solve derives the hug from content. The two render
  different pictures, so the construct is refused rather than solved
  divergently. Pinned by `variables-bound.json`, whose `Fill` cards sit in
  a hug-width root.
- **`layoutPositioning: ABSOLUTE`**, **`strokesIncludedInLayout: true`**,
  **`itemReverseZIndex: true`** — an out-of-flow child, layout-consuming
  strokes, and reversed paint order have no vocabulary; treating any of
  them as the default reflows or repaints siblings silently.

## Known runtime gap this lowering exposed (debt #236)

The lowering's negative-gap output is correct — the derived
`lowering-negative-gap` fixture's children solve to Figma's captured boxes
exactly — but Taffy 0.12's intrinsic (hug) sizing mis-sums children with
negative margins, so a hug-sized container over a lowered negative gap
solves to a collapsed main-axis size. Filed as engine debt #236; the
fidelity test pins the wrong value so the fix is loud.

## Trace

- Satisfies: issue #140 (auto-layout scope; grid remains refused —
  reported at the story, resolved by the v0.8 slice), R2 via
  `docs/decisions/flex-vocabulary-shape.md`, P1/P4.
- Verified by: `crates/dashc/tests/flex_lowering.rs` (fixture lowering,
  Figma-captured-rect fidelity, refusals, goldens),
  `crates/dashc/tests/round_trip.rs::flex_intent_round_trips_through_the_document`.
- Related: `docs/decisions/figma-auto-layout-refused-on-two-grounds.md`,
  `docs/decisions/negative-gap-lowering.md`,
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`.
