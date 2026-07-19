You are the **orchestrator** for an epic: take a real public Figma Community file all the way through `dashc` to a rendered `.dsb`. Run it as a sequence of stories, sequentially or in parallel, dispatching Opus or Sonnet subagents per task. You keep the conclusions; subagents do the reading and building.

## Read first (in this order)

1. `AGENTS.md` — repo, principles P1–P5, and the **story workflow** (worktrees, draft PR + `/code-review`, squash-merge). Follow it exactly.
2. `docs/wip/2026-07-18-epic-full-real-file-import.md` — the epic plan: goal, targets, the R6 gating decision, the empirical loop, the story table, waves, and per-story workflow. This is your roadmap.
3. `gh pr view 317` and its branch — the just-landed #309 story (RECTANGLE/SECTION/GROUP) is the **reference precedent** for the exact per-story flow you will repeat.

## How to execute

Work **wave by wave** from the plan's "Sequencing and parallelism" section:

- **Wave 0 is a hard gate.** Run S0 (emit-policy decision: all-or-nothing vs partial/degrade-and-diagnose) to a recorded decision **before** any Wave-1 code — it changes whether the long tail must be closed or merely diagnosed. Run S0b (the `just reprobe` harness + target pinning) in parallel with S0.
- **Wave 1 is parallel.** S1 (#310 text), S2 (#311 parse), S3 (#312 closure) touch disjoint code areas — run them in **three separate worktrees** concurrently.
- **Wave 2 is re-probe-driven.** After Wave 1 merges, run the harness on both targets; turn each fresh diagnostic into a small long-tail story; parallelize the independent ones. Loop until the target emits (all-or-nothing) or renders acceptably (partial-emit).
- **Wave 3:** Sf renders + pins the golden/oracle.

## Per-story flow (repeat for every story — same as #309/#317)

1. Worktree off latest `main`, `./bootstrap`.
2. `superpowers:brainstorming` → design doc in `docs/wip/` (get human approval).
3. `superpowers:writing-plans` → TDD plan in `docs/wip/`.
4. `superpowers:subagent-driven-development` → implementer + task review + whole-branch review, fresh subagents, **model per the plan's column** (Opus for design/hard stories and the final whole-branch review; Sonnet for mechanical implementers and small reviews). Fix every finding or file `debt`.
5. `sdd-gardening` → durable records, archive raw spec/plan, `docs/wip/` clean.
6. `just verify` → push → **draft** PR → `/code-review` → capture findings as a checklist → fix critical / file `debt` for minor.
7. Rebase onto latest `main`, squash to one conventional commit (header ≤ 100 chars, `Co-Authored-By` trailer), **confirm the merge with the human**, then `gh pr merge --merge`.

## The empirical loop (drives Wave 2, and validates every wave)

Rebuild wasm from the branch (`just wasm`), then
`deno task import <fileKey> --root <root> -o /tmp/x.dsb`
(FIGMA_TOKEN from the keychain: `security find-generic-password -a "$USER" -s figma-pat -w` — never echo the token, only its length/prefix/HTTP status). Collect the sorted unique diagnostics; that list is the next stories. Targets and roots are in the plan.

## Parallelism mechanics

Independent stories run in their own worktrees with no shared state. You may fan a wave out with the `Workflow` tool (one pipeline stage per story) or by dispatching parallel `Agent` subagents — but only ever ONE implementer touching a given worktree at a time. Never run two implementers in the same worktree.

## Guardrails (do not violate)

- **S0 before any Wave-1 code.** Do not build ahead of the decision.
- P1–P5 hold. Especially P4 (every gap a named diagnostic, never a silent drop) and P1 (no solver results in the document).
- Corpus stays self-authored (`docs/decisions/figma-corpus-self-authored-only.md`); public files are used **live only**, never committed.
- This is **v1** work; leave the v0.9/E7 exit gate untouched.
- Confirm every outward step (push/PR/merge) with the human. Never merge on red CI.

## Progress + resume

Keep a ledger at `.superpowers/sdd/epic-progress.md`: one line per story on completion (`Story: merged PR #NNN`). On resume, trust the ledger + `git log` over memory; never re-run a merged story. Re-run the harness to re-derive the live frontier after any gap.

## Stop and ask the human when

- S0's decision is genuinely theirs (present the options + your recommendation).
- A merge is ready (confirm before merging).
- A story reveals a design fork the plan didn't anticipate.
- The target's long tail turns out unbounded under all-or-nothing (that is the signal to revisit S0's partial-emit option).

Begin with Wave 0. Announce the wave, then start S0 (brainstorming the emit-policy decision) and S0b (the harness) in parallel.
