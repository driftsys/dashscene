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
| Q-4 Taffy baseline alignment behavior                 | **resolved** — story #43, see below                   |
| Q-5 remote producer admission scope                   | open — no tracking issue                              |
| Q-6 group-opacity RT budget value on target hardware  | **measured** — #1128, see below                       |

- **Q-1** — resolved for v0: MSDF-only, no per-size bitmap atlases.
  `docs/decisions/q1-msdf-below-14px.md`.
- **Q-2** — whether `KHR_blend_equation_advanced` is available and performant on
  target drivers; decides blend-mode phasing. No issue currently references it.
- **Q-3** — whether sandboxed plugin producers need a declared state-machine
  layer (Rive-class), or whether instantiate+bind+ transitions is enough. No
  issue currently references it.
- **Q-4** — resolved at story #43
  (`docs/decisions/v08-layout-vocabulary-shape.md` D5): Taffy computes baselines
  for flex rows only. A leaf — including a measured text leaf, since the measure
  seam carries no glyph baseline — synthesizes its baseline as its bottom edge
  (`height + margin.top`); a nested row propagates its first line's real
  baseline; in a `Vertical` container `Baseline` degrades to start alignment.
  Pinned by the mixed-size acceptance case in
  `crates/dashscene-engine/tests/solve.rs`. Glyph-true text baselines would need
  a baseline channel through the measure seam — a reported debt candidate, not
  built in v0.8.
- **Q-5** — remote producer admission scope: composition+binding only, or raw
  node construction from untrusted peers. Touched by
  `docs/decisions/dsb-format-and-one-schema.md` and
  `docs/technotes/runtime-content.md`, but neither is a tracking issue.
- **Q-6** — the group-opacity render-target budget value on target hardware.
  **Measured 2026-08-17** on a Pixel 5 (Adreno 620, a tiling GPU): one more
  mid-frame render-target switch costs **1.95 ms ± 0.29 ms** at 1920x1080,
  against 0.20-0.43 ms for the same sweep on an Apple M3.
  `docs/design/android-toolchain.md` carries the run.

  **The placeholder is unchanged, and that is the finding rather than a
  shortfall.** `RENDER_TARGET_BUDGET_PLACEHOLDER` is a **count** and what was
  measured is a **cost**; converting one to the other needs a frame budget, and
  no display geometry is pinned (#549). What the number does settle is that a
  fixed count cannot be right at any value — the affordable number of switches
  is `budget / cost` and both terms belong to the target — and that eight is not
  a conservative choice: at this cost it is about 15.6 ms, a whole 60 Hz frame.

  **This row pointed at #44 until 2026-08-17**, a closed v0.8 story that built
  the feature the budget bounds and never addressed the value, which is how the
  question stayed invisible. It is #1128 that measured it.

## Later-raised questions

Not part of the seed document's six above; indexed here as later design work
raised further standing questions worth tracking centrally.

| question                                                                          | status                                                                                                       |
| --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Should the reference painter blend blur in linear light or in sRGB-encoded space? | **resolved** 2026-07-30 — sRGB-encoded, `docs/decisions/blur-blends-in-srgb-encoded-space.md`; unblocks #412 |

- **Blur colour space** — resolved by measurement against Figma's own render,
  not by preference. The reference painter's surface carries no colour space, so
  blur averages raw sRGB-encoded channel values rather than linear light.
  Measured on the committed `backdrop-blur` frame, that is what Figma does too:
  over the frosted panel, sRGB-encoded blending sits a mean of 1.187 code points
  from Figma's export at its best-fitting sigma, against 10.363 for linear light
  at its best, and sRGB-encoded wins at every sigma from 0.20 to 0.60 · radius.
  Both blur frames already fail on a linear-light mutation, by 5.429 % and 4.866
  % against a 2 % budget. The surface stays as it is, and the same allocation
  remains what makes MSDF sampling correct
  (`docs/decisions/q1-msdf-below-14px.md`) — the two requirements agree.

  The question stood for two slices on a premise that was true when written and
  stale six days later: that no backdrop-blur frame over multi-coloured content
  existed. Story #393 committed one on 2026-07-26. Full record, including that
  account, in the decision; the original analysis is archived at
  `docs/archive/2026-07-19-color-space-blur-and-msdf.md` (#412, #474).
