# dashscene-core owns its committed-output types (boundary B, v0.1)

    status   accepted (story #2, 2026-07-12)
    scope    dashscene-core committed output; reconciliation due in story #4

## Context

Boundary B's v0.1 contract was pinned for stories #2 and #3 to build
against in parallel: a flat rect table indexed by document DFS node
index, blittable entries (`x, y, w, h` as `f32` + paint index as
`u32`), solid-fill paint as 4×`f32` RGBA, and the double buffer /
generation stamp / dirty set owned by `dashscene-core`. Story #2 had
to decide where core's output types come from and pin the details the
contract left open.

## Options

1. Core defines its own `RectEntry`/`Paint`/`Color` types with exactly
   the pinned shapes; story #4 reconciles with `dashpaint`.
2. Depend on `dashpaint` and use its types directly.
3. Reuse `dashbuf`'s flatc-generated structs.

## Choice

Option 1, with these pinned details:

- `RectEntry { x, y, w, h: f32, paint: u32 }` and
  `Color { r, g, b, a: f32 }`, both `#[repr(C)]` + `Copy`; layout
  asserted by test (20 bytes / 16 bytes, align 4).
- A node with no fill carries paint index `NO_PAINT = u32::MAX`
  (mirrors `dashbuf`'s `NO_PARENT` sentinel); painters skip such
  entries. **Story #4 must reconcile this sentinel with `dashpaint`'s
  entry definition.**
- Paint table deduplicates by exact color bit pattern
  (`f32::to_bits`), ordered by first use in DFS order, rebuilt per
  commit — deterministic output (R7).
- Dirty set = exact per-index diff of consecutive committed rect
  tables. Op-touched tracking was rejected: it misses descendants
  whose absolute position changes via a parent move.
- Generation increments on every commit, including no-change commits —
  the stamp says a commit happened, the dirty set says what changed.

## Why

- Stories #2 and #3 run in parallel without seeing each other; a
  `dashpaint` dependency (option 2) was explicitly excluded by the
  story contract, and the umbrella dependency direction
  (`dashpaint` consuming core's output vs. core producing into
  `dashpaint`'s types) is exactly what story #4 is for.
- `dashbuf`'s generated structs (option 3) are document-format types
  behind flatbuffer accessors — the committed output is runtime
  output, deliberately not the document (P1), and linking generated
  code for three plain structs buys nothing.
