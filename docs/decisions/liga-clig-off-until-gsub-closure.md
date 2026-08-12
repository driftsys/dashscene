# Decision: ligature shaping features (liga/clig) stay off until GSUB closure lands

    status   accepted (story #28, 2026-07-12) — resolves the #27 seam
             note; the re-enable landed per-run at #33, 2026-07-16
             (see Resolution)
    scope    crates/dashscene-typeset text module — shape.rs feature
             list passed to rustybuzz::shape
    evidence docs/decisions/atlas-closure-cmap-plus-extras.md (the #27
             seam note this decision resolves)

## Context

Story #27's atlas closure is cmap-only
(`docs/decisions/
atlas-closure-cmap-plus-extras.md`): the atlas covers only the
glyph ids reachable directly from a charset codepoint via cmap, plus whatever a
caller lists in `extra_glyph_ids`. Story #28 shapes text with rustybuzz, which
by default enables OpenType's standard ligature features (`liga`, `clig`). A
ligature glyph (for example "fi" → a single ligature glyph in fonts that have
one) is reachable only through GSUB substitution, not cmap — so with cmap-only
atlas closure, that glyph id is not guaranteed to be in the atlas. P4 forbids a
shaped glyph silently failing to paint; #28 had to decide what shapes text
before an atlas that can guarantee coverage.

## Options

1. Shape with `liga`/`clig` disabled. This removes the Latin ligature
   substitutions — the class the fixture font actually applies — but it is a
   narrowing, not a guarantee: other default-on GSUB features (`ccmp`, and
   `calt`/`rlig` in other fonts) can still substitute to glyph ids outside
   cmap's image in fonts that use them. The painter's missing-glyph diagnostic
   (#30) is the named backstop (P4), and GSUB charset closure (#34) closes the
   gap for real. Kerning (`kern`, a GPOS feature that repositions pen advances
   but produces no new glyph ids) stays on.
2. Shape with `liga`/`clig` on, and extend `AtlasSpec::extra_glyph_ids` (already
   part of the #27 contract) with a hand-maintained list of common ligature
   glyph ids per font, so the atlas covers them without full GSUB closure.

## Choice

Option 1. `shape()` (`crates/dashscene-typeset/src/text/shape.rs`) passes
`Feature::new(Tag::from_bytes(b"liga"), 0, ..)` and the same for `b"clig"` — a
zero value disables the feature over the whole buffer. `kern` is left at its
rustybuzz default (on). Proven by `liga_disabled_keeps_fi_two_glyphs`: shaping
"fi" against the committed Noto Sans fixture yields two glyphs whose ids equal
`cmap('f')`/`cmap('i')`, not a single ligature glyph.

## Why

- Option 2 couples every atlas build to a hand-maintained, per-font glyph id
  list that has to be kept in sync with whichever ligatures the font actually
  defines and rustybuzz actually produces — a maintenance burden with no
  automated check that the list is complete, and a silent-coverage-gap risk
  exactly where P4 forbids one.
- #34 is already the story that owns per-locale charset vocabulary and is
  expected to implement real GSUB closure (walking the font's substitution rules
  to compute which contextual/ligature glyphs a charset can produce). Turning
  ligatures on then, as one coordinated change with GSUB closure, means shaping
  features and atlas coverage move together by construction — there is no window
  where shaping can produce a glyph id the atlas does not carry.
- Kerning needs no atlas coverage change (it only repositions pen advances via
  GPOS, producing no new glyph ids), so leaving it on costs nothing and
  preserves normal Latin letter spacing quality now.

## Consequences

- v0.5 Latin text renders without ligatures (for example "fi" as two glyphs, not
  a ligated glyph) — a cosmetic gap versus a font's full typographic behavior,
  accepted until #34.
- `docs/decisions/atlas-closure-cmap-plus-extras.md`'s seam note for #28/#30 is
  resolved by this decision: #28 disables the feature rather than feeding
  ligature glyph ids through `extra_glyph_ids`.
- #34 re-enabling `liga`/`clig` must land together with GSUB closure in the same
  change; enabling one without the other reopens the coverage gap this decision
  closes.

## Resolution (story #33, 2026-07-16)

The two halves of the planned coordinated change landed in sequence, not in one
change: #34 delivered the GSUB closure, and #33 delivered the feature flip. The
flip is also narrower than this record planned — per level run, not global:

- An Arabic-context run (a run holding strong Arabic characters — UAX #9 bidi
  class AL — or a digit run whose isolate-aware nearest strong character is AL;
  `text::shape`'s `RunContext`) shapes with rustybuzz's full default feature
  set, `liga`/`clig` included. This is the exact feature configuration
  `atlas::charset_closure` shapes with, so production output and atlas coverage
  move together by construction. The #33 acceptance test pins the coupling in
  two sizes: production-shaped output is a subset of the closure's coverage for
  the declared charset (`crates/dashscene-typeset/tests/typeset_arabic.rs` for
  the corpus-charset variant on every test run; `tests/atlas_pipeline.rs` for
  the full-charset pin in CI's atlas-repro job).
- Every other run keeps `liga`/`clig` disabled. #34's ligature sweep is
  pairwise: a three-character Latin ligature (Noto Sans carries `ffi`/`ffl`) is
  reachable by shaping but not covered, so a global flip would send words like
  "office" into the painter's missing-glyph diagnostic (#30). Latin text keeps
  rendering exactly as v0.5 rendered it.

Measured against the committed fixture font (Noto Sans Arabic v2.013), the
Arabic-side flip changes no output glyph: lam-alef is an `rlig` ligature, and
the contextual forms come from `ccmp`/`isol`/`init`/`medi`/`fina`, all
default-on in rustybuzz and never disabled here. The flip's value is feature-set
parity with the closure, not a rendering change — a future Arabic font that does
carry `liga`-gated forms shapes and covers them consistently.

Re-enabling `liga`/`clig` for non-Arabic runs stays blocked on closure coverage
of ligature chains longer than two characters (a sweep extension or real
GSUB-table walking). No v0 story carries that work, and E2 — the v0.6 gate —
contains no Latin text; the gap is cosmetic for Latin exactly as the
Consequences above accepted it.
