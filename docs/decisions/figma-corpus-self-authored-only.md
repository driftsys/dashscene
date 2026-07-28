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

### The rule scopes to the Figma document, not to every byte inside it

    ruled 2026-07-28 by the repository owner, raised by issue #455

**A raster payload placed inside a self-authored fixture is governed by its
own licence, not by this rule**, provided that licence is CC0. The Figma
document stays self-authored; a CC0 image sitting in one of its image fills
is not a Figma Community asset and the licence this record routes around
does not reach it.

Nothing above is relaxed. No third-party Figma file's JSON enters `corpus/`,
and no third-party file's render enters the repo, whatever licence is
claimed for it. What is clarified is a boundary the original ruling did not
address because the question had not come up: the difference between the
_document_ and the _pixels a designer drops into it_.

**CC0 only, and CC0 specifically.** Not CC-BY, which obliges attribution
this repository would then have to carry and keep accurate. Not the
Unsplash or Pexels licences, which are similar in effect but are each a
bespoke instrument rather than CC0 — and a bespoke instrument is the exact
shape of ambiguity this record exists to avoid. Wikimedia Commons and Poly
Haven publish genuine CC0.

**Provenance is recorded or the asset does not enter.** Every such payload
carries the source URL, the licence as stated at the source, the retrieval
date, and what the asset is — in the `README.md` of the `corpus/`
subdirectory holding it, beside the payload rather than in one central
list. CC0 obliges no attribution, so this is not a licence condition — it
is this repository's own audit trail, and it is what makes the claim
checkable later rather than taken on trust.

    corrected 2026-07-29: this first read "in
    `corpus/figma-fixtures/README.md`", which assumed such a payload would
    only ever sit inside a Figma fixture. The first four to arrive do not
    (`corpus/photo/`, issue #455) — they are measured directly, the way
    `corpus/atlas/*/atlas.png` is, because the licence question is about
    the payload and not about any document around it.

**The preparation is part of the payload.** How an original was cropped,
scaled or converted is recorded beside the provenance, because it changes
what is measured: scaling a photograph down averages away the
high-frequency detail block compression is worst at, and it moves which
rung the packer selects. A payload whose preparation is unrecorded cannot
be reproduced from its source.

**Three limits CC0 does not cover**, to be confirmed per asset before it is
committed. CC0 waives copyright and neighbouring rights only: it does not
clear trademark, it does not clear rights held by third parties depicted
_in_ the work (a recognisable person needs a release; a copyrighted artwork
or, in some jurisdictions, a building carries separate rights), and it is
only as sound as the uploader's right to have applied it.

## Why

Not a legal opinion — an ambiguity being routed around rather than
resolved. Revisit (ask Figma, or a specific creator, for explicit
permission) only if a real need for a specific third-party file
appears; the default stays self-authored-only until then.

The 2026-07-28 clarification does not meet that revisit test and does not
claim to: issue #455 needs _any_ image with dense high-frequency detail, not
a particular file. It was ruled on because the boundary between a document
and its raster contents was genuinely unaddressed, and leaving it unstated
would have had each future asset re-argue it. The owner considered
self-authoring the content instead — photographing and rendering it, which
needs no amendment at all — and chose the CC0 route for the breadth of
content it reaches. That choice buys a provenance discipline as its cost,
which is why the discipline is written as a condition of entry rather than
as advice.
