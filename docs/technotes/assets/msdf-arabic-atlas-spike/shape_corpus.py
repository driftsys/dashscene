#!/usr/bin/env python
"""Shape an Arabic corpus with HarfBuzz and collect GSUB output glyph ids.

Outputs:
  <out>/glyphs.txt          glyph-id set, msdf-atlas-gen -glyphset syntax
  <out>/shaped.json         per-string shaping results (gid, offsets, advances)
  stdout                    coverage report
"""
import json
import sys
from pathlib import Path

import uharfbuzz as hb

FONT = sys.argv[1]
OUT = Path(sys.argv[2])
OUT.mkdir(parents=True, exist_ok=True)

# --- corpus: realistic UI strings -----------------------------------------
UI_STRINGS = [
    "السلام عليكم ورحمة الله",          # greeting, lam-alef, Allah ligature
    "مرحبا بالعالم",                     # hello world
    "درجة الحرارة ٢٣ درجة",             # temperature + Arabic-Indic digits
    "السرعة ١٢٠ كم/س",                   # speed + digits + slash
    "المسافة المتبقية ٢٥٠ كم",           # remaining distance
    "شحن البطارية ٨٥٪",                  # battery + percent
    "تكييف الهواء مُفعَّل",              # A/C enabled — damma, shadda+fatha
    "الوضع الليلي: مفعل",                # night mode
    "فحص ضغط الإطارات",                  # tire pressure check
    "إشارة GPS ضعيفة",                   # mixed Latin/Arabic
    "لا لأ لإ لآ",                        # all lam-alef ligature variants
    "بلا بلأ بلإ بلآ",                    # ...in joined (final) context
    "ذهب الطالب إلى المدرسة صباحاً",     # tanween
    "مـــرحبا",                          # tatweel / kashida
    "بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ",  # fully vocalized (marks stress)
    "قِفْ ثُمَّ انطَلِقْ",                # sukun, shadda, kasra
    "؟ ، ؛ ٪ ٫ ٬",                        # Arabic punctuation
]

# --- systematic sweep: every Arabic letter in 4 joining contexts -----------
# isolated "X", final "بX", initial "Xب", medial "بXب"
ARABIC_LETTERS = [chr(c) for c in range(0x0621, 0x064B)]   # hamza..yeh
HARAKAT = [chr(c) for c in range(0x064B, 0x0653)]           # fathatan..sukun
DIGITS = [chr(c) for c in range(0x0660, 0x066A)]            # ٠-٩

sweep = []
for x in ARABIC_LETTERS:
    sweep += [x, "ب" + x, x + "ب", "ب" + x + "ب"]
for m in HARAKAT:
    sweep += ["ب" + m, "بب" + m]                            # mark on isolated + joined base
sweep += DIGITS

ALL = UI_STRINGS + sweep

# --- shape ------------------------------------------------------------------
blob = hb.Blob.from_file_path(FONT)
face = hb.Face(blob)
font = hb.Font(face)
upem = face.upem

# reverse cmap: which gids are directly reachable from a codepoint
cmap_gids = set()
for cp in face.unicodes:
    gid = font.get_nominal_glyph(cp)
    if gid is not None:
        cmap_gids.add(gid)

def shape(text: str):
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(font, buf, {})
    return [
        {
            "gid": info.codepoint,
            "cluster": info.cluster,
            "xa": pos.x_advance,
            "ya": pos.y_advance,
            "xo": pos.x_offset,
            "yo": pos.y_offset,
        }
        for info, pos in zip(buf.glyph_infos, buf.glyph_positions)
    ]

shaped = {}
all_gids = set()
for text in ALL:
    glyphs = shape(text)
    shaped[text] = glyphs
    all_gids.update(g["gid"] for g in glyphs)

gsub_only = sorted(g for g in all_gids if g not in cmap_gids)
notdef = 0 in all_gids

def gname(gid):
    return font.glyph_to_string(gid)

report = {
    "font": FONT,
    "upem": upem,
    "glyph_count_in_font": face.glyph_count,
    "strings_shaped": len(ALL),
    "distinct_output_gids": len(all_gids),
    "gsub_only_gids (no cmap entry -> contextual/ligature forms)": len(gsub_only),
    "notdef_produced": notdef,
}
print(json.dumps(report, indent=2, ensure_ascii=False))
print("\nsample GSUB-only glyphs (first 40):")
for g in gsub_only[:40]:
    print(f"  {g:5d}  {gname(g)}")

(OUT / "glyphs.txt").write_text(" ".join(str(g) for g in sorted(all_gids)))
(OUT / "shaped.json").write_text(
    json.dumps({"font": FONT, "upem": upem, "strings": shaped}, ensure_ascii=False)
)
(OUT / "gid_names.json").write_text(
    json.dumps({str(g): gname(g) for g in sorted(all_gids)}, ensure_ascii=False)
)
print(f"\nwrote {OUT}/glyphs.txt ({len(all_gids)} gids), shaped.json, gid_names.json")
