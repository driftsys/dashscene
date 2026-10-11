# Unity CPU per presented frame — the fixture capture

Device result, from the Unity showcase player — `measure/android/unity-frame-cost.sh`, one row per compositor window and nothing averaged across sweeps. The engine floor is in every figure here, so the renderer's share is the difference from the empty entry's row and not the row itself. Name the device beside this table when it is recorded.

One row per compositor window: `dumpsys SurfaceFlinger --timestats` cleared and dumped once per entry — a window over the whole sweep would give one figure per sweep rather than per row. The compositor window is longer than `window s` by about three `adb shell` round trips: it opens at the `-clear` before the scene change and the start marker, and closes at the `-dump` after the end marker. `presented frames` and `dropped` are read from the package's own layer in that dump; a dump whose layer cannot be found is reported under Unreadable below rather than as a row of zeroes.

| sweep | entry | extent | window s | presented frames | dropped | cpu % of one core | cpu ms per presented frame | drawn frames (player) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| unity-frame-cost | 1 | 1080x2340 | 8.4 | 163 | 0 | 39 | 19.87 | 240 |
| unity-frame-cost | 3 | — | 10.0 | 200 | 2 | — | — | 0 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
`cpu ms per presented frame` is `window s × cpu % / 100 × 1000 / presented frames`, computed from the unrounded `window s` and `cpu %`, so the rounded columns reproduce it only approximately. `cpu %` here is over the sampler's readings at or before the window's start and at or before its end, not over the window's own seconds. `drawn frames (player)` sums the frame-cost samples whose report falls inside the window, each with all of its frames, so it is not the number of frames drawn inside the window. `(open)` in `window s` here means no start marker or no end marker was matched for this entry: either one of the two `dashscene-window` markers was never written (every `log` call `unity-frame-cost.sh` makes to write one is `|| true`), or the sweep's pid could not be resolved — from its own samples, or, for a sweep with none (one held at rest), from its window markers, which resolve it only when they state one pid — so no marker is matched. Both differ from the cause the CPU footnote above states for its own `(open)`.

## Unreadable

2 dump(s) could not be placed in the table above — no layer naming the package (a capture taken before the app's first frame, one the ring cut mid-write, or a layer name changed by a SurfaceFlinger release), a matched layer whose totalFrames or droppedFrames did not parse (the ring cut it mid-write), or a name `TIMESTATS_NAME` does not fit at all. Each is named verbatim, with its own reason.

- `unity-frame-cost-entry-2-sf-timestats.txt`: no layer named the package, or its totalFrames/droppedFrames did not parse
- `unity-frame-cost-stray-sf-timestats.txt`: name does not fit <sweep>-entry-<n>-sf-timestats.txt
