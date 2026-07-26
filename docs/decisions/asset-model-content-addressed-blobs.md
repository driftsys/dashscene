# Assets are content-addressed raw blobs referenced from a hot AssetTable

    status   accepted (design session, 2026-07-12); AS-BUILT 2026-07-26
             (v0.11, story #107) — the v0.3 inline `Document.images`
             storage is retired, `AssetTable` and resident blob sections
             ship, and the two open points below are resolved. Remote
             fetch still lands v1+.
    scope    crates/dashbuf schema, the .dsb blob sections, the future
             asset transport

## Context

`docs/design/architecture.md` places heavy decor payloads (images, baked
shadows, atlas textures) in cold sections, and the sectioned-container decision
(`dsb-sectioned-container.md`) gives them a physical home: blob
sections. Remoting (see `remoting-two-transports.md`) needs the same
payloads fetched lazily over a pull channel. The question was what an
asset _is_ in the schema: where its bytes live, where its metadata
lives, and what identifies it.

## Options

1. Asset bytes inline in the ui flatbuffer — the as-built v0.3 state:
   story #13 landed `Document.images: [Image]` with
   `Image.bytes: [ubyte]` embedded in the ui buffer, referenced by
   `ImageFill.image` (the schema comment already marks it "the
   simplest storage form").
2. A hot `AssetTable` of references — content-hash identity plus
   layout-relevant metadata — with payloads as raw blob sections in
   the file and content-addressed fetches on the wire.
3. As option 2, but each blob wrapped in a dashscene framing header
   (magic, kind, version) so blobs are self-describing in isolation.

## Choice

Option 2. The ui document carries asset _identity and metadata_, never
asset _bytes_ — P1 ("intent, never results") applied to assets:

- `AssetTable` is a hot, section-destined table, one offset from
  `Document`, per the container decision's rule 2. Consumers (image
  fills, baked shadows, later font atlases) reference entries by
  `u32` index, per rule 1.
- An `AssetEntry` carries: the **content hash** of the payload (the
  asset's identity), a **kind** enum, the **intrinsic metadata** the
  runtime needs before the payload exists (intrinsic size, pixel
  format, placeholder color), and a **flavor/locator** bit
  (resident-raw / resident-compressed / external, per
  `docs/design/architecture.md`).
- The payload is **exactly the well-known format's bytes** — a KTX2, a
  PNG, a raw compressed-texture slab — with no dashscene framing
  inside. Interpretation lives in the `AssetEntry`, which is hot,
  signed, and always available before a fetch is issued.
- The client-side asset cache is a content-addressed store: blob on
  disk named by its hash.

**The hash algorithm is BLAKE3-256** (resolved 2026-07-26, story #399,
when the container envelope was built and needed one). The candidate
named here became the choice for the reason given here: BLAKE3's tree
structure gives chunk-level verified streaming when the remote asset
channel lands (v2), which SHA-256 cannot. It also builds for
`wasm32-unknown-unknown` with no extra work, which `dashc` requires.
What would reverse it: a target program that mandates a FIPS-validated
digest. The same 32-byte hash will serve both roles in the file — a
section's content hash, which the envelope already writes, and an
asset's identity, when `AssetEntry` replaces the inline `Document.images`
storage — so there is one algorithm in the format, not two.

Left open deliberately: a one-to-many `AssetEntry → payload` extension
so texture mip tiers can be separate blobs fetched by priority.

## Why

- **File/wire byte identity.** The same bytes sit in a cold section
  and travel over the asset channel; one hash verifies both. There is
  no "file form" vs "wire form" of an asset.
- **Layout never blocks on payloads.** Intrinsic metadata in the hot
  entry means layout and first paint are fully computable with zero
  assets resident; a missing payload degrades to a defined placeholder
  paint (P4: defined behavior, not surprise).
- **The trust story unifies.** The signed root covers the hot
  sections; the hot sections contain the content hashes; so a lazily
  fetched blob is transitively authenticated by the same signature
  whether it arrived from a cold file section or the remote channel.
- **Content addressing makes the cache trivial and correct.**
  Deduplication across documents is automatic, re-requests are
  idempotent, and cached blobs are themselves mmap-able.
- **Tool transparency.** A blob section extracted byte-for-byte is a
  valid file of its format; existing pipelines can produce and inspect
  assets with no dashscene tooling. Option 3's framing header would
  break this and provide nothing the `AssetEntry` does not already
  provide; option 1 (the as-built v0.3 state) is the direction the
  container decision exists to move away from — bytes inside the ui
  buffer can never be evicted, page-aligned, or fetched lazily.

## As built (story #107, 2026-07-26)

`Document.assets: [AssetEntry]` ships; `ImageFill.image` and
`VectorAtlas.image` index it; `Document.images` is **deprecated, not
deleted**. Deleting a field shifts the vtable slot of every field
declared after it, which breaks every `.dsb` already written — R7's
whole subject. The slot stays reserved and unreadable, and `table Image`
survives only because a deprecated field still needs its type to exist.

One consequence worth recording: `crates/dashbuf/tests/fixtures/v0_5_document.dsb`
can no longer be rebuilt from its own writer, because that writer can no
longer address the pool it wrote. The fixture's job is to be old bytes,
so being unable to regenerate it is the stronger state, not a loss. Its
suite says so at the writer.

### What an entry carries, and what it does not

v0.11 writes the two fields that have both a producer and a consumer:
the content hash, and the intrinsic extent plus format that `dashc`'s
image gate read from the payload's own header
(`dashc-identifies-images-never-decodes.md`). Of the three fields this
record names as deliberately absent until they have both, **`kind` landed
at v0.12** and two remain absent.

- **`kind`** — **landed (story #432)**, as `AssetKind { Image = 0,
  DistanceField = 1 }`, with the rule that keys on it: a distance field has
  no lossy rung under any profile
  (`asset-quality-profile-bands.md`). The producer sets it, because a baked
  MSDF atlas is a PNG on the wire exactly as an image fill is and nothing
  downstream can tell them apart from the bytes. `Image` is value 0, so the
  append wrote no slot for any existing entry and no committed `.dsb` byte
  moved.
- **A placeholder colour** — computing one needs pixel access `dashc`
  cannot have and will not get; Figma's REST supplies none, so the
  importer cannot either; a neutral grey invented at compile time is a
  _result_ the document did not intend, which P1 forbids; and packer
  back-fill would mutate hot data after compile. Its consumer —
  placeholder activation while a payload is not resident — is v1, and in
  v0.11 every payload is resident before first paint. So the field lands
  with its consumer, producer-supplied.
- **The flavor/locator bit** — every payload here is resident-raw, and
  the section table already says where. The bit becomes representable
  when external payloads or compressed banks exist.

All three are appends, which is the R7-cheap change. Adding them now
would put fields in the format that no producer writes and no consumer
reads.

### Hash semantics — the open point, resolved

The hash is the **canonical payload's** identity: BLAKE3-256 over the
well-known format's own bytes, with no dashscene framing. It resolves to
bytes **through a binding**. v0.11 ships one profile, RAW, whose binding
is the identity map — find the blob section whose content hash equals the
entry's — so the resident payload _is_ the canonical payload and the
tier-neutral and payload-hash readings coincide. A later profile binds
the same canonical hash to a derived payload through the derivation
manifest, and only the binding changes.

Because an entry names a hash and never a section index, the ui section
does not depend on where a payload sits in the file. That is what makes
the "hot sections byte-identical across assemblies of one document"
invariant reachable.

**Measured 2026-07-26 (v0.12, story #433).** v0.11 shipped one assembly,
so the invariant could only be recorded as intent. Cold-bank assembly
made a second one constructible, and `crates/dashbuf/tests/bank.rs`
assembles one document under two banks: the structured sections come out
byte-identical, the asset count fixes the section count so the ui section
does not even change offset, and every byte that differs lies in the
envelope or in the cold payloads. Assembly reads the asset entries out of
the ui section it is about to write, which is what makes the invariant
structural rather than incidental — nothing in the assembly path can
write into a hot section. Byte layout and the refusals:
`docs/design/dsb-container-format.md`, "Assembly".

Half of the derived side is still story #434's. A payload bound to a hash
that is not its own preimage assembles correctly today, but a reader
cannot resolve it until the derivation manifest lands, so the second bank
in that measurement is a stand-in rather than packer output. The property
measured is where assembly puts bytes, which does not depend on where the
payloads came from.

### The recorded format and extent are cross-checked

An entry's recorded format and extent are checked against the payload the
entry names, by `dashscene_validator::validate_asset_payloads` — the load
gate's second half. It was deferred through v0.11 as debt #416, because
the check needs a header parser in a crate published before `dashc` and
because one code path wrote both halves from one `identify` call, so they
could not disagree. Story #437 closed it when the v0.12 packer became the
second writer: the parser moved to `dashpaint`, which every writer and the
validator reach
(`docs/decisions/image-header-parser-lives-in-dashpaint.md`).
