//! The device, the pipeline, and the frame path (stories #580 and #585).
//!
//! # What this draws, and what it does not
//!
//! Rounded rects with a solid, gradient or image fill, their outline stroke,
//! positioned glyph runs, a fill masked by a baked vector field, and both
//! shadow kinds (story #584) — all clipped by their region, a render-target
//! group as a layer composited at its own alpha (story #583), and a backdrop
//! blur as a snapshot of the target run through a separable Gaussian
//! (story #733). That is the whole v0 paint vocabulary.
//!
//! A stroke and a drop shadow are the two kinds whose ink does not coincide
//! with the instance's own `bounds`. The packer resolves how far each reaches
//! into [`Instance::outset`](crate::Instance::outset), and the vertex stage
//! grows the quad by it, so an Outside stroke is not clipped by its own
//! geometry and a shadow's falloff is not cut off. A masked instance is a third
//! case of a different shape: its quad is the coverage field's padded plane
//! quad *instead of* the node's box, substituted in the vertex stage.
//!
//! An instance whose kind this shader does not implement draws nothing. It does
//! not fall through to a colour: [`InstanceKind`] carries
//! the sub-kind, so a shader that reads the discriminant alone cannot resolve a
//! shadow against the solid-fill table — the collision that made story #580
//! paint an inner shadow from `solids[shadow_row]` is unrepresentable now.
//!
//! # Two targets, one device
//!
//! This renderer draws into a texture view. Which view is the caller's:
//! [`Renderer::render`] makes its own offscreen one and reads the pixels back,
//! which is what lets layer 3 run as an ordinary test; [`crate::surface`] hands
//! it a window's swapchain texture, which is how the host draws (story #585).
//! Everything between the two — the device, the pipeline, the buffers and the
//! upload — is the same code, so the picture the host shows is drawn by the
//! path the tests exercise.
//!
//! # The frame path allocates nothing (R-T4)
//!
//! `docs/specification/03-target-hardware-rules.md` R-T4 bounds per-frame CPU
//! cost to "dirty-range instance-buffer upload from the rect table +
//! submission. Nothing else." Until story #585 this call allocated four
//! buffers, a texture, a view and a bind group **per frame**, because its only
//! caller rendered one frame and then dropped the renderer. It now holds them
//! across frames, grows them only when a frame outgrows one, and uploads only
//! the byte ranges the dirty rects name — see `Frame::upload_instances` for
//! the condition under which a partial upload is sound, and for the check that
//! fires when it is not.
//!
//! # Layer 3 is a gate on the pipeline, not a fidelity check
//!
//! `docs/decisions/shader-library-and-layer-2.md` draws the line and epic #569
//! insists on it: that pipelines build, that naga validates the modules, that
//! coverage is high inside a shape and zero outside it, and that a clip
//! rejects. None of that says how the painter looks on a real driver, which is
//! layer 4's job and needs hardware.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use dashpaint::{ClipTable, GlyphRunTable, ImageTable, PaintTable, ScaleMode};

use crate::composite;
use crate::instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan, Layer};
use crate::residency::{PayloadKey, Residency, ResidencyError};

/// The per-frame uniform the shaders read: the drawable, the antialiasing
/// width, and where the paint heap's gradient region begins.
///
/// It was `Viewport` and held only the first two. The third is what makes the
/// heap readable at all — a region base is a property of the frame's tables
/// rather than of any row — and once the struct carries it, "viewport" names
/// less than half of what is in it.
///
/// **Thirty-two bytes since story #584**, where it was sixteen for issue #715
/// and for every frame before it. A fifth member takes the struct to twenty
/// bytes, and a uniform-address-space struct's size rounds up to a multiple of
/// sixteen — so the three declared pad words below are what the shadow region's
/// base costs, and they are declared on both sides rather than left implicit.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Pod, Zeroable)]
struct Globals {
    size: [f32; 2],
    aa: f32,
    /// The first word of the paint heap's gradient region — see
    /// [`GRADIENT_WORDS`] and [`paint_heap`].
    ///
    /// Zero is a real base, reached whenever a frame has no solid fills at all,
    /// so nothing reads it as "absent". A frame with no gradients never indexes
    /// past it, because no instance names a gradient row.
    gradient_base: u32,
    /// The first word of the paint heap's shadow region, which follows the
    /// gradients — see [`SHADOW_WORDS`] and [`paint_heap`].
    ///
    /// A real base at zero for the same reason
    /// [`gradient_base`](Self::gradient_base) gives, and it coincides with that
    /// one exactly when the frame has neither solids nor gradients.
    shadow_base: u32,
    /// Declared padding to the sixteen-byte multiple a uniform binding needs.
    ///
    /// Three scalars, never one three-component vector on the WGSL side: such a
    /// vector aligns to sixteen there, so it would sit at offset 32 and take
    /// the struct to 48 while this one stayed at 32. That is the mismatch story
    /// #583 met in `GpuComposite`, where wgpu reported "bound with size 16
    /// where the shader expects 32".
    _pad: [u32; 3],
}

/// One stroke, in the shader's own layout.
///
/// `dashpaint::Stroke` is `{f32, StrokeAlign, Color}`, which is 24 bytes with a
/// Rust-layout enum in the middle. This is the std430 shape the shader reads:
/// the colour first so it sits at a 16-byte offset, then the width and the
/// alignment as a plain `u32`.
///
/// The alignment is mapped by an exhaustive `match` rather than by `as u32`
/// (see [`stroke_align`]). A copy through a local type rather than a cast, for
/// the reason [`GpuClipBox`] gives.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuStroke {
    color: [f32; 4],
    width: f32,
    align: u32,
    _pad: [u32; 2],
}

/// The value [`GpuStroke::align`] carries, and the one place the mapping is
/// written.
///
/// An exhaustive `match`, never `align as u32`: a reordered variant in
/// `dashpaint` would silently change the number a shader compares against, and
/// nothing would catch it — not the compiler, not a golden, which pins the
/// packer's output rather than the shader's reading of it. This is the same
/// hazard `InstanceKind` was merged into one enum to remove
/// (`crates/dashscene-gpu/src/instance.rs`). A new alignment is a compile error
/// here.
///
/// The numbers are the ones `sdf.wgsl`'s `stroke_coverage` documents, and the
/// ones its layer-2 conformance suite is stated over.
const fn stroke_align(align: dashpaint::StrokeAlign) -> u32 {
    match align {
        dashpaint::StrokeAlign::Inside => 0,
        dashpaint::StrokeAlign::Center => 1,
        dashpaint::StrokeAlign::Outside => 2,
    }
}

/// One image fill, in the shader's own layout, with its residency slot resolved
/// into it.
///
/// `dashpaint::ImageFill` names an image-table index; this names a rectangle of
/// the atlas that index was made resident in. That resolution is the whole of
/// what residency adds to a frame, and it happens once per table row rather
/// than once per instance.
///
/// The extent comes from the payload rather than from the slot, so that the two
/// cannot disagree: a slot's rectangle is where the texels are, and `size` is
/// how many of them there are.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuImage {
    /// The payload's rectangle in its atlas, normalised: `[u0, v0, du, dv]`.
    uv: [f32; 4],
    /// `Mat23`'s linear part, row-major: `[a, b, c, d]`.
    transform: [f32; 4],
    /// `Mat23`'s translation.
    translate: [f32; 2],
    /// The payload's extent in texels.
    size: [f32; 2],
    scale_mode: u32,
    tile_scale: f32,
    _pad: [u32; 2],
}

/// The value [`GpuImage::scale_mode`] carries.
///
/// An exhaustive `match`, never `mode as u32`, for the reason [`stroke_align`]
/// gives: a reordered variant in `dashpaint` would change the number the shader
/// compares against and nothing would catch it.
const fn scale_mode(mode: ScaleMode) -> u32 {
    match mode {
        ScaleMode::Fill => 0,
        ScaleMode::Fit => 1,
        ScaleMode::Crop => 2,
        ScaleMode::Tile => 3,
    }
}

/// The render entry points, as source — concatenated after
/// [`crate::SDF_WGSL`], which holds the math they share with the layer-2
/// conformance harness.
///
/// Named rather than inlined at the one call site so that a test can read it,
/// which is what pins the two constants this file and the shaders both state.
const PAINT_WGSL: &str = include_str!("shaders/paint.wgsl");

/// The composite pipeline's whole source: no SDF math, no sampler, its own
/// `@group(0)`. See the file for why it is separate from [`PAINT_WGSL`].
const COMPOSITE_WGSL: &str = include_str!("shaders/composite.wgsl");

/// The backdrop blur's two entry points (story #733) — concatenated after
/// [`crate::SDF_WGSL`], which it reads `rounded_box_sdf` and `coverage` from,
/// and its own `@group(0)`. See the file for why a backdrop cannot be drawn by
/// the paint pipeline at all.
const BLUR_WGSL: &str = include_str!("shaders/blur.wgsl");

/// The distance, in document units, over which an antialiased edge ramps.
///
/// One unit at unit scale, which is what `globals.aa` carries to the paint
/// pipeline. Named since story #733, because the blur pipeline binds no
/// `Globals` and has to state the same number — and a number stated twice with
/// nothing holding the copies together is the failure the scale-mode and
/// gradient-kind tests exist to catch.
const AA_WIDTH: f32 = 1.0;

/// How many sigmas of a Gaussian a blur's kernel spans on each side.
///
/// Three, where the weight has fallen to about 1.1 % of the peak and the
/// truncated tail is under 0.3 % of the total. The reference painter states no
/// equivalent — Skia picks its own window from the sigma — so this is this
/// painter's approximation of an unbounded kernel rather than a term either
/// side of boundary B carries, and story #586 is where the difference is
/// measured rather than argued about.
const BLUR_SUPPORT_SIGMAS: f32 = 3.0;

/// The value the paint heap carries for a gradient's kind, and the one place
/// the mapping is written.
///
/// An exhaustive `match`, never `kind as u32`, for the reason [`stroke_align`]
/// gives.
const fn gradient_kind(kind: dashpaint::GradientKind) -> u32 {
    match kind {
        dashpaint::GradientKind::Linear => 0,
        dashpaint::GradientKind::Radial => 1,
        dashpaint::GradientKind::Angular => 2,
        dashpaint::GradientKind::Diamond => 3,
    }
}

/// How many `vec4f` words one gradient occupies in the paint heap.
///
/// Two for the frame and its discriminant, two for the eight stop offsets, and
/// one per stop colour:
///
/// ```text
/// +0        (origin.x, origin.y, primary.x, primary.y)
/// +1        (secondary.x, secondary.y, kind, stop count)
/// +2        stop offsets 0..3
/// +3        stop offsets 4..7
/// +4 .. +11 stop colours 0..7
/// ```
///
/// **A fixed stride rather than a packed one.** A gradient row's words are then
/// `gradient_base + row * GRADIENT_WORDS`, and the fragment stage finds its
/// stops by arithmetic — where a packed layout would need the row's own offset,
/// which is one more storage read per gradient fragment or one more table. It
/// costs 192 bytes per *interned* gradient, and gradients are interned whole
/// (`dashpaint::PaintTable::intern_fill`), so a scene with a hundred distinct
/// ones spends 19 KiB. The trade is deliberately on the fragment stage's side.
///
/// The stop slots past a gradient's own count are written as zeroes and never
/// read: `gradient_ramp` walks `count` of them. Zeroes rather than a repeat of
/// the last stop, so that a shader reading past the count paints transparent
/// black — a visible absence — rather than a plausible colour.
const GRADIENT_WORDS: usize = 4 + dashpaint::MAX_GRADIENT_STOPS;

/// How many `vec4f` words one shadow occupies in the paint heap.
///
/// One for the geometry the painter resolves per instance, one for the colour:
///
/// ```text
/// +0   (offset.x, offset.y, sigma, spread)
/// +1   the shadow's colour
/// ```
///
/// **The sigma, not the authored blur radius.** The mapping between them is
/// [`dashpaint::BLUR_SIGMA_PER_RADIUS`] — Figma's measured constant, which the
/// reference painter fitted (`docs/decisions/blur-sigma-is-figmas-mapping.md`)
/// — and applying it once on this side keeps the number out of the shader
/// entirely. A shader that multiplied by its own copy would be a second home
/// for a measured value, which is the drift `stroke_align` and `gradient_kind`
/// are written the way they are to avoid.
///
/// The **kind** is not on the row. A shadow instance carries it in
/// [`InstanceKind`], where a drop and an inner shadow are separate variants, so
/// the fragment stage knows which coverage to build before it reads the row at
/// all — and `ShadowKind`'s own discriminant never crosses into the shader,
/// which is what makes the tag collision that once painted a shadow from the
/// solid table unrepresentable.
const SHADOW_WORDS: usize = 2;

/// The paint heap and where each region past the first begins.
///
/// Two bases of the same type, and a tuple would let a call site swap them. The
/// symptom of that swap is a *plausible* picture rather than an absent one —
/// every shadow reads a gradient's handles as its geometry — so it is worth a
/// named field to make unrepresentable.
struct PaintHeap {
    /// Every fill and effect parameter the fragment stage reads, in one array.
    words: Vec<[f32; 4]>,
    /// The first word of the gradient region.
    gradient_base: u32,
    /// The first word of the shadow region.
    shadow_base: u32,
}

/// The paint heap and the first word of each region past the first: every fill
/// and effect parameter the fragment stage reads as one array of `vec4f` words.
///
/// # Why one buffer holds three tables
///
/// `wgpu::Limits::downlevel_defaults` allows four storage buffers per shader
/// stage, and the fragment stage already read four — solids, clips, strokes,
/// images. A gradient needs its rows *and* its stop array there, and a stop is
/// looked up by a normalised position the fragment computes from its own
/// coordinate, so neither can cross as a varying the way story #582's tables do
/// (`docs/decisions/tables-the-vertex-stage-reads.md` D2). Two more bindings
/// against zero free slots is not a thing that can be arranged, so the tables
/// share one binding instead.
///
/// The shadow region is story #584's, and it arrived under the same constraint
/// with the answer already settled: a fragment-side parameter table extends this
/// heap rather than looking for a binding there is not
/// (`docs/decisions/the-paint-parameter-heap.md`). It costs one more base in
/// [`Globals`], which is what took that uniform from sixteen bytes to
/// thirty-two.
///
/// The solids region is first and at base zero, so a solid fill's word is still
/// `heap[row]` and that path is unchanged. The gradient region follows, then the
/// shadows; where each begins travels in [`Globals`] because both move with the
/// counts of the regions before them.
///
/// **The strokes and images tables stay where they are.** Folding them in would
/// free no binding — the stage would still read a heap and the clips — and it
/// would rewrite two shipped paths for nothing.
///
/// # Panics
///
/// Panics on a gradient carrying more than `dashpaint::MAX_GRADIENT_STOPS`
/// stops. `dashscene-skia` asserts the same bound with the same reason: the
/// ceiling is a vocabulary rule that `dashscene-validator` reports by name
/// (`paint.gradient.stop-budget`, P4), so a scene that reaches a painter with
/// more has already been refused upstream. Refusing it loudly here rather than
/// storing the first eight keeps the two painters agreeing about what happens
/// next.
fn paint_heap(paints: &PaintTable) -> PaintHeap {
    let mut heap: Vec<[f32; 4]> = paints
        .all_solids()
        .iter()
        .map(|c| [c.r, c.g, c.b, c.a])
        .collect();
    let gradient_base = heap.len() as u32;
    heap.reserve(paints.all_gradients().len() * GRADIENT_WORDS);

    for gradient in paints.all_gradients() {
        let stops = paints.stops(gradient);
        assert!(
            stops.len() <= dashpaint::MAX_GRADIENT_STOPS,
            "gradient stop budget exceeded: {} stops, budget {} (validated upstream, P4)",
            stops.len(),
            dashpaint::MAX_GRADIENT_STOPS
        );
        let (origin, primary, secondary) = (
            gradient.handle_origin,
            gradient.handle_primary,
            gradient.handle_secondary,
        );
        heap.push([origin.x, origin.y, primary.x, primary.y]);
        // The kind and the count as floats. Both are small non-negative
        // integers — the kind is one of four and the count is at most eight —
        // so an `f32` holds either exactly and the shader's `u32()` recovers
        // it. Stated rather than bitcast so that a heap dumped for a person to
        // read shows a 2 where the kind is Angular.
        heap.push([
            secondary.x,
            secondary.y,
            gradient_kind(gradient.kind) as f32,
            stops.len() as f32,
        ]);

        let mut offsets = [0.0f32; dashpaint::MAX_GRADIENT_STOPS];
        for (slot, stop) in offsets.iter_mut().zip(stops) {
            *slot = stop.offset;
        }
        heap.push([offsets[0], offsets[1], offsets[2], offsets[3]]);
        heap.push([offsets[4], offsets[5], offsets[6], offsets[7]]);

        for slot in 0..dashpaint::MAX_GRADIENT_STOPS {
            let colour = stops
                .get(slot)
                .map(|stop| [stop.color.r, stop.color.g, stop.color.b, stop.color.a])
                .unwrap_or([0.0; 4]);
            heap.push(colour);
        }
    }

    // The stride the shader indexes by, held against the stride this function
    // wrote. Not a restatement of the pushes above: it is the one place the two
    // meet, and a row written a word short would leave every gradient after it
    // reading the previous one's stop colours as handles — a plausible picture,
    // which no coverage assertion catches.
    assert_eq!(
        heap.len() - gradient_base as usize,
        paints.all_gradients().len() * GRADIENT_WORDS,
        "the gradient region must be {GRADIENT_WORDS} words per row"
    );

    let shadow_base = heap.len() as u32;
    heap.reserve(paints.all_shadows().len() * SHADOW_WORDS);
    for shadow in paints.all_shadows() {
        // The offset and the spread verbatim, the blur radius through the one
        // mapping both painters share. The kind is not written: the instance
        // carries it, and a `ShadowKind` discriminant on a row beside a
        // `PaintTag` one is the collision `InstanceKind` was merged to remove.
        heap.push([
            shadow.offset.x,
            shadow.offset.y,
            crate::pack::blur_sigma(shadow.blur),
            shadow.spread,
        ]);
        let colour = shadow.color;
        heap.push([colour.r, colour.g, colour.b, colour.a]);
    }

    // The shadow stride, held the same way the gradient stride above is, and
    // for the same reason: a row written short leaves every shadow after it
    // reading the previous one's colour as its geometry, which draws a shadow
    // rather than nothing.
    assert_eq!(
        heap.len() - shadow_base as usize,
        paints.all_shadows().len() * SHADOW_WORDS,
        "the shadow region must be {SHADOW_WORDS} words per row"
    );

    // An empty table would make a zero-sized binding, which wgpu refuses, so
    // one dead word stands in — no instance can name it, because an instance's
    // row comes from the table it was packed against.
    if heap.is_empty() {
        heap.push([0.0; 4]);
    }
    PaintHeap {
        words: heap,
        gradient_base,
        shadow_base,
    }
}

/// One glyph run, in the shader's own layout, with its atlas's residency slot
/// resolved into it.
///
/// Per run rather than per glyph: the colour, the screen-pixel range and the
/// atlas mapping are constant across a run, and the one thing that is not — the
/// glyph's own rectangle — rides on [`Instance::corners`], which a glyph has no
/// other use for.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuGlyphRun {
    /// The run's fill colour, with its free-path alpha still on
    /// [`Instance::opacity`]. The MSDF coverage modulates it.
    color: [f32; 4],
    /// Source-atlas texels to residency-atlas normalised coordinates:
    /// `[origin_u, origin_v, scale_u, scale_v]`, so texel `t` of the run's own
    /// atlas sits at `origin + t * scale`.
    ///
    /// Two mappings composed on the CPU rather than two in the shader: the
    /// atlas's own extent normalises the texel, and the residency slot places
    /// that normalised point inside the atlas texture. Both are constant per
    /// run.
    uv: [f32; 4],
    /// Half a source texel, in residency-atlas normalised units — what a sample
    /// is held inside the glyph's own rectangle by.
    ///
    /// Before [`px_range`](Self::px_range), not after it. WGSL aligns a `vec2f`
    /// to eight bytes, so the other order puts it at offset 40 with a hole at
    /// 36 and rounds the struct to 64 — while Rust packs it at 36 and makes the
    /// struct 48. Every row after the first would then be read from the wrong
    /// offset. The `size_of` assertion at the foot of this file is what holds
    /// the Rust half of that; this ordering is the WGSL half.
    half_uv: [f32; 2],
    /// The field's range in **screen** pixels:
    /// `distance_range_px * size / px_per_em`, which is `dashscene-skia`'s own
    /// formula. The painter draws at unit scale, so the run's size in document
    /// units is its size in pixels.
    px_range: f32,
    _pad: f32,
}

/// One baked-vector coverage mask, in the shader's own layout, with its atlas's
/// residency slot resolved into it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuShape {
    /// The padded field quad in shape space, node-box-relative and y-down:
    /// `[left, top, right, bottom]`, straight from
    /// `dashpaint::VectorField::plane_bounds`. The device quad is the node's
    /// origin plus this, at unit scale.
    plane: [f32; 4],
    /// The shape's sub-rect in its residency atlas, normalised:
    /// `[u0, v0, du, dv]`.
    uv: [f32; 4],
    /// Half an atlas texel, in residency-atlas normalised units. Before
    /// `px_range` for the alignment reason [`GpuGlyphRun::half_uv`] gives.
    half_uv: [f32; 2],
    /// The field's range in screen pixels: `distance_range` scaled by the
    /// device pixels one atlas texel covers. That scale is the field's own
    /// quad over its atlas rectangle — a vector field carries no `px_per_em`,
    /// because that ratio already is the scale.
    px_range: f32,
    /// Non-zero when the four members above describe a field this frame
    /// actually made resident (issue #972).
    ///
    /// **The third state a coverage mask has**, and the one that was missing.
    /// A row is zeroed both when a field is degenerate — no quad, or no atlas
    /// rectangle, which [`field_draws`] rejects before residency — and when its
    /// payload was *refused*, and neither draws. What made that a defect rather
    /// than a saving is that both consumers inferred "this instance is masked"
    /// from [`Instance::shape`] alone: a zeroed row then means `px_range = 0`,
    /// and `msdf_coverage(sample, 0)` is `0.5` for every sample there is. Half
    /// coverage over the antialiasing margin, on both pipelines.
    ///
    /// Stated rather than inferred, for the reason `blur.wgsl` gives against
    /// its own `masked`: a zero `px_range` is a degenerate field and not an
    /// absent one, and inferring absence from a value a real field could take
    /// is how a sentinel goes wrong.
    resolved: u32,
}

/// One clip box, in the shader's own layout.
///
/// `dashpaint::ClipBox` is `{f32 x4, CornerRadii}` and is already exactly this
/// shape, but it is copied through a local type rather than cast: boundary B's
/// row is a contract with every painter, and a std430 array stride is this
/// painter's business. Tying them together would make a layout change in one a
/// silent change in the other.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
struct GpuClipBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    corners: [f32; 4],
}

/// A device, a queue, the one pipeline, and the buffers a frame reuses.
pub struct Renderer {
    /// Held because a [`wgpu::Surface`] is created from it and must not outlive
    /// it. The offscreen path needs it only to build the adapter, but keeping
    /// it here means there is one lifetime rule rather than two.
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    /// The pipeline that blends a render-target group's layer into the target
    /// around it, and the layout its bind group is built from (story #583).
    ///
    /// A second pipeline rather than another [`InstanceKind`], and that is the
    /// general answer for anything this painter has to *sample a rendered
    /// target* for. The paint pipeline binds seven storage buffers across two
    /// stages that allow four each — four and four, with nothing spare — so a
    /// composite folded into it would need an eighth binding that does not
    /// exist. A pipeline owns its own bind group layout, so this one costs the
    /// paint pipeline nothing at all. Story #733's backdrop blur took the same
    /// route, in two more pipelines of its own — and it reuses this one for a
    /// second job, blitting the frame's own base target into the caller's view
    /// at alpha one.
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    /// The two pipelines a backdrop blur draws through — one per axis of the
    /// separable kernel — and the layout their bind groups are built from
    /// (story #733).
    ///
    /// Two pipelines rather than one with a flag, because they differ in more
    /// than a uniform: the axis pass writes a blurred colour and the resolve
    /// pass writes the finished destination, mixed against the sharp original
    /// it samples. They share one bind group layout, so a bind group built for
    /// either works with the other.
    blur_axis_pipeline: wgpu::RenderPipeline,
    blur_resolve_pipeline: wgpu::RenderPipeline,
    blur_layout: wgpu::BindGroupLayout,
    /// The layer textures a frame's render-target groups draw into, held across
    /// frames for the reason the offscreen target is, and rebuilt when the
    /// extent or the layer count changes.
    layers: LayerTargets,
    /// The targets and parameters a frame's backdrop blurs draw through, and —
    /// for a frame that holds one — the frame's own target (story #733).
    blurs: BlurTargets,
    adapter_info: wgpu::AdapterInfo,
    frame: Frame,
    /// The colour format the pipeline writes. [`TARGET_FORMAT`] offscreen, the
    /// window's own format behind a surface — both sRGB-encoded, which is what
    /// `docs/decisions/pipelines-and-layer-3.md` D3 requires.
    format: wgpu::TextureFormat,
    /// The offscreen target [`Renderer::render`] draws into, kept across calls
    /// for the same reason the frame buffers are, and rebuilt when the extent
    /// changes.
    offscreen: Option<Offscreen>,
    /// The largest either dimension of a drawable may be on this device — the
    /// device's own `max_texture_dimension_2d`. Copied out at construction
    /// rather than read per call: it cannot change, and `wgpu::Device::limits`
    /// returns the whole limit set by value.
    max_extent: u32,
    /// Device objects allocated for the offscreen target, counted beside
    /// [`Frame::allocations`] — see [`Renderer::allocations`].
    offscreen_allocations: u64,
    /// Which payloads are on the device and where (story #581).
    residency: Residency,
    /// Payloads this frame could not make resident, named rather than dropped
    /// in silence (issues #718 and #720). Cleared at the start of every
    /// `resolve_frame`, so it describes the frame most recently resolved.
    refusals: Vec<Refusal>,
    /// How many refusals this renderer has recorded since it was built, which
    /// no per-frame list can answer. Monotonic, like `evictions` and `decodes`
    /// beside it: a host that samples once a second still sees that something
    /// was refused, and a test can assert on zero without polling every frame.
    refusals_seen: u64,
    /// The `(consumer, row)` pairs already refused in the frame being resolved,
    /// so one refused payload is one refusal however many instances name it.
    refused_this_frame: std::collections::HashSet<(&'static str, u32)>,
    /// The sampler an image fill's payload is read through: nearest, clamped.
    /// See [`crate::residency`] for why nearest, and for what changing it costs.
    sampler: wgpu::Sampler,
    /// The sampler an MSDF payload — a glyph atlas or a coverage mask — is read
    /// through: linear, clamped. Built where it is, with the reason.
    msdf_sampler: wgpu::Sampler,
    /// A 1x1 texture bound when a frame samples no atlas at all.
    ///
    /// A bind group must name a texture for every texture binding its layout
    /// declares, and a frame with no image fills has no atlas to name. Building
    /// a second pipeline for the textureless case would be a second thing to
    /// keep in step with the first.
    placeholder: wgpu::TextureView,
}

/// What a renderer could not be built for, or could not be asked to draw.
#[derive(Debug)]
pub enum RendererError {
    /// No adapter at all — a machine or a runner with no GPU and no software
    /// device installed.
    NoAdapter,
    /// An adapter that will not give a device at the limits this painter needs.
    NoDevice(wgpu::RequestDeviceError),
    /// The window handle produced no surface.
    NoSurface(wgpu::CreateSurfaceError),
    /// Every format the surface offers converts to sRGB in the hardware, so
    /// blending would happen in linear light.
    ///
    /// Refused rather than accepted, because
    /// `docs/decisions/pipelines-and-layer-3.md` D3 makes the blending space a
    /// term of the contract and measures the two spaces roughly 50 code points
    /// apart across a saturated seam. A picture that is wrong in a way nobody
    /// named is worse than a window that did not open.
    NoLinearFormat(Vec<wgpu::TextureFormat>),
    /// A drawable larger on either axis than [`Renderer::max_extent`], the
    /// maximum this device can address.
    ///
    /// Reported *before* the call that would fail rather than caught after it,
    /// because there is nothing to catch. Both `wgpu::Surface::configure` and
    /// `wgpu::Device::create_texture` raise a validation error for an
    /// over-large extent, and a wgpu validation error reaches the uncaptured
    /// error handler, which panics; inside the swapchain configure that panic
    /// is non-unwinding and takes the process down with it. Issue #714 aborted
    /// the showcase host that way on an ordinary window resize.
    Extent { width: u32, height: u32, max: u32 },
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::NoAdapter => write!(
                f,
                "no wgpu adapter is available; on a runner this means no software device is \
                 installed (CI installs mesa-vulkan-drivers)"
            ),
            RendererError::NoDevice(e) => write!(f, "the adapter provided no device: {e}"),
            RendererError::NoSurface(e) => write!(f, "the window provided no surface: {e}"),
            RendererError::NoLinearFormat(offered) => write!(
                f,
                "the surface offers only sRGB-converting formats ({offered:?}); this painter \
                 blends in sRGB-encoded space and has no format to do it in"
            ),
            RendererError::Extent { width, height, max } => write!(
                f,
                "a {width}x{height} drawable exceeds the {max} px maximum this device can \
                 address on either dimension"
            ),
        }
    }
}

impl std::error::Error for RendererError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RendererError::NoDevice(e) => Some(e),
            RendererError::NoSurface(e) => Some(e),
            RendererError::NoAdapter
            | RendererError::NoLinearFormat(_)
            | RendererError::Extent { .. } => None,
        }
    }
}

/// What a caller knows about how this frame differs from the last one.
///
/// # Why the generation travels with the dirty set
///
/// A dirty set names the rects whose entry differs **from the commit before
/// it**. That makes a partial upload sound only when the device holds the
/// commit immediately before this one — and a host cannot promise that. Story
/// #585's own presenter breaks it three ways: a swapchain acquire can time out,
/// a window can be occluded, and a minimised window has no drawable. Each of
/// those declines a frame while the host still records the commit as shown, and
/// the next commit's dirty set will not mention what the declined one changed.
///
/// It is not a theoretical gap. It was found by running the showcase for two
/// minutes: a spring's *last* step landed on a declined frame, the value then
/// converged and never changed again, and the device kept a rect 0.02 units too
/// narrow with no later frame that could correct it. Invisible in the picture,
/// permanent, and caught only because the renderer checks itself.
///
/// Carrying the generation makes the gap unrepresentable rather than forbidden:
/// the renderer applies ranges only when this frame is the immediate successor
/// of the one on the device, and writes everything otherwise. A caller that
/// skips a frame, restarts an arena, or hands over frames out of order gets a
/// correct picture without having to know that it did any of those things.
#[derive(Debug, Clone, Copy)]
pub struct Changes<'a> {
    /// Boundary B's advisory dirty set: sorted rect indices whose entry differs
    /// from the previous commit's.
    pub rects: &'a [u32],
    /// The commit these rects were reported against —
    /// `dashscene_core::CommittedScene::generation`.
    pub generation: u64,
}

/// How one frame's instance rows reached the device.
///
/// Public because it is the instrument the frame-path tests are stated over,
/// and there is no other way to tell the two paths apart from outside: both
/// draw the same picture, which is the whole point of the partial one. A test
/// that asserted only the picture would pass just as happily if every frame
/// quietly wrote the whole buffer — the exact green-for-the-wrong-reason this
/// crate has already been caught by twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceUpload {
    /// Every row was written: the first frame, a frame that outgrew the buffer,
    /// a frame whose spans moved, a frame that does not follow the one on the
    /// device, or a caller that passed no [`Changes`] at all.
    Whole { rows: usize },
    /// Only the ranges the dirty rects named, as a count of `write_buffer`
    /// calls and of rows. Zero of both is a frame that redrew the commit the
    /// device already held.
    Ranges { ranges: usize, rows: usize },
}

/// The texture format the renderer draws into and reads back.
///
/// `Rgba8Unorm` rather than `Rgba8UnormSrgb`: this painter blends in
/// sRGB-encoded space, which `docs/decisions/blur-blends-in-srgb-encoded-space.md`
/// makes a term of the boundary-B contract rather than a per-painter choice. A
/// `Srgb` format would have the hardware convert on write and blend in linear
/// light, which is the divergence that record measures at roughly 50 code
/// points across a saturated seam.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// How many texels on a side each residency atlas is, before the device's own
/// maximum is applied.
///
/// # Why a budget rather than the device's maximum
///
/// An atlas is allocated whole, the first time a payload of its format appears,
/// so its extent is a memory commitment and not a ceiling: 2048 square is 16 MiB
/// of `Rgba8Unorm`, and the 16384 an Apple M3 reports would be 1 GiB. That is the
/// opposite of the question [`Renderer::max_extent`] answers, which is how large
/// a *drawable* the hardware can address — issue #714 took that one from the
/// adapter deliberately, and this one must not follow it.
///
/// 2048 is `wgpu::Limits::downlevel_defaults`' own texture maximum, which makes
/// it the largest atlas the entry-tier floor this painter targets is guaranteed
/// to hold. It is stated here as a number with a reason rather than read back
/// out of `downlevel_defaults`, because the device is no longer requested at
/// those limits and reading it there would say something untrue.
///
/// A payload larger than this is **not** refused: since issue #720 it gets a
/// texture of its own, sized to itself, and only a payload past the device's own
/// `max_texture_dimension_2d` is refused by name
/// ([`crate::ResidencyError::TooLarge`]). A dedicated texture rather than a
/// bigger atlas, because this constant is a memory commitment and raising it
/// would charge every document for the one that needed it.
pub const ATLAS_EXTENT: u32 = 2048;

impl Renderer {
    /// Acquires an adapter and builds the pipeline, drawing offscreen.
    ///
    /// Fallible where the conformance harness panics, because a renderer is
    /// something a host constructs and a host can report; the harness is a test
    /// and a missing device there is the runner being wrong.
    ///
    /// Native only, and deliberately absent on wasm rather than present and
    /// broken. A browser's main thread has nothing to block with: the adapter
    /// request resolves by returning to the JS event loop, which the blocking
    /// wait is holding, so the two deadlock on each other. A web host calls
    /// [`Renderer::new_async`] instead, which is this same construction with
    /// the wait handed back to the caller.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Result<Self, RendererError> {
        pollster::block_on(Self::new_async())
    }

    /// [`Renderer::new`] without the blocking wait: the constructor a web host
    /// drives, and the one every target has.
    pub async fn new_async() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: None,
                ..Default::default()
            })
            .await
            .map_err(|_| RendererError::NoAdapter)?;
        Self::on_adapter(instance, adapter, TARGET_FORMAT).await
    }

    /// Requests a device from `adapter` and builds everything over it.
    ///
    /// Shared with the surface path, which differs only in how the adapter was
    /// chosen — compatible with a window — and in the format the pipeline
    /// writes.
    ///
    /// Async because the device request is, and because this is the one step
    /// both constructors share: making it `async` here is what leaves a single
    /// blocking wait, in one native-only wrapper per constructor, rather than
    /// one per await.
    pub(crate) async fn on_adapter(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let adapter_info = adapter.get_info();
        // ASTC when the adapter has it, nothing else. A requested feature the
        // adapter lacks fails the request outright, so this is intersected
        // rather than asked for — the painter draws on an adapter without it,
        // and says so through `GpuPainter::samples` instead of failing to
        // start. It has to be *requested*, though: a feature the adapter
        // advertises and the device did not ask for is not a feature the device
        // has, and the atlas texture is created on the device.
        let baked = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_ASTC;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("dashscene-gpu"),
                required_features: baked,
                // Downlevel defaults, so this painter runs on the entry-tier
                // class of device R3 names rather than only on a desktop one —
                // but with the adapter's own resolution limits rather than
                // downlevel's, which cap `max_texture_dimension_2d` at 2048.
                //
                // A drawable's size is a property of the window the host opened
                // rather than of the features this painter uses, and a 2288x1410
                // window is an ordinary one: issue #714 aborted the host on the
                // first resize past 2048 on a device whose own maximum is 16384.
                // An entry-tier adapter still reports its own smaller maximum
                // here, so the painter stays bounded by the real constraint
                // rather than by a synthetic one — which is what
                // `using_resolution` is for, and it leaves every other downlevel
                // limit in place.
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await
            .map_err(RendererError::NoDevice)?;

        // The shader library and the render entry points, concatenated. Naga
        // validates the result when the module is created, which is the "naga
        // validates" half of layer 3.
        let source = format!("{}\n{}", crate::shader::SDF_WGSL, PAINT_WGSL);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dashscene-gpu paint"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        // Visibility is per binding, and it is a correctness constraint rather
        // than a tidiness one: `wgpu::Limits::downlevel_defaults` allows four
        // storage buffers **per shader stage**, and this pipeline binds seven.
        // Declaring each where it is actually read is what makes seven fit:
        //
        //     vertex    instances(0), glyph runs(8), shapes(9)
        //     fragment  paints(1), clips(2), strokes(4), images(5)
        //
        // Three and four. The fragment stage reads no instance array at all;
        // `VertexOut` in `shaders/paint.wgsl` carries the values it needs
        // across, and story #581 is why. The vertex stage read the stroke rows
        // too until story #584 took the quad's outset off that row and onto
        // `Instance::outset`, which is the one spare slot on either side —
        // `docs/decisions/tables-the-vertex-stage-reads.md` D4 says why a free
        // slot is not an invitation.
        //
        // Story #582's two tables took the same route deliberately. A glyph
        // run's parameters and a coverage mask's are five and eleven floats
        // that are **constant across the instance**, so the stage that runs
        // four times per quad can read them and hand the fragment stage the
        // values — which costs the fragment stage no binding at all. That works
        // because neither is a variable-length array
        // (`docs/decisions/tables-the-vertex-stage-reads.md` D2).
        //
        // **Issue #715's gradients could not**, and that is why binding 1 is a
        // heap rather than the solid table it used to be. A gradient's stop
        // array is indexed by a value the fragment computes from its own
        // coordinate, so it crosses as no varying at any width — and two more
        // fragment bindings against zero free slots is not an arrangement that
        // exists. The solid colours and the gradient rows share one binding
        // instead; [`paint_heap`] is the layout and
        // `docs/decisions/the-paint-parameter-heap.md` is the reasoning.
        let storage = |binding: u32, visibility: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dashscene-gpu paint"),
            entries: &[
                // The instance rows: vertex only, since story #581.
                storage(0, wgpu::ShaderStages::VERTEX),
                storage(1, wgpu::ShaderStages::FRAGMENT),
                storage(2, wgpu::ShaderStages::FRAGMENT),
                // The stroke rows, fragment only since story #584. They were
                // the one table both stages read — the vertex stage took the
                // quad's outset from the row — until a shadow needed the same
                // growth from a table that stage cannot bind at all. The packer
                // resolves both onto [`Instance::outset`] now, so the vertex
                // stage reads three storage buffers of the four
                // `downlevel_defaults` allows rather than four.
                storage(4, wgpu::ShaderStages::FRAGMENT),
                storage(5, wgpu::ShaderStages::FRAGMENT),
                // Story #582's two tables, vertex only — see the comment above.
                storage(8, wgpu::ShaderStages::VERTEX),
                storage(9, wgpu::ShaderStages::VERTEX),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // The atlas, and the sampler it is read through. One texture
                // binding rather than one per format: a frame that needs two
                // atlases is drawn as two runs over the same pipeline, because
                // `wgpu::Limits::downlevel_defaults` has no binding arrays and
                // `docs/decisions/pipelines-and-layer-3.md` D2 holds this
                // painter to those limits.
                // Declared filterable because binding 10 filters it. That is a
                // constraint on the atlas *formats*, and every format this set
                // holds meets it: `Rgba8Unorm` is filterable on every adapter,
                // and an ASTC format is filterable wherever
                // `TEXTURE_COMPRESSION_ASTC` is supported at all — which is the
                // only condition under which one of those textures exists here.
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dashscene-gpu paint"),
            // Option-wrapped since wgpu 30; `immediate_size` is its
            // replacement for push constants and this pipeline uses none.
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dashscene-gpu paint"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // No vertex buffers: the quad's corners come from the vertex
                // index and the instance's own bounds, so a frame uploads the
                // instance rows and nothing else. That is what R-T4 bounds the
                // per-frame cost to.
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Premultiplied source-over: the fragment shader multiplies
                    // colour by alpha, so the source factor is one.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // The composite pipeline: its own module, its own layout, its own
        // `@group(0)`. See `shaders/composite.wgsl` for why it is not another
        // entry point in `paint.wgsl`.
        let composite_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dashscene-gpu composite"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_WGSL.into()),
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dashscene-gpu composite"),
            entries: &[
                // The layer, read by `textureLoad`. Declared unfilterable
                // because nothing filters it: the composite is a 1:1 pixel copy
                // and the layout has no sampler at all.
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("dashscene-gpu composite"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dashscene-gpu composite"),
            layout: Some(&composite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &composite_module,
                entry_point: Some("vs_composite"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_module,
                entry_point: Some("fs_composite"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // The same premultiplied source-over the paint pipeline
                    // blends with, and for the same reason: the layer's texels
                    // are premultiplied, and `fs_composite` scales them by the
                    // group's alpha before they arrive here.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // The blur pipelines: their own module, their own layout, their own
        // `@group(0)`, for the reason the composite has its own — and one more,
        // which is that the blend state below is not the paint pipeline's.
        let blur_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dashscene-gpu blur"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}\n{}", crate::shader::SDF_WGSL, BLUR_WGSL).into(),
            ),
        });
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dashscene-gpu blur"),
            entries: &[
                // The taps' source, and the sharp original. Both unfilterable,
                // because nothing filters them: every read is a `textureLoad`
                // at an integer texel, which is what makes the kernel a pixel
                // kernel rather than a resampling one.
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Visible to both stages: the vertex stage builds the pass's
                // quad from it, and the fragment stage reads everything else.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // The coverage mask's atlas, and the sampler a distance field
                // is read through. Filterable here where the two above are not:
                // an MSDF edge ramp sampled nearest becomes a staircase, which
                // is the reason `msdf_sampler` exists at all.
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dashscene-gpu blur"),
            bind_group_layouts: &[Some(&blur_layout)],
            immediate_size: 0,
        });
        let blur_pipeline = |label: &'static str, entry: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&blur_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &blur_module,
                    entry_point: Some("vs_blur"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &blur_module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        // **No blending at all**, unlike either pipeline above.
                        // A backdrop filter *replaces* the region it covers —
                        // `dashscene-skia`'s `backdrop_layer_paint` says so with
                        // `BlendMode::Src` — and the lerp its antialiased edge
                        // needs cannot be expressed in blend factors, so the
                        // resolve shader samples the destination and writes the
                        // whole answer. See `shaders/blur.wgsl`.
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let blur_axis_pipeline = blur_pipeline("dashscene-gpu blur axis", "fs_blur_axis");
        let blur_resolve_pipeline = blur_pipeline("dashscene-gpu blur resolve", "fs_blur_resolve");

        let max_extent = device.limits().max_texture_dimension_2d;
        // Nearest and clamped, matching the reference painter's own
        // `SamplingOptions::default()`. Declared `NonFiltering` in the layout to
        // match, which also means an atlas texture needs no `filterable` format
        // capability — `Rgba8Unorm` has it anyway, and an ASTC format's
        // filterability is a device property this painter does not have to ask
        // about.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dashscene-gpu atlas"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        // The second sampler, and the one thing it is for.
        //
        // A distance field is not a colour. `dashscene-skia` samples its MSDF
        // atlases `Linear` and its image fills `Nearest`, deliberately and for
        // two different reasons, and this painter needs both for the same two.
        // Nearest on a distance field quantises the edge ramp to the atlas's
        // own texel grid: at a 48-unit render size off a 32 px/em atlas one
        // texel covers 1.5 pixels while the ramp is 6 pixels wide, so a smooth
        // edge becomes a four-step staircase.
        //
        // The gutter `crate::residency` names as the first thing to add if
        // filtering arrived is **not** needed for this, and that is a property
        // of the read rather than of the allocator: `msdf_sample` in
        // `shaders/paint.wgsl` clamps half a source texel inside the payload's
        // own sub-rect, and a bilinear footprint taken from there weights only
        // texels of that payload. It is the same clamp `image_colour` already
        // relies on for the nearest case, doing more work than it had to.
        let msdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dashscene-gpu msdf"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let placeholder = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("dashscene-gpu no atlas"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Every atlas is this many texels on a side, and never more than the
        // device will give.
        //
        // Clamped rather than taken from the device, which is the opposite of
        // what `max_extent` above does and is deliberate: an atlas is a
        // *budget*, not a maximum. Sizing it by the adapter would ask a
        // 16384-capable device for a 1 GiB texture the moment one image fill
        // appeared. `ATLAS_EXTENT` is what that budget is and says why.
        let residency = Residency::new(ATLAS_EXTENT.min(max_extent), max_extent);

        let frame = Frame::new(&device, &layout, &sampler, &msdf_sampler, &placeholder);
        Ok(Self {
            _instance: instance,
            device,
            queue,
            pipeline,
            layout,
            composite_pipeline,
            composite_layout,
            blur_axis_pipeline,
            blur_resolve_pipeline,
            blur_layout,
            layers: LayerTargets::default(),
            blurs: BlurTargets::default(),
            adapter_info,
            frame,
            format,
            offscreen: None,
            offscreen_allocations: 0,
            max_extent,
            residency,
            refusals: Vec::new(),
            refusals_seen: 0,
            refused_this_frame: std::collections::HashSet::new(),
            sampler,
            msdf_sampler,
            placeholder,
        })
    }

    /// The adapter this renderer runs on, for a measurement to be recorded
    /// beside.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// The largest either dimension of a drawable may be — a texture rendered
    /// into, or a window's swapchain.
    ///
    /// This is the adapter's own `max_texture_dimension_2d` and not a number
    /// this painter chose. The device is requested at downlevel limits with the
    /// adapter's resolution, so a drawable is bounded by the hardware rather
    /// than by the entry-tier feature floor the painter targets.
    pub fn max_extent(&self) -> u32 {
        self.max_extent
    }

    /// Refuses a drawable this device cannot address, on either axis.
    ///
    /// Every caller that is about to hand an extent to `wgpu` goes through
    /// here first. See [`RendererError::Extent`] for why the check is made
    /// ahead of the call rather than around it.
    pub(crate) fn check_extent(&self, width: u32, height: u32) -> Result<(), RendererError> {
        if width > self.max_extent || height > self.max_extent {
            return Err(RendererError::Extent {
                width,
                height,
                max: self.max_extent,
            });
        }
        Ok(())
    }

    /// Whether this device can hold an ASTC block texture at all.
    ///
    /// Asked of the device rather than the adapter: a feature the adapter
    /// advertises but that was not requested is not a feature the device has,
    /// and it is the device the atlas is created on.
    pub fn samples_astc(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC)
    }

    /// How this frame's instance rows reached the device.
    ///
    /// Reports the frame most recently drawn; before the first, the whole of
    /// nothing.
    pub fn last_instance_upload(&self) -> InstanceUpload {
        self.frame.last_upload
    }

    /// Forgets what the device holds, so the next frame is written whole.
    ///
    /// A caller must call this when the commits it is about to hand over come
    /// from a **different chain** than the ones before them — a document
    /// replaced, an arena rebuilt, a scene swapped. [`Changes`] carries a
    /// generation, and a generation is only meaningful within one chain: a
    /// fresh arena counts from the start, so its generation *G+1* can follow
    /// the old arena's *G* by arithmetic while naming a completely different
    /// picture. Nothing in the rows themselves distinguishes the two, and the
    /// spans of one scene rebuilt at a new extent are identical.
    ///
    /// The host is the only thing that knows, which is why this is a call and
    /// not a check.
    pub fn forget_uploaded(&mut self) {
        self.frame.uploaded.clear();
        self.frame.spans.clear();
        self.frame.uploaded_generation = None;
        // Residency is keyed by the image table's own row, and a rebuilt arena
        // starts that table again from zero — so the same key can name a
        // different picture across this call. See `Residency::forget_resident`.
        self.residency.forget_resident();
        // And the refusals, for the same reason and one table over: a `Refusal`
        // names a row, `Refusal`'s own doc says a row means nothing outside the
        // frame it came from, and a host reading `refusals()` after replacing a
        // document would otherwise get rows of the dead one. This is the
        // stale-row hazard story #585 fixed for instance rows.
        self.refusals.clear();
        self.refused_this_frame.clear();
    }

    /// Every device object this renderer has allocated since it was built —
    /// buffers, textures, views and bind groups.
    ///
    /// R-T4 budgets a steady-state frame for a dirty-range upload and a
    /// submission, so the number a test should see is one that stops moving:
    /// it rises while a frame outgrows a buffer or changes extent, and not
    /// otherwise. It is a counter rather than a claim in a comment because the
    /// per-frame allocation this replaced looked exactly like correct code
    /// while it ran.
    pub fn allocations(&self) -> u64 {
        // Residency's textures and views are counted here rather than only on
        // its own getter. They were not, and the omission had teeth: the test
        // that asserts a steady-state frame allocates nothing "residency
        // included" could not have failed if residency had reallocated an atlas
        // every frame. Found in review.
        //
        // **`blurs` was the third time the same term went missing** — after
        // residency in story #581 and the layer targets in story #583 — so the
        // rule is worth stating rather than rediscovering: every struct that
        // owns device objects and counts them adds its count here, and a term
        // here is unfalsifiable until some fixture makes it non-zero.
        // `a_frame_with_a_backdrop_allocates_and_a_steady_one_does_not` is that
        // fixture for this one; it differences a scene with a backdrop against
        // the same scene without, rather than asserting an absolute number.
        //
        // The constants are the two samplers and the placeholder texture with
        // its view, built once in `new` and never again.
        const AT_CONSTRUCTION: u64 = 4;
        self.frame.allocations
            + self.offscreen_allocations
            + self.residency.allocations()
            + self.layers.allocations
            + self.blurs.allocations
            + AT_CONSTRUCTION
    }

    /// The device, for [`crate::surface`] to configure a swapchain against.
    /// Crate-private: a device handed to a host is a device a host can build
    /// pipelines on, and boundary B has one painter per device by design.
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue, for [`crate::surface`] to present a frame on.
    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Draws `buffer` into a `width` x `height` texture and returns its pixels
    /// as unpremultiplied RGBA8, the space `goldens/README.md` compares in.
    ///
    /// # Errors
    ///
    /// [`RendererError::Extent`] if either dimension is past
    /// [`Renderer::max_extent`]. That is the one failure a caller can be told
    /// about rather than aborted by, and it is a `Result` where the empty
    /// buffer below is a panic because an extent is a number a caller computes
    /// — from a window, from a fixture — while an empty pack is a bug in the
    /// call itself.
    ///
    /// # Panics
    ///
    /// Panics if the frame has no instances to draw: a caller asking for an
    /// empty frame wants a cleared texture and should say so, and silently
    /// returning one hides an empty pack.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RendererError> {
        self.render_dirty(buffer, paints, images, clips, glyphs, None, width, height)
    }

    /// [`Renderer::render`], with boundary B's dirty set passed through.
    ///
    /// Separate from `render` rather than a parameter on it, because every
    /// caller but the incremental-upload test wants the whole frame written and
    /// an `Option` at each of those call sites would say nothing. Passing `None`
    /// is always correct; see `Frame::upload_instances` for what passing the
    /// set buys and for what it must not be trusted for.
    ///
    /// # Errors
    ///
    /// As [`Renderer::render`].
    ///
    /// # Panics
    ///
    /// As [`Renderer::render`].
    #[allow(clippy::too_many_arguments)]
    pub fn render_dirty(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        changes: Option<Changes<'_>>,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, RendererError> {
        // Before the assert, and before anything is allocated: an over-large
        // extent reaches `Device::create_texture` two statements below, and a
        // caller cannot be told about a validation error that panicked.
        self.check_extent(width, height)?;
        assert!(
            !buffer.instances().is_empty(),
            "render was given a frame with no instances"
        );

        let offscreen = match self.offscreen.take() {
            Some(offscreen) if offscreen.width == width && offscreen.height == height => offscreen,
            _ => {
                // A texture, its view and the staging buffer: three objects,
                // and the extent is the only thing that makes them stale.
                self.offscreen_allocations += 3;
                Offscreen::new(&self.device, self.format, width, height)
            }
        };
        self.draw(
            &offscreen.view,
            buffer,
            paints,
            images,
            clips,
            glyphs,
            changes,
            width,
            height,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dashscene-gpu readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &offscreen.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &offscreen.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(offscreen.padded as u32),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = offscreen.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| {
            r.expect("the readback buffer maps");
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device completes the frame");
        let data = slice
            .get_mapped_range()
            .expect("the mapped range is readable");
        let mut pixels = Vec::with_capacity(offscreen.unpadded * height as usize);
        for row in 0..height as usize {
            let start = row * offscreen.padded;
            pixels.extend_from_slice(&data[start..start + offscreen.unpadded]);
        }
        drop(data);
        offscreen.readback.unmap();
        self.offscreen = Some(offscreen);
        unpremultiply(&mut pixels);
        Ok(pixels)
    }

    /// Uploads what this frame changed and draws it into `view`.
    ///
    /// The whole of the per-frame work, and the one path both targets take. An
    /// empty frame clears and draws nothing rather than failing: a host whose
    /// document has no ink still has a window to fill, where the offscreen
    /// caller in [`Renderer::render`] asked for a picture and gets a panic.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        view: &wgpu::TextureView,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        glyphs: &GlyphRunTable,
        changes: Option<Changes<'_>>,
        width: u32,
        height: u32,
    ) {
        // The fill parameters the fragment stage reads, as one heap. Written
        // whole every frame, and deliberately not filtered by the dirty set: a
        // colour that animates changes this table without changing the rect
        // entry that names its row, so no rect is dirty and the row still has
        // to arrive.
        let heap = paint_heap(paints);
        let mut boxes: Vec<GpuClipBox> = clips
            .all_boxes()
            .iter()
            .map(|b| GpuClipBox {
                x: b.x,
                y: b.y,
                w: b.w,
                h: b.h,
                corners: [
                    b.corners.top_left,
                    b.corners.top_right,
                    b.corners.bottom_right,
                    b.corners.bottom_left,
                ],
            })
            .collect();
        if boxes.is_empty() {
            boxes.push(GpuClipBox::default());
        }
        let mut strokes: Vec<GpuStroke> = paints
            .all_strokes()
            .iter()
            .map(|stroke| GpuStroke {
                color: [
                    stroke.color.r,
                    stroke.color.g,
                    stroke.color.b,
                    stroke.color.a,
                ],
                width: stroke.width,
                align: stroke_align(stroke.align),
                _pad: [0; 2],
            })
            .collect();
        if strokes.is_empty() {
            strokes.push(GpuStroke::default());
        }
        let globals = Globals {
            size: [width as f32, height as f32],
            aa: AA_WIDTH,
            gradient_base: heap.gradient_base,
            shadow_base: heap.shadow_base,
            _pad: [0; 3],
        };

        // Residency, and the rows it resolves into. Before the upload, because
        // making a payload resident can create an atlas, and a bind group names
        // the atlas it draws from.
        let resolved = self.resolve_frame(buffer, paints, images, glyphs);
        let atlases = self.residency.atlas_count();

        let rebound = self.frame.upload(
            &self.device,
            &self.queue,
            &self.layout,
            &self.sampler,
            &self.msdf_sampler,
            &self.placeholder,
            &self.residency,
            buffer,
            &heap.words,
            &boxes,
            &strokes,
            &resolved,
            globals,
            changes,
        );

        let runs = draw_runs(buffer, &resolved);
        debug_assert!(
            runs.iter()
                .all(|run| run.atlas.is_none_or(|a| (a as usize) < atlases)),
            "a draw run names an atlas that does not exist"
        );

        // The render-target groups, and the passes they turn this one ordered
        // stream into. A frame with no groups plans to exactly one pass over
        // the whole buffer, which is what every frame before story #583 was.
        self.layers.prepare(
            &self.device,
            &self.queue,
            &self.composite_layout,
            self.format,
            width,
            height,
            buffer.layers(),
        );
        let plan = composite::plan(buffer);

        // The backdrop blurs this frame resolves, and the targets they need.
        // A frame with none allocates nothing here and draws into the caller's
        // view exactly as every frame before story #733 did.
        //
        // The atlas each one samples comes from the same resolution the paint
        // pipeline's draw runs come from, rather than a second walk: a masked
        // backdrop and the masked fill beneath it are the same node's coverage
        // field, so they name one row and one atlas.
        let backdrop_masks: Vec<(Option<u32>, &wgpu::TextureView)> = plan
            .iter()
            .filter_map(|pass| pass.backdrop)
            .map(|index| {
                let atlas = resolved.atlas_of(&buffer.instances()[index as usize]);
                let view = match atlas {
                    Some(index) => self.residency.view(index),
                    // A backdrop with no coverage mask still needs a texture
                    // named for the binding its layout declares, exactly as a
                    // frame with no atlas does for the paint pipeline.
                    None => &self.placeholder,
                };
                (atlas, view)
            })
            .collect();
        let backdrops = backdrop_masks.len();
        self.blurs.prepare(
            &self.device,
            &self.blur_layout,
            &self.composite_layout,
            self.format,
            width,
            height,
            &backdrop_masks,
            &self.msdf_sampler,
            &self.frame.clips,
            rebound,
        );
        // A backdrop snapshots the target it draws into, and a snapshot is a
        // texture-to-texture copy — which needs a `Texture`, where this function
        // is handed a `TextureView`. So a frame with a backdrop draws into a
        // texture this painter owns and composites it into `view` at the end.
        let frame_view = if backdrops == 0 {
            view
        } else {
            &self.blurs.base().view
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dashscene-gpu frame"),
            });
        let mut draws = 0usize;
        // Which backdrop of the plan is being resolved, counting the refused
        // ones — the index `BlurTargets::pass` is stated in. Named for the
        // position rather than for the outcome, because the two came apart once
        // and nothing but a two-backdrop frame can tell.
        let mut backdrop_ordinal = 0usize;
        for planned in &plan {
            let target = if planned.target == Instance::NONE {
                frame_view
            } else {
                self.layers.view(planned.target)
            };
            // The backdrop this pass resolves, before anything draws into the
            // target — the blur reads it as the previous pass on it left it.
            if let Some(index) = planned.backdrop {
                if planned.clear {
                    // Nothing has been written here yet, so the texture holds
                    // whatever the allocator handed over and the blur would
                    // read it. There is nothing to see beneath this backdrop,
                    // but it has to be *transparent* nothing.
                    clear(&mut encoder, target);
                }
                let texture = if planned.target == Instance::NONE {
                    &self.blurs.base().texture
                } else {
                    self.layers.texture(planned.target)
                };
                // **The ordinal always advances, whether or not anything was
                // encoded.** `BlurTargets` builds one bind-group pair per
                // backdrop of `backdrop_masks`, which is `plan` in order, and
                // each pair binds that backdrop's own coverage atlas — so this
                // is a position in the plan and not a count of the ones that
                // drew. Skipping it for a refused backdrop moved every backdrop
                // behind it onto the previous one's mask, which for a refused
                // field is the placeholder nothing writes: the next node's
                // frost vanished with no refusal recorded, and a silent drop is
                // what P4 forbids. The slot is allocated either way; what a
                // refusal saves is the two draws.
                let drew = self.resolve_backdrop(
                    &mut encoder,
                    backdrop_ordinal,
                    index,
                    buffer,
                    paints,
                    &resolved,
                    texture,
                    target,
                    width,
                    height,
                );
                backdrop_ordinal += 1;
                if drew {
                    draws += 2;
                }
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dashscene-gpu frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // A layer starts transparent, which is the state
                        // `save_layer` starts the reference painter's in. A
                        // target returned to must **load**, or this pass
                        // discards what the earlier one drew — and a frame
                        // whose only group starts at instance 0 cannot tell the
                        // two apart, which is why the planner decides it.
                        //
                        // A pass that resolved a backdrop above has already had
                        // its clear, and the resolve drew into the target after
                        // it: clearing again here would erase the frosted
                        // region this pass exists to draw over.
                        load: if planned.clear && planned.backdrop.is_none() {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for step in &planned.steps {
                match step {
                    // Four vertices per instance, as a triangle strip, and one
                    // draw per atlas run. Slice order is draw order, and both
                    // partitions of the buffer — this pass's range and the
                    // atlas runs — are ordered, so the buffer's own order is
                    // still the stacking order.
                    composite::Step::Instances(range) => {
                        pass.set_pipeline(&self.pipeline);
                        for run in overlapping(&runs, range) {
                            pass.set_bind_group(0, self.frame.bind_group(run.atlas), &[]);
                            pass.draw(0..4, run.instances.clone());
                            draws += 1;
                        }
                    }
                    // One quad over the whole target, blending the layer at its
                    // group's alpha.
                    composite::Step::Composite(slot) => {
                        pass.set_pipeline(&self.composite_pipeline);
                        pass.set_bind_group(0, self.layers.bind_group(*slot), &[]);
                        pass.draw(0..4, 0..1);
                        draws += 1;
                    }
                }
            }
        }
        // The frame drew into a texture this painter owns so that its backdrops
        // could read it; this is what puts it on the caller's target. One
        // composite at alpha one, through the pipeline story #583 built —
        // `fs_composite` scales a premultiplied texel by that alpha, which at
        // one is the identity, and the source-over onto a cleared target is a
        // copy.
        if backdrops > 0 {
            self.queue.write_buffer(
                self.blurs
                    .blit_alpha
                    .as_ref()
                    .expect("prepared alongside the bind group"),
                0,
                bytemuck::bytes_of(&GpuComposite {
                    alpha: 1.0,
                    _pad: [0.0; 3],
                }),
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dashscene-gpu base blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(
                0,
                self.blurs.blit.as_ref().expect("prepared with the base"),
                &[],
            );
            pass.draw(0..4, 0..1);
            drop(pass);
            draws += 1;
        }
        self.queue.submit([encoder.finish()]);
        self.frame.last_runs = draws;
    }

    /// Resolves the `slot`th backdrop blur of this frame — instance `index` —
    /// into the target it draws into.
    ///
    /// Three encoded things, in order: a copy of the target, the horizontal
    /// half of the separable kernel over a scratch, and the vertical half
    /// composited back into the target. `shaders/blur.wgsl` is the arithmetic
    /// and the reason for each.
    ///
    /// Returns whether it encoded anything. **A backdrop confined to a coverage
    /// field this frame could not make resident encodes nothing at all** (issue
    /// #972), and that is not the same as encoding it unmasked: unmasked means
    /// the parametric rounded box, so a refused field would frost the node's
    /// whole box rather than its outline — a larger wrong picture than the one
    /// the issue was filed for, measured. A baked-vector node's silhouette *is*
    /// its field, so with no field there is no region to frost.
    ///
    /// # Panics
    ///
    /// Panics when `index` does not name a backdrop instance, or when its row
    /// is not a row of the blur table it names. Both are broken contracts
    /// between the packer and this renderer rather than frames to skip (P4).
    #[allow(clippy::too_many_arguments)]
    fn resolve_backdrop(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        index: u32,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        resolved: &Resolved,
        target: &wgpu::Texture,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> bool {
        let instance = &buffer.instances()[index as usize];
        debug_assert_eq!(
            instance.kind,
            InstanceKind::Backdrop.as_u32(),
            "the planner named instance {index} a backdrop and it is a {}",
            InstanceKind::from_u32(instance.kind).name(),
        );
        let blur = paints
            .all_blurs()
            .get(instance.row as usize)
            .unwrap_or_else(|| {
                panic!(
                    "backdrop instance {index} names blur row {} of {}: a row is valid only in the \
                 table that assigned it",
                    instance.row,
                    paints.all_blurs().len(),
                )
            });
        // The one mapping, applied where every other blur in this painter
        // applies it. `pack::frosts` is what guarantees it is positive here.
        let sigma = crate::pack::blur_sigma(blur.radius);
        let support = (BLUR_SUPPORT_SIGMAS * sigma).ceil();
        let size = [width as f32, height as f32];

        // The coverage mask, when the node carries one. `Instance::shape` rides
        // on the backdrop instance for exactly this — a masked node's blur is
        // confined to the field's outline rather than to its box — and the row
        // is the same one the node's own fill resolves, so the parameters come
        // from the frame's own resolution rather than a second derivation.
        // `row`, not `slot`: this function's own `slot` is the backdrop's
        // ordinal in the frame, and the two are index-like values whose
        // confusion is silent.
        let mask = match instance.shape {
            Instance::NONE => None,
            row => {
                let shape = resolved.shapes[row as usize - 1];
                if shape.resolved == 0 {
                    return false;
                }
                Some(shape)
            }
        };

        // **A masked backdrop's quad is the field's padded plane quad, not the
        // node's box**, and that is a correctness property rather than a saving.
        // `msdf_sample` clamps its coordinate into the payload's own sub-rect,
        // so a fragment outside the field's quad reads the field's edge texel
        // and comes back with whatever coverage that texel carries — full
        // coverage, for any field whose outline touches its rectangle. The
        // geometry is the only thing that says "not here". `paint.wgsl`'s vertex
        // stage substitutes the same quad for the same reason, and this is that
        // substitution on the pipeline that has no instance array to read.
        let silhouette = mask.map_or(instance.bounds, |shape| {
            let [left, top, right, bottom] = shape.plane;
            [
                instance.bounds[0] + left,
                instance.bounds[1] + top,
                right - left,
                bottom - top,
            ]
        });

        // The resolve pass writes that silhouette and the half-ramp its
        // antialiased edge reaches past it. The horizontal pass writes every
        // texel the resolve pass then reads, which is the same rectangle plus
        // the support **in y**, since y is the axis the resolve pass steps
        // along.
        //
        // Both quads are stated over the silhouette's **axis-aligned bounds**
        // once the node turns (story #832). The quads are not themselves
        // rotated: the horizontal pass's dilation is stated in y alone, which is
        // reasoning about an axis-aligned rectangle, and rotating the geometry
        // would leave that dilation covering the wrong texels. Growing the
        // rectangle instead costs fill rate and nothing else — outside the
        // node's shape the resolve pass writes back the texel it just read, so
        // a quad larger than the shape draws exactly the same picture. The
        // shaping is done by the mask, which the fragment stage turns.
        let covered = rotated_bounds(silhouette, instance.rotation, instance.rotation_pivot);
        let resolve_quad = clamped_quad(covered, [AA_WIDTH; 2], size);
        let axis_quad = clamped_quad(covered, [AA_WIDTH, AA_WIDTH + support], size);
        let base = GpuBlur {
            bounds: instance.bounds,
            corners: instance.corners,
            quad: axis_quad,
            plane: mask.map_or([0.0; 4], |shape| shape.plane),
            uv: mask.map_or([0.0; 4], |shape| shape.uv),
            size,
            step: [1.0, 0.0],
            half_uv: mask.map_or([0.0; 2], |shape| shape.half_uv),
            sigma,
            support,
            opacity: instance.opacity,
            aa: AA_WIDTH,
            px_range: mask.map_or(0.0, |shape| shape.px_range),
            masked: u32::from(mask.is_some()),
            clip_offset: instance.clip_offset,
            clip_count: instance.clip_count,
            rotation_pivot: instance.rotation_pivot,
            rotation: instance.rotation,
            _pad: [0; 3],
        };

        // 1. The target as it stands, so the passes below can read it while it
        //    is also the thing they write. A full-target copy: the showcase
        //    holds one backdrop, and debt filed with this story is where a
        //    bounded one goes if that ever stops being true.
        let snapshot = self.blurs.snapshot();
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &snapshot.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // 2. The horizontal half, over the dilated quad, into the scratch.
        let (bind_group, uniform) = self.blurs.pass(slot, false);
        self.queue
            .write_buffer(uniform, 0, bytemuck::bytes_of(&base));
        self.axis_pass(
            encoder,
            "dashscene-gpu backdrop axis",
            &self.blurs.scratch().view,
            bind_group,
        );

        // 3. The vertical half, mixed against the sharp original and written
        //    over the node's own box.
        let (bind_group, uniform) = self.blurs.pass(slot, true);
        self.queue.write_buffer(
            uniform,
            0,
            bytemuck::bytes_of(&GpuBlur {
                quad: resolve_quad,
                step: [0.0, 1.0],
                ..base
            }),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dashscene-gpu backdrop resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Everything already drawn into this target stays: the
                    // resolve pass writes the node's box and nothing else, and
                    // what it writes there it computed from what was underneath.
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.blur_resolve_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1);
        true
    }

    /// One axis of the separable kernel, into a scratch that is cleared because
    /// nothing else writes it.
    fn axis_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &'static str,
        view: &wgpu::TextureView,
        bind_group: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.blur_axis_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1);
    }

    /// Every payload-backed table as the shaders read it, with each row's
    /// residency slot resolved into it.
    ///
    /// One walk over the instance rows, not three. Three tables reach the
    /// device through residency — image fills, glyph atlases and baked vector
    /// fields — and an instance names at most one row of one of them, so the
    /// walk that finds an image fill is the same walk that would find a glyph.
    ///
    /// # Residency follows the frame, not the table
    ///
    /// Only the rows some instance names are made resident. That is the whole
    /// point of an eviction policy: a document's asset table is every image it
    /// could ever show, and what has to fit in VRAM is what it shows *now*.
    ///
    /// Resolving the table instead was the first shape of this function, and it
    /// was wrong in a way worse than slow. A document holding more image assets
    /// than one atlas can carry would fail `ResidencyError::FrameExceedsAtlas`
    /// while drawing two of them, because the whole table was the working set by
    /// construction — and the LRU could never help, since it would be asked for
    /// every row every frame. Issue #460's measurement is about exactly this,
    /// one level up.
    ///
    /// It costs one pass over the instance rows, and only in a frame that has an
    /// image fill at all. R-T4 bounds the per-frame cost to the dirty-range
    /// upload and the submission; this is outside that budget and is stated
    /// rather than hidden. The alternative is for the packer to record the rows
    /// it emitted, which is free — and which would be a second record of a fact
    /// the instances already carry, so it is not taken lightly and is not taken
    /// here.
    ///
    /// A row the frame does not draw still gets a `GpuImage`, zeroed, so that a
    /// row index means the same thing in this array as in the table. Nothing
    /// samples it: an instance naming it is what would have made it resident.
    ///
    /// # A payload that cannot be made resident is named, not fatal
    ///
    /// This used to panic on every arm of [`ResidencyError`], on the reasoning
    /// that each was a broken promise rather than a condition to recover from.
    /// Two of them turned out to be reachable from an ordinary document, and a
    /// host crash is not an acceptable answer to either:
    ///
    /// - **Issue #718** — a JPEG or GIF image fill. `TexelPayload::of` panicked
    ///   by name, and `Painter::samples`, the declaration that was supposed to
    ///   stop the payload arriving, has no production call site at all.
    /// - **Issue #720** — a payload larger than [`ATLAS_EXTENT`]. Story #582
    ///   widened it from image fills to glyph atlases and baked-vector atlases
    ///   too, and a glyph atlas is the likeliest of the three to reach 2048
    ///   square: one sheet for a whole script at a whole weight, so a CJK
    ///   coverage set exceeds it where an oversized *photograph* has to be
    ///   authored deliberately. That arm is now mostly gone rather than
    ///   reported — such a payload gets a texture of its own — and only one
    ///   past the device's own limit is refused.
    ///
    /// So the row draws nothing and the refusal is recorded, named, on
    /// [`Renderer::refusals`]. `Painter::paint` still returns nothing by
    /// decision, so there is still no channel *back*; what changed is that
    /// there is now a channel *out*, which is what P4's "never a silent drop"
    /// needs. Widening boundary B, and refusing the document at load where
    /// `Painter::samples` would finally have a caller, both stay open.
    fn resolve_frame(
        &mut self,
        buffer: &InstanceBuffer,
        paints: &PaintTable,
        images: &ImageTable,
        glyphs: &GlyphRunTable,
    ) -> Resolved {
        self.residency.begin_frame();
        self.refusals.clear();
        self.refused_this_frame.clear();
        let fills = paints.all_images();
        let fields = paints.all_shapes();
        let runs = glyphs.runs();
        let mut out = Resolved {
            images: vec![GpuImage::default(); fills.len()],
            runs: vec![GpuGlyphRun::default(); runs.len()],
            shapes: vec![GpuShape::default(); fields.len()],
            atlas_of_image: vec![None; fills.len()],
            atlas_of_run: vec![None; runs.len()],
            atlas_of_shape: vec![None; fields.len()],
        };

        let entries = images.all_entries();
        for instance in buffer.instances() {
            // A coverage mask is read for whatever kind carries it, so the
            // fill and the backdrop of one masked node resolve the same row.
            // They name the same field, so the second is a cache hit and adds
            // nothing to the working set.
            if instance.shape != Instance::NONE {
                let row = instance.shape as usize - 1;
                debug_assert!(
                    instance.kind != InstanceKind::FillImage.as_u32(),
                    "a masked image fill would need two atlases for one quad; the packer emits \
                     none, matching the reference painter"
                );
                let field = &fields[row];
                if out.atlas_of_shape[row].is_none()
                    && field_draws(field)
                    && let Some(slot) =
                        self.resident_image(images, entries, field.image, "a vector field's atlas")
                {
                    let extent = self.residency.atlas_extent(slot.atlas);
                    let asset = images.resolve(field.image);
                    out.atlas_of_shape[row] = Some(slot.atlas);
                    out.shapes[row] = gpu_shape(field, slot.uv(extent), asset.width, asset.height);
                }
            }

            if instance.kind == InstanceKind::FillImage.as_u32() {
                let row = instance.row as usize;
                if out.atlas_of_image[row].is_some() {
                    continue;
                }
                let fill = &fills[row];
                let Some(slot) =
                    self.resident_image(images, entries, fill.image, "an image fill's payload")
                else {
                    continue;
                };
                let asset = images.resolve(fill.image);
                out.atlas_of_image[row] = Some(slot.atlas);
                let t = fill.transform;
                out.images[row] = GpuImage {
                    // Normalised against the atlas this slot landed in, not
                    // against the residency set's nominal extent: a compressed
                    // atlas is rounded down to whole blocks and the two differ.
                    uv: slot.uv(self.residency.atlas_extent(slot.atlas)),
                    transform: [t.a, t.b, t.c, t.d],
                    translate: [t.tx, t.ty],
                    size: [asset.width as f32, asset.height as f32],
                    scale_mode: scale_mode(fill.scale_mode),
                    tile_scale: fill.tile_scale,
                    _pad: [0; 2],
                };
            } else if instance.kind == InstanceKind::Text.as_u32() {
                let row = instance.row as usize;
                if out.atlas_of_run[row].is_some() {
                    continue;
                }
                let run = &runs[row];
                let atlas = glyphs.atlas(run.atlas);
                // An atlas with no extent has no texels to sample, and every
                // mapping below divides by it. The same case, and the same
                // treatment, as a zero-extent image payload.
                if atlas.width == 0 || atlas.height == 0 {
                    continue;
                }
                let resident = self.residency.resident(
                    &self.device,
                    &self.queue,
                    PayloadKey::atlas(run.atlas.0, atlas),
                    // Built here rather than through `ImageAsset::as_ref`,
                    // which re-parses the payload's header on every call:
                    // an `Atlas` already states its extent, and this runs
                    // once per run per frame.
                    dashpaint::ImageRef {
                        format: atlas.image.format,
                        bytes: &atlas.image.bytes,
                        width: atlas.width,
                        height: atlas.height,
                    },
                );
                let slot = match resident {
                    Ok(slot) => slot,
                    // The run draws nothing and the refusal is named. Before
                    // issues #718 and #720 this was a panic, which took the
                    // host down over a document the reference painter draws.
                    Err(error) => {
                        self.refuse("a glyph atlas", run.atlas.0, error);
                        continue;
                    }
                };
                let extent = self.residency.atlas_extent(slot.atlas);
                out.atlas_of_run[row] = Some(slot.atlas);
                out.runs[row] = gpu_glyph_run(run, atlas, slot.uv(extent));
            }
        }
        // The two records of one fact agree. `atlas_of_shape` is what this side
        // segments draw runs and picks bind groups by, and `GpuShape::resolved`
        // is what the shaders read, because a shader cannot see the map. Both
        // are written in the one arm above, two lines apart — this is what says
        // a later arm cannot write one without the other and leave the backdrop
        // path and the draw-run path disagreeing about whether a field exists.
        debug_assert!(
            out.atlas_of_shape
                .iter()
                .zip(&out.shapes)
                .all(|(atlas, shape)| atlas.is_some() == (shape.resolved != 0)),
            "a coverage-mask row is resolved exactly when it landed in an atlas",
        );
        out
    }

    /// Makes image-table row `index` resident and returns where it sits, or
    /// `None` for a payload with no extent.
    ///
    /// # A payload with no extent draws nothing, and is never made resident
    ///
    /// Boundary B stores a payload whose binding supplied no bytes at 0 x 0
    /// rather than refusing it, because `dashscene-validator`'s image.no-bytes
    /// rule is what names that case. Left to reach the residency path, an
    /// encoded one panics in the decoder — on a payload the validator has
    /// already reported — and a baked one divides by zero in the shader. Its row
    /// stays zeroed, its atlas stays `None`, and `paint.wgsl`'s own guards cover
    /// the same case from the other side.
    ///
    /// # A refused payload returns `None` and is recorded
    ///
    /// For the reason [`Renderer::resolve_frame`] gives. `what` names the
    /// caller on the [`Refusal`], because an image fill and a vector field's
    /// atlas are the same table row with very different symptoms.
    ///
    /// The caller cannot tell this apart from the no-extent case above, and
    /// does not need to: both draw nothing. The difference is that the
    /// no-extent case was already named upstream by the validator's
    /// `image.no-bytes` rule, and this one is named here because nothing
    /// upstream reports it.
    fn resident_image(
        &mut self,
        images: &ImageTable,
        entries: &[dashpaint::ImageEntry],
        index: u32,
        // Static because a refusal keeps it: both call sites pass a literal
        // naming their consumer, and a `Refusal` outlives the call. The third
        // consumer, a glyph atlas, calls `Residency::resident` directly and
        // never passes through here.
        what: &'static str,
    ) -> Option<crate::residency::Slot> {
        let asset = images.resolve(index);
        if asset.width == 0 || asset.height == 0 {
            return None;
        }
        let resident = self.residency.resident(
            &self.device,
            &self.queue,
            PayloadKey::image(index, &entries[index as usize]),
            asset,
        );
        match resident {
            Ok(slot) => Some(slot),
            // Same treatment as a payload with no extent above: the row stays
            // zeroed, its atlas stays `None`, and nothing draws it. The
            // difference is that this one is named rather than silent, because
            // no upstream rule reported it (issues #718 and #720).
            Err(error) => {
                self.refuse(what, index, error);
                None
            }
        }
    }

    /// How many draw calls the frame most recently drawn took.
    ///
    /// One unless the frame's image fills sat in more than one atlas, or it
    /// held a render-target group: since story #583 a group costs one draw for
    /// its composite, plus one more for each run its own quads are split into.
    /// Public because a test that asserted only the picture could not tell a
    /// frame that batched from one that did not, and the batching is the
    /// property R-T2 cares about.
    pub fn last_draw_runs(&self) -> usize {
        self.frame.last_runs
    }

    /// Payloads evicted from the atlases to make room, since this renderer was
    /// built.
    pub fn evictions(&self) -> u64 {
        self.residency.evictions()
    }

    /// How many encoded payloads have been decoded since this renderer was
    /// built — see [`crate::Residency::decodes`].
    pub fn decodes(&self) -> u64 {
        self.residency.decodes()
    }

    /// What the frame most recently resolved could not make resident.
    ///
    /// Empty for every frame that drew everything it was asked to, which is
    /// every frame in this repository's corpus.
    ///
    /// # Why this exists, and why it is not a return value
    ///
    /// `Painter::paint` returns nothing by decision, so a refusal inside a
    /// frame has no channel to travel back on. Every arm of [`ResidencyError`]
    /// used to be a panic for that reason. Two of them turned out to be
    /// reachable from an ordinary document rather than from a broken contract —
    /// a JPEG or GIF image fill (issue #718) and a payload larger than the
    /// atlas (issue #720) — and a host crash is not an acceptable answer to
    /// either.
    ///
    /// So the row draws nothing and the refusal is recorded here, named. That
    /// is what P4 asks for: never a silent drop. It deliberately does **not**
    /// widen boundary B, which would change every painter's signature, and it
    /// is not the larger fix — refusing the document at load, where
    /// `Painter::samples` would finally have a caller — which stays open.
    pub fn refusals(&self) -> &[Refusal] {
        &self.refusals
    }

    /// How many refusals this renderer has recorded since it was built.
    pub fn refusals_seen(&self) -> u64 {
        self.refusals_seen
    }

    /// Records one refusal and counts it, once per consumer and row per frame.
    ///
    /// The dedup is not cosmetic. `resolve_frame`'s memo arrays record only what
    /// *resolved*, so a refused row stays unresolved and every further instance
    /// naming it asks again — a document with a hundred rects sharing one refused
    /// image fill would otherwise record a hundred identical refusals per frame,
    /// and `refusals_seen` would count retries rather than refused payloads.
    fn refuse(&mut self, what: &'static str, row: u32, error: ResidencyError) {
        if !self.refused_this_frame.insert((what, row)) {
            return;
        }
        self.refusals_seen += 1;
        self.refusals.push(Refusal { what, row, error });
    }
}

/// One payload a frame could not make resident, and why.
///
/// The row it names is meaningful only against the tables of the frame it came
/// from, the same way [`dashpaint::GlyphRun::rect`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Which consumer asked — an image fill's payload, a vector field's atlas,
    /// or a glyph atlas. The three reach residency through one call and have
    /// very different symptoms.
    pub what: &'static str,
    /// The table row that went undrawn.
    pub row: u32,
    /// Why it could not be made resident.
    pub error: ResidencyError,
}

/// Every payload-backed table of one frame, as the shaders read it, plus which
/// atlas each row landed in — `None` for a row this frame does not draw.
///
/// A row the frame does not draw still gets a zeroed row, so that a row index
/// means the same thing in these arrays as in the table it came from. Nothing
/// samples it: an instance naming it is what would have made it resident.
struct Resolved {
    images: Vec<GpuImage>,
    runs: Vec<GpuGlyphRun>,
    shapes: Vec<GpuShape>,
    atlas_of_image: Vec<Option<u32>>,
    atlas_of_run: Vec<Option<u32>>,
    atlas_of_shape: Vec<Option<u32>>,
}

impl Resolved {
    /// The atlas `instance` samples, or `None` when it samples none.
    ///
    /// A masked instance samples its coverage field whatever its kind, and the
    /// packer emits no masked image fill — which is what makes "at most one
    /// atlas per instance" true, and what the debug assertion in
    /// [`Renderer::resolve_frame`] holds.
    fn atlas_of(&self, instance: &Instance) -> Option<u32> {
        if instance.shape != Instance::NONE {
            return self.atlas_of_shape[instance.shape as usize - 1];
        }
        if instance.kind == InstanceKind::FillImage.as_u32() {
            return self.atlas_of_image[instance.row as usize];
        }
        if instance.kind == InstanceKind::Text.as_u32() {
            return self.atlas_of_run[instance.row as usize];
        }
        None
    }

    /// Every atlas this frame samples, ascending and without repeats.
    fn atlases(&self) -> Vec<u32> {
        let mut distinct: Vec<u32> = self
            .atlas_of_image
            .iter()
            .chain(&self.atlas_of_run)
            .chain(&self.atlas_of_shape)
            .flatten()
            .copied()
            .collect();
        distinct.sort_unstable();
        distinct.dedup();
        distinct
    }
}

/// One glyph run as the shader reads it, over the slot its atlas landed in.
///
/// `uv` arrives as the atlas payload's own rectangle in the residency texture,
/// normalised. What the shader wants is a map from a *source* texel to that
/// texture, so the atlas's own extent is folded in here — once per run, rather
/// than once per fragment.
fn gpu_glyph_run(run: &dashpaint::GlyphRun, atlas: &dashpaint::Atlas, uv: [f32; 4]) -> GpuGlyphRun {
    let scale = [uv[2] / atlas.width as f32, uv[3] / atlas.height as f32];
    GpuGlyphRun {
        color: [run.color.r, run.color.g, run.color.b, run.color.a],
        uv: [uv[0], uv[1], scale[0], scale[1]],
        half_uv: [0.5 * scale[0], 0.5 * scale[1]],
        // `dashscene-skia`'s own formula. `plane_em` and `atlas_px` bake the
        // range into the bounds, so this scales the sharpness of the edge and
        // not the size.
        px_range: atlas.distance_range_px() * run.size / f32::from(atlas.px_per_em()),
        _pad: 0.0,
    }
}

/// One coverage mask as the shader reads it, over the slot its atlas landed in.
///
/// `uv` is the whole atlas payload's rectangle in the residency texture; the
/// field occupies `atlas_rect` texels of that payload, so the two are composed
/// here into the field's own sub-rect.
fn gpu_shape(
    field: &dashpaint::VectorField,
    uv: [f32; 4],
    atlas_width: u32,
    atlas_height: u32,
) -> GpuShape {
    let [ax, ay, aw, ah] = field.atlas_rect;
    let (width, height) = (atlas_width as f32, atlas_height as f32);
    let sub = [
        uv[0] + ax as f32 / width * uv[2],
        uv[1] + ay as f32 / height * uv[3],
        aw as f32 / width * uv[2],
        ah as f32 / height * uv[3],
    ];
    let [left, _, right, _] = field.plane_bounds;
    GpuShape {
        plane: field.plane_bounds,
        uv: sub,
        half_uv: [0.5 * sub[2] / aw as f32, 0.5 * sub[3] / ah as f32],
        // Device pixels per atlas texel, at unit scale. `dashscene-skia` takes
        // the x ratio alone, and this matches it rather than re-deriving it.
        px_range: field.distance_range * (right - left) / aw as f32,
        // This function is reached only from the arm that made the payload
        // resident, so building a row *is* resolving one. Every other row keeps
        // `GpuShape::default()`'s zero.
        resolved: 1,
    }
}

/// Whether a coverage mask has a quad and an atlas rectangle to sample.
///
/// The reference painter's own degenerate guard, and the reason it is checked
/// before the payload is made resident rather than after: every mapping in
/// [`gpu_shape`] divides by the atlas rectangle, and a field with no quad
/// sampled nothing anyway.
fn field_draws(field: &dashpaint::VectorField) -> bool {
    let [left, top, right, bottom] = field.plane_bounds;
    right > left && bottom > top && field.atlas_rect[2] > 0 && field.atlas_rect[3] > 0
}

/// A contiguous range of instances drawn with one atlas bound.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DrawRun {
    instances: std::ops::Range<u32>,
    /// The atlas these instances sample, or `None` when none of them samples
    /// one.
    atlas: Option<u32>,
}

/// Splits the frame into runs, one per atlas the instances need.
///
/// **Every kind that samples**, not only image fills: a glyph instance samples
/// its run's atlas and a masked instance samples its coverage field's, and
/// [`Resolved::atlas_of`] is the one place that mapping is written. Three tables
/// reach residency and an instance names at most one row of one of them.
///
/// # Why this is not a per-frame walk in the common case
///
/// A frame whose payloads all landed in one atlas — which is every frame of one
/// texel format, and so every frame this repository draws today, since a glyph
/// atlas and a decoded image are both `Rgba8` — takes one run over the whole
/// buffer, decided from the resolved rows alone without looking at an instance.
/// Segmenting happens only when a frame genuinely mixes texel formats, which a
/// document does when a host binds `dashpack` derivations for some assets and
/// not others.
///
/// That is a claim about *this* pass and not about the frame:
/// [`Renderer::resolve_frame`] already walks the instance rows once in any
/// frame that samples anything, and says why.
fn draw_runs(buffer: &InstanceBuffer, resolved: &Resolved) -> Vec<DrawRun> {
    let total = buffer.instances().len() as u32;
    // The rows this frame did not draw are `None` and contribute no atlas: a
    // sentinel counted here would conjure a run for an atlas nothing samples.
    let distinct = resolved.atlases();

    match distinct.as_slice() {
        // No image row at all: nothing samples an atlas.
        [] => vec![DrawRun {
            instances: 0..total,
            atlas: None,
        }],
        // Every image row in one atlas: one run, and the instance rows are
        // never read.
        [only] => vec![DrawRun {
            instances: 0..total,
            atlas: Some(*only),
        }],
        _ => {
            let mut runs: Vec<DrawRun> = Vec::new();
            let mut start = 0u32;
            let mut current: Option<u32> = None;
            for (index, instance) in buffer.instances().iter().enumerate() {
                // An instance that samples nothing, and a row that was not made
                // resident — a zero-extent payload — both draw without an
                // atlas, so neither constrains a run.
                let Some(wanted) = resolved.atlas_of(instance) else {
                    continue;
                };
                match current {
                    Some(atlas) if atlas == wanted => {}
                    None => current = Some(wanted),
                    Some(_) => {
                        let index = index as u32;
                        runs.push(DrawRun {
                            instances: start..index,
                            atlas: current,
                        });
                        start = index;
                        current = Some(wanted);
                    }
                }
            }
            runs.push(DrawRun {
                instances: start..total,
                atlas: current,
            });
            runs
        }
    }
}

/// The atlas runs overlapping `range`, each clipped to it.
///
/// Two independent partitions of one instance buffer meet here: [`draw_runs`]
/// splits it by the atlas a quad samples, and [`crate::composite::plan`] splits
/// it by the target a quad draws into. Neither is a refinement of the other —
/// an atlas run can span a group boundary and a group can hold quads from two
/// atlases — so the draws are their intersection. Both are ordered ranges over
/// the same index space, which is what makes the intersection a filter rather
/// than a merge.
fn overlapping<'a>(
    runs: &'a [DrawRun],
    range: &'a Range<u32>,
) -> impl Iterator<Item = DrawRun> + 'a {
    runs.iter().filter_map(move |run| {
        let start = run.instances.start.max(range.start);
        let end = run.instances.end.min(range.end);
        (start < end).then_some(DrawRun {
            instances: start..end,
            atlas: run.atlas,
        })
    })
}

/// The offscreen target and the staging buffer its pixels are read back
/// through, held across calls for the reason [`Frame`] is.
struct Offscreen {
    target: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    /// One row of pixels in bytes, and that row padded to wgpu's 256-byte copy
    /// alignment. Kept because the readback re-assembles the unpadded rows.
    unpadded: usize,
    padded: usize,
}

impl Offscreen {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dashscene-gpu target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = width as usize * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * height as usize) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            target,
            view,
            readback,
            width,
            height,
            unpadded,
            padded,
        }
    }
}

/// The layer textures a frame's render-target groups draw into, one per layer
/// (story #583).
///
/// # Why full extent, and one per layer
///
/// Each layer is the **whole target**, which is `dashscene-skia`'s own choice
/// and made for a reason this painter shares: a group's ink reaches past its
/// rect range through shadows and blurs, so a bound tight to the geometry would
/// have to be derived from the effects instead, and getting it wrong moves
/// pixels. Story #584 adds exactly those effects.
///
/// One texture per layer rather than one per nesting *depth*, which would let
/// siblings share. Depth-keyed pooling is the smaller allocation and it is not
/// what this builds, because the measured shape does not call for it: the
/// showcase's only render-target group is one group nesting one deep, and
/// `dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` warns at eight. A
/// pool would also have to keep a layer alive until its parent's pass, so it
/// saves nothing until sibling groups are both numerous and deep. It is the
/// optimization to reach for if a scene ever makes this cost visible, and the
/// planner would not change: [`crate::composite::plan`] names layers by slot
/// and never says where their pixels live.
///
/// Held across frames for the reason [`Offscreen`] is, and rebuilt when the
/// extent or the layer count changes. The alphas are written every frame
/// regardless: a group's alpha animates without changing how many layers there
/// are, and a stale uniform would draw the previous frame's opacity.
#[derive(Default)]
struct LayerTargets {
    /// One texture per layer, indexed by slot minus one.
    ///
    /// Held beside the views since story #733: a backdrop blur inside a group
    /// snapshots that group's layer, and `copy_texture_to_texture` names a
    /// texture where a render pass attaches a view.
    textures: Vec<wgpu::Texture>,
    /// One view per layer, indexed by slot minus one.
    views: Vec<wgpu::TextureView>,
    /// One bind group per layer, naming that layer's view and its own alpha
    /// uniform. Per layer rather than one shared group with a dynamic offset:
    /// the texture differs per layer, so the group has to be rebuilt anyway.
    bind_groups: Vec<wgpu::BindGroup>,
    /// One uniform buffer per layer. Separate buffers rather than one written
    /// between draws: writes queued against a single buffer inside one
    /// submission do not interleave with the draws that read it, so every
    /// composite would blend at the last alpha written.
    alphas: Vec<wgpu::Buffer>,
    width: u32,
    height: u32,
    /// Device objects allocated here, counted beside the others — see
    /// [`Renderer::allocations`].
    allocations: u64,
}

/// One layer's composite parameters, as `shaders/composite.wgsl` reads them.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuComposite {
    alpha: f32,
    _pad: [f32; 3],
}

impl LayerTargets {
    /// Makes `count` layers of `width` x `height` available, rebuilding when
    /// either has changed, and writes each layer's alpha.
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        layers: &[Layer],
    ) {
        if self.width != width || self.height != height || self.views.len() != layers.len() {
            self.textures.clear();
            self.views.clear();
            self.bind_groups.clear();
            self.alphas.clear();
            for _ in 0..layers.len() {
                // Drawn into, then read by `textureLoad` when it composites —
                // and copied from since story #733, because a backdrop blur
                // inside this group snapshots the layer rather than the frame's
                // target, which is what makes a render-target group a backdrop
                // root.
                let Owned { texture, view } =
                    Owned::new(device, "dashscene-gpu layer", format, width, height);
                let alpha = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("dashscene-gpu layer alpha"),
                    size: size_of::<GpuComposite>() as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.bind_groups
                    .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("dashscene-gpu layer"),
                        layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: alpha.as_entire_binding(),
                            },
                        ],
                    }));
                self.textures.push(texture);
                self.views.push(view);
                self.alphas.push(alpha);
                // A texture, its view, its uniform buffer and its bind group.
                self.allocations += 4;
            }
            self.width = width;
            self.height = height;
        }
        for (layer, buffer) in layers.iter().zip(&self.alphas) {
            queue.write_buffer(
                buffer,
                0,
                bytemuck::bytes_of(&GpuComposite {
                    alpha: layer.alpha,
                    _pad: [0.0; 3],
                }),
            );
        }
    }

    /// The view a pass draws into for layer `slot`, where `slot` is an
    /// [`Instance::layer`] value — so slot 1 is the first layer.
    ///
    /// # Panics
    ///
    /// Panics for a slot this frame holds no layer for, which is the same
    /// broken contract [`crate::composite::plan`] panics on one step earlier.
    fn view(&self, slot: u32) -> &wgpu::TextureView {
        self.views.get(slot as usize - 1).unwrap_or_else(|| {
            panic!(
                "a pass draws into layer {slot} of {} allocated",
                self.views.len()
            )
        })
    }

    /// The bind group that blends layer `slot` at its own alpha.
    ///
    /// # Panics
    ///
    /// As [`LayerTargets::view`].
    fn bind_group(&self, slot: u32) -> &wgpu::BindGroup {
        self.bind_groups.get(slot as usize - 1).unwrap_or_else(|| {
            panic!(
                "a pass composites layer {slot} of {} allocated",
                self.bind_groups.len()
            )
        })
    }

    /// The texture behind layer `slot`, which a backdrop inside that group
    /// snapshots.
    ///
    /// # Panics
    ///
    /// As [`LayerTargets::view`].
    fn texture(&self, slot: u32) -> &wgpu::Texture {
        self.textures.get(slot as usize - 1).unwrap_or_else(|| {
            panic!(
                "a backdrop snapshots layer {slot} of {} allocated",
                self.textures.len()
            )
        })
    }
}

/// Makes `view` transparent and draws nothing into it.
///
/// A pass with no steps, which is the only way to clear a target that a copy —
/// rather than a draw — is about to read. A backdrop at the head of a target
/// needs exactly this: the texture holds whatever the allocator handed over
/// until something writes it, and "nothing beneath this node" has to be
/// transparent nothing rather than undefined nothing.
fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("dashscene-gpu clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

/// `bounds` grown by `pad` per axis and clipped to a target of `size`, as
/// `[x, y, w, h]`.
///
/// Both halves matter and for different reasons. The growth is what keeps the
/// geometry from clipping ink the coverage means to draw — the argument
/// [`Instance::outset`] makes, one pipeline over. The clip is a cost bound: a
/// quad reaching past the target shades fragments the rasteriser then discards,
/// and a blur's quad is the one this painter builds from a radius rather than
/// from a measured box.
///
/// The pad is per axis because the two blur passes need different ones, and
/// the difference is not symmetry. The horizontal pass reads the **snapshot**,
/// which is the whole target, so its own taps need no room. What needs room is
/// the vertical pass reading the horizontal pass's *output*: it steps in y over
/// `± support`, and a row the horizontal pass never wrote is transparent, which
/// would darken the panel's top and bottom edges. So the support is added on
/// the second pass's axis alone.
///
/// An empty result is possible — a node entirely off-target — and is left
/// empty rather than clamped to a minimum: a zero-area quad draws nothing,
/// which is the right answer for a backdrop nobody can see.
/// The axis-aligned bounds of `bounds` turned by `rotation` radians about
/// `pivot`, both in document space (story #832).
///
/// Returned unchanged at a zero rotation, which is what makes an unrotated
/// backdrop build exactly the quads it built before that story.
///
/// The blur pipeline draws axis-aligned quads and shapes the result with a mask
/// the fragment stage turns, so what the geometry has to guarantee is *cover* —
/// every texel the rotated silhouette reaches. A quad larger than the shape
/// costs fill rate; one smaller clips the frosted region, which reads as a
/// panel with a corner cut off rather than as a bug.
fn rotated_bounds(bounds: [f32; 4], rotation: f32, pivot: [f32; 2]) -> [f32; 4] {
    if rotation == 0.0 {
        return bounds;
    }
    let (sin, cos) = rotation.sin_cos();
    let [x, y, w, h] = bounds;
    let turn = |px: f32, py: f32| {
        let (dx, dy) = (px - pivot[0], py - pivot[1]);
        (
            pivot[0] + dx * cos - dy * sin,
            pivot[1] + dx * sin + dy * cos,
        )
    };
    let corners = [
        turn(x, y),
        turn(x + w, y),
        turn(x, y + h),
        turn(x + w, y + h),
    ];
    let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

fn clamped_quad(bounds: [f32; 4], pad: [f32; 2], size: [f32; 2]) -> [f32; 4] {
    let left = (bounds[0] - pad[0]).max(0.0);
    let top = (bounds[1] - pad[1]).max(0.0);
    let right = (bounds[0] + bounds[2] + pad[0]).min(size[0]);
    let bottom = (bounds[1] + bounds[3] + pad[1]).min(size[1]);
    [left, top, (right - left).max(0.0), (bottom - top).max(0.0)]
}

/// A texture this painter owns, with the view it is drawn through.
///
/// Both, because a backdrop needs each in a place the other will not do: a
/// render pass attaches the *view*, and `copy_texture_to_texture` names the
/// *texture*. Keeping the pair together is what makes "snapshot the target"
/// expressible at all.
struct Owned {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Owned {
    /// A target-sized texture that can be drawn into, sampled, and copied both
    /// ways.
    fn new(
        device: &wgpu::Device,
        label: &'static str,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }
}

/// One backdrop blur pass's parameters, as `shaders/blur.wgsl` reads them.
///
/// A hundred and forty-four bytes, and the assertions below are the only thing
/// holding that: the shader's `Blur` is a second declaration of this layout in
/// another language, and nothing in either one holds the two together.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuBlur {
    bounds: [f32; 4],
    corners: [f32; 4],
    quad: [f32; 4],
    /// [`GpuShape::plane`], for a backdrop confined to a coverage mask.
    plane: [f32; 4],
    /// [`GpuShape::uv`].
    uv: [f32; 4],
    size: [f32; 2],
    step: [f32; 2],
    /// [`GpuShape::half_uv`].
    half_uv: [f32; 2],
    sigma: f32,
    support: f32,
    opacity: f32,
    aa: f32,
    /// [`GpuShape::px_range`].
    px_range: f32,
    /// Non-zero when the four mask members describe one. Carried rather than
    /// inferred from `px_range` or `uv`, because a degenerate field is not an
    /// absent one.
    masked: u32,
    clip_offset: u32,
    clip_count: u32,
    /// The point the node turns about, in document space, and the angle it
    /// turns by (story #832) — [`Instance::rotation_pivot`] and
    /// [`Instance::rotation`], carried through unchanged.
    ///
    /// The backdrop blur is its own pipeline, so the rotation the packer stamps
    /// on the backdrop instance reaches the frosted region only through here. A
    /// pipeline that ignored it drew the frosted region upright under a rotated
    /// node, which is a silent wrong picture rather than an absent one.
    ///
    /// The pivot is first, at an eight-aligned offset, for the reason
    /// `Instance` states: WGSL aligns a `vec2f` to eight bytes and would place
    /// it differently from Rust behind an odd number of words.
    rotation_pivot: [f32; 2],
    rotation: f32,
    /// Declared, and always zero. Scalars rather than a `vec2u`, for the reason
    /// `GpuComposite` states: a vector member aligns to its own size and can
    /// move everything after it.
    _pad: [u32; 3],
}

// **Every offset, not only the size.** `shaders/blur.wgsl`'s `Blur` is this
// layout declared a second time in another language, and nothing in either one
// holds the two together. The size alone does not: reordering the four mask
// members in one and not the other during this story left both at 144 bytes
// and moved eight of the eighteen offsets, so every backdrop read its sigma out
// of a texture coordinate while this assertion stayed green. The offsets below
// are what a reorder now fails on.
const _: () = assert!(size_of::<GpuBlur>() == 160);
const _: () = assert!(align_of::<GpuBlur>() == 4);
const _: () = assert!(std::mem::offset_of!(GpuBlur, bounds) == 0);
const _: () = assert!(std::mem::offset_of!(GpuBlur, corners) == 16);
const _: () = assert!(std::mem::offset_of!(GpuBlur, quad) == 32);
const _: () = assert!(std::mem::offset_of!(GpuBlur, plane) == 48);
const _: () = assert!(std::mem::offset_of!(GpuBlur, uv) == 64);
const _: () = assert!(std::mem::offset_of!(GpuBlur, size) == 80);
const _: () = assert!(std::mem::offset_of!(GpuBlur, step) == 88);
const _: () = assert!(std::mem::offset_of!(GpuBlur, half_uv) == 96);
const _: () = assert!(std::mem::offset_of!(GpuBlur, sigma) == 104);
const _: () = assert!(std::mem::offset_of!(GpuBlur, support) == 108);
const _: () = assert!(std::mem::offset_of!(GpuBlur, opacity) == 112);
const _: () = assert!(std::mem::offset_of!(GpuBlur, aa) == 116);
const _: () = assert!(std::mem::offset_of!(GpuBlur, px_range) == 120);
const _: () = assert!(std::mem::offset_of!(GpuBlur, masked) == 124);
const _: () = assert!(std::mem::offset_of!(GpuBlur, clip_offset) == 128);
const _: () = assert!(std::mem::offset_of!(GpuBlur, clip_count) == 132);
const _: () = assert!(std::mem::offset_of!(GpuBlur, rotation_pivot) == 136);
const _: () = assert!(std::mem::offset_of!(GpuBlur, rotation) == 144);
// WGSL aligns a `vec2f` to eight bytes, so the pivot's offset must be a
// multiple of eight or the shader places it somewhere Rust does not.
const _: () = assert!(std::mem::offset_of!(GpuBlur, rotation_pivot) % 8 == 0);

/// What a frame's backdrop blurs draw through (story #733).
///
/// # Why the frame's own target moves in here
///
/// [`Renderer::draw`] is handed a [`wgpu::TextureView`], and
/// `copy_texture_to_texture` names a [`wgpu::Texture`] — so whatever the caller
/// is drawing into, this painter cannot snapshot it. A frame that holds a
/// backdrop therefore draws into [`base`](Self::base) and composites that into
/// the caller's view as its last act. That is the whole of the cost, it is paid
/// only by a frame that has a backdrop in it, and two of the three showcase
/// scenes have none.
///
/// A backdrop *inside a render-target group* snapshots that group's layer
/// instead, which is already a texture this painter owns — so the base is only
/// ever the frame's own target.
///
/// Held across frames for the reason [`LayerTargets`] is, and rebuilt when the
/// extent or the number of backdrops changes. The parameters are written every
/// frame regardless: a blur radius animates without changing how many backdrops
/// there are, and a stale uniform would blur at the previous frame's sigma.
#[derive(Default)]
struct BlurTargets {
    /// The frame's own target, when the frame holds a backdrop.
    base: Option<Owned>,
    /// The destination as it stood before the backdrop being resolved — the
    /// blur's input, and the sharp original the resolve pass mixes against.
    snapshot: Option<Owned>,
    /// The horizontal pass's output, which the vertical pass reads.
    scratch: Option<Owned>,
    /// The bind group that blits [`base`](Self::base) into the caller's view.
    /// One composite at alpha one, through the pipeline story #583 built.
    blit: Option<wgpu::BindGroup>,
    /// The alpha uniform that blit reads. Always one; held because a bind group
    /// must name a buffer that outlives it.
    blit_alpha: Option<wgpu::Buffer>,
    /// Two uniform buffers per backdrop the frame holds — the axis pass's, then
    /// the resolve pass's.
    ///
    /// Separate buffers rather than one written between passes, for the reason
    /// [`LayerTargets::alphas`] gives: writes queued against a single buffer
    /// inside one submission do not interleave with the passes that read it, so
    /// every blur in the frame would run at the last parameters written.
    uniforms: Vec<wgpu::Buffer>,
    /// Two bind groups per backdrop, in the same order as
    /// [`uniforms`](Self::uniforms).
    bind_groups: Vec<wgpu::BindGroup>,
    width: u32,
    height: u32,
    /// The atlas each backdrop's bind groups were built naming, in plan order,
    /// with `None` for a backdrop that carries no coverage mask.
    ///
    /// A bind group names one texture view, so a backdrop whose mask moved to a
    /// different atlas — or that gained or lost a mask — needs its groups
    /// rebuilt. Recorded rather than rebuilt every frame, which is what keeps
    /// a steady scene off the allocation path. Its length is also the count the
    /// buffers were built for.
    bound_atlases: Vec<Option<u32>>,
    /// Device objects allocated here, counted beside the others — see
    /// [`Renderer::allocations`].
    allocations: u64,
}

impl BlurTargets {
    /// Makes `count` backdrops' worth of targets and parameters available at
    /// `width` x `height`, rebuilding when any of those has changed or when the
    /// frame's buffers moved under the bind groups.
    ///
    /// A frame with no backdrop releases everything: the three full-target
    /// textures are the largest allocation this painter makes per frame, and
    /// holding them for a scene that stopped having a frosted panel would keep
    /// three drawable-sized textures alive for nothing.
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        blur_layout: &wgpu::BindGroupLayout,
        composite_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        masks: &[(Option<u32>, &wgpu::TextureView)],
        msdf_sampler: &wgpu::Sampler,
        clips: &wgpu::Buffer,
        rebind: bool,
    ) {
        let count = masks.len();
        if count == 0 {
            *self = Self {
                allocations: self.allocations,
                ..Self::default()
            };
            return;
        }
        let resized = self.width != width || self.height != height;
        if resized || self.base.is_none() {
            self.base = Some(Owned::new(
                device,
                "dashscene-gpu base",
                format,
                width,
                height,
            ));
            self.snapshot = Some(Owned::new(
                device,
                "dashscene-gpu backdrop snapshot",
                format,
                width,
                height,
            ));
            self.scratch = Some(Owned::new(
                device,
                "dashscene-gpu backdrop scratch",
                format,
                width,
                height,
            ));
            // Three textures and three views.
            self.allocations += 6;
            self.blit = None;
            self.width = width;
            self.height = height;
        }
        if self.blit.is_none() {
            let alpha = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dashscene-gpu base blit"),
                size: size_of::<GpuComposite>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.blit = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dashscene-gpu base blit"),
                layout: composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.base().view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: alpha.as_entire_binding(),
                    },
                ],
            }));
            self.blit_alpha = Some(alpha);
            self.allocations += 2;
        }
        // The bind groups name the clip buffer and one atlas view each, so a
        // frame that reallocated the buffer or moved a mask has to rebuild them
        // — the same reason `Frame::rebind` exists, one pipeline over.
        let atlases: Vec<Option<u32>> = masks.iter().map(|(atlas, _)| *atlas).collect();
        if resized || rebind || self.bound_atlases != atlases {
            self.uniforms.clear();
            self.bind_groups.clear();
            let snapshot = &self.snapshot.as_ref().expect("allocated above").view;
            let scratch = &self.scratch.as_ref().expect("allocated above").view;
            for (_, mask) in masks {
                // The axis pass reads the snapshot; the resolve pass reads the
                // axis pass's output. Both read the snapshot as the sharp
                // original, and only the resolve pass looks at it.
                for source in [snapshot, scratch] {
                    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("dashscene-gpu backdrop"),
                        size: size_of::<GpuBlur>() as wgpu::BufferAddress,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.bind_groups
                        .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("dashscene-gpu backdrop"),
                            layout: blur_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(source),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::TextureView(snapshot),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: uniform.as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 3,
                                    resource: clips.as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 4,
                                    resource: wgpu::BindingResource::TextureView(mask),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 5,
                                    resource: wgpu::BindingResource::Sampler(msdf_sampler),
                                },
                            ],
                        }));
                    self.uniforms.push(uniform);
                    self.allocations += 2;
                }
            }
            self.bound_atlases = atlases;
        }
    }

    /// The frame's own target for a frame that holds a backdrop.
    ///
    /// # Panics
    ///
    /// Panics when the frame holds none, which is a caller that asked for the
    /// base without planning a backdrop.
    fn base(&self) -> &Owned {
        self.base
            .as_ref()
            .expect("a frame with no backdrop draws into the caller's view")
    }

    /// The snapshot the blur reads, as a texture — the copy's destination.
    fn snapshot(&self) -> &Owned {
        self.snapshot.as_ref().expect("as `base`")
    }

    /// The horizontal pass's target.
    fn scratch(&self) -> &Owned {
        self.scratch.as_ref().expect("as `base`")
    }

    /// The axis pass's bind group and uniform for the `index`th backdrop of the
    /// frame, then the resolve pass's.
    fn pass(&self, index: usize, resolve: bool) -> (&wgpu::BindGroup, &wgpu::Buffer) {
        let at = index * 2 + usize::from(resolve);
        (&self.bind_groups[at], &self.uniforms[at])
    }
}

/// The buffers one frame binds, and the record of what they hold.
///
/// Held across frames rather than built per frame: R-T4 budgets a frame for a
/// dirty-range upload and a submission, and four buffer allocations, a texture,
/// a view and a bind group are none of those.
struct Frame {
    instances: wgpu::Buffer,
    /// The paint heap — solid colours then gradient rows. See [`paint_heap`].
    paints: wgpu::Buffer,
    clips: wgpu::Buffer,
    globals: wgpu::Buffer,
    strokes: wgpu::Buffer,
    images: wgpu::Buffer,
    glyph_runs: wgpu::Buffer,
    shapes: wgpu::Buffer,
    /// One bind group per atlas, plus [`Frame::NO_ATLAS`] at the front for a
    /// frame that samples none.
    ///
    /// They differ in exactly one entry — the texture view — so they are built
    /// together and rebuilt together: a bind group that named a stale buffer
    /// after a reallocation would draw one run of a frame from the previous
    /// frame's rows.
    bind_groups: Vec<wgpu::BindGroup>,
    /// How many atlases [`bind_groups`](Self::bind_groups) was built for, so a
    /// frame that created one rebuilds and a frame that did not does nothing.
    bound_atlases: usize,
    /// Capacities in elements, not bytes. A buffer is reallocated only when a
    /// frame needs more than it holds.
    instance_capacity: usize,
    paint_capacity: usize,
    clip_capacity: usize,
    stroke_capacity: usize,
    image_capacity: usize,
    glyph_run_capacity: usize,
    shape_capacity: usize,
    /// How many draw calls the frame most recently drawn took.
    last_runs: usize,
    /// What the instance buffer on the device currently holds, and the spans
    /// those rows were packed against. This is the record a partial upload is
    /// stated over: without it there is nothing to say what the device already
    /// has, and every frame would have to send everything.
    uploaded: Vec<Instance>,
    spans: Vec<InstanceSpan>,
    /// The globals currently on the device, so a frame whose extent and heap
    /// layout are both unchanged writes nothing.
    uploaded_globals: Globals,
    /// The commit the device's rows came from, or `None` when they came from a
    /// caller that named no commit. A frame may be applied incrementally only
    /// if it follows this one — see [`Changes`].
    uploaded_generation: Option<u64>,
    /// How the rows of the frame most recently drawn reached the device.
    last_upload: InstanceUpload,
    /// Device objects this frame has allocated — see [`Renderer::allocations`].
    allocations: u64,
}

/// The smallest buffer this painter allocates, in elements. A zero-sized
/// binding is a validation error rather than an empty draw, so every buffer
/// holds at least one element even when the frame it was built for has none.
const MINIMUM_CAPACITY: usize = 1;

impl Frame {
    /// The [`Frame::bind_groups`] entry that names no atlas.
    const NO_ATLAS: usize = 0;

    fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
    ) -> Self {
        let storage = |label: &'static str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let instances = storage(
            "instances",
            (size_of::<Instance>() * MINIMUM_CAPACITY) as u64,
        );
        let paints = storage(
            "paint heap",
            (size_of::<[f32; 4]>() * MINIMUM_CAPACITY) as u64,
        );
        let clips = storage(
            "clip boxes",
            (size_of::<GpuClipBox>() * MINIMUM_CAPACITY) as u64,
        );
        let strokes = storage(
            "strokes",
            (size_of::<GpuStroke>() * MINIMUM_CAPACITY) as u64,
        );
        let images = storage(
            "image fills",
            (size_of::<GpuImage>() * MINIMUM_CAPACITY) as u64,
        );
        let glyph_runs = storage(
            "glyph runs",
            (size_of::<GpuGlyphRun>() * MINIMUM_CAPACITY) as u64,
        );
        let shapes = storage(
            "coverage masks",
            (size_of::<GpuShape>() * MINIMUM_CAPACITY) as u64,
        );
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut frame = Self {
            instances,
            paints,
            clips,
            globals,
            strokes,
            images,
            glyph_runs,
            shapes,
            bind_groups: Vec::new(),
            bound_atlases: 0,
            instance_capacity: MINIMUM_CAPACITY,
            paint_capacity: MINIMUM_CAPACITY,
            clip_capacity: MINIMUM_CAPACITY,
            stroke_capacity: MINIMUM_CAPACITY,
            image_capacity: MINIMUM_CAPACITY,
            glyph_run_capacity: MINIMUM_CAPACITY,
            shape_capacity: MINIMUM_CAPACITY,
            last_runs: 0,
            uploaded: Vec::new(),
            spans: Vec::new(),
            // Zero on both axes, which no drawable is, so the first frame always
            // writes it.
            uploaded_globals: Globals::default(),
            uploaded_generation: None,
            last_upload: InstanceUpload::Whole { rows: 0 },
            // The eight buffers above.
            allocations: 8,
        };
        frame.rebind(device, layout, sampler, msdf_sampler, placeholder, &[]);
        frame
    }

    /// The bind group for a run that samples `atlas`, or none.
    fn bind_group(&self, atlas: Option<u32>) -> &wgpu::BindGroup {
        let index = match atlas {
            Some(atlas) => atlas as usize + 1,
            None => Self::NO_ATLAS,
        };
        &self.bind_groups[index]
    }

    /// Rebuilds every bind group over the buffers this frame now holds.
    ///
    /// One per atlas view plus the no-atlas one, all built here so that a
    /// reallocated buffer cannot be reflected in some of them and not others.
    #[allow(clippy::too_many_arguments)]
    fn rebind(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
        atlases: &[&wgpu::TextureView],
    ) {
        self.bind_groups.clear();
        for view in std::iter::once(placeholder).chain(atlases.iter().copied()) {
            self.bind_groups.push(bind(
                device,
                layout,
                &self.instances,
                &self.paints,
                &self.clips,
                &self.globals,
                &self.strokes,
                &self.images,
                &self.glyph_runs,
                &self.shapes,
                view,
                sampler,
                msdf_sampler,
            ));
            self.allocations += 1;
        }
        self.bound_atlases = atlases.len();
    }

    /// Puts this frame's data on the device, writing as little as it can, and
    /// reports whether any buffer moved.
    ///
    /// The return value is what a bind group built outside this struct needs:
    /// a reallocated buffer leaves every bind group naming the old one, and
    /// [`BlurTargets`] names `clips`. `Frame::rebind` handles this struct's own.
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        msdf_sampler: &wgpu::Sampler,
        placeholder: &wgpu::TextureView,
        residency: &Residency,
        buffer: &InstanceBuffer,
        heap: &[[f32; 4]],
        boxes: &[GpuClipBox],
        strokes: &[GpuStroke],
        resolved: &Resolved,
        globals: Globals,
        changes: Option<Changes<'_>>,
    ) -> bool {
        let mut rebind = self.upload_instances(device, queue, buffer, changes);

        // The two tables are written whole. A dirty set says which *rect*
        // changed and nothing about which table row did, so filtering these by
        // it would be reading it for a claim it does not make.
        if heap.len() > self.paint_capacity {
            self.paint_capacity = grown(heap.len());
            self.paints = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("paint heap"),
                size: (size_of::<[f32; 4]>() * self.paint_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.paints, 0, bytemuck::cast_slice(heap));

        if boxes.len() > self.clip_capacity {
            self.clip_capacity = grown(boxes.len());
            self.clips = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("clip boxes"),
                size: (size_of::<GpuClipBox>() * self.clip_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.clips, 0, bytemuck::cast_slice(boxes));

        if strokes.len() > self.stroke_capacity {
            self.stroke_capacity = grown(strokes.len());
            self.strokes = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("strokes"),
                size: (size_of::<GpuStroke>() * self.stroke_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.strokes, 0, bytemuck::cast_slice(strokes));

        // The three resolved tables, written whole for the same reason the two
        // above are. A frame that has none of a kind still writes one dead row,
        // because a zero-sized binding is a validation error.
        let dead_image = [GpuImage::default()];
        let gpu_images = or_dead(&resolved.images, &dead_image);
        if gpu_images.len() > self.image_capacity {
            self.image_capacity = grown(gpu_images.len());
            self.images = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image fills"),
                size: (size_of::<GpuImage>() * self.image_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.images, 0, bytemuck::cast_slice(gpu_images));

        let dead_run = [GpuGlyphRun::default()];
        let gpu_runs = or_dead(&resolved.runs, &dead_run);
        if gpu_runs.len() > self.glyph_run_capacity {
            self.glyph_run_capacity = grown(gpu_runs.len());
            self.glyph_runs = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("glyph runs"),
                size: (size_of::<GpuGlyphRun>() * self.glyph_run_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.glyph_runs, 0, bytemuck::cast_slice(gpu_runs));

        let dead_shape = [GpuShape::default()];
        let gpu_shapes = or_dead(&resolved.shapes, &dead_shape);
        if gpu_shapes.len() > self.shape_capacity {
            self.shape_capacity = grown(gpu_shapes.len());
            self.shapes = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("coverage masks"),
                size: (size_of::<GpuShape>() * self.shape_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            rebind = true;
        }
        queue.write_buffer(&self.shapes, 0, bytemuck::cast_slice(gpu_shapes));

        if globals != self.uploaded_globals {
            queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
            self.uploaded_globals = globals;
        }

        // A new atlas is as much a reason to rebuild as a reallocated buffer:
        // the bind groups are per atlas, and one that does not exist yet cannot
        // be bound.
        if rebind || residency.atlas_count() != self.bound_atlases {
            let views: Vec<&wgpu::TextureView> = (0..residency.atlas_count())
                .map(|index| residency.view(index as u32))
                .collect();
            self.rebind(device, layout, sampler, msdf_sampler, placeholder, &views);
        }
        rebind
    }

    /// Writes the instance rows, and reports whether the buffer was
    /// reallocated.
    ///
    /// # When a partial upload is sound, and when it is not
    ///
    /// Three things have to hold, and none of them is assumed.
    ///
    /// **This frame follows the one on the device.** A dirty set is stated
    /// against the commit before it, so it says nothing about a commit the
    /// device never received. [`Changes`] carries the generation for exactly
    /// this reason, and the arithmetic below — `held + 1` — is the whole of the
    /// check. A frame that skipped one, or that came from a fresh arena whose
    /// generations start again, is written whole.
    ///
    /// **The buffer has the same shape.** An instance is a function of the rect
    /// entry it was packed from, of the rows that entry names, and of the group
    /// stack the packer was in when it reached it. A rect the set leaves out can
    /// still have moved for a reason the set does not carry — a group opening
    /// over its range changes its instances' `layer` without touching its entry
    /// — and every such change moves a span, so comparing spans catches it.
    ///
    /// **The set names rects this buffer has.** A dirty index past the span
    /// table means the two disagree about what a rect index is.
    ///
    /// The debug assertion at the end holds the rest: it compares every row
    /// against what the device now has, so an assumption that stops being true
    /// fails a test run rather than leaving a stale quad on the screen of a
    /// release build. That is not hypothetical — it is what found the missing
    /// generation check, on a value that had converged and could never be
    /// reported again.
    fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buffer: &InstanceBuffer,
        changes: Option<Changes<'_>>,
    ) -> bool {
        let rows = buffer.instances();
        let spans = buffer.spans();

        let mut rebind = false;
        if rows.len() > self.instance_capacity {
            self.instance_capacity = grown(rows.len());
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instances"),
                size: (size_of::<Instance>() * self.instance_capacity) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.allocations += 1;
            // Nothing of the previous frame survives a reallocation, so the
            // partial path cannot apply to this frame.
            self.uploaded.clear();
            self.uploaded_generation = None;
            rebind = true;
        }

        let held = self.uploaded_generation;
        let ranges = match changes {
            // The commit the device already holds, handed over again — a forced
            // redraw with no tick between. The rows are a pure function of the
            // tables, so there is nothing to write.
            Some(changes) if held == Some(changes.generation) => Vec::new(),
            Some(changes)
                if held.map(|held| held + 1) == Some(changes.generation)
                    // The spans partition the buffer, so equal spans already
                    // imply an equal row count and no mutation of this line
                    // alone changes a frame's path. It stays as the bound that
                    // keeps the `self.uploaded[range]` indexing below in range
                    // without appealing to that invariant, which is held in
                    // another module.
                    && self.uploaded.len() == rows.len()
                    && self.spans == spans
                    && changes
                        .rects
                        .iter()
                        .all(|&rect| (rect as usize) < spans.len()) =>
            {
                dirty_ranges(changes.rects, spans)
            }
            _ => {
                queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(rows));
                self.uploaded.clear();
                self.uploaded.extend_from_slice(rows);
                self.spans.clear();
                self.spans.extend_from_slice(spans);
                self.uploaded_generation = changes.map(|changes| changes.generation);
                self.last_upload = InstanceUpload::Whole { rows: rows.len() };
                return rebind;
            }
        };

        let mut written = 0;
        for range in &ranges {
            queue.write_buffer(
                &self.instances,
                (range.start * size_of::<Instance>()) as wgpu::BufferAddress,
                bytemuck::cast_slice(&rows[range.clone()]),
            );
            self.uploaded[range.clone()].copy_from_slice(&rows[range.clone()]);
            written += range.len();
        }
        self.uploaded_generation = changes.map(|changes| changes.generation);
        self.last_upload = InstanceUpload::Ranges {
            ranges: ranges.len(),
            rows: written,
        };

        debug_assert!(
            self.uploaded == rows,
            "a row outside the dirty set changed, so the device now holds a stale instance: this \
             frame follows the one on the device and its spans match, so something the set does \
             not report has moved a row"
        );
        rebind
    }
}

/// The instance ranges a dirty set names, with adjacent ones merged.
///
/// One `write_buffer` per range rather than per rect: a scene where most rects
/// changed would otherwise queue one staging copy each, and consecutive rects
/// are consecutive in the buffer by construction.
///
/// `CommittedScene::dirty` is sorted and this does not require it to be — an
/// unsorted set merges less and writes the same bytes.
///
/// # Panics
///
/// Panics if a dirty index names no span. The caller checks that before
/// choosing this path, because a dirty set and a rect table that disagree are
/// two views of one frame that cannot both be right.
fn dirty_ranges(dirty: &[u32], spans: &[InstanceSpan]) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for &rect in dirty {
        let span = spans[rect as usize];
        // A layout-only container draws nothing. Its span still records where
        // the next rect begins, so it must not break the merge either.
        if span.count == 0 {
            continue;
        }
        let start = span.offset as usize;
        let end = start + span.count as usize;
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// `rows` unless it is empty, in which case the one dead row `dead` holds.
///
/// A zero-sized binding is a validation error rather than an empty draw, so a
/// frame that has no row of a kind still binds one. Nothing can name it: an
/// instance's row comes from the table it was packed against, and that table
/// had no rows.
fn or_dead<'a, T: Pod>(rows: &'a [T], dead: &'a [T; 1]) -> &'a [T] {
    if rows.is_empty() { &dead[..] } else { rows }
}

/// The capacity a buffer is grown to when a frame outgrows it.
///
/// Rounded up to a power of two, so a scene that adds a rect per frame
/// reallocates a logarithmic number of times rather than every frame.
fn grown(needed: usize) -> usize {
    needed.max(MINIMUM_CAPACITY).next_power_of_two()
}

/// Binds everything the shaders read. One place, so the bind group a frame is
/// built with and the one it is rebuilt with cannot drift apart.
#[allow(clippy::too_many_arguments)]
fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    paints: &wgpu::Buffer,
    clips: &wgpu::Buffer,
    globals: &wgpu::Buffer,
    strokes: &wgpu::Buffer,
    images: &wgpu::Buffer,
    glyph_runs: &wgpu::Buffer,
    shapes: &wgpu::Buffer,
    atlas: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    msdf_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dashscene-gpu paint"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: paints.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: strokes.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: images.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(atlas),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: glyph_runs.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: shapes.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: wgpu::BindingResource::Sampler(msdf_sampler),
            },
        ],
    })
}

/// Undoes the premultiplication the blend state produced.
///
/// `goldens/README.md` compares decoded pixels in unpremultiplied RGBA8888, and
/// `docs/decisions/golden-comparison-space.md` is why. A painter that returned
/// premultiplied bytes would be comparable only against itself.
fn unpremultiply(pixels: &mut [u8]) {
    for texel in pixels.chunks_exact_mut(4) {
        let a = texel[3];
        if a == 0 || a == 255 {
            continue;
        }
        for channel in &mut texel[..3] {
            *channel = ((*channel as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
        }
    }
}

/// The stroke rows, as the shader's array reads them. A `vec4f` puts the WGSL
/// struct's alignment at 16 and rounds its stride to 32, so a Rust type of any
/// other size would have every element after the first read from the wrong
/// offset — the same hazard the trailing word of `Instance` was declared for,
/// and the reason both are asserted rather than reasoned about.
const _: () = assert!(size_of::<GpuStroke>() == 32);

/// The instance rows, as bytes. `Instance` is `#[repr(C)]` with no padding
/// (`docs/decisions/instance-buffer-contract.md` D2), which is what lets the
/// buffer be cast rather than rebuilt.
const _: () = assert!(size_of::<Instance>() == 80);

/// The image rows, for the reason [`GpuStroke`]'s assertion gives — a `vec4f`
/// puts the WGSL struct's alignment at 16 and rounds its stride to 64, so a
/// Rust type of any other size makes every row after the first read from the
/// wrong offset. Added in review: this struct is read as `array<Image>` exactly
/// as the two above are read, and was the only one of the three pinned by
/// nothing.
const _: () = assert!(size_of::<GpuImage>() == 64);

/// The glyph-run rows and the coverage-mask rows, for the same reason: three
/// 16-byte-aligned groups each, so both strides are 48 and neither type may
/// change size without this failing.
const _: () = assert!(size_of::<GpuGlyphRun>() == 48);
const _: () = assert!(size_of::<GpuShape>() == 48);

/// And [`GpuShape`]'s offsets, because since issue #972 its last word carries
/// meaning rather than padding.
///
/// A size assertion alone does not pin a layout — [`GpuBlur`]'s own block says
/// so and calls itself the proof, having been reordered on one side at an
/// unchanged size. This struct is now the case that reasoning was written for:
/// swapping `px_range` and `resolved` on one side alone keeps it at 48 bytes,
/// and the shader would then read `resolved` out of the range slot — non-zero
/// for any real field, so the gate opens — and the range out of the flag slot,
/// where `1u32`'s bit pattern is 1.4e-45. `msdf_coverage(sample, ~0)` is `0.5`,
/// which is exactly the defect #972 removed, restored for every masked fill in
/// the frame and caught by nothing.
const _: () = assert!(std::mem::offset_of!(GpuShape, plane) == 0);
const _: () = assert!(std::mem::offset_of!(GpuShape, uv) == 16);
const _: () = assert!(std::mem::offset_of!(GpuShape, half_uv) == 32);
const _: () = assert!(std::mem::offset_of!(GpuShape, px_range) == 40);
const _: () = assert!(std::mem::offset_of!(GpuShape, resolved) == 44);

/// The per-frame uniform. **Thirty-two bytes on both sides since story #584**,
/// where it was sixteen: five scalars is twenty bytes, and a uniform's size
/// rounds up to a multiple of sixteen, so both declarations carry three pad
/// words to the same 32. A member added without a matching pad would make the
/// two disagree about where a base sits, and the symptom would be a gradient
/// reading solid colours as handles or a shadow reading a gradient's.
const _: () = assert!(size_of::<Globals>() == 32);
// Added in review: `composite.wgsl` reads this as a uniform, and it was the one
// struct crossing into WGSL that nothing pinned. A `vec3f` pad here would make
// it 32 — which is exactly the mismatch this catches at compile time rather
// than as a wgpu validation error at the first composite draw.
const _: () = assert!(size_of::<GpuComposite>() == 16);

#[cfg(test)]
mod tests {
    use super::{
        DrawRun, GRADIENT_WORDS, GpuGlyphRun, GpuImage, GpuShape, MINIMUM_CAPACITY, PAINT_WGSL,
        PaintHeap, Resolved, SHADOW_WORDS, dirty_ranges, draw_runs, gradient_kind, grown,
        paint_heap, scale_mode,
    };
    use crate::instance::{Instance, InstanceBuffer, InstanceKind, InstanceSpan};
    use dashpaint::{
        Color, EntryParts, FillSpec, Gradient, GradientKind, GradientStop, PaintEntry, PaintTable,
        ScaleMode, Shadow, ShadowKind, StopRange, Vec2,
    };

    /// A resolved frame whose image rows landed in `images`, whose glyph runs
    /// landed in `runs`, and whose coverage masks landed in `shapes`.
    ///
    /// Only the atlas maps matter to `draw_runs`; the row arrays are sized to
    /// match so that an index into either means the same thing.
    fn resolved(images: &[Option<u32>], runs: &[Option<u32>], shapes: &[Option<u32>]) -> Resolved {
        Resolved {
            images: vec![GpuImage::default(); images.len()],
            runs: vec![GpuGlyphRun::default(); runs.len()],
            shapes: vec![GpuShape::default(); shapes.len()],
            atlas_of_image: images.to_vec(),
            atlas_of_run: runs.to_vec(),
            atlas_of_shape: shapes.to_vec(),
        }
    }

    /// A resolved frame with image rows only — the shape every pre-#582 case
    /// here was written against.
    fn images_only(images: &[Option<u32>]) -> Resolved {
        resolved(images, &[], &[])
    }

    fn span(offset: u32, count: u32) -> InstanceSpan {
        InstanceSpan { offset, count }
    }

    /// A buffer of one rect whose instances are `kinds` paired with the rows
    /// they name. Built through the buffer's own API, so the rows are laid out
    /// the way the packer lays them out.
    fn buffer(kinds: &[(InstanceKind, u32)]) -> InstanceBuffer {
        let mut out = InstanceBuffer::new();
        out.begin_rect(0);
        for &(kind, row) in kinds {
            out.push(Instance {
                kind: kind.as_u32(),
                row,
                ..Instance::default()
            });
        }
        out
    }

    fn run(instances: std::ops::Range<u32>, atlas: Option<u32>) -> DrawRun {
        DrawRun { instances, atlas }
    }

    /// A frame whose image rows all sit in one atlas is one draw call, decided
    /// from the resolved rows without segmenting the buffer.
    #[test]
    fn one_atlas_is_one_run_over_the_whole_buffer() {
        let frame = buffer(&[
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
        ]);
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(0)])),
            vec![run(0..3, Some(0))]
        );
    }

    /// A frame with no image fill binds no atlas and is still one run.
    #[test]
    fn no_image_row_is_one_run_naming_no_atlas() {
        let frame = buffer(&[(InstanceKind::FillSolid, 0), (InstanceKind::Stroke, 0)]);
        assert_eq!(draw_runs(&frame, &images_only(&[])), vec![run(0..2, None)]);
    }

    /// A table row this frame does not draw contributes no atlas and no run.
    ///
    /// Residency follows the frame rather than the table, so an undrawn row is
    /// `None` — and a `None` counted as an atlas would conjure a run binding a
    /// texture nothing samples, or worse, split the frame around it.
    #[test]
    fn an_undrawn_image_row_contributes_no_run() {
        let frame = buffer(&[(InstanceKind::FillSolid, 0), (InstanceKind::FillImage, 1)]);
        // Row 0 exists in the table and no instance names it; row 1 is drawn.
        assert_eq!(
            draw_runs(&frame, &images_only(&[None, Some(0)])),
            vec![run(0..2, Some(0))]
        );
        // And a table whose rows are all undrawn is the no-atlas case.
        let solid_only = buffer(&[(InstanceKind::FillSolid, 0)]);
        assert_eq!(
            draw_runs(&solid_only, &images_only(&[None, None])),
            vec![run(0..1, None)]
        );
    }

    /// Two atlases split the frame where the atlas changes, and nowhere else.
    ///
    /// The boundary is at the *second* image instance rather than at the first,
    /// because everything before an atlas is first needed can be drawn with it
    /// bound. A split that started a run at every image instance would draw the
    /// same picture with more calls, so the count is what pins it.
    #[test]
    fn a_frame_mixing_two_atlases_splits_where_the_atlas_changes() {
        let frame = buffer(&[
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillSolid, 0),
        ]);
        // Row 0 is in atlas 0 and row 1 in atlas 1.
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(1)])),
            vec![run(0..3, Some(0)), run(3..5, Some(1))]
        );
    }

    /// Consecutive image instances of the same atlas do not split, even when a
    /// third atlas is in the frame — the run boundary follows the atlas, not the
    /// row.
    #[test]
    fn instances_sharing_an_atlas_stay_in_one_run() {
        let frame = buffer(&[
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillImage, 2),
        ]);
        // Rows 0 and 1 share atlas 0; row 2 is in atlas 1.
        assert_eq!(
            draw_runs(&frame, &images_only(&[Some(0), Some(0), Some(1)])),
            vec![run(0..2, Some(0)), run(2..3, Some(1))]
        );
    }

    /// The runs partition the buffer, in order, with no gap and no overlap.
    ///
    /// Stated separately from the cases above because it is the property that
    /// makes slice order still be draw order: a run boundary that dropped or
    /// repeated an instance would draw a wrong picture in a way a per-case
    /// expectation might not name.
    #[test]
    fn the_runs_partition_the_buffer_in_order() {
        let frame = buffer(&[
            (InstanceKind::FillImage, 0),
            (InstanceKind::FillImage, 1),
            (InstanceKind::FillSolid, 0),
            (InstanceKind::FillImage, 2),
            (InstanceKind::FillImage, 0),
        ]);
        let runs = draw_runs(&frame, &images_only(&[Some(0), Some(1), Some(0)]));
        assert_eq!(runs.first().expect("at least one run").instances.start, 0);
        assert_eq!(
            runs.last().expect("at least one run").instances.end,
            frame.instances().len() as u32
        );
        for pair in runs.windows(2) {
            assert_eq!(
                pair[0].instances.end, pair[1].instances.start,
                "the runs must meet exactly: {runs:?}"
            );
        }
    }

    /// The four scale modes are four distinct numbers, and they are the ones
    /// `paint.wgsl` compares against.
    #[test]
    fn the_scale_modes_are_distinct_and_match_the_shader() {
        let mapped = [
            scale_mode(ScaleMode::Fill),
            scale_mode(ScaleMode::Fit),
            scale_mode(ScaleMode::Crop),
            scale_mode(ScaleMode::Tile),
        ];
        assert_eq!(mapped, [0, 1, 2, 3]);
        let shader = include_str!("shaders/paint.wgsl");
        for (name, value) in [
            ("SCALE_FILL", 0),
            ("SCALE_FIT", 1),
            ("SCALE_CROP", 2),
            ("SCALE_TILE", 3),
        ] {
            assert!(
                shader.contains(&format!("const {name}: u32 = {value}u;")),
                "{name} must be {value} in the shader, which is what the Rust mapping assigns"
            );
        }
    }

    /// The four gradient kinds are four distinct numbers, and they are the ones
    /// `paint.wgsl` compares against — the same claim the scale modes make, for
    /// the same reason.
    ///
    /// The three shader-side strides ride along, because they are the other
    /// numbers stated twice. `MAX_GRADIENT_STOPS` is boundary B's, restated in
    /// `sdf.wgsl` so that the library stays free of Rust; `GRADIENT_WORDS` and
    /// `SHADOW_WORDS` are this file's heap strides, restated in `paint.wgsl`.
    /// None has a compiler holding it, and none fails visibly: a wrong stop
    /// ceiling truncates a ramp, a wrong gradient stride reads the previous
    /// gradient's stop colours as handles, and a wrong shadow stride reads a
    /// colour as an offset and a sigma — all three draw a plausible picture.
    #[test]
    fn the_gradient_kinds_are_distinct_and_match_the_shader() {
        let mapped = [
            gradient_kind(GradientKind::Linear),
            gradient_kind(GradientKind::Radial),
            gradient_kind(GradientKind::Angular),
            gradient_kind(GradientKind::Diamond),
        ];
        assert_eq!(mapped, [0, 1, 2, 3]);
        for (name, value) in [
            ("GRADIENT_LINEAR", 0),
            ("GRADIENT_RADIAL", 1),
            ("GRADIENT_ANGULAR", 2),
            ("GRADIENT_DIAMOND", 3),
            ("GRADIENT_WORDS", GRADIENT_WORDS as u32),
            ("SHADOW_WORDS", SHADOW_WORDS as u32),
        ] {
            assert!(
                PAINT_WGSL.contains(&format!("const {name}: u32 = {value}u;")),
                "{name} must be {value} in paint.wgsl, which is what the Rust side assigns"
            );
        }
        assert!(
            crate::shader::SDF_WGSL.contains(&format!(
                "const MAX_GRADIENT_STOPS: u32 = {}u;",
                dashpaint::MAX_GRADIENT_STOPS
            )),
            "sdf.wgsl's stop ceiling must be boundary B's {}",
            dashpaint::MAX_GRADIENT_STOPS
        );
    }

    fn colour(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    fn stop(offset: f32, colour: Color) -> GradientStop {
        GradientStop {
            offset,
            color: colour,
        }
    }

    /// A table holding two solids and two gradients that agree in nothing: a
    /// different kind, three different handles, a different stop count, and
    /// different stop offsets and colours.
    ///
    /// Every one of those axes is varied deliberately. A fixture whose two
    /// gradients agreed in any field could not tell a stride error from a
    /// correct read of that field, and a fixture with one gradient could not
    /// tell a stride error at all.
    fn two_gradients() -> PaintTable {
        let mut paints = PaintTable::new();
        paints.intern_fill(&FillSpec::Solid {
            color: colour(0.1, 0.2, 0.3, 1.0),
        });
        paints.intern_fill(&FillSpec::Solid {
            color: colour(0.4, 0.5, 0.6, 1.0),
        });
        paints.intern_fill(&FillSpec::Gradient {
            gradient: Gradient {
                kind: GradientKind::Linear,
                handle_origin: Vec2 { x: 0.1, y: 0.2 },
                handle_primary: Vec2 { x: 0.3, y: 0.4 },
                handle_secondary: Vec2 { x: 0.5, y: 0.6 },
                stops: StopRange::NONE,
            },
            stops: vec![
                stop(0.0, colour(1.0, 0.0, 0.0, 1.0)),
                stop(0.75, colour(0.0, 0.0, 1.0, 1.0)),
            ],
        });
        paints.intern_fill(&FillSpec::Gradient {
            gradient: Gradient {
                kind: GradientKind::Angular,
                handle_origin: Vec2 { x: 0.7, y: 0.8 },
                handle_primary: Vec2 { x: 0.9, y: 0.15 },
                handle_secondary: Vec2 { x: 0.25, y: 0.35 },
                stops: StopRange::NONE,
            },
            stops: vec![
                stop(0.2, colour(0.0, 1.0, 0.0, 1.0)),
                stop(0.5, colour(1.0, 1.0, 0.0, 0.5)),
                stop(0.9, colour(0.0, 1.0, 1.0, 0.25)),
            ],
        });
        paints
    }

    /// The heap's shape: solids at the head, gradients after them at a fixed
    /// stride, and the base that says where one ends and the other begins.
    #[test]
    fn the_paint_heap_puts_solids_first_and_gradients_at_a_fixed_stride() {
        let paints = two_gradients();
        let PaintHeap {
            words: heap,
            gradient_base,
            ..
        } = paint_heap(&paints);

        assert_eq!(
            gradient_base, 2,
            "the base is the solid count, so a heap with solids in it cannot report zero"
        );
        assert_eq!(heap.len(), 2 + 2 * GRADIENT_WORDS);
        assert_eq!(heap[0], [0.1, 0.2, 0.3, 1.0], "solid row 0 is heap word 0");
        assert_eq!(heap[1], [0.4, 0.5, 0.6, 1.0], "solid row 1 is heap word 1");
    }

    /// The second gradient's words, read at the stride the shader uses. This is
    /// the assertion a wrong stride fails: at any other stride these words are
    /// the *first* gradient's, which is a well-formed row and draws a picture.
    #[test]
    fn a_gradient_row_is_found_at_its_own_stride() {
        let paints = two_gradients();
        let PaintHeap {
            words: heap,
            gradient_base,
            ..
        } = paint_heap(&paints);
        let base = gradient_base as usize + GRADIENT_WORDS;

        assert_eq!(
            heap[base],
            [0.7, 0.8, 0.9, 0.15],
            "the origin and primary handles of the second gradient"
        );
        assert_eq!(
            heap[base + 1],
            [0.25, 0.35, 2.0, 3.0],
            "the secondary handle, the Angular kind, and three stops"
        );
        assert_eq!(
            heap[base + 2],
            [0.2, 0.5, 0.9, 0.0],
            "its three stop offsets, and a zero in the fourth slot"
        );
        assert_eq!(heap[base + 3], [0.0; 4], "offsets 4..7, all unused");
        assert_eq!(heap[base + 4], [0.0, 1.0, 0.0, 1.0], "stop 0's colour");
        assert_eq!(heap[base + 5], [1.0, 1.0, 0.0, 0.5], "stop 1's colour");
        assert_eq!(heap[base + 6], [0.0, 1.0, 1.0, 0.25], "stop 2's colour");
        for slot in 7..GRADIENT_WORDS {
            assert_eq!(
                heap[base + slot],
                [0.0; 4],
                "colour slot {slot} is past the count and is written as a zero"
            );
        }
    }

    /// The first gradient is where the base says it is, and it is not the
    /// second. Stated separately from the row above so that a base off by a
    /// stride fails one of the two rather than passing both.
    #[test]
    fn the_first_gradient_sits_at_the_base() {
        let paints = two_gradients();
        let PaintHeap {
            words: heap,
            gradient_base,
            ..
        } = paint_heap(&paints);
        let base = gradient_base as usize;

        assert_eq!(heap[base], [0.1, 0.2, 0.3, 0.4], "its origin and primary");
        assert_eq!(
            heap[base + 1],
            [0.5, 0.6, 0.0, 2.0],
            "its secondary handle, the Linear kind, and two stops"
        );
        assert_eq!(heap[base + 2], [0.0, 0.75, 0.0, 0.0], "its two offsets");
    }

    /// A frame with no fills and no shadows still uploads one word, because a
    /// zero-sized binding is a validation error.
    #[test]
    fn an_empty_paint_table_still_makes_a_bindable_heap() {
        let PaintHeap {
            words: heap,
            gradient_base,
            shadow_base,
        } = paint_heap(&PaintTable::new());
        assert_eq!(heap.len(), MINIMUM_CAPACITY);
        assert_eq!(gradient_base, 0);
        assert_eq!(
            shadow_base, 0,
            "with nothing before it the shadow region starts at the base too, and zero is a \
             real base rather than an absence"
        );
    }

    /// Two shadows that agree in nothing: a different offset on both axes, a
    /// different blur, a different spread, and a different colour on all four
    /// channels.
    ///
    /// Every axis varied for the reason [`two_gradients`] gives, and one more
    /// that is specific to this row: the shadow *kind* is deliberately **not**
    /// an axis here, because it never reaches the heap. A drop and an inner
    /// shadow differ by `InstanceKind`, so a fixture that varied the kind would
    /// be varying something these words do not carry.
    fn two_shadows() -> PaintTable {
        let mut paints = PaintTable::new();
        let mut push = |shadow: Shadow| {
            paints.push_with(
                PaintEntry::default(),
                EntryParts {
                    shadows: &[shadow],
                    ..EntryParts::default()
                },
            );
        };
        push(Shadow {
            kind: ShadowKind::Drop,
            offset: Vec2 { x: 3.0, y: -5.0 },
            blur: 8.0,
            spread: 2.0,
            color: colour(0.1, 0.2, 0.3, 0.4),
        });
        push(Shadow {
            kind: ShadowKind::Inner,
            offset: Vec2 { x: -7.0, y: 11.0 },
            blur: 16.0,
            spread: -4.0,
            color: colour(0.5, 0.6, 0.7, 0.8),
        });
        paints
    }

    /// The shadow region sits after the gradients, and a shadow's row is found
    /// at its own stride.
    ///
    /// **Two rows, because one cannot falsify a stride**: row 0 sits at the
    /// base whatever the multiplier is, so a stride of any value reads it
    /// correctly. Issue #715's whole gradient suite passed with the stride
    /// multiplied by anything until a second row was added.
    #[test]
    fn a_shadow_row_is_found_at_its_own_stride_after_the_gradients() {
        let paints = two_shadows();
        let PaintHeap {
            words: heap,
            gradient_base,
            shadow_base,
        } = paint_heap(&paints);

        assert_eq!(
            gradient_base, 0,
            "this table has no solids, so the gradient region starts at zero"
        );
        assert_eq!(
            shadow_base, 0,
            "and no gradients either, so the shadow region starts there too"
        );
        assert_eq!(heap.len(), 2 * SHADOW_WORDS);

        let base = shadow_base as usize;
        assert_eq!(
            heap[base],
            [3.0, -5.0, 3.5, 2.0],
            "the first shadow's offset, its sigma — 8 blur through 0.4375 — and its spread"
        );
        assert_eq!(heap[base + 1], [0.1, 0.2, 0.3, 0.4], "its colour");

        let second = base + SHADOW_WORDS;
        assert_eq!(
            heap[second],
            [-7.0, 11.0, 7.0, -4.0],
            "the second shadow's own geometry, at its own stride"
        );
        assert_eq!(heap[second + 1], [0.5, 0.6, 0.7, 0.8], "and its colour");
    }

    /// The shadow region begins after the gradient region, not over it.
    ///
    /// Stated with both kinds of table present because that is the only
    /// arrangement where the two bases differ: a fixture with shadows alone
    /// reports both at zero and cannot tell a shadow base that was written as
    /// the gradient base from a correct one.
    #[test]
    fn the_shadow_base_clears_the_gradient_region() {
        let mut paints = two_gradients();
        paints.push_with(
            PaintEntry::default(),
            EntryParts {
                shadows: &[Shadow {
                    kind: ShadowKind::Drop,
                    offset: Vec2 { x: 1.0, y: 2.0 },
                    blur: 4.0,
                    spread: 0.5,
                    color: colour(0.9, 0.8, 0.7, 0.6),
                }],
                ..EntryParts::default()
            },
        );
        let PaintHeap {
            words: heap,
            gradient_base,
            shadow_base,
        } = paint_heap(&paints);

        assert_eq!(gradient_base, 2, "two solids");
        assert_eq!(
            shadow_base as usize,
            2 + 2 * GRADIENT_WORDS,
            "the solids and both gradient rows come first"
        );
        assert_eq!(heap.len(), shadow_base as usize + SHADOW_WORDS);
        assert_eq!(
            heap[shadow_base as usize],
            [1.0, 2.0, 1.75, 0.5],
            "the shadow's own geometry, at a base past every gradient word"
        );
    }

    /// The sigma the row carries is the authored radius through the one mapping
    /// both painters share, and it is applied here rather than in the shader.
    ///
    /// `radius / 2` — the CSS convention this project measured against Figma
    /// and rejected (issue #412) — fails this: at radius 8 it gives 4.0 where
    /// the mapping gives 3.5.
    #[test]
    fn the_row_carries_figmas_sigma_rather_than_the_authored_radius() {
        let paints = two_shadows();
        let heap = paint_heap(&paints);
        let sigma = heap.words[heap.shadow_base as usize][2];

        assert_eq!(sigma, 8.0 * dashpaint::BLUR_SIGMA_PER_RADIUS);
        assert_ne!(
            sigma, 8.0,
            "the radius itself would blur four times too far"
        );
        assert_ne!(
            sigma, 4.0,
            "and the CSS convention twice as far as measured"
        );
    }

    /// A gradient over the vocabulary's stop budget is refused by name rather
    /// than stored eight stops deep, which is what `dashscene-skia` does with
    /// the same bound. `dashscene-validator` reports it upstream (P4), so a
    /// scene reaching here with nine stops was never validated.
    #[test]
    #[should_panic(expected = "gradient stop budget exceeded")]
    fn a_gradient_over_the_stop_budget_is_refused() {
        let mut paints = PaintTable::new();
        let stops: Vec<GradientStop> = (0..=dashpaint::MAX_GRADIENT_STOPS)
            .map(|i| {
                stop(
                    i as f32 / dashpaint::MAX_GRADIENT_STOPS as f32,
                    colour(1.0, 0.0, 0.0, 1.0),
                )
            })
            .collect();
        paints.intern_fill(&FillSpec::Gradient {
            gradient: Gradient {
                kind: GradientKind::Linear,
                handle_origin: Vec2 { x: 0.0, y: 0.0 },
                handle_primary: Vec2 { x: 1.0, y: 0.0 },
                handle_secondary: Vec2 { x: 0.0, y: 1.0 },
                stops: StopRange::NONE,
            },
            stops,
        });
        let _ = paint_heap(&paints);
    }

    /// The row a fill instance names is the row the shader indexes, so an
    /// instance built here is the one the runs are stated over.
    #[test]
    fn an_image_instance_names_its_table_row() {
        let frame = buffer(&[(InstanceKind::FillImage, 7)]);
        assert_eq!(frame.instances()[0].row, 7);
        assert_eq!(frame.instances()[0].kind, InstanceKind::FillImage.as_u32());
        let _ = Instance::default();
    }

    /// The property the merge exists for: rects that follow each other in the
    /// buffer are written as one copy.
    #[test]
    fn adjacent_dirty_rects_merge_into_one_range() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[0, 1, 2], &spans), vec![0..6]);
    }

    /// And the property that would make the merge wrong if it went further: the
    /// gap between two dirty rects is a rect that did not change, and its rows
    /// must keep the values the device already holds.
    #[test]
    fn a_clean_rect_between_two_dirty_ones_splits_the_range() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[0, 2], &spans), vec![0..2, 5..6]);
    }

    /// A layout-only container packs no instances. Its span still records where
    /// the next rect begins, so skipping it must not break the merge of the
    /// rects on either side of it.
    #[test]
    fn a_rect_that_draws_nothing_contributes_no_range() {
        let spans = [span(0, 2), span(2, 0), span(2, 1)];
        assert!(dirty_ranges(&[1], &spans).is_empty());
        assert_eq!(dirty_ranges(&[0, 1, 2], &spans), vec![0..3]);
    }

    /// `CommittedScene::dirty` is sorted, so this is not a case that arises —
    /// but the merge must not lose a write if it ever does. Every named rect's
    /// rows are still written; only the merging is worse.
    #[test]
    fn an_unsorted_dirty_set_still_writes_every_named_rect() {
        let spans = [span(0, 2), span(2, 3), span(5, 1)];
        assert_eq!(dirty_ranges(&[2, 0, 1], &spans), vec![5..6, 0..5]);
    }

    /// A buffer is never grown to nothing, because a zero-sized binding is a
    /// validation error rather than an empty draw.
    #[test]
    fn a_buffer_is_never_grown_to_nothing() {
        assert_eq!(grown(0), MINIMUM_CAPACITY);
        assert_eq!(grown(1), 1);
        assert_eq!(grown(3), 4);
        assert_eq!(grown(4), 4);
        assert_eq!(grown(5), 8);
    }
}
