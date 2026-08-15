# Technote — the Figma REST shapes the capture pinned

    status   informative — the normative rules live in docs/design/dashc.md
    story    #139 (epic #12, v0.3 — basic paint + importer)
    date     2026-07-13
    informs  crates/dashc/src/figma/rest.rs, #17 (the Deno importer), and
             every future widening of the Figma front end

Story #16 deliberately deferred the Figma lowering until a fixture existed,
because guessing the REST shape would have built the lowering against a fiction
(`docs/decisions/dashc-document-model-and-load-path.md`). This note records what
the capture actually showed — the field shapes that a careful reading of Figma's
documentation would plausibly have got wrong, and what each one would have cost.

The fixtures are `corpus/figma-fixtures/v03-paint.json` (the emission fixture),
`effects-2025.json` (the diagnostic fixture), and
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

Every node carries both `absoluteBoundingBox` and `absoluteRenderBounds`. In the
capture the two differ by exactly the stroke expansion — half the weight for a
`CENTER` stroke, the full weight for `OUTSIDE`, nothing for `INSIDE`.

That is the painter's output measured back. Reading it would bake a rendering
result into a document that carries only intent (P1), so the lowering reads
`absoluteBoundingBox` and never the render bounds. The field is not even present
in `rest.rs`, so it cannot be read by accident.

## Figma's boxes are page-absolute; `Document`'s are parent-relative

`absoluteBoundingBox` is in page coordinates. `Document`'s `Box2D` is
parent-relative intent. The lowering owns the subtraction: a child's box is its
absolute box minus its parent's absolute origin.

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
`strokeDashes` gate is the only thing that does. `dashpaint::Stroke` has no dash
vocabulary, so without that gate a dashed border repaints as a continuous one —
a drop the designer cannot see in the output.

## Smaller shapes worth keeping

- **Paint `opacity` multiplies the color's alpha.** Ignoring it is a silent
  drop; honoring it is two lines.
- **`rotation` is omitted entirely when it is zero.** `None` and `Some(0.0)`
  both mean unrotated. What no capture pins is the converse — that a node whose
  `relativeTransform` carries a turn always carries `rotation` too. No committed
  capture holds that shape, but none could: the corpus holds exactly one rotated
  node, `node-fx.json`'s `rotated-15deg`, and it carries both encodings. The
  lowering therefore reads both, through `rest::Node::turn`: `rotation` first,
  and the matrix where `rotation` is absent or zero (issue #878).
- **A matrix turns when its determinant is positive.** That is what separates a
  mirror from a half-turn, which the off-diagonal alone cannot:
  `[[-1, 0], [0, 1]]` and `[[-1, 0], [0, -1]]` both have zero off-diagonals, and
  only the second is a rotation. A negative determinant is a mirror and reads as
  unrotated, because the document has no mirror and drawing one at 180° would be
  a new wrong picture rather than a repair. A derived turn below 1e-6 rad reads
  as zero: `relativeTransform` is written for every node whether it turns or
  not, so an unrotated one is an identity matrix a round trip could leave a
  residue in, and reading that residue as a turn refuses the node.
- **`relativeTransform` is row-major `[[m00, m01, tx], [m10, m11, ty]]`**, the
  same six components as `imageTransform` below, and `rotation` is
  `atan2(m10, m00)`. `node-fx.json`'s `rotated-15deg` carries
  `rotation: -0.26179940325453416` beside an `m10` of `-0.2588190734386444` and
  an `m00` of `0.9659258723258972`. Only the turn is read: a node's position
  comes from `absoluteBoundingBox`, and the document has no vocabulary for the
  scale.
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

- **`layoutSizingHorizontal`/`layoutSizingVertical` are the per-node, per-axis
  sizing** (`FIXED`/`HUG`/`FILL`), present on every node the captures place in
  an auto-layout context and absent outside one. The older container-side
  `primaryAxisSizingMode`/`counterAxisSizingMode` and child-side
  `layoutGrow`/`layoutAlign` appear alongside them but carry no extra
  information, so the lowering reads the modern pair only.
- **A zero padding edge is omitted**, not written as `0` (the synthetic `column`
  test pins the asymmetric case).
- **Absent alignment means `MIN`.** No capture carries `MIN` spelled out,
  `CENTER`, `MAX`, or `SPACE_BETWEEN`; those values are Figma's documented enum
  and are marked synthetic at their tests. `BASELINE` **is** captured
  (`lowering-baseline.json`, `counterAxisAlignItems`).
- **`itemSpacing` goes negative** (`lowering-negative-gap.json`, `-16`) — legal
  authored overlap, not an error.
- **`layoutWrap` is written on every auto-layout frame** (`NO_WRAP` or `WRAP`),
  and `counterAxisSpacing` appears only alongside `WRAP`.
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
  refuses each by name anyway — treating them as their defaults would silently
  reflow or repaint siblings — with the shapes taken from Figma's documentation
  and flagged as such at the tests. A capture that carries them should be added
  to a fixture when one is next authored.

## `style.fontPostScriptName` cannot confirm which face Figma applied (story #368)

The field looks like the obvious way to check that a TEXT node really carries
the face its `fontWeight` claims. It is not, on two counts found while authoring
the weight fixtures.

**It is `null` for a Regular face.** Every weight-400 node of the v0.10 hero
returns `null` while every heavier node returns a real name (`Inter-Medium`,
`Inter-SemiBold`, `Inter-Bold`). Any probe that requires a non-null PostScript
name therefore fails on exactly the rows that are correct.

**It can be `null` on every row, whatever the weight.** In the captured
`text-bold.json` — three TEXT rows at weights 400, 600 and 700, authored in Noto
Sans by the fixture-author plugin — the field is `null` on all three.
`style.fontFamily`, `style.fontWeight` and `style.fontStyle` do carry the three
weights correctly, so the metadata gap is in this field alone, not in the
capture.

Where the metadata cannot answer the question, Figma's own render can. Counting
non-background pixels inside each row's reported `absoluteBoundingBox` gives
2001 px (Regular), 2552 px (SemiBold) and 2902 px (Bold) — strictly increasing,
because a heavier face lays down measurably more ink for the same string at the
same size — and Figma's three reported row widths differ for the same reason
(271, 280, 285). That is what confirmed three distinct faces were applied, so
the `null` PostScript names are a REST metadata gap rather than evidence of a
substitution. A fixture probe should assert on `fontWeight` and `fontStyle`, and
use ink or advance measurements when it needs to know which physical face
rendered.

## The prototype-interaction shapes (story #773)

Pinned by `prototype-smart-animate.json` and `prototype-refused.json`, the first
two captures in this corpus to carry a prototype interaction at all. Every
capture before them reported `prototypeStartNodeID: null` and an empty
`interactions` array on every node.

These shapes are unusual in this note because there is a **published
machine-readable spec** for them — `@figma/rest-api-spec`'s `api_types.ts` — so
the temptation to skip the fixture was real. The first item below is why the
fixture was authored anyway.

### One node carries the duration twice, in two different units

Every node with a `NODE` action carries both representations at once:

- `interactions[].actions[].transition.duration` is in **seconds**. `0.3`
  written by the plugin comes back as `0.30000001192092896`, the float32
  rounding of that value.
- the flat `transitionDuration` beside it is in **milliseconds**: `300`.

`@figma/rest-api-spec` documents `TransitionSourceTrait.transitionDuration` as
milliseconds, which is correct, and `SimpleTransition.duration` as "the duration
of the transition in milliseconds", **which is wrong**. A lowering that read the
nested field and trusted the comment would have divided by 1000 and animated
every transition in under a millisecond, and nothing in the type system would
have objected — both fields are `number`.

`DirectionalTransition.duration` carries the same wrong comment, and
`refused-push-left` in `prototype-refused.json` pins it: a `PUSH` transition
written at `0.3` returns `0.30000001192092896`, seconds again.

`AfterTimeoutTrigger.timeout` is seconds as well — `1.5` written, `1.5` returned
— but note that in the pinned `@figma/rest-api-spec@0.41.0` that field carries
**no** doc comment at all, so it is the value that is pinned here and not a
correction to the spec.

### The flat triple is lossy, and partly fabricated

`transitionNodeID` / `transitionDuration` / `transitionEasing` are emitted
exactly when a node's **first action is a `NODE` action**, whatever its
navigation, and are absent entirely for `URL`, `SET_VARIABLE` and `CONDITIONAL`
(`prototype-refused.json`: ten nodes carry the triple, thirteen carry
interactions).

They cannot express the trigger, the navigation, the transition type, or a
second action — and where the interaction says there is no transition, the
triple invents one. `refused-on-key-down` carries `"transition": null` inside
its action and `transitionDuration: 300` outside it, a default no author wrote.

So the lowering reads `interactions` and never the flat fields. Reading the flat
triple is not a shortcut to the same data; it is a different, worse answer that
happens to be shaped like the right one.

### The spring presets carry no parameters; the cubic bezier does

`GENTLE`, `QUICK`, `BOUNCY` and `SLOW` all arrive as a bare
`{"type": "GENTLE"}`, with no `easingFunctionSpring`. `CUSTOM_CUBIC_BEZIER`, by
contrast, arrives with its four control points populated, so the omission is
specific to springs rather than general to easing parameters.

That costs dashscene a table: mapping a preset onto `dashcue`'s
`Spring { stiffness, damping_ratio }` needs the four presets' physical
parameters, and REST does not supply them for a preset. The flat
`transitionEasing` carries the same bare name, so it is no help either.

**Scope of that claim.** It covers the four presets, which were captured. It
does **not** cover `CUSTOM_SPRING`, where a caller supplies mass, stiffness and
damping explicitly: the `setReactionsAsync` write of that arm was refused by the
Plugin API, so no `CUSTOM_SPRING` reaction has ever reached a captured file, and
whether REST populates `easingFunctionSpring` for one is still unknown.
`easingFunctionSpring` appearing zero times across both captures is therefore
evidence about presets only — for `CUSTOM_SPRING` the count is circular, because
the arm that would have produced the field is the arm that never landed.

### An instance echoes an inherited interaction in full

`instance-inherited` in `prototype-smart-animate.json` has its reactions
untouched, and REST reports the component's interaction on it verbatim,
identical to the one on `state=rest`. So a lowering reads reactions off the node
it is walking and never has to resolve back through the component set.

### The page-level prototype fields

`prototypeStartNodeID` is non-null in a capture for the first time. Beside it
the page carries `flowStartingPoints: [{nodeId, name}]` — the same node, named —
and `prototypeDevice: {"type": "NONE", "rotation": "NONE"}`.

`prototype-smart-animate` sets its flow starting point explicitly and
`prototype-refused` does not, yet both captures carry one: Figma created
`"Flow 1"` by itself on the frame holding the interactions. A non-null
`prototypeStartNodeID` therefore says nothing about authorial intent.

Which is also why **nothing should assert on `prototype-refused`'s page-level
prototype fields**. Its flow is Figma's choice, made over a node set the plugin
deletes and rebuilds on every run, so both the node id and the flow name can
move between re-authors. `prototype-smart-animate`'s are stable, because that
command names them.

### `Reaction.action` never appears

The Plugin API's `Reaction` has a deprecated singular `action` beside `actions`.
The string `"action"` appears zero times in either capture: REST emits `actions`
only, so the lowering needs no fallback for the singular form.

### Figma's transition is per-interaction; `dashcue`'s is per-prop

Not a field shape, but the structural fact the captures make concrete, and the
one a lowering has to bridge.

One `SMART_ANIMATE` carries a single duration and a single easing. Nothing in
the interaction names a property. Smart Animate then interpolates whatever
differs between the two variants — so the tracks come from **diffing the
variants**, and one Figma spec fans out across all of them.
`prototype-smart-animate` spreads its diff over three children on purpose so
that fan-out is exercised: `bar` differs in Width, `dot` in X, `panel` in Y and
Height.

Two consequences. Figma has no stagger, so `VariantTransition.stagger` lowers to
0 from this producer always. And a fill difference between two variants — which
Smart Animate animates as readily as a rect one — fans out onto
`FillR`/`FillG`/`FillB`, which a variant transition cannot carry. That case is
`refused-fill-diff` in `prototype-refused.json`, and it is the one every real
Figma file will hit.

**As built (story #773, 2026-08-11), the producer refuses it and the load gate
never sees it.** `dashc` emits no fill track at all, so `dashscene_validator`'s
`TRANSITION_CHANNEL_NOT_A_RECT` is not reached from the Figma path — the refusal
is `figma.prototype.unsupported-motion`, a warning, and the fill difference
itself still lowers as a `VariantFill` override. The reason the rule is
rect-only is not P1 but the absence of a paint seam in commit (issue #891,
`docs/decisions/motion-is-document-data-keyed-on-the-destination.md`).

## `version` tracks neither content nor time (measured 2026-08-15)

`GET /file` returns a `version` string beside `lastModified`, and the capture
tool's pre-check skips the full fetch when the metadata endpoint reports the
`version` a fixture's receipt already holds (`importers/figma/src/capture.ts`).
That pre-check assumes `version` moves when the file does. Re-capturing the two
prototype fixtures showed it does not, in two different directions.

`prototype-refused.json` changed materially and its `version` did **not move at
all** — `2385782740689572770` before and after. Between those two captures the
file gained two nodes (`refused-mouse-down` and `scroll-anchor`), grew its root
frame from 256 to 384 px, had every node id renumbered, and had all three of its
component keys reminted. Only `lastModified` moved, `2026-08-09T20:01:02Z` →
`2026-08-11T05:23:19Z`.

`prototype-smart-animate.json` moved the other way: its `version` went
**backwards**, `2385802802996040746` → `2385790038346563365`, while its
`lastModified` moved forwards, `2026-08-09T19:51:20Z` → `2026-08-11T05:23:33Z`.

Both readings come from the committed captures, which are the raw `GET /file`
response minus `thumbnailUrl` — so this is Figma's own field and not something
this repository stamps. What causes either behaviour is **not established
here**; establishing it needs API calls this note cannot make.

The consequence is what matters, and the two readings carry different ones.
`prototype-refused`'s is the damaging case: a version-equality pre-check reads
identical versions across a real change and reports the fixture unchanged, so
the capture stays stale and no further run corrects it. Issue #965 was filed
from that same reading and carries the live-side half of it — 23 named nodes
against the committed 21 — which is the half a committed capture cannot show.
`prototype-smart-animate`'s backwards move costs a wasted fetch rather than a
stale fixture, since an unequal version re-captures either way; it is recorded
because it rules out "`version` only fails to advance" as the description of the
fault. Note the field that behaved in all three captures measured:
`lastModified` moved forwards every time. The pre-check is unchanged here; a
fixture refreshed only when `version` moves is refreshed on Figma's terms rather
than on the corpus's.
