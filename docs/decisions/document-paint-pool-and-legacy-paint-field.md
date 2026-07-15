# dashbuf: paint lives in a document-level pool; the legacy paint field stays

    status   accepted (story #13, 2026-07-12)
    scope    crates/dashbuf/schema/dashbuf.fbs

## Context

Story #13 grows the schema's paint vocabulary from one solid fill to
solids, gradients, image fills, strokes, corner radii, and clip.
`docs/design/dashbuf.md` describes the document as a flattened DFS node tree
with interned strings and a **dedup style pool**, and boundary B's
runtime paint table is already index-based (`RectEntry.paint: u32`).
`Node.paint: SolidFill` (the v0.1 walking-skeleton shorthand, inline on
the node) already exists on `main`. R7 requires reproducible, evolvable
documents; FlatBuffers' evolution rule is append-only field ids.

## Options

1. A document-level pool: `Document.paints: [Paint]` where
   `Paint { fill: Fill union, stroke, corners, clip }`, referenced by a
   new `Node.paint_entry: uint32` index; `paint` kept unchanged.
2. Inline growth: append `fill`/`stroke`/`corners`/`clip` fields
   directly to `Node`; `paint` kept unchanged.
3. Retype or remove `paint` in place as part of either shape.

## Choice

Option 1: the pooled shape, keeping the legacy `paint` field.

An earlier draft of this story shipped option 2; the story's review
pass flagged the divergence from `docs/design/dashbuf.md`'s dedup style
pool and it was reworked to option 1 before merge.

## Why

- `docs/design/dashbuf.md`'s document shape is a dedup style pool: distinct nodes sharing a
  style share one pooled entry instead of repeating inline paint tables
  per node (real documents have thousands of nodes over tens of
  styles).
- The pool's index model matches boundary B exactly —
  `dashscene-core`'s committed output resolves paints by table index
  (typed `PaintIndex` since story #4), and `Node.paint_entry` uses the
  same `uint32::MAX` sentinel convention as `Node.parent`. The sentinel
  is document-level only: since story #4 the committed output has no
  sentinel — every rect resolves to a pool entry
  (`docs/decisions/boundary-b-unification.md`). Lowering stays an
  index-preserving copy rather than per-node hash-consing.
- Removing or retyping `paint` (option 3) would violate the append-only
  discipline R7 relies on. No crate outside `dashbuf`'s own tests reads
  `Node.paint` today (checked on `main`: `dashscene-core` has no
  `dashbuf` dependency), but the v0.1 walking-skeleton stories (#5
  `dashlang`, #6 golden harness) are planned against the current
  surface, and append-only evolution is the standing rule regardless of
  the consumer count.
- Cost accepted: a solid fill has two representations until cleanup.
  The precedence rule is written in the schema comment (`paint_entry`
  supersedes `paint` when set) and becomes a `dashscene-validator`
  diagnostic when profile enforcement lands.
- Cleanup condition: once the v0.1 producers stop writing `paint`, the
  field is removed in a coordinated change — one PR touching every
  writer and reader, at a phase boundary, per the plan-revision rule in
  `AGENTS.md`.
