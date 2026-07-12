# negative-gap lowering — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The negative-gap → margins lowering, its margin vocabulary,
and the acceptance proof, per the design spec
(`docs/wip/2026-07-12-negative-gap-lowering-design.md`).

**Architecture:** design spec D1–D4.

**Tech Stack:** taffy 0.12, Rust 2024. No new dependencies.

## Global Constraints

- Additive schema evolution only (append-only ids, R7).
- The lowering is deterministic (R7) and idempotent.
- `just build` green; scopes `dashbuf` / `dashscene-core` /
  `dashscene-engine` / `docs` / `repo` (corpus files).

---

### Task 1: dashbuf — margin on LayoutConstraints

**Files:** `crates/dashbuf/schema/dashbuf.fbs`,
`crates/dashbuf/tests/roundtrip.rs`

- [ ] **Step 1: Failing test** — extend the flex round-trip test to
      set `margin: EdgeInsets(1,2,3,4)` and assert it back; assert the
      empty-`LayoutConstraints` case reads `margin().is_none()` (absent =
      zero insets).
- [ ] **Step 2: Verify failure** (compile error, no `margin`).
- [ ] **Step 3: Schema change** — append `margin: EdgeInsets;` to
      `LayoutConstraints` with a comment: child-side, absent = zero
      insets, negative values legal (the negative-gap lowering target,
      DESIGN §5).
- [ ] **Step 4: Verify pass** — `cargo test -p dashbuf`.
- [ ] **Step 5: Commit** —
      `feat(dashbuf): add child margin to LayoutConstraints`.

### Task 2: dashscene-core — margin intent + Prop::Margin

**Files:** `crates/dashscene-core/src/arena.rs`, `tests/arena.rs`

**Interfaces produced:** `Layout.margin: EdgeInsets`,
`Prop::Margin { left, top, right, bottom }`.

- [ ] **Step 1: Failing test** — `Prop::Margin` sets `layout.margin`
      read back via `Arena::layout`; default is zero insets.
- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** — `margin: EdgeInsets` field on `Layout`
      (after `padding`), `Prop::Margin` variant, its `set_prop` arm.
- [ ] **Step 4: Verify pass** — core suite green.
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): add child margin to the layout intent`.

### Task 3: dashscene-core — Txn::lower_negative_gaps

**Files:** `crates/dashscene-core/src/arena.rs`, `tests/arena.rs`

**Interfaces produced:** `Txn::lower_negative_gaps(&mut self)`.

- [ ] **Step 1: Failing tests** — design test 3: Horizontal gap −8,
      three children → gap 0, child[1]/child[2] `margin.left −8`,
      child[0] unchanged; Vertical variant on `margin.top`; positive gap
      untouched; pre-existing child margin added to; idempotent (run
      twice, same result).
- [ ] **Step 2: Verify failure.**
- [ ] **Step 3: Implement** — walk every node; for a container with
      mode H/V and `layout.gap < 0`, set `gap = 0` and for each child
      after the first add `gap` to `margin.left` (H) or `margin.top` (V).
      Read children order from `node.children`.
- [ ] **Step 4: Verify pass.**
- [ ] **Step 5: Commit** —
      `feat(dashscene-core): lower negative gap to child margins`.

### Task 4: dashscene-engine — margin in the Taffy mapping

**Files:** `crates/dashscene-engine/src/lib.rs`,
`crates/dashscene-engine/tests/solve.rs`

- [ ] **Step 1: Failing tests** — design tests 4 and 5: A == B
      (negative-gap-lowered vs margin-authored, identical rects with
      8-unit overlaps); Vertical negative-gap column overlaps on the main
      axis; an authored-margin scene solves without any lowering.
- [ ] **Step 2: Verify failure** (margins currently ignored → no
      overlap).
- [ ] **Step 3: Implement** — map `layout.margin` to
      `style.margin: Rect<LengthPercentageAuto>` (left/top/right/bottom).
- [ ] **Step 4: Verify pass** — engine + workspace suites.
- [ ] **Step 5: Commit** —
      `feat(dashscene-engine): map child margin into the Taffy style`.

### Task 5: corpus case + records + gate

**Files:** `corpus/dsl-generated/README.md`,
`corpus/dsl-generated/negative-gap.md`,
`docs/decisions/negative-gap-lowering.md` (D1–D4),
`docs/decisions/README.md` (index),
`docs/design/dashscene-engine.md` (margin in the mapping table),
`docs/design/dashbuf.md` + `docs/design/dashscene-core-arena.md`
(margin field mentions)

- [ ] **Step 1: Write the corpus entry (scene + expected overlap +
      the #46 note), the decision record, and the record edits.**
- [ ] **Step 2: `just build` green.**
- [ ] **Step 3: Commit** —
      `docs(docs): record the negative-gap lowering; add the corpus case`.

---

## Self-review

- Spec coverage: D1 → Tasks 1/2/4; D2 → Task 3; D3 → Task 4 test;
  D4 → Task 5; tests 1–5 all assigned. ✓
- Placeholders: none — field names and the lowering rule are fixed in
  the design spec. ✓
- Type consistency: `Layout.margin`/`Prop::Margin`/`EdgeInsets` names
  match the schema and the engine mapping. ✓
