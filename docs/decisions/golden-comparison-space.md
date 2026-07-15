# Goldens compare decoded pixels in unpremultiplied RGBA8888

    status   accepted (story #6, 2026-07-12); resolves debt #86.
             Extended by story #14 (a differing-pixel tolerance for
             anti-aliased content — see "Cross-machine anti-aliasing"
             below) and story #11 (the exact-match constraint on v0.2
             flex goldens — see "Flex goldens are exact-match by
             construction" below).
    scope    goldens/ tooling; binds golden authoring for every painter

## Context

Story #4 left the golden comparison space open (debt #86): the
reference painter's surface is N32 premultiplied, and its readback
(`SkiaPainter::rgba_bytes`) converts to unpremultiplied RGBA8888 —
a conversion that shifts semi-transparent channels by up to one code
point against direct quantization of the authored color. The harness
needed one defined space and one defined failure criterion.

## Options

1. Compare decoded pixels in unpremultiplied RGBA8888; encoded-byte
   drift with identical pixels passes with a note.
2. Compare encoded PNG bytes.
3. Compare premultiplied pixels (the surface's native space).

## Choice

Option 1.

## Why

- Encoded bytes (option 2) fail on encoder changes that alter zero
  pixels — a skia version bump would break every golden with no
  rendering change, and the report could not tell encoding drift from
  pixel drift. A golden is a picture, not a container format.
- Premultiplied comparison (option 3) would compare the surface's
  internal representation; PNG itself is unpremultiplied, so the
  checked-in artifact and the comparison space would disagree, and
  every inspection tool shows unpremultiplied values.
- Unpremultiplied comparison is still bit-exact for a pinned skia
  version: opaque colors round-trip exactly, and a semi-transparent
  fill's premul quantization is deterministic — the quantized value IS
  the expected value, baked into the golden. Documented in
  `goldens/README.md`.
- v0.1 fixtures stay opaque and integer-aligned; the sub-pixel
  geometry policy remains open as debt #85 (GPU-painter perceptual
  diffs, when they come, revisit the space — unpremultiplied
  comparison amplifies channel error at low alpha, noted in story #4's
  review).

## Cross-machine anti-aliasing (story #14 extension)

Story #14 turned anti-aliasing on for the reference painter
(`reference-painter-antialiasing.md`, resolving debt #85). AA is
deterministic for a pinned skia version on one machine, but skia's CPU
coverage rounding at a fractional edge is **not bit-identical across CPU
architectures**: the v0.3 paint golden, generated on one architecture,
differed by 32 of 9216 pixels (0.35%) on the CI runner's architecture,
all at gradient and curve edges.

Bit-exact comparison therefore holds only for content that renders
identically across machines — integer-aligned, un-antialiased geometry
(solid fills). For anti-aliased content the harness offers a
differing-pixel **tolerance**: `assert_matches_golden_within(name, png,
max_fraction)` fails only when more than `max_fraction` of pixels
differ. This is `docs/technotes/rendering-and-painters.md`'s
"tolerance-based perceptual diff", which the design placed at GPU painters, brought forward to CPU-raster
AA because the same cross-architecture coverage jitter applies.

- `assert_matches_golden` stays exact (fraction 0) — v0.1's
  integer-aligned solid golden uses it and passes bit-exact everywhere.
- The v0.3 paint golden uses a 1% tolerance (~3× the observed 0.35%
  jitter, and far below any real rendering change — the smallest scene
  element covers several percent of the canvas, so a regression moves
  far more than a thin edge).
- Interior correctness is not left to the tolerance: the painter's
  per-kind unit tests assert exact bytes at interior probe pixels away
  from AA edges, and those are bit-stable across machines (they pass in
  CI). The golden is the coarse full-frame visual-regression check.

## Flex goldens are exact-match by construction (story #11 extension)

Story #11 goldens the v0.2 flex vocabulary (nesting, sizing, clamping,
alignment) with four scenes, each dimensioned so every solved rect
lands on an integer. Integer-aligned solid fills carry no
anti-aliased edges, so all four goldens use `assert_matches_golden` —
the exact-match form, no tolerance budget — the same guarantee the
v0.1 golden already relies on, now proven again against
`dashscene-engine`'s `TaffySolver` output rather than the fixed
solver.

This is a constraint that binds every future flex golden, not an
incidental property of these four scenes: if a construct cannot be
made integral, the scene's dimensions are what change, not the
comparison function. `assert_matches_golden_within` only enters for a
construct that is genuinely impossible to make integral, with the
reason recorded at the call site — no v0.2 flex golden needed it.
