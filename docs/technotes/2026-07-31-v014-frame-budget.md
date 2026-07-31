# The first frame budget, measured on the v0.14 showcase host

Informative. Recorded at the v0.14 close (epic #568), which built the first
thing in this repository ever to draw into a window. Nothing depends on this
note. It exists because every performance argument the project has made until
now rested on an offscreen raster compared against a PNG, and because epic #476
holds twenty items behind the observation that **resolvable is not the same as
measurable**. This is the first measurement on a frame loop.

**It is a measurement, not a threshold.** Nothing asserts these numbers, no
test fails if they move, and no CI job reads them.

## What was measured, and on what

|         |                                                                               |
| ------- | ----------------------------------------------------------------------------- |
| machine | Apple M3, 8 cores, macOS 15.6.1 (Darwin 24.6.0), arm64                        |
| painter | `dashscene-skia`, CPU raster, blitted to the window through `softbuffer`      |
| extent  | 1920x1200 physical pixels (960x600 logical at scale factor 2)                 |
| build   | release: `lto = true`, `codegen-units = 1`                                    |
| sample  | 600 frames per scene, `LiveScene::tick` and `Painter::paint` timed separately |
| commit  | `20b1569`                                                                     |

The scenes are the three in `corpus/showcase/`, driven the way the host drives
them: a scripted pulse on the host's own cadence, so every frame measured is one
in which something is actually moving.

### What these numbers do not include

**The blit.** The harness runs offscreen, so it measures `tick` plus `paint` and
stops there. Presenting needs a window and cannot be measured this way. That
omission is not negligible: issue #603 records that the Skia-to-window blit
allocates a window-sized buffer per frame and unpremultiplies then re-premultiplies
every pixel. The real per-frame cost is these numbers plus that.

**Target hardware.** This is a desktop CPU raster on an M3. It is emphatically
**not** the target-SoC budget epic #476 waits for, and it does not release any
item held there. What it does is give the per-frame path a number where it
previously had none.

## The animated case

| scene      | rects | tick mean | paint mean | frame mean | frame p95 | frame max |
| ---------- | ----: | --------: | ---------: | ---------: | --------: | --------: |
| surfaces   |    31 |      0.02 |      16.54 |      16.57 |     17.31 |     25.49 |
| typography |    14 |      0.03 |       5.95 |       5.98 |      6.20 |      6.91 |
| layout     |    28 |      0.01 |       0.76 |       0.77 |      0.83 |      0.91 |

All figures in milliseconds.

**`surfaces` does not hold 60 Hz.** The budget at 60 Hz is 16.67 ms. Its mean is
16.57 ms and its 95th percentile is 17.31 ms, so `tick` plus `paint` alone
already sits at the edge and exceeds it under load — and the blit is on top of
that and is not counted here. `typography` and `layout` have ample headroom.

**The solver is not the cost.** `tick` is between 0.01 and 0.03 ms in every
scene, against a paint of 0.76 to 16.54 ms. The whole of the per-frame cost is
in the painter. That is worth stating because four of the five debt items
v0.14 pulled forward (#191, #205, #225, #226) are on the tick side, and this
says their per-frame contribution is small on this hardware — which is a
finding, not a criticism of fixing them.

## The static case: zero, not small

Epic #568 required the static and animated cases be measured separately, and
gave the reason: because no painter has a partial-redraw path, the two differ
by **whether a frame runs at all** rather than by how much of it runs, so a
single averaged number hides which case produced it. A settled scene must show
that the loop stopped painting, not that it painted something cheap.

It stopped. **A settled scene costs zero ticks, zero paints and zero presents.**

From an interactive run of `cargo run --release -p demo -- layout` on `20b1569`,
performed by the repository owner:

    settled at generation 1224 after 1224 ticks and 1222 presents — waiting for an event
    forced redraw — a key press
    woken by pulse 8 after 1.01 s parked — 1 ticks and 1 presents ran while parked
    settled at generation 1317 after 1315 ticks and 1312 presents

Seven settles across the run. Every park reported `0 ticks and 0 presents ran
while parked` except one, and that one is the frame the owner's key press forced
— which is the forced-redraw path working, not the skip failing. A longer
`--all` run recorded 90 park and wake cycles with 87 reporting `0 ticks and 0
presents`, the other three each preceded by a logged forced redraw after
occlusion.

So the static budget is not a small number to compare against the animated one.
There is no frame.

## What issue #101 was worth, measured

Story #570 pulled five per-frame debt items into v0.14 on the argument that
**v0.14 is their measurement**. This is that measurement for issue #101, the
image-decode cache, and it is the only one of the five with a controlled
before-and-after on a real scene.

Both arms use the same harness, the same machine and the same extent, run back
to back. The "before" arm is commit `e97b026` — the showcase scenes on the frame
loop, before the decode cache merged — confirmed by inspection to contain no
`image_cache`.

| scene      | without #101 | with #101 | ratio |
| ---------- | -----------: | --------: | ----: |
| surfaces   |        23.04 |     16.57 |  1.39 |
| typography |         6.05 |      5.98 |  1.01 |
| layout     |         0.83 |      0.77 |  1.08 |

Frame mean in milliseconds.

`surfaces` is the only scene with image fills — four scale modes sharing one
payload — and it is the only one that moves. `typography` and `layout` are flat,
which is the expected result and is recorded because a fix that moved everything
would have meant the measurement was wrong.

The "after" arm ran under a **higher** load average than the "before" arm (6.34
against 3.58) and was still faster, so if the load skews this comparison it
skews it against the result.

## What this does not settle

- The blit is unmeasured (#603), so no total per-frame figure exists yet.
- No target-hardware number exists, so epic #476's entry condition is unchanged.
- The other four items story #570 pulled forward have no controlled
  before-and-after on a scene, only the count assertions and microbenchmarks in
  their own pull requests. This note does not supply one.
- `surfaces` exceeding the 60 Hz budget has not been investigated. The paint is
  the cost and the scene is deliberately the densest in the corpus; whether that
  is a painter problem or a scene problem is not established here.
