# atlas pipeline — font → MSDF glyph atlas + metrics blob

    crate    crates/dashscene-typeset (module `atlas`)
    covers   v0.5 — text I: Latin (story #27, epic #24)
    traces   docs/archive/2026-07-14-design-1-seed.md §7.2 (build-time
             half), docs/design/architecture.md (stack: msdf-atlas-gen,
             ttf-parser), R1 (text quality), R7 (byte-reproducible
             builds), P4 (validated vocabulary, named diagnostics),
             docs/decisions/q1-msdf-below-14px.md (32 px/em, pxrange 4),
             docs/technotes/msdf-arabic-atlas-spike.md (#25 evidence)

## Purpose

The build-time half of `docs/archive/2026-07-14-design-1-seed.md` §7.2: given a font file and a charset,
produce the two artifacts every painter consumes at runtime —

    font.ttf ──► glyph atlas image (MSDF, keyed by GLYPH ID)
             ──► metrics blob (font + per-glyph + atlas parameters)

Both artifacts are byte-reproducible from the same inputs (R7). The
charset is an input parameter; per-locale charsets arrive with #34.

## Contract pins

- The atlas is keyed by **glyph id**, never by codepoint — contextual
  forms are just glyphs via GSUB (`docs/archive/2026-07-14-design-1-seed.md`
  §7.2, confirmed by spike #25:
  Noto Sans Arabic is unrepresentable under codepoint keying).
- Generator: `msdf-atlas-gen`, `-type msdf -size 32 -pxrange 4`, pinned
  tool version and seed (spike #25 +
  `docs/decisions/atlas-gen-external-pinned-binary.md`,
  `docs/decisions/q1-msdf-below-14px.md`).
- The atlas JSON's `distanceRange` travels into the metrics blob; the
  painter's screen-pixel-range computation needs it. `px_per_em` is
  recorded from the request, not reparsed from the tool's JSON `size`
  field — the tool was invoked with that exact `-size`, so the two are
  guaranteed equal and the JSON field is redundant.

## Home

`dashscene-typeset` (`docs/design/architecture.md` maps the atlas pipeline
to the text crate). Module `atlas` under `crates/dashscene-typeset/src/atlas/`,
public API re-exported from the crate root as `dashscene_typeset::atlas`.
No CLI in this story — `dashc` owns command-line surfaces; a cargo
example regenerates the committed test fixture.

## Components

    atlas/mod.rs       public API: AtlasSpec, AtlasBundle, AtlasError,
                       generate(); joins closure + tool output into
                       AtlasMetrics
    atlas/closure.rs   charset → (sorted gid set, missing codepoints)
                       via ttf-parser cmap
    atlas/tool.rs      binary discovery, version gate, invocation,
                       JSON layout parse (`Layout` and the related
                       layout types are `pub(crate)` — not public API)
    atlas/metrics.rs   AtlasMetrics model, font-metrics extraction
                       (ttf-parser), postcard write/load
    tests/atlas_pipeline.rs
                       tool-gated integration tests; also owns the
                       ignored `regenerate_committed_fixture` test that
                       rewrites the committed ASCII fixture, so the
                       fixture writer and the fixture checker share one
                       spec definition
    corpus/fonts/noto-sans/
                       NotoSans-Regular.ttf v2.015, unhinted/ttf build
                       (OFL, license file committed alongside) — shared
                       test/golden fixture for #27, #28, #29, #30

## Public API

    pub struct AtlasSpec {
        font_path: PathBuf, charset: BTreeSet<char>,
        extra_glyph_ids: BTreeSet<u16>, px_per_em: u16, px_range: u16,
        seed: u64,
    }
    impl AtlasSpec {
        fn new(font_path: impl Into<PathBuf>, charset: BTreeSet<char>) -> Self
        // defaults: px_per_em 32, px_range 4, seed 1
    }
    pub struct AtlasBundle { image_png: Vec<u8>, metrics: AtlasMetrics }
    impl AtlasBundle {
        fn write_to_dir(&self, dir: &Path) -> Result<(), AtlasError>
        fn load_from_dir(dir: &Path) -> Result<Self, AtlasError>
    }
    pub fn generate(spec: &AtlasSpec) -> Result<AtlasBundle, AtlasError>
    pub fn find_tool_checked() -> Result<PathBuf, AtlasError>
    pub const REQUIRED_TOOL_VERSION: &str = "1.4.0"
    pub const ATLAS_IMAGE_FILE: &str = "atlas.png"
    pub const ATLAS_METRICS_FILE: &str = "atlas.metrics"
    pub fn charset_closure(face: &ttf_parser::Face<'_>,
        charset: &BTreeSet<char>, extra_glyph_ids: &BTreeSet<u16>) -> Closure
    pub struct Closure { glyph_ids: Vec<u16>, missing_codepoints: Vec<u32> }

## Data flow

    AtlasSpec { font_path, charset, extra_glyph_ids,
                px_per_em = 32, px_range = 4, seed = 1 }
      → closure(font, charset, extras)   → gids (incl. extras), missing
      → tool::run(font, gids)            → (atlas.png bytes, layout JSON)
      → build_glyph_entries(font, gids, layout)
                                          → Vec<GlyphEntry>
                                            (hmtx-vs-tool cross-check,
                                            1e-3 em tolerance)
      → AtlasBundle { image_png, metrics: AtlasMetrics { ... } }
      → write_to_dir / load_from_dir     → atlas.png + atlas.metrics

`generate()` records provenance args built from the canonical bundle
file names (`glyphs.txt`, `atlas.png`, `atlas.json`) and the font's file
name only — never the caller's (possibly absolute, possibly scratch-dir)
path — so the blob stays machine-independent (see Determinism).

## The metrics model

    AtlasMetrics
      format_version   u32 (= FORMAT_VERSION, currently 1)
      generator        tool_version String, args Vec<String>  (provenance:
                       rerunning args must reproduce the artifacts)
      font             units_per_em u16, ascender/descender/line_gap in
                       raw font units (i16 — exact integers, consumers
                       normalize by upem; same numbers FreeType reads)
      atlas            width/height px u32, px_per_em u16,
                       distance_range_px f32
      glyphs           Vec<GlyphEntry>, sorted by glyph id, unique:
                         glyph_id u16
                         advance_units u16          (hmtx, authoritative)
                         plane_em  Option<[f32;4]>  (l,b,r,t quad in em,
                                                     y-up, baseline origin)
                         atlas_px  Option<[f32;4]>  (texel bounds,
                                                     bottom-left origin;
                                                     None ⇔ empty outline,
                                                     e.g. space)
      missing_codepoints  Vec<u32>, ascending (cmap gaps — R6 surface)

Fixed by `FORMAT_VERSION` (not stored as a per-field value): atlas kind
is MSDF, and the atlas texel-bounds origin is bottom-left
(`-yorigin bottom`) — `tool::parse_layout` rejects any other
`yOrigin` the tool reports, so a drifted invocation fails loudly rather
than silently changing the stored convention.

Advances come from `ttf-parser` (hmtx), not the tool JSON —
`docs/design/architecture.md` names ttf-parser as the metrics source; `build_glyph_entries` rejects
the pipeline (`AtlasError::ToolOutput`) if the tool's advance disagrees
with hmtx by more than 1e-3 em, catching parameter drift such as an
accidental `-fontscale`. Plane/atlas bounds come from the tool JSON
(they describe the generated image). Glyph-run y-up convention
conversion is the painter's job (#30), documented on the type.

## Determinism (R7)

- Sorted, deduplicated gid list; fixed argument order
  (`tool::build_args`); explicit `-yorigin bottom -potr -seed N
  -nokerning` (kerning comes from GPOS via rustybuzz at runtime, never
  from the atlas).
- The tool's PNG bytes are stored untouched (no recompression).
- postcard + pre-sorted vectors ⇒ canonical blob bytes
  (`blob_bytes_are_canonical` test).
- Provenance (tool version + full canonical-name args) recorded in the
  blob. The executed argv and the provenance vector are built by one
  function (`tool::build_invocation`), so recorded == executed holds by
  construction; execution uses `OsString` paths (non-UTF-8-safe) while
  provenance holds canonical names only, never machine paths.
- Same-machine repro: `double_run_is_byte_identical` generates twice and
  byte-compares both artifacts.
- Cross-machine repro: the CI `atlas-repro` job (Linux) regenerates the
  committed ASCII fixture (`crates/dashscene-typeset/tests/fixtures/ascii/`,
  produced on macOS by the ignored `regenerate_committed_fixture` test)
  and byte-compares (`committed_fixture_is_reproducible`) — this
  empirically answers the spike's open cross-machine question on every
  CI run. If a toolchain change breaks it, that is a finding to record
  (per-platform fixtures or a pinned generation platform), not to hide.

## Error handling

`AtlasError` (std-only, `Display` + `Error`, matching dashpaint's
no-thiserror posture): `FontRead(PathBuf, io::Error)`, `FontParse`,
`ToolMissing` (with an install hint), `ToolVersion { found, required }`,
`ToolFailed { status, stderr }`, `ToolOutput` (JSON/layout mismatch,
non-bottom `yOrigin`, missing gid in layout, or an hmtx/tool advance
mismatch), `Metrics` (blob decode failure, unsupported `format_version`, trailing
bytes, or unsorted vectors — the documented field contracts are
enforced at the parse boundary, not only in the producer), `Io`. The
version gate decodes the leading version field before the body, so a
future-version blob reports the version error, not a decode error. The
`-help` probe's error includes the child's exit code and stderr, so a
binary that spawns but cannot run (for example a loader error on a
stale cached build) names its own cause. Every failure is named and
actionable (P4 spirit); nothing panics on user input.

## Testing

- `atlas::closure` (in-module, no tool): known cmap hits resolve to
  sorted/deduplicated gids including `.notdef`; missing codepoints
  reported sorted; `extra_glyph_ids` merged.
- `atlas::metrics` (in-module, no tool): blob round-trip equality; blob
  bytes canonical across two encodes of the same value; unknown
  `format_version` and garbage bytes rejected; `font_metrics` extracts
  sane values from the fixture font.
- `atlas::tool` (in-module, no tool): version-banner parsing (accepted
  and rejected forms); canonical argument order; layout JSON parsing,
  including rejection of a non-`"bottom"` `yOrigin`.
- `tests/atlas_pipeline.rs` (needs the tool): full ASCII-charset
  generation over the committed Noto Sans fixture (every requested gid
  present, dims sane, space glyph has no bounds, every other glyph
  does); double-run byte-identity; bundle write/load round-trip;
  uncovered codepoints reported, not dropped; the committed-fixture
  reproducibility check.
- Tool-dependent tests self-skip when the binary is absent, but fail
  loudly when `DASHSCENE_REQUIRE_ATLAS_TOOL=1` (the library-owned
  `REQUIRE_TOOL_ENV` constant) — CI's `atlas-repro` job sets it, so a
  skipped test cannot be reported as passing there.
- CI: the `atlas-repro` job restores a cached source build of the
  pinned `Chlumsky/msdf-atlas-gen` commit (the `v1.4` tag's SHA — a
  tag is movable, a SHA is not), or builds it once, then runs
  `cargo test -p dashscene-typeset --test atlas_pipeline` with the env
  gate on. The job is in the aggregate `ci` job's `needs` list.

## Seams to later stories

- **#28** (Latin shaping) decides whether v0.5 shaping disables the
  `liga` OpenType feature or feeds ligature glyph ids (`fi`, `fl`, ...)
  in via `AtlasSpec::extra_glyph_ids` — with cmap-only closure those
  ligature glyphs are not in the atlas unless supplied that way.
- **#30** (Skia glyph quads) must treat a shaped glyph id absent from
  the atlas as a named diagnostic, never a silent skip (P4) — this is
  the same contract `build_glyph_entries` enforces build-time (a
  requested gid missing from the tool's layout is `AtlasError::
  ToolOutput`, not a dropped glyph).
- **#34** (per-locale charsets) is expected to extend `closure` with
  real GSUB closure; `extra_glyph_ids` keeps `AtlasSpec`'s contract
  stable across that change (see
  `docs/decisions/atlas-closure-cmap-plus-extras.md`).

## Out of scope (this story)

GSUB-closure of charsets (#34), shaping/line-breaking (#28), painter
consumption (#30), `.dsb` packaging of atlas assets (later slice),
per-size bitmap fallback (parked by
`docs/decisions/q1-msdf-below-14px.md`), CLI surface (`dashc`, later).

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.2 (build-time
  atlas pipeline), `docs/design/architecture.md` (stack), R1, R7, P4;
  issue #27 acceptance criteria.
- Blocks: #30 (Skia glyph quads), #34 (per-locale charsets).
- Related decisions:
  `docs/decisions/atlas-gen-external-pinned-binary.md`,
  `docs/decisions/atlas-metrics-postcard-blob.md`,
  `docs/decisions/atlas-closure-cmap-plus-extras.md`,
  `docs/decisions/q1-msdf-below-14px.md`.
- Related technote: `docs/technotes/msdf-arabic-atlas-spike.md`.
