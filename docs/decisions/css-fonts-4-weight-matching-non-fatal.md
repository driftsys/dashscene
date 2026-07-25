# Decision: weight matching follows CSS Fonts 4 §5.2, and is non-fatal

    status   accepted (story #368, epic #344 — the design gate the
             repository owner approved before implementation)
    scope    crates/dashscene-typeset — the rule that picks one face out
             of a family for a requested CSS weight
             (src/text/weight.rs)
    binds    every family whose declared weights do not exactly match a
             requested weight, including every single-face family

## Context

The document carries CSS-scale weights 100–900 (`dashbuf`'s
`weight: ushort = 400`, `dashscene_core::TextStyle`). The corpus offers a
small discrete set of faces — three Latin weights and one Arabic weight
(`docs/decisions/atlas-directory-per-script-weight.md`) — so most of the
900-point scale has no exact face, and the second step of cascade
selection needs a rule for what happens then
(`docs/decisions/weight-selection-in-the-cascade.md`).

## Options

1. **Nearest available weight.** Pick the face whose declared weight is
   closest to the request.
2. **The CSS Fonts Level 4 font-matching algorithm, weight step (§5.2),
   adopted verbatim.** If the request is inclusively within 400..=500,
   try weights at or above the request ascending but no higher than 500,
   then weights below the request descending, then weights above 500
   ascending. If the request is below 400, descend first, then ascend. If
   it is above 500, ascend first, then descend.

Independently of the rule, what happens when a family offers nothing at
or near the request:

- Fail the layout.
- Resolve to the rule's best candidate and continue.

## Choice

Option 2, non-fatal. `match_weight(weights, requested)` returns an index
that is always valid: the phases cover every weight, an exact match is
the first candidate of the first phase in each of the three branches, a
tie between two faces at one weight resolves to the first declared, and
index 0 is the final fallback.

## Why

- **A nearest rule is underspecified at ties, and the ties are not
  hypothetical.** With faces {400, 600, 700}, a request for 500 is
  equidistant from 400 and 600, so the answer would depend on an
  arbitrary tie-break. The CSS rule resolves 500 to 400 by
  specification.
- **It costs nothing to adopt.** With faces {400, 700} the two rules
  agree at every requested weight, so the choice only becomes visible
  once a third face exists — which it now does.
- **It is the rule the designer's own tools already follow.** Every
  browser implements it, so a substitution this rule makes matches what
  the person who authored the design saw, which is what makes a
  substitution defensible rather than surprising.
- **Failing is not an option the committed fixtures permit.** Two
  committed fixtures request weight 700 against single-face cascades —
  `lowering-baseline.json` (one Inter Bold node) and
  `lowering-variant-topology.json` (three Inter Bold nodes) — and their
  goldens are consumed by self-oracle tests that build single-font
  typesetters. Those goldens keep their committed PNGs only if a request
  for 700 against a family offering 400 resolves to 400 and continues.
  Beyond the fixtures, the general case is the same: a corpus is a
  renderer's asset set, and a document may legitimately ask for a weight
  it does not hold.

## Consequences

- **A request for 500 resolves to Regular**, which is why no Medium (500)
  face was committed even though the hero authors two weight-500 nodes.
  Those nodes render Regular by specification, not by omission.
- **A single-face family absorbs every request**, so every pre-#368 call
  site behaves exactly as before: it declares 400, is asked for 400, and
  matches exactly.
- **Above 500, ascending beats absolute distance.** A request for 600
  against faces {400, 900} resolves to 900, not to the numerically nearer
  400. This is the specified behavior and is pinned by a unit test so it
  cannot be "fixed" into a nearest rule by accident.
- **A substitution is never silent.** The resolved face is reported
  through the named `text.weight-substituted` diagnostic
  (`docs/decisions/weight-substitution-is-a-render-time-diagnostic.md`),
  which is what keeps the non-fatal behavior compatible with P4.
- The rule is a pure function of a family's declared weights and the
  requested weight, so it is tested on weight lists alone — the exact
  committed corpus {400, 600, 700} across every CSS weight, plus the
  single-face, tie, below-400 and above-500 branches — with no font file
  or atlas involved.
