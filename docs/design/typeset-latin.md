# typeset — the runtime text pipeline

    crate    crates/dashscene-typeset (module `text`)
    covers   v0.5 — text I: Latin (story #28, epic #24);
             v0.6 — text II: bidi (story #32) and Arabic shaping +
             digit shapes (story #33), epic #31;
             v0.7 — multi-font fallback (story #219)
    traces   docs/archive/2026-07-14-design-1-seed.md §7.2 (runtime
             half: shape → line break → positioned glyph runs;
             shaped-run cache), §2 (rustybuzz, ttf-parser,
             unicode-bidi), P1/P2 (one typesetter; painters only
             color), R1 (text quality),
             docs/design/atlas-pipeline.md (build-time half, the #33
             coverage coupling),
             docs/decisions/liga-clig-off-until-gsub-closure.md (the
             per-run feature posture),
             docs/technotes/msdf-arabic-atlas-spike.md (offsets and
             mis-ordered-digits findings, spike #25)

## Purpose

The runtime half of `docs/archive/2026-07-14-design-1-seed.md` §7.2:
text and size in, positioned glyph runs out — split into UAX #9 level
runs, shaped per run by rustybuzz, broken into lines, reordered per
line for display, positioned on baselines, cached so re-layout of
unchanged text costs a lookup. v0.5 built the pipeline single-direction
LTR; #32 added the bidi itemization and RTL placement, #33 the
Arabic-context shaping and digit-shape selection on top of the same
seam.

`Typesetter::layout` is a standalone entry point: it takes `text` and
`size` as plain parameters, not a `NodeId`. It does not read
`dashscene-core`'s arena. The seam that lets a producer's authored
text and style (`Arena::text`, `Arena::text_style` —
`docs/design/dashscene-core-arena.md`, story #26) reach this pipeline
is the measure callback (#29), which reads those accessors and calls
`Typesetter::layout` with the result.

## Pipeline shape

    Typesetter::with_fonts([primary, ..fallbacks])   ordered font list;
    Typesetter::new(Font)                            new = one-element list
    Typesetter::layout(text, size, max_width)   → TextLayout
      split text on '\n'                        → chunks
      per-chunk cache lookup (key: text)         → ShapedText (font
        miss: bidi-resolve (UAX #9), then per      units, unpositioned,
        level run: resolve direction + context,    logical order, each
        split by font coverage, shape each         glyph tagged with its
        sub-run, rebase clusters                    font index)
      per bidi paragraph: greedy line break      → lines of glyph
        (space runs / '\n', logical order)         ranges
      position (visual reorder per line — UAX    → TextLayout
        #9 L2 — scale by size/upem, baselines,
        flush-right shift for RTL paragraphs)

Each `'\n'`-delimited chunk is shaped (or cache-hit) as a whole; a
UAX #9 block separator inside a chunk (CR, NEL, U+2029, …) ends a bidi
paragraph exactly as `'\n'` ends a chunk, so no line ever spans two
paragraphs and each paragraph reorders and aligns under its own base
direction. Lines from every paragraph stack in the final `TextLayout`
in logical order, each baseline continuing the running line count.

## Bidi (story #32)

- `BidiInfo::new(chunk, None)` resolves levels; the base direction per
  paragraph comes from the first strong character (UAX #9 P2/P3).
- `level_runs` cuts a paragraph into maximal byte ranges of one
  resolved level; each run shapes with its level's direction. Shaping
  a mixed-direction string as a single run mis-orders embedded digits
  (spike #25), so the split comes first.
- Shaped glyphs are stored in logical order (clusters non-decreasing
  across the paragraph): rustybuzz emits an RTL run in visual order,
  so `shape()` reverses it back. Line breaking walks logical order, as
  UAX #9 prescribes.
- `position_line` reorders each line's level runs for display
  (`BidiInfo::visual_runs`, L1+L2) and re-reverses RTL runs' glyphs to
  visual order; the pen then advances left-to-right over the result.
- RTL-base paragraphs sit flush-right within `max_width` (or within
  the widest line when `None`); LTR lines stay flush-left at x = 0.
  `TextLayout::width` stays the widest line's pen advance — the
  measure contract — so an RTL line's glyph positions can reach up to
  `max_width`, past `width`.

## Arabic shaping (story #33)

Every level run shapes under a `RunContext` derived from its paragraph
— never authored (P1: the document carries the authored codepoints;
digit shapes and feature sets are resolved results that live only in
the layout output):

- **Arabic** — the run contains a strong Arabic character (UAX #9
  bidi class AL, the shared `is_arabic_strong` predicate: the Arabic
  letters of every block, but not Arabic-block neutrals such as
  U+060C ARABIC COMMA), or it is a digit run whose digit-shape
  context resolves Arabic (below). The run shapes with rustybuzz's
  full default feature set, `liga`/`clig` included — the exact
  configuration `atlas::charset_closure` shapes with, so production
  output and atlas coverage move together
  (`docs/decisions/liga-clig-off-until-gsub-closure.md`, Resolution).
  Contextual forms (`isol`/`init`/`medi`/`fina`), lam-alef (`rlig`),
  and mark composition (`ccmp`) are default-on complex-shaper
  features; the flip to defaults adds `liga`/`clig` parity on top.
- **Plain** — every other run keeps `liga`/`clig` disabled: the
  closure's ligature sweep is pairwise, so a three-character Latin
  ligature (`ffi`) would shape to a glyph id the atlas cannot cover.

Harakat arrive from shaping as separate glyphs with zero advance and
nonzero GPOS x/y offsets (mark-to-base, mark-to-mark stacking); the
offsets flow through `ShapedGlyph` into positioning, where the y-up →
y-down negation places a fatha above its shadda above its base — and
a dot-below glyph below the baseline (both offset signs occur in the
fixture font).

The coupling with the build-time half is pinned by an acceptance test
in two sizes: every glyph id production lays out for text composed
from the declared charset is inside `charset_closure`'s coverage — a
failure means the two modules drifted on direction, feature set, or
digit selection. The corpus-charset variant runs on every `cargo
test` (`tests/typeset_arabic.rs`); the full-charset E2 pin costs
seconds of pairwise sweep and runs in CI's atlas-repro job behind its
env gate (`tests/atlas_pipeline.rs`,
`production_layout_stays_within_full_charset_coverage`).

## Digit-shape selection (story #33)

A European digit (U+0030..=U+0039) displays with its Arabic-Indic
counterpart's glyph (U+0660..=U+0669) when its context is Arabic; the
authored codepoints never change, and clusters keep indexing the
authored bytes (the substitution happens at buffer-fill time, per
char, with the authored byte index as the cluster).

The context rule (`run_context`): strong characters inside the run
decide directly — any AL makes the run Arabic, otherwise any L or R
makes it Plain (UAX #9 W7 folds Latin-anchored digits into their L
run, so those digits resolve here). For a strong-free digit run the
nearest strong character before the run's first digit decides — AL
selects Arabic-Indic, L or R keeps European; when no strong character
precedes, the nearest one after it decides (a number opening an
Arabic sentence); with no reachable strong character at all, the
authored European shapes stay. Both scans are isolate-aware (UAX #9
P2): an isolate's interior is sealed, so the scans jump over
initiator..PDI pairs and stop at the enclosing isolate's boundary.
Authored Arabic-Indic digits are never substituted; anchored to
Arabic text they take the Arabic posture (closure-parity features),
and unanchored they shape Plain to the same cmap glyphs.

This is a context-derived default, deliberately not an authored
property: the document carries which digits were authored, and a
producer that wants a specific digit system authors those codepoints.
A locale-style authored preference (for example European digits inside
Arabic text, the Maghreb convention) would override this default; no
v0 story carries it, and the E2 screen does not need it.

The atlas side mirrors the rule structurally: a charset declaring
strong Arabic characters next to European digits also covers the
Arabic-Indic counterpart glyphs, derived through the text module's
own `is_arabic_strong` predicate and `arabic_indic_digit` mapping —
one definition, so the coverage rule cannot drift from the production
rule (`docs/design/atlas-pipeline.md`, Charset closure).

## Font fallback (story #219)

A `Typesetter` holds an ordered font list — the primary font first,
then fallbacks (`Typesetter::with_fonts`; `Typesetter::new` is the
one-element list). The list is the runtime's font configuration
resolved from the document's single font reference per style: the
document carries one font reference (P1), and no `.dsb` schema change
was needed — the fallback list is a runtime-side property of the
`Typesetter`, not authored (`docs/decisions/font-fallback-deferred-past-v06.md`).

The cascade sits between the bidi level-run split and shaping. Each
UAX #9 level run is split into contiguous font sub-runs by coverage
(`shape.rs`, `font_split`). A **base** codepoint goes to the first
font in the list whose cmap covers the glyph it will actually shape
to, and a base no font covers stays in the primary font, where it
shapes to `.notdef` — the painter's named missing-glyph diagnostic
(#30), never a silent drop (P4). A **continuation** codepoint is not
routed on its own coverage: a combining mark (general category
Mn/Mc/Me) and a format control (Cf — the joiners ZWJ/ZWNJ, bidi
controls, and other invisible format characters) have no independent
identity to a shaper, so each inherits the font of the base it
attaches to; a leading continuation with no base takes the run's
first resolved base font. This keeps a mark and its base in one
shaping call (so GPOS mark-to-base fires) and a joiner with the
letters it joins (so Arabic joining is not broken by a font-split
boundary) — the routing regression the coverage-only split would
cause. Each sub-run then shapes with its own font, and every glyph is
tagged with that font's index (`ShapedGlyph::font` →
`PositionedGlyph::font`). A single-font list yields one sub-run per
level run spanning the whole run, so the pre-#219 output is
byte-for-byte unchanged and every glyph is tagged font 0 — the E2
Arabic golden depends on this.

Four couplings hold the cascade consistent with the rest of the
pipeline:

- **Continuations follow their base, not their own coverage.** As
  above: marks and format controls inherit the preceding base's font.
  Routing them by coverage would strand a ZWJ that both fonts' cmaps
  carry in the wrong font (splitting an Arabic word into isolated
  forms), or split a mark from its base (so it shapes alone and its
  GPOS positioning never fires).
- **Context is per level run, inherited by the sub-runs.** The
  Arabic-context rule and the digit-shape context scan
  (`run_context`, isolate-aware) run once per level run, on the full
  paragraph, before the font split. A font-split boundary therefore
  cannot confuse a digit run's context scan — the sub-runs of one
  level run all share its resolved context.
- **Coverage is probed against the display shape, not the authored
  codepoint.** In Arabic context a European digit is probed against
  its Arabic-Indic counterpart (the same `arabic_indic_digit` the
  shaper substitutes with), so the digit cascades to the font that
  can render its display form rather than to one that would shape it
  to `.notdef`.
- **Each glyph scales by its own font's upem.** Advances and offsets
  are in the glyph's own font units, so positioning and line breaking
  scale each glyph by `size / its_font.units_per_em`
  (`layout.rs`), not by the primary's — a fallback font of a
  different upem behind the primary would otherwise mis-size and
  mis-place all its text. Only the line-box metrics (ascent, descent,
  line gap, and so the baseline advance) come from the primary font;
  cross-font metric unification (a line box sized to the tallest font
  on the line) is deliberately out of scope, which keeps a single-font
  layout unchanged.

A neutral base character covered by more than one font (an ASCII
space) goes to the first font that covers it, per the cascade rule,
rather than staying with its surrounding script's font. For the
invisible space this is harmless; a script-aware neutral itemization
is a later refinement, not built speculatively.

## Public surface

    crates/dashscene-typeset/src/text/mod.rs

    pub struct Typesetter { fonts: Vec<Font>, cache: HashMap<Box<str>, Arc<ShapedText>>, hits, misses }
    impl Typesetter {
        pub fn new(font: Font) -> Typesetter;           // one-element list
        pub fn with_fonts(fonts: Vec<Font>) -> Typesetter;   // primary first
        pub fn font(&self) -> &Font;                    // the primary
        pub fn fonts(&self) -> &[Font];                 // the cascade order
        pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout;
        pub fn cache_stats(&self) -> CacheStats;
    }
    pub struct CacheStats { pub hits: u64, pub misses: u64 }
    pub struct TextLayout { pub lines: Vec<Line>, pub width: f32, pub height: f32, pub size: f32 }
    pub struct Line { pub glyphs: Vec<PositionedGlyph>, pub width: f32, pub baseline_y: f32 }
    pub struct PositionedGlyph { pub glyph_id: u16, pub font: u16, pub x: f32, pub y: f32 }
    pub struct Font { /* Arc<Vec<u8>>, face index,
                          atlas::FontMetrics (shared with the blob) */ }
    pub enum TypesetError { FontParse(String) }

The surface grew at #219: `Typesetter` holds a font list
(`with_fonts`, `fonts()`), and `PositionedGlyph` carries the `font`
index the cascade resolved — the value a boundary-B stager groups runs
by, one glyph run per atlas. `new`, `font()`, and every other field
keep their v0.5 meaning; a single-font `Typesetter` and its output are
byte-identical to before. `Font` is public — the measure callback
(#29) and the painter (#30) both need the font handle and its metrics,
including `Font::line_advance()` (the baseline-to-baseline distance
layout uses, from the primary font). `Font::metrics()` returns the
same `atlas::FontMetrics` type the blob records, extracted through the
same shared function, so the runtime and the build-time artifacts
cannot disagree. `ShapedText`/`ShapedGlyph`/`RunContext` stay
crate-private: they are the cache-value and shaping-posture
representations, and publishing them before a consumer exists would
freeze them into the public API. `shape_with_face()` and
`Font::face()` are also crate-private; nothing outside
`dashscene-typeset` constructs a `rustybuzz::Face`.

## Coordinate conventions

Document space is y-down, origin at the layout's top-left. `baseline_y`
is a line's baseline measured from that origin. Each glyph's `(x, y)`
is its pen position on that baseline with the shaping offsets applied:
HarfBuzz/rustybuzz offsets are y-up, so `y = baseline_y - y_offset *
scale`. Per-glyph offsets are carried through, not dropped — GPOS
positions marks through offsets (spike #25 finding,
`docs/technotes/msdf-arabic-atlas-spike.md`). For offset-less Latin
glyphs the negation is a no-op (`g.y == baseline_y`); Arabic marks
exercise both directions (harakat above the baseline, Noto Sans
Arabic's composed dot glyphs below).

The painter (#30) combines these pen positions with the atlas blob's
y-up `plane_em` quad (`docs/design/atlas-pipeline.md`'s metrics
model) — that conversion from `PositionedGlyph`'s document-space,
y-down pen position to a painted quad is the painter's, not this
crate's.

`docs/archive/2026-07-14-design-1-seed.md` §7.2's run tuple names
`(glyph id, x, y, size, atlas page)`:
`size` lives once on `TextLayout` (uniform per layout — one style per
text node), and the atlas page field waits until multi-page atlases
exist; both are additive later without a `PositionedGlyph` shape
change.

## Line metrics

Ascent, descent, and line gap come from hhea (`Font::ascender`,
`Font::descender`, `Font::line_gap` — the same numbers the atlas
metrics blob carries, `docs/design/atlas-pipeline.md`). Descent is
negative in font units. All three are scaled by `size / units_per_em`.

- First baseline: `ascent` (scaled) from the layout's top.
- Line advance (baseline-to-baseline distance):
  `ascent - descent + line_gap` (scaled).
- `TextLayout.width` = the widest line's width.
- `TextLayout.height` = line count × line advance.

This is the standard box model both a painter and the #29 measure
callback need.

## Greedy line-break semantics

Break opportunities: after a run of ASCII space glyphs, and at
`'\n'` (handled by paragraph splitting, not by the breaker itself).
Breaking walks the logical glyph order; the display reorder is
`position_line`'s. `max_width: Option<f32>`: `None` means one line per
paragraph regardless of width.

- A run of space glyphs contributes its scaled advance to the current
  line's running width, but never to the line's _trimmed_ end index.
  If that space run turns out to precede a wrap point, it is dropped
  from both the line before the break and the line after — a
  wrap-consumed space appears on neither line and its width does not
  count toward either line's reported `width`.
  If the space run instead falls inside a line (more text follows on
  the same line), its glyphs are kept and its width is included — the
  line's glyph range then covers the space along with the words
  around it.
- Leading space glyphs before the first word of a paragraph are
  preserved: the first word is always placed on its line
  (there is no previous content to overflow), and the line's start
  index is never advanced past a leading space, so those glyphs stay
  in the emitted range.
- A word wider than `max_width` overflows its line rather than
  breaking mid-word (an RTL line overflows leftward past x = 0,
  mirroring LTR overflow). Mid-word breaking and UAX #14 line
  breaking are out of scope for v0.5/v0.6.
- An empty string lays out to zero lines and zero size.

## Cache semantics

The cache lives on `Typesetter` (`HashMap<Box<str>, Arc<ShapedText>>`
plus hit/miss counters) — it is not a separate type; the cache is too
small to warrant its own module.

- Stores font-unit, unpositioned `ShapedText` — shaping output (glyph
  ids, advances, offsets in font units) is size-independent, so the
  px scale is applied only at positioning time.
- Keyed by chunk text alone. Resolved bidi levels, run directions,
  digit-shape contexts, and the font cascade are all pure functions of
  that text, so one entry serves every layout of the chunk.
  `docs/archive/2026-07-14-design-1-seed.md`
  §7.2 describes the key as
  "string + style"; the shaping-relevant style component — the ordered
  font list (story #219) — is fixed per `Typesetter` (runtime
  configuration, not a per-call axis), so the key reduces to the
  string. The cached `ShapedText` records the cascade's result (each
  glyph's font index), so a mixed-script paragraph is cascaded and
  shaped once and reused across sizes. Different `size` and
  `max_width` values reuse the same cache entry — see
  `docs/decisions/shaped-run-cache-font-units.md`. Only a shaping-
  relevant axis that varies per `layout` call (none today) would grow
  the key.
- Unbounded: cockpit UI text is a bounded set; an eviction policy is
  speculative until a real producer shows growth.
- `Typesetter::cache_stats() -> CacheStats { hits, misses }` makes hit
  and miss counts observable for tests and for #29's caller.

## Error handling

`TypesetError::FontParse(String)` is the only variant, raised by
`Font::from_bytes` for a font that either `ttf-parser` or `rustybuzz`
cannot parse (both are checked at construction, so `Font::face()`
downstream can `.expect()` rather than propagate a second error).
Everything after construction is infallible over valid input: shaping
an empty or unknown-codepoint string produces `.notdef` glyphs, not
errors. A `.notdef` (or any shaped glyph id absent from the atlas) is
the painter's named-diagnostic surface at #30 (P4) — this crate does
not look the glyph id up in an atlas at all.

## What is deliberately absent

- **UAX #14 line breaking.** Break points are ASCII space runs and
  `'\n'` only; the Unicode line-breaking algorithm arrives when real
  content demands it — no issue number assigned yet.
- **Cross-font line-metric unification.** Line-box metrics (ascent,
  descent, line gap) come from the primary font (Font fallback,
  above); a line box sized to the tallest font on the line is a later
  refinement. Per-glyph advances and offsets do scale by each glyph's
  own font upem — only the shared line box is the primary's. A
  codepoint no font in the list covers still shapes to `.notdef` and
  hits the painter's missing-glyph diagnostic (#30).
- **Script-aware neutral itemization.** A neutral base character
  covered by more than one font follows the plain cascade rule (first
  font that covers it), not the surrounding script's font. (Combining
  marks and format controls are not neutrals here — they inherit their
  base's font; see Font fallback.)
- **Latin ligatures.** `liga`/`clig` stay disabled for non-Arabic
  runs; see `docs/decisions/liga-clig-off-until-gsub-closure.md`
  (Resolution) for what unblocks them.
- **An authored digit-system preference.** Digit shapes are
  context-derived only (see Digit-shape selection); an authored
  override is a producer-side property no v0 story carries.
- **Pre-shaped-numerals fast path.** Frequently-changing numeric
  values miss the shaped-run cache each frame; the optimisation
  couples to the v0.4 incremental-commit path and is tracked as debt
  from story #33, not built speculatively.
- **Cache eviction.** The cache never shrinks (see Cache semantics
  above).
- **Arena wiring.** No dependency on `dashscene-core`; `Arena::text`/
  `Arena::text_style` reach this pipeline through #29, not through
  this crate.

## Components

    crates/dashscene-typeset/src/text/mod.rs     public surface:
                                                  Typesetter, TextLayout,
                                                  Line, PositionedGlyph,
                                                  CacheStats,
                                                  TypesetError; re-exports
                                                  Font; the per-paragraph
                                                  layout loop and the
                                                  RTL flush-right shift
    crates/dashscene-typeset/src/text/font.rs    Font (bytes, face
                                                  index, hhea metrics);
                                                  builds the on-demand
                                                  rustybuzz::Face
    crates/dashscene-typeset/src/text/shape.rs   ShapedText/ShapedGlyph
                                                  (font units, per-glyph
                                                  font index); level_runs
                                                  (UAX #9 itemization);
                                                  RunContext + run_context
                                                  (per-run features and
                                                  digit shapes); font_split
                                                  + is_continuation (the
                                                  coverage cascade with
                                                  mark/control inheritance,
                                                  #219); the rustybuzz
                                                  shaping call
                                                  (shape_with_face)
    crates/dashscene-typeset/src/text/layout.rs  break_lines (greedy
                                                  breaker, logical order),
                                                  position_line (visual
                                                  reorder + baseline
                                                  positioning); both scale
                                                  each glyph by its own
                                                  font's upem (#219)

`atlas` (build-time) and `text` (runtime) stay sibling modules of the
one typesetter crate (`docs/design/architecture.md`,
`docs/design/atlas-pipeline.md`'s Home section). `Font::from_bytes`
validates with both `ttf-parser::Face::parse` (metrics) and
`rustybuzz::Face::from_slice` (shaping) up front; `rustybuzz::Face`
itself is constructed on demand inside `shape()` rather than stored on
`Font` — the shaped-run cache sits in front of shaping, so `Face`
construction is off the hot path. A self-referential holder or a
re-parsing wrapper crate would provide no benefit here; revisit only with
profiling evidence.

## Testing (with the committed corpus Noto Sans + Noto Sans Arabic)

- `font.rs`: metrics match `ttf-parser` directly (upem, ascender,
  descender, line gap); garbage bytes are rejected.
- `shape.rs`: "AV" shapes to two non-`.notdef` glyphs with cmap-
  matching ids and kerning that tightens the A→V advance below the
  plain hmtx sum; "fi" shapes to two glyphs (liga off for Plain runs
  proven); clusters are byte indices; empty text shapes to nothing;
  RTL runs come back in logical cluster order; level runs and visual
  run order match hand-derived UAX #9 references; paragraph shaping
  rebases clusters across run boundaries. Arabic (fixture-pinned glyph
  ids, cross-checked against spike #25): beh takes distinct
  skeleton+dot forms per joining context and never its nominal cmap
  glyph; lam-alef ligates through `rlig` to its contextual forms;
  a fatha carries the font's exact GPOS offsets with zero advance;
  European digits display as Arabic-Indic in Arabic context with
  authored-byte clusters, and stay European in Plain context; digit
  runs resolve their context from the nearest strong character
  (before, after, none, Hebrew, Latin-embedded, extended-Arabic,
  isolate-sealed, and Arabic-comma cases). `font_split` (#219) segments
  a level run by coverage: a single-font list is one sub-run; an
  Arabic/Latin run splits; an Arabic-context European digit routes to
  the font that renders its Arabic-Indic display shape; an uncovered
  codepoint stays in the primary.
- `tests/typeset_latin.rs`: the v0.5 pipeline pins — breaks, wraps,
  widths, baselines, cache hits, offset-less glyphs on the baseline.
- `tests/typeset_bidi.rs` (issue #32): embedded digits keep LTR order
  in RTL text; an RTL-base paragraph places an LTR segment leftmost;
  RTL paragraphs sit flush-right (wrapped, unconstrained, and around
  empty paragraphs); class-B separators split paragraphs within a
  chunk and align independently.
- `tests/typeset_arabic.rs` (issue #33): a real word lays out with its
  contextual forms in visual order; marks position through GPOS
  offsets in document space (stacked harakat above, dot below the
  baseline, exact scaled offset); Arabic-Indic and substituted
  European digits keep LTR order left of the word; digit substitution
  on/off per context through the public API, including the
  isolate-sealed and Arabic-comma cases; and the corpus-charset
  coupling pin — production-shaped output stays within the declared
  charset's closure coverage. The full-charset variant (a
  joining-context sweep of every letter and haraka plus realistic
  strings) lives in `tests/atlas_pipeline.rs` behind the atlas-repro
  env gate. The fixture-path and helper trio (`font_data`,
  `typesetter`, `cmap`) is shared by all four typeset test files
  through `tests/common/mod.rs`.
- `tests/typeset_fallback.rs` (issue #219): a `with_fonts` typesetter
  lays out "sur'a km/h" with the Arabic word in the primary font and
  the Latin unit cascaded to the fallback, tagged by font index and
  placed as one visual line; an uncovered codepoint is `.notdef` in
  the primary; a single-font typesetter tags every glyph font 0; the
  cache key stays the text across sizes; the digit-shape context
  survives the font split (Arabic-context "120" renders Arabic-Indic
  in the primary while "km/h" cascades); in the reverse configuration
  an Arabic-context European digit cascades to the font that renders
  its Arabic-Indic shape; a joining control (ZWJ) and a non-joiner
  (ZWNJ) route with the Arabic they steer rather than stranding in the
  Latin primary (the word shapes identically to the single-font
  output); a combining mark inherits its base's font instead of
  splitting off to shape alone; and a font split strictly inside one
  RTL level run (an ampersand between two Arabic words) keeps the run's
  visual order. The per-font glyph scaling and the mark/control
  inheritance also have machine-independent unit tests in
  `layout.rs` (two-upem positioning and wrapping) and `shape.rs`
  (`font_split` inheritance and leading-mark cases).

## Out of scope (this record)

Cross-font line-metric unification and script-aware neutral
itemization (Font fallback, above), the measure callback (#29 —
`docs/design/dashscene-engine.md`), painting and atlas lookup (#30),
hyphenation/UAX #14, vertical text, letter-spacing and other style
axes, cache eviction, the pre-shaped-numerals fast path (debt
from #33).

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.2 (runtime
  shape → break → position, shaped-run cache), §2 (rustybuzz,
  unicode-bidi), P1, P2, R1; issue #28, #32, #33 acceptance criteria.
- Resolves: the #27 seam note in
  `docs/decisions/atlas-closure-cmap-plus-extras.md` (ligatures off
  until GSUB closure; re-enabled per-run at #33), and spike #25's
  per-glyph-offset and bidi-before-shaping requirements.
- Blocks: #35 (the `E2` golden) consumes this pipeline through #29/#30.
- Related design: `docs/design/atlas-pipeline.md` (build-time half,
  y-up `plane_em` convention, the charset-closure digit rule),
  `docs/design/dashbuf.md` and `docs/design/dashscene-core-arena.md`
  (the #26 text intent this pipeline is fed through at #29).
- Related decisions:
  `docs/decisions/liga-clig-off-until-gsub-closure.md`,
  `docs/decisions/shaped-run-cache-font-units.md`,
  `docs/decisions/font-fallback-deferred-past-v06.md`.
- Related technote: `docs/technotes/msdf-arabic-atlas-spike.md`.
