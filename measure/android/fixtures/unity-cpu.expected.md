# Unity CPU per presented frame — the fixture capture

Device result, from the Unity showcase player — `measure/android/unity-frame-cost.sh`, one row per reported sample and nothing averaged across sweeps. The engine floor is in every figure here: both instruments are inside a Unity frame, so the renderer's share is the difference from the empty entry's row and not the row itself. Name the device beside this table when it is recorded.

One row per compositor window: `dumpsys SurfaceFlinger --timestats` cleared and dumped once per entry, over the same seconds as that entry's dwell — a window over the whole sweep would give one figure per sweep rather than per row. `presented frames` and `dropped` are read from the package's own layer in that dump; a dump whose layer cannot be found is reported under Unreadable below rather than as a row of zeroes.

| sweep | entry | extent | window s | presented frames | dropped | cpu % of one core | cpu ms per presented frame | drawn frames (player) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| unity-frame-cost | 1 | 1080x2340 | 8.4 | 163 | 0 | 39 | 19.87 | 240 |
| unity-frame-cost | 3 | — | 10.0 | 200 | 2 | — | — | 0 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
`cpu ms per presented frame` is `window s × cpu % / 100 × 1000 / presented frames` — from the three columns beside it on the same row, never recomputed from a different pair. `(open)` in `window s` here means one of this entry's two `dashscene-window` markers is missing — every `log` call `unity-frame-cost.sh` makes to write one is `|| true`, so a dropped write reads as an incomplete window rather than aborting the sweep — which is a different cause from the one the CPU footnote above states for its own `(open)`.

## Unreadable

2 dump(s) could not be placed in the table above — no layer naming the package (a capture taken before the app's first frame, one the ring cut mid-write, or a layer name changed by a SurfaceFlinger release), a matched layer whose totalFrames or droppedFrames did not parse (the ring cut it mid-write), or a name `TIMESTATS_NAME` does not fit at all. Each is named verbatim, with its own reason.

- `unity-frame-cost-entry-2-sf-timestats.txt`: no layer named the package, or its totalFrames/droppedFrames did not parse
- `unity-frame-cost-stray-sf-timestats.txt`: name does not fit <sweep>-entry-<n>-sf-timestats.txt
