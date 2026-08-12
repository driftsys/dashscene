# Decision: multi-font fallback is deferred past v0.6

    status   accepted (v0.5-close plan revision, 2026-07-16)
    scope    dashscene-typeset — one font per declared charset through
             v0.6; per-style font lists and per-font charset unions
             deferred
    evidence docs/technotes/arabic-atlas-coverage.md §4; tracking
             issue #219 (v0.7)

## Context

The v0.5 Arabic-atlas spike surfaced a requirement no v0.6 story carries: Noto
Sans Arabic contains no Latin letters and no U+002F solidus, so "GPS" and
"km/h"-style strings shape to `.notdef`. Mixed-script UI text therefore needs
font fallback — an ordered font list per style, the charset union split per font
rather than per document, and atlas coverage per font. The `Typesetter` is
single-font per instance today, and the per-locale charset vocabulary (#34)
declares each charset against one font.

## Options

1. Add fallback inside v0.6, widening #34 (per-font charset splits) and #33
   (per-run font selection before shaping).
2. Defer fallback past v0.6: v0.6 covers exactly one font per declared charset,
   and mixed-script text stays out of the slice's scenes.

## Choice

Option 2. v0.6 delivers Arabic text in an Arabic font; no v0.6 story grows a
font list.

## Why

- `E2` does not need it: the `E2` golden (#35) is an Arabic screen — Arabic
  script plus numerals, no Latin letters — so one font covers the whole scene.
- The first producer of mixed-script content is the Figma text lowering (#160,
  v0.7): real imported documents are where "km/h"-style strings arrive.
  Deferring keeps the fallback design next to the work that exercises it.
- Widening #34 and #33 now would grow v0.6's join point (#33 already depends on
  both #32 and #34 after the v0.5-close revision) for a capability nothing in
  the slice consumes.

## Consequences

- Through v0.6, a codepoint outside the style's single font shapes to `.notdef`
  and hits the painter's missing-glyph diagnostic (#30) — a named P4 diagnostic,
  not a silent drop.
- Placed at the v0.6-close revision (2026-07-16): #219 is its own v0.7
  `dashscene-typeset` story, not folded into #160. Fallback is a typeset-runtime
  capability (per-style font lists, per-font charset unions, per-font atlas
  pages), so it does not belong inside the producer-side text-lowering story; it
  depends only on the completed v0.6 typeset runtime and runs in parallel with
  #140. #160 names multi-font fallback as explicit non-scope: through #160, a
  codepoint outside a style's single font is #30's named missing-glyph
  diagnostic (P4), never a silent drop.
- #34's charset vocabulary must not bake in a per-document charset union: the
  spike pinned that unions are per font, and that constraint holds even while
  each charset maps to one font.

## Resolution (story #219, 2026-07-16)

Fallback landed in v0.7, runtime-side, with no `.dsb` schema change — the design
the deferral anticipated. A `Typesetter` now holds an ordered font list
(`Typesetter::with_fonts`), resolved from the runtime's font configuration; the
document still carries one font reference per style (P1). Each UAX #9 level run
splits by coverage before shaping — a codepoint goes to the first font in the
list that covers the glyph it will shape to, and a codepoint no font covers
stays in the primary as `.notdef` (P4). The charset union stays per font: the
mixed-script golden uses one committed atlas per font (`corpus/atlas/arabic`
primary, `corpus/atlas/ascii` fallback), each already R7-reproducible. The
as-built cascade, the per-glyph font index it produces, and the cache-key
reasoning are in `docs/design/typeset-latin.md` (Font fallback); the boundary-B
consequence — one glyph run per font, no `dashpaint` type change — is in
`docs/decisions/glyph-runs-cross-boundary-b.md` (Resolution). #160 remains free
of multi-font work: its named non-scope is discharged by this typeset-runtime
capability, which it may consume but need not build.

The cascade grew a second axis at story #368: it is now a list of **families**,
each family an ordered set of weighted faces, with coverage picking the family
and the requested CSS weight picking the face within it. The deferral's two
constraints survive that change unmodified — the charset union is still per
font, and the document still carries one font reference per style (P1) — because
the weight axis is also runtime-side configuration, resolved from the renderer's
asset set rather than authored
(`docs/decisions/weight-selection-in-the-cascade.md`). What the cascade still
does not do is substitute one **family** for another; an out-of-corpus family
remains unrecorded scope.
