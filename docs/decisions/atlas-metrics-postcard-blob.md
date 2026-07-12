# Decision: atlas metrics blob is a versioned struct, postcard-serialized, canonical bytes

    status   accepted (story #27, 2026-07-12)
    scope    crates/dashscene-typeset atlas module — the metrics blob
             (atlas/metrics.rs)

## Context

The atlas pipeline emits a second artifact next to `atlas.png`: font,
atlas, and per-glyph metrics a painter or the typesetter needs at
runtime. Story #27 had to pick the on-disk format, with byte-reproducible
output (R7) as a hard constraint.

## Options

1. A versioned Rust struct (`AtlasMetrics`) serialized with `postcard`,
   file name `atlas.metrics` next to `atlas.png`, with vectors explicitly
   pre-sorted (glyphs by glyph id, missing codepoints ascending) so
   encoding is canonical.
2. Canonical JSON.
3. A hand-rolled binary writer.
4. A flatbuffer table in `dashbuf`, alongside the `.dsb` schema.

## Choice

Option 1. `AtlasMetrics::to_bytes`/`from_bytes` wrap
`postcard::to_allocvec`/`from_bytes`; `from_bytes` rejects any
`format_version` other than the crate's `FORMAT_VERSION` constant.
`AtlasBundle::write_to_dir`/`load_from_dir` round-trip the blob through
`atlas.metrics`.

## Why

- Option 2 (canonical JSON) is rejected: float-to-text formatting is a
  reproducibility trap (the same `f32` can serialize differently across
  formatting-library versions or float edge cases), and the blob is
  machine-only data with no need for human readability.
- Option 3 (hand-rolled binary writer) is rejected: it is more code to
  write and maintain for no advantage over postcard's already-stable,
  deterministic wire format.
- Option 4 (flatbuffer in `dashbuf` now) is rejected for this story: the
  atlas is an asset, not the document; packaging atlases into `.dsb`
  sections is a later slice's concern. The typed `AtlasMetrics` loader
  isolates painters and the typesetter from that future change — moving
  to a `.dsb`-embedded representation later does not have to change the
  in-memory type, only its I/O.
- postcard's field-order-determined encoding plus pre-sorted vectors
  make identical inputs produce identical bytes; this is what the
  `blob_bytes_are_canonical` and `double_run_is_byte_identical` tests
  verify, and what CI's cross-machine `atlas-repro` check depends on
  (see `docs/design/atlas-pipeline.md` Determinism).
- `format_version` is a stored field, not just a doc convention: an
  unsupported version fails to load (`Metrics` error) rather than being
  silently misinterpreted as the current layout.
