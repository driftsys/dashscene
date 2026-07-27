# Negative main-axis margins rebate into the flex basis (debt #236)

    status   accepted (story #43, 2026-07-17; arithmetic revised in the
             same story's review pass, findings R1-R3; extended to Hug
             children 2026-07-27, debt #270)
    scope    crates/dashscene-engine (the style mapping)
    binds    #46 (E3's negative-gap case), every hug-sized container over
             the negative-gap lowering's output

## Context

Debt #236: a flex container sized `Hug` on its main axis solves to a
collapsed size when any child carries a negative leading margin — the
exact output of the negative-gap lowering
(`docs/decisions/negative-gap-lowering.md`). Children position
correctly; only the intrinsic (max-content) container size mis-sums.

The defect is in taffy 0.12 (`src/compute/flexbox.rs`,
`determine_container_main_size`, the `MinContent | MaxContent` branch),
verified against 0.12.1 and 0.12.2. The pass computes each item's
`content_flex_fraction` as `diff / f32_max(1.0, flex_shrink *
inner_flex_basis)` — divisor 1.0 for a shrink-0 item — and then
reconstructs the item's size with `f32_max(1.0, flex_shrink) *
inner_flex_basis * flex_fraction` — multiplier = the item's **inner**
flex basis (its flex basis minus its own padding and border) for a
shrink-0 item. The two disagree exactly when `diff < 0`, and a negative
main-axis margin is what makes `diff` (content contribution minus flex
basis) negative for a fixed-size child. The negative margin is thereby
amplified by the inner flex basis: a 56-wide child with margin-left −16
contributes 56 − 896 instead of 40, and the summed hug size collapses.
This reproduces issue #236's table exactly (margin −1 → width 56,
margin −16 → width 0). Positive margins take the `diff > 0` path, whose
two formulas agree — which is why only negative margins mis-sum.

Two more taffy facts shape the arithmetic (review findings R2/R3):
taffy floors every flex basis at the item's own padding+border sum
(`determine_flex_base_size`), and the two formulas above also agree
when the inner flex basis is exactly 1 (`max(1, 0×1) = 1` and
`max(1, 0)×1 = 1`).

## Options

1. **Fork/patch taffy** — make the two scaled-shrink formulas agree at
   the source.
2. **An engine pre-pass computing hug main sizes** — sum the children
   (margins included) outside taffy and pin the container size.
3. **Map `Fixed` children as `flex_shrink: 1` plus a min floor** —
   the formulas agree at shrink = 1.
4. **The basis rebate with a padding anchor** — for a flex-flow child
   whose main-axis sizing is `Fixed` and whose main-axis margin sum is
   negative:
   - `flex_basis = size + margin_sum` when that stays at or above the
     child's own main-axis padding sum (taffy's basis floor), else
     `flex_basis = padding_sum + 1` — the anchor at which the broken
     branch's two formulas agree exactly;
   - the main-axis `min_size` floors at the authored size, clamped by
     an authored max (finding R1), maxed with an authored min.

## Choice

Option 4. Above the padding floor, the item's content contribution
(clamped preferred size plus margins) equals its flex basis, so
`diff = 0` and the broken reconstruction is never entered. At or below
the floor, the anchored basis makes the inner flex basis exactly 1, so
the broken branch reconstructs `basis + diff = size + margin_sum` — the
correct outer contribution — for any overlap depth, including one
deeper than the child's own width (a negative contribution). In every
case the min-size floor clamps the hypothetical size back to the
authored (max-capped) size in the definite pass, so positions and sizes
away from the bug are unchanged. Verified by hand against the taffy
source and pinned by the tests below.

Why not the others:

- Option 1 carries a fork of a core dependency for one arithmetic line.
  The defect should still be reported upstream (tracked as a debt
  candidate); the rebate is removable when a fixed taffy releases.
- Option 2 re-implements intrinsic sizing outside taffy — a second
  solver in all but name (against P2's one-solver posture), and wrong
  the day taffy's evaluation order changes.
- Option 3 changes every `Fixed` child's style everywhere and leans on
  the two formulas agreeing only at shrink = 1 — a wider blast radius
  for the same effect.

## The `Hug` child: a shrink factor of 1, not a rebate (debt #270)

A `Hug`-sized child has no authored main-axis size, so there is no
constant to rebate into its flex basis — the basis taffy gives it is
its own content size, measured during the same pass that mis-sums. The
rebate above therefore cannot reach it, and story #43 declared it a
residual.

The second agreement point closes it. The broken branch divides by
`f32_max(1.0, flex_shrink * inner_flex_basis)` and multiplies back by
`f32_max(1.0, flex_shrink) * inner_flex_basis`. At `flex_shrink = 0`
those are `1` and `inner_flex_basis` — the disagreement the rebate
works around. At `flex_shrink = 1` they are
`f32_max(1.0, inner_flex_basis)` and `inner_flex_basis`, which are
equal for every inner flex basis of 1 or more. The item then
contributes exactly `flex_basis + margin_sum`, which is the arithmetic
the hug sum wants.

So a flex-flow child whose main-axis sizing is `Hug` and whose
main-axis margin sum is negative maps at `flex_shrink = 1` instead of
`0`. Two conditions keep the switch inside the pass it repairs:

- The margin sum must be negative. A positive margin takes the
  `diff > 0` path, whose two formulas already agree.
- The **parent must hug the same axis**. Taffy only enters the broken
  `MinContent | MaxContent` branch when the container's main size is
  indefinite; a container with an authored main size takes the
  `Definite` branch, where the two formulas never appear. A hugging
  container is sized to its own content sum, so the definite pass that
  follows has no negative free space for a shrink factor to act on and
  the child never actually shrinks. Under any other parent sizing the
  child keeps `flex_shrink = 0` and solves exactly as it did before.

Why not option 3's blanket `flex_shrink: 1`: story #43 rejected it for
`Fixed` children because it restyles every child everywhere. The
narrowed form here restyles only the children that reach the defect,
and only where a shrink factor is inert.

## Residual gaps, declared

- A `Hug` child whose **inner flex basis is below 1** — its content
  size minus its own padding — still mis-sums under a negative margin.
  Below 1 the divisor `f32_max(1.0, 1 * inner_flex_basis)` floors at 1
  while the multiplier stays at `inner_flex_basis`, so the two
  expressions part company again. A container whose padding leaves it
  under one document unit of content is a degenerate intent, and the
  same corner is already declared below for `Fixed` children.
- A `Hug` child under a **`Fill`-on-main parent** still mis-sums when a
  hugging ancestor measures that parent intrinsically. The switch is
  gated on the parent hugging, and a `Fill` parent's main size is
  indefinite during that measurement without its sizing saying so.
  Widening the gate to `Fill` would let the child shrink in the
  definite pass, which is a real layout change; it is left for the
  upstream fix (#269) to remove instead.
- A child whose **authored size is smaller than its own padding sum**
  renders one unit wider under a negative margin (the anchor sits at
  `padding + 1`, above the authored size). The intent is
  self-contradictory — taffy pins such a child at its padding sum
  regardless of the authored size — and the deviation is one document
  unit in that corner only.
- Under an **authored max below the authored size**, the intrinsic sum
  counts the authored size (taffy's own min-wins clamp in the
  contribution formula), while the child renders at the max. The same
  overestimate exists without any margin — a pre-existing taffy quirk
  the rebate neither adds to nor fixes; the child's rendered size is
  correct in both cases (finding R1's test pins it).

## Trace

- Satisfies: debt #236 (folded into story #43); debt #270 (the `Hug`
  child); prerequisite of #46's E3 negative-gap case; review findings
  R1, R2, R3 on PR #267.
- Verified by `crates/dashscene-engine/tests/solve.rs`:
  `a_hug_row_over_negative_child_margins_sums_like_positive_ones` (the
  issue's reproduction table),
  `a_hug_row_over_a_negative_margin_hug_child_sums_like_positive_ones`
  and `a_hug_column_over_a_negative_margin_hug_child_sums_on_the_vertical_axis`
  (#270, both axes),
  `a_negative_margin_hug_child_under_a_fixed_parent_still_never_shrinks`
  (#270's gate — the shrink factor stays out of the definite pass),
  `the_rebate_respects_an_authored_max_alongside_a_negative_margin`
  (R1), `the_rebate_survives_a_padded_childs_basis_floor` (R2 — both
  sides of taffy's padding floor),
  `a_deep_overlap_beyond_the_childs_own_width_still_sums_exactly` (R3),
  `the_rebate_respects_an_authored_min_alongside_a_negative_margin`;
  and `crates/dashc/tests/flex_lowering.rs::the_negative_gap_fixture_solves_to_figmas_captured_rects`
  (the pinned collapsed root width flipped to the Figma-captured 264).
- Related: `docs/decisions/negative-gap-lowering.md` (what authors the
  negative margins), `docs/decisions/figma-flex-lowering.md` (where the
  gap was found), `docs/decisions/v08-layout-vocabulary-shape.md` D5
  (the wrap-side refusal — a negative wrap gap has no margin encoding
  at all).
