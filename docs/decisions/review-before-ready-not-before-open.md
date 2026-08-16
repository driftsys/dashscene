# Decision: the story-PR gate is merge — garden, open, review and build in parallel

    status   accepted
    date     2026-07-13, revised 2026-08-01, revised 2026-08-12
    scope    process — applies to every story PR
    session  marshal/coordinator lane

The filename still reads `review-before-ready-not-before-open` — the name this
decision carried between 2026-07-13 and 2026-08-01. It is kept unchanged so the
links to it from `docs/archive/` stay valid. The 2026-08-12 revision widened
this record from the draft question to the whole story-PR sequence and did not
rename the file either, for the same reason.

## Context

AGENTS.md required `/code-review` to run **before opening the PR**. That wording
was stricter than every other rule this repo inherits, and it fought the
tooling.

What the surrounding rules actually gate on:

- `superpowers:requesting-code-review` lists review as mandatory "before merge
  to main", and summarises itself as "review before merge". It never mentions
  opening a PR.
- The `sdd-working-memory-lifecycle` rule requires gardening "before opening
  **or merging**" a `main`-targeting PR — either is acceptable — and states the
  binding condition as a non-empty `docs/wip/` not being a _mergeable_ state.
  (The 2026-08-12 revision narrows both halves of that for this repository:
  gardening happens before the PR is opened, and the binding condition is what
  the branch adds rather than whether the directory is empty. See "Decision".)
- `superpowers:subagent-driven-development` runs its final review before
  `finishing-a-development-branch`, which is where PR creation happens. That
  orders review before the PR, but incidentally — as an artifact of where PR
  creation sits in that skill, not as a stated rule.

So the invariant every source shares is: **the review is complete and critical
findings are fixed before merge.** Only AGENTS.md tied it to PR creation.

Tying it there also gave up real capability. The `/code-review` skill can post
findings as inline PR comments (`--comment`) and can target a PR number directly
— both of which require the PR to exist. Reviewing a pushed PR also means
reviewing the exact artifact CI ran against, rather than a working tree that may
still move.

The failure this decision closes came from story #11's follow-up PRs. The review
was run _after_ opening a **non-draft** PR (#123), and it found a real defect:
the fix for issue #110 had made the pre-push hook fail open, silently skipping
the commit-message lint when `origin/main` could not be resolved. Nothing bad
happened, but for the window between opening and reviewing, a PR that looked
ready to merge carried an unreviewed defect — and PRs in this repo are merged
promptly.

## Why the draft step was dropped (2026-08-01)

The first version of this decision closed that window by opening every story PR
as a draft and marking it ready once the review was complete. That step is now
removed. Three things were wrong with it.

**Draft already means something else.** On GitHub, draft means "not ready for
review": reviewers are not requested, and review tooling is expected to skip the
PR. The draft step reused that state to carry a different claim — "review in
progress, do not merge". The author knew which meaning was intended. Every other
reader saw the published one.

**It blocked the review it was meant to sequence.** The `/code-review` command's
first step is to stop if the pull request is closed, is a draft, is too simple
to need review, or has already been reviewed. This repo's workflow therefore
pointed the review tool at exactly the PRs it declines. Stories worked around it
by reviewing inline and recording in the PR body why the fan-out had not been
run — a workaround for a rule that, followed literally, produced no review at
all.

**What it protected was narrower than the record claimed.** GitHub refuses to
merge a draft, so the step did stop the author merging while their own review
was still running. That is the whole of what it bought, and it guards against
the author's own haste rather than against an unreviewed merge: the author is
also the person who marks the PR ready.

No server-side replacement was available when the draft step was dropped. Both
`GET /repos/{owner}/{repo}/rulesets` and
`GET /repos/{owner}/{repo}/branches/main/protection` returned HTTP 403 —
"Upgrade to GitHub Pro or make this repository public to enable this feature" —
because the repository was private on a free plan.

**That condition has since fired.** The repository is public, both endpoints
answer, and the 2026-08-12 revision configured the ruleset described under
"Decision". It enforces the pull request and the `ci` check. It does not enforce
the review, and the reason is not the plan: GitHub does not accept a
self-approval, and `stasson` is the only collaborator and the only member of the
`driftsys` org, so a required approving review would not make merging strict —
it would stop every merge, with no second account in existence to unblock it.
The review half is still held by the findings checklist below.

## Decision

The gate is **merge**. A story PR is opened as an ordinary pull request and is
never a draft. The sequence is:

- **Garden before the PR is opened.** Working memory **this branch adds** under
  `docs/wip/` does not stay there unexplained. By the time the PR opens, every
  file the branch added is in one of three states:

  - **Gardened.** The durable record is written under `docs/specification/`,
    `docs/design/`, `docs/decisions/` or `docs/technotes/`, and the raw original
    has moved to `docs/archive/` — both halves in one commit. A record written
    while the original stays in `docs/wip/` has been copied, not gardened.
  - **Partly gardened.** The implemented half is a durable record; the file
    stays for the half not built yet, and its own `status` line says which is
    which. This is the honest state for a capture spanning several slices, and
    it is what the slice closes have actually done; several of the captures held
    today are in it.
  - **Held, with its exit condition recorded.** A capture for work that has not
    started, or a driver prompt, which is spent only when the work it carries
    lands and is then archived verbatim rather than gardened into anything. It
    stays, and `docs/wip/README.md` records the condition that empties it: a
    table row for a capture, that file's prose for a driver prompt.

  A file the branch added that is in none of the three is ungardened debt.

  **Removing a file from `docs/wip/` and updating `docs/wip/README.md` is one
  commit, not two** — its own words are "archiving a capture and updating this
  ledger are one change, not two". That ledger has gone stale in two distinct
  ways and both are worth guarding against: an archiving that never edited it at
  all, and an edit that updated one of its two copies of the count and left the
  other. Nothing but this rule asks for the pairing, and nothing at all asks for
  the second copy.

  Moving a file out of `docs/wip/` also invalidates every record that cited it
  there, and durable records do cite it: nineteen in `docs/decisions/` carry a
  `docs/wip/` path, and one has pointed at nothing since 2026-07-29 without
  anything noticing (issue #914). The commit that archives a capture re-points
  the citations it breaks — `grep -rln "docs/wip/<file>" docs/` finds them.

  All of it binds what the branch adds, and nothing else. Phrased as
  "`docs/wip/` must be empty" the rule would block every pull request opened
  against this repository, because that directory is also a standing shelf. That
  is the state the shelf is for, not a backlog. How many are held is that
  ledger's number to state, and this record deliberately does not repeat it:
  that file documents, occasion by occasion, every time a second copy of the
  count went stale, and a copy here would be one more.

- **Open the PR.** Do not pass `--draft`.

- **Run the review and CI at the same time.** `/code-review` against the PR
  while the build runs. Neither needs the other's answer, so running them in
  series adds the shorter one to the wall clock for nothing. The merge gate is
  unchanged: both must be complete.

- **Capture every finding** as a checklist in the PR description. Fix critical
  findings. File one `debt`-labeled issue per minor finding, **against a
  milestone** — the current slice, the next one, or `v1` for work not scheduled
  to a slice. Debt filed with no milestone is invisible at every slice close,
  and that is the largest population there is: measured 2026-08-12, 52 open
  `debt` issues carry no milestone against 42 in `v1`, the largest that does.
  Re-derive rather than trusting those two numbers —
  `gh issue list --label debt --state open --limit 300
  --json milestone` is
  the whole derivation.

- **Review the fix.** When a critical finding changes the implementation, the
  change that fixes it is reviewed too — a pass over what changed, not a second
  full pass.

- **Merge** only once CI is green on the commit being merged, the review pass is
  complete, and every critical finding is resolved.

The signal that a PR is not ready to merge is the findings checklist in its
description: an absent or unticked checklist means the review is unfinished.
That checklist is an artifact the review already produces, it is visible on the
PR, and unlike draft it does not tell readers the diff is not ready to read.

### What is enforced mechanically, since 2026-08-12

A ruleset on `main` with an empty bypass list, so it binds the repository admin
as well as everyone else:

    pull_request             a change reaches `main` through a pull request
      approvals required: 0  (see above — a self-approval is not accepted,
                             and there is no second account)
      allowed_merge_methods    ["merge"] since 2026-08-16 — squash and rebase
                               are refused rather than discouraged
    required_status_checks   `ci` green on the head being merged
      strict: false            since 2026-08-16 — the queue below supersedes
                               it; see the consequence at the end of this
                               section
    merge_queue              since 2026-08-16 — a branch lands through the
      merge_method: MERGE      queue, which builds `main` plus everything
      grouping: ALLGREEN       queued ahead and runs `ci` on that
      max_entries_to_build: 5
      max_entries_to_merge: 5
      min_entries_to_merge: 1
      min_entries_to_merge_wait_minutes: 5
      check_response_timeout_minutes: 60
    non_fast_forward         `main` cannot be force-pushed
    deletion                 `main` cannot be deleted

The queue's seven parameters are written out because the recovery below removes
the rule, and re-adding it without them restores GitHub's defaults rather than
this configuration.

It buys the CI half of the gate, which
`docs/decisions/ci-green-before-story-merge.md` previously held in prose alone.
Until 2026-08-16 the `strict` flag also made the rebase-before-merge step in
`AGENTS.md` a precondition rather than a convention; the queue now covers what
that flag covered, and the rebase step is a convention again.

**What an empty bypass list does and does not buy.** It stops the rules being
bypassed at the merge button, by the admin as much as anyone. It does not stop
the admin editing the ruleset: `enforcement` is a mutable field and ruleset
20731537 can be set to `disabled` or deleted in one API call, after which a
merge proceeds with nothing on the pull request recording that it happened. That
is a weaker guarantee than "cannot be declined", and this record holds itself to
the standard it applies to the `just merge` recipe under "Alternatives
considered": what it rejects there is a gate that can be stepped around
**without anyone noticing**, and turning a ruleset off is at least an act with
an audit-log entry rather than a silence.

**Direct pushes to `main` are refused, including the release path.**
`just release` is `git std bump`, which commits the version bump, the changelog
**and the tag** onto the current branch; run on `main`, the push that follows is
now rejected like any other. A release has to travel the same route as
everything else — a branch, a pull request, a green `ci` — or the ruleset has to
be lifted for the duration, deliberately and visibly. The same applies to any
hotfix.

The tag is the part that does not survive the detour. This repository's merge
shaping squashes the branch, rebases it and force-pushes — that order since
2026-08-16, and `AGENTS.md` carries the reason — so the commit `git std bump`
tagged is replaced by one with a different hash, and the tag is left pointing at
a commit unreachable from `main`. The ruleset does not catch it either: it
targets branches, so pushing that tag succeeds. A release therefore tags
**after** the merge, on the commit that actually landed, rather than on the
branch before it.

It does not upgrade what `ci` means. On a documentation-only diff every compile
and test job skips and the aggregate passes having run no test tier at all,
which is what `AGENTS.md` already says and what this rule now mechanically
requires: a green `ci`, not a suite that ran.

## Consequences

- `/code-review` runs against story PRs instead of declining them.
- Review findings can be posted as inline PR comments, anchored to the lines
  they concern, instead of only as prose in the description.
- The review target is the pushed diff CI has already run against.
- The review pass is visible in the PR's timeline rather than only inside an
  agent session.
- Between opening the PR and completing the review, an open PR carries an
  unreviewed diff. This is the #123 window, reopened deliberately. What bounds
  it is that the PR is opened and reviewed in the same session, and that the
  checklist is absent or unticked for the whole window.
- **The gardened records are inside the reviewed diff.** That is the reason
  gardening moved ahead of the PR rather than staying at the merge button. Prose
  asserting what the code does not do is this repository's most common defect —
  the `docs/features.md` pull request needed four review rounds to remove 35
  factual errors, most of them written out of this repository's own design and
  specification records — so a record gardened after the review would skip the
  pass most likely to catch that, which is the one class of defect it is best at
  catching. It is not the only thing that can land after the review; that is why
  the review-the-fix rule exists. It is the one that could be moved earlier by
  changing nothing but the order.
- **A critical finding that changes the implementation can invalidate a record
  already gardened.** The record is then re-gardened, and that change is
  reviewed under the review-the-fix rule like any other. This is the accepted
  cost of the ordering above, not an argument against it: the alternative
  exempts every gardened record from review to spare the minority that a finding
  invalidates.
- The CI half of the gate is now enforced by the ruleset above. The review half
  is not. The mechanism that would enforce it directly — a required approving
  review — needs a second account, and there is not one. A required **status
  check** is a second route, and it is worth naming even though it is not
  obviously better: a check reporting whether the review ran would gate merges
  with no second account, and the ruleset already depends on one such check for
  `ci`. What it does not escape is the objection this record makes to the
  `just merge` recipe below. Something has to post that check, on this
  repository the author's own session is what would post it, and an author who
  can set it green directly is back to a gate that can be stepped around without
  anyone noticing. It is a real option rather than a recommended one. The review
  half stays held by the description's checklist and by whoever presses merge.
- **`strict: true` serialises merges when several branches are ready at once.**
  Each merge leaves every other open branch behind `main`, and each must then
  rebase and wait for a fresh `ci` before its own merge — so N ready branches
  cost N sequential CI cycles rather than one. This repository runs stories in
  parallel worktrees by design, so that is the common case rather than an edge
  one. It also interacts with the parallel-review rule above: a rebase discards
  the CI run the review was raced against, and the review does not need
  re-running for it. The flag is one field, and dropping it is the remedy if the
  serialisation costs more than the staleness it prevents.

  **That remedy was taken on 2026-08-16, together with the thing that replaces
  it.** `strict` is now `false` and the ruleset carries a `merge_queue` rule
  instead. The queue builds a temporary branch holding `main` plus everything
  queued ahead and runs `ci` on that, so the combination is tested once per
  batch rather than once per branch per rebase.

  **Why `strict` went off is a weaker claim than "the two would have fought".**
  GitHub's own documentation frames the queue as providing "the same benefits as
  Require branches to be up to date before merging, but does not require a pull
  request author to update their pull request branch" — that is supersession,
  not conflict, so leaving the flag on would most likely have been redundant
  rather than harmful. It was turned off because a redundant precondition that
  forces a rebase is exactly the serialisation this consequence describes, and
  not because the pair was measured to misbehave. Neither position was tested
  here. If the queue ever appears not to engage, turning `strict` back on is the
  first thing to try: there are community reports of a ruleset `merge_queue`
  rule engaging only once strict was enabled.

  **The run that decides a merge is the merge group's, not the pull request's.**
  A green pull request is what admits the branch to the queue; it says nothing
  about the state of `main` afterwards.

  **What can go wrong, stated accurately.** A wrong field name in `ci.yml`'s
  merge-group expressions does **not** make the check silent: the expression
  yields an empty string, `scripts/is-code-change` fails closed to `true`, and
  `ci` reports green over the wrong range. That is the likely defect and it is
  loud in the wrong direction, not quiet. The genuinely silent failures are
  narrower — the `merge_group` trigger removed, the workflow failing to parse on
  the queue's branch, or the aggregate job renamed out from under the required
  check — and only those time the queue out rather than failing it. Recovery for
  the silent class is to remove the `merge_queue` rule, restoring the seven
  parameters recorded above when it goes back, fix through an ordinary pull
  request, and re-add it. Note that `strict` is `false` throughout that window,
  so branches merging at the button during it are compiled against whatever
  `main` they were cut from.

  `AGENTS.md` carries the working procedure, and `docs/decisions/test-tiers.md`
  carries what the third event schedules.

## Alternatives considered

**Keep the draft step.** Rejected: it publishes "not ready for review" to every
reader while meaning "do not merge yet" to one of them, and `/code-review`
declines drafts, so following the rule literally produced no review.

**Keep "review before opening the PR"** — the wording this record replaced on
2026-07-13. Rejected then and still rejected: it forfeits inline comments and
the CI signal, and it reviews a working tree that can still move rather than the
pushed artifact.

**A `review-in-progress` label, applied at open and removed once the review
completes.** Rejected: it carries exactly the advisory weight the findings
checklist already carries, while adding a second piece of per-story bookkeeping
that can be forgotten independently of the first.

**A `just merge` recipe that refuses while findings are unticked.** Rejected: it
guards only the merges that go through the recipe, and the merge button stays
available next to it. It would put the gate in tooling that can be bypassed
without noticing.

**Branch protection or a ruleset requiring an approving review.** The condition
this entry deferred to has fired — the repository is public and both endpoints
answer — and the answer is narrower than the entry expected. The enforceable
rules are configured; see "What is enforced mechanically" above. A required
approving review is not among them, and the obstacle is no longer the plan:
GitHub does not accept a self-approval, and `stasson` is the only collaborator
and the only member of the `driftsys` org, so the rule would stop every merge
rather than gate one. Listing the admin as a bypass actor would restore merging
and make the requirement advisory, which is what the `just merge` entry above
rejects in the same words. Revisit when a second maintainer account exists. The
review-reporting status check named under "Consequences" needs no second
account, but inherits this section's objection to any gate the author's own
tooling reports, so it is an option rather than the answer.

**Garden at the merge button rather than before the PR is opened.** What the
`sdd-working-memory-lifecycle` rule itself permits — it says "before opening
**or** merging" — and what this record's Context section quoted approvingly in
2026-08-01. Rejected: the second half of that "or" is satisfied by gardening
after the review has already passed, so the durable records land unreviewed. The
records are the artifact this repository's review catches the most defects in.

**Require the review only after CI reports green.** Rejected: the two answer
independent questions and neither consumes the other's result, so ordering them
adds the shorter one to the wall clock and buys nothing. The merge gate already
requires both to be complete.

**Re-review the whole pull request after a critical fix.** Rejected: the cost
scales with the size of the diff rather than with the size of the change, so it
is paid most heavily where the fix is smallest and least likely to need it. A
pass over what changed is what the rule asks for.
