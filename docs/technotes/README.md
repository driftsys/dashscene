# technotes

Explanatory notes: informative, not binding — nothing downstream depends on
a technote the way it depends on a decision. Gardened from `docs/wip/`
sessions into durable, as-built records.

If a note reaches a conclusion that binds downstream work, that conclusion
belongs in `docs/decisions/`, not here — the note links to the record that
now holds it instead of restating the ruling.

Notes:

- [msdf-arabic-atlas-spike.md](msdf-arabic-atlas-spike.md) — methodology and
  findings of the #25 spike: Arabic contextual-form coverage in
  msdf-atlas-gen and the Q-1 small-size legibility check.
- [figma-rest-shapes-the-capture-pinned.md](figma-rest-shapes-the-capture-pinned.md)
  — the Figma REST field shapes the tier-1 capture settled, several of which
  contradict what the documentation suggests (story #139).
- [producers-and-ir.md](producers-and-ir.md) — where producer knowledge lives:
  the Figma export boundary, no neutral IR above dashscene, compile-path vs
  arena-path, Penpot as a candidate second producer, and the Slint
  build-vs-adopt call.
- [rendering-and-painters.md](rendering-and-painters.md) — the SDF-quad-atlas
  model and why it is fast, the tiered backends (Unity / trimmed-Skia / lean),
  the Skia trim profile, and the Unity painter internals (BRG, Burst,
  lit/unlit, AA/colour calibration).
- [runtime-content.md](runtime-content.md) — the decision rule for
  runtime-provided content: downloaded images, streamed Glance-like producers,
  Lottie triage, and the ThorVG-to-texture escape hatch.
- [glossary.md](glossary.md) — project, graphics, and tooling terms used across
  the notes, plus a principle / requirement / target-hardware-rule /
  open-question shorthand.
- [figma-plugin-api-findings.md](figma-plugin-api-findings.md) — three Figma
  Plugin API shapes found while authoring the tier-1 corpus fixtures
  programmatically (`docs/archive/2026-07-14-scope-decisions.md` §8).
- [open-questions.md](open-questions.md) — status index for
  `docs/archive/2026-07-14-design-1-seed.md` §12's `Q-1`..`Q-6`, so a
  `Q-N` citation still resolves once `specs/` is gone.
- [engineering-guardrails.md](engineering-guardrails.md) — the design-review
  and slice-sign-off checklist `G-1`..`G-23`, each anchored to the principle,
  requirement, target-hardware rule, or open question it makes falsifiable.

The four notes above were captured from a 2026-07-13 design discussion and
carry DECISION / CANDIDATE / OPEN tags. Every `DECISION` and `DECISION
direction` item now links to the `docs/decisions/` record that holds it;
`CANDIDATE` and `OPEN` items stay here until they harden into one.

See the `sdd-working-memory-lifecycle` rule.
