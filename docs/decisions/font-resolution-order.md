# Decision: the font resolution order, and what happens when a family is missing

    status   accepted (issue #379, epic #344 — the repository owner's
             decision, 2026-07-25)
    scope    crates/dashscene-typeset (the cascade and its selection),
             crates/dashbuf + crates/dashscene-core (the family the
             document already carries), the render walk in goldens/tooling
    binds    #368 (the weight axis this extends), #107 and #344 (the asset
             model, where an embedded font would live), #345 (dashpack,
             where a font would be baked), #375
    related  docs/decisions/weight-selection-in-the-cascade.md,
             docs/decisions/weight-substitution-is-a-render-time-diagnostic.md,
             docs/decisions/css-fonts-4-weight-matching-non-fatal.md,
             docs/decisions/asset-model-content-addressed-blobs.md,
             docs/decisions/unsupported-figma-constructs-refuse-the-compile.md

## Context

The document carries the font family and always has. `dashbuf.fbs` declares
`family: string (required)`, commented "a family-less style has no meaning";
`dashc` lowers it verbatim; `dashscene-core` stores it as
`TextStyle::family`.

Nothing reads it. A search across `dashscene-typeset`, `dashscene-engine` and
the render walk finds no consumer: the render walk builds a Noto Sans cascade
from fixed paths, and the typesetter selects a face by script coverage. So a
document that asks for Inter renders in Noto Sans, and no code anywhere
observes that it happened.

This is the same defect #368 fixed one field over. Weight was lowered, carried
and never consumed; family is lowered, carried and never consumed. #368 built
the consumer for weight and left family untouched. It is arguably the worse of
the two, because the family name is a required field being ignored, and because
a substituted family changes letterforms, widths and metrics rather than only
stroke weight.

After #368 this is the largest remaining cause of the hero's live pixel
difference: weight substitution is gone, family substitution remains.

## Options

**Refuse the compile when the family is unavailable.** Rejected. It repeats
what #368 explicitly decided against: which fonts exist is a property of the
renderer's asset set, not of the document's intent, so recording the failure at
compile time violates P1 — one document compiled once and rendered by two
runtimes with different asset sets would carry one runtime's outcome as though
it were authored. It would also refuse every fixture currently authored in
Inter. `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` is
not a counter-argument: that decision was itself revised at S0 into a per-compile
`EmitPolicy` rather than a fixed refusal.

**Resolve from the host's installed fonts.** Rejected as a default, accepted as
an opt-in preview mode (see below). `docs/specification/05-qualification.md`
states that golden stability across machines rests on a pinned Skia, a committed
atlas, and no atlas generation at render time. A host-resolved font breaks all
three: the same document would render differently on a developer machine, on
CI, and on a target, which makes every golden image and every oracle band
meaningless. The oracle is this project's measurement instrument, and it has
caught four real defects (#272, #310, #314, #332). P4's wording is also directly
against it: vocabulary is "validated, never **discovered**", and probing a host
font store for the nearest match is discovery.

Note this does **not** breach R7. R7 is "same input, byte-identical document",
and the host's fonts do not change the document. What breaks is golden and
oracle stability, which is E5 and E7.

**Substitute within the provided cascade and name it.** Accepted. It mirrors
the shape #368 established for weight, so the two gaps have one mechanism
rather than two.

## Choice

Font resolution proceeds in this order, and the family name is load-bearing at
every step:

1. **A font embedded in the document**, when one is present. A font file is
   authored input — outlines and metrics — in the same category as the encoded
   image bytes the `.dsb` already embeds, so carrying one is consistent with P1.
   A rasterised **atlas** is the opposite and must never be embedded: it is a
   result. Nothing implements this step yet; see "Consequences".
2. **A matching family in the renderer's pinned cascade.** This is the
   reproducible default, and it is what ships and what CI measures.
3. **Substitution, reported as `text.family-substituted`** — a render-time
   diagnostic beside `text.weight-substituted`, deduplicated per distinct
   (requested family, resolved family) pair. Resolution is non-fatal, for the
   same reason weight matching is: committed fixtures request families the
   cascade does not carry, and a hard error would break their goldens.
4. **The host's installed fonts, only in an explicitly opted-in preview mode.**
   Never the default, never in CI, never in a target build, and never in a
   golden or oracle measurement. `EmitPolicy` is the precedent for a behaviour
   that is one of two policies chosen per run rather than a fixed rule.

Coverage still selects before weight within a family, so the full order is
family, then script coverage, then the CSS Fonts 4 weight step.

## Why

- It makes a required field load-bearing. A field the schema marks required and
  no code reads is a defect, independent of where fonts come from.
- It keeps the product path deterministic. Steps 1 to 3 depend only on the
  document and on a pinned, committed asset set, so a render is reproducible on
  any machine.
- It satisfies P4 without violating P1. The renderer made the substitution, so
  the renderer reports it; the document records intent and stays
  renderer-agnostic.
- It gives the host-font convenience a home without letting it reach the
  measurement path, which is where it would do real damage.

## Consequences

- **Step 1 is not implementable yet, and the blocker is the atlas, not the
  format.** `Font::from_bytes` already builds a face from bytes, and the `.dsb`
  already embeds image bytes, so the format and runtime halves are close. But
  the render path consumes an `AtlasBundle`, and the MSDF baker is an external
  pinned binary (`docs/decisions/atlas-gen-external-pinned-binary.md`), so
  nothing can turn embedded font bytes into glyphs at load time. The two exits
  are baking at pack time (#345, dashpack) or baking in process, which changes
  the pinning and reproducibility story. Embedding must not land before one of
  them exists, or documents would carry bytes no renderer can use.
- **An embedded font should ride the content-addressed asset table** (#107,
  `docs/decisions/asset-model-content-addressed-blobs.md`), which already
  contemplates "later font atlases", rather than a parallel mechanism. Schema
  growth stays additive under R7.
- **Subsetting is what makes this viable on target hardware.** A full face is
  roughly 431 KB. The atlas closure already computes exactly which glyph ids a
  document needs (`crates/dashscene-typeset/src/atlas/closure.rs`,
  `AtlasSpec::extra_glyph_ids`), so the same closure can drive a font subset.
- **Figma cannot supply font bytes.** Its REST API serves node JSON and rendered
  images, and the plugin API exposes font names, never binaries. A font
  therefore comes from its upstream origin under its own licence, which makes
  embedding a per-font producer decision rather than a default.
- Making the family load-bearing changes what a cascade must carry: a
  `WeightedFont` records a weight but no family name today, so the renderer
  cannot currently compare a requested family against the ones it holds.
- Which families the pinned cascade ships is a separate decision, recorded in
  `docs/decisions/corpus-ships-inter.md`.
