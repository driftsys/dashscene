# dashscene-skia — the Skia CPU-raster reference painter

    crate    crates/dashscene-skia
    covers   v0.1 first Painter implementation (story #4); v0.3 paint
             vocabulary (story #14); resolved subtree clips (story #97);
             v0.5 MSDF glyph-run rendering (story #30); v0.8 group opacity
             (story #44)

## Purpose

`dashscene-skia` is the first implementation of the `Painter` trait `dashpaint`
defines (boundary B, `docs/archive/2026-07-14-design-1-seed.md` §8.1): a
CPU-raster reference painter over `skia-safe`, deterministic and bit-exact. It
is the golden generator (`docs/technotes/rendering-and-painters.md`), not a
throwaway — it stays the reference painter as later backends (Unity, the lean
native painter) land.

Its `[dependencies]` are `dashpaint` and `skia-safe` only; `dashscene-core` is a
`[dev-dependencies]` entry used by the scene-building test. The painter never
sees the arena, staging, or Taffy (P1, P2 — a painter only colors; it never
measures, wraps, kerns, or moves anything).

## Public interface

All in `crates/dashscene-skia/src/lib.rs`:

    pub struct SkiaPainter { /* private: a skia_safe::Surface */ }

    impl SkiaPainter {
        pub fn new(width: i32, height: i32) -> Self;
        pub fn png_bytes(&mut self) -> Vec<u8>;
        pub fn rgba_bytes(&mut self) -> Vec<u8>;
    }

    impl Painter for SkiaPainter {
        fn rotates(&self) -> bool;   // true — story #770
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
                 images: &ImageTable, clips: &ClipTable,
                 groups: &[GroupComposite], glyphs: &GlyphRunTable,
                 dirty: Option<&[u32]>);
    }

- `new(width, height)` allocates a CPU raster surface (N32 premultiplied) of the
  given pixel size. Panics if `width` or `height` is not positive.
- `png_bytes(&mut self)` PNG-encodes the current surface contents.
- `rgba_bytes(&mut self)` reads the current surface back as tightly packed
  RGBA8888 rows, unpremultiplied — used by the painter's own tests and by future
  golden tooling for exact pixel comparison.

## Paint semantics (v0.3 vocabulary, story #14)

`paint(rects, paints, images, clips)` runs in one pass over `rects`:

- Clears the surface to transparent, then draws every rect in slice order. Slice
  order is stacking order — dashpaint's contract is that a later entry
  composites over an earlier one, since DFS order encodes document stacking;
  painting back-to-front is this implementation's choice of how to realize that,
  not part of the contract itself.
- Per rect, resolves `paints.resolve(rect.paint)`. A fill-less entry
  (`fill: None`) draws nothing — this is the shared draws-nothing entry an
  unfilled node interns at commit (`docs/decisions/boundary-b-unification.md`),
  not a per-painter skip rule. Non-default corner radii shape the entry's box as
  a rounded rect for fills and strokes alike.
- Every draw is anti-aliased
  (`docs/decisions/reference-painter-antialiasing.md`): deterministic on pinned
  CPU raster, a no-op on integer-aligned axis-aligned edges.
- Gradients build one affine frame (unit space through the three normalized
  handles into the box); linear/radial/angular are skia gradient shaders under
  that frame, diamond is an SkSL runtime effect sampling a 1D skia ramp child
  (§8.1 — not a Skia primitive). A degenerate frame falls back to the first
  stop's color; more than 8 stops panics by name (a budget the validator will
  enforce upstream).
- Stroke align lowers by geometry expansion (§8.1): inside/outside offset the
  stroked rrect by half the width (radii adjust) and center-stroke.
- Image fills resolve their asset in the `ImageTable`
  (`docs/decisions/image-assets-cross-boundary-b.md`), decode, and draw clipped
  to the entry's (rounded) box: Fill covers, Fit contains, Tile repeats at
  `tile_scale`, Crop maps the normalized transform. Nearest sampling, for
  determinism. Decoding is cached by `ImageTable` index on the painter, so an
  asset decodes once no matter how many rects reference it (issue #101) or how
  many frames are painted (issue #639). The baked-vector-field atlases share
  that cache — they are ordinary `ImageTable` entries, so there is one key space
  and one decode per asset.

  The cache is invalidated by comparing the incoming `ImageTable` against the
  one the decodes came from, by value. An index alone is not an identity: a host
  keeps one painter across a document change (`demo/`), and two documents both
  have an asset at index 0. A content hash would be the honest key, but
  `dashbuf`'s `AssetEntry.hash` is consumed by the loader and does not cross
  boundary B — `ImageAsset` carries a format and bytes and no hash.

  What the cache retains is two copies of each asset's _encoded_ payload, one in
  the kept table and one inside the Skia image. The decoded pixels live in
  Skia's own global resource cache, which is limited and purges under pressure.
  Nothing bounds the number of entries, and no memory budget exists to size it
  against (issue #462, and issue #614 for the same shape of risk on retained
  group composites).
- Subtree clipping arrives resolved (story #97): the rect's `ClipRegion` is
  intersected before it draws — `save`, one anti-aliased `clip_rrect(Intersect)`
  per box (outermost first), draw fill and stroke, `restore`. The painter never
  asks which node a box came from (P2), and a region with no boxes costs no
  save/restore. The clip applies to the rect's own fill and stroke, not to the
  rects after it in slice order. Masks arrive as clip regions too (story #44),
  so the painter needs no mask code.
- Group opacity arrives resolved (story #44,
  `docs/decisions/masks-and-group-opacity.md`). The free path rides on
  `RectEntry.opacity`: each draw's paint alpha is multiplied by it
  (`set_alpha_f` modulates a shader paint's output, so one path covers solid,
  gradient, and image fills). The render-target path is the `groups` slice: the
  painter begins an offscreen composite at each `GroupComposite`'s `start` and
  blends it at the group's alpha when the innermost open group's `end` is
  reached, so an overlapping group at partial opacity flattens before its alpha
  applies. The groups nest by range, so a stack of pending layers closes them
  innermost-first.

  There are two realisations of that composite, one per `DirtyMode` (issue
  #278). `Full` uses Skia's own `save_layer_alpha` / `restore`, which owns the
  layer and discards it — nothing is retained, which is what lets that mode be
  correct without reading the dirty set, and it is the mode every golden renders
  through. `Retained` draws each group's subtree into its own raster surface,
  blends the snapshot at the origin with the same 8-bit alpha, and keeps the
  snapshot; a later frame whose dirty set leaves the group's rect range alone
  blends the same snapshot again and skips the subtree entirely. The two are
  pixel-identical, asserted frame by frame in
  `crates/dashscene-skia/tests/painter.rs` over nested groups with anti-aliased
  corners and an anchored glyph run.

  The rule for when a composite may be blended again lives in
  `crates/dashscene-skia/src/retention.rs` and is generic over the backend's
  layer handle: it is expressed in rect indices and the dirty set only, and
  names no Skia type, so a GPU painter can implement it against its own render
  targets. A composite is invalidated by any dirty index inside `[start, end)`,
  by its range moving or disappearing, and by a frame the painter could not
  treat incrementally at all. Alpha is not part of the identity — the layer is
  built at full alpha and the group's alpha applies at the blend.
- Shadows render live (v0.8, story #45,
  `docs/decisions/effects-vocabulary-shadows.md`). A drop shadow draws before
  the fill: the node's rendered outline — its fill box grown by the stroke
  outset for an outside/center stroke — outset by `spread`, offset, filled with
  the shadow color under a Gaussian blur mask filter (`sigma =
  0.4375 * blur`,
  Figma's measured constant; no filter at `blur = 0`). An inner shadow draws
  after the stroke: clip to the shape, then fill an even-odd path (outer rect
  minus the offset, spread-inset inner rounded rect) so the blur bleeds inward.
  Both draw inside the rect's clip-region `save`/`restore` and any open
  render-target `save_layer`, and each is modulated by `RectEntry.opacity`, so a
  shadowed node dims with a folded opacity, composites inside a render-target
  group, and clips to its ancestor region. Stacked shadows draw in
  `PaintEntry.shadows` order, which is Figma's back-to-front `effects` array
  order, so the last-listed shadow composites on top — no reversal.
- Backdrop blur renders live (v0.11, story #393,
  `docs/decisions/backdrop-blur-is-core-vocabulary.md`). Skia has it natively: a
  `save_layer` whose `SaveLayerRec` carries a backdrop `ImageFilter` initializes
  the new layer with the current layer's contents passed through that filter.
  The painter clips to the node's own rounded box, opens such a layer, and
  restores it immediately — nothing is drawn into it, so its whole content is
  the blurred backdrop and the restore composites that over the sharp original.
  Skia reads the halo the kernel needs from outside the clip, so the blur is
  built from the real backdrop rather than from a copy truncated at the node's
  box. The sigma mapping is `sigma = 0.4375 * radius`, the same one the shadows
  use (`blur_sigma`, stated once so the two cannot drift — and measured to be
  genuinely the same, `docs/decisions/blur-sigma-is-figmas-mapping.md`), and the
  filter clamps at its input edge so a node frosting the canvas edge picks up
  that edge's color instead of darkening. The blur draws **before** the node's
  own shadows, fills and stroke: boundary B states the guarantee over rects at a
  lower index, so the backdrop is what those composited, not this node's own
  ink. `RectEntry.opacity` rides on the layer's paint like every other draw, so
  a dimmed node frosts proportionally. A baked-vector node (story B1) is
  confined to the field's coverage instead of a box — the coverage shader is
  installed as a **clip** (`clip_rect` over the field's padded quad, then
  `clip_shader` against the coverage) and the layer opens inside it — because
  the live hero's frosted panel is exactly that: a Figma VECTOR carrying
  `BACKGROUND_BLUR`.

  It was a `BlendMode::DstIn` mask inside the layer until commit `3aa5eb4`, and
  this paragraph still said so until debt #1036. The two are not equivalent:
  `DstIn` clears the layer outside the outline, but the restore then still has
  to composite what is left, leaving the destination weighted
  `1 - blurred_alpha × coverage` where a replacement weights it `1 - coverage`.
  Those agree only where the backdrop is opaque (debt #503). Promoting the mask
  to a clip moves the coverage into the term the blend already lerps by. `DstIn`
  **is** still how the masked _fill_ path works (`draw_vector_field`), which is
  why the stale sentence read as describing something real. Inside a
  render-target `GroupComposite` the sample reads that group's layer, not the
  canvas beneath it: Skia filters the innermost open layer, which is the
  backdrop-root reading the decision record settles. A `BlurKind::Layer` blur is
  skipped by name — node-local, budgeted at v1, and nothing in this tree emits
  one.

## Text — MSDF glyph runs (v0.5 Latin, story #30)

The painter draws the `GlyphRunTable` (boundary B's text half,
`docs/decisions/glyph-runs-cross-boundary-b.md`) **inside** the rect pass: each
run is drawn immediately after the rect at its `GlyphRun::rect` anchor, before
that rect's clip save is restored and before any enclosing group layer closes
(issues #275 and #274). Placing the draw there is the whole of both fixes — the
run inherits the anchor's clip region, lands inside every `GroupComposite` layer
around it, and takes the anchor's z position, with no change to how group layers
open or close. Each glyph is one textured MSDF atlas quad: the glyph's
`plane_em` bounds map to a device quad at the run's render size (y-up ems to
y-down document space), and its `atlas_px` bounds map to the atlas texels. An
SkSL runtime effect samples the atlas with linear filtering, takes the median of
the three distance channels, and resolves coverage over the screen-pixel range
(`distance_range_px * render_size / px_per_em`), modulating the run's fill
color. The reference painter uses the atlas as the product path; Skia's native
text (`SkTextBlob`) is a debug overlay only (`DESIGN_1.md` §7.2).

Runs composited over every rect as unconditional foreground through v0.12. Two
consequences of ending that: text is now covered by an overlapping rect at a
higher index, which is the correct reading of DFS stacking; and a run is now
inside the backdrop barrier, so a painter that reorders must count runs in its
barrier accounting rather than only rects.

The MSDF setup splits by how long each part stays valid. The SkSL compile and
one decode per atlas are held on the painter in `MsdfCache`, so a text scene
compiles the resolve shader once and decodes each atlas once for as long as that
atlas set is the one being drawn (issue #644). The run-by-anchor index is
genuinely per-frame — the runs change every commit — and is built in
`MsdfFrame`, which borrows the cache's shader and decodes rather than rebuilding
them. A text-free scene still builds none of it.

**All three constant shaders are held the same way**, and two of them only since
issue #1186. `FIELD_MASK_SKSL` was a `paint()` frame local, so it recompiled on
every call that drew a baked shape — about 30 us, measured over 200 iterations
against release Skia. `DIAMOND_SKSL` was worse and was found reviewing that fix:
it lived in a `const` inside `diamond_shader` and compiled per **draw**, so a
scene of twenty diamond-gradient nodes paid twenty compiles a frame. All three
rest on one invariant, which `MsdfCache::effect` states: the shader is a
constant, so no input can stale it. All three are lazy, so a scene carrying none
of text, a baked shape or a diamond gradient compiles nothing.

These atlases hang off the `GlyphRunTable` and are not `ImageTable` entries, so
the painter-lifetime image cache above could not reach them; this is that
cache's twin for the text half of the input, and it invalidates on the same
principle. The difference is what it compares. The atlas set arrives behind an
`Arc` that commit shares rather than rebuilds, so `Arc::ptr_eq` settles the
steady-state frame in constant time and comparing contents is only the fallback
for an equal set rebuilt behind a fresh allocation. Holding the handle is also
what makes keeping it free, where the image cache must keep a copy of its table.
Both properties — that a shared set cannot change under its holder, and that a
live handle's address cannot be reused — follow from `push_atlas` going through
`Arc::make_mut`, and are recorded on `GlyphRunTable::atlas_set`.

`MsdfCache::frame` also checks every anchor against the rect table up front:
under the interleave, a run bucketed at an index the loop never visits would
simply never be drawn, so the check turns a silent drop into a named panic (P4).

## Testing

`crates/dashscene-skia/tests/painter.rs` is the story's acceptance path and the
first end-to-end, cross-crate exercise of boundary B: it builds a scene through
`dashscene-core`'s `Arena`/`Txn` API, commits it, and paints the result through
`SkiaPainter`, asserting on the RGBA readback. It covers: an exact-pixel scene
with a nested filled child, asserted byte-for-byte
(`paints_a_core_committed_scene_with_exact_pixels`); an unfilled parent that
draws nothing while its filled child paints (`an_unfilled_node_draws_nothing`,
pinning the boundary-b-unification crossing); the PNG signature on encoded
output (`encodes_png`); and, since story #14, per-kind render tests over
hand-built boundary-B input: hard-stop gradients probed at exact bytes for all
four kinds, the degenerate-frame fallback, stroke-align band placement,
rounded-corner coverage, the four image scale modes (plus tile scaling, crop
transform, and rounded clipping of image overflow), the named panic for the
gradient stop budget, and — since story #97 — resolved subtree clips: a sharp
region confining a rect to its ancestor's box, a rounded region rounding it, a
two-box region intersecting, and a clipped rect leaving the rect painted after
it untouched.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §8.1 (CPU raster
  reference painter, v0.3 lowerings); issues #4, #14 and #97 acceptance
  criteria.
- Blocks: #44/#45 (v0.8 masks/shadows build on this surface).
- Related decisions: `docs/decisions/boundary-b-unification.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/image-assets-cross-boundary-b.md`,
  `docs/decisions/reference-painter-antialiasing.md`,
  `docs/decisions/resolved-clip-regions-at-commit.md`.
