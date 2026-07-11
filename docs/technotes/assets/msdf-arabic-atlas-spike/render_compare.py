#!/usr/bin/env python
"""Q-1 check: render shaped Arabic strings from the MSDF atlas at small sizes
and compare against direct FreeType rasterization (unhinted + hinted).

Usage: render_compare.py <fontdir(out/noto)> <font.ttf> <sheet.png> <string> [<string>...]
"""
import json
import sys
from pathlib import Path

import freetype
import numpy as np
from PIL import Image, ImageDraw

OUTDIR = Path(sys.argv[1])
FONTFILE = sys.argv[2]
SHEET = sys.argv[3]
STRINGS = sys.argv[4:]

SIZES = [9, 10, 12, 14, 16, 20, 28]
MAG = 3  # magnification for the sheet
PAD = 6  # px padding around each strip

atlas_meta = json.load(open(OUTDIR / "atlas.json"))
shaped = json.load(open(OUTDIR / "shaped.json"))
upem = shaped["upem"]

A = atlas_meta["atlas"]
ATLAS_PX_PER_EM = A["size"]
DIST_RANGE_ATLAS_PX = A["distanceRange"]
DIST_RANGE_EM = DIST_RANGE_ATLAS_PX / ATLAS_PX_PER_EM
assert A["yOrigin"] == "bottom"

atlas_img = np.asarray(Image.open(OUTDIR / "atlas.png").convert("RGB"), dtype=np.float32) / 255.0
AH, AW, _ = atlas_img.shape
glyph_by_index = {g["index"]: g for g in atlas_meta["glyphs"]}


def sample_bilinear(img, x, y):
    """Sample img at float coords (x, y), top-left origin, clamped."""
    x = np.clip(x, 0, AW - 1.001)
    y = np.clip(y, 0, AH - 1.001)
    x0 = np.floor(x).astype(int); y0 = np.floor(y).astype(int)
    fx = (x - x0)[..., None]; fy = (y - y0)[..., None]
    p00 = img[y0, x0]; p10 = img[y0, x0 + 1]
    p01 = img[y0 + 1, x0]; p11 = img[y0 + 1, x0 + 1]
    return (p00 * (1 - fx) + p10 * fx) * (1 - fy) + (p01 * (1 - fx) + p11 * fx) * fy


def median3(rgb):
    return np.median(rgb, axis=-1)


def line_metrics(glyphs, px):
    """Pen extents of a shaped line in px (x advance sum)."""
    return sum(g["xa"] for g in glyphs) * px / upem


def render_msdf(text, px):
    glyphs = shaped["strings"][text]
    width = int(np.ceil(line_metrics(glyphs, px))) + 2 * PAD
    asc = int(np.ceil(1.2 * px)); desc = int(np.ceil(0.7 * px))
    height = asc + desc
    out = np.zeros((height, width), dtype=np.float32)
    baseline = asc
    pen_x = float(PAD)
    for g in glyphs:
        gi = glyph_by_index.get(g["gid"])
        ox = g["xo"] * px / upem
        oy = g["yo"] * px / upem
        if gi and "atlasBounds" in gi:
            pb, ab = gi["planeBounds"], gi["atlasBounds"]
            # screen-space quad (top-left origin)
            x0 = pen_x + ox + pb["left"] * px
            x1 = pen_x + ox + pb["right"] * px
            y0 = baseline - oy - pb["top"] * px
            y1 = baseline - oy - pb["bottom"] * px
            ix0, ix1 = int(np.floor(x0)), int(np.ceil(x1))
            iy0, iy1 = int(np.floor(y0)), int(np.ceil(y1))
            ix0c, iy0c = max(ix0, 0), max(iy0, 0)
            ix1c, iy1c = min(ix1, width), min(iy1, height)
            if ix1c > ix0c and iy1c > iy0c:
                ys, xs = np.mgrid[iy0c:iy1c, ix0c:ix1c]
                # pixel centers -> uv in plane quad
                u = (xs + 0.5 - x0) / (x1 - x0)
                v = (ys + 0.5 - y0) / (y1 - y0)
                # atlas coords (json yOrigin=bottom -> flip to image top-left)
                ax = ab["left"] + u * (ab["right"] - ab["left"])
                ay_bottom = ab["bottom"] + (1 - v) * (ab["top"] - ab["bottom"])
                ay = AH - ay_bottom
                sd = median3(sample_bilinear(atlas_img, ax, ay))
                screen_px_range = DIST_RANGE_EM * px
                alpha = np.clip((sd - 0.5) * screen_px_range + 0.5, 0, 1)
                region = out[iy0c:iy1c, ix0c:ix1c]
                out[iy0c:iy1c, ix0c:ix1c] = np.maximum(region, alpha)
        pen_x += g["xa"] * px / upem
    return out


def render_freetype(text, px, hinting):
    glyphs = shaped["strings"][text]
    width = int(np.ceil(line_metrics(glyphs, px))) + 2 * PAD
    asc = int(np.ceil(1.2 * px)); desc = int(np.ceil(0.7 * px))
    height = asc + desc
    out = np.zeros((height, width), dtype=np.float32)
    baseline = asc
    face = freetype.Face(FONTFILE)
    face.set_pixel_sizes(0, px)
    flags = freetype.FT_LOAD_RENDER
    if not hinting:
        flags |= freetype.FT_LOAD_NO_HINTING
    pen_x = float(PAD)
    for g in glyphs:
        ox = g["xo"] * px / upem
        oy = g["yo"] * px / upem
        face.load_glyph(g["gid"], flags)
        bmp = face.glyph.bitmap
        if bmp.width and bmp.rows:
            arr = np.array(bmp.buffer, dtype=np.float32).reshape(bmp.rows, bmp.pitch)[:, : bmp.width] / 255.0
            x0 = int(round(pen_x + ox)) + face.glyph.bitmap_left
            y0 = int(round(baseline - oy)) - face.glyph.bitmap_top
            ix0c, iy0c = max(x0, 0), max(y0, 0)
            ix1c = min(x0 + bmp.width, width); iy1c = min(y0 + bmp.rows, height)
            if ix1c > ix0c and iy1c > iy0c:
                sub = arr[iy0c - y0 : iy1c - y0, ix0c - x0 : ix1c - x0]
                region = out[iy0c:iy1c, ix0c:ix1c]
                out[iy0c:iy1c, ix0c:ix1c] = np.maximum(region, sub)
        pen_x += g["xa"] * px / upem
    return out


def to_img(a):
    return Image.fromarray((255 * (1 - a)).astype(np.uint8), "L").convert("RGB")  # black on white


COLS = ["MSDF 32px/em atlas", "FreeType unhinted", "FreeType hinted"]

strips = []   # (label, [img, img, img], mae)
for text in STRINGS:
    for px in SIZES:
        r_msdf = render_msdf(text, px)
        r_unh = render_freetype(text, px, hinting=False)
        r_hin = render_freetype(text, px, hinting=True)
        h = min(r_msdf.shape[0], r_unh.shape[0])
        w = min(r_msdf.shape[1], r_unh.shape[1])
        mae = float(np.abs(r_msdf[:h, :w] - r_unh[:h, :w]).mean())
        strips.append((f"{px:>2}px  {text}", [to_img(r) for r in (r_msdf, r_unh, r_hin)], mae))
        print(f"{px:>3}px  MAE(msdf vs ft-unhinted)={mae:.4f}  {text}")

# --- compose sheet ---------------------------------------------------------
label_h = 14
col_gap = 12
col_w = max(im.width for _, ims, _ in strips for im in ims) * MAG
row_hs = [max(im.height for im in ims) * MAG + label_h + 4 for _, ims, _ in strips]
W = 3 * col_w + 2 * col_gap + 20
H = sum(row_hs) + 30
sheet = Image.new("RGB", (W, H), "white")
d = ImageDraw.Draw(sheet)
for c, name in enumerate(COLS):
    d.text((10 + c * (col_w + col_gap), 4, ), name, fill="black")
y = 24
for (label, ims, mae) in strips:
    d.text((10, y), f"{label}   (MAE {mae:.3f})", fill=(120, 0, 0))
    y += label_h + 2
    for c, im in enumerate(ims):
        big = im.resize((im.width * MAG, im.height * MAG), Image.NEAREST)
        sheet.paste(big, (10 + c * (col_w + col_gap), y))
    y += max(im.height for im in ims) * MAG + 2
sheet.save(SHEET)
print("sheet:", SHEET, sheet.size)
