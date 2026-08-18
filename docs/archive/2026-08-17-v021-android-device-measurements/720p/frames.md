# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | # | pid | frames | tick ms | draw mean | p50 | p95 | max | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1 | 7155 | 240 | 1.24 | 19.92 | 17.29 | 25.21 | 227.96 | 47.3 | 16.2 (open) | 36 |
| surfaces | 2 | 7155 | 240 | 1.21 | 18.56 | 17.27 | 21.44 | 71.71 | 50.6 | 13.1 | 39 |
| surfaces | 3 | 7155 | 240 | 1.25 | 19.26 | 17.35 | 21.65 | 72.47 | 48.8 | 14.2 | 37 |
| typography | 1 | 7557 | 240 | 1.52 | 11.82 | 10.27 | 16.15 | 158.53 | 74.9 | 10.8 (open) | 36 |
| typography | 2 | 7557 | 240 | 1.24 | 11.23 | 10.26 | 12.84 | 54.28 | 80.2 | 7.7 | 41 |
| typography | 3 | 7557 | 240 | 1.23 | 11.11 | 10.29 | 12.60 | 57.06 | 81.0 | 7.7 | 42 |
| layout | 1 | 7822 | 240 | 1.11 | 7.81 | 5.64 | 13.10 | 31.74 | 112.2 | 16.4 (open) | 14 |
| layout | 2 | 7822 | 240 | 1.23 | 5.54 | 5.56 | 6.85 | 23.26 | 147.6 | 14.7 | 15 |
| layout | 3 | 7822 | 240 | 1.07 | 7.68 | 4.66 | 13.15 | 17.90 | 114.3 | 14.6 | 15 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
