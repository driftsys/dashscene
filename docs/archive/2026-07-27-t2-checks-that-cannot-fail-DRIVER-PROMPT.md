# Driver prompt — the checks that cannot fail

    status  live. The `t2-check-has-no-teeth` tier of v0.13 (epic #362,
            19 items). Written 2026-07-27 at the close of the session
            that opened v0.13. Archive verbatim to docs/archive/ when
            the tier is burnt down, and update docs/wip/README.md's
            count.

You are burning down v0.13's **`t2-check-has-no-teeth`** tier in
`driftsys/dashscene-staging`. Read `AGENTS.md` first — its conventions override
your defaults. Then `./bootstrap`.

    gh issue list --milestone "v0.13 — pre-v1 hardening" \
      --state open --label t2-check-has-no-teeth

## What this tier is, and why it is not ordinary test-writing

v0.12 ran nine stories. **Every one had a real defect found in review**, and the
recurring kind was not a wrong calculation — it was **a check that could not
fail**. A severity assertion that never fired. A threshold that stayed green
when the thing it guarded was broken. An error-message test that only checked
the message was non-empty. Reading did not find them; breaking the code and
watching the test stay green did.

This tier is that same class, already filed. Its items are not "add a missing
test" so much as "there is an assertion here that cannot distinguish right from
wrong, and nobody noticed because the suite is green either way."

So the job is not to make the suite bigger. **It is to make it falsifiable.**

## The one discipline that matters

**Mutation-test every check, and treat a green mutation as the finding.**

Break what a check is supposed to catch. Confirm a _named_ test goes red. If it
stays green, you have found something, and the finding is more valuable than the
item you were working on. Do not patch the test until it fails — first work out
_why_ it cannot fail, because the answer is often that the fixture cannot
express the difference.

Two examples from the day this tier was created, both worth understanding before
you start:

- A new end-to-end test for `ligatures_off=true` under Arabic looked correct and
  was correct — but making the flag a no-op left it **green**. The reason is
  structural: for the corpus font, Arabic output is byte-identical either way,
  because the lam-alef ligature comes from `rlig`, which `ligatures_off` does
  not touch, and a Latin word can never reach `RunContext::Arabic` through real
  bidi. No assertion over that corpus can distinguish the settings. The test was
  not weak; the **fixture** could not express the difference. That became issue
  #499 rather than a silently-passing test.
- Removing a guard from a fix left every test green. That turned out to be
  correct — the guard was an optimisation, not a correctness gate — but it was
  only knowable by working out why, and the conclusion was then pinned by its
  own test so the guard cannot quietly become load-bearing later.

**Record every mutation and its result in the PR description, including the ones
that stayed green.** A run where all mutations go red is a weaker signal than one
where a green result is explained.

## Where the tier's items cluster

Roughly, and they suggest natural batching by territory:

- **goldens and the oracle** — #119, #180, #233, #306, #355, #409, #458, #495,
  #501. The largest group, and the one where a check that cannot fail is most
  dangerous, because a green oracle reads as evidence.
- **backdrop-blur behaviours unpinned by any test** — #406, #407, #408.
- **dashpack and dashc** — #286, #361, #455.
- **the rest** — #182, #257, #499.

Two carry more than a test:

- **#422** is a decision the repository owner already ruled on. Read the ruling
  comment on the issue: the `blur-falloff` band's 12 % number splits into the
  residual it was written for plus a separate, tighter gate chosen against the
  six measured mutations. It ships with the mutation that fails it, and the
  layer-clip-removed case (2.476 %, which no band caught) must be stated
  explicitly as caught or not caught rather than quietly finessed.
- **#257** is a new validator rule, not a test — the R4 containment check. It
  belongs to this tier because it makes a requirement checkable that currently
  cannot fail at all.

**Split by territory, not by tier.** Items in the same crate collide even in
separate worktrees. Three concurrent branches is the ceiling — review is the
throughput bottleneck, not free crates.

## The artifact permit

Several of these items touch `goldens/` and the oracle, which is Stream A's
territory (epic #475) — the only stream that may regenerate a committed
artifact.

- Default is zero movement, asserted per file with `git hash-object` against
  `origin/main`, never inferred from a green suite.
- A story that moves an artifact declares it before starting, lands alone, and
  records both measurements with the reason.
- **The permit does not travel with the item.** Discovering that you must move
  one does not grant the right to.

Tightening a band is the case to watch: it changes a recorded number without
changing a pixel, and the recorded number is the artifact.

## Standing rules, all earned the same day

- **An issue can be wrong.** Five times in one session an issue's own text was
  wrong, or its stated blocker had never been checked, and each time the code
  and the decision records were right. One issue's per-node table contradicted
  the decided trim semantics; a test written from it would have failed against
  correct code, and the tempting repair would have turned a documentation error
  into a behaviour regression. **Read the implementation and the decision record
  before treating an issue as a specification.**
- **A stated reason for deferral may never have been true.** One item sat across
  four slices on the belief that fixing it forced a golden re-baseline; no
  golden rendered the thing at all. Another was disclosed as a limit for want of
  a capture that took one plugin command to produce. If an issue explains why it
  was deferred, check that reason before honouring it.
- **Restoring a mutated fixture:** `git checkout --` cannot restore an untracked
  file, and a recently captured fixture may not be tracked yet. Verify the
  restore with `git status` and a diff — mutated fixtures nearly got committed
  this way.
- **Review inline. Do not spawn subagents to review.** And never wait on a
  notification from a command you started: run it in the foreground and read its
  exit code. Both stalls happened.
- `just verify` must exit 0. CI cannot run (billing, issue #263), so it is the
  only gate. Verify the 1-4 second no-steps failure signature rather than
  assuming it.

## Workflow

One worktree per branch, `./bootstrap` after `git worktree add`, never edit
another branch's checkout. Rebase onto `origin/main` before each PR; it moves.

**Never** `git reset --soft origin/main` — it silently reverts anything that
landed in between, and `just verify` still passes because a revert is
self-consistent. Check `git diff --name-only origin/main HEAD` before pushing.

Squash to one commit per PR. Conventional commit, scope mandatory and validated.
Amend with `--no-verify`.

Draft PR, `/code-review`, findings captured as a checklist, critical fixed and
minor filed as one `debt` issue each, ready only after review, merge with a
merge commit (`gh pr merge --merge`, never rebase).

End each PR body with `Closes #N`, one line per issue actually resolved.
**Never write "closes", "fixes" or "resolves" followed by a number anywhere else
in the body, including mid-sentence** — GitHub acts on a closing keyword
wherever it appears, and a story was closed by accident exactly that way.
Equally, do not write only `Refs #N` for an issue you did resolve: two issues
were left open the same day for that reason.

Prose everywhere in plain, literal English. No idioms.

## What finishing this tier buys

Nothing visible. No pixel moves, no number improves. What changes is that the
suite starts being able to say something — which is the precondition for every
later change being safe to make, and the reason this tier was separated from the
correctness and cleanup work rather than mixed into them.
