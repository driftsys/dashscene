# Figma TEXT vocabulary — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:test-driven-development.
> Steps use checkbox (`- [ ]`) syntax for tracking. This plan implements the
> approved design `docs/wip/2026-07-18-figma-text-vocabulary-design.md`.

**Goal:** Add four `TextStyle` fields — `line_height_px`, `letter_spacing`,
`text_align`, `text_align_v` — so first-light's TEXT nodes lower instead of
being refused; give the typesetter the capability to honor them; carry the
intent through the .dsb format and the runtime load.

**Architecture:** Append the four fields at the `TextStyle` flatbuffer table
tail with behavior-preserving defaults (R7). Flip the four refusal guards in
`dashc` `text_of` to populate them (PIXELS line height only; `%` and JUSTIFIED
stay refused). Mirror the fields in the two IR `TextStyle` structs (dashc
`document.rs`, core `arena.rs`) and read them in core `load.rs`. Give the
typesetter an additive `TextShape` + `layout_with(...)`; `layout(...)` keeps
its signature and delegates with `TextShape::default()` (E7 guard). Vertical
alignment is a stager placement (goldens/tooling).

**Tech Stack:** Rust 2024, flatbuffers (flatc-generated bindings), rustybuzz,
Taffy, skia-safe (unchanged — P2).

## Global Constraints

- **R7 byte-reproducibility:** append the four fields at the `TextStyle` table
  tail with defaults `line_height_px = null`, `letter_spacing = 0`,
  `text_align = Left`, `text_align_v = Top`. A document using none must emit
  byte-identically. **Do NOT regenerate `crates/dashbuf/tests/fixtures/v0_5_document.dsb`**
  or the text golden `.dsb`s under `goldens/dsb/`.
- **E7 exit gate:** keep `Typesetter::layout(text, size, max_width)` signature.
  New knobs ride on an additive `TextShape` with a default reproducing current
  behavior, and `layout_with(...)` that `layout()` delegates to. Do NOT touch
  the E7 oracle (`goldens/tooling/tests/render_oracle.rs`,
  `goldens/oracle/*`, the E7 sections of `docs/specification/05-qualification.md`).
- **P1/P2:** the painter never changes; horizontal align in the typesetter,
  vertical align in the stager; the document carries the align intent (an enum),
  never a resolved offset.
- **Scope:** lower ONLY `PIXELS` line height. `%`/`FONT_SIZE_%` line height,
  `JUSTIFIED` alignment, and mixed-style segments (`styleOverrideTable`) stay
  refused.
- **Commit scopes** (git-std allowlist): `dashc`, `dashbuf`, `dashscene-core`,
  `dashscene-typeset`, `goldens`, `docs`. One conventional commit per task.

---

## File structure

- `crates/dashc/src/document.rs` — IR `TextStyle` + `TextAlign`/`TextAlignV` enums.
- `crates/dashc/src/figma/mod.rs` — `text_of` guard flips.
- `crates/dashc/src/emit.rs` — `build_text_style` write + `text_style_key`.
- `crates/dashc/tests/text_lowering.rs` — lowering + emit-dedup tests.
- `crates/dashbuf/schema/dashbuf.fbs` — schema fields + enums.
- `crates/dashbuf/tests/text_roundtrip.rs` — raw round-trip.
- `crates/dashbuf/tests/schema_evolution.rs` — compile fix only (no regen).
- `crates/dashscene-core/src/arena.rs` — core `TextStyle` + enums.
- `crates/dashscene-core/src/load.rs` — read the four fields.
- `crates/dashscene-core/tests/load.rs` — load round-trip.
- `crates/dashscene-typeset/src/text/mod.rs` — `TextShape`/`TextAlign`, `layout_with`.
- `crates/dashscene-typeset/src/text/layout.rs` — letter-spacing threading.
- `crates/dashscene-typeset/tests/typeset_shape.rs` — typeset knob tests.
- `goldens/tooling/src/lib.rs` — vertical-align stager offset.
- `docs/specification/06-dashc-figma-lowering.md`, `docs/decisions/figma-text-lowering.md`.

---

## Task 1 — dashc: lower the four text style axes

**Scope:** `dashc`. Flip the four `text_of` refusal guards to populate the IR
`TextStyle`. Tests inspect the lowered `Document` directly (pre-emit), so this
task needs no schema change.

**Files:**

- Modify: `crates/dashc/src/document.rs` (add enums + 4 fields to `TextStyle`)
- Modify: `crates/dashc/src/figma/mod.rs` (`text_of`, ~lines 1201-1296)
- Test: `crates/dashc/tests/text_lowering.rs`

- [ ] **Step 1: Write the failing tests.** In `text_lowering.rs`, remove the
      `align`, `valign`, `line-height`, `letter-spacing` cases from
      `out_of_vocabulary_text_features_are_named_diagnostics` (they now lower), and
      add:

```rust
#[test]
fn the_four_style_axes_lower_into_the_text_style() {
    use dashc_wasm::document::{TextAlign, TextAlignV};
    let mut style = base_style();
    let m = style.as_object_mut().unwrap();
    m.insert("lineHeightUnit".into(), "PIXELS".into());
    m.insert("lineHeightPx".into(), 30.0.into());
    m.insert("letterSpacing".into(), 2.5.into());
    m.insert("textAlignHorizontal".into(), "CENTER".into());
    m.insert("textAlignVertical".into(), "BOTTOM".into());
    let mut text = text_json("t", "hi", 16.0, 400);
    text["style"] = style;
    let json = wrap_single(text);
    let (doc, diags) = lower(&serde_json::from_str(&json).unwrap(), Profile::Core, &BTreeMap::new()).unwrap();
    assert!(unsupported(&diags).is_empty(), "{:?}", unsupported(&diags));
    let ts = node(&doc, "t").1.text_style.as_ref().unwrap();
    assert_eq!(ts.line_height_px, Some(30.0));
    assert_eq!(ts.letter_spacing, 2.5);
    assert_eq!(ts.text_align, TextAlign::Center);
    assert_eq!(ts.text_align_v, TextAlignV::Bottom);
}

#[test]
fn left_top_auto_and_zero_lower_to_the_defaults() {
    use dashc_wasm::document::{TextAlign, TextAlignV};
    let (doc, _) = lower(&parse(HUG_IN_FILL), Profile::Core, &BTreeMap::new()).unwrap();
    let ts = node(&doc, "hug inside fill").1.text_style.as_ref().unwrap();
    assert_eq!(ts.line_height_px, None);
    assert_eq!(ts.letter_spacing, 0.0);
    assert_eq!(ts.text_align, TextAlign::Left);
    assert_eq!(ts.text_align_v, TextAlignV::Top);
}

#[test]
fn a_percent_line_height_and_justified_align_are_still_refused() {
    for (field, value, expected) in [
        ("lineHeightUnit", "FONT_SIZE_%", "a FONT_SIZE_% line height"),
        ("lineHeightUnit", "PERCENT", "a PERCENT line height"),
        ("textAlignHorizontal", "JUSTIFIED", "text alignment JUSTIFIED"),
    ] {
        let mut style = base_style();
        style.as_object_mut().unwrap().insert(field.into(), value.into());
        let mut text = text_json("t", "hi", 16.0, 400);
        text["style"] = style;
        let json = wrap_single(text);
        let (_, diags) = lower(&serde_json::from_str(&json).unwrap(), Profile::Core, &BTreeMap::new()).unwrap();
        assert_eq!(unsupported(&diags), vec![("/root/t".into(), expected.into())], "{value}");
    }
}
```

Also add `lineHeightPx` and `letterSpacing` reads to the Figma REST `TextStyle`
in `crates/dashc/src/figma/rest.rs` (`line_height_px: Option<f32>` — Figma's
`lineHeightPx`).

- [ ] **Step 2: Run — expect FAIL.** `cargo test -p dashc --test text_lowering`
      fails: `TextStyle` has no `line_height_px`/`text_align` fields; guards still refuse.

- [ ] **Step 3: Implement.** In `document.rs` add:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlign { #[default] Left, Center, Right }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextAlignV { #[default] Top, Center, Bottom }
```

and four fields on `TextStyle`: `line_height_px: Option<f32>`,
`letter_spacing: f32`, `text_align: TextAlign`, `text_align_v: TextAlignV`.
In `figma/mod.rs` `text_of`: replace the four `blockers.push(...)` guards with
reads (map LEFT/CENTER/RIGHT → `TextAlign`, JUSTIFIED → still push;
TOP/CENTER/BOTTOM → `TextAlignV`; `PIXELS` → `line_height_px = Some(lineHeightPx)`,
keep `None`/`INTRINSIC_%` → `None`, `FONT_SIZE_%`/`PERCENT` → still push;
`letterSpacing` → `letter_spacing`). Populate the four fields in the
`DocTextStyle { ... }` construction (~line 1290).

- [ ] **Step 4: Run — expect PASS.** `cargo test -p dashc --test text_lowering`.
      Then `cargo build -p dashc` (emit.rs ignores the new doc fields — still compiles).

- [ ] **Step 5: Commit.** `feat(dashc): lower Figma text line height, letter spacing, and alignment`

---

## Task 2 — dashbuf: carry the four fields in the schema, emit, and dedup key

**Scope:** `dashbuf`. Append the fields/enums to the schema, write them in
`emit.rs`, and include them in the pool dedup key. Uses precedent `300152b`
(a dashbuf-scoped commit spanning dashc).

**Files:**

- Modify: `crates/dashbuf/schema/dashbuf.fbs` (`TextStyle` table + two enums)
- Modify: `crates/dashc/src/emit.rs` (`build_text_style`, `text_style_key`)
- Modify: `crates/dashbuf/tests/text_roundtrip.rs`, `crates/dashbuf/tests/schema_evolution.rs`
- Test: `crates/dashc/tests/text_lowering.rs` (emit-dedup)

- [ ] **Step 1: Write the failing tests.**

```rust
// crates/dashbuf/tests/text_roundtrip.rs
#[test]
fn the_new_style_fields_round_trip() {
    let mut b = FlatBufferBuilder::new();
    let family = b.create_string("Inter");
    let style = TextStyle::create(&mut b, &TextStyleArgs {
        family: Some(family), size: 16.0, weight: 400,
        color: Some(&Color::new(0.1, 0.2, 0.3, 1.0)),
        line_height_px: Some(30.0), letter_spacing: 2.5,
        text_align: dashbuf::TextAlign::Center,
        text_align_v: dashbuf::TextAlignV::Bottom,
    });
    let styles = b.create_vector(&[style]);
    let doc = Document::create(&mut b, &DocumentArgs { text_styles: Some(styles), ..Default::default() });
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();
    let s = root_as_document(&bytes).unwrap().text_styles().unwrap().get(0);
    assert_eq!(s.line_height_px(), Some(30.0));
    assert_eq!(s.letter_spacing(), 2.5);
    assert_eq!(s.text_align(), dashbuf::TextAlign::Center);
    assert_eq!(s.text_align_v(), dashbuf::TextAlignV::Bottom);
}

#[test]
fn a_default_style_omits_the_new_fields() {
    let mut b = FlatBufferBuilder::new();
    let family = b.create_string("Inter");
    let style = TextStyle::create(&mut b, &TextStyleArgs { family: Some(family), ..Default::default() });
    let styles = b.create_vector(&[style]);
    let doc = Document::create(&mut b, &DocumentArgs { text_styles: Some(styles), ..Default::default() });
    b.finish(doc, None);
    let s = root_as_document(&b.finished_data().to_vec()).unwrap().text_styles().unwrap().get(0);
    assert_eq!(s.line_height_px(), None);
    assert_eq!(s.letter_spacing(), 0.0);
    assert_eq!(s.text_align(), dashbuf::TextAlign::Left);
    assert_eq!(s.text_align_v(), dashbuf::TextAlignV::Top);
}
```

```rust
// crates/dashc/tests/text_lowering.rs
#[test]
fn two_styles_differing_only_in_alignment_are_two_pool_entries() {
    let a = { let mut t = text_json("a", "OK", 16.0, 400); t["style"]["textAlignHorizontal"] = "CENTER".into(); t };
    let b = { let mut t = text_json("b", "OK", 16.0, 400); t["style"]["textAlignHorizontal"] = "LEFT".into(); t };
    let json = serde_json::json!({ "document": { "name":"D","type":"DOCUMENT","children":[{
        "name":"Page 1","type":"CANVAS","children":[{ "name":"row","type":"FRAME","layoutMode":"HORIZONTAL",
        "absoluteBoundingBox":{"x":0.0,"y":0.0,"width":200.0,"height":40.0}, "children":[a,b] }]}]}}).to_string();
    let (bytes, report) = compile_figma(&json, Profile::Core, &BTreeMap::new()).unwrap();
    assert!(report.is_empty(), "{report}");
    let doc = dashbuf::root_as_document(&bytes).unwrap();
    assert_eq!(doc.strings().unwrap().len(), 1, "one shared string");
    assert_eq!(doc.text_styles().unwrap().len(), 2, "distinct alignments must not collapse");
}
```

- [ ] **Step 2: Run — expect FAIL.** `cargo test -p dashbuf --test text_roundtrip`
      fails to compile (no `line_height_px` arg). `cargo test -p dashc --test text_lowering
  two_styles_differing_only_in_alignment` fails (collapses to one entry).

- [ ] **Step 3: Implement schema + emit.** In `dashbuf.fbs` add `enum TextAlign :
  uint8 { Left = 0, Center = 1, Right = 2 }` and `enum TextAlignV : uint8 { Top =
  0, Center = 1, Bottom = 2 }`, and append to `TextStyle`:
      `line_height_px: float32 = null; letter_spacing: float32 = 0; text_align:
  TextAlign = Left; text_align_v: TextAlignV = Top;`. In `emit.rs`
      `build_text_style` set the four args (mapping doc→dashbuf enums); extend
      `TextStyleKey` to `(String, u32, u16, [u32;4], Option<u32>, u32, u32, u32)` and
      `text_style_key` to include `line_height_px.map(f32::to_bits)`,
      `letter_spacing.to_bits()`, `text_align as u32`, `text_align_v as u32`. Add
      `..Default::default()` to the `TextStyleArgs` literals in
      `text_roundtrip.rs::build_doc` and `schema_evolution.rs::build_fixture` (compile
      only — do NOT regenerate the fixture).

- [ ] **Step 4: Run — expect PASS.** `cargo test -p dashbuf` (all suites, incl.
      frozen `schema_evolution` and `text_roundtrip`), then
      `cargo test -p dashc --test text_lowering` — including
      `the_text_fixtures_emit_their_golden_dsbs` (R7: byte-identical, no regen).

- [ ] **Step 5: Commit.** `feat(dashbuf): carry text line height, letter spacing, and alignment in the schema`

---

## Task 3 — dashscene-core: load the four fields into the arena

**Scope:** `dashscene-core`. Mirror the fields on core `TextStyle` and read them
in `load.rs`. Adding public fields forces every `Prop::TextStyle(TextStyle {…})`
construction site to compile — update all eleven with the default values
(behavior-preserving; no golden PNG changes).

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs` (enums + 4 fields, ~line 353)
- Modify: `crates/dashscene-core/src/load.rs` (read, ~line 124)
- Modify (compile): `crates/dashscene-core/tests/arena.rs:472`,
  `crates/dashscene-engine/tests/baseline.rs:33`,
  `crates/dashscene-engine/tests/measure.rs:28`,
  `goldens/tooling/tests/v05_text.rs` (×4), `goldens/tooling/tests/v06_arabic.rs`
  (×3), `goldens/tooling/tests/v07_fallback.rs` (×1)
- Test: `crates/dashscene-core/tests/load.rs`

- [ ] **Step 1: Write the failing test.** In `crates/dashscene-core/tests/load.rs`,
      build a `.dsb` via `dashc_wasm::compile_figma` (or a raw dashbuf builder) whose
      text style sets the four fields, load it, and assert the arena's `TextStyle`
      carries them:

```rust
#[test]
fn the_text_style_metrics_and_alignment_reach_the_arena() {
    use dashscene_core::{TextAlign, TextAlignV};
    // Build a one-node .dsb with the four fields set (raw dashbuf builder).
    let bytes = /* build TextStyle { line_height_px: Some(30.0), letter_spacing: 2.5,
                   text_align: Center, text_align_v: Bottom } on one text node */;
    let doc = dashbuf::root_as_document(&bytes).unwrap();
    let mut arena = Arena::new();
    load_document(&doc, &mut arena);
    let leaf = /* the text node */;
    let s = arena.text_style(leaf).unwrap();
    assert_eq!(s.line_height_px, Some(30.0));
    assert_eq!(s.letter_spacing, 2.5);
    assert_eq!(s.text_align, TextAlign::Center);
    assert_eq!(s.text_align_v, TextAlignV::Bottom);
}
```

- [ ] **Step 2: Run — expect FAIL.** `cargo test -p dashscene-core --test load`
      fails: core `TextStyle` has no such fields.

- [ ] **Step 3: Implement.** In `arena.rs` add core-local `TextAlign` (Left/Center/
      Right) and `TextAlignV` (Top/Center/Bottom) enums (re-export from the crate root
      next to `TextStyle`) and the four fields on `TextStyle`. In `load.rs` read
      `style.line_height_px()`, `style.letter_spacing()`, and map
      `style.text_align()`/`style.text_align_v()` into core enums. Append the four
      default fields (`line_height_px: None, letter_spacing: 0.0, text_align:
  TextAlign::Left, text_align_v: TextAlignV::Top`) to each of the eleven
      `TextStyle { … }` literals so they compile.

- [ ] **Step 4: Run — expect PASS.** `cargo test -p dashscene-core`, then
      `cargo build --workspace` (engine + goldens test crates compile).

- [ ] **Step 5: Commit.** `feat(dashscene-core): load text line height, letter spacing, and alignment`

---

## Task 4 — dashscene-typeset: honor the knobs via `layout_with`

**Scope:** `dashscene-typeset`. Additive `TextShape` + `layout_with`; `layout`
keeps its signature and delegates with the default (E7 guard).

**Files:**

- Modify: `crates/dashscene-typeset/src/text/mod.rs`
- Modify: `crates/dashscene-typeset/src/text/layout.rs` (thread `letter_spacing`)
- Test: `crates/dashscene-typeset/tests/typeset_shape.rs` (new)

- [ ] **Step 1: Write the failing tests.** New file `typeset_shape.rs`:

```rust
use dashscene_typeset::text::{TextAlign, TextShape};
mod common;
use common::FONT;
fn ts() -> dashscene_typeset::text::Typesetter { common::typesetter(FONT) }

#[test]
fn layout_equals_layout_with_default() {
    let mut a = ts(); let mut b = ts();
    let l0 = a.layout("Hello world", 24.0, Some(120.0));
    let l1 = b.layout_with("Hello world", 24.0, Some(120.0), TextShape::default());
    assert_eq!(l0, l1);
}

#[test]
fn line_height_px_overrides_the_line_advance() {
    let mut t = ts();
    let l = t.layout_with("a\nb", 24.0, None, TextShape { line_height_px: Some(50.0), ..Default::default() });
    assert!((l.height - 100.0).abs() < 1e-3, "height {}", l.height);
    assert!((l.lines[1].baseline_y - l.lines[0].baseline_y - 50.0).abs() < 1e-3);
}

#[test]
fn letter_spacing_widens_the_measured_line() {
    let mut t = ts();
    let base = t.layout("abc", 24.0, None).lines[0].width;
    let wide = t.layout_with("abc", 24.0, None, TextShape { letter_spacing: 4.0, ..Default::default() }).lines[0].width;
    assert!((wide - base - 12.0).abs() < 1e-3, "base {base} wide {wide}"); // 3 glyphs × 4
}

#[test]
fn center_and_right_shift_the_line_within_the_container() {
    let mut t = ts();
    let w = t.layout("abc", 24.0, None).lines[0].width + 100.0;
    let c = t.layout_with("abc", 24.0, Some(w), TextShape { align: TextAlign::Center, ..Default::default() });
    let r = t.layout_with("abc", 24.0, Some(w), TextShape { align: TextAlign::Right, ..Default::default() });
    assert!((c.lines[0].glyphs[0].x - (w - c.lines[0].width) / 2.0).abs() < 1e-3);
    assert!((r.lines[0].glyphs[0].x - (w - r.lines[0].width)).abs() < 1e-3);
}
```

- [ ] **Step 2: Run — expect FAIL.** `cargo test -p dashscene-typeset --test
  typeset_shape` fails: no `TextShape`/`layout_with`.

- [ ] **Step 3: Implement.** In `mod.rs` add `TextAlign` (Left/Center/Right,
      `#[default] Left`) and `TextShape { line_height_px: Option<f32>, letter_spacing:
  f32, align: TextAlign }` with a `Default` of `{ None, 0.0, Left }`. Add
      `layout_with(&mut self, text, size, max_width, shape)`; make `layout` call it
      with `TextShape::default()`. In `layout_with`: pass `shape.letter_spacing` into
      `break_lines` and `position_line`; use `shape.line_height_px.unwrap_or(advance)`
      as the per-line advance (baseline stays `pen_y + ascent`); replace the RTL-only
      shift with a per-line shift by `shape.align` (Left → current LTR-0/RTL-flush,
      Center → `(container - width)/2`, Right → `container - width`). In `layout.rs`
      add a `letter_spacing: f32` param to `break_lines`, `position_line`, and `place`
      (add it to each glyph's advance); update the two `#[cfg(test)]` call sites in
      `layout.rs` to pass `0.0`.

- [ ] **Step 4: Run — expect PASS.** `cargo test -p dashscene-typeset` (all
      suites — `layout_equals_layout_with_default` proves E7 byte-for-byte behavior).

- [ ] **Step 5: Commit.** `feat(dashscene-typeset): add TextShape and layout_with for line height, letter spacing, and alignment`

---

## Task 5 — goldens: the stager offsets a text block by vertical alignment

**Scope:** `goldens`. Vertical alignment is a placement of the whole block within
the node box (P2), resolved where the stager adds the box origin.

**Files:**

- Modify: `goldens/tooling/src/lib.rs`

- [ ] **Step 1: Write the failing test.** In `goldens/tooling/src/lib.rs`
      `#[cfg(test)] mod tests`:

```rust
#[test]
fn vertical_offset_places_the_block_within_the_box() {
    use super::{VerticalAlign, vertical_offset};
    assert_eq!(vertical_offset(100.0, 40.0, VerticalAlign::Top), 0.0);
    assert_eq!(vertical_offset(100.0, 40.0, VerticalAlign::Center), 30.0);
    assert_eq!(vertical_offset(100.0, 40.0, VerticalAlign::Bottom), 60.0);
}
```

- [ ] **Step 2: Run — expect FAIL.** `cargo test -p goldens-tooling
  vertical_offset` fails: no `vertical_offset`.

- [ ] **Step 3: Implement.** Add `pub enum VerticalAlign { Top, Center, Bottom }`
      and `pub fn vertical_offset(box_height: f32, content_height: f32, align:
  VerticalAlign) -> f32` returning `0.0` / `slack/2.0` / `slack` where
      `slack = box_height - content_height`.

- [ ] **Step 4: Run — expect PASS.** `cargo test -p goldens-tooling`.

- [ ] **Step 5: Commit.** `feat(goldens): resolve text vertical alignment in the stager`

---

## Task 6 — docs: record the text vocabulary extension

**Scope:** `docs`.

**Files:**

- Modify: `docs/specification/06-dashc-figma-lowering.md` (add a text-lowering
  section describing the four now-lowered axes + remaining refusals, referencing
  `crates/dashc/tests/text_lowering.rs`)
- Modify: `docs/decisions/figma-text-lowering.md` (add a "Revised at #310"
  section: the four-axis widening — PIXELS line height only, letter spacing,
  LEFT/CENTER/RIGHT, TOP/CENTER/BOTTOM; the remaining refusals; update the
  right-alignment cost note; update Verified-by)

- [ ] **Step 1: Edit both records.** Keep the as-built framing; the decision
      record documents the transition its own D1 anticipated ("when the runtime
      gains alignment or line-height, the schema widens with it").
- [ ] **Step 2: Verify.** `just lint` (markdownlint + dprint) passes; no MarkSpec
      entry blocks are involved (these are prose records).
- [ ] **Step 3: Commit.** `docs(docs): record the Figma text vocabulary extension`

---

## Final verification

- [ ] `just build` green (workspace assemble + full check). If frozen dashbuf
      roundtrip/schema-evolution fails, R7 is broken — fix append-only/defaults.
- [ ] Rebuild wasm: `just wasm`.
- [ ] First-light probe (importer default = Partial):
      `FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w)` then
      `cd importers/figma && deno task import MRk9I5cYY6yJa8JhljzkBn --root 2411:10795
  -o .probe.dsb` — confirm the four text `figma.unsupported` warnings (PIXELS line
      height, letter spacing, text align CENTER, vertical align CENTER) are GONE.
      Clean up `.probe.dsb`/`.probe.vars.json`; never echo the token (length/status
      only). If token missing / API 401/403, report status and move on.
