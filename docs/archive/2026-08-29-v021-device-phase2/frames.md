# Frame costs — Pixel 5 (redfin), Android 14 / API 34, device, release build

Device result. Name the device beside this table when it is recorded, and read `docs/design/android-toolchain.md` for what the adapter probe adds to it.

`paint` is this project's own instance packing, pure CPU. `submit` is the upload, the encode, the submit and the swapchain — named `submit` rather than `present` because `demo`'s desktop host prints `present` for paint plus present, and one word must not name two quantities. They are reported apart because they are different optimisation targets. `glyphs` is the glyph-quad count of the frame that **closed** the sample — a snapshot rather than a per-sample constant, since a scene whose text changes moves it: consecutive samples of `typography` reported 444 and 446. Read it as the order of magnitude the sample was drawing, never as a denominator exact for every frame in it.

Every row below was drawn at 1080x1984.

One row per reported sample of 240 **drawn** frames (`demo-android/src/timing.rs`). Rows are not averaged: the first sample of a scene carries pipeline warm-up, which reaches `max` and not `p50`.

`fps if unpaced` is **not the frame rate** — the loop is paced by vsync, and this is the rate the measured work alone would allow, which is what says how much headroom there is. `wall` is how long the 240 drawn frames took, and it exceeds 240 vsyncs whenever the scene idles between pulse phases: the loop skips a frame that would draw nothing.

| scene | extent | # | pid | frames | tick ms | paint mean | paint p50 | submit mean | p50 | p95 | max | glyphs | fps if unpaced | wall s | cpu % of one core |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| surfaces | 1080x1984 | 1 | 32222 | 240 | 0.24 | 0.03 | 0.03 | 21.47 | 20.82 | 28.77 | 56.67 | 32 | 46.0 | 14.1 (open) | 11 |
| surfaces | 1080x1984 | 2 | 32222 | 240 | 0.24 | 0.03 | 0.03 | 22.04 | 21.17 | 28.85 | 58.63 | 32 | 44.8 | 12.5 | 12 |
| surfaces | 1080x1984 | 3 | 32222 | 240 | 0.24 | 0.03 | 0.03 | 21.75 | 21.00 | 28.22 | 55.89 | 32 | 45.4 | 12.5 | 12 |
| typography | 1080x1984 | 1 | 32529 | 240 | 0.42 | 0.10 | 0.09 | 5.31 | 3.02 | 11.05 | 22.20 | 443 | 171.6 | 10.2 (open) | 12 |
| typography | 1080x1984 | 2 | 32529 | 240 | 0.36 | 0.10 | 0.09 | 10.32 | 10.30 | 11.14 | 13.86 | 445 | 92.8 | 6.2 | 14 |
| typography | 1080x1984 | 3 | 32529 | 240 | 0.37 | 0.10 | 0.09 | 10.29 | 10.29 | 10.96 | 13.69 | 445 | 93.0 | 7.5 | 14 |
| layout | 1080x1984 | 1 | 32742 | 240 | 0.18 | 0.01 | 0.01 | 7.41 | 9.74 | 11.25 | 25.23 | 0 | 131.5 | 16.2 (open) | 8 |
| layout | 1080x1984 | 2 | 32742 | 240 | 0.18 | 0.01 | 0.01 | 10.41 | 10.53 | 11.41 | 19.82 | 0 | 94.3 | 14.5 | 8 |
| layout | 1080x1984 | 3 | 32742 | 240 | 0.18 | 0.01 | 0.01 | 7.55 | 10.04 | 11.24 | 17.92 | 0 | 129.0 | 12.0 | 10 |

CPU is `utime + stime` from `/proc/<pid>/stat` over the interval each sample covers, at 100 jiffies per second, as a percentage of one core — so a value above 100 is a process using more than one. `—` means the sampler was not running across that interval, which is not the same as an idle process. A `(open)` interval begins when the sampler started rather than at a sample boundary.
