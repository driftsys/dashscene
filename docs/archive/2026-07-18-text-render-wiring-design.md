# #327 — wire the lowered text axes through the render/measure path

    status   design (working memory)
    story    #327 (follow-up from S1/#310) of the full real-file-import epic
    scope    dashscene-engine (measure seam), goldens/tooling (render.rs stager)
    base     main 270c022

## Why

S1 lowered four Figma text axes into the `TextStyle` schema (line-height-px,
letter-spacing, horizontal align, vertical align) and gave `dashscene-typeset`
the capability to honor them (`TextShape` + `layout_with`), but left them NOT
wired to the render/measure path (tracked as #327): the engine measure seam
builds a `TextContext { text, size }` and calls `layout()` (default axes), and
the render stager calls `ts.layout(...)` too. So a lowered document's line-height,
letter-spacing, and alignment persist to the `.dsb` but do not affect a measured
or rendered result — visible as imperfect text in the live `just render` output.

This wires the axes from the arena `TextStyle` through to `layout_with`, so an
imported document's text renders honoring them.

## Current shapes (re-verify against the tree — #320 reworked this area)

- `struct TextContext` (`crates/dashscene-engine/src/lib.rs:377`) — carries
  `text`, `size` only.
- `text_context(arena, node) -> Option<TextContext>` (:386) — reads the node's
  style; today takes only `style.size`.
- `measure_text(...)` (:405) — calls `typesetter.layout(&context.text,
  context.size, max_width)` (:416).
- A SECOND `typesetter.layout(...)` call at :946 (a function ~:924 — investigate:
  measure vs a staging/positioning pass). Both call sites must honor the axes.
- `dashscene-typeset`: `TextShape { line_height_px: Option<f32>, letter_spacing,
  align }` + `layout_with(text, size, max_width, shape)`; `layout(...)` delegates
  with `TextShape::default()` (byte-identical). Confirm the exact `TextShape`
  field names/shape from the S1 code before use.
- The render stager: `goldens/tooling/src/render.rs` `text_runs`/`stage_text`
  (calls `ts.layout(text, size, None)`), plus the `vertical_offset` helper
  (`goldens/tooling/src/lib.rs`) for v-align, which has no caller yet.

## Design

### 1. Engine measure seam (production)

- `TextContext` carries a `TextShape` (the three _measure-affecting_ axes:
  line-height-px, letter-spacing, horizontal align). Vertical align does NOT
  affect the measured/solved box — it is placement only (stager, below) — so it
  is NOT in `TextContext`.
- `text_context` reads `line_height_px`, `letter_spacing`, `text_align` from the
  arena `TextStyle` into the `TextShape`.
- Both engine `layout(...)` call sites (:416 and :946) become
  `layout_with(..., shape)` using the node's `TextShape`.

### 2. Render stager (goldens/tooling/src/render.rs — the imported-file path)

- `text_runs`/`stage_text` read the node's `TextStyle`, build a `TextShape`, and
  call `layout_with(text, size, Some(box_width), shape)` so placement honors
  line-height / letter-spacing / horizontal align. **Refinement (recorded during
  #327):** the container is the node's _resolved box width_, not `None`. With
  `None` the container equals the widest line, so horizontal alignment is a no-op
  for single-line text — it would not honor `text_align` in the live render,
  contradicting this section's own intent. The box width is the width the engine
  measured with the same axes, so the stager's line breaks match the solved box.
  `render_dsb` carries no golden, so there is no byte-identical constraint, and
  E7's own stager copy (`render_oracle.rs`) is untouched.
- Apply vertical alignment: offset the staged run block by
  `(box_height − content_height) × factor` using the existing `vertical_offset`
  helper, reading the node's resolved box height. (This gives `vertical_offset`
  its first caller.)

## Guardrails (E7 exit gate — untouched)

- **Do NOT touch** `goldens/tooling/tests/render_oracle.rs` (its own `stage_text`
  stays on `layout()`), `goldens/oracle/manifest.json`, the design-source PNGs,
  or the bands in `goldens/tooling/src/oracle.rs`. The E7 fixtures use DEFAULT
  axes, so:
  - The engine change is byte-identical for them: `TextShape::default()` ⇒
    `layout_with` == `layout`, so the measure/solve is unchanged and the E7
    render is byte-identical. **Verify `cargo test -p goldens --test render_oracle`
    still passes (15/15).**
  - The E7 stager (render_oracle.rs) is left on `layout()` — untouched.
- **E7 track is active in `dashscene-engine`** (#320 baseline). Rebase before
  merge; the changes are distinct regions (baseline rows vs TextContext-carries-
  TextShape) but coordinate carefully.
- **P1/P2:** the document carries the axis intent (enums/values), never resolved
  offsets; the painter is unchanged; alignment is resolved in typeset (h) and the
  stager (v), not the painter.

## Test strategy (TDD)

- Engine: a TEXT node with a fixed `line_height_px` (or letter-spacing) measures
  to a DIFFERENT solved height/width than the same node with default axes (proves
  the measure seam honors the axis). A default-axis node measures identically to
  before (byte-identical guard).
- Stager (render.rs): `render_dsb` on a small in-process `.dsb` with a
  center-aligned / vertically-centered text node places the glyph run shifted vs
  a left/top node (assert glyph x / block y differ as expected).
- The E7 oracle test passes unchanged (default-axis fixtures byte-identical).
- Empirical: re-render first-light via `just render` and confirm the text
  placement is visibly improved (honoring its PIXELS line-height + alignment).

## Alternatives considered

- **Wire only the engine, not the stager.** Rejected: measurement alone would
  size the box right but the rendered glyphs would still place with default axes —
  the visible gap remains. Both are needed for the render to honor the axes.
- **Also convert the E7 stager to `layout_with`.** Rejected: unnecessary (E7
  fixtures are default-axis → identical output) and it touches the live E7 test —
  collision + gate risk. Leave it on `layout()`.
- **Stager lays out at `max_width = None`.** Rejected during implementation: with
  `None` the alignment container is the widest line, so horizontal alignment does
  nothing for the common single-line label centered in a wide box — the exact
  imperfection #327 exists to fix. The stager passes the resolved box width
  instead (see §2), which also matches the box the engine measured.
