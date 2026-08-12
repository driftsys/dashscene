# One repository, not two: the working repo took the public name

    status   accepted
    date     2026-07-11
    revised  2026-08-11 — carried out, and not by either mechanism this record
             originally anticipated
    revised  2026-08-12 — the visibility flip is done; the stub links resolve
             unauthenticated, and rulesets became available as a side effect
    scope    repo topology — `driftsys/dashscene`,
             `driftsys/dashscene-name-reservations`

## Context

`driftsys/dashscene` already existed before development started, created
purely to reserve a family of crate names on crates.io
(`docs/decisions/crate-name-map.md`). It had three commits, one open dependabot
pull request, and doc-comment stubs for twelve crates — no implementation to
preserve. The question was whether to build the working monorepo there, or
start a second repository.

## Options

1. Repurpose `dashscene` in place as the working monorepo.
2. Leave `dashscene` untouched, reserved for a future role as the project's
   facade (docs, book, site), and develop in a new private repo.

## Choice

Option 2 at the time: development happened in `driftsys/dashscene-staging`.

**On 2026-08-11 that was carried out, and the answer was neither of the two
mechanisms this record left open.** It anticipated promoting staging's content
into `dashscene` by "a fresh push or a history merge". Instead the repositories
swapped names:

- `driftsys/dashscene` was renamed `driftsys/dashscene-name-reservations` and
  archived.
- `driftsys/dashscene-staging` was renamed `driftsys/dashscene`, keeping its
  history, its issues and its milestones.

There is one repository now. The facade role folded into it.

## Why the rename rather than a promotion

- **The issue tracker is load-bearing.** The v0 plan lives as GitHub issues,
  and the documentation references them 4,896 times across 303 of 346 Markdown
  files. A fresh push would have left every one of those pointing at a tracker
  nobody could see, and lost 501 issues and 21 milestones. A rename keeps the
  numbers, so `#598` still resolves.
- **A rename is not a migration.** Nothing is copied, so nothing can be copied
  wrongly.
- **The reservation repo is archived rather than deleted**, to keep the record
  of when the names were taken and by what. That is the whole reason, and an
  earlier draft of this record gave a different one that does not survive
  checking.

## Consequences

- **All 21 published stubs carry `repository = https://github.com/driftsys/dashscene`,
  and that is the working repository, not the archived one.** Renaming the
  working repo into that name replaced GitHub's redirect rather than following
  it, so the archived repo is not where those links lead and never will be.

  Those links returned 404 to anyone outside the organisation for as long as
  this repository was private. **They resolve now.** Checking authenticated
  shows 200 and proves nothing; `curl -I` with no credentials is the test, and
  run that way on 2026-08-12 it returns 200 for both
  `https://github.com/driftsys/dashscene` and the archived
  `driftsys/dashscene-name-reservations`. The stubs land on the project
  itself — a better destination than the reservation repo would have been.
- Working memory, decisions and archive all publish from one place. There is no
  promotion step to get wrong later, and no second history to reconcile.
- **The repository is public.** The visibility flip was the last part of this
  not yet done, and it is done: `gh repo view driftsys/dashscene --json
  visibility` returns `PUBLIC`. It made one thing possible that this record did
  not anticipate — branch protection and rulesets are free on a public
  repository, and `main` now carries a ruleset requiring a pull request and a
  green `ci` (`docs/decisions/review-before-ready-not-before-open.md`). Every
  earlier statement in this repository that branch protection needs a paid plan
  was written against the private state and is superseded.

## What the original decision got right, and what it did not

Right: keeping an early, messy history out of the repository that reserved the
public name cost nothing while the outcome was uncertain, and the crates.io
`repository` field is metadata that can be repointed at publish time — so
nothing forced development to happen under the public name.

Not right: this record framed the endgame as promoting content from one
repository into another, and left the mechanism open on the assumption that the
choice would be about commits. It was not. It was about the issue tracker,
which neither option would have preserved, and which turned out to be the most
expensive thing to lose.
