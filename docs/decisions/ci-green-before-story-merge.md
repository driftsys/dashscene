# Decision: repair CI in its own PR before merging stories with red checks

    status   accepted
    date     2026-07-12
    revised  2026-08-12 — the rule is enforced by a ruleset rather than by
             prose, and the premise that it could not be has expired
    scope    process — applies to every story PR; first applied to #50
    session  parallel session C

## Context

The repo's first PR (#50) surfaced three pre-existing CI defects (issue #51):
the `changes` job cannot read PR files (missing token permission), the `convco`
job 404s installing git-std on Linux, and the aggregate `ci` job does not
include the PR-only jobs in its `needs`, so their failures do not gate anything.
The repo had no branch protection at the time — private, on a free plan — so a
merge was technically possible despite the red jobs.

## Options

1. Merge the story PR anyway — the failures are provably unrelated to its diff,
   and the aggregate `ci` check passed.
2. Fold the CI fixes into the story PR.
3. Fix CI in a separate, minimal PR first; re-run the story PR's checks and
   merge only once they are actually green.

## Choice

Option 3.

## Why

- AGENTS.md's story workflow says "merge only when CI is green" — a literal
  reading, and red checks on merged history train everyone to ignore red checks.
- Folding infrastructure fixes into a story PR (option 2) hides them from review
  and violates the one-change-per-PR discipline the story workflow assumes.
- The defects block all three parallel sessions' PRs, so the fix belongs on
  `main` immediately and independently of any story.

## The rule is now enforced (2026-08-12)

This record's Context names the reason it had to be held by prose: no branch
protection was available on a private repository on a free plan. The repository
is public, so it is available, and `main` carries a ruleset requiring a green
`ci` on the head being merged
(`docs/decisions/review-before-ready-not-before-open.md` states the whole
ruleset). Merging over a red `ci` is refused rather than discouraged, and the
bypass list is empty, so that holds for the repository admin too.

Two things this does **not** change. The rule above is still the one to follow —
a ruleset can only see the aggregate `ci` check, so repairing CI in its own
minimal PR rather than folding the fix into a story PR remains a judgement no
configuration makes for anyone. And a green `ci` still does not mean everything
ran: `calibration` and `deno` are path-filtered, and eleven jobs skip on a
documentation-only diff. **The `test` job is no longer among them** — issue
#1361 ungated it, because the suite reads records and a documentation-only diff
could take it red with CI green. `AGENTS.md` and `docs/decisions/test-tiers.md`
carry it at more length.
