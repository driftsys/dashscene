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
| `attach.md`, `attach-*.log` | #960 | run 3 of the cold-launch table: release 0.35 s, debug 0.95 s to a first frame (0.33 s is that row's `acquire`, a different column) |
| `frames.md`, `frames-*.log` | #1304, #842 | the frame capture re-run through the streamed follower |
| `adapter-report.txt` | #885 | Vulkan 32 storage buffers, GLES exactly 4, device request OK on both |
| `layer-cost.txt` | #1128 | the render-target sweep, and see the warning below |
| `environment.md` | — | what the bundle was taken on. **It carries no cpu block**: this bundle was taken at `0b02637`, before `ds_environment` was changed |
| `environment-with-cpu.md` | #1270 | the same block regenerated on the same device after the change, and the only capture in this directory that holds `cores present` / `cores online` |
| `gpu-time.txt`, `sf-*.txt`, `gfxinfo.txt` | #842 | the GPU pass |

## Two readings that are not clean, recorded rather than dropped

**`layer-cost.txt` is noisier than the run it should reproduce, and the cause
is not established.** The record puts one mid-frame render-target switch at
1.95 ms ± 0.29 ms at 1920x1080 on this GPU. All twelve marginal minima from this
sweep:

    +4.115  +0.919  +6.008  +2.073  +0.722  +2.196  -4.631
    +13.588  -8.880  +1.755  +1.945  +15.254

**Two of them are negative**, and `max` steps from about 50 ms at 0-4 layers to
about 135 ms from 5 layers on. A negative marginal is not a cost, so this sweep
does not measure one.

**An earlier draft of this file blamed thermal state from prior work in the
bundle, and that is refuted by the ordering.** `run.sh` runs this sweep as step
2, before the release APK is packaged and before any frame capture: the bundle's
own file times put `adapter-report.txt` at 12:15:41 and `layer-cost.txt` at
12:17:29, under two minutes in, with `frames.md` not written until 12:21:45. The
device was near idle beforehand, and the host does the cross-compiling.

What remains consistent with the numbers is **self-heating inside the sweep**,
which walks 0 to 12 layers at 1920x1080 with 120 frames per point and never
cools: the noise grows with layer count and `max` steps exactly where the load
does. That is a hypothesis this bundle cannot settle — sweeping in reverse, or
with a cooldown between points, would — and it is recorded as one rather than as
a cause. **It is not a retraction of the 1.95 ms figure**: a noisier measurement
does not refute a quieter one, and this is not a second reading of it. Issue
#1387 carries it.

**Which recorded table this run compares against.** `frames.md` here is
1080x1984, and the record carries three sets. The `paint` and `glyphs` figures
come from the **1280x445** table, not the 2340x805 one, which has no `paint`
column at all. Against it, `paint` spans 0.01 to 0.10 ms across all three scenes
in both, `typography` is 0.09 ms at 445 glyphs against 0.09 at 446, and `layout`
is 0.01 ms at 0 glyphs in both.

**`submit` is comparable after all, and it reproduces.** The record's
lean-painter table is at **1080x1984** — this run's own extent, same device,
same day — so the fill-rate objection does not apply to it. `surfaces` submit
mean is 21.47-22.04 here against 21.64-21.90 there, p50 20.82-21.17 against
20.73-21.27, p95 28.22-28.85 against 28.35-29.51, with `tick` 0.24 against
0.23-0.24 and `paint` 0.03 in both.

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

**The strongest reproduction is `submit` against the lean-painter table above**,
which the first draft of this file wrongly set aside as incomparable.

**`run.sh`** — its own capture is the GPU pass. Against
`docs/archive/2026-08-17-v021-android-device-measurements/run-2-complete/gpu-capture.log`,
this run identifies the same layer (the `DemoActivity` SurfaceView), reports the
same `--latency: 127 frame row(s)` and the same `gfxinfo: Total frames rendered:
2`. `totalFrames` over the 15 s window differs — 291 here against 532 — and it
should: that is a compositor count over wall-clock on a device in a different
state, not a property of the capture path.

## The cpu block was captured after the bundle, not in it

`environment-with-cpu.md` is a separate artifact and the table above says so,
because the ordering matters and would otherwise be invisible: the
`just android-measure` bundle here ran at `0b02637`, before `ds_environment`
learned to read the core count, so its own `environment.md` ends at
`ro.boot.qemu`. The block was regenerated against the same attached device
afterwards. The raw device reads behind it are
`/sys/devices/system/cpu/present` = `0-7` and `.../online` = `0-7`.

**The next bundle taken on any device carries it inline**, which is the point of
putting it in `ds_environment` rather than reading it by hand.

Both runs' bundles are named by a device clock that is years out: this one
`20240109T212654Z`, the 2026-08-17 one `20231229T090010Z`. That is the same
condition in both, recorded rather than corrected.
