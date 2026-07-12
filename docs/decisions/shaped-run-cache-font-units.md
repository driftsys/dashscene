# Decision: the shaped-run cache stores font-unit runs keyed by paragraph text alone

    status   accepted (story #28, 2026-07-12)
    scope    crates/dashscene-typeset text module — Typesetter::cache
             (mod.rs), ShapedText (shape.rs)

## Context

DESIGN_1.md §7.2 describes the shaped-run cache as keyed by
"string + style". Story #28 had to decide what the cache stores
(positioned pixels or unpositioned font-unit data) and what exactly
the key covers, given that `Typesetter` in v0.5 holds exactly one
`Font` (no fallback list yet — that is #34) and one text node style
maps to one render `size`.

## Options

1. Cache font-unit, unpositioned `ShapedText` (glyph ids, advances,
   offsets straight from rustybuzz), keyed by the paragraph text
   alone. Positioning (scaling by `size / units_per_em`, computing
   baselines) happens fresh on every `layout()` call from the cached
   `ShapedText`.
2. Cache fully positioned `TextLayout`s (or `Line`s), keyed by
   `(text, size, max_width)`.
3. Cache a constructed `rustybuzz::Face` (self-referential, or held
   behind a leaked/pinned wrapper) so repeated shaping calls skip
   font parsing.

## Choice

Option 1. `Typesetter::cache: HashMap<Box<str>, Arc<ShapedText>>`
(`crates/dashscene-typeset/src/text/mod.rs`) is keyed by the
paragraph's text; `Typesetter::shaped` looks up or inserts, then every
`layout()` call scales and positions the (possibly shared) cached
`ShapedText` against the requested `size`/`max_width`. This is a
refinement of DESIGN §7.2's "string + style" key: while the font is
fixed per `Typesetter`, the only shaping-relevant style component is
already pinned, so the key reduces to the string alone — a `(text,
size)` pair is not needed because shaping output (glyph ids, advances,
offsets in font units) does not depend on size at all. Proven by
`cache_hits_across_sizes_and_counts`: the same text at two different
sizes produces one miss and two hits.

## Why

- Shaping (running rustybuzz over a `UnicodeBuffer`) is the expensive
  step; scaling font-unit numbers by `size / units_per_em` is a cheap
  multiplication done at every `layout()` call regardless. Option 1
  caches exactly the expensive part, once, and lets every render size
  and every `max_width` reuse it.
- Option 2 re-shapes nothing extra on a cache miss, but re-caches the
  same shaping work under a new key for every distinct
  `(size, max_width)` combination of the same text — for cockpit UI
  text re-rendered at a handful of sizes (for example a resizable
  panel), that multiplies cache entries and cache misses for no
  benefit, since the underlying shaped glyphs never actually changed.
- Option 3 (holding a constructed `rustybuzz::Face`) buys nothing once
  option 1 is in place: the cache already sits in front of shaping, so
  `Face` construction is off the hot path for repeated text. Holding a
  `Face` would additionally require a self-referential struct (a
  `Face` borrows the font bytes) or a leaked/pinned wrapper, for a
  cost the cache already hides. See also `docs/design/typeset-latin.md`'s
  Components section on `Font::face()`.
- When style grows a shaping-relevant axis (font selection by weight
  or family, once #34 adds fallback lists), the key grows with it —
  this decision covers only the v0.5 single-font case.

## Consequences

- The cache is unbounded in v0.5 (no eviction): cockpit UI text is a
  bounded set of strings, and an eviction policy ahead of a real
  producer showing unbounded growth would be speculative. Revisit if
  a producer's text set turns out to be large or unbounded.
- `Typesetter::cache_stats() -> CacheStats { hits, misses }` exposes
  hit/miss counters so tests and #29's caller can observe cache
  behavior instead of inferring it indirectly.
- `ShapedText`/`ShapedGlyph` are public types (not merely an internal
  cache value) because #29/#30 may need to inspect shaped, unpositioned
  runs directly.
