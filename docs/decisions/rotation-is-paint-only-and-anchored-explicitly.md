# Rotation is paint-only, and its anchor is explicit rather than a centre

    status   accepted
    date     2026-08-09
    scope    `Prop`, `BindingChannel`, the variant prop union, `dashbuf`'s node
             table, `dashpaint`'s per-rect row, both painters, and the Figma
             lowering path
    issue    #770, ruled at the opening of slice v0.18 (epic #769)
    refs     #143, #255, #772, #774,
             `visible-is-layout-opacity-is-paint.md`,
             `optional-members-are-ranges-of-arity-one.md`,
             `docs/wip/2026-08-07-motion-in-the-document.md`

A node cannot rotate. There is no transform of any kind in the schema, in
`BindingChannel`, in the variant prop union, or among `Prop`'s 37 variants, so
a spinner — which `technotes/runtime-content.md` §4 names as a canonical
example of the bucket it says to prefer whenever it applies — is not
expressible. This record rules on the two questions story #770 requires
answered before the channel is built, and on how far the vocabulary lands
ahead of the painters.

## The anchor is a point in the node's own space, canonically (0, 0)

**Neither producer rotates about a centre.** The proposal this ruling replaced
had the anchor default to the node's centre, on the stated grounds that it
matched Figma's model and SVG's bare `rotate(a)`; it matched neither, and the
error survived until the fixture below was actually mapped through its own
matrix. That is what makes the anchor worth deciding in a record rather than
inside a story: it is wrong in a way that looks right, and it would otherwise
have been found only when the SVG route landed.

**Figma rotates about the node's local origin.**
`corpus/figma-fixtures/node-fx.json` holds a RECTANGLE named `rotated-15deg`,
`size` 100 × 100, `rotation: -0.26179940325453416` — which is −15° in radians.
Its `relativeTransform` is `[[cos15, +sin15, 30], [-sin15, cos15, 30]]`.
Mapping the four local corners of its own box through that matrix gives

    (0,0)     -> (30.0000,  30.0000)
    (100,0)   -> (126.5926,  4.1181)
    (0,100)   -> (55.8819, 126.5926)
    (100,100) -> (152.4745, 100.7107)

whose axis-aligned bounds are `x = 30`, `y = 4.118092656135559`,
`122.47449457645416 × 122.47449457645416` — the fixture's `absoluteBoundingBox`
to every digit. The rotation is about the local origin and `tx`/`ty` place that
corner.

**SVG rotates about the user-space origin.** MDN's `transform` reference states
it directly: _"If optional parameters `x` and `y` are not supplied, the rotation
is about the origin of the current user coordinate system."_ The centre default
belongs to CSS `transform-origin`, which is a different mechanism on a
different element model.

So the anchor is a point in the node's own coordinate space, with `(0, 0)` — the
node's top-left — as the canonical value rather than a magic default. Every
form resolves into that frame:

| source                                            | anchor           |
| ------------------------------------------------- | ---------------- |
| Figma `relativeTransform` / `rotation`            | `(0, 0)`         |
| SVG `rotate(a cx cy)` on an element at `(ex, ey)` | `(cx−ex, cy−ey)` |
| SVG bare `rotate(a)`                              | `(−ex, −ey)`     |
| a designer's "about the centre"                   | `(w/2, h/2)`     |

Choosing a centre default would have encoded neither producer's convention
while resembling both, and would have silently mis-lowered every SVG
`rotate(a)`. Choosing a bare angle with no anchor would have refused
`rotate(a cx cy)` by name and needed a second append to admit it. The canonical
`(0, 0)` is the same shape `optional-members-are-ranges-of-arity-one.md` argues
for against a sentinel.

The angle is in **radians**, which is already this repository's convention for
an angle — `crates/dashc/src/figma/rest.rs` pins radians for arc angles — and
is Figma's wire unit.

## All three scalars are bindable

`BindingChannel` gains the angle and both anchor components rather than the
angle alone. SVG's `<animateTransform type="rotate">` carries `"a cx cy"` in
its `values` list and animates all three, so an angle-only channel set would be
incomplete against one of the two routes this vocabulary exists for.

## Rotation is paint-only

It does not perturb layout: the solver never sees it, the solved box is
unchanged, and the painter rotates when drawing.

**Both producers agree, so this follows evidence rather than preference.** SVG
transforms are coordinate-system operations applied at paint time, with no flow
layout to disturb. Figma keeps a rotated node's own `size` unchanged — the
fixture above stays 100 × 100 — and only the derived `absoluteBoundingBox`
grows, which is a _result_ that P1 already forbids the document from carrying.
CSS reaches the same place from the other direction, which is why the standard
guidance is to animate transform and opacity only.

It also matches the neighbouring ruling. `visible-is-layout-opacity-is-paint.md`
put `Prop::Opacity` on the paint side for the same reason, and rotation belongs
beside it.

**The consequence for the lowering is not cosmetic.** The Figma path currently
takes a node's box from `absoluteBoundingBox`, which for a rotated node is the
bounds of the _rotated_ shape. For the square fixture above that reads
122.4745 against a true 100 — 22.5 % high at −15°, and √2 ≈ 41.4 % at 45°. The
factor is not bounded by 41 %: it grows with the aspect ratio, and a 10 × 1000
node at 89° reads 100 times its true width. A rotated node's box must come from
`size`.

## Scale and skew are not in this slice

Rotation alone blocks the spinner. `BindingChannel` and the variant prop union
are append-only at the tail, so scale joins later without an R7 break, and
Figma cannot author skew at all — shipping it now would put a construct in the
vocabulary with no producer able to exercise it.

## The vocabulary and the lowering land complete; a painter may refuse

The vocabulary and the lowering land whole, rather than being cut to what one
painter can draw today. The anchor question above is the argument: a partial API
decided against a single producer is what produces a wrong default, and widening
it afterwards costs a second append and a second lowering pass.

**There is one lowering path, not two.** This record was written naming "the
Figma and SVG lowering paths"; building it found that no SVG lowering exists.
The only SVG in `dashc` is path-data parsing for a Figma VECTOR node's
`fillGeometry` (`crates/dashc/src/figma/vector_field.rs`) — an SVG _document_
importer is story #774, unbuilt. The SVG rows in the anchor table above are
still the right rows; they are the contract that importer will be built
against, not a path that exists today.

What may lag is a painter. **A painter that accepted a rotation and drew the
node unrotated would be a silent drop, which P4 forbids**, and this repository
has shipped that failure before — two tests once passed while the feature
rendered nothing. So a painter that cannot rotate declares the gap through the
capability mechanism `dashpaint` already carries for `samples`, and the debt is
filed against that painter rather than hidden inside it. A capability that is
declared can be asserted against; a silent no-op cannot.

## A rotation does not compose down the tree

Found while building the story, and a consequence of "paint-only" this record
did not state. `Prop::Rotation` is per-node: the commit walk resolves every
node's box absolutely and hands the painter one rect per node, and a clip
region is an axis-aligned box, so **nothing carries a parent's turn onto a
descendant**. Figma's rotation is hierarchical — rotating a frame rotates its
contents.

So the Figma lowering accepts a rotated **leaf** and refuses a rotated node
that has children, by name:

    a rotated node with children (a rotation does not compose down the tree)

Lowering it would draw the frame turned with its contents left straight, which
is the silent-wrong-picture P4 forbids and the same failure the painter
capability above exists to prevent. Whether the document should gain a
composing transform is issue #845, and it is the third thing that would justify
revisiting the per-node 2×3 matrix this record deferred.

A rotated node with no `size` is refused for the neighbouring reason: its
extent would have to come from `absoluteBoundingBox`.

## Alternatives considered

- **A full 2×3 transform per node.** It is what both `relativeTransform` and
  SVG's `matrix()` carry, and it would close rotation, scale and skew at once.
  Rejected for this slice: a matrix is not a bindable channel, and animation
  needs scalars, so the channels would have to exist beside it anyway. It stays
  available as the storage form if scale and skew later justify it.
- **A bare angle, anchor deferred.** Cheapest, and it covers Figma completely.
  Rejected because it refuses SVG's `rotate(a cx cy)` and the three-value
  `animateTransform` by name, which is half of this vocabulary's purpose.
- **An angle with the anchor defaulting to the node's centre.** Rejected on the
  evidence above: it matches neither producer.
