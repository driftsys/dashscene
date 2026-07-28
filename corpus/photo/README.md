# Photographic payloads

Real photographic and photorealistic-render content, for measuring the asset
pipeline's quality bands against the content class the product actually ships
(`docs/wip/2026-07-28-photorealistic-3d-content.md`, issue #455).

These are **not** Figma fixtures. Nothing here is captured from a Figma file;
each is a raster payload measured directly by
`crates/dashpack/tests/band_contract.rs` and
`goldens/tooling/tests/perceptual_calibration.rs`, the same way
`corpus/atlas/*/atlas.png` is. That is what makes them admissible without a
Figma document: the licence question is about the payload alone
(`docs/decisions/figma-corpus-self-authored-only.md`, the 2026-07-28 scoping
ruling).

## Why these exist

Before them, every band number in the asset pipeline was measured on a
gradient with flat rectangles, a 16x16 near-solid, two MSDF atlases, and
generated noise. `lofi-image-fill`'s 5 % area budget was the binding term for
exactly one fixture, and that fixture was generated inside the test rather than
committed — the state issue #422 recorded against `blur-falloff`, and what
issue #455 exists to close.

## Provenance

Every payload here carries its source. CC0 obliges no attribution, so this
table is this repository's own audit trail rather than a licence condition —
an unlisted third-party payload is a defect regardless of how it is licensed.

| payload | source | licence as stated at source | retrieved | original |
| ------- | ------ | --------------------------- | --------- | -------- |
| `interior-render.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File%3AMission_Flats_C2_interior_render_%283%29.jpg) | CC0 | 2026-07-28 | 2560x1440 |
| `coast-forest.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File%3ALandscape-coast-forest-mountain-architecture-sky-1139883.jpg) | CC0 | 2026-07-28 | 3056x2034 |
| `snowy-forest.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File%3ASnowy_Forest_and_Mountain_Landscape.jpg) | CC0 | 2026-07-28 | 994x1500 |
| `dawn-mountains.png` | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File%3ADawn-landscape-mountains-forest_%2824031322510%29.jpg) | CC0 | 2026-07-28 | 5472x3648 |

Each was verified CC0 through the Commons API rather than by reading a page
banner: `action=query&prop=imageinfo&iiprop=extmetadata` returns
`LicenseShortName` and `UsageTerms`, and all four report `CC0` /
"Creative Commons Zero, Public Domain Dedication".

**What that verification does not establish**, per the ruling: it does not
clear trademark, it does not clear rights held by third parties depicted in
the work, and it cannot confirm the uploader held the right to apply CC0. Those
were reviewed by eye for these four — landscapes and an architectural render,
no identifiable people, no visible artworks — and that review is a judgement,
not a guarantee.

## How each payload was prepared

**The preparation is part of the fixture, not an import detail.** It changes
which rung the packer selects, measured: `wilderness` selects astc-10x10 when
the whole frame is scaled down and astc-12x12 when a native-resolution region
is cropped, and `forest-lake` moves between astc-5x5 and astc-6x6 the same way.
Downscaling averages away exactly the high-frequency detail block compression
is worst at, which is the property these payloads exist to carry.

All four use the **whole frame**, scaled to fill 512x512 and centre-extended:

```sh
magick <original> -resize '512x512^' -gravity center -extent 512x512 \
    -alpha off PNG24:<name>.png
```

Whole-frame rather than a native crop, because the landscape class is defined
by its *composition* — broad smooth sky or water beside very fine foliage or
rock detail, with the smooth part usually dominant. A 512x512 crop of a 20
megapixel photograph is a 2.5 % window that lands entirely in one or the other
and loses the property being measured.

The originals are JPEG, so each payload carries JPEG artifacts beneath the
ASTC error. That is realistic — a real image fill arrives the same way — but it
means these do not isolate ASTC error the way a synthetic fixture does.

## What each one covers

| payload | class | what it exercises |
| ------- | ----- | ----------------- |
| `interior-render.png` | photorealistic 3D interior render | dense material detail throughout; the class named first in the target-content capture |
| `coast-forest.png` | landscape photograph | the ladder's astc-4x4 rung, which no previous fixture reached |
| `snowy-forest.png` | landscape photograph | the astc-5x5 rung, the other previously unreached one |
| `dawn-mountains.png` | landscape, mostly smooth | the only payload here on which HiFi selects a *lossy* rung at all |
