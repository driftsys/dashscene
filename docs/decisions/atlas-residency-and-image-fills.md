# Atlas residency: one texture per texel format, and the image fills that use it

    status   accepted (2026-08-03)
    scope    dashscene-gpu (residency, render, paint.wgsl), dashpack (preview),
             goldens

## Context

Story #581 is where a payload first reaches the GPU. Three consumers need the
same thing: MSDF glyph atlases, baked vector fields, and image fills. The
direction note for epic 569 picked `etagere` plus `lru` for it, which is the
pairing `glyphon` uses for a dynamic glyph atlas.

Two things forced the shape it took. `wgpu::Limits::downlevel_defaults` has no
binding arrays, so a texture per payload would need a bind group per draw; and
it allows **four storage buffers per shader stage**, which this pipeline had
already spent before an image table was added.

The story is also where the lean painter's `Painter::samples` stops being the
trait default. That is the last step of the path opened by issue 640 and
completed by issue 716.

## Decision

**D1 — one atlas texture per texel format, packed by shelf, evicted by
recency.** `AtlasFormat` is `Rgba8` or `Astc { block }`, derived from
`ImageFormat` by an exhaustive match.

**D2 — the payload's colour space does not choose an atlas.** Every atlas is
created with wgpu's **Unorm** channel, so the two colour spaces of one block
footprint share one texture.

**D3 — a frame is one draw call per atlas it samples**, in instance order.

**D3a — residency follows the frame, not the table.** Only the image rows some
instance names are made resident.

**D4 — the fragment stage reads no instance array.** `VertexOut` carries the
values it needs.

**D5 — the sampler is nearest and clamped, and there is no gutter between
allocations.**

**D6 — `GpuPainter::samples` is built from an adapter**: PNG, both `Rgba8`
rungs, and the ASTC rungs when the device has `TEXTURE_COMPRESSION_ASTC`. Not
JPEG, not GIF.

**D7 — a derivation is unwrapped at load, not in the frame.**
`dashpack::preview::blocks` parses the KTX2 container, inflates the Zstandard
level and stops before the ASTC decode.

## Why

**D1, and what it rules out.** Atlasing is what lets one texture binding serve
many payloads, which is the only shape available under downlevel limits. The
allocator is `etagere::AtlasAllocator` rather than the `BucketedAtlasAllocator`
`glyphon` uses: buckets quantise height, which is the right trade when every
allocation is a glyph and the wrong one when the same atlas holds a 380x380
photograph.

Eviction is frame-aware. A payload the current frame has already asked for is
never a victim, because evicting it would free space the same frame is about to
need again and the allocation loop would not terminate. When every resident
payload of an atlas is in the current frame's working set, that is
`FrameExceedsAtlas` — a named refusal rather than a loop.

**A compressed atlas is not the size it was asked for.** A block-compressed
texture's dimensions must be a multiple of its footprint, and four of the six
ladder rungs do not divide 2048 — wgpu refuses it by name. Each atlas is
therefore the nominal extent rounded **down** to whole blocks, 2046 at 6x6, and
a slot normalises against its own atlas rather than against the set's nominal
extent. The copy is block-aligned for the same reason: a 380x380 payload at 6x6
is written as 384x384, which is the region the allocator reserved, because a
partial-block copy is legal only where it reaches the texture's own edge and an
allocation in the middle of an atlas never does.

**D2, and why it is not a shortcut.**
`docs/decisions/blur-blends-in-srgb-encoded-space.md` makes sRGB-encoded
blending a term of the boundary-B contract rather than a per-painter choice, and
`pipelines-and-layer-3.md` D3 is why the render target is `Rgba8Unorm`. A
`*Srgb` texture view would have the sampler linearise on read, putting image
texels in a different space from every other colour in the shader. So an
sRGB-encoded payload is sampled as the encoded value it is, and a linear one —
what `dashpack` writes for a distance field — as the raw value it is. Both are
"give me the stored number".

The consequence worth stating: `Astc6x6Srgb` and `Astc6x6Unorm` share an atlas.
They are different _payloads_ and the same _texture format_ here, and a painter
that ever wanted hardware sRGB decode would have to split them.

**D3, and what it costs.** A frame whose image rows all landed in one atlas —
every frame with one image format, which is every frame this repository draws
today — is a single run over the whole buffer, decided from the resolved rows
alone without looking at an instance. Segmenting the buffer happens only when a
frame genuinely mixes texel formats, which a document does when a host binds
derivations for some assets and not others.

That is a claim about the _segmenting_ pass and not about the frame as a whole:
D3a below walks the instance rows once in any frame that has an image fill, so a
frame with images costs one pass whatever its formats are, and a frame without
them costs none.

The runs partition the buffer in order, so slice order is still draw order.

**D3a, and the defect that produced it.** The first shape of this resolved every
row of the fill table, which reads as a harmless over-approximation and is not
one. A document's asset table is every image it _could_ show; what has to fit in
VRAM is what it shows now. Resolving the table makes the whole table the working
set by construction, so a document holding more assets than one atlas can carry
fails `FrameExceedsAtlas` while drawing two of them — and the eviction policy
can never help, because every row is asked for every frame. That is issue #460's
measurement one level up, arriving as a crash rather than as memory pressure.

Resolving the drawn set costs one pass over the instance rows, in a frame that
has an image fill at all. R-T4 bounds the per-frame cost to the dirty-range
upload and the submission, so this is outside that budget and is stated rather
than hidden. The alternative — the packer recording the rows it emitted, which
is free — would be a second record of a fact the instances already carry, and is
not taken here.

The check that holds it is a fixture whose undrawn asset is one **no atlas could
hold**. Three ordinary payloads would land in one atlas either way and every
counter would agree; an asset one texel past the device's largest texture fails
by name the moment anything asks for it, and passes in silence when nothing
does.

**D4, and the limit that forced it.** The pipeline binds five storage buffers
and downlevel defaults allow four per stage. Declaring each binding where it is
actually read leaves the vertex stage with two — the instances, and the stroke
rows its outset needs — and the fragment stage with four.

It is also the more ordinary shape for an instanced renderer: a fragment needs
the instance's _values_, not the instance _array_.

**It creates no headroom at all, and an earlier draft of this paragraph said it
created one slot.** The fragment stage reads bindings 1, 2, 4 and 5 — solids,
clips, strokes, images — which is four of the four allowed. What the change
bought was the ability to add the image table without exceeding the limit, not
room for the next one. The gradient rows and their stop array, filed as issue
715, are two more fragment bindings against **zero** free slots, so that story
cannot be a binding away: it has to change the structure, and a paint-parameter
heap — one storage buffer of `vec4f` with a per-kind base offset — is the
obvious candidate. `pipelines-and-layer-3.md` D6's `wgsl_to_wgpu` question rides
along with it.

The arithmetic is worth stating because the wrong version of it would have sent
issue 715 looking for a seat that does not exist.

**Since story #582 the vertex stage is full too.** That story needed two more
parameter tables and did not build the heap: it bound both to the vertex stage
and carried their values across in `VertexOut`, which works because every value
a fragment needs of a glyph run or of a coverage mask is constant across the
instance. `docs/decisions/tables-the-vertex-stage-reads.md` records it, and
records why a gradient's stop array cannot take the same route — it is indexed
by a value the fragment computes, so it does not cross as a varying at any
width. The count is now four and four, and issue 715 is still where the heap
gets built.

**The heap was built at issue 715, and the forecast held.** Binding 1 stopped
being the solid table and became a `vec4f` word heap carrying the solid colours
and the gradient rows, with the gradient region's base in the per-frame uniform.
The fragment stage's count is unchanged at four, and the image table this record
is about kept its own binding — folding it in would have freed nothing.
`docs/decisions/the-paint-parameter-heap.md` is the record.

**D5, as amended by story #582.** An _image fill_ is still sampled nearest, and
the paragraph below is why. A **glyph atlas or a coverage mask** is sampled
through a second, filtering sampler added by that story, because a distance
field is not a colour — `dashscene-skia` draws the same distinction. The gutter
this paragraph names is still not needed, and
`docs/decisions/tables-the-vertex-stage-reads.md` D5 says why: the clamp half a
texel inside the sub-rect, which was already there for the nearest case, is
exactly the condition a bilinear footprint needs.

Nearest matches `dashscene-skia`'s `SamplingOptions::default()`, which that
painter chose deliberately — "deterministic and exact for the v0.3 corpus;
filtering quality is a later, deliberate decision". With nearest sampling and a
clamp to texel centres inside the sub-rect, no sample can reach a neighbouring
allocation, so a gutter would be padding against a hazard this painter does not
have. **It is the first thing to add if filtering ever becomes linear**, and the
residency module says so where the sampler is built.

**D6, and why the adapter is part of the answer.** ASTC is a device capability.
A painter that claimed the block formats unconditionally would have a host bind
a derivation the device then refuses, which is the failure `samples` exists to
make impossible — so the declaration is built from an adapter and
`GpuPainter::new` is the conservative answer for a painter that has not met one.
The feature is also _requested_ when the adapter has it: a feature the adapter
advertises and the device did not ask for is not a feature the device has.

Refusing JPEG and GIF is the narrowing that pays for the rest. Every container
this painter claims is a decoder it links, and the trim profile whose existence
justifies the crate removes libpng, libjpeg and libwebp alike. `dashpack`'s
ladder takes any canonical container to ASTC or RGBA8 before a product build
ships, so the gap is a raw-document one — issue #718 records it, with the two
ways to close it.

**D7, and why it is not a transcode.**
`docs/specification/03-target-hardware-rules.md` requires product assets ship
"as native ASTC directly, with no Basis and no transcode step of any kind". A
derivation is a KTX2 file, so something has to unwrap it, and `preview::blocks`
is that step: the blocks that come out are byte for byte the blocks the encoder
wrote, at the footprint it wrote them, and nothing re-encodes anything. What the
rule forbids is arriving at the block format at run time from another one. It
runs once, at load, which is also what P3 requires.

`preview::decode` is now written in terms of it, so the container handling has
one implementation rather than two.

## Verified where, and where not

Layer 3 gains an image suite: the payload's own texels drawn in the payload's
own axes, two payloads in one atlas each drawing their own, Fit letterboxing
where Fill covers, Tile repeating from the box origin, Crop mapping the box
through the fill's transform, and a PNG decoded and drawn. Every fixture is
asymmetric in extent and distinct in every texel, because a square payload
cannot fail a transposed extent and a flat one cannot fail a wrong atlas offset.

The residency set is tested against a small atlas rather than a 2048-texel one:
disjoint rectangles for payloads of three different extents, a slot kept across
frames, eviction taking the least recently used and never the current frame's
own, and both refusals by name.

`draw_runs` is a pure function and is tested as one, including that the runs
partition the buffer in order.

**The whole chain is tested once, in `goldens`**: `dashpack` encodes a corpus
image to ASTC 6x6, `preview::blocks` unwraps it, boundary B carries it, and the
lean painter uploads and draws it — compared against the same blocks through
this project's own software decoder. That is the strongest statement available
without hardware: the GPU's ASTC unit and this project's decoder agree about
these blocks. It fails if the blocks arrive at the wrong footprint, in the wrong
colour space, at the wrong extent, or byte-shifted.

**Not verified: ASTC on a runner.** The block arm skips, loudly, on an adapter
without `TEXTURE_COMPRESSION_ASTC`, and whether lavapipe advertises it has not
been established — no Linux runner has executed this suite, because CI has been
unable to schedule a job since before the story started. The uncompressed rung
exercises the same `push_baked`, the same residency upload and the same
declaration with the block arithmetic removed, and runs everywhere.

**Not verified: fidelity.** Nothing here compares against the reference painter.
Layer 4 is the instrument, it needs hardware, and it is story #586's.

## What this does not do

**Glyphs and vector fields do not use it yet.** The mechanism is general and
story #582 is where the other two consumers arrive. What is shipped is one
consumer and the seam the other two will come through.

**No host selects a derivation.** `preview::blocks` makes the load-time step
available and a test performs it; wiring a rung choice into the showcase host is
later work, and the profile question `dashpack` already answers.

**A payload larger than one atlas is refused, not tiled.** The atlas is
`ATLAS_EXTENT` texels on a side — 2048, or 2046 at a 6x6 footprint — clamped by
what the device will give. That is a **budget**, not the device's maximum, and
the two must not be confused: an atlas is allocated whole the first time a
payload of its format appears, so sizing it by an adapter reporting 16384 would
commit a gigabyte the moment one image fill appeared. It is the opposite of the
question `Renderer::max_extent` answers, which issue #714 deliberately took from
the adapter.

An earlier draft of this record said the atlas "is the device's largest texture
under downlevel limits", and the code read it back out of
`Limits::downlevel_defaults()` beside a comment saying the device had been
requested at those limits. Neither was true after issue #714 changed the request
to `.using_resolution(adapter.limits())`. Both are now stated as the constant
they are, with the reason attached.

The limitation that remains is real: on a device that could hold a 3000-texel
image, this painter refuses it by name rather than drawing it. Issue #720
carries the fix, which is a dedicated texture outside the atlas — not a bigger
atlas.
