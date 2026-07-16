# The Figma REST shapes the capture pinned

    status   informative — the normative rules live in docs/design/dashc.md
    story    #139 (epic #12, v0.3 — basic paint + importer)
    date     2026-07-13
    informs  crates/dashc/src/figma/rest.rs, #17 (the Deno importer), and
             every future widening of the Figma front end

Story #16 deliberately deferred the Figma lowering until a fixture existed,
because guessing the REST shape would have built the lowering against a
fiction (`docs/decisions/dashc-document-model-and-load-path.md`). This note records
what the capture actually showed — the field shapes that a careful reading of
Figma's documentation would plausibly have got wrong, and what each one would
have cost.

The fixtures are `corpus/figma-fixtures/v03-paint.json` (the emission
fixture), `effects-2025.json` (the diagnostic fixture), and
`lowering-variant-topology.json`. Nothing below is inferred; each item is a
field in a captured file.

## `cornerRadius` and `rectangleCornerRadii` are mutually exclusive

Figma nulls whichever does not apply. A node has a uniform radius or a
per-corner array, never both, and never both absent-but-implied.

`rectangleCornerRadii` is `[top_left, top_right, bottom_right, bottom_left]`,
which matches `dashpaint::CornerRadii`'s field order exactly.

So the lowering reads the per-corner array first and falls back to the uniform
radius, rather than reading one and overlaying the other. A lowering that
expected both fields to be populated — the shape the two names suggest — would
have read a null.

## `strokeWeight` and `strokeAlign` are present even when `strokes` is empty

A node with no stroke at all still carries a weight and an alignment. So the
stroke lowering must gate on a **non-empty `strokes` array** and never on the
presence of a weight. Gating on the weight paints a stroke on every node that
has none.

## `absoluteRenderBounds` is a result, not intent

Every node carries both `absoluteBoundingBox` and `absoluteRenderBounds`. In
the capture the two differ by exactly the stroke expansion — half the weight
for a `CENTER` stroke, the full weight for `OUTSIDE`, nothing for `INSIDE`.

That is the painter's output measured back. Reading it would bake a rendering
result into a document that carries only intent (P1), so the lowering reads
`absoluteBoundingBox` and never the render bounds. The field is not even
present in `rest.rs`, so it cannot be read by accident.

## Figma's boxes are page-absolute; `Document`'s are parent-relative

`absoluteBoundingBox` is in page coordinates. `Document`'s `Box2D` is
parent-relative intent. The lowering owns the subtraction: a child's box is
its absolute box minus its parent's absolute origin.

The root frame drops its page position entirely and lowers to `(0, 0, w, h)`,
because where a frame happens to sit on the Figma canvas is a page-layout
artifact of the design file, not intent about the UI.

## A progressive blur serializes as a `LAYER_BLUR` with `blurType: PROGRESSIVE`

There is no `PROGRESSIVE_BLUR` effect type. The effect type alone therefore
cannot decide the band: a plain layer blur is LATER (a warning), and a
progressive blur is REJECT (an error), and both arrive as `LAYER_BLUR`.

A triage table keyed on `effects[].type` alone — which is what the shape
suggests — would have silently accepted progressive blur as a warning.

## A dashed stroke keeps `strokeType: "BASIC"`

This is the one that would have been missed. A dashed stroke carries
`strokeDashes: [10, 5]` (pinned by `lowering-variant-topology.json`, whose root
carries exactly that) and **still** reports
`complexStrokeProperties.strokeType: "BASIC"`.

So Figma expresses a dash pattern without changing the stroke type, and the
`complexStrokeProperties` gate can never catch a dashed stroke. The
`strokeDashes` gate is the only thing that does. `dashpaint::Stroke` has no
dash vocabulary, so without that gate a dashed border repaints as a continuous
one — a drop the designer cannot see in the output.

## Smaller shapes worth keeping

- **Paint `opacity` multiplies the color's alpha.** Ignoring it is a silent
  drop; honoring it is two lines.
- **`rotation` is omitted entirely when it is zero.** `None` and `Some(0.0)`
  both mean unrotated.
- **`layoutMode` is written as `NONE` on a frame with auto-layout off**, and
  omitted on a node that cannot have one. The newer `GRID` value exists, so the
  field is read as an open string rather than a closed enum.
- **`imageTransform` is row-major `[[a, b, tx], [c, d, ty]]`** — the same six
  components as `dashpaint::Mat23`. Absent means identity.
- **Node vocabulary is open.** `type`, `blendMode`, `strokeType`, `layoutMode`,
  and effect `type` are all read as strings, so an unrecognized value is a loud
  error at the node rather than a parse failure of the whole file. Every
  _closed_ set (`PaintTag`, `ScaleMode`, `StrokeAlign`) is a real enum, so an
  unknown value fails the parse instead of defaulting silently.

## The auto-layout shapes (story #140, the five lowering fixtures)

Pinned by `lowering-hug-in-fill.json`, `lowering-negative-gap.json`,
`lowering-wrap.json`, `lowering-baseline.json`, `grid-basic.json`, and
`variables-bound.json`:

- **`layoutSizingHorizontal`/`layoutSizingVertical` are the per-node,
  per-axis sizing** (`FIXED`/`HUG`/`FILL`), present on every node the
  captures place in an auto-layout context and absent outside one. The
  older container-side `primaryAxisSizingMode`/`counterAxisSizingMode` and
  child-side `layoutGrow`/`layoutAlign` appear alongside them but carry no
  extra information, so the lowering reads the modern pair only.
- **A zero padding edge is omitted**, not written as `0` (the synthetic
  `column` test pins the asymmetric case).
- **Absent alignment means `MIN`.** No capture carries `MIN` spelled out,
  `CENTER`, `MAX`, or `SPACE_BETWEEN`; those values are Figma's documented
  enum and are marked synthetic at their tests. `BASELINE` **is** captured
  (`lowering-baseline.json`, `counterAxisAlignItems`).
- **`itemSpacing` goes negative** (`lowering-negative-gap.json`, `-16`) —
  legal authored overlap, not an error.
- **`layoutWrap` is written on every auto-layout frame** (`NO_WRAP` or
  `WRAP`), and `counterAxisSpacing` appears only alongside `WRAP`.
- **The grid fields** (`grid-basic.json`): `gridRowCount`/`gridColumnCount`,
  `gridRowsSizing`/`gridColumnsSizing` (a CSS-like track string, e.g.
  `"96px minmax(0,1fr) minmax(0,1fr)"`), and per-child
  `gridRowSpan`/`gridColumnSpan`/`gridRowAnchorIndex`/`gridColumnAnchorIndex`.
  Captured for the v0.8 grid lowering; the v0.7 walk refuses `GRID` before
  reading them.
- **`id` is on every node** (`"1:23"`), unique across the file — what a
  diagnostic path uses to split duplicate sibling names.
- **Not pinned by any capture:** `layoutPositioning: "ABSOLUTE"`,
  `strokesIncludedInLayout: true`, and `itemReverseZIndex: true`. The walk
  refuses each by name anyway — treating them as their defaults would
  silently reflow or repaint siblings — with the shapes taken from Figma's
  documentation and flagged as such at the tests. A capture that carries
  them should be added to a fixture when one is next authored.
