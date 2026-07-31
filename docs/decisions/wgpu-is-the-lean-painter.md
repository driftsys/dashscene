# wgpu is the lean painter, and Skia-GPU is not planned

    status   accepted (story #577, 2026-08-01)
    scope    dashscene-gpu (new), dashscene-skia, dashscene-web (retired),
             docs/decisions/backend-tiering-unity-skia-lean.md (amended),
             every v0.15 story under epic #569
    source   docs/wip/2026-07-19-wgpu-painter-direction.md, which declined
             to make this decision and said so: "Adopting wgpu is a
             painter-strategy decision … It should be made on those grounds
             and recorded in docs/decisions/, not inherited as a side effect
             of the backdrop-blur discussion that produced this note."

## Context

`backend-tiering-unity-skia-lean.md` (2026-07-13) set three tiers: Unity for
high-end, a trimmed Skia-GPU build as the entry-tier **bridge**, and a lean
native painter built only if measurement on a real entry SoC demanded it. That
sequencing was correct for what was known then: the lean painter was
hypothetical and expensive, and Skia-GPU was the cheap way to have _something_
named on the entry tier.

Two things have changed since.

**A wgpu painter is being built anyway, for the web.** `dashscene-web` reserved
a name for a wasm/tiny-skia painter and never became one. One Rust codebase over
`wgpu` covers native and web, which removes the reason to have a second,
different painter for the browser.

**The vocabulary turned out to be GPU-shaped already.** `Painter::paint` takes
seven parallel tables and no path primitive; vector shapes are baked MSDF fields
(`baked-vector-msdf-field.md`), not live path rasterisation. Every primitive in
the v0 vocabulary maps onto an instanced quad with a fragment shader. So the
lean painter is not the research problem the 2026-07-13 record was hedging
against — it is a thin translation of the paint table into draw calls, which is
what P2 says a painter is.

## Decision

**1. `dashscene-gpu` is the lean painter.** It is the product painter for web,
and the named candidate for the entry tier. It is built over `wgpu`.

**2. The crate is named for the role, not the dependency.** `wgpu` is the
backend this crate is built on, and the contingency in point 5 below names a
direct-GLES backend over the same instance buffer and the same shaders. A crate
called `dashscene-wgpu` would have to be renamed the day that contingency was
taken, or would carry a name that no longer described it. `dashscene-gpu`
survives the contingency; the epic and its stories were filed under the older
name and are amended to this one.

**3. `dashscene-web` is retired.** Its reserved name described a wasm/tiny-skia
painter and no longer describes any planned component. The crate is a 3-line
placeholder; there is nothing to migrate.

**4. Skia-GPU is not planned.** Not deferred, not a fallback with a trigger —
not planned. See "Why Skia-GPU is not the safe option" below, which is the part
of this record most likely to be re-litigated.

**5. If wgpu's GL backend fails on a target**, the response is an upstream fix,
Vulkan where the target offers it, or a direct-GLES backend written over the
instance buffer of story #578 and the shader library of story #579. It is not
importing a 2D engine to draw a vocabulary that needs none of one.

**6. Skia stays, permanently, as the bit-exact CPU oracle.** The `skia-safe`
dependency does not leave the workspace, and `dashscene-skia` is not on a
deprecation path. What this decision retires is the Skia **trim profile** — the
from-source GLES build, `skia_use_gl`, the codec and `textlayout`/ICU exclusions
that would have made a shipping entry-tier Skia viable, and the
Ganesh-to-Graphite migration watch that came with it.

## What "Skia keeps the entry tier" actually meant

Stated plainly, because the 2026-07-13 record reads as though something exists
there: **the plan of record named Skia on the entry tier, and nothing was ever
built under that name.**

`dashscene-skia` is CPU-raster only. `surfaces::raster_n32_premul` is the only
surface constructor anywhere in the workspace, and `skia-safe` is pinned at
`=0.81.0` with no `gl`, `vulkan` or `metal` feature enabled. So the entry tier
is unimplemented under every painter, and this decision does not remove a
working path — it replaces a named intention with a different named intention,
one that is being built.

## Why Skia-GPU is not the safe option

An audit on 2026-07-31 tested six candidate reasons to keep Skia-GPU as a
fallback. Five fail, and two of them fail in the wrong direction:

| candidate trigger       | verdict                                                                         |
| ----------------------- | ------------------------------------------------------------------------------- |
| smaller footprint       | **anti-trigger** — Skia is the heavier dependency                               |
| better performance      | **anti-trigger** — Ganesh tessellates where instanced quads do not              |
| certification           | helps neither; no certification story distinguishes them                        |
| GL-expressibility       | avoidable, and story #578 is where it is avoided                                |
| the QNX display path    | reachable from `wgpu-hal` as well                                               |
| vendor GLES driver bugs | **survives** — Skia carries fifteen years of accumulated per-driver workarounds |

Only the last is real. And it has a cheaper answer than importing Skia-GPU,
which is why point 5 above is written the way it is: stories #578 and #579
produce a flat instance buffer and a shader library that are **backend-agnostic
artifacts**. A thin direct-GLES backend reusing both is writing a backend, not
writing a renderer. That is precisely what `backend-tiering-unity-skia-lean.md`
calls the lean painter.

Skia-GPU was only ever the bridge for the case where the lean painter did not
exist. wgpu is the lean painter, so the bridge has nothing left to bridge.

## What this amends, and what it does not

This **amends** `backend-tiering-unity-skia-lean.md`; it does not supersede it.
What that record decided and this one keeps:

- High-end is Unity, firm. Untouched.
- Backend selection is whole-scene, not per-node (R3). Untouched.
- The painter trait is the seam, each tier is one implementation behind it, and
  adding or removing a painter is a re-golden rather than an architecture
  change. That property is what makes this amendment cheap.
- The lean painter is still gated on measurement **for the entry tier
  specifically**. Nothing here claims wgpu meets an entry-SoC budget; no such
  measurement exists. What changed is that the lean painter is being built for
  another reason (the web), so "build it only if measurement demands it" no
  longer describes a cost anyone is avoiding.

What it reverses: the choice of Skia-GPU as the entry-tier bridge.

## Consequences

- The render oracle needs per-painter tolerance bands. A wgpu painter will not
  pixel-match Skia — different anti-aliasing, different gradient dithering,
  different blur falloff. `render-oracle-tolerance-and-gating.md` governs how
  bands are set; this adds a painter axis to it. This is the most repo-specific
  risk in the change and it is tuning work, not a rewrite of the fidelity story.
- Skia remains the golden generator. Goldens stay bit-exact against the CPU
  raster painter, and the GPU painter is compared perceptually against them.
- **Pathlessness is a property of the current vocabulary, not a guarantee.**
  Both GPUI and Flutter's Impeller ended up carrying a real path rasteriser
  beside their quad pipeline. If Figma vector networks ever need live path
  rendering rather than baked MSDF fields, this painter lands in the same place.
  Recorded so that outcome reads as a known risk rather than a surprise.

## Alternatives considered

**Adopt Vello.** Ruled out. Its README on `main` lists blur and filter effects
among its open gaps — the things a painter would want it for. It renders glyphs
as outlines where this design uses an MSDF atlas, and exposes no custom-shader
extension point to add MSDF through. `vello_hybrid` does have a real WebGL2 path,
correcting an earlier claim, but is at 0.0.9 and self-described as "roughly beta
quality". Adopting it would mean pushing rounded rectangles through a Bézier
flattener to reach a problem a quad already solves.

**Adopt an existing quad+SDF painter.** None is adoptable. `iced_wgpu` and GPUI
both use analytic rounded-box SDF, but both are welded into their frameworks;
`vger` is architecturally right and MIT but has unimplemented images and no
release since 2024-09-07; `femtovg` is healthy but is a tessellating path
renderer. This is not an ecosystem gap: every framework above wants to own
layout, text and scene state, which is exactly what P1/P2/P3 forbid a painter to
have.

**Keep `dashscene-web` as a separate wasm painter.** Rejected. One codebase over
`wgpu` reaches both targets, and a second painter is a second set of tolerance
bands and a second set of shader bugs for no capability the first does not have.

## Traces

- amends `backend-tiering-unity-skia-lean.md`
- amends `crate-name-map.md` (adds `dashscene-gpu`, retires `dashscene-web`)
- constrains `render-oracle-tolerance-and-gating.md` (per-painter bands)
- gardened from `docs/wip/2026-07-19-wgpu-painter-direction.md`
- epic #569; stories #577, #578, #579, #580, #588
