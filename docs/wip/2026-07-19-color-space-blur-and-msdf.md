# Colour space in the paint path: blur blending and MSDF sampling are coupled

    status   WIP — design-discussion capture (2026-07-19, user + Opus);
             feeds a decision record when v0.11 opens. Nothing here is
             implemented and no code was changed. One question below is
             genuinely open; the rest is a record of what is already
             settled, so it is not re-investigated.
    scope    the working colour space of the reference painter's surface,
             and the two behaviours that depend on it — blur blending and
             MSDF distance sampling
    builds on docs/decisions/golden-comparison-space.md (a different
             question — premultiplication and tolerance, not gamma),
             docs/design/atlas-pipeline.md,
             docs/wip/2026-07-19-backdrop-blur-v011.md

## The finding

The reference painter allocates `surfaces::raster_n32_premul((width,
height))` — **with no colour space attached**
(`crates/dashscene-skia/src/lib.rs:76`). One property, two consequences,
and they pull in opposite directions:

1. **MSDF sampling is correct because of it.** The distance channels are
   read raw, with no sRGB transfer applied. This is deliberate and
   already documented in the code
   (`crates/dashscene-skia/src/lib.rs:324-328`):

   > The MSDF field is a distance, not a color: linear filtering
   > interpolates the field (the point of MSDF's crisp edges); nearest
   > would step it. The surface carries no color space
   > (raster_n32_premul), so the channels sample raw — no sRGB
   > conversion mangling the distances.

   Chlumsky's own guidance says exactly this: interpret MSDF channels in
   linear space like alpha, never as sRGB, whatever the PNG suggests.
   We comply, by construction.

2. **Blur therefore happens in sRGB-encoded space, not linear light.**
   With no colour space on the surface, Skia's blur is a weighted average
   of raw 8-bit sRGB values. That is a real choice, and it was made
   implicitly rather than decided.

**These are coupled.** Attaching a linear working colour space to the
surface to obtain gamma-correct blur would apply an sRGB-to-linear
transfer when sampling the atlas image and corrupt the distance
channels — the precise failure the code comment above guards against.
Anyone changing one must handle the other. The MSDF atlas would need to
be tagged or sampled as colour-space-independent data explicitly, rather
than relying on the surface having no colour space at all.

This coupling is the reason to record the two together instead of
filing gamma as a blur-only concern.

## What is settled

The MSDF side needs no work. Three commonly-cited MSDF failure modes
were checked against this repo during the discussion that produced this
note, and all three are already handled:

| Reported risk                                                                                                   | Status here                                                                                                                                                                                                                    |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `pxRange` at the tooling default of 2 sits on the documented failure threshold (safe minimum ≈ `2.5 + 1/scale`) | Not applicable. We use **4**, pinned in `docs/decisions/q1-msdf-below-14px.md:29` and `crates/dashscene-typeset/src/atlas/mod.rs:63`; the vector bake matches at `crates/dashc/src/figma/vector_field.rs:46-47`.               |
| The compact `d/fwidth(d)` screen-pixel-range form can produce NaN where the field is flat                       | Not applicable. We derive it from a uniform — `atlas.distance_range_px * run.size / atlas.px_per_em` (`crates/dashscene-skia/src/lib.rs:333`) — which is the form Chlumsky recommends for 2D. `MSDF_SKSL` uses no derivatives. |
| MSDF channels sampled through an sRGB transfer                                                                  | Not applicable. Handled and documented, as quoted above.                                                                                                                                                                       |

Recorded so this ground is not covered a third time.

## What is open

**Should blur blend in linear light or in sRGB-encoded space?**

Why it has not mattered so far: blurring a single flat colour against
transparency is largely insensitive to the difference. Our only blurs
today are drop and inner shadow, which blur one flat shadow colour over
the node's own silhouette (`crates/dashscene-skia/src/lib.rs:610-680`),
and the `blur-falloff` oracle band passes at 0.022% and 0.000% against
Figma's export.

Why it starts mattering at v0.11: backdrop blur averages **multi-coloured
backdrop content**, where sRGB-space averaging is visibly different from
linear-light averaging — muddier and darker across coloured edges. This
is the first construct that exposes the choice.

What the current oracle result does and does not tell us: the shadow
band passing means either Figma also blurs in sRGB space, or flat-colour
shadow blur is too insensitive to distinguish the two. **The existing
data cannot separate those.** A backdrop-blur oracle frame over
multi-coloured content would, and that frame does not exist yet.

## Why this is not only a Skia question

Cross-painter parity depends on it. Blur colour space is not an
implementation detail a painter may choose: two painters that blur in
different spaces produce visibly different pixels from the same
document, which breaks the premise that boundary B is a contract. The
value has to be pinned in the contract, not left per painter.

The reference painter's answer also becomes the target the future Unity
and wgpu painters must match, so it should be decided against Figma
before a second painter exists — not retrofitted afterwards.

## Suggested order of work when v0.11 opens

1. Author a backdrop-blur oracle fixture over multi-coloured content.
2. Measure our sRGB-space blur against Figma's export. If it holds a
   band, the current implicit choice is confirmed and gets written down
   as an explicit decision rather than an accident of surface
   allocation.
3. Only if it does not hold, evaluate a linear working space — and treat
   the MSDF coupling above as part of that change, not as follow-up.

Do not switch the working colour space speculatively. The current
configuration is load-bearing for MSDF correctness and is passing every
band it is measured against.
