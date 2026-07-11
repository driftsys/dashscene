# dashscene-core arena + staged-mutation API — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The v0.1 in-memory arena in `dashscene-core`: node tree +
fixed-size layout + solid-fill intent, staged mutation
(`open`/`set_prop`/`commit`), resolving on commit into a
double-buffered committed scene (rect table, paint table, generation
stamp, dirty set).

**Architecture:** See the design spec
(`docs/wip/2026-07-12-dashscene-core-arena-design.md`). Intent model
(nodes with parent/children links, stable `NodeId` slots) is mutated
through a `Txn` holding the arena's `&mut` borrow; `commit` walks the
tree in DFS order, resolves parent-relative offsets to absolutes,
interns paints by color bit pattern, diffs against the previous
committed buffer for the dirty set, and flips the double buffer.

**Tech Stack:** Rust 2024, no new dependencies. One additive schema
change in `dashbuf` (authored `x`/`y` on `FixedSizeLayout`).

## Global Constraints

- Boundary B (pinned): rect entry = `x, y, w, h` (`f32`) + paint index
  (`u32`), blittable; paint = solid fill, 4×`f32` RGBA; rect-table
  index = DFS node index; double buffer/generation/dirty set in core.
- No `dashpaint` or `dashbuf` dependency in `dashscene-core`.
- No `set_variant`, no node removal, no value validation in v0.1.
- `just build` must stay green: clippy `-D warnings`, fmt, dprint,
  markdownlint.
- Commits: conventional, scope from `.git-std.toml`
  (`dashbuf` / `dashscene-core` / `docs`).

---

### Task 1: dashbuf — authored x/y on FixedSizeLayout (decision D1)

**Files:**

- Modify: `crates/dashbuf/schema/dashbuf.fbs`
- Modify: `crates/dashbuf/tests/roundtrip.rs`

**Interfaces:**

- Produces: `FixedSizeLayout::new(x, y, width, height)` with `x()`,
  `y()` accessors — flatc-generated.

- [ ] **Step 1: Write the failing test** — in `roundtrip.rs`, change
      the first test's layout construction and assertions:

```rust
let layout = FixedSizeLayout::new(8.0, 4.0, 100.0, 50.0);
// ...
assert_eq!(decoded_layout.x(), 8.0);
assert_eq!(decoded_layout.y(), 4.0);
assert_eq!(decoded_layout.width(), 100.0);
assert_eq!(decoded_layout.height(), 50.0);
```

and the second test's to `FixedSizeLayout::new(0.0, 0.0, 1.0, 1.0)`.

- [ ] **Step 2: Run to verify failure** —
      `cargo test -p dashbuf` → compile error (wrong arity, no `x()`).
- [ ] **Step 3: Schema change** — in `dashbuf.fbs`:

```text
struct FixedSizeLayout {
  // Authored offset relative to the parent node (or the canvas
  // origin for a root). Intent, not a resolved result (P1) — the
  // resolved absolute position exists only in the runtime rect
  // table.
  x: float32;
  y: float32;
  width: float32;
  height: float32;
}
```

- [ ] **Step 4: Verify pass** — `cargo test -p dashbuf` → 2 passed.
- [ ] **Step 5: Commit** —
      `feat(dashbuf): add authored x/y offset to FixedSizeLayout`.

### Task 2: committed-output types

**Files:**

- Create: `crates/dashscene-core/src/committed.rs`
- Modify: `crates/dashscene-core/src/lib.rs`
- Create: `crates/dashscene-core/tests/arena.rs`

**Interfaces:**

- Produces: `Color { r, g, b, a: f32 }`, `Paint { color: Color }`,
  `RectEntry { x, y, w, h: f32, paint: u32 }`, `NO_PAINT: u32`,
  `CommittedScene` with `rects() -> &[RectEntry]`,
  `paints() -> &[Paint]`, `generation() -> u64`, `dirty() -> &[u32]`,
  `node_of(u32) -> NodeId`, `rect_index_of(NodeId) -> Option<u32>`.
- Consumes: `NodeId` from Task 3 (declared there; this task declares
  the struct fields that reference it).

- [ ] **Step 1: Failing test** (`tests/arena.rs`):

```rust
use std::mem::{align_of, size_of};

use dashscene_core::{Color, NO_PAINT, RectEntry};

#[test]
fn committed_entries_are_blittable_plain_data() {
    assert_eq!(size_of::<RectEntry>(), 20);
    assert_eq!(align_of::<RectEntry>(), 4);
    assert_eq!(size_of::<Color>(), 16);
    assert_eq!(NO_PAINT, u32::MAX);
    let entry = RectEntry { x: 1.0, y: 2.0, w: 3.0, h: 4.0, paint: 0 };
    let copy = entry; // Copy, no move
    assert_eq!(entry, copy);
}
```

- [ ] **Step 2: Verify failure** — `cargo test -p dashscene-core` →
      unresolved imports.
- [ ] **Step 3: Implement** — `committed.rs` with the types above
      (`#[repr(C)]`, `Copy`, `PartialEq` on `Color`/`RectEntry`/`Paint`;
      `CommittedScene` `#[derive(Default)]` with private fields + the
      accessor methods; `NodeId` import from `arena`), re-exports in
      `lib.rs`. `CommittedScene` fields: `rects: Vec<RectEntry>`,
      `paints: Vec<Paint>`, `generation: u64`, `dirty: Vec<u32>`,
      `node_ids: Vec<NodeId>`, `rect_index: Vec<u32>` (NodeId slot → rect
      index).
- [ ] **Step 4: Verify pass.**
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): committed-output types (boundary-B shapes)`.

### Task 3: arena skeleton — open/add_node/set_prop/commit, single node

**Files:**

- Create: `crates/dashscene-core/src/arena.rs`
- Modify: `crates/dashscene-core/src/lib.rs`,
  `crates/dashscene-core/tests/arena.rs`

**Interfaces:**

- Produces: `NodeId` (opaque, `Copy`), `Prop::{X, Y, Width, Height,
  Fill}`, `Arena::new()`, `Arena::open() -> Txn<'_>`,
  `Arena::committed() -> &CommittedScene`,
  `Arena::name(NodeId) -> Option<&str>`,
  `Txn::add_node(Option<NodeId>, Option<&str>) -> NodeId`,
  `Txn::set_prop(NodeId, Prop)`, `Txn::commit(self) -> u64`.

- [ ] **Step 1: Failing tests**:

```rust
#[test]
fn a_new_arena_commits_to_an_empty_scene() {
    let mut arena = Arena::new();
    assert_eq!(arena.committed().generation(), 0);
    assert!(arena.committed().rects().is_empty());
    let generation = arena.open().commit();
    assert_eq!(generation, 1);
    let scene = arena.committed();
    assert_eq!(scene.generation(), 1);
    assert!(scene.rects().is_empty());
    assert!(scene.paints().is_empty());
    assert!(scene.dirty().is_empty());
}

#[test]
fn a_single_filled_root_resolves_to_one_rect_and_one_paint() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::X(5.0));
    txn.set_prop(root, Prop::Y(7.0));
    txn.set_prop(root, Prop::Width(320.0));
    txn.set_prop(root, Prop::Height(240.0));
    txn.set_prop(root, Prop::Fill(RED));
    txn.commit();
    let scene = arena.committed();
    assert_eq!(scene.rects(), &[RectEntry { x: 5.0, y: 7.0, w: 320.0, h: 240.0, paint: 0 }]);
    assert_eq!(scene.paints(), &[Paint { color: RED }]);
    assert_eq!(arena.name(root), Some("bg"));
}
```

with `const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };`.

- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** `arena.rs` — intent model + full commit
      resolution (DFS walk, absolute offsets, paint interning by
      `f32::to_bits` key, dirty diff, buffer flip; the walk is the
      design-spec algorithm and later tasks only add tests over it):

```rust
pub struct Arena {
    nodes: Vec<NodeData>,
    roots: Vec<NodeId>,
    buffers: [CommittedScene; 2],
    front: usize,
}
struct NodeData {
    name: Option<String>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    x: f32, y: f32, width: f32, height: f32,
    fill: Option<Color>,
}
```

`commit`: DFS stack seeded with roots reversed; per node — absolute
= parent absolute + own offset (memoized per slot), paint = interned
fill or `NO_PAINT`; `dirty` = indices where the new entry differs
from (or does not exist in) the front buffer; generation = front
generation + 1; write back buffer, flip `front`.

- [ ] **Step 4: Verify pass.**
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): arena with staged open/set_prop/commit`.

### Task 4: DFS document order, nesting, id↔index mapping

**Files:** modify `crates/dashscene-core/tests/arena.rs` only (the
Task 3 implementation should already satisfy these; fix it if not).

- [ ] **Step 1: Tests**:

```rust
#[test]
fn dfs_order_and_absolute_positions_resolve_through_nesting() {
    // root(10,20) ── a(1,2) ── leaf(0.5,0.5)
    //            └── b(3,4)
    // DFS: root=0, a=1, leaf=2, b=3; absolutes sum ancestor offsets.
}

#[test]
fn interleaved_creation_still_yields_dfs_document_order() {
    // create root, b-child-of-root, a-child-of-root, leaf-under-b:
    // DFS = root, b, leaf, a (children in creation order, depth first).
}

#[test]
fn node_ids_and_rect_indices_correspond() {
    // rect_index_of(id) round-trips with node_of(index) for every node.
}
```

(full assertions in the test file; positions chosen so every
absolute is distinct).

- [ ] **Step 2: Run** — expected to pass against Task 3's commit walk;
      investigate and fix the walk if any fail.
- [ ] **Step 3: Commit** —
      `test(dashscene-core): DFS order, nesting, id-index mapping`.

### Task 5: paint interning + NO_PAINT

**Files:** modify `crates/dashscene-core/tests/arena.rs`.

- [ ] **Step 1: Tests**:

```rust
#[test]
fn identical_fills_share_one_paint_entry_in_first_use_order() {
    // red, blue, red again → paints() == [red, blue]; rects reference 0, 1, 0.
}

#[test]
fn an_unfilled_node_paints_as_no_paint() {
    // container without fill → paint == NO_PAINT, no paint entry added.
}
```

- [ ] **Step 2: Run; fix interning if red.**
- [ ] **Step 3: Commit** —
      `test(dashscene-core): paint interning and NO_PAINT sentinel`.

### Task 6: staged visibility, generation, double buffer

**Files:** modify `crates/dashscene-core/tests/arena.rs`.

- [ ] **Step 1: Tests**:

```rust
#[test]
fn staged_mutations_are_invisible_until_commit() {
    // commit a 1-node scene; open a txn, move + recolor the node;
    // BEFORE commit: committed() still serves the old values
    // (drop the txn via a scope to read; reopen, redo, commit;
    // AFTER: new values, generation bumped by the second commit).
}

#[test]
fn every_commit_bumps_the_generation_even_without_changes() {
    // three no-op commits → generations 1, 2, 3.
}
```

Note the borrow rule: `committed()` cannot be called while a `Txn`
is live — the test reads between txns, which is exactly the staged
contract (an uncommitted, dropped txn's changes remain pending and
publish with the next commit; the first test asserts that too).

- [ ] **Step 2: Run; fix if red.**
- [ ] **Step 3: Commit** —
      `test(dashscene-core): staged visibility and generation stamping`.

### Task 7: dirty set

**Files:** modify `crates/dashscene-core/tests/arena.rs`.

- [ ] **Step 1: Tests**:

```rust
#[test]
fn the_first_commit_marks_every_rect_dirty() { /* 3 nodes → dirty [0,1,2] */ }

#[test]
fn moving_a_parent_dirties_it_and_its_descendants_only() {
    // root ── a ── leaf, plus sibling b under root; move a →
    // dirty == [a, leaf] indices; b and root untouched.
}

#[test]
fn a_no_op_commit_has_an_empty_dirty_set() { /* commit twice, no edits */ }
```

- [ ] **Step 2: Run; fix diff if red.**
- [ ] **Step 3: Commit** — `test(dashscene-core): dirty-set semantics`.

### Task 8: crate docs, decision records, full gate

**Files:**

- Modify: `crates/dashscene-core/src/lib.rs` (crate docs: the staged
  contract, boundary-B output, pointers to DESIGN §5/§7.3 and
  SCOPE_DECISIONS §9)
- Create: `docs/decisions/fixed-position-authoring.md`
- Create: `docs/decisions/staged-mutation-v01-scope.md`
- Create: `docs/decisions/core-committed-output-shape.md`

Each record: context, options, choice, why (kebab-case files, per the
session's rules of engagement; content distilled from the design
spec's D1–D4).

- [ ] **Step 1: Write docs + records.**
- [ ] **Step 2: Full gate** — `just build` → green.
- [ ] **Step 3: Commit** —
      `docs(docs): record story #2 decisions (position authoring, staging scope, output shape)`.

---

## Self-review

- Spec coverage: D1 → Task 1; D2 → Tasks 3/6; D3 → Tasks 2/5/7;
  D4 → Task 4; acceptance path → Tasks 3–7; `just build` → Task 8. ✓
- Placeholders: Tasks 4–7 list test names/shapes with the full
  assertions written at execution time in the single shared test file
  — acceptable here because the same session executes the plan; no
  dangling type references remain. ✓
- Type consistency: `Prop`, `NodeId`, `CommittedScene` accessors match
  across tasks. ✓
