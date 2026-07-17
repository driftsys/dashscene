# Corpus case: variant topology change

    construct  a set_variant switch that hides a child, changing the laid-out child set
    exercised  crates/dashlang/tests/corpus.rs (a_variant_switch_hides_a_child_and_reflows_the_laid_out_set)

## The scene

A Hug-width horizontal row, height 20, no gap, of three fixed 30x20 chips,
with a variant set of two members — one showing all three, one hiding the
middle chip:

    row (mode Horizontal, SizingH Hug, height 20)
      ├── a (fixed 30x20)
      ├── b (fixed 30x20)
      └── c (fixed 30x20)

    variant set
      ├── member 0 "all"          (no overrides)
      └── member 1 "hide-middle"  (b.Visible = false)

## Expected solved rects

    member 0 (all shown):
      row: (0, 0, 90, 20)   hugs all three chips
      a:   (0, 0, 30, 20)
      b:   (30, 0, 30, 20)  in the middle
      c:   (60, 0, 30, 20)  last

    member 1 (hide-middle, after set_variant):
      row: (0, 0, 60, 20)   collapses by the hidden child's width
      a:   (0, 0, 30, 20)   unaffected
      b:   (0, 0, 0, 0)     degenerate — left the laid-out set (Display::None)
      c:   (30, 0, 30, 20)  reflowed into b's place

    member 0 again (after switching back):
      row: (0, 0, 90, 20)   restored — b re-entered the set
      b:   (30, 0, 30, 20)
      c:   (60, 0, 30, 20)

## Why it is an edge case

This is E3's "different child counts" form: a `set_variant` switch that adds
or removes a child from the solved layout. A variant member sets
`VariantValue::Visible(false)` on chip b, which lowers to Taffy
`Display::None` — b leaves the laid-out set, resolves to a degenerate rect, c
closes into its place, and the Hug row collapses by 30. Switching back re-adds
b and grows the row, the reverse topology change. `Visible` joined the variant
override vocabulary in story #283 (core `VariantValue` plus the `dashbuf`
variant table, append-only R7); before that a variant could only change the
resolved layout of a fixed child set, never the set itself
(`docs/decisions/variant-set-flat-index.md`). Authored against core's `Txn`
(`add_variant_set`/`set_variant`) because variant declaration is not `dashlang`
builder vocabulary.

The animated form of a variant switch is proven end to end by
`goldens/tooling/tests/v04_flip.rs` (E5); this case pins the exact
before/after geometry. A hidden-to-shown transition itself is not tweened by
the FLIP path (it animates rect channels only, no visibility/opacity channel —
see `crates/dashscene-engine/src/flip.rs`); the appearing node pops while its
reflowing siblings animate.
