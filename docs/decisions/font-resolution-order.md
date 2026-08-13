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
`dashc` lowers it verbatim; `dashscene-core` stores it as `TextStyle::family`.

Nothing reads it. A search across `dashscene-typeset`, `dashscene-engine` and
the render walk finds no consumer: the render walk builds a Noto Sans cascade
from fixed paths, and the typesetter selects a face by script coverage. So a
document that asks for Inter renders in Noto Sans, and no code anywhere observes
that it happened.

This is the same defect #368 fixed for a neighbouring field. Weight was lowered,
carried and never consumed; family is lowered, carried and never consumed. #368
built the consumer for weight and left family untouched. It is arguably the
worse of the two, because the family name is a required field being ignored, and
because a substituted family changes letterforms, widths and metrics rather than
only stroke weight.

After #368 this is the largest remaining cause of the hero's live pixel
difference: weight substitution is gone, family substitution remains.

## Options

**Refuse the compile when the family is unavailable.** Rejected. It repeats what
#368 explicitly decided against: which fonts exist is a property of the
renderer's asset set, not of the document's intent, so recording the failure at
compile time violates P1 — one document compiled once and rendered by two
runtimes with different asset sets would carry one runtime's outcome as though
it were authored. It would also refuse every fixture currently authored in
Inter. `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` is
not a counter-argument: that decision was itself revised at S0 into a
per-compile `EmitPolicy` rather than a fixed refusal.

**Resolve from the host's installed fonts.** Rejected as a default, accepted as
an opt-in preview mode (see below). `docs/specification/05-qualification.md`
states that golden stability across machines rests on a pinned Skia, a committed
atlas, and no atlas generation at render time. A host-resolved font breaks all
three: the same document would render differently on a developer machine, on CI,
and on a target, which makes every golden image and every oracle band
meaningless. The oracle is this project's measurement instrument, and it has
caught real defects: the BASELINE leaf alignment (#272), the line height taken
from the cascade's primary font rather than the shaping font (#314), and — on
the import oracle's first measurement — a fixed line height placed at the full
intrinsic ascent instead of half-leading, together with an absent
`textAutoResize` mis-lowered as auto-size (both recorded in
`docs/decisions/figma-text-lowering.md`). P4's wording is also directly against
it: vocabulary is "validated, never **discovered**", and probing a host font
store for the nearest match is discovery.

Note this does **not** breach R7. R7 is "same input, byte-identical document",
and the host's fonts do not change the document. What breaks is golden and
oracle stability, which is E5 and E7.

**Substitute within the provided cascade and name it.** Accepted. It mirrors the
shape #368 established for weight, so the two gaps have one mechanism rather
than two.

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
   same reason weight matching is: a document may name any family (P5), and a
   hard error would make the renderer's asset set decide whether a valid
   document loads. Since story #385 every committed fixture names a family the
   cascade now carries, so nothing in the corpus exercises this today — it is
   the rule for documents the corpus does not contain.
4. **The host's installed fonts, only in an explicitly opted-in preview mode.**
   Never the default, never in CI, never in a target build, and never in a
   golden or oracle measurement. `EmitPolicy` is the precedent for a behaviour
   that is one of two policies chosen per run rather than a fixed rule.

Coverage still selects before weight within a family, so the full order is
family, then script coverage, then the CSS Fonts 4 weight step.

## Why

- It makes a required field affect the render. A field the schema marks required
  and no code reads is a defect, independent of where fonts come from.
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
- **Step 1's absence had a second cost nobody had written down, and story #863
  is where it was found.** The order above presumes a renderer that _has_ a
  cascade. Until 2026-08-13 no `.dsb` load path did: all four built
  `TaffySolver::new()`, the constructor with neither a typesetter nor an atlas
  set, so a loaded document containing text reached step 2 with nothing to match
  against. The effect was not the substitution this record is about — it was
  **no text at all**. `TaffySolver::stage_text` returns on its first guard,
  `self.atlases.is_empty()`, checked before the typesetter, so the committed
  glyph-run table was empty; and a hug-sized text node measured as an empty
  leaf, resolving to 0 x 0 and making its siblings lay out around a box the
  design did not specify. Measured on the committed
  `goldens/dsb/v07-text-hug-in-fill.dsb`: four rects, zero glyph runs, and the
  node holding "hug inside fill" at 0 x 0.

  **It stayed invisible because everything that draws text builds its own
  solver.** `corpus/showcase` does, so `demo`, `demo-web` and `demo-android`
  draw text for their own scenes; `goldens/tooling` does, so every text golden
  and the Figma oracle render text and match it. Both populations build the
  scene themselves and so already hold the font. A loaded document is the one
  case where something else produced the scene. `goldens/tooling/src/render.rs`
  had known this and worked around it in a comment since the goldens were
  written — it loads, then re-commits through a typesetter-backed solver — and
  nothing carried that knowledge to the hosts.

- **The host supplies both, and that is now an argument rather than a default.**
  `dashscene_engine::TextResources` is the pair, and
  `dashscene_desktop::Document::load`, `dashscene_desktop::load_bytes` and
  `dashscene_web::load_document` each take an `Option` of it. `None` is the
  pre-#863 behaviour and stays legitimate for a document with no text; it is no
  longer what a caller gets without asking. `TaffySolver::owning` holds the
  typesetter inside the solver rather than wrapping it, because
  `dashlang::attach_live` keeps its `Box<dyn LayoutSolver>` for the life of the
  scene and a solver rebuilt per call throws away Taffy's retained tree on every
  frame — issue #164's saving, paid back per frame.

  **The C ABI is not fixed and the reason is different in kind.** Neither a
  `Typesetter` nor an `Atlas` can cross a C boundary, so
  `ds_runtime_load_document` still builds the bare solver and the Android host,
  which loads through it, still draws no text. That is undesigned rather than
  blocked — a second entry point is a new symbol and does not move
  `DS_ABI_VERSION` — and it is issue #947.

- **What stays blocked is the document carrying its own.** Everything above is
  the _host_ half. Step 1 is unchanged: an embedded font still cannot become
  glyphs at load time, and a rasterised atlas must never be embedded at all. Two
  further findings from #863, for whoever revisits it:

  - **An `AssetKind` append would not pack.** `dashpack`'s `AssetClass::of`
    matches `Image` and `DistanceField` and returns `PackError::UnknownKind` for
    anything else, so a `Font` variant would compile and then fail to pack.
    Making it work needs an `AssetClass` variant, a colour space and a
    lossy-rung ladder, none of which mean anything for a binary face.
  - **The bank was never answered.** #863 asked whether these come from the
    document, the bank, or the host, and only the host half is settled.
    `dashpack` exists for cold-bank assembly and `crates/dashbuf/tests/bank.rs`
    assembles one document under two banks — the natural home for bytes shared
    across many documents, and the branch neither that issue nor this record has
    addressed.

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
- Reading the family changed what a cascade carries. A `WeightedFont` records a
  weight but no family name, so story #385 added `FontFamily { name, faces }`
  and `Typesetter::with_named_font_families` beside it, and `layout_styled`
  takes the requested family. The name is declared by whoever assembles the
  cascade rather than read from a face's own name table: Inter's Medium and
  SemiBold faces declare name ID 1 as `Inter Medium` and `Inter Semi Bold`, so
  reading it per face would put those weights in families of their own.
- **Steps 2 and 3 are implemented** (story #385). Matching is a probe-order
  permutation: the requested family moves to the head of the coverage probe
  order and the flattened positional slot list is untouched, so
  `PositionedGlyph::font` keeps its meaning, `dashpaint` is unchanged, and no
  stager's parallel atlas list changes shape. Coverage still decides which
  family shapes each codepoint, so naming a family never costs a reader an
  uncovered one; a family the cascade does not carry is reported as
  `text.family-substituted` and never refused.
- Which families the pinned cascade ships is a separate decision, recorded in
  `docs/decisions/corpus-ships-inter.md`.
