# Boundary B unified: dashpaint owns the types, every rect resolves, indices are typed

    status   accepted (story #4, 2026-07-12)
    scope    dashpaint, dashscene-core, dashscene-skia;
             docs/archive/2026-07-14-scope-decisions.md §7/§15

## Context

Stories #2 and #3 built `dashscene-core` and `dashpaint` in parallel against a
pinned contract, each holding its own copy of the boundary-B shapes, and each
recorded the reconciliation as story #4's job
(`docs/decisions/core-committed-output-shape.md`,
`docs/decisions/dashpaint-owns-boundary-b-types.md`). Their records also left
two named conflicts: core's `NO_PAINT = u32::MAX` sentinel versus
`PaintTable::resolve`'s panic-on-unresolvable contract (debt #55), and the
untyped `u32` paint index (debt #54).

## Options

1. `dashpaint` owns the types; `dashscene-core` depends on `dashpaint`.
2. `dashscene-core` owns the types; `dashpaint` depends on core.
3. Both keep their own types with a conversion layer at the seam.

With, orthogonally: keep the `NO_PAINT` sentinel (painters skip) vs intern a
shared draws-nothing entry (every rect resolves); and bare `u32` paint indices
vs a `PaintIndex` newtype.

## Choice

Option 1, with every rect resolving and typed indices:

- `dashscene-core` deletes its mirror types and depends on `dashpaint`;
  `CommittedScene.paints` is a `dashpaint::PaintTable`, so `Arena::committed()`
  yields exactly the two values `Painter::paint` consumes. Core re-exports what
  it consumes.
- Publish order becomes dashbuf → dashpaint → dashscene-core → …
  (`docs/decisions/house-style.md`, workspace `Cargo.toml`, `justfile`).
- An unfilled node interns `PaintEntry::default()` — the shared draws-nothing
  entry. `NO_PAINT` is deleted from core's public API; `dashbuf`'s
  `Node.paint_entry` keeps its document-level `uint32::MAX` sentinel (a document
  node referencing no pool entry still resolves to a rect whose runtime entry is
  the shared empty one).
- `RectEntry.paint` is `#[repr(transparent)] PaintIndex(u32)`;
  `PaintTable::push`/`get`/`resolve` take and return it. Layout unchanged
  (entries stay 20 blittable bytes, pinned by test). Closes #54.

## Why

- Dependency direction (option 1 over 2): §4's pipeline runs producers → runtime
  → painters; a painter crate depending on the runtime would build the arena and
  (from v0.2) Taffy into every painter, against §8's "painters only color" and
  R3's lean-painter goal. The boundary types live in the crate that defines the
  boundary.
- Against option 3: a permanent per-frame conversion between identical shapes,
  and two definitions of one contract that drift — the state both prior records
  explicitly marked as temporary.
- Every-rect-resolves over the sentinel: painters get no skip rule to
  re-implement (per-backend divergence is the failure mode
  `painter-trait-infallible-slice-input.md` records), `resolve` stays the single
  failure mechanism, and from v0.3 vocabulary onward a fill-less entry is a real
  entry anyway (it can carry stroke or clip). Cost: at most one pooled entry per
  commit.
- `PaintIndex` now: core's `commit` builds node ids, DFS indices, and paint
  indices in one function — the confusion #54 records — and the unification
  already touched every affected line.

## Consequences

- Debt #54 and #55 close with story #4.
- Unfilled nodes lose the sentinel's re-interning immunity: the shared empty
  entry's index follows first-use DFS order like every other entry, so an early
  fill change can shift it and conservatively over-mark unchanged layout-only
  rects as dirty. Same direction and mechanism as the recorded index-shift
  behavior for solid entries (`core-committed-output-shape.md`'s dirty-set
  definition); over-marking never under-paints.
- The v0.1 pinned-contract phase ends; the boundary-B surface is defined in one
  place (`dashpaint`) and recorded as-built in `docs/design/dashpaint.md`.
