# decisions

Decision records: normative, binding on downstream work, traced to what they
affect. Gardened from `docs/wip/` sessions into durable, as-built records.

The project's scope-level decision log still lives in
`specs/SCOPE_DECISIONS.md`, a living addendum to `specs/DESIGN_1.md`. It'll
move here, or be superseded by records written here, as future work gardens
it in. Per-story decisions land here directly:

- [text-track-early-start.md](text-track-early-start.md) — start the v0.5
  text/atlas track before v0.1 completes (plan sequencing, session C).
- [q1-msdf-below-14px.md](q1-msdf-below-14px.md) — MSDF-only text rendering
  in v0; resolves DESIGN_1.md Q-1; binds #27/#28/#30 and the validator's
  future text checks.
- [fixed-position-authoring.md](fixed-position-authoring.md) — authored
  parent-relative x/y on `FixedSizeLayout` (story #2); binds the `dashbuf`
  schema and the arena's resolution semantics.
- [staged-mutation-v01-scope.md](staged-mutation-v01-scope.md) — v0.1
  producer API is `open`/`set_prop`/`commit` with batched-publish staging
  (story #2); binds `dashlang` (#5) and the v0.4 variants work.
- [core-committed-output-shape.md](core-committed-output-shape.md) —
  `dashscene-core` owns its boundary-B output types; `NO_PAINT` sentinel and
  dirty-set semantics (story #2); binds the story #4 wiring.

See the `sdd-working-memory-lifecycle` rule.
