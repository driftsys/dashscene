# Decision: RAW is always the profile, lowercase raw is always an encoding

    status   accepted
    scope    the v0.12 asset pipeline's vocabulary — quality-profile names
             and encoding terms, in docs, code identifiers, and comments,
             enforced from story #436 onward
    source   docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md,
             "Targets and codec plan"
    related  docs/decisions/native-astc-codec-table.md,
             docs/decisions/asset-model-content-addressed-blobs.md

## Context

The asset pipeline has three quality profiles — RAW, HiFi, Lite — and the
word "raw" also names an encoding concept (uncompressed pixel data, as
opposed to a compressed encoding like ASTC or BC7). Using one spelling for
both meanings makes a sentence like "the raw profile stores raw pixels"
genuinely ambiguous: which "raw" is the profile and which is the pixel
format. Earlier design iterations also used four other profile names —
Lossless, Access, Master, Eco — which are no longer part of the vocabulary
and must not resurface from stale notes or habit.

## Choice

- **RAW**, capitalized, always names the profile: the truth, the
  qualification baseline, the null binding
  (`docs/decisions/asset-model-content-addressed-blobs.md`).
- **raw**, lowercase, always names an encoding concept, never the profile.
  Prefer the word **uncompressed** instead, wherever "raw" as an encoding
  term could be misread as the RAW profile.
- **Lossless**, **Access**, **Master**, and **Eco** are retired names and
  stay out of the vocabulary entirely — not used as synonyms, aliases, or
  historical references in new prose.

## Why

- The profile and the encoding concept are genuinely different things: RAW
  the profile can, in principle, be carried by more than one encoding (v0's
  inline-bytes `.dsb` is its current null-binding form), so the two
  meanings need to stay distinguishable in running prose, not just in
  code.
- Retiring the four earlier names outright, rather than keeping them as
  documented synonyms, avoids a document accumulating two vocabularies for
  the same three profiles.

## Consequences

- Every doc and decision record gardened from
  `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` follows this
  naming from here on, including
  `docs/decisions/native-astc-codec-table.md`.
- A future story that introduces `dashpack` code identifiers, CLI flags, or
  manifest field names for the profiles is expected to follow the same
  capitalization rule.
