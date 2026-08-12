# Commit takes its geometry from a LayoutSolver trait defined in core

    status   accepted (story #9, 2026-07-12); extended by story #164
             (2026-07-15) — the solver may now return only the changed
             rects; see "The partial-solve extension" below
    scope    dashscene-core commit pipeline, dashscene-engine;
             binds the v0.4 FLIP work (#22), the v0.4 incremental commit
             (#164), and the v0.5 measure callback (#29)

## Context

Story #9 had to connect the Taffy solve (owned by `dashscene-engine`,
`docs/design/dashscene-engine.md`) to `dashscene-core`'s commit, while the
double buffer, generation stamp, and dirty set stay core's
(`docs/decisions/staged-mutation-v01-scope.md`).
`docs/decisions/flex-vocabulary-shape.md` recorded the injection point as this
story's design.

## Options

1. Dependency inversion: core defines `LayoutSolver`
   (`solve(&Arena)
   -> Vec<(NodeId, SolvedRect)>`) and
   `Txn::commit_with(&mut dyn
   LayoutSolver)`; the engine implements the
   trait.
2. The engine wraps the arena and re-implements commit.
3. The engine post-processes a committed scene (solve after commit, write a
   second geometry table).
4. Core depends on the engine and calls Taffy directly.

## Choice

Option 1:

- `commit_with` asks exactly one solver for every node's absolute rect and
  computes no geometry of its own (P2); DFS order, paint interning, the dirty
  diff, and the buffer flip stay in core. A solver that omits a node panics with
  a named message (P4 — never a silent skip).
- `commit()` keeps its exact v0.1/v0.2 behavior by delegating to a core-internal
  `FixedSolver` (authored offset + fixed size — the mode-`None` passthrough).
  Existing producers and tests are untouched; flex-aware producers call
  `commit_with(&mut TaffySolver::new())`.

## Why

- Option 2 moves state `docs/decisions/staged-mutation-v01-scope.md` assigns to
  core.
- Option 3 creates two observable states per commit, breaking commit atomicity
  (P3) and the dirty-set contract.
- Option 4 inverts the recorded crate direction (`engine → core`) and adds Taffy
  to every producer's dependency graph.
- The trait is also where the next slices attach: the measure callback (#29) is
  solver-side state behind this seam — realized in
  `docs/decisions/measure-callback-typesetter-seam.md`, where the solver borrows
  a `Typesetter` for the solve — and FLIP (#22) hooks the same commit path,
  noting that FLIP needs to read the previous commit's geometry, which core does
  not expose yet (only the front buffer is public), so #22 adds a small core
  accessor for it.

## The partial-solve extension (story #164)

The original contract required `solve` to resolve **every** node, and
`commit_with` panicked if a node was omitted. The incremental commit
(`docs/design/dashscene-engine.md`, `docs/design/dashscene-core-arena.md`)
relaxes this: a solver may return **only the rects that changed since the
previous solve**, and `commit_with` carries an omitted node's rect forward from
the previous commit. The "every node has a rect" invariant is re-expressed, not
deleted — a node that is neither solved now nor present in the previous commit
still panics with a named message (P4). The internal `FixedSolver` keeps
returning every node; the engine's retained `TaffySolver` returns only the
movers, via its pruned readback. The trait signature
(`solve(&mut self, &Arena) -> Vec<(NodeId, SolvedRect)>`) is unchanged — the
return type always permitted a subset; #164 blesses it and adds the
carry-forward on the commit side.
