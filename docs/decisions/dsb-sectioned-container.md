# .dsb becomes a thin sectioned container; each section is one flatbuffer

    status   accepted (spike #56, 2026-07-12); binds schema design.
             Refined 2026-07-12 (design session): envelope form,
             section kinds, alignment policy, endianness — see
             "Refinements" below.
             AS-BUILT 2026-07-26 (v0.11, stories #399 and #401): the
             envelope was written in dashbuf (#399) and `.dsb` files
             became containers (#401). The deferral below to "the v1
             loading-performance work" no longer holds — the v0.10 close
             moved that work into v0.11. Byte layout:
             docs/design/dsb-container-format.md. The one-time golden
             re-baseline it caused:
             docs/decisions/r7-survives-the-envelope-rebaseline.md.
    scope    crates/dashbuf, the .dsb file format

## Context

`docs/design/dashbuf.md` (R5) requires hot sections (tree, tables, strings)
packed at the file head, cold sections (heavy decor) page-aligned at the tail,
and per-section hashes so the load gate verifies hot sections without touching
cold pages. Spike #56 measured how much of that a single flatbuffer can express,
against the exact toolchain this repo pins: `flatc` 25.12.19 and the
`flatbuffers` Rust crate 25.12.19. Full evidence is on the issue
(<https://github.com/driftsys/dashscene/issues/56>).

## Options

1. Keep `.dsb` a single flatbuffer; approximate the section discipline with
   write ordering and padding hacks.
2. A thin container: fixed envelope (magic, version, section table with
   per-section id, flags, offset, length, hash), one complete flatbuffer per
   section.
3. In-band composition inside one flatbuffer (`nested_flatbuffer` or
   size-prefixed concatenation).

## Choice

Option 2: `.dsb` is a thin sectioned container. Each section is an independent
flatbuffer with its own `root_type` and `file_identifier`. Two rules bind the
schema stories (#8, #13, #20, #26) immediately:

1. Cross-table references are integer indices, never flatbuffer offsets that
   reach into another future section.
2. Every section-destined table (node tree, layout, paint, variant,
   text/strings, heavy decor) stays one offset away from `Document`, so lifting
   it to a section root is mechanical.

## Why

- Page alignment for cold sections is not expressible through any supported
  route for a Rust producer. `flatc` caps struct `force_align` at 32; vector
  `force_align` above 32 parses, but only C++ codegen honors it (upstream
  documents the attribute as C++-only, and the C++ runtime asserts
  `alignment <=
  FLATBUFFERS_MAX_ALIGNMENT` (32) in debug builds). Rust codegen
  silently ignores vector `force_align`, and `FlatBufferBuilder` in the Rust
  crate has no direct alignment method (no `ForceVectorAlignment`/`PreAlign`
  equivalent). The public `Push`/`PushAlignment` traits are uncapped, but the
  vector constructors do not take a custom alignment, so page alignment is
  reachable only through unsupported custom-`Push` workarounds. The producer
  side is Rust (`dashc`), so the C++-only routes do not apply.
- In the Rust builder, vtable deduplication is unconditional: a hot table's
  vtable can physically live among cold bytes when shapes collide, which no
  schema discipline can prevent. C++ exposes `DedupVtables(bool)`; the Rust
  crate has no equivalent. A clean hot/cold page partition inside one Rust-built
  buffer is therefore not guaranteeable even with perfect write ordering.
- The verifier's supported entry points walk everything reachable from the root,
  and `VerifierOptions` has no scoping option. A hot-only load gate on a single
  buffer would have to be hand-composed from the crate's lower-level verifier
  primitives (`Verifiable::run_verifier`), which upstream documents as not meant
  to be called directly.
- FlatBuffers has no integrity layer; in a single buffer, sections have no
  stable byte ranges to hash. In the container, each hash covers a contiguous
  range any external signing tool can compute.
- Option 3 fails for the same reasons: `nested_flatbuffer` is a `[ubyte]` field
  inside the outer buffer (same alignment situation, Rust ignores its
  `force_align`), and size-prefixed concatenation gives no random access,
  alignment, or integrity.
- The container preserves `docs/decisions/dsb-format-and-one-schema.md` ("one
  schema for file and wire"): both roles frame the same per-section flatbuffers
  — the wire keeps length-prefixed framing, the file role uses the envelope.
  Cross-section references as indices cost little: the design already prescribes
  indices (flattened DFS tree, doc index = rect-table index, a u32 paint index
  in the boundary-B rect entry). The one deviation at spike time was
  `Node.paint`, an inline table; story #13 has since landed the
  `Document.paints` pool indexed by `Node.paint_entry`, satisfying rule 1 (the
  legacy inline field remains until the coordinated cleanup — see
  `document-paint-pool-and-legacy-paint-field.md`).

## Refinements (design session, 2026-07-12)

A design session elaborated the container's concrete form. Related records from
the same session: `asset-model-content-addressed-blobs.md` (what the blob
sections carry) and `remoting-two-transports.md` (how the same sections travel
without the envelope).

### Envelope form

The envelope is a hand-specified fixed layout — a `#[repr(C)]` struct in
`dashbuf` plus a format doc (the slot DESIGN's repo sketch already reserves:
"format doc (section table, hashes, reserved fields)"):

- **Header**: magic, format version, header size, flags, section count, root
  hash, and a reserved signature reference. The magic is a byte array, not an
  integer, so it is endianness-free and visible in a hex dump.
- **Section table**: N fixed-stride 64-byte entries directly after the header —
  per entry: section kind, flavor flags, byte offset (u64), byte length (u64),
  content hash (32 bytes). Fixed stride gives O(1) indexed access, and the
  signed byte range is `header_size + count * 64` by construction.
- **Not flatbuffers, including not flatbuffer structs.** The envelope is
  validated before any parser is trusted, so checking it must be plain
  bounds/magic/version comparisons on fixed offsets. A struct cannot be a
  flatbuffer `root_type`, so "flatbuffer structs" would still pull in root-table
  framing and the verifier — into the one component that exists to stand outside
  them. The envelope evolves by version bump, not by field-id rules; a frozen
  layout is the intent.
- Layout hygiene: no implicit padding (every gap is a named reserved field) and
  a compile-time size assertion pins the struct size.

Two narrowings, recorded when the envelope was built (story #399, 2026-07-26;
byte layout in `docs/design/dsb-container-format.md`):

- The header carries one field beyond the list above: **`section_stride`**, next
  to `header_size`. Both are recorded so the table is self-describing and the
  external signing tool this record names can compute
  `header_size + count * stride` without hardcoding either number. It is not a
  forward-compatibility mechanism — the version rule above already refuses a
  version this reader does not implement — but it turns a stride mismatch into a
  named error instead of a misparse.
- **`flavor` is an enumerated role, not flags.** A section has exactly one role,
  and the reader compares the whole field for equality; two roles would need two
  entries, not two bits. The wording above is narrowed accordingly.

### Section kinds: structured vs blob

The section table distinguishes two kinds:

- **Structured** — a complete flatbuffer (the ui document; later splits if
  profiling justifies them). Verified with the stock flatbuffers verifier after
  its hash check.
- **Blob** — raw payload bytes with no dashscene framing (see the asset-model
  record). Verified by hash only.

So the file holds exactly three byte-languages: the hand-specified envelope,
ordinary flatbuffers, and raw well-known payload formats.

### Alignment policy

Page alignment is required in exactly one place: the boundary between the hot
region (envelope + structured sections) and the first cold byte — that is what
lets the load gate verify hot bytes without faulting cold pages. Everything else
is writer policy, not format law:

- Blobs align to a small universal quantum (64 bytes) so a pointer into the
  mapping satisfies any consumer's natural alignment.
- Large blobs (threshold, for example 64 KiB) are page-aligned so they can be
  individually prefetched and evicted (`madvise` per range); small blobs pack
  densely — two small blobs sharing a page is harmless because verification and
  readiness are per-blob, and a shared page faulting early is free prefetch.
- The format only records offsets; packing heuristics can improve without a
  format change.

### Loading model

One `mmap`, once, and until story #1124 that was always of the whole file. It
can now also be of a byte range **inside** a larger file — a `.dsb` packed
uncompressed in an Android APK has no path of its own, and copying it out is the
cost mapping exists to avoid
(`docs/decisions/the-document-is-mapped-where-it-is-packed.md`). Nothing below
changes for that case: the mapped region is still one mapping and sections are
still offset ranges inside it, measured from the document's first byte rather
than the file's. What it costs is the exactness of the page alignment named
above, which D4 of that record states.

The envelope is read through the mapping (page 0 faults — it is the hottest data
in the file): validate magic/version/bounds, hash the section table against the
root hash, then hash + verify structured sections, and hand `Document` to the
arena. Blob sections are untouched until the loader thread prefetches them
(touch + hash + mark ready). There is no read-then-map two-step and no
per-section mapping; sections are offset ranges inside the one mapping.

### Endianness

The format is little-endian, declared once in the format doc — the same
commitment flatbuffers already makes, so the whole file has one byte-order
story. Readers never reinterpret native memory: every envelope field is read
through an explicit little-endian accessor (`from_le_bytes`-style, or
explicit-endian field types), which compiles to a plain load on little-endian
targets. Big-endian remains correct by construction but is deferred as a target:
no BE hardware is tested or supported until one exists on the roadmap. The rule
is structural because every shipping and testing target (x86_64, aarch64,
wasm32) is little-endian — a native-endian cast would pass every test and still
be wrong, so no test can enforce the rule.
