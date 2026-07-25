# Decision: the shaped-run cache stores font-unit runs keyed by paragraph text alone

    status   accepted (story #28, 2026-07-12)
    scope    crates/dashscene-typeset text module — Typesetter::cache
             (mod.rs), ShapedText (shape.rs)

## Context

`docs/design/typeset-latin.md` describes the shaped-run cache as keyed by
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
refinement of `docs/design/typeset-latin.md`'s "string + style" key: while the font is
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
- Option 3 (holding a constructed `rustybuzz::Face`) provides no benefit once
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
- `ShapedText`/`ShapedGlyph` stay crate-private: they are the
  cache-value representation, and publishing them before a consumer
  exists would freeze it into the public API. If #29/#30 turn out to
  need direct access to shaped, unpositioned runs, exporting them
  then is an additive change.

## Resolution (story #219, 2026-07-16) — the key stayed the text

Multi-font fallback landed in v0.7 (the "Why" bullet above named it
as #34; it was deferred to #219,
`docs/decisions/font-fallback-deferred-past-v06.md`), and the cache key
did **not** grow. The bullet anticipated a growing key because it
treated fallback as a per-`layout`-call axis; as built, the ordered
font list is fixed per `Typesetter` (runtime configuration), so the
cascade — which font each codepoint resolves to — is a pure function of
the paragraph text, exactly as bidi levels and digit contexts already
were. The key stays the text alone. The cached `ShapedText` now records
the cascade's result, one `font` index per `ShapedGlyph`, so a
mixed-script paragraph is cascaded and shaped once and reused across
render sizes — proven by
`tests/typeset_fallback.rs::cache_key_is_text_across_sizes_for_a_multi_font_typesetter`
(one miss, two hits). Only a shaping-relevant axis that varies per
`layout` call would grow the key.

## Revision (story #341, then #368) — two per-call axes appeared

The paragraph above closed by saying no per-`layout`-call shaping axis
existed. Two have since landed, so the key did grow:

- **#341** added `ligatures_off`, a per-call knob rather than a property
  of the text. It was first handled with a second map
  (`cache_ligatures_off`), which works for one boolean and does not
  generalise.
- **#368** added the requested font weight. Weight changes advances,
  kerning and potentially glyph ids, so it is a shaping input, not only
  a rasterisation input.

As built after #368 the key is `(text, posture)`, where a **posture** is
one interned `(resolved slot set, ligatures_off)` pair. The resolved
slot set is the face each family resolves to for the requested weight,
so two requested weights that resolve to the same faces share one cache
entry. The all-weight-400, ligatures-on posture is interned as id 0, so
the default path stays a single lookup and behaves exactly as it did
before #368.

The reasoning in the paragraph above still holds for the part it got
right: the font _list_ is fixed per `Typesetter`, and the cascade is a
pure function of the text **for a given posture**. What changed is that
the posture itself now varies per call.

Why the resolved slot set — rather than the requested weight — is the key
component is part of the cascade design:
`docs/decisions/weight-selection-in-the-cascade.md`. That the split is
real is pinned by
`tests/typeset_weight.rs::each_weight_gets_its_own_cache_entry` and
`::the_same_string_at_two_weights_shapes_differently`; that the default
path is unchanged is pinned by `::with_fonts_is_the_all_regular_cascade`.
