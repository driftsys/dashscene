# Committed MSDF atlas fixtures

    generator  msdf-atlas-gen v1.4.0 (pinned commit, see .github/workflows/ci.yml)
    keyed by   glyph id (docs/design/atlas-pipeline.md)
    license    derived from the OFL fonts under corpus/fonts/ — see each font's OFL.txt

Prebuilt glyph atlases — the MSDF `atlas.png` plus the `atlas.metrics`
postcard blob (`docs/design/atlas-pipeline.md`). They are the committed,
byte-reproducible output of the atlas pipeline for a fixed font and
charset, so a golden or a runtime test can load an atlas without the
`msdf-atlas-gen` tool at test time.

The shared home lives here, beside the fonts each atlas is generated
from (`corpus/fonts/`), rather than inside a crate's private `tests/`
tree — so a golden in another crate loads a fixture without reaching
across into `dashscene-typeset`'s test directory (debt #217).

## Fixtures

- `ascii/` — from `corpus/fonts/noto-sans`, charset printable ASCII
  (0x20..=0x7e) plus the Latin ligatures the GSUB closure adds. Consumed
  by the v0.5 Latin text golden (`goldens/tooling/tests/v05_text.rs`) and,
  as the Latin fallback atlas, by the v0.7 multi-font golden
  (`goldens/tooling/tests/v07_fallback.rs`).
- `ascii-semibold/`, `ascii-bold/` — from `corpus/fonts/noto-sans`'s
  SemiBold and Bold faces, over the same charset as `ascii/` (story #368).
  One atlas directory per (script, weight): the Regular fixtures are never
  rewritten when a weight is added, so the atlas format needs no change,
  `AtlasMetrics::FORMAT_VERSION` stays 1, and every frame that renders at
  weight 400 is provably unaffected. Consumed by the production render walk
  (`goldens/tooling/src/render.rs`), whose cascade offers the Latin family
  at weights 400/600/700 and mirrors it with `[ascii, ascii-semibold,
  ascii-bold, arabic]`.
- `arabic/` — from `corpus/fonts/noto-sans-arabic`, charset the standard
  Arabic letters, harakat, Arabic-Indic digits, and space (the GSUB
  closure adds the contextual forms and ligatures those shape to).
  Consumed by the v0.6 Arabic (E2) golden
  (`goldens/tooling/tests/v06_arabic.rs`) and, as the primary atlas, by
  the v0.7 multi-font golden (`goldens/tooling/tests/v07_fallback.rs`).

Each atlas is a directory of two files:

    atlas.png       the MSDF atlas image, keyed by glyph id
    atlas.metrics   the postcard metrics blob (font + per-glyph + atlas
                    parameters, plus generator provenance)

## Regenerating

The fixtures are not hand-built. Each has one ignored regenerator test
and one reproducibility test in
`crates/dashscene-typeset/tests/atlas_pipeline.rs`, both building from
one `AtlasSpec` per fixture so the writer and the checker cannot drift:

    # rewrite a fixture from the current pipeline (needs msdf-atlas-gen)
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_ascii_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_arabic_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_ascii_semibold_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_ascii_bold_fixture

Run a regenerator only after a deliberate parameter or toolchain change,
then commit the result with a note recording why. Do not hand-edit the
files.

## Cross-machine byte-identity

The fixtures are generated on one platform (macOS) and byte-reproduced
on another (Linux) by the CI `atlas-repro` job, which builds the pinned
`msdf-atlas-gen` commit and runs `committed_ascii_fixture_is_reproducible`,
`committed_arabic_fixture_is_reproducible`,
`committed_ascii_semibold_fixture_is_reproducible` and
`committed_ascii_bold_fixture_is_reproducible` under
`DASHSCENE_REQUIRE_ATLAS_TOOL=1`. A byte difference fails that job, so a
toolchain change that breaks reproducibility is surfaced, not hidden
(R7; `docs/design/atlas-pipeline.md`, Determinism).
