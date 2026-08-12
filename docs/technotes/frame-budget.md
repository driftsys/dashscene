# Technote — the first frame budget, measured on the showcase host

Informative. **Measured 2026-07-31**, at the v0.14 close (epic #568), which
built the first thing in this repository ever to draw into a window. Nothing
depends on this note. It exists because every performance argument the project
has made until now rested on an offscreen raster compared against a PNG, and
because epic #476 holds twenty items behind the observation that **resolvable is
not the same as measurable**. This is the first measurement on a frame loop.

**It is a measurement, not a threshold.** Nothing asserts these numbers, no test
fails if they move, and no CI job reads them.

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

### The blit was measured afterwards, and it dominates

The offscreen harness measures `tick` plus `paint` and stops there, because
presenting needs a window. **That omission turned out to matter more than the
numbers it left out.** It was closed by instrumenting the host to time `present`
directly, and the result is in "The blit is the largest cost here" below. The
first version of this note led with "`surfaces` does not hold 60 Hz", which was
true and badly misleading: the blit is a larger term than the painter for two of
the three scenes.

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

**`surfaces` does not hold 60 Hz on paint alone.** The budget at 60 Hz is 16.67
ms. Its mean is 16.57 ms and its 95th percentile is 17.31 ms, so `tick` plus
`paint` already sits at the edge. `typography` and `layout` have ample headroom
_in this table_ — but see the blit section above before drawing any conclusion
from that, because none of these three scenes reaches 60 Hz in the host and for
two of them the reason is not here.

**The solver is not the cost.** `tick` is between 0.01 and 0.03 ms in every
scene, against a paint of 0.76 to 16.54 ms. The whole of the per-frame cost is
in the painter. That is worth stating because four of the five debt items v0.14
pulled forward (#191, #205, #225, #226) are on the tick side, and this says
their per-frame contribution is small on this hardware — which is a finding, not
a criticism of fixing them.

## The blit is the largest cost here

Measured on `main` with the host instrumented to time `present`, which is
`paint` plus the `softbuffer` blit. Same machine and extent, means over 240
frames.

| scene      | paint (offscreen) | present | blit | frames per second |
| ---------- | ----------------: | ------: | ---: | ----------------: |
| layout     |              0.51 |     9.9 |  9.4 |              57.2 |
| typography |              6.31 |    15.2 |  8.9 |              59.2 |
| surfaces   |             17.92 |    27.0 |  9.1 |              37.3 |

**The blit costs about 9.1 ms and does not vary with scene content.**

**Amended after issue #603 was fixed:** this note originally attributed the
whole 9.1 ms to the premultiply round trip. That was wrong, and measuring the
fix is what showed it. Removing the round trip is worth about **2.2 ms**. The
per-step split on `layout` — readback 1.847 to 0.191, shuffle 0.816 to 0.337,
`buffer_mut` 0.423 to 0.402, `softbuffer::Buffer::present` 5.699 to 5.646 — puts
**86 % of the remainder inside softbuffer's own post to the window server**,
which is a compositor handoff rather than a conversion and which no pixel-format
change reaches. Issue #641 records that `Surface::new_raster_direct` would reach
0.53 ms of the remaining 6.58 ms.

So the blit is still the largest single term for `layout`, and most of it is not
work this project performs.

For `layout` that is **95 % of the frame**: 0.51 ms of painting inside 9.4 ms of
copying. The repository owner reported that `layout` felt slow before this was
measured, which is what prompted the measurement — the cheapest scene in the
corpus to paint is the one most completely dominated by the blit.

So the answer to "why is this not 60 Hz" is two costs, not one:

- a flat blit tax on every scene, which is waste and is fixable
- `surfaces` genuinely painting slowly, at roughly 6.4 ms fixed plus 5.0 ms per
  megapixel from the extent sweep below — the per-megapixel term being the shape
  a backdrop blur has

`layout` and `typography` are already at the loop's 60 Hz pacing rather than
work-bound, so removing blit cost does not raise their frame rate — only
`surfaces` gained, from 38.1 to 41.2 frames per second. **It does not reach 60
Hz on this painter**, and the remaining terms are the compositor post, the blur
and the gradients rather than anything identified as waste.

**Amended after issue #639 was fixed:** this note predicted that removing the
per-frame image decode would put `surfaces` near 20.4 ms and about 49 frames per
second. Measured, it is **21.7 ms and 45.7 frames per second** — the
painter-lifetime decode cache is worth 2.22 ms of the 3.7 ms the profile
attributed to PNG decoding, not all of it. The remainder is the MSDF glyph
atlases, which hang off the `GlyphRunTable` rather than the `ImageTable` and so
are decoded on every `paint` regardless (issue #644, about 2 ms for the three
atlases every scene loads). `typography` and `layout` did not move, which is the
expected result for scenes with no image fills.

| scene      | present before | present after | frames per second |
| ---------- | -------------: | ------------: | ----------------: |
| surfaces   |          23.96 |         21.74 |      41.4 -> 45.7 |
| typography |          12.68 |         12.63 |      57.1 -> 57.0 |
| layout     |           7.23 |          7.20 |      55.4 -> 55.4 |

Milliseconds, means over 240 frames, two repeats per scene in alternating order,
one-minute load average between 3.1 and 4.3.

**Amended again after issue #644 was fixed.** The atlas decode above was
estimated at "about 2 ms" from the issue's own microbenchmark. Holding the atlas
decodes and the resolve-shader compile on the painter, the way issue #639 holds
the image decodes, measures this on `paint`:

| scene      | paint before | paint after | removed |
| ---------- | -----------: | ----------: | ------: |
| surfaces   |         6.21 |        4.73 |    1.48 |
| typography |         3.91 |        1.55 |    2.36 |
| layout     |         0.28 |        0.29 |    0.00 |

Milliseconds, medians of the per-frame median over 600 frames per scene, two
alternating before/after rounds on the machine named at the top of this note.
Offscreen at 1920x1200 — `paint` only, so the blit and the present are outside
it and the frames-per-second table above was **not** re-measured; that needs an
interactive window run.

Three things the numbers say. The estimate was close for the two text scenes and
the saving is real. `layout` does not move, which is the control: it carries no
glyph runs, so the cache is never built for it. And `typography` loses more than
`surfaces` does — 2.36 against 1.48 for the same three atlases — which is the
noise floor of a 600-frame median rather than a difference in what was removed.

The prediction chain in this note has now been closed twice, and both times the
measured value was smaller than the estimate. Worth remembering before quoting
the next one.

Worth weighing before anyone spends on #603: `dashscene-gpu` (v0.15) has no blit
at all, because it presents to its own surface rather than handing pixels back.

### How paint scales with extent

Paint only, offscreen, means over 200 frames per point.

| scene      | 480x300 | 960x600 | 1358x849 | 1920x1200 |
| ---------- | ------: | ------: | -------: | --------: |
| surfaces   |    5.44 |    8.18 |    12.16 |     17.92 |
| typography |    2.96 |    3.55 |     4.75 |      6.31 |
| layout     |    0.06 |    0.17 |     0.38 |      0.51 |

Milliseconds. Fitting the top two points of each row gives a fixed cost plus a
per-megapixel cost: `surfaces` about 6.4 ms plus 5.0 ms/Mpx, `typography` about
3.2 ms plus 1.4 ms/Mpx, `layout` about 0.25 ms plus 0.11 ms/Mpx. None of the
three is proportional to area alone, so none is purely fill-rate bound.

## The static case: zero, not small

Epic #568 required the static and animated cases be measured separately, and
gave the reason: because no painter has a partial-redraw path, the two differ by
**whether a frame runs at all** rather than by how much of it runs, so a single
averaged number hides which case produced it. A settled scene must show that the
loop stopped painting, not that it painted something cheap.

It stopped. **A settled scene costs zero ticks, zero paints and zero presents.**

From an interactive run of `cargo run --release -p demo -- layout` on `20b1569`,
performed by the repository owner:

    settled at generation 1224 after 1224 ticks and 1222 presents — waiting for an event
    forced redraw — a key press
    woken by pulse 8 after 1.01 s parked — 1 ticks and 1 presents ran while parked
    settled at generation 1317 after 1315 ticks and 1312 presents

Seven settles across the run. Every park reported
`0 ticks and 0 presents ran
while parked` except one, and that one is the frame
the owner's key press forced — which is the forced-redraw path working, not the
skip failing. A longer `--all` run recorded 90 park and wake cycles with 87
reporting `0 ticks and 0
presents`, the other three each preceded by a logged
forced redraw after occlusion.

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

## Extended at v0.15 (story #586): the lean painter, and what happened to the blit

Same machine, same three scenes, the same 1920x1200 physical extent and the same
240-present samples — so this table is read against the ones above rather than
beside them. Both painters measured on `dashscene-gpu` at the close of the
backdrop-blur story, when the lean painter drew the whole v0 paint vocabulary
for the first time. Sample counts differ per cell — a first sweep took three per
scene, and the colour-management investigation below added 32 more for
`surfaces` alone — so each cell states its own rather than one figure standing
for all six.

| scene      | rects | skia present |      gpu present |
| ---------- | ----: | -----------: | ---------------: |
| surfaces   |    33 | 19.90 (±0.2) |  1.4 (0.30–2.48) |
| typography |    16 | 10.68 (±0.4) | 0.74 (0.64–2.32) |
| layout     |    30 |  8.75 (±0.1) | 0.71 (0.67–0.75) |

Milliseconds. Each figure is the **median of that cell's 240-present sample
means**, with the observed range beside it: n = 8 for `skia surfaces` and
`skia typography`, 4 for `skia layout`, 41 for `gpu surfaces`, 8 for
`gpu typography` and 5 for `gpu layout`. `tick` stayed between 0.05 and 0.30 ms
throughout and is not the cost in either painter, which is the finding above
unchanged.

**Read the two columns with different precision.** The reference painter's
samples agree to about 1 %, so its figures carry two significant digits
honestly. The lean painter's do not: `surfaces` produced sample means from 0.30
to 2.48 ms across 41 samples. A swapchain present returns once the frame is
queued, so what it costs depends on how full the queue already is — which is a
property of the moment, not of the scene. **No gpu figure here should be read to
better than "about a millisecond".**

That is enough for the claim being made. Even taking the lean painter's worst
observed sample against the reference painter's best, `surfaces` is 2.48 against
19.73, and the other two scenes never overlap at all.

**The flat blit tax is gone, which is what story #585 was for.** This note's
central v0.14 finding was a blit of about 9.1 ms on every scene regardless of
content, most of it inside softbuffer's post to the window server. The lean
painter has no blit: it draws into a swapchain texture it acquired from the
window. Nothing in the gpu column resembles a flat 9 ms term.

**`surfaces` no longer exceeds the 60 Hz budget on the CPU.** It was the one
scene this note recorded as genuinely work-bound — 19.94 ms here against a 16.67
ms budget, and it is the densest scene in the corpus. Through the lean painter
its CPU cost is about 1.4 ms. The last item under "What this does not settle"
asked whether that was a painter problem or a scene problem; it was a painter
problem, and a second painter answered it.

### The gpu column is CPU time, and is not a frame rate

**A swapchain present returns once the frame is queued, not once it is drawn.**
So the gpu column measures what the CPU spends handing the frame over, and it
excludes the GPU's own execution entirely. The Skia column has no such gap — a
CPU raster plus a CPU blit is all CPU — which is exactly why the ratio column is
labelled a ratio and not a speed-up.

What the gpu column does support is the claim the ratio is being used for: the
**CPU** is no longer the frame's limiting term. It does not support "69x
faster", and no frames-per-second figure should be taken from it. An offscreen
render of `surfaces` at 960x600 through the same painter, which does serialise
on a readback, measures 3.66 ms — and `layout`, with 27 instances and no
backdrop, measures 3.4 ms through that same path, so about 3 ms of it is the
readback rather than the drawing. Neither number is a GPU time; measuring one
needs timestamp queries and nothing here has them.

**`surfaces` is both the most expensive and by far the most variable, and the
backdrop blur is the likely reason — inferred, not proven.** It is the only
scene with one, and the blur is the only construct in the vocabulary whose cost
is not a function of the instance count: story #733 gives it a full-target copy
and two extra passes per backdrop instance, and how long those take depends on
GPU scheduling rather than on how many rects the frame holds. That matches the
shape observed — `layout`, with no blur, is the tightest column here (0.67 to
0.75 across every sample) while `surfaces` spans an order of magnitude. It is
the same shape the v0.14 note attributed a per-megapixel term to on the CPU
painter, for the same scene and the same suspected reason.

No per-feature profile was taken, so this stays an inference. Confirming it
needs either a `surfaces` variant with the blur removed or GPU timestamp
queries, and neither exists.

### How this was measured, and why that is now reproducible

The v0.14 blit number came from instrumenting the host by hand, which is the
reason this section exists at all: that instrument was not kept, and the
question "what happened to the blit" could not be answered by re-running
anything.

The instrument is now in the host, behind `DASHSCENE_FRAME_TIMING`, off by
default. It reports per 240 presents, and it excludes two things that silently
corrupted the first sweep taken with it:

- **frames the presenter declined.** A zero-area drawable, an occluded surface
  and a timed-out acquire all returned `Ok(())` and were indistinguishable from
  a frame that drew. They are near-free, so a window moved off-screen pulled the
  mean down by two orders of magnitude. `dashscene_gpu::Drawn` makes the
  distinction observable and the instrument now counts only frames that reached
  the window.
- **samples that span a scene change or a painter swap.** The host advances
  scenes on a dwell timer and swaps painters on a key. A sample crossing either
  boundary describes neither side, so it is discarded with a printed warning.

Both were found the same way: the first sweep produced numbers that disagreed
with the reference painter's by a factor of a thousand, and the owner mentioned
having swapped painters and moved the window during it. Every sweep behind the
table above reports zero discarded samples.

### One attribution this instrument nearly got wrong

A sweep taken before story #750 merged put `surfaces` at 0.27 ms and one taken
after put it at 1.43 ms, on an otherwise identical tree. #750 changes the
swapchain's colour space, so the per-frame cost of a colour-managed present is
exactly the kind of thing that difference would be evidence for.

It was not. An alternating A/B — the same binary and instrument built at both
commits, four runs in `pre, post, pre, post` order — put the two arms on top of
each other: pre-#750 sample means from 0.56 to 2.48 ms, post-#750 from 0.30 to
1.74, with medians of 1.05 and 0.86. The apparent regression was this column's
own variance, sampled twice.

Recorded because the wrong conclusion was one paragraph away from being written
down, and because it is the reason the ranges above are published rather than
the means alone. **A before-and-after on a noisy measurement is not evidence
until it is alternated.**

## What this does not settle

- Why `surfaces` costs 5.0 ms per megapixel is inferred, not proven. The shape
  matches a backdrop blur and it is the only scene with one, but no per-feature
  profile was taken.
- No target-hardware number exists, so epic #476's entry condition is unchanged.
  The v0.15 numbers do not change this: an M3 through Metal is further from the
  target SoC than the CPU raster was, not closer.
- The other four items story #570 pulled forward have no controlled
  before-and-after on a scene, only the count assertions and microbenchmarks in
  their own pull requests. This note does not supply one.
- **No GPU-side time is measured anywhere in this note.** Every figure in the
  v0.15 table is CPU. Whether the lean painter's GPU work fits a frame is
  unmeasured, and on this hardware it is not the term under pressure.
- Why the lean painter's cost does not track rect count at all is unexplained:
  `typography` has 16 rects and `layout` 30, and they cost the same 0.7 ms,
  while `surfaces` at 33 costs twice that with far more variance.
