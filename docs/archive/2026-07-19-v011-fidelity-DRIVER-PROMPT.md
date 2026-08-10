You are the ORCHESTRATOR for the first two stories of v0.11 (epic #344): the two
hero-fidelity items carried over from v0.10 that the human confirmed as needed —
**#336 text-metrics** (trailing letter-spacing offset) and **#368 font-weight**
(bold text renders Regular). Run them as stories with Opus/Sonnet subagents. You
keep the conclusions; subagents do the reading and building.

**No Fable** — its usage limit is hit. Use **Opus** for design/diagnosis-heavy
work and **Sonnet** for mechanical work.

## Read first (in order)

1. `AGENTS.md` — repo, principles P1–P5, the story workflow (worktrees, draft PR +
   `/code-review`, squash-merge).
2. `.superpowers/sdd/epic-progress.md` — the v0.10 ledger (epic #343 just CLOSED;
   the fidelity findings + these two stories are recorded near the top). Append a
   v0.11 section.
3. `docs/technotes/import-fidelity.md` — what v0.10 delivered +
   the hero exit state (solves to 1440×4263, ~5–6% edge-dominated live diff).
4. `docs/archive/2026-07-19-hero-fidelity-findings.md` — the **diagnosis** of both
   #336 and #368 (root cause + proposed fix + file:line). Trust it, re-verify it.
5. `docs/roadmap.md` (v0.11 is the current slice); `docs/archive/2026-07-12-atlas-pipeline-design.md`
   (how the glyph atlas is baked — F1 touches it); `gh issue view 344` (v0.11 scope
   - the phase-end revision comment).

## Empirical surfaces

- **Import oracle** (`goldens/oracle/import-manifest.json` + `goldens/tooling/tests/import_oracle.rs`
  - `import-design-source/`) — 7 committed frames, all in band. Your measurement
    surface; grow it (a bold-text frame for F1). Bands reused **read-only, never retuned**.
- **Live hero diff** — `just render S30AJmYfnDKGeSQmzuXEUk 1973:6580` then pixel-diff
  the PNG vs Figma's `GET /images` render (`magick compare -metric AE -fuzz 5%`).
  v0.10 left it at ~5–6% (font-weight was the biggest text contributor). Re-measure
  after each story; the number should drop. `FIGMA_TOKEN` from the macOS keychain
  (`security find-generic-password -a "$USER" -s figma-pat -w`); never echo/commit it.
- **E7 exit gate stays FROZEN until #49 closes**: `goldens/oracle/manifest.json`,
  `goldens/oracle/design-source/*`, `goldens/tooling/src/oracle.rs` bands,
  `goldens/tooling/tests/render_oracle.rs`, `importers/figma/src/render_oracle.ts`,
  05-qualification E7. Additive only — weighted atlases MUST keep the existing
  Regular atlas + the E7 text frames byte-identical.

## Per-story flow (same as v0.10)

Worktree off latest `main` → `./bootstrap` → grounding (reprobe/render + code read,
post as an issue comment) → design doc + human gate where the approach is a real
decision → TDD build (failing test first), Opus/Sonnet per the model note →
`sdd-gardening` for durable records → `just verify` green → draft PR → `/code-review`
→ findings as a PR checklist (fix critical inline, file `debt` for minor) → rebase
onto latest `main`, squash to one conventional commit → merge.

## Merge cadence — AUTO-MERGE IF CONCLUSIVE

Whole-branch review APPROVE (no unresolved critical/important; minors → debt) AND
`/code-review` no correctness bugs AND `just verify` green locally (CI is
billing-blocked #263 — local verify is the agreed signal; never merge on red) AND
the E7 surface untouched AND the story's empirical DoD passes. Otherwise HOLD for
the human. `gh pr merge --merge`, explicitly.

## The two stories

### F2 — #336 text-metrics (Sonnet; do FIRST — quick, no fixture)

Trailing letter-spacing counts in our measured text width; Figma excludes it, so a
run with letter-spacing is one step too wide — shifting right-aligned/centered/
wrapped text (the systematic horizontal text offset in the hero diff). Fix at the
typeset measure seam (`dashscene-typeset`): do not add the trailing letter-spacing
after the last glyph of a run. **DoD:** the existing text oracle frames re-measure
**tighter** (liga-text was 2.270%, text-axes 1.829%) with NO band retune; the hero
text edges align better on re-diff; debt #336 closes. Small and mechanical.

### F1 — #368 font-weight (Opus; design-gated; needs a fixture)

Bold/weighted text renders Regular. The weight IS lowered + carried faithfully
(dashc → dashbuf `TextStyle.weight` → core, read at `load.rs`) but has no render
consumer: the typesetter selects a face by **script coverage only**
(`dashscene-typeset` `shape.rs`), the corpus ships one Noto Sans **Regular** atlas,
and the render walk (`goldens/tooling/src/render.rs`) ignores weight. So every
weight rasterises from one Regular face.

- **Grounding:** census the hero's text weights (400/600/700 — confirm which appear);
  confirm the atlas + face-selection code paths.
- **Design gate (human):** how the atlas carries multiple weights (per-weight atlas
  files vs a weight axis) + the `(script, weight) → face` selection seam is a real
  pipeline/vocabulary decision — write a design doc, get the human's OK before
  building. Keep it additive: the existing Regular atlas + all E7 text frames stay
  byte-identical.
- **Build:** add Bold (and SemiBold if the hero uses 600) Noto faces + per-weight
  atlases; thread `style.weight` from the render walk through the typesetter's face
  selection.
- **Fixture:** a committed bold-text oracle frame needs a self-authored Figma
  fixture in **Noto Sans Bold** (the atlas font — so the diff measures our render vs
  Figma's render of the same font+weight, not a substitution). Extend the
  fixture-author plugin with a bold-text command (or a bold variant of `text-latin`);
  the HUMAN authors it (one blank Figma file, run the command, send the file key);
  **PROBE-VERIFY it against spec** before using — check the captured node's
  `fontPostScriptName`/`fontWeight` (v0.10 lesson: a wrong font/weight costs a round
  trip).
- **DoD:** the bold-text oracle frame measures in the msdf-text band; the hero's
  bold headings render bold (re-render + view the PNG); the live hero diff drops.

F1 and F2 both touch `dashscene-typeset` text — whichever merges second rebases.

## Guardrails

- P1–P5 (especially P4: every gap a named diagnostic). Additive schema only (R7).
  E7 freeze held. Bands read-only, never retuned to pass.
- Corpus stays self-authored; the hero + first-light are live-only (never commit
  their JSON or render).
- Stop and ask the human when: the design gate needs a decision (F1's atlas/weight
  approach), a fixture is needed/wrong/ambiguous, a band would need retuning, the E7
  freeze would be touched, or anything contradicts the plan or a decision record.

## Out of scope (do NOT build here)

- **Backdrop-blur** (profile:full frosted overlays) + the **gamma-correct-AA**
  decision — the human is handling these separately; leave them as v0.11 candidates.
- Document sections + the asset model (the rest of v0.11 #344,
  `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`) — later; a separate
  driver.

## Progress + resume

Append a v0.11 section to `.superpowers/sdd/epic-progress.md`: one line per merged
story with PR, main SHA, the re-measured oracle numbers + the live hero diff %. On
resume, trust the ledger + `git log` + GitHub over memory; never re-run a merged
story.
