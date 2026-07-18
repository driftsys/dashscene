# Text-render-wiring Implementation Plan (#327)

> **For agentic workers:** implement task-by-task, TDD, one conventional commit
> per task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the four lowered Figma text axes (fixed line height, letter
spacing, horizontal align, vertical align) from the arena `TextStyle` through the
engine measure seam and the goldens render stager, so an imported document's text
measures and renders honoring them.

**Architecture:** `dashscene-typeset` already honors the three measure-affecting
axes through `TextShape` + `Typesetter::layout_with` (story #310); `layout()`
delegates with `TextShape::default()` (byte-identical). This story is pure
_wiring_: the engine's `TextContext` carries a `TextShape` built from the node's
style and both `typesetter.layout(...)` call sites become `layout_with(..., shape)`;
the render stager builds a `TextShape` per text node, lays out at the node's
resolved box width (so horizontal alignment centers within the box), and offsets
the whole block by `vertical_offset(box_height, content_height, valign)`.

**Tech Stack:** Rust 2024, cargo workspace, Taffy solver, `dashscene-typeset`,
Skia reference painter, `just` recipes.

## Global Constraints

- **P1** — the document carries axis _intent_ (enums/values), never resolved
  offsets. Horizontal alignment is resolved in typeset; vertical alignment in the
  stager. No resolved offset is stored in the arena.
- **P2** — the painter is unchanged: it colors placed glyphs, it does not measure,
  wrap, or move anything.
- **E7 exit gate is UNTOUCHED.** Do NOT modify
  `goldens/tooling/tests/render_oracle.rs`, `goldens/oracle/manifest.json`,
  `goldens/oracle/design-source/*`, or the bands in `goldens/tooling/src/oracle.rs`.
  E7 fixtures use DEFAULT axes ⇒ `TextShape::default()` ⇒ `layout_with == layout`
  ⇒ the engine measure/solve is byte-identical for them, and E7's own stager copy
  stays on `layout()`. `cargo test -p goldens --test render_oracle` must still pass
  15/15.
- One conventional commit per task. git-std scopes: `docs`, `dashscene-engine`,
  `goldens`.

## Confirmed current shapes (re-verified against the tree, base main 270c022)

- `dashscene_typeset::text::TextShape { line_height_px: Option<f32>,
  letter_spacing: f32, align: TextAlign }`, `TextShape::default()` =
  `{ None, 0.0, TextAlign::Left }`.
  `TextShape` derives `Debug, Clone, Copy, PartialEq`.
- `dashscene_typeset::text::TextAlign { Left, Center, Right }`.
- `Typesetter::layout_with(&mut self, text: &str, size: f32, max_width: Option<f32>, shape: TextShape) -> TextLayout`.
  `layout(...)` delegates with `TextShape::default()`.
- `dashscene_core::TextStyle { family, size, weight, color, line_height_px:
  Option<f32>, letter_spacing: f32, text_align: TextAlign, text_align_v: TextAlignV }`.
- `dashscene_core::TextAlign { Left, Center, Right }`,
  `dashscene_core::TextAlignV { Top, Center, Bottom }`.
- Engine `crates/dashscene-engine/src/lib.rs`: `struct TextContext { text: String,
  size: f32 }` (:377); `fn text_context(arena, node) -> Option<TextContext>` (:386);
  `fn measure_text(...)` calls `typesetter.layout(&context.text, context.size,
  max_width)` (:416); `fn collect_baseline_offsets(...)` calls
  `typesetter.layout(text, style.size, Some(child_layout.size.width))` (:946) with
  `style` (the arena `&TextStyle`) already in scope.
- Stager `goldens/tooling/src/render.rs`: `fn origin_of(arena, node) -> (f32, f32)`
  (:93); `fn text_runs(ts, atlases, origin, text, size, color)` calls
  `ts.layout(text, size, None)` (:111); `fn stage_text(...)` (:141) walks the arena
  and calls `text_runs` at `origin_of(...)`.
- `goldens/tooling/src/lib.rs`: `enum VerticalAlign { Top, Center, Bottom }` and
  `fn vertical_offset(box_height: f32, content_height: f32, align: VerticalAlign)
  -> f32` (:149) — no caller yet; this story is its first.
- `dashpaint::AtlasIndex(pub u32)`, `GlyphQuad { glyph_id, x, y }`,
  `GlyphRun { atlas, size, color, glyphs, opacity }`.
- `dashscene_core::SolvedRect { x, y, w, h }`.

---

### Task 1: Record the plan and the box-width design refinement

**Files:**

- Create: `docs/wip/2026-07-18-text-render-wiring-plan.md` (this file).
- Modify: `docs/wip/2026-07-18-text-render-wiring-design.md` (§2 + Alternatives).

**Refinement recorded:** the design's §2 wrote `layout_with(text, size, None,
shape)`. With `max_width = None` the container equals the widest line, so
horizontal alignment is a no-op for single-line text — it would not honor
`text_align` in the live render, contradicting §2's own intent. The stager instead
passes the node's resolved box **width** as the container, so single-line text
centers/right-aligns within the Figma box, consistent with the box the engine
measured with the same axes. `render_dsb` carries no golden, so there is no
byte-identical constraint; E7's own stager (`render_oracle.rs`) is untouched.

- [ ] **Step 1: Write this plan file and update the design doc §2/Alternatives.**
- [ ] **Step 2: Commit**

```bash
git add docs/wip/2026-07-18-text-render-wiring-plan.md docs/wip/2026-07-18-text-render-wiring-design.md
git commit -m "docs(text-render-wiring): add TDD plan and record box-width stager refinement"
```

---

### Task 2: Engine measure seam carries the TextShape

**Files:**

- Modify: `crates/dashscene-engine/src/lib.rs` — imports (:33), `TextContext`
  (:377), `text_context` (:386), `measure_text` (:416), `collect_baseline_offsets`
  (:946); add a private `fn text_shape(style: &dashscene_core::TextStyle) -> TextShape`.
- Test: `crates/dashscene-engine/tests/measure.rs`.

**Interfaces:**

- Produces (private): `TextContext { text: String, size: f32, shape: TextShape }`;
  `fn text_shape(style: &dashscene_core::TextStyle) -> TextShape`.
- Consumes: `Typesetter::layout_with`, `TextShape`, `dashscene_core::TextAlign`.

- [ ] **Step 1: Write the failing test** (append to `tests/measure.rs`)

```rust
/// A styled hug-height text node solved through the measure seam, returning
/// (width, height). `wrap` fixes the width (else the node hugs); `line_height`
/// and `align` exercise the wired axes.
fn solved_text_box(
    text: &str,
    size: f32,
    wrap: Option<f32>,
    line_height: Option<f32>,
    align: TextAlign,
) -> (f32, f32) {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let node = txn.add_node(None, None);
    match wrap {
        Some(w) => {
            txn.set_prop(node, Prop::SizingH(AxisSizing::Fixed));
            txn.set_prop(node, Prop::Width(w));
        }
        None => txn.set_prop(node, Prop::SizingH(AxisSizing::Hug)),
    }
    txn.set_prop(node, Prop::SizingV(AxisSizing::Hug));
    txn.set_prop(node, Prop::Text(text.to_string()));
    txn.set_prop(
        node,
        Prop::TextStyle(TextStyle {
            family: "Noto Sans".to_string(),
            size,
            weight: 400,
            color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
            line_height_px: line_height,
            letter_spacing: 0.0,
            text_align: align,
            text_align_v: TextAlignV::Top,
        }),
    );
    let mut ts = typesetter();
    txn.commit_with(&mut TaffySolver::with_typesetter(&mut ts));
    let rect = arena.committed().rects()[0];
    (rect.w, rect.h)
}

#[test]
fn a_fixed_line_height_grows_the_measured_height_and_default_is_byte_identical() {
    let text = "Hello world";
    let size = 32.0;
    // A width that fits one word but not the whole string forces two lines, so
    // the line advance drives the total height.
    let one_line = typesetter().layout(text, size, None).width;
    let wrap = one_line * 0.75;

    let (_, default_h) = solved_text_box(text, size, Some(wrap), None, TextAlign::Left);
    let (_, tall_h) = solved_text_box(text, size, Some(wrap), Some(80.0), TextAlign::Left);

    assert!(
        tall_h > default_h,
        "a fixed line height larger than the auto advance grows the measured height \
         (default {default_h}, fixed {tall_h})"
    );

    // Byte-identical guard: a default-axis node measures exactly the pre-#327
    // `layout()` height.
    let expected = typesetter().layout(text, size, Some(wrap));
    assert_eq!(
        default_h, expected.height,
        "a default-axis node's measured height is byte-identical to layout()"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dashscene-engine --test measure a_fixed_line_height -v`
Expected: FAIL — `tall_h > default_h` is false (the seam ignores the axis, so
both heights are the auto advance).

- [ ] **Step 3: Implement the wiring**

Change the typeset import (:33) to add `TextShape`:

```rust
use dashscene_typeset::text::{TextShape, Typesetter};
```

Add the shape field to `TextContext` (:377):

```rust
#[derive(Debug)]
struct TextContext {
    text: String,
    size: f32,
    /// The measure-affecting axes (fixed line height, letter spacing, horizontal
    /// align) from the node's `TextStyle` (story #327). Vertical align is not
    /// here: it is block placement, not a measured extent, so it lives in the
    /// stager, not the solve.
    shape: TextShape,
}
```

Build the shape in `text_context` (:386):

```rust
fn text_context(arena: &Arena, node: NodeId) -> Option<TextContext> {
    let text = arena.text(node)?;
    let style = arena.text_style(node)?;
    Some(TextContext {
        text: text.to_string(),
        size: style.size,
        shape: text_shape(style),
    })
}

/// The measure-affecting shaping axes of a node's text style (story #327): a
/// fixed line height, letter spacing, and horizontal alignment. Vertical
/// alignment is placement (the stager), not a measured extent, so it is not
/// carried here. `TextShape::default()` for a default-axis style, so the solve
/// stays byte-identical to the pre-#327 `layout()` path (the E7 guard).
fn text_shape(style: &dashscene_core::TextStyle) -> TextShape {
    TextShape {
        line_height_px: style.line_height_px,
        letter_spacing: style.letter_spacing,
        align: match style.text_align {
            dashscene_core::TextAlign::Left => dashscene_typeset::text::TextAlign::Left,
            dashscene_core::TextAlign::Center => dashscene_typeset::text::TextAlign::Center,
            dashscene_core::TextAlign::Right => dashscene_typeset::text::TextAlign::Right,
        },
    }
}
```

Change the `measure_text` call site (:416):

```rust
let laid = typesetter.layout_with(&context.text, context.size, max_width, context.shape);
```

Change the `collect_baseline_offsets` call site (:946) — `style` is the arena
`&TextStyle` already bound in the match arm:

```rust
let laid = typesetter.layout_with(
    text,
    style.size,
    Some(child_layout.size.width),
    text_shape(style),
);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p dashscene-engine`
Expected: PASS — the new test plus all existing `measure.rs`, `baseline.rs`,
`incremental.rs`, `solve.rs`, `flip.rs` tests (the latter guard :946 and default
identity).

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-engine/src/lib.rs crates/dashscene-engine/tests/measure.rs
git commit -m "feat(dashscene-engine): wire the lowered text axes through the measure seam"
```

---

### Task 3: Render stager honors the axes and vertical alignment

**Files:**

- Modify: `goldens/tooling/src/render.rs` — imports (:22-23), `origin_of` (:93),
  `text_runs` (:103), `stage_text` (:141); add private `fn text_shape` and
  `fn vertical_align`.
- Test: `goldens/tooling/src/render.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**

- Consumes: `Typesetter::layout_with`, `TextShape`, `crate::vertical_offset`,
  `crate::VerticalAlign`, `dashscene_core::{TextAlign, TextAlignV}`,
  `dashscene_core::TextStyle`.
- Produces (private): `text_runs(ts, atlases, origin: (f32, f32), box_size:
  (f32, f32), text, size, color, shape: TextShape, valign: VerticalAlign)`.

- [ ] **Step 1: Write the failing test** (add to the `mod tests` in `render.rs`)

```rust
    use dashpaint::{AtlasIndex, Color};
    use dashscene_typeset::text::{TextAlign, TextShape};

    use crate::VerticalAlign;

    use super::{oracle_typesetter, text_runs};

    #[test]
    fn the_stager_shifts_glyphs_for_center_and_vertical_center_alignment() {
        let mut ts = oracle_typesetter();
        // The atlas index only tags a run; it does not affect placement, so a
        // bare index pair is enough (font 0 = Latin, font 1 = Arabic).
        let atlases = [AtlasIndex(0), AtlasIndex(1)];
        let text = "Hi";
        let size = 32.0;
        let black = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
        let origin = (0.0, 0.0);
        // A box much wider and taller than "Hi", so centering has slack.
        let box_size = (400.0, 200.0);

        let left = text_runs(
            &mut ts, &atlases, origin, box_size, text, size, black,
            TextShape::default(), VerticalAlign::Top,
        );
        let center = text_runs(
            &mut ts, &atlases, origin, box_size, text, size, black,
            TextShape { line_height_px: None, letter_spacing: 0.0, align: TextAlign::Center },
            VerticalAlign::Center,
        );

        let left_glyph = left[0].glyphs[0];
        let center_glyph = center[0].glyphs[0];
        assert!(
            center_glyph.x > left_glyph.x,
            "center alignment shifts the first glyph right within the box \
             (left {}, center {})",
            left_glyph.x, center_glyph.x
        );
        assert!(
            center_glyph.y > left_glyph.y,
            "vertical centering shifts the block down within the box \
             (left {}, center {})",
            left_glyph.y, center_glyph.y
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p goldens --lib render::tests::the_stager_shifts_glyphs`
Expected: FAIL to COMPILE — `text_runs` still has the old 6-argument signature
(no `box_size`, `shape`, `valign`). This is the RED state.

- [ ] **Step 3: Implement the wiring**

Add to the typeset import (:23) and bring in the vertical-align helpers:

```rust
use dashscene_typeset::text::{Font, TextShape, Typesetter};
```

Extend `origin_of` (:93) to return the box height and width too:

```rust
/// The resolved box of a committed node: origin (x, y) and size (w, h).
fn box_of(arena: &Arena, node: NodeId) -> (f32, f32, f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.x, rect.y, rect.w, rect.h)
}

/// The `TextShape` for a node's text style (story #327): the fixed line height,
/// letter spacing, and horizontal alignment the stager lays the run out under.
fn text_shape(style: &dashscene_core::TextStyle) -> TextShape {
    TextShape {
        line_height_px: style.line_height_px,
        letter_spacing: style.letter_spacing,
        align: match style.text_align {
            dashscene_core::TextAlign::Left => dashscene_typeset::text::TextAlign::Left,
            dashscene_core::TextAlign::Center => dashscene_typeset::text::TextAlign::Center,
            dashscene_core::TextAlign::Right => dashscene_typeset::text::TextAlign::Right,
        },
    }
}

/// The stager's vertical alignment for a node's text style (story #327).
fn vertical_align(align: dashscene_core::TextAlignV) -> crate::VerticalAlign {
    match align {
        dashscene_core::TextAlignV::Top => crate::VerticalAlign::Top,
        dashscene_core::TextAlignV::Center => crate::VerticalAlign::Center,
        dashscene_core::TextAlignV::Bottom => crate::VerticalAlign::Bottom,
    }
}
```

Rewrite `text_runs` (:103) to lay out at the box width under `shape` and offset
the block by the vertical alignment:

```rust
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    origin: (f32, f32),
    box_size: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
    shape: TextShape,
    valign: crate::VerticalAlign,
) -> Vec<GlyphRun> {
    let (box_w, box_h) = box_size;
    // Lay out within the node's resolved box width so horizontal alignment
    // centers/right-aligns within the box (P1: the axis is intent; typeset
    // resolves it). The box width is the width the engine measured with the
    // same axes, so the line breaks match the solved box.
    let laid = ts.layout_with(text, size, Some(box_w), shape);
    // Vertical alignment is block placement, not paint (P2) and not measured
    // (P1): shift every glyph down by the box's free space above the block.
    let voff = crate::vertical_offset(box_h, laid.height, valign);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + voff + g.y,
            };
            match runs.last_mut() {
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
                    atlas,
                    size,
                    color,
                    glyphs: vec![quad],
                    opacity: 1.0,
                }),
            }
        }
    }
    runs
}
```

Update `stage_text`'s `walk` (:149-159) to pass the box, shape, and valign:

```rust
if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
    let (x, y, w, h) = box_of(arena, node);
    out.extend(text_runs(
        ts,
        atlases,
        (x, y),
        (w, h),
        text,
        style.size,
        style.color,
        text_shape(style),
        vertical_align(style.text_align_v),
    ));
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p goldens --lib` then `cargo test -p goldens --test render_oracle`
Expected: PASS — the new stager test, the existing `render_dsb` unit test, and the
E7 oracle 15/15 (untouched; its own stager copy stays on `layout()`).

- [ ] **Step 5: Commit**

```bash
git add goldens/tooling/src/render.rs
git commit -m "feat(goldens): honor the lowered text axes in the render stager"
```

---

### Task 4: Full build and E7 verification

- [ ] **Step 1:** `just build` — expect green.
- [ ] **Step 2:** `cargo test -p goldens --test render_oracle` — expect 15/15.
- [ ] **Step 3:** `cargo test -p dashscene-engine` — expect green.

No commit (verification only).

---

### Task 5: Empirical live re-render (leave artifact for the orchestrator)

- [ ] **Step 1:** `just wasm` (rebuild `dashc.wasm`).
- [ ] **Step 2:** `FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w) just render MRk9I5cYY6yJa8JhljzkBn 2411:10795` — never echo the token. Copy the output PNG to `/tmp/first-light-327.png`.
- [ ] **Step 3:** Report a one-line before/after on text placement. If the token is
      missing or the call returns 401/403, report that status instead.

No commit. Do NOT commit `.dsb`/`.png`/public content.

## Self-Review

- **Spec coverage:** engine measure seam (Task 2, both call sites), render stager
  - vertical alignment (Task 3), byte-identical default guard (Task 2 test), E7
    untouched (Global Constraints + Task 4), empirical (Task 5). Covered.
- **Placeholder scan:** none — every code step is complete.
- **Type consistency:** `text_shape` maps `dashscene_core::TextStyle` →
  `dashscene_typeset::text::TextShape` identically in engine (Task 2) and stager
  (Task 3); `text_runs`'s new signature in Task 3's implementation matches its use
  in Task 3's test; `box_of`/`vertical_align` names are used consistently.
