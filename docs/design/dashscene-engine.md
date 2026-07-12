# dashscene-engine — the Taffy layout solve

    crate    crates/dashscene-engine
    covers   v0.2 flex core (story #9); variants/FLIP (v0.4) and the
             measure callback (v0.5) land at their own slices

## Purpose

`dashscene-engine` is the runtime that resolves the model
(DESIGN_1.md §7.1): the one Taffy solve every backend shares (P2).
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
locations). f32 passthrough, no rounding (R7; deterministic given the
same intent).

## Style mapping (Layout → taffy::Style)

Container side (how a node lays out its children):

| intent             | taffy                                    |
| ------------------ | ---------------------------------------- |
| mode `None`        | `display: Block`; children positioned    |
|                    | `Absolute` at their authored offsets —   |
|                    | the passthrough, asserted equal to       |
|                    | `commit()`'s fixed resolution            |
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
`min_size`/`max_size` (absent = auto). A `Fill` child under a
mode-`None` parent has no free-space axis and behaves as `Hug`; the
validator diagnoses that construct at its own slice (P4).

## Trace

- Satisfies: DESIGN_1.md §7.1 (Taffy as sole solver, R2 vocabulary),
  §11 v0.2; issue #9 acceptance criteria.
- Blocks: #10 (negative-gap lowering), #11 (flex goldens), #22 (FLIP),
  #29 (measure callback), #43 (v0.8 layout fidelity).
- Related decisions: `docs/decisions/layout-solver-seam.md`,
  `docs/decisions/flex-vocabulary-shape.md`.
