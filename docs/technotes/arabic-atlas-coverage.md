# Technote — Arabic coverage in msdf-atlas-gen

    status   done — findings recorded on issue #25
    story    #25 (epic #24, v0.5 — text I: Latin)
    date     2026-07-12
    informs  #27 (atlas pipeline), #34 (charsets), epic #31 (v0.6),
             docs/technotes/open-questions.md's Q-1

This note records how the spike was run and what it found, so the result can be
reproduced or re-checked later (for example on target hardware in v1). The
normative outcome lives in `docs/decisions/q1-msdf-below-14px.md` and
`docs/archive/2026-07-14-scope-decisions.md` §14.

## Questions

1. Does `msdf-atlas-gen` produce correct glyphs for Arabic contextual forms —
   glyphs that are reachable only through GSUB and have no codepoint (cmap)
   entry?
2. Q-1: is MSDF rendering legible at small sizes (below ~14 px per em), or does
   the project need per-size bitmap atlases there?

## Setup

    tool      msdf-atlas-gen 1.4.0 (MSDFgen 1.13.0), Homebrew bottle
    shaping   HarfBuzz (uharfbuzz 0.55.0) — stand-in for rustybuzz
    reference FreeType 2.13.2 rasterizer (grayscale, hinted and
              unhinted), via freetype-py
    fonts     Noto Sans Arabic v2.013 Regular, hinted build
              (representative UI font);
              Amiri 1.003 Regular (Naskh — stress case: hairline
              strokes, rich GSUB, stacked marks)

Scripts and images are in `assets/msdf-arabic-atlas-spike/`:

- `shape_corpus.py` — shapes a corpus of 211 strings: 17 realistic UI strings
  (digits, marks, lam-alef variants, tatweel, mixed Latin/Arabic) plus a
  systematic sweep that places every Arabic letter (U+0621–U+064A) in all four
  joining contexts and every haraka (U+064B–U+0652) on a base letter. Emits the
  union of output glyph ids as an `-glyphset` file, plus the shaped positions.
- `render_compare.py` — reconstructs whole shaped lines from the MSDF atlas
  (bilinear sample, median-of-three, one sample per pixel — the same math a
  product shader uses) next to FreeType rasterization of the same glyph ids at
  the same positions.
- `glyph_audit.py` — per-glyph mask comparison (MSDF reconstruction at 64 px vs
  FreeType) with a small shift search so placement conventions do not pollute
  the geometry measurement.

## Findings

### 1. Contextual-form coverage: pass

- `msdf-atlas-gen` accepts glyph ids directly (`-glyphset` / `-glyphs`), and
  loads GSUB-only glyphs with no cmap entry without complaint. Its JSON output
  keys each atlas entry by glyph `index`.
- Noto Sans Arabic: the corpus shapes to 113 distinct glyph ids, 28 of them
  GSUB-only. All 113 loaded (`113 out of 113`), all present in the JSON layout;
  only `space` has no bitmap, correctly (empty outline).
- Amiri: 248 distinct glyph ids, 176 GSUB-only. All 248 loaded and present.
- Geometry audit at 64 px per em, 1 px tolerance: Noto — zero glyphs deviate by
  more than 1 % of mask area outside the tolerance band. Amiri — 11 glyphs show
  1–5 % residual; visual overlays (`amiri-worst-glyph-overlays.png`) show these
  are hairline-stroke quantization at 32 px/em atlas resolution, not structural
  errors (no missing dots or marks, no inverted fill regions).
- Noto Sans Arabic builds letters as dotless skeletons plus separate dot glyphs,
  composed by GSUB. Dot glyphs are atlas entries of their own. This confirms the
  pinned contract: the atlas must be keyed by glyph id; codepoint keying cannot
  represent this font at all.

### 2. Q-1 small-size legibility: MSDF is good from 14 px per em up

Comparison sheets (MSDF reconstruction vs FreeType unhinted vs FreeType hinted,
9–28 px per em): `q1-noto-32.png`, `q1-noto-48.png`, `q1-amiri-32.png`.

- At 14 px/em and above, the MSDF render of Noto Sans Arabic is fully legible
  and visually close to the FreeType reference.
- At 12 px/em it stays legible but is visibly softer; letter teeth and dots
  begin to smear.
- At 9–10 px/em it degrades clearly: dots merge, harakat become blobs. FreeType
  (especially hinted) is distinctly better there.
- Raising the atlas resolution from 32 to 48 px/em (and `-pxrange` from 4 to 6)
  does not materially improve the range below 14 px/em. The limit is
  screen-pixel sampling and the absence of hinting, not atlas resolution — so a
  larger MSDF atlas is not the fix for small text; per-size bitmap glyphs would
  be.
- Amiri, as expected for a calligraphic Naskh with stacked marks, is harder
  everywhere: fully vocalized text needs roughly 16 px/em even under FreeType,
  and MSDF adds only a small extra softness on top. The practical floor is set
  by the style, not by MSDF.

### 3. Reproducibility (R7 input)

Two consecutive multi-threaded runs with a pinned `-seed` produced
byte-identical atlas PNG and JSON on this machine. For the #27 pipeline: pin the
`msdf-atlas-gen` version and the seed, and verify cross-machine byte-identity in
CI (not proven by this spike — one machine only).

### 4. Incidental findings for later stories

- Noto Sans Arabic contains no Latin letters and no U+002F solidus: "GPS" and
  "km/h"-style strings shape to `.notdef`. Mixed-script UI text therefore
  requires font fallback (a font list per style), and per-font charsets must be
  unioned per font, not per document (#34, #28).
- Marks arrive from shaping as separate glyphs with nonzero x/y offsets (GPOS).
  The glyph-run table and the runtime quad path must carry per-glyph offsets,
  not only advances (#26, #28).
- Shaping a mixed-direction string as a single run mis-orders embedded digits;
  the bidi split (unicode-bidi) must run before shaping, exactly as
  `docs/design/typeset-latin.md` already specifies.
- Practical defaults for #27 confirmed by use:
  `-type msdf -size 32
  -pxrange 4`, glyph-id input, JSON output; the JSON
  `distanceRange` and `size` must travel with the atlas into the metrics blob,
  since the shader needs them for the screen-pixel range computation.

## How to reproduce

    brew install msdf-atlas-gen harfbuzz
    python3 -m venv venv && venv/bin/pip install numpy pillow freetype-py uharfbuzz
    venv/bin/python shape_corpus.py <font.ttf> out/<name>
    msdf-atlas-gen -font <font.ttf> -glyphset out/<name>/glyphs.txt \
      -type msdf -size 32 -pxrange 4 -potr -seed 1 \
      -json out/<name>/atlas.json -imageout out/<name>/atlas.png
    venv/bin/python glyph_audit.py out/<name> <font.ttf>
    venv/bin/python render_compare.py out/<name> <font.ttf> sheet.png "<text>" ...
