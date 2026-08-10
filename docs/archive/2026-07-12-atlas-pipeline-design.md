# Design: atlas pipeline — font → MSDF glyph atlas + metrics blob (#27)

    status   working memory (Superpowers spec) — gardened on story finish
    story    #27 (epic #24, v0.5 — text I: Latin)
    date     2026-07-12
    traces   DESIGN_1.md §7.2 (build-time half), §2 (stack: msdf-atlas-gen,
             ttf-parser), R1 (text quality), R7 (byte-reproducible builds),
             P4 (validated vocabulary, named diagnostics),
             docs/decisions/q1-msdf-below-14px.md (32 px/em, pxrange 4),
             docs/technotes/arabic-atlas-coverage.md (#25 evidence)
    blocks   #30 (Skia glyph quads), #34 (per-locale charsets)

## Purpose

The build-time half of DESIGN §7.2: given a font file and a charset,
produce the two artifacts every painter consumes at runtime —

    font.ttf ──► glyph atlas image (MSDF, keyed by GLYPH ID)
             ──► metrics blob (font + per-glyph + atlas parameters)

Both artifacts are byte-reproducible from the same inputs (R7). The
charset is an input parameter; per-locale charsets arrive with #34.

## Contract pins (fixed before this design)

- The atlas is keyed by **glyph id**, never by codepoint — contextual
  forms are just glyphs via GSUB (DESIGN §7.2, confirmed by spike #25:
  Noto Sans Arabic is unrepresentable under codepoint keying).
- Generator: `msdf-atlas-gen`, `-type msdf -size 32 -pxrange 4`,
  pinned tool version and seed (spike #25 + Q-1 decision record).
- The atlas JSON's `distanceRange`/`size` must travel into the metrics
  blob — the painter's screen-pixel-range computation needs them.

## Home

`dashscene-typeset` (DESIGN §13 maps the atlas pipeline to the text
crate). New module `atlas`, public API re-exported from the crate root.
No CLI in this story — `dashc` owns command-line surfaces; a cargo
example regenerates the committed test fixture.

## Approach A — how the generator runs

**Chosen: shell out to an external, version-pinned `msdf-atlas-gen`
binary.** Discovery: `MSDF_ATLAS_GEN` env var override, else `PATH`.
The pipeline runs `msdf-atlas-gen` with no input file to print the
banner, parses the version, and refuses to run with anything but the
pinned version (named error, per P4 posture — no silent drift). Pinned:
`1.4.0` (the spike-validated version).

Alternatives considered:

- _Pure-Rust MSDF crates (`fdsm`, `msdf`)_ — replaces the component the
  spike validated with an unvalidated one; output quality and parity
  with msdf-atlas-gen unknown; would invalidate spike evidence.
  Rejected for v0; revisit only if the external-tool dependency becomes
  a real operational problem.
- _Vendor the C++ and build via `build.rs`_ — makes every workspace
  build pay a C++ toolchain cost and complicates contributor setup for
  a tool that runs at asset-build time only. Rejected.

Availability: macOS devs `brew install msdf-atlas-gen` (bottled 1.4.0);
upstream publishes Windows-only release binaries, so Linux CI builds
the pinned tag from source once and caches the binary (see Testing/CI).

## Approach B — the metrics blob format

**Chosen: a versioned Rust struct serialized with `postcard`** (small,
stable wire format, deterministic given deterministic field order), file
name `atlas.metrics` next to `atlas.png`. Vectors are explicitly sorted
(glyphs by glyph id, missing codepoints ascending) so serialization is
canonical. Loader (`AtlasBundle::load_from_dir`) round-trips it.

Alternatives considered:

- _Canonical JSON_ — float-to-text formatting is a reproducibility trap
  and the blob is machine-only data. Rejected.
- _Hand-rolled binary writer_ — more code for no benefit over postcard's
  stable format. Rejected.
- _Flatbuffer in `dashbuf` now_ — the atlas is an asset, not the
  document; packaging atlases into `.dsb` sections is a later slice's
  concern (R5 section discipline). The typed loader isolates that
  future change from painters. Rejected for now.

## Approach C — charset → glyph-id closure

**Chosen: cmap-only closure plus an `extra_glyph_ids` input.** For each
charset codepoint, resolve the nominal glyph via `ttf-parser`'s cmap;
codepoints without coverage go into a `missing_codepoints` list in the
blob (R6: named diagnostic surface, never a silent drop — the caller
decides severity). `extra_glyph_ids` lets a caller add glyphs that only
shaping can discover.

Alternatives considered:

- _Full GSUB closure now_ — required for Arabic (#34/v0.6), where
  charset-declared coverage must pull contextual forms; rustybuzz
  exposes shaping, not glyph closure, so this needs real design work.
  Deferred to #34, which the `extra_glyph_ids` parameter already
  future-proofs: the contract does not change when closure improves.

Seam note for #28/#30: with cmap-only closure, discretionary/standard
Latin ligature glyphs (`fi`, `fl`) are not in the atlas. #28 decides
whether v0.5 shaping disables `liga` or feeds the ligature glyph ids in
via `extra_glyph_ids`; #30 must treat a shaped glyph id absent from the
atlas as a named diagnostic, not a silent skip (P4).

## Components

    atlas/mod.rs       public API: AtlasSpec, AtlasBundle, AtlasError,
                       generate()
    atlas/closure.rs   charset → (sorted gid set, missing codepoints)
                       via ttf-parser cmap
    atlas/tool.rs      binary discovery, version gate, invocation,
                       JSON layout parse (serde types stay private)
    atlas/metrics.rs   AtlasMetrics model, font-metrics extraction
                       (ttf-parser), postcard write/load
    examples/generate_fixture.rs
                       regenerates the committed ASCII fixture
    corpus/fonts/noto-sans/
                       NotoSans-Regular.ttf v2.015 (OFL, license file
                       committed alongside) — shared by #28/#29/#30

## Data flow

    AtlasSpec { font_path, charset, extra_glyph_ids,
                px_per_em = 32, px_range = 4, seed = 1 }
      → closure(font, charset)            → (gids, missing)
      → tool::run(font, gids ∪ extras)    → (atlas.png bytes, layout JSON)
      → metrics::build(font, layout, …)   → AtlasMetrics
      → AtlasBundle { image_png, metrics }
      → write_to_dir / load_from_dir      → atlas.png + atlas.metrics

## The metrics model

    AtlasMetrics
      format_version   u32 (= 1)
      generator        tool_version String, args Vec<String>  (provenance:
                       rerunning args must reproduce the artifacts)
      font             units_per_em u16, ascender/descender/line_gap in
                       raw font units (i16 — exact integers, consumers
                       normalize by upem; same numbers FreeType reads)
      atlas            width/height px u32, px_per_em u16,
                       distance_range_px f32, y-origin: bottom (fixed),
                       kind: MSDF (fixed)
      glyphs           Vec<GlyphEntry>, sorted by glyph id:
                         glyph_id u16
                         advance_units u16          (hmtx, authoritative)
                         plane_em  Option<[f32;4]>  (l,b,r,t quad in em,
                                                     y-up, baseline origin)
                         atlas_px  Option<[f32;4]>  (texel bounds; None ⇔
                                                     empty outline, e.g.
                                                     space)
      missing_codepoints  Vec<u32>, ascending (cmap gaps — R6 surface)

Advances come from `ttf-parser` (hmtx), not the tool JSON — DESIGN §2
names ttf-parser as the metrics source; a test cross-checks the two
within epsilon to catch parameter drift (e.g. an accidental
`-fontscale`). Plane/atlas bounds come from the tool JSON (they describe
the generated image). Glyph-run y-up convention conversion is the
painter's job (#30), documented on the type.

## Determinism (R7)

- Sorted, deduplicated gid list; fixed argument order; explicit
  `-yorigin bottom -potr -seed N -nokerning` (kerning comes from GPOS
  via rustybuzz at runtime, never from the atlas).
- The tool's PNG bytes are committed to the bundle untouched.
- postcard + pre-sorted vectors ⇒ canonical blob bytes.
- Provenance (tool version + full args) recorded in the blob.
- Same-machine repro test: generate twice, byte-compare both artifacts.
- Cross-machine repro test: CI regenerates the committed fixture and
  byte-compares — this empirically answers the spike's open question
  (macOS-built vs Linux-built generator). If it fails, that is a
  finding to record, not to paper over: options then are per-platform
  fixtures or pinning the generation platform.

## Error handling

`AtlasError` enum (std-only, Display + Error, matching dashpaint's
no-thiserror posture): `FontRead`, `FontParse`, `ToolMissing` (with an
install hint), `ToolVersion { found, required }`, `ToolFailed { status,
stderr }`, `ToolOutput` (JSON/layout mismatch), `Io`. Every failure is
named and actionable (P4 spirit); nothing panics on user input.

## Testing

- `closure`: known cmap hits, missing codepoints reported sorted,
  extras merged, output sorted+deduped. (No tool needed.)
- `metrics`: blob round-trip equality; version field present; vectors
  sorted; advance cross-check vs tool JSON within epsilon.
- `pipeline` (needs tool): ASCII charset over committed Noto Sans —
  every requested gid present, dims sane, double-run byte-identity.
- `fixture`: regenerate and byte-compare the committed fixture.
- Tool-dependent tests self-skip when the binary is absent, but fail
  loudly when `DASHSCENE_REQUIRE_ATLAS_TOOL=1` — CI sets it, so absence
  or skip can never masquerade as green in CI (verification-before-
  completion posture).
- CI: new `atlas-repro` job — restores the pinned-tag source build from
  cache (or builds it once), then runs the typeset tests with the env
  gate on. Job added to the aggregate `ci` needs list.

## Out of scope (this story)

- GSUB-closure of charsets (#34), shaping/line-breaking (#28), painter
  consumption (#30), `.dsb` packaging of atlas assets (later slice),
  per-size bitmap fallback (parked by Q-1 decision), CLI surface
  (`dashc`, later).
