# Technote — the rendering model & the painters

    status   design note, 2026-07-13. Captures conclusions from a design
             discussion; extends docs/archive/2026-07-14-design-1-seed.md
             and docs/archive/2026-07-14-scope-decisions.md without
             superseding them. DECISION = settled; CANDIDATE / OPEN =
             not.
    scope    what the SDF-quad painting model is and why it is fast; how the
             product backends are tiered (Unity / Skia / lean); and the internals
             of the Unity painter (quality, calibration, resource cost, BRG).

## 1. The SDF-quad-atlas model — what it actually is

"Quads (SDF-parametric + atlas + baked)" is neither runtime vector graphics nor
naive raster. It is a **specialised GPU compositor over a fixed primitive
vocabulary**. A GPU does two things: place triangles, then run a per-pixel
program (fragment shader) to colour covered pixels. Everything below rides on
that.

- **quad** — a rectangle (two triangles). Almost every UI node lives in a
  rectangular box, so a rectangle you can place anywhere and colour is the unit.
  "Instanced quads" = one quad stamped N times from a table of per-copy params.
- **SDF-parametric** — a rounded rect / ellipse / stroke / gradient is one quad
  whose fragment shader computes, per pixel, the _signed distance_ to the
  shape's boundary from a few parameters, and antialiases from that distance. No
  path, no curve, no tessellation — a few ALU ops, and _resolution-independent_
  (crisp at any scale). Vector quality without a vector engine. "Parametric" =
  the shape is a formula over per-instance numbers, so one shader draws every
  rounded box.
- **atlas** — glyphs (MSDF) and arbitrary icons too irregular for a formula are
  distance fields baked into a texture; the quad samples it. Still a quad, still
  resolution-independent. MSDF (multi-channel) preserves sharp corners plain SDF
  rounds off.
- **baked** — anything genuinely arbitrary (complex path, drop/inner shadow) is
  turned into an SDF/mesh/texture at _compile time_ (dashc) and drawn as a quad.

So there is **no runtime vector-graphics engine** — no per-frame path
tessellation or coverage rasterisation of arbitrary curves. It is not "only
raster" either: the shading is analytic (SDF), so it keeps vector's
crisp-at-any-scale behaviour. The expensive general work is not deleted, it is
_moved_ (to compile-time baking) and _replaced_ (by shader math for the
primitives).

## 2. Why it wins on performance

It wins by doing less at runtime and by being specialised to the tiling GPU
(§9), not because raster beats vector:

- **No per-frame geometry generation.** A Skia-class renderer tessellates
  dynamic Béziers or computes per-pixel coverage every frame — the hard 90% of a
  2D renderer. The quad painter's geometry is always a quad; shape is a shader
  evaluation. That whole cost category is absent (and is why the painter is
  ~5–10 KLOC).
- **No intermediate render targets.** General VG does clips/masks/group-opacity/
  effects via off-screen layers; on a _tiling_ GPU each mid-frame RT switch is a
  tile-memory flush (R-T1), and bandwidth is the shared-SoC bottleneck (R3). The
  quad painter has no runtime layers — effects baked, clips in-shader — so it
  pays that ~zero times. **Not by scissor**: a scissor rect gives an aliased
  edge and cannot express a clip box's corner radii at all, which
  `../decisions/clip-edge-semantics.md` rules out.
- **Exploits the tiler.** A simple quad splits into an opaque core (z-tested,
  front-to-back → hidden-surface rejection kills covered pixels) plus a thin
  blended AA fringe (R-T2). General renderers blend back-to-front and cannot use
  early-Z that way.
- **CPU cost ≈ dirty-range instance-buffer upload + submit (R-T4).** No
  scene-graph walk, no re-tessellation, no per-frame layout
  (cached/incremental).
- **Footprint.** No managed heap, no GC, no scene-graph duplication.

## 3. The tradeoff — a closed, validated vocabulary

The price of the speed is generality. The painter can only draw its fixed,
validated vocabulary (P4 / profile:core); an arbitrary runtime path that was not
baked is not expressible, and the validator makes that a compile-time
diagnostic, not a runtime fallback. A general VG engine is slower _on this
hardware_ precisely because it stays general — it must keep runtime tessellation
and off-screen compositing, the tiling GPU's worst cases. We trade "draw
anything" for "draw this closed set extremely cheaply," which is a good trade
only because the input is a validated design vocabulary, not arbitrary canvas
commands.

## 4. Proven technique — provenance and the one crispness caveat

The individual techniques are well-known and shipped, not novel:

- SDF text: Valve, "Improved Alpha-Tested Magnification for Vector Textures and
  Special Effects" (Chris Green, SIGGRAPH 2007) — shipped in production games.
- MSDF: Viktor Chlumský (2015), implemented by `msdf-atlas-gen` (our tool) —
  fixes single-channel SDF's corner rounding, so it is the crispest DF variant.
- Analytic SDF shapes: Inigo Quilez's 2D distance functions are the canonical
  reference.
- In our domain: Unity's TextMeshPro renders SDF text ("TMP-style rendering
  without TMP typesetting", `docs/archive/2026-07-14-design-1-seed.md` §7.2); Qt
  Quick renders glyphs via distance fields by default on GL, and Qt is a
  dominant automotive HMI stack; Godot, Unreal, Kanzi use SDF fonts. Crisp SDF
  text in a cockpit is industry-standard.

Crispness is proven resolution-independent for the sizes and transforms UI uses.
The one documented limit is **very small text** (below ~12–14px), where a texel
spans too much of a thin stroke and fine features lose contrast — this is Q-1,
and the standard mitigation (already planned) is a per-size bitmap/hinted atlas
below the threshold. So the architecture delivers the crisp quality across
normal sizes; the only asterisk is small text, and it is bounded and scheduled.

## 5. Backend tiering — Unity high-end, trimmed Skia entry, lean painter gated

DECISION →
[`backend-tiering-unity-skia-lean.md`](../decisions/backend-tiering-unity-skia-lean.md)

Lit / 3D / world-space UI is a **firm requirement for high-end products** and
**not needed on entry products**. That is a whole-scene, per-product backend
split (R3: "backend selection is whole-scene, not per-node"), not "Skia vs
Unity":

- **High-end → Unity.** Lit world-space rendering; firm.
- **Entry → trimmed Skia-GPU (bridge), then the lean painter only if measurement
  demands it.** Skia-GPU on GLES is already the named on-target path until the
  lean painter exists (§8.1), and the v1 plan already defers the lean-painter
  decision to measurement (§11). Entry hardware is where footprint and bandwidth
  are tightest, so the lean painter's justification is strongest there — but the
  sequence is: ship entry on trimmed Skia, measure on the real entry SoC, build
  the lean painter only if trimmed Skia busts the budget.

Note the correction to §8.3's "Skia-GPU rejected": that rejection ("permanently
non-identical to the engine painter") only bites when Skia ships _alongside_ a
shipping SDF engine painter. Since Unity and entry-Skia never render the same
frame (different products), the AA-model difference softens to a cross-product
brand-fidelity + golden-tolerance concern, already handled by the CPU-oracle +
perceptual-GPU-diff model (§8). Flutter ships Skia/Impeller into production
automotive, so "Skia on embedded automotive GLES" is proven, not a gamble.

Nothing here burns the architecture: the painter trait (boundary B) stays; each
tier is one implementation behind it; re-adding a painter later is a re-golden.

## 6. The Skia trim profile

For the entry tier, build Skia from source (skia-safe builds from source when
the feature combo is not prebuilt) trimmed to what the runtime painter actually
needs:

- **No `textlayout`** → no SkShaper, no HarfBuzz, no **ICU**. We typeset
  upstream with rustybuzz and draw MSDF atlas quads, so Skia needs zero text
  intelligence; ICU alone is multiple MB, so this is the single largest cut and
  it costs nothing (we never used Skia's typesetter — §7.2). Draw glyphs as
  textured quads (`drawImageRect`); `SkTextBlob` is optional.
- **No codecs** (libpng/libjpeg/libwebp) — runtime uploads pre-transcoded
  KTX2/Basis→ASTC/EAC textures; decoders live in the offline asset pipeline.
- **GLES only** (`skia_use_gl`, not Vulkan/Metal/D3D); `skia_enable_pdf=false`;
  no SVG/expat; `is_official_build=true` for the size-optimised config.

What remains is core 2D — paths, gradients, SDF-via-SkSL, textured-quad draw,
GLES backend. Caveat: a truly minimal config means building from source (slow,
needs the Skia toolchain) and occasionally patching `skia-bindings`; budget a
spike to measure the trimmed binary on the target triple. Watch Skia-GPU backend
churn (Ganesh → Graphite; §8.1 already warns "don't couple to Graphite; bindings
lag") — pin skia-safe deliberately. A no-text/no-codec Skia is small enough that
it likely narrows the gap with the lean painter to where, on all but the most
bandwidth- starved entry SoCs, the lean painter stops being worth building.

## 7. Own the typesetter — rationale and residual risk

For a single-backend app, delegating to a painter's typesetter is the safe
choice; for a multi-backend pipeline with R1 (identical Arabic on every
backend), it is the risky one and owning text is safe. You cannot use Skia's
typesetter on Skia and rustybuzz on Unity/native — they would disagree on line
breaks, metrics, shaping — so any painter's built-in typesetter is off the table
by construction.

Reassurance: **rustybuzz _is_ HarfBuzz** (a port); Skia's own shaper drives
HarfBuzz too. Same shaping engine, run once, in pure Rust. Residual risks are
inherent to owning text, not to the Skia boundary, and all are on the board:
small-size MSDF legibility (Q-1); colour/emoji fonts (monochrome-SDF atlas can't
hold them — confirm out of scope for cockpit); font fallback (owned via declared
per-locale charsets, §6.1, undeclared codepoint → import diagnostic, not runtime
tofu); line-breaking + Arabic atlas coverage (own code — the v0.5/v0.6 work,
with kashida already deferred). Bonus: because the painter consumes positioned
glyph runs and Taffy's measure callback reads the same shaped-run cache, layout
and paint cannot disagree (§7.2) — more robust than delegating, not less.

## 8. Performance vs Compose and Flutter

For its job — rendering a _validated vocabulary_ on tiling-GPU embedded hardware
— the architecture is designed to be faster and, more importantly, lighter and
more predictable. Structural (not tuning) reasons: no GC (Compose = ART, Flutter
= Dart, both GC → jank risk; ours is Rust, and this is what lets R4 promise
_provable frame cost_); layout + typeset run once, not per-frame; no runtime
tessellation / RT round-trips in the lean painter; per-frame CPU ≈
instance-buffer upload; lighter footprint + mmap cold-start (R5). Tellingly,
Flutter built a whole new renderer (Impeller, replacing Skia) largely to kill
shader-compilation jank — even the best general engine fights frame-time
consistency, which we sidestep with precompiled single-sourced SDF shaders
(R-T5).

Three honest caveats: (1) it is faster _because_ specialised — a narrower tool
rather than a faster general one; compare on "render a pre-validated design
scene" and it wins, on "render anything at runtime" it does not play. (2) The
biggest wins depend on the lean painter, which does not exist yet; today's
interim is Skia, which was never chosen for speed, and the Unity path is heavier
not lighter. (3) It is unmeasured — v0.2 against years-tuned engines; benchmark
on target. The defensible claim today is **predictability + footprint**, which
on a device that must never drop a frame and must boot fast on a shared SoC
matters more than peak fps. That is the automotive requirement, and it is
equally the requirement for any embedded panel held to a fixed frame budget.

## 9. Unity painter — quality and calibration

The C# layer is a _projection_, not a renderer (§8.2): it consumes the finished
rect table + positioned glyph runs and only assigns transforms and materials. So
layout positions and text positioning are the shared runtime's — identical to
every backend by construction — and divergence is bounded to pixel shading/AA.

- **Text**: MSDF atlas quads with an SDF material — the proven TMP-quality path,
  same atlas + same glyph runs + same typesetter as every backend, so R1 holds
  by construction.
- **Shapes**: analytic SDF materials, crisp, resolution-independent, SDF math
  single-sourced with the lean painter (R-T5); diamond gradient and stroke-align
  are _easier_ here than in Skia.
- **Not bit-exact**: Unity is a GPU painter → perceptually matched to the Skia
  CPU oracle within tolerance, not bit-identical (different AA model). That is
  the correct bar; bit-exactness is only for the CPU oracle.

The added dimension is **lit, world-space rendering** — a quality superset the
2D painters can't do. Consequence: lit-opaque/lit-cutout nodes are
_scene-dependent by design_ (lighting changes appearance), so "right quality"
for them means "looks right in the lit scene," not "pixel-matches the flat
design"; unlit-overlay nodes hold the flat-oracle bar. Transparent surfaces have
physical limits (no SSR, no clean shadows — §8.2), opted into per node via
material class.

**AA/color calibration** is the work of landing GPU output inside the oracle's
tolerance and making lit UI read as its authored colour:

- _AA_: tune the SDF edge-coverage band (size it with `fwidth` so it stays ~1px
  in screen space; on Unity, account for world-space/perspective anisotropy);
  match MSDF `pxRange` between atlas-gen and shader; **blend in linear space**
  (gamma- incorrect AA makes text the wrong weight — the canary); keep HLSL/GLSL
  parity.
- _Color_: define the document's canonical colour + blend space once so every
  painter converts to the same target (gradients are the canary for
  linear-vs-gamma blending). The big lit-specific item is **tone mapping**:
  Unity's HDR pipeline applies ACES/Neutral, which shifts colour — an authored
  `#FF0000` does not reach the panel as `#FF0000` through a tone-mapped lit
  pass. Route unlit-overlay UI to _bypass_ tone mapping (renders as authored);
  lit nodes accept scene-dependence. Match premultiplied-vs-straight alpha to
  avoid edge halos.

This is one-time-per-painter "painter swap = re-golden" tuning, not per-scene.

## 10. Unity painter — resource cost & the C# projection (Burst / Collections / BRG)

Unity is the **heaviest** painter, by design (R3, §8.3): it hosts a whole game
engine (managed runtime + GC, SRP, asset system, game loop) even for UI, tens of
MB vs a trimmed Skia's few. The resource ordering is lean native < trimmed Skia
< Unity. This is not a flaw — Unity's cost buys lit/3D and is why it is
high-end-only; the entry tier is Skia/lean _because_ it is a fraction of the
cost. For the 2D drawing alone Unity's SDF-material approach can be
GPU-competitive with (or leaner than) Skia on effects; the gap is the engine
baseline + the lit-3D GPU work.

**IL2CPP does not remove the GC.** It is an AOT compiler (C# → C++ → native):
fast, and no JIT (good for locked-down/certified OSes — the same W^X constraint
that rejected Blend2D), but it still ships the Boehm conservative GC. You don't
_eliminate_ GC, you _avoid triggering it_ with discipline — which §8.2 already
specifies: struct/Span, pooled, GC-free, one commit across the FFI seam, typed
keys via codegen, pre-instantiated GameObjects (no runtime Instantiate/Destroy
churn). Extra levers: Incremental GC; `GarbageCollector.GCMode = Disabled`
during critical windows + manual collect at safe points (only safe if truly
zero-alloc); and the ceiling — **Burst + `Unity.Collections`** unmanaged, SIMD
jobs that never touch the managed heap. Boehm is non-compacting → fragments over
long runs, so zero-alloc steady state matters double for a long-running cockpit.
This zeroes _dash's_ delta; it cannot remove Unity's own engine floor.

DECISION direction →
[`unity-painter-uses-brg.md`](../decisions/unity-painter-uses-brg.md):
**BatchRendererGroup (BRG) over GameObject-per-node** for the bulk SDF-quad UI.
This section records why BRG was chosen; how it behaves once chosen, and the
ordering pitfall it presents to a painter's-algorithm renderer, is in
[batch-renderer-group.md](batch-renderer-group.md). GameObject-per-node
maintains a full scene-graph mirror (Transform hierarchy, per-renderer culling,
a managed object per node) — the "scene-graph duplication" §8.3 avoids. BRG
draws N instances of a quad+material from a `GraphicsBuffer` filled from a
NativeArray (ideally a Burst job): no GameObjects, no Transforms, per-instance
SDF params in the buffer. It makes the Unity painter's data model _the same
shape_ as the lean native painter ("instance buffer → SDF shader → GPU"), so the
dirty-set/double-buffer logic maps onto R-T4 directly, and it is the natural
endpoint of the Burst+Collections choice (the NativeArray the Burst jobs fill
_is_ the BRG instance buffer; Transforms can't be written from Burst except via
`TransformAccessArray`).

**Lit BRG is possible — lighting is a shader-pass concern, not a rendering-path
fork.** Entities Graphics renders fully lit, shadow-casting instances via BRG
with zero GameObjects; the SRP's light/shadow passes operate on draws, not
GameObjects. So keep the 99% on BRG and express the material classes as shader
variants/passes on that one path: unlit-overlay = unlit variant (no light/shadow
passes, cheapest); lit-opaque = lit forward/GBuffer + ShadowCaster pass;
lit-cutout = SDF alpha-clip lit + SDF-clipped shadow-caster (exactly §8.2's "SDF
alpha-clip; shadow-caster pass with clip"). What you take on vs GameObjects:
author the SDF material as a _lit_ SRP shader with those passes; emit BRG draw
commands into the shadow (and motion) passes with the right flags; supply
per-instance SH/probe data for baked GI (direct lights need none of this).
Limits that are technique, not BRG: transparent/AA-fringe casts only clipped
(binary-threshold) shadows and gets no SSR (§8.2); every lit node adds passes →
tile-flush cost (R-T1), so keep most UI unlit-overlay and mark only
genuinely-physical nodes lit. GameObjects then reserved **only** for
node-replacement (arbitrary 3D/particles/per-frame engine content in a layout
box, §10.2), not for lit UI.

Gotchas: `Unity.Collections` disposal/safety discipline (pool lifecycle); BRG is
low-level and thin on docs (Entities Graphics is the reference); verify BRG
platform support on the exact automotive GLES 3.2 target; you own culling
(coarse or none for UI is fine and cheaper than per-renderer culling). **Spike
early**: the lit + SDF-clipped-shadow-caster shader on the target SRP
(especially if HDRP) — that is where the engineering risk sits.

**The fallback is one rung of a ladder, and this note does not state the
ladder.** Costly and unsupported are different failures with different answers,
which the sentence above does not distinguish.
[`../decisions/unity-painter-uses-brg.md`](../decisions/unity-painter-uses-brg.md)
D3 is the ladder and D4 is the read that tells the two failures apart, and
**they are stated in full only there**. Other files name whichever rung bears on
them; none of them is the place to read the ladder from.

## 11. lit vs unlit — the underlying concept

- **Unlit** — the authored colour _is_ the on-screen colour, absolute,
  unaffected by the scene. How all normal 2D UI (Figma, web, Skia) works.
  Metaphor: a glowing screen.
- **Lit** — the final colour is computed from the surface's material colour
  _plus_ the 3D scene's lighting (brighter in light, darker in shade,
  highlights, casts/ receives shadows). Depends on the environment. Metaphor: a
  painted physical object.

Default UI to **unlit** (information should read exactly as designed, cheap,
cross-backend); opt into **lit** only for elements meant to feel physically part
of the 3D cockpit (Unity-only, more expensive, scene-dependent). This is the
per-node choice behind the three material classes and the reason unlit-overlay
nodes match the flat design exactly while lit nodes intentionally do not.

## 12. References

- Chris Green (Valve), _Improved Alpha-Tested Magnification for Vector Textures
  and Special Effects_, SIGGRAPH 2007 — SDF text.
- Viktor Chlumský, _Shape Decomposition for Multi-Channel Distance Fields_, 2015
  — MSDF; `msdf-atlas-gen`.
- Inigo Quilez — 2D signed distance functions (canonical shape SDFs).
- Unity TextMeshPro — production SDF text ("TMP-style").
- Qt Quick — distance-field glyph rendering (automotive HMI precedent).
- Flutter Impeller — precompiled-shader renderer built to remove jank
  (frame-time predictability precedent):
  <https://docs.flutter.dev/perf/impeller>

## 13. Open items

- Measure trimmed-Skia binary + bandwidth on the real entry SoC (§5/§6) — gates
  the lean painter.
- Q-1 small-text: MSDF vs per-size bitmap atlas below ~14px (§4/§7).
- Define the document's canonical colour + blend space (§9).
- Spike lit + SDF-clipped-shadow-caster BRG shader on the target SRP (§10).
  **Still open, and now carried rather than blocking**: the decision record was
  ratified on 2026-08-18 against a fallback ladder, so a bad outcome here
  selects the hybrid rather than holding the slice.
- Confirm BRG platform support on the automotive GLES 3.2 target (§10). **Still
  open, and now an assumption**: the owner ruled on 2026-08-18 to assume the
  board supports it and to confirm with Unity directly. The check that
  discharges it is a read of `BatchRendererGroup.BufferTarget` on the target,
  and it has been taken on no device.
