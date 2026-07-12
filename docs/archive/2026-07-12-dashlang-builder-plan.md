# dashlang builder DSL — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The minimal value-tree builder DSL over `dashscene-core`'s
staged-mutation API, per the design spec
(`docs/wip/2026-07-12-dashlang-builder-design.md`).

**Architecture:** Inert `Node` value tree + `scene(...)` collector;
`Scene::build(&mut Arena)` performs one `open`/walk/`commit`. See the
design spec's D1–D3.

**Tech Stack:** Rust 2024; new workspace-internal dependency
`dashlang → dashscene-core`.

## Global Constraints

- The DSL adds vocabulary, never semantics: output must equal the
  hand-built `dashscene-core` equivalent (acceptance criterion,
  issue #5).
- `just build` stays green (clippy -D warnings, fmt, dprint,
  markdownlint).
- Commits: conventional, scopes `dashlang` / `docs`.

---

### Task 1: the DSL — acceptance test first, then the value tree

**Files:**

- Modify: `crates/dashlang/Cargo.toml` (add
  `dashscene-core.workspace = true`; correct the stale description to
  name the `dashscene-core` producer surface per SCOPE_DECISIONS §9)
- Modify: `crates/dashlang/src/lib.rs` (replace the stub with the DSL)
- Create: `crates/dashlang/tests/builder.rs`

**Interfaces:**

- Consumes: `dashscene_core::{Arena, Color, Prop, NO_PAINT}`.
- Produces: `node(&str) -> Node`, `anon() -> Node`,
  `rgba(f32, f32, f32, f32) -> Color`, `Node::{at, size, fill, child,
  children}` (consuming, chainable),
  `scene(impl IntoIterator<Item = Node>) -> Scene`,
  `Scene::build(&mut Arena) -> u64`.

- [ ] **Step 1: Write the failing acceptance test**
      (`tests/builder.rs`): the design-spec scene built via the DSL,
      compared field-for-field (`rects()`, `paints()`) against the same
      scene built by hand with `Arena`/`Txn`; plus the repeater,
      multi-root, append-to-existing-arena, and defaults tests from the
      design spec's test list (5 tests total, full assertions).
- [ ] **Step 2: Verify failure** — `cargo test -p dashlang` fails to
      compile (no such items).
- [ ] **Step 3: Implement** `lib.rs`:

```rust
pub struct Node {
    name: Option<String>,
    x: f32, y: f32, width: f32, height: f32,
    fill: Option<Color>,
    children: Vec<Node>,
}
pub struct Scene { roots: Vec<Node> }

impl Scene {
    pub fn build(&self, arena: &mut Arena) -> u64 {
        let mut txn = arena.open();
        for root in &self.roots { add(&mut txn, None, root); }
        txn.commit()
    }
}
fn add(txn: &mut Txn<'_>, parent: Option<NodeId>, node: &Node) {
    let id = txn.add_node(parent, node.name.as_deref());
    txn.set_prop(id, Prop::X(node.x));   // …Y/Width/Height
    if let Some(c) = node.fill { txn.set_prop(id, Prop::Fill(c)); }
    for child in &node.children { add(txn, Some(id), child); }
}
```

plus the chainable setters and the `node`/`anon`/`rgba`/`scene`
constructors, and crate docs with a doctest showing the
design-spec example.

- [ ] **Step 4: Verify pass** — `cargo test -p dashlang`, then
      `cargo test --workspace`.
- [ ] **Step 5: Commit** —
      `feat(dashlang): minimal value-tree builder over dashscene-core`.

### Task 2: decision record + gate

**Files:**

- Create: `docs/decisions/dashlang-value-tree-builder.md` (D1/D2/D3
  from the design spec: context, options, choice, why)
- Modify: `docs/decisions/README.md` (index the new record)

- [ ] **Step 1: Write the record + index line.**
- [ ] **Step 2: Full gate** — `just build` green.
- [ ] **Step 3: Commit** —
      `docs(docs): record the dashlang value-tree builder decision`.

---

## Self-review

- Spec coverage: D1/D2 → Task 1; D3 → Task 1 tests; decision record →
  Task 2. ✓
- Placeholders: none — the test list is enumerated in the design spec
  and the implementation core is shown above. ✓
- Type consistency: `Scene::build(&mut Arena) -> u64` matches the
  design spec and `Txn::commit(self) -> u64`. ✓
