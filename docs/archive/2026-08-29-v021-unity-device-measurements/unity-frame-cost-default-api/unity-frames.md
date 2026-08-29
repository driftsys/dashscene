# The Unity painter's frame cost on a device (issue #1347)

    device    Pixel 5
    app       com.driftsys.dashscene.showcase
    taken     20240109T102949Z (the device's own clock)
    host      20260828T232231Z (this machine's clock)
    commit    0cf9024b
    display   wm size 2340x1080, rotated to landscape after launch
    sweeps    3, 14 s per entry

`tick` is `ds_runtime_tick` and is the same quantity
`demo/src/shell.rs` reports. `draw` is the lease, `BrgPainter.Draw`
and the release — every part of the frame this project executes — and
EXCLUDES the GPU's execution of the batches, URP's passes, culling and
the swapchain present, because Unity runs those after `Update` returns.
`unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs`
states the definition term by term.

One row per reported sample of 240 drawn frames. Rows are not averaged:
the first sample of an entry carries pipeline warm-up.

| sweep | entry | extent | tick ms | draw mean | p50 | p95 | max | fps if unpaced |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A | scene surfaces | 2340x1080 | 0.10 | 0.20 | 0.18 | 0.26 | 4.36 | 3234.1 |
| A | scene typography | 2340x1080 | 0.15 | 0.33 | 0.34 | 0.40 | 0.49 | 2082.5 |
| A | scene layout | 2340x1080 | 0.05 | 0.18 | 0.19 | 0.24 | 0.34 | 4342.4 |
| A | paint: fills, strokes, corners, and one image fill the painter refuses | 2340x1080 | 0.01 | 0.19 | 0.19 | 0.25 | 0.88 | 4889.3 |
| A | layout: a variant set, at rest — nothing here can switch it | 2340x1080 | 0.01 | 0.16 | 0.16 | 0.23 | 0.32 | 5662.3 |
| A | layout: the variant shelf | 2340x1080 | 0.01 | 0.17 | 0.17 | 0.22 | 0.25 | 5391.4 |
| B | scene surfaces | 1080x2340 | 0.11 | 0.22 | 0.20 | 0.26 | 3.96 | 3019.8 |
| B | scene typography | 1080x2340 | 0.15 | 0.33 | 0.34 | 0.41 | 0.45 | 2047.8 |
| B | scene layout | 1080x2340 | 0.05 | 0.18 | 0.18 | 0.24 | 0.25 | 4476.2 |
| B | paint: fills, strokes, corners, and one image fill the painter refuses | 1080x2340 | 0.01 | 0.19 | 0.18 | 0.23 | 1.08 | 5033.3 |
| B | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.17 | 0.17 | 0.21 | 0.24 | 5543.2 |
| B | layout: the variant shelf | 1080x2340 | 0.01 | 0.17 | 0.17 | 0.22 | 0.27 | 5483.3 |
| C | scene surfaces | 1080x2340 | 0.11 | 0.23 | 0.21 | 0.28 | 5.55 | 2888.4 |
| C | scene typography | 1080x2340 | 0.15 | 0.34 | 0.34 | 0.40 | 0.46 | 2049.8 |
| C | scene layout | 1080x2340 | 0.05 | 0.18 | 0.18 | 0.24 | 0.30 | 4468.3 |
| C | paint: fills, strokes, corners, and one image fill the painter refuses | 1080x2340 | 0.01 | 0.18 | 0.18 | 0.23 | 0.85 | 5095.9 |
| C | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.17 | 0.17 | 0.23 | 0.25 | 5472.9 |
| C | layout: the variant shelf | 1080x2340 | 0.01 | 0.17 | 0.16 | 0.23 | 0.25 | 5560.3 |

The raw captures are `sweep-<letter>.log` beside this file — this run's, not
the Vulkan run's, which are one directory over under `unity-frame-cost/`. Every row
above is one line out of them, reshaped and not recomputed.
