# Unity frame cost — the fixture capture

Device result, from the Unity showcase player — `measure/android/unity-frame-cost.sh`, one row per reported sample and nothing averaged across sweeps. The engine floor is in every figure here: both instruments are inside a Unity frame, so the renderer's share is the difference from the empty entry's row and not the row itself. Name the device beside this table when it is recorded.

`tick` is `ds_runtime_tick`, the one term directly comparable with the lean host's. `draw` is the frame lease, `BrgPainter.Draw`, the mark and the release — every part of the frame this project executes — and EXCLUDES the GPU's execution of the batches, URP's own passes, culling and the swapchain present, because Unity runs those after `Update` returns. `unity-threads.md` beside this file reports what that excludes. `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs` states the definition term by term.

Every row below was drawn at 1080x2340.

One row per reported sample of 240 **drawn** frames. Rows are not averaged: the first sample of an entry carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — Unity paces the loop, and this is the rate the two measured terms alone would allow. `wall` is how long the sample's frames took.

| sweep | entry | extent | # | pid | frames | tick ms | draw mean | p50 | p95 | max | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| unity-frame-cost | scene surfaces | 1080x2340 | 1 | 20396 | 240 | 0.12 | 0.20 | 0.21 | 0.26 | 0.29 | 3102.9 | — | — |
| unity-frame-cost | scene surfaces | 1080x2340 | 2 | 20396 | 240 | 0.14 | 0.22 | 0.22 | 0.26 | 0.33 | 2810.2 | 7.9 | — |
| unity-frame-cost | scene typography | 1080x2340 | 1 | 20396 | 240 | 0.11 | 0.31 | 0.35 | 0.38 | 0.44 | 2399.7 | 8.4 | 39 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.

## Unreadable

1 line(s) carried the `unity-frames` marker and did not parse — a record the logcat ring cut, or an instrument whose line shape changed without this parser. Each is quoted verbatim; none of them is in the table above.

- `unity-frame-cost.log`: `1705476872.561 20396 20422 I Unity   : [showcase] frame cost — scene typography at 1080x2340 over 240 frames — tick 0.15 ms, draw mean 0.34 p50 0.36 p95 0.41 max 0.46 ms`
