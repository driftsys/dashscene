# An optional member on a boundary-B row is a range, and an empty range is (0, 0)

    status   accepted (2026-08-02)
    scope    dashpaint's PaintEntry and the flat arrays PaintTable owns;
             any future boundary-B row that needs an optional member

## Context

Story #578 gives boundary B a C representation. `PaintEntry` was the last
type to flatten, and it carried three `Option`s — `fill: Option<PaintKind>`,
`stroke: Option<Stroke>`, `shape: Option<VectorField>` — plus one `Vec`.

`Option<T>` has no C representation for a `T` without a niche, so each of
them had to become something a C header can declare. The `Vec` had an
established answer already: four earlier steps of this story turned nested
collections into `(offset, count)` ranges into one flat array on the table.
The `Option`s did not.

## Decision

**D1 — an optional member becomes a range, with its arity bound stated
rather than encoded.** `stroke` and `shape` are `StrokeRange` and
`ShapeRange`, each carrying `count` 0 or 1. `extra_fills` is a `FillRange`
with no bound. A fill-less entry is `PaintKind::NONE`, a tag variant rather
than a range, because `PaintKind` was already a tag and adding a `None`
variant costs no bytes.

**D2 — an empty range is canonically `(0, 0)`.** `PaintTable::push_with`
assigns `(0, 0)` for any part it was given none of, rather than the offset
the part would have started at.

## Why

**Against a sentinel index.** `docs/decisions/boundary-b-unification.md`
already weighed this for `RectEntry.paint` and chose "every rect resolves"
over a `NO_PAINT = u32::MAX` sentinel, because a sentinel is a skip rule
every painter has to remember, and per-backend divergence in that rule is
the failure `docs/decisions/painter-trait-infallible-slice-input.md`
records. A range keeps that property and improves on it: an empty range
resolves to an empty slice, so the common read is a loop that runs zero
times and needs no test at all.

**Against a presence flag beside the value.** `PaintEntry { stroke: Stroke,
has_stroke: u8 }` is FFI-legal — the story's own rules permit `u8` for
booleans — but it stores one fact twice, and the two can disagree. The same
objection this repo already records against a separate backdrop-sampling
flag beside `BlurKind::Backdrop` (`PaintEntry::blurs`, "two records of one
fact can disagree").

**Against keeping the value inline.** `Stroke` is 24 bytes and
`VectorField` is 40, and most entries carry neither. Inline, the row is 112
bytes, most of it dead for the common case; as ranges it is 64. R-T4 plans
dirty-range instance-buffer uploads of this row, so the smaller uniform row
is the one that gets uploaded.

**Why the arity bound is stated, not encoded.** A `count` of 5 strokes is
representable and meaningless. The alternative — a type that cannot express
it — costs a second shape for the same idea, and this repo already handles
exactly this case the other way: `MAX_GRADIENT_STOPS` bounds a gradient's
stops as a validator diagnostic, not as a type. `PaintTable::stroke` and
`PaintTable::shape` panic by name above one, and the reader gets the same
"refused, not discovered" treatment P4 asks for.

It also leaves debt #146's stroke half expressible without a second
migration: a node that one day stacks strokes needs a wider bound here, not
a different shape.

**Why D2, and how it was found.** An empty range still recorded where it
_would_ have started. That offset names nothing, but it is observable, and
it made two entries that both draw nothing compare unequal — one with
`stroke: (0, 0)` and one with `stroke: (1, 0)`, differing only in a number
that means nothing. It broke every comparison against
`PaintEntry::default()`, which is the shared draws-nothing entry
`boundary-b-unification.md` introduced.

The same latent defect existed for `shadows` and `blurs` from the moment
they became ranges: `push_with_effects` assigned `offset: self.shadows.len()`
unconditionally. It went unnoticed because no test until this one had both
a scene with effects and a comparison against the default entry.

## Consequences

- `PaintEntry` is `#[repr(C)]`, `Copy`, 64 bytes, and joins
  `dashscene-unity`'s `improper_ctypes_definitions` surface, which is what
  turns all of the above from intent into a build failure. Its layout is
  pinned by test there.
- `PaintTable` grows three flat arrays — `extra_fills`, `strokes`, `shapes`
  — and the accessors that resolve them. `push_with_effects` is replaced by
  `push_with`, which takes an `EntryParts` and assigns every range.
- A `PaintEntry` is meaningful only against the table that assigned its
  ranges. `dashscene-core`'s `compact_paints` re-homes all of them when it
  rebuilds, and every cross-arena comparison has to resolve them before
  comparing two arenas — `corpus/showcase/tests/migration.rs` did, until it
  was deleted in commit `535b547`; `crates/dashlang/tests/` and
  `goldens/tooling/tests/v02_flex.rs` still do
  (`docs/decisions/cross-arena-comparison-resolves-indices.md`).
- `ImageAsset` is the last unflattened boundary-B type. It is a different
  problem: its `Vec<u8>` is a payload, not a reference into a table, so
  flattening it means deciding where a decoded-ready blob lives rather than
  which array holds it.
