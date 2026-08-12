# Shadows are a per-entry effect list, rendered live in the painter

    status   accepted (story #45, 2026-07-17)
    scope    dashbuf schema, dashpaint (boundary B), dashscene-core
             (commit + load), dashscene-skia, dashscene-validator, dashc
             lowering, goldens

## Context

v0.8 (`docs/archive/2026-07-14-design-1-seed.md` §10.1) puts baked drop and
inner shadows in the NOW band. Until this story the document had no effects
vocabulary at all, so `dashc` refused a `DROP_SHADOW`/`INNER_SHADOW` by name
(debt #144). This story adds the vocabulary and renders it.

Two facts shape the design:

- **A shadow is a per-node paint property with no cross-node relation.** Unlike
  a mask (which stencils _siblings_) or a group opacity (which composites a
  _subtree_), a shadow depends only on the node's own box, corners, and shadow
  parameters. So it needs no commit-time resolution against the tree — it is the
  corners case, not the masks/opacity case.
- **P1** — the document carries intent, never results. Offset, blur radius,
  spread, and color are intent; the spread-expanded, offset, blurred shape is a
  painter result and lives nowhere in the document.
- **P2** — a painter only colors. The shadow parameters reach the painter on the
  flat paint-table entry; the painter derives the shadow geometry from the
  rect's box and the entry's corners at draw time.

Scope boundary: shadows render **live** in the Skia painter this slice.
Compile-time shadow baking and `profile:core` enforcement are v1, so the
content-addressed asset model (#107) is not a dependency and stays deferred.

## Options

For the schema shape:

1. A `shadows: [Shadow]` list on the paint-pool entry.
2. Fixed slots — one drop-shadow field plus one inner-shadow field.
3. A shadow variant on the `Fill` union.

For the inner-shadow rendering technique:

1. Clip to the shape, then fill the complement of the (offset, spread-inset)
   inner rounded rect under a Gaussian blur.
2. An offscreen image-filter layer (blur the shape's complement, composite
   source-in).

For placement (where the shadow parameters live between schema and painter):

1. Carry them through `dashscene-core` (`Prop::Shadows`) into the committed
   paint entry, exactly as corners flow.
2. Have the painter or a golden build the boundary-B tables directly, bypassing
   core.

## Choice

**Schema — option 1.** The `Paint` pool entry gains `shadows: [Shadow]`,
appended at the tail (R7). A `Shadow` table carries `kind`
(`ShadowKind::Drop`/`Inner`), `offset` (`Vec2`), `blur` (`float32`,
non-negative), `spread` (`float32`, any sign), and a required `color`. Boundary
B mirrors it: `PaintEntry.shadows: Vec<Shadow>`. A shadow rides on the
deduplicated paint entry, so two nodes sharing a style and shadows share one
pool entry, and `Painter::paint` gains no new parameter.

**Inner shadow — option 1.** The painter clips to the node's rounded box, then
fills an even-odd path (an outer rect minus the offset, spread-inset inner
rounded rect) with the shadow color under a Gaussian blur mask filter. The blur
bleeds inward from the shape's edge, thicker on the offset side. Drop shadows
are the same geometry outset by spread, offset, and drawn behind the fill.

**Drop shadow casts from the rendered outline.** A drop shadow's base shape is
the node's rendered silhouette, which an outside or center stroke widens past
the fill box — an outside stroke by its full width, a center stroke by half, an
inside stroke not at all. The painter grows the shadow shape by that stroke
outset (the same amount `draw_stroke` expands the stroke geometry) before the
spread and offset apply, so a stroked node casts a shadow the size of what it
draws, not of its fill box alone.

**Stacking order matches Figma's `effects` array.** Figma's `effects` array is
back-to-front, the same convention as its `children` array: `effects[0]` is the
backmost shadow and the last element renders on top. (Confirmed from Figma's
documented `children` back-to-front ordering and the community report that the
REST/plugin `effects` array is the reverse of the top-to-bottom Effects panel,
so the panel-top / on-top shadow is the array's last element.) `dashc` preserves
array order into `PaintEntry.shadows`, and the painter draws the list forward —
a later draw composites over an earlier one (the same DFS-order rule the rect
table uses), so the last shadow ends up on top with no reversal. A same-kind
stacked golden with two semi-transparent colored drop shadows pins this: the
overlap is red-over-blue only in this order and would flip if the draw loop
reversed.

**Spread math** (the seed §8.1 lowering Skia has no primitive for): a drop
shadow's box is the node box outset by `spread`, its corners
`r > 0 ? max(0, r + spread) : 0`; an inner shadow's lit hole is the node box
inset by `spread`, corners `r > 0 ? max(0, r - spread) : 0`; both translated by
the offset. The blur radius maps to a Skia mask-filter sigma as
`sigma = 0.4375 * blur`; a zero-blur shadow uses no mask filter. **This was
`blur / 2`, the CSS/browser convention, until issue #412 measured Figma's own
mapping and found it nearer `0.4375`
(`docs/decisions/blur-sigma-is-figmas-mapping.md`, which supersedes the constant
below).**

**Placement — option 1.** `dashscene-core` gains `Prop::Shadows(Vec<Shadow>)`,
stored on the node, classified paint-affecting, emitted into
`PaintEntry.shadows` at commit and folded into the paint-intern key. The loader
reads `Paint.shadows` into `Prop::Shadows`. This mirrors the corners plumbing
end to end and makes the slice a complete vertical (`.dsb` → core → painter), so
the schema and the `dashc` lowering feed a real runtime path rather than dead
bytes.

**Figma lowering.** `dashc`'s `shadows_of` reads a node's visible
`DROP_SHADOW`/`INNER_SHADOW` effects into `Shadow`s (color, offset, radius,
spread); the triage un-pins the refusal — a lowered shadow is no diagnostic at
all. Noise, texture, and progressive blur stay REJECT; a shadow with no color is
a named refusal (like a `SOLID` with no color); a hidden effect is skipped, like
a hidden paint.

A **non-`NORMAL` shadow blend mode** degrades exactly as a non-`NORMAL` paint
blend mode does: the shadow still lowers (its offset/blur/spread/ color are
carried and it draws `NORMAL`, because the painter has no blend-mode vocabulary)
and the blend mode raises `AdvancedBlendMode` — a warning under `Profile::Full`
(a visible degrade, never a silent drop-to-`NORMAL`, P4) and an error under
`Profile::Core` (which blocks the document, so the shadow never renders). This
double representation — lower the geometry, diagnose the unsupported blend — is
the established posture for blend modes, not a shadow-specific rule.

## Why

- **A list, not fixed slots (over option 2).** Figma's `effects` is an ordered
  array, and a real design routinely stacks several drop shadows for layered
  elevation. Fixed slots would re-create the #146 gap for effects — a node with
  two drop shadows would have to be refused, growing the refusal band rather
  than shrinking it. The list is also why this story does **not** touch
  `Paint.fill`/`.stroke` arity: shadows are a separate effect list, so those
  stay single-valued and #146 stays open and unexercised (recommend re-anchoring
  it at the next revision).
- **A list on the entry, not a `Fill` variant (over option 3).** A shadow is not
  a way to fill the box; it is a separate mark behind or inside the fill. A
  `Fill` variant would force a node to choose between a fill and a shadow, which
  is wrong.
- **Clip + inverse-fill for inner shadows (over option 2).** Pure geometry,
  deterministic for a pinned skia, and it reuses the same rounded-rect and clip
  machinery the drop shadow and the existing stroke/clip code already use. An
  offscreen image-filter layer adds a render-target round-trip (which R-T1
  discourages) and skia image-filter output is less bit-stable across versions
  than a plain path fill.
- **Through core, not around it (over option 2).** Carrying shadows in
  `Prop::Shadows` reuses the corners plumbing verbatim and keeps the schema and
  the lowering honest — without it, `Paint.shadows` would be written by `dashc`
  and populated into no runtime entry. The goldens then author through core,
  like the #44 mask/opacity goldens.

## Consequences

- `PaintEntry` grows a `Vec<Shadow>`. It was never `Copy` (it already held
  `Vec`/`Option` fields), so the field costs nothing there; the commit clones
  the shadow list only on a paint-intern cache miss, like the gradient's stops.
  The paint-intern key (core and the `dashc` emit pool) folds the shadow list
  in, so two entries differing only in a shadow no longer dedup to one.
- **The painter draws shadows inside the existing per-rect envelope.** Drop
  shadows draw before the fill, inner shadows after the stroke, both inside the
  rect's clip-region `save`/`restore` and inside any open render-target group
  `save_layer`. So a shadowed node under a folded (free-path) opacity dims with
  it (`rect.opacity` modulates each shadow), under a render-target group
  composites its shadow inside the layer, and a clipped node's drop shadow is
  clipped to its ancestor region. `showShadowBehindNode` is not modeled — the
  painter always draws a drop shadow behind the node (the Figma default); a
  documented fidelity limitation, and a debt candidate.
- **A mask or hidden node casts no shadow.** Both resolve to
  `PaintEntry::default()` (empty shadows), so an authored shadow on a mask node
  does not reach boundary B.
- **The load gate and the paint gate both domain-check shadows.** Offsets and
  spread must be finite; the blur radius finite and non-negative (a negative
  Gaussian is meaningless); color channels finite and in `[0, 1]`; the kind is
  an append-only enum, range-checked like the layout enums. New rules
  `paint.shadow.invalid-geometry` and `paint.shadow.color-out-of-range`. Spread
  may be negative (CSS/Figma shrink a shadow with it), so only the blur is
  floored.
- **The r7 fixture carries a non-default shadow.** The frozen
  `v0_5_document.dsb` gains an `Inner` shadow with every field distinguishable
  from its default, so a shifted field id or renumbered `ShadowKind` reads back
  wrong (`dsb-frozen-fixture-r7-guard.md`).
- **The goldens pin the shadow, not just the frame.** A drop-shadow and an
  inner-shadow scene each compare at the 2% tolerance and add a sensitivity
  guard: a broken variant with the shadow removed differs by 1159 px (drop) /
  748 px (inner), far above the ~82-px tolerance budget, so a regression that
  drops the shadow fails.
- **`dashc` un-pin (debt #144 resolved).** The lowering now lowers drop and
  inner shadows; the corresponding row in
  `unsupported-figma-constructs-refuse-the-compile.md` is retired. The effect
  bands otherwise stand: noise/texture/progressive blur REJECT, layer/backdrop
  blur LATER.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §10.1 (baked drop/inner
  shadows) and §8.1 (the shadow-spread math); issue #45 acceptance criteria;
  debt #144.
- Resolves: debt #144 (the effects-vocabulary gap that forced the
  `DROP_SHADOW`/`INNER_SHADOW` refusal).
- Related: `docs/decisions/masks-and-group-opacity.md` (the #44 painter/commit
  surface this extends — `RectEntry.opacity`, the render- target group path this
  composites inside),
  `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` (the
  baked-shadow row retired here),
  `docs/decisions/dsb-frozen-fixture-r7-guard.md` (the schema-append guard),
  `docs/decisions/golden-comparison-space.md` (the goldens' tolerance and
  sensitivity discipline).
- Files: `docs/design/dashbuf.md`, `docs/design/dashpaint.md`,
  `docs/design/dashscene-core-arena.md`, `docs/design/dashscene-skia.md`,
  `docs/design/dashc.md`, `docs/design/dashscene-validator.md`,
  `docs/design/goldens.md`.
- Leaves open: `showShadowBehindNode` (a drop shadow is always drawn behind the
  node); compile-time shadow baking and `profile:core` enforcement (v1); debt
  #146 (stacked fills/strokes, still unexercised).
- Debt candidates (reported, not filed): a zero-alpha or zero-opacity shadow is
  drawn (blurred and rasterized) rather than skipped early — a wasted per-frame
  cost the painter could short-circuit.

- **The sigma mapping is no longer a debt candidate: it was measured, and it
  changed.** This paragraph previously read that `sigma = blur / 2` was the
  CSS/browser convention, "still not measured against a real Figma capture",
  with both oracle slots at `status: pending-265`. Those slots were captured,
  and the measurement (issue #412, 2026-07-31) found the shadow frames fit Figma
  at `0.4375 * blur` rather than `blur / 2` — by 7.1x on mean for the drop
  shadow and 5.3x for the inner shadow. The constant is now `0.4375 * blur`; see
  `docs/decisions/blur-sigma-is-figmas-mapping.md`, which is the authority for
  it.
