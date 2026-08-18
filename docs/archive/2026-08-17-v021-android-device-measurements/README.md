# The first Android measurements taken on target hardware

    taken   2026-08-17, on a Google Pixel 5 (`redfin`), Android 14 / API 34,
            Adreno 620 — a tiling GPU
    by      `just android-measure` (story #1229's apparatus), plus two
            frame-only captures and one run by hand
    record  [`../../design/android-toolchain.md`](../../design/android-toolchain.md),
            "What the device measured" — the numbers, and what they do and do
            not support
    closed  #885, #842, #1128 and #969 against these

**This is the raw evidence, archived verbatim**, on the same rule this directory
applies to a spent driver prompt: the durable record is written, and the original
it was derived from is kept rather than deleted. It is here rather than in a
neighbouring repository because evidence separated from the record it supports is
evidence nobody finds, and it is here rather than nowhere because **these
measurements are not reproducible** — re-running gave 1.95 then 1.77 ms for Q-6,
and 14.57 then 0.93 s for the debug attach. Re-running does not recover a number
a record cites.

`measure/android/frame-table.py` re-derives every `frames.md` here from the
`frames-*.log` beside it, and reads both the pre- and post-split line shapes.

## What each directory is, and which claim it carries

| directory | extent | instrument | what the record cites it for |
| --- | --- | --- | --- |
| `landscape/` | 2340x805 | combined | the three-scene table, and half the resolution comparison |
| `720p/` | 1280x445 | combined | the other half — 70% fewer pixels cost 32%, 11% and **0%** less |
| `720p-split/` | 1280x445 | split | paint 0.01-0.10 ms; 446 glyphs cost 0.09 ms more than none |
| `run-2-complete/` | 1280x445 | split | Q-6 run 2, attach run 2, and 532 frames with 5 missed |
| `run-1-partial/` | — | combined | Q-6 run 1, attach run 1, the D3a adapter report |
| `text-path-969/` | 2340x805 | — | that the glyphs drew |

## What was deliberately not kept

**Three emulator bundles.** They verified the apparatus before the device
arrived and the record cites no result from them. What they did establish — that
`-gpu host` is required, and what the emulator's adapters report — is in the
record in prose.

**`run-1-partial/` is partial on purpose.** Its frame captures were confounded:
`user_rotation` resets whenever the capture force-stops back to the
portrait-locked launcher, so that run measured two scenes in landscape and one in
portrait, and `layout` rotated part-way through its own capture. Two of its three
frame captures are dropped as a wrong measurement.

**`frames-layout.log` is kept from it anyway**, and it is the exception that
proves the rule: it is the scene that rotated mid-capture, and it is the evidence
for **#1236** — that `frames.md` records no extent, so nothing in the table shows
a reader that the rows describe different geometries. Grep it for
`attached a WxH surface` to see both.

## One caveat about the timestamps

Two directories are named `20231229T...` in their own `environment.md` because
**the device's clock read December 2023**. Every interval inside them is
device-clock to device-clock and is therefore correct; only the absolute dates
are wrong. They are renamed here to `run-1-partial` and `run-2-complete` so the
directory names do not repeat the error.

## What none of this settles

How much of `submit` is GPU work and how much is waiting on the swapchain.
Wall-clock cannot separate them and nothing here timed the GPU.
`measure/android/perfetto-frames.pbtx` plus Adreno counters — nameable now the
adapter is known — is what would.
