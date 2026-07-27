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

The asset pipeline has three quality profiles — RAW, HiFi, LoFi — and the
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
- **LoFi**, not **Lite** (renamed 2026-07-27, owner's call). **Lite** joins
  the retired list on the same terms.

## Why LoFi replaced Lite

Recorded because the rename touched code identifiers and a measured number,
so it should not be re-litigated as a matter of taste.

**"Lite" names the wrong axis.** It reads as weight or size, and
`docs/decisions/compress-raster-only.md` establishes that the objective is
memory bandwidth, residency and load-time CPU, with **file size a
constraint rather than the goal**. A profile is defined as a set of
per-asset-class tolerance bands, so what actually varies between profiles
is band width — a fidelity axis. `RAW / HiFi / LoFi` names that axis
consistently; `RAW / HiFi / Lite` mixes two metaphors and points the second
one at the axis the design deliberately does not optimise.

**The objection considered and rejected.** "Lo-fi" was thought to risk
implying deliberate degradation, which would misdescribe a profile whose
central property is that over-compression is structurally impossible — the
packer escalates until the band holds. That objection does not survive: in
music and film, lo-fi does not mean failed fidelity. It means a fidelity
envelope chosen on purpose, with the texture as part of the intent and
usually meticulous work behind it. That connotation is the correct one
here. LoFi is not the profile that fell short of HiFi; it is the profile
that targets a different envelope deliberately, and the band contract is
what guarantees it lands there.

**A rendered measurement moved, and is recorded.** The
`profile-stress` preview scene carries a text overlay labelling the band,
so renaming the label changed the glyphs drawn and moved the measured
mutation fractions — 51.8707 % to 51.8097 % under HiFi, and 9.7900 % to
9.7733 % under LoFi. The oracle caught the change rather than absorbing it.
The new figures are recorded in `goldens/oracle/profile-manifest.json` with
the reason, and no golden moved.

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
