# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | # | pid | frames | tick ms | draw mean | p50 | p95 | max | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1 | 9713 | 240 | 1.07 | 26.38 | 24.53 | 59.90 | 227.28 | 36.4 | 16.7 (open) | 36 |
| surfaces | 2 | 9713 | 240 | 1.04 | 27.49 | 25.90 | 47.14 | 76.89 | 35.1 | 14.5 | 37 |
| surfaces | 3 | 9713 | 240 | 1.04 | 27.64 | 26.43 | 46.71 | 74.85 | 34.9 | 14.6 | 37 |
| typography | 1 | 10136 | 240 | 1.70 | 15.10 | 11.60 | 19.91 | 160.26 | 59.5 | 13.2 (open) | 32 |
| typography | 2 | 10136 | 240 | 1.41 | 14.74 | 11.59 | 19.51 | 54.10 | 61.9 | 10.2 | 36 |
| typography | 3 | 10136 | 240 | 1.39 | 14.62 | 11.58 | 19.58 | 54.16 | 62.4 | 10.2 | 35 |
| layout | 1 | 10432 | 240 | 1.13 | 8.40 | 5.85 | 13.42 | 31.35 | 104.9 | 16.4 (open) | 15 |
| layout | 2 | 10432 | 240 | 1.07 | 5.39 | 4.64 | 11.54 | 18.83 | 154.7 | 14.6 | 15 |
| layout | 3 | 10432 | 240 | 1.05 | 6.51 | 4.67 | 13.98 | 23.29 | 132.3 | 14.6 | 15 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
