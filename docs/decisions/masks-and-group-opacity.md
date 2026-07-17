# Masks resolve into clip regions; group opacity splits free vs render-target

    status   accepted (story #44, 2026-07-17)
    scope    dashscene-core (commit), dashpaint (boundary B), dashscene-skia,
             dashbuf schema, dashscene-validator, dashc lowering, goldens

## Context

v0.8 (`docs/archive/2026-07-14-design-1-seed.md` §10.1) makes the pipeline
render shape masks and group opacity, both in the NOW band. Two facts
constrain the design:

- **P1** — the document carries intent, never rasterized results. A mask is
  _which node stencils which siblings_; a group opacity is _a node's alpha_.
  The resolved clip region and the resolved overlap verdict are results, so
  they live on the committed scene, never in the document.
- **P2** — a painter only colors; the flat rect table has no sibling or
  ancestor structure for it to walk. So both constructs must reach the
  painter already resolved, the posture
  `docs/decisions/resolved-clip-regions-at-commit.md` set for subtree clips.

## Options

For masks:

1. Resolve at commit into the existing clip-region table: a mask node adds
   its box to the clip regions of the siblings it masks, and resolves to the
   draws-nothing paint entry itself.
2. A dedicated mask table crossing boundary B, with the painter stenciling
   against a mask shape.
3. Painter-side sibling stenciling.

For group opacity's boundary-B shape:

1. A per-rect `opacity` field for the free path plus a `GroupComposite`
   slice for the render-target path.
2. Fold the opacity into the resolved paint entry's fill alpha.
3. One group table with a mode flag, no per-rect field.

For the render-target budget (Q-6):

1. A named placeholder in the validator profile config, checked at the paint
   gate by counting render-target groups.
2. Enforce in core at commit.
3. Enforce in the dashc lowering.

## Choice

**Masks — option 1.** A visible mask node contributes its resolved
(rounded) box to the `ClipRegion` of every following sibling within the same
parent, and to those siblings' subtrees, exactly as a clipping ancestor
does. Successive sibling masks **accumulate** (their boxes chain onto the
region, so a following sibling intersects every mask before it), not
replace. A **hidden** mask (`Visible(false)`) does not mask — Figma disables
a hidden mask, and a mask resolved 0×0 under the solver would otherwise clip
everything to nothing. The mask node itself resolves to
`PaintEntry::default()` (draws nothing). No new painter concept: masks
arrive as clip regions. Core gains `Prop::Mask(bool)` and the arena a `mask`
node flag; the document gains `Node.mask`. A mask add, remove (toggle-off),
move, or visibility change re-resolves the regions its following siblings
receive, so the change reaches the clip index and the dirty set in both
directions.

Only a **box outline mask** lowers from Figma: `dashc` refuses a soft alpha
or luminance `maskType`, and a mask whose shape is not a box (a text node's
letterforms; a `VECTOR`/`BOOLEAN_OPERATION` shape is already an unsupported
node type), by name (P4) — the hard clip-region model cannot express a soft
or letterform stencil.

**Group opacity — option 1.** Boundary B's `RectEntry` gains
`opacity: f32` — the effective _free_-path alpha for that rect. A new
`GroupComposite { start, end, alpha }` slice carries the render-target
groups (the subtree rect range and its composite alpha). `Painter::paint`
gains a `groups: &[GroupComposite]` parameter. Core gains `Prop::Opacity(f32)`
(paint-only, never reaches Taffy — §23) and the document `Node.opacity`.

At commit, a node with opacity below 1:

- whose painted subtree rects are mutually **non-overlapping** takes the
  **free path** — the alpha multiplies into each subtree rect's per-rect
  `opacity`, no render target;
- whose painted subtree rects **overlap** takes the **render-target path** —
  the subtree becomes a `GroupComposite` whose offscreen layer composites at
  the node's alpha times the carried free product, and the subtree draws
  into that layer at a reset product of 1.

Overlap is the pairwise test of the painting rects in the node's subtree
(a mask or layout-only node paints nothing and does not count). Non-overlap
is exactly the condition under which per-rect alpha and a group composite
produce the same pixels, so the free path is a sound optimization, not an
approximation.

**Budget — option 1.** `dashscene-validator` gains
`RENDER_TARGET_BUDGET_PLACEHOLDER` (a named placeholder pending Q-6) and the
`paint.render-target-budget` rule: the paint gate (`validate_scene`) counts
render-target groups and **warns** — never errors — when the count exceeds
the placeholder.

## Why

- **Masks reuse the clip machinery (over option 2/3).** Within this repo's
  lowered vocabulary a mask is a (rounded) shape, which is exactly what a
  `ClipBox` already expresses. A second, parallel clip-like mechanism a
  painter must keep in step is the trap
  `resolved-clip-regions-at-commit.md` rejected for clip. Option 3 is a
  direct P2 violation. The reuse limits masks to rectangular / rounded
  shapes — the same limit clip carries; luminance and alpha masks stay
  LATER band, and an ellipse mask is the ellipse-as-circle limit.
- **A mask node draws nothing.** In Figma a mask's fill is consumed as the
  stencil, not painted on top; the common case is an opaque shape whose only
  effect is where its siblings show. Resolving it to the draws-nothing entry
  is faithful and needs no painter rule.
- **Per-rect free alpha, not folded paint (over option 2).** Effective alpha
  is positional (it depends on the ancestor group-opacity chain), not paint
  content, so it belongs on the rect — the same argument
  `resolved-clip-regions-at-commit.md` made for clip. Folding it into the
  deduplicated paint entry would make two identically-filled nodes under
  different group opacities need distinct entries, destroying the pool, and
  is lossy for image and gradient fills.
- **A per-rect field _and_ a group slice (over option 3).** Without the
  per-rect field a free-path opacity change changes no rect-entry bits, so
  the dirty set misses it and an incremental painter never repaints the
  node. `entry_bits` includes the alpha for exactly this reason.
- **The budget is a painter budget on the solved scene (over option 2/3).**
  The overlap verdict exists only after commit, and core does not validate
  (that is the validator's job, published after core). A document carries no
  resolved overlap (P1), so the dashc lowering cannot see it either. A
  warning, not an error, because a fabricated hard limit on an unmeasured
  value would fail real scenes for no measured reason.

## Consequences

- `RectEntry` grows from 24 to 28 bytes, still `#[repr(C)]` blittable
  (pinned by test). `Painter::paint` grows one parameter; every current
  caller passes an empty groups slice.
- The Skia reference painter multiplies each rect's paint alpha by
  `rect.opacity` (a shader paint's alpha modulates its shader output, so one
  path covers every fill kind) and wraps each render-target group's rect
  range in `save_layer_alpha`. Groups nest by range; a stack of pending end
  indices closes the layers innermost first.
- **Dirty set.** A rect is dirty when its entry bits changed — now including
  the free-path alpha. A mask toggle marks the node paint-dirty (its own
  entry becomes draws-nothing) and feeds the clip-region cascade for its
  following siblings, mirroring the clip cascade's change flag. A
  render-target group's alpha lives outside the rect entry bits, so a group
  forming, dissolving, or changing alpha dirties its whole subtree range
  explicitly (the range of any group present on exactly one side of the
  commit); the differential dirty oracle exercises this and the mask toggle.
- **Hidden nodes draw nothing.** The fixed solver ignores `Visible(false)`
  for geometry, so a hidden subtree keeps its box and index but resolves to
  the draws-nothing paint entry — a solver-less consumer (the `.dsb`
  loader) does not paint a toggled-off layer. A visibility toggle re-interns
  the subtree's paint.
- **Overlap uses the painted extent.** The overlap test grows each node's
  box by its stroke outset (an outside or center stroke paints past the
  box), so a shared stroke band forces the render-target path rather than
  seaming at the free path's double-blended alpha. The test is clip-blind:
  clip-disjoint content is still judged overlapping, which over-composites
  (a needless render target) but never under-composites — a named debt
  candidate, not a correctness bug.
- **`opacity == 0`** stays on the free path (the subtree draws nothing, no
  compositing) even when its children overlap. `Node.opacity` has a load-gate
  domain rule (finite, `0..=1`) and `set_prop` refuses a non-finite value by
  name; both keep a `NaN` that reads back as fully opaque out (M7).
- **Text and group opacity/masks.** A glyph run's free-path alpha rides on
  `GlyphRun::opacity` (the painter dims the run). Render-target group layers
  and clip/mask regions are **not** applied to glyph runs — the z-interleave
  is deferred (`glyph-runs-cross-boundary-b.md`) — so a text node inside an
  overlapping partial-opacity group draws at full strength; the paint gate
  warns (`paint.text-outside-group`), and text-in-clip/mask (glyph runs never
  honored clip regions) is a pre-existing debt candidate. Not silent.
- **Fill-plus-stroke on the free path** applies the alpha to fill and stroke
  separately, which double-blends only in the stroke band of a
  partially-transparent node; the goldens avoid it and it is a known minor
  fidelity gap, not a correctness bug for the golden set.
- **An inert mask** (no following sibling, or a root mask) stencils nothing;
  the load gate warns (`paint.inert-mask`) rather than doing so silently.
- **dashc un-pin (debt #143).** The Figma lowering now lowers node opacity,
  mask nodes, and hidden nodes (`Node.opacity` / `Node.mask` /
  `Node.visible`). A hidden node keeps its DFS index (`Prop::Visible` →
  `Display::None`) instead of shifting the indices every later node depends
  on. **Rotation stays a named refusal** — no schema or paint support for it
  lands here (P4).
- **Opacity channel (debt #253).** `Channel::Opacity` joins the binding
  vocabulary, classified paint-only, with a dashc property-path mapping
  (`opacity` → `Channel::Opacity`) and a dashlang A1-mirror test. The other
  unmapped bindable paths (corner radius, stroke weight, font metrics) stay
  named warnings.
- **Stacked fills/strokes (debt #146) is untouched.** Masks and group
  opacity do not exercise stacked paint — a mask clips, an opacity modulates
  or composites — so `Paint.fill`/`.stroke` stay single-valued and #146
  remains open.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §10.1 (shape masks,
  group opacity, the overlap rule); issue #44 acceptance criteria; debts
  #143 (partly — rotation stays refused) and #253.
- Resolves: the mask/opacity/hidden-node refusals of debt #143; the Opacity
  channel of debt #253.
- Related: `docs/decisions/resolved-clip-regions-at-commit.md` (the
  precedent this reuses), `docs/decisions/visible-is-layout-opacity-is-paint.md`
  (the Visible/Opacity split), `docs/decisions/glyph-runs-cross-boundary-b.md`
  and `docs/decisions/image-assets-cross-boundary-b.md` (the precedents for
  growing `Painter::paint`'s input), `docs/decisions/dsb-frozen-fixture-r7-guard.md`
  (the schema-append guard), `docs/decisions/golden-comparison-space.md`
  (the goldens' tolerance).
- Files: `docs/design/dashpaint.md`, `docs/design/dashscene-skia.md`,
  `docs/design/dashscene-core-arena.md`, `docs/design/dashbuf.md`,
  `docs/design/dashc.md`, `docs/design/goldens.md`.
- Open question: Q-6 (`docs/technotes/open-questions.md`) — the render-target
  budget value, unmeasured, held by the placeholder here.
- Leaves open: debt #146 (stacked fills/strokes); glyph runs composited into
  render-target group layers and clipped to clip/mask regions (the deferred
  z-interleave); the clip-blind overlap test's over-composite direction.
