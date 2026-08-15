# Decision: the shaped-run cache stores font-unit runs keyed by paragraph text alone

    status   accepted (story #28, 2026-07-12)
    scope    crates/dashscene-typeset text module — Typesetter::cache
             (mod.rs), ShapedText (shape.rs)

## Context

`docs/design/typeset-latin.md` describes the shaped-run cache as keyed by
"string + style". Story #28 had to decide what the cache stores (positioned
pixels or unpositioned font-unit data) and what exactly the key covers, given
that `Typesetter` in v0.5 holds exactly one `Font` (no fallback list yet — that
is #34) and one text node style maps to one render `size`.

## Options

1. Cache font-unit, unpositioned `ShapedText` (glyph ids, advances, offsets
   straight from rustybuzz), keyed by the paragraph text alone. Positioning
   (scaling by `size / units_per_em`, computing baselines) happens fresh on
   every `layout()` call from the cached `ShapedText`.
2. Cache fully positioned `TextLayout`s (or `Line`s), keyed by
   `(text, size, max_width)`.
3. Cache a constructed `rustybuzz::Face` (self-referential, or held behind a
   leaked/pinned wrapper) so repeated shaping calls skip font parsing.

## Choice

Option 1. `Typesetter::cache: HashMap<Box<str>, Arc<ShapedText>>`
(`crates/dashscene-typeset/src/text/mod.rs`) is keyed by the paragraph's text;
`Typesetter::shaped` looks up or inserts, then every `layout()` call scales and
positions the (possibly shared) cached `ShapedText` against the requested
`size`/`max_width`. This is a refinement of `docs/design/typeset-latin.md`'s
"string + style" key: while the font is fixed per `Typesetter`, the only
shaping-relevant style component is already pinned, so the key reduces to the
string alone — a `(text,
size)` pair is not needed because shaping output (glyph
ids, advances, offsets in font units) does not depend on size at all. Proven by
`cache_hits_across_sizes_and_counts`: the same text at two different sizes
produces one miss and two hits.

## Why

- Shaping (running rustybuzz over a `UnicodeBuffer`) is the expensive step;
  scaling font-unit numbers by `size / units_per_em` is a cheap multiplication
  done at every `layout()` call regardless. Option 1 caches exactly the
  expensive part, once, and lets every render size and every `max_width` reuse
  it.
- Option 2 re-shapes nothing extra on a cache miss, but re-caches the same
  shaping work under a new key for every distinct `(size, max_width)`
  combination of the same text — for cockpit UI text re-rendered at a handful of
  sizes (for example a resizable panel), that multiplies cache entries and cache
  misses for no benefit, since the underlying shaped glyphs never actually
  changed.
- Option 3 (holding a constructed `rustybuzz::Face`) provides no benefit once
  option 1 is in place: the cache already sits in front of shaping, so `Face`
  construction is off the hot path for repeated text. Holding a `Face` would
  additionally require a self-referential struct (a `Face` borrows the font
  bytes) or a leaked/pinned wrapper, for a cost the cache already hides. See
  also `docs/design/typeset-latin.md`'s Components section on `Font::face()`.
- When style grows a shaping-relevant axis (font selection by weight or family,
  once #34 adds fallback lists), the key grows with it — this decision covers
  only the v0.5 single-font case.

## Consequences

- The cache is unbounded in v0.5 (no eviction): cockpit UI text is a bounded set
  of strings, and an eviction policy ahead of a real producer showing unbounded
  growth would be speculative. Revisit if a producer's text set turns out to be
  large or unbounded.
- `Typesetter::cache_stats() -> CacheStats { hits, misses }` exposes hit/miss
  counters so tests and #29's caller can observe cache behavior instead of
  inferring it indirectly.
- `ShapedText`/`ShapedGlyph` stay crate-private: they are the cache-value
  representation, and publishing them before a consumer exists would freeze it
  into the public API. If #29/#30 turn out to need direct access to shaped,
  unpositioned runs, exporting them then is an additive change.

## Resolution (story #219, 2026-07-16) — the key stayed the text

Multi-font fallback landed in v0.7 (the "Why" bullet above named it as #34; it
was deferred to #219, `docs/decisions/font-fallback-deferred-past-v06.md`), and
the cache key did **not** grow. The bullet anticipated a growing key because it
treated fallback as a per-`layout`-call axis; as built, the ordered font list is
fixed per `Typesetter` (runtime configuration), so the cascade — which font each
codepoint resolves to — is a pure function of the paragraph text, exactly as
bidi levels and digit contexts already were. The key stays the text alone. The
cached `ShapedText` now records the cascade's result, one `font` index per
`ShapedGlyph`, so a mixed-script paragraph is cascaded and shaped once and
reused across render sizes — proven by
`tests/typeset_fallback.rs::cache_key_is_text_across_sizes_for_a_multi_font_typesetter`
(one miss, two hits). Only a shaping-relevant axis that varies per `layout` call
would grow the key.

## Revision (story #341, then #368) — two per-call axes appeared

The paragraph above closed by saying no per-`layout`-call shaping axis existed.
Two have since landed, so the key did grow:

- **#341** added `ligatures_off`, a per-call knob rather than a property of the
  text. It was first handled with a second map (`cache_ligatures_off`), which
  works for one boolean and does not generalise.
- **#368** added the requested font weight. Weight changes advances, kerning and
  potentially glyph ids, so it is a shaping input, not only a rasterisation
  input.

As built after #368 the key is `(text, posture)`, where a **posture** is one
interned `(resolved slot set, ligatures_off)` pair. The resolved slot set is the
face each family resolves to for the requested weight, so two requested weights
that resolve to the same faces share one cache entry. The all-weight-400,
ligatures-on posture is interned as id 0, so the default path stays a single
lookup and behaves exactly as it did before #368.

The reasoning in the paragraph above still holds for the part it got right: the
font _list_ is fixed per `Typesetter`, and the cascade is a pure function of the
text **for a given posture**. What changed is that the posture itself now varies
per call.

Why the resolved slot set — rather than the requested weight — is the key
component is part of the cascade design:
`docs/decisions/weight-selection-in-the-cascade.md`. That the split is real is
pinned by `tests/typeset_weight.rs::each_weight_gets_its_own_cache_entry` and
`::the_same_string_at_two_weights_shapes_differently`; that the default path is
unchanged is pinned by `::with_fonts_is_the_all_regular_cascade`.

## Revision (issue #975, 2026-08-15) — the revisit condition was met

The first Consequences bullet made the no-eviction choice conditional and named
the condition to revisit it: "Revisit if a producer's text set turns out to be
large or unbounded." That condition is now met, so the bullet is superseded
rather than merely dated.

What met it is issue #621, not a new producer. Its fix made `stage_text` a
per-frame call rather than a per-solve one, so a text node whose string differs
every frame — a clock, a formatted numeric readout, a counter — presents a key
that has never been seen before at frame rate, for the process lifetime. That is
the headline case issue #621 existed to fix, so the growth is a property of the
supported feature set rather than of a hypothetical producer.

Both text-keyed caches are now bounded at `text::CACHE_CAPACITY` paragraphs and
evict the least recently used entry. The bidi cache added by issue #225 is
included: it carries the same key and grew for the same reason.

Three points the original reasoning did not settle:

- **Recency rather than insertion order.** A real scene lays out its fixed
  labels every frame alongside the one changing readout. Evicting by insertion
  order would drop the labels, which are the entries worth keeping, and retain
  the readout strings, which are never asked for again. Pinned by
  `tests/typeset_cache_bound.rs::a_paragraph_used_every_frame_survives_a_flood_of_new_ones`.
- **The capacity is a working-set bound, not a memory budget.** No memory budget
  exists in this project to size a cache against — that is issue #462, which
  `dashscene-skia`'s `ImageCache` also waits on. The constraint that does exist
  is that the capacity must exceed the distinct paragraphs one frame lays out,
  or eviction moves shaping into the frame loop. Pinned from both sides by
  `::a_changing_string_cannot_grow_the_caches_without_bound` and
  `::a_working_set_under_the_capacity_is_never_evicted`.
- **The bound is per posture for shaped runs, and global for bidi.** Each
  posture's map bounds itself independently, because a posture is a distinct
  shaping result; the bidi resolution has no posture, so one bounded map serves
  every posture.

The second Consequences bullet is also stale as written. `CacheStats` has not
carried only `{ hits, misses }` since issue #225, and this change adds
`shaped_entries`, `bidi_entries` and `evictions` to it. The first two are what
make the bound assertable: every field the struct carried before this change was
a monotone event counter, so `misses` keeps climbing across an eviction and none
of them could observe how much the cache actually holds. `evictions` separates
the two states the others cannot tell apart — a working set inside the capacity
and one far past it both show the entry count pinned and `misses` climbing, and
only the second is reshaping paragraphs it just dropped.

**`clear` is deliberately not added, and issue #975 asked for it.** The issue
names three absences — "no eviction, no clear and no capacity bound" — and this
change closes the first and third. A `Typesetter` outliving the document it was
built for keeps up to `postures * CACHE_CAPACITY` paragraphs of the old
document's text, so a host loading a second `.dsb` carries the first one's
labels until something displaces them.

That is bounded staleness rather than a leak, which is what makes deferring it
defensible: the entries cost a fixed ceiling, they are displaced by the new
document's own text as it lays out, and no caller in this workspace holds a
`Typesetter` across two document loads today. Adding a public `clear` with no
caller would be building the API before the case for its shape exists — in
particular whether a host wants to drop everything or only the postures the new
document does not use, which the second document's cascade decides. It is issue
#1004, on the v0.20 milestone, rather than left implicit.
