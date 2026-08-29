# Phase 2 of the v0.21 device lane — one `just android-measure` bundle

    taken     2026-08-29 on a Pixel 5 (redfin), Android 14 / API 34, Adreno 620
    from      `just android-measure` at commit 0b02637
    device    11181FDD4002MY

**The bundle's own directory was named `20240109T212654Z`, and that is not a
typo.** Every timestamp in a bundle is the device's, deliberately, so the
directory name and the logcat epochs agree — and this device's clock is unset,
reading 2024-01-09 while the host read 2026-08-29. `environment.md` records both
and says which one everything is timed by. The intervals are all
device-to-device and correct; only the provenance would mislead, which is why
this directory is named by the host date and the bundle's own name is stated
here. That is issue #1236's second half working as intended.

## What each file is evidence for

| file | issue | what it shows |
| --- | --- | --- |
| `attach.md`, `attach-*.log` | #960 | run 3 of the cold-launch table: release 0.33 s, debug 0.95 s to a first frame |
| `frames.md`, `frames-*.log` | #1304, #842 | the frame capture re-run through the streamed follower |
| `adapter-report.txt` | #885 | Vulkan 32 storage buffers, GLES exactly 4, device request OK on both |
| `layer-cost.txt` | #1128 | the render-target sweep, and see the warning below |
| `environment.md` | #1270 | the first bundle to carry `cores present` and `cores online` |
| `gpu-time.txt`, `sf-*.txt`, `gfxinfo.txt` | #842 | the GPU pass |

## Two readings that are not clean, recorded rather than dropped

**`layer-cost.txt` is noisier than the run it should reproduce.** The record
puts one mid-frame render-target switch at 1.95 ms ± 0.29 ms at 1920x1080 on
this GPU. This sweep gives marginal minima of +4.115, +0.919, +6.008, +2.073,
+0.722, +2.196, **-4.631** and +13.588 ms, and `max` jumps from about 50 ms to
about 135 ms once five layers are in play. A negative marginal is not a cost, so
this sweep does not measure one.

The likely cause is thermal state rather than the code: this ran about ten
minutes into a bundle that had already cross-compiled four Android binaries and
driven three 240-frame captures, on a phone. **It is not a retraction of the
1.95 ms figure** — a noisier measurement does not refute a quieter one — and it
is not a second reading of it either. What it is evidence for is that this
sweep needs a cool device, which nothing in the apparatus currently states or
enforces.

**`frames.md` is portrait and the record's table is landscape.** 1080x1984 here
against 2340x805 there, because this run did not set `wm size`. The
extent-independent quantities reproduce exactly — `paint` spans 0.01 to 0.10 ms
across all three scenes in both, `typography` 0.09 ms at 445 glyphs against 0.09
at 446, `layout` 0.01 ms at 0 glyphs in both. `submit` is fill-rate bound and is
**not** comparable across the two extents; nothing here should be read as a
second reading of it.

## Issue #1304's device half, which is what it was still open for

Its code half landed in `3eff33b`: `frame-capture.sh` and `run.sh` stream into a
host file through `ds_logcat_follow` instead of dumping the whole logcat ring,
and `ds_capture_state` guards their verdicts. What it still owed was a device
re-run of each, against a figure it had already produced.

**`frame-capture.sh`** — `paint` spans 0.01 to 0.10 ms across all three scenes,
the same span the record carries; `typography` 0.09 ms at 445 glyphs against
0.09 at 446, `layout` 0.01 ms at 0 glyphs in both. See the extent warning above:
only the extent-independent quantities are compared, because this run is
portrait and the recorded one is landscape.

**`run.sh`** — its own capture is the GPU pass. Against
`docs/archive/2026-08-17-v021-android-device-measurements/run-2-complete/gpu-capture.log`,
this run identifies the same layer (the `DemoActivity` SurfaceView), reports the
same `--latency: 127 frame row(s)` and the same `gfxinfo: Total frames rendered:
2`. `totalFrames` over the 15 s window differs — 291 here against 532 — and it
should: that is a compositor count over wall-clock on a device in a different
state, not a property of the capture path.

Both runs' bundles are named by a device clock that is years out: this one
`20240109T212654Z`, the 2026-08-17 one `20231229T090010Z`. That is the same
condition in both, recorded rather than corrected.
