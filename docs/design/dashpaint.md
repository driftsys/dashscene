# dashpaint — boundary B

    crate    crates/dashpaint
    covers   v0.1 walking skeleton (story #3) + v0.3 paint vocabulary
             (story #13) + resolved subtree clips (story #97) + v0.5
             glyph-run table (story #30) + v0.8 group opacity (story #44)
             + v0.11 backdrop contract (story #393)

## Purpose

`dashpaint` defines boundary B (`docs/design/architecture.md`): the
complete input a painter consumes, and the trait every painter implements.
Principle P2 (`AGENTS.md`) holds throughout — a painter only colors; it
never measures, wraps, kerns, or moves anything.

Boundary B is a rect table plus a paint table plus a clip table. The paint vocabulary is
the v0.3 slice's set (`docs/roadmap.md`'s v0.3, drawn from
`docs/specification/04-figma-vocabulary-profile.md`'s NOW list): solid fills, the four
gradient kinds, image fills with scale modes, stroke with align,
per-corner radii, and clip. The crate has no dependencies, including no
`dashscene-core` and no `dashbuf` — see
`docs/decisions/dashpaint-owns-boundary-b-types.md` for why.

## The boundary-B contract

- The rect-table index is the document DFS node index
  (`docs/design/dashbuf.md`); a `RectEntry` carries no id field of its own.
- `RectEntry.paint` is an index into the `PaintTable`.
- `RectEntry.clip` is an index into the `ClipTable`. Clipping crosses
  this boundary already resolved: `dashscene-core` walks the clipping
  ancestors at commit, because a flat rect table carries none for a
  painter to walk (P2, story #97). **Masks reuse this table** (story #44):
  a mask node's box is added at commit to the clip regions of the siblings
  it stencils, so a painter needs no mask concept.
- `RectEntry.opacity` is the rect's resolved _free_-path group alpha
  (story #44): the product of the enclosing group opacities that took the
  free path, `1.0` when none applies. A painter multiplies the rect's
  paint alpha by it. The _render-target_ path crosses separately, as a
  `groups: &[GroupComposite]` parameter on `Painter::paint` — each names a
  subtree rect range `[start, end)` and the alpha its offscreen layer
  composites at, so an overlapping group at partial opacity flattens
  before its alpha applies. Both are resolved by `dashscene-core` at commit
  from `Prop::Opacity` intent (`docs/decisions/masks-and-group-opacity.md`).
- Text crosses as a `GlyphRunTable` — positioned glyph runs plus the
  MSDF atlases they sample (story #30,
  `docs/decisions/glyph-runs-cross-boundary-b.md`). Runs arrive already
  shaped, wrapped, and positioned in absolute document space by the one
  typesetter; the painter draws each glyph as a textured atlas quad and
  never moves anything (P2). The atlas is a plain mirror of the
  `dashscene-typeset` metrics blob, so `dashpaint` still depends on no
  crate.
- Solid-fill color is 4×f32 RGBA — the same shape as `dashbuf`'s `Color`
  struct (`crates/dashbuf/schema/dashbuf.fbs`), reproduced here as a
  plain type rather than shared by dependency.
- The generation stamp (`docs/design/architecture.md`) belongs to the double
  buffer `dashscene-core` owns, not to individual rect entries; it is out
  of scope for this crate.

## Public interface

All types and the trait live in `crates/dashpaint/src/lib.rs`:

- `Color` — `#[repr(C)]` RGBA, 4×f32 fields `r`, `g`, `b`, `a`.
- `RectEntry` — `#[repr(C)]`, fields `x`, `y`, `w`, `h: f32`,
  `paint: PaintIndex`, `clip: ClipIndex`, `opacity: f32` (28 bytes total,
  pinned by test).
- `GroupComposite` — a render-target group opacity: a rect subtree range
  `start`/`end: u32` and the `alpha: f32` its offscreen layer composites at
  (story #44).
- `PaintIndex` — `#[repr(transparent)]` newtype over `u32` (story #4,
  debt #54): a node index or other bare `u32` cannot cross into a
  paint index without an explicit wrap; layout unchanged.
- `PaintKind` — the fill vocabulary: `Solid { color }` (the pinned
  v0.1 shape), `Gradient(Gradient)`, `Image { image, scale_mode,
  transform, tile_scale }` (crop transform and tile magnification,
  story #14).
- Vocabulary value types — `Vec2`, `GradientStop`, `GradientKind`
  (Linear/Radial/Angular/Diamond), `Gradient` (kind + three normalized
  handle positions + stops), `ScaleMode` (Fill/Fit/Crop/Tile),
  `StrokeAlign` (Inside/Center/Outside), `Stroke` (width + align +
  solid color), `CornerRadii` (per-corner, `Default` = sharp).
- `MAX_GRADIENT_STOPS` — the gradient stop budget (story #15). It lives
  here, on boundary B, because it is a property of the paint vocabulary
  rather than of one backend: `dashscene-skia` asserts against it and
  `dashscene-validator` rejects it upstream (P4), and two hard-coded
  copies that drifted would make the validator's guarantee false.
- `PaintEntry` — the paint-table entry: `fill: Option<PaintKind>`
  (`None` = a paint-less, layout-only node), `stroke: Option<Stroke>`,
  `corners: CornerRadii`, `shadows: Vec<Shadow>` (v0.8, story #45);
  `PaintEntry::solid(Color)` is the v0.1 shorthand. See
  `docs/decisions/paint-entry-composition.md`. It carries no clip flag —
  whether a node clips its children is intent, and lives in the document
  and the arena, not in resolved painter input
  (`docs/decisions/resolved-clip-regions-at-commit.md`).
- `PaintEntry::samples_backdrop()` — whether a rect painted from the
  entry reads the already-composited backdrop beneath it, which is true
  when any of its `blurs` is a `BlurKind::Backdrop` (v0.11, story #393).
  Derived rather than stored: `blurs` already carries the fact, and a
  flag beside it would be a second copy of it that nothing keeps in
  agreement. It is the property the `Painter::paint` ordering guarantee
  is stated over, and it widens by itself if a further
  backdrop-sampling effect is added.
- `Shadow` / `ShadowKind` — a drop or inner shadow (v0.8, story #45):
  `kind` (`Drop`/`Inner`), `offset: Vec2`, `blur: f32` (Gaussian radius,
  non-negative), `spread: f32`, `color: Color`. Authored intent — the
  painter derives the shadow geometry from the rect's box and the entry's
  corners (P1). A list, not a fill kind, so a node stacks any number and
  `Paint.fill`/`.stroke` arity stays single-valued
  (`docs/decisions/effects-vocabulary-shadows.md`).
- `PaintTable` — a dense entry list behind a private field, indexed by
  `RectEntry.paint`: `new`, `push(&mut self, PaintEntry) -> PaintIndex`
  (returns the sequential index just assigned), `get(&self, PaintIndex)
  -> Option<&PaintEntry>`, `resolve(&self, PaintIndex) -> &PaintEntry`
  (the lookup painters use — panics on an out-of-range index), `len`,
  `is_empty`.
- `ImageTable` / `ImageAsset` / `ImageFormat` — encoded, format-tagged
  image assets (mirrors `dashbuf`'s `Document.images`), indexed by
  `PaintKind::Image`'s `image` field; same push/get/resolve contract as
  `PaintTable`. See
  `docs/decisions/image-assets-cross-boundary-b.md` (story #14).
- `Mat23` — row-major 2×3 affine; the image crop transform's shape.
- `ClipBox` — `#[repr(C)]`, one clipping ancestor's resolved box:
  `x`, `y`, `w`, `h: f32` plus `corners: CornerRadii` (all-zero radii =
  a sharp box).
- `ClipRegion` — the clip that applies to one rect: the boxes to
  **intersect**, outermost ancestor first (`boxes()`), behind a private
  field. No boxes = unclipped (`unclipped()`, `is_unclipped()`). The
  list is not pre-intersected into one box because the intersection of
  two rounded rects is not a rounded rect.
- `ClipTable` / `ClipIndex` — the region pool, same push/get/resolve
  contract as `PaintTable`, with one addition: `ClipTable::new()`
  reserves index 0 (`ClipIndex::UNCLIPPED`) for the unclipped region, so
  every rect resolves without a sentinel. `len()` counts it; a clip
  table is never empty, so there is no `is_empty`.
- `Painter` — the one trait every paint backend implements:
  `fn paint(&mut self, rects, paints: &PaintTable, images: &ImageTable,
  clips: &ClipTable, groups: &[GroupComposite], glyphs: &GlyphRunTable,
  dirty: Option<&[u32]>)` (an empty image table is valid input for
  image-less scenes; a fresh `ClipTable` for a scene that clips nothing;
  an empty `groups` slice for a scene with no render-target opacity).

`Color`, `RectEntry` and `ClipBox` are `#[repr(C)]` because
`docs/design/architecture.md` calls rect entries blittable and R-T4 plans
dirty-range instance-buffer uploads of per-frame painter input; fixing
the layout now costs nothing. A `RectEntry` is 28 bytes — four
coordinates, the paint and clip indices, and the free-path group alpha —
pinned by test.

`Painter::paint` is infallible and the trait is object-safe (`Box<dyn
Painter>` must work — backend selection is whole-scene, R3). Slice order
defines stacking — a later entry composites over an earlier one, since
DFS order encodes document stacking. The composited result is the
contract; iteration order is the implementation's choice (the lean
painter draws opaque cores front-to-back,
`docs/specification/03-target-hardware-rules.md`'s R-T2) — with the one
exception "Backdrop sampling" below states.
An out-of-range paint index is a broken contract between crates;
`PaintTable::resolve` centralizes the panic for that case, so no painter
invents its own failure path (a silent skip would be the silent drop P4
forbids). See `docs/decisions/painter-trait-infallible-slice-input.md`
for the alternatives considered on the trait's signature.

## Testing

`crates/dashpaint/tests/boundary_b.rs` exercises the public API only,
against hand-built fixtures, with no `dashscene-core` dependency. It
covers the `PaintTable` and `ClipTable` indexing contracts (including
both `resolve` panics and the reserved unclipped region), the
`PaintEntry` composition (solid shorthand, paint-less entry, full
gradient+stroke+corners entry, image fill), the recorded output of a
`RecordingPainter` test double over a two-rect fixture — including the
clip region it resolves per rect — the backdrop declaration and the
ordering barrier a painter reads out of the paint table for it, and
dyn-dispatch through `&mut dyn Painter`. The test file is
the executable statement of the boundary-B contract; this section
deliberately does not restate its cases.

## Subtree clipping

`Paint.clip` ("clips its children to its box", `docs/design/architecture.md`)
is a relation between a node and its descendants — the one construct a
painter cannot be handed directly, since the flat rect table has no
ancestors and P2 forbids re-deriving them. `dashscene-core` resolves it
at commit (issue #97): every rect carries the `ClipRegion` its clipping
ancestors add up to, and a painter intersects the boxes it is given
without asking which node each came from. A clipping node does not clip
itself — only its descendants; its own corner radii still shape its own
fill and stroke. The full contract and the rejected alternatives are
`docs/decisions/resolved-clip-regions-at-commit.md`.

## Masks and group opacity

Story #44 adds two more constructs a painter cannot be handed directly.
A **mask** node stencils the siblings that follow it in the same parent —
another producer-side relation — so `dashscene-core` resolves it at commit
into those siblings' `ClipRegion`s (the mask node's box added to each), and
the mask node itself resolves to the draws-nothing entry. Boundary B needs
no mask type: masks arrive as clip regions. **Group opacity** splits by the
overlap rule: a non-overlapping subtree folds its alpha into each rect's
`RectEntry.opacity` (the free path), while an overlapping one becomes a
`GroupComposite` the painter draws through an offscreen layer. The full
model, the overlap rule, and the render-target budget are
`docs/decisions/masks-and-group-opacity.md`.

## Backdrop sampling

Every effect before v0.11 is node-local: a shadow is built from the
node's own rounded-rect geometry, and a `GroupComposite` flattens a
subtree's **own** rects offscreen and composites that layer over what
lies beneath — it writes an isolated layer and never samples one. A
backdrop blur is the first effect that reads the already-composited
backdrop, so boundary B carries two things for it
(`docs/decisions/backdrop-blur-is-core-vocabulary.md`, story #393).

- **The declaration.** `PaintEntry::samples_backdrop()` answers whether
  a rect painted from the entry reads that backdrop. It sits in the
  paint entry rather than in `RectEntry` for the reason corners already
  do (`docs/decisions/paint-entry-composition.md`): `RectEntry`'s
  layout is pinned and blittable, and this is a paint-side effect
  parameter that shares the paint table's dedup pool. It is not a
  parallel table either — a `GroupComposite` spans a rect **range** and
  so cannot live on one entry, while a backdrop sample belongs to
  exactly one rect and already has a per-node home.
- **The ordering guarantee.** A painter still chooses its iteration
  order, except that every rect at a lower index than a
  backdrop-sampling rect is composited before that rect is drawn. The
  sampling rect is a barrier in any reorder, and the licence holds
  unchanged on either side of it. A painter that iterates in slice
  order satisfies this without doing anything, because it already
  composites back-to-front into one target; only a painter that
  reorders pays for the barrier.

The guarantee fixes order alone. Which surface the sample reads when a
barrier rect falls inside a `GroupComposite` range is not settled here —
it belongs to the first painter that implements the sampling
(`dashscene-skia`). Glyph runs are outside the guarantee for the same
reason they are outside `groups`: the v0.5 subset composites every run
over all rects, so no run is ever beneath a barrier and no run can enter
a sampled backdrop — a named limitation, not a silent drop.

## Trace

- Satisfies: `docs/design/architecture.md` painter trait (boundary B)
  and output shape, `docs/roadmap.md`'s v0.3 paint vocabulary (from
  `docs/specification/04-figma-vocabulary-profile.md`'s NOW list);
  issue #3, #13 and #97 acceptance
  criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness), #14 (v0.3 painting).
- Related decisions: `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`,
  `docs/decisions/resolved-clip-regions-at-commit.md`,
  `docs/decisions/backdrop-blur-is-core-vocabulary.md`.
