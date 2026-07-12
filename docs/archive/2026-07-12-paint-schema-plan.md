# v0.3 paint vocabulary (dashbuf + dashpaint) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Grow `dashbuf.fbs` and `dashpaint` to the v0.3 paint vocabulary (four gradient kinds, corner radii, stroke with align, image fills + scale modes, clip), additively, with round-trip tests for every new kind and field.

**Architecture:** Two independent halves. Half 1 extends the flatbuffer schema (flatc regenerates Rust at build time) plus a new round-trip test file. Half 2 mirrors the vocabulary as plain Rust types in `dashpaint` and re-shapes the table entry to `PaintEntry { fill, stroke, corners, clip }`. No painter work, no importer work.

**Tech Stack:** FlatBuffers (flatc 25.12.19, build-time codegen), Rust edition 2024. Gate: `just build`.

## Global Constraints

- Schema changes are strictly additive: existing field ids keep their
  positions; new `Node`/`Document` fields append after existing ones.
  `Node.paint` is NOT removed or retyped (session A reads it from main).
- `PaintKind::Solid { color: Color }`, `RectEntry`, `Color`, and the
  `Painter` trait signature are pinned — do not touch them.
- `dashpaint` keeps zero dependencies.
- Commits: conventional; scope `dashbuf` for half 1, `dashpaint` for
  half 2, `docs` where only docs move.
- Exact generated-API names (union accessors, enum consts) must be
  confirmed against flatc's output during Task 1 Step 2 — the test code
  below assumes flatc's standard Rust naming (`fill_type()`,
  `fill_as_gradient()`, `Fill::Gradient`, `GradientKind::Radial`, …);
  adjust the test to the real names if flatc 25.x differs, not the
  schema.

---

### Task 1: dashbuf schema growth + round-trip tests

**Files:**

- Modify: `crates/dashbuf/schema/dashbuf.fbs`
- Create: `crates/dashbuf/tests/paint_roundtrip.rs`

**Interfaces:**

- Produces (generated): `Vec2`, `GradientStop`, `CornerRadii`,
  `GradientKind`, `StrokeAlign`, `ScaleMode`, `ImageFormat`, `Gradient`,
  `ImageFill`, `Fill` (union), `Stroke`, `Image`, new `Node` fields
  `fill`/`stroke`/`corners`/`clip`, new `Document` field `images`.
  Consumed later by dashc/core lowering (not in this story).

- [ ] **Step 1: Extend the schema** — append to `dashbuf.fbs` (enums,
      structs, tables, union as specified in the design doc §"dashbuf schema
      additions"), append the four new fields to `Node` after `paint`, and
      `images: [Image]` to `Document` after `nodes`. Schema comments carry
      the handle-position semantics and the fill-supersedes-paint rule.

- [ ] **Step 2: Confirm generated names** — run
      `cargo build -p dashbuf` and inspect
      `target/debug/build/dashbuf-*/out/dashbuf_generated.rs` for the union
      accessor names and enum constant style. Adjust test code to match.

- [ ] **Step 3: Write the round-trip tests** (RED first is impractical
      here — the schema and its test compile together; the failing state is
      Step 1 without Step 3 asserting, so write tests immediately after and
      watch them pass, then mutate one expectation to prove the test bites,
      then restore). Tests, one per construct:
  - `gradient_fill_round_trips_all_four_kinds` — for each
    `GradientKind`, build a node whose `fill` is a `Gradient` with
    handles (0,0),(1,0),(0,1) and stops [(0.0, red),(1.0, half-blue)];
    read back kind, three handles, both stops.
  - `image_fill_round_trips_every_scale_mode` — `Document.images` =
    [Image { format: Png, bytes: [1,2,3,4] }]; for each `ScaleMode`, a
    node with `ImageFill { image: 0, scale_mode }`; read back index,
    mode, image bytes, format.
  - `stroke_round_trips_every_align` — width 2.5, each align, color;
    read back all fields.
  - `corners_and_clip_round_trip` — corners (1,2,3,4), clip true; read
    back; a second node without corners/clip reads corners `None` and
    clip `false`.
  - `fill_union_discriminates_and_legacy_paint_still_reads` — one node
    per fill member asserting `fill_type()` + `fill_as_*()`; one node
    with only legacy `paint` (fill absent) still yields its solid color.

- [ ] **Step 4: Run** `cargo test -p dashbuf` — all pass (plus the
      existing `roundtrip.rs` regression stays green). Mutate one assert,
      see RED, restore, see GREEN.

- [ ] **Step 5: Commit** —
      `feat(dashbuf): grow the schema to the v0.3 paint vocabulary`

---

### Task 2: dashpaint vocabulary types + PaintEntry

**Files:**

- Modify: `crates/dashpaint/src/lib.rs`
- Modify: `crates/dashpaint/tests/boundary_b.rs`

**Interfaces:**

- Consumes: nothing new (independent of Task 1).
- Produces: `Vec2`, `GradientStop`, `GradientKind`, `Gradient`,
  `ScaleMode`, `StrokeAlign`, `Stroke`, `CornerRadii`,
  `PaintKind::{Gradient, Image}` variants, `PaintEntry` with
  `PaintEntry::solid(Color)`; `PaintTable` now stores `PaintEntry`
  (same method names). Story #4 wires core against this.

- [ ] **Step 1: Write the failing tests** — append to
      `boundary_b.rs`:

```rust
#[test]
fn paint_entry_solid_is_fill_only() {
    let entry = PaintEntry::solid(RED);

    assert_eq!(entry.fill, Some(PaintKind::Solid { color: RED }));
    assert_eq!(entry.stroke, None);
    assert_eq!(entry.corners, CornerRadii::default());
    assert!(!entry.clip);
}

#[test]
fn a_paint_less_entry_pushes_and_resolves() {
    let mut table = PaintTable::new();
    let index = table.push(PaintEntry::default());

    assert_eq!(table.resolve(index).fill, None);
}

#[test]
fn a_full_entry_round_trips_through_the_table() {
    let gradient = Gradient {
        kind: GradientKind::Radial,
        handles: [
            Vec2 { x: 0.5, y: 0.5 },
            Vec2 { x: 1.0, y: 0.5 },
            Vec2 { x: 0.5, y: 1.0 },
        ],
        stops: vec![
            GradientStop { offset: 0.0, color: RED },
            GradientStop { offset: 1.0, color: HALF_BLUE },
        ],
    };
    let entry = PaintEntry {
        fill: Some(PaintKind::Gradient(gradient.clone())),
        stroke: Some(Stroke { width: 2.0, align: StrokeAlign::Inside, color: RED }),
        corners: CornerRadii { top_left: 1.0, top_right: 2.0, bottom_right: 3.0, bottom_left: 4.0 },
        clip: true,
    };
    let mut table = PaintTable::new();
    let index = table.push(entry.clone());

    assert_eq!(table.resolve(index), &entry);
}

#[test]
fn an_image_fill_names_its_asset_and_scale_mode() {
    let entry = PaintEntry {
        fill: Some(PaintKind::Image { image: 7, scale_mode: ScaleMode::Crop }),
        ..PaintEntry::default()
    };

    assert_eq!(
        entry.fill,
        Some(PaintKind::Image { image: 7, scale_mode: ScaleMode::Crop })
    );
}
```

and migrate the existing tests: table tests push
`PaintEntry::solid(RED)` etc.; `RecordingPainter` resolves the entry
and matches `entry.fill` (`Some(PaintKind::Solid { color })` records,
other fills/None are unreachable in the fixture); imports updated.

- [ ] **Step 2: Run to verify RED** —
      `cargo test -p dashpaint --test boundary_b` fails to compile
      (`PaintEntry`, `Gradient`, … unresolved).

- [ ] **Step 3: Implement** — in `lib.rs`, add the types from the
      design doc §"dashpaint additions" with rustdoc tracing each to
      DESIGN_1.md §10.1/§5; change `PaintTable.entries` to
      `Vec<PaintEntry>`; `push`/`get`/`resolve` signatures change entry
      type only; `PaintKind` derives drop `Copy` (keep Clone, Debug,
      PartialEq); update the trait doc's `resolve` reference if wording
      needs it.

- [ ] **Step 4: Run to verify GREEN** — all dashpaint tests pass.

- [ ] **Step 5: Full gate** — `just build` green.

- [ ] **Step 6: Commit** —
      `feat(dashpaint): grow the paint table to the v0.3 vocabulary`

---

### Task 3: Decision records + design-record update

**Files:**

- Create: `docs/decisions/fill-union-keeps-legacy-paint-field.md`
- Create: `docs/decisions/paint-entry-composition.md`
- Modify: `docs/design/dashpaint.md`

**Interfaces:**

- Consumes: the spec's "Alternatives considered" section (source
  material for both records).
- Produces: the durable records the PR reviewer reads; `#55`'s
  resolution is documented in `paint-entry-composition.md`.

- [ ] **Step 1: Write `fill-union-keeps-legacy-paint-field.md`** —
      context (session A reads `Node.paint`; R7 append-only), options
      (replace vs add union), choice (add `fill`, keep `paint`,
      fill-supersedes-paint precedence), why, cleanup condition (validator
      diagnostic; coordinated removal after v0.1 stories stop writing it).

- [ ] **Step 2: Write `paint-entry-composition.md`** — context (entry
      needed fill+stroke+shape params; #55's paint-less gap), options
      (grow PaintKind only vs PaintEntry composition vs PaintKind::None),
      choice (PaintEntry with `fill: Option<PaintKind>`), why (DESIGN §5
      paint-table row; #55 resolved; pinned v0.1 shapes untouched; core
      unaffected until #4 — same-session wiring), plus the solid-only
      stroke deferral and the gradient-handles geometry choice.

- [ ] **Step 3: Update `docs/design/dashpaint.md`** — Public interface
      section gains the new types and the `PaintEntry` entry shape; Testing
      section sentence widens to the new coverage; add an "Open for #14"
      note (image pixel data crossing boundary B).

- [ ] **Step 4: Lint + commit** — `just lint` green;
      `docs(dashpaint): record the v0.3 paint-vocabulary decisions`.
