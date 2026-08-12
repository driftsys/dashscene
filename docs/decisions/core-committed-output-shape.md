# dashscene-core owns its committed-output types (boundary B, v0.1)

    status   accepted (story #2, 2026-07-12); reconciled by story #4 —
             see docs/decisions/boundary-b-unification.md (core now uses
             dashpaint's types; NO_PAINT is gone from the committed
             output — every rect resolves to a pool entry); reconciled
             again by story #164 — the paint/clip interners are now
             retained across commits, so the dirty diff is a plain
             entry-bit compare with no resolved-color clause (as-built
             definition in docs/design/dashscene-core-arena.md)
    scope    dashscene-core committed output; reconciliation due in story #4

## Context

Boundary B's v0.1 contract was pinned for stories #2 and #3 to build against in
parallel: a flat rect table indexed by document DFS node index, blittable
entries (`x, y, w, h` as `f32` + paint index as `u32`), solid-fill paint as
4×`f32` RGBA, and the double buffer / generation stamp / dirty set owned by
`dashscene-core`. Story #2 had to decide where core's output types come from and
pin the details the contract left open.

## Options

1. Core defines its own `RectEntry`/`Paint`/`Color` types with exactly the
   pinned shapes; story #4 reconciles with `dashpaint`.
2. Depend on `dashpaint` and use its types directly.
3. Reuse `dashbuf`'s flatc-generated structs.

## Choice

**Superseded by story #4 (`docs/decisions/boundary-b-unification.md`):**
`dashscene-core` no longer defines these types or the `NO_PAINT` sentinel; it
depends on `dashpaint`, and every committed rect resolves to a pool entry. The
details below record the story #2 decision as it stood before that
reconciliation.

Option 1, with these pinned details:

- `RectEntry { x, y, w, h: f32, paint: u32 }` and `Color { r, g, b, a: f32 }`,
  both `#[repr(C)]` + `Copy`; layout asserted by test (20 bytes / 16 bytes,
  align 4).
- A node with no fill carries paint index `NO_PAINT = u32::MAX` (mirrors
  `dashbuf`'s `NO_PARENT` sentinel). **Known conflict for story #4 to resolve:**
  `dashpaint` as merged on `main` defines an identical `RectEntry` shape but a
  `Painter` contract that paints every rect and a `PaintTable::resolve` that
  panics on any unresolvable index
  (`docs/decisions/painter-trait-infallible-slice-input.md`) — so a committed
  scene containing an unfilled node cannot be handed to a `Painter` as-is. Story
  #4 must decide how an unfilled node crosses boundary B (for example a
  `PaintKind` for "none", or core guaranteeing every emitted rect resolves).
- `add_node` refuses the `u32::MAX`-th node, so neither a `NodeId` nor a paint
  index (the paint table never outgrows the node count) can ever equal its
  sentinel (`NO_PARENT` / `NO_PAINT`).
- Paint table deduplicates by exact color bit pattern (`f32::to_bits`), ordered
  by first use in DFS order, rebuilt per commit (until story #164 retained the
  interner) — deterministic output (R7).
- Dirty set = per-index diff of consecutive committed rect tables: an entry is
  dirty when its bits changed (`f32::to_bits` comparison, so NaN does not
  self-compare unequal forever) or when its resolved fill color changed.
  Comparing entry bits alone is insufficient because the paint table is
  re-interned every commit — a stable index can reference a different color and
  an index shift can leave the color unchanged. Op-touched tracking was
  rejected: it misses descendants whose absolute position changes via a parent
  move. **Story #164 retained the interners**, which stabilizes the indices and
  removes the resolved-color clause: an entry-bit compare is now sufficient
  because a changed fill earns a new paint index. The op-touched rejection still
  holds for the _published_ dirty array, which stays a per-index diff.
- Generation increments on every commit, including no-change commits — the stamp
  says a commit happened, the dirty set says what changed.

## Why

- Stories #2 and #3 run in parallel without seeing each other; a `dashpaint`
  dependency (option 2) was explicitly excluded by the story contract, and the
  umbrella dependency direction (`dashpaint` consuming core's output vs. core
  producing into `dashpaint`'s types) is exactly what story #4 is for.
- `dashbuf`'s generated structs (option 3) are document-format types behind
  flatbuffer accessors — the committed output is runtime output, deliberately
  not the document (P1), and linking generated code for three plain structs
  provides no benefit.
