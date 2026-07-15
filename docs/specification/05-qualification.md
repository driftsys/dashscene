# Qualification

    status  as-built, gardened 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §11

A requirement with no proof is indistinguishable from a requirement with one.
This file is the chain that closes that gap:

    requirement (R1)
      → criterion  (E2)                    this file
        → case     (an RTL corpus scene)   corpus/
          → proof  (a golden test)         goldens/ or a crate test

Criteria whose slice has not landed are listed as **open**, not omitted — a
missing proof must be visible.

## v0 exit criteria

| Criterion                         | Verifies | Status                                    |
| --------------------------------- | -------- | ----------------------------------------- |
| E1 same screen authored both ways | G1       | open — v0.9 (epic #47)                    |
| E2 Arabic golden-stable           | R1       | open — v0.6 (epic #31)                    |
| E3 stress corpus green            | R2       | partial — v0.8 (epic #42, issue #46 open) |
| E4 dirty Figma file → report      | R6       | open — v0.7 (epic #36)                    |
| E5 variant switch via FLIP        | R4       | open — v0.4 (epic #19)                    |
| E6 byte-identical `.dsb`          | R7       | **met**                                   |

The file carries no version in its name. "v0 exit criteria" is a heading
inside it; v1's criteria will be a second heading, not a second file.

### E3 — partial

The stress-corpus generator itself (`dashlang`-driven, story/issue #46) has
not landed — epic #42 (v0.8 — fidelity) is open. Two of the six named cases
are already proven independently of the generator, each by a hand-written
case plus an executable test in the crate that owns the construct:

- `negative-gap` (story #10) — `crates/dashscene-engine/tests/solve.rs`.
- `hug-in-fill` (story #11) — `goldens/tooling/tests/v02_flex.rs`.

`wrap`, `grid spans`, `baseline`, and `variant topology change` have no test
yet. See `corpus/dsl-generated/README.md` for the case-by-case status.

### E6 — met

Cross-machine byte-identity is proven by the committed fixture verified in CI.
`goldens/dsb/README.md` states: "Two suites pin it, in two CI jobs that never
meet: `crates/dashc/tests/figma_lowering.rs` (the native library call) and
`importers/figma/src/wasm_test.ts` (the same compile through the wasm ABI, from
Deno). That is what makes story #17's byte-identical to dashc-native output
checkable: each side asserts against the same committed bytes, so identity is
transitive." Each suite runs in a separate CI job on separate machines (GitHub
Actions runners); `crates/dashc/tests/abi.rs:92`'s `the_fixture_compiles_to_the_golden_dsb()`
asserts that freshly compiled output matches the committed `goldens/dsb/v03-paint.dsb`.

Schema-evolution safety is a second layer: a field-id shift or reordered union
would break byte-identity for every previously emitted `.dsb` without failing
the transitive proof above, because both sides build and decode with freshly
generated bindings. `docs/decisions/dsb-frozen-fixture-r7-guard.md` (issue #64,
closed in v0.3) closes that gap with a frozen `.dsb` byte fixture decoded by
today's bindings with value assertions.

E6 was scheduled for v0.7 in the original plan; the fixture guard landed early,
as v0.3 debt (issue #64).
