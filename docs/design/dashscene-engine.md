# dashscene-engine — the Taffy layout solve

    crate    crates/dashscene-engine
    covers   v0.2 flex core (story #9), the v0.5 measure callback
             (story #29 — text drives hug sizing), the v0.4 retained
             Taffy tree + pruned readback (story #164), the v0.4 Visible
             lowering (story #165), and the v0.4 variant-switch FLIP
             (story #22)

## Purpose

`dashscene-engine` is the runtime that resolves the model
(`docs/archive/2026-07-14-design-1-seed.md` §7.1): the one Taffy solve every backend shares (P2).
It implements `dashscene-core`'s `LayoutSolver` seam
(`docs/decisions/layout-solver-seam.md`); producers commit flex
scenes through `txn.commit_with(&mut TaffySolver::new())`.

Source: `crates/dashscene-engine/src/lib.rs`. Acceptance path:
`crates/dashscene-engine/tests/solve.rs` (hand-computed rects).

## The solve

One Taffy tree per arena root — roots are independent coordinate
islands, translated by their authored offsets at readback. The tree
builds from the arena's read seam (`roots`/`children`/`layout`),
solves with max-content available space, and reads back absolute
rects by accumulating parent origins (Taffy reports parent-relative
locations). f32 passthrough: Taffy's default whole-pixel rounding is
disabled (`disable_rounding`), asserted by test on fractional
geometry (R7; deterministic given the same intent — exact bits follow
Taffy's evaluation order).

### Retained tree + pruned readback (v0.4, story #164)

`LayoutSolver` takes `&mut self` so the solver can retain state across
solves; #164 realizes it. The first solve builds the tree; every later
solve reuses it. `TaffySolver` holds a `TreeState` — the persistent
`TaffyTree`, a `taffy_of: Vec<taffy::NodeId>` map keyed by arena `NodeId`
slot (so a `NodeId` maps to a stable Taffy node across solves),
`parent_of`, the roots, and the previous relative layouts and root
origins for the readback prune. A `solves` counter is exposed
(`solves()`) so a test can assert that a paint-only commit performed no
solve.

`solve` dispatches on whether the tree is structurally current — a
**grown** arena node count forces a full `rebuild` returning every node;
otherwise `incremental` runs. Incremental invalidates only the nodes
`arena.layout_dirty()` names, through Taffy's `set_style` (which marks a
node and its ancestor chain dirty) plus `set_node_context` for their
measure inputs; a clean subtree returns from Taffy's cache without
re-descent. An empty dirty set is the paint-only fast path — `solve`
returns no rects and never calls Taffy.

**Pruned readback** (`read_back_pruned`) is the only genuinely new layout
logic. Taffy stores layouts relative to the parent; converting to
absolute naively is an `O(n)` walk that would consume the win. Instead a
node is emitted only when its relative layout changed **or** its parent's
absolute origin moved, and the walk descends into a subtree only when the
node moved or it lies on the path to a dirty descendant (`on_path` = the
dirty set plus its ancestors). A subtree that neither moved nor guards a
dirty descendant is skipped whole, so the readback — like the solve —
scales with the change. This is what lets `commit_with` accept a
partial solve (`docs/decisions/layout-solver-seam.md`).

## Style mapping (Layout → taffy::Style)

Container side (how a node lays out its children):

| intent             | taffy                                    |
| ------------------ | ---------------------------------------- |
| mode `None`        | `display: Block`; children positioned    |
|                    | `Absolute` at their authored offsets —   |
|                    | the passthrough, asserted equal (for     |
|                    | fixed-sized trees) to `commit()`         |
| mode `Horizontal`/ | `display: Flex` + `flex_direction`,      |
| `Vertical`         | `gap`, `padding`, `justify_content` from |
|                    | `MainAxisAlign`, `align_items` from      |
|                    | `CrossAxisAlign` (never `Stretch` at the |
|                    | container level)                         |

Child side (axis-relative — the parent's direction decides which
authored axis is main):

| sizing  | main axis                           | cross axis          |
| ------- | ----------------------------------- | ------------------- |
| `Fixed` | `flex_basis: length`, grow/shrink 0 | `size: length`      |
| `Hug`   | `flex_basis: auto`, grow/shrink 0   | `size: auto`        |
| `Fill`  | `flex_basis: 0`, grow/shrink 1      | `align_self:        |
|         |                                     | Stretch`, size auto |

`min_width`/`max_width`/`min_height`/`max_height` map to
`min_size`/`max_size` (absent = auto). `margin` maps to
`taffy::Style::margin` (a `Rect` of `LengthPercentageAuto`); negative
margins are legal and express overlap — the target the negative-gap
lowering rewrites to (`docs/decisions/negative-gap-lowering.md`).

Degenerate constructs, all pinned by test and named here for the
validator slice to diagnose (P4):

- A `Fill` child under a mode-`None` parent has no free-space axis
  and behaves as `Hug`.
- A `Fill` root has nothing to fill (no viewport concept yet) and
  collapses to content size.
- `Hug` keeps its content-wrapping meaning under a mode-`None`
  parent too (a hug group inside a plain frame is real vocabulary):
  a childless `Hug` node sizes to zero — authored width/height feed
  `Fixed` sizing only. The `commit()`-equivalence guarantee therefore
  applies to fixed-sized trees; trees using `Hug`/`Fill` are solver
  vocabulary the fixed resolve deliberately ignores.

The single authored `gap` maps to both taffy gap axes; the cross-axis
half is inert until wrap (v0.8), which decides whether row and column
gaps become separate authored properties.

## Visibility (v0.4, issue #165)

`Prop::Visible(false)` overrides both sides of the style mapping above:
`style_for` sets `Display::None` on the node's own style regardless of
its layout mode. Taffy's `Display::None` hides the node from its
parent's flow — the container's flex sizing (Hug, Fill splits) no
longer accounts for it, so a hidden child's share collapses and its
siblings reflow — and recursively hides every descendant during layout
regardless of the descendant's own style, computing a zeroed-out
(degenerate) layout for the whole hidden subtree. `commit()`'s
`FixedSolver` (`dashscene-core`) ignores `Visible`, like the rest of
the flex vocabulary; the fixed-commit equivalence guarantee does not
extend to it.

## Measure callback — text drives hug sizing

Text enters the solve through Taffy's per-node measure callback
(`compute_layout_with_measure`), added at v0.5 (story #29). A node that
carries both text content and a text style
(`Arena::text`/`Arena::text_style`, story #26) becomes a Taffy leaf
with a `TextContext` — the paragraph text and the render size (px per
em in document units). Every other node is a context-free leaf whose
measure is a no-op, so a text-free scene solves exactly as before.

Taffy calls the measure function for each text leaf during the solve.
`measure_text` lays the text out through the typesetter and returns its
box. The wrap width is the width Taffy has already fixed for the node
if there is one, else a definite available width, else none: a
min/max-content probe imposes no wrap, so an unconstrained hug node
lays its paragraph on one line and hugs that natural width. A hug-sized
text node therefore solves to its shaped width and height; a
width-constrained one keeps its width and grows taller as the text
wraps. A known axis is returned unchanged, so measurement never
overrides a dimension Taffy has already fixed.

### One cache, borrowed not owned

The typesetter is passed in, never constructed here.
`TaffySolver::with_typesetter(&mut Typesetter)` borrows the caller's
single `Typesetter` for the solve; `TaffySolver::new()` carries none —
the text-free path, and what every non-text solve and the fixed-commit
equivalence tests use. The borrow is the single-source discipline:
layout measures text against the same shaped-run cache the painter
reads at paint time (#30), so the two cannot disagree about a glyph's
size (P2 — one typesetter). The shaped-run cache stores font-unit,
unpositioned runs keyed by paragraph text alone
(`docs/decisions/shaped-run-cache-font-units.md`), so one entry serves
every render size and re-measuring unchanged text costs a lookup, not a
re-shape.

The `TextContext` owns its text so the Taffy tree can outlive the arena
borrow. Shaping itself is not repeated across solves, because the cache
sits in front of it. The v0.4 retained tree (#164, "Retained tree +
pruned readback" above) rebased onto this measure seam and the
`with_typesetter` signature: the incremental solve refreshes a node's
`TextContext` through `set_node_context` for the nodes it re-styles, so a
dirtied text/style node re-measures while a clean one keeps Taffy's
cached measurement — which is why the contract is recorded here and in
`docs/decisions/measure-callback-typesetter-seam.md` rather than left as
wiring.

## Variant-switch FLIP (v0.4, story #22)

`crates/dashscene-engine/src/flip.rs` animates the layout delta a variant
switch (or any re-solve) produces. It is a thin engine-side binder onto
`dashcue`, not standalone geometry math and not a `dashcue` producer:

- `Channel { X, Y, W, H }` and `prop_key(node, channel) -> dashcue::PropKey`
  pack `(node index << 2) | channel` into `dashcue`'s opaque key; the
  engine owns this packing, the way `dashlang`'s reactive layer owns its
  own `PropKey` packing.
- `VariantFlip::start(before, after, &dashcue::VariantTransition)` takes the
  two solved layouts as `&[(NodeId, SolvedRect)]` slices and binds a
  caller-declared transition: it resolves each track's `from`/`to` from the
  before/after rects and hands them to `dashcue`'s `Scheduler`. `dashcue`
  carries no resolved values (P1), so the engine binds them at commit time.
- `advance(dt)`, then `sample(node)` / `sampled_rects()` reassemble a full
  `SolvedRect` per node by overlaying the live per-channel scheduler samples
  on the `after` target.

The two snapshots need no new bookkeeping: `commit` writes the back buffer
while the previous generation's rects are still live in the front buffer,
so the caller reads `before` and `after` straight from `arena.committed()`
across the switch (this is the previous-commit-geometry accessor
`docs/decisions/layout-solver-seam.md` anticipated for #22). A mid-flight
retarget resumes from the current sample, because `start` delegates
interruption to the scheduler's retarget rule. Acceptance is in
`crates/dashscene-engine/tests/flip.rs` (a linear tween between two
layouts; a second switch mid-flight that retargets without snapping; a
spring FLIP that replays bit-identically, E5).

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.1 (Taffy as
  sole solver, R2 vocabulary), §7.2 (the common runtime's measure
  callback — text drives hug sizing), and §6.3 (FLIP);
  `docs/roadmap.md`'s v0.2, v0.4, and v0.5; issue #9, issue #29, issue
  #165, issue #164, and issue #22 acceptance criteria.
- Blocks: #10 (negative-gap lowering), #11 (flex goldens), #43 (v0.8
  layout fidelity). The measure seam blocks #30 (the hug-sizing text
  golden). The retained tree (#164) and Visible lowering (#165) serve
  #166 (the reactive layer's contained-write skip and bounded pools);
  FLIP (#22) serves #23 (the FLIP golden sampling).
- Related decisions: `docs/decisions/layout-solver-seam.md` (the
  partial-solve contract #164 extended, and the FLIP hook),
  `docs/decisions/flex-vocabulary-shape.md`,
  `docs/decisions/measure-callback-typesetter-seam.md`,
  `docs/decisions/shaped-run-cache-font-units.md`,
  `docs/decisions/visible-is-layout-opacity-is-paint.md`.
- Related design: `docs/design/typeset-latin.md` (the shaped-run cache
  the measure callback consumes); `docs/design/dashscene-core-arena.md`
  ("Incremental commit"); `docs/design/dashcue.md` (the scheduler and
  `VariantTransition` FLIP binds).
