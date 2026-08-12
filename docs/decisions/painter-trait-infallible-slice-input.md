# Painter trait: one infallible, object-safe method over bare slices

    status   accepted (story #3, 2026-07-12)
    scope    crates/dashpaint

## Context

Boundary B needs a trait signature for v0.1. `docs/design/architecture.md`
defines the eventual painter input as a triple (rect entries, glyph runs, dirty
set) plus paint-table indices, but v0.1 has no text and no incremental painting.
Three shape questions had to be settled: parameter packaging, error handling,
and dispatch.

One input shape was settled outside this record: issue #3 sketched the rect
entry as "(id, x, y, w, h)", but the pinned cross-session contract has no id
field — the rect-table index is the document DFS node index
(`docs/design/dashbuf.md`) — and the generation stamp of
`docs/design/architecture.md` belongs to the double buffer `dashscene-core`
owns, not to each entry. The pinned contract wins; both stories build to it. The
as-built shape is recorded in `docs/design/dashpaint.md`.

## Options

1. `fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable)` — bare
   slices, infallible, object-safe.
2. A `Scene`/`FrameInput` wrapper struct as the single parameter, so glyph runs
   (v0.5) and the dirty set can be added without changing the signature.
3. The same as option 1 but returning `Result<(), PaintError>`.

## Choice

Option 1.

## Why

- Wrapper struct (option 2): rejected as speculative structure (YAGNI). All
  painters live in this workspace pre-1.0; widening the signature at v0.5 is a
  cheap, compiler-guided refactor.
- `Result` return (option 3): rejected because no fallible operation exists in
  the v0.1 contract. P4 places vocabulary and index validation upstream
  (validator/commit), so an out-of-range `paint` index reaching a painter is a
  broken inter-crate contract, not a runtime condition. `PaintTable::resolve`
  centralizes the panic for that case, so every backend fails the same way;
  silently skipping the rect would be the silent drop P4 forbids.
- Object safety is required, not incidental: backend selection is whole-scene
  (R3), so `Box<dyn Painter>` must work; a unit test pins it.
