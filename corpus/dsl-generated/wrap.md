# Corpus case: wrap

    construct  a wrapping horizontal row with a distinct cross gap
    exercised  crates/dashlang/tests/corpus.rs (wrap_breaks_lines_and_hugs_to_them,
               a_fixed_height_wrap_packs_its_lines_at_the_cross_start)
    golden     goldens/tooling/tests/v08_fidelity.rs (v08-wrap, hand-built #43)

## The scene

A 200-wide, Hug-height wrap row, padding 10, main gap 10, cross gap 20,
holding four fixed chips 30 high:

    row (mode Wrap, 200 wide, SizingV Hug, gap 10, cross gap 20, padding 10)
      ├── chip0 (fixed 80x30)
      ├── chip1 (fixed 60x30)
      ├── chip2 (fixed 70x30)
      └── chip3 (fixed 50x30)

## Expected solved rects

    row:    (0, 0, 200, 100)     hug height 10 + 30 + 20 + 30 + 10
    chip0:  (10, 10, 80, 30)     line 1
    chip1:  (100, 10, 60, 30)    80 + 10 gap
    chip2:  (10, 60, 70, 30)     line 2 (10 + 30 + 20 cross gap)
    chip3:  (90, 60, 50, 30)     70 + 10 gap

## Why it is an edge case

The inner width is 200 − 2×10 = 180. The greedy line fill takes
80 + 10 + 60 = 150, then + 10 + 70 = 230 > 180, so the row breaks after
chip1. The distinct cross gap (20, against the main gap 10) is what the
line spacing shows. A Hug-height wrap container packs its lines at the
cross start rather than spreading them
(`docs/decisions/v08-layout-vocabulary-shape.md` D5).

## Fixed-height line packing

`align_content = FlexStart` (D5) is inert in a Hug-height container, where
the lines define the height, so a second case
(`a_fixed_height_wrap_packs_its_lines_at_the_cross_start`) fixes the height
at 200: two 60×30 boxes in a 100-wide row wrap, and the second line packs
at y = 40 (30 + 10 cross gap), not at the container's far edge as taffy's
default stretch would place it. Break-and-revert to stretch fails it.
