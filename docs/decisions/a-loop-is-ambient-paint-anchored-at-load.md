# A loop is ambient paint, anchored at load

    status   accepted 2026-08-09, story #772 (epic #769)
    affects  dashcue, dashbuf, dashscene-core, dashscene-validator, dashlang
    related  docs/decisions/motion-is-document-data-keyed-on-the-destination.md
             docs/decisions/a-step-is-a-pair-of-keyframes.md
             docs/decisions/visible-is-layout-opacity-is-paint.md

Every animation before this slice was bound to a variant switch. The
scheduler is reactive by construction — a track binds `(from, to)` at commit
time — so motion that runs without a state change behind it could not be
expressed at all. That excludes the ambient class: shimmer, spinner, pulse,
breathing, skeleton loaders. It is also the one class Figma cannot author,
because Figma has no timeline and its prototype model is variant-to-variant,
so it does not arrive from the importer either. If the vocabulary does not
carry it, it reaches a document by no route.

A `LoopTrack` is that vocabulary: one channel of one node repeating a curve
indefinitely. Four decisions shape it, and each was taken against what the
code and the two target producers actually do.

## A loop's phase comes from document load, plus a per-track offset

The document declares no clock. A loop's cycle starts when the runtime
attaches to the arena, and a `phase_offset` in seconds places each track
inside its own cycle — which is what staggers a row of skeleton bars out of
step. The runtime owns the clock (P3); the document owns the offset.

**A visibility-anchored phase is deliberately not shipped.** It is
buildable — `Arena::visible_toggled` is a real change log, pushed on a flip
and cleared at commit — but it is a second feature, not a second enum arm: it
needs a per-node clock and a re-entry policy, because a node hidden and shown
again may either restart its cycle or resume it, and those are different
behaviours for a skeleton loader and a spinner. All five cases in the class
are visible when the document loads, so one origin covers them. The field is
append-only, so the origin arrives when something needs it.

An offset of a whole cycle or more is the same phase as its remainder, so a
producer cannot push a track arbitrarily far into the future by inflating it.

## Nothing ends a loop

No repeat count, no end signal. Every case in the class is endless by
definition, and the two producers that would lower onto this both express
finiteness the same way — SMIL's `repeatCount` and CSS's
`animation-iteration-count`, each a number or a distinguished infinite. Both
lower onto a `repeat` field appended at `LoopTrack`'s tail later with no R7
break, so a count arrives when a producer needs one rather than ahead of it.

A signal-driven end was rejected outright: it couples the loop table to the
signal table for a case neither Figma nor SVG can author, and a producer that
wants a loop to stop can already hide the node.

The consequence is stated here rather than discovered: **a document carrying
one loop never idles.** `LiveScene::tick`'s idle frame test reads
`Scheduler::is_settled`, and a track that never finishes is never settled, so
every frame commits, the generation moves every frame, and both hosts draw
continuously for as long as the document is loaded.

## A loop animates paint channels only

The fill components, `Opacity`, `Rotation` and its two anchor components. A
loop naming a layout channel is refused by name (`loop.channel-not-paint`).

This is the mirror of the rule a variant transition already carries
(`transition.channel-not-a-rect`), and together the two constructs partition
the channel space **by where their values come from**. A variant transition
travels between two _resolved_ rects, which the engine binds at the switch
and the document never stores (P1). A loop has no two states to travel
between, so it names its own `from` and `to` — authored values, the kind the
node tree and `VariantWidth` have always carried.

The frame cost is the other half, and it follows from the section above.
Because a loop never settles, a loop on a layout channel would force a real
solve on every frame for as long as the document is loaded. On a paint
channel the same loop commits through the retained-geometry replay and never
solves at all — asserted, not assumed, by a test that counts solves across a
full cycle. Every case in the class is paint-only; a size-animating loop
wants a scale channel, which this slice does not carry (the epic put scale
and skew out of scope).

## A loop is the sole writer of its channel

Refused against a binding row, a variant transition track, a second loop, and
a variant member's override of the same paint prop. The first three collide
in the runtime's packed `PropKey`, where one writer silently shadows another.
The fourth is subtler and is why the rule is stated as "sole writer" rather
than as three separate clashes: `Arena::commit` resolves a node's fill and
rotation from the variant overlay _before_ the node's own values, so an
overriding member masks every sample a loop writes to the same prop for as
long as it is active. Seven of the eight channels a loop may use are
reachable that way.

A shadowed transition eventually finishes. A shadowed loop never gives the
channel back, which is what makes precedence the wrong answer and a named
refusal the right one (P4).

## Alternatives considered

- **A preset library** — a `LoopPreset` enum naming shimmer, spinner, pulse
  and the rest. Rejected: it makes a named behaviour something the _format_
  defines, which is what P5 warns against, every preset needs its own
  rendering rule, and a producer wanting a variation it does not cover has no
  route. The general mechanism covers all five in one table, and SMIL and
  Lottie both emit general curves rather than presets, so presets would help
  neither import route. Preset constructors are producer-side sugar if they
  are wanted at all, and belong in `dashlang`.
- **Looping a spring** — rejected because a spring carries no duration and so
  has no cycle to repeat. Looping it on an invented period would silently
  reinterpret the spec, so it is refused by name, the shape story #852 chose
  for a third keyframe sharing a `t`.
- **A scrubbable timeline** — out of scope by construction, not by budget.
  `advance(dt)` forward-integrates and a spring carries velocity, so a spring
  track cannot be seeked; a timeline and the reactive spring model are two
  scheduling regimes that coexist rather than merge. A loop is the ambient
  case, and adding it does not imply seek.
