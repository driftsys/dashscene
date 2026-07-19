# The Figma text lowering carries four style axes and diagnoses the rest

    status   accepted (story #160, 2026-07-16); revised (story #310,
             2026-07-18; story #341, 2026-07-19)
    scope    crates/dashc (the figma module and the document model), the
             dashbuf schema, dashscene-core (load), and dashscene-typeset
    binds    every consumer of the .dsb string/text-style pools

## Revised at #341 (2026-07-19): the measured OpenType flag, `LIGA: 0`, now lowers

Story #341 measured what the "OpenType feature flags" refusal (D1 below) was
actually blocking on a real imported file: the hero
(`S30AJmYfnDKGeSQmzuXEUk` root `1973:6580`) carries 58 TEXT nodes, and every
one of them sets exactly one OpenType flag, `{"LIGA": 0}` (standard ligatures
explicitly off) — no stylistic sets, no fractions, no smallcaps, nothing else.
That is narrow enough to widen the vocabulary by exactly the measured shape,
the same discipline #310 followed per axis: **`{LIGA: 0}` lowers into a new
`ligatures_off: bool` on `TextStyle`; every other OpenType flag, every other
value on `LIGA`, and `LIGA: 0` alongside any other flag still has no
vocabulary and stays the named `figma.unsupported: OpenType features`
diagnostic** (`crates/dashc/src/figma/mod.rs` `text_of`). `ligatures_off` is
appended at the `TextStyle` table tail with a behavior-preserving default
(`false`), so a style using it emits byte-identically to before this story
(R7).

Where it lowers to: `dashscene-typeset`'s `Typesetter::layout_with` already
takes a per-call `TextShape` (the #310 seam); `ligatures_off` is a fourth
knob on it, threaded down to `shape::shape_with_face`'s feature-list
selection.
There it forces `liga`/`clig` off regardless of the run's resolved
`RunContext` — overriding an Arabic-context run's otherwise full default
feature set (`docs/decisions/liga-clig-off-until-gsub-closure.md`) — while
leaving every other default feature (digit shapes, joining, `rlig`/`ccmp`,
kerning) exactly as `RunContext` already decides. A non-Arabic run already
shapes with `liga`/`clig` off by default, so the flag is a no-op there; the
override only does real work on an Arabic-context run, which is why the unit
test proving it (`crates/dashscene-typeset/src/text/shape.rs`) forces
`RunContext::Arabic` on an English three-letter-ligature word ("office",
"waffle") rather than relying on a Latin run to show a difference. Because
the same paragraph text can now shape two different ways depending on this
per-call knob, the `Typesetter`'s shaped-run cache — previously keyed by
paragraph text alone — splits into two maps, one per `ligatures_off` posture;
the `false` posture's map and its lookup/insert code path are unchanged from
before this story.

The goldens render stager's `text_shape()` (`goldens/tooling/src/render.rs`)
threads the node's `TextStyle.ligatures_off` into the `TextShape` it builds,
the same way it already threads line height, letter spacing, and alignment
(#327). The `layout()`/default-axis path — `TextShape::default()`,
`ligatures_off: false` — stays byte-identical to pre-#341 output; every E7
oracle and golden call site is unaffected.

**Not yet done by this story:** the liga-text oracle frame and the hero
re-measure (confirming the 31 skipped text blocks now lower and the hero's
solve height moves toward Figma's own render) wait on a captured Figma
fixture exercising `LIGA: 0`; they are a follow-up once that fixture lands.

## Revised at #310 (2026-07-18): four more axes now lower

Story #310 does exactly what D1 below anticipated ("when the runtime gains
alignment or line-height, the schema widens with it, and the diagnostic becomes
a lowering"). The `TextStyle` vocabulary — schema (`dashbuf.fbs`), both IR
mirrors (`dashc` `document.rs`, `dashscene-core` `arena.rs`), and the loader —
widens by four axes, each appended at the `TextStyle` table tail with a
behavior-preserving default so a style using none of them emits byte-identically
(R7): the frozen `v0_5_document.dsb` and the text golden `.dsb`s are unchanged,
no regeneration.

- `line_height_px: Option<f32>` — a fixed line height. **Only Figma's `PIXELS`
  unit lowers.** `INTRINSIC_%` (auto) lowers as `None` as before; a percentage
  line height (`FONT_SIZE_%`, `PERCENT`) stays a named diagnostic — the runtime
  has no percentage model yet, and the minimal safe scope is a fixed pixel value.
  The typesetter places each line's baseline by Figma's model: the line's
  intrinsic box is centered within the fixed line box (half-leading), so the
  baseline sits at `ascent + (line_height - intrinsic) / 2`. The #310
  implementation first kept the baseline at the full intrinsic ascent; the
  #332 import oracle measured the run 7 px below Figma's own render of the
  same frame (24 px type, 18 px line height) and the half-leading placement
  was pinned against that capture.
- `letter_spacing: f32` — tracking; zero is the default.
- `text_align: TextAlign` (`Left`/`Center`/`Right`) — `LEFT` is the default;
  `JUSTIFIED` stays a named diagnostic (no vocabulary).
- `text_align_v: TextAlignV` (`Top`/`Center`/`Bottom`) — `TOP` is the default.

**Where each axis will live, and the boundary each respects.** The typesetter
gains an additive `TextShape { line_height_px, letter_spacing, align }` and a
`layout_with(text, size, max_width, shape)`; `layout(text, size, max_width)`
keeps its signature and delegates with `TextShape::default()`, which reproduces
the previous output exactly — the E7 oracle and every golden call `layout` and
render unchanged. Horizontal alignment belongs in the typesetter (over the
container width); vertical alignment is a stager placement — an offset
`(box_height − content_height) × factor` applied where the box origin is added
(`goldens/tooling`, `vertical_offset`) — because the block's box height is a
solver result the typesetter does not hold. The painter never changes (P2), and
the document carries the alignment _intent_ as an enum, never a resolved offset
(P1).

**Lowered and capable, not yet wired to render (as-built).** This story adds the
four axes to the schema and the lowering, and gives the typesetter and the
stager the _capability_ to honor them — but it does not yet connect that
capability to the render path. The engine measure seam (`TextContext`,
`crates/dashscene-engine/src/lib.rs`) still carries only text and size and calls
`layout()`, and `vertical_offset` has no production caller. So a lowered
document's line-height, letter-spacing, and alignment persist to the `.dsb` and
load into the arena, but do not yet affect a rendered result. This is
deliberate: keeping `layout()` byte-identical leaves the v0.9 E7 oracle
untouched until a non-default text render fixture verifies the wiring. The
end-to-end render wiring is tracked as a follow-up (#327).

**Still refused (updated by #341):** a percentage line height, `JUSTIFIED`
alignment, multiple style segments (`styleOverrideTable`), italic, decoration,
a case transform, truncation, a hyperlink, any OpenType flag other than
exactly `{LIGA: 0}`, and a text outline.

The emit pool dedup key (`text_style_key`) now covers all nine axes, so two
styles differing only in, for example, alignment or `ligatures_off` stay two
distinct pool entries rather than collapsing to one (which would render one
node with the wrong style).

The `binds` line above is updated: this record now binds the dashbuf schema,
`dashscene-core`, and `dashscene-typeset`, not just `dashc`. The rest of this
record is the original #160 decision, kept for its rationale; the paragraphs
below on the right-alignment refusal and the "not widened speculatively" stance
describe the pre-#310 posture and are superseded for the four axes above.

## Context

Story #160 lowers a Figma `TEXT` node into the `.dsb` document. The schema
already carries the text vocabulary (story #26): a `strings` pool, a
`text_styles` pool (`TextStyle { family, size, weight, color }`), and
`Node.text`/`Node.text_style` indices. `dashscene-core::load_document` reads
all four `TextStyle` fields into the arena (a 1:1 mirror), and the measure
callback (#29) reads the arena's `text`/`text_style` to drive hug sizing. No
schema change was needed — the producer side was simply never filled from
Figma.

The walk follows the conventions story #140 set
(`docs/decisions/figma-flex-lowering.md`): a `TEXT` node runs the same shared
property refusals (hidden, opacity, rotation, mask, absolute positioning) and
the same per-axis sizing (`layoutSizingHorizontal`/`layoutSizingVertical`,
D1) as a frame; only the type-specific lowering differs.

## D1 — The vocabulary is four axes; every other feature is a diagnostic

The document's `TextStyle` carries `family`, em `size`, CSS-scale `weight`,
and the fill `color`. Those are what the runtime consumes: the typesetter
shapes from family and size, the painter fills with the color, and the weight
selects the font's face. Every other authored text feature — a non-default
horizontal or vertical alignment, a fixed line height, letter spacing, a text
decoration, a case transform, italic, a hyperlink, an OpenType feature flag,
or multiple style segments (`styleOverrideTable`) — has **nothing to lower
into**. It is a named diagnostic (P4), never lowered approximately: lowering
centered text flush-left, or dropping a letter-spacing, would paint a picture
the designer never authored, and a silent drop is exactly what P4 forbids.

The schema is **not** widened to carry these speculatively. A field the
runtime does not consume would still render the wrong picture (centered text
would paint flush-left regardless), so carrying it would trade a loud refusal
for a silent visual drop — the worse outcome under P4. When the runtime gains
alignment or line-height, the schema widens with it, and the diagnostic
becomes a lowering; until then the refusal is correct. The default values
(`LEFT`/`TOP` alignment, `INTRINSIC_%` — Figma's "Auto" — line height, zero
letter spacing, upright, no decoration) lower cleanly, so every captured
fixture lowers with no text diagnostic.

### The right-alignment refusal has a real cost

`textAlignHorizontal: RIGHT` is refused **even for RTL text, where it renders
the same as the flush-right the runtime already produces by direction**. This
follows from P1: the producer must not resolve bidi. Whether `RIGHT` is
redundant with the runtime's placement depends on the text's resolved base
direction — an RTL paragraph aligned `RIGHT` looks identical to the default,
but an LTR paragraph aligned `RIGHT` does not — and that base direction is a
runtime UAX #9 result (`docs/design/typeset-latin.md`), not something the
producer may compute. Lowering `RIGHT` as "no alignment" would be correct only
when the text happens to be RTL; the producer cannot know that without running
bidi, so it refuses rather than guess.

The cost is concrete and worth stating: a designer who sets an Arabic label to
`RIGHT` — an ordinary, often-default choice for RTL text in Figma — has that
label refuse to import. The workaround until the schema carries alignment is to
author the RTL text with the default `LEFT` alignment and let the runtime flush
it right by direction (which the Arabic golden path exercises). This is
friction the alignment widening (a later slice, when the runtime consumes
alignment) removes; the refusal is the honest interim, not a silent
mis-render.

## D2 — The `textAutoResize` mapping

A text node's sizing is read from the modern per-axis
`layoutSizingHorizontal`/`layoutSizingVertical` pair (D1 of the flex
lowering), exactly as a frame's is. `textAutoResize` is Figma's text-specific
mirror of that pair; each of its states corresponds to a sizing the pair
already expresses, except one:

    textAutoResize     ↔  layoutSizing (h, v)        lowering
    ----------------------------------------------------------------
    WIDTH_AND_HEIGHT   ↔  (HUG, HUG)                 text drives the box
    HEIGHT             ↔  (FIXED|FILL, HUG)          fixed width, text wraps,
                                                     height grows
    NONE               ↔  (FIXED, FIXED)             fixed box
    absent             ↔  (FIXED, FIXED)             same as NONE — the REST
                                                     API omits the default
    TRUNCATE           ↔  (no pair equivalent)       diagnosed

An **absent** `textAutoResize` is `NONE`: `NONE` is the REST default and the
API omits it, so a fixed-box label serializes with no field at all, while an
auto-sizing node always carries the field explicitly (every committed capture
does, and a live probe of a fixed-box label confirms the omission). The
lowering first mapped absent to `WIDTH_AND_HEIGHT` — the Figma editor's
authoring default for a new text object, which is not the API's serialization
default — and the #332 import oracle caught the consequence against Figma's
own render: a fixed free-standing label collapsed to its content and its
alignment had no room to act. Revised here with #332.

A `HUG` axis flows through the engine's measure seam (#29), which sizes it to
the shaped text's own extent (`docs/decisions/rtl-text-width-is-the-placed-extent.md`,
which #160 also settled). `TRUNCATE` is the one `textAutoResize` value the
sizing pair cannot express — an ellipsis has no vocabulary — so it is
diagnosed by name.

## D3 — A text node's fill lowers into the style's color

A Figma `TEXT` node's `fills` array is its glyph color, not a rectangle fill.
The single visible SOLID fill lowers into `TextStyle.color`, and the node
carries no paint entry (`Node.paint_entry` stays the "draws nothing"
sentinel). The same stacking and non-solid refusals `fill_of` applies to a
frame apply here: more than one visible fill, or a gradient/image text fill,
has no lowering into one color, so it is refused rather than painted an
invented one. A text node with no fill is refused too — the color is required
(the load gate's `text.style-no-color`, P4), so the lowering never emits a
color-less style.

## D4 — Strings and styles pool in first-use DFS order

The emitter interns strings and text styles into their pools the same way it
interns the paint pool: first-use DFS order, keyed by value (the `size` and
`color` `f32`s by bit pattern), so two text nodes sharing a string or a style
share one entry and the bytes are reproducible (R7). A text-free document
emits byte-identically to before this vocabulary was filled — the sentinels
are the schema defaults, so they are omitted from the buffer — which is why
the frozen fixture and the Deno byte-identity goldens hold unchanged.

## Known validator gap (flagged, not fixed here)

The weight lowers verbatim from Figma's `fontWeight`; the 100–900 range check
is the validator's, folded into #41 (via #129), which this story does not
implement. A Figma weight outside 100–900 would currently pass the load gate
unvalidated. No captured fixture carries one (weights are 400/700), so nothing
emits an out-of-range weight today; the gap is flagged for #41.

## Trace

- Satisfies: issue #160 (text lowering), issue #310 (the four-axis widening —
  line height, letter spacing, horizontal and vertical alignment), and issue
  #341 (the measured `LIGA: 0` → `ligatures_off` widening); P1 (authored intent
  only — the document carries the codepoints, style, and alignment enums,
  never the shaped lines, glyph positions, resolved offsets, or
  `absoluteRenderBounds`), P2 (horizontal align in the typesetter, vertical
  align in the stager, the painter unchanged), P4 (out-of-vocabulary features,
  including every OpenType flag but the one measured, are named diagnostics),
  P5 (Figma compatibility is one producer's property).
- Verified by: `crates/dashc/tests/text_lowering.rs` (characters + style
  lowering, the four widened axes and their still-refused neighbors, the
  narrowed `LIGA: 0` lowering and its still-refused OpenType neighbors, the
  Arabic RTL run's authored codepoints, round-trip through `dashscene-core`,
  pool dedup including the alignment- and ligatures-off-distinct-entries
  guards, the out-of-vocabulary diagnostics, the golden `.dsb`s),
  `crates/dashbuf/tests/text_roundtrip.rs` (the new fields round-trip and a
  default style reads back the defaults), `crates/dashscene-core/tests/load.rs`
  (the axes reach the arena), `crates/dashscene-typeset/src/text/shape.rs`
  (`ligatures_off` overrides an Arabic-context run's default-on ligatures),
  `crates/dashscene-typeset/tests/typeset_shape.rs` (`layout_with` honors each
  knob and `layout` is byte-identical to the default), `goldens/tooling/tests/
  v07_text_lowering.rs` (the lowered scene solves through the measure callback
  and paints).
- Related: `docs/decisions/figma-flex-lowering.md` (the shared walk
  conventions and per-axis sizing), `docs/decisions/rtl-text-width-is-the-placed-extent.md`
  (the width-vs-bounds decision #160 settled, #224),
  `docs/design/typeset-latin.md` (the runtime this feeds through #29),
  `docs/design/dashbuf.md` (the string/text-style pools),
  `docs/decisions/liga-clig-off-until-gsub-closure.md` (the per-run
  `RunContext` posture #341's `ligatures_off` overrides).
