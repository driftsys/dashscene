# The fixture corpus publishes its Figma file keys

    status   accepted
    date     2026-08-10, revised 2026-08-11 — every fixture ruled viewable,
             applied, and verified by `just figma-sharing`
    scope    corpus/figma-fixtures/manifest.json; .gitleaksignore, which
             deferred this question; the public-release plan

## Context

`corpus/figma-fixtures/manifest.json` carries a Figma **file key** for every
fixture — 32 of them. A file key is the identifier in a Figma URL. It is not a
credential and grants nothing on its own, but where a file's sharing allows
link access, the key is enough to open it.

A file key **cannot be rotated**. Once published it is public permanently, and
it identifies the file for the rest of that file's life. So the question is not
whether a file is acceptable to publish today; it is whether its key is
acceptable to publish forever.

`.gitleaksignore` triages these keys as non-secrets and explicitly declines to
settle the publication question. This record settles it.

## What the captures recorded

Sharing is recorded in each capture as `linkAccess`. Ten were captured as
`view` — anyone with the link can open them — and twenty-two as `inherit`,
which is not a state this repository can see: it means sharing follows the
containing Figma team or project, and can change without any commit here.

`inherit` is therefore not a decision. It is the absence of one.

## Choice

Publish the keys — they are in `manifest.json` and cannot be withheld
without withholding the corpus.

**Every fixture is explicitly link-viewable.** Explicit, not
inherited: `inherit` is not a value but a deferral, and it defers to a Figma
project setting outside this repository — invisible to the corpus and
changeable by anyone with project admin without a commit here. Since the key
is published and cannot be rotated, what a file exposes must be answerable
from the repository.

**Every fixture was read through first** — all pages and the comments, not
only the fixture frame — by the repository owner on 2026-08-10 and 2026-08-11.
That is the irreversible half, and it is done.

**Applied and verified.** `just figma-sharing` reports every fixture in the
manifest explicitly `view`, checked against the Figma API on 2026-08-11. Issue #895 closes on that
measurement rather than on a claim.

The check was written before the setting was applied, and reported 22 fixtures
still `inherit`, then six, then none. That order matters: the ruling was
recorded as a decision, the check told the truth about the state, and the two
were only reconciled by doing the work.

## Why

- **A reader who can open the source design can check the importer against it.**
  The captured JSON and the golden output only prove dashscene is
  self-consistent. The Figma file is the independent term. This is P5/R7 —
  the design file stays the source of truth — demonstrated rather than asserted.
- **The keys publish either way.** They are in `manifest.json`, which is the
  fixture corpus's index and cannot be withheld without withholding the corpus.
  Restricting the files would hide the designs while still publishing their
  identifiers, which costs the benefit above and buys nothing.
- **All 32 were read before this ruling** — every page, and the comments panel,
  not only the fixture frame.

## Consequences

- **Nothing else goes in a fixture file.** No scratch pages, no notes, no
  unrelated work, no client material. Every fixture's entire contents are
  public, so the file is the unit of publication, not the frame.

  That rule tolerates deliberate fixture content: `trim-demo` carries a
  `_`-prefixed scratch layer and a spec note, as
  `corpus/figma-fixtures/README.md` records, and both are there on purpose.
  The rule is about what arrives by accident.
- **A new fixture's sharing is set explicitly**, never left to inherit, and it
  is read through before it is made viewable. Both, every time — the read is
  the part that cannot be undone once the key is public.
- **`linkAccess` in a capture is a snapshot**, recorded when the fixture was
  pulled. It is evidence of what was true then, not of what is true now.
  **`just figma-sharing` asks Figma** and fails on anything that is not
  explicitly `view`. Nothing here restates its result, and nothing should: the
  sharing state lives in Figma, is changeable by any project admin without a
  commit, and a paragraph asserting it goes stale silently. Run the check; do
  not quote it.

## Alternatives considered

- **Restrict every file.** Rejected: the keys still publish, so the
  irreversible part happens anyway, and the verifiability above is lost. This
  was the safer default and was considered — a private file's key opens
  nothing, so no read-through would have been needed. It was declined because
  a reader who cannot open the design cannot check the importer against it,
  which is the whole reason the corpus exists.
- **Strip the keys from `manifest.json`.** Rejected: `manifest.json` is the
  source of truth mapping fixture to file, and the capture tool parses
  `fileKey`. Removing it would break re-capture, which is the mechanism that
  keeps the corpus honest.
