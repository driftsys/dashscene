# Decision: the font resolution order, and what happens when a family is missing

    status   accepted (issue #379, epic #344 — the repository owner's
             decision, 2026-07-25)
    scope    crates/dashscene-typeset (the cascade and its selection),
             crates/dashbuf + crates/dashscene-core (the family the
             document already carries), the render walk in goldens/tooling
    binds    #368 (the weight axis this extends), #107 and #344 (the asset
             model, where an embedded font would live), #345 (dashpack,
             where a font would be baked)
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

This is the same defect #368 fixed for a neighbouring field. Weight was lowered, carried
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
caught real defects: the BASELINE leaf alignment (#272), the line height taken
from the cascade's primary font rather than the shaping font (#314), and — on
the import oracle's first measurement — a fixed line height placed at the full
intrinsic ascent instead of half-leading, together with an absent
`textAutoResize` mis-lowered as auto-size (both recorded in
`docs/decisions/figma-text-lowering.md`). P4's wording is also directly
against it: vocabulary is "validated, never **discovered**", and probing a host
font store for the nearest match is discovery.

Note this does **not** breach R7. R7 is "same input, byte-identical document",
and the host's fonts do not change the document. What breaks is golden and
oracle stability, which is E5 and E7.

**Substitute within the provided cascade and name it.** Accepted. It mirrors
the shape #368 established for weight, so the two gaps have one mechanism
rather than two.

## Choice

Font resolution proceeds in this order, and the family name is read and affects
the result at every step:

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

- It makes a required field affect the render. A field the schema marks
  required and no code reads is a defect, independent of where fonts come from.
- It keeps the product path deterministic. Steps 1 to 3 depend only on the
  document and on a pinned, committed asset set, so a render is reproducible on
  any machine.
- It satisfies P4 without violating P1. The renderer made the substitution, so
  the renderer reports it; the document records intent and stays
  renderer-agnostic.
- It keeps the host-font convenience available without letting it reach the
  measurement path, where it would invalidate every golden and every band.

## The tension with R6, stated rather than avoided

R6 reads: "Unsupported design vocabulary is a named import diagnostic
(warning/error), never a silent drop **or runtime fallback**." Steps 3 and 4 are
both runtime fallbacks, so the conflict has to be faced.

This record reads R6 as governing **vocabulary**, not **asset availability**. A
family name is not out-of-profile vocabulary: the format accepts any family
string by design (P5), and the document expressing "Inter" is perfectly valid
intent. What is missing is an asset in a particular renderer's set. That is the
same distinction #368 drew for weight, and the same reason a compile-time record
would violate P1 — the document is valid, the renderer is incomplete.

Under that reading the operative word in R6 is **silent**, and every step here
satisfies it: a substitution is always reported as `text.family-substituted`,
including in the opt-in preview mode, which additionally reports which host face
it resolved to. Nothing falls back without saying so.

If a reader concludes instead that R6 governs asset availability as well, then
this decision cannot stand as written and R6 itself needs revising — which is a
decision for the repository owner, not something to settle by interpretation
here. Recorded so that choice is visible rather than buried.

Note also that R6 names an **import** diagnostic. `text.family-substituted` is a
render-time diagnostic, so it is not the diagnostic R6 describes; it is the
render-side counterpart, exactly as
`docs/decisions/weight-substitution-is-a-render-time-diagnostic.md` established
for weight.

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
- **An embedded font should use the content-addressed asset table** (#107,
  `docs/decisions/asset-model-content-addressed-blobs.md`), which already
  contemplates "later font atlases", rather than a parallel mechanism. Schema
  growth stays additive under R7.
- **Subsetting is what makes this viable on target hardware**, and it is not
  free. A full face is roughly 431 KB. The GSUB closure that exists
  (`crates/dashscene-typeset/src/atlas/closure.rs`, plus
  `AtlasSpec::extra_glyph_ids`) expands a **declared** charset, and
  `docs/decisions/atlas-closure-cmap-plus-extras.md` is explicit that coverage
  comes from declared charsets, never from document text. So nothing today
  computes what a given document needs. That per-document glyph set has to be
  built before a subset can be derived; the existing closure is the second half
  of the job, not the whole of it.
- **Figma cannot supply font bytes.** Its REST API serves node JSON and rendered
  images, and the plugin API exposes font names, never binaries. A font
  therefore comes from its upstream origin under its own licence, which makes
  embedding a per-font producer decision rather than a default.
- Reading the family changes what a cascade must carry: a
  `WeightedFont` records a weight but no family name today, so the renderer
  cannot currently compare a requested family against the ones it holds.
- Which families the pinned cascade ships is a separate decision, recorded in
  `docs/decisions/corpus-ships-inter.md`.
