---
name: shipping-a-change
description: Use when opening, reviewing, pushing, or merging a pull request in this repository — the definition of done, gardening docs/wip/, the findings checklist and its three dispositions, closing-keyword hazards, the squash-then-rebase order, the two revert detectors, and the merge-queue procedure. Read before pushing a branch and again before pressing merge.
---

# Shipping a change

The definition of done for every pull request against this repository, whatever
the branch carries, and how the branch then lands on `main`.

Two rules below read differently when the PR **closes a `debt` issue**, which is
not the same as being on a `debt` branch: a debt ticket is regularly closed from
a story branch. Both say so where they do.

## Branch workflow

repository, whatever the branch carries. Two rules below read differently when
the PR **closes a `debt` issue**, which is not the same as being on a `debt`
branch: a debt ticket is regularly closed from a story branch. Both say so where
they do:

- **Garden what this branch added to `docs/wip/` first** — before the
  `just build` below, so that build covers the prose just written, and before
  the PR, so the durable records sit inside the reviewed diff. Prose asserting
  what the code does not do is this repo's most common defect, so a record
  gardened after the review would be exactly the wrong artifact to exempt.

  Three states are acceptable for a file the branch added, and
  `docs/decisions/review-before-ready-not-before-open.md` states them in full
  rather than this file restating them: **gardened** (durable record written,
  raw original moved to `docs/archive/`, one commit — a record written while the
  original stays put is a copy), **partly gardened** (the implemented half is a
  record, the file stays for the rest, its `status` line says which is which),
  or **held** with the condition that empties it recorded in
  `docs/wip/README.md` — a table row for a capture, that file's prose for a
  driver prompt. Anything else the branch added is ungardened debt.

  **Removing a file from `docs/wip/` and updating that ledger is one commit, not
  two.** It has gone stale both ways: through an archiving that never touched
  it, and through an edit that updated one of its two copies of the count and
  left the other. The same commit re-points the records that cited the file at
  its old path — nineteen records in `docs/decisions/` carry a `docs/wip/`
  citation, and one has pointed at nothing since 2026-07-29 (issue #914).

  All of it binds **what the branch adds**, not the directory — `docs/wip/` is a
  standing shelf and is not expected to be empty.
- `just build` green.
- Open the PR as an ordinary pull request — **never a draft**. Draft means "not
  ready for review", which is the opposite of why the PR was opened: reviewers
  are not requested, and `/code-review` stops without reviewing when the PR is a
  draft (`docs/decisions/review-before-ready-not-before-open.md`).
- Run `/code-review` on the PR (`--comment` posts the findings as inline PR
  comments) **while CI runs, not after it**. Neither answer depends on the
  other, so waiting for green before starting the review only adds the shorter
  of the two to the wall clock. The merge gate is unchanged: both must be
  complete. Capture every finding as a checklist in the PR description — never
  drop a finding silently.
- **Fix findings in the pull request that found them.** File one as `debt` only
  in these two cases:

  - **The fix cannot be made here** — blocked on target hardware, on a
    dependency this workspace does not have, on a v1 consumer, or on a ruling
    only the repository owner can give. Name the blocker in the issue, and add
    `owner-input` when the blocker is a ruling. Severity and cost do not enter:
    a blocked critical finding is filed like any other, or its PR could never
    merge.
  - **Not critical, over half a day, and it names no correctness defect.** When
    the PR closes a `debt` issue, file only a **nice-to-have** — a finding that
    names no defect at all. Working a debt ticket does not file debt for a
    defect, however small.

  A finding you judge **incorrect** is rejected on the checklist with the
  reasoning beside it. Everything else is fixed here, and nothing is dropped
  silently. Record fixed / rejected / filed against each checklist item; a
  ticked box alone does not say which.

  "Critical" and "over half a day" are left to judgement on purpose — defining
  them generated four rounds of contradictions. "Nice-to-have" is defined,
  because it is narrower than `superpowers:requesting-code-review`'s
  `#### Minor (Nice to Have)`, whose Minor covers small real defects that are
  fixed here. Full rule and the measurement behind it:
  `docs/decisions/review-before-ready-not-before-open.md`.
- **Give a debt ticket the design it asks for, not a stopgap.** When a missing
  dependency or a named blocker stops that, record it on the ticket and leave
  the ticket open — do not land a partial fix and file the remainder as fresh
  debt.
- **Put every filed finding on a milestone, and link it to the PR that found
  it** — the current slice, the next one, or `v1` for anything not scheduled to
  a slice. A **long** finding never goes to `v0.23` — that milestone is work
  under half a day. A **blocked** one can, since blockedness carries no cost
  condition; issues #874 and #886 are already that shape. Debt with no milestone
  is invisible at every slice close, which is why the rule exists; the record
  above carries the measurement, and re-derive with
  `gh issue list --label debt --state open
  --limit 300 --json milestone`
  rather than assuming that population is still empty.
- **Review what the review changed** — every change made after the review pass
  gets a pass of its own, over what changed rather than a second full pass. It
  is written under more time pressure than the original and lands after the pass
  that would have caught it. Widened on 2026-08-16 from "when a critical finding
  changes the implementation": now that most findings are fixed in the PR rather
  than filed, they are most of what lands late. The rebase and squash before
  merging change no content and need no pass.
- The findings checklist is what says the PR is not ready to merge: an absent or
  unticked checklist means the review is still running. **The review half is not
  enforced mechanically**: the direct mechanism is a required approving review,
  GitHub refuses self-approvals, and there is no second account to give one — so
  it is held by the checklist and by whoever presses merge. What _is_ enforced,
  since 2026-08-12, is the rest: a ruleset on `main` with an empty bypass list
  requires a pull request and a green `ci`, and refuses force-pushes and
  deletion. That also means **`main` takes no direct push at all**, so
  `just release` and any hotfix travel through a PR like everything else.
- **A closing keyword next to an issue number closes that issue — in PR prose
  and in any commit message that lands on `main`.** The keywords are `close`,
  `fix` and `resolve`, in any inflection, optionally followed by a colon. GitHub
  matches them anywhere, including mid-sentence, and **a negation is not a
  defence**: a sentence saying an issue was _not_ fixed matches exactly as well
  as one saying it was.

  Three incidents, all of them prose that meant the opposite:

  - Story #49 was closed by a docs PR discussing whoever would close it. The
    story was never built, and two shipped documents then described its
    deliverable as shipped.
  - On 2026-08-11 a commit recording that a debt had been filed **rather than**
    fixed closed it two seconds after its PR merged. The debt was silently gone
    from the milestone.
  - The same phrasing in another commit closed an issue nearly six hours before
    the work that settled it.

  Only the **first** number after the keyword is taken, so a sentence naming
  three issues closes one and leaves two — which makes the damage look arbitrary
  and easy to miss.

  **Write `Refs #N`.** It carries no keyword and cannot fire under any later
  edit, which a keyword separated from the number by a few words can. Reserve a
  closing keyword for the one issue the change actually completes, and put it on
  its own line at the end. When naming an issue mid-sentence, write "issue #N"
  or restructure.

  After merging, check `gh issue view <n> --json state` for **every** issue the
  branch's commits named, not only those in the PR body. An issue closed this
  way is reopened by hand, with a comment saying it was closed by accident and
  not fixed — otherwise the reopen reads as a reversal of someone's judgement
  rather than a correction.
- **Re-read the milestone's open issues before merging**, not only the story's
  own: `gh issue list --milestone "<slice>" --state open`. Debt filed against a
  slice in progress is often a warning about the story that is open right now.
  Issue #783 predicted that a `dashbuf::Residency` would collide with the
  existing `dashscene_gpu::Residency`, and it was filed **twelve minutes after
  story #597's PR was opened and twenty-six before it merged** — so checking at
  the start would have found nothing, and checking before the merge button would
  have saved the rename a whole extra PR cost. A slice's other sessions file
  against the work in flight, not against the work that is finished.
- Merge only when the review pass is complete, **every** finding has one of the
  three dispositions recorded against it — fixed, rejected with the reasoning,
  or filed — and CI is green on the commit being merged. Every finding rather
  than only the critical ones: under the replaced rule a minor finding was filed
  by definition and so always had one, and now it can be ticked with nothing
  behind it. A green run earlier is not a promise: a later push, or a rebase
  onto a moved `main`, can turn it red again, so check the commit you are about
  to merge.

## Merging a PR


- Shape the branch before you merge it, not at the merge button. The branch ends
  as one conventional commit, force-pushed, so the PR carries exactly one commit
  and it applies to `main` without conflict.
- **Squash first, rebase second, and let git compute both bases.** The order is
  load-bearing and it is the opposite of what this file said until 2026-08-16:

      git fetch origin
      git reset --soft "$(git merge-base HEAD origin/main)"
      git commit -m "<conventional message>"
      git rebase origin/main

  Fetch **once, first**, and then leave the ref alone. Each of the three
  commands after it derives its own base from the same snapshot, so neither can
  be aimed at the wrong commit, and the rebase and the `--stat` check below both
  see the lanes that have landed. Skipping the fetch is its own defect: the
  rebase becomes a no-op and the check below cannot see the lane it exists to
  catch. Nothing refuses it either: the ruleset's `strict` flag was what made
  this step a precondition, and since 2026-08-16 it is off because the merge
  queue covers what it covered. The queue compiles the combination, so a stale
  branch is caught — but as a red batch after the fact, not as a refusal at the
  point where it is cheap to fix.

  The old order — rebase, then squash — is safe only while `origin/main` is
  still the branch's merge base, and a fetch **between** the two steps closes
  that window silently.

  **Never name `origin/main`, or any other moving ref, as the squash base.**
  `git fetch && git reset --soft origin/main` conflates the two steps, and when
  another lane has landed in between it moves HEAD onto a commit this worktree
  has never seen — so the re-commit records that lane's landed work as a
  deletion of its own. That is not a hypothetical:

  - **PR #1037** reverted PR #1038 twenty minutes after it merged. #1038 (merge
    `b9d8451f`) touched **11** files; #1037's branch head `f3a4ab64`, whose
    single parent is that merge, reverted **10** of them. PR #1063 restored
    **7** — the code, in `dashpaint`, `dashscene-skia` and
    `dashscene-validator`. `main` was missing work four issues read as closed
    for about 90 minutes. **The other three are still reverted today** (issue
    #1168), and one of them now contradicts the code #1063 put back. Count the
    restoration against the revert, not against the crates you were thinking
    about.
  - **PR #978** did the same to PR #961. Commit `076aebaf` has one parent —
    `ea63006d`, the #961 merge — and deletes **122 lines** of `justfile` on its
    own, taking the `android-splitscreen` recipe with it. It took three passes
    (#1003) to restore.

  Both were read at the time as a merge resolving a file badly. Neither was:
  each deletion lives in a **single-parent** commit, so the merge button did not
  create it. The corruption is made by the hand-run re-parenting before the
  merge, which is why the fix is the order above and not the merge method.

  A merge is not proof against this in general — it does drop content the branch
  never edited when the branch's own history already records that deletion
  against the merge base, which `-X ours`, a conflict once resolved by keeping
  the branch's side, and a criss-cross history all produce. Run the check below
  rather than trusting the shape of the commit.

  `git rebase -i` is unavailable in the agent harness, which is why
  `reset --soft` is the squash mechanism here at all. The ordering above is what
  makes it safe without an interactive rebase.
- **`git diff origin/main...HEAD --stat` before every push — three dots, every
  time, not once.** Three dots diffs from the merge base; two dots diffs against
  the moved ref, which is what shows the phantom deletions. Count the files
  against what the PR body claims. A mismatch is the whole tell, and in both
  incidents above it was the **only** signal: `just build` passed, CI passed,
  and three `/code-review` rounds passed over the reverted state, because an
  earlier consistent tree still compiles and still passes its own tests.
- **After `main` moves, confirm the previous lane's work survived.** It runs
  after the enqueue steps further down, and it is written here rather than after
  them because it is the other half of the bullet directly above: both are
  revert detectors and neither is discoverable without the other. This is the
  step the merge-queue rules further down name as one of "the post-merge steps",
  and until 2026-08-18 it was named there and given nowhere: the command lived
  only in the per-slice lane driver prompts, which are archived with the slice
  that wrote them. It is the pre-push check's other half — that one asks what
  this branch changed, this one asks what **your merge** changed underneath it.

      git fetch origin   # without it the merge commit is not local yet
      M=$(gh pr view <your PR> --json mergeCommit --jq .mergeCommit.oid)
      git log --oneline --merges --first-parent -5 "$M^1"  # lanes before yours

      P=<a PR number that listing names>
      git -C "$(git rev-parse --show-toplevel)" diff --stat "$M^1" "$M" -- \
        $(gh api "repos/{owner}/{repo}/pulls/$P/files" --paginate --jq '.[].filename')

  **Run the second command once per lane the first names**, starting with the
  most recent. A stale squash base spanning two merges reverts both, and only
  one lane's files fall inside a single pathspec list.

  **`-5` is a window you choose, not a bound anything derives**, and widening it
  costs one diff each. Nothing in git can tell you how far back to look, because
  a squash against a moving ref is precisely what destroys the record of where
  your base was — which is the defect this check exists for. Two candidate
  bounds were measured against both incidents above and neither works:
  `git merge-base "$M^1" "$M^2"` returns `$M^1` **itself** — `ea63006d` for PR
  #978, `b9d8451f` for #1037 — so a range **starting** there and ending at
  `$M^1` is empty; and a `--since` bound off the branch tip lists nothing,
  because the tip is the squash commit, made after those lanes had landed.

  **This is verified against the two incidents above rather than reasoned
  about.** For PR #978, `$M^1` is `ea63006d`, the #961 merge, and the diff over
  #961's files reports `3 files changed, 257 deletions(-)` — including the
  122-line `justfile` deletion that took the `android-splitscreen` recipe with
  it, and that took three passes to restore.

  **Four details fail the check open if they are dropped, and each has been hit
  — two of them while writing this bullet.**

  - **`git log --merges --first-parent "$M^1"`, walking back from your merge's
    first parent, with a count.** Without `--first-parent` the window also
    returns merges made _into_ a branch rather than onto `main` — this history
    holds twelve — and each one displaces a real lane out of a fixed count. Not
    a range starting at `git merge-base "$M^1" "$M^2"`: that merge base _is_
    `$M^1` whenever the branch tip sits on `main`'s tip, which both the mandated
    pre-merge rebase and the `reset --soft origin/main` defect produce, so the
    range is empty in exactly the cases this check exists for.
  - **`"$M^1" "$M"` as the range**, which asks what your merge did and nothing
    else: `$M^1` is `main` immediately before you landed, `$M` immediately
    after. Naming one commit diffs it against your **working tree** instead —
    and from a clean checkout at the branch head, or at a pulled `main`, that is
    **empty**, which the paragraph below defines as the pass. It is the easiest
    of the four to get wrong and the quietest when you do.
  - **`git -C "$(git rev-parse --show-toplevel)"`**, because
    `git diff -- <path>` resolves pathspecs against the **current directory**:
    run from a crate directory, repo-root paths match nothing and it prints the
    empty output this bullet calls the pass. `:(top)` on each pathspec does the
    same job.
  - **`gh api --paginate`, not `gh pr view --json files`**, which caps at 100
    and does not paginate — verified on `rust-lang/rust` PR #161256, where it
    answers 100 against the API's 146. A revert past the hundredth file would
    otherwise read as a pass.

  **An empty `--stat` is the pass, and it is the only answer `--stat` gives.** A
  non-empty one is a question, not a verdict: drop `--stat` and read the diff.
  Every line in it was made by your merge, so it is a revert unless every line
  is a change your branch meant to make — a legitimate non-empty answer is
  ordinary when both branches edited the same file. Both incidents above would
  have been caught here and were not; catching it at the merge costs one diff.
  Run it after `main` carries the merge commit, not after `gh pr merge` returns.
- Keep separate commits only when they are separately meaningful — for example a
  preparatory refactor and the behavior change that builds on it, each
  independently reviewable and revertable.
- **A branch lands through the merge queue, not the merge button.** Since
  2026-08-16 ruleset 20731537 carries a `merge_queue` rule. GitHub builds a
  temporary branch holding `main` plus everything queued ahead, runs `ci` on
  **that**, and fast-forwards `main` only if it passes. The queue merges with a
  merge commit and `allowed_merge_methods` is `["merge"]`, so squash and rebase
  are refused rather than discouraged in prose, and `main` still reads as one
  change per PR.

  **Enqueue only once `ci` is green, and check what the command actually did.**

      gh pr checks <n>          # confirm green FIRST
      gh pr merge <n> --merge   # then enqueue
      gh pr view <n> --json state,mergeStateStatus

  `gh pr merge` behaves differently depending on what it finds: with the
  required checks passed it adds the pull request to the queue, and **with them
  still running it silently enables auto-merge instead** — which merges later,
  unattended, with nobody reading the findings checklist. This repository's own
  advice is to review while CI runs, so that is the normal state of a PR when
  the review finishes, and it is exactly when the wrong thing happens. Under a
  queue the `--merge` strategy flag is ignored; it is kept for the case where
  the queue is lifted.

  **`gh`'s output does not tell the two apart**, which is what the third command
  above is for. It asks for auto-merge either way and prints the same success
  line; which one happened is decided server-side by the check state at that
  moment, and `state,mergeStateStatus` is what answers it.

  **The queue runs on `allow_auto_merge`, a repository setting no ruleset
  carries.** GitHub implements "merge when ready" through auto-merge, so with it
  off every enqueue fails with `Auto merge is not allowed for this repository`.
  It is on. That also means the unattended merge above and the queue are one
  mechanism: the hazard cannot be closed by turning the setting off without
  breaking every merge on the repository. Confirming `gh pr checks` first is the
  whole remedy. `docs/decisions/review-before-ready-not-before-open.md` carries
  the measurement, the check to run, and why the recovery there leaves the
  setting alone.

  **Enqueuing is asynchronous.** The command returns before `main` has moved, so
  the post-merge steps — the accidental-closure check, and confirming the
  previous lane's work survives, both above — have nothing to look at yet and
  will pass over a merge that has not happened. Wait for `main` to carry the
  merge commit before running them. A batch that goes red, or that hits
  `check_response_timeout_minutes`, drops the pull request back out of the queue
  and leaves it open, and nothing announces that.

  **The run that decides the merge is the merge group's, not the pull
  request's.** A green pull request is what admits the branch to the queue; it
  says nothing about the state of `main` afterwards. Read the merge group's own
  run when something lands red.
- **The squash-and-rebase shaping above is still required; only its enforcement
  changed.** `strict_required_status_checks_policy` is now `false`, so nothing
  refuses a stale branch at the button, and the queue would catch it later as a
  red batch. Shaping is what keeps `main` at one commit per PR, which no ruleset
  ever enforced. Keep doing all of it — the rebase is now a convention rather
  than a precondition (`docs/decisions/review-before-ready-not-before-open.md`).
- Avoid "Rebase and merge" if the queue is ever lifted. It replays each branch
  commit onto the current `main`, so a conflict already resolved on the branch
  can come back during the replay (this is what blocked PR #108). A merge commit
  integrates the branch as-is and does not re-raise resolved conflicts.
- **A broken merge-group expression reports green over the wrong range — it does
  not go silent.** A wrong field name in `ci.yml` yields an empty string,
  `scripts/is-code-change` fails closed to `true`, and the suite runs against a
  degraded range and passes. Verified by running the script with an empty BASE.
  Only a narrower class times the queue out instead: the `merge_group` trigger
  removed, the workflow failing to parse on the queue's branch, or the aggregate
  `ci` job renamed. For that class, remove the `merge_queue` rule from ruleset
  20731537 — restoring its seven parameters from the decision record when it
  goes back, since re-adding it bare restores GitHub's defaults — fix through an
  ordinary PR, and re-add it.

## The review, in shape

The bullets above mandate a review and a findings checklist. What the review
consists of:

1. **A multi-seat pass, the seats dispatched in parallel and each given only the
   work product** — a two-sentence description, the requirements, `BASE`/`HEAD`
   and the rule-file paths. Never your session, never your reasoning, never the
   pull-request body. Handing a seat your own account of the work makes that
   account part of what gets reviewed, which is what stops the cycle
   terminating. The seats cover requirements and repository rules, whether each
   test would fail if the behaviour were wrong, and whether every statement of
   the changed behaviour matches the code.
2. **An independent bug sweep**, run from a fresh context so it inherits nothing.
3. **One refutation per finding**, scored for confidence that the finding is
   real. Findings that do not survive are recorded as dropped with the
   reasoning — never silently discarded.
4. **One consolidated ledger**, one line per finding, in the pull-request body.
   Keep it terse: a round-by-round narrative becomes the next round's subject.

Then fix, and run **one** further pass scoped to the fix diff only — not the
whole pull request. Two passes is the cap. It is a judgement about diminishing
returns, not a claim that nothing remains, so report it that way.

Dispositions: critical findings and major findings that are not heavy are fixed
here. Everything else is filed as `debt` with a milestone under the rules above.

## Clean up the worktree

Once the branch has landed:

    git -C <repo> worktree remove <path>
    git -C <repo> worktree prune

**Re-verify at the moment you delete, not when you listed.** A worktree listed as
dead minutes ago may have been re-used. And a removed worktree can leave entries
behind in the shared target directory that make unrelated tests fail with paths
naming a directory that no longer exists — if tests start failing that way after
a cleanup, that is the cause.
