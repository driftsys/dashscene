# flex layout fields end-to-end — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** v0.2 flex-layout vocabulary in `dashbuf.fbs` and
`dashscene-core`'s intent model + staged props, per the design spec
(`docs/wip/2026-07-12-flex-schema-core-design.md`). No solving.

**Architecture:** design spec D1–D3.

**Tech Stack:** Rust 2024; flatc optional scalars (`= null`).

## Global Constraints

- Additive schema evolution only (append-only ids, R7).
- No committed-output behavior change (Taffy is #9).
- `just build` green; scopes `dashbuf` / `dashscene-core` / `docs`.

---

### Task 1: dashbuf — flex vocabulary in the schema

**Files:** `crates/dashbuf/schema/dashbuf.fbs`,
`crates/dashbuf/tests/roundtrip.rs`

- [ ] **Step 1: Failing test** — extend `roundtrip.rs` with
      `flex_and_constraint_fields_round_trip` (build a Node with
      `FlexContainer { Horizontal, gap 8, padding (1,2,3,4), Center,
  End }` and `LayoutConstraints { Hug, Fill, min_width 10,
  max_width 100, min_height 5, max_height 50 }`, assert every field
      back) and `a_node_without_flex_tables_reads_back_absent`.
- [ ] **Step 2: Verify failure** (compile error).
- [ ] **Step 3: Schema change** — design spec D1 verbatim, with
      comments carrying D2's semantics (fixed fields as the datum).
- [ ] **Step 4: Verify pass** — `cargo test -p dashbuf`.
- [ ] **Step 5: Commit** —
      `feat(dashbuf): add the v0.2 flex layout vocabulary (mode, sizing, gap, padding, align, min/max)`.

### Task 2: dashscene-core — mirror + props + getter

**Files:** `crates/dashscene-core/src/arena.rs`, `src/lib.rs`,
`tests/arena.rs`

**Interfaces produced:** `LayoutMode`, `AxisSizing`, `MainAxisAlign`,
`CrossAxisAlign`, `Layout` (Copy snapshot), `Prop::{Mode, Gap,
Padding, MainAlign, CrossAlign, SizingH, SizingV, MinWidth, MaxWidth,
MinHeight, MaxHeight}`, `Arena::layout(NodeId) -> Layout`.

- [ ] **Step 1: Failing tests** — design-spec tests 2 and 3:
      set-every-prop/read-back, defaults, and
      `flex_props_do_not_change_committed_output_yet` (set flex props on
      a committed scene; recommit; rects identical, dirty empty).
- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** — extend `NodeData` with the layout-intent
      fields (defaults per design), `Prop` variants, `Layout` snapshot +
      `Arena::layout` (panics on out-of-range NodeId like `name`),
      re-exports.
- [ ] **Step 4: Verify pass** — `cargo test -p dashscene-core` and
      workspace.
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): mirror the flex layout vocabulary in the intent model and props`.

### Task 3: decision record + records + gate

**Files:** `docs/decisions/flex-vocabulary-shape.md` (D1/D2/D3 + the
no-load-path scoping), `docs/decisions/README.md` (index),
`docs/design/dashscene-core-arena.md` (extend: intent model now
carries flex intent; commit still resolves fixed-only until #9).

- [ ] **Step 1: Write records.**
- [ ] **Step 2: `just build` green.**
- [ ] **Step 3: Commit** —
      `docs(docs): record the flex vocabulary shape decision (story #8)`.

---

## Self-review

- Spec coverage: D1 → Task 1; D2 → Task 1 comments + record; D3 →
  Task 2; scoping decisions → Task 3. ✓
- Placeholders: none (field lists enumerated in the design spec). ✓
- Type consistency: `Layout` field names match `Prop` variants and
  the schema names. ✓
