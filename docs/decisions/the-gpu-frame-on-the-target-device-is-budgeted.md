# The GPU frame on the target device is budgeted: one display frame at native resolution

    status   **accepted (2026-09-04, owner ruling on the measurement of PR
             #1409)**. What is accepted is the BUDGET and the instrument that
             reads it — B1 to B3 below bind the two GPU stories on v0.21. What
             is not settled is how the budget is met, which those stories own.
    date     2026-09-04
    source   docs/design/android-toolchain.md, "The Unity host's presented
             rate, and what bounds it"; docs/technotes/batch-renderer-group.md
             §5e; issues #1347, #1412, #1413
    scope    both painters — unity/com.driftsys.dashscene/Runtime/Engine/
             BrgPainter.cs and crates/dashscene-gpu — on the showcase scenes at
             the target device's native resolution
    related  docs/specification/03-target-hardware-rules.md (R-T2, the rule
             that does most of the work); issue #549 (no display geometry is
             pinned by the specification, which this record does not change:
             it names one device and one extent, not a class)

## Context

Until 2026-09-03 no whole-frame figure existed for the Unity host on a device.
`docs/design/android-toolchain.md`'s Unity table brackets the lease, the pack
and the upload — a few tenths of a millisecond — and excludes the GPU by
construction; the lean painter's `submit` includes the swapchain and is not the
same quantity. Reading the compositor on the Pixel 5 (Adreno 620, Android 14)
closed the gap: with every pacing cap lifted, the Unity host presents the
`surfaces` scene at 32.5 fps at 1080x2340, a frame every 31 ms, with the CPU
idle; the same build at half resolution presents at the display rate. The host
is GPU fill-bound at native resolution, and the record names what is known about
why: shaded area per scene, derived with no device, is 2.40 panels' worth on
`surfaces`, and per-pixel cost differs by paint kind, so solid fills are cheap
and gradients, strokes and glyph sampling are not.

The owner's ruling on that measurement, 2026-09-04, was to halve the GPU frame.

## Decision

- **B1 — the budget is one display frame at native resolution.** The showcase
  scenes, drawn by either painter on the target device at the panel's native
  extent, present at the panel's refresh rate. On the Pixel 5 that is 1080x2340
  at 60 Hz, so the GPU frame on `surfaces` moves from about 31 ms to inside 16.7
  ms. The halving is the ruling; the frame is the number it lands on.
- **B2 — the instrument is the compositor, not the painter.** The rate that
  meets or misses the budget is read from `dumpsys SurfaceFlinger --timestats`
  on the player's surface, with the entry named by the player's own `drew` line
  and the dump kept. Neither painter's per-frame report is the instrument: the
  Unity host's `fps if unpaced` is the inverse of tick plus draw, and the lean
  painter's excludes the present. A player capped by its own pacing reads the
  cap, not the GPU, so the reading is taken with the pacing changes of issue
  #1408 in place.
- **B3 — the route is R-T2 first, then the per-kind cost.** Opaque cores drawn
  front-to-back with depth, and a blended fringe, are projected to remove about
  10 ms of the 31 on `surfaces` by rejecting the full-screen gradient backdrop
  under the opaque panel and tiles; the remainder is the per-pixel cost of the
  kinds that stay shaded, which a variant sweep measures before any fast path is
  written. Issue #1412 is the first for the Unity painter and #1293 for the lean
  painter; issue #1413 is the second. The shaded-area instrument of issue #1296
  is what shows the rejected pixels without a device.

## Consequences

- The two stories carry a before-and-after compositor reading each, in
  `docs/design/android-toolchain.md`, and close against B1 on the device this
  record names. A reading on another device, another extent or another scene is
  a new row, not a substitute.
- Epic #1120 is declared non-gating for v0.21. Whether the halving gates the
  slice is a separate ruling this record does not make; the stories are placed
  so that either reading leaves them placed.
- Issue #549 stands: this record pins one device and one extent for one budget,
  and the specification still pins no display geometry.

## Alternatives considered

- **A budget in milliseconds of GPU time.** Rejected: no GPU timer is readable
  on this device from shell (`/sys/class/kgsl` is refused), the lean painter's
  timestamp queries exclude the present, and the compositor's rate is what a
  person sees. The frame is the unit the panel imposes.
- **Reducing resolution or render scale.** Rejected as the route: it meets the
  number by drawing less of the document, which the goldens would show, and says
  nothing about the painter. It stays the diagnostic it was.
- **Turning HDR off in the URP asset.** Measured at about one frame per second
  on every scene, so it is not a route on its own; it may travel with the
  per-kind story if the sweep says so.
