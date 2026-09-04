# The GPU frame on the target device is budgeted: one display frame at native resolution

    status   **accepted (2026-09-04, owner ruling on the measurement of PR
             #1409)**. What is accepted is the BUDGET and the instruments that
             read it — D1 to D3 below bind the stories that carry it, not the
             slice. What is not settled is how the budget is met, which
             stories #1412 and #1413 own.
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

- **D1 — the budget is one display frame at native resolution.** The `surfaces`
  scene, drawn by either painter on the target device at the drawable the player
  reports and in the display mode measured, shall present at that mode's refresh
  rate — as each painter draws the scene today; a construct the Unity painter
  refuses landing later (the backdrop blur, the shadow, the image fill, the
  render-target groups) reopens the reading. On the Pixel 5 the drawable is
  2340x1080, the panel's 1080x2340 in landscape, in the 60 Hz mode (a 90 Hz mode
  exists and is not the one measured): the scene's GPU frame, about 31 ms when
  this was ruled, shall come inside 16.7 ms — the halving, rounded to the frame
  the panel imposes. **Met when**, over the window D2 names, the compositor's
  `averageFPS` on the player's surface is at or above 59 with `droppedFrames` 0,
  and no more than one presented interval in a hundred exceeds 17 ms. The tool's
  window average can read above the panel's rate; that is at the rate, not above
  it. `typography`, at 42 to 50 fps, is over the frame too and has no carrier in
  this record; another device's budget is that device's mode, and each is its
  own row.
- **D2 — the instruments are the compositor's, not the painter's.** Two
  readings, both from `dumpsys SurfaceFlinger` on the player's
  `SurfaceView …
  (BLAST)` layer, both kept: `--timestats` over a 10 s window —
  the window the 31 ms baseline was read over — gives the rate D1 is met by,
  with the display mode pinned by its `displayRefreshRate` line; and
  `--latency`'s `frameReady` cadence gives the GPU frame itself, which is how
  the 31 ms was read and the only way to see a frame that shortens without
  crossing a vsync — a frame anywhere between 16.7 and 33.3 ms presents at the
  same rate, so a story's progress short of D1 is read from the cadence, not the
  rate. The entry is named by the player's own `[showcase] drew` logcat line; a
  plain launch opens entry 0, `surfaces`, and no intent extra selects it.
  `measure/android/gpu-capture.sh <out> com.driftsys.dashscene.showcase` with
  `DS_GPU_WINDOW=10` takes both dumps; its package parameter defaults to the
  lean painter's demo and must be given. Neither painter's per-frame report is
  an instrument: the Unity host's `fps if unpaced` is the inverse of tick plus
  draw and excludes the GPU; the lean painter's `Sample::fps_if_unpaced` is the
  inverse of tick plus paint plus submit, and its submit term is mostly waiting
  on the swapchain, so it counts waiting as work. Both are headroom figures. A
  player capped by its own pacing reads the cap, not the GPU: issue #1408's two
  changes must be in place, and a tree without them reads 30 fps and does not
  test D1.
- **D3 — the route is R-T2 first, then the per-kind cost.** Opaque cores drawn
  front-to-back with depth, and a blended fringe, come first; the cost of the
  kinds that stay shaded — per pixel, or per command where issue #1406 finds the
  command count is the term — is measured by a variant sweep before any fast
  path is written, and comes second. The projection behind the order, at
  2340x1080: the full-screen gradient backdrop is 2.53 Mpx; the opaque instances
  over it are the header and the sixteen tiles, about 1.3 Mpx — the gallery is a
  layout node that draws nothing and the frost is translucent — and at the 5 to
  6 ms per megapixel the overlay path costs on this scene, rejecting that area
  is about 7 ms of the 31. Nothing yet falsifies it; the shaded-area derivation
  of issue #1296 gives the area each shape submits before and after, without a
  device — no derivation in the tree counts the pixels depth rejects — and D2's
  cadence is what shows the frame. The fringe's order against the transparent
  instances is issue #1402's ruling, which story #1412 depends on and must not
  re-derive, and #1296 lands before #1412. Story #1412 is the Unity painter's
  first; issue #1293 holds the lean painter's half and stays
  measure-then-decide, as its own body says the saving there is unproven; story
  #1413 is the second.

## Consequences

- Stories #1412 and #1413 each carry a before-and-after reading of both D2
  instruments in `docs/design/android-toolchain.md`; the pair closes the Unity
  painter against D1 on the device this record names, and each story's own
  target is its own. The lean painter's `surfaces` frame is about 22 ms on the
  same device, over the budget too; issue #1293 measures it against D1 and
  either implements or files the story that does — D1 binds it as much as the
  Unity painter, and #1293 is where that can fail. A reading on another device,
  another extent or another scene is a new row, not a substitute.
- This record binds the two stories and not the slice. Under epic #1120's
  standing declaration (`docs/roadmap.md`, v0.21) they may move out unfinished
  at the slice close; if the owner rules that the halving gates v0.21, they move
  to an MVP epic, since #1120's own body says an epic that could hold the slice
  open on optimization defeats its purpose. That ruling is not made here.
- The records that said this project has no frame budget now say it has one for
  this device and none for a display class (issue #549 stands, as the `related`
  line says) — `docs/design/android-toolchain.md`'s "Frame costs", Q-6 and Unity
  sections, `docs/features.md`, `docs/technotes/open-questions.md` and the
  frame-delta record above. Issues #1347 and #1107, which say no budget is set,
  are told by comment; their bodies stand as written on their dates.

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
