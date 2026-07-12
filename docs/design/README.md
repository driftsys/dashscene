# design

Architecture: interfaces and components, gardened from `docs/wip/` sessions
into durable, as-built records.

The project's system-wide architecture still lives in `specs/DESIGN_1.md`
(stack, pipeline, document format, producers, common runtime, painters,
target-hardware rules). It'll move here, or be superseded by records written
here, as future work gardens it in. Per-component records land here directly:

- [dashpaint.md](dashpaint.md) — boundary B: the paint table + painter
  trait (v0.1 walking skeleton, story #3; v0.3 paint vocabulary,
  story #13).
- [dashscene-core-arena.md](dashscene-core-arena.md) — the arena's intent
  model, staged mutation, commit resolution pipeline, and committed output
  (story #2); text content and style as intent, v0.5 (story #26).
- [dashbuf.md](dashbuf.md) — boundary A: the `.dsb` document schema (v0.1
  walking skeleton; v0.3 paint vocabulary, story #13; v0.5 text
  vocabulary, story #26).
- [dashlang.md](dashlang.md) — the value-tree builder surface and its
  one-commit mapping onto `dashscene-core` (story #5).
- [atlas-pipeline.md](atlas-pipeline.md) — the build-time font → MSDF glyph
  atlas + metrics blob pipeline (v0.5, story #27).
- [dashscene-skia.md](dashscene-skia.md) — the Skia CPU-raster reference
  painter, the first `Painter` implementation (story #4).

See the `sdd-working-memory-lifecycle` rule.
