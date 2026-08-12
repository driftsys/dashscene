# v0.3 paint goldens are per-family, complementing story #14

    status   accepted (story #18, 2026-07-12)
    scope    goldens/; epic #12 plan (v0.3)

## Context

Issue #18 asks for "golden scenes exercising each v0.3 paint kind". By the time
it ran, story #14 had already shipped, for the same vocabulary: per-kind painter
unit tests asserting exact interior bytes (the correctness gate, bit-stable
across machines), and one combined `v03-paint.png` golden covering every kind on
one canvas. #18's literal scope also lists "clips", which subtree `clipsContent`
(#97, deferred) cannot satisfy. The story needed reshaping to add value rather
than duplicate #14.

## Options

1. Per-family goldens: three focused scenes (gradients, strokes, images), each
   isolating one construct family.
2. One golden per construct (~12 files).
3. Treat #18 as already satisfied by #14 and close it.

## Choice

Option 1.

## Why

- The combined `v03-paint.png` (option 3) is one image; a construct regression
  there requires reading which pixels moved. Per-family goldens give the
  `docs/technotes/rendering-and-painters.md` bisect-by-construction the slice
  asks for — a regression fails only the affected family's golden — without
  claiming #18 was a no-op.
- One golden per construct (option 2) is redundant with the per-kind unit tests,
  which already bisect to a single construct with exact bytes; 12 PNGs and 12
  inspection passes buy little over three family images.
- Constraints carried from #14, recorded here so downstream stories inherit
  them:
  - **Clips**: subtree `clipsContent` is deferred to #97 and the painter panics
    on it, so it is not goldened. Rounded clipping of a node's own content
    (implemented) is shown instead.
  - **Tolerance**: the goldens' anti-aliased gradients and curves are not
    bit-identical across CPU architectures
    (`docs/decisions/golden-comparison-space.md`); the 64×64 family goldens
    compare at a 2% differing-pixel tolerance (higher than the combined golden's
    1% because edge jitter is a larger fraction of a smaller canvas), with
    clamped-region interior probes pinning key properties bit-stably.
- This is a within-epic scope reshape, not an epic-close revision (epic #12
  stays open — the importer stories #15/#16 remain); it is recorded here so the
  v0.3 plan reflects what #14 and #18 together actually deliver.
