# Decision: a blur radius maps to sigma by Figma's constant, not the CSS one

    status   accepted (2026-07-31). Settles issue #412, open since PR #410
             (story #393 stage B-4) and held at the 2026-07-27 triage
             pending the blur colour space, which was settled 2026-07-30.
    scope    crates/dashscene-skia (the mapping, shared by shadow blur and
             backdrop blur), goldens (three golden images re-recorded, and
             the test that pins the constant)
    binds    the reference painter's output. It does not bind other
             painters the way the blend space does — see "What this does
             and does not bind" below
    related  docs/decisions/blur-blends-in-srgb-encoded-space.md,
             docs/decisions/effects-vocabulary-shadows.md,
             docs/decisions/render-oracle-tolerance-and-gating.md
    supersedes the `sigma = blur / 2` CSS/browser convention adopted at
             story #45 and recorded in
             docs/decisions/effects-vocabulary-shadows.md G-1

## The decision

**`sigma = 0.4375 * radius`**, replacing `radius / 2`. One constant, still
shared by shadow blur and backdrop blur.

`0.4375` is `7/16` — the value is not chosen for that, it is simply where the
measurement lands.

## Why the constant changed

Issue #412 measured Figma's `BACKGROUND_BLUR` as fitting nearer `0.42-0.45 *
radius` and was held for three reasons. Two are now discharged; the third
turned out to point the opposite way from how it was read.

1. ~~The `backdrop-blur` frame cannot decide it.~~ True of the frame's headline
   count, which is the ellipse's rim and identical for every sigma from 4 to
   10. Untrue of the panel region, where the fit has a clear minimum.
2. ~~Colour space is an unresolved confound.~~ Discharged 2026-07-30
   (`docs/decisions/blur-blends-in-srgb-encoded-space.md`). The painter blends
   in the same space Figma does, so the fit measures what it claims.
3. **The constant is shared with shadows.** This was recorded as the reason
   changing it was risky — `blur-falloff` was tuned against the shadow
   fixtures, so a refit would move them. **Measured, the shadow frames do not
   defend `radius / 2`; they argue against it as strongly as the backdrop frame
   does.**

## What was measured

Each frame rendered through its own oracle path and diffed against Figma's
`GET /images` export, sweeping only the mapping constant. Mean per-pixel
max-over-RGBA delta, over the region where each frame's blur actually is:

| frame                         | region        | best fit   | mean at best | mean at 0.5 |
| ----------------------------- | ------------- | ---------- | ------------ | ----------- |
| `v08-drop-shadow` (radius 6)  | outside card  | 0.40–0.475 | **0.1016**   | 0.7264      |
| `v08-inner-shadow` (radius 6) | inside card   | 0.40–0.475 | **0.6937**   | 3.6956      |
| `backdrop-blur` (radius 16)   | frosted panel | 0.4375     | **1.187**    | 2.704       |

All three prefer the same window, and `0.4375` lies inside every one of them.
The shipped `0.5` is 7.1x, 5.3x and 2.3x worse respectively.

**The shadow fixtures cannot pinpoint the value.** At their radius 6, Skia
quantises every constant in `[0.40, 0.475]` onto one box-blur window, so those
frames render identically across it — 7 distinct renders across 13 swept
constants. What they can do, and do cleanly, is exclude `0.5`. The precision
comes from the backdrop fixture at radius 16, where `0.4375` is a distinct
minimum against `0.40` (1.622) and `0.45` (1.930).

## The honest weakness: the tolerance bands barely see this

This change is justified by mean and RMS fit, **not** by the numbers the
project's own gates report. Against Figma, at the bands' own metrics:

| frame                  | before                   | after                    |
| ---------------------- | ------------------------ | ------------------------ |
| `v08-drop-shadow`      | 2/9216 (0.022 %), max 27 | 4/9216 (0.043 %), max 28 |
| `v08-inner-shadow`     | 0/9216 (0.000 %), max 15 | 0/9216 (0.000 %), max 13 |
| `backdrop-blur`        | 10/57600 (0.017 %)       | 10/57600 (0.017 %)       |
| `vector-backdrop-blur` | 19/57600 (0.033 %)       | 19/57600 (0.033 %)       |

**The drop-shadow count gets worse, from 2 pixels to 4.** Those pixels are the
card's own anti-aliased rim, not its shadow; the same frame's mean over the
shadow region improves 7-fold. The two backdrop frames do not move at all,
because their counts are the ellipse rim — the blind spot #412 recorded.

This is stated rather than buried because it is the one argument against the
change. A reader is entitled to ask why a constant was moved when three of four
gate numbers stayed flat and the fourth regressed. The answer is that those
counts threshold at delta 24 and 40 and so cannot see a wide, low-amplitude
falloff difference — which is exactly what
`docs/decisions/render-oracle-tolerance-and-gating.md` says a residual budget
is for, and exactly why #412 had to be settled by a fit rather than by a gate.

**No band or gate was retuned here.** Doing so would make the change
self-justifying.

## How tightly the constant is pinned

`the_backdrop_blur_spreads_at_the_mapped_sigma`
(`goldens/tooling/tests/v011_backdrop_blur.rs`) re-recorded. Measured at two
radii, the constants that render byte-identical pixels:

| authored radius | nominal sigma | rendered sigma | constants that render the same |
| --------------- | ------------- | -------------- | ------------------------------ |
| 12              | 5.25          | 5.1373         | 0.4212 … 0.4654                |
| 24              | 10.5          | 10.1869        | 0.4322 … 0.4543                |

Intersection **0.4322 … 0.4543**, about −1.2 % / +3.8 % around `0.4375`.

That is **wider than the pin this test carried at `0.5`** (0.4988 … 0.5092,
about ±1 %), because the box-blur windows these radii land on are wider
relative to the smaller constant. The new value is therefore held less
precisely than the old one was — a property of Skia's quantisation, not
evidence that it is worse. The test records it as a measured upper bound on
precision rather than letting it read as accuracy.

That test existed specifically so this refit could not land quietly, and it
worked: it failed the moment the constant moved, and re-recording it is a
deliberate step in this change rather than a side effect.

## What this does and does not bind

**Unlike the blend space, this is not a boundary-B contract.** A painter that
maps radius to sigma differently produces a slightly different falloff width,
not a different colour of light; and a painter on hardware that cannot afford a
true Gaussian is expected to approximate anyway
(`docs/decisions/backdrop-blur-is-core-vocabulary.md` reserves dual-Kawase for
constrained hardware). So this records the reference painter's measured value
and the evidence for it, and a second painter should match it where it
reasonably can rather than be held to it byte-for-byte.

The **single mapping** claim is now measured rather than assumed. `blur_sigma`
has always been stated once, on the argument that a shadow blur and a backdrop
blur are the same mapping and two copies could drift apart. Until now that was
an assertion; the shadow frames and the backdrop frame agreeing on the same
window is the evidence for it. Nothing needs to split per effect.

## What is still not settled

**Only two radii were measured**, 6 and 16 (three counting the synthetic 12 and
24 in the sigma test, which measure Skia's quantisation rather than Figma). Two
points cannot reveal a mapping that is non-linear in radius. If Figma's
`BACKGROUND_BLUR` is not linear, this constant is the best linear fit over the
radii the corpus happens to author, not the mapping itself.

Testing that needs a fixture authoring the same construct at several radii,
which needs a Figma Desktop session. It is not scheduled, and this record
should not be read as having ruled it out.

## Alternatives considered

- **Keep `radius / 2`.** Rejected: every frame that measures blur fits worse at
  it, on all three, by 2.3x to 7.1x on mean.
- **Split the mapping per effect** — one constant for shadows, another for
  backdrop blur. Rejected because the measurement removed the reason to: the
  shadow and backdrop frames prefer the same window. Splitting would also give
  up the drift protection `blur_sigma`'s single definition exists for.
- **Retune `blur-falloff` so the improvement is visible at the gate.**
  Rejected. The band was measured against Figma and is not this change's to
  move; adjusting the ruler to show that a change helped is how a measurement
  stops meaning anything.
- **Wait for a multi-radius fixture before moving at all.** Rejected on
  balance: the current value is measurably wrong at every radius in the corpus,
  and holding a known-worse constant for a fixture nobody has scheduled is a
  worse default than adopting the better fit and recording its limits — which
  this section does.
