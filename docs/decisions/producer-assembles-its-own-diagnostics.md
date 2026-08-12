# A producer assembles its own diagnostics into a `Report`

    status   accepted (story #139, 2026-07-13)
    scope    crates/dashscene-validator, crates/dashc
    binds    every producer that calls the import gate; #41 (waivers key on
             the same `Report`)

## Context

`docs/decisions/validator-three-gates.md` assigns the Figma-to-`Construct`
mapping to the producer (P5: the producer owns the mapping, the validator owns
the verdict). The import gate is therefore per-construct:

    triage(construct, profile, node) -> Diagnostic

It hands back one bare `Diagnostic` at a time. But `Report::push` was
`pub(crate)`, and there was no public constructor, no `FromIterator`, and no
`From<Vec<Diagnostic>>`. So a producer could triage a construct and then have no
way to report the result — a silent drop by construction, which is exactly what
P4 forbids and exactly what the import gate exists to prevent.

This is a gap, not a deliberate constraint. The validator's own decision record
gave `dashc` the job and then gave it no container to put the answer in.

## Choice

`dashscene-validator` grows two trait implementations:

    impl FromIterator<Diagnostic> for Report
    impl Extend<Diagnostic> for Report

`Report` stays the single diagnostic container across all three gates. No new
type, no second report shape, and no public `push`: a producer collects its
findings and then assembles them, rather than accreting into a shared mutable
container.

`Extend` is what lets `dashc::compile_figma` fold the load gate's `Report` into
the import gate's, so both gates decide emission from one merged report.

## Why

- The alternative — making `Report::push` public — invites a producer to hold a
  half-built report across the whole walk, and makes the container mutable
  everywhere it is passed. `FromIterator` expresses the same need with less
  public surface.
- A separate import-side report type would give the two gates two vocabularies
  to merge, and #41's waiver machinery would then have to key on both.

## Consequences

- **`compile_figma` returns the report on the success path too**, not only on
  failure. Warnings do not block emission, so `Result<Vec<u8>, Report>` would
  discard them — the silent drop again. The signature is therefore
  `Result<(Vec<u8>, Report), CompileError>`.
- Any future producer (`dashlang`, a second importer) assembles diagnostics the
  same way, with no further validator change.
