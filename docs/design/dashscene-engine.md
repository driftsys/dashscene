# dashscene-engine — the Taffy layout solve

    crate    crates/dashscene-engine
    covers   v0.2 flex core (story #9); variants/FLIP (v0.4) and the
             measure callback (v0.5) land at their own slices

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

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.1 (Taffy as
  sole solver, R2 vocabulary), `docs/roadmap.md`'s v0.2; issue #9
  acceptance criteria.
- Blocks: #10 (negative-gap lowering), #11 (flex goldens), #22 (FLIP),
  #29 (measure callback), #43 (v0.8 layout fidelity).
- Related decisions: `docs/decisions/layout-solver-seam.md`,
  `docs/decisions/flex-vocabulary-shape.md`.
