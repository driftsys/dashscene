---
name: implementing-a-change
description: Use when writing any feature or bug fix in this repository — the worktree setup, the test-first loop and what counts as a real RED, when a mutation is required rather than optional, which test tier to run between edits, and the light review pass to run after each behaviour change instead of saving it all for the pull request. Read before the first edit, not after.
---

# Implementing a change

Stages 3 to 5 of the process in `AGENTS.md`. Stage 6 onward is the
**shipping-a-change** skill.

## Isolate first

Work in a worktree on the branch named in the story issue. Never a branch in the
primary checkout.

    git fetch origin
    git worktree add <path> -b <branch> origin/main
    cd <path> && ./bootstrap

The shell working directory resets between commands. Use absolute paths and
`git -C <worktree>` in every command — relative-path edits have landed on `main`
from inside a worktree because of this.

## The test-first loop, one behaviour at a time

Follow `superpowers:test-driven-development`. What that skill does not carry is
what counts as a real RED here.

1. **Write the failing test.**
2. **Run it and read the failure.** It must fail *for the reason the behaviour is
   missing* — not because a file is absent, a name does not resolve, a fixture
   is wrong, or a filter matched nothing. A `cargo test` name filter that matches
   nothing exits **0**; a missing target exits 101. Neither is a RED.
3. **Make it pass.** The smallest change that does it.
4. **Run the sanity tier** — `just test`, seconds. Between edits and before
   every commit.

**A bug fix is not exempt.** Reproduce the bug in a failing test first, then fix
it. A fix with no test that failed beforehand has not been shown to fix anything.

## When a mutation is required, not optional

A test pins a behaviour only if it would fail when that behaviour is wrong.
Running the suite proves the tests pass; it does not prove they would ever fail.
**Break the production code on purpose and confirm the test goes red** in these
four cases:

- **The test asserts an absence** — "nothing calls X", "no allocation here", "the
  list is empty". These pass trivially when the check itself is broken.
- **The test guards a gate, a script or a CI step.** A gate that reports success
  without reaching the thing it checks is the most common defect class recorded
  in this project's memory.
- **The change is a fix for a finding or an issue.** Mutate the *fix*, not only
  the original defect — a fix aimed at the reported instance often leaves the
  class standing.
- **The assertion is over a derived value** — a length, a count, a hash, a
  ratio. Assert the identity as well, or a swap of two elements passes.

Mutation evidence expires. If a later round changes the code, re-run the
mutation rather than citing the earlier result.

## Which tier, while you work

`just test` between edits. `just build` before pushing and before opening the
pull request — the pre-push hook runs **no** test tier, so a green push is not a
statement that any test ran. Full tier selection, the calibration schedule and
the corpus: the **project-gates** skill.

## Review each behaviour change as it lands

Do not save every review for the pull request. After each behaviour change, run
a light pass over **that change alone**, while it is still one edit old.

- **Bugs — always.** A single-change diff wants fewer, higher-confidence
  findings, so run the bug sweep at a moderate effort level rather than the
  broadest one.
- **Prose against code — whenever the change alters behaviour any document,
  record, rustdoc or comment describes.** This is the pass that earns its cost.
- **Tests — whenever the change adds or alters behaviour a test should pin.**
  The cheapest moment to find a vacuous test is before anything is built on it.

Findings here are advisory and unscored: fix what is real, discard what is not,
and continue. **No refuter and no ledger at this stage**, and this pass gets no
pass of its own over its own fixes — treating each quick pass as a review round
is how twenty of them become twenty rounds.

**This does not replace the pull-request review.** A branch that had ten quick
passes still gets the full pass in **shipping-a-change**.

## Never

- Never implement a feature or a bug fix without a test that failed first.
- Never work on a branch in the primary checkout.
- Never claim a gate passed without naming the observable it examined.
- Never state a count you did not derive in the same command.
