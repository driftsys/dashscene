# atlas pipeline — font → MSDF glyph atlas + metrics blob

    crate    crates/dashscene-typeset (module `atlas`)
    covers   v0.5 — text I: Latin (story #27, epic #24);
             v0.11 — the per-weight committed fixtures (story F1/#368,
             epic #344)
    traces   docs/archive/2026-07-14-design-1-seed.md §7.2 (build-time
             half), docs/design/architecture.md (stack: msdf-atlas-gen,
             ttf-parser), R1 (text quality), R7 (byte-reproducible
             builds), P4 (validated vocabulary, named diagnostics),
             docs/decisions/q1-msdf-below-14px.md (32 px/em, pxrange 4),
             docs/technotes/arabic-atlas-coverage.md (#25 evidence)

## Purpose

The build-time half of `docs/archive/2026-07-14-design-1-seed.md` §7.2: given a
font file and a charset, produce the two artifacts every painter consumes at
runtime —

    font.ttf ──► glyph atlas image (MSDF, keyed by GLYPH ID)
             ──► metrics blob (font + per-glyph + atlas parameters)

Both artifacts are byte-reproducible from the same inputs (R7). The charset is
an input parameter; per-locale charsets arrive with #34.

## Contract pins

- The atlas is keyed by **glyph id**, never by codepoint — contextual forms are
  just glyphs via GSUB (`docs/archive/2026-07-14-design-1-seed.md` §7.2,
  confirmed by spike #25: Noto Sans Arabic is unrepresentable under codepoint
  keying).
- Generator: `msdf-atlas-gen`, `-type msdf -size 32 -pxrange 4`, pinned tool
  version and seed (spike #25 +
  `docs/decisions/atlas-gen-external-pinned-binary.md`,
  `docs/decisions/q1-msdf-below-14px.md`).
- The atlas JSON's `distanceRange` travels into the metrics blob; the painter's
  screen-pixel-range computation needs it. `px_per_em` is recorded from the
  request, not reparsed from the tool's JSON `size` field — the tool was invoked
  with that exact `-size`, so the two are guaranteed equal and the JSON field is
  redundant.

## Charset closure

`charset_closure` turns a declared charset into the glyph-id set the atlas must
cover. It takes a `rustybuzz::Face` (which derefs to `ttf_parser::Face`, so the
cmap lookups are unchanged) and unions three sources:

- **cmap** — the nominal glyph of each charset codepoint; codepoints the cmap
  cannot represent go to `missing_codepoints` (R6), never dropped.
- **GSUB** — the contextual forms, mark forms, and ligatures shaping produces.
  rustybuzz exposes shaping but no standalone glyph-closure operation, so the
  closure shapes representative strings and unions the output glyph ids (spike
  #25's method): each character isolated, each Arabic letter through the four
  joining contexts (a dual-joining beh connector gives final/initial/medial),
  each haraka on a base letter, and every ordered character pair. The pair
  sweep, extended with the joining contexts when the first character is an
  Arabic letter, is what carries lam-alef with its contextual variants and the
  Latin `fi`.
- **extra_glyph_ids** — the caller's manual additions.

Shaping runs with the default OpenType feature set (ligatures on) — the same
configuration production shaping uses for Arabic-context runs since the #33 join
(`docs/decisions/liga-clig-off-until-gsub-closure.md`, Resolution); the closure
changes no shaping feature itself.

A charset that declares strong Arabic characters (UAX #9 bidi class AL, any
Arabic block) next to European digits also covers the Arabic-Indic digit glyphs
(U+0660..=U+0669 counterparts of the declared digits): production shaping
substitutes those display shapes in Arabic context
(`docs/design/typeset-latin.md`, digit-shape selection). Trigger and mapping are
the text module's own `is_arabic_strong` and `arabic_indic_digit` functions —
one definition, so this derivation cannot drift from the production rule — and
the derived digits join the charset for both cmap and GSUB, so a font without
them reports the gap in `missing_codepoints`.

Two scope boundaries hold at v0.6:

- **Two-character ligatures only.** Ligatures of three or more characters
  (`ffi`/`ffl`, the Allah ligature) are outside the pairwise sweep. A shaped run
  that reaches one is the painter's named missing-glyph diagnostic (#30), never
  a silent drop (P4).
- **Standard Arabic only.** `ARABIC_LETTERS` (0x0621..=0x064A) and
  `ARABIC_HARAKAT` (0x064B..=0x0652) are the sweep ranges. An extended-Arabic
  codepoint (Persian/Urdu, presentation forms) still gets its isolated form, its
  ligature forms, and the contextual forms the pair sweep reaches incidentally,
  but no joining-context sweep of its own.

Coverage is computed from the run's natural direction (Arabic right-to-left), so
it assumes the production shaper also shapes Arabic in its natural direction —
which holds as-built: #32's seam shapes each UAX #9 level run with its resolved
direction, and #33 shapes Arabic-context runs with the closure's default feature
set. The #33 acceptance test pins the coupling: production-shaped output is a
subset of the closure's coverage for the declared charset
(`crates/dashscene-typeset/tests/typeset_arabic.rs`).

## One atlas per (script, weight) (story #368)

Weight is a property of a face, and a face is what an atlas is baked from, so a
second weight is a second rasterization of the same charset. It is carried as a
**sibling atlas directory**, not as a face axis inside one atlas:
`corpus/atlas/ascii-semibold` and `corpus/atlas/ascii-bold` hold the same two
files as `corpus/atlas/ascii`, produced by the same `AtlasSpec` over the same
charset with a different `font_path`.

Nothing in this pipeline changed to allow it. `AtlasSpec` gains no field — the
weight is carried by the face the spec points at —
`AtlasMetrics::FORMAT_VERSION` stays 1, and the two Regular fixtures are never
rewritten, so their bytes and the frames that render through them are untouched
by adding a weight. The alternative, a face axis inside `AtlasMetrics`, would
have been a breaking wire change (the blob is postcard, which is not
self-describing) and would have forced regenerating both committed atlases
through the pinned generator; the full comparison is
`docs/decisions/atlas-directory-per-script-weight.md`.

The consumer side is the typesetter's cascade, which resolves a requested CSS
weight to one face per family and tags each glyph with that face's flat slot, so
a stager maps slot to atlas positionally (`docs/design/typeset-latin.md`, Font
weight).

## Home

`dashscene-typeset` (`docs/design/architecture.md` maps the atlas pipeline to
the text crate). Module `atlas` under `crates/dashscene-typeset/src/atlas/`,
public API re-exported from the crate root as `dashscene_typeset::atlas`. No CLI
in this story — `dashc` owns command-line surfaces; a cargo example regenerates
the committed test fixture.

## Components

    atlas/mod.rs       public API: AtlasSpec, AtlasBundle, AtlasError,
                       generate(); joins closure + tool output into
                       AtlasMetrics
    atlas/closure.rs   charset → (sorted gid set, missing codepoints)
                       via ttf-parser cmap plus a shaping-based GSUB
                       closure (rustybuzz) that adds contextual forms
                       and ligatures
    atlas/tool.rs      binary discovery, version gate, invocation,
                       JSON layout parse (`Layout` and the related
                       layout types are `pub(crate)` — not public API)
    atlas/metrics.rs   AtlasMetrics model, font-metrics extraction
                       (ttf-parser), postcard write/load
    tests/atlas_pipeline.rs
                       tool-gated integration tests; also owns the four
                       ignored regenerators (`..._ascii_fixture`,
                       `..._arabic_fixture`, `..._ascii_semibold_fixture`,
                       `..._ascii_bold_fixture`) that rewrite the committed
                       fixtures, so each fixture's writer and checker share
                       one spec definition
    corpus/fonts/noto-sans/
                       NotoSans-Regular.ttf v2.015, unhinted/ttf build
                       (OFL, license file committed alongside) — shared
                       test/golden fixture for #27, #28, #29, #30; plus
                       NotoSans-SemiBold.ttf and NotoSans-Bold.ttf from
                       the same release and build variant, under the same
                       committed OFL.txt (#368)
    corpus/fonts/noto-sans-arabic/
                       NotoSansArabic-Regular.ttf v2.013, unhinted/ttf
                       build (OFL, license file committed alongside) —
                       the Arabic fixture the GSUB closure tests shape
                       against (#33, #34, #35)
    corpus/atlas/ascii/
                       the committed ASCII atlas fixture (atlas.png +
                       atlas.metrics). The shared home — beside the fonts,
                       not under a crate's private tests/ — so a golden in
                       another crate loads it without reaching across
                       (debt #217; the v0.5 Latin golden and the
                       reproducibility check both read it here)
    corpus/atlas/ascii-semibold/, corpus/atlas/ascii-bold/
                       the committed SemiBold (600) and Bold (700) ASCII
                       fixtures (#368) — the same charset and parameters as
                       ascii/, a different face; byte-reproduced in CI
                       alongside it
    corpus/atlas/arabic/
                       the committed Arabic atlas fixture (#35), source of
                       the E2 golden's glyphs, generated from the Arabic
                       charset and byte-reproduced in CI like the ASCII one

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
    pub fn charset_closure(face: &rustybuzz::Face<'_>,
        charset: &BTreeSet<char>, extra_glyph_ids: &BTreeSet<u16>) -> Closure
    pub struct Closure { glyph_ids: Vec<u16>, missing_codepoints: Vec<u32> }
    pub const ARABIC_LETTERS: RangeInclusive<u32>  // 0x0621..=0x064A
    pub const ARABIC_HARAKAT: RangeInclusive<u32>  // 0x064B..=0x0652

## Data flow

    AtlasSpec { font_path, charset, extra_glyph_ids,
                px_per_em = 32, px_range = 4, seed = 1 }
      → closure(font, charset, extras)   → gids (cmap + GSUB forms +
                                            extras), missing
      → tool::run(font, gids)            → (atlas.png bytes, layout JSON)
      → build_glyph_entries(font, gids, layout)
                                          → Vec<GlyphEntry>
                                            (hmtx-vs-tool cross-check,
                                            1e-3 em tolerance)
      → AtlasBundle { image_png, metrics: AtlasMetrics { ... } }
      → write_to_dir / load_from_dir     → atlas.png + atlas.metrics

`generate()` records provenance args built from the canonical bundle file names
(`glyphs.txt`, `atlas.png`, `atlas.json`) and the font's file name only — never
the caller's (possibly absolute, possibly scratch-dir) path — so the blob stays
machine-independent (see Determinism).

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

Fixed by `FORMAT_VERSION` (not stored as a per-field value): atlas kind is MSDF,
and the atlas texel-bounds origin is bottom-left (`-yorigin bottom`) —
`tool::parse_layout` rejects any other `yOrigin` the tool reports, so a drifted
invocation fails loudly rather than silently changing the stored convention.

Advances come from `ttf-parser` (hmtx), not the tool JSON —
`docs/design/architecture.md` names ttf-parser as the metrics source;
`build_glyph_entries` rejects the pipeline (`AtlasError::ToolOutput`) if the
tool's advance disagrees with hmtx by more than 1e-3 em, catching parameter
drift such as an accidental `-fontscale`. Plane/atlas bounds come from the tool
JSON (they describe the generated image). Glyph-run y-up convention conversion
is the painter's job (#30), documented on the type.

## Determinism (R7)

- Sorted, deduplicated gid list; fixed argument order (`tool::build_args`);
  explicit `-yorigin bottom -potr -seed N
  -nokerning` (kerning comes from GPOS
  via rustybuzz at runtime, never from the atlas).
- The tool's PNG bytes are stored untouched (no recompression).
- postcard + pre-sorted vectors ⇒ canonical blob bytes
  (`blob_bytes_are_canonical` test).
- Provenance (tool version + full canonical-name args) recorded in the blob. The
  executed argv and the provenance vector are built by one function
  (`tool::build_invocation`), so recorded == executed holds by construction;
  execution uses `OsString` paths (non-UTF-8-safe) while provenance holds
  canonical names only, never machine paths.
- Same-machine repro: `double_run_is_byte_identical` generates twice and
  byte-compares both artifacts.
- Cross-machine repro: the CI `atlas-repro` job (Linux) regenerates all four
  committed fixtures (`corpus/atlas/ascii/`, `corpus/atlas/ascii-semibold/`,
  `corpus/atlas/ascii-bold/` and `corpus/atlas/arabic/`, produced on macOS by
  the four ignored regenerators) and byte-compares each
  (`committed_ascii_fixture_is_reproducible`,
  `committed_ascii_semibold_fixture_is_reproducible`,
  `committed_ascii_bold_fixture_is_reproducible`,
  `committed_arabic_fixture_is_reproducible`) — this empirically answers the
  spike's open cross-machine question on every CI run, over a Latin ASCII
  charset at three weights and an Arabic charset whose closure runs the full
  GSUB sweep. If a toolchain change breaks it, that is a finding to record
  (per-platform fixtures or a pinned generation platform), not to hide.
- A reproducibility check cannot notice a fixture baked from the wrong face,
  because it regenerates from the same spec.
  `the_three_ascii_weights_are_distinct_faces` covers that gap without the tool:
  the three ASCII atlases must cover the same 99 glyphs with no missing
  codepoints, and `H` — a two-stem glyph — must advance wider at each heavier
  weight (#368).

## Error handling

`AtlasError` (std-only, `Display` + `Error`, matching dashpaint's no-thiserror
posture): `FontRead(PathBuf, io::Error)`, `FontParse`, `ToolMissing` (with an
install hint), `ToolVersion { found, required }`,
`ToolFailed { status, stderr }`, `ToolOutput` (JSON/layout mismatch, non-bottom
`yOrigin`, missing gid in layout, or an hmtx/tool advance mismatch), `Metrics`
(blob decode failure, unsupported `format_version`, trailing bytes, or unsorted
vectors — the documented field contracts are enforced at the parse boundary, not
only in the producer), `Io`. The version gate decodes the leading version field
before the body, so a future-version blob reports the version error, not a
decode error. The `-help` probe's error includes the child's exit code and
stderr, so a binary that spawns but cannot run (for example a loader error on a
stale cached build) names its own cause. Every failure is named and actionable
(P4 spirit); nothing panics on user input.

## Testing

- `atlas::closure` (in-module, no tool): known cmap hits resolve to
  sorted/deduplicated gids including `.notdef`; missing codepoints reported
  sorted; `extra_glyph_ids` merged; the GSUB closure covers the Latin `fi` and
  Arabic lam-alef ligatures (including lam-alef's seen-joined form) and a haraka
  on a base, covers every glyph a set of real Arabic words shape to, and adds
  glyphs beyond cmap.
- `atlas::metrics` (in-module, no tool): blob round-trip equality; blob bytes
  canonical across two encodes of the same value; unknown `format_version` and
  garbage bytes rejected; `font_metrics` extracts sane values from the fixture
  font.
- `atlas::tool` (in-module, no tool): version-banner parsing (accepted and
  rejected forms); canonical argument order; layout JSON parsing, including
  rejection of a non-`"bottom"` `yOrigin`.
- `tests/atlas_pipeline.rs` (needs the tool): full ASCII-charset generation over
  the committed Noto Sans fixture (every requested gid present, dims sane, space
  glyph has no bounds, every other glyph does); double-run byte-identity; bundle
  write/load round-trip; uncovered codepoints reported, not dropped; the
  committed-fixture reproducibility check; and, over the Arabic fixture, that a
  generated atlas covers every glyph real Arabic words shape to and is
  byte-identical across a double run. The same file carries the full-charset
  production↔coverage pin
  (`production_layout_stays_within_full_charset_coverage`, no tool needed): it
  self-skips on a plain `cargo test` because its pairwise sweep costs seconds,
  and runs under the `atlas-repro` job's env gate where CI already demands
  thoroughness.
- Tool-dependent tests self-skip when the binary is absent, but fail loudly when
  `DASHSCENE_REQUIRE_ATLAS_TOOL=1` (the library-owned `REQUIRE_TOOL_ENV`
  constant) — CI's `atlas-repro` job sets it, so a skipped test cannot be
  reported as passing there.
- CI: the `atlas-repro` job restores a cached source build of the pinned
  `Chlumsky/msdf-atlas-gen` commit (the `v1.4` tag's SHA — a tag is movable, a
  SHA is not), or builds it once, then runs
  `cargo test -p dashscene-typeset --test atlas_pipeline` with the env gate on.
  The job is in the aggregate `ci` job's `needs` list.

## Seams to later stories

- **#28** (Latin shaping) decides whether v0.5 shaping disables the `liga`
  OpenType feature or feeds ligature glyph ids (`fi`, `fl`, ...) in via
  `AtlasSpec::extra_glyph_ids` — with cmap-only closure those ligature glyphs
  are not in the atlas unless supplied that way.
- **#30** (Skia glyph quads) must treat a shaped glyph id absent from the atlas
  as a named diagnostic, never a silent skip (P4) — this is the same contract
  `build_glyph_entries` enforces build-time (a requested gid missing from the
  tool's layout is `AtlasError::
  ToolOutput`, not a dropped glyph).
- **#34** (per-locale charsets) delivered the shaping-based GSUB closure (see
  Charset closure above); `extra_glyph_ids` kept `AtlasSpec`'s contract stable
  across the change, as `docs/decisions/atlas-closure-cmap-plus-extras.md`
  planned.
- **#33** (Arabic shaping) delivered its side of the join: Arabic-context runs
  shape in their natural direction with the closure's default feature set
  (`liga`/`clig` stay off for other runs —
  `docs/decisions/liga-clig-off-until-gsub-closure.md`, Resolution), and the
  subset assertion this seam asked for is
  `production_shaped_output_stays_within_declared_charset_coverage` (see the
  Charset closure direction note).

## Out of scope (this story)

Shaping/line-breaking (#28), painter consumption (#30), `.dsb` packaging of
atlas assets (later slice), per-size bitmap fallback (parked by
`docs/decisions/q1-msdf-below-14px.md`), CLI surface (`dashc`, later).

## Trace

- Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §7.2 (build-time atlas
  pipeline), `docs/design/architecture.md` (stack), R1, R7, P4; issue #27
  acceptance criteria.
- Blocks: #30 (Skia glyph quads), #34 (per-locale charsets).
- Related decisions: `docs/decisions/atlas-gen-external-pinned-binary.md`,
  `docs/decisions/atlas-metrics-postcard-blob.md`,
  `docs/decisions/atlas-closure-cmap-plus-extras.md`,
  `docs/decisions/q1-msdf-below-14px.md`,
  `docs/decisions/atlas-directory-per-script-weight.md`.
- Related technote: `docs/technotes/arabic-atlas-coverage.md`.
