# Future painter: a wgpu quad + SDF backend

    status   WIP — direction, NOT a commitment. Design-discussion capture
             plus verified ecosystem research (2026-07-19, user + Opus).
             The web painter is parked and the lean native painter is
             "later" per AGENTS.md; this note exists so the question is
             not researched from scratch when that slice opens. Nothing
             here is implemented and no crate has been adopted.
    scope    whether a wgpu painter is a viable alternative to the parked
             tiny-skia web painter and a candidate for the lean native
             painter; what would have to be written vs adopted
    builds on docs/technotes/rendering-and-painters.md (§1 quad model),
             docs/specification/03-target-hardware-rules.md,
             docs/design/architecture.md (boundary B)

All versions, dates, and download figures below were verified against
the crates.io and GitHub APIs on 2026-07-19.

## Why this is even a short conversation

Our painter vocabulary is already GPU-shaped. The `Painter` trait takes
`paint(rects, paints, images, clips, groups, glyphs, dirty)`
(`crates/dashpaint/src/lib.rs:845-853`) — seven parallel tables and **no
path primitive**. Vector shapes are baked MSDF fields, not live path
rasterisation (`docs/decisions/baked-vector-msdf-field.md`;
`crates/dashscene-skia/src/lib.rs:513`).

This matters because it removes the hard problem. A general 2D renderer
must solve anti-aliased rasterisation of arbitrary Bézier paths. We do
not have arbitrary paths. Every primitive we do have maps onto an
instanced quad with a fragment shader:

| Primitive          | Technique                                             |
| ------------------ | ----------------------------------------------------- |
| rounded rect + AA  | analytic rounded-box SDF in the fragment shader       |
| gradients          | fragment shader                                       |
| images             | texture sample                                        |
| glyphs             | existing MSDF atlas, textured quads                   |
| MSDF vector fields | median-of-3 + smoothstep, the same resolve            |
| clips              | scissor, or a per-instance clip rect in the shader    |
| group opacity      | render to texture, composite once                     |
| shadows            | analytic blurred rounded-rect SDF (see below)         |
| backdrop blur      | render backdrop to texture, ping-pong blur, composite |

That is conventional GPU UI rendering, not a research problem.

## Vello is the wrong tool

Ruled out, with reasons, so it is not re-proposed:

- The README on `main` still states Vello is **alpha**, and lists blur
  and filter effects among its open gaps — precisely what we would want
  it for.
- It renders glyphs as **outlines**. Our design is an MSDF atlas. That
  is an architectural opposite, and Vello exposes no custom-shader
  extension point to add MSDF through.
- Classic `vello` requires compute shaders and states "the web is not
  currently a primary target".
- `vello_hybrid` **does** have a real WebGL2 path (CPU path processing,
  GPU fragment/vertex rasterisation, with a `vello_sparse_shaders`
  WGSL-to-GLSL step). Correcting an earlier claim in discussion that no
  WebGL2 fallback existed. But it is at 0.0.9, self-described by
  Linebender as "roughly beta quality", with no API stability guarantee.

Adopting Vello would mean pushing rounded rectangles through a Bézier
flattener to reach a problem a quad already solves.

## No drop-in crate exists

Verified. The quad + SDF painters that exist are welded into UI
frameworks:

- **`iced_wgpu`** does use analytic SDF — Inigo Quilez's `sdRoundedBox`,
  in `wgpu/src/shader/quad.wgsl`. Framework-internal; `iced_graphics`
  contains no quad shader and no SDF code. Not extractable, but it is
  only about 4 KB of WGSL.
- **GPUI** (Zed) is the same architecture. It **removed blade and moved
  to wgpu** in PR #46758, merged 2026-02-13: Metal on macOS, wgpu on
  Linux, HLSL on Windows, plus a new `gpui_web` wasm target. GPUI is
  **Apache-2.0** (the Zed application is GPL; GPUI is not), and
  `crates/gpui_wgpu/src/shaders.wgsl` is a single self-contained file.
  It is the best available reference implementation.
- **`egui`/`epaint`** CPU-tessellates rounded rects from precomputed
  circle lookup tables with vertex-based feathering. Different
  architecture; not a model to follow for rect rasterisation.

Standalone candidates and why none is adopted:

- **`vger`** ([audulus/vger-rs](https://github.com/audulus/vger-rs), MIT)
  is architecturally exactly this — SDF quads, standalone by design. But
  last crates.io release 2024-09-07, last push 2025-10-27, images
  unimplemented. Adopting it means owning a wgpu-version treadmill.
- **`femtovg`** v0.25.1 (2026-05-29) is healthy, rendering-only, and now
  has a wgpu backend — but it is a NanoVG-style tessellating path
  renderer, not SDF quads.
- **`wgpu_canvas`**, **`contrast_renderer`** — too small or too quiet to
  depend on.

The conclusion is not an ecosystem gap. Boundary B says painters only
colour (P2); the solver and typesetter are singular and live elsewhere.
Every framework above wants to own layout, text, and scene state, which
is exactly what P1/P2/P3 forbid a painter to have. A painter that only
colours is a thin translation of our paint table into draw calls — that
is write-it-yourself territory by design.

## Techniques worth taking, and two to avoid

**Blurred rounded-rect shadows have a closed form.** Evan Wallace's
[fast rounded rectangle shadows](https://madebyevan.com/shaders/fast-rounded-rectangle-shadows/)
uses the gaussian integral (an `erf` approximation) analytically along
one axis and a small number of weighted samples along the other; GPUI
implements it essentially verbatim, with four samples over a ±3σ range.
Raph Levien's
[blurred rounded rectangles](https://raphlinus.github.io/graphics/2020/04/21/blurred-rounded-rects.html)
is fully closed form with no sampling loop, folding the blur radius into
an effective corner radius. Levien's is likely better for us; Wallace's
has far more production mileage. Levien describes his constants as
empirically tuned, so either choice should be validated against a real
multi-pass blur before it is trusted.

**Do not copy iced's shadow.** It is `1.0 - smoothstep(-blur, blur,
max(dist, 0.0))` — a smoothstep ramp, not a gaussian approximation.
Given our render oracle measures shadow falloff against Figma
(`blur-falloff` band, currently 0.043% and 0.000%), iced's shadow would
very likely not hold the band.

**Do not copy GPUI's group opacity.** GPUI multiplies each primitive's
alpha independently at submission time rather than compositing the group
offscreen. That is only equivalent to true group opacity when the
group's contents do not overlap; where they overlap, a lower element
shows through. Our `GroupComposite` render-to-texture
(`crates/dashpaint/src/lib.rs:774-782`) is the correct approach, matches
Figma's compositing-group semantics, and is the same machinery backdrop
blur needs. We are ahead of GPUI here.

**Clipping.** iced uses `set_scissor_rect` — axis-aligned only, and a
scissor change forces a new draw call. GPUI carries a per-instance
`content_mask` evaluated in the shader, which does not break batching.
GPUI's approach is the better model. Neither supports rounded clips
through the general mechanism; the pattern to generalise is carrying one
rounded-rect clip as SDF parameters per instance and multiplying
coverage.

## Helper crate stack (verified current)

| Crate          | Version | Date       | Note                                             |
| -------------- | ------- | ---------- | ------------------------------------------------ |
| `wgpu`         | 30.0.0  | 2026-07-01 | see breaking changes below                       |
| `bytemuck`     | 1.25.2  | 2026-07-19 | wgpu itself depends on it                        |
| `glam`         | 0.33.2  | 2026-06-28 | `cgmath` is dead (last release 2021)             |
| `encase`       | 0.12.0  | 2025-09-12 | uniform/storage layout; slow-releasing but alive |
| `etagere`      | 0.3.0   | 2026-03-18 | glyph atlas packing with eviction                |
| `guillotiere`  | 0.7.0   | 2026-03-18 | better for heterogeneous image atlases           |
| `wgsl_to_wgpu` | 0.19.0  | 2026-07-06 | build-script codegen for typed bind groups       |
| `naga_oil`     | 0.23.0  | 2026-07-14 | WGSL imports, `#ifdef`, module composition       |

Practical notes:

- **`encase` 0.12 removed trait implementations on foreign types.** The
  dependency direction inverted: enable _glam's_ `encase` feature, not
  encase's `glam` feature. There is also an unreleased UB fix pending in
  `SizeValue::mul`.
- **wgpu 30.0.0 has a silent WGSL break**: integer shader I/O no longer
  defaults to `@interpolate(flat)` and must be stated explicitly. Also
  `VertexState::buffers` and `PipelineLayoutDescriptor::bind_group_layouts`
  became `Option`-wrapped slices. The 29 line is still being backported
  (29.0.4 shipped after 30.0.0), so it is a viable conservative pin.
- **`etagere` is the right atlas packer** for a dynamic glyph atlas with
  eviction; `glyphon` pairs `etagere` with `lru` for exactly that, which
  is a battle-tested design to copy.
- **Render-graph crates are a dead category** — `rend3` is archived,
  `rafx` is stale, Bevy's is inseparable from its ECS. Write our own
  pass sequencing.
- **No MSDF rendering crate exists.** It is roughly ten lines of WGSL.
  The canonical reference is the WebGPU samples'
  [`msdfText.wgsl`](https://github.com/webgpu/webgpu-samples/blob/main/sample/textRenderingMsdf/msdfText.wgsl).
  Note that our `px_range` is 4 and our screen-pixel range is derived
  from a uniform rather than from `fwidth` derivatives
  (`crates/dashscene-skia/src/lib.rs:333`), which is what Chlumsky
  recommends for 2D and avoids a documented NaN failure mode in the
  compact derivative form.

## Cautions

- **A quad + SDF painter may not stay pathless.** Both GPUI and Flutter's
  Impeller ended up carrying a real path rasteriser alongside their quad
  pipeline (GPUI has `vs_path_rasterization` and a `lyon` dependency). If
  Figma vector networks ever need live path rendering rather than baked
  MSDF fields, we would land in the same place. Pathlessness is a
  property of the current vocabulary, not a permanent guarantee.
- **A wgpu painter will not pixel-match Skia.** Different AA, different
  gradient dithering, different blur falloff. The render oracle would
  need per-painter tolerance bands. This is a tuning exercise rather than
  a rewrite of the fidelity story, but it is real work and it is the most
  repo-specific risk on this list.
- **The lift is the painter, not the shaders.** The shader set is small.
  The bulk of the work is pipeline and bind-group management, instance
  buffer packing, atlas residency and eviction, and mapping our `clips`
  and `groups` tables onto scissor/stencil and render-to-texture.

## What this note does not decide

Adopting wgpu is a painter-strategy decision — GPU performance, one Rust
codebase covering native and web, dropping the Skia C++ dependency for
the lean target. It should be made on those grounds and recorded in
`docs/decisions/`, not inherited as a side effect of the backdrop-blur
discussion that produced this note. Backdrop blur becoming easier on a
GPU painter is a consequence, not a justification.
