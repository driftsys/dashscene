# S1 — Figma TEXT vocabulary: line-height, letter-spacing, alignment

    status   design (working memory)
    story    S1 / #310 of the "full real-file import" epic
             (docs/wip/... epic; ledger .superpowers/sdd/epic-progress.md)
    scope    dashbuf schema, dashc (emit + figma lowering), dashscene-core,
             dashscene-typeset, goldens/tooling stager
    base     main 27d8d80 (post-#320 E7 baseline work)

## Why

Under partial-emit (S0-impl, merged), the first-light target emits but its TEXT
nodes are _skipped with warnings_ — they carry constructs the document cannot
express yet. The live probe shows exactly four:

- a `PIXELS` line height
- letter spacing (`letterSpacing` != 0)
- `textAlignHorizontal` != LEFT (CENTER/RIGHT)
- `textAlignVertical` != TOP (CENTER/BOTTOM)

S1 lowers these four so first-light's text renders instead of being omitted. It
raises first-light fidelity from "layout skeleton with text holes" toward "text
present."

## Scope

**In:** the four blockers above, as four new `TextStyle` fields.

**Out:** mixed-style segments (`styleOverrideTable` → per-run segments). Not hit
by first-light, and structurally larger — it breaks the one-style-per-text-node
invariant baked into `GlyphRun`/`TextContext`. It stays in #310 as a separate
follow-up story. `JUSTIFIED` horizontal alignment also stays refused (only
LEFT/CENTER/RIGHT lower).

## The current text path (grounded, main 27d8d80 — re-verify engine cites)

- `dashc` lowers TEXT in `crates/dashc/src/figma/mod.rs` `text_of`: it builds a
  `TextStyle` from four fields only — `family`, `size`, `weight`, `color`. The
  four blockers are refusal guards inside `text_of` (line-height, letter-spacing,
  h-align, v-align), each pushing a `figma.unsupported` and skipping the node.
- The IR `TextStyle` (`crates/dashc/src/document.rs`, `dashscene-core/src/arena.rs`,
  flatbuffer `crates/dashbuf/schema/dashbuf.fbs`) carries only those four fields.
  **There is no line-height, letter-spacing, or alignment field anywhere.**
- Runtime: `.dsb` → core load → engine measure seam builds a `TextContext` and
  calls `Typesetter::layout(text, size, max_width)` → typeset produces positioned
  glyphs → a stager adds the node box origin and emits `GlyphRun`s → the Skia
  painter only draws the quads (P2). NB: #320 (just merged) reworked the engine
  measure/baseline path — the implementer must re-verify the `dashscene-engine`
  cites against the current tree.

## Design

### 1. Schema — four new `TextStyle` fields (append-only, R7-safe)

Append at the tail of the `TextStyle` flatbuffer table, with defaults that
reproduce today's behavior so existing documents emit byte-identically:

- `line_height_px: float32 = null` — null ⇒ auto/intrinsic (font metrics).
- `letter_spacing: float32 = 0`.
- `text_align: TextAlign = LEFT` (enum LEFT/CENTER/RIGHT).
- `text_align_v: TextAlignV = TOP` (enum TOP/CENTER/BOTTOM).

Mirror the fields in `dashc` `document.rs` `TextStyle` and `dashscene-core`
`arena.rs` `TextStyle`; read them in `dashscene-core` `load.rs`.

### 2. Emit — dedup key MUST include the new fields

`dashc/emit.rs`: `build_text_style` writes the four fields; **`text_style_key`
(the pool dedup key) must include all four** — otherwise two text styles that
differ only in, say, alignment would collapse to one pool entry (a correctness
bug). Defaults omitted so a plain style still dedups and emits identically.

### 3. Lowering — flip the four guards from refuse to populate

`dashc/figma/mod.rs` `text_of`: replace each of the four refusal guards with a
read that populates the corresponding `TextStyle` field:

- line height: `PIXELS` ⇒ `line_height_px = value`; keep `INTRINSIC_%`/absent ⇒
  null (auto). A percentage line height stays refused for now (no field for it)
  OR is converted to px if `font_size` is known — **decide in the plan**; the
  minimal, safe choice is to lower only `PIXELS` and keep `%` refused.
- letter spacing: populate `letter_spacing`.
- h-align: LEFT/CENTER/RIGHT ⇒ `text_align`; JUSTIFIED stays refused.
- v-align: TOP/CENTER/BOTTOM ⇒ `text_align_v`.

### 4. Typeset — additive API, layout() signature UNCHANGED (E7 guard)

`dashscene-typeset`: add an options struct carried alongside the existing call —
e.g. `TextShape { line_height_px: Option<f32>, letter_spacing: f32, align: TextAlign }`
with `TextShape::default()` reproducing current behavior, and a
`layout_with(text, size, max_width, shape)` that `layout(text, size, max_width)`
delegates to with the default. **Do not change `layout()`'s signature** — every
E7 oracle/golden call site uses it and must compile + render identically.

- line height: override the per-line advance (`line_box`/`pen_y`) with
  `line_height_px` when set, re-deriving baseline placement.
- letter spacing: add tracking to both the measure width (`layout.rs`) and the
  placement pen advance.
- horizontal align: generalize the existing flush-left/RTL shift to CENTER/RIGHT
  over the container width (this is the P2 home for h-align — inside the
  typesetter).

### 5. Stager — vertical alignment

Vertical alignment is a placement of the whole text block within the node box:
offset `= (box_height - content_height) * factor`. It is applied at the stager
(where the box origin is added). Today the only stager is in `goldens/tooling`
(the oracle/golden text path). Add the v-align offset there, reading the node's
resolved box height (already available to the stager). Additive — nodes without
v-align (all existing fixtures) are unchanged.

## Guardrails

- **R7 (byte-reproducibility):** append fields with behavior-preserving defaults;
  a document using none of them emits byte-identically. **Do not regenerate the
  frozen `crates/dashbuf/tests/fixtures/v0_5_document.dsb`** — it must decode
  unchanged (new fields read back absent).
- **E7 (v0.9 exit gate — untouched):** keep `Typesetter::layout(...)` signature;
  add knobs via the additive `TextShape`. Existing oracle fixtures
  (`msdf-text` latin/arabic, etc.) use auto line-height / zero spacing / top-left,
  so their rendered output is unchanged and **no E7 band is retuned**. Do not
  edit the E7 sections of `05-qualification.md`.
- **P2:** the painter never changes; horizontal align lives in the typesetter,
  vertical align in the stager. **P1:** the document carries the align _intent_
  (an enum), never a resolved offset.

## Alternatives considered

- **Split the 4 blockers into parallel stories.** Rejected: all four touch the
  same schema/emit/core/load plumbing (`dashbuf.fbs`, `emit.rs` incl. the dedup
  key, `document.rs`, `arena.rs`, `load.rs`), so parallel branches collide on
  every one. Split by _commit_ on one branch instead (plumbing; typeset metrics;
  align).
- **Include mixed-style segments.** Rejected: not on the first-light frontier and
  a larger structural change (per-run segments) — separate follow-up.
- **Convert `%` line height to px at lowering.** Deferred: the minimal safe scope
  lowers only `PIXELS`; `%` stays a named refusal until a target needs it.

## Test strategy (TDD — detail in the plan)

- Lowering: each of the four guards, a synthetic TEXT node now lowers into the
  correct `TextStyle` field (was refused); JUSTIFIED / `%` line height still
  refused.
- Emit: two styles differing only in alignment produce two distinct pool
  entries (the dedup-key regression).
- Round-trip: a document with the four fields set survives `.dsb` emit→load; a
  document using none is byte-identical to today (R7).
- Typeset: `line_height_px` changes line advance; `letter_spacing` widens the
  measured line; CENTER/RIGHT shift the line within the container.
- Empirical: rebuild wasm, re-probe first-light — the text `figma.unsupported`
  warnings for the four constructs are gone; the text nodes now lower.
