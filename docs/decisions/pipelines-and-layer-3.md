# The lean painter draws one instanced quad per row, offscreen, and layer 3 gates the pipeline

    status   accepted (2026-08-03)
    scope    dashscene-gpu's Renderer, shaders/paint.wgsl, and layer 3 of epic
             #569's verification net

## Context

Story #580 puts the first pixels on screen. The instance buffer (#578) says
what to draw and the shader library (#579) says how to shade it; what was
missing was a device, a pipeline and a target.

## Decision

**D1 — one pipeline, one draw call, four vertices per instance.** No vertex
buffer: the quad's corners come from the vertex index and the instance's own
bounds. A frame uploads the instance rows and nothing else.

**D2 — the painter draws offscreen and reads the pixels back.** A surface needs
a window, and the window is the host's — story #585 puts this painter behind
v0.14's `Present` seam.

**D3 — the target is `Rgba8Unorm`, not `Rgba8UnormSrgb`.**

**D4 — the renderer returns unpremultiplied RGBA8.**

**D5 — layer 3 asserts that the pipeline built, that naga validated, and that
coverage, clipping, stacking, opacity and orientation each did _something_.**
It is never described as a fidelity check.

**D6 — `wgsl_to_wgpu` and `naga_oil` are not adopted.** Story #580 names both;
this is a deliberate departure, recorded rather than quietly skipped.

## Why

**D1.** R-T4 bounds per-frame CPU cost to "dirty-range instance-buffer upload
from the rect table + submission. Nothing else." A vertex buffer would be a
second per-frame upload of data derivable from the first.

The _shader_ design meets that; the frame path does not yet. `Renderer::render`
allocates four buffers, a texture, a view and a bind group on every call,
because this story's caller is a test that renders one frame. Reusing them
across frames, and uploading only the dirty rects' spans, is the work R-T4
actually asks for; it needs a caller that renders more than once, which is the
story that puts this painter behind the host's frame loop (#585).

**D3, and it matters.** `docs/decisions/blur-blends-in-srgb-encoded-space.md`
requires a painter to average a _blur kernel_ over sRGB-encoded values, and
measures the two spaces roughly 50 code points apart across a saturated seam.
Strictly it scopes that rule to blur averaging, and choosing the same space for
ordinary source-over compositing is a wider reading than the record states —
so it is decided here rather than inherited.

It is decided the same way for two reasons. The reference painter composites in
`raster_n32_premul` with no colour space attached, so every golden this project
holds was produced by sRGB-encoded blending; a lean painter blending in linear
light would differ from all of them. And a painter cannot easily use one space
for blur and another for compositing when the blur is realised as
render-to-texture, which is what story #584 plans.

Measured on this painter: half-opaque red over opaque green reads
`[128, 128, 0]` under `Rgba8Unorm` and `[188, 188, 0]` under `Rgba8UnormSrgb`,
60 code points apart. `compositing_happens_in_srgb_encoded_space` pins it, and
was added because a one-character change to `TARGET_FORMAT` passed every other
test in the suite.

**D5, on what layer 3 cannot say.** A coarse per-pixel check on a software
rasteriser establishes that the painter drew something in about the right
place. It says nothing about how the result looks on a real automotive GLES
driver, which is the residual risk epic #569 states plainly and which layer 4
(story #586) is the only instrument for. A green layer 3 read as evidence of
fidelity would be the `t2-check-has-no-teeth` failure v0.13 spent a slice
removing.

**D6, against `wgsl_to_wgpu`.** Its value is that a binding mismatch becomes a
compile error rather than a runtime validation failure. What this crate has
instead is that the mismatch becomes a _test_ failure, immediately and by name:
`create_render_pipeline` validates the bind group layout against the shader's
declarations, and `the_pipeline_builds_and_the_shaders_validate` does nothing
but reach that call. The gap between "compile error" and "the first test that
runs" is real but small, and it costs a build-script code generator and a
generated-source review surface. Worth revisiting when the binding surface
grows past one group of four — story #581's atlas residency is the likely
trigger.

**D6 revisited at five bindings (story #710), and still not adopted.** The
trigger above fired earlier than expected: drawing strokes needed the stroke
table, which is a fifth binding on the same group. The argument did not change
with it. The group is still one group, its entries are still four storage
buffers and a uniform declared in one place each side, and the mismatch is
still a named test failure rather than a silent one — `stroke_align` maps the
alignment by an exhaustive `match`, so the enum half of the hazard is a compile
error already. What would change the answer is a _second_ group, or bindings
whose layout is derived rather than written — story #581's atlas is still the
candidate for both.

**D6, against `naga_oil`.** Its value is `#import` and module composition, so
the SDF math is one source rather than copies. That property already holds:
`docs/decisions/shader-library-and-layer-2.md` D1 single-sources through
textual inclusion, and `paint.wgsl` is concatenated after `sdf.wgsl` exactly as
the conformance suite concatenates its probes. `naga_oil` would make the
inclusion declarative rather than positional, which is better hygiene and not a
new capability. The cost is a preprocessor between the source and naga, which
is a layer to debug through when a shader fails to validate.

Both are recorded as **not adopted with reasons**, not as oversights, so that
the next story to want either has the argument in front of it.

## What is drawn, and what is not

Opaque rounded rects with a solid fill and their outline stroke, clipped by
their region, composited in slice order at their free-path opacity.

**The stroke arrived at story #710**, which exists because nothing in
epic #569's breakdown drew one: the packer has emitted `InstanceKind::Stroke` since
story #578, and no story after it named the kind. Found by running the two
painters against one scene once story #585 made that possible — the borders
were missing and no issue owned them.

Two things it needed, and only one was new. `sdf.wgsl`'s `stroke_coverage` was
already written and already conformance-tested by layer 2 (story #579), so the
band is not this story's arithmetic. What was new is that **a stroke instance
draws outside the bounds its quad is built from**: an instance is stated over
the node's fill box, and an Outside stroke paints a full width beyond it, a
Center stroke half of one. The vertex shader grows the quad by that outset,
read from the stroke row the instance names. Without it the outer half of every
non-Inside stroke is clipped by its own geometry, which looks like a thinner
stroke and not like a defect — `an_outside_stroke_draws_past_the_box_its_quad_is_built_from`
is the assertion that fails when it is not done.

One divergence from the reference painter is **not** ruled out here. Skia
strokes by expanding the geometry — `rrect.with_inset(w/2)` or `with_outset` —
and then clamps the offset rounded rect's own radii; this shader shifts the
band on the _unoffset_ box, whose radii were clamped before the shift. The two
agree wherever the level sets of a convex rounded box are its offsets, which is
everywhere the radii are not over-subscribed. A node whose radii exceed its
box, stroked, is the case where they could part. Layer 4 (story #586) is the
instrument that would measure it; layer 3 cannot, because it does not compare
against the reference at all.

An instance whose kind or fill tag this shader does not implement **draws
nothing**, and does not fall through to a colour. `Instance::tag` means a
different enum for each `kind` and their discriminants collide —
`PaintTag::Solid`, `ShadowKind::Inner` and `BlurKind::Backdrop` are all 1 — so
a shader that read the tag alone painted a shadow instance with
`solids[shadow_row]`, over the fill it belonged to. Review found it with a
node carrying an inner shadow; the fragment shader gates on both now.

Drawing nothing rather than black is deliberate. Black would be loud, but an
inner shadow is packed _after_ the fill, so a black shadow instance covers the
node it belongs to and every shadowed node is corrupted. Drawing nothing leaves
the picture correct for the subset this story implements and absent for the
rest. That is not a silent drop: the packer emits the instance, the layer-1
golden shows it, and this record lists what is drawn.

Text and baked vector fields are story #582's, shadows and backdrop blur #584's,
render-target group opacity #583's. The instance buffer already carries all of
them; this story draws the first kind.

**Corrected 2026-08-03.** That sentence said "gradients and image fills are
story #582's" from the day it was written, and neither was: story #582 is glyph
runs and vector fields by its own body. The misattribution is what hid the gap
for three story closes — image fills landed with story #581 and its residency
work, and gradients had no owner at all until issue #715 was filed. This
paragraph is now stated against the story bodies rather than from memory.

## Verified where, and where not

Developed and run on an **Apple M3 via Metal**; a test prints the adapter.
Seven layer-3 checks, and six of seven mutations against the render shader are
caught by name.

The seventh was not, and the fix is worth recording: **every fixture was
vertically centred on the canvas**, so flipping the y axis mapped each shape
onto itself and no assertion moved. That is the uniform-fixture defect in a
third guise — after uniform data (#650, #651, #699) and uniform arguments
(#579), uniform _symmetry_. `the_documents_y_down_origin_maps_to_the_top_of_the_image`
uses an off-centre rect and kills it.

**Not verified on lavapipe**, for the same reason story #579's suite is not:
the account's Actions billing is unsettled and no CI job can be scheduled. A
missing device fails by name rather than passing vacuously.
