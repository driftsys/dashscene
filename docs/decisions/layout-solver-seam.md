# Commit takes its geometry from a LayoutSolver trait defined in core

    status   accepted (story #9, 2026-07-12)
    scope    dashscene-core commit pipeline, dashscene-engine;
             binds the v0.4 FLIP work (#22) and the v0.5 measure
             callback (#29)

## Context

Story #9 had to connect the Taffy solve (owned by `dashscene-engine`,
DESIGN_1.md §7.1) to `dashscene-core`'s commit, while the double
buffer, generation stamp, and dirty set stay core's
(SCOPE_DECISIONS.md §9). `docs/decisions/flex-vocabulary-shape.md`
recorded the injection point as this story's design.

## Options

1. Dependency inversion: core defines `LayoutSolver` (`solve(&Arena)
   -> Vec<(NodeId, SolvedRect)>`) and `Txn::commit_with(&mut dyn
   LayoutSolver)`; the engine implements the trait.
2. The engine wraps the arena and re-implements commit.
3. The engine post-processes a committed scene (solve after commit,
   write a second geometry table).
4. Core depends on the engine and calls Taffy directly.

## Choice

Option 1:

- `commit_with` asks exactly one solver for every node's absolute
  rect and computes no geometry of its own (P2); DFS order, paint
  interning, the dirty diff, and the buffer flip stay in core. A
  solver that omits a node panics with a named message (P4 — never a
  silent skip).
- `commit()` keeps its exact v0.1/v0.2 behavior by delegating to a
  core-internal `FixedSolver` (authored offset + fixed size — the
  mode-`None` passthrough). Existing producers and tests are
  untouched; flex-aware producers call
  `commit_with(&mut TaffySolver::new())`.

## Why

- Option 2 moves state SCOPE_DECISIONS §9 assigns to core.
- Option 3 creates two observable states per commit, breaking commit
  atomicity (P3) and the dirty-set contract.
- Option 4 inverts the recorded crate direction (`engine → core`) and
  drags Taffy into every producer build.
- The trait is also the natural landing point for what comes next:
  FLIP (#22) compares solver outputs across commits, and the measure
  callback (#29) is solver-side state — both live behind this seam
  without further core surgery.
