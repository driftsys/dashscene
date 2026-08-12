# Baked texel payloads cross boundary B as a flat format on a flattened table

    status   accepted (2026-08-03)
    scope    dashpaint (ImageFormat, ImageTable, Painter), dashscene-core's
             loader, dashc's emitter, every painter

## Context

`dashpack` shipped in full at v0.12 — vendored astcenc, a KTX2 writer, cold
banks, per-profile derivations. None of it could reach a painter. Boundary B's
`ImageFormat` was `{ Png, Jpeg, Gif }`, so the only thing a painter could be
handed was source-encoded bytes to decode at runtime.

Issue #640 recorded three things that follow from that, and the third is the one
that makes it urgent rather than untidy:

- PNG decoding is 20.4 % of every frame in the `surfaces` showcase scene.
- `docs/specification/03-target-hardware-rules.md` requires product assets ship
  "as native ASTC directly, with no Basis and **no transcode step of any
  kind**". Decoding PNG and uploading RGBA is a transcode step of exactly that
  kind, on the tier least able to afford it.
- The Skia trim profile removes libpng/libjpeg/libwebp entirely
  (`docs/technotes/rendering-and-painters.md` §6). On a trimmed product build a
  document image fill has **no decoder at all** — so this is not a slow path, it
  is a path the shipping configuration cannot execute.

It had to land before story #581, which is where image assets reach the GPU: a
texture path written against `{ Png, Jpeg, Gif }` is a texture path that has to
be rewritten.

## Decision

**D1 — `ImageFormat` grows baked variants, and stays one flat `#[repr(u32)]`
enum.** Not `Baked(TexelFormat)`.

**D2 — the colour space is part of the format**, as it is in KTX2 and Vulkan:
`Astc6x6Srgb` and `Astc6x6Unorm` are two variants.

**D3 — the variants are exactly the rungs `dashpack` can produce.** Six ASTC
block sizes in two colour spaces, plus `Rgba8` in two — no format the packer
cannot emit.

**D4 — `ImageTable` flattens: one byte pool plus `#[repr(C)]` rows.** The three
types are `ImageAsset` (producer, owns its bytes), `ImageEntry` (stored, fixed
width, **on `dashscene-unity`'s FFI gate**), `ImageRef` (reader, borrows).

**D5 — a payload binding may state its own format.** `BoundPayload` carries the
bytes and, for a derivation, what they are; `load_document` is
`load_document_bound` with every payload canonical.

**D6 — a painter _declares_ which formats it can use.** `Painter::samples`,
defaulting to the source-encoded half.

## Why

**D1.** The nested form reads better, and issue #640 proposed it. What decides
against it is that this value crosses the FFI gate: story #600 holds every
boundary-B row to `#[repr(C)]` with fixed-width members, and `ImageAsset` was
the last row that did not meet it. A nested enum needs a mapping in each
direction, which is a second place for the correspondence to be written and a
second place for it to drift. `dashscene-gpu`'s `InstanceKind` was two fields
for exactly this reason and their discriminants collided — a shadow was painted
from the solid-fill table. One flat discriminant makes that unrepresentable
rather than forbidden.

Seventeen variants is more than a nested form would show, and that is the honest
cost of the choice.

**D3, and what it rules out.** A format the packer cannot produce would be a
branch in every painter that nothing can reach — vocabulary discovered rather
than validated, which is what P4 exists to prevent. ETC2 and EAC-R11 are named
in the target-hardware rules and are deliberately **absent**: `dashpack`'s
image-fill ladder emits ASTC and uncompressed RGBA only
(`crates/dashpack/src/profile.rs`, `IMAGE_FILL_RUNGS`). They arrive as variants
on the day the packer emits them, which is the additive evolution
`image-assets-cross-boundary-b.md` reserved.

**D4, and why it did not cost what it looked like it would.** Flattening the
table sounded like it would touch every caller: 66 files mention these types and
42 construct an `ImageAsset`. It touched five files outside `dashpaint`, because
the producer keeps its shape — `push` still takes an owning `ImageAsset` and
copies into the pool, so nobody assembles an offset by hand — and only the read
sites moved, from `&ImageAsset` to a borrowed `ImageRef` with the same field
names. That is the same producer/table/reader split
`instance-buffer-contract.md` took, and it is why doing this _with_ #640 rather
than after it was the cheaper order.

`ImageEntry` joins `dashscene-unity`'s gate in this change, which is what makes
"meets story #600's rule" a fact rather than a claim: the crate names each gated
type in an `extern "C"` signature, so `improper_ctypes_definitions` fires on
anything unrepresentable, and pins its size and alignment beside it. Story
#600's own doc said `ImageAsset` "is what remains"; it now says what replaced
it. A record that had said this without widening the gate would have been
describing intent — the failure this repository has recorded more than once.

**D5, and the hole it closes.** The loader took the format from the document
entry and the bytes from the caller's binding, and nothing checked that the two
described the same thing. A host binding an ASTC payload today gets an asset
tagged `Png`: the tag and the bytes describe different things and no painter can
find out. That is not a hypothetical — it is what a host would have to do to use
`dashpack`'s output at all.

A document records the **canonical** payload's format and never carries a
derivation, because `dashpack` writes derivations beside the document and does
not rewrite it (`docs/decisions/asset-model-content-addressed-blobs.md`). So the
binding is the only place that can state a derived format, and `BoundPayload` is
where the bytes and their format are stated together. `dashc`'s emitter now
refuses a baked format by name rather than mapping it to something close,
because a baked format arriving there would mean a derivation had been written
back into the source of truth.

**D6, and why it is a declaration rather than a result.** `Painter::paint`
returns nothing, by decision
(`docs/decisions/painter-trait-infallible-slice-input.md`), so "this painter
cannot sample ASTC 6x6" cannot be reported from inside a frame. P4 forbids
discovering it there in any case. So the question is asked before a payload is
bound, by whoever selects the derivation.

The default claims the source-encoded half only, which is what every painter
written before this record does. It is safe in the direction that matters: a
painter that _could_ upload a baked payload but forgot to say so is handed an
encoded one and decodes it — slower, and correct. The reverse could not be made
safe, which is why the default is not "everything". `dashscene-skia` takes the
default and asserts by name if a baked payload reaches it anyway, since that
would mean the binding ignored the declaration rather than that its decoder is
incomplete.

## What this does not do

**Nothing uploads a baked payload yet.** `dashscene-gpu` has no texture path —
that is story #581, and it is where the lean painter's `samples` stops being the
default. This record makes the path representable and binds nothing to walking
it a particular way.

## Amended by issue #716: the row carries the extent

Story #581 found the representation one field short. A painter needs an image's
pixel extent before it draws anything — to size a texture, and because
`ScaleMode::Fill`, `Fit` and `Crop` are each stated over the image's intrinsic
size — and boundary B carried no extent at all.

The encoded half hid it. `dashscene-skia` recovers the extent by decoding, and
`dashpaint::image_id::identify` recovers it from a PNG, JPEG or GIF header with
no decode. Neither route exists for a baked payload: ASTC blocks and raw RGBA8
texels carry no header, and a payload length does not determine an extent —
`ceil(w/6) * ceil(h/6) * 16` maps many extents onto one length. So the format
this record added could be uploaded by nobody.

Three changes, and the shape of them follows D4's own split:

- **`ImageEntry` and `ImageRef` carry `width` and `height`.** The row grows from
  12 bytes to 20, stays `#[repr(C)]` with fixed-width members, and its pin on
  `dashscene-unity`'s gate moves with it.
- **`ImageTable::push` derives the extent for an encoded payload** by calling
  `identify`, and `push_baked` takes it from the caller. So `ImageAsset` keeps
  its shape and none of the 47 construction sites across 20 files changed — the
  same reason flattening the table cost five files and not forty-two.
- **`load_document_bound` passes the document's own extent** for a baked
  payload. The document records it on `AssetEntry` and the validator has already
  checked it against the canonical payload, and a derivation preserves it —
  `dashpack`'s rungs are block footprints, not mip levels — so `BoundPayload`
  gains nothing.

**What this closed that D5 could not.** D5 made the format and the bytes state
themselves together, and could say nothing about the extent, because there was
none to state. `push_baked` now refuses a payload that is not the length its
format and extent require, so a binding can no longer describe one image with
its extent and a different one with its bytes.

**What it deliberately did not do.** An encoded payload whose binding supplied
no bytes is stored at a zero extent rather than refused. `dashscene-validator`'s
`image.no-bytes` rule names that case, over a table it is handed already built,
and a panic in `push` would replace a named diagnostic with a crash — the
inversion of P4. A non-empty payload whose header does not parse _is_ refused
there, because nothing else owns it.

**No host selects a derivation yet.** `BoundPayload::derived` exists and is
exercised by a test; choosing _which_ rung to bind is the profile question
`dashpack` already answers, and wiring it into a host is #581's or later.

**The document is unchanged.** `dashbuf` gains no variant, by D5's argument.

## Verified where, and where not

Five new assertions at boundary B and two through a real compiled `.dsb`: a
derivation bound for one asset arrives with its own format and its own bytes
while the others stay canonical, and the canonical entry point loads exactly
what it loaded before.

The flattened table is checked over three assets of different lengths, with the
assertions stated over the second and third — a table that ignored the stored
offset would return the first asset's bytes for all three and pass any check
written over asset zero alone. Every format's discriminant is round-tripped, and
the seventeen are checked to be distinct, which the round trip alone would not
catch.

**Not verified: any baked payload actually rendering.** No painter uploads one,
so what is tested is that boundary B can carry it and that the loader can bind
it. The first end-to-end evidence is story #581's.
