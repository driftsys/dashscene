# Decision: charset→glyph-id closure is cmap-only, with an extra_glyph_ids escape hatch

    status   accepted (story #27, 2026-07-12) — full GSUB closure
             deferred to #34
    scope    crates/dashscene-typeset atlas module — closure
             (atlas/closure.rs), `AtlasSpec::extra_glyph_ids`
    evidence docs/technotes/msdf-arabic-atlas-spike.md (spike #25)

## Context

The atlas must cover every glyph id a shaped run can produce, but a
declared charset (DESIGN_1.md §6.1) is a set of codepoints, not glyph
ids. Story #27 had to decide how much of that codepoint→glyph closure it
computes now, given that full closure (following GSUB substitution
rules) requires real design work and #34 (per-locale charsets, v0.6) is
the story that owns charset-driven vocabulary.

## Options

1. cmap-only closure: for each charset codepoint, resolve the nominal
   glyph via `ttf-parser`'s cmap; codepoints without cmap coverage are
   reported in `missing_codepoints`, never silently dropped. Add an
   `extra_glyph_ids` input parameter so a caller can add glyphs that
   only shaping can discover (ligatures, contextual forms).
2. Full GSUB closure now: walk the font's substitution rules so a
   charset also pulls in every contextual/ligature glyph it can produce.

## Choice

Option 1. `charset_closure(face, charset, extra_glyph_ids)` returns a
`Closure { glyph_ids, missing_codepoints }`; `glyph_ids` is sorted,
deduplicated, and always includes glyph id 0 (`.notdef`), so painters
can draw a visible fallback for unmapped input; `missing_codepoints` is
ascending.

## Why

- Full GSUB closure (option 2) is mandatory for Arabic, where
  charset-declared coverage must pull in contextual forms — but
  `rustybuzz` exposes shaping, not a standalone glyph-closure operation,
  so computing it correctly is real design work, not a small addition.
  It is deferred to #34/v0.6, which is already the story that owns
  charset semantics.
- `extra_glyph_ids` future-proofs the contract for that deferral: it is
  part of `AtlasSpec` today, so #34 extending closure quality does not
  change `generate()`'s signature or the metrics blob shape — only what
  populates the glyph id set internally.
- `.notdef` is always included unconditionally (not left to caller
  discretion) so every atlas has a fallback glyph, matching P4's "never
  a silent drop" posture applied to unmapped runtime input, not just
  import-time diagnostics.
- `missing_codepoints` is a named diagnostic surface (R6): the pipeline
  itself does not decide severity — `generate()` reports the gap in the
  metrics blob and the caller (eventually the validator, per
  `docs/decisions/q1-msdf-below-14px.md`'s posture for the related
  small-size case) decides what to do with it.

## Seam note for #28/#30

With cmap-only closure, discretionary/standard Latin ligature glyphs
(`fi`, `fl`) are not in the atlas by default. #28 decides whether v0.5
shaping disables the `liga` OpenType feature or feeds the ligature glyph
ids in via `extra_glyph_ids`; #30 must treat a shaped glyph id absent
from the atlas as a named diagnostic, not a silent skip (P4) — see
`docs/design/atlas-pipeline.md`'s Seams section.
