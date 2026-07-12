# Decision: ligature shaping features (liga/clig) stay off until GSUB closure lands

    status   accepted (story #28, 2026-07-12) — resolves the #27 seam
             note; re-enable together with GSUB closure at #34
    scope    crates/dashscene-typeset text module — shape.rs feature
             list passed to rustybuzz::shape
    evidence docs/decisions/atlas-closure-cmap-plus-extras.md (the #27
             seam note this decision resolves)

## Context

Story #27's atlas closure is cmap-only (`docs/decisions/
atlas-closure-cmap-plus-extras.md`): the atlas covers only the glyph
ids reachable directly from a charset codepoint via cmap, plus
whatever a caller lists in `extra_glyph_ids`. Story #28 shapes text
with rustybuzz, which by default enables OpenType's standard ligature
features (`liga`, `clig`). A ligature glyph (for example "fi" → a
single ligature glyph in fonts that have one) is reachable only
through GSUB substitution, not cmap — so with cmap-only atlas
closure, that glyph id is not guaranteed to be in the atlas. P4
forbids a shaped glyph silently failing to paint; #28 had to decide
what shapes text before an atlas that can guarantee coverage.

## Options

1. Shape with `liga`/`clig` disabled. Every glyph id shaping can
   produce is then reachable via cmap (no GSUB substitution occurs),
   matching cmap-only atlas closure exactly. Kerning (`kern`, a GPOS
   feature that repositions pen advances but produces no new glyph
   ids) stays on.
2. Shape with `liga`/`clig` on, and extend `AtlasSpec::extra_glyph_ids`
   (already part of the #27 contract) with a hand-maintained list of
   common ligature glyph ids per font, so the atlas covers them
   without full GSUB closure.

## Choice

Option 1. `shape()` (`crates/dashscene-typeset/src/text/shape.rs`)
passes `Feature::new(Tag::from_bytes(b"liga"), 0, ..)` and the same
for `b"clig"` — a zero value disables the feature over the whole
buffer. `kern` is left at its rustybuzz default (on). Proven by
`liga_disabled_keeps_fi_two_glyphs`: shaping "fi" against the
committed Noto Sans fixture yields two glyphs whose ids equal
`cmap('f')`/`cmap('i')`, not a single ligature glyph.

## Why

- Option 2 couples every atlas build to a hand-maintained,
  per-font glyph id list that has to be kept in sync with whichever
  ligatures the font actually defines and rustybuzz actually
  produces — a maintenance burden with no automated check that the
  list is complete, and a silent-coverage-gap risk exactly where P4
  forbids one.
- #34 is already the story that owns per-locale charset vocabulary
  and is expected to implement real GSUB closure (walking the font's
  substitution rules to compute which contextual/ligature glyphs a
  charset can produce). Turning ligatures on then, as one coordinated
  change with GSUB closure, means shaping features and atlas coverage
  move together by construction — there is no window where shaping
  can produce a glyph id the atlas does not carry.
- Kerning needs no atlas coverage change (it only repositions pen
  advances via GPOS, producing no new glyph ids), so leaving it on
  costs nothing and preserves normal Latin letter spacing quality now.

## Consequences

- v0.5 Latin text renders without ligatures (for example "fi" as two
  glyphs, not a ligated glyph) — a cosmetic gap versus a font's full
  typographic behavior, accepted until #34.
- `docs/decisions/atlas-closure-cmap-plus-extras.md`'s seam note for
  #28/#30 is resolved by this decision: #28 disables the feature
  rather than feeding ligature glyph ids through `extra_glyph_ids`.
- #34 re-enabling `liga`/`clig` must land together with GSUB closure
  in the same change; enabling one without the other reopens the
  coverage gap this decision closes.
