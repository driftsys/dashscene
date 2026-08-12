# Fixed positioning is authored as x/y on FixedSizeLayout

    status   accepted (story #2, 2026-07-12)
    scope    dashbuf schema, dashscene-core resolution semantics

## Context

`dashbuf`'s v0.1 `FixedSizeLayout` struct carried `width`/`height` but no
position, and the walking skeleton needs more than one visible rect. P1 (the
document carries intent, never results) forbids resolved coordinates in the
document, so the open question was how fixed positioning gets authored at all.

## Options

1. Extend the `FixedSizeLayout` struct with authored `x`/`y`.
2. Add a position field to the `Node` table (the FlatBuffers-evolvable route —
   tables accept new fields, structs do not).
3. Keep position out of the schema; author it only through the arena's mutation
   API.

## Choice

Option 1: `FixedSizeLayout` is now `x, y, width, height`, where `x`/`y` is an
authored offset relative to the parent node (canvas origin for a root).

## Why

- An authored offset is intent, not a resolved result, so P1 allows it in the
  document. The resolved absolute position (parent absolute + own offset, summed
  down the tree) exists only in the runtime rect table.
- Struct non-evolvability has no cost here: no `.dsb` documents exist outside
  dashbuf's own round-trip test, and `FixedSizeLayout` is not a long-lived
  evolution surface — the schema header already plans for layout modes to become
  a union when v0.2 introduces Taffy modes. Option 2 would scatter fixed-layout
  intent across two places for an evolution guarantee nothing needs yet.
- Option 3 would make position inexpressible in the document, breaking the v0.9
  same-scene-both-ways exit criterion (E1).
