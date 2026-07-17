# dashscene-skia — the Skia CPU-raster reference painter

    crate    crates/dashscene-skia
    covers   v0.1 first Painter implementation (story #4); v0.3 paint
             vocabulary (story #14); resolved subtree clips (story #97);
             v0.5 MSDF glyph-run rendering (story #30); v0.8 group opacity
             (story #44)

## Purpose

`dashscene-skia` is the first implementation of the `Painter` trait
`dashpaint` defines (boundary B,
`docs/archive/2026-07-14-design-1-seed.md` §8.1): a CPU-raster
reference painter over `skia-safe`, deterministic and bit-exact. It is
the golden generator (`docs/technotes/rendering-and-painters.md`), not a throwaway — it stays the reference
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
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
                 images: &ImageTable, clips: &ClipTable);
    }

- `new(width, height)` allocates a CPU raster surface (N32
  premultiplied) of the given pixel size. Panics if `width` or
  `height` is not positive.
- `png_bytes(&mut self)` PNG-encodes the current surface contents.
- `rgba_bytes(&mut self)` reads the current surface back as tightly
  packed RGBA8888 rows, unpremultiplied — used by the painter's own
  tests and by future golden tooling for exact pixel comparison.

## Paint semantics (v0.3 vocabulary, story #14)

`paint(rects, paints, images, clips)` runs in one pass over `rects`:

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
  rule. Non-default corner radii shape the entry's box as a rounded
  rect for fills and strokes alike.
- Every draw is anti-aliased
  (`docs/decisions/reference-painter-antialiasing.md`): deterministic
  on pinned CPU raster, a no-op on integer-aligned axis-aligned edges.
- Gradients build one affine frame (unit space through the three
  normalized handles into the box); linear/radial/angular are skia
  gradient shaders under that frame, diamond is an SkSL runtime effect
  sampling a 1D skia ramp child (§8.1 — not a Skia primitive). A
  degenerate frame falls back to the first stop's color; more than 8
  stops panics by name (a budget the validator will enforce upstream).
- Stroke align lowers by geometry expansion (§8.1): inside/outside
  offset the stroked rrect by half the width (radii adjust) and
  center-stroke.
- Image fills resolve their asset in the `ImageTable`
  (`docs/decisions/image-assets-cross-boundary-b.md`), decode, and
  draw clipped to the entry's (rounded) box: Fill covers, Fit
  contains, Tile repeats at `tile_scale`, Crop maps the normalized
  transform. Nearest sampling, for determinism.
- Subtree clipping arrives resolved (story #97): the rect's
  `ClipRegion` is intersected before it draws — `save`, one
  anti-aliased `clip_rrect(Intersect)` per box (outermost first), draw
  fill and stroke, `restore`. The painter never asks which node a box
  came from (P2), and a region with no boxes costs no save/restore. The
  clip applies to the rect's own fill and stroke, not to the rects after
  it in slice order. Masks arrive as clip regions too (story #44), so the
  painter needs no mask code.
- Group opacity arrives resolved (story #44,
  `docs/decisions/masks-and-group-opacity.md`). The free path rides on
  `RectEntry.opacity`: each draw's paint alpha is multiplied by it
  (`set_alpha_f` modulates a shader paint's output, so one path covers
  solid, gradient, and image fills). The render-target path is the
  `groups` slice: the painter opens a `save_layer_alpha` at each
  `GroupComposite`'s `start` and closes it (`restore`) when the innermost
  open group's `end` is reached, so an overlapping group at partial
  opacity flattens before its alpha applies. The groups nest by range, so
  a stack of pending end indices closes them innermost-first.

## Text — MSDF glyph runs (v0.5 Latin, story #30)

After the rect pass, the painter draws the `GlyphRunTable` (boundary B's
text half, `docs/decisions/glyph-runs-cross-boundary-b.md`). Each glyph is
one textured MSDF atlas quad: the glyph's `plane_em` bounds map to a
device quad at the run's render size (y-up ems to y-down document space),
and its `atlas_px` bounds map to the atlas texels. An SkSL runtime effect
samples the atlas with linear filtering, takes the median of the three
distance channels, and resolves coverage over the screen-pixel range
(`distance_range_px * render_size / px_per_em`), modulating the run's
fill color. The reference painter uses the atlas as the product path;
Skia's native text (`SkTextBlob`) is a debug overlay only (`DESIGN_1.md`
§7.2). Runs composite over every rect (text foreground) in v0.5.

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
(`encodes_png`); and, since story #14, per-kind render tests over hand-built
boundary-B input: hard-stop gradients probed at exact bytes for all
four kinds, the degenerate-frame fallback, stroke-align band placement,
rounded-corner coverage, the four image scale modes (plus tile scaling,
crop transform, and rounded clipping of image overflow), the named panic
for the gradient stop budget, and — since story #97 — resolved subtree
clips: a sharp region confining a rect to its ancestor's box, a rounded
region rounding it, a two-box region intersecting, and a clipped rect
leaving the rect painted after it untouched.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §8.1 (CPU
  raster reference painter, v0.3 lowerings); issues #4, #14 and #97
  acceptance criteria.
- Blocks: #44/#45 (v0.8 masks/shadows build on this surface).
- Related decisions: `docs/decisions/boundary-b-unification.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/image-assets-cross-boundary-b.md`,
  `docs/decisions/reference-painter-antialiasing.md`,
  `docs/decisions/resolved-clip-regions-at-commit.md`.
