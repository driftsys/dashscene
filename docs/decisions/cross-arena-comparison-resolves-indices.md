# Comparing two arenas resolves every index and offset first

    status   accepted (2026-08-01)
    scope    every test that compares the committed output of two independent
             arenas; today corpus/showcase/tests/migration.rs and the
             DSL-equals-hand-built assertions in crates/dashlang/tests/ and
             goldens/tooling/tests/v02_flex.rs

## Context

The repo's standing way to prove two producers agree is to build both into
separate arenas and compare the committed painter input
(`docs/decisions/dashlang-flex-vocabulary.md` D3,
`docs/decisions/dashlang-paint-vocabulary.md` D5). The showcase migration
proof is the largest such comparison: three scenes, each built by a frozen
pre-migration builder and by the migrated one-pass builder.

A committed value may carry an **index or an offset into a per-arena table** —
a `PaintIndex` on a rect, a `ClipRegion`'s flattened range, a `ShadowRange` or
`BlurRange` inside a `PaintEntry`. Such a position is a function of that
arena's own commit history, not of the picture: the paint interner is
retained, so an entry keeps the index it was first assigned, a changed entry
earns a new one, and the entry it replaced stays in the table
(`Arena::paint_map`, issue #164).

Two arenas reaching the same picture by different commit sequences therefore
earn different positions for the same content — and reaching it by different
commit sequences is exactly what these comparisons are for. The frozen showcase
builder stages all of its paint in the second commit; the migrated one stages
all of it in the first except what needs an arena-issued image index.

## Decision

**No value carrying an index or offset into a per-arena table may be compared
directly. Resolve it to its contents first, and compare only the contents.**

Comparing tables whole asserts the order the interner handed out its indices,
which no migration that moves paint between commits can preserve, and which no
painter can observe. Comparing what a painter draws is both the weaker
assumption and the exact one.

The corollary: a table a rect reaches only _through_ an index still has to be
compared whole somewhere, or a swapped payload behind a matching index passes.
`migration.rs` compares the image table whole for this reason, having compared
paints per rect through `PaintTable::resolve`.

## Why this is written down rather than left in the helper

The rule has bitten the same helper three times, each time as a passing test
that should not have passed, or a failing test that was right to fail for a
reason nobody predicted:

- the paint-table index on a rect — resolved through `PaintTable::resolve`;
- `ClipRegion`'s flattening to a range — `ClipView` derives `PartialEq` over
  its stored `offset`/`count` as well as its boxes, so the comparison reads
  `ClipView::boxes()` rather than comparing the views;
- `PaintEntry`'s `shadows`/`blurs`, which story #578 turned into
  arena-relative `ShadowRange`/`BlurRange` positions in their own right —
  resolved through `PaintTable::shadows`/`PaintTable::blurs` rather than
  compared as part of the whole `PaintEntry`.

The third is the reason this is a record rather than a comment. It arrived
from an unrelated story, in a field the helper already compared and had been
correct to compare, and it will happen again the next time a committed
structure is flattened into a table. Any such flattening is a change to what
this rule covers.

The worked example lives in `corpus/showcase/tests/migration.rs`
(`assert_same_committed` and its doc comment). That file is a one-way ratchet
that is deleted scene by scene as the scenes deliberately change
(`docs/decisions/dashlang-paint-vocabulary.md` D5), so the rule is recorded
here, where it outlives its current host.

## Trace

- Worked example: `corpus/showcase/tests/migration.rs`.
- Related decisions: `docs/decisions/golden-comparison-space.md` (the same
  posture one layer down — goldens compare decoded pixels, never encoded
  bytes); `docs/decisions/resolved-clip-regions-at-commit.md` and
  `docs/decisions/paint-entry-composition.md` (the tables this resolves
  through); `docs/decisions/dashlang-paint-vocabulary.md` D5 (the proof that
  forced the rule into its current shape).
