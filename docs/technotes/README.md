# technotes

Explanatory notes: informative, not binding — nothing downstream depends on a
technote the way it depends on a decision. Gardened from `docs/wip/` sessions
into durable, as-built records.

If a note reaches a conclusion that binds downstream work, that conclusion
belongs in `docs/decisions/`, not here — the note links to the record that now
holds it instead of restating the ruling.

Notes:

- [arabic-atlas-coverage.md](arabic-atlas-coverage.md) — methodology and
  findings of the #25 spike: Arabic contextual-form coverage in msdf-atlas-gen
  and the Q-1 small-size legibility check.
- [figma-rest-shapes.md](figma-rest-shapes.md) — the Figma REST field shapes the
  tier-1 capture settled, several of which contradict what the documentation
  suggests (story #139).
- [implementing-a-backend.md](implementing-a-backend.md) — how to implement a
  backend, and which of the two seams you are on: `Painter` at boundary B, or
  the instance buffer behind the lean painter. The worked example that keeps it
  honest is `goldens/tooling/tests/worked_example.rs` (story #727).
- [producers-and-ir.md](producers-and-ir.md) — where producer knowledge lives:
  the Figma export boundary, no neutral IR above dashscene, compile-path vs
  arena-path, Penpot as a candidate second producer, and the Slint
  build-vs-adopt call.
- [batch-renderer-group.md](batch-renderer-group.md) — what `BatchRendererGroup`
  is, how the Unity painter uses it, and the ordering pitfall it presents to a
  painter's-algorithm renderer: BRG does not preserve the order draw commands
  are emitted in, which is why the Unity painter drew no text in any player
  build (issue #1389). Also the three smaller pitfalls in the same API.
- [rendering-and-painters.md](rendering-and-painters.md) — the SDF-quad-atlas
  model and why it is fast, the tiered backends (Unity / trimmed-Skia / lean),
  the Skia trim profile, and the Unity painter internals (BRG, Burst, lit/unlit,
  AA/colour calibration).
- [runtime-content.md](runtime-content.md) — the decision rule for
  runtime-provided content: downloaded images, streamed Glance-like producers,
  Lottie triage, and the ThorVG-to-texture escape hatch.
- [prior-art.md](prior-art.md) — projects solving nearby problems, and what
  dashscene is built on. The entry point for how dashscene relates to its
  neighbours, and the only place a claim about one carries a pinned citation and
  a retrieval date. Where it and another note disagree, prior-art is the one to
  correct first: it makes the strongest claim, so it carries the burden.
- [glossary.md](glossary.md) — project, graphics, and tooling terms used across
  the notes, plus a principle / requirement / target-hardware-rule /
  open-question shorthand.
- [figma-plugin-api-findings.md](figma-plugin-api-findings.md) — three Figma
  Plugin API shapes found while authoring the tier-1 corpus fixtures
  programmatically (`docs/archive/2026-07-14-scope-decisions.md` §8).
- [open-questions.md](open-questions.md) — status index for
  `docs/archive/2026-07-14-design-1-seed.md` §12's `Q-1`..`Q-6`, so a `Q-N`
  citation still resolves once `specs/` is gone.
- [engineering-guardrails.md](engineering-guardrails.md) — the design-review and
  slice-sign-off checklist `G-1`..`G-23`, each anchored to the principle,
  requirement, target-hardware rule, or open question it makes falsifiable.
- [real-file-import.md](real-file-import.md) — how the project took two real
  public Figma files end to end through `dashc` to a rendered `.dsb` under
  partial-emit, and what remained (the full real-file import epic,
  2026-07-18/19).
- [import-fidelity.md](import-fidelity.md) — what the v0.10 slice delivered
  (standard-ligatures-off, JPEG/GIF fills, baked-vector MSDF shapes, stacked
  fills, node opacity/mask/hidden lowering, the component-instance trim fix),
  the seven in-band import-oracle frames, and the Landify hero's fidelity state
  at the close (solves to Figma's 1440×4263 canvas, ~5–6 % edge-dominated live
  diff; #368 font weight, backdrop-blur, #336).
- [document-sections-and-assets.md](document-sections-and-assets.md) — what epic
  #344's own scope delivered (the sectioned `.dsb` envelope, the
  content-addressed asset table, the shared image gate), the one-time R7 golden
  re-baseline and how it was attributed, the container's size cost measured on
  the hero, and the live hero at the close (1.8829 % at 5 % fuzz, decomposed:
  #394 1.6222 points, #393 0.0640; sections and assets moved zero pixels).
- [tolerance-band-coverage.md](tolerance-band-coverage.md) — what the two v0.11
  backdrop-blur frames measured about the render oracle's three tolerance bands:
  `blur-falloff` cannot fail on a bounded-area blur defect, `aa-edge` is blind
  to an amplitude one, and neither dominates the other. Informative; the
  decision is #422.
- [frame-budget.md](frame-budget.md) — the project's first frame budget, taken
  on the v0.14 showcase host: the animated cost per scene, the static case
  measured separately (it is zero frames, not a cheap frame), and a controlled
  before-and-after for issue #101 on a real scene. Informative, and explicitly
  not the target-hardware budget epic #476 waits for.
- [measured-verification.md](measured-verification.md) — the pattern behind the
  goldens, the oracles and the guards, named in six parts: the corpus and
  expectations split, the oracle triad, exactness before tolerance, two-bound
  calibration, the sensitivity guard, and kind-assigned bands with the two-axis
  gate. Also what a surviving mutant indicts, and where the same discipline
  already governs measurements that produce no image. Informative and binds
  nothing; it links to the records that do.
- [taffy-scaled-shrink.md](taffy-scaled-shrink.md) — the upstream report for the
  taffy 0.12 defect the negative-margin workarounds exist for (debt #269): where
  the two scaled-shrink expressions disagree, the minimal plain-taffy
  reproduction, the margin sweep, and the suggested fix. The reproduction also
  lives as a canary test, so a taffy upgrade that fixes the defect names the
  workarounds to retire.
- [unity-toolchain.md](unity-toolchain.md) — what it took to get a Unity build
  environment onto the development machine and prove the C ABI seam from C#: the
  editor and module versions, the two Android toolchains that do not agree, the
  `just host-lib` gap, and `ds_abi_version` answering in the editor and in an
  Android player. Informative, and as much about what is still unknown —
  packaging, the data plane, target hardware — as about what worked (story
  #1230).

Four of the notes above — `producers-and-ir.md`, `rendering-and-painters.md`,
`runtime-content.md` and `glossary.md`, named here because "the four above" has
not meant the last four entries since the list passed eight — were captured from
a 2026-07-13 design discussion and carry DECISION / CANDIDATE / OPEN tags. Every
`DECISION` item, and every `DECISION direction` one, now links to the
`docs/decisions/` record that holds it; `CANDIDATE` and `OPEN` items stay here
until they harden into one.

See the `sdd-working-memory-lifecycle` rule.

## Conventions in this directory

**Every title reads `Technote — <subject>`.** No other `docs/` directory uses a
type prefix: a decision record states its claim as its title, a design record
names its component. Technotes carry the label because a technote is
**informative and binds nothing**, and a decision record is normative and binds
downstream work — a reader arriving from a search result or a link preview
cannot otherwise tell which they are holding. The prefix says so before the
first sentence.

**A title and a filename name the subject, not the work that produced them.**
Not the slice that closed, not the spike that ran, not the issue number, and not
the date. Those belong in the note itself — in its status block where it has
one, and in its opening paragraph where it does not. Five notes here carry no
status block, so "it is in the status block" is not a safe assumption to rename
against; check the body.

Six notes were once named `YYYY-MM-DD-…`, and four titles carried a slice
number. The names were duplicating what the notes said — except where they were
not: three of the six stated their slice but not their date, so stripping the
filename removed the measurement date from the repository entirely. Those three
now carry it in their opening line. Check before renaming that the note says
what the name says.

**A note that records a measurement keeps its date in the body, and that date is
load-bearing.** A bank size, a tolerance band, a frame budget, what a slice
shipped: each was true when measured, and a measurement without its date is a
claim without a scope. Those notes stay here rather than moving to
`docs/archive/`, because they still explain something. `docs/wip/` holds working
memory and `docs/archive/` holds spent originals; neither is where a live
measurement belongs.
