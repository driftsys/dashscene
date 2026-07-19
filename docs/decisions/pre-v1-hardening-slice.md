# The v1 debt backlog splits: a pre-v1 hardening slice, feature deferrals stay v1

    status   accepted (2026-07-19 debt triage) — creates the v0.13 slice
             (milestone #14, epic #362); binds where accumulated debt is
             resolved.
    scope    docs/roadmap.md, the v0.x slice plan, the debt backlog

## Context

The 2026-07-19 debt-backlog triage swept every open `debt` issue and
re-anchored each to the slice where it next matters. A large cluster verified
as independent debt — perf and allocation micro-debt, cleanup, test-gaps, and
latent-correctness guards — with no near-term consumer, and was anchored to v1.

v1's scope is "Unity, full feature set, performance, production toolchain": a
large slice whose principal work is the Unity painter, the full feature set,
the rendering-performance pass, and the production toolchain. Debt anchored
there does not get a focused pass — it sits under those large deliverables and
never surfaces.

## Decision

The independent code-debt splits out of v1 into a dedicated pre-v1 hardening
slice, **v0.13** (milestone #14, epic #362): the items across the `dashcue`,
`dashlang`, `dashscene-core`, `dashscene-engine`, `dashscene-typeset`, paint,
goldens, and repo/importers clusters that are resolvable on their own — perf
and allocation, cleanup, test-gaps, and latent-correctness guards. They
parallelize by crate (one PR per cluster), and none is gated on a v1
deliverable.

Feature scope gated on a specific v1 consumer **stays on v1** — it is not debt
to burn down, it is work that unlocks when its consumer lands: STRING/BOOL and
smoothing binding serialization and the `Format` transform (gated on the v1
text-parity fixture #299), the cross-file and library importer maturation, the
strict waiver gate, the anti-aliased-clip decision (needs the second painter),
retained group composition (needs the v1 incremental painter), extended-Arabic
joining (needs an imported extended-Arabic charset), mixed style segments, and
the docs items that need v1 inputs — target-hardware numbers for the
measurable-requirements work, and their own v1 or v2 subject slices for the
decision-record ratifications and the open-question tracking.

The dividing line: **resolvable before v1 goes to v0.13; unlocks with a v1
consumer stays on v1.**

## Alternatives considered

- **Leave everything on v1.** Simplest, but conflates "unlocks with its
  consumer" with "resolvable debt", so the hygiene debt never gets a focused
  pass — the problem this decision exists to fix.
- **An unnumbered "pre-v1 debt" milestone outside the version sequence.** Keeps
  it off the v0.x numbering, but breaks the convention that every milestone
  maps to a roadmap slice.
- **An epic on the v1 milestone, no new slice.** Lightest, but the debt still
  reads as "v1" in milestone and board views — the drift this avoids.

## Consequences

- v0 now runs through v0.13. The slice is provisional and revised at the v0.12
  close, like v0.11 and v0.12.
- The v0.13 epic (#362) carries the full by-crate checklist and the
  out-of-scope rationale; the milestone (#14) holds the items.
- The v0 exit gate (#49) is unchanged — v0.13 is post-qualification hardening,
  not a qualification criterion.
