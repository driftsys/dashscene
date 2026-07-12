# dashscene-skia — the Skia CPU-raster reference painter

    crate    crates/dashscene-skia
    covers   v0.1 first Painter implementation (story #4)

## Purpose

`dashscene-skia` is the first implementation of the `Painter` trait
`dashpaint` defines (boundary B, `specs/DESIGN_1.md` §8.1): a CPU-raster
reference painter over `skia-safe`, deterministic and bit-exact. It is
the golden generator (§8), not a throwaway — it stays the reference
painter as later backends (Unity, the lean native painter) land.

Its `[dependencies]` are `dashpaint` and `skia-safe` only;
`dashscene-core` is a `[dev-dependencies]` entry used by the
scene-building test. The painter never sees the arena, staging, or
Taffy (P1, P2 — a painter only colors; it never measures, wraps,
kerns, or moves anything).

## Public interface

All in `crates/dashscene-skia/src/lib.rs`:

    pub struct SkiaPainter { /* private: a skia_safe::Surface */ }

    impl SkiaPainter {
        pub fn new(width: i32, height: i32) -> Self;
        pub fn png_bytes(&mut self) -> Vec<u8>;
        pub fn rgba_bytes(&mut self) -> Vec<u8>;
    }

    impl Painter for SkiaPainter {
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
    }

- `new(width, height)` allocates a CPU raster surface (N32
  premultiplied) of the given pixel size. Panics if `width` or
  `height` is not positive.
- `png_bytes(&mut self)` PNG-encodes the current surface contents.
- `rgba_bytes(&mut self)` reads the current surface back as tightly
  packed RGBA8888 rows, unpremultiplied — used by the painter's own
  tests and by future golden tooling for exact pixel comparison.

## Paint semantics (v0.1 vocabulary)

`paint(rects, paints)` runs in one pass over `rects`:

- Clears the surface to transparent, then draws every rect in slice
  order. Slice order is stacking order — dashpaint's contract is that
  a later entry composites over an earlier one, since DFS order
  encodes document stacking; painting back-to-front is this
  implementation's choice of how to realize that, not part of the
  contract itself.
- Per rect, resolves `paints.resolve(rect.paint)`. A fill-less entry
  (`fill: None`) draws nothing — this is the shared draws-nothing
  entry an unfilled node interns at commit
  (`docs/decisions/boundary-b-unification.md`), not a per-painter skip
  rule. A `Solid` fill draws an axis-aligned rect with anti-aliasing
  off: v0.1 geometry is always axis-aligned, and disabling AA avoids
  per-platform coverage-math variance on those edges, which is what
  keeps goldens bit-exact and machine-independent.
- Any construct this painter cannot draw yet — a stroke, a non-default
  corner radius, clip, or a gradient or image fill — panics via
  `unimplemented!`, naming story #14 in the message. This is not a
  silent drop (P4): v0.1 producers cannot emit these (core's `Prop`
  only stages solid fill), so reaching the panic means a producer
  emitted vocabulary the painter does not implement yet, not that the painter silently dropped input.

## Testing

`crates/dashscene-skia/tests/painter.rs` is the story's acceptance
path and the first end-to-end, cross-crate exercise of boundary B: it
builds a scene through `dashscene-core`'s `Arena`/`Txn` API, commits
it, and paints the result through `SkiaPainter`, asserting on the RGBA
readback. It covers: an exact-pixel scene with a nested filled child,
asserted byte-for-byte
(`paints_a_core_committed_scene_with_exact_pixels`); an unfilled
parent that draws nothing while its filled child paints
(`an_unfilled_node_draws_nothing`, pinning the
boundary-b-unification crossing); the PNG signature on encoded output
(`encodes_png`); and the honest-failure contract for vocabulary this
painter cannot yet draw, via a hand-built `PaintEntry` with a gradient
fill (`unimplemented_vocabulary_panics_by_name`,
`#[should_panic(expected = "story #14")]`).

## Trace

- Satisfies: `specs/DESIGN_1.md` §8.1 (CPU raster reference painter);
  issue #4 acceptance criteria.
- Blocks: #6 (golden harness), #14 (v0.3 painting — implements the
  vocabulary this painter currently panics on).
- Related decisions: `docs/decisions/boundary-b-unification.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`.
