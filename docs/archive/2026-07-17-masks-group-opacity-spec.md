# Spec — masks + group opacity (story #44, v0.8)

Working memory (`docs/wip/`), gardened into durable records before the PR.

## Goal

Teach the pipeline shape masks and group opacity, per
`docs/archive/2026-07-14-design-1-seed.md` §10.1's NOW band, and un-pin the
Figma-lowering refusals the two now support (#143). Deliver goldens for a
masked scene and for a group-opacity scene on both the free path and the
render-target path.

Principles that bind the work: P1 (the document carries intent, never
rasterized results), P2 (one solver/typesetter; painters only color), P4
(every unsupported construct is a named diagnostic, never a silent drop).

## Requirements

- **Shape masks.** A mask node stencils the siblings that follow it within
  the same parent (until the next mask sibling or the end of the parent),
  clipping them to the mask node's own (rounded) box. The mask node itself
  draws nothing (its shape is a stencil, not paint). Resolution lives at
  commit, reusing the resolved-clip-region machinery
  (`docs/decisions/resolved-clip-regions-at-commit.md`). P1: the document
  carries mask _intent_ (`Node.mask` / `Prop::Mask`); the region a painter
  consumes is the resolved result.
- **Group opacity with the overlap rule.** A node carries `opacity` in
  `[0, 1]` (default 1). At commit the compiler decides per node:
  - `opacity == 1` — nothing to do.
  - `opacity < 1` and the node's painted subtree rects are mutually
    non-overlapping — the _free_ path: the alpha multiplies into each
    subtree rect's effective per-rect alpha, no render target.
  - `opacity < 1` and two painted subtree rects overlap — the
    _render-target_ path: the subtree composites offscreen and the layer
    composites at the node's alpha. This is the budgeted case (Q-6).
- **`Prop::Opacity(f32)`** is paint-only and never reaches Taffy (§23). Its
  pair `Prop::Visible` landed at v0.4.
- **Q-6 budget.** The render-target budget value is unmeasured; use a named
  placeholder in the validator's profile config and a scene-gate warning
  that counts render-target groups against it.
- **#253 — the `Opacity` Channel.** Add `Channel::Opacity` to
  `dashscene-core`'s channel vocabulary, classified paint-only, with an
  A1-mirror test in `crates/dashlang/tests/reactive.rs` (the pattern story
  #167 used for the `Fill.*` channels). The remaining unmapped bindable
  paths (corner radius, stroke weight, font metrics) stay named warnings
  (P4).
- **#143 — dashc un-pin.** The Figma lowering rejects node opacity,
  rotation, mask nodes, and hidden nodes. Un-pin node opacity, mask nodes,
  and hidden nodes (all three now have a lowering). Rotation stays a named
  refusal (no schema or paint support lands here — P4).
- **#146 — stacked fills/strokes.** Folded judiciously. Masks and group
  opacity do not exercise stacked paint (a mask clips, opacity modulates or
  composites — neither needs a second fill or stroke). Implement nothing for
  it here and report the remainder as a debt candidate rather than dropping
  it silently.
- **Schema.** Append `Node.opacity`, `Node.mask`, `Node.visible` to
  `dashbuf.fbs`, and `BindingChannel.Opacity`. Append-only. Regenerate the
  frozen r7 fixture in the same commit, exercising the new fields at
  non-default values.
- **Goldens.** Masked scene; group-opacity free path; group-opacity
  render-target path.

## Out of scope

Shadows/effects (#45), wrap/grid/baseline (#43), the stress corpus (#46),
rotation (stays refused), luminance/alpha masks (LATER band — a mask is a
shape stencil only), clip-on-rotated (v0.8 LATER).

## Boundary-B shape

The rect table gains a per-rect `opacity: f32` (the resolved _free_-path
alpha for that rect). A new `GroupComposite { start, end, alpha }` slice
carries the render-target groups (the subtree rect range and its composite
alpha). `Painter::paint` gains a `groups: &[GroupComposite]` parameter.
Masks reuse the existing `ClipTable` — no new painter concept.

## Alternatives considered

### Where mask resolution lives

1. **Resolve at commit into the clip-region table (chosen).** A mask node
   contributes its resolved box to the `ClipRegion` of the siblings it
   masks (and their subtrees), exactly as a clipping ancestor does; the mask
   node itself resolves to the draws-nothing paint entry. The painter needs
   no mask concept — masks arrive as clip regions.
2. A dedicated mask table crossing boundary B, with the painter stenciling
   against a mask shape. Rejected: within this repo's lowered vocabulary a
   mask is a (rounded) shape, which is exactly what a `ClipBox` already
   expresses; a second, parallel clip-like mechanism a painter must keep in
   step is the trap `resolved-clip-regions-at-commit.md` rejected for clip.
3. Painter-side sibling stenciling. Rejected on P2: a flat rect table has no
   sibling structure, and re-deriving one is what P2 forbids.

The chosen option limits masks to (rounded) rectangular shapes, the same
limit clip has. Luminance and alpha masks are LATER band; an ellipse mask is
the ellipse-as-circle limit clip already carries.

### Group-opacity representation across boundary B

1. **A per-rect `opacity` field for the free path, plus a `GroupComposite`
   slice for the render-target path (chosen).** Effective alpha is
   positional (it depends on the ancestor group-opacity chain), not paint
   content, so it belongs on the rect, not folded into the deduplicated
   paint entry — the same argument `resolved-clip-regions-at-commit.md` made
   for clip. Putting free-path alpha in the rect entry also lets a free-path
   opacity change reach the dirty set (the A1-mirror case). The
   render-target path needs a structural range instruction the free path
   cannot express, so it is a separate table.
2. Fold opacity into the resolved paint entry's fill alpha. Rejected: it
   destroys paint-pool dedup (two identically-filled nodes under different
   group opacities would need distinct entries), and it is lossy for image
   and gradient fills.
3. Represent both paths as one group table with a mode flag, no per-rect
   field. Rejected: without the per-rect field a free-path opacity change
   changes no rect-entry bits, so the dirty set misses it and an
   incremental painter never repaints the node.

### The render-target budget placement

1. **A named placeholder constant in the validator profile config, checked
   at the paint gate (`validate_scene`) by counting render-target groups
   (chosen).** The budget is a painter budget on the solved scene, exactly
   like the gradient-stop and inside-stroke budgets already there; the count
   exists only after commit produces the groups, so it belongs on the solved
   scene, not the document (P1). A warning, not an error: exceeding an
   unmeasured placeholder must not hard-fail a build.
2. Enforce in core at commit. Rejected: core does not validate (that is the
   validator's job, and the validator is published after core).
3. Enforce in the dashc lowering. Rejected: the budget is about the solved,
   overlapping geometry, which the document does not carry (P1).

### Whether to widen `Node.opacity`/`mask`/`visible` on the Node table vs a pool

Opacity, mask membership, and visibility are per-node attributes (positional,
like `parent`), not shared style, so they append to the `Node` table rather
than the deduplicated `Paint` pool. `Prop::Opacity`/`Prop::Mask`/
`Prop::Visible` are node props in the arena for the same reason.
