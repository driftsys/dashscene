# Corpus case: hug-in-fill

    construct  a Hug-sized node among Fill-sized siblings
    exercised  goldens/tooling/tests/v02_flex.rs (sizing_matches_its_golden),
               crates/dashlang/tests/corpus.rs (hug_in_fill_sizes_content_first_then_splits_the_rest, #46)
    golden     goldens/images/v02-sizing.png

## The scene

A horizontal container, 120×60, holding a `Hug` node followed by two
`Fill` nodes. The `Hug` node has no authored width — it holds one fixed
30×40 child:

    root (mode Horizontal, 120×60)
      ├── hug  (SizingH Hug, height 60)
      │     └── inner (fixed 30×40)
      ├── fill-a (SizingH Fill, height 60)
      └── fill-b (SizingH Fill, height 60)

## Expected solved rects

    hug:     x = 0    w = 30    (its content's width)
    fill-a:  x = 30   w = 45    ((120 - 30) / 2)
    fill-b:  x = 75   w = 45

## Why it is an edge case

The two sizing modes resolve against each other in one pass: the `Hug`
node's width is content-driven and must be known before the free space
the `Fill` siblings divide can be computed. Getting the order wrong
gives the `Fill` children the full 120 and pushes the `Hug` node out of
the container.

Core has no fill weight, and `dashscene-engine` maps every `Fill` to
`flex_grow = 1`, so the two `Fill` siblings always split the free space
equally.
