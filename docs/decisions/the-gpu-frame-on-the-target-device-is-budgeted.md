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
             the target device's native resolution. "The target device" in
             this record is the device it names, the Pixel 5: the one
             target-class device this project has, and not the fleet
    related  docs/specification/03-target-hardware-rules.md (R-T2, the rule
             projected to do the larger part); issue #549 (no display geometry
             is pinned by the specification, which this record does not
             change: it names one device and one extent, not a class);
             docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md
             (whose upper bound waited on a frame budget on a named device,
             which D1 now supplies for one)

## Context

Until 2026-09-03 no whole-frame figure existed for the Unity host on a device:
its own table brackets the lease, the pack and the upload and excludes the GPU,
and the lean painter's `submit` is a different quantity. Reading the compositor
on the Pixel 5 supplied it — `docs/design/android-toolchain.md`, "The Unity
host's presented rate, and what bounds it", carries the figures and what is
known about why. The two that the ruling rests on: with every pacing cap lifted,
the Unity host presents the `surfaces` scene at 32.5 fps at 1080x2340, a frame
every 31 ms; and the same build with the display forced to a quarter of the
pixels presents at the display rate. The host is GPU fill-bound at native
resolution.

The owner's ruling on that measurement, 2026-09-04, was to halve the GPU frame.

## Decision

- **D1 — the budget is one display frame at native resolution.** The showcase
  scenes, drawn by either painter on the target device at the panel's native
  extent and in the display mode measured, shall present at that mode's refresh
  rate. On the Pixel 5 that is 1080x2340 in the 60 Hz mode (a 90 Hz mode exists
  and is not the one measured): the `surfaces` scene's GPU frame, about 31 ms
  when this was ruled, shall come inside 16.7 ms — the halving, rounded to the
  frame the panel imposes. **Met when**, over the window D2 names, the
  compositor's `averageFPS` on the player's surface is at or above 59 with
  `droppedFrames` 0, and, where the latency dump has rows, no more than one
  presented interval in a hundred exceeds 17 ms. The tool's window average can
  read above the panel's rate; that is at the rate, not above it.
- **D2 — the instrument is the compositor, not the painter.** The rate that
  meets or misses the budget is read from `dumpsys SurfaceFlinger --timestats`
  on the player's `SurfaceView … (BLAST)` layer over a 10 s window — the window
  the 31 ms baseline was read over — with the display mode pinned by the dump's
  `displayRefreshRate` line, the entry named by the player's own
  `[showcase]
  drew` logcat line, and the dump kept.
  `measure/android/gpu-capture.sh <out>
  com.driftsys.dashscene.showcase` with
  `DS_GPU_WINDOW=10` runs that sequence; its package parameter defaults to the
  lean painter's demo and must be given. A plain launch of the showcase player
  opens entry 0, `surfaces`; no intent extra selects it. Neither painter's
  per-frame report is the instrument: the Unity host's `fps if unpaced` is the
  inverse of tick plus draw and excludes the GPU; the lean painter's
  `Sample::fps_if_unpaced` is the inverse of tick plus paint plus present, and
  its present term is mostly waiting on the swapchain, so it counts waiting as
  work. Both are headroom figures, not a rate the compositor showed. A player
  capped by its own pacing reads the cap, not the GPU: issue #1408's two changes
  must be in place, and a tree without them reads 30 fps and does not test D1.
- **D3 — the route is R-T2 first, then the per-kind cost.** Opaque cores drawn
  front-to-back with depth, and a blended fringe, come first; the cost of the
  kinds that stay shaded — per pixel, or per command where issue #1406 finds the
  command count is the term — is measured by a variant sweep before any fast
  path is written, and comes second. The projection behind the order — about 10
  ms of the 31 on `surfaces` from rejecting the covered backdrop — and its
  arithmetic are story #1412's, and nothing yet falsifies it; the shaded-area
  derivation of issue #1296 is what would show the rejected pixels without a
  device, so it lands before #1412, and the fringe's order against the
  transparent instances is issue #1402's ruling, which #1412 depends on and must
  not re-derive. Story #1412 is the Unity painter's first; issue #1293 holds the
  lean painter's half and stays measure-then-decide, as its own body says the
  saving there is unproven; story #1413 is the second.

## Consequences

- Stories #1412 and #1413 each carry a before-and-after compositor reading in
  `docs/design/android-toolchain.md`; the pair closes the Unity painter against
  D1 on the device this record names, and each story's own target is its own.
  The lean painter's `surfaces` frame is about 22 ms on the same device, over
  the budget too; issue #1293 measures it against D1 and either implements or
  files the story that does — D1 binds it as much as the Unity painter, and
  #1293 is where that can fail. A reading on another device, another extent or
  another scene is a new row, not a substitute.
- Whether the halving gates v0.21 is a separate ruling this record does not
  make; epic #1120's standing declaration (`docs/roadmap.md`, v0.21) is where
  that is held, and stories #1412 and #1413 stay on v0.21 under #1120 under
  either ruling.
- The records that said this project has no frame budget now say it has one for
  this device and none for a display class (issue #549 stands, as the `related`
  line says) — `docs/design/android-toolchain.md`'s "Frame costs" and Q-6
  sections, and the frame-delta record above.

## Alternatives considered

- **A budget in milliseconds of GPU time.** Rejected: no GPU timer is readable
  on this device from shell (`/sys/class/kgsl` is refused), the lean painter's
  timestamp queries exclude the present, and the compositor's rate is what a
  person sees. The frame is the unit the panel imposes.
- **Reducing resolution or render scale.** Rejected as the route: it meets the
  number by drawing less of the document, which the goldens would show, and says
  nothing about the painter. It remains what it was used as on 2026-09-03: the
  diagnostic that showed the frame is fill-bound.
- **Turning HDR off in the URP asset.** Worth about one frame per second on
  `surfaces` and `typography` for the intermediate and its blit, and
  unmeasurable on `layout`, which is at the panel rate either way; not a route
  on its own, and it may be combined with the per-kind story if the sweep says
  so.
