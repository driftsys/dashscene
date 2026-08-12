# Group opacity draws into a layer, and a second pipeline composites it

    status   accepted (2026-08-04)
    scope    dashscene-gpu's composite planner, its layer targets, the
             composite pipeline and shaders/composite.wgsl; the layer table on
             the instance buffer. Story #584 inherits all of it.

## Context

`docs/decisions/masks-and-group-opacity.md` split group opacity into two paths
at commit. A group whose painted rects do not overlap takes the **free path**:
the alpha multiplies into each rect's `opacity`, and this painter has drawn that
since story #578 — `Instance::opacity` is the whole of it. A group whose rects
**overlap** takes the **render-target path**, because per-rect alpha is not
equivalent there: where two members overlap, the lower one shows through.

Nothing drew the render-target path. `Instance::layer` has carried the innermost
enclosing group since story #578 and `render.rs` had no reader for it — the
field was packed, pinned by a layer-1 golden, and never used.

Two constraints shaped what could be built. The pipeline binds seven storage
buffers across two stages that allow four each — four and four, with nothing
spare (`docs/decisions/the-paint-parameter-heap.md`) — so nothing new could be
bound to it. And the reference painter had already fixed the semantics, which
`docs/specification/02-principles.md` P2 and this project's posture make the
specification rather than one option among several.

## Decision

**D1 — the layer table travels in the instance buffer.** `InstanceBuffer` gains
`layers: Vec<Layer>`, index-aligned with the `GroupComposite` slice, each row
carrying the group's `alpha` and its `parent` — the enclosing layer's slot, in
the same plus-one biasing `Instance::layer` uses. The packer already reads the
group slice to assign `layer`, so it records the rest of that row in the same
walk. `Renderer::render` grows no parameter.

**D2 — a layer is the full target extent, one texture per layer.** Transparent
on first use, which is the state `save_layer` starts the reference painter's
layer in. Not the group's bounds: a group's ink reaches past its rect range
through shadows and blurs, so a tight bound would have to be derived from the
effects rather than from the geometry, and getting it wrong moves pixels.

**D3 — a second pipeline composites, and this is the general route for sampling
a rendered target.** `shaders/composite.wgsl` is its own module with its own
`@group(0)`: one full-target quad, `textureLoad` at the fragment's own pixel,
multiplied by the group's alpha, blended premultiplied source-over. It declares
no sampler — the composite is a 1:1 pixel copy, so there is nothing to filter. A
pipeline owns its bind group layout, so this costs the paint pipeline no binding
at all.

**D4 — the passes are planned from the instance stream alone.**
`composite::plan` reads `Instance::layer` and `Layer::parent` and nothing else,
and returns the passes: a target, whether it clears or loads, and the ordered
steps drawn into it. A layer composites at the point its instances **end**, so
members following a group in slice order draw over it.

## Why

- **The layer table belongs with the instances (D1, over a `render`
  parameter).** The alpha and the routing are one fact. Split across two
  arguments, a caller could pass a group slice that disagreed with the `layer`
  values already packed into the buffer, and nothing would catch it. Keeping
  both in the artifact the packer produces also means layer 1 pins the whole
  group structure with no device — which is what layer 1 is for — and it left
  roughly twenty existing `render` call sites untouched.

- **Full extent, over bounds-tight layers (D2).** This is the reference
  painter's own choice and `dashscene-skia`'s `offscreen_layer` states the
  reason this painter shares. Story #584 adds precisely the effects that make a
  geometric bound wrong.

- **One texture per layer, over a depth-keyed pool (D2).** A pool is the smaller
  allocation and it is not what this builds. Measured: the showcase's only
  render-target group is one group nesting one deep, and
  `dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` warns at eight. A pool
  also has to keep a layer alive until its parent's pass, so it saves nothing
  until sibling groups are both numerous and deep. `composite::plan` names
  layers by slot and never says where their pixels live, so the pool can arrive
  later without touching the planner or its tests.

- **A second pipeline, over another `InstanceKind` (D3).** Folding the composite
  into the paint pipeline needs a fifth fragment-stage binding for the layer
  texture, against a stage that is full. This is not a workaround for the
  binding budget but the answer to it, and **story #584's backdrop blur takes
  the same route** — it also has to sample a rendered target, and it also cannot
  be a binding on the paint pipeline.

- **Planning from the stream, over replaying the group ranges (D4).** The ranges
  are boundary B's and the packer has already consumed them. Re-deriving nesting
  in the renderer would be a second derivation of one fact, which is the shape
  `docs/decisions/instance-buffer-contract.md` rejects — and the two could
  disagree.

- **Compositing at the group's end, not the frame's (D4).** The reference
  painter closes its layer when the group's rect range ends, and slice order is
  the stacking order. A plan that deferred every composite to the end of the
  frame would put every group on top of everything after it.

## What this costs

One render pass per target change, so a frame with _g_ groups encodes at most
_2g + 1_ passes where it previously encoded one, and a target returned to loads
rather than clears. One texture of the target's extent per layer, allocated when
the extent or the layer count changes and held across frames otherwise. One
uniform buffer and one bind group per layer, written every frame — a group's
alpha animates without changing how many layers there are.

`Renderer::last_draw_runs` now counts composite draws as well as atlas runs,
which is what it always claimed to count.

## What this does not do

Group opacity was the only thing that drew into a layer when this was written,
and three `InstanceKind` variants stayed undrawn: `ShadowDrop`, `ShadowInner`
and `Backdrop`. Story #584 drew the two shadow kinds and story #733 the
backdrop, so nothing in the v0 paint vocabulary is undrawn now. The backdrop
took the second-pipeline route this record established but **not** the layers:
it has to read the destination, and a texture cannot be a render attachment and
a sampled binding in the same pass, so the planner splits the pass and the
renderer snapshots the target
(`a-backdrop-blur-snapshots-the-target-it-draws-into.md`).

The free path is unchanged — a non-overlapping group still resolves to per-rect
alpha at commit and emits no group at all.

## Alternatives considered

**A depth-keyed texture pool.** Deferred, with the measurement above and a note
that the planner does not have to change for it.

**Compositing every layer after the last instance, into the frame's target.**
Simpler to encode and wrong in two independent ways: it puts every group above
everything drawn after it, and it flattens nesting so an inner group reaches the
target at its own alpha instead of through its parent's. Both are asserted
against directly, because a fixture with one group at the end of a frame cannot
tell any of these apart.

**Carrying the group's alpha on every instance instead of a layer table.** It
would need no layer table and no parent pointers — and it cannot express
nesting, which is the case the table exists for. It is also the free path
wearing a different name.

Refs #583. Refs #569. Refs #578. Refs #133.
