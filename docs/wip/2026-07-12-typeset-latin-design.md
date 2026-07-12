# Design: dashscene-typeset — Latin pipeline (#28)

    status   working memory (Superpowers spec) — gardened on story finish
    story    #28 (epic #24, v0.5 — text I: Latin)
    date     2026-07-12
    traces   DESIGN_1.md §7.2 (runtime half: bidi split → shape → line
             break → positioned glyph runs; shaped-run cache), §2
             (rustybuzz, ttf-parser), P1/P2 (one typesetter; painters
             only color), R1 (text quality),
             docs/design/atlas-pipeline.md (seam notes),
             docs/technotes/msdf-arabic-atlas-spike.md (offsets finding)
    blocks   #29 (measure callback), #30 (glyph painting); v0.6 #32/#33
             build on this pipeline

## Purpose

The runtime half of DESIGN §7.2, Latin subset: text + style in,
positioned glyph runs out — shaped once by rustybuzz, broken into
lines, positioned on baselines, cached so re-layout of unchanged text
costs a lookup. Bidi splitting is v0.6 (#32); this story's pipeline is
single-direction LTR by construction.

## Shape of the pipeline

    Typesetter::new(Font)                       one font in v0.5;
                                                fallback lists are the
                                                v0.6 charset story's
    Typesetter::layout(text, size, max_width)   → TextLayout
      cache lookup (key: text)                  → ShapedText (font
        miss: rustybuzz shape, liga off           units, unpositioned)
      greedy line break (space / '\n')          → lines of glyph slices
      position (scale by size/upem, baselines)  → TextLayout

    TextLayout { lines: Vec<Line>, width, height, size }
    Line { glyphs: Vec<PositionedGlyph>, width, baseline_y }
    PositionedGlyph { glyph_id: u16, x: f32, y: f32 }

Coordinates: document space, y-down, origin at the layout's top-left.
`baseline_y` is the line's baseline; each glyph's `(x, y)` is its pen
position on that baseline with the shaping offsets applied (HarfBuzz
offsets are y-up, so `y = baseline_y - y_offset_scaled`). Per-glyph
offsets are carried, not dropped — GPOS positions marks through
offsets (spike #25 finding). The painter (#30) combines these pen
positions with the atlas blob's y-up `plane_em` quads; that conversion
is the painter's, documented on both types.

DESIGN §7.2's run tuple names "(glyph id, x, y, size, atlas page)":
`size` lives once on the `TextLayout` (uniform per layout in v0.5, one
style per text node), and the atlas page field waits until multi-page
atlases exist — both are additive later.

## Decisions

**Font ownership.** `Font` owns the font bytes (`Arc<Vec<u8>>` + face
index) and exposes the vertical metrics (ttf-parser hhea, the same
numbers the atlas blob carries). rustybuzz's `Face` borrows the bytes,
so it is constructed on demand inside shaping rather than stored (a
self-referential holder or a re-parsing wrapper crate would buy
nothing here: the run cache sits in front of shaping, so `Face`
construction is off the hot path). Revisit only with profiling
evidence.

**Cache stores font units; the key is the text alone.** Shaping output
(glyph ids, advances, offsets in font units) is size-independent — the
px scale is a multiplication at positioning time. Caching unpositioned
font-unit runs keyed by text therefore covers every size with one
entry, and DESIGN §7.2's "keyed string+style" reduces to "keyed
string" while the shaping-relevant style component (the font) is fixed
per `Typesetter`. When style grows shaping-relevant axes (font
selection by weight/family), the key grows with it. The cache is
unbounded in v0.5 (cockpit UI text is a bounded set; an eviction
policy is speculative until a real producer shows growth) and exposes
hit/miss counters so tests and #29 can observe it.

**Ligatures stay off in v0.5.** Shaping runs with `liga`/`clig`
disabled: the atlas closure is cmap-only (#27, decision record
`atlas-closure-cmap-plus-extras.md`), so a ligature glyph would shape
to a gid the atlas cannot cover, and P4 forbids silently losing it at
paint time. This resolves the seam note the #27 design left for this
story. Ligatures return with GSUB closure at #34, as one coordinated
change (enable features + close the charset over GSUB). Kerning
(GPOS `kern`) stays on — it moves pen positions, needs no atlas
coverage.

**Line breaking is greedy word wrap.** Break opportunities: after
runs of ASCII space (which collapse at line ends — a broken-at space
contributes no width to either line) and at explicit `'\n'`. A word
wider than `max_width` overflows its line rather than breaking
mid-word (documented; mid-word breaking and UAX #14 line breaking are
not v0.5 problems — the corpus that needs them arrives with real
content). `max_width: Option<f32>`: `None` = single line per `'\n'`
segment.

**Line metrics.** ascent/descent/line-gap from hhea (the blob's
numbers, `docs/design/atlas-pipeline.md`): first baseline at
`ascent`, line advance = `ascent - descent + line_gap` (descent is
negative in font units), all scaled by `size / units_per_em`.
`TextLayout.width` = widest line; `.height` = line count × line
advance (the standard box model painters and the #29 measure callback
both want).

Alternatives considered:

- _Cache positioned layouts keyed (text, size, max_width)_ — every
  size/width combination re-caches the same shaping work; the
  font-unit split caches the expensive part exactly once. Rejected.
- _Store a constructed `rustybuzz::Face` (self-referential or
  leaked)_ — complexity/deps for a cost the cache already hides.
  Rejected.
- _UAX #14 line breaking now_ — Latin UI strings in v0.5 goldens need
  space/newline only; the algorithm arrives when non-Latin scripts and
  real content demand it. Rejected for now.
- _Ligatures on + `extra_glyph_ids` at atlas build_ — couples every
  atlas build to a hand-maintained gid list; one coordinated
  GSUB-closure change at #34 is strictly better. Rejected.

## Components

    crates/dashscene-typeset/src/text/mod.rs     public surface:
                                                 Typesetter, TextLayout,
                                                 Line, PositionedGlyph,
                                                 TypesetError
    crates/dashscene-typeset/src/text/font.rs    Font (bytes, index,
                                                 vertical metrics)
    crates/dashscene-typeset/src/text/shape.rs   ShapedText/ShapedGlyph
                                                 (font units), rustybuzz
                                                 wrapper, feature config
    crates/dashscene-typeset/src/text/layout.rs  greedy breaker +
                                                 positioning
    (cache lives in mod.rs inside Typesetter: HashMap<Arc<str>,
    Arc<ShapedText>> + counters — too small for its own file)

`atlas` (build-time) and `text` (runtime) stay sibling modules of the
one typesetter crate (DESIGN §13).

## Error handling

`TypesetError::FontParse(String)` on `Font::new` for unparseable
bytes — everything after construction is infallible over valid input
(shaping an empty or unknown-codepoint string produces `.notdef`
glyphs, not errors; P4's named-diagnostic surface for missing glyphs
is the painter's at #30, where the atlas lookup happens).

## Testing (with the committed corpus Noto Sans)

- Font: metrics match ttf-parser directly (upem 1000, ascent > 0).
- Shaping: "AV" → two non-`.notdef` glyphs with cmap-matching ids;
  kerning moves the A→V advance below the plain hmtx sum; "fi" →
  two glyphs (liga off proven); offsets preserved in ShapedGlyph.
- Cache: two layouts of one string → 1 miss then 1 hit; different
  size still hits (font-unit invariance); different text misses.
- Layout: explicit '\n' breaks; greedy wrap at a width that forces
  "Hello world" onto two lines (second line starts with the 'w'
  glyph); broken-at space contributes no width; single-line width ==
  scaled advance sum; baselines advance by the line metric; empty
  string → zero-size layout with no lines.
- Overflow: one word wider than max_width stays on one line.

## Out of scope (this story)

Bidi/RTL and Arabic shaping (#32/#33), font fallback and charset
unions (#34), the measure callback (#29), painting and atlas lookup
(#30), hyphenation/UAX #14, vertical text, letter-spacing and other
style axes, cache eviction.
