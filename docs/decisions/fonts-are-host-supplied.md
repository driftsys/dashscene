# Fonts are host-supplied; the glyph atlas is the document's

    status   accepted 2026-08-12, issue #863 (epic #833)
    affects  dashscene-web, dashscene-desktop, dashscene-ffi, dashscene-engine,
             dashbuf
    related  docs/decisions/dsb-format-and-one-schema.md
             docs/decisions/asset-model-content-addressed-blobs.md
             docs/design/host-integration.md

A `.dsb` containing text, loaded through any of the three integration crates,
is laid out as though its text nodes were empty and is drawn with no glyphs at
all. Issue #863 found it; this record settles the question it asked rather than
the code it reported, because the fix depends on the answer and the answer was
never written down.

## What the format can and cannot carry

**The glyph atlas is the document's.** `AssetKind::DistanceField` is defined in
the schema as "a baked vector's MSDF, a glyph atlas", and `GlyphAtlas` carries
`image: uint32` — an index into `Document.assets`. A document that draws text
already carries the sheet its glyphs were baked into.

**The font is not.** `AssetKind` has exactly two variants, `Image` and
`DistanceField`. There is no font payload.

**And adding one would be cheap**, which is worth saying because it makes this a
decision rather than a constraint. The schema is append-extensible by design —
"append new fields at the tail of a table, and append new enum and union members
at the tail" — and it calls an append "the R7-cheap change". A `Font = 2` at the
tail of `AssetKind` would cost almost nothing structurally.

**Nor can the document sidestep the question by carrying shaped runs.** P1 is
that the document carries intent and never results — no resolved geometry, no
glyph positions. Shaping is therefore always work the runtime does at load or
solve time, and shaping needs the font: `Typesetter::new` takes a `Font`, and
`TaffySolver::stage_text` returns nothing without one.

## The ruling

**The host supplies the font. The document supplies the atlas.**

This is a choice, not a forced move — the paragraph above is deliberate about
that. The format could carry a font for the price of an enum append. It should
not, for two reasons that outlast this slice:

- **The target shares typefaces across documents.** An embedded panel ships a
  handful of faces and many screens. A font in every document that draws text is
  the same bytes repeated per document, on the target least able to afford it,
  and the asset model is content-addressed precisely to stop that kind of
  duplication.
- **A font is a licensed artefact.** Embedding one in a distributed document is
  a redistribution question, and it is not one this format should answer on an
  integrator's behalf by making it the default path.

P1 settles the remaining escape independently: shaped runs are results, so the
document cannot carry them whatever the asset model does.

Recording the ruling matters because the shape of the fix follows from it:
constructing the solver _after_ the document is known — the ordering issue #863
identifies, where `dashlang::attach_live` takes a solver built before the
document exists — solves the atlas half and leaves text unshaped. Both halves
are needed and only one of them is an ordering problem.

## What that costs the integration crates

None of `dashscene-web`, `dashscene-desktop` or `dashscene-ffi` depends on
`dashscene-typeset` today, so none can name a `Font` to accept one. Supplying a
font therefore means a new public dependency or a re-export on three
published-shaped crates, and on the C ABI it is a versioned change. That is the
build half of issue #863 and it is deliberately not done here.

## The limitation, until it is

**A `.dsb` containing text draws no glyphs through any integration crate, and
its text nodes measure as empty leaves**, so a hug-sized text node collapses and
its siblings reflow around a box the design did not specify. The layout is wrong
rather than merely bare, which is the half that is easier to miss.

The programmatic path is unaffected and hides this: `corpus/showcase` builds its
own solver with a real `Typesetter` and atlas set, so `demo`, `demo-web` and
`demo-android` all draw text correctly. Only the document path is mute.

Each integration crate's module documentation states this, so an embedder meets
it before hitting it, and `docs/design/host-integration.md` carries it under
"Known gaps, named".

## Alternatives considered

**Add a font asset kind.** Rejected on the two arguments above, and explicitly
**not** on cost — an append to `AssetKind` is what the schema itself calls the
R7-cheap change, and an earlier draft of this record wrongly gave cost as the
reason. If a case arrives where a document genuinely must be self-contained —
one shipped alone, to a host that has nothing — this is the change to make, it
is cheap, and this record is what it revisits.

**Bake shaped runs into the document.** Rejected on P1, which is not negotiable
here: resolved glyph positions are results, and a document carrying them stops
being an intermediate representation the runtime can re-solve at another size.

**Have the runtime discover a system font.** Rejected because it makes layout
depend on what is installed, which contradicts the target-hardware rule that
layout resolves identically on every backend. A host that wants a system font
can pass one; the runtime will not go looking.
