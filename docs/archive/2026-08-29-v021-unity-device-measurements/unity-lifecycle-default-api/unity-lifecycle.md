# Unity's Android lifecycle over the lease (issue #1346)

    device    Pixel 5
    app       com.driftsys.dashscene.showcase
    activity  com.driftsys.dashscene.showcase/com.unity3d.player.UnityPlayerActivity
    taken     20240109T103540Z (the device's own clock)
    host      20260828T232441Z (this machine's clock)
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
| rotation to landscape | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.16 ms, draw mean 0.20 p50 0.19 p95 0.25 max 0.36 ms (2842.7 fps if unpaced) |
| rotation back to portrait | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.15 ms, draw mean 0.19 p50 0.19 p95 0.25 max 0.30 ms (2916.8 fps if unpaced) |
| backgrounded and resumed | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.14 ms, draw mean 0.19 p50 0.18 p95 0.27 max 1.44 ms (3001.0 fps if unpaced) |
| split-screen cold launch | survived | scene surfaces at 1080x2340 over 240 frames — tick 0.11 ms, draw mean 0.23 p50 0.21 p95 0.27 max 5.85 ms (2950.2 fps if unpaced) |

The raw capture is `lifecycle.log` beside this file.
