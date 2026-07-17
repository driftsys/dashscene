# Corpus case: variant topology change

    construct  a set_variant switch that changes the resolved layout topology
    exercised  crates/dashlang/tests/corpus.rs (a_variant_switch_changes_the_wrap_line_topology)

## The scene

A Hug-height horizontal wrap row, width 120, main gap 10, cross gap 10, of
two chips, with a variant set of two members that both override chip a's
width away from its authored 20:

    row (mode Wrap, width 120, SizingV Hug, gap 10, cross gap 10)
      ├── a (authored 20x20)
      └── b (fixed 50x20)

    variant set
      ├── member 0 "one-line"  (a.Width = 50)
      └── member 1 "wrapped"   (a.Width = 80)

## Expected solved rects

    member 0 (one-line):
      row: (0, 0, 120, 20)   one line tall
      a:   (0, 0, 50, 20)    the override (not the authored 20)
      b:   (60, 0, 50, 20)   50 + 10 gap, same line

    member 1 (wrapped, after set_variant):
      row: (0, 0, 120, 50)   a second line appeared: 20 + 10 + 20
      a:   (0, 0, 80, 20)    widened override
      b:   (0, 30, 50, 20)   wrapped to line 2

## Why it is an edge case

Core variants are sparse scalar overrides — the five-prop slice
X/Y/Width/Height/Fill (`docs/decisions/variant-set-flat-index.md`) — so a
`set_variant` switch never changes the arena node tree. It changes the
*resolved* layout: overriding chip a's width past the point where chip b
still fits pushes b onto a new wrap line, so a line appears and the Hug
container grows taller. Both members override a's width, so the pre-switch
assertions witness member 0's override (a = 50, not the authored 20) rather
than a no-op. Authored against core's `Txn` (`add_variant_set`/`set_variant`)
because variant declaration is not `dashlang` builder vocabulary.

## Reported limit — a child leaving the laid-out set

The stronger reading of "variant topology change" — a child LEAVING the
solved layout so its siblings close up — is a child-count change. In this
repo's variant model that means a variant member setting `Prop::Visible(false)`
(→ `Display::None`). `Visible` is **not** in the five-prop variant slice
(`VariantValue` carries only X/Y/Width/Height/Fill), and widening it is
"additive future work" (`docs/decisions/variant-set-flat-index.md`). It would
touch core `VariantValue` and the `dashbuf` variant table (append-only, R7),
out of story #46's scope. So the child-leaves realization is a reported
blocker, not a silent gap; this case realizes the topology change the current
vocabulary can express (a wrap line appearing).

The animated form of a variant switch is proven end to end by
`goldens/tooling/tests/v04_flip.rs` (E5); this case pins the exact
before/after geometry.
