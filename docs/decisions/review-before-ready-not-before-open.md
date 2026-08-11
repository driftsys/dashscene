# Decision: review before merge — a story PR is never a draft

    status   accepted
    date     2026-07-13, revised 2026-08-01
    scope    process — applies to every story PR
    session  marshal/coordinator lane

The filename still reads `review-before-ready-not-before-open` — the name
this decision carried between 2026-07-13 and 2026-08-01. It is kept
unchanged so the links to it from `docs/archive/` stay valid.

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

## Why the draft step was dropped (2026-08-01)

The first version of this decision closed that window by opening every
story PR as a draft and marking it ready once the review was complete.
That step is now removed. Three things were wrong with it.

**Draft already means something else.** On GitHub, draft means "not ready
for review": reviewers are not requested, and review tooling is expected
to skip the PR. The draft step reused that state to carry a different
claim — "review in progress, do not merge". The author knew which
meaning was intended. Every other reader saw the published one.

**It blocked the review it was meant to sequence.** The `/code-review`
command's first step is to stop if the pull request is closed, is a
draft, is too simple to need review, or has already been reviewed. This
repo's workflow therefore pointed the review tool at exactly the PRs it
declines. Stories worked around it by reviewing inline and recording in
the PR body why the fan-out had not been run — a workaround for a rule
that, followed literally, produced no review at all.

**What it protected was narrower than the record claimed.** GitHub
refuses to merge a draft, so the step did stop the author merging while
their own review was still running. That is the whole of what it bought,
and it guards against the author's own haste rather than against an
unreviewed merge: the author is also the person who marks the PR ready.

No server-side replacement is available on this repository. Both
`GET /repos/{owner}/{repo}/rulesets` and
`GET /repos/{owner}/{repo}/branches/main/protection` return HTTP 403 —
"Upgrade to GitHub Pro or make this repository public to enable this
feature" — because the repository is private on a free plan. Nothing
mechanically prevents a merge here, and this record no longer implies
otherwise.

## Decision

The gate is **merge**. A story PR is opened as an ordinary pull request
and is never a draft.

- Open the PR. Do not pass `--draft`.
- Run `/code-review` against it. Capture every finding as a checklist in
  the PR description.
- Fix critical findings; file one `debt`-labeled issue per minor finding.
- Merge only once CI is green on the commit being merged, the review pass
  is complete, and every critical finding is resolved.

The signal that a PR is not ready to merge is the findings checklist in
its description: an absent or unticked checklist means the review is
unfinished. That checklist is an artifact the review already produces, it
is visible on the PR, and unlike draft it does not tell readers the diff
is not ready to read.

## Consequences

- `/code-review` runs against story PRs instead of declining them.
- Review findings can be posted as inline PR comments, anchored to the
  lines they concern, instead of only as prose in the description.
- The review target is the pushed diff CI has already run against.
- The review pass is visible in the PR's timeline rather than only inside
  an agent session.
- Between opening the PR and completing the review, an open PR carries an
  unreviewed diff. This is the #123 window, reopened deliberately. What
  bounds it is that the PR is opened and reviewed in the same session,
  and that the checklist is absent or unticked for the whole window.
- Nothing enforces the gate mechanically. It is held by the description's
  checklist and by whoever presses merge.

## Alternatives considered

**Keep the draft step.** Rejected: it publishes "not ready for review" to
every reader while meaning "do not merge yet" to one of them, and
`/code-review` declines drafts, so following the rule literally produced
no review.

**Keep "review before opening the PR"** — the wording this record
replaced on 2026-07-13. Rejected then and still rejected: it forfeits
inline comments and the CI signal, and it reviews a working tree that can
still move rather than the pushed artifact.

**A `review-in-progress` label, applied at open and removed once the
review completes.** Rejected: it carries exactly the advisory weight the
findings checklist already carries, while adding a second piece of
per-story bookkeeping that can be forgotten independently of the first.

**A `just merge` recipe that refuses while findings are unticked.**
Rejected: it guards only the merges that go through the recipe, and the
merge button stays available next to it. It would put the gate in tooling
that can be bypassed without noticing.

**Branch protection or a ruleset requiring an approving review.** The
only option that cannot be bypassed, and the right answer if it becomes
available. Unavailable today: both endpoints return HTTP 403 on this
repository's plan. Revisit if the plan changes, or when this repository goes
public — that is what makes the endpoints available on a free plan, and it is
the only step still outstanding.
