# Assets borrow from the mapping through an owner handle

    status   accepted (2026-08-05); **AS-BUILT 2026-08-06 (v0.16, story #596,
             PR #762)** — every decision below shipped as written, and the
             as-built section at the end records the two things the record did
             not anticipate.
    scope    `dashpaint`'s `ImageTable` and the asset half of
             `dashscene-core`'s load path. Boundary B's `Painter` trait is
             deliberately unchanged, and that is the point of the shape
             chosen.

## Context

R5 requires cold-start cost proportional to what is shown rather than to file
size. Today an asset payload is read once and copied twice before any painter
sees it, and twice more inside `dashscene-skia`. Measured on `36aba72`:

| where                                                     | what                                          |
| --------------------------------------------------------- | --------------------------------------------- |
| `dashbuf::open` → `blob_by_hash` → `verify_section`       | **reads** every byte of every blob (BLAKE3)   |
| `dashscene-core/src/load.rs:172` `payload.bytes.to_vec()` | **copies** into `ImageAsset`                  |
| `dashpaint/src/lib.rs:646` `blobs.extend_from_slice`      | **copies** into `ImageTable`'s pool           |
| `dashscene-skia/src/lib.rs:301` `images.clone()`          | **copies** the whole pool, inside the painter |
| `dashscene-skia/src/lib.rs:1763` `Data::new_copy`         | **copies** into Skia, inside the painter      |

Epic #594 names the second row only. The first is not a copy and is the
expensive one for cold start: hashing every blob faults every page of the file
in, which is exactly what mapping the file is supposed to avoid. It belongs to
story #597, which owns "touch + hash + mark ready", and is recorded on that
issue; the record for where verification happens is that story's to write, not
this one's. The last two rows are inside a painter rather than between the
mapping and one, so they fall outside epic #594's definition of done; D8 names
the one worth removing.

**Why the pool exists, and why that makes this change small.** `ImageTable`
stores every asset's bytes concatenated in one `Vec<u8>`, with each
`ImageEntry` a `{ format, offset, len, width, height }` range into it. That is
not an optimisation, it is story #600's rule: a row crossing boundary B must be
`#[repr(C)]`, fixed-width and free of owning members, so a C or C# painter can
read the table as a plain struct array. A `Vec<u8>` inside a row is a Rust-only
type. So the bytes had to leave the row for one shared region, exactly as clip
boxes, gradient stops and glyph quads did before it — `ImageEntry` was the last
row to be flattened, at story #640.

The pool is therefore already "one contiguous byte region, with rows naming
ranges into it". **A memory mapping is also one contiguous byte region.** The
change is which region, not what a row is.

**Story #596's premise that `Cow<'a, [u8]>` is ruled out by the compiler is
wrong, and correcting it does not change the answer.** The FFI gate is stated
over `ImageEntry`, the stored row. `crates/dashscene-unity/src/lib.rs` says so
directly: `ImageAsset` "stays as the owning producer type, which no
`extern "C"` signature names". Neither `ImageAsset` nor `ImageTable` appears
in any `extern "C"` signature, so a lifetime on either compiles. What rules the
borrow out is its blast radius, below, not a build failure — which means the
loud failure story #600 was built to provide does **not** cover this choice,
and this record is what holds it instead.

## Decision

**D1 — the table's pool is either owned or mapped, and never both.** One arm
holds the `Vec<u8>` it allocated; the other holds a reference-counted handle to
a region it does not own. A table built by a producer takes the first; a table
built by the loader from a mapped file takes the second. **A mixed table is
refused rather than supported**: per-row base selection would widen the row,
which is the one thing this design does not spend, and nothing in v0 needs a
runtime-supplied image beside a mapped one. That is a named limitation, not a
silent one (P4).

**D2 — `ImageEntry` does not change shape.** It stays `#[repr(C)]`, twenty
bytes, `{ format, offset, len, width, height }`. `offset` is relative to the
pool in both arms; for a mapped pool the pool is the file, so the offset is a
file offset. The FFI gate is satisfied unchanged, and no painter reads the
table differently.

**D3 — the `Painter` trait does not change, and no boundary-B type gains a
lifetime.** `paint(&mut self, …, images: &ImageTable, …)` is what it is today.
This is the whole reason for the handle: a lifetime would propagate into every
painter, into the arena's `Arc<ImageTable>` and into the showcase host, and a C
header has no way to express one — which would quietly close the non-Rust
backend path G2 requires, the same failure story #600 exists to make loud.

**D4 — the handle is `Send + Sync`.** Not decorative: the arena holds
`Arc<ImageTable>`, so the table crosses threads by construction. (Story #597
did not add the loader thread this sentence anticipated — see its own record's
D6 — but `dashbuf::residency::Residency` is `Send + Sync` for the same reason,
and a thread is the next step rather than a different design.)

**D5 — `ImageAsset` stays the owning producer type, unchanged.** A producer
that has bytes in hand and wants a table writes what it means. The mapped arm
is reached through a separate constructor that takes the handle and appends
rows by range, never through `push`.

**D6 — the loader builds a mapped table from ranges, not from slices.**
`dashbuf::prefix::Envelope::blob_by_hash` already returns a `Range<u64>`, which
is the shape this consumes; recovering an offset by subtracting one slice's
pointer from another's would work and is refused as the kind of arithmetic that
is correct until someone passes a slice from elsewhere.

**D7 — a mapped document is capped at 4 GiB, and the cap is stated rather than
discovered.** `ImageEntry.offset` is a `u32`. A file past that ceiling must be
refused with a named diagnostic at load, not truncated into a plausible offset.

**D8 — `ImageTable`'s `PartialEq` is out of scope for story #596.** The mapped
arm could compare handle identity plus ranges and skip the byte walk that
`dashscene-skia`'s frame cache pays every frame — 200 873 B for the `surfaces`
scene, and linear in the table's encoded size. That is a real improvement and a
different change: the owned arm must keep comparing bytes, because two
independently built tables holding equal bytes have to stay equal for that
cache to work at all, so `PartialEq` becomes two behaviours behind one trait
and needs its own test for the mixed comparison. Filed as debt #752 rather than
widened into the story.

## Consequences

- Two of the five rows above disappear: nothing copies a payload between the
  mapping and a painter, which is epic #594's definition of done.
- Every painter compiles unchanged. The change is visible only to whoever
  constructs a table.
- A payload that is a pointer into the mapping can go straight to the GPU —
  `wgpu::Queue::write_texture` takes a `&[u8]` — with no copy and no
  intermediate allocation. `ImageFormat` has carried the baked variants that
  make this reachable since story #640, so nothing at the seam blocks it.
- `dashscene-skia` still copies twice internally. Those are painter-internal
  and outside this record's scope; D8 names the one worth removing.
- The web path is unaffected in shape. wasm has no mapping, so `demo-web`
  continues to hold fetched bytes and takes the owned arm — the same table
  type, the same rows, a different pool.

## Alternatives considered

**`ImageTable<'a>` borrowing the region directly.** The least machinery and
zero indirection, and it compiles — the FFI gate does not stop it. Refused for
reach: the lifetime lands on `Painter::paint`, which every painter implements,
on the arena's `Arc<ImageTable>`, on `LiveScene` and on both hosts, and it is
inexpressible in a C header. It buys nothing over the handle, which costs one
pointer chase per resolve and no signature anywhere.

**`Cow<'a, [u8]>` on `ImageAsset`.** Story #596's first option. Same lifetime
reach as above, reached through the producer type instead of the table, and it
leaves the table's own pool copy in place — so it removes one of the two copies
and pays the full blast radius for it.

**Keep the owning form for `dashscene-skia` and expose a borrowed view only
where it matters.** Smallest immediate diff. Refused because it makes the seam
two shapes rather than one, which cuts directly against G2's claim that a
painter swap is a re-golden rather than a redesign — and the second painter now
exists to make that claim testable.

**Widen `ImageEntry` with a per-row base so a table can mix arms.** Refused
under D1: it widens the one row the FFI gate pins, to serve a case nothing in
v0 has.

## As built (story #596, 2026-08-06, PR #762)

Every decision above shipped as written. `dashpaint::Region` and
`Pool::{Owned, Mapped}` are the handle and the two-armed pool;
`dashscene_core::load_document_mapped` takes ranges; `ImageEntry` keeps its
twenty bytes; the `Painter` trait and every boundary-B type are untouched, which
is what D3 was for.

Two things the record did not anticipate.

**`PartialEq` had to change, and D8 does not cover it.** The derived
implementation compared **pools**, and a mapped pool is a whole file — so two
tables holding the same payloads at necessarily different offsets compared
unequal, and an owned table could never equal a mapped one. It now compares rows
plus payload bytes, with `offset` deliberately excluded. Removing the byte
comparison is killed by an existing `dashscene-skia` test, so the narrowing is
held by something other than this note. Debt #752 carries the frame cache that
still runs that comparison every frame.

**The mapped path turns loud failures silent, and needed two replacements.** It
reads no payload header by design, so the owning path's header parse — the
guard from issue (#640), which catches a KTX2 arriving where the entry says
`Png` — is simply gone. Two guards replace it: `demo` refuses a file that binds through a
derivation manifest, because a host with no quality profile cannot name the rung
it would be binding; and `ImageTable::push_mapped` asserts a baked row's length
equals `payload_len(w, h)`. The review found the second. The author's own test
had staged a PNG's byte range tagged `Astc4x4Unorm` and passed.

**What story #597 changed here afterwards.** Nothing in this record, and that is
worth saying: the residency work moved _when_ a payload is proven and left the
ownership shape alone. What it added is that the image table may now hold rows
whose payloads were never made resident — a many-frame document's other frames —
which is debt #779 rather than a change to D1.
