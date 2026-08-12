# Authored fill weights are declined (#117)

    status   accepted (v0.2-close plan revision, 2026-07-13)
    scope    dashscene-core's Prop set, dashscene-engine's Taffy mapping,
             the dashbuf schema
    binds    v02-flex-goldens-per-construct.md; reopen only behind a stated
             consumer requirement

## Context

Epic #7 (v0.2 — flex core)'s scope list named authored fill weights (an unequal
split among `Fill` siblings, analogous to CSS `flex-grow: 2`). As built, core's
`AxisSizing::{Fixed, Hug, Fill}` carries no weight, and `dashscene-engine` maps
every `Fill` to `flex_grow = 1.0`, so `Fill` siblings always split free space
equally. Story #11 goldened the equal split rather than inventing a weighted
one.

## Options

1. Add an authored weight to `Fill` sizing now, matching CSS `flex-grow`.
2. Decline the construct: keep the equal split, carry no weight anywhere in the
   schema or producer API.

## Choice

Option 2. Fill weights are declined outright; "fill weights" is dropped from the
v0.2 scope wording, and issue #117 closes on this decision rather than on an
implementation.

## Why

- Figma auto-layout has no flex weight either, so an authored weight would be a
  CSS-flexbox construct with no Figma counterpart and no producer emitting it.
- P4 — vocabulary is validated, never discovered — means a weight would have to
  be carried by the schema, core's `Prop` set, the engine's mapping, and every
  validator profile, permanently, for a construct nothing produces.
- P5 ("no producer's limitations define the format") is the argument on the
  other side, and a real one — the code-DSL path could plausibly want a 2:1
  split. But P5 says Figma's limits must not _bound_ the format, not that the
  format should grow constructs nobody has asked for yet.

## Consequences

- Reopen only when a real consumer appears — the C# declarative DSL, or a
  stress-corpus case needing an unequal split. At that point it is a schema
  change with a stated requirement behind it, not a speculative addition.
- `docs/decisions/v02-flex-goldens-per-construct.md` records fill weights as out
  of the v0.2 golden scope, consistent with this decision.
