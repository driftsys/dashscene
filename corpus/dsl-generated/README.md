# DSL-generated stress corpus

Edge-case scenes the runtime must handle, authored as code against the
producer surface rather than hand-built in Figma (docs/design/architecture.md,
docs/specification/05-qualification.md E3): wrap, hug-in-fill, grid spans, baseline, variant topology
change, negative gap.

**Status:** the executable generator is story #46 (v0.8). It will be
`dashlang`-driven, which needs `dashlang`'s flex-builder vocabulary
first (see `docs/decisions/negative-gap-lowering.md` D3). Until then,
cases land here as documented entries plus an executable acceptance
test in the crate that owns the construct, and #46 turns them into
generated scenes.

## Cases

- [negative-gap.md](negative-gap.md) — negative flex gap lowered to
  child margins (story #10). Executable proof:
  `crates/dashscene-engine/tests/solve.rs`.
- [hug-in-fill.md](hug-in-fill.md) — a Hug-sized node among Fill-sized
  siblings (story #11). Executable proof:
  `goldens/tooling/tests/v02_flex.rs`.
