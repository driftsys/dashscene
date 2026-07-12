# Flex Goldens Implementation Plan (story #11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin the v0.2 flex vocabulary with four exact-match golden images and record the hug-in-fill stress-corpus case, closing the last open story of epic #7.

**Architecture:** Each golden scene is authored against `dashscene-core`'s `Txn` (dashlang has no flex vocabulary), solved by `dashscene-engine`'s `TaffySolver` through `commit_with`, and painted by `SkiaPainter` — the same painter path every existing golden uses. One scene per construct, so a regression implicates one construct. Every scene is dimensioned so each solved rect lands on an integer, which means no anti-aliased edges and therefore exact-match goldens.

**Tech Stack:** Rust 2024, `dashscene-core`, `dashscene-engine` (Taffy), `dashscene-skia` (skia-safe), the `goldens` harness crate.

**Spec:** `docs/wip/2026-07-12-flex-goldens-design.md`

## Global Constraints

- Test-only story. No new public API in any crate; no `dashbuf` schema change; no behavior change to core, the engine, or the painter.
- Workspace dependencies are declared as `<name>.workspace = true`, never with an inline version.
- Every golden uses `goldens::assert_matches_golden` (exact, zero tolerance). If a scene turns out to solve to a fractional rect, do not reach for `assert_matches_golden_within` — first fix the scene's dimensions so it does not. Only if a construct genuinely cannot be made integral does the tolerance variant apply, with the reason recorded at the call site.
- Prose in code comments is plain, literal English (no idioms).
- Comment density and style match the surrounding golden tests (`v01.rs`, `v03_families.rs`): comments state the arithmetic a reader would otherwise have to re-derive, not what the next line does.
- `cargo fmt --all`, `clippy -D warnings`, `dprint check`, and `markdownlint` must all pass — `just build` runs them.
- A golden image is never committed without being looked at. Every task that generates one has an explicit inspection step.

---

### Task 1: Wire the goldens crate to the engine, and land the nesting golden

**Files:**

- Modify: `goldens/tooling/Cargo.toml` (dev-dependencies)
- Create: `goldens/tooling/tests/v02_flex.rs`
- Create: `goldens/images/v02-nesting.png` (generated, then committed)

**Interfaces:**

- Consumes: `dashscene_core::{Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, Txn}`; `dashscene_engine::TaffySolver`; `dashpaint::{ImageTable, Painter}`; `dashscene_skia::SkiaPainter`; `goldens::assert_matches_golden`.
- Produces: the helpers `rgba`, the colour constants, `boxed`, `rect`, and `render_and_compare` in `v02_flex.rs`, used by Tasks 2–4. Their exact signatures are defined in Step 3 below.

- [ ] **Step 1: Add the engine as a dev-dependency of the goldens crate**

The goldens crate has never depended on `dashscene-engine`, because nothing painted a flex-solved scene before. In `goldens/tooling/Cargo.toml`, add one line to the existing `[dev-dependencies]` block. The existing block is not alphabetically ordered; leave its order alone and insert the new line as shown, so the diff is one line:

```toml
[dev-dependencies]
dashlang.workspace = true
tempfile.workspace = true
dashpaint.workspace = true
dashscene-core.workspace = true
dashscene-engine.workspace = true
dashscene-skia.workspace = true
```

- [ ] **Step 2: Verify the dependency resolves**

Run: `cargo check -p goldens --tests`
Expected: PASS (no code uses the new dependency yet; this confirms the workspace dependency exists and resolves).

- [ ] **Step 3: Write the nesting test, with the shared helpers**

Create `goldens/tooling/tests/v02_flex.rs`:

```rust
//! v0.2 flex goldens (issue #11): one focused scene per construct —
//! nesting, sizing, clamping, alignment — so a regression implicates one
//! construct (DESIGN_1.md §8 bisect-by-construction) rather than one
//! opaque combined image.
//!
//! Scenes are authored against dashscene-core's `Txn` and solved by
//! dashscene-engine's `TaffySolver`. dashlang is not used: its builder
//! has no flex vocabulary and `Scene::build` commits through the fixed
//! solver, which ignores flex (`docs/decisions/negative-gap-lowering.md`
//! D3).
//!
//! Every scene is dimensioned so that each solved rect lands on an
//! integer. Integer-aligned solid fills produce no anti-aliased edges,
//! so these goldens compare exactly — unlike the v0.3 paint goldens,
//! whose gradients and curves need a tolerance
//! (`docs/decisions/golden-comparison-space.md`).

use dashpaint::{ImageTable, Painter};
use dashscene_core::{
    Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, Txn,
};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;

const NAVY: Color = Color { r: 0.05, g: 0.1, b: 0.2, a: 1.0 };
const RED: Color = Color { r: 0.8, g: 0.1, b: 0.1, a: 1.0 };
const GREEN: Color = Color { r: 0.1, g: 0.7, b: 0.2, a: 1.0 };
const GOLD: Color = Color { r: 0.9, g: 0.7, b: 0.1, a: 1.0 };
const BLUE: Color = Color { r: 0.2, g: 0.4, b: 0.9, a: 1.0 };

/// Adds a fixed-size filled child to `parent`.
fn boxed(txn: &mut Txn<'_>, parent: NodeId, w: f32, h: f32, color: Color) -> NodeId {
    let id = txn.add_node(Some(parent), None);
    txn.set_prop(id, Prop::Width(w));
    txn.set_prop(id, Prop::Height(h));
    txn.set_prop(id, Prop::Fill(color));
    id
}

/// Rect (x, y, w, h) of the DFS index `i` — the same index order
/// dashscene-engine's `tests/solve.rs` uses.
fn rect(arena: &Arena, i: usize) -> (f32, f32, f32, f32) {
    let r = arena.committed().rects()[i];
    (r.x, r.y, r.w, r.h)
}

/// Paints the committed scene on a `width`×`height` canvas and compares
/// it against the exact-match golden `name`.
fn render_and_compare(arena: &Arena, name: &str, width: i32, height: i32) {
    let scene = arena.committed();
    let mut painter = SkiaPainter::new(width, height);
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());
    goldens::assert_matches_golden(name, &painter.png_bytes());
}

#[test]
fn nesting_matches_its_golden() {
    // A 120×80 row of two 50×70 columns, gap 10, padding 5 — the
    // content fits the root exactly: 50 + 10 + 50 = 110 = 120 - (5 + 5).
    // Each column stacks two 50×30 cells with gap 10, which fills the
    // column's height exactly: 30 + 10 + 30 = 70.
    //
    // The cells cover their column edge to edge, so the column's own
    // fill shows only through the 10-high gap between them, and the
    // root's fill shows only through the padding. That is what makes
    // the nesting visible in the image.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(80.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Gap(10.0));
    txn.set_prop(
        root,
        Prop::Padding { left: 5.0, top: 5.0, right: 5.0, bottom: 5.0 },
    );
    txn.set_prop(root, Prop::Fill(NAVY));

    for (column_fill, cells) in [(RED, [GOLD, GREEN]), (BLUE, [GREEN, GOLD])] {
        let column = txn.add_node(Some(root), None);
        txn.set_prop(column, Prop::Width(50.0));
        txn.set_prop(column, Prop::Height(70.0));
        txn.set_prop(column, Prop::Mode(LayoutMode::Vertical));
        txn.set_prop(column, Prop::Gap(10.0));
        txn.set_prop(column, Prop::Fill(column_fill));
        for cell in cells {
            boxed(&mut txn, column, 50.0, 30.0, cell);
        }
    }
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 80.0), "root");
    assert_eq!(rect(&arena, 1), (5.0, 5.0, 50.0, 70.0), "first column at the padding origin");
    assert_eq!(rect(&arena, 2), (5.0, 5.0, 50.0, 30.0), "its first cell");
    assert_eq!(rect(&arena, 3), (5.0, 45.0, 50.0, 30.0), "its second cell: 5 + 30 + 10");
    assert_eq!(rect(&arena, 4), (65.0, 5.0, 50.0, 70.0), "second column: 5 + 50 + 10");
    assert_eq!(rect(&arena, 5), (65.0, 5.0, 50.0, 30.0), "its first cell");
    assert_eq!(rect(&arena, 6), (65.0, 45.0, 50.0, 30.0), "its second cell");

    render_and_compare(&arena, "v02-nesting", 120, 80);
}
```

- [ ] **Step 4: Run the test — the rect assertions must pass, the golden must be reported missing**

Run: `cargo test -p goldens --test v02_flex -- nesting_matches_its_golden --nocapture`
Expected: FAIL, with the panic `golden .../goldens/images/v02-nesting.png is missing — generate and commit it with UPDATE_GOLDENS=1`.

This is the meaningful red step. Reaching that panic means all seven rect assertions passed, which is what confirms the hand-computed layout. If a rect assertion fails instead, the arithmetic in the spec is wrong: understand why before changing any expected value — do not simply paste in whatever the solver returned, because that would make the test assert the solver's current behavior rather than the intended layout.

- [ ] **Step 5: Generate the golden**

Run: `UPDATE_GOLDENS=1 cargo test -p goldens --test v02_flex -- nesting_matches_its_golden`
Expected: PASS, with `UPDATE_GOLDENS: wrote .../goldens/images/v02-nesting.png` on stderr.

- [ ] **Step 6: Look at the generated image before committing it**

Read `goldens/images/v02-nesting.png` and confirm it shows what the scene intends: a navy 5-pixel border around the whole image; two 50-wide columns separated by a 10-wide navy stripe; within each column a 10-high stripe of the column's own colour (red on the left, blue on the right) between its two cells; cells gold/green on the left and green/gold on the right.

If the image does not show this, the scene is wrong — fix it, and regenerate. Do not commit an image you have not looked at.

- [ ] **Step 7: Verify the golden now compares clean**

Run: `cargo test -p goldens --test v02_flex -- nesting_matches_its_golden`
Expected: PASS, with no tolerance note on stderr (an exact match prints nothing).

- [ ] **Step 8: Commit**

```bash
git add goldens/tooling/Cargo.toml goldens/tooling/tests/v02_flex.rs goldens/images/v02-nesting.png
git commit -m "test(goldens): pin H/V flex nesting with an exact-match golden

The goldens crate gains dashscene-engine as a dev-dependency: scenes are
authored against core's Txn and solved with TaffySolver, because dashlang
has no flex vocabulary and its build() commits through the fixed solver.

The scene is dimensioned so every solved rect lands on an integer, so the
fills carry no anti-aliased edges and the golden compares exactly.

Part of #11.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: The sizing golden (hug beside equal-split fills)

**Files:**

- Modify: `goldens/tooling/tests/v02_flex.rs` (append one test)
- Create: `goldens/images/v02-sizing.png`

**Interfaces:**

- Consumes: `boxed`, `rect`, `render_and_compare`, and the colour constants from Task 1.
- Produces: nothing new. This test is also the executable proof referenced by the corpus case in Task 5, so its name — `sizing_matches_its_golden` — is quoted there.

- [ ] **Step 1: Write the failing test**

Append to `goldens/tooling/tests/v02_flex.rs`:

```rust
#[test]
fn sizing_matches_its_golden() {
    // A 120×60 row: a Hug node followed by two Fill siblings.
    //
    // The Hug node has no authored width — it takes its 30-wide child's
    // width. That leaves 120 - 30 = 90 of free space, which the two Fill
    // siblings split equally (45 each): core has no fill weight, and the
    // engine maps every Fill to flex_grow = 1.
    //
    // The Hug node's child is only 40 high against the node's 60, so the
    // node's own fill shows below it — otherwise the child would cover
    // the node exactly and the hug box would be invisible in the image.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(60.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(root, Prop::Fill(NAVY));

    let hug = txn.add_node(Some(root), Some("hug"));
    txn.set_prop(hug, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(hug, Prop::SizingH(AxisSizing::Hug));
    txn.set_prop(hug, Prop::Height(60.0));
    txn.set_prop(hug, Prop::Fill(RED));
    boxed(&mut txn, hug, 30.0, 40.0, GOLD);

    for fill_color in [GREEN, BLUE] {
        let fill = txn.add_node(Some(root), None);
        txn.set_prop(fill, Prop::SizingH(AxisSizing::Fill));
        txn.set_prop(fill, Prop::Height(60.0));
        txn.set_prop(fill, Prop::Fill(fill_color));
    }
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 60.0), "root");
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 30.0, 60.0), "hug takes its content's width");
    assert_eq!(rect(&arena, 2), (0.0, 0.0, 30.0, 40.0), "the hug node's fixed child");
    assert_eq!(rect(&arena, 3), (30.0, 0.0, 45.0, 60.0), "first Fill: (120 - 30) / 2");
    assert_eq!(rect(&arena, 4), (75.0, 0.0, 45.0, 60.0), "second Fill: the equal split");

    render_and_compare(&arena, "v02-sizing", 120, 60);
}
```

- [ ] **Step 2: Run the test — rects pass, golden missing**

Run: `cargo test -p goldens --test v02_flex -- sizing_matches_its_golden --nocapture`
Expected: FAIL with `golden .../v02-sizing.png is missing — generate and commit it with UPDATE_GOLDENS=1`, reached only after all five rect assertions pass.

- [ ] **Step 3: Generate the golden**

Run: `UPDATE_GOLDENS=1 cargo test -p goldens --test v02_flex -- sizing_matches_its_golden`
Expected: PASS, `UPDATE_GOLDENS: wrote .../v02-sizing.png`.

- [ ] **Step 4: Look at the image**

Read `goldens/images/v02-sizing.png` and confirm: a gold 30×40 block in the top-left, red beneath it down to the bottom edge (the hug node's own fill), then a green 45-wide band, then a blue 45-wide band reaching the right edge. No navy is visible — the three children cover the root's full width.

- [ ] **Step 5: Verify it compares clean**

Run: `cargo test -p goldens --test v02_flex -- sizing_matches_its_golden`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add goldens/tooling/tests/v02_flex.rs goldens/images/v02-sizing.png
git commit -m "test(goldens): pin hug-in-fill and the equal Fill split

The Hug node takes its child's width; the two Fill siblings split the
remaining space equally, which is the whole of core's fill vocabulary —
there is no authored weight, and the engine maps every Fill to
flex_grow = 1.

Part of #11.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: The clamping golden (min/max beat the flex distribution)

**Files:**

- Modify: `goldens/tooling/tests/v02_flex.rs` (append one helper and one test)
- Create: `goldens/images/v02-clamping.png`

**Interfaces:**

- Consumes: `rect`, `render_and_compare`, and the colour constants from Task 1.
- Produces: the file-local helper `clamped_row(txn, root, clamp, first, second)`, used only by this test.

- [ ] **Step 1: Write the failing test**

Append to `goldens/tooling/tests/v02_flex.rs`:

```rust
/// A 120×30 row of two Fill children, the first carrying `clamp`.
/// Unclamped the two would split 60/60, so the row shows exactly what
/// the clamp changed.
fn clamped_row(txn: &mut Txn<'_>, root: NodeId, clamp: Prop, first: Color, second: Color) {
    let row = txn.add_node(Some(root), None);
    txn.set_prop(row, Prop::Width(120.0));
    txn.set_prop(row, Prop::Height(30.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));

    let clamped = txn.add_node(Some(row), None);
    txn.set_prop(clamped, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(clamped, clamp);
    txn.set_prop(clamped, Prop::Height(30.0));
    txn.set_prop(clamped, Prop::Fill(first));

    let rest = txn.add_node(Some(row), None);
    txn.set_prop(rest, Prop::SizingH(AxisSizing::Fill));
    txn.set_prop(rest, Prop::Height(30.0));
    txn.set_prop(rest, Prop::Fill(second));
}

#[test]
fn clamping_matches_its_golden() {
    // A 120×60 column of two 120×30 rows. Both rows hold two Fill
    // children that would split 60/60; the clamp on the first child
    // moves the split in each direction, and the freed space goes to the
    // unclamped sibling:
    //   row one, MaxWidth 40  ->  40 / 80
    //   row two, MinWidth 100 -> 100 / 20
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(120.0));
    txn.set_prop(root, Prop::Height(60.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(root, Prop::Fill(NAVY));

    clamped_row(&mut txn, root, Prop::MaxWidth(40.0), RED, GREEN);
    clamped_row(&mut txn, root, Prop::MinWidth(100.0), GOLD, BLUE);
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 120.0, 60.0), "root");
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 120.0, 30.0), "the max row");
    assert_eq!(rect(&arena, 2), (0.0, 0.0, 40.0, 30.0), "capped at MaxWidth 40");
    assert_eq!(rect(&arena, 3), (40.0, 0.0, 80.0, 30.0), "its sibling takes the rest");
    assert_eq!(rect(&arena, 4), (0.0, 30.0, 120.0, 30.0), "the min row");
    assert_eq!(rect(&arena, 5), (0.0, 30.0, 100.0, 30.0), "floored at MinWidth 100");
    assert_eq!(rect(&arena, 6), (100.0, 30.0, 20.0, 30.0), "its sibling keeps only 20");

    render_and_compare(&arena, "v02-clamping", 120, 60);
}
```

- [ ] **Step 2: Run the test — rects pass, golden missing**

Run: `cargo test -p goldens --test v02_flex -- clamping_matches_its_golden --nocapture`
Expected: FAIL with `golden .../v02-clamping.png is missing`, reached only after all seven rect assertions pass.

Note on the min case: `dashscene-engine`'s own tests pin `MaxWidth` on a Fill child (`min_and_max_constraints_clamp_fill_and_hug`) but pin `MinHeight` only on a Hug node. The 100/20 split here is the standard flexbox resolution — the min violation freezes the first child at 100 and the remaining 20 goes to the second — but it is the one number in this plan that no existing test already demonstrates. If it comes out differently, stop and investigate the engine's style mapping rather than adjusting the expectation.

- [ ] **Step 3: Generate the golden**

Run: `UPDATE_GOLDENS=1 cargo test -p goldens --test v02_flex -- clamping_matches_its_golden`
Expected: PASS, `UPDATE_GOLDENS: wrote .../v02-clamping.png`.

- [ ] **Step 4: Look at the image**

Read `goldens/images/v02-clamping.png` and confirm: the top half is a narrow red band (40 wide) followed by a wide green band (80 wide); the bottom half is a wide gold band (100 wide) followed by a narrow blue band (20 wide). The two rows are visibly mirror images of each other in proportion. No navy is visible.

- [ ] **Step 5: Verify it compares clean**

Run: `cargo test -p goldens --test v02_flex -- clamping_matches_its_golden`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add goldens/tooling/tests/v02_flex.rs goldens/images/v02-clamping.png
git commit -m "test(goldens): pin that min/max clamps beat the Fill distribution

Two rows that would each split 60/60: MaxWidth pulls the first child to
40 and MinWidth pushes it to 100, and in both rows the freed space goes
to the unclamped sibling.

Part of #11.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: The alignment golden (every main and cross alignment)

**Files:**

- Modify: `goldens/tooling/tests/v02_flex.rs` (append one helper and one test)
- Create: `goldens/images/v02-alignment.png`

**Interfaces:**

- Consumes: `boxed`, `rect`, `render_and_compare`, and the colour constants from Task 1.
- Produces: the file-local helper `align_row(txn, root, main, cross, padding, colors)`, used only by this test.

- [ ] **Step 1: Write the failing test**

Append to `goldens/tooling/tests/v02_flex.rs`:

```rust
/// A 160×20 row holding two 30×10 children with gap 10, under the given
/// alignments and padding. Content is 30 + 10 + 30 = 70 wide.
fn align_row(
    txn: &mut Txn<'_>,
    root: NodeId,
    main: MainAxisAlign,
    cross: CrossAxisAlign,
    padding: (f32, f32, f32, f32),
    colors: [Color; 2],
) {
    let row = txn.add_node(Some(root), None);
    txn.set_prop(row, Prop::Width(160.0));
    txn.set_prop(row, Prop::Height(20.0));
    txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
    txn.set_prop(row, Prop::Gap(10.0));
    let (left, top, right, bottom) = padding;
    txn.set_prop(row, Prop::Padding { left, top, right, bottom });
    txn.set_prop(row, Prop::MainAlign(main));
    txn.set_prop(row, Prop::CrossAlign(cross));
    for color in colors {
        boxed(txn, row, 30.0, 10.0, color);
    }
}

#[test]
fn alignment_matches_its_golden() {
    // A 160×80 column of four 160×20 rows, one per alignment pairing.
    // The rows carry no fill, so the root's navy shows through and each
    // row's two children read as blocks against it.
    //
    // Main-axis free space in an unpadded row is 160 - 70 = 90 and
    // cross-axis free space is 20 - 10 = 10, so both centre offsets are
    // whole numbers (45 and 5).
    //
    //   row 0, y = 0    Start / Start, padding (10, 2, 10, 2)
    //   row 1, y = 20   Center / Center
    //   row 2, y = 40   End / End
    //   row 3, y = 60   SpaceBetween / Center
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(160.0));
    txn.set_prop(root, Prop::Height(80.0));
    txn.set_prop(root, Prop::Mode(LayoutMode::Vertical));
    txn.set_prop(root, Prop::Fill(NAVY));

    align_row(
        &mut txn,
        root,
        MainAxisAlign::Start,
        CrossAxisAlign::Start,
        (10.0, 2.0, 10.0, 2.0),
        [RED, GOLD],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::Center,
        CrossAxisAlign::Center,
        (0.0, 0.0, 0.0, 0.0),
        [GREEN, BLUE],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::End,
        CrossAxisAlign::End,
        (0.0, 0.0, 0.0, 0.0),
        [GOLD, RED],
    );
    align_row(
        &mut txn,
        root,
        MainAxisAlign::SpaceBetween,
        CrossAxisAlign::Center,
        (0.0, 0.0, 0.0, 0.0),
        [BLUE, GREEN],
    );
    txn.commit_with(&mut TaffySolver::new());

    assert_eq!(rect(&arena, 0), (0.0, 0.0, 160.0, 80.0), "root");

    // Start / Start, padded: content begins at the padding origin.
    assert_eq!(rect(&arena, 1), (0.0, 0.0, 160.0, 20.0), "row 0");
    assert_eq!(rect(&arena, 2), (10.0, 2.0, 30.0, 10.0), "start, at the left padding");
    assert_eq!(rect(&arena, 3), (50.0, 2.0, 30.0, 10.0), "10 + 30 + 10 gap");

    // Center / Center: 90 free on the main axis, 10 on the cross.
    assert_eq!(rect(&arena, 4), (0.0, 20.0, 160.0, 20.0), "row 1");
    assert_eq!(rect(&arena, 5), (45.0, 25.0, 30.0, 10.0), "centered: 90 / 2");
    assert_eq!(rect(&arena, 6), (85.0, 25.0, 30.0, 10.0), "45 + 30 + 10 gap");

    // End / End: content is flush with the right and bottom edges.
    assert_eq!(rect(&arena, 7), (0.0, 40.0, 160.0, 20.0), "row 2");
    assert_eq!(rect(&arena, 8), (90.0, 50.0, 30.0, 10.0), "end: 160 - 70");
    assert_eq!(rect(&arena, 9), (130.0, 50.0, 30.0, 10.0), "flush right: 160 - 30");

    // SpaceBetween: the free space becomes the space between the two,
    // so the authored gap is subsumed by it.
    assert_eq!(rect(&arena, 10), (0.0, 60.0, 160.0, 20.0), "row 3");
    assert_eq!(rect(&arena, 11), (0.0, 65.0, 30.0, 10.0), "flush left");
    assert_eq!(rect(&arena, 12), (130.0, 65.0, 30.0, 10.0), "flush right");

    render_and_compare(&arena, "v02-alignment", 160, 80);
}
```

- [ ] **Step 2: Run the test — rects pass, golden missing**

Run: `cargo test -p goldens --test v02_flex -- alignment_matches_its_golden --nocapture`
Expected: FAIL with `golden .../v02-alignment.png is missing`, reached only after all thirteen rect assertions pass.

- [ ] **Step 3: Generate the golden**

Run: `UPDATE_GOLDENS=1 cargo test -p goldens --test v02_flex -- alignment_matches_its_golden`
Expected: PASS, `UPDATE_GOLDENS: wrote .../v02-alignment.png`.

- [ ] **Step 4: Look at the image**

Read `goldens/images/v02-alignment.png` and confirm, against a navy background, four bands of two blocks each: the top pair pushed to the left and up; the second pair centred both ways; the third pair pushed right and down; the bottom pair split to the two edges. The four rows should be visibly distinguishable from one another — if two rows look the same, an alignment is not being applied.

- [ ] **Step 5: Verify it compares clean**

Run: `cargo test -p goldens --test v02_flex -- alignment_matches_its_golden`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add goldens/tooling/tests/v02_flex.rs goldens/images/v02-alignment.png
git commit -m "test(goldens): pin every main and cross alignment with gap and padding

Four rows cover all four MainAxisAlign variants and all three
CrossAxisAlign variants. Free space is 90 on the main axis and 10 on the
cross, so both centre offsets are whole numbers and the golden stays
exact-match.

Part of #11.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: The hug-in-fill corpus case

**Files:**

- Create: `corpus/dsl-generated/hug-in-fill.md`
- Modify: `corpus/dsl-generated/README.md` (the "Cases" list)

**Interfaces:**

- Consumes: the test `sizing_matches_its_golden` from Task 2, which the corpus entry names as its executable proof.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the corpus case**

`corpus/dsl-generated/hug-in-fill.md` follows the shape of the existing `negative-gap.md` — the construct, the scene, the expected solved rects, and the executable proof:

```markdown
# Corpus case: hug-in-fill

    construct  a Hug-sized node among Fill-sized siblings
    exercised  goldens/tooling/tests/v02_flex.rs (sizing_matches_its_golden)
    golden     goldens/images/v02-sizing.png

## The scene

A horizontal container, 120×60, holding a `Hug` node followed by two
`Fill` nodes. The `Hug` node has no authored width — it holds one fixed
30×40 child:

    root (mode Horizontal, 120×60)
      ├── hug  (SizingH Hug, height 60)
      │     └── inner (fixed 30×40)
      ├── fill-a (SizingH Fill, height 60)
      └── fill-b (SizingH Fill, height 60)

## Expected solved rects

    hug:     x = 0    w = 30    (its content's width)
    fill-a:  x = 30   w = 45    ((120 - 30) / 2)
    fill-b:  x = 75   w = 45

## Why it is an edge case

The two sizing modes resolve against each other in one pass: the `Hug`
node's width is content-driven and must be known before the free space
the `Fill` siblings divide can be computed. Getting the order wrong
gives the `Fill` children the full 120 and pushes the `Hug` node out of
the container.

Core has no fill weight, and `dashscene-engine` maps every `Fill` to
`flex_grow = 1`, so the two `Fill` siblings always split the free space
equally.
```

- [ ] **Step 2: Add it to the corpus README's case list**

In `corpus/dsl-generated/README.md`, the "Cases" section currently lists only the negative-gap case. Add the new entry, keeping the existing one:

```markdown
## Cases

- [negative-gap.md](negative-gap.md) — negative flex gap lowered to
  child margins (story #10). Executable proof:
  `crates/dashscene-engine/tests/solve.rs`.
- [hug-in-fill.md](hug-in-fill.md) — a Hug-sized node among Fill-sized
  siblings (story #11). Executable proof:
  `goldens/tooling/tests/v02_flex.rs`.
```

- [ ] **Step 3: Lint the Markdown**

Run: `dprint fmt corpus/dsl-generated/hug-in-fill.md corpus/dsl-generated/README.md && dprint check corpus/dsl-generated/hug-in-fill.md corpus/dsl-generated/README.md && markdownlint corpus/dsl-generated/hug-in-fill.md corpus/dsl-generated/README.md`
Expected: PASS (no output from `markdownlint`).

- [ ] **Step 4: Commit**

```bash
git add corpus/dsl-generated/hug-in-fill.md corpus/dsl-generated/README.md
git commit -m "docs(corpus): record the hug-in-fill case

The one case from DESIGN_1.md §11 E3's stress list that v0.2 reaches;
wrap and grid spans are v0.8, bidi is v0.6, variant topology is v0.4.
Its executable proof is the v02-sizing golden test.

Part of #11.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Full verification and the two out-of-scope issues

**Files:** none changed.

**Interfaces:** none.

- [ ] **Step 1: Run the whole golden suite from cold**

Run: `cargo test -p goldens`
Expected: PASS — the four new tests plus the existing v01 and v03 ones. No `UPDATE_GOLDENS` in the environment, and no tolerance notes on stderr from the four v02 goldens.

- [ ] **Step 2: Confirm no stale failure artifacts are about to be committed**

Run: `git status --short goldens/`
Expected: empty. An `*.actual.png` here would mean a golden failed at some point and the artifact was left behind; the harness removes it on a pass, so anything remaining is a real problem to investigate.

- [ ] **Step 3: Run the full build**

Run: `just build`
Expected: PASS — this is what CI runs (`cargo test --workspace`, `clippy -D warnings`, `cargo fmt --check`, `dprint check`, `markdownlint`).

- [ ] **Step 4: File the fill-weights issue**

The epic's scope list for #11 names "fill weights", but no weight vocabulary exists in core, `dashbuf`, or the engine, and the spec removed it from this story. It needs a home rather than a silent drop. Note the open question in the issue rather than assuming the answer: Figma's auto-layout splits its Fill children equally too, so authored weights may be a CSS concept SCD does not need for parity at all.

```bash
gh issue create \
  --title "debt(dashscene-core): decide whether SCD needs authored fill weights" \
  --label debt \
  --body "Story #11's scope list (epic #7) names \"fill weights\", but no such vocabulary exists: core has \`AxisSizing::{Fixed, Hug, Fill}\`, and \`dashscene-engine\` maps every \`Fill\` to \`flex_grow = 1.0\`. Fill siblings therefore always split free space equally, which is what \`goldens/tooling/tests/v02_flex.rs\` (\`sizing_matches_its_golden\`) now pins.

Story #11 goldened the equal split rather than inventing a weight, and recorded that in \`docs/decisions/\` via its gardening pass.

The open question is whether SCD wants authored weights at all. Figma auto-layout has no flex weight — its \"fill container\" children divide the space equally — so weights would be a CSS-flexbox concept with no Figma counterpart, and P5 says no producer's limitations define the format but does not say the format should grow constructs nothing needs.

Decide at v0.8 (fidelity), where the layout-fidelity work lives:

- If yes: a \`dashbuf\` \`LayoutConstraints\` field, a core \`Prop\`, an engine \`flex_grow\` mapping, and a golden for an unequal split.
- If no: close this, and drop \"fill weights\" from the epic's scope wording."
```

- [ ] **Step 5: File the dashlang flex-vocabulary issue**

`docs/decisions/negative-gap-lowering.md` D3 deferred this, story #11 confirmed it is still deferred, and corpus generator #46 depends on it. Filing it now means #46 does not discover the gap at v0.8.

```bash
gh issue create \
  --title "story: dashlang — flex builder vocabulary + Scene::build_with(solver)" \
  --label story \
  --body "\`dashlang\`'s builder exposes \`at\`/\`size\`/\`fill\`/\`child\` only, and \`Scene::build\` commits through \`commit()\` — the fixed solver, which ignores flex. So no flex scene can be authored in the DSL today, and \`goldens/tooling/tests/v02_flex.rs\` (story #11) authors its scenes against core's \`Txn\` directly instead.

\`docs/decisions/negative-gap-lowering.md\` D3 deferred both halves of this: the flex vocabulary itself, and how a dashlang scene reaches the engine's solver.

Scope:

- The v0.2 layout props on \`dashlang::Node\`: mode, gap, padding, margin, main/cross alignment, per-axis sizing, min/max.
- \`Scene::build_with(&self, arena: &mut Arena, solver: &mut dyn LayoutSolver) -> u64\`, calling \`Txn::commit_with\`. \`LayoutSolver\` is a **core** trait, so the caller injects \`TaffySolver\` and dashlang keeps its core-only dependency (story #5's constraint) — it must not gain a \`dashscene-engine\` dependency.
- Port \`v02_flex.rs\`'s scenes onto the DSL once it exists.

**Blocks #46** (the DSL-generated stress corpus), which \`corpus/dsl-generated/README.md\` already records as needing this vocabulary."
```

- [ ] **Step 6: Comment on the story issue with the state of play**

```bash
gh issue comment 11 --body "Implemented on \`story/flex-goldens\`.

Four exact-match goldens, one per construct (DESIGN §8 bisect-by-construction): \`v02-nesting\`, \`v02-sizing\`, \`v02-clamping\`, \`v02-alignment\`. Scenes are authored against core's \`Txn\` and solved with \`TaffySolver\`; every scene is dimensioned so its solved rects land on integers, so the fills carry no anti-aliased edges and the goldens compare exactly (no tolerance, unlike the v0.3 paint goldens).

Plus the \`hug-in-fill\` corpus case — the one entry from DESIGN §11 E3's stress list that v0.2 reaches.

Two items left the scope, each with its own issue rather than a silent drop:

- **Fill weights** (named in the epic's scope list): no weight vocabulary exists anywhere in core, dashbuf, or the engine, and Figma has no flex weight either — so the question is whether SCD wants the construct at all, not just how to golden it.
- **dashlang's flex vocabulary** (negative-gap D3): still deferred; #46 depends on it."
```

- [ ] **Step 7: Garden the working memory, then open the PR**

`docs/wip/` must be empty of this story's spec and plan before a `main`-targeting PR lands — the `sdd-working-memory-lifecycle` rule, enforced by `wip-gate.sh`.

Invoke the `sdd-gardening` skill. It moves this spec and plan into `docs/archive/` and writes the durable records. Expect one decision record for what this story actually settled — that flex goldens are exact-match because their scenes are dimensioned to solve to integers, which is a constraint binding every future flex golden — and an update to `docs/decisions/negative-gap-lowering.md` noting that D3 now has an issue against it.

Then run `/code-review` on the diff (per AGENTS.md, at `high`), capture every finding as a checklist in the PR description, fix the critical ones, and file one `debt` issue per minor one.

Open the PR with `Closes #11`, and note in the body that this is the last story of epic #7 — merging it closes the epic and triggers the phase-end plan revision.

---

## Self-Review

**Spec coverage.** Every section of `docs/wip/2026-07-12-flex-goldens-design.md` maps to a task: D1 (Txn-authored scenes) → Task 1 Step 3; D2 (one golden per construct) → Tasks 1–4, one each; D3 (integer rects, exact-match) → the Global Constraints plus each task's Step 2 red step; D4's four scenes → Tasks 1–4; D5 (assert rects, then the golden) → the structure of every test; D6 (hug-in-fill only) → Task 5. The spec's acceptance criteria map to Task 6: `cargo test -p goldens` (Step 1), `just build` (Step 3), and the two issues (Steps 4–5).

**Placeholders.** None. Every code step carries complete code; every command carries its expected output.

**Type consistency.** `boxed`, `rect`, and `render_and_compare` are defined once in Task 1 Step 3 and called with those exact signatures in Tasks 2–4. `clamped_row` (Task 3) and `align_row` (Task 4) are each defined and used within their own task. `Prop::Padding` is always constructed with all four named fields, matching core's definition. `SkiaPainter::new` takes `i32`, which is why `render_and_compare` takes `width: i32, height: i32`.

**One risk worth naming.** Task 3's `MinWidth` case (100/20) is the only expected value in this plan that no existing test already demonstrates — the engine's own suite pins `MaxWidth` on a Fill child but `MinHeight` only on a Hug node. Task 3 Step 2 says so, and says to investigate rather than adjust the expectation if it comes out differently.
