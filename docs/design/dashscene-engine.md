# dashscene-engine — the Taffy layout solve

    crate    crates/dashscene-engine
    covers   v0.2 flex core (story #9) and the v0.5 measure callback
             (story #29 — text drives hug sizing); variants/FLIP (v0.4)
             land at their own slice

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
locations; readback advances a cursor over the build-order pairing,
O(n)). f32 passthrough: Taffy's default whole-pixel rounding is
disabled (`disable_rounding`), asserted by test on fractional
geometry (R7; deterministic given the same intent — exact bits follow
Taffy's evaluation order).

The tree is rebuilt from scratch every solve — deliberate at v0.2
scale. `LayoutSolver` takes `&mut self` exactly so this solver can
hold retained trees (Taffy supports style updates + dirty marking)
when per-frame animated commits arrive (v0.4, #22); revisit then.

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
borrow. The tree is still rebuilt from scratch every solve, as the rest
of the solve is at this scale; shaping itself is not repeated, because
the cache sits in front of it. The v0.4 retained Taffy tree (#164) will
keep the tree across commits and must then invalidate a node's cached
measurement when its text or style changes — it rebases onto this
measure seam and the `with_typesetter` signature, which is why the
contract is recorded here and in
`docs/decisions/measure-callback-typesetter-seam.md` rather than left as
wiring.

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.1 (Taffy as
  sole solver, R2 vocabulary) and §7.2 (the common runtime's measure
  callback — text drives hug sizing), `docs/roadmap.md`'s v0.2 and
  v0.5; issue #9 and issue #29 acceptance criteria.
- Blocks: #10 (negative-gap lowering), #11 (flex goldens), #22 (FLIP),
  #43 (v0.8 layout fidelity). The measure seam blocks #30 (the
  hug-sizing text golden) and #164 (the v0.4 retained Taffy tree, which
  invalidates a cached measurement when text changes).
- Related decisions: `docs/decisions/layout-solver-seam.md`,
  `docs/decisions/flex-vocabulary-shape.md`,
  `docs/decisions/measure-callback-typesetter-seam.md`,
  `docs/decisions/shaped-run-cache-font-units.md`.
- Related design: `docs/design/typeset-latin.md` (the shaped-run cache
  the measure callback consumes).
