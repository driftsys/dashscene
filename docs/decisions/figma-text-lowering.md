# The Figma text lowering carries four style axes and diagnoses the rest

    status   accepted (story #160, 2026-07-16)
    scope    crates/dashc (the figma module and the document model)
    binds    the v0.8+ text widening, and every consumer of the .dsb
             string/text-style pools

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
    TRUNCATE           ↔  (no pair equivalent)       diagnosed

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

- Satisfies: issue #160 (text lowering), P1 (authored intent only — the
  document carries the codepoints and style, never the shaped lines, glyph
  positions, or `absoluteRenderBounds`), P4 (out-of-vocabulary features are
  named diagnostics), P5 (Figma compatibility is one producer's property).
- Verified by: `crates/dashc/tests/text_lowering.rs` (characters + style
  lowering, the Arabic RTL run's authored codepoints, round-trip through
  `dashscene-core`, pool dedup, the out-of-vocabulary diagnostics, the golden
  `.dsb`s), `goldens/tooling/tests/v07_text_lowering.rs` (the lowered scene
  solves through the measure callback and paints).
- Related: `docs/decisions/figma-flex-lowering.md` (the shared walk
  conventions and per-axis sizing), `docs/decisions/rtl-text-width-is-the-placed-extent.md`
  (the width-vs-bounds decision #160 settled, #224),
  `docs/design/typeset-latin.md` (the runtime this feeds through #29),
  `docs/design/dashbuf.md` (the string/text-style pools).
