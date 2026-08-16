# dashscene-gpu — the lean painter, instanced quads and analytic SDF over wgpu

    crate    crates/dashscene-gpu
    covers   v0.15 (epic #569) end to end: the crate and its seam
             (story #577), the instance-buffer contract and layer 1
             (story #578), the shader library and layer 2 (story #579),
             pipelines, clips and the first pixels (story #580), atlas
             residency and image fills (story #581), text and baked vector
             fields (story #582), render-target group opacity (story #583),
             the two shadow kinds (story #584), the swapchain path and the
             showcase host (story #585), layer 4 measured (story #586), the
             web target (story #587), boundary B kept FFI-representable
             (story #600), baked texel payloads (story #640), the outline
             stroke (story #710), gradient fills (issue #715), the image
             extent (story #716), the backdrop blur (story #733), and the
             macOS presentation fix (story #746)

## Purpose

`dashscene-gpu` is the second implementation of the `Painter` trait `dashpaint`
defines (boundary B): instanced quads with analytic signed-distance fields over
`wgpu`, covering native and web from one codebase. It is the lean painter of
`docs/decisions/backend-tiering-unity-skia-lean.md`, named for the role rather
than for its backend — the strategy record's contingency, if `wgpu`'s GL backend
fails on a target, is a direct-GLES backend written over the same instance
buffer and the same shaders (`docs/decisions/wgpu-is-the-lean-painter.md`).

The whole v0 paint vocabulary draws through it: rounded rects with a solid,
gradient or image fill, their outline stroke, positioned glyph runs, a fill
masked by a baked vector field, both shadow kinds and the backdrop blur — all
clipped by their region, and render-target group opacity as an offscreen layer
composited at the group's alpha.

P2 binds it as it binds the reference painter: it only colours, and never
measures, wraps, kerns, or moves anything. There is no path primitive at
boundary B, and that absence is what makes this crate a translation of the paint
table into draw calls rather than a 2D rasteriser — every primitive in the
vocabulary maps onto an instanced quad with a fragment shader.

This crate does not replace `dashscene-skia`. Skia stays permanently as the
bit-exact CPU oracle and as the entry-tier bridge until this painter is measured
on a real entry SoC; what wgpu retires is Skia's trim profile — the from-source
GLES build, `skia_use_gl`, and the Ganesh-to-Graphite churn watch.

## Public interface

Five types carry the crate, in `crates/dashscene-gpu/src/`:

    pub struct GpuPainter { /* private: an InstanceBuffer */ }

    impl Painter for GpuPainter {
        fn samples(&self, format: ImageFormat) -> bool;
        fn rotates(&self) -> bool;   // true — story #832
        fn paint(&mut self, rects, paints, images, clips, groups, glyphs, dirty);
    }

    impl GpuPainter {
        pub fn new() -> Self;              // claims no baked block format
        pub fn on(renderer: &Renderer) -> Self;   // claims what the device can sample
        pub fn instances(&self) -> &InstanceBuffer;
    }

`GpuPainter::paint` packs boundary B's tables into an `InstanceBuffer` and
**submits nothing**. That split is boundary B's own shape: a `Painter` is handed
tables and returns nothing, and a device is not part of that contract. What
draws the buffer is one of two renderers:

    impl Renderer {                        // offscreen, render.rs
        #[cfg(not(target_arch = "wasm32"))]
        pub fn new() -> Result<Self, RendererError>;
        pub async fn new_async() -> Result<Self, RendererError>;
        pub fn render(&mut self, buffer, paints, images, clips, glyphs,
                      width, height) -> Result<Vec<u8>, RendererError>;
        pub fn render_dirty(&mut self, ..., changes: Option<Changes<'_>>, ...)
                      -> Result<Vec<u8>, RendererError>;
        pub fn max_extent(&self) -> u32;
    }

    impl SurfaceRenderer {                 // to a window or canvas, surface.rs
        #[cfg(not(target_arch = "wasm32"))]
        pub fn new(target: impl Into<wgpu::SurfaceTarget<'static>>,
                   width: u32, height: u32) -> Result<Self, RendererError>;
        pub async fn new_async(...) -> Result<Self, RendererError>;
        #[cfg(target_arch = "wasm32")]
        pub async fn for_canvas(canvas: web_sys::HtmlCanvasElement,
                   width: u32, height: u32) -> Result<Self, RendererError>;
        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RendererError>;
        pub fn present(&mut self, buffer, paints, images, clips, glyphs,
                       changes: Option<Changes<'_>>) -> Result<Drawn, FrameError>;
    }

**The blocking constructors are native-only, and that is enforced rather than
documented.** `pollster::block_on` cannot succeed on a browser's main thread —
the promise it waits on resolves only by returning to the JS event loop the wait
is holding — so `new` is gated behind `cfg(not(target_arch = "wasm32"))` and
`pollster` is a native-only dependency. A web host calls `new_async`, or
`for_canvas` for a `SurfaceRenderer`. `just wasm-painter` is the gate that keeps
it that way.

`Residency` (`residency.rs`) holds the atlases both renderers sample, and
`Pass`/`Step` (`composite.rs`) is the pass plan they execute. The painter holds
its `InstanceBuffer` across frames rather than returning a fresh one, so a
steady-state frame repacks into an allocation it already has.

Two constants are part of the contract. `TARGET_FORMAT` is `Rgba8Unorm` rather
than `Rgba8UnormSrgb`, because this painter blends in sRGB-encoded space and a
`Srgb` format would have the hardware convert on write and blend in linear light
(`docs/decisions/blur-blends-in-srgb-encoded-space.md`). `ATLAS_EXTENT` is 2048
— a memory budget rather than a ceiling, since an atlas is allocated whole the
first time a payload of its format appears, and 2048 square is 16 MiB of
`Rgba8Unorm` where the 16384 an M3 reports would be 1 GiB. It is applied as
`ATLAS_EXTENT.min(max_extent)`, so on a device whose maximum is smaller the
atlas is smaller too. It is deliberately not the same question as
`Renderer::max_extent`, which is how large a _drawable_ the hardware can address
and is taken from the adapter.

## The instance buffer is the painter's output

`docs/specification/03-target-hardware-rules.md` R-T4 bounds per-frame CPU cost
to "dirty-range instance-buffer upload from the rect table + submission. Nothing
else." If that is the whole of the painter's frame, then the instance buffer
**is** what the painter produces, and the GPU is a pure function of it and of
the boundary-B tables its rows index.

That is why the largest class of painter defect — a dropped clip, a wrong paint
row, a wrong draw order, a group applied to the wrong set — is a data defect,
testable bit-exactly on a runner with no GPU. It is also why `instance.rs` names
no `wgpu` type: the struct is shared with the future Unity painter, which epic
#569 plans as instanced SDF quads too.

Every quad is one `Instance` in one ordered, kind-tagged stream, with an
`InstanceSpan` per rect naming that rect's range and a `Layer` per group
(`docs/decisions/instance-buffer-contract.md` D1). The per-node order within a
span is copied from `dashscene-skia` rather than re-derived from boundary B, so
both painters stack a node's parts identically (D5). Some instances reach ink
outside their own `bounds` — a Center stroke by half its width, an Outside
stroke by the whole of it, and a drop shadow by its blur and spread — and the
packer resolves how far each reaches into `Instance::outset`, which the vertex
stage grows the quad by (D8, D9). An Inside stroke reaches nothing and takes a
zero outset. A masked instance is a different case: its quad is the coverage
field's padded plane quad substituted for the node's box.

A glyph run's instances are appended to the span of the rect the run is anchored
to, after that rect's inner shadows — the position `dashscene-skia` draws an
anchored run at, which puts the run inside the rect's clip region and inside
every enclosing group layer. The instance carries the glyph's rectangle in the
run's **source** atlas in that atlas's own texels; where residency put that
atlas is a device question, and the packer has no device, which is exactly what
keeps layer 1 device-free.

## Pipelines, bindings, and the four-storage-buffer wall

`docs/decisions/pipelines-and-layer-3.md` holds this painter to the entry-tier
floor `wgpu::Limits::downlevel_defaults` describes, which allows **four storage
buffers per shader stage**. The device is requested at
`downlevel_defaults().using_resolution(adapter.limits())` rather than at
`downlevel_defaults` itself: issue #714 aborted the host when a window larger
than downlevel's 2048 `max_texture_dimension_2d` was configured, so the
resolution limits come from the adapter and every other downlevel limit stays.
`Renderer::max_extent` is what a caller asks for the drawable ceiling, and
`check_extent` refuses past it rather than panicking inside
`Surface::configure`.

The storage-buffer count is the limit that shaped the crate. The paint
pipeline's bind group stands at:

    vertex    instances(0), glyph runs(8), shapes(9)          3 of 4
    fragment  paints(1), clips(2), strokes(4), images(5)      4 of 4

The fragment stage is full and the vertex stage has one slot free — free because
story #584 preferred to move a value onto the instance rather than spend it, not
because nothing wanted it (`docs/decisions/tables-the-vertex-stage-reads.md`,
whose D4 is revised to say exactly this). A value that fits on the instance
costs no binding at all, which is why a free slot is not an invitation.

That ceiling is the single strongest force on this crate's shape — it is why
three later features took the form they did:

- **Gradients** (issue #715). A gradient's stop array is indexed by a value the
  fragment stage computes, so it can cross as no varying, and that stage had no
  binding left. Solid colours and gradient rows share one storage buffer instead
  — the paint-parameter heap (`docs/decisions/the-paint-parameter-heap.md`).
- **Shadows** (story #584). They extend that same heap by a third region rather
  than adding a binding, and the quad growth a drop shadow needs moved onto
  `Instance::outset` because the vertex stage cannot read the heap.
- **Text and baked vector fields** (story #582). Both tables are read by the
  **vertex** stage, because the fragment stage has none left
  (`docs/decisions/tables-the-vertex-stage-reads.md`).

An instance whose kind the shader does not implement draws nothing, and does not
fall through to a colour: `InstanceKind` carries the sub-kind, so a shader
reading the discriminant alone cannot resolve a shadow against the solid-fill
table.

The signed-distance math lives in one file, `shader::SDF_WGSL`, and every
consumer includes that string rather than copying from it — the render pipelines
and the layer-2 conformance harness alike. That is R-T5's "SDF shader math
single-sourced into both painters' shading languages", reduced to the one
mechanism WGSL has, which is textual inclusion
(`docs/decisions/shader-library-and-layer-2.md`).

Clips follow GPUI's model rather than iced's: a per-instance clip region
evaluated in the shader, so a clip change does not break batching.

## Layers, and the two things a pass cannot do for itself

`composite.rs` turns one ordered instance stream into the passes that draw it.
Two features force more than one pass, and each for its own reason.

**Group opacity** (story #583). A group whose painted rects overlap cannot be
drawn by multiplying each rect's alpha — where two members overlap, the lower
shows through the upper. The subtree draws into an offscreen layer at full alpha
and the layer composites at the group's alpha, through a second pipeline, which
is the route anything sampling a rendered target has to take. A layer is the
**full target extent**, transparent-initialised, not the group's bounds: a
group's ink reaches past its rect range through shadows and blurs, so a tight
bound would have to be derived from the effects rather than the geometry. Groups
nest, and a layer closes into whatever was open around it
(`docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`).

**Backdrop blur** (story #733). A backdrop reads what is already in the render
target, which no binding on the paint pipeline can do and no pass can do for its
own attachment. The planner ends the pass at a backdrop instance, the renderer
snapshots the target between the two, and two more pipelines run a separable
Gaussian over the snapshot and write the result back. The target a backdrop
reads is the pass's own target — which is the correct reading of "the backdrop
beneath this node" when the node is inside a group layer
(`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`).

**An empty frame clears** (issue #1025). `composite::plan` returns one pass over
the frame's own target with no steps rather than no passes at all, so
`Renderer::draw` begins a render pass that loads by clearing and encodes nothing
into it. It planned nothing before: the loop ran no iterations and the final
`emit` was dropped by its own empty-range guard. `Renderer::render` asserts a
non-empty buffer, so no offscreen caller could reach it — but
`SurfaceRenderer::present` hands `draw` the swapchain view and presents
immediately after, so a document with no ink presented whatever the compositor
last had, or undefined contents on the first frame. The planner is what emits
it, because deciding who clears is what `Pass::clear` is for and a renderer
clearing on its own initiative would be a second such decision.

## Atlas residency

One mechanism serves three consumers: image fills, MSDF glyph atlases and baked
vector fields are all "a payload of some texel format that has to be somewhere a
shader can sample". The alternative — a texture per payload — needs either a
bind group per draw or a binding array, and a binding array is not in
`downlevel_defaults`.

There is one atlas per `AtlasFormat`, packed by `etagere` and evicted by recency
through an LRU, following glyphon's design. The two colour spaces of one block
footprint share an atlas, because this painter samples every payload through
wgpu's **Unorm** channel whatever the payload's declared colour space is: a
`*Srgb` view would have the sampler linearise on read, putting image texels in a
different space from every other colour in the shader
(`docs/decisions/atlas-residency-and-image-fills.md`).

Block-compressed formats constrain the allocation rather than the sampling: a
block texture's dimensions must be a multiple of its footprint, and 2048 is not
a multiple of four of the six ASTC footprints, so `usable_extent` trims the
atlas to what the footprint divides. A feature must also be requested from the
device, not merely advertised by the adapter.

### A payload the atlas cannot hold, and one this build cannot decode

Both used to abort the frame, and both were reachable from an ordinary document
rather than from a broken contract between crates (issues #720 and #718).

**Larger than the atlas is not larger than the device.** A payload that exceeds
`ATLAS_EXTENT` but fits `max_texture_dimension_2d` now gets a texture of its
own, sized to itself and rounded **up** to whole blocks where a shared atlas
rounds down. The machinery was already there: residency answers with a
`Slot { atlas, rect }`, each atlas carries its own extent, and the renderer
already binds one atlas per draw run and segments the instance buffer when a
frame needs more than one. A dedicated texture is another entry in that list
whose allocator is fully occupied by one payload, and `atlas_for`'s by-format
lookup skips it — a later payload matching it would evict the oversized one it
exists for and then still not fit. Only a payload past the device's own limit is
refused, which is a document no arrangement of textures on that adapter can
draw. A glyph atlas is the likeliest of the three consumers to reach the old
bound: one sheet for a whole script at a whole weight, so a CJK coverage set
exceeds 2048 square where an oversized photograph has to be authored
deliberately.

**A JPEG or GIF payload is refused by name.** This painter links one decoder and
that is `png`, for the reason the trim profile exists at all. `TexelPayload::of`
panicked on the other two, on the live
`resolve_frame → resident_image → Residency::resident` path, so a `.dsb` with a
JPEG image fill crashed the host — and `Painter::samples`, the declaration meant
to stop the payload arriving, has no production call site, so nothing read it
before binding.

**Where a refusal goes.** `Painter::paint` still returns nothing by decision, so
there is no channel back to the caller. What there is now is a channel _out_:
the row draws nothing and the refusal is recorded on `Renderer::refusals`,
naming the consumer, the row and the `ResidencyError`, with a monotonic
`Renderer::refusals_seen` beside `evictions` and `decodes` for a host that
samples rather than polls. That is what P4's "never a silent drop" asks for
without widening boundary B for every painter. Two larger changes stay open and
are not this: widening the painter's return type, and refusing the document at
load, which is the only shape that would give `Painter::samples` a caller and
make `baked-texel-payloads-cross-boundary-b.md` D6 true rather than decorative.

**A row that did not resolve keeps its instance out of every draw range** (issue
#1024). The flags below make the fragment stage discard, which is what makes the
picture right; they do not stop the quad being submitted. Until this change
`draw_runs` emitted ranges covering the whole buffer and dropped none, so a text
node whose atlas was refused still cost one instance per glyph — the vertex
stage, the rasterizer, `rounded_box_sdf`, the gate, `clip_coverage` and a
`discard`, per fragment — on every frame it was drawn, and `Residency` memoizes
the refusal so it never recovered. The likeliest refusal is a CJK coverage set,
one sheet for a whole script at a whole weight, which is also the run with the
most glyphs.

So the ranges are now an ordered, disjoint **subsequence** of the buffer rather
than a partition of it, and `Resolved::sampled_row` is the one place that says
which instances belong to it — three answers where `Resolved::atlas_of` gives
two, and both of `draw_runs`'s questions read it rather than dispatching on the
kind themselves. Order and disjointness are what draw order rests on and both
survive; the cover does not. Two consequences are stated rather than hidden. A
gap between two drawn stretches **splits one range into two**, so the frame
encodes one more `pass.draw` than it did — **one more per gap, not per frame**.
A document whose CJK sheet was refused has every text stretch unresolved, so a
screen interleaving fifty text nodes with fifty rects goes from one draw to
about fifty. That is still the trade this is for, fifty draws against the glyph
quads of fifty runs, but it is a trade rather than a saving, and it is why
`Renderer::draw` binds a run's atlas only when it differs from the last: a gap
splits a range without changing what either half samples, and before the gap
existed two consecutive runs always differed. And the whole-buffer answers
`draw_runs` takes when a frame's payloads all landed in one atlas need to know
there is no gap, which `Resolved::undrawn` carries out of `resolve_frame` rather
than costing a second walk over the instances on the path R-T4 bounds. That flag
is set at the three arms that leave a row unresolved — four until issue #1001
closed `Atlas`'s extent at its constructor and the painter's own guard went with
it — and those include a _degenerate_ coverage field, which is authored content
rather than a residency failure, so the population taking the walk is wider than
a refusal. A debug assertion derives the flag independently at the foot of
`resolve_frame`: three sites is three places to drift, and the derivation is
what catches it, compiled out of the frames the bound is about.

**All three resolved tables state the flag** (issues #972, #993 and #1023).
`GpuMsdfRow::resolved` — the tail `GpuShape` and `GpuGlyphRun` share since issue
#1027 — and `GpuImage::resolved` each say whether the row's other members
describe a payload this frame made resident, and each of the three shader arms
that reads such a row gates on it. The image row was the last to get one, and
until it did what emptied a refused fill's picture was `image_colour` returning
transparent black on `size.x <= 0.0` — a guard written for
`dashscene-validator`'s `asset.image-no-bytes` case, where boundary B stores a
payload whose binding supplied no bytes at 0 x 0 rather than refusing it. The
two coincided only because `resolve_frame` writes the row on the success path
alone, and the asset's extent is in hand two lines from the refusal.

That guard is now **unreachable from the fragment stage**, and it is worth
saying so rather than leaving it reading as a second case still covered. A 0 x 0
row is always an _unwritten_ row — `resident_image` answers `None` for a payload
with no extent, so the no-bytes payload leaves the row at `Default` exactly as a
refusal does — and the flag turns both away one call earlier. The guard stays as
`image_colour`'s own precondition, because every branch inside it divides by
`fill.size`.

The flag costs no bytes: `GpuImage` already carried two pad words and now
carries one, pinned by its own `offset_of!` block for the reason `GpuShape`'s
is. Those assertions constrain the Rust side; the WGSL side is pinned by
`the_image_arm_gates_on_the_row_the_frame_resolved`, which asserts `Image`'s
member order over the shader source. Both halves are needed, and neither swap
they admit is the obvious one — a refused row is `GpuImage::default()`, every
word zero, so no reordering can leave the gate _open_ on one. Swapping
`resolved` with `_pad` closes it permanently and no image fill in the document
draws at all, which any fixture sees; swapping it with `tile_scale` leaves the
gate working by accident and gives `tile_scale` the bit pattern of `1u32`,
1.4e-45, destroying every `SCALE_TILE` fill and nothing else. The second is the
quiet one, and no fixture in this crate tiles.

**"The row draws nothing" is a property of the resolved row, not of the
instance.** A coverage mask reaching this painter is resolved, degenerate —
whatever `field_draws` rejects before residency, which is a quad whose width or
height is not finite and positive, or a missing atlas rectangle — or refused.
`GpuMsdfRow::resolved` is what distinguishes the first from the other two, and
both consumers read it rather than inferring the mask from `Instance::shape`.
Inferring it is what made a refused field draw: an unresolved row carries a zero
`px_range`, and `msdf_coverage(sample, 0)` is `0.5` for every sample there is,
so the fill drew the node's ink at half alpha and the backdrop drew
half-strength frost — each over the antialiasing margin at the node's top-left
corner, since a zeroed plane has no area for the margin to grow (issue #972).
The flag is stated rather than derived from `px_range` for the reason
`blur.wgsl` gives against its own `masked`: a zero range is a degenerate field,
not an absent one, and a real field can take a value a sentinel would claim.

**This closes the route through an unresolved row and not the class.** A field
that _does_ resolve carrying `distance_range == 0` would reach the same
`msdf_coverage(sample, 0)` and paint the same half coverage, this time over the
field's whole plane rather than the antialiasing margin. `field_draws` does not
reject it, and neither painter guards it — the reference painter derives
`px_range` from the same operand — so the seam is `dashpaint`, where issue #964
put the matching guard on the atlas's two operands.

Issue #986 did that work: `PaintTable::push_with` now refuses a `distance_range`
that is not finite and greater than zero, and `PaintTable::shapes` is private
with `push_with` its only writer, so no such field reaches either painter. The
paragraph above therefore describes a route that is closed at the seam rather
than here. What is still open on this path is `atlas_rect` and `plane_bounds`:
both painters agree such a field draws nothing — `VectorField::draws` is one
method both call since issue #1144, and issue #1000's divergence is closed — but
no seam refuses either, which is the half of issue #1034 that stays open.

**An unresolved mask makes the backdrop encode nothing at all**, which is not
the same as encoding it unmasked. Unmasked means the parametric rounded box, so
clearing `GpuBlur::masked` would frost the node's whole box where the defect had
frosted a corner patch — a larger wrong picture, measured rather than reasoned
about. A baked-vector node's silhouette _is_ its field, so with no field there
is no region to frost.

**A refused backdrop is dropped before anything is allocated for it** (issue
#994). `backdrop_mask` decides it where `backdrop_masks` is built, which is
ahead of `BlurTargets::prepare`: a frame whose only backdrop is refused now
allocates **twelve fewer device objects** — three drawable-sized textures and
their three views, the base blit's bind group and its uniform, and the two
uniform buffers and two bind groups of the slot itself — and draws into the
caller's view rather than through `BlurTargets::base` and a full-target blit.
`BlurTargets::prepare` is where that inventory is derivable and the one place it
is written down. All of it is what a frame with no backdrop planned at all
already did, and `a_frame_whose_only_backdrop_is_refused_allocates_nothing`
measures the difference between the two.

That saving had a cost in the other direction, and issue #1020 is where it was
paid. `prepare` released the three textures for any frame it was prepared for no
backdrop at all, which since #994 includes a refused-only frame — so a refusal
that changed from frame to frame rebuilt all twelve objects on each change.
`ResidencyError::FrameExceedsAtlas` makes that reachable rather than
hypothetical: it is returned as a bare `Err` and is deliberately not memoized,
so it is decided per frame from what else that frame made resident and can
differ on every frame indefinitely.

**The release is now in two steps.** The per-backdrop uniforms and bind groups —
four objects each, the line of `prepare`'s inventory that scales — go on the
first frame prepared for none, because each names one backdrop's coverage atlas
view and that frame named no mask. The frame-wide base, snapshot, scratch and
blit survive `TARGET_GRACE_FRAMES` of them.

That constant is **one**, and what it buys is exact: a gap of one frame costs
the per-backdrop objects to come back from, and a gap of two or more costs those
and the frame-wide ones underneath them — four against twelve, for one backdrop.

Since issue #1055 it is shared. `LayerTargets` keys its own release on the same
counter through `Grace`, and had no grace at all before: a render-target group
that comes and goes on alternate frames rebuilt four objects per layer on every
change — a drawable-sized texture, its view, a uniform buffer and a bind group —
measured at four for one layer.

The two holders differ in **what** the grace covers, and that is a property
rather than a choice. It covers everything in `LayerTargets`, because a layer's
bind group names only that layer's own view and its own alpha buffer; it covers
the frame-wide half alone in `BlurTargets`, whose per-backdrop groups name a
coverage atlas view belonging to the residency set.

They also differ in what an extra frame of holding costs, and the layer half is
the larger of the two on a scene that uses it. The blur half holds three
drawable-sized textures whatever the scene does. The layer half holds one **per
layer**, and `dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` warns at
eight — so a scene at that warning holds around eight full-target textures for
one extra frame where a frosted scene holds three. That is the figure to weigh
if this constant is ever revisited, and it is the reason the release half is
pinned by a test rather than left to the hold's. No fixed number covers every
pattern, and this one is chosen for the pathological case: a refusal that
flickers frame to frame and never settles, which `FrameExceedsAtlas` can do
indefinitely. A refusal lasting two consecutive frames still releases, and that
is the intended answer rather than a shortfall — three drawable-sized textures
are about 24 MiB at 1920 x 1080, and a scene that has not frosted for two frames
is asking for them back.

The per-backdrop half is dropped on the first such frame because a bind group
there **can** name a coverage atlas view the frame did not name. Not all of them
do: a backdrop with no coverage mask binds the renderer-lifetime placeholder and
`bound_atlases` records `None` for it, so holding those would be sound and would
make an unmasked alternating backdrop free rather than four objects per change —
the showcase's own frosted panel is one. That saving is not taken: it needs a
second condition on the branch and a second claim about which bindings are safe
to keep, where dropping the lot needs neither.

`an_alternating_refusal_does_not_rebuild_the_frame_wide_targets` straddles the
boundary, and its two inequalities pin one half each. That returning from one
refused frame costs **less** than returning from two is what says the frame-wide
half was held — with the grace removed both cost twelve. That it costs **more
than nothing** is what says the per-backdrop half was not.

That split was also what kept the grace clear of issue #1050 while it was open.
It is closed now: `Renderer::forget_uploaded` drops everything naming an atlas
through `BlurTargets::forget_atlases` and `Frame::forget_atlases`, so no atlas
key — the blur's list of indices, or the paint pipeline's count — survives the
call that moves what those keys name. The frame-wide targets are held across a
document replacement, because nothing they name belongs to the residency set.

**`BlurTargets::bound_atlases` keys the rebuild per slot, not per backdrop**
(issue #1026), and that is correct for one reason: the atlas texture view is the
only per-backdrop entry in the blur layout. Since issue #994 the list holds only
the backdrops that draw, so two frames whose masked and refused backdrops swap
places both produce `[Some(0)]` and slot 0's group — built for the first — is
reused for the second. The snapshot, the scratch, the clips and the sampler are
frame-wide, and the `GpuBlur` uniform is rewritten from that backdrop's own
instance every frame, so the reuse binds exactly the right things. The hazard is
the next entry, which would bind the previous frame's value with no rebuild
triggered. `BLUR_BINDINGS` is what makes that fail rather than drift: both entry
arrays are declared at that length, so a seventh is a type error and whoever
adds one has to decide what the rebuild is keyed on first.

**The filter and the slot come from one list**, and that is the whole of why
this is safe. `BlurTargets` builds one bind-group pair per entry of
`backdrop_masks`, each binding that backdrop's own coverage atlas, and
`BlurTargets::pass` indexes them by position — so a filter applied in one place
and an ordinal counted in another are two records of one fact. When they
disagreed, every backdrop behind a refused one drew through the previous one's
mask, which for a refused field is the placeholder nothing writes: the next
node's frost vanished with no refusal recorded, which is the silent drop P4
forbids. `PlannedBackdrop::slot` is assigned by the same step that decides the
backdrop draws at all, so there is no second count to disagree with, and
`a_refused_backdrop_does_not_renumber_the_one_behind_it` is what fails when the
slot is taken from anywhere else — it panics on the bound now, where it drew a
wrong picture before.

**The entry list steps for a refused backdrop too**, because it holds one entry
per _planned_ backdrop and is what the slot is read out of. It is consumed as an
iterator rather than indexed by a counter, so taking an entry and advancing past
it are one step — a counter advanced in a separate statement is the shape the
defect above had.

It is also what says whether anything was encoded, which the render pass then
reads. Only the **first** pass on a target clears — `D5` of
`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`, and
`Pass::clear` is where the planner decides it — and when that pass also resolves
a backdrop the clear has to happen before the snapshot rather than at the pass,
which is why the pass then loads. A first pass whose backdrop was refused
encodes no clear of its own, so it clears at the pass exactly as it would with
no backdrop planned. Reading the plan rather than what was encoded leaves it
loading a texture nothing cleared, and
`a_refused_backdrop_does_not_leave_the_previous_frame_on_the_target` is what
sees it — the offscreen texture is held across `render` calls, so the frame
comes back carrying the previous one.

The third consumer, a glyph run, states the same thing in the same word (issue
#993) — the two share one `GpuMsdfRow` since #1027. It did not until then: the
row kept `GpuGlyphRun::default()`, whose zero alpha the text arm carried into a
colour it never discards on, so an empty picture was two defaults agreeing
across two files. The vertex stage now carries the flag into `params2.w` — the
component the masked path already uses for exactly this — and the `KIND_TEXT`
arm leaves the coverage at zero without it, so the fragment is discarded before
any colour is read. **Deleting that gate changes no rendered texel**, measured,
which is why `both_msdf_arms_gate_on_the_row_the_frame_resolved` reads the
shader source: it is the only thing that can fail on it.
`the_image_arm_gates_on_the_row_the_frame_resolved` is the same instrument on
the third table, for the same reason.

**The packer-contract checks run over every _planned_ backdrop**, not over the
ones that draw (issue #1022). `backdrop_blur_radius` is where they live and it
is called above the filter that drops a refused one, so a residency outcome
cannot decide whether a mismatch is named. They opened
`Renderer::resolve_backdrop` until issue #994 moved the refusal up past them and
left them covering only half the population.

The two are not the same guard, and only one of them is there in release. The
**row** check is a `panic!` — the mistake it names, a caller packing against one
`PaintTable` and drawing against another, draws a plausible wrong picture rather
than an absent one, which is why it is a release panic and why half a population
was the wrong half. The **kind** check is a `debug_assert_eq!`, because the plan
it reads is built inside this crate where the blur table crosses boundary B. An
embedded release build therefore runs one of the two. `PlannedBackdrop::radius`
carries the validated row's value to the backdrops that do draw, which is what
lets `resolve_backdrop` read it without looking the row up a second time.

**A degenerate coverage field is still named by no diagnostic** (issue #1021).
`field_draws` rejects it before residency, so `Renderer::refuse` is never
reached and the drop reaches no seam at all — where a _refused_ payload is
recorded and named. The diagnostic half belongs at the validator, which already
names `asset.image-no-bytes` for the sibling case and would settle both painters
at once where a check here names it for this one alone. Issue #1000 is the
divergence that has to be settled with it.

`a_degenerate_coverage_field_draws_nothing` pins the painter's half over both
consumers, and **the two rejections are not guarded by the same observable**.
Dropping the positive-extent half of `field_draws` changes the picture, and so
does dropping its finite half — six rows carry a non-finite quad extent, four
from an infinite bound and two from finite bounds whose difference overflows,
and without that term the frame takes five draws where one is correct (issue
#1034). Dropping the atlas-rectangle half changes no texel either consumer
draws, measured — the row goes out with a NaN `half_uv` and an infinite
`px_range`, and both arms come back at zero coverage and discard. What catches
that one is the draw count: a field that resolves plans the backdrop **and**
brings the masked fill naming it back into a range of its own, so the frame
encodes that instance, the blur's two passes and the base blit on top of the
halves, and `Renderer::last_draw_runs` goes from 1 to 5. The picture is not the
only observable, which is the reason that accessor is public.

**A third observable pins the order rather than the answer** (issue #1159).
`field_draws` is asked _before_ `resident_image`, which
`dashpaint::VectorField::draws` states as part of its contract: a field the
predicate rejects samples nothing, so making its atlas resident is pure waste.
Nothing held that — swapping the two operands left this whole crate's suite
green, and the test above is blind to it, because a degenerate field made
resident is refused by nothing and still leaves its row unresolved.
`a_field_that_draws_nothing_makes_no_atlas_resident` reads `Renderer::decodes`
over an **encoded** atlas, since the coverage fixtures here are baked and a
baked payload is never decoded. That bounds the claim: for a baked atlas the
waste is an upload rather than a decode, and no counter sees it today.

## Two targets, one device

`Renderer` draws into a texture view and reads it back; `SurfaceRenderer`
configures a swapchain and presents. The surface lives in this crate rather than
in the host because its format has to agree with the format the render pipeline
was built for, and those two live one field apart in `Renderer` — putting the
surface in the host would make that agreement something a caller has to hold
rather than something the type does. The host still owns the window and the
seam, and hands a window handle to `SurfaceRenderer::new` once
(`docs/decisions/the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md`).

The web target presents through this same type, but a canvas is not
interchangeable with a window at the type level: `wgpu` has a blanket
`From<W> for SurfaceTarget` for anything window-shaped and **none for a
canvas**, which must be wrapped as `SurfaceTarget::Canvas` explicitly. That
wrapping is why `for_canvas` exists here rather than in the host — it keeps a
web host from naming a `wgpu` type or taking a `wgpu` dependency, the same
property `demo` holds natively.

`present` returns `Drawn`, not `()`. A frame can decline to draw — a zero-extent
surface, a timeout, an occluded window — and a host that recorded the commit as
shown regardless would leave the dirty set stated against a commit that never
reached the screen.

`Changes` carries boundary B's advisory dirty set together with the generation
it was reported against, and `InstanceUpload` reports how the rows actually
reached the device (`Whole` or `Ranges`). That instrument is public because both
paths draw the same picture by design, so a test asserting only the picture
would pass just as happily if every frame quietly wrote the whole buffer.

## What this painter declares it can sample

`Painter::samples` is a capability declaration, meant to be asked before a
payload is bound rather than inside a frame, because `Painter::paint` is
infallible by decision
(`docs/decisions/baked-texel-payloads-cross-boundary-b.md`).

**Nothing asks it.** It has no production call site — every caller is a test,
and `GpuPainter::sampled_formats` has none at all — which is why a JPEG payload
reached the renderer and panicked there rather than being turned away at the
bind (issue #718). The declaration is correct and currently decorative;
`docs/technotes/implementing-a-backend.md` says the same. Until a host reads it,
the refusal described under "Atlas residency" below is the behaviour that
actually holds.

`SampledFormats` is the first override of the trait's default. It claims PNG but
neither JPEG nor GIF — this painter links one decoder, because the trim profile
whose existence justifies the crate removes `libpng`, `libjpeg` and `libwebp`
alike, and `dashpack` derives every canonical container away before a product
build ships. It claims RGBA8 in both colour spaces unconditionally, and the ASTC
block formats **only if the adapter advertises them**: ASTC is a device
capability, not a property of this crate, so the declaration is built from an
adapter and `Default` is the conservative answer for a painter that has met
none.

## Verification — four layers, and where the line is

CI runs on runners with no GPU, so fidelity is decomposed so that only the
smallest part needs real hardware.

| Layer                                           | What it catches                                                                           | GPU |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------- | --- |
| 1. Instance-buffer goldens, bit-exact           | wrong table-to-draw translation: dropped clip, wrong paint row, wrong z, wrong group      | no  |
| 2. Shader-math conformance, compute on lavapipe | wrong SDF distance, AA ramp, MSDF median-of-3 resolve, blurred-rounded-rect closed form   | no  |
| 3. Render smoke on lavapipe                     | pipelines, bind groups, formats, naga validation; coverage, clip rejection, group overlap | no  |
| 4. Perceptual band against the Skia CPU oracle  | how it actually looks on a real driver                                                    | yes |

Layers 1 to 3 are the gate. **Layer 4 is a measurement, not a gate**, and layer
3 is not a fidelity check — describing it as one would be exactly the
`t2-check-has-no-teeth` failure v0.13 exists to remove.

Compute-shader evaluation is what makes layer 2 trustworthy on lavapipe: it
removes the rasteriser, the antialiasing resolve, the blend stage and the
texture sampler, leaving float arithmetic. The same software rasteriser is
untrustworthy for layer 4 for the same reason.

Layers 1 and 2 are shared with the future Unity painter, which makes R-T5 an
executable conformance suite both painters run rather than a review promise.

**What layer 4 found was the opposite of what the slice expected.** The
breakdown predicted the bands would move — different antialiasing, different
gradient dithering, different blur falloff. Measured on an Apple M3 through
Metal, every one of the oracle's seven frames sits inside its existing band
through **both** painters, six of the seven agreeing to within 0.006 percentage
points. **One band set serves both painters**: no per-painter bands, no second
band set, and zero goldens moved
(`docs/decisions/one-band-set-serves-both-painters.md`). The corpus is split,
because the two candidate corpora measure different things — the band half runs
on the oracle's design-sourced frames, which makes it a fidelity measurement
against Figma's own render; the frame-cost half runs on the showcase scenes,
where the only thing measurable is parity between the two painters.

## Known limits

Stated rather than implied, each with the issue that carries it.

- **The packer walks every rect** (issue #708). `paint` ignores the dirty set
  and repacks the whole frame, which is always correct because the set is
  advisory — the set is honoured one level down, where it decides which byte
  ranges are uploaded. Repacking only the changed rects needs the previous
  frame's tables held for comparison. R-T4's upload half is met; its pack half
  is not.
- **JPEG and GIF are refused** (issue #718), by the trim-profile argument above
  rather than by omission.
- **The web target is WebGPU only.** A WebGL2 fallback is not buildable for this
  painter: `wgpu::Limits::downlevel_webgl2_defaults` allows **zero** storage
  buffers per shader stage, and this painter's whole design is storage-buffer
  tables. A fallback means a second shader variant expressing every table as a
  uniform buffer or a texture, with its own binding budget and its own
  conformance suite — a redesign, and a v1 question rather than a deferred task.
  A browser without WebGPU is told so and draws nothing.
- **The entry tier does not switch here.** Skia remains the entry-tier bridge
  until this painter is measured on a real entry SoC, and no such hardware is in
  the loop (epic #476).
- **No CI instrument catches "it looks wrong on a real automotive GLES
  driver."** Layer 4 is the only one, and it is not automated.

## Testing

`crates/dashscene-gpu/tests/` holds layers 1 to 3 — the gate. Layer 4 is not
there, and the last paragraph of this section says where it is:

- `layer1_instances.rs` with `tests/goldens/` — the bit-exact instance-buffer
  goldens, stated over the rows rather than the parameters behind them, since a
  wrong parameter is a defect in the table and boundary B's own tests own it.
- `layer2_conformance.rs` with `tests/shaders/conformance.wgsl` — the compute
  harness over `SDF_WGSL`.
- `layer3_render_smoke.rs`, `layer3_text_and_fields.rs`,
  `layer3_image_fills.rs`, `layer3_backdrop_blur.rs` — pipeline, bind group and
  format validation, plus coverage inside versus outside a shape, clip
  rejection, and group opacity where contents overlap.
- `frame_path.rs` — the incremental-upload path, asserted through
  `InstanceUpload` rather than through the picture.

`goldens/tooling/tests/lean_painter_text.rs` and `lean_painter_baked_assets.rs`
exercise this painter against real corpus assets from the golden side of the
tree, where those fixtures already live.

Layer 4 is not part of any tier and is not automated: it is run by hand from
`goldens/tooling/examples/layer4-band.rs` against the Skia CPU oracle, on
hardware named beside every number.

## Trace

- Satisfies: `docs/design/architecture.md` boundary B (the painter trait and its
  input), `docs/specification/03-target-hardware-rules.md` R-T4 and R-T5,
  `docs/roadmap.md`'s v0.15 slice; epic #569 and its stories' acceptance
  criteria.
- Design capture, gardened into this record and archived:
  `docs/archive/2026-07-19-wgpu-painter-direction.md` (the ecosystem research,
  the ruled-out crates, the pinned helper stack) and
  `docs/archive/2026-07-29-v014-v015-showcase-and-wgpu-wbs.md` (the work
  breakdown and the four-layer verification net).
- Related decisions: `docs/decisions/wgpu-is-the-lean-painter.md`,
  `docs/decisions/instance-buffer-contract.md`,
  `docs/decisions/shader-library-and-layer-2.md`,
  `docs/decisions/pipelines-and-layer-3.md`,
  `docs/decisions/tables-the-vertex-stage-reads.md`,
  `docs/decisions/the-paint-parameter-heap.md`,
  `docs/decisions/atlas-residency-and-image-fills.md`,
  `docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`,
  `docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`,
  `docs/decisions/one-band-set-serves-both-painters.md`,
  `docs/decisions/the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md`,
  `docs/decisions/baked-texel-payloads-cross-boundary-b.md`,
  `docs/decisions/blur-blends-in-srgb-encoded-space.md`,
  `docs/decisions/backend-tiering-unity-skia-lean.md`.
