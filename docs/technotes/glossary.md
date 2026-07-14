# Technote — glossary

    status   reference, 2026-07-13. Accompanies producers-and-ir.md,
             rendering-and-painters.md, runtime-content.md and the specs in
             specs/DESIGN_1.md / specs/SCOPE_DECISIONS.md. Definitions are scoped
             to how the terms are used in this project.
    note     working names retired: the document/IR is **dashscene** (serialized
             as **`.dsb`**), the compiler is **`dashc`**. Older drafts said "SCD"
             and "scdc"; those are not used here. Ruling:
             `docs/decisions/dashscene-document-is-the-ir.md`.

## Project terms

**arena** — the in-memory node tree owned by `dashscene-core`; the live document
a producer mutates through the staged-mutation API. `.dsb` is one way to populate
it; the arena is "the real contract" (DESIGN §4).

**atlas** — a texture holding baked distance fields (MSDF glyphs, icons). Quads
sample it. See _SDF_, _MSDF_, _baked_.

**baked / baking** — compile-time (`dashc`) conversion of arbitrary vector or
effects (complex paths, drop/inner shadows, Lottie frames) into atlas/mesh/texture
assets, so the runtime only draws quads. The alternative to a runtime vector
engine.

**boundary A** — the `.dsb` load gate: version check + per-section hashes before a
document is trusted (DESIGN §4).

**boundary B** — the painter contract: rect table + positioned glyph runs + paint
indices. A painter never measures, wraps, kerns, or moves anything (P2).

**commit** — the arena operation that publishes staged mutations: swaps the double
buffer, bumps the generation stamp, updates the dirty set. Part of the
staged-mutation API.

**dashscene** — the intent-only intermediate representation (IR): the semantic
model of a scene (layout tables, paint tables, variant tables, text), carried by
`dashscene-core` in memory and serialized as `.dsb`. Producer-neutral by design
(P5).

**`.dsb`** — the flatbuffer document format ("dashscene buffer"), owned by the
`dashbuf` crate. One schema serves both the on-disk _file role_ (mmap sections +
hashes) and the _wire role_ (length-prefixed messages for streaming) — SCOPE §3.

**`dashc`** — the compiler: lowers a producer's input into `dashscene`, validates
it, and emits `.dsb` (or a diagnostics report). Runs native (CLI/CI) and compiled
to wasm (called from the Deno Figma importer). The build-time stage.

**crates (roles, SCOPE §2)** — `dashscene` (umbrella/facade), `dashscene-core`
(arena + semantic model + staged-mutation API), `dashscene-engine` (Taffy solve,
variants, FLIP, measure), `dashscene-typeset` (bidi/shaping/atlas), `dashbuf`
(the `.dsb` schema), `dashpaint` (paint table + painter trait = boundary B),
`dashscene-skia` (Skia reference painter), `dashcue` (descriptive animation
vocabulary + scheduling), `dashlang` (Rust DSL + corpus generator), `dashc`
(compiler), `dashscene-unity` (Unity FFI bindings), `dashscene-web` /
`dashscore` / `dashscene-compose` (parked).

**dirty set** — the set of changed rect/glyph entries, enabling per-frame upload
of only what moved (R-T4).

**double buffer** — the generation-stamped two-copy output of rect entries + glyph
runs the painter consumes, so a commit never tears a frame in flight.

**FLIP** — First-Last-Invert-Play: animate a layout change by measuring before and
after and interpolating the delta, rather than animating layout every frame.

**glyph run** — positioned glyphs (glyph id, x, y, size, atlas page) produced once
by the typesetter and drawn by every painter as atlas quads. See _typesetter_.

**painter** — a backend behind boundary B that only _colours_ finished rects and
quads. Painters: Skia (reference + entry-tier candidate), Unity (high-end), the
lean native painter (later), tiny-skia (wasm). A painter swap is a re-golden, not
a redesign.

**placeholder** — a node that reserves a declared-size box and is filled at bind
time by a registered producer/implementation (DESIGN §10.2). Fillable by a
streamed subtree, a decoded image, or a ThorVG texture (see runtime-content).

**producer** — any source that populates `dashscene`: the Figma importer, the code
DSLs (`dashlang`, the C#/Kotlin skins), or a streaming producer. Each chooses the
compile path or the arena path.

**profile (profile:full / profile:core)** — a named paint-vocabulary subset a
painter honours. `profile:full` = Unity-class (runtime blur, backdrop blur);
`profile:core` = lean/native painters (blur banned or budgeted, shadows baked).
The validator enforces it (P4).

**rect table** — the resolved rectangles the runtime produces for painters; its
index equals the document node index. Carries geometry, never pixels (P1).

**staged-mutation API** — `open` / `set_prop` / `set_variant` / `commit` on the
`dashscene-core` arena — the producer contract (lives in core, not `dashcue`;
SCOPE §9).

**typesetter** — the one shared text pipeline (bidi split → rustybuzz shape → line
break → positioned glyph runs). Runs once in Rust; painters never typeset (P2,
R1).

**variant** — a member of a component set; the variant table carries sparse
per-variant overrides, never duplicate trees. `set_variant` is the structural
switch; how it animates is `dashcue` data.

**waiver** — an explicit, recorded exception that lets a strict build proceed past
a specific validator diagnostic (v0.7).

## Graphics terms

**AA (antialiasing)** — smoothing the jagged edge of a shape. In SDF rendering it
comes from the distance value (a soft band around the zero-crossing), and must be
computed in linear colour space to keep text weight correct.

**ASTC / ETC2 / EAC** — GPU-native compressed texture formats (mobile). **KTX2 /
Basis (UASTC / ETC1S)** — the distribution container/codecs transcoded to ASTC/EAC
at install/prefetch. Distance-field atlases are never lossy-compressed.

**bidi (bidirectional text)** — mixing right-to-left (Arabic, Hebrew) and
left-to-right runs; requires run splitting before shaping (`unicode-bidi`).

**BRG (BatchRendererGroup)** — Unity's low-level API to draw many instances of a
mesh+material from a native buffer, with no GameObject/Transform per instance. The
data-oriented alternative to GameObject-per-node; can be fully lit/shadowed via
shader passes (rendering-and-painters §10).

**Burst** — Unity's AOT compiler for a C# subset (HPC#): unmanaged, SIMD, GC-free.
Used with `Unity.Collections` to build the painter's instance buffers off the
managed heap.

**fragment shader (pixel shader)** — the per-pixel program the GPU runs on every
pixel a triangle covers; where an SDF is evaluated and the pixel coloured.

**GC (garbage collection)** — automatic memory reclamation (Compose/ART,
Flutter/Dart, Unity/Boehm). A source of frame-time pauses; avoided by zero-alloc
steady state, not removed by IL2CPP.

**GLES (OpenGL ES)** — the embedded/mobile graphics API; the target is GLES 3.2 on
tiling GPUs.

**IL2CPP** — Unity's AOT pipeline (C# → C++ → native): fast, JIT-free (good for
locked-down/certified OSes), but still ships a garbage collector.

**instancing / instanced quads** — drawing one mesh (a quad) many times from a
table of per-instance data (transform + paint params); minimal draw calls and
state changes.

**lit / unlit** — _unlit_: the authored colour is the on-screen colour, absolute
(normal 2D UI). _lit_: the final colour is computed from the material colour plus
scene lighting (shadows, highlights, scene-dependent). UI defaults to unlit; lit is
opt-in per node for physical presence in a 3D scene (Unity only). See the material
classes below.

**material class (Unity)** — `unlit-overlay` (HUD-style, excluded from lighting),
`lit-opaque` (full lighting, casts/receives shadows), `lit-cutout` (SDF alpha-clip,
shadow-caster with clip). The per-node lighting-participation choice (DESIGN §8.2).

**MSDF (multi-channel signed distance field)** — an SDF variant using R/G/B
channels to preserve sharp corners that single-channel SDF rounds off; produced by
`msdf-atlas-gen`. The crispest distance-field text/shape representation.

**premultiplied alpha** — a blending convention where colour channels are
pre-scaled by alpha; painters must agree on it to avoid edge halos.

**quad** — a rectangle made of two triangles; the drawing unit of the painter,
since almost every UI node lives in a rectangular box.

**RT (render target)** — an off-screen buffer. On a tiling GPU each mid-frame RT
switch is a tile-memory flush (costly); the lean painter avoids them by baking
effects. See _saveLayer_, _tiling GPU_.

**saveLayer** — a 2D-API operation (Skia) that allocates an off-screen layer (an
RT) to composite an effect (blur, group opacity, mask); the main source of
tiling-GPU bandwidth cost in a general renderer.

**SDF (signed distance field / function)** — representing a shape by, for each
point, the distance to its edge (negative inside, positive outside). Evaluated per
pixel in a shader, it gives cheap, resolution-independent, antialiased shapes and
text without runtime path tessellation. "SDF-parametric" = the shape is a formula
over per-instance parameters (rounded rect, ellipse, stroke, gradient).

**SH (spherical harmonics)** — a compact representation of ambient/probe lighting;
BRG instances must be fed SH per-instance for baked GI (GameObjects get it free).

**SRP (Scriptable Render Pipeline)** — Unity's configurable render pipeline (URP /
HDRP / custom); lit/shadow passes live here and operate on draw calls, not
GameObjects.

**SSR (screen-space reflections)** — a reflection technique; transparent surfaces
cannot receive it (a technique limit, not a bug).

**tessellation** — converting vector paths (Béziers) into triangles the GPU can
draw; the expensive per-frame work a general vector renderer does and the quad
painter avoids.

**tiling GPU** — the mobile/embedded GPU architecture (the target) that renders the
screen in small on-chip tiles. It rewards one render pass per frame and punishes
mid-frame RT switches and bandwidth — the cost model the whole painter design is
shaped around (DESIGN §9).

**tone mapping** — mapping HDR scene colours to the display range (ACES, Neutral);
it shifts colours, so authored UI colours only survive it if unlit-overlay UI
bypasses the tone-mapped pass (rendering-and-painters §9).

## Tools & external tech

**Compose (Jetpack Compose)** — Android's Kotlin reactive UI toolkit; runs on ART,
uses recomposition. Referenced for the code-DSL ergonomics and the perf comparison.

**Figma** — the primary design source; accessed via its REST API
(`@figma/rest-api-spec`). Its auto-layout needs Figma≠CSS lowering in `dashc`.

**Flutter** — Google's Dart UI framework with its own renderer (Skia, now
Impeller); used in automotive (Toyota). The perf comparison baseline.

**Glance** — a Jetpack library that builds Android widgets / Wear tiles with the
Compose runtime but renders _remotely_ (its composition emits a tree translated to
RemoteViews / protobuf). The template for a streaming producer into a placeholder
(runtime-content §3).

**HarfBuzz / rustybuzz** — the industry text-shaping engine / its pure-Rust port.
`rustybuzz` _is_ HarfBuzz's algorithm in Rust, so shaping quality equals what Skia
would produce.

**Impeller** — Flutter's precompiled-shader renderer replacing Skia, built largely
to remove shader-compilation jank — evidence that frame-time predictability is hard
for general engines.

**Lottie** — a JSON vector-animation format (After Effects via Bodymovin). Triaged
in `dashc`: transform-only → `dashcue` + baked SDF; canned → sprite-sheet; full
vector → ThorVG (runtime-content §4).

**lyon** — a Rust path-tessellation library; used at build time to bake arbitrary
paths into meshes.

**Penpot** — an open-source, self-hostable design tool whose layout _is_ literal
CSS Flexbox/Grid. Candidate second producer via the arena path, deferred post-v0
(producers-and-ir §4).

**resvg / usvg / tiny-skia** — the pure-Rust SVG parse / simplify / raster stack
(same author as `ttf-parser` / `rustybuzz`); preferred over ThorVG for offline SVG
baking. `tiny-skia` is also the parked wasm painter (DESIGN §8.4).

**Skia** — Google's 2D graphics library (BSD), via `skia-safe`. The reference
painter (CPU raster = bit-exact goldens; GPU on GLES = the entry-tier bridge). Its
typesetter and codecs are trimmed out (rendering-and-painters §6).

**Slint** — a Rust declarative GUI toolkit (GPLv3 / commercial-embedded). Different
problem (renders itself) and its licence is incompatible with the MIT stack —
reference for ideas only, never adopted or code-borrowed (producers-and-ir §5).

**Taffy** — the pure-Rust CSS flex/grid layout solver; the sole layout engine for
all backends (covers all four Figma auto-layout modes).

**TextMeshPro (TMP)** — Unity's SDF text solution; the Unity painter uses its
rendering style ("TMP-style") driven from the shared atlas, not its typesetter.

**ThorVG** — a lightweight (~150KB) MIT vector-graphics engine (SVG + Lottie),
SW/GL backends, embedded-proven. Used as a runtime render-to-texture escape hatch
for arbitrary runtime vector, and as an offline Lottie frame-renderer — never a
general painter (runtime-content §5–6).

**ttf-parser** — a pure-Rust font-table / metrics parser; supplies the same numbers
FreeType would, feeding the typesetter and atlas.

**Unity** — the game engine hosting the high-end product painter (lit, world-space,
3D). The heaviest backend by design; used only where lit/3D is required and the
hardware affords it.

**unicode-bidi** — the pure-Rust implementation of the Unicode bidirectional
algorithm; splits mixed RTL/LTR text into runs before shaping.

## DESIGN_1 shorthand

**Principles (P).** P1 the document carries intent, never results. P2 one solver,
one typesetter; painters only colour. P3 producers mutate, the runtime owns time.
P4 vocabulary is validated, never discovered. P5 Figma compatibility is a property
of one producer.

**Requirements (R).** R1 perfect Arabic text, identical on every backend, high
performance. R2 full Figma auto-layout vocabulary. R3 GPU is the bottleneck; the
native painter uses far less memory/CPU than the engine; backend selection is
whole-scene. R4 animation reproducible + statically provable frame cost. R5
documents load fast (cold-start proportional to what's shown; mmap). R6 unsupported
vocabulary is a named diagnostic, never a silent drop. R7 reproducible builds: same
input → byte-identical document.

**Target-hardware rules (R-T).** R-T1 one render pass per frame; RT switches are
tile flushes; blurs budgeted. R-T2 split quads into an opaque z-tested core + thin
AA fringe. R-T3 compress every texture/framebuffer the driver allows. R-T4 CPU
frame cost = dirty-range instance-buffer upload + submit. R-T5 SDF shader math
single-sourced into both painters' shading languages.

**Open questions (Q).** Q-1 MSDF vs per-size bitmap atlas below ~14px. Q-2
KHR_blend_equation_advanced availability/perf. Q-3 a declared state-machine
(Rive-class) layer, or is instantiate+bind+transitions enough. Q-4 Taffy baseline
alignment. Q-5 remote-producer admission scope. Q-6 group-opacity RT budget value.
