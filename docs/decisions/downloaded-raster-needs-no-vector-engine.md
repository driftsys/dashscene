# Downloaded raster (PNG/WebP) needs no runtime vector engine

    status   accepted
    date     2026-07-13
    source   docs/technotes/runtime-content.md §2
    scope    the image-fill vocabulary; the Skia trim profile

## Context

A downloaded PNG/WebP is already a bitmap. Image fills are already NOW
vocabulary (`docs/specification/04-figma-vocabulary-profile.md`: "image
fills + scale modes"); a downloaded
image is just an image fill whose source is bound at runtime.

## Choice

Path: decode → upload → bind. Decode to RGBA with a small pure-Rust
decoder (`png`, `image-webp`/`image`) — not by re-enabling Skia's codecs,
so the Skia trim (`docs/technotes/rendering-and-painters.md` §6) stays
intact — upload as a GPU texture, point the node's image fill at it, and
show `interim_fill` while loading.

## Why

- Keeps the Skia-codec cut in place: the trim profile removes
  libpng/libjpeg/libwebp on the assumption the runtime never needs them,
  and this path holds that assumption.
- Fully cross-backend and consistent across tiers: image fill is in
  every profile, Skia draws the quad, Unity binds the texture to a
  material slot, and it is static so there is nothing tier-specific to
  calibrate.
- Fits P1: the document carries "image fill → source slot"; pixels
  arrive at runtime like a slot-bound string, with no node-replacement
  machinery involved.

## Consequences

- Decoded RGBA is uncompressed, so it costs more VRAM/bandwidth than
  pre-transcoded ASTC assets — acceptable for occasional images (avatars,
  album art, map tiles); runtime transcoding to ASTC/EAC at scale is
  available since this is a photo, not a distance field.
- Being a bitmap, it scales like a photo: download at the box's size.
