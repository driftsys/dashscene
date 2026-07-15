# Two producer entry paths — compile-path or arena-path, never a third

    status   accepted
    date     2026-07-13
    source   docs/technotes/producers-and-ir.md §3
    scope    dashc, dashscene-core; every future producer

## Context

`dashbuf` and the `dashscene-core` arena give producers exactly two ways
into dashscene (`docs/decisions/no-neutral-ir-above-dashscene.md`), and
picking the right one per producer is what keeps "no new format" workable.

## Choice

Every producer enters dashscene through one of two paths:

- **Offline compile path** — external JSON → `dashc` lowering → dashscene
  → `.dsb`. Figma uses this because Figma is far from CSS and needs real
  lowering.
- **In-memory arena path** — producer code → `dashscene-core`'s
  staged-mutation API → dashscene, with no serialized intermediate. The
  Rust DSL uses this ("direct arena calls, no serialization",
  `docs/design/dashlang.md`).

The question for any new producer is which of these two paths it uses,
never whether dashscene needs a new format.

## Why

A producer that is already close to dashscene's model (structurally, not
just semantically) pays nothing for a serialization step it does not
need; a producer far from dashscene's model (Figma≠CSS) needs the
lowering and validation the compile path provides. Framing every new
producer as a path choice keeps the two-format answer
(`no-neutral-ir-above-dashscene.md`) from eroding one producer at a time.

## Consequences

- Penpot is a candidate to enter via the arena path, since its layout is
  already CSS Flexbox/Grid — near-identity onto dashscene's tables. This
  stays a `CANDIDATE`, deferred to post-v0
  (`docs/technotes/producers-and-ir.md` §4), not a ratified decision.
