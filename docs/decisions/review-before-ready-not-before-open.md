# Decision: the review gate is "ready for review", not "PR opened"

    status   accepted
    date     2026-07-13
    scope    process — applies to every story PR
    session  marshal/coordinator lane

## Context

AGENTS.md required `/code-review` to run **before opening the PR**. That
wording was stricter than every other rule this repo inherits, and it
fought the tooling.

What the surrounding rules actually gate on:

- `superpowers:requesting-code-review` lists review as mandatory
  "before merge to main", and summarises itself as "review before
  merge". It never mentions opening a PR.
- The `sdd-working-memory-lifecycle` rule requires gardening "before
  opening **or merging**" a `main`-targeting PR — either is acceptable —
  and states the binding condition as a non-empty `docs/wip/` not being
  a _mergeable_ state.
- `superpowers:subagent-driven-development` runs its final review before
  `finishing-a-development-branch`, which is where PR creation happens.
  That orders review before the PR, but incidentally — as an artifact of
  where PR creation sits in that skill, not as a stated rule.

So the invariant every source shares is: **the review is complete and
critical findings are fixed before merge.** Only AGENTS.md tied it to PR
creation.

Tying it there also gave up real capability. The `/code-review` skill can
post findings as inline PR comments (`--comment`) and can target a PR
number directly — both of which require the PR to exist. Reviewing a
pushed PR also means reviewing the exact artifact CI ran against, rather
than a working tree that may still move.

The failure this decision closes came from story #11's follow-up PRs. The
review was run _after_ opening a **non-draft** PR (#123), and it found a
real defect: the fix for issue #110 had made the pre-push hook fail open,
silently skipping the commit-message lint when `origin/main` could not be
resolved. Nothing bad happened, but for the window between opening and
reviewing, a PR that looked ready to merge carried an unreviewed defect —
and PRs in this repo are merged promptly.

## Decision

The gate is **"ready for review"**, not "PR opened".

- Open the PR as a draft.
- Run `/code-review` against it. Capture every finding as a checklist in
  the PR description.
- Fix critical findings; file one `debt`-labeled issue per minor finding.
- Mark the PR ready for review only once CI is green, the review pass is
  complete, and every critical finding is resolved.

A draft PR is a work-in-progress signal. A non-draft PR is a request to
merge, and must never carry an unreviewed diff.

## Consequences

- Review findings can be posted as inline PR comments, anchored to the
  lines they concern, instead of only as prose in the description.
- The review target is the pushed diff CI has already run against.
- The review pass is visible in the PR's timeline rather than only inside
  an agent session.
- Approvers can trust that a non-draft PR has been reviewed, which is the
  same guarantee the old "before opening" wording gave, without forbidding
  a draft PR from existing first.

## Alternatives considered

**Keep "review before opening the PR".** It does guarantee the invariant,
and the miss on #123 was a failure to follow the rule rather than a flaw
in it. Rejected because it forfeits inline comments and the CI signal for
no additional safety: a draft PR cannot be merged by accident, so nothing
is protected by refusing to create one.

**Gate on merge only ("review before merging"), with no draft step.** This
is what `superpowers:requesting-code-review` says, and it is sufficient in
principle. Rejected because it relies on the merger remembering the gate at
the moment they press the button, while the PR's own state says nothing.
A PR is merged when it looks ready, so "looks ready" has to _mean_
"reviewed" — otherwise the only thing standing between an unreviewed diff
and `main` is the approver's memory. The draft step encodes the gate in the
PR's state instead of in a person's discipline, which is what makes it
worth the extra step.
