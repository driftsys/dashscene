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
  (story #2).
- [dashbuf.md](dashbuf.md) — boundary A: the `.dsb` document schema (v0.1
  walking skeleton; v0.3 paint vocabulary, story #13).
- [dashlang.md](dashlang.md) — the value-tree builder surface and its
  one-commit mapping onto `dashscene-core` (story #5).

See the `sdd-working-memory-lifecycle` rule.
