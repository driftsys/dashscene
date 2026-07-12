# dashscene-engine v0.2 — Taffy solve (design)

    story    #9 (epic #7, v0.2 flex core)
    branch   story/engine-taffy
    date     2026-07-12
    status   working memory — garden before the PR lands

## Goal

Introduce Taffy as the sole layout solver (DESIGN_1.md §7.1, P2):
map `dashscene-core`'s intent tree to a Taffy tree, solve H/V flex
(hug/fill/fixed per axis, gap, padding, alignment, min/max), and
write the resolved rects into the double-buffered committed output on
commit. Mode `None` stays a passthrough, not a second engine.

Acceptance (issue #9): unit tests for representative H/V,
hug-in-fill, fill-weight, and min/max cases against hand-computed
rects; `just build` green.

## Decisions (alternatives considered)

### D1 — The solve-injection seam: a `LayoutSolver` trait in core

The open point `docs/decisions/flex-vocabulary-shape.md` left for
this story: how solved rects reach core's committed output while the
double buffer, generation, and dirty set stay core's
(SCOPE_DECISIONS.md §9).

- **Chosen:** dependency inversion. `dashscene-core` defines:

      pub struct SolvedRect { pub x, y, w, h: f32 }   // absolute
      pub trait LayoutSolver {
          /// Resolve every node to an absolute rect. Called by commit
          /// with the arena's read seam (roots/children/layout).
          fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)>;
      }

  and `Txn::commit_with(&mut dyn LayoutSolver) -> u64`: DFS order,
  ask the solver for geometry, intern paints, dirty-diff, flip — one
  resolution pipeline, geometry pluggable. `commit()` keeps its exact
  v0.1 behavior by delegating to a core-internal `FixedSolver`
  (authored offset + fixed size — the passthrough resolution), so
  every existing producer and test is untouched. `commit_with`
  asserts the solver returned a rect for every node (P4: a missing
  node is a broken contract, panic — not a silent skip).
  `dashscene-engine` implements `LayoutSolver` with Taffy; producers
  that want flex call `txn.commit_with(&mut TaffySolver::new())`.
- Rejected — engine wraps/owns the arena and re-implements commit:
  moves the double buffer/generation/dirty out of core, against
  SCOPE_DECISIONS §9.
- Rejected — engine post-processes a committed scene (solve after
  commit, write a second table): two observable states per commit,
  breaks commit atomicity (P3) and the dirty-set contract.
- Rejected — core depends on engine (call Taffy directly from
  commit): inverts the crate graph (`engine → core` is the recorded
  direction) and drags Taffy into every producer build.

### D2 — Taffy style mapping (axis-relative, Figma-shaped)

One mapping function `Layout → taffy::Style`, engine-internal:

- Container, mode `None`: `display: Block`; each child is
  `position: Absolute` with `inset` left/top from its authored
  `x`/`y` — Taffy resolves absolute children against the parent box,
  which reproduces core's fixed resolution (passthrough by
  construction, asserted by test D-t5).
- Container, mode `Horizontal`/`Vertical`: `display: Flex`,
  `flex_direction: Row`/`Column`, `gap`, `padding` from `EdgeInsets`,
  `justify_content` from `MainAxisAlign`
  (Start/Center/End/SpaceBetween), `align_items` from
  `CrossAxisAlign` (FlexStart/Center/FlexEnd).
- Child sizing is axis-relative (the Figma→CSS mapping):
  - main axis — `Fixed`: `flex_basis: length(size)`, grow 0,
    shrink 0; `Hug`: basis auto, grow 0, shrink 0; `Fill`: grow 1,
    shrink 1, basis 0.
  - cross axis — `Fixed`: cross `size: length`; `Hug`: `size: auto`,
    `align_self: Start` (do not stretch); `Fill`:
    `align_self: Stretch`, `size: auto`.
  - a child of a `None` parent uses width/height directly (`Fixed`)
    or `auto` (`Hug`); `Fill` under a `None` parent has no free-space
    axis and behaves as `Hug` (documented; the validator diagnoses it
    at its own slice, P4).
- `min_width`/`max_width`/`min_height`/`max_height` →
  `min_size`/`max_size` lengths (absent = auto).
- Roots: each root solves independently (`compute_layout` per root)
  with available space = its own resolved size (fixed size, or
  max-content when hug), then the subtree translates by the root's
  authored `(x, y)`. Multi-root scenes remain independent coordinate
  islands, exactly like core's fixed resolve.

### D3 — Absolute-rect readback

Taffy reports per-node `location` relative to the parent. The engine
walks the solved tree once, accumulating parent origins to produce
the absolute rects core's table carries (same convention as the fixed
resolve). Rounding: none — f32 passthrough of Taffy's f32 output,
deterministic across runs (R7; Taffy is pure Rust, no platform
branches in the used feature set).

## Crate impact

    crates/dashscene-core     LayoutSolver trait, SolvedRect,
                              commit_with, FixedSolver extraction
                              (behavior-neutral refactor of commit)
    crates/dashscene-engine   TaffySolver (Taffy dep, workspace-pinned
                              taffy 0.12), style mapping, readback
    Cargo.toml                taffy in workspace dependencies

## Testing

Core (behavior-neutral seam):

1. `commit()` output is bit-identical before/after the FixedSolver
   extraction (existing suite stays green — that is the assertion).
2. `commit_with` with a stub solver that returns fabricated rects:
   the committed table carries exactly those rects; paints/dirty
   logic unchanged.
3. `commit_with` panics (named message) when the solver omits a node.

Engine (hand-computed cases; all f32-exact):

4. Horizontal row, fixed children, gap + padding:
   `row(pad 10, gap 5)[a 30×20 fixed, b 50×20 fixed]` →
   a=(10,10,30,20), b=(45,10,50,20); container fixed 200×40.
5. Mode-`None` passthrough equivalence: a nested fixed-geometry tree
   solved via `TaffySolver` equals `commit()`'s output exactly.
6. Fill-weight: two `Fill` children split free space equally; a
   `Fixed` sibling keeps its size (hug-in-fill row).
7. Hug-in-fill: a `Hug` container inside a `Fill` sibling context
   sizes to its fixed children, not the free space.
8. Vertical column with `main_align: SpaceBetween` and
   `cross_align: Center`.
9. Min/max: a `Fill` child clamped by `max_width`; a `Hug` child
   floored by `min_height`.
10. Multi-root: two roots keep independent origins (authored x/y).

The golden harness stays on `commit()` (fixed scenes) this story;
flex goldens are #11's scope.
