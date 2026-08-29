# Unity's Android lifecycle over the lease (issue #1346)

    device    Pixel 5
    app       com.driftsys.dashscene.showcase
    activity  com.driftsys.dashscene.showcase/com.unity3d.player.UnityPlayerActivity
    taken     20240109T110403Z (the device's own clock)
    host      20260828T235307Z (this machine's clock)
    commit    0cf9024b
    watched   40 s per case

A Unity host calls none of `ds_runtime_attach_surface`,
`ds_runtime_detach_surface`, `ds_runtime_resize` or `ds_runtime_draw`,
so these cases exercise the lease and the painter's GPU resources across
an event Unity owns — not the surface handshake D4 describes for a
platform host.

`survived` means the player reported a frame cost AFTER the event, so 240
frames were drawn after it. `NO FRAME OBSERVED` is the wedge the lease
record makes possible and is a bound, not a duration.

| case | outcome | the frame cost it reported after |
| --- | --- | --- |
| rotation to landscape | survived | scene surfaces at 2340x1080 over 240 frames — tick 0.13 ms, draw mean 0.21 p50 0.22 p95 0.28 max 0.34 ms (2876.3 fps if unpaced) |
| rotation back to portrait | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.12 ms, draw mean 0.16 p50 0.17 p95 0.21 max 0.25 ms (3514.7 fps if unpaced) |
| backgrounded and resumed | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.13 ms, draw mean 0.17 p50 0.17 p95 0.24 max 0.56 ms (3340.9 fps if unpaced) |
| split-screen cold launch | NOT EXERCISED — the extent never changed | scene surfaces at 1080x2340 over 240 frames — tick 0.10 ms, draw mean 0.20 p50 0.18 p95 0.24 max 4.11 ms (3358.2 fps if unpaced) — windowing mode fullscreen, drawable 1080x2340 before |

The raw capture is `lifecycle.log` beside this file.
