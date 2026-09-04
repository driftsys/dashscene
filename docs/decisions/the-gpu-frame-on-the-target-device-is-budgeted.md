# The GPU frame on the target device is budgeted: one display frame at native resolution

    status   **accepted (2026-09-04, owner ruling on the measurement of PR
             #1409)**. What is accepted is the BUDGET and the instrument that
             reads it — D1 to D3 below bind the work that carries it. What is
             not settled is how the budget is met, which stories #1412 and
             #1413 own.
    date     2026-09-04
    source   docs/design/android-toolchain.md, "The Unity host's presented
             rate, and what bounds it"; issues #1347, #1412, #1413
    scope    both painters — unity/com.driftsys.dashscene/Runtime/Engine/
             BrgPainter.cs and crates/dashscene-gpu — on the showcase scenes at
             the target device's native resolution
    related  docs/specification/03-target-hardware-rules.md (R-T2, the rule
             projected to do the larger part); issue #549 (no display geometry
             is pinned by the specification, which this record does not
             change: it names one device and one extent, not a class);
             docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md
             (whose upper bound waited on a frame budget on a named device,
             which D1 now supplies for one)

## Context

Until 2026-09-03 no whole-frame figure existed for the Unity host on a device.
`docs/design/android-toolchain.md`'s Unity table brackets the lease, the pack
and the upload — a few tenths of a millisecond — and excludes the GPU by
construction; the lean painter's `submit` includes the swapchain and is not the
same quantity. Reading the compositor on the Pixel 5 (Adreno 620, Android 14)
supplied the missing figure: with every pacing cap lifted, the Unity host
presents the `surfaces` scene at 32.5 fps at 1080x2340, a frame every 31 ms,
with `UnityMain` at 14 % of a core and the render thread at 7 % in one `top`
sample; the same build with the display forced to 540x1170 — a quarter of the
pixels — presents at the display rate. The host is GPU fill-bound at native
resolution. What is known about why: the shaded area per scene, derived with no
device, is 2.40 panels' worth on `surfaces`, of which the Unity host draws less,
since it refuses nine rects (the shadow, the backdrop blur, the image fill, the
baked vectors, the render-target groups); shaded area alone does not order the
three scenes; and per-pixel cost differs by paint kind, so solid fills are cheap
and gradients, strokes and glyph sampling are not.

The owner's ruling on that measurement, 2026-09-04, was to halve the GPU frame.

## Decision

- **D1 — the budget is one display frame at native resolution.** The showcase
  scenes, drawn by either painter on the target device at the panel's native
  extent, shall present at the panel's refresh rate. On the Pixel 5 that is
  1080x2340 at 60 Hz: the `surfaces` scene's GPU frame, about 31 ms when this
  was ruled, shall come inside 16.7 ms. The halving is the ruling; the frame is
  the number that results.
- **D2 — the instrument is the compositor, not the painter.** The rate that
  meets or misses the budget is read from `dumpsys SurfaceFlinger --timestats`
  on the player's surface, with the entry named by the player's own `drew` line
  and the dump kept. Neither painter's per-frame report is the instrument: the
  Unity host's `fps if unpaced` is the inverse of tick plus draw and excludes
  the GPU; the lean painter's `Sample::fps_if_unpaced` is the inverse of tick
  plus paint plus present, and its present term is mostly waiting on the
  swapchain, so it counts waiting as work. Both are headroom figures, not a rate
  the compositor showed. A player capped by its own pacing reads the cap, not
  the GPU, so the reading is taken with the pacing changes of issue #1408 in
  place.
- **D3 — the route is R-T2 first, then the per-kind cost.** Opaque cores drawn
  front-to-back with depth, and a blended fringe, are projected to remove about
  10 ms of the 31 on `surfaces`: the full-screen gradient backdrop is 2.53 Mpx,
  the opaque panel and tiles cover roughly three quarters of it, and at the 5 to
  6 ms per megapixel the overlay path costs on that scene, rejecting about 1.9
  Mpx is about 10 ms. That is a projection nothing yet falsifies; the
  shaded-area instrument of issue #1296 is what would show the rejected pixels
  without a device, and the compositor reading of D2 is what would show the
  frame. The remainder is the per-pixel cost of the kinds that stay shaded,
  which a variant sweep measures before any fast path is written. Story #1412 is
  the first for the Unity painter; issue #1293 holds the lean painter's half and
  stays measure-then-decide, as its own body says the saving there is unproven;
  story #1413 is the second.

## Consequences

- Stories #1412 and #1413 each carry a before-and-after compositor reading in
  `docs/design/android-toolchain.md`; the pair closes against D1 on the device
  this record names, and each story's own target is its own. A reading on
  another device, another extent or another scene is a new row, not a
  substitute.
- Whether the halving gates v0.21 is a separate ruling this record does not
  make; epic #1120's standing declaration (`docs/roadmap.md`, v0.21) is where
  that is held, and the stories are placed so that they stay placed under either
  ruling.
- Issue #549 stands: this record pins one device and one extent for one budget,
  and the specification still pins no display geometry. The records that said
  this project has no frame budget now say it has one for this device and none
  for a display class — `docs/design/android-toolchain.md`'s "Frame costs" and
  Q-6 sections, and the frame-delta record above.

## Alternatives considered

- **A budget in milliseconds of GPU time.** Rejected: no GPU timer is readable
  on this device from shell (`/sys/class/kgsl` is refused), the lean painter's
  timestamp queries exclude the present, and the compositor's rate is what a
  person sees. The frame is the unit the panel imposes.
- **Reducing resolution or render scale.** Rejected as the route: it meets the
  number by drawing less of the document, which the goldens would show, and says
  nothing about the painter. It stays the diagnostic it was.
- **Turning HDR off in the URP asset.** Measured at about one frame per second
  for the intermediate and its blit, on the two scenes below the panel rate, so
  it is not a route on its own; it may travel with the per-kind story if the
  sweep says so.
