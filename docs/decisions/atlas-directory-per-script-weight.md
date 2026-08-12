# Decision: one committed atlas directory per (script, weight)

    status   accepted (story #368, epic #344 — the design gate the
             repository owner approved before implementation)
    scope    corpus/atlas/, corpus/fonts/noto-sans/, and the
             crates/dashscene-typeset atlas module's committed fixtures —
             how the render corpus carries more than one font weight
    evidence the hero weight census below;
             docs/decisions/atlas-metrics-postcard-blob.md (the metrics
             blob is not self-describing)

## Context

The authored weight already reaches the committed arena intact: Figma's
`style.fontWeight` lowers into `DocTextStyle.weight`, travels as `dashbuf`'s
`weight: ushort = 400`, and is read back into `dashscene_core::TextStyle`.
Nothing downstream read it, and nothing downstream could: `corpus/fonts/` held
one Regular face per script and `corpus/atlas/` one atlas per script, so every
run rendered Regular whatever the document asked for.

The census of the v0.10 hero (`S30AJmYfnDKGeSQmzuXEUk`, node `1973:6580`) is the
evidence for how much weight coverage is worth committing. Its 58 TEXT nodes
carry four distinct weights, and no node carries `characterStyleOverrides`, so
every node is exactly one style run — there is no mid-node weight mixing to
design for:

    weight  Figma PostScript name  nodes  characters
    400     null (absent)            31        1659
    500     Inter-Medium              2          22
    600     Inter-SemiBold            6          48
    700     Inter-Bold               19         328

Weight is a property of a **face**, and a face is what an atlas is baked from,
so carrying a second weight means carrying a second rasterization of the same
charset. The question this record settles is where that second rasterization
lives.

## Options

1. **One atlas directory per (script, weight).** `corpus/atlas/ascii` and
   `corpus/atlas/arabic` stay exactly as they are; sibling directories
   (`ascii-semibold`, `ascii-bold`, later `arabic-bold`) hold the same two files
   (`atlas.png` + `atlas.metrics`) produced by the same `AtlasSpec` with a
   different `font_path`.
2. **One atlas holding several face slots.** `AtlasMetrics` grows a face axis —
   either a per-face vector with weights and glyph ranges, or a `weight` field
   plus a convention that several weights share one image.
3. **One variable font, instanced at runtime.** Noto Sans ships a variable font
   with a `wght` axis, so in principle one file could serve every weight.

## Choice

Option 1. Two faces of the pinned Noto Sans release were added — SemiBold (600)
and Bold (700) — with `corpus/atlas/ascii-semibold` and
`corpus/atlas/ascii-bold` baked from them over the ASCII charset
`corpus/atlas/ascii` already declares. `AtlasSpec` is unchanged: the weight is
carried by the face the spec points at, not by a new spec field, so
`AtlasMetrics::FORMAT_VERSION` stays 1 and the two Regular fixtures are never
rewritten.

## Why

- The decisive argument is the proof obligation, not the file count. Option 1
  changes no format, rewrites no committed atlas, and touches no file frozen by
  the E7 exit gate, so "E7 renders identically" follows from the structure of
  the change plus the measured fact that no E7 fixture carries a weight other
  than 400. Every E7 frame's fixture was censused: `v08-wrap`, `v08-drop-shadow`
  and `v08-inner-shadow` carry no TEXT nodes at all, and `v08-grid-spans`,
  `v08-baseline`, `v06-text-arabic` and `v05-text-latin` carry weight 400 only.
- Option 2 would make the same claim contingent on a regeneration. The metrics
  blob is postcard, which is not self-describing
  (`docs/decisions/atlas-metrics-postcard-blob.md`), so adding a field is a
  breaking wire change: an old blob decoded against a new struct fails, and the
  `FORMAT_VERSION` gate and the trailing-byte and sortedness checks correctly
  reject it. Option 2 therefore forces regenerating both committed atlases
  through the pinned `msdf-atlas-gen` v1.4.0 and a fresh pass of the
  cross-machine `atlas-repro` job. Its only real benefit is a small reduction in
  committed file count, and the glyph bitmaps dominate the bytes, so even the
  size saving is small. A fidelity fix should not put the exit gate's evidence
  on its critical path.
- Option 3 is rejected on the same grounds plus a new capability.
  `msdf-atlas-gen` bakes a static raster, so a variable font must still be
  instanced to a fixed weight before baking — the committed artifacts would be
  per-weight atlases regardless, which is Option 1 with extra steps. The runtime
  side would additionally need variation-coordinate support through `rustybuzz`
  and `ttf-parser`, which the pipeline does not use.
- Adding a further weight later is a fixture-only change under Option 1: one
  font file, one ignored regenerator test following the existing pattern, one
  README entry, one more slot in the cascade — and no code change in the
  typesetter or any stager, because slot selection is positional
  (`docs/decisions/weight-selection-in-the-cascade.md`).

## Consequences

- The committed corpus grew by two font files and two atlas directories (about
  990 KB). This repository already accepts that cost for two fonts and two
  atlases.
- **Only Bold (700) and SemiBold (600) were added; Medium (500) was not.** Those
  two cover 25 of the hero's 27 non-Regular nodes and 376 of its 398 non-Regular
  characters. The two remaining nodes are weight 500, and under the matching
  rule this story adopted a request for 500 tries 400 before anything else
  (`docs/decisions/css-fonts-4-weight-matching-non-fatal.md`), so those nodes
  resolve to Regular by specification rather than by compromise. Medium can be
  added later on evidence, as a pure fixture addition.
- **No Arabic bold face.** The hero contains no Arabic and the E7 Arabic frame
  is Regular. The asymmetry is handled correctly by coverage ranking above
  weight: a bold Arabic run finds no bold face in the Arabic family and resolves
  to Arabic Regular, reporting `text.weight-substituted`
  (`docs/decisions/weight-substitution-is-a-render-time-diagnostic.md`).
- All three faces come from one release archive and one build variant
  (`NotoSans-v2.015`, `unhinted/ttf`), and the single committed `OFL.txt` covers
  all of them, so no new licence obligation arises.
  `corpus/fonts/noto-sans/README.md` records the provenance check.
- The cross-machine `atlas-repro` job now byte-reproduces four fixtures rather
  than two (`committed_ascii_semibold_fixture_is_reproducible` and
  `committed_ascii_bold_fixture_is_reproducible` join the two Regular checks),
  and two further ignored regenerators write them.
  `the_three_ascii_weights_are_distinct_faces` additionally pins that the three
  ASCII atlases cover the same charset and that a two-stem glyph advances wider
  at each heavier weight — a fixture accidentally baked from the wrong face
  passes both reproducibility checks and fails this one.
- The cost carried forward is the parallel list. A stager mirrors the cascade's
  slot order into an atlas list and indexes it directly, and that correspondence
  is enforced by comment rather than by a type. One more weight makes the list
  longer and the mistake easier to make.
