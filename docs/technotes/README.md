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
- [2026-07-19-real-file-import.md](2026-07-19-real-file-import.md) — how the
  project took two real public Figma files end to end through `dashc` to a
  rendered `.dsb` under partial-emit, and what remained (the full real-file
  import epic, 2026-07-18/19).
- [2026-07-19-v010-real-file-fidelity.md](2026-07-19-v010-real-file-fidelity.md)
  — what the v0.10 slice delivered (standard-ligatures-off, JPEG/GIF fills,
  baked-vector MSDF shapes, stacked fills, node opacity/mask/hidden lowering, the
  component-instance trim fix), the seven in-band import-oracle frames, and the
  Landify hero's fidelity state at the close (solves to Figma's 1440×4263 canvas,
  ~5–6 % edge-dominated live diff; #368 font weight, backdrop-blur, #336).
- [2026-07-26-v011-sections-and-assets.md](2026-07-26-v011-sections-and-assets.md)
  — what epic #344's own scope delivered (the sectioned `.dsb` envelope, the
  content-addressed asset table, the shared image gate), the one-time R7 golden
  re-baseline and how it was attributed, the container's size cost measured on
  the hero, and the live hero at the close (1.8829 % at 5 % fuzz, decomposed:
  #394 1.6222 points, #393 0.0640; sections and assets moved zero pixels).
- [2026-07-26-tolerance-band-coverage.md](2026-07-26-tolerance-band-coverage.md)
  — what the two v0.11 backdrop-blur frames measured about the render oracle's
  three tolerance bands: `blur-falloff` cannot fail on a bounded-area blur
  defect, `aa-edge` is blind to an amplitude one, and neither dominates the
  other. Informative; the decision is #422.
- [taffy-scaled-shrink-report.md](taffy-scaled-shrink-report.md) — the upstream
  report for the taffy 0.12 defect the negative-margin workarounds exist for
  (debt #269): where the two scaled-shrink expressions disagree, the minimal
  plain-taffy reproduction, the margin sweep, and the suggested fix. The
  reproduction also lives as a canary test, so a taffy upgrade that fixes the
  defect names the workarounds to retire.

The four notes above were captured from a 2026-07-13 design discussion and
carry DECISION / CANDIDATE / OPEN tags. Every `DECISION` and `DECISION
direction` item now links to the `docs/decisions/` record that holds it;
`CANDIDATE` and `OPEN` items stay here until they harden into one.

See the `sdd-working-memory-lifecycle` rule.
