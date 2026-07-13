# A frozen .dsb byte fixture is the R7 append-only guard

    status   accepted (issue #64, 2026-07-13)
    scope    crates/dashbuf — the .dsb schema and its tests

## Context

R7 (`specs/DESIGN_1.md`) requires additive schema evolution: existing
field ids keep their positions, new fields append at the tail. An edit
that violates it — inserting a field mid-table, reordering a union's
members, renumbering an enum — breaks every `.dsb` already written to
disk.

Nothing tested that. `dashbuf`'s three round-trip suites build a
document and decode it in the same process, with the same freshly
generated bindings: writer and reader move together, so a breaking
schema edit keeps them green. Verified by mutating the schema (a field
inserted after `Node.name`, the `Fill` union's members reordered):
`roundtrip.rs`, `paint_roundtrip.rs`, and `text_roundtrip.rs` all
passed.

## Options

1. Convention only — R7 stated in the schema comments and enforced in
   code review.
2. A schema-text check — parse `dashbuf.fbs` and diff its field order
   against a checked-in snapshot.
3. A frozen byte fixture — a `.dsb` written by one schema generation,
   checked in, decoded by the current bindings with value assertions.

## Choice

Option 3. `crates/dashbuf/tests/fixtures/v0_5_document.dsb` is a
committed binary document; `crates/dashbuf/tests/schema_evolution.rs`
decodes it with today's generated bindings and asserts the values back.
Its bytes are frozen: they are only rewritten under
`UPDATE_DSB_FIXTURE=1`, and only on a deliberate, reviewed
format-generation bump — the same posture, and the same shape of
environment gate, as `goldens/`' `UPDATE_GOLDENS=1` (`goldens/README.md`).

Two properties make the guard work, and both are binding on anyone
extending the fixture:

- **The bytes are not generated at test time.** A fixture rebuilt by
  the run that reads it reintroduces exactly the writer-and-reader-move-
  together blindness this record exists to remove.
- **Assertions are on values, never on "it decoded".** A field-id shift
  usually still decodes: the verifier passes and the accessor quietly
  returns another field's value, or the field's default. So every value
  in the fixture is deliberately non-default — an `Angular` gradient
  that would read back as `Linear`, a `paint_entry` of 1 that would read
  back as `NO_PAINT`, `clip = true` against a `false` default,
  `weight = 700` against a `400` default. The silent-wrong-value case is
  the one worth catching.

Option 1 is what already existed and demonstrably catches nothing.
Option 2 checks the schema's spelling rather than its meaning: it would
miss a `flatc` codegen change, and it enforces R7 against a snapshot
maintained by the same edit that breaks it.

## Consequences

- A breaking schema edit now turns `just build` red in `dashbuf`, with
  the failing assertion naming the field that moved.
- The fixture covers the constructs most exposed to an id shift: the
  four sentinel-defaulted `Node` fields (`parent`, `paint_entry`,
  `text`, `text_style`), all three `Fill` union members, `Paint.clip`,
  the legacy inline `Node.paint`, both flex tables (including an absent
  optional scalar and a negative margin), and both text pools. Fields
  added by a future slice should be added to the fixture in the same
  commit that adds them — which regenerates it, legitimately, as an
  append.
- The fixture is a single flatbuffer, matching today's `.dsb`. When the
  sectioned container lands (`dsb-sectioned-container.md`), the envelope
  needs its own frozen fixture; this one stays as the structured-section
  guard.

## Trace

- Satisfies: `specs/DESIGN_1.md` R7 (additive schema evolution); issue
  #64.
- Binds: every future edit to `crates/dashbuf/schema/dashbuf.fbs`.
- Related: `docs/design/dashbuf.md` ("Testing"),
  `docs/decisions/dsb-sectioned-container.md`,
  `docs/decisions/golden-comparison-space.md` (the `UPDATE_*` gate
  precedent).
