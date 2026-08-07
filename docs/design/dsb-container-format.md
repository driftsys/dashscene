# The .dsb container format

    crate    crates/dashbuf (`src/container.rs`, `src/bank.rs`)
    covers   v0.11 the sectioned envelope (story #399);
             v0.12 cold-bank assembly (story #433)
    traces   docs/decisions/dsb-sectioned-container.md,
             docs/decisions/asset-model-content-addressed-blobs.md,
             docs/decisions/dsb-format-and-one-schema.md,
             docs/decisions/dsb-frozen-fixture-r7-guard.md

## Purpose

A `.dsb` file is a thin container. It holds a fixed envelope — a header and a
section table — followed by section payloads. Each section is either a complete
flatbuffer or a raw payload blob.

The container exists for four properties the schema alone cannot give
(`docs/decisions/dsb-sectioned-container.md` has the measured evidence from
spike #56):

- **A page-aligned hot/cold boundary**, so a load gate can verify everything it
  needs to lay out and paint without faulting a single cold page.
- **Stable byte ranges to hash.** FlatBuffers has no integrity layer; inside one
  buffer a table has no contiguous range an external signing tool could cover.
- **Per-section verification.** The stock flatbuffers verifier walks everything
  reachable from the root and cannot be scoped.
- **Random access to payloads** that can be evicted, prefetched, and fetched
  lazily — none of which is possible for bytes embedded in the ui buffer.

The file therefore holds exactly three byte-languages: this envelope, ordinary
flatbuffers, and raw well-known payload formats (a PNG, a JPEG, a KTX2). A blob
extracted byte for byte is a valid file of its own format.

## Endianness

The format is little-endian. Every multi-byte field is read and written through
an explicit little-endian accessor; a reader never reinterprets native memory.
Big-endian remains correct by construction and is deferred as a target — no
big-endian hardware is tested or supported until one appears on the roadmap.

The rule is structural rather than tested on purpose: every target this repo
builds for (`x86_64`, `aarch64`, `wasm32`) is little-endian, so a native cast
would pass every test and still be wrong.

## Versioning

The envelope is a frozen layout. It evolves by **version bump**, not by
field-id rules: a reader that does not implement `format_version` refuses the
file whole rather than guessing at it. Reserved fields are written zero, and a
non-zero reserved field is a parse error for the same reason — a writer that
used one without bumping the version produced something this reader must not
interpret.

This is the opposite of the schema's rule inside a section, where evolution is
append-only and old readers keep working (`docs/design/dashbuf.md`, R7). The two
rules coexist because they protect different things: the schema protects
documents already written to disk, and the envelope protects the one component
that runs before any parser is trusted.

## Header — 64 bytes at offset 0

| offset | size | field              | notes                               |
| ------ | ---- | ------------------ | ----------------------------------- |
| 0      | 8    | `magic`            | `89 44 53 42 0D 0A 1A 0A`           |
| 8      | 2    | `format_version`   | 1                                   |
| 10     | 2    | `header_size`      | 64                                  |
| 12     | 2    | `section_stride`   | 64                                  |
| 14     | 2    | `reserved_0`       | zero                                |
| 16     | 4    | `section_count`    |                                     |
| 20     | 4    | `flags`            | zero in version 1                   |
| 24     | 32   | `root_hash`        | BLAKE3 over the section-table bytes |
| 56     | 4    | `signature_offset` | reserved; zero in version 1         |
| 60     | 4    | `signature_length` | reserved; zero in version 1         |

The magic is a byte array rather than an integer, so it is endianness-free and
visible in a hex dump. It is built like PNG's rather than as a bare `"DSB1"`:
the high bit in byte 0 catches a transport that strips to seven bits, and the
`\r\n` / `\n` pair catches one that translates line endings. It still reads as
`DSB` in a dump.

`header_size` and `section_stride` are recorded rather than assumed so the
table is self-describing: the external signing tool the container decision
names walks it and computes the signed range without hardcoding either number.
A reader of this version does not use them to skip a grown structure — under
the version rule above it never sees one — but a mismatch becomes a named error
instead of a misparse. `section_stride` is an addition to the field list the
decision's Refinements section gives; the decision records it.

`signature_offset` / `signature_length` are the reserved signature reference.
Signing itself — and its relation to the derivation manifest — is deferred with
the packer to v0.12; v0.11 writes zeros, and **refuses a file that does not**.

That refusal generalizes: `reserved_0`, `flags`, `signature_offset`, and
`signature_length` must all be zero in version 1, and a parse fails if any is
not. These four sit outside `root_hash`'s range, so nothing else in the file
would notice them being set. Without the check, a later writer could put
meaning in them and a version-1 reader would silently ignore it.

## Section table — `section_count` entries of 64 bytes, directly after the header

| offset | size | field        | notes                                     |
| ------ | ---- | ------------ | ----------------------------------------- |
| 0      | 2    | `kind`       | `1` structured, `2` blob                  |
| 2      | 2    | `flavor`     | role within the kind                      |
| 4      | 4    | `reserved_0` | zero                                      |
| 8      | 8    | `offset`     | payload offset from the start of the file |
| 16     | 8    | `length`     | payload length                            |
| 24     | 32   | `hash`       | BLAKE3 over the payload                   |
| 56     | 8    | `reserved_1` | zero                                      |

The fixed stride gives O(1) indexed access, and the signed byte range is
`header_size + section_count * section_stride` by construction.

Flavors defined in version 1: for a structured section, `1` is the ui document;
for a blob, `1` is an asset payload.

An asset payload is found by **content hash**, not by section index: the ui
document's `AssetEntry` carries a BLAKE3-256 hash, and the null binding resolves
it to the blob section whose recorded content hash equals it
(`Container::blob_by_hash`, and either reader for the whole file at once:
`dashbuf::open` resolves every entry to where its payload lies and reads none of
them, `dashbuf::open_verified` resolves and hashes each one).
That is why the section table can be reordered or re-assembled without touching
the ui section — the document names payloads, never places. Flavor is an **enumerated role**, compared
for equality — a section has exactly one role, and two roles would need two
entries, not two bits. The decision's Refinements section calls the field
"flavor flags"; this narrows it, and the decision records that.

### The table describes an ordered partition of the file

Section byte ranges are ascending and non-overlapping, and no range reaches
into the header or the table. That is what makes "the hot region" a single
contiguous byte range rather than a set of scattered ones.

Every structured section precedes every blob section, so the hot region is a
contiguous **prefix**.

Both rules are enforced at parse, not only by the writer. The distinction
matters: this component exists to validate a file before any parser is trusted,
so it cannot take the writer's word for the one property everything downstream
derives a byte range from. A file that put a structured section behind the cold
boundary would make a hot-region verification fault exactly the page the format
exists to avoid faulting.

A section's payload is never empty. A structured section with no bytes is not a
flatbuffer and a blob with no bytes is not an asset, and an empty section would
still claim its alignment — for the first blob, a whole page of padding for a
payload that does not exist.

## Alignment

Page alignment is required in exactly one place: the boundary between the last
hot byte and the first cold byte. Everything else is writer policy — the format
records only offsets, so a better packing heuristic needs no version bump.

The v0.11 writer's policy:

- every section starts on a 64-byte quantum, so a pointer into the mapping
  satisfies any consumer's natural alignment;
- the first blob starts on a 4096-byte page, whatever its size — that is the
  hot/cold boundary;
- a blob of 64 KiB or more starts on a page of its own, so it can be prefetched
  and evicted with a single `madvise` range; smaller blobs pack densely, which
  is harmless because verification and readiness are per blob and a shared page
  faulting early is free prefetch;
- **a file with no blob sections has no boundary and gets no padding for one.**
  A document without assets must not grow by up to a page of zeros;
- every alignment gap is zero-filled, and the file ends at the last payload
  byte.

Two things a reader deliberately does **not** check. It does not require a
section to be aligned, because alignment is writer policy and a reader that
enforced it would freeze a heuristic the format leaves open. And it accepts
bytes after the last payload, because that is where the reserved signature will
live — rejecting them now would foreclose the field the header already carries.

## Assembly — filling a container

`crates/dashbuf/src/bank.rs` (`covers` v0.12 story #433). The container writer
places bytes; assembly decides which bytes to place.

An asset has one canonical payload and one canonical hash. A **quality
profile** binds that hash to the bytes a file actually carries
(`docs/decisions/asset-quality-profile-naming.md`). A **cold bank** is one
profile's side of that binding for one document: the payloads, each keyed by
the canonical hash it stands for. `bank::assemble` takes a ui section and a
cold bank and produces the file — one structured section, then one blob
section per asset entry, in entry order.

**RAW is the null binding** — `ColdBank::raw` binds each payload to its own
hash. The resident payload is the canonical payload, so a RAW assembly has
nothing to derive and therefore nothing to move. That is what makes it
checkable in a form no other profile allows: `goldens/tooling/tests/cold_bank_assembly.rs`
takes each committed golden apart and reassembles it under a RAW bank, and
requires the result to equal the file it came from byte for byte. A failure
there is an assembly bug, never a golden to regenerate.

### Assembly reads the document it is about to write

`assemble` takes the ui section as bytes and reads the asset entries out of it,
rather than accepting a payload list paired positionally by the caller. Two
consequences follow, and both are the point:

- **The ui section is an input to assembly and never an output.** Nothing in
  the assembly path writes into a hot section, so a hot section cannot vary
  with the bank. The alternative form makes the same guarantee only for as long
  as a caller keeps the two lists in the same order.
- **Resolution is by hash, not by index.** An `AssetEntry` names a content hash
  and never a section index
  (`docs/decisions/asset-model-content-addressed-blobs.md`), and assembly is the
  writer-side use of that. `dashbuf::open` is the exact inverse: it resolves the
  same entry hashes back to the same blob sections.

Assembly refuses a bank that binds no payload to a hash an entry names — the
file would otherwise fail at load with `NoBlobForHash` — and a bank holding
payloads no entry names, which would become cold bytes nothing in the file
could reach.

Because `bank` parses the ui section, it is a separate module from `container`,
which exists to validate an untrusted file before any parser is trusted and so
cannot depend on one. Assembly is a writer running on bytes its caller just
produced, so the constraint does not apply to it.

### Hot sections do not vary with the bank

The invariant `docs/decisions/asset-model-content-addressed-blobs.md` recorded
as intent, now measured (`crates/dashbuf/tests/bank.rs`). Assemble one document
under two banks and:

- every structured section's payload bytes are identical;
- the asset count fixes the section count, so the section table is the same
  length and the ui section does not even change offset;
- every byte that does differ lies in the envelope — the header's `root_hash`
  and the section table — or in the cold payload bytes.

The header's `root_hash` is part of what differs, because it covers the table
and the table records where the cold bytes are. "Confined to the section table
and the cold bytes" in the design capture means the envelope as a whole.

v0.11 shipped one assembly, so this could only be stated. `ColdBank::derived`
is what makes a second one constructible, and a second one is what turns the
statement into a measurement. The derived side is only half-built until the
packer lands: a payload bound to a hash that is not its own preimage assembles
correctly but cannot yet be resolved by a reader, because resolving it needs
the derivation manifest (story #434).

## Content hashes

BLAKE3, 32 bytes, over the payload exactly as stored. The header's `root_hash`
covers the section-table bytes, so the table's own integrity is established
before anything is read out of it.

The choice is recorded in
`docs/decisions/asset-model-content-addressed-blobs.md`, which left the
algorithm open when the model was accepted.

## Loading model

One `mmap` of the whole file, once. The envelope is read through the mapping —
page 0 faults, and it is the hottest data in the file. Then: validate magic,
version, and bounds; hash the section table against the root hash; hash and
verify the structured sections; hand the ui section to the flatbuffers verifier
and then to the arena. Blob sections stay untouched until a loader thread
prefetches them, touching, hashing, and marking each ready.

There is no read-then-map two-step and no per-section mapping — sections are
offset ranges inside the one mapping.

`Container::parse` is built for exactly that: it borrows the buffer it is given,
copies no payload, and hashes no payload. `verify_section` and `verify_hot` are
separate calls so that verifying the hot region cannot accidentally fault a cold
page. The prefetch choreography, the placeholder activation, and the startup
scaling benchmark are v1 (`docs/roadmap.md`, R5 / guardrail G-20); what exists
here is the shape they need.

## The wire role is unchanged

`docs/decisions/dsb-format-and-one-schema.md` keeps one schema for both the file
and the wire role. The envelope belongs to the file role only: the wire keeps
length-prefixed framing of the same per-section flatbuffers, with no header and
no section table (`docs/decisions/remoting-two-transports.md`). Nothing in v0.11
changes it.

## Testing

Four suites, for four different questions. The first two are about the
envelope, the last two about assembly.

`crates/dashbuf/tests/container.rs` is behavioural: layout asserted by offset
rather than by round trip, determinism, zero-filled padding, and one rejection
test per failure mode. It writes and reads inside one process, so it proves the
code is self-consistent.

`crates/dashbuf/tests/container_frozen.rs` is the R7 guard, and it is the one
that can fail when self-consistency is not enough. It decodes a committed
envelope — `tests/fixtures/v0_11_container.dsb` — at literal byte offsets,
against literal values transcribed from the tables above, importing no layout
constant from the crate. A change that moved a field, altered the magic, bumped
the version, or changed the alignment policy in both directions at once leaves
the behavioural suite green and fails here. It also pins the digest against the
two published BLAKE3 reference vectors, so the fixture's hashes are anchored to
the specification rather than to whatever the dependency produced.

This is the same split, and the same reasoning, as the schema's frozen fixture
(`docs/decisions/dsb-frozen-fixture-r7-guard.md`), which named this obligation
when it landed: the schema fixture stays the structured-section guard, and the
envelope gets its own. Regenerate with `UPDATE_CONTAINER_FIXTURE=1`, only on a
deliberate, reviewed version bump.

`crates/dashbuf/tests/bank.rs` covers assembly on hand-built documents. Its
documents carry **three** assets, not one, because the corpus's only
asset-bearing golden has exactly one image and every index in a one-asset
document is 0 — such a document cannot fail an ordering, resolution, or
wrong-index bug. It is also where the two-assembly invariant is measured, which
needs a second bank the corpus cannot supply.

`goldens/tooling/tests/cold_bank_assembly.rs` covers assembly on the committed
bytes: each golden taken apart and reassembled under a RAW bank, required to be
byte-identical. It is the narrower test — it takes the committed ui section as
given, so it says nothing about the compiler above it — and it is the one that
cannot be compensated for by a matching change elsewhere in the pipeline. The
two are complements: the hand-built suite has the coverage, the golden suite
has the real bytes.

## Trace

    Satisfies:          R5 (loading performance — the shape it needs),
                        R7 (byte-identical emission, extended to the envelope)
    Implements:         docs/decisions/dsb-sectioned-container.md
    Resolves:           the hash algorithm left open by
                        docs/decisions/asset-model-content-addressed-blobs.md
    Related decisions:  docs/decisions/dsb-format-and-one-schema.md,
                        docs/decisions/dsb-frozen-fixture-r7-guard.md,
                        docs/decisions/remoting-two-transports.md
    Enabled:            story #401 (the .dsb file became an envelope, done),
                        story #107 (asset payloads are blob sections, done),
                        story #433 (cold-bank assembly, RAW, done)
