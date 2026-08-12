# dashpaint owns its boundary-B input types, with no dependencies

    status   accepted (story #3, 2026-07-12); ownership resolved at
             story #4 — dashpaint owns the types and dashscene-core
             depends on it, see docs/decisions/boundary-b-unification.md
    scope    crates/dashpaint

## Context

Story #3 defines boundary B (`docs/design/architecture.md`): the types a painter
consumes (`Color`, `RectEntry`, `PaintKind`, `PaintTable` — joined at story #13
by `PaintEntry` and the v0.3 vocabulary types) and the `Painter` trait.
Solid-fill color is pinned to "4×f32 RGBA exactly as `dashbuf`'s `Color`
struct". The question was whether `dashpaint` should depend on another workspace
crate to obtain these types.

## Options

1. Define all types in `dashpaint`, dependency-free; the runtime converts at the
   boundary.
2. Depend on `dashbuf` and reuse its generated `Color`.
3. Depend on `dashscene-core` and reuse its rect/paint shapes.

## Choice

Option 1: `dashpaint` defines its own plain types and has no dependencies.

## Why

- Painters sit downstream of the runtime, not of the document format
  (`docs/design/architecture.md` pipeline). A `dashbuf` dependency couples
  boundary B to the file format and leaks the flatbuffers crate into every
  painter.
- The pinned contract says "same shape", not "same Rust type" — a plain
  `Color { r, g, b, a: f32 }` satisfies it without the coupling.
- A `dashscene-core` dependency is explicitly excluded by story #3 (the trait
  must compile and be testable standalone), and it would prevent story A and
  story B from proceeding in parallel.
- Single ownership of the shared shapes (whether `dashscene-core` ends up
  depending on `dashpaint` or the reverse) is deliberately deferred to story #4,
  where both crates exist and the better direction is visible.
  `docs/decisions/house-style.md`'s publish order is updated then if needed.
