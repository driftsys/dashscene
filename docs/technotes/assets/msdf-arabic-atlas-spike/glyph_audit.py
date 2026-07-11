#!/usr/bin/env python
"""Per-glyph correctness audit: reconstruct each atlas glyph at 64px from the
MSDF and compare its thresholded mask against FreeType's direct raster of the
same glyph id. Low IoU = broken/incorrect glyph geometry in the atlas.

Usage: glyph_audit.py <outdir> <font.ttf>
"""
import json
import sys
from pathlib import Path

import freetype
import numpy as np
from PIL import Image

OUTDIR = Path(sys.argv[1])
FONTFILE = sys.argv[2]
PX = 64

atlas_meta = json.load(open(OUTDIR / "atlas.json"))
names = json.load(open(OUTDIR / "gid_names.json"))
A = atlas_meta["atlas"]
DIST_RANGE_EM = A["distanceRange"] / A["size"]
atlas_img = np.asarray(Image.open(OUTDIR / "atlas.png").convert("RGB"), dtype=np.float32) / 255.0
AH, AW, _ = atlas_img.shape

face = freetype.Face(FONTFILE)
face.set_pixel_sizes(0, PX)

def msdf_mask(g):
    pb, ab = g["planeBounds"], g["atlasBounds"]
    w = int(np.ceil((pb["right"] - pb["left"]) * PX)) + 2
    h = int(np.ceil((pb["top"] - pb["bottom"]) * PX)) + 2
    ys, xs = np.mgrid[0:h, 0:w]
    u = (xs + 0.5 - 1 - 0) / ((pb["right"] - pb["left"]) * PX)
    v = (ys + 0.5 - 1) / ((pb["top"] - pb["bottom"]) * PX)
    ax = ab["left"] + u * (ab["right"] - ab["left"])
    ay = AH - (ab["bottom"] + (1 - v) * (ab["top"] - ab["bottom"]))
    ax = np.clip(ax, 0, AW - 1.001); ay = np.clip(ay, 0, AH - 1.001)
    x0 = np.floor(ax).astype(int); y0 = np.floor(ay).astype(int)
    fx = (ax - x0)[..., None]; fy = (ay - y0)[..., None]
    p = (atlas_img[y0, x0] * (1 - fx) + atlas_img[y0, x0 + 1] * fx) * (1 - fy) + \
        (atlas_img[y0 + 1, x0] * (1 - fx) + atlas_img[y0 + 1, x0 + 1] * fx) * fy
    sd = np.median(p, axis=-1)
    return sd > 0.5, (pb["left"], pb["top"])

def ft_mask(gid, origin, shape):
    face.load_glyph(gid, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_NO_HINTING)
    bmp = face.glyph.bitmap
    out = np.zeros(shape, dtype=bool)
    if bmp.width and bmp.rows:
        arr = np.array(bmp.buffer, dtype=np.float32).reshape(bmp.rows, bmp.pitch)[:, : bmp.width] / 255.0
        # place FT bitmap into the same plane-bounds frame as the msdf mask
        ox = int(round(1 + face.glyph.bitmap_left - origin[0] * PX))
        oy = int(round(1 + origin[1] * PX - face.glyph.bitmap_top))
        x0, y0 = max(ox, 0), max(oy, 0)
        x1 = min(ox + bmp.width, shape[1]); y1 = min(oy + bmp.rows, shape[0])
        if x1 > x0 and y1 > y0:
            out[y0:y1, x0:x1] = arr[y0 - oy : y1 - oy, x0 - ox : x1 - ox] > 0.5
    return out

def best_iou(m, f):
    """IoU maximized over small integer shifts — placement-convention-proof."""
    best = 0.0
    for dy in range(-2, 3):
        for dx in range(-2, 3):
            fs = np.roll(np.roll(f, dy, 0), dx, 1)
            union = (m | fs).sum()
            iou = (m & fs).sum() / union if union else 1.0
            best = max(best, iou)
    return best

rows = []
for g in atlas_meta["glyphs"]:
    if "atlasBounds" not in g:
        continue
    m, origin = msdf_mask(g)
    f = ft_mask(g["index"], origin, m.shape)
    rows.append((best_iou(m, f), g["index"], names.get(str(g["index"]), "?")))

rows.sort()
ious = np.array([r[0] for r in rows])
print(f"glyphs audited: {len(rows)}  IoU mean={ious.mean():.4f} min={ious.min():.4f} median={np.median(ious):.4f}")
print(f"glyphs with IoU < 0.90: {(ious < 0.90).sum()}")
print("worst 15:")
for iou, gid, name in rows[:15]:
    print(f"  IoU={iou:.4f}  gid={gid:5d}  {name}")
