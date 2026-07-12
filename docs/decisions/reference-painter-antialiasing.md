# The reference painter anti-aliases every draw

    status   accepted (story #14, 2026-07-12); resolves debt #85
    scope    dashscene-skia; golden authoring

## Context

Story #4 shipped the painter with anti-aliasing off and left the
sub-pixel geometry policy open (debt #85): with AA off, skia snaps
fractional edges to whole pixels, discarding geometric coverage. The
v0.3 vocabulary makes fractional geometry unavoidable — rounded
corners are curves, gradient and stroke geometry is not pixel-aligned.

## Options

1. Anti-aliasing on for every draw.
2. Keep AA off; accept snapped/stair-stepped edges.
3. Mixed: AA off for axis-aligned rects, on for curved geometry.

## Choice

Option 1.

## Why

- CPU-raster AA is deterministic for the exactly pinned skia version
  (`=0.81.0`), so goldens stay bit-exact — determinism never required
  integer alignment, only a fixed implementation.
- Integer-aligned axis-aligned edges have analytic coverage exactly 0
  or 1, so AA is a no-op there by construction — the committed v0.1
  golden passed unchanged, which is this decision's regression proof.
- Stair-stepped rounded corners (option 2) misrepresent the vocabulary;
  a mixed policy (option 3) is the same output as option 1 for
  integer-aligned rects at the cost of two code paths.
- The lean painter's SDF-fringe AA model will differ per §8.3 —
  cross-backend identity is structural (rect tables); pixel truth is
  per-painter goldens.
