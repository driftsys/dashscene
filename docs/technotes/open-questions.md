# Technote — open questions

    status   index
    date     2026-07-14
    source   docs/archive/2026-07-14-design-1-seed.md §12

The seed document raised six open questions, `Q-1` through `Q-6`, cited by
identifier across the codebase. This note exists so that a citation of, for
example, `Q-4` still resolves once `specs/` is gone. It decides nothing.

| question                                              | status                                                |
| ----------------------------------------------------- | ----------------------------------------------------- |
| Q-1 MSDF vs. per-size bitmap atlases below ~14 px     | **resolved** — `docs/decisions/q1-msdf-below-14px.md` |
| Q-2 `KHR_blend_equation_advanced` availability/perf   | open — no tracking issue                              |
| Q-3 declared state-machine layer for plugin producers | open — no tracking issue                              |
| Q-4 Taffy baseline alignment behavior                 | open — #43                                            |
| Q-5 remote producer admission scope                   | open — no tracking issue                              |
| Q-6 group-opacity RT budget value on target hardware  | open — #44                                            |

- **Q-1** — resolved for v0: MSDF-only, no per-size bitmap atlases.
  `docs/decisions/q1-msdf-below-14px.md`.
- **Q-2** — whether `KHR_blend_equation_advanced` is available and
  performant on target drivers; decides blend-mode phasing. No issue
  currently references it.
- **Q-3** — whether sandboxed plugin producers need a declared
  state-machine layer (Rive-class), or whether instantiate+bind+
  transitions is enough. No issue currently references it.
- **Q-4** — Taffy's baseline alignment behavior, a corpus test. Tracked
  by story #43, "layout fidelity — wrap, grid spans, baseline (Q-4)".
- **Q-5** — remote producer admission scope: composition+binding only,
  or raw node construction from untrusted peers. Touched by
  `docs/decisions/dsb-format-and-one-schema.md` and
  `docs/technotes/runtime-content.md`, but neither is a tracking issue.
- **Q-6** — the group-opacity render-target budget value on target
  hardware. Tracked by story #44, "masks + group opacity", which uses a
  placeholder budget in profile config until this is measured.
