# Corpus case: negative gap

    construct  negative flex gap (Figma auto-layout item spacing < 0)
    lowering   Figma≠CSS → child margins (docs/design/dashbuf.md, story #10)
    exercised  crates/dashscene-engine/tests/solve.rs

## The scene

A horizontal container, 200×20, three fixed 30×20 children, with a
container gap of −8 (children overlap by 8):

    row (mode Horizontal, gap -8, 200×20)
      ├── a (fixed 30×20)
      ├── b (fixed 30×20)
      └── c (fixed 30×20)

## The lowering

`Txn::lower_negative_gaps` rewrites the container to gap 0 and adds the
negative gap to the leading main-axis margin of every child after the
first:

    row (mode Horizontal, gap 0, 200×20)
      ├── a (fixed 30×20)
      ├── b (fixed 30×20, margin.left -8)
      └── c (fixed 30×20, margin.left -8)

## Expected solved rects (x positions)

    a: x = 0    (0..30)
    b: x = 22   (30 - 8, overlaps a by 8)
    c: x = 44   (52 - 8, overlaps b by 8)

The lowered scene and the margin-authored scene above solve to
byte-identical rect tables — the story #10 acceptance criterion. The
vertical case is the same on `margin.top`.
