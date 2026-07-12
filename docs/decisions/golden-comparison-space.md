# Goldens compare decoded pixels in unpremultiplied RGBA8888

    status   accepted (story #6, 2026-07-12); resolves debt #86
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
