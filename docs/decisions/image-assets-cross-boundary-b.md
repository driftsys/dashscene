# Image assets cross boundary B as encoded bytes in an ImageTable

    status   accepted (story #14, 2026-07-12); extended by
             baked-texel-payloads-cross-boundary-b.md (story #640, 2026-08-03),
             which adds the baked half this record's "new `ImageFormat`
             variants arrive additively" anticipated
    scope    dashpaint (Painter trait), every painter

## Context

The v0.3 vocabulary has image fills; stories #13 and #4 both recorded
"how decoded pixels reach a painter" as an open item for this story.
Boundary B is "the entire painter input" (`docs/design/architecture.md`), so the
asset path had to be part of the trait, not a side channel.

## Options

1. `Painter::paint` gains an `images: &ImageTable` parameter holding
   encoded, format-tagged bytes (`ImageAsset { format, bytes }`,
   mirroring `dashbuf`'s `Document.images`); painters decode with
   their own machinery.
2. The table holds decoded RGBA pixels (the runtime decodes once).
3. Out-of-band registration (painters expose their own image-upload
   API called before `paint`).

## Choice

Option 1.

## Why

- One decoded format (option 2) fits nobody: Skia decodes PNG natively,
  and the lean painter wants GPU-native compressed containers
  (`docs/specification/03-target-hardware-rules.md` — KTX2/Basis, transcoded, never re-decoded RGBA);
  format-tagged encoded bytes let each backend take its optimal path,
  and new `ImageFormat` variants arrive additively.
- Out-of-band registration (option 3) breaks §8's
  bisect-by-construction: the painter input would no longer be the
  complete, comparable value.
- The signature widening is exactly the in-workspace evolution the
  trait decision record reserved (glyph runs at v0.5 widen it again).
  An empty table is valid input for image-less scenes.
- `ImageTable` has the same push/get/resolve contract as `PaintTable`
  (resolve panics on an out-of-range index — validated upstream, P4).
  Index typing for image indices stays with debt #63's validator work.
