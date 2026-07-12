# dashscene-skia + boundary-B unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the skia-safe CPU-raster `Painter` and unify boundary B: dashpaint owns the types, core depends on dashpaint, every rect resolves (no sentinel), paint indices are typed.

**Architecture:** Three sequential tasks — the newtype lands in `dashpaint` first, core migrates onto `dashpaint` second, the painter consumes both third. Sequential because each task's tests compile against the previous task's surface.

**Tech Stack:** Rust edition 2024, skia-safe 0.81 (CPU raster; prebuilt binaries verified). Gate: `just build`.

## Global Constraints

- `RectEntry` stays `#[repr(C)]`, 20 bytes, align 4 (core's layout test
  pins it) — `PaintIndex` must be `#[repr(transparent)]` over `u32`.
- `dashscene-skia`'s `[dependencies]` are `dashpaint` + `skia-safe`
  only; `dashscene-core` is a `[dev-dependencies]` entry (P2).
- No silent drops: unimplemented vocabulary panics naming story #14.
- Commits: conventional; scopes `dashpaint`, `dashscene-core`,
  `dashscene-skia`, `repo` (publish-order files), `docs`.

---

### Task 1: PaintIndex newtype in dashpaint

**Files:**

- Modify: `crates/dashpaint/src/lib.rs`
- Modify: `crates/dashpaint/tests/boundary_b.rs`

**Interfaces:**

- Produces: `#[repr(transparent)] pub struct PaintIndex(pub u32)` with
  `Debug/Clone/Copy/PartialEq/Eq/Hash`; `RectEntry.paint: PaintIndex`;
  `PaintTable::push(&mut self, PaintEntry) -> PaintIndex`;
  `get(&self, PaintIndex) -> Option<&PaintEntry>`;
  `resolve(&self, PaintIndex) -> &PaintEntry`. Tasks 2 and 3 consume.

- [ ] **Step 1 (RED):** update `boundary_b.rs`: import `PaintIndex`;
      fixture rect literals take `paint: red` where `red: PaintIndex` came
      from `push`; add a test that `PaintIndex` is transparent:

```rust
#[test]
fn paint_index_is_transparent_over_u32() {
    assert_eq!(std::mem::size_of::<PaintIndex>(), 4);
    assert_eq!(std::mem::size_of::<RectEntry>(), 20);
}
```

and update the out-of-range tests to `PaintIndex(1)` /
`PaintIndex(u32::MAX)` (panic message unchanged: "paint index 1 out
of range"). Run: FAIL to compile (no `PaintIndex`).

- [ ] **Step 2 (GREEN):** add the newtype above `RectEntry`; change the
      field and the three `PaintTable` signatures; `resolve`'s panic
      message formats `index.0`. Run: PASS.

- [ ] **Step 3:** `just build`; commit
      `feat(dashpaint): type the paint index (PaintIndex newtype)`.

---

### Task 2: core migrates onto dashpaint's types; every rect resolves

**Files:**

- Modify: `crates/dashscene-core/Cargo.toml` (add
  `dashpaint.workspace = true`)
- Modify: `crates/dashscene-core/src/committed.rs`
- Modify: `crates/dashscene-core/src/arena.rs`
- Modify: `crates/dashscene-core/src/lib.rs`
- Modify: `crates/dashscene-core/tests/arena.rs`
- Modify: `Cargo.toml` + `justfile` + `specs/SCOPE_DECISIONS.md` §7
  (publish order: dashbuf → dashpaint → dashscene-core → …)

**Interfaces:**

- Consumes: Task 1's `dashpaint` surface.
- Produces: `CommittedScene::paints() -> &PaintTable`;
  `dashscene_core::{Color, PaintEntry, PaintIndex, PaintKind,
  PaintTable, RectEntry}` re-exports; `NO_PAINT` and `committed::Paint`
  deleted. An unfilled node's rect resolves to the shared
  `PaintEntry::default()` pool entry.

- [ ] **Step 1 (RED):** rewrite `tests/arena.rs` expectations:
  - imports drop `NO_PAINT`/`Paint`, gain `PaintEntry`/`PaintIndex`;
  - pool assertions become
    `assert_eq!(scene.paints().resolve(rect.paint), &PaintEntry::solid(RED))`
    (and table length via `scene.paints().len()`);
  - the unfilled-node test asserts
    `scene.paints().resolve(scene.rects()[0].paint).fill == None`
    and that two unfilled nodes share one index;
  - the layout test (20-byte entry) keeps passing via the re-export.
    Run: FAIL to compile.

- [ ] **Step 2 (GREEN):**
  - `committed.rs`: delete `Color`/`RectEntry`/`Paint`/`NO_PAINT`;
    `pub use dashpaint::{Color, PaintEntry, PaintIndex, PaintKind, PaintTable, RectEntry};`
    `CommittedScene { rects: Vec<RectEntry>, paints: PaintTable, … }`.
  - `arena.rs` `commit`: interner becomes
    `HashMap<Option<[u32; 4]>, PaintIndex>`; key `None` for unfilled →
    `PaintEntry::default()`, `Some(bits)` → `PaintEntry::solid(color)`;
    push through `PaintTable::push`. `entry_bits` reads
    `entry.paint.0`; `resolved_color_bits` resolves the entry and
    matches `Some(PaintKind::Solid { color })` → bits, `None` fill →
    `None`. Update the `add_node` guard comment (the sentinel argument
    is now about `NO_PARENT`/`NodeId` only, plus "every paint index
    stays representable").
  - `lib.rs` re-export line matches.
  - Publish order: workspace `Cargo.toml` comment, `justfile` publish
    recipe, `SCOPE_DECISIONS.md` §7 list (plus a new dated section in
    that file recording the ownership decision at scope level — next
    free section number at commit time).
    Run: PASS (whole workspace).

- [ ] **Step 3:** `just build`; commit
      `feat(dashscene-core): consume dashpaint's boundary-B types; drop the NO_PAINT sentinel`.

---

### Task 3: the Skia CPU-raster painter

**Files:**

- Modify: `crates/dashscene-skia/Cargo.toml` (deps already wired;
  add `[dev-dependencies] dashscene-core.workspace = true`)
- Modify: `crates/dashscene-skia/src/lib.rs`
- Create: `crates/dashscene-skia/tests/painter.rs`

**Interfaces:**

- Consumes: `dashpaint::{Painter, RectEntry, PaintTable, PaintKind,
  PaintEntry, Color}`; `dashscene_core::{Arena, Prop}` (tests only).
- Produces: `dashscene_skia::SkiaPainter` —
  `new(width: i32, height: i32) -> Self` (panics on non-positive),
  `png_bytes(&mut self) -> Vec<u8>`, `rgba_bytes(&mut self) -> Vec<u8>`,
  `impl Painter`.

- [ ] **Step 1 (RED):** write `tests/painter.rs` with the four tests
      from the design doc ("Testing" 1–4): exact-pixel scene test (red
      4×4 root, blue 2×2 child at offset 1,1 — RGBA readback asserted
      byte-for-byte), unfilled-parent test, PNG-signature test,
      `#[should_panic(expected = "story #14")]` gradient-entry test.
      Scene building goes through `Arena::open`/`add_node`/`set_prop`/
      `commit`. Run: FAIL to compile (`SkiaPainter` missing).

- [ ] **Step 2 (GREEN):** implement `src/lib.rs`:

```rust
use dashpaint::{PaintKind, PaintTable, Painter, RectEntry};
use skia_safe::{Color4f, Paint as SkPaint, Rect, surfaces};

pub struct SkiaPainter {
    surface: skia_safe::Surface,
}

impl SkiaPainter {
    pub fn new(width: i32, height: i32) -> Self {
        assert!(width > 0 && height > 0, "surface dimensions must be positive");
        let surface = surfaces::raster_n32_premul((width, height))
            .expect("raster surface allocation");
        Self { surface }
    }

    pub fn png_bytes(&mut self) -> Vec<u8> { /* image_snapshot + PNG encode */ }
    pub fn rgba_bytes(&mut self) -> Vec<u8> { /* read_pixels into RGBA8888 */ }
}

impl Painter for SkiaPainter {
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable) {
        let canvas = self.surface.canvas();
        canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
        for rect in rects {
            let entry = paints.resolve(rect.paint);
            /* stroke/clip/corners/gradient/image → unimplemented!("… story #14") */
            /* Solid → SkPaint::new(Color4f::new(r,g,b,a), None), anti_alias(false), draw_rect */
        }
    }
}
```

(exact skia-safe 0.81 API names checked against the crate during
implementation; the crate compiles in this environment). Run: PASS.

- [ ] **Step 3:** `just build`; commit
      `feat(dashscene-skia): CPU-raster Painter over skia-safe (boundary B end to end)`.

---

### Task 4: decision records + record updates

**Files:**

- Create: `docs/decisions/boundary-b-unification.md` (ownership +
  publish order + PaintIndex + every-rect-resolves; context/options/
  choice/why from the design doc's three decision sections)
- Modify: `docs/decisions/core-committed-output-shape.md` (status note:
  reconciliation done here, NO_PAINT clause superseded),
  `docs/decisions/dashpaint-owns-boundary-b-types.md` (status: ownership
  resolved at #4), `docs/decisions/paint-entry-composition.md`
  ("Relation to debt #55" section: resolved — empty-entry crossing),
  `docs/decisions/README.md` (index entry),
  `docs/design/dashpaint.md` + `docs/design/dashscene-core-arena.md`
  (as-built type ownership; PaintIndex; no sentinel)
- [ ] Write, `just lint`, commit
      `docs(dashscene-skia): record the boundary-B unification`.
