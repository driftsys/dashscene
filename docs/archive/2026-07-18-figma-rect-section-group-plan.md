# RECTANGLE leaf + SECTION/GROUP containers — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Admit `RECTANGLE`, `SECTION`, and `GROUP` to the Figma lowering's node-type allowlist so real files stop failing on `figma.unsupported: node type …`.

**Architecture:** These three types route through the walk's existing `else` branch (`container_of` + `paint_of`) exactly like `FRAME`. `RECTANGLE` is a leaf box; `SECTION`/`GROUP` are absolute containers (`layoutMode` absent → `container_of` returns `None` → children positioned by authored offset). No new lowering logic — the opacity/mask/effect/constraint machinery already applies. The change is the allowlist plus tests.

**Tech Stack:** Rust (edition 2024), `dashc` crate, `serde_json` synthetic test fixtures, `cargo test`.

## Global Constraints

- Edition `2024`; `just build` must be green (that is what CI runs).
- P1: the document carries intent, never solver results. P4: every out-of-profile construct is a named diagnostic, never a silent drop.
- Tests are **synthetic** `serde_json` node fixtures built through the existing `document(root)` helper — no captured Figma file (the corpus self-authoring rule does not gate unit tests).
- Conventional commit messages; end each with `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Do not refactor the allowlist's negative-condition style; extend it in place (surgical change).

---

### Task 1: `RECTANGLE` lowers as a box leaf

**Files:**

- Modify: `crates/dashc/src/figma/mod.rs:509-516` (the node-kind allowlist and its comment)
- Test: `crates/dashc/tests/figma_lowering.rs` (add one test + a `BTreeMap` import if absent — it is already imported at line 15)

**Interfaces:**

- Consumes: `dashc_wasm::figma::lower(&FigmaFile, Profile, &BTreeMap<String, ImageAsset>) -> Result<(Document, Vec<Diagnostic>), CompileError>`; `document(root: serde_json::Value) -> FigmaFile` (test helper, line 47); `common::node(doc, name) -> (u32, &dashc_wasm::Node)`; `common::unsupported(&[Diagnostic]) -> Vec<(String, String)>`.
- Produces: nothing new; validates behavior.

- [ ] **Step 1: Write the failing test**

Add to `crates/dashc/tests/figma_lowering.rs`:

```rust
#[test]
fn a_rectangle_lowers_as_a_box_leaf() {
    // A RECTANGLE is a paint-bearing leaf: no children, its authored box and
    // fill lower through the same paint path a FRAME uses.
    let file = document(serde_json::json!({
        "name": "rect",
        "type": "RECTANGLE",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 80.0, "height": 40.0 },
        "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        "cornerRadius": 8.0,
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("a rectangle lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "RECTANGLE must not be an unsupported node type: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, rect) = node(&doc, "rect");
    assert_eq!((rect.box2d.width, rect.box2d.height), (80.0, 40.0));
    assert!(rect.paint.is_some(), "the rectangle carries its fill");
    assert!(rect.container.is_none(), "a rectangle is a leaf, not a container");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashc --test figma_lowering a_rectangle_lowers_as_a_box_leaf`
Expected: FAIL — the assertion `common::unsupported(...).is_empty()` fails with `("", "node type RECTANGLE")`.

- [ ] **Step 3: Write minimal implementation**

In `crates/dashc/src/figma/mod.rs`, extend the allowlist condition at line 509 and its comment. Replace the block that currently reads:

```rust
// `FRAME`, `INSTANCE`, `TEXT`, and `ELLIPSE` are the node kinds with a
// lowering (stories #140, #242, #160, #239). An `INSTANCE` lowers like a
// `FRAME`: Figma bakes the referenced component's content — with the
// instance's overrides applied — into the instance's own children, so
// the baked subtree goes through the ordinary walk and an
// out-of-vocabulary override on it is a named diagnostic like any other
// (P4). Any other kind reports its type and nothing else: its other
// properties belong to whatever story lowers that type (the remaining
// shape kinds when a shape construct lands), so diagnosing them here
// would be noise around the verdict that matters.
if node.kind != "FRAME"
    && node.kind != "INSTANCE"
    && node.kind != "TEXT"
    && node.kind != "ELLIPSE"
{
```

with:

```rust
// `FRAME`, `INSTANCE`, `TEXT`, and `ELLIPSE` are the node kinds with a
// lowering (stories #140, #242, #160, #239). An `INSTANCE` lowers like a
// `FRAME`: Figma bakes the referenced component's content — with the
// instance's overrides applied — into the instance's own children, so
// the baked subtree goes through the ordinary walk and an
// out-of-vocabulary override on it is a named diagnostic like any other
// (P4). `RECTANGLE` is a paint-bearing leaf lowered through the same
// container/paint path with no `layoutMode` and no children (#309). Any
// other kind reports its type and nothing else: its other properties
// belong to whatever story lowers that type (the remaining shape kinds
// when a shape construct lands), so diagnosing them here would be noise
// around the verdict that matters.
if node.kind != "FRAME"
    && node.kind != "INSTANCE"
    && node.kind != "TEXT"
    && node.kind != "ELLIPSE"
    && node.kind != "RECTANGLE"
{
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashc --test figma_lowering a_rectangle_lowers_as_a_box_leaf`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "$(cat <<'EOF'
feat(dashc): lower RECTANGLE as a box leaf (#309)

Admit RECTANGLE to the Figma node-type allowlist; it lowers through the
existing container/paint path as a leaf box (no layoutMode, no children).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `SECTION` lowers as an absolute container with offset children

**Files:**

- Modify: `crates/dashc/src/figma/mod.rs:509-516` (add one guard line)
- Test: `crates/dashc/tests/figma_lowering.rs` (add one test)

**Interfaces:**

- Consumes: same `lower`/`document`/`node`/`unsupported` as Task 1. Relies on Task 1 having admitted `RECTANGLE` (the child in this test is a `RECTANGLE`).
- Produces: nothing new.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn a_section_lowers_as_an_absolute_container_with_offset_children() {
    // A SECTION has no layoutMode, so it is an absolute container: its child's
    // position is the authored offset (child bbox - section bbox), and the
    // child carries its authored size (absent sizing outside auto-layout is
    // Fixed).
    let file = document(serde_json::json!({
        "name": "section",
        "type": "SECTION",
        "absoluteBoundingBox": { "x": 100.0, "y": 100.0, "width": 400.0, "height": 300.0 },
        "children": [{
            "name": "card",
            "type": "RECTANGLE",
            "absoluteBoundingBox": { "x": 150.0, "y": 180.0, "width": 80.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 0.0, "b": 1.0, "a": 1.0 } }],
        }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("a section lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "SECTION must not be unsupported: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, card) = node(&doc, "card");
    // Offset from the section origin (150-100, 180-100), authored size preserved.
    assert_eq!((card.box2d.x, card.box2d.y), (50.0, 80.0));
    assert_eq!((card.box2d.width, card.box2d.height), (80.0, 40.0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashc --test figma_lowering a_section_lowers_as_an_absolute_container_with_offset_children`
Expected: FAIL — `common::unsupported` reports `("", "node type SECTION")`.

- [ ] **Step 3: Write minimal implementation**

In `crates/dashc/src/figma/mod.rs`, add one line to the allowlist condition (after the `RECTANGLE` line added in Task 1):

```rust
if node.kind != "FRAME"
    && node.kind != "INSTANCE"
    && node.kind != "TEXT"
    && node.kind != "ELLIPSE"
    && node.kind != "RECTANGLE"
    && node.kind != "SECTION"
{
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashc --test figma_lowering a_section_lowers_as_an_absolute_container_with_offset_children`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "$(cat <<'EOF'
feat(dashc): lower SECTION as an absolute container (#309)

Admit SECTION to the allowlist; it lowers as a no-layoutMode container whose
children are positioned by authored offset through the existing machinery.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `GROUP` lowers as an absolute container; visual intent is preserved, not dropped

**Files:**

- Modify: `crates/dashc/src/figma/mod.rs:509-516` (add one guard line)
- Test: `crates/dashc/tests/figma_lowering.rs` (add two tests)

**Interfaces:**

- Consumes: same helpers. `dashc_wasm::Node.opacity: f32` (the lowered node's opacity field, `mod.rs:750`).
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_group_lowers_as_an_absolute_container_carrying_opacity() {
    // GROUP is an inert container: it lowers as an absolute container, and its
    // own opacity rides the existing node-opacity machinery (v0.8, #44).
    let file = document(serde_json::json!({
        "name": "group",
        "type": "GROUP",
        "opacity": 0.5,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 200.0, "height": 120.0 },
        "children": [{
            "name": "member",
            "type": "RECTANGLE",
            "absoluteBoundingBox": { "x": 10.0, "y": 20.0, "width": 40.0, "height": 40.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 0.0, "g": 1.0, "b": 0.0, "a": 1.0 } }],
        }],
    }));

    let (doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("a group lowers");

    assert!(
        common::unsupported(&diagnostics).is_empty(),
        "GROUP must not be unsupported: {:?}",
        common::unsupported(&diagnostics),
    );
    let (_, group) = node(&doc, "group");
    assert_eq!(group.opacity, 0.5, "group opacity is carried, not dropped");
    let (_, member) = node(&doc, "member");
    assert_eq!((member.box2d.x, member.box2d.y), (10.0, 20.0));
}

#[test]
fn a_group_with_an_advanced_blend_mode_is_diagnosed_not_dropped() {
    // The P4 guard: an inert container is passed through, but a GROUP carrying
    // visual intent the schema cannot express (a non-NORMAL blend mode) is a
    // named diagnostic, never a silent accept.
    let file = document(serde_json::json!({
        "name": "blended-group",
        "type": "GROUP",
        "blendMode": "MULTIPLY",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [],
    }));

    let (_doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    assert!(
        !diagnostics.is_empty(),
        "an advanced blend mode on a group must surface a diagnostic",
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering a_group_`
Expected: FAIL — `a_group_lowers_as_an_absolute_container_carrying_opacity` reports `("", "node type GROUP")`. (`a_group_with_an_advanced_blend_mode_…` may already pass, since an unsupported GROUP type is itself a diagnostic — the opacity test is the one that must go from fail to pass.)

- [ ] **Step 3: Write minimal implementation**

Add the final guard line to `crates/dashc/src/figma/mod.rs`:

```rust
if node.kind != "FRAME"
    && node.kind != "INSTANCE"
    && node.kind != "TEXT"
    && node.kind != "ELLIPSE"
    && node.kind != "RECTANGLE"
    && node.kind != "SECTION"
    && node.kind != "GROUP"
{
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p dashc --test figma_lowering a_group_`
Expected: PASS (both).

- [ ] **Step 5: Commit**

```bash
git add crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "$(cat <<'EOF'
feat(dashc): lower GROUP as an absolute container (#309)

Admit GROUP to the allowlist. Its opacity/mask/effect intent rides the
existing node-property machinery: an inert group passes through, a group
carrying a blend mode the schema cannot express is diagnosed (P4).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: regression guard — an unadmitted type is still refused — and full build

**Files:**

- Test: `crates/dashc/tests/figma_lowering.rs` (add one guard test)

**Interfaces:**

- Consumes: same helpers.
- Produces: nothing.

- [ ] **Step 1: Write the guard test**

```rust
#[test]
fn a_vector_is_still_an_unsupported_node_type() {
    // #309 admits exactly RECTANGLE/SECTION/GROUP. Path-geometry types stay
    // refused by name — the boundary this story did not cross.
    let file = document(serde_json::json!({
        "name": "vec",
        "type": "VECTOR",
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
    }));

    let (_doc, diagnostics) = lower(&file, Profile::Core, &BTreeMap::new())
        .expect("lowering returns the doc plus diagnostics");

    assert!(
        common::unsupported(&diagnostics)
            .iter()
            .any(|(_, what)| what == "node type VECTOR"),
        "VECTOR must remain unsupported: {:?}",
        common::unsupported(&diagnostics),
    );
}
```

- [ ] **Step 2: Run the guard test**

Run: `cargo test -p dashc --test figma_lowering a_vector_is_still_an_unsupported_node_type`
Expected: PASS immediately (VECTOR was never admitted). This is a characterization guard, not a red-green step.

- [ ] **Step 3: Run the full lowering suite**

Run: `cargo test -p dashc --test figma_lowering`
Expected: PASS — all new and pre-existing tests green (confirms the widened allowlist did not regress the refusal tests).

- [ ] **Step 4: Full build**

Run: `just build`
Expected: PASS (clippy `-D warnings`, fmt, tests, the whole CI gate).

- [ ] **Step 5: Commit**

```bash
git add crates/dashc/tests/figma_lowering.rs
git commit -m "$(cat <<'EOF'
test(dashc): guard that unadmitted node types stay refused (#309)

Pin the #309 boundary: VECTOR (path geometry) remains an unsupported node
type after RECTANGLE/SECTION/GROUP are admitted.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review

**Spec coverage:**

- RECTANGLE leaf → Task 1. ✓
- SECTION container → Task 2. ✓
- GROUP container + P4 guard → Task 3. ✓
- Deferred types stay refused → Task 4 guard. ✓
- "Lower as container, not passthrough" → realized by routing through the existing `else`/`container_of` path; no hoisting code exists in any task. ✓
- Testing via synthetic `serde_json` fixtures → all tasks. ✓

**Placeholder scan:** none — every step has concrete code, exact paths, exact commands, expected output.

**Type consistency:** `lower(&FigmaFile, Profile::Core, &BTreeMap::new())`, `node(&doc, name) -> (u32, &Node)`, `Node.box2d.{x,y,width,height}`, `Node.paint: Option<_>`, `Node.container: Option<_>`, `Node.opacity: f32`, `common::unsupported -> Vec<(String, String)>` — used identically across all tasks. The allowlist condition grows by exactly one `&& node.kind != "…"` line per implementation task.

## Out of scope (record, do not silently drop)

- `VECTOR`/`LINE`/`STAR`/`REGULAR_POLYGON`/`BOOLEAN_OPERATION` (path geometry) — separate issue.
- Text gaps (#310) and the unknown-enum parse crash (#311) — the demo still will not render a real file until those land; this story does not attempt it.
- An emit/render golden (`compile_figma` → `.dsb` → PNG) — the existing refusal/lowering tests use `lower`, and a captured-fixture golden is gated by the corpus self-authoring rule; not required for #309.
