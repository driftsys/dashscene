# RTL text width is the placed extent, not a separate bounds field

    status   accepted (story #160, 2026-07-16); closes #224
    scope    dashscene-typeset (the width contract), dashscene-engine
             (the measure seam #29), dashc (the first importer consumer);
             binds every consumer that sizes a text box from
             TextLayout::width

## Context

`TextLayout::width` is the widest line's pen advance
(`docs/design/typeset-latin.md`, Line metrics). For a fixed-width RTL line,
glyphs are placed flush-right in `(max_width − line, max_width]`, so their
positions can reach up to `max_width`, past `width` (the same record, Bidi).
Debt #224 named the risk: a consumer that treats `width` as the content bounding
box would clip or mis-bound RTL text.

Story #160 is the first importer consumer that sizes real text boxes from
`width` — a Figma `TEXT` node with HUG sizing flows through the engine's measure
seam (#29), which reads `TextLayout::width`. So #160 settles the decision point
#224 left open: does `width` report the placed extent, or is a separate bounds
field added?

Two consumers read the layout:

- The **measure seam** (#29, `dashscene-engine`'s `measure_text`) sizes a hug
  axis to `TextLayout::width` and returns a fixed axis's known width unchanged
  (`known.width.unwrap_or(laid.width)`). It only reads `width` for a hug axis,
  and it passes `max_width = None` when probing a hug's intrinsic size.
- The **painter** (#30) draws glyphs from their per-glyph absolute positions
  (`GlyphQuad { x, y }`, `docs/decisions/glyph-runs-cross-boundary-b.md`). It
  never reads `TextLayout::width` at all.

## Options

1. `width` reports the placed extent — the widest line's pen advance, unchanged.
   A fixed-width box is bounded by its authored width (the `max_width` the
   consumer passed), within which RTL glyphs sit flush-right.
2. Add a second field (`bounds`, or a placed-extent rectangle) so a consumer can
   distinguish "content advance" from "rightmost glyph position".

## Choice

Option 1. `TextLayout::width` stays the widest line's pen advance — the content
advance. No field is added, and no `dashscene-typeset` or `dashscene-engine`
code changes: the seam already does the right thing. The decision pins the
contract and its two obligations:

- **The hug datum is the placed advance extent.** Hug sizing — the only sizing
  that reads `width` — always passes `max_width = None`. In that mode the
  flush-right shift is computed against the widest line, so every glyph's **pen
  position** (its advance box) lies within `[0, width]`. `width` is therefore
  the placed **advance** extent for a hugged box; sizing the box to it clips no
  glyph's advance.
- **A fixed axis is bounded by its authored width.** When a box's width is fixed
  at `w`, the consumer passed `w` as `max_width`, and RTL glyphs flush right
  within `[0, w]`. The consumer sizes that box from the authored `w` (which the
  measure seam already returns unchanged), never from `width`. `width` remains
  the content's own advance, which for a fixed box is smaller than `w`.

### Ink may overhang the advance box; the box is not a clip

The invariant above is over **advance boxes** (pen positions), not over glyph
**ink**. Glyph ink — a left/right side bearing, or a mark's GPOS offset — may
fall outside `[0, width]`. The concrete case #224's thread folded in: reh +
kasra laid out at size 16 (`رِ`) advances `width ≈ 5.87`, but the kasra is a
zero-advance mark whose GPOS offset places its ink at `x ≈ −0.08` — left of the
box origin, outside `[0, width]`, even in hug mode. This is not a defect and not
a reason for a bounds field:

- The **painter does not clip** a glyph run to the text box; it draws each quad
  at its absolute position (`glyph-runs-cross-boundary-b.md`), so sub-pixel ink
  overhang paints normally.
- A **consumer must not** treat `width` (or any box) as a clip rectangle for the
  ink. `width` sizes the layout box for flow and placement; ink that overhangs
  it by a fraction of a pixel is expected and harmless.
- A bounds field would not help: it would report an advance-level rectangle too,
  so the ink overhang would sit outside _it_ as well. The right model is
  advances for layout, per-glyph positions for painting — which is what exists.

## Why

- **The hug case needs no bounds field.** For `max_width = None` the placed
  extent and the content advance are the same number, so a second field would
  carry a value identical to `width` in exactly the case a hug consumer reads.
- **The painter needs no bounds field either.** It positions glyphs from their
  own absolute coordinates, which already carry the flush-right placement (P2 —
  runs cross boundary B already placed). A layout-level bounds rectangle would
  duplicate information the glyph positions already hold.
- **A bounds field would encode a result the consumer already owns (P1
  spirit).** The only quantity a bounds field adds over `width` is the fixed box
  width `w` — which the consumer authored and passed in. Re-exposing it as
  typeset output would make the layout report back the caller's own input.
- **One number keeps the measure seam simple.** The seam needs exactly one width
  per axis; a second field would force it to choose between them and get the
  choice right per axis, for no gain.

## Consequences

- The lowering is consistent with the contract by construction: a
  `WIDTH_AND_HEIGHT` text node lowers to HUG/HUG (`max_width = None` at measure,
  `width` = placed extent), and a fixed-width text node lowers to a Fixed axis
  (bounded by its authored width). See the `textAutoResize` mapping in
  `docs/decisions/figma-flex-lowering.md`'s sibling text lowering.
- Consumers added later inherit the obligation: size a hug axis from `width`, a
  fixed axis from its authored width — never a fixed axis from `width`.
- If a future consumer genuinely needs the rightmost-glyph position of a
  fixed-width RTL box without walking the glyphs (none does today), that is an
  additive field then, not a reason to add one speculatively now.

## Trace

- Satisfies: issue #224 (the width-vs-bounds decision point), issue #160 (the
  first importer RTL consumer, which settles it), P1, P2.
- Verified by: `crates/dashscene-typeset/tests/typeset_arabic.rs`
  (`hug_advance_box_holds_and_mark_ink_may_overhang` — the advance box plus the
  reh + kasra ink overhang on the committed Arabic fixture font;
  `a_fixed_width_rtl_box_is_bounded_by_its_authored_width_not_by_width`). The
  Latin fixture font shapes RTL to `.notdef` with zero offsets, so it cannot
  exercise the mark-ink overhang the contract turns on — the tests use the
  Arabic font.
- Related: `docs/design/typeset-latin.md` (the width contract this pins),
  `docs/decisions/glyph-runs-cross-boundary-b.md` (the painter reads glyph
  positions, not `width`), `docs/decisions/figma-flex-lowering.md` (the per-axis
  sizing lowering the text nodes reuse).
