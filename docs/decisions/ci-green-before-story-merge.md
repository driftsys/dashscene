# Decision: repair CI in its own PR before merging stories with red checks

    status   accepted
    date     2026-07-12
    scope    process — applies to every story PR; first applied to #50
    session  parallel session C

## Context

The repo's first PR (#50) surfaced three pre-existing CI defects
(issue #51): the `changes` job cannot read PR files (missing token
permission), the `convco` job 404s installing git-std on Linux, and
the aggregate `ci` job does not include the PR-only jobs in its
`needs`, so their failures do not gate anything. The repo has no
branch protection (private, free plan), so a merge was technically
possible despite the red jobs.

## Options

1. Merge the story PR anyway — the failures are provably unrelated to
   its diff, and the aggregate `ci` check passed.
2. Fold the CI fixes into the story PR.
3. Fix CI in a separate, minimal PR first; re-run the story PR's
   checks and merge only once they are actually green.

## Choice

Option 3.

## Why

- AGENTS.md's story workflow says "merge only when CI is green" — a
  literal reading, and red checks on merged history train everyone to
  ignore red checks.
- Folding infrastructure fixes into a story PR (option 2) hides them
  from review and violates the one-change-per-PR discipline the story
  workflow assumes.
- The defects block all three parallel sessions' PRs, so the fix
  belongs on `main` immediately and independently of any story.
