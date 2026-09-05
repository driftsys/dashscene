# Unity thread cost — the fixture capture

Device result, from the Unity showcase player — `measure/android/unity-frame-cost.sh`, one row per reported sample and nothing averaged across sweeps. The engine floor is in every figure here: both instruments are inside a Unity frame, so the renderer's share is the difference from the empty entry's row and not the row itself. Name the device beside this table when it is recorded.

These are Unity's own `ProfilerRecorder` counters, not a bracket around code this project executes — so they include what `unity-frames.md` excludes by construction: the culling callback, the render thread's encode, URP's passes and a Canvas rebuild. `canvas` is `Canvas.SendWillRenderCanvases` plus `Canvas.BuildBatch`, which is zero for the painter and is the term a Canvas renderer is judged on. `gc` is `GC Allocated In Frame` divided by the sample's frames. `unity/com.driftsys.dashscene/Runtime/Engine/DashsceneThreadCost.cs` states the definition term by term.

**Every column carries the engine floor.** Subtract the empty entry's row, taken in the same run, for a renderer's own share; a figure read off one row alone describes Unity as much as it describes the renderer.

Every row below was drawn at 1080x2340.

One row per reported sample of 240 **drawn** frames, after 60 warm-up frames discarded at every entry change — so no row carries an entry's load or its first Canvas bakes.

| sweep | entry | extent | # | pid | frames | main mean | main p95 | render mean | render p95 | canvas ms | gc B/frame | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| unity-frame-cost | scene surfaces | 1080x2340 | 1 | 20396 | 240 | 32.83 | 34.14 | — | — | — | — | — | — |
| unity-frame-cost | scene surfaces | 1080x2340 | 2 | 20396 | 240 | 32.76 | 34.76 | — | — | — | — | 7.9 | — |
| unity-frame-cost | scene typography | 1080x2340 | 1 | 20396 | 240 | 16.76 | 17.77 | — | — | — | — | 7.5 | 39 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.

## Unreadable

None. Every `unity-threads` line in these captures parsed whole.
