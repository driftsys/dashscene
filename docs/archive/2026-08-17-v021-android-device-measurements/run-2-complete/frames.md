# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

`paint` is this project's own instance packing, pure CPU. `present` is the upload, the submit and the swapchain. They are reported apart because they are different optimisation targets, and `glyphs` is the frame's glyph-quad count so a cost per glyph is arithmetic.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | # | pid | frames | tick ms | paint mean | paint p50 | present mean | p50 | p95 | max | glyphs | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1 | 10186 | 240 | 0.24 | 0.03 | 0.03 | 11.93 | 11.88 | 13.17 | 34.20 | 32 | 81.9 | 8.7 (open) | 17 |
| surfaces | 2 | 10186 | 240 | 0.24 | 0.03 | 0.03 | 11.33 | 11.98 | 13.24 | 19.69 | 32 | 86.2 | 7.1 | 18 |
| surfaces | 3 | 10186 | 240 | 0.24 | 0.03 | 0.03 | 9.18 | 11.74 | 12.91 | 22.85 | 32 | 105.8 | 7.0 | 20 |
| typography | 1 | 10428 | 240 | 0.39 | 0.10 | 0.09 | 7.08 | 9.36 | 10.70 | 25.53 | 444 | 132.3 | 10.2 (open) | 12 |
| typography | 2 | 10428 | 240 | 0.36 | 0.10 | 0.09 | 7.27 | 9.54 | 10.52 | 11.15 | 446 | 129.4 | 6.2 | 15 |
| typography | 3 | 10428 | 240 | 0.36 | 0.09 | 0.09 | 9.81 | 10.05 | 10.90 | 12.57 | 446 | 97.4 | 7.5 | 14 |
| layout | 1 | 10655 | 240 | 0.18 | 0.01 | 0.01 | 5.05 | 2.86 | 10.29 | 22.58 | 0 | 190.8 | 16.2 (open) | 8 |
| layout | 2 | 10655 | 240 | 0.19 | 0.01 | 0.01 | 4.93 | 2.52 | 11.03 | 33.47 | 0 | 195.1 | 14.6 | 8 |
| layout | 3 | 10655 | 240 | 0.19 | 0.01 | 0.01 | 9.66 | 10.08 | 11.04 | 15.79 | 0 | 101.4 | 14.6 | 9 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
