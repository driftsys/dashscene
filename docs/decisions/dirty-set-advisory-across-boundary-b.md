# The dirty set crosses boundary B as advisory input

    status   accepted (story #163, 2026-07-15)
    scope    crates/dashpaint (Painter trait), crates/dashscene-skia (dirty modes)

## Context

`commit` computes a dirty set on every commit, and no painter could see
it: `Painter::paint` took only the four tables (rects, paints, images,
clips). R-T4 (`docs/specification/03-target-hardware-rules.md`) specifies
the per-frame CPU cost as "dirty-range instance-buffer upload from the rect
table + submission. Nothing else." — which no painter could implement,
because the set never crossed boundary B.

This is the widening `painter-trait-infallible-slice-input.md` anticipated:
that record named the eventual painter input as a triple (rect entries,
glyph runs, dirty set), and settled the v0.1 signature over bare slices
until text and incremental painting arrived. The dirty set is the first of
the two to land.

## Options

1. Pass the dirty set as an advisory `Option<&[u32]>` on `paint`.
2. Add a separate `paint_incremental` method with a default implementation
   that delegates to `paint`.
3. Pass the whole `CommittedScene`.

## Choice

Option 1. `None` means the caller has no dirty information (hand-built
tables, or a first frame); `Some(&[])` means nothing changed. Ignoring the
set is always correct, and a painter that honors it must produce identical
output.

`DirtyMode::Full` and `DirtyMode::Retained` on `SkiaPainter` implement both
halves of that contract, and the two are compared over a mutation sequence
in `goldens/tooling/tests/dirty_oracle.rs`.

## Why

- Option 3 would make `dashpaint` depend on `dashscene-core`, collapsing
  boundary B (`painter-trait-infallible-slice-input.md`,
  `boundary-b-unification.md`). A slice keeps the painter free of the
  semantic model.
- Option 2 leaves two entry points that can disagree, and the product
  painter would implement one and stub the other. One method with an
  explicit "no information" case is the honest signature.
- The retained mode models the **instance buffer**, not the pixels. It is
  not damage-region partial redraw: restoring a framebuffer into tile
  memory to repaint part of it is the flush-and-resolve R-T1 forbids. The
  GPU redraws every quad in one pass; what R-T4 removes is the CPU work and
  the upload.

  **Amended (issue #278, v0.14):** the retained mode now also keeps the
  **render-target group composites** across frames, and blends a stored
  one again when no dirty index falls inside its rect range. This is not
  the damage-region redraw the paragraph above rules out, and the
  difference is which surface is being reloaded. A render-target group is
  an offscreen pass the scene demanded — a group at partial opacity whose
  subtree overlaps has to flatten before its alpha applies
  (`masks-and-group-opacity.md`), so the pass, and its resolve, are paid
  whether or not anything is retained. Reusing its resolved result skips a
  pass that would produce identical pixels; it never loads the main
  framebuffer back into tile memory, and every quad outside the group is
  still redrawn in one pass. What is retained is a render target the
  document asked for, not a cache of the frame.
- The reference painter's second mode exists as a **test oracle**, not for
  speed. A dirty set that omits a changed rect is a stale instance-buffer
  entry on the product painter — intermittent, and diagnosed on target
  hardware. The same bug is a deterministic pixel diff in CI here, with no
  GPU. That is what makes the incremental commit (the retained Taffy tree
  and the derived dirty set) safe to build next.
