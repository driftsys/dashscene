# A step is a pair of keyframes, not a fourth transition spec

    status   accepted
    date     2026-08-09
    scope    `dashcue`'s `Keyframe` invariant and `validate_spec`; the
             `TransitionSpec` union `dashbuf` serializes at story #771
    issue    #852, raised by the SVG producer capture before #771 pins the
             schema
    refs     #771, #255, #143, #853 (the SVG capture this was raised from,
             merged 2026-08-09), `docs/design/dashcue.md`

`TransitionSpec` is `Tween`, `Spring` and `Keyframes`, and all three are
continuous. A **timed discrete change** — "flip at 0.4 of the duration" — had no
representation, so `calcMode="discrete"` and the timed form of SVG's `<set>`
had nowhere to land. This rules on where it goes, before story #771 turns the
union into schema rows.

It is not hypothetical, on figures **this repository cannot yet check**.
Issue #852 reports a census of the 525-test W3C SVG 1.1 suite in which
`<set>` appears in 35 files and `visibility` (46 uses) and `display` (28) are
among the most animated non-geometry attributes. That census lives in
`docs/wip/2026-08-09-svg-as-a-second-producer.md`, which was on a different
branch when this record was written and landed with pull request #853 the same
day. **It is re-derivable**: the capture ends with the commands that produce
every figure in it, from the suite's own stable archive URL, so the numbers can
be checked rather than taken. Cited as the capture's measurement rather than
restated as this record's own.

The direction does not rest on the exact counts: the construct is standard SVG,
and it has not surfaced from Figma only because Figma reaches the same behaviour
through variants.

## Decision

**Two keyframes may share a `t`, and that pair is a step.** `Keyframe.t` becomes
non-decreasing rather than strictly increasing. A step at 0.4 is
`[(0.4, 0.0), (0.4, 1.0)]`; a two-step sequence is four frames.

**At most two may share a `t`.** Sampling walks to the last frame at a given
`t`, so a third carries a value no sample can ever return. That is authored data
disappearing without a diagnostic, which P4 forbids, so it is a named producer
error.

**The open interval is unchanged.** A frame still sits strictly inside (0, 1),
because the endpoints (0, 0) and (1, 1) are implicit and a frame at either would
restate one of them.

## Why, against the alternatives

**The sampler already did this.** `keyframes_progress` walks the frame list and
interpolates only across a segment it entered, so a duplicate `t` yields an
exact step with no division by zero — verified before the decision was taken,
for a single step and for a two-step sequence. **The sampler is unchanged**, and
the whole cost falls in `validate_spec`: the ordering test relaxes from `>` to
`>=`, and the at-most-two rule below adds one flag and one assertion beside it.
Issue #852 put the cost of this option as "it changes a documented invariant that
the scheduler and any consumer rely on"; the scheduler does not rely on it.

**A fourth union variant** — `Steps { duration, count }` or
`Discrete { duration }` — was the alternative. Rejected because it duplicates a
curve `Keyframes` can already describe, while costing a union arm, a `dashbuf`
table, a loader arm and a validator arm at story #771. An append is cheap under
R7; an append that says nothing new is still a second way to write the same
thing, and two representations of one curve is what a consumer has to be told
about.

**Ruling it out of the vocabulary** was the third option, and is what P4 would
require if the construct were unrepresentable. It is not: the representation
costs one comparison. Refusing a construct with this much measured demand, at
that price, would be a gap chosen rather than a gap found.

## What this does not close

**A discrete change of a non-scalar prop is still unreachable, and this is not
the reason why.** Every animatable channel is scalar — `X`, `Y`, `Width`,
`Height`, `Gap`, the four fill components, `Opacity`, and story #770's three
rotation channels. `Prop::Visible` is a bool and is reachable only as an
instantaneous `VariantVisible` override, never as a track, so no
`TransitionSpec` of any shape can drive it.

For the two SVG attributes that raised this, that splits them:

- **`visibility`** maps onto a step on `Opacity`, which this decision makes
  expressible.
- **`display`** is layout-affecting and maps onto `Prop::Visible`, which has no
  channel at all. A step does not help it, and neither would a `Steps` variant.

Whether a bool channel should exist is a separate question about the channel
vocabulary, not about how a curve is shaped. It is recorded here so that the
absence reads as decided rather than overlooked — the failure mode issue #852
names, and the one that lost the rotation channel at #143.

## Alternatives considered

- **`Steps { duration, count }`.** Above: a second spelling of a curve
  `Keyframes` already carries, at four sites of schema cost.
- **Relaxing the interval to admit a frame at 0 or 1.** Would let a step sit at
  the very start or end, which is an instantaneous change — already expressible
  by omitting the prop from `VariantTransition.tracks`, where it takes its new
  value at commit.
- **Permitting any number of frames at one `t`.** Simpler to state, and it makes
  the middle values silently unreachable.
