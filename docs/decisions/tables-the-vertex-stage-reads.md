# The vertex stage reads a table whose values are constant per instance, and hands the fragment stage the values

    status   accepted (2026-08-04)
    scope    dashscene-gpu's bind group layout, shaders/paint.wgsl, and the
             tables story #582 added — glyph runs and baked-vector coverage
             masks

## Context

`wgpu::Limits::downlevel_defaults` allows **four storage buffers per shader
stage**, and `docs/decisions/atlas-residency-and-image-fills.md` D4 records that
after story #581 the fragment stage read exactly four — solids, clips, strokes,
images — with no headroom at all.

Story #582 needs two more parameter tables. A glyph instance needs its run's
colour and screen-pixel range; a masked instance needs its coverage field's
plane bounds, its rectangle in the atlas and its own range. Neither fits in
`Instance`, which is full and whose width is a boundary-B-shaped contract, and
both are more than a spare word.

D4 forecast a **paint-parameter heap** — one storage buffer of `vec4f` with a
per-kind base offset — and assigned it to issue #715, the gradient story. That
would also have solved this one. This record says why story #582 did not take
it, and what the difference is between the two cases.

## Decision

**D1 — the two new tables are bound to the vertex stage only, and `VertexOut`
carries their values to the fragment stage.**

    vertex    instances(0), strokes(4), glyph runs(8), shapes(9)   4 of 4
    fragment  solids(1), clips(2), strokes(4), images(5)           4 of 4

Story #584 changed the vertex row: the stroke table left that stage, which now
reads three of four. See D4.

**D2 — the test that decides whether a table may take this route is whether
every value a fragment needs of it is constant across the instance.** A glyph
run's colour and range are; a coverage mask's plane, rectangle and range are. A
gradient's **stop array is not** — it is indexed by a value the fragment
computes — so issue #715 still has to change the structure, and the heap D4
names is still that story's.

**D3 — `VertexOut` carries three shared `vec4f` slots rather than one pair per
kind.** An instance is one kind, so the slots are read by whichever branch wrote
them, and adding a kind costs no varying.

**D4 — the vertex stage is now full, and this route cannot be taken a third
time.** The next table to arrive, whatever it is, changes the structure.

**Issue #715 was that next table, and it did.** The gradient rows and their stop
array share binding 1 with the solid colours, which is the heap D4 below names —
`docs/decisions/the-paint-parameter-heap.md` records it. Nothing in this record
changed: the two tables story #582 added are still vertex-only, and D2's test is
still what decided that they could be.

**D4 is no longer true as written, and story #584 is why.** That story needed the
quad's outset for a shadow, whose parameters live in the heap the fragment stage
binds — a table this stage cannot read at any price. The outset moved onto
`Instance` instead (`instance-buffer-contract.md` D9), and the stroke's outset
moved with it, so the stroke table left the vertex stage:

    vertex    instances(0), glyph runs(8), shapes(9)              3 of 4
    fragment  paints(1), clips(2), strokes(4), images(5)          4 of 4

So this route **can** be taken once more. What has not changed is D2's test, and
the reason a free slot is not an invitation: a value that fits on the instance
costs no binding at all, and the free slot exists because story #584 preferred
that.

**D5 — the MSDF atlas is sampled through a second, filtering sampler.** Image
fills keep the nearest sampler `docs/decisions/atlas-residency-and-image-fills.md`
D5 chose. The texture binding is declared filterable to allow it.

## Why

**D1.** It is the same move story #581 made, for the same reason and with the
evidence already in hand: the fragment stage does not need the instance
_array_, it needs the instance's _values_, and `VertexOut` is how a value
crosses. Extending that to a table whose row is equally constant costs one
varying per four floats and no binding at all.

It is also the smaller change. The heap touches every table's upload path and
every shader read site, including the image-fill and stroke paths that the two
stories before this one landed days earlier. Story #582's diff would then have
been mostly not about text, and a defect in the restructure would have been
indistinguishable from a defect in the text work.

**D2, and why this is not a way to avoid the heap forever.** The property is
narrow and it is worth naming precisely, because "put it in the vertex stage" is
otherwise a rule that sounds like it always works. A varying is a value per
vertex; an array indexed at fragment time is not one. A gradient's stops are
looked up by a normalised position the fragment computes from its own
coordinate, so they cannot cross as varyings at any width. The heap is
unavoidable for #715 and this decision does not delay it — it stops #582 from
building it.

**D4, stated because the arithmetic is what an earlier draft of D4 in the
residency record got wrong**, in the direction that would have sent a story
looking for a seat that does not exist. The count now is four and four. There is
no fifth slot in either stage, and the next table is a structural change
whichever stage it would have been read from.

**D5, and why the gutter the residency record warns about is not needed.** That
record names a one-texel gutter as "the first thing to add if filtering ever
becomes linear", because a linear sample taken at a sub-rect's edge reads the
allocation beside it. `msdf_sample` does not sample at the edge: it clamps half
a source texel inside the payload's own rectangle, and a bilinear footprint
taken from a texel's centre weights that texel alone. The clamp was already
there for the nearest case — it is what keeps a glyph from reading its
neighbour along the antialiasing fringe the quad is grown by — and it turns out
to be exactly the condition filtering needs.

The reason to filter at all is that a distance field is not a colour, which is
why `dashscene-skia` samples its MSDF atlases `Linear` and its image fills
`Nearest`. Nearest quantises the edge ramp to the atlas's own texel grid: at a
48-unit render size off a 32 px/em atlas one texel covers 1.5 pixels while the
ramp is 6 pixels wide, so a smooth edge becomes a four-step staircase. Layer 3
would not have caught it — it is a gate on the pipeline, not a fidelity check —
and layer 4 (story #586) would have measured the painter against a decision
nobody made.

Declaring the texture binding filterable is a constraint on the atlas _formats_,
and every format the residency set holds meets it: `Rgba8Unorm` is filterable on
every adapter, and an ASTC format is filterable wherever
`TEXTURE_COMPRESSION_ASTC` is supported at all — which is the only condition
under which one of those textures exists.

## What this costs, and what it does not

**Four `@location` slots**, taking `VertexOut` from five to nine: `shape`, and
the three shared `vec4f` parameter slots. The limit is
`wgpu::Limits::downlevel_defaults`' `max_inter_stage_shader_variables`, which is
**15** — so six slots are free.

That limit counts _variables_, one per `@location`, not summed float components.
A `vec4f` and a `u32` cost one slot each. An earlier draft of this paragraph
said "twelve interpolated components ... against the sixty
`downlevel_defaults` allows", and every part of that was wrong: wgpu 30 has no
`max_inter_stage_shader_components` field at all, the real ceiling is a quarter
of the number quoted, and the count omitted the `shape` varying this same story
added. It is corrected here rather than left, because this is the paragraph
issue #715 reads to decide whether the gradient rows can take the same route,
and a headroom four times too generous is the wrong input to that decision.

Also two sampler bindings against sixteen, and one extra texture read per masked
or text fragment — which is the read the story exists to perform.

It does not change `Instance`, the instance-buffer contract, the layer-1
goldens' shape, or any table on boundary B.

## Alternatives considered

**The paint-parameter heap, in this story.** Rejected for the reason D1 gives:
it is #715's named work, it rewrites paths two stories landed days earlier, and
it would have made this story's diff mostly not about text. It is not rejected
as an idea — D2 says it is unavoidable, and the next table forces it.

**A heap, as a separate preparatory pull request before this one.** Rejected as
the more expensive of the two orders for the same outcome: the preparatory
change moves no pixels, so its only evidence would be that the existing goldens
and layer-3 suites stay green, and #715 would still have to extend it for the
stop array. Doing it once, in the story that needs the stop array, is one review
rather than two.

**A per-glyph row instead of a per-run one.** Rejected: a run's colour and range
are constant across it, and a row per glyph would be a table the size of the
text. `Instance::corners` carries the one thing that does vary — the glyph's own
rectangle — which is the slot the instance-buffer contract reserved for exactly
this, and which an image fill could not have used because an image still needs
its rounded box.

**Nearest sampling for MSDF, deferring the divergence to layer 4's
measurement.** Rejected under D5: the divergence is a decision, and deferring it
to the story that measures the result would have had that story measure a
painter against a choice nobody made.

Refs #582. Refs #569. Refs #715.
