# Goldens compare decoded pixels in unpremultiplied RGBA8888

    status   accepted (story #6, 2026-07-12); resolves debt #86.
             Extended by story #14 (a differing-pixel tolerance for
             anti-aliased content — see "Cross-machine anti-aliasing"
             below), story #11 (the exact-match constraint on v0.2
             flex goldens — see "Flex goldens are exact-match by
             construction" below), story #35 (an absolute-pixel
             budget for sparse text goldens — see "Text goldens use an
             absolute-pixel budget" below), issue #233 (a text
             budget is calibrated against the scene's smallest
             regression, not its total erase — see "A text budget is
             calibrated against a partial regression" below), issue
             #533 (regenerating a committed atlas fixture restates every
             golden that samples it — see "Regenerating an atlas fixture
             restates every golden that samples it" below), and issue
             #532 (the same calibration applied to the v0.6 Arabic
             golden — see "The v0.6 Arabic budget takes the same
             calibration" below), and issue #539 (the cross-architecture
             text residual, measured 2026-08-01 at 0 to 4 px against
             budgets of 200 to 1,200 px — the extrapolation it replaces
             was wrong in the safe direction by two to three orders of
             magnitude; see "Cross-architecture text-golden
             measurement" below).
    scope    goldens/ tooling; binds golden authoring for every painter

## Context

Story #4 left the golden comparison space open (debt #86): the
reference painter's surface is N32 premultiplied, and its readback
(`SkiaPainter::rgba_bytes`) converts to unpremultiplied RGBA8888 —
a conversion that shifts semi-transparent channels by up to one code
point against direct quantization of the authored color. The harness
needed one defined space and one defined failure criterion.

## Options

1. Compare decoded pixels in unpremultiplied RGBA8888; encoded-byte
   drift with identical pixels passes with a note.
2. Compare encoded PNG bytes.
3. Compare premultiplied pixels (the surface's native space).

## Choice

Option 1.

## Why

- Encoded bytes (option 2) fail on encoder changes that alter zero
  pixels — a skia version bump would break every golden with no
  rendering change, and the report could not tell encoding drift from
  pixel drift. A golden is a picture, not a container format.
- Premultiplied comparison (option 3) would compare the surface's
  internal representation; PNG itself is unpremultiplied, so the
  checked-in artifact and the comparison space would disagree, and
  every inspection tool shows unpremultiplied values.
- Unpremultiplied comparison is still bit-exact for a pinned skia
  version: opaque colors round-trip exactly, and a semi-transparent
  fill's premul quantization is deterministic — the quantized value IS
  the expected value, baked into the golden. Documented in
  `goldens/README.md`.
- v0.1 fixtures stay opaque and integer-aligned; the sub-pixel
  geometry policy remains open as debt #85 (GPU-painter perceptual
  diffs, when they come, revisit the space — unpremultiplied
  comparison amplifies channel error at low alpha, noted in story #4's
  review).

## Cross-machine anti-aliasing (story #14 extension)

Story #14 turned anti-aliasing on for the reference painter
(`reference-painter-antialiasing.md`, resolving debt #85). AA is
deterministic for a pinned skia version on one machine, but skia's CPU
coverage rounding at a fractional edge is **not bit-identical across CPU
architectures**: the v0.3 paint golden, generated on one architecture,
differed by 32 of 9216 pixels (0.35%) on the CI runner's architecture,
all at gradient and curve edges.

Bit-exact comparison therefore holds only for content that renders
identically across machines — integer-aligned, un-antialiased geometry
(solid fills). For anti-aliased content the harness offers a
differing-pixel **tolerance**: `assert_matches_golden_within(name, png,
max_fraction)` fails only when more than `max_fraction` of pixels
differ. This is `docs/technotes/rendering-and-painters.md`'s
"tolerance-based perceptual diff", which the design placed at GPU painters, brought forward to CPU-raster
AA because the same cross-architecture coverage jitter applies.

- `assert_matches_golden` stays exact (fraction 0) — v0.1's
  integer-aligned solid golden uses it and passes bit-exact everywhere.
- The v0.3 paint golden uses a 1% tolerance (~3× the observed 0.35%
  jitter, and far below any real rendering change — the smallest scene
  element covers several percent of the canvas, so a regression moves
  far more than a thin edge).
- Interior correctness is not left to the tolerance: the painter's
  per-kind unit tests assert exact bytes at interior probe pixels away
  from AA edges, and those are bit-stable across machines (they pass in
  CI). The golden is the coarse full-frame visual-regression check.

## Flex goldens are exact-match by construction (story #11 extension)

Story #11 goldens the v0.2 flex vocabulary (nesting, sizing, clamping,
alignment) with four scenes, each dimensioned so every solved rect
lands on an integer. Integer-aligned solid fills carry no
anti-aliased edges, so all four goldens use `assert_matches_golden` —
the exact-match form, no tolerance budget — the same guarantee the
v0.1 golden already relies on, now proven again against
`dashscene-engine`'s `TaffySolver` output rather than the fixed
solver.

This is a constraint that binds every future flex golden, not an
incidental property of these four scenes: if a construct cannot be
made integral, the scene's dimensions are what change, not the
comparison function. `assert_matches_golden_within` only enters for a
construct that is genuinely impossible to make integral, with the
reason recorded at the call site — no v0.2 flex golden needed it.

## Text goldens use an absolute-pixel budget (story #35 extension)

The `assert_matches_golden_within` tolerance is a fraction of the whole
canvas. That is the right model when the inked content is a large share
of the canvas — a v0.3 paint family, where a real regression moves
several percent of the canvas and the cross-machine edge jitter is a
small fraction of it. It is the wrong model for sparse content.

Text is sparse. The v0.6 Arabic screen (story #35) inks about 2,820 of
its 71,400 canvas pixels — 3.95 %. A canvas fraction wide enough to
absorb the anti-aliasing jitter (the v0.5 Latin text golden used 5 %,
3,570 px) is then wider than the entire inked footprint, so a paint
regression that draws no text at all — a 2,818-px difference — passes the
compare. Every shaping regression E2 names fits under it too. Measured
empirically on the story #35 review.

The cause is a model mismatch: cross-machine coverage jitter scales with
the scene's anti-aliased **edge count**, an absolute number, not with the
canvas area. So text goldens use `assert_matches_golden_max_pixels(name,
png, max_pixels)` — an absolute differing-pixel budget — sized to the
edge count rather than the canvas:

- The budget is set a few times the scene's anti-aliased edge
  population, so it clears cross-machine jitter, while staying well below
  the scene's inked footprint, so a text-erasing regression exceeds it.
  Story #35 set the v0.6 Arabic golden's budget to 1,000 px on that
  reasoning: ~2.5x the ~400-px edge population (measured as the pixels
  that shift by one code point on a premultiply round-trip), and well
  under the 2,818-px text-erase and 4,633-px form-isolation breaks it must
  catch (both demonstrated failing in the story #35 review). Issue #532
  recalibrated it to 440 px against the smaller break the section below
  defines; the model is unchanged, only the number.
- The pixel golden stays a coarse full-frame check. A text golden pairs
  it with a glyph-id-level guard test (shaped output compared to expected
  forms, machine-independent and exact), which pins the shaping features
  the coarse budget cannot resolve — the same division of labor the v0.3
  goldens use between the tolerance golden and the painter's
  interior-probe unit tests.

`Budget` (the `Fraction`/`Pixels` choice) is the one comparison model;
`assert_matches_golden` (exact), `assert_matches_golden_within`
(fraction), and `assert_matches_golden_max_pixels` (absolute) select it.

## A text budget is calibrated against a partial regression (issue #233)

Story #35 left the v0.5 Latin text golden on its 5 % fraction, on the
argument that at 8.95 % ink it is above its own budget, so a text-erasing
regression already fails it. That argument covers only the total erase.
Issue #233 named the case it does not cover: the scene has two strings,
and either one vanishing on its own is a smaller difference than the
budget. Measured on the scene as it stands, the heading alone is 1,822 px
and the chip's string 1,941 px, against the 2,240-px fraction — so both
passed.

So the calibration standard for a text budget is the **smallest
regression the scene can express**, not the total erase. Where a scene
has one text run those are the same number; where it has several they are
not, and the total erase is the weakest of them. The v0.5 golden moves to
`assert_matches_golden_max_pixels` at 1,200 px — two thirds of its
smallest measured break (1,214), rounded down to the nearest hundred — and
commits the measurement as a
test (`dropping_either_string_exceeds_the_budget`), the same shape
`v07-text-fallback` already uses. A budget stated in a comment is an
estimate; a budget with a committed break is a gate.

Three facts about the numbers, recorded so a later reader does not treat
them as fixed:

- **They drift.** The v0.5 ink measured 4,008 px at the story #35 review
  and 3,763 px on this branch; the v0.6 ink measured 2,820 px then and
  2,421 px now. Text rendering changed underneath both (the #314
  line-height fix, the #272 baseline correction). A recorded ink figure is
  a measurement with a date, so re-measure before reusing one.
- **The committed v0.5 image itself had drifted.** A fresh render of the
  scene differed from `v05-text-latin.png` by 3 of 44,800 px, where the
  v0.6 golden renders bit-exact. A 2,240-px budget had ample room to hide
  that, and so did the 1,200-px one, which is why the drift was recorded
  rather than folded into an unrelated recalibration. Issue #533 then
  found its cause
  and re-recorded the image; the section below carries the result, and the
  1,822/1,941 figures above are the post-re-record measurement (they read
  1,823/1,943 against the stale image).
- **The v0.6 budget failed to catch the same case at its own scale.** Its
  1,000 px caught the total erase (2,421 px) but not one of its three
  strings vanishing: the banner is 934 px, the harakat word 671 px, the
  speed chip 816 px, each measured against the committed golden. Retuning
  it was out of scope for #233 — it is the live E7 frame, and the number
  was CI-proven at its current value — so it was filed as #532 and closed
  separately, below.

The v0.6 golden's own `~400-px edge population` figure could not be
reproduced by this work: the stated instrument (pixels that shift by one
code point on a premultiply round-trip) is the identity on an opaque
canvas, and these scenes are opaque. The v0.5 calibration therefore does
not use it. It bounds the budget from below by the only cross-machine
difference this project has actually measured — 32 px on the v0.3 paint
golden — and from above by the scene's smallest break.

## Regenerating an atlas fixture restates every golden that samples it (issue #533)

The 3-px drift recorded above was neither non-determinism nor an unknown:
the render is deterministic, and the golden was stale. Both halves were
measured rather than argued.

- **Deterministic.** Five renders in one process and three separate
  processes produce byte-identical output, and the three differing pixels
  are each ±1 in a single channel at an anti-aliased glyph edge.
- **Stale, with a named cause.** `git bisect` over `9412e7a..main`, using
  "does re-recording reproduce the committed bytes" as the test, returns
  `48b721b` — _feat(dashscene-typeset): compute GSUB charset closure for
  Arabic_. That commit regenerated the committed ASCII atlas fixture
  (`atlas.png` 56,335 to 58,002 bytes, `atlas.metrics` 3,715 to 3,829),
  and the v0.5 Latin golden samples that atlas. Confirmed directly at
  `main`: rendering today's scene against the pre-`48b721b` atlas bytes
  reproduces the committed golden exactly, 0 px. Every other change in the
  render path since is pixel-neutral for this scene.

So the image was re-recorded, and the scene is bit-exact against it again.

The general rule this makes explicit: **a committed atlas fixture is an
input to every golden that loads it, so regenerating one is a change to
those goldens.** Regenerating an atlas and leaving its goldens alone does
not fail anything — a tolerance budget absorbs the difference silently,
and the images go stale without a signal. `corpus/atlas/README.md` states
the obligation at the point of regeneration, and lists each atlas's
consumers as the set to re-record.

That list has to be complete to be worth anything, and it was not: it
named two of the seven files that load `corpus/atlas/ascii`, so a
regeneration that followed it literally would have repeated this defect on
the five it omitted. Completed there, with the command that regenerates it
(`grep -rl 'corpus/atlas/ascii"' goldens/`) recorded beside it, because a
hand-maintained list of consumers drifts the same way a hand-maintained
budget does. Committed images are also not the whole obligation: ink-pixel
counts quoted in a budget's rationale, and the oracle's per-frame
residuals, are measured against the atlas bytes and go stale with no test
failing.

An Arabic-closure commit moved a Latin golden because both scripts shared
one `ascii/` fixture directory. The project has since adopted one
directory per (script, weight) precisely so an added weight never rewrites
an existing fixture — that convention prevents the recurrence this issue
found, and predates it.

## The v0.6 Arabic budget takes the same calibration (issue #532)

Issue #532 applied the standard above to the golden the #233 measurement
found sharing the defect. The v0.6 Arabic screen has three text runs, so
its smallest expressible regression is one run vanishing, not the total
erase its 1,000-px budget was sized against. Re-measured on the scene as
it stands — the same figures #532 records, reproduced before retuning:

| quantity                               | pixels                       |
| -------------------------------------- | ---------------------------- |
| canvas                                 | 71,400 (340x210)             |
| healthy render vs the committed golden | 0                            |
| whole text erased                      | 2,421 (3.39 %)               |
| the banner vanishes                    | 934                          |
| the harakat word vanishes              | **671** — the smallest break |
| the speed chip vanishes                | 816                          |

The budget moves to **440 px**, two thirds of 671 rounded down, and the
measurement is committed as `dropping_any_string_exceeds_the_budget` — the
same shape the v0.5 golden and `v07-text-fallback` already use.

Two notes on the number:

- **The rounding differs from v0.5's on purpose.** The v0.5 calibration
  rounded its two thirds down to the nearest hundred (1,215 to 1,200).
  Here that lands on 400, which collides numerically with the discredited
  `~400-px edge population` figure above, so this rounds to the nearest
  ten instead. The provenance of a budget should be readable from the
  number.
- **It is tighter per inked pixel than either budget calibrated the same
  way**, at 0.18 px of tolerance against v0.5's 0.32 and
  `v07-text-fallback`'s 0.34. This scene renders bit-exact against its
  golden on the machine it is recorded on (0 px of 71,400), and 440 px was
  set at about 13.75x a 32-px anchor taken on the v0.3 paint golden — a
  gradient-and-stroke scene on Skia's own rasteriser rather than the MSDF
  path — against 31.25x for the ratio it replaces. That multiplier was an
  extrapolation; #539 has since measured the real figure, and this golden
  differs by **4 px of 71,400** across architectures, so 440 px clears the
  cross-machine floor by about 110x. Raising the budget instead would spend the
  drift margin the two-thirds rule provides: anything above about 600 px
  is within 71 px of the 671-px break.

This closes the last of the text goldens sized against a total erase.
Story #219 had already recalibrated `v07-text-fallback` the same way, and
that budget is unaffected: it is gated on its own 714-px break, not on the
v0.6 number. Its stated comparison to "the CI-proven v0.6 Arabic golden"
quotes the 1,000-px value and is left unedited as the historical note it
now is.

## Cross-architecture text-golden measurement (issue #539, measured 2026-08-01)

Every text budget above is set against one number: **32 px**, measured
once on `v03-paint.png` (story #14, "Cross-machine anti-aliasing"
above) — a gradient-and-stroke scene drawn by Skia's own path
rasteriser. The MSDF resolve is a different rendering path, so each text
budget's multiplier against the 32-px anchor was an extrapolation from a
path other than the one it guards.

It is no longer an extrapolation. The goldens are recorded on macOS
arm64 and CI runs them on x86_64, so **every green run was already a
cross-architecture diff** — and `compare_against` already prints its
residual on a within-budget pass. The `test` job runs under nextest,
which captures a passing test's output, so the number was measured and
discarded on every run. Re-running them with `--nocapture` in the
`render-oracle` job records it:

| golden                  | budget (px)   | ratio to the 32-px vector-AA anchor | cross-architecture residual | rate    | headroom |
| ----------------------- | ------------- | ----------------------------------- | --------------------------- | ------- | -------- |
| **v03-paint** (anchor)  | 1 % tolerance | —                                   | **32 px of 9,216**          | 0.347 % | 2.9x     |
| v05-text-latin          | 1,200         | 37.5x                               | 1 px of 44,800              | 0.002 % | 1200x    |
| v06-text-arabic         | 440           | 13.75x                              | 4 px of 71,400              | 0.006 % | 110x     |
| v07-text-fallback       | 500           | 15.6x                               | 2 px of 34,560              | 0.006 % | 250x     |
| v07-text-lowering       | 200           | 6.25x                               | 0 px — byte-identical       | 0 %     | infinite |
| v07-variant-topology    | 200           | 6.25x                               | 0 px — byte-identical       | 0 %     | infinite |
| v013-baseline-hug-cross | 400           | 12.5x                               | 3 px of 36,000              | 0.008 % | 133x     |
| v07-ellipse             | 500           | 15.6x                               | 0 px — byte-identical       | 0 %     | infinite |

The anchor scene is measured in the same run, on the same toolchain, so
this is a comparison across architectures rather than across time. It
lands on **exactly 32 px of 9,216**, reproducing the historical figure —
the anchor was accurate; only its application to a different rendering
path was not.

Issue #539 asked whether MSDF edge coverage is more or less stable
across architectures than the gradient AA the 32-px figure measured.
**Far more stable, and the gap is large.** The anchor scene moves 32 px
on a 9,216-px canvas; the text frames move 0 to 4 px on canvases three
to eight times larger. Per pixel that is **58x to 170x** less movement,
and three of the seven calibrated-budget goldens do not move at all.

`v07-ellipse` is the useful control: it is curved anti-aliased shape
edges rather than text, so it sits between the two paths — and it is
byte-identical. The instability the 32-px anchor captured is specific to
what `v03-paint` draws, gradients and strokes through Skia's own path
rasteriser, not a general property of anti-aliased edges.

The earlier `--release` data point below reads differently in hindsight:
it was treated as a much weaker perturbation than a change of
architecture, and it turns out that a change of architecture is itself a
weak perturbation for this path.

### What the headroom does and does not mean

A text budget has two constraints, and only one of them is now measured.
It must sit **above** the cross-architecture noise floor — which the
table shows it clears by 110x or more — and **below** the smallest
regression it must catch, which each golden pins with its own
sensitivity guard (issue #233; for example
`dropping_either_string_exceeds_the_budget`). Those guards prove a whole
string vanishing exceeds the budget. They do not prove anything about a
smaller regression.

That gap is now quantified rather than suspected: with the noise floor at
1 to 4 px and budgets at 200 to 1,200 px, a regression that moves a few
glyphs by a pixel — a baseline nudge, a kerning change, a single
glyph substituted — passes silently on every one of these frames.
Tightening the budgets toward the measured floor is the obvious
consequence, and it is deliberately **not** done here: each budget's
sensitivity guard is calibrated against its current value, so retuning
them is a change to the gate, and belongs in its own change with its own
measurement, not folded into the measurement that revealed it.

One data point narrows the question without answering it, recorded
during PR #536's review rather than left there: re-recording
`v05-text-latin`, `v06-text-arabic`, and `v07-text-fallback` from a
`--release` build — which reorders and vectorises the same float
arithmetic — rewrote nothing; all three stayed byte-identical to the
committed, debug-generated images. Optimisation-level codegen on one
target is a much weaker perturbation than a different CPU architecture
and a different libc, so this does not close the measurement gap above.
It says only that the MSDF resolve is not sitting on a float-rounding
knife-edge on this target — so if a text golden ever fails on Linux at
its current budget, the more likely cause is Skia's coverage
rasterisation, not the MSDF distance maths.

**What would turn the "not measured" column above into a number**, and
the condition for reopening this question once #263 resolves:

1. run the golden suite on the Linux runner and record the actual
   differing-pixel count for every row in the table above;
2. replace "not measured" with the recorded count in this table;
3. re-derive each text budget's margin against the measured number
   instead of the extrapolated one, and retune any budget the
   measurement contradicts.

Until then the failure mode stays the one #539 named as acceptable in
the meantime: if a budget above is too tight for real macOS-to-Linux
MSDF jitter, its golden fails without a regression behind it —
diagnosable and fixable by re-measuring. A budget wide enough to hide a
real regression is the opposite failure, and the one #233 and #532 both
exist to prevent.
