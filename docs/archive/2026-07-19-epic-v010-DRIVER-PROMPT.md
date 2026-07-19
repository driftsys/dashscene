You are the **orchestrator** for epic #343 (v0.10 — real-file fidelity):
close the measured real-file vocabulary gaps until the Landify hero solves
to Figma's canvas and pixel-diffs inside a declared band. Run it as a
sequence of stories, dispatching Opus or Sonnet subagents per the plan's
model column. You keep the conclusions; subagents do the reading and
building.

## Read first (in this order)

1. `AGENTS.md` — repo, principles P1–P5, story workflow (worktrees, draft
   PR + `/code-review`, squash-merge). Follow it exactly.
2. `docs/wip/2026-07-19-epic-v010-real-file-fidelity.md` — the epic plan:
   waves, per-story touchpoints, models, DoD, the user fixture schedule.
3. `docs/technotes/2026-07-19-real-file-import.md` — what the prior epic
   proved and how (the reprobe loop, partial-emit, the import oracle).
4. `gh pr view 337` — the reference precedent: the import-oracle story,
   including the TDD bug fixes it folded and its review flow.

## How to execute

- **Wave A0 first: build the six fixture-author plugin commands** (plan,
  "Fixture authoring"), then ask the user to run them — one blank Figma
  file per fixture, one session — and send the file keys. Capture with
  `just deno-capture` + `deno task import-oracle-capture` as each
  arrives, and verify every fixture against its spec by probing the file
  (the #332 story's lesson — a wrong font or a wrong byte format costs a
  round trip even when a plugin authored it).
- **Wave A parallel** (A1 #341, A2 #342 — separate worktrees). **Wave B**
  (#340) after A2 merges (dashbuf collision avoidance). **Wave C** (C1,
  C2) after their grounding passes, parallel in worktrees. **Wave D**
  closes.
- The design gates were closed up front with the human (2026-07-19; see
  the issue comments on #340/#143/#310): B1's carrier is pre-approved
  (shape-as-mask paint), rotation is deferred, #310 is demoted to v1.
  B1 still writes its design doc, elaborating WITHIN the approved
  direction — re-open the human gate only if the build contradicts it.

## Per-story flow (repeat for every story)

1. Worktree off latest `main`, `./bootstrap`.
2. Grounding (where the plan says so): reprobe + code reading; post the
   grounding as an issue comment.
3. Design doc + human approval where gated; otherwise a brief plan.
4. TDD build (failing test first — the #337 precedent), fresh subagents;
   Opus/Sonnet per the plan's column.
5. `sdd-gardening` -> durable records, `docs/wip/` clean for the story.
6. `just verify` -> push -> **draft** PR -> `/code-review` -> findings as
   a PR checklist -> fix critical, file `debt` for minor.
7. Rebase onto latest `main`, squash to one conventional commit
   (`Co-Authored-By: Claude ...` trailer), then apply the cadence below.

## Merge cadence (standing, from the human, 2026-07-19)

AUTO-MERGE if the review is conclusive: whole-branch review APPROVE (no
unresolved critical/important; minors -> debt) AND `/code-review` no
correctness bugs AND `just verify` green locally (Actions is
billing-blocked, #263 — local verify is the agreed signal; never merge on
an actual red) AND the E7 surface untouched AND the story's empirical DoD
check passes. Otherwise HOLD for the human. `gh pr merge --merge`,
explicitly.

## Guardrails (do not violate)

- **The E7 exit-gate freeze STILL HOLDS until #49 closes**:
  `goldens/oracle/manifest.json`, `goldens/oracle/design-source/*`,
  `goldens/tooling/src/oracle.rs` bands, `goldens/tooling/tests/
  render_oracle.rs`, `importers/figma/src/render_oracle.ts`,
  `05-qualification.md` E7. The import oracle
  (`import-manifest.json` + `import_oracle.rs` + `import-design-source/`)
  is the surface this epic grows. Bands are reused read-only — **never
  retuned to pass**; a frame that fails its band is a bug to fix or a
  human decision.
- P1–P5. Especially P4 (every gap a named diagnostic) and the
  partial-emit line: omission is diagnosed, **approximation stays
  refused** — no silent degrades, ever.
- Corpus stays self-authored; the two real files are live-only, never
  committed (JSON or renders).
- R7: frozen goldens untouched; additive schema changes only
  (wire-compatible, the S1 precedent).
- Widen vocabulary by exactly what is measured (the LIGA lesson): do not
  build speculative generality the census does not demand.

## Progress + resume

Ledger at `.superpowers/sdd/epic-progress.md` (append a v0.10 section):
one line per merged story with PR, main SHA, measured numbers (hero solve
size, oracle percentages), and debts filed. On resume trust ledger +
`git log` + GitHub over memory; never re-run a merged story; re-derive
the frontier with `just reprobe`.

## Stop and ask the human when

- The build contradicts a pre-approved design direction (#340 carrier).
- A fixture is needed, wrong, or ambiguous.
- A band would need retuning, an approximation seems tempting, or the E7
  freeze would be touched.
- The cadence's conclusive bar is not met.
- Anything contradicts the plan or a decision record — surface, never
  silently diverge.
