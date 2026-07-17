# Corpus case: negative gap

    construct  negative flex gap (Figma auto-layout item spacing < 0)
    lowering   Figma≠CSS → child margins (docs/design/dashbuf.md, story #10)
    exercised  crates/dashscene-engine/tests/solve.rs (the lowering + #236 rebate),
               crates/dashlang/tests/corpus.rs (the DSL-generated Hug case, #46)

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

## The DSL-generated case (#46)

The stress-corpus generator authors the same construct under a **Hug**
container, where it also exercises the #236 rebate: a Hug-width row of
three fixed 30×20 boxes with gap −8.

    row (mode Horizontal, SizingH Hug, height 20, gap -8)
      ├── a (fixed 30x20)
      ├── b (fixed 30x20)
      └── c (fixed 30x20)

    a: (0, 0, 30, 20)
    b: (22, 0, 30, 20)     30 - 8 overlap
    c: (44, 0, 30, 20)     52 - 8 overlap
    row hug width: 74      30 + 22 + 22

The Hug width (74) is correct only under the negative-margin-hug rebate
(`docs/decisions/negative-margin-hug-rebate.md`, debt #236); taffy 0.12
alone collapses the intrinsic sum over the lowered negative margins. This
is why #46's negative-gap case could not go green until #236 landed. The
corpus test pins the DSL margin form against the core `gap` +
`lower_negative_gaps` form, so both the lowering output and the Hug sum are
checked. It is a plain flex row, never wrap: a negative wrap gap is a named
refusal (`docs/decisions/v08-layout-vocabulary-shape.md` D5).
