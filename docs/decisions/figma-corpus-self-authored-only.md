# `corpus/` accepts only fixtures the project itself authored

    status   accepted
    date     2026-07-11
    scope    corpus/figma-fixtures/, all future Figma fixture work
    binds    every fixture that ever enters corpus/figma-fixtures/

## Context

`docs/design/dashc.md` planned record-and-replay Figma fixtures ("no public
fixture corpus is recent enough"). That claim was re-verified before
deciding: Grid shipped at Config 2025 (GA ~May 7, 2025) as its own
auto-layout section, and the same event introduced the Figma Draw
effects (noise/texture, progressive blur, variable-width strokes) that
sit on `docs/specification/04-figma-vocabulary-profile.md`'s REJECT list. Any corpus assembled before
mid-2025 is therefore structurally missing all three coverage targets
(grid, `boundVariables` at scale, 2025 effects), and the project has no
proprietary production Figma files to draw from — public sources only.

## The licensing finding

Figma's Community Free Resource License grants rights "solely in
connection with your authorized use of the Figma Platform," prohibits
derivative works, and has no carve-out for API-based extraction.
Capturing a third-party Community file's REST JSON and committing it to
a repo as a standing test fixture is at best ambiguous under that
license — and this appears to apply even to files published by Figma's
own official Community account (same license framework, no
platform-owner exemption found).

## Decision

**Nothing enters `corpus/` that the project did not author.** Fixtures
are authored in the project's own Figma account and captured from
there; no third-party Figma file's JSON is ever committed, regardless
of its source or license terms.

Live, uncommitted validation against public files remains available
separately: an importer run against a public target reviewed live,
storing no JSON, is a different activity from committing a fixture and
is not restricted by this ruling. The tier-1/tier-2 fixture split this
produces is recorded in `corpus/figma-fixtures/README.md`.

This ruling was originally scoped to the fixture JSON in
`corpus/figma-fixtures/`. Committing Figma's _render_ of a self-authored
fixture — the `GET /images` PNG export the render oracle diffs against, in
`goldens/oracle/design-source/` — is equally in-scope and license-clean:
it is the owner exporting their own design from their own account, the
same self-authored-only basis. Only self-authored fixtures' exports are
committed; no third-party file's render enters the repo.

## Why

Not a legal opinion — an ambiguity being routed around rather than
resolved. Revisit (ask Figma, or a specific creator, for explicit
permission) only if a real need for a specific third-party file
appears; the default stays self-authored-only until then.
