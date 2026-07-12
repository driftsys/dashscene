# dashbuf: the Fill union is added; the legacy paint field stays

    status   accepted (story #13, 2026-07-12)
    scope    crates/dashbuf/schema/dashbuf.fbs

## Context

Story #13 grows the schema's paint vocabulary from one solid fill to
solids, gradients, and image fills. `Node.paint: SolidFill` already
exists on `main`, and session A's in-flight story #2 (`dashscene-core`)
reads it. R7 requires reproducible, evolvable documents; FlatBuffers'
evolution rule is append-only field ids.

## Options

1. Add a new `fill: Fill` union field (`SolidFill | Gradient |
   ImageFill`) and keep `paint` unchanged.
2. Retype or remove `paint` in place, making the union the only fill
   representation.

## Choice

Option 1: add `fill`, keep `paint`.

## Why

- Retyping or removing a field breaks every reader built against the
  current schema — including session A's story #2, being written against
  `main` right now — and violates the append-only discipline R7 relies
  on.
- Cost accepted: a solid fill has two representations until cleanup.
  The precedence rule is written in the schema comment (`fill`
  supersedes `paint` when both are present) and becomes a
  `dashscene-validator` diagnostic when profile enforcement lands.
- Cleanup condition: once the v0.1 producers stop writing `paint`
  (after the v0.1 stories close and their code moves to the union), the
  field is removed in a coordinated change — one PR touching every
  writer and reader, at a phase boundary, per the plan-revision rule in
  `AGENTS.md`.
