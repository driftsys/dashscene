# No neutral IR above dashscene — `dashbuf` and the core arena are the two producer-neutral formats

    status   accepted
    date     2026-07-13
    source   docs/technotes/producers-and-ir.md §2
    scope    dashbuf, dashscene-core; docs/decisions/dsb-format-and-one-schema.md

## Context

Two producer-neutral representations already exist: `dashbuf` / `.dsb` (the
dashscene schema and its in-memory semantic model, producer-neutral by
design per P5), and the `dashscene-core` arena's staged-mutation API
(`open`/`set_prop`/`set_variant`/`commit` —
`docs/archive/2026-07-14-design-1-seed.md` §4 calls it "the
real contract; `.dsb` is one way to populate it").

## Choice

Do not build a third, neutral "design interchange" format above dashscene
that Figma and Penpot would both translate into.

## Why

A layer above dashscene would carry the same design intent dashscene's
layout/variant/paint tables already carry — roughly 90% schema overlap —
meaning two schemas to evolve in lockstep, two validators, a translation at
every seam, and the classic interchange-format failure mode (lossy or
bloated). It would also dilute the "one schema, file and wire" discipline
(`docs/decisions/dsb-format-and-one-schema.md`). The tell that a neutral-IR-above-dashscene is
redundant is that it would look almost exactly like dashscene.

## Consequences

- What is worth formalizing is smaller: naming and versioning the
  "canonical post-closure JSON" handoff as a driftsys-owned _input schema
  for the lowering step_, not a second IR. This seam contract is still an
  open item — `docs/technotes/producers-and-ir.md` §7.
