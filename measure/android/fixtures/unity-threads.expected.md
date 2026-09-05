# Unity thread cost — the fixture capture

Device result, from the Unity showcase player — `measure/android/unity-frame-cost.sh`, one row per reported sample and nothing averaged across sweeps. The engine floor is in every figure here: both instruments are inside a Unity frame, so the renderer's share is the difference from the empty entry's row and not the row itself. Name the device beside this table when it is recorded.

These are Unity's own `ProfilerRecorder` counters, not a bracket around code this project executes — so they include what `unity-frames.md` excludes by construction: the culling callback, the render thread's encode, URP's passes and a Canvas rebuild. `canvas` is `Canvas.SendWillRenderCanvases` plus `Canvas.BuildBatch`, which is zero for the painter and is the term a Canvas renderer is judged on. `gc` is `GC Allocated In Frame` divided by the sample's frames. `unity/com.driftsys.dashscene/Runtime/Engine/DashsceneThreadCost.cs` states the definition term by term.

**Every column carries the engine floor.** Subtract the empty entry's row, taken in the same run, for a renderer's own share; a figure read off one row alone describes Unity as much as it describes the renderer.

Every row below was drawn at 1080x2340.

One row per reported sample of 240 **drawn** frames, after a warm-up discarded at every entry change — so no row carries an entry's load or its first Canvas bakes. The count is `ThreadCostAccumulator.Sample`, and `unity/package-gate`'s `thread_cost_instrument` re-derives this number from that constant rather than letting the two drift.

| sweep | entry | extent | # | pid | frames | main mean | main p50 | main p95 | main max | render mean | render p50 | render p95 | render max | canvas ms | gc B/frame | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| unity-frame-cost | scene surfaces | 1080x2340 | 1 | 23903 | 240 | 32.76 | 33.39 | 34.55 | 50.90 | — | — | — | — | — | — | — | — |
| unity-frame-cost | scene surfaces | 1080x2340 | 2 | 23903 | 240 | 32.83 | 33.38 | 34.46 | 50.13 | — | — | — | — | — | — | 7.9 | — |
| unity-frame-cost | scene typography | 1080x2340 | 1 | 23903 | 240 | 16.69 | 16.70 | 17.43 | 20.09 | — | — | — | — | — | — | 7.4 | 39 |
| unity-frame-cost | scene typography | 1080x2340 | 2 | 23903 | 240 | 16.69 | 16.70 | 17.43 | 20.09 | 3.10 | 2.90 | 4.20 | 6.30 | 5.30 | 640 | 0.0 | — |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.

## Unreadable

1 line(s) carried the `unity-threads` marker and did not parse — a record the logcat ring cut, or an instrument whose line shape changed without this parser. Each is quoted verbatim; none of them is in the table above.

- `unity-frame-cost.log`: `1705482050.973 23903 23922 I Unity   : [showcase] thread cost — scene surfaces at 1080x2340 over 240 frames — main mean 32.76 p50 33.39 p95 34.55 max 50.90 ms, render mean — p50 — p95 — max — ms,`
