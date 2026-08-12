# The fill parameters the fragment stage reads share one storage buffer

    status   accepted (2026-08-04), extended by story #584 with a third region
    scope    dashscene-gpu's bind group layout, shaders/paint.wgsl, the gradient
             fill, the stop ramp in shaders/sdf.wgsl, and the shadow rows

## Context

`wgpu::Limits::downlevel_defaults` allows **four storage buffers per shader
stage**, and both stages of this pipeline reached four.

    vertex    instances(0), strokes(4), glyph runs(8), shapes(9)
    fragment  solids(1), clips(2), strokes(4), images(5)

Issue #715 draws gradient fills, which needs two more things in the fragment
stage: the gradient rows and the flat stop array a `dashpaint::StopRange`
indexes.

Story #582's route is not available.
`docs/decisions/tables-the-vertex-stage-reads.md` D2 states the test — a table
may be read by the vertex stage and handed across in `VertexOut` only when
**every value a fragment needs of it is constant across the instance**. A
gradient's rows pass that test; its stop array does not. A stop is looked up by
a normalised position the fragment computes from its own coordinate, and a
varying is a value per vertex, so the array crosses at no width. D4 of the same
record adds that the vertex stage is full anyway.

That record, and `docs/decisions/atlas-residency-and-image-fills.md` D4 before
it, both forecast a **paint-parameter heap** — one storage buffer of `vec4f`
with a per-kind base offset — and both assigned it to this story. This record is
that heap as built.

## Decision

**D1 — binding 1 is a heap of `vec4f` words, not the solid table.** Three
regions since story #584: the solid colours at base zero, then the gradient
rows, then the shadow rows. The fragment stage's binding count is unchanged at
four.

**D2 — the solid region is first, so a solid fill's colour is still
`paints[row]`.** Every region after it needs a base, and each travels in the
per-frame uniform: `Globals::gradient_base`, then `Globals::shadow_base`.

**D3 — a gradient occupies a fixed twelve words**, whatever its stop count:

    +0        (origin.x, origin.y, primary.x, primary.y)
    +1        (secondary.x, secondary.y, kind, stop count)
    +2        stop offsets 0..3
    +3        stop offsets 4..7
    +4 .. +11 stop colours 0..7

**D3a — a shadow occupies two words** (story #584):

    +0   (offset.x, offset.y, sigma, spread)
    +1   the shadow's colour

**The sigma, not the authored blur radius.** `dashpaint::BLUR_SIGMA_PER_RADIUS`
is the mapping, and applying it on the Rust side keeps a measured number out of
the shader entirely — `blur-sigma-is-figmas-mapping.md` is where that number
comes from, and it is now shared rather than restated per painter.

**The kind is not on the row.** A drop and an inner shadow are separate
`InstanceKind` variants, so the fragment stage knows which coverage to build
before it reads the row, and `ShadowKind`'s discriminant never crosses into the
shader — which is what keeps the tag collision that once painted a shadow from
the solid table unrepresentable (`instance-buffer-contract.md` D3).

**D4 — the strokes and images tables stay in their own bindings.** The heap
holds what it had to hold to fit, and nothing else.

**D5 — the gradient's frame is resolved in the shader, from the instance's own
`bounds`.** The handles stay normalised to the node's box on the row, which is
what `dashpaint::Gradient` carries.

**D6 — the stop ramp is `sdf.wgsl`'s, and takes fixed-size arrays.** It is
conformance-tested by layer 2 like every other function in that file, and it
reads no buffer.

**D7 — `wgsl_to_wgpu` is revisited here, and still not adopted.**
`docs/decisions/pipelines-and-layer-3.md` D6 named the growing binding surface
as the trigger; this is that growth, and it is recorded there rather than left
to drift.

## Why

**D1, and why the heap is the only shape left.** Two more fragment bindings
against zero free slots is not an arrangement that exists — the limit is a
device limit and `pipelines-and-layer-3.md` D2 holds this painter to downlevel
defaults deliberately, because that is the entry-tier class of device R3 names.
The alternatives were to raise the limit, to move something else out of the
fragment stage, or to share a binding.

Raising the limit gives up the target class. Moving something out was considered
and does not work: a stroke's parameters and an image fill's are both constant
across an instance, so either could cross as varyings — but the vertex stage
that would have to read them is itself at four of four, so the binding moves
rather than disappears.

Sharing is what is left, and it costs nothing at the read: a heap word is a
`vec4f` load exactly as a solid colour was.

**D2, and why the solid path pays nothing.** Putting the solids first means the
region that every scene uses keeps the indexing it had, and the region that some
scenes use carries the offset. The reverse ordering would have made every solid
fill in every frame pay for a base it did not need.

`gradient_base` is a frame value rather than a constant because it is the solid
count, which changes with the document. Zero is a real value for it — a scene
with gradients and no solids — so nothing reads it as "absent".

**D3, and what a fixed stride buys and costs.** It buys the fragment stage its
row by arithmetic: `gradient_base + row * 12`. A packed layout would need each
row's own word offset, which is either another table or another storage read per
gradient fragment.

It costs 192 bytes per **interned** gradient. `PaintTable::intern_fill`
deduplicates a gradient whole — kind, three handles and the stop list — so the
count is distinct gradients in a document rather than gradient-filled nodes; a
scene with a hundred spends 19 KiB. The unused stop slots are written as zeroes
rather than as a repeat of the last stop, so that a walk running past the count
paints transparent black — a visible absence — instead of a plausible colour.

The kind and the count are stored as floats rather than bit-cast integers. Both
are small non-negative integers — one of four, and at most eight — so an `f32`
holds either exactly, and a heap dumped for a person to read shows a 2 where the
kind is Angular. The **mapping** from `dashpaint::GradientKind` to that number
is an exhaustive `match` in `render.rs`, never `kind as u32`, for the reason
`stroke_align` and `scale_mode` already give: a reordered variant would change
the number a shader compares against and nothing would catch it.

The stride is stated twice — once in Rust, once in `paint.wgsl` — and nothing in
either language holds them together, so
`the_gradient_kinds_are_distinct_and_match_the_shader` reads the shader source
and asserts it. That is the same mechanism the scale modes already use, and it
is here because a wrong stride is the failure with no symptom: a gradient row
read at the wrong offset finds the previous row's stop colours where its handles
should be, which is a well-formed frame and draws a picture.

**D4, and why the heap stopped where it did.** Folding the strokes and the
images in would free no binding — the fragment stage would still read a heap and
the clip boxes — and it would rewrite the upload and read paths that the stroke
story and the image-fill story landed days apart.
`tables-the-vertex-stage-reads.md` gives that same argument for why story #582
did not build the heap at all; it applies just as well to how far this story
takes it.

**D5, and the case that makes it falsifiable.** A gradient row is interned and
shared by every node that authored the same gradient, so the box-to-document
mapping cannot sit on the row. Resolving it in the shader is also what P1 asks
for: the document carries the handles as intent, and the painter resolves the
geometry.

A **masked** gradient is where this can go wrong, because a masked instance's
quad is the coverage field's plane rather than the node's box, and taking the
frame from the quad is the natural mistake. `VertexOut.bounds` has carried the
node box for exactly this reason since story #582, and `dashscene-skia` builds
`gradient_frame` from the entry's box for a masked node exactly as for an
unmasked one. The layer-3 fixture separates them by construction: its field's
quad is 20 units of a 40-unit box, so the two frames read twice as far along the
ramp as each other, sixteen code points apart on two channels.

**D6, and why the ramp takes arrays rather than reading the heap.**
`docs/decisions/shader-library-and-layer-2.md` D2 says the library samples
nothing and reads no derivative: every function takes its arguments and returns
a number. `shaders/sdf.wgsl`'s own header states the rest of that property —
nothing there touches a uniform either — and together they are what make the
file evaluable in a compute shader with no rasteriser. A pointer into the
storage buffer would have been cheaper at the call and would have taken the ramp
out of layer 2's reach, which is the one instrument that checks this math
against an independent derivation.

`dashpaint::MAX_GRADIENT_STOPS` is 8 and boundary B fixes it, so a fixed-size
array is a faithful parameter rather than a guess. The offsets are read from the
heap as two whole words and the colours only as far as the count.

The interpolation is a plain `mix` of the stored components, which is
sRGB-encoded space. `docs/decisions/blur-blends-in-srgb-encoded-space.md` makes
that a term of the boundary-B contract rather than a per-painter choice, and the
reference painter agrees by construction: its surface is `raster_n32_premul`
with **no colour space attached**, and its stops are passed as `Color4f` with a
`None` colour space, so Skia interpolates them componentwise in the same space.

**D7, and what the trigger actually says.** `pipelines-and-layer-3.md` D6 was
revisited at five bindings and left the answer at "what would change it is a
second group, or bindings whose layout is derived rather than written". This
change is close to the second of those and still does not meet it. The bind
group layout is still one group of eleven entries written out by hand in one
place and declared in one place in the shader, and a mismatch between the two is
still a named test failure at `create_render_pipeline`.

What is genuinely new is that the _heap's_ layout is not a WGSL struct at all —
it is a word array with hand-written offsets — and that is precisely the part a
binding-layout generator would not have checked. `wgsl_to_wgpu` reflects
declared bindings; it has nothing to say about what the words inside one mean.
Adopting it here would have bought nothing for the riskiest thing this change
introduces, which is why the answer is unchanged rather than merely
unreconsidered. What holds the heap instead is the source-text assertion D3
describes and the unit tests that read a second gradient's row at its own
stride.

## What this costs, and what it does not

**No new binding**, on either stage. The count is still four and four.

**Two extra uniform members, and the uniform is thirty-two bytes.** It was
sixteen when this record was first written, because `gradient_base` took the
slot the old trailing pad word held and nothing grew. `shadow_base` had no such
slot to take: five scalars is twenty bytes, and a uniform-address-space struct's
size rounds up to a multiple of sixteen, so both declarations carry three pad
words to the same 32. The pads are scalars on the WGSL side and never one
three-component vector, which aligns to sixteen there and would put the struct
at 48 while the Rust type stayed at 32.

It does not change `Instance`, the instance-buffer contract, the layer-1
goldens, or any table on boundary B. It does not change what a solid fill,
stroke, image fill, glyph or coverage mask does.

**A gradient is not dithered**, where `dashscene-skia` dithers its gradients.
That is one of the three divergences story #586 already expects between the two
painters, named in its own body alongside different antialiasing and different
blur falloff. It is stated here rather than deferred silently, and it is not
measured: layer 3 is a gate on the pipeline, not a fidelity check, and layer 4
is the instrument.

## Alternatives considered

**A heap over every fill table, strokes and images included.** Rejected under
D4: it frees no binding and rewrites two shipped paths.

**Two more bindings, at `wgpu::Limits::default` rather than downlevel.**
Rejected: `pipelines-and-layer-3.md` D2 holds this painter to downlevel defaults
because that is the device class the crate exists for, and a painter that ran
only on a desktop adapter would not be the lean painter.

**Packing the stops variably, with the row carrying its own stop offset.**
Rejected under D3: it saves at most 160 bytes per interned gradient and costs a
storage read per gradient fragment, and the fragment stage is the side of that
trade that runs millions of times a frame.

**A `count`-free layout, with the unused stop slots repeating the last stop.**
It would have removed one comparison from the walk, because the ramp would clamp
by construction. Rejected because the invariant then lives only in the CPU
writer: a row filled wrongly draws a plausible gradient, where a count read as
zero draws nothing. The count also lets the fragment stage read only the colours
it needs.

**Storing the kind and stop count bit-cast rather than as floats.** Rejected as
exactness this does not need — both are integers an `f32` represents exactly —
against a heap that becomes unreadable in a dump.

Refs #715. Refs #569. Refs #578. Refs #582.
