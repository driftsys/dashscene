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
  (0x20..=0x7e) plus the Latin ligatures the GSUB closure adds. Seven
  files load it, and all seven are what a regeneration must re-record
  (issue #533 found this list naming only the first two):

      goldens/tooling/tests/v05_text.rs            v05-text-latin
      goldens/tooling/tests/v07_fallback.rs        v07-text-fallback
                                                   (the Latin fallback atlas)
      goldens/tooling/tests/v07_text_lowering.rs   v07-text-lowering
      goldens/tooling/tests/v07_variant_topology.rs
                                                   v07-variant-topology
      goldens/tooling/tests/v013_uncovered_shapes.rs
                                                   v013-baseline-hug-cross
      goldens/tooling/tests/render_oracle.rs       no committed PNG — the E7
                                                   oracle's measured residuals
      goldens/tooling/src/render.rs                no committed PNG — the
                                                   production render walk, used
                                                   by the import and
                                                   profile-preview oracles

  Regenerate the list rather than trusting it:
  `grep -rl 'corpus/atlas/ascii"' goldens/`.
- `ascii-semibold/`, `ascii-bold/` — from `corpus/fonts/noto-sans`'s
  SemiBold and Bold faces, over the same charset as `ascii/` (story #368).
  One atlas directory per (script, weight): the Regular fixtures are never
  rewritten when a weight is added, so the atlas format needs no change,
  `AtlasMetrics::FORMAT_VERSION` stays 1, and every frame that renders at
  weight 400 is provably unaffected. Consumed by the production render walk
  (`goldens/tooling/src/render.rs`), whose cascade offers Noto Sans at
  weights 400/600/700 and mirrors it, since story #385, with `[ascii,
  ascii-semibold, ascii-bold, inter-ascii, inter-ascii-medium,
  inter-ascii-semibold, inter-ascii-bold, arabic]`.
- `inter-ascii/`, `inter-ascii-medium/`, `inter-ascii-semibold/`,
  `inter-ascii-bold/` — from `corpus/fonts/inter`'s Regular, Medium,
  SemiBold and Bold faces, over the same charset as `ascii/` (story
  #385). The family real Figma files are authored in, so a document
  naming `Inter` resolves to Inter rather than being substituted
  (`docs/decisions/corpus-ships-inter.md`). Each holds 113 glyphs
  against the Noto fixtures' 99: the charset is identical, and Inter's
  `liga` feature simply closes over more pairs.

  The Noto directories keep their family-less names. Renaming them to
  match this scheme would rewrite the atlases the shipped goldens load,
  and one directory per (script, weight) is a rule about never
  regenerating an existing fixture, not about spelling.

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
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_inter_ascii_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_inter_ascii_medium_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_inter_ascii_semibold_fixture
    cargo test -p dashscene-typeset --test atlas_pipeline -- \
      --ignored regenerate_committed_inter_ascii_bold_fixture

Run a regenerator only after a deliberate parameter or toolchain change,
then commit the result with a note recording why. Do not hand-edit the
files.

A regenerated fixture changes every golden that samples it. The goldens
compare within a pixel budget, so a regenerated atlas does not fail them —
it moves them a few pixels and leaves the committed images stale, with no
signal. Re-record every consumer listed with the fixture above in the same
commit, and state in the message that the atlas moved them.

Committed images are not the only thing to restate. Two kinds of derived
number are measured against the atlas bytes and go stale the same way,
with no test failing:

- the ink-pixel counts a golden's budget rationale quotes — for `ascii/`,
  `v07_text_lowering.rs` ("the lowered text inks 484 px") and
  `v07_variant_topology.rs` ("the resolved instance's label inks 480 px");
- the per-frame residuals recorded in `goldens/oracle/README.md`, which
  `render_oracle.rs` and `goldens/tooling/src/render.rs` measure.

Re-measure both, in the same commit.

This rule is drawn from issue #533: commit `48b721b` regenerated `ascii/`
on 2026-07-16, about eight hours after `9412e7a` recorded the v0.5 Latin
golden against the previous bytes, and
`goldens/images/v05-text-latin.png` then remained 3 px stale until #533
found it (`docs/decisions/golden-comparison-space.md`).

## Cross-machine reproducibility

The fixtures are generated on one platform (macOS, arm64) and reproduced
on another (Linux, both x86_64 and arm64) by the CI `atlas-repro` job,
which builds the pinned
`msdf-atlas-gen` commit and runs `committed_ascii_fixture_is_reproducible`,
`committed_arabic_fixture_is_reproducible`,
`committed_ascii_semibold_fixture_is_reproducible`,
`committed_ascii_bold_fixture_is_reproducible` and the four
`committed_inter_ascii*_fixture_is_reproducible` tests under
`DASHSCENE_REQUIRE_ATLAS_TOOL=1` — the job runs the whole
`atlas_pipeline` binary, so a new fixture's checker is picked up by
adding it there and nowhere else. A difference fails that job, so a
toolchain change that breaks reproducibility is surfaced, not hidden
(R7; `docs/design/atlas-pipeline.md`, Determinism).

What "reproduced" means is exact for the metrics blob and bounded for the
image, and the split is measured rather than conceded:

- `atlas.metrics` — packing, per-glyph boxes, atlas parameters, generator
  provenance — is compared **byte for byte**. It is byte-identical on
  every machine measured, both architectures included.
- `atlas.png` is compared **decoded**, under two bounds: no channel may
  move by more than one step, and at most 0.1 % of the pixels may differ
  at all.

The image cannot be compared byte for byte because `msdf-atlas-gen`'s
floating-point arithmetic differs between CPU architectures. The Bold
fixture decodes 4 pixels of 65536 apart between arm64 and x86_64 — 0.006
%, each by a single channel step, at identical dimensions and identical
packing. The tool is external C++, so this is not a difference the
pipeline can round away, and one committed fixture cannot be
byte-identical on two architectures at once (story #654).

The bounds are tight enough to keep the gate: a generator that
re-rasterises a glyph, moves the packing, or changes the distance range
moves some channel by more than one step, and a systematic shift of the
whole field moves far more than 0.1 % of the pixels. Both bounds are
mutation-proven — one pixel moved two steps fails, and the real
cross-architecture 4 fails against a zero budget.

Same-machine determinism is still byte-identity, asserted by
`double_run_is_byte_identical` and `arabic_atlas_double_run_is_byte_identical`,
which compare two independent runs on one machine and admit no tolerance
at all.
