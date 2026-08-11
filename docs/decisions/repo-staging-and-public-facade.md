# One repository, not two: the working repo took the public name

    status   accepted
    date     2026-07-11
    revised  2026-08-11 — carried out, and not by either mechanism this record
             originally anticipated
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
- **The reservation repo is archived rather than deleted**, because every stub
  published to crates.io carries a `repository` field pointing at it, and a
  published version's metadata cannot be changed. Deleting it would leave a
  dead link on twelve crates permanently. Archived, it stays readable and
  GitHub's rename redirect keeps those links resolving.

## Consequences

- The reserved crates.io names now point, through that redirect, at an archived
  repository whose README says where the project went. New releases carry the
  current `repository` field, so the redirect matters only for the `0.1.0`
  stubs.
- Working memory, decisions and archive all publish from one place. There is no
  promotion step to get wrong later, and no second history to reconcile.
- **The repository is still private.** The visibility flip is a separate,
  irreversible step and is the only part of this not yet done.

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
