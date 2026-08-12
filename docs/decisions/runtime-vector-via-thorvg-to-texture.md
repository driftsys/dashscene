# Arbitrary runtime vector (SVG/Lottie) renders to a texture via ThorVG

    status   accepted
    date     2026-07-13
    source   docs/technotes/runtime-content.md §5
    scope    the runtime vector escape hatch; native/entry-tier painters

## Context

Genuinely runtime-provided, non-bakeable vector content — arbitrary SVG,
morphing/masked Lottie — has no faithful bake (see
`docs/decisions/lottie-bake-when-possible.md`).

## Choice

Render it to a texture with ThorVG and fill the placeholder with that texture as
an image fill, since every painter can already draw an image. ThorVG fits:
lightweight (~150KB), MIT, SW+GL backends, native SVG and Lottie,
embedded-proven (LVGL, Tizen, Crank Storyboard).

Treat it as the bounded escape hatch it is:

- The node becomes a bitmap — it loses crisp-at-scale and re-renders on resize.
  Only for genuinely runtime content; anything bakeable should be baked (SDF),
  which stays crisp and free per frame.
- Lottie playback means a per-frame re-render — a per-frame offscreen render
  target for that node, count-budgeted like blurs (Q-6). Use ThorVG's GL backend
  to render into a GL texture on the painter's context and avoid a CPU→GPU
  upload per frame.
- P3 holds: ThorVG runs its own clock inside the fixed placeholder box; the
  runtime never calls into it mid-frame.
- It is primarily the native/entry-tier mechanism; on Unity high-end,
  node-replacement is the more likely fit instead.

## Why

Baking is preferred wherever it is faithful
(`docs/decisions/lottie-bake-when-possible.md`), so this path only ever serves
the residual, genuinely non-bakeable class, and ThorVG's size and license make
it cheap to carry as a bounded escape hatch rather than a general painter behind
boundary B.

## Consequences

- A node on this path is not pixel-identical across tiers — nor should it be; it
  is dynamic content, same as node-replacement.
- ThorVG's role stays scoped: runtime use is only this bucket; at build time it
  is also a candidate offline Lottie/SVG frame-renderer for sprite-sheet baking,
  kept out of the steady-state render path either way
  (`docs/technotes/runtime-content.md` §6).
