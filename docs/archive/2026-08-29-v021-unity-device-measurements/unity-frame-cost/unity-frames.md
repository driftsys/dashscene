# The Unity painter's frame cost on a device (issue #1347)

    device    Pixel 5
    app       com.driftsys.dashscene.showcase
    taken     20240109T111107Z (the device's own clock)
    host      20260829T000353Z (this machine's clock)
    commit    0cf9024b
    display   asked for wm size 2340x1080, and one rotation after launch
    extents   1080x2340 (as reported)
    graphics  Vulkan, Adreno (TM) 620, Vulkan 1.1.0 [512.490.0 (0x801ea000)]
    rung      RawBuffer
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
| A | scene surfaces | 1080x2340 | 0.12 | 0.19 | 0.19 | 0.26 | 0.31 | 3244.4 |
| A | scene typography | 1080x2340 | 0.15 | 0.32 | 0.33 | 0.41 | 0.72 | 2120.8 |
| A | scene layout | 1080x2340 | 0.05 | 0.19 | 0.18 | 0.24 | 0.31 | 4312.0 |
| A | paint: fills, strokes, corners, and one image fill the painter refuses | 1080x2340 | 0.01 | 0.19 | 0.18 | 0.23 | 0.91 | 5061.5 |
| A | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.16 | 0.15 | 0.21 | 0.28 | 5880.1 |
| A | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.17 | 0.17 | 0.21 | 0.30 | 5492.9 |
| A | layout: the variant shelf | 1080x2340 | 0.01 | 0.16 | 0.17 | 0.21 | 0.25 | 5723.5 |
| B | scene surfaces | 1080x2340 | 0.12 | 0.19 | 0.19 | 0.25 | 0.32 | 3278.5 |
| B | scene typography | 1080x2340 | 0.15 | 0.33 | 0.34 | 0.39 | 0.76 | 2104.2 |
| B | scene layout | 1080x2340 | 0.05 | 0.19 | 0.19 | 0.24 | 0.31 | 4282.4 |
| B | paint: fills, strokes, corners, and one image fill the painter refuses | 1080x2340 | 0.01 | 0.17 | 0.17 | 0.21 | 0.73 | 5379.2 |
| B | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.16 | 0.16 | 0.20 | 0.25 | 5848.3 |
| B | layout: the variant shelf | 1080x2340 | 0.01 | 0.16 | 0.15 | 0.21 | 0.27 | 5832.2 |
| C | scene surfaces | 1080x2340 | 0.12 | 0.19 | 0.19 | 0.26 | 0.32 | 3240.5 |
| C | scene typography | 1080x2340 | 0.15 | 0.32 | 0.33 | 0.40 | 0.45 | 2124.5 |
| C | scene layout | 1080x2340 | 0.05 | 0.19 | 0.18 | 0.25 | 0.37 | 4253.2 |
| C | paint: fills, strokes, corners, and one image fill the painter refuses | 1080x2340 | 0.01 | 0.19 | 0.18 | 0.23 | 1.22 | 4935.7 |
| C | layout: a variant set, at rest — nothing here can switch it | 1080x2340 | 0.01 | 0.16 | 0.16 | 0.20 | 0.36 | 5807.1 |
| C | layout: the variant shelf | 1080x2340 | 0.01 | 0.16 | 0.16 | 0.21 | 0.27 | 5747.6 |

The raw captures are `sweep-<letter>.log` beside this file. Every row
above is one line out of them, reshaped and not recomputed.
