# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

`paint` is this project's own instance packing, pure CPU. `present` is the upload, the submit and the swapchain. They are reported apart because they are different optimisation targets, and `glyphs` is the frame's glyph-quad count so a cost per glyph is arithmetic.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | # | pid | frames | tick ms | draw mean | p50 | p95 | max | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1 | 8468 | 240 | 0.25 | 0.04 | 0.03 | 11.15 | 11.82 | 12.97 | 35.93 | 32 | 87.5 | 8.7 (open) | 17 |
| surfaces | 2 | 8468 | 240 | 0.24 | 0.03 | 0.03 | 11.72 | 11.87 | 12.75 | 21.65 | 32 | 83.4 | 7.1 | 18 |
| surfaces | 3 | 8468 | 240 | 0.25 | 0.03 | 0.03 | 9.10 | 11.61 | 12.60 | 18.57 | 32 | 106.6 | 7.1 | 20 |
| typography | 1 | 8714 | 240 | 0.39 | 0.10 | 0.09 | 7.24 | 9.52 | 10.49 | 27.99 | 444 | 129.5 | 10.2 (open) | 12 |
| typography | 2 | 8714 | 240 | 0.36 | 0.09 | 0.09 | 7.26 | 9.43 | 10.71 | 13.96 | 446 | 129.6 | 6.2 | 14 |
| typography | 3 | 8714 | 240 | 0.36 | 0.09 | 0.09 | 7.05 | 9.28 | 10.41 | 12.10 | 446 | 133.2 | 7.5 | 14 |
| layout | 1 | 8946 | 240 | 0.18 | 0.01 | 0.01 | 7.11 | 9.64 | 10.90 | 17.50 | 0 | 136.9 | 16.2 (open) | 8 |
| layout | 2 | 8946 | 240 | 0.18 | 0.01 | 0.01 | 6.98 | 9.49 | 10.73 | 15.93 | 0 | 139.5 | 14.6 | 8 |
| layout | 3 | 8946 | 240 | 0.17 | 0.01 | 0.01 | 7.18 | 9.71 | 10.88 | 16.79 | 0 | 135.8 | 14.6 | 8 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
