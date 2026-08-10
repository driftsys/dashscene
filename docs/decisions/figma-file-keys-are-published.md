# The fixture corpus publishes its Figma file keys

    status   accepted
    date     2026-08-10
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

## The two populations

Sharing is recorded in each capture as `linkAccess`, and the captures split:

**Ten captured as `view`** — anyone with the link can open them:
`drop-shadow`, `effects-2025`, `inner-shadow`, `lowering-baseline`,
`lowering-hug-in-fill`, `lowering-negative-gap`, `lowering-variant-topology`,
`lowering-wrap`, `text-arabic`, `variables-bound`.

**Twenty-two captured as `inherit`** — sharing follows the containing team or
project: `backdrop-blur`, `gif-fill`, `grid-basic`, `grid-fr-overflow`,
`import-image-fill`, `import-text-axes`, `jpeg-fill`, `liga-text`, `node-fx`,
`prototype-refused`, `prototype-smart-animate`, `real-file`, `stacked-fills`,
`text-baseline`, `text-bold`, `text-latin`, `trim-demo`, `v03-paint`,
`vector-backdrop-blur`, `vector-shapes`, `xfile-consumer`, `xfile-library`.

**`inherit` does not mean private.** It means the answer lives somewhere this
repository cannot see and that can change without any commit.

## Choice

Publish the 32 keys — they are in `manifest.json` and cannot be withheld
without withholding the corpus. Keep the ten link-viewable files
link-viewable.

**This record rules on those ten only.** The other 22 are `inherit`, which is
not a state this repository can see, and each still needs the read-through the
ten had. That is a precondition of publication, tracked as issue #895 — and
because #895 carries the `debt` label, which this repository defines as
non-blocking, the blocking status is asserted here instead: **the repository
does not go public until #895 is closed.**

## Why

- **A reader who can open the source design can check the importer against it.**
  The captured JSON and the golden output only prove dashscene is
  self-consistent. The Figma file is the independent term. This is P5/R7 —
  the design file stays the source of truth — demonstrated rather than asserted.
- **The keys publish either way.** They are in `manifest.json`, which is the
  fixture corpus's index and cannot be withheld without withholding the corpus.
  Restricting the files would hide the designs while still publishing their
  identifiers, which costs the benefit above and buys nothing.
- **Each of the ten was read before this ruling** — every page, and the comments
  panel, not only the fixture frame.

## Consequences

- **Nothing else goes in the ten.** No scratch pages, no notes, no unrelated
  work, no client material. A link-viewable fixture's entire contents are
  public.

  What to look for is not hypothetical: `trim-demo` — which is one of the 22,
  not one of the ten — deliberately carries a `_`-prefixed scratch layer and a
  spec note, as `corpus/figma-fixtures/README.md` records. That is fixture
  content by design there, and it is exactly the kind of thing #895's
  read-through has to rule on before any of the 22 becomes viewable.
- **A new fixture's sharing is set explicitly**, never left to inherit.
- **The 22 `inherit` fixtures are not covered by this ruling.** Their exposure
  depends on Figma team and project settings, which are outside this repository.
  Before publication each needs the same read-through the ten had, and an
  explicit setting. Tracked as issue #895, which blocks publication.
- **`linkAccess` in a capture is a snapshot**, recorded when the fixture was
  pulled. It is evidence of what was true then, not of what is true now. Check
  Figma when the answer matters.

## Alternatives considered

- **Restrict all 32 files.** Rejected: the keys still publish, so the
  irreversible part happens anyway, and the verifiability above is lost.
- **Strip the keys from `manifest.json`.** Rejected: `manifest.json` is the
  source of truth mapping fixture to file, and the capture tool parses
  `fileKey`. Removing it would break re-capture, which is the mechanism that
  keeps the corpus honest.
