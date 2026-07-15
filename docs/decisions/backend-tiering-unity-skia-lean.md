# Backend tiering: Unity high-end, trimmed Skia entry, lean painter gated on measurement

    status   accepted
    date     2026-07-13
    source   docs/technotes/rendering-and-painters.md §5
    scope    dashscene-skia, dashscene-unity; the deferred lean native painter

## Context

Lit / 3D / world-space UI is a firm requirement for high-end products and
not needed on entry products. That is a whole-scene, per-product backend
split (R3: "backend selection is whole-scene, not per-node"), not a
per-feature choice between Skia and Unity.

## Choice

- **High-end → Unity.** Lit world-space rendering; firm.
- **Entry → trimmed Skia-GPU (bridge), then the lean painter only if
  measurement demands it.** Skia-GPU on GLES is the named on-target path
  until the lean painter exists (`docs/technotes/rendering-and-painters.md`
  §8.1), and the v1 plan already defers the lean-painter decision to
  measurement (`docs/roadmap.md`). The sequence is: ship entry on trimmed
  Skia, measure on the real entry SoC, build the lean painter only if
  trimmed Skia busts the budget.

## Why

Entry hardware is where footprint and bandwidth are tightest, so the lean
painter's justification is strongest there, but building it before
measuring would be speculative. `docs/technotes/rendering-and-painters.md`
§8.3's "Skia-GPU rejected" only bites when Skia ships alongside a shipping
SDF engine painter on the same product; since Unity and entry-Skia never
render the same frame (different products), the AA-model difference
softens to a cross-product brand-fidelity + golden-tolerance concern,
already handled by the CPU-oracle + perceptual-GPU-diff model. Flutter
ships Skia/Impeller into production automotive, so "Skia on embedded
automotive GLES" is proven, not a gamble.

## Consequences

- The painter trait (boundary B) stays; each tier is one implementation
  behind it; adding or removing a painter later is a re-golden, not an
  architecture change.
- The Skia trim profile (no `textlayout`/ICU, no codecs, GLES only) that
  makes the entry tier viable is detailed in
  `docs/technotes/rendering-and-painters.md` §6, not restated here.
