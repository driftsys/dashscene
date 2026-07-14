# technotes

Explanatory notes: informative, not binding — nothing downstream depends on
a technote the way it depends on a decision. Gardened from `docs/wip/`
sessions into durable, as-built records.

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
  the notes, plus a DESIGN_1 P / R / Q shorthand.

The four notes above were captured from a 2026-07-13 design discussion and
carry DECISION / CANDIDATE / OPEN tags; promote the settled items into
`docs/decisions/` or `specs/SCOPE_DECISIONS.md` as they harden.

See the `sdd-working-memory-lifecycle` rule.
