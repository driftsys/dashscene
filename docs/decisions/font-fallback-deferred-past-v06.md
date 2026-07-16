# Decision: multi-font fallback is deferred past v0.6

    status   accepted (v0.5-close plan revision, 2026-07-16)
    scope    dashscene-typeset — one font per declared charset through
             v0.6; per-style font lists and per-font charset unions
             deferred
    evidence docs/technotes/msdf-arabic-atlas-spike.md §4; tracking
             issue #219 (v0.7)

## Context

The v0.5 Arabic-atlas spike surfaced a requirement no v0.6 story
carries: Noto Sans Arabic contains no Latin letters and no U+002F
solidus, so "GPS" and "km/h"-style strings shape to `.notdef`.
Mixed-script UI text therefore needs font fallback — an ordered font
list per style, the charset union split per font rather than per
document, and atlas coverage per font. The `Typesetter` is
single-font per instance today, and the per-locale charset
vocabulary (#34) declares each charset against one font.

## Options

1. Add fallback inside v0.6, widening #34 (per-font charset splits)
   and #33 (per-run font selection before shaping).
2. Defer fallback past v0.6: v0.6 covers exactly one font per
   declared charset, and mixed-script text stays out of the slice's
   scenes.

## Choice

Option 2. v0.6 delivers Arabic text in an Arabic font; no v0.6 story
grows a font list.

## Why

- `E2` does not need it: the `E2` golden (#35) is an Arabic screen —
  Arabic script plus numerals, no Latin letters — so one font covers
  the whole scene.
- The first producer of mixed-script content is the Figma text
  lowering (#160, v0.7): real imported documents are where
  "km/h"-style strings arrive. Deferring keeps the fallback design
  next to the work that exercises it.
- Widening #34 and #33 now would grow v0.6's join point (#33 already
  depends on both #32 and #34 after the v0.5-close revision) for a
  capability nothing in the slice consumes.

## Consequences

- Through v0.6, a codepoint outside the style's single font shapes
  to `.notdef` and hits the painter's missing-glyph diagnostic (#30)
  — a named P4 diagnostic, not a silent drop.
- Tracking issue #219 (anchored to v0.7) carries the requirement;
  the v0.7 revision at the v0.6 close places it — its own story, or
  folded into #160.
- #34's charset vocabulary must not bake in a per-document charset
  union: the spike pinned that unions are per font, and that
  constraint holds even while each charset maps to one font.
