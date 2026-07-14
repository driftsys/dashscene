# design

Architecture: interfaces and components, gardened from `docs/wip/` sessions
into durable, as-built records.

The project's system-wide architecture still lives in `specs/DESIGN_1.md`
(stack, pipeline, document format, producers, common runtime, painters,
target-hardware rules). It'll move here, or be superseded by records written
here, as future work gardens it in. Per-component records land here directly:

- [dashpaint.md](dashpaint.md) — boundary B: the paint table + clip
  table + painter trait (v0.1 walking skeleton, story #3; v0.3 paint
  vocabulary, story #13; resolved subtree clips, story #97).
- [dashscene-core-arena.md](dashscene-core-arena.md) — the arena's intent
  model, staged mutation, commit resolution pipeline, and committed output
  (story #2); text content and style as intent, v0.5 (story #26); clip and
  corner intent, and commit-time clip resolution (story #97).
- [dashscene-engine.md](dashscene-engine.md) — the Taffy solve behind the
  `LayoutSolver` seam: per-root trees, the axis-relative style mapping
  (story #9).
- [dashbuf.md](dashbuf.md) — boundary A: the `.dsb` document schema (v0.1
  walking skeleton; v0.3 paint vocabulary, story #13; v0.5 text
  vocabulary, story #26).
- [dashlang.md](dashlang.md) — the value-tree builder surface and its
  one-commit mapping onto `dashscene-core` (story #5).
- [atlas-pipeline.md](atlas-pipeline.md) — the build-time font → MSDF glyph
  atlas + metrics blob pipeline (v0.5, story #27).
- [typeset-latin.md](typeset-latin.md) — the runtime Latin text pipeline:
  shape → greedy break → baseline positioning, with a font-unit
  shaped-run cache (v0.5, story #28).
- [dashscene-skia.md](dashscene-skia.md) — the Skia CPU-raster reference
  painter, the first `Painter` implementation (story #4); the v0.3 paint
  vocabulary (story #14) and resolved subtree clips (story #97).
- [goldens.md](goldens.md) — the golden-image diff tooling and the v0.1
  walking-skeleton golden scene, the v0.1 slice's closing component
  (story #6); the v0.2 flex-vocabulary goldens, closing epic #7
  (story #11); the v0.3 paint and clip goldens (stories #14, #18, #97).
- [dashscene-validator.md](dashscene-validator.md) — the three validation
  gates, the diagnostic shape, and the v0.1–v0.3 rule set (story #15);
  producer-assembled reports (story #139).
- [dashc.md](dashc.md) — the DSB compile pipeline: the in-memory `Dsb`
  model, the deterministic `.dsb` emitter, the emission gate, and the
  `dashscene-core` load path (story #16); the Figma REST front end — the
  lowering walk, the import gate, and `compile_figma` (story #139); the
  wasm ABI the Deno importer calls it through (story #17).

See the `sdd-working-memory-lifecycle` rule.
