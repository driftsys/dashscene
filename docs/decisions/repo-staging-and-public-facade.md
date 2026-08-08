# `dashscene` stays the public facade; `dashscene-staging` is where work happens

    status   accepted
    date     2026-07-11
    scope    repo topology — `driftsys/dashscene`, `driftsys/dashscene-staging`

## Context

`driftsys/dashscene` already existed before this project's development
started, created earlier purely to reserve a family of crate names on
crates.io (`docs/decisions/crate-name-map.md`). It had 3 commits, one open
dependabot PR, and its crates were doc-comment stubs only — no real
implementation to preserve. The question was whether to build the working
monorepo directly in `dashscene`, or start a second repo.

## Options

1. Repurpose `dashscene` in place as the working monorepo.
2. Leave `dashscene` untouched, reserved for its future role as the
   project's facade (docs, book, marketing/landing site), and do all
   actual development in a new private repo.

## Choice

Option 2. `dashscene` stays public as-is. All development happens in a new
private repo, `driftsys/dashscene-staging`.

## Why

- crates.io's `repository =` field is metadata on a crate — it can point
  anywhere and be repointed at publish time, so there is no technical
  requirement that development happen in the repo the reserved names
  nominally point at.
- Keeping `dashscene` untouched avoids an early, messy commit history
  landing in what is meant to become the project's public-facing surface.

## Consequences

- When there is a real version running, `dashscene-staging`'s content
  gets promoted into `dashscene`. The exact mechanism — a fresh push or a
  history merge — is intentionally undecided until that point.
- Nothing in `dashscene-staging` is public yet. `dashscene` carries no
  working code until the promotion happens.
- **It is the `repository` every reserved name points at, and there are now
  19 of them** — the 12 originally reserved, plus 7 reserved during
  development, each verified against crates.io at the v0.17 close (story
  #796). Seventeen are this workspace's crates; `dashscore` and
  `dashscene-compose` stay parked. The names, their dates and the reason
  each was added late are in [crate-name-map.md](crate-name-map.md).
