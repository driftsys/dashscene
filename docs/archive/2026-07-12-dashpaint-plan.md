# dashpaint v0.1 (boundary B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `dashpaint`'s boundary-B surface — `Color`, `RectEntry`, `PaintKind`, `PaintTable`, and the `Painter` trait — per the pinned v0.1 contract, with unit tests that need no other workspace crate.

**Architecture:** One dependency-free library crate. All types live in `src/lib.rs` (the whole surface is ~80 lines; no module split needed). Integration-style tests live in `tests/boundary_b.rs` and exercise the public API only, including a `RecordingPainter` test double.

**Tech Stack:** Rust (edition 2024), no dependencies. Gate: `just build` (test + clippy -D warnings + fmt --check + dprint + markdownlint).

## Global Constraints

- No dependencies in `crates/dashpaint/Cargo.toml` — in particular no `dashscene-core`, no `dashbuf` (spec: "Public API").
- `RectEntry { x, y, w, h: f32, paint: u32 }` and `Color { r, g, b, a: f32 }` exactly — pinned cross-session contract; do not rename fields.
- `Color` and `RectEntry` are `#[repr(C)]` (blittable, DESIGN §7.3 / R-T4).
- `Painter::paint` is infallible and the trait must be object-safe.
- Commits: conventional, scope `dashpaint`.

---

### Task 1: Paint table + boundary-B value types

**Files:**

- Modify: `crates/dashpaint/src/lib.rs`
- Create: `crates/dashpaint/tests/boundary_b.rs`

**Interfaces:**

- Produces: `dashpaint::{Color, RectEntry, PaintKind, PaintTable}` with
  `PaintTable::new() -> PaintTable`, `push(&mut self, PaintKind) -> u32`,
  `get(&self, u32) -> Option<&PaintKind>`, `len(&self) -> usize`,
  `is_empty(&self) -> bool`. Task 2 consumes all of these.

- [ ] **Step 1: Write the failing test**

`crates/dashpaint/tests/boundary_b.rs`:

```rust
//! Boundary-B contract tests against hand-built fixtures (issue #3):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{Color, PaintKind, PaintTable};

const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const HALF_BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 0.5 };

#[test]
fn paint_table_push_returns_sequential_indices_and_get_resolves_them() {
    let mut table = PaintTable::new();
    assert!(table.is_empty());

    let red = table.push(PaintKind::Solid { color: RED });
    let blue = table.push(PaintKind::Solid { color: HALF_BLUE });

    assert_eq!(red, 0);
    assert_eq!(blue, 1);
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(red), Some(&PaintKind::Solid { color: RED }));
    assert_eq!(table.get(blue), Some(&PaintKind::Solid { color: HALF_BLUE }));
}

#[test]
fn paint_table_get_past_the_end_returns_none() {
    let mut table = PaintTable::new();
    table.push(PaintKind::Solid { color: RED });

    assert_eq!(table.get(1), None);
    assert_eq!(table.get(u32::MAX), None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashpaint --test boundary_b`
Expected: FAIL to compile — `unresolved imports dashpaint::{Color, PaintKind, PaintTable}`.

- [ ] **Step 3: Write minimal implementation**

Replace the stub body of `crates/dashpaint/src/lib.rs` (keep the `//!` crate doc, extend it):

```rust
//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (DESIGN_1.md §8).
//!
//! v0.1 walking-skeleton scope: solid fills only. The rect-table index is
//! the document DFS node index (DESIGN_1.md §5); `RectEntry.paint` indexes
//! the [`PaintTable`].

/// An RGBA color, 4×f32 — the same shape as `dashbuf`'s `Color` struct.
///
/// `#[repr(C)]`: rect/paint data is blittable by design (DESIGN_1.md §7.3),
/// so the layout is fixed now even though nothing uploads it yet.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// One resolved rectangle — boundary B's per-node unit (DESIGN_1.md §7.3).
///
/// The rect-table index of this entry is the document DFS node index, so
/// there is no id field. `paint` indexes the [`PaintTable`].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: u32,
}

/// One paint-table entry. v0.1 knows solid fills only; gradients, images,
/// strokes, and effects land at v0.3 as new variants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintKind {
    Solid { color: Color },
}

/// The paint table (DESIGN_1.md §5): dense, indexed by `RectEntry.paint`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintTable {
    entries: Vec<PaintKind>,
}

impl PaintTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an entry and returns its index — the value a
    /// [`RectEntry::paint`] field holds to reference it.
    pub fn push(&mut self, kind: PaintKind) -> u32 {
        let index = u32::try_from(self.entries.len())
            .expect("paint table exceeds u32::MAX entries");
        self.entries.push(kind);
        index
    }

    pub fn get(&self, index: u32) -> Option<&PaintKind> {
        self.entries.get(index as usize)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashpaint --test boundary_b`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/dashpaint/src/lib.rs crates/dashpaint/tests/boundary_b.rs
git commit -m "feat(dashpaint): add boundary-B value types and the paint table"
```

---

### Task 2: Painter trait + recording-painter contract test

**Files:**

- Modify: `crates/dashpaint/src/lib.rs` (append the trait)
- Modify: `crates/dashpaint/tests/boundary_b.rs` (append tests + test double)

**Interfaces:**

- Consumes: `Color`, `RectEntry`, `PaintKind`, `PaintTable` from Task 1.
- Produces: `dashpaint::Painter` with
  `fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable)` —
  the trait story #4's Skia painter implements.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashpaint/tests/boundary_b.rs`:

```rust
use dashpaint::{Painter, RectEntry};

/// Test double: resolves each rect's paint index and records what a real
/// painter would color. A painter only colors (P2) — so recording
/// (rect, resolved color) pairs is a complete observation of the contract.
#[derive(Default)]
struct RecordingPainter {
    painted: Vec<(RectEntry, Color)>,
}

impl Painter for RecordingPainter {
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable) {
        for rect in rects {
            let PaintKind::Solid { color } = paints
                .get(rect.paint)
                .expect("paint index validated upstream (P4)");
            self.painted.push((*rect, *color));
        }
    }
}

fn two_rect_fixture() -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let red = paints.push(PaintKind::Solid { color: RED });
    let blue = paints.push(PaintKind::Solid { color: HALF_BLUE });
    let rects = vec![
        RectEntry { x: 0.0, y: 0.0, w: 100.0, h: 50.0, paint: red },
        RectEntry { x: 10.0, y: 20.0, w: 30.0, h: 40.0, paint: blue },
    ];
    (rects, paints)
}

#[test]
fn painter_receives_rects_in_slice_order_with_resolved_colors() {
    let (rects, paints) = two_rect_fixture();
    let mut painter = RecordingPainter::default();

    painter.paint(&rects, &paints);

    assert_eq!(painter.painted, vec![(rects[0], RED), (rects[1], HALF_BLUE)]);
}

#[test]
fn painter_trait_is_object_safe() {
    let (rects, paints) = two_rect_fixture();
    let mut painter: Box<dyn Painter> = Box::new(RecordingPainter::default());

    painter.paint(&rects, &paints);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashpaint --test boundary_b`
Expected: FAIL to compile — `unresolved import dashpaint::Painter`.

- [ ] **Step 3: Write minimal implementation**

Append to `crates/dashpaint/src/lib.rs`:

```rust
/// Boundary B (DESIGN_1.md §4, §8): the one trait every paint backend
/// implements. A painter only colors — it never measures, wraps, kerns,
/// or moves anything (P2).
pub trait Painter {
    /// Paints every rect in slice order (back-to-front: DFS order encodes
    /// document stacking), resolving each [`RectEntry::paint`] index
    /// against `paints`.
    ///
    /// Infallible by design: vocabulary and indices are validated upstream
    /// (P4), so an out-of-range `paint` index is a broken contract between
    /// crates — implementations may panic on it.
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dashpaint --test boundary_b`
Expected: PASS (4 tests).

- [ ] **Step 5: Run the full gate**

Run: `just build`
Expected: green (workspace tests, clippy -D warnings, fmt, dprint, markdownlint all pass).

- [ ] **Step 6: Commit**

```bash
git add crates/dashpaint/src/lib.rs crates/dashpaint/tests/boundary_b.rs
git commit -m "feat(dashpaint): add the Painter trait (boundary B)"
```

---

### Task 3: Decision records

**Files:**

- Create: `docs/decisions/dashpaint-owns-boundary-b-types.md`
- Create: `docs/decisions/painter-trait-infallible-slice-input.md`

**Interfaces:**

- Consumes: nothing — prose records of the choices made autonomously in
  the spec (see spec "Alternatives considered").
- Produces: the two decision records the PR reviewer reads.

- [ ] **Step 1: Write both records** (content in the spec's "Alternatives
      considered" section — context, options, choice, why; one file per
      decision, kebab-case names as above)

- [ ] **Step 2: Verify docs lint passes**

Run: `just lint`
Expected: markdownlint + dprint green.

- [ ] **Step 3: Commit**

```bash
git add docs/decisions/
git commit -m "docs(dashpaint): record boundary-B ownership and trait-shape decisions"
```
