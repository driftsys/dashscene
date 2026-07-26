# dashc identifies and header-parses images; it never decodes, and it owns the accept-list

    status   accepted (story #400, 2026-07-26)
    scope    crates/dashc (the compile gate), importers/figma (its
             pre-flight), any future producer using the native compile API

## Context

`dashc`'s compile contract takes producer-tagged image bytes — the `images`
map, `BTreeMap<String, ImageAsset>`, where `ImageAsset` is `{ format, bytes }`.
Until this story the tag was asserted by the producer and verified by nobody:

- The Deno importer classified by magic bytes (`importers/figma/src/images.ts`)
  and tagged accordingly, so that one path was checked.
- Every other producer reaching the native compile API supplied the tag
  directly. `crates/dashc/src/abi/wire.rs` read the format number off the wire
  and built the `ImageAsset` with no check at all.

A mistagged, truncated, or non-image payload therefore travelled to a painter's
decoder before anything noticed. That breaks P4 — every out-of-profile construct
is a named diagnostic, never a silent drop — and it puts the first failure inside
a decoder, the one component the target-hardware rules want kept out of the
trusted path.

P5 says image support is a property of the format, not of one producer, so the
check belongs at the compile gate every producer already passes.

`docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` recorded the
implementation question as a two-way door, to decide when the slice opened:
an own module scoped to the format closure, or the `imagesize` crate pinned
after an audit.

## Options

1. An own module: magic-byte identification plus a header parse for exactly
   PNG, JPEG, and GIF.
2. The `imagesize` crate, version-pinned after an audit — pure-Rust header
   sniffing, no decode.
3. Leave identification producer-side and keep trusting the tag.

## Choice

**Option 1.** `image_id.rs` — a few hundred lines, the same
complexity class as the container's section table — does signature matching and
a header parse for PNG, JPEG, and GIF, and the compile gate raises four named
errors: an unknown signature, a signature contradicting the producer's tag, a
malformed header, and a header reporting a zero dimension. The Deno-side
`isPng`/`isJpeg`/`isGif` demote to a courtesy pre-flight: still useful for a
fast local message, no longer the only gate.

**Decode never enters the compiler.** Entropy coding and pixel reconstruction —
decompression bombs, out-of-bounds writes from a malformed Huffman table, LZW
dictionary overruns — are the part of an image codec that carries the CVEs. The
module reads chunk and segment headers, bounds-checked, and returns. This is a
permanent boundary, not a current limitation: a later want for a thumbnail or a
pixel checksum belongs in a painter, behind the trust boundary every other pixel
decode already sits behind.

## Why

- **The accept-list is ours, not a library's.** P4 turns on what the compiler
  agrees to represent. With a dependency, that list is whatever the crate
  happens to recognise this version, and it widens on a `cargo update` without
  anyone deciding to widen it. A JPEG variant the compiler cannot represent
  should be refused by name — `image_id.rs` refuses SOF1/3/5/6/7 and the
  arithmetic-coded SOF9/10/11/13/14/15 and JPEG-LS explicitly, rather than
  parsing them against a header shape they may not have.
- **Zero third-party code on the emit path.** The emitted bytes are
  byte-reproducible (R7) and the file will be signed. Keeping the emit path free
  of dependencies for image bytes keeps that surface as small as the format
  closure.
- **The closure is closed for v0.** Figma's REST API can serve an image fill as
  PNG, JPEG, or GIF and nothing else — WebP enters only with the
  runtime-download story. A module covering exactly three containers is not
  going to grow into a parser suite.
- **One implementation for every producer.** `dashlang` reaches it through the
  native compile API, the Figma importer through `dashc.wasm`, a future packer
  through the workspace. Moving identification into `dashc` is what closes the
  asymmetry rather than adding a second check beside the Deno one.

## Why not the others

- **Option 2** is a reasonable engineering choice and stays available; what
  decides against it is ownership of the accept-list, not code volume. Revisit
  if the format closure ever widens past what one module can hold — that is the
  reversal condition, and it is the design capture's own caveat.
- **Option 3** is the state this record ends. It leaves the native compile API
  ungated, which is precisely the asymmetry P5 forbids.

## Consequences

- Four `figma.image-*` diagnostics exist and are errors in both emit policies.
  An image that cannot be identified has no approximation to degrade to, so
  there is nothing for `EmitPolicy::Partial` to soften.
- A real Figma file with a mistagged asset now fails at compile with a message
  naming the `imageRef`, instead of at paint time inside a decoder.
- The intrinsic width and height the parse recovers have one consumer today, the
  zero-dimension diagnostic. Story #107 makes them an `AssetEntry`'s metadata.
  It does **not** add a load-gate rule that the recorded size agrees with the
  payload's own header: that check needs a header parser in a crate published
  before `dashc`, which is a crate-boundary decision rather than a rule to add,
  and debt #416 carries it. The two cannot disagree today, because one code path
  writes both from one `identify` call. There is one walk over image bytes in the
  compiler, and #107 calls this one rather than adding a second.
- **Superseded in part by story #437 (v0.12).** The module now lives at
  `crates/dashpaint/src/image_id.rs`, not in `dashc`, so that
  `dashscene-validator` and the packer reach the same implementation. The
  choice above — an own module rather than `imagesize`, and decode never
  entering the compiler — is unchanged and is now enforced by a test on
  `dashpaint`'s manifest, because `dashc` depends on `dashpaint` and a
  dependency there would be reachable from the compiler. Debt #416 is closed:
  the recorded format and extent _are_ cross-checked against the payload, by
  `dashscene_validator::validate_asset_payloads`. See
  `docs/decisions/image-header-parser-lives-in-dashpaint.md`.
- One path into `Document.images` is deliberately not gated: the MSDF vector
  atlas PNG the compiler generates itself. Its format is asserted by nobody, so
  there is no tag to verify — running the gate there would test our own encoder.
  The exemption is stated at the call site so it reads as a decision.
