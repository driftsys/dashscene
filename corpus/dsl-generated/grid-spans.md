# Corpus case: grid spans

    construct  a grid with per-track sizing, cell anchors, and row/column spans
    exercised  crates/dashlang/tests/corpus.rs (grid_spans_place_children_across_tracks)
    golden     goldens/tooling/tests/v08_fidelity.rs (v08-grid-spans, hand-built #43)

## The scene

A fixed 200x160 grid, padding 10, both gaps 10, columns [60px, 1fr, 1fr]
and rows [40px, 1fr, 1fr]:

    grid (mode Grid, 200x160, gap 10, cross gap 10, padding 10,
          columns [60px, 1fr, 1fr], rows [40px, 1fr, 1fr])
      ├── header (Fill/Fill, row 0 col 0, column span 3)
      ├── tall   (Fill/Fill, row 1 col 0, row span 2)
      ├── plain  (Fill/Fill, row 1 col 1)
      ├── footer (Fill/Fill, row 2 col 1, column span 2)
      └── fixed  (fixed 30x20, row 1 col 2)

## Expected solved rects

    grid:   (0, 0, 200, 160)
    header: (10, 10, 180, 40)    spans all three columns
    tall:   (10, 60, 60, 90)     spans rows 1 and 2 (40 + 10 + 40)
    plain:  (80, 60, 50, 40)     one fraction cell
    footer: (80, 110, 110, 40)   spans columns 1 and 2 (50 + 10 + 50)
    fixed:  (140, 60, 30, 20)    at its cell origin, not stretched

## Why it is an edge case

Each fraction column is `minmax(0, 1fr)`, so the two fraction columns take
(200 − 20 padding − 20 gaps − 60 fixed) / 2 = 50 and the two fraction rows
(160 − 20 − 20 − 40) / 2 = 40. Spans cover contiguous tracks and the gaps
between them; a `Fill` child stretches over its cell area while a fixed
child keeps its size at the cell origin. Placement is by cell anchor, so
`main_align`/`cross_align` do not apply
(`docs/decisions/v08-layout-vocabulary-shape.md` D2/D5).
