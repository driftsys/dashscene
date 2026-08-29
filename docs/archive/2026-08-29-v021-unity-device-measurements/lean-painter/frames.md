# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

`paint` is this project's own instance packing, pure CPU. `submit` is the upload, the encode, the submit and the swapchain — named `submit` rather than `present` because `demo`'s desktop host prints `present` for paint plus present, and one word must not name two quantities. They are reported apart because they are different optimisation targets. `glyphs` is the glyph-quad count of the frame that **closed** the sample — a snapshot rather than a per-sample constant, since a scene whose text changes moves it: consecutive samples of `typography` reported 444 and 446. Read it as the order of magnitude the sample was drawing, never as a denominator exact for every frame in it.

Every row below was drawn at 1080x1984.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | extent | # | pid | frames | tick ms | paint mean | paint p50 | submit mean | p50 | p95 | max | glyphs | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1080x1984 | 1 | 29978 | 240 | 0.24 | 0.03 | 0.03 | 21.86 | 21.27 | 29.51 | 66.75 | 32 | 45.2 | 14.1 (open) | 11 |
| surfaces | 1080x1984 | 2 | 29978 | 240 | 0.23 | 0.03 | 0.03 | 21.90 | 21.07 | 28.35 | 65.67 | 32 | 45.1 | 12.5 | 11 |
| surfaces | 1080x1984 | 3 | 29978 | 240 | 0.24 | 0.03 | 0.03 | 21.64 | 20.73 | 28.67 | 56.21 | 32 | 45.6 | 12.5 | 12 |
| typography | 1080x1984 | 1 | 30291 | 240 | 0.39 | 0.10 | 0.10 | 7.67 | 9.83 | 10.98 | 19.55 | 443 | 122.5 | 10.2 (open) | 12 |
| typography | 1080x1984 | 2 | 30291 | 240 | 0.37 | 0.10 | 0.10 | 10.36 | 10.29 | 10.93 | 27.97 | 445 | 92.4 | 6.2 | 14 |
| typography | 1080x1984 | 3 | 30291 | 240 | 0.36 | 0.10 | 0.09 | 10.14 | 10.17 | 10.90 | 13.87 | 445 | 94.4 | 7.5 | 14 |
| layout | 1080x1984 | 1 | 30498 | 240 | 0.18 | 0.01 | 0.01 | 10.24 | 10.23 | 11.17 | 19.91 | 0 | 95.8 | 16.1 (open) | 8 |
| layout | 1080x1984 | 2 | 30498 | 240 | 0.19 | 0.01 | 0.01 | 7.60 | 10.15 | 11.13 | 20.36 | 0 | 128.2 | 14.5 | 9 |
| layout | 1080x1984 | 3 | 30498 | 240 | 0.19 | 0.01 | 0.01 | 7.48 | 9.94 | 10.84 | 18.40 | 0 | 130.2 | 14.5 | 9 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
