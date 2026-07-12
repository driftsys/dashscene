# dashscene-engine Taffy solve — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Taffy layout solve behind a `LayoutSolver` seam, per the
design spec (`docs/wip/2026-07-12-engine-taffy-design.md`).

**Architecture:** design spec D1–D3.

**Tech Stack:** taffy 0.12 (workspace-pinned), Rust 2024.

## Global Constraints

- `commit()`'s observable behavior must not change (design test 1).
- One resolution pipeline in core; geometry pluggable via the trait.
- `just build` green; scopes `dashscene-core` / `dashscene-engine` /
  `deps` / `docs`.

---

### Task 1: core — the LayoutSolver seam (behavior-neutral)

**Files:** `crates/dashscene-core/src/arena.rs`, `src/lib.rs`,
`tests/arena.rs`

**Interfaces produced:** `SolvedRect { x, y, w, h: f32 }` (Copy),
`trait LayoutSolver { fn solve(&mut self, &Arena) -> Vec<(NodeId, SolvedRect)> }`,
`Txn::commit_with(&mut dyn LayoutSolver) -> u64`; `commit()`
delegating to the internal `FixedSolver`.

- [ ] **Step 1: Failing tests** — design tests 2 and 3 (stub solver's
      rects land verbatim in the committed table; omitted node panics
      with a named message).
- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** — extract the geometry computation from
      the commit walk into `FixedSolver` (an internal `LayoutSolver`);
      `commit_with` runs DFS, collects the solver result into a
      slot-indexed lookup, builds rects (panicking on a missing node),
      then interns/diffs/flips exactly as today; `commit()` calls
      `commit_with(&mut FixedSolver)`.
- [ ] **Step 4: Verify pass** — full core suite green (design
      test 1), new tests green.
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): pluggable LayoutSolver seam for commit`.

### Task 2: engine — TaffySolver

**Files:** `Cargo.toml` (workspace dep `taffy = "0.12"`),
`crates/dashscene-engine/Cargo.toml`, `src/lib.rs` (or `src/solver.rs`
if lib.rs passes ~250 lines), `crates/dashscene-engine/tests/solve.rs`

**Interfaces produced:** `TaffySolver::new()`,
`impl dashscene_core::LayoutSolver for TaffySolver`.

- [ ] **Step 1: Failing tests** — the engine test list, design spec (hand-computed
      rects; the design spec fixes the numbers for the first case, the rest are
      computed in-test from the same arithmetic).
- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** — per-root Taffy tree build (style
      mapping per design D2), `compute_layout`, absolute readback (D3).
- [ ] **Step 4: Verify pass** — engine + workspace suites.
- [ ] **Step 5: Commit** —
      `feat(dashscene-engine): Taffy solve over the LayoutSolver seam`.

### Task 3: records + gate

**Files:** `docs/decisions/layout-solver-seam.md` (D1),
`docs/decisions/flex-vocabulary-shape.md` (edit in place: the open
injection point now resolves to the new record),
`docs/design/dashscene-engine.md` (new as-built record: mapping table
D2/D3), `docs/decisions/README.md` + `docs/design/README.md` indexes,
`docs/design/dashscene-core-arena.md` (commit pipeline step 2 now
delegates geometry to the solver).

- [ ] **Step 1: Write/edit records.**
- [ ] **Step 2: `just build` green.**
- [ ] **Step 3: Commit** —
      `docs(docs): record the layout-solver seam and the Taffy mapping (story #9)`.

---

## Self-review

- Spec coverage: D1 → Task 1; D2/D3 → Task 2; records → Task 3;
  all design-spec tests assigned. ✓
- Placeholders: none; test numbers for the representative case are in
  the design spec. ✓
- Type consistency: `SolvedRect`/`LayoutSolver`/`commit_with` names
  match across tasks. ✓
