# typeset-latin — the runtime text pipeline (Latin subset)

    crate    crates/dashscene-typeset (module `text`)
    covers   v0.5 — text I: Latin (story #28, epic #24)
    traces   DESIGN_1.md §7.2 (runtime half: shape → line break →
             positioned glyph runs; shaped-run cache), §2 (rustybuzz,
             ttf-parser), P1/P2 (one typesetter; painters only color),
             R1 (text quality),
             docs/design/atlas-pipeline.md (build-time half, seam
             notes),
             docs/decisions/atlas-closure-cmap-plus-extras.md (the
             #27 seam this story resolves),
             docs/technotes/msdf-arabic-atlas-spike.md (offsets
             finding, spike #25)

## Purpose

The runtime half of DESIGN §7.2, Latin subset: text and size in,
positioned glyph runs out — shaped once by rustybuzz, broken into
lines, positioned on baselines, cached so re-layout of unchanged text
costs a lookup. Bidi splitting is v0.6 (#32); this pipeline is
single-direction LTR by construction.

`Typesetter::layout` is a standalone entry point: it takes `text` and
`size` as plain parameters, not a `NodeId`. It does not read
`dashscene-core`'s arena. The seam that lets a producer's authored
text and style (`Arena::text`, `Arena::text_style` —
`docs/design/dashscene-core-arena.md`, story #26) reach this pipeline
is wired up at #29 (the measure callback), which is expected to read
those accessors and call `Typesetter::layout` with the result.

## Pipeline shape

    Typesetter::new(Font)                       one font in v0.5;
                                                 fallback lists are the
                                                 v0.6 charset story's
    Typesetter::layout(text, size, max_width)   → TextLayout
      split text on '\n'                        → paragraphs
      per-paragraph cache lookup (key: text)     → ShapedText (font
        miss: rustybuzz shape, liga/clig off       units, unpositioned)
      greedy line break (space runs / '\n')      → lines of glyph
                                                    ranges
      position (scale by size/upem, baselines)   → TextLayout

Each `'\n'`-delimited paragraph is shaped (or cache-hit) and broken
into lines independently; lines from every paragraph stack in the
final `TextLayout` in paragraph order, each baseline continuing the
running line count from the previous paragraph.

## Public surface

    crates/dashscene-typeset/src/text/mod.rs

    pub struct Typesetter { font: Font, cache: HashMap<Box<str>, Arc<ShapedText>>, hits, misses }
    impl Typesetter {
        pub fn new(font: Font) -> Typesetter;
        pub fn font(&self) -> &Font;
        pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout;
        pub fn cache_stats(&self) -> CacheStats;
    }
    pub struct CacheStats { pub hits: u64, pub misses: u64 }
    pub struct TextLayout { pub lines: Vec<Line>, pub width: f32, pub height: f32, pub size: f32 }
    pub struct Line { pub glyphs: Vec<PositionedGlyph>, pub width: f32, pub baseline_y: f32 }
    pub struct PositionedGlyph { pub glyph_id: u16, pub x: f32, pub y: f32 }
    pub struct Font { /* Arc<Vec<u8>>, face index, cached hhea metrics */ }
    pub struct ShapedText { pub glyphs: Vec<ShapedGlyph> }
    pub struct ShapedGlyph { pub glyph_id: u16, pub cluster: u32,
        pub x_advance: i32, pub x_offset: i32, pub y_offset: i32 }
    pub enum TypesetError { FontParse(String) }

`ShapedText`/`ShapedGlyph` and `Font` are public (re-exported from
`text::mod`) because the shaped-run cache and the font handle are
part of the #29/#30 surface — the measure callback and the painter
both need font metrics, and tests exercise shaping directly.
`shape()` itself and `Font::face()` stay crate-private; nothing
outside `dashscene-typeset` constructs a `rustybuzz::Face`.

## Coordinate conventions

Document space is y-down, origin at the layout's top-left. `baseline_y`
is a line's baseline measured from that origin. Each glyph's `(x, y)`
is its pen position on that baseline with the shaping offsets applied:
HarfBuzz/rustybuzz offsets are y-up, so `y = baseline_y - y_offset *
scale`. Per-glyph offsets are carried through, not dropped — GPOS
positions marks through offsets (spike #25 finding,
`docs/technotes/msdf-arabic-atlas-spike.md`). For offset-less Latin
glyphs the negation is a no-op (`g.y == baseline_y`); real coverage of
the negation direction arrives with the v0.6 Arabic story's GPOS mark
offsets.

The painter (#30) combines these pen positions with the atlas blob's
y-up `plane_em` quad (`docs/design/atlas-pipeline.md`'s metrics
model) — that conversion from `PositionedGlyph`'s document-space,
y-down pen position to a painted quad is the painter's, not this
crate's.

DESIGN §7.2's run tuple names `(glyph id, x, y, size, atlas page)`:
`size` lives once on `TextLayout` (uniform per layout in v0.5 — one
style per text node), and the atlas page field waits until multi-page
atlases exist; both are additive later without a `PositionedGlyph`
shape change.

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
`max_width: Option<f32>`: `None` means one line per paragraph
regardless of width.

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
  preserved: the first word always lands on a line unconditionally
  (there is no previous content to overflow), and the line's start
  index is never advanced past a leading space, so those glyphs stay
  in the emitted range.
- A word wider than `max_width` overflows its line rather than
  breaking mid-word. Mid-word breaking and UAX #14 line breaking are
  not v0.5 problems.
- An empty string lays out to zero lines and zero size.

## Cache semantics

The cache lives on `Typesetter` (`HashMap<Box<str>, Arc<ShapedText>>`
plus hit/miss counters) — it is not a separate type; the cache is too
small to warrant its own module.

- Stores font-unit, unpositioned `ShapedText` — shaping output (glyph
  ids, advances, offsets in font units) is size-independent, so the
  px scale is applied only at positioning time.
- Keyed by paragraph text alone. DESIGN §7.2 describes the key as
  "string + style"; while the font is fixed per `Typesetter` (v0.5:
  one font, no fallback), the shaping-relevant style component is
  fixed too, so the key reduces to the string. Different `size` and
  `max_width` values reuse the same cache entry. When style grows a
  shaping-relevant axis (font selection by weight/family), the key
  grows with it — see `docs/decisions/shaped-run-cache-font-units.md`.
- Unbounded in v0.5: cockpit UI text is a bounded set; an eviction
  policy is speculative until a real producer shows growth.
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

- **Bidi and RTL.** This pipeline is single-direction LTR by
  construction; bidi splitting and Arabic shaping are #32/#33 (v0.6).
- **UAX #14 line breaking.** Break points are ASCII space runs and
  `'\n'` only; the Unicode line-breaking algorithm arrives when
  non-Latin scripts and real content demand it — no issue number
  assigned yet, tracked alongside #32/#33.
- **Font fallback / charset unions.** One `Font` per `Typesetter`;
  fallback lists and per-locale charset coverage are #34.
- **Cache eviction.** The cache never shrinks in v0.5 (see Cache
  semantics above).
- **Arena wiring.** No dependency on `dashscene-core`; `Arena::text`/
  `Arena::text_style` reach this pipeline through #29, not through
  this crate.
- **Ligatures.** `liga`/`clig` are shaped off; see
  `docs/decisions/liga-clig-off-until-gsub-closure.md`.

## Components

    crates/dashscene-typeset/src/text/mod.rs     public surface:
                                                  Typesetter, TextLayout,
                                                  Line, PositionedGlyph,
                                                  CacheStats,
                                                  TypesetError; re-exports
                                                  Font, ShapedText,
                                                  ShapedGlyph
    crates/dashscene-typeset/src/text/font.rs    Font (bytes, face
                                                  index, hhea metrics);
                                                  builds the on-demand
                                                  rustybuzz::Face
    crates/dashscene-typeset/src/text/shape.rs   ShapedText/ShapedGlyph
                                                  (font units); the
                                                  rustybuzz shaping call
                                                  and its liga/clig
                                                  feature config
    crates/dashscene-typeset/src/text/layout.rs  break_lines (greedy
                                                  breaker), position_line
                                                  (baseline positioning),
                                                  line_advance

`atlas` (build-time, story #27) and `text` (runtime, this story) stay
sibling modules of the one typesetter crate (DESIGN §13,
`docs/design/atlas-pipeline.md`'s Home section). `Font::from_bytes`
validates with both `ttf-parser::Face::parse` (metrics) and
`rustybuzz::Face::from_slice` (shaping) up front; `rustybuzz::Face`
itself is constructed on demand inside `shape()` rather than stored on
`Font` — the shaped-run cache sits in front of shaping, so `Face`
construction is off the hot path. A self-referential holder or a
re-parsing wrapper crate would buy nothing here; revisit only with
profiling evidence.

## Testing (with the committed corpus Noto Sans)

- `font.rs`: metrics match `ttf-parser` directly (upem, ascender,
  descender, line gap); garbage bytes are rejected.
- `shape.rs`: "AV" shapes to two non-`.notdef` glyphs with cmap-
  matching ids and kerning that tightens the A→V advance below the
  plain hmtx sum; "fi" shapes to two glyphs (liga off proven);
  clusters are byte indices; empty text shapes to nothing.
- `tests/typeset_latin.rs` (integration, no external tool): `'\n'`
  forces a break; greedy wrap at a width that fits only "Hello" breaks
  "Hello world" into two lines with the broken-at space on neither
  line; a mid-line space keeps its glyph and widens the line; a
  single line's width equals the scaled hmtx advance sum; baselines
  advance by the line metric across two lines; a word wider than
  `max_width` overflows onto one line; empty text lays out to zero
  lines and zero size; the cache hits across different sizes/widths
  for the same text and misses on new text; offset-less Latin glyphs
  sit exactly on the baseline (documenting the negation convention).

## Out of scope (this story)

Bidi/RTL and Arabic shaping (#32/#33), font fallback and charset
unions (#34), the measure callback (#29), painting and atlas lookup
(#30), hyphenation/UAX #14, vertical text, letter-spacing and other
style axes, cache eviction.

## Trace

- Satisfies: DESIGN_1.md §7.2 (runtime shape → break → position,
  shaped-run cache), §2 (rustybuzz), P1, P2, R1; issue #28 acceptance
  criteria.
- Resolves: the #27 seam note in
  `docs/decisions/atlas-closure-cmap-plus-extras.md` (ligatures off
  until GSUB closure).
- Blocks: #29 (measure callback), #30 (glyph painting); v0.6 #32/#33
  build on this pipeline; #34 re-enables ligatures as one coordinated
  change with GSUB closure.
- Related design: `docs/design/atlas-pipeline.md` (build-time half,
  y-up `plane_em` convention), `docs/design/dashbuf.md` and
  `docs/design/dashscene-core-arena.md` (the #26 text intent this
  pipeline will be fed through at #29).
- Related decisions:
  `docs/decisions/liga-clig-off-until-gsub-closure.md`,
  `docs/decisions/shaped-run-cache-font-units.md`.
- Related technote: `docs/technotes/msdf-arabic-atlas-spike.md`.
