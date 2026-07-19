# Epic — Full import of a real public Figma file

    status   proposed roadmap (working memory)
    date     2026-07-18
    tracks   #308 (real-file import probe); #309 landed (RECTANGLE/SECTION/GROUP)
    author   Opus 4.8, grounded in the #308 probe + source-vs-spec audit +
             the empirical re-probe run after #309

## Goal and exit criteria

Take a **real, public** Figma Community file (duplicated into drafts) all the
way through `dashc` to a rendered `.dsb`, measured against Figma's own render.

- **First-light target** — an instance-free Auto Layout Playground section
  (file key `MRk9I5cYY6yJa8JhljzkBn`, root `2411:10795`). After #309 it stops
  only at text (#310). Smallest path to "a real file emits."
- **Hero target** — a Landify landing-page screen (file key
  `S30AJmYfnDKGeSQmzuXEUk`). Media-rich, component-built: needs text (#310) +
  parse robustness (#311) + component closure (#312) + long-tail.

**Exit:** the first-light target emits a `.dsb` and renders through Skia; then
the hero target renders (fully, or acceptably under a partial-emit policy —
see S0) inside the render oracle's band vs Figma `GET /images`.

## The gating fact: R6 is all-or-nothing

The lowering emits a document only if the **entire** exported subtree is
in-vocabulary. One unsupported construct anywhere refuses the whole file. So
"full import" means either (a) covering ~everything a file uses, or (b)
changing the emit policy (S0). This decision reshapes the whole epic, so it
is **first**.

## The empirical loop (the engine of this epic)

Real files reveal their blockers only when run. The loop, per target:

1. Rebuild wasm from the branch (`just wasm`).
2. `deno task import <fileKey> --root <root> -o /tmp/x.dsb` (FIGMA_TOKEN from
   the keychain: `security find-generic-password -a "$USER" -s figma-pat -w`).
3. Collect the sorted, unique `figma.unsupported` / closure diagnostics.
4. Each distinct blocker becomes (or maps to) a story. Land it, re-probe.

S0b formalizes this as a repeatable harness so every wave re-derives the
frontier instead of guessing.

## Stories

| #     | Story                                      | Scope (grounded in real diagnostics)                                                                                                                                                                                                                                                                   | Code area                                                    | Depends      | Wave | Model                                            |
| ----- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------ | ------------ | ---- | ------------------------------------------------ |
| S0    | **Emit-policy decision**                   | all-or-nothing (R6) vs partial/degrade-and-diagnose emit. Brainstorm → decision record. If partial chosen, S0-impl adds a "skip-unsupported-node-with-diagnostic, still emit" mode — this shrinks the long tail from "block everything" to "diagnose and continue".                                    | design (+ `dashc`/`dashscene-validator` if partial chosen)   | —            | 0    | **Opus**                                         |
| S0b   | **Re-probe harness + target pinning**      | a `just reprobe <key> <root>` recipe (wasm rebuild + import + sorted blocker list); pin first-light + hero targets; record their current blocker lists.                                                                                                                                                | `justfile`, `importers/figma`                                | —            | 0    | Sonnet                                           |
| S1    | **#310 text vocabulary**                   | `PIXELS` line height; `letterSpacing`; `textAlignHorizontal` ≠ default; `textAlignVertical` ≠ TOP; mixed style segments (`styleOverrideTable` → per-run segments). Confirmed today as the current wall.                                                                                                | `dashscene-typeset`, `dashc/figma` text_of                   | S0           | 1    | **Opus**                                         |
| S2    | **#311 parse robustness**                  | model image `scaleMode: STRETCH` and paint `PATTERN`; add a serde catch-all so any unknown enum variant becomes a named `figma.unsupported` diagnostic instead of a hard parse crash.                                                                                                                  | `dashc/figma/rest.rs`, `triage.rs`                           | S0           | 1    | Sonnet                                           |
| S3    | **#312 component closure**                 | multi-root export, OR auto-pull a single root's local component masters into its closure, so component-built files resolve their instances. (Remote-library half is #259/#261.)                                                                                                                        | `importers/figma/closure.ts` (+ `dashc`)                     | S0           | 1    | **Opus**                                         |
| S4…Sn | **Long-tail vocabulary (re-probe-driven)** | one small story per remaining diagnostic the target hits: node rotation (#143), stacked fills/strokes (#146), dashed/non-BASIC strokes (#145), advanced blend modes, layer/backdrop blur, Fill-on-a-hug-axis, absolute-in-auto-layout, … Enumerated by the S0b harness after Wave 1, not guessed here. | `dashc/figma`, `dashpaint`                                   | S1–S3        | 2    | Sonnet (Opus if a construct needs schema design) |
| Sf    | **Render + golden + oracle**               | render the emitted `.dsb` through Skia; commit a golden; wire the target into the render oracle to measure fidelity vs Figma `GET /images` (the E7 pattern).                                                                                                                                           | `dashscene-skia`, `goldens`, `importers/figma` render oracle | target emits | 3    | Sonnet                                           |

## Sequencing and parallelism

- **Wave 0 (gate):** S0 decides the policy (changes scope for everything after);
  S0b builds the harness in parallel with S0. Do not start Wave 1 until S0's
  decision is recorded — it determines whether the long tail must be _closed_
  or merely _diagnosed-and-skipped_.
- **Wave 1 (parallel):** S1, S2, S3 touch disjoint code areas (text lowering,
  parse enums, importer closure) → three parallel worktrees, no shared state.
- **Wave 2 (parallel, re-probe-driven):** after Wave 1 merges, run the harness
  on both targets; each fresh blocker becomes a long-tail story. These are
  mostly independent → parallelizable, but the _list_ is only known after the
  re-probe. Loop until the target emits (all-or-nothing) or renders acceptably
  (partial-emit).
- **Wave 3:** Sf renders and pins the golden/oracle.

## Per-story workflow (every story)

Follow the repo's story flow, exactly as #309 (PR #317) did:

1. Worktree off latest `main` (`.claude/worktrees/<name>`), `./bootstrap`.
2. `superpowers:brainstorming` → design doc in `docs/wip/`.
3. `superpowers:writing-plans` → TDD plan in `docs/wip/`.
4. Build via `superpowers:subagent-driven-development` (implementer + task review
   - whole-branch review; fresh subagents; Opus/Sonnet per the model column).
5. `sdd-gardening` → durable records; archive raw spec/plan; `docs/wip/` clean.
6. `just verify`; push; **draft** PR; `/code-review`; capture findings as a
   checklist; fix critical, file `debt` for minor.
7. Rebase onto latest `main`, squash to one conventional commit, merge with a
   merge commit (`gh pr merge --merge`) — **confirm each merge with the human**.

## Guardrails

- **S0 first.** Do not grind the long tail before the emit-policy decision — it
  changes whether the long tail must be closed at all.
- P1–P5 hold (esp. P4: every gap a named diagnostic, never a silent drop; P1:
  no solver results in the document).
- Corpus stays self-authored (`figma-corpus-self-authored-only.md`); public
  files are used **live only**, never committed.
- This is v1-milestone work; keep the v0.9/E7 exit gate untouched. Land it as
  v1 stories.
- Confirm every outward step (push/PR/merge) with the human; never merge on red.

## Notes

- The self-authored `real-file` fixture (#265) is an alternative controlled
  target if a public file proves too long-tailed; the epic can pivot to it.
- Rough size: S1/S2/S3 are each ~one #309-sized story. The long tail is
  open-ended under all-or-nothing, and much shorter under partial-emit — which
  is the strongest argument for choosing partial-emit in S0.
