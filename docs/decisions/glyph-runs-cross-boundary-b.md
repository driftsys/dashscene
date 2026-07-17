# Glyph runs cross boundary B as a run table plus a plain-data atlas

    status   accepted (story #30, 2026-07-16)
    scope    dashpaint, dashscene-skia, goldens;
             docs/design/dashpaint.md, docs/design/dashscene-skia.md

## Context

v0.5 (text I: Latin) makes the reference painter draw positioned glyph
runs as textured MSDF atlas quads (`DESIGN_1.md` §7.2). Boundary B —
`dashpaint` — carried a rect table, a paint table, an image table, and a
clip table, but no glyph or text type.

Three facts constrain the addition:

- `DESIGN_1.md` §7.3 already names the painter input: "rect entries + the
  glyph-run table + a dirty set. That triple plus paint-table indices is
  the entire painter input (boundary B)." The glyph-run table is a
  first-class sibling of the rect table, not a side channel.
- P2 — one typesetter; a painter never measures, shapes, wraps, or moves
  anything. Runs must reach the painter already shaped, wrapped, and
  positioned by the one `dashscene-typeset` typesetter.
- `dashpaint` carries plain data and depends on no crate
  (`boundary-b-unification.md`, `dashpaint-owns-boundary-b-types.md`).
  Its `Color` and `Mat23` mirror `dashbuf`'s shapes rather than depend on
  `dashbuf`, so a painter depends only on `dashpaint`.

The painter also needs the atlas the runs sample. The build-time pipeline
(#27) produces an `AtlasBundle` (the MSDF image plus a metrics blob of
per-glyph `plane_em` / `atlas_px` bounds); the runtime typesetter (#28)
produces positioned glyph runs (`PositionedGlyph { glyph_id, x, y }`).

## Options

1. Add a glyph-run table to `Painter::paint`, as plain `dashpaint` types
   that mirror the typeset run and the atlas metrics; the table carries
   both the runs and the atlases they reference.
2. Make `dashpaint` depend on `dashscene-typeset` and re-export its
   `PositionedGlyph` and atlas-metrics types.
3. Leave `paint` unchanged and add a second trait method (`paint_text`)
   with a default no-op, so text-free painters need not change.

## Choice

Option 1. One new parameter on the single `paint` call,
`glyphs: &GlyphRunTable`:

- `GlyphQuad { glyph_id, x, y }` — one placed glyph in **absolute**
  document space (the mirror of typeset's `PositionedGlyph`).
- `GlyphRun { atlas, size, color, glyphs }` — a run of glyphs that share
  a render size, a fill color, and an atlas (one style per text node in
  v0.5).
- `Atlas { image, width, height, px_per_em, distance_range_px, glyphs }`
  — a plain mirror of the metrics blob; `AtlasGlyph { glyph_id,
  plane_em, atlas_px }` per painted glyph, sorted by glyph id.
- `GlyphRunTable` holds the runs and the atlases they index. Empty
  (`GlyphRunTable::new()`) for a text-free scene.

## Why

- **A parameter, not a second method (over option 3).** §7.3 defines the
  painter input as one triple; a second method would split one contract
  in two and let a painter honor rects while silently ignoring text. The
  cost is mechanical: every existing caller passes an empty table.
- **Plain data, not a typeset dependency (over option 2).** A
  `dashpaint -> dashscene-typeset` edge would pull the whole shaping stack
  (rustybuzz, ttf-parser, unicode-bidi) into every painter, against the
  lean-painter goal (R3) and the reason boundary-B types mirror `dashbuf`
  rather than depend on it. A stager converts the metrics blob into the
  boundary-B `Atlas`, exactly as image bytes become an `ImageAsset`
  (`image-assets-cross-boundary-b.md`).
- **Absolute positions keep P2.** Runs cross the boundary already placed;
  whoever stages a run adds the text node's resolved box origin, so the
  painter draws quads and never adds an origin — the same posture
  `resolved-clip-regions-at-commit.md` took for clip boxes.
- **The atlas travels with the runs.** A run's glyph ids are meaningless
  without the atlas that places them, so bundling them keeps the text
  payload self-contained, the way an image fill needs its `ImageTable`
  entry.

## Consequences

- `Painter::paint` grows one parameter. Every current caller (tests and
  goldens) passes `GlyphRunTable::new()`; the trait stays the single
  painter contract.
- v0.5 composites every run over all rects (text is foreground). A full
  z-interleave of runs with rects, and clipped runs, are later work,
  noted at the trait.
- The representation is defined and the reference painter consumes it
  now; wiring `dashscene-core`'s `commit` to **emit** the glyph-run table
  (running the typesetter at commit) is a later producer story. v0.5
  stages runs at boundary B from the same typesetter the measure callback
  (#29) used, so measure and paint agree by construction — the same way
  the v0.3 paint vocabulary was hand-staged at boundary B before a
  producer emitted it.
- MSDF resolve is anti-aliased at every glyph edge, so the Latin text
  golden compares with a tolerance (`golden-comparison-space.md`), not
  bit-exact. The painter's per-glyph unit tests stay exact by using a
  synthetic all-inside atlas.

## Resolution (story #219, 2026-07-16) — multi-font fallback

Font fallback widened this contract as-built, and the widening is
**conceptual, not structural**: no `dashpaint` type changed. Option 1
already made the table multi-atlas — `GlyphRunTable::push_atlas`
returns an `AtlasIndex`, and each `GlyphRun` names the `atlas` it
samples — so a single scene could carry runs against different
atlases from the start. Through v0.6 every scene used one atlas because
one text node had one style and one font (the "one style per text node
in v0.5" note above). Story #219 exercises the latent capability: a
single mixed-script text node now shapes across an ordered font list
(`docs/design/typeset-latin.md`, Font fallback), so the stager splits
its layout into **one glyph run per font**, each referencing that
font's atlas. The Skia painter already decodes every atlas in
`GlyphRunTable::atlases()` and samples the run's own atlas
(`decoded[run.atlas.0]`), so it needed no change either.

What did grow is upstream of boundary B, in the typeset output:
`dashscene-typeset`'s `PositionedGlyph` gained a `font` index (the
cascade's result). That index is what a stager groups a line's glyphs
by — consecutive same-font glyphs become one `GlyphRun` against that
font's `AtlasIndex`. The boundary-B `GlyphQuad` stays
`{ glyph_id, x, y }`: the font-to-atlas mapping is resolved on the
producer side of the boundary, exactly as absolute positions are, so
the painter still only draws quads (P2). A future commit-time stager
(#160) reads `PositionedGlyph::font` the same way the goldens'
staging helpers do now (`goldens/tooling/tests/v07_fallback.rs`).

Per-fallback-font atlases follow the committed-fixture convention
unchanged: the mixed-script golden reuses the two existing
R7-reproducible fixtures — `corpus/atlas/arabic` (primary) and
`corpus/atlas/ascii` (Latin fallback) — each already carrying its own
regenerator and cross-machine reproducibility test
(`docs/design/atlas-pipeline.md`, Determinism). One atlas per font is
the charset-union-per-font posture the spike pinned
(`docs/decisions/atlas-closure-cmap-plus-extras.md`).

## Resolution (story #44, 2026-07-17) — free-path group alpha on runs

Group opacity (`docs/decisions/masks-and-group-opacity.md`) added a
`GlyphRun::opacity` field, mirroring `RectEntry::opacity`: a group opacity
that took the free path folds into it, and the painter multiplies the run's
fill alpha by it. The **render-target** group path and clip/mask regions are
still not applied to glyph runs — a run draws as foreground, not composited
into a group's offscreen layer nor clipped to a region — because that needs
the full z-interleave of runs with rects this record already deferred. The
paint gate names the combination (`paint.text-outside-group`), so a text
node inside an overlapping partial-opacity group is a named limitation, not
a silent wrong pixel. Compositing runs into group layers and clipping runs
to clip/mask regions are debt candidates.
