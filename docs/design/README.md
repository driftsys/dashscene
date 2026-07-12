# design

Architecture: interfaces and components, gardened from `docs/wip/` sessions
into durable, as-built records.

The project's system-wide architecture still lives in `specs/DESIGN_1.md`
(stack, pipeline, document format, producers, common runtime, painters,
target-hardware rules). It'll move here, or be superseded by records written
here, as future work gardens it in. Per-component records land here directly:

- [dashpaint.md](dashpaint.md) — boundary-B value types, the paint table,
  and the `Painter` trait (story #3).
- [dashscene-core-arena.md](dashscene-core-arena.md) — the arena's intent
  model, staged mutation, commit resolution pipeline, and committed output
  (story #2).

See the `sdd-working-memory-lifecycle` rule.
