# Corpus case: baseline

    construct  baseline cross-axis alignment in a horizontal row
    exercised  crates/dashlang/tests/corpus.rs (baseline_aligns_mixed_height_boxes_on_their_bottoms,
               baseline_propagates_from_a_nested_row)
    golden     goldens/tooling/tests/v08_fidelity.rs (v08-baseline, hand-built #43)

## The scene

A fixed 140x60 row, gap 10, baseline-aligned, holding three mixed-height
fixed boxes:

    row (mode Horizontal, 140x60, gap 10, cross align Baseline)
      ├── short  (fixed 30x20)
      ├── tall   (fixed 40x48)
      └── middle (fixed 30x32)

## Expected solved rects

    row:    (0, 0, 140, 60)
    short:  (0, 28, 30, 20)      48 − 20
    tall:   (40, 0, 40, 48)      the tallest, baseline 48
    middle: (90, 16, 30, 32)     48 − 32

## Why it is an edge case

A leaf's baseline is its bottom edge — the measure seam carries no glyph
baseline (Q-4, `docs/decisions/v08-layout-vocabulary-shape.md` D5, debt
issue #273). The three boxes align their bottoms at the tallest child's
baseline (48), so each sits at y = 48 − its own height. In a vertical
container the
Baseline keyword degrades to start alignment, pinned separately in the
engine's `baseline_in_a_vertical_container_degrades_to_start`.

## Nested-row propagation

A nested row does not use its own bottom edge — it propagates its FIRST
line's baseline. A second case (`baseline_propagates_from_a_nested_row`)
puts a 60×40 row (padding-top 4, one 20×10 leaf) beside the leaf boxes: its
baseline is 4 + 10 = 14, so it aligns 14 below the line's baseline (48),
landing at y = 34 with its inner leaf at y = 38. This is the other half of
the baseline construct D5 names.
