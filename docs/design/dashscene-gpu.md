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

`dashscene-gpu` is the second implementation of the `Painter` trait
`dashpaint` defines (boundary B): instanced quads with analytic
signed-distance fields over `wgpu`, covering native and web from one
codebase. It is the lean painter of
`docs/decisions/backend-tiering-unity-skia-lean.md`, named for the role
rather than for its backend — the strategy record's contingency, if
`wgpu`'s GL backend fails on a target, is a direct-GLES backend written
over the same instance buffer and the same shaders
(`docs/decisions/wgpu-is-the-lean-painter.md`).

The whole v0 paint vocabulary draws through it: rounded rects with a
solid, gradient or image fill, their outline stroke, positioned glyph
runs, a fill masked by a baked vector field, both shadow kinds and the
backdrop blur — all clipped by their region, and render-target group
opacity as an offscreen layer composited at the group's alpha.

P2 binds it as it binds the reference painter: it only colours, and never
measures, wraps, kerns, or moves anything. There is no path primitive at
boundary B, and that absence is what makes this crate a translation of the
paint table into draw calls rather than a 2D rasteriser — every primitive
in the vocabulary maps onto an instanced quad with a fragment shader.

This crate does not replace `dashscene-skia`. Skia stays permanently as
the bit-exact CPU oracle and as the entry-tier bridge until this painter
is measured on a real entry SoC; what wgpu retires is Skia's trim profile
— the from-source GLES build, `skia_use_gl`, and the Ganesh-to-Graphite
churn watch.

## Public interface

Five types carry the crate, in `crates/dashscene-gpu/src/`:

    pub struct GpuPainter { /* private: an InstanceBuffer */ }

    impl Painter for GpuPainter {
        fn samples(&self, format: ImageFormat) -> bool;
        fn rotates(&self) -> bool;   // false — story #832
        fn paint(&mut self, rects, paints, images, clips, groups, glyphs, dirty);
    }

    impl GpuPainter {
        pub fn new() -> Self;              // claims no baked block format
        pub fn on(renderer: &Renderer) -> Self;   // claims what the device can sample
        pub fn instances(&self) -> &InstanceBuffer;
    }

`GpuPainter::paint` packs boundary B's tables into an `InstanceBuffer` and
**submits nothing**. That split is boundary B's own shape: a `Painter` is
handed tables and returns nothing, and a device is not part of that
contract. What draws the buffer is one of two renderers:

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

**The blocking constructors are native-only, and that is enforced rather
than documented.** `pollster::block_on` cannot succeed on a browser's main
thread — the promise it waits on resolves only by returning to the JS
event loop the wait is holding — so `new` is gated behind
`cfg(not(target_arch = "wasm32"))` and `pollster` is a native-only
dependency. A web host calls `new_async`, or `for_canvas` for a
`SurfaceRenderer`. `just wasm-painter` is the gate that keeps it that way.

`Residency` (`residency.rs`) holds the atlases both renderers sample, and
`Pass`/`Step` (`composite.rs`) is the pass plan they execute. The painter
holds its `InstanceBuffer` across frames rather than returning a fresh
one, so a steady-state frame repacks into an allocation it already has.

Two constants are part of the contract. `TARGET_FORMAT` is `Rgba8Unorm`
rather than `Rgba8UnormSrgb`, because this painter blends in sRGB-encoded
space and a `Srgb` format would have the hardware convert on write and
blend in linear light
(`docs/decisions/blur-blends-in-srgb-encoded-space.md`). `ATLAS_EXTENT` is
2048 — a memory budget rather than a ceiling, since an atlas is allocated
whole the first time a payload of its format appears, and 2048 square is
16 MiB of `Rgba8Unorm` where the 16384 an M3 reports would be 1 GiB. It is
applied as `ATLAS_EXTENT.min(max_extent)`, so on a device whose maximum is
smaller the atlas is smaller too. It is deliberately not the same question
as `Renderer::max_extent`, which is how large a _drawable_ the hardware
can address and is taken from the adapter.

## The instance buffer is the painter's output

`docs/specification/03-target-hardware-rules.md` R-T4 bounds per-frame CPU
cost to "dirty-range instance-buffer upload from the rect table +
submission. Nothing else." If that is the whole of the painter's frame,
then the instance buffer **is** what the painter produces, and the GPU is
a pure function of it and of the boundary-B tables its rows index.

That is why the largest class of painter defect — a dropped clip, a wrong
paint row, a wrong draw order, a group applied to the wrong set — is a
data defect, testable bit-exactly on a runner with no GPU. It is also why
`instance.rs` names no `wgpu` type: the struct is shared with the future
Unity painter, which epic #569 plans as instanced SDF quads too.

Every quad is one `Instance` in one ordered, kind-tagged stream, with an
`InstanceSpan` per rect naming that rect's range and a `Layer` per group
(`docs/decisions/instance-buffer-contract.md` D1). The per-node order
within a span is copied from `dashscene-skia` rather than re-derived from
boundary B, so both painters stack a node's parts identically (D5). Some
instances reach ink outside their own `bounds` — a Center stroke by half
its width, an Outside stroke by the whole of it, and a drop shadow by its
blur and spread — and the packer resolves how far each reaches into
`Instance::outset`, which the vertex stage grows the quad by (D8, D9). An
Inside stroke reaches nothing and takes a zero outset. A masked instance
is a different case: its quad is the coverage field's padded plane quad
substituted for the node's box.

A glyph run's instances are appended to the span of the rect the run is
anchored to, after that rect's inner shadows — the position
`dashscene-skia` draws an anchored run at, which puts the run inside the
rect's clip region and inside every enclosing group layer. The instance
carries the glyph's rectangle in the run's **source** atlas in that
atlas's own texels; where residency put that atlas is a device question,
and the packer has no device, which is exactly what keeps layer 1
device-free.

## Pipelines, bindings, and the four-storage-buffer wall

`docs/decisions/pipelines-and-layer-3.md` holds this painter to the
entry-tier floor `wgpu::Limits::downlevel_defaults` describes, which
allows **four storage buffers per shader stage**. The device is requested
at `downlevel_defaults().using_resolution(adapter.limits())` rather than
at `downlevel_defaults` itself: issue #714 aborted the host when a window
larger than downlevel's 2048 `max_texture_dimension_2d` was configured, so
the resolution limits come from the adapter and every other downlevel
limit stays. `Renderer::max_extent` is what a caller asks for the
drawable ceiling, and `check_extent` refuses past it rather than
panicking inside `Surface::configure`.

The storage-buffer count is the limit that shaped the crate. The paint
pipeline's bind group stands at:

    vertex    instances(0), glyph runs(8), shapes(9)          3 of 4
    fragment  paints(1), clips(2), strokes(4), images(5)      4 of 4

The fragment stage is full and the vertex stage has one slot free — free
because story #584 preferred to move a value onto the instance rather than
spend it, not because nothing wanted it
(`docs/decisions/tables-the-vertex-stage-reads.md`, whose D4 is revised to
say exactly this). A value that fits on the instance costs no binding at
all, which is why a free slot is not an invitation.

That ceiling is the single strongest force on this crate's shape — it is
why three later features took the form they did:

- **Gradients** (issue #715). A gradient's stop array is indexed by a
  value the fragment stage computes, so it can cross as no varying, and
  that stage had no binding left. Solid colours and gradient rows share
  one storage buffer instead — the paint-parameter heap
  (`docs/decisions/the-paint-parameter-heap.md`).
- **Shadows** (story #584). They extend that same heap by a third region
  rather than adding a binding, and the quad growth a drop shadow needs
  moved onto `Instance::outset` because the vertex stage cannot read the
  heap.
- **Text and baked vector fields** (story #582). Both tables are read by
  the **vertex** stage, because the fragment stage has none left
  (`docs/decisions/tables-the-vertex-stage-reads.md`).

An instance whose kind the shader does not implement draws nothing, and
does not fall through to a colour: `InstanceKind` carries the sub-kind, so
a shader reading the discriminant alone cannot resolve a shadow against
the solid-fill table.

The signed-distance math lives in one file, `shader::SDF_WGSL`, and every
consumer includes that string rather than copying from it — the render
pipelines and the layer-2 conformance harness alike. That is R-T5's
"SDF shader math single-sourced into both painters' shading languages",
reduced to the one mechanism WGSL has, which is textual inclusion
(`docs/decisions/shader-library-and-layer-2.md`).

Clips follow GPUI's model rather than iced's: a per-instance clip region
evaluated in the shader, so a clip change does not break batching.

## Layers, and the two things a pass cannot do for itself

`composite.rs` turns one ordered instance stream into the passes that draw
it. Two features force more than one pass, and each for its own reason.

**Group opacity** (story #583). A group whose painted rects overlap cannot
be drawn by multiplying each rect's alpha — where two members overlap, the
lower shows through the upper. The subtree draws into an offscreen layer
at full alpha and the layer composites at the group's alpha, through a
second pipeline, which is the route anything sampling a rendered target
has to take. A layer is the **full target extent**, transparent-initialised,
not the group's bounds: a group's ink reaches past its rect range through
shadows and blurs, so a tight bound would have to be derived from the
effects rather than the geometry. Groups nest, and a layer closes into
whatever was open around it
(`docs/decisions/group-opacity-draws-into-a-layer-and-a-second-pipeline-composites-it.md`).

**Backdrop blur** (story #733). A backdrop reads what is already in the
render target, which no binding on the paint pipeline can do and no pass
can do for its own attachment. The planner ends the pass at a backdrop
instance, the renderer snapshots the target between the two, and two more
pipelines run a separable Gaussian over the snapshot and write the result
back. The target a backdrop reads is the pass's own target — which is the
correct reading of "the backdrop beneath this node" when the node is
inside a group layer
(`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`).

## Atlas residency

One mechanism serves three consumers: image fills, MSDF glyph atlases and
baked vector fields are all "a payload of some texel format that has to be
somewhere a shader can sample". The alternative — a texture per payload —
needs either a bind group per draw or a binding array, and a binding array
is not in `downlevel_defaults`.

There is one atlas per `AtlasFormat`, packed by `etagere` and evicted by
recency through an LRU, following glyphon's design. The two colour spaces
of one block footprint share an atlas, because this painter samples every
payload through wgpu's **Unorm** channel whatever the payload's declared
colour space is: a `*Srgb` view would have the sampler linearise on read,
putting image texels in a different space from every other colour in the
shader (`docs/decisions/atlas-residency-and-image-fills.md`).

Block-compressed formats constrain the allocation rather than the sampling:
a block texture's dimensions must be a multiple of its footprint, and 2048
is not a multiple of four of the six ASTC footprints, so `usable_extent`
trims the atlas to what the footprint divides. A feature must also be
requested from the device, not merely advertised by the adapter.

## Two targets, one device

`Renderer` draws into a texture view and reads it back; `SurfaceRenderer`
configures a swapchain and presents. The surface lives in this crate
rather than in the host because its format has to agree with the format
the render pipeline was built for, and those two live one field apart in
`Renderer` — putting the surface in the host would make that agreement
something a caller has to hold rather than something the type does. The
host still owns the window and the seam, and hands a window handle to
`SurfaceRenderer::new` once
(`docs/decisions/the-host-selects-the-painter-and-the-frame-path-holds-its-buffers.md`).

The web target presents through this same type, but a canvas is not
interchangeable with a window at the type level: `wgpu` has a blanket
`From<W> for SurfaceTarget` for anything window-shaped and **none for a
canvas**, which must be wrapped as `SurfaceTarget::Canvas` explicitly.
That wrapping is why `for_canvas` exists here rather than in the host — it
keeps a web host from naming a `wgpu` type or taking a `wgpu` dependency,
the same property `demo` holds natively.

`present` returns `Drawn`, not `()`. A frame can decline to draw — a
zero-extent surface, a timeout, an occluded window — and a host that
recorded the commit as shown regardless would leave the dirty set stated
against a commit that never reached the screen.

`Changes` carries boundary B's advisory dirty set together with the
generation it was reported against, and `InstanceUpload` reports how the
rows actually reached the device (`Whole` or `Ranges`). That instrument is
public because both paths draw the same picture by design, so a test
asserting only the picture would pass just as happily if every frame
quietly wrote the whole buffer.

## What this painter declares it can sample

`Painter::samples` is a capability declaration, asked before a payload is
bound rather than inside a frame, because `Painter::paint` is infallible
by decision (`docs/decisions/baked-texel-payloads-cross-boundary-b.md`).

`SampledFormats` is the first override of the trait's default. It claims
PNG but neither JPEG nor GIF — this painter links one decoder, because the
trim profile whose existence justifies the crate removes `libpng`,
`libjpeg` and `libwebp` alike, and `dashpack` derives every canonical
container away before a product build ships. It claims RGBA8 in both
colour spaces unconditionally, and the ASTC block formats **only if the
adapter advertises them**: ASTC is a device capability, not a property of
this crate, so the declaration is built from an adapter and `Default` is
the conservative answer for a painter that has met none.

## Verification — four layers, and where the line is

CI runs on runners with no GPU, so fidelity is decomposed so that only the
smallest part needs real hardware.

| Layer                                           | What it catches                                                                           | GPU |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------- | --- |
| 1. Instance-buffer goldens, bit-exact           | wrong table-to-draw translation: dropped clip, wrong paint row, wrong z, wrong group      | no  |
| 2. Shader-math conformance, compute on lavapipe | wrong SDF distance, AA ramp, MSDF median-of-3 resolve, blurred-rounded-rect closed form   | no  |
| 3. Render smoke on lavapipe                     | pipelines, bind groups, formats, naga validation; coverage, clip rejection, group overlap | no  |
| 4. Perceptual band against the Skia CPU oracle  | how it actually looks on a real driver                                                    | yes |

Layers 1 to 3 are the gate. **Layer 4 is a measurement, not a gate**, and
layer 3 is not a fidelity check — describing it as one would be exactly
the `t2-check-has-no-teeth` failure v0.13 exists to remove.

Compute-shader evaluation is what makes layer 2 trustworthy on lavapipe:
it removes the rasteriser, the antialiasing resolve, the blend stage and
the texture sampler, leaving float arithmetic. The same software
rasteriser is untrustworthy for layer 4 for the same reason.

Layers 1 and 2 are shared with the future Unity painter, which makes R-T5
an executable conformance suite both painters run rather than a review
promise.

**What layer 4 found was the opposite of what the slice expected.** The
breakdown predicted the bands would move — different antialiasing,
different gradient dithering, different blur falloff. Measured on an Apple
M3 through Metal, every one of the oracle's seven frames sits inside its
existing band through **both** painters, six of the seven agreeing to
within 0.006 percentage points. **One band set serves both painters**: no
per-painter bands, no second band set, and zero goldens moved
(`docs/decisions/one-band-set-serves-both-painters.md`). The corpus is
split, because the two candidate corpora measure different things — the
band half runs on the oracle's design-sourced frames, which makes it a
fidelity measurement against Figma's own render; the frame-cost half runs
on the showcase scenes, where the only thing measurable is parity between
the two painters.

## Known limits

Stated rather than implied, each with the issue that carries it.

- **The packer walks every rect** (issue #708). `paint` ignores the dirty
  set and repacks the whole frame, which is always correct because the set
  is advisory — the set is honoured one level down, where it decides which
  byte ranges are uploaded. Repacking only the changed rects needs the
  previous frame's tables held for comparison. R-T4's upload half is met;
  its pack half is not.
- **JPEG and GIF are refused** (issue #718), by the trim-profile argument
  above rather than by omission.
- **The web target is WebGPU only.** A WebGL2 fallback is not buildable
  for this painter: `wgpu::Limits::downlevel_webgl2_defaults` allows
  **zero** storage buffers per shader stage, and this painter's whole
  design is storage-buffer tables. A fallback means a second shader
  variant expressing every table as a uniform buffer or a texture, with
  its own binding budget and its own conformance suite — a redesign, and a
  v1 question rather than a deferred task. A browser without WebGPU is
  told so and draws nothing.
- **The entry tier does not switch here.** Skia remains the entry-tier
  bridge until this painter is measured on a real entry SoC, and no such
  hardware is in the loop (epic #476).
- **No CI instrument catches "it looks wrong on a real automotive GLES
  driver."** Layer 4 is the only one, and it is not automated.

## Testing

`crates/dashscene-gpu/tests/` holds layers 1 to 3 — the gate. Layer 4 is
not there, and the last paragraph of this section says where it is:

- `layer1_instances.rs` with `tests/goldens/` — the bit-exact
  instance-buffer goldens, stated over the rows rather than the parameters
  behind them, since a wrong parameter is a defect in the table and
  boundary B's own tests own it.
- `layer2_conformance.rs` with `tests/shaders/conformance.wgsl` — the
  compute harness over `SDF_WGSL`.
- `layer3_render_smoke.rs`, `layer3_text_and_fields.rs`,
  `layer3_image_fills.rs`, `layer3_backdrop_blur.rs` — pipeline, bind
  group and format validation, plus coverage inside versus outside a
  shape, clip rejection, and group opacity where contents overlap.
- `frame_path.rs` — the incremental-upload path, asserted through
  `InstanceUpload` rather than through the picture.

`goldens/tooling/tests/lean_painter_text.rs` and `lean_painter_baked_assets.rs`
exercise this painter against real corpus assets from the golden side of
the tree, where those fixtures already live.

Layer 4 is not part of any tier and is not automated: it is run by hand
from `goldens/tooling/examples/layer4-band.rs` against the Skia CPU
oracle, on hardware named beside every number.

## Trace

- Satisfies: `docs/design/architecture.md` boundary B (the painter trait
  and its input), `docs/specification/03-target-hardware-rules.md` R-T4
  and R-T5, `docs/roadmap.md`'s v0.15 slice; epic #569 and its stories'
  acceptance criteria.
- Design capture, gardened into this record and archived:
  `docs/archive/2026-07-19-wgpu-painter-direction.md` (the ecosystem
  research, the ruled-out crates, the pinned helper stack) and
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
