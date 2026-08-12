# Decision: charset→glyph-id closure is cmap-only, with an extra_glyph_ids escape hatch

    status   accepted (story #27, 2026-07-12) — full GSUB closure
             deferred to #34, delivered there 2026-07-16 (see Resolution)
    scope    crates/dashscene-typeset atlas module — closure
             (atlas/closure.rs), `AtlasSpec::extra_glyph_ids`
    evidence docs/technotes/arabic-atlas-coverage.md (spike #25)

## Context

The atlas must cover every glyph id a shaped run can produce, but a declared
charset (`docs/design/dashc.md`) is a set of codepoints, not glyph ids. Story
#27 had to decide how much of that codepoint→glyph closure it computes now,
given that full closure (following GSUB substitution rules) requires real design
work and #34 (per-locale charsets, v0.6) is the story that owns charset-driven
vocabulary.

## Options

1. cmap-only closure: for each charset codepoint, resolve the nominal glyph via
   `ttf-parser`'s cmap; codepoints without cmap coverage are reported in
   `missing_codepoints`, never silently dropped. Add an `extra_glyph_ids` input
   parameter so a caller can add glyphs that only shaping can discover
   (ligatures, contextual forms).
2. Full GSUB closure now: walk the font's substitution rules so a charset also
   pulls in every contextual/ligature glyph it can produce.

## Choice

Option 1. `charset_closure(face, charset, extra_glyph_ids)` returns a
`Closure { glyph_ids, missing_codepoints }`; `glyph_ids` is sorted,
deduplicated, and always includes glyph id 0 (`.notdef`), so painters can draw a
visible fallback for unmapped input; `missing_codepoints` is ascending.

## Why

- Full GSUB closure (option 2) is mandatory for Arabic, where charset-declared
  coverage must pull in contextual forms — but `rustybuzz` exposes shaping, not
  a standalone glyph-closure operation, so computing it correctly is real design
  work, not a small addition. It is deferred to #34/v0.6, which is already the
  story that owns charset semantics.
- `extra_glyph_ids` future-proofs the contract for that deferral: it is part of
  `AtlasSpec` today, so #34 extending closure quality does not change
  `generate()`'s signature or the metrics blob shape — only what populates the
  glyph id set internally.
- `.notdef` is always included unconditionally (not left to caller discretion)
  so every atlas has a fallback glyph, matching P4's "never a silent drop"
  posture applied to unmapped runtime input, not just import-time diagnostics.
- `missing_codepoints` is a named diagnostic surface (R6): the pipeline itself
  does not decide severity — `generate()` reports the gap in the metrics blob
  and the caller (eventually the validator, per
  `docs/decisions/q1-msdf-below-14px.md`'s posture for the related small-size
  case) decides what to do with it.

## Seam note for #28/#30

With cmap-only closure, discretionary/standard Latin ligature glyphs (`fi`,
`fl`) are not in the atlas by default. #28 decides whether v0.5 shaping disables
the `liga` OpenType feature or feeds the ligature glyph ids in via
`extra_glyph_ids`; #30 must treat a shaped glyph id absent from the atlas as a
named diagnostic, not a silent skip (P4) — see `docs/design/atlas-pipeline.md`'s
Seams section.

## Resolution (story #34, 2026-07-16)

Story #34 delivered the deferred GSUB closure (Option 2's goal) by the means
Option 1 anticipated: `charset_closure` still returns the same `Closure` and
`AtlasSpec::extra_glyph_ids` still exists, so `generate()`'s signature and the
metrics-blob shape did not change. Only what fills the glyph-id set internally
grew. `charset_closure`'s first parameter changed from `&ttf_parser::Face` to
`&rustybuzz::Face` (which derefs to the former for the unchanged cmap lookups)
so the module can shape.

The closure is shaping-based, not a walk of the GSUB tables: rustybuzz exposes
shaping but no standalone glyph-closure operation, so the closure shapes the
declared charset in the contexts that trigger substitution — each character
isolated, each Arabic letter through the four joining contexts (a beh connector
gives final/initial/medial), each haraka on a base letter, and every ordered
character pair — and unions the output glyph ids. Spike #25's method. The
as-built method, its `&rustybuzz::Face` signature, and its boundaries are
documented in `docs/design/atlas-pipeline.md`'s Charset closure section.

Two boundaries remain, both P4-safe (an uncovered shaped glyph is the painter's
named missing-glyph diagnostic, #30, never a silent drop):

- Ligatures of three or more characters (`ffi`/`ffl`, the Allah ligature) are
  outside the pairwise sweep.
- Only standard Arabic (`0x0621..=0x064A` letters, `0x064B..=0x0652` harakat)
  gets the joining-context sweep; extended Arabic gets its isolated, ligature,
  and incidentally-swept forms only.

The `liga`/`clig` re-enable that this record's seam note left to #28 is now the
#33 join, not #34: the closure already covers the ligature and contextual-form
glyphs, so #33 can turn those features on without reopening a coverage gap
(`docs/decisions/liga-clig-off-until-gsub-closure.md`).
