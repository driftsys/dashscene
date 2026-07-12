# The v0.2 flex vocabulary is two optional Node tables, mirrored as stored intent

    status   accepted (story #8, 2026-07-12)
    scope    dashbuf schema, dashscene-core intent model; binds the
             story #9 Taffy solve and the v0.8 wrap/grid/baseline work

## Context

v0.2 (epic #7) carries the flex-layout vocabulary — mode NONE/H/V,
hug/fill/fixed sizing per axis, gap, padding, alignment, min/max (R2)
— into the document schema and the semantic model before the solver
exists (Taffy is story #9). The schema change had to be additive (R7)
and coexist with story #13's parallel paint-schema work.

## Options

1. Two new optional `Node` tables: `LayoutContainer` (container-side:
   mode, gap, padding, aligns — mode-neutral name, so the v0.8 grid
   mode adds its track fields here rather than in a parallel table)
   and `LayoutConstraints` (child-side: per-axis sizing, optional
   min/max).
2. Flat new fields appended directly on `Node`.
3. Replace `FixedSizeLayout` with a layout union.

## Choice

Option 1, with these semantics:

- The split mirrors how the vocabulary is actually owned — container
  properties versus child properties — which is also how both Figma
  and Taffy group it. Enum members carry explicit wire values so a
  mid-enum insertion cannot silently renumber existing documents; a
  reader older than an appended value receives a raw integer and must
  range-check it into a named diagnostic (P4/R6).
- `FixedSizeLayout` stays the only geometry carrier: width/height are
  the datum `Fixed` sizing uses; authored x/y apply under a
  `mode = None` parent and are ignored under H/V (the solver owns
  placement, P1/P2).
- Absent min/max scalars (`= null`) mean unconstrained — absence of
  intent is not a value of intent (P1), so no sentinel values.
- Wrap and Grid append to `LayoutMode` at v0.8; `Baseline` appends to
  `CrossAxisAlign` (Q-4) — enum-value appends are additive.
- `dashscene-core` mirrors the vocabulary as its own types (no
  `dashbuf` dependency, per
  `docs/decisions/core-committed-output-shape.md`): a public `Layout`
  snapshot struct with a named `EdgeInsets` padding (positional edge
  arrays invite silent transposition against solver types), granular
  `Prop` variants (one property per call — the staged API's existing
  grain; padding is one prop because the schema carries it as one
  struct), and the read seam for story #9: `Arena::layout(NodeId)`
  plus tree traversal via `Arena::roots()` and
  `Arena::children(NodeId)`. How the solved rects are injected back
  into the committed output was story #9's design — resolved:
  `docs/decisions/layout-solver-seam.md`. Setting a min/max constraint cannot be undone (no clear
  operation) — the same deliberate gap as fill-clearing
  (`docs/decisions/staged-mutation-v01-scope.md`).
- Until story #9, flex intent is stored, not solved: `commit` keeps
  resolving fixed geometry only, and setting flex props changes no
  committed output (test-asserted).

## Why

- Option 2 mixes two ownership domains into one namespace and puts
  eight rarely-set scalars on every node's table for no structural
  gain.
- Option 3 is not additive — existing writers break — and misreads
  the model: fixed geometry is not an alternative mode but the datum
  the flex vocabulary reinterprets.

## Scoping note: no `.dsb` load path exists

Issue #8 lists "the `.dsb` load path" as a mirror site. No load path
exists anywhere yet — no crate outside `dashbuf` links the generated
code (verified at story start). The fields are mirrored in schema and
arena so a future loader has nothing missing; building the loader
belongs to the importer slice (v0.3+), where `dashc` first consumes
`.dsb` documents.
