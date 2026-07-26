# The image header parser lives in dashpaint, and dashpaint stays dependency-free to keep decode out

    status   accepted (story #437, 2026-07-26)
    scope    crates/dashpaint (the parser's home), crates/dashscene-validator
             (the load gate that needed reach), crates/dashc and crates/dashpack
             (the two writers), and any future crate that writes an AssetEntry

## Context

An `AssetEntry` records a payload's `format`, `width`, and `height`; the payload
itself lives in its own blob section of the `.dsb` file. That is two places
describing one asset, and nothing checked that they agree — debt #416, deferred
from story #107.

It was deferred for a reason that was not laziness. The check could not be
written where it belonged. `dashscene-validator` owns the load gate, and the
only image header parser in the workspace was `crates/dashc/src/image_id.rs`.
`dashscene-validator` publishes **before** `dashc` in the workspace publish
order, so it could not call it:

    dashbuf -> dashpaint -> dashscene-core -> dashscene-typeset -> dashcue ->
    dashscene-engine -> dashscene-validator -> dashscene-skia -> dashlang ->
    dashc -> dashpack -> dashscene-unity -> dashscene-web -> dashscene

The rule also had nothing to catch. `dashc` was the only writer, and it derived
the recorded metadata and the payload from a single `identify` call, so the two
could not disagree. #416 recorded both facts and waited for a second writer.

The packer (`dashpack`, epic #345) is that second writer. It re-derives payloads
and rewrites banks, so a recorded format or extent can finally disagree with the
bytes it describes. The rule now has something to catch, and where the parser
lives has to be settled to write it.

One constraint shapes the answer more than publish order does. The packer will
also want to **decode** — canonical bytes to RGBA for encoding, GIF frames for
the animation bake. Decode is a different trust boundary from header parsing:
entropy coding and pixel reconstruction are the CVE-bearing part of an image
codec, and `docs/decisions/dashc-identifies-images-never-decodes.md` keeps that
out of the compiler permanently. A crate shared between the validator, `dashc`,
and the packer must not become the seam through which decode reaches `dashc`.

## Options

Two questions, taken together because the answer to the second depends on the
first.

**Where the parser lives:**

1. `dashpaint`. Publishes second, so every writer and the validator reach it.
   It already owns `ImageFormat`, the type the parser's answer is phrased in.
2. A new crate for image identification. Maximum isolation of the decode seam.
3. `dashbuf`. Publishes first, so it reaches even further.
4. Leave it in `dashc` and give the validator its own copy.

**What the parser is** — the two-way door
`docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md` recorded and that
story #400 first walked through:

- **A.** The own module scoped to exactly the PNG/JPEG/GIF closure.
- **B.** The `imagesize` crate, version-pinned after audit — pure-Rust header
  sniffing, no decode.

## Choice

**Option 1 and option A.** `crates/dashc/src/image_id.rs` moves to
`crates/dashpaint/src/image_id.rs` unchanged apart from its import, and stays
the hand-rolled module story #400 chose. `dashscene-validator` gains
`validate_asset_payloads`, the load gate's second half, raising four named
diagnostics: `asset.payload-unreadable`, `asset.format-mismatch`,
`asset.extent-mismatch`, and `asset.payload-missing`.

**`dashpaint` carries no third-party dependencies, and a test enforces it.**
That sentence is the decision, not a note attached to it. `dashc` depends on
`dashpaint`, so anything reachable from `dashpaint` is reachable from the
compiler — which is exactly the seam the decode boundary has to survive. No
production-grade decoder is written without a dependency (`png`, `zune-jpeg`,
`gif`, `image`), so "this crate declares no dependencies" is a cheap, mechanical
proxy for "no decoder lives here". `manifest_carries_no_third_party_dependencies`
in `crates/dashpaint/src/image_id.rs` fails on the manifest line that would
introduce one, and names this record in its failure message. The packer's decode
belongs in the packer, which publishes after everything that would be harmed by
it.

## Why

- **It is the earliest crate all three callers share.** The validator, `dashc`,
  and `dashpack` must all reach one implementation — the alternative is the
  asymmetry P5 forbids, where a check exists on one producer's path and not
  another's. `dashpaint` is where that becomes possible.
- **Zero new dependency edges.** `dashscene-validator` already depends on
  `dashpaint`, and so does `dashc`. Moving the module changes no crate's
  dependency list at all; `dashpack` will add the one edge it needs. A move that
  adds no edges cannot introduce a cycle or change what `dashc.wasm` links.
- **The type is already there.** `ImageHeader` answers in `dashpaint::ImageFormat`
  and always did — the module imported it across a crate boundary. Putting the
  parser beside the enum it speaks in removes that import rather than adding one.
- **The invariant that protects the seam is checkable.** "Do not put a decoder
  here" as a comment is a request. As a failing test attached to the manifest
  line, it is a decision someone has to overturn deliberately, in a diff a
  reviewer sees. This is what makes option 1 safe enough to prefer over option 2.
- **The two-way door stays where #400 left it, and for a stronger reason now.**
  The accept-list is ours, not a library's: `image_id.rs` refuses SOF1/3/5/6/7,
  the arithmetic-coded SOF9/10/11/13/14/15, and JPEG-LS by name rather than
  parsing them against a header shape they may not have, and P4 turns on what
  the compiler agrees to represent. Taking `imagesize` now would _also_ breach
  the zero-dependency invariant that keeps decode out — the two halves of this
  decision hold each other up.

## Why not the others

- **Option 2, a new crate,** isolates the decode seam best, and it is what to do
  if the closure ever widens past what one module holds. It buys nothing today
  that the enforced zero-dependency invariant does not, and it costs a
  fifteenth crate name to register, a publish-order entry, and a crate whose
  entire content is one module that already has a natural home next to its own
  enum. Reversal condition: if `dashpaint` ever needs a real dependency for
  painting reasons, the invariant test stops being a proxy for "no decoder", and
  the parser should move to its own crate rather than the test being widened.
- **Option 3, `dashbuf`,** publishes even earlier, but it is the schema crate —
  it owns the wire format. `ImageFormat` as a _semantic_ type lives in
  `dashpaint`; `dashbuf`'s is the serialized mirror of it. Putting a
  format-identification module in the schema crate would make the schema depend
  on knowing what a PNG is, which inverts the layering. `dashbuf` also carries
  `flatbuffers`, so the dependency-free invariant would not be available there.
- **Option 4, a copy in the validator,** is two accept-lists that drift. The
  whole point of #400 was that one implementation serves every producer; two
  implementations of a P4 gate is the defect, not the fix.
- **Option B, `imagesize`,** stays available on its own merits and is a
  reasonable engineering choice. What decides against it is ownership of the
  accept-list — a dependency's list widens on a `cargo update` without anyone
  deciding to widen it — plus, now, that it would put a third-party crate inside
  the boundary this record just made load-bearing.

## Consequences

- `crates/dashc/src/image_id.rs` no longer exists; `dashpaint::image_id` is the
  one implementation. `dashc`'s Figma gate calls it across the crate boundary,
  and its four `figma.image-*` diagnostics are unchanged.
- The load gate has two entry points, `validate_document` and
  `validate_asset_payloads`, because an `AssetEntry` describes bytes the
  document does not contain. The second takes the payloads explicitly rather
  than being folded into the first and quietly doing nothing when they are
  absent — a check that silently does not run is worse than no check.
  `dashbuf::open` returns exactly the pair both halves need.
- **The gate runs on `dashc`'s emit path too, which closed a real hole.**
  `emit_and_validate` now cross-checks what it just emitted. Story #400 gated
  the Figma path, but the native `compile` API took an `Asset`'s `format`,
  `width`, `height`, and `bytes` from the producer and verified none of them, so
  `dashlang` and any native producer could record anything. They cannot now.
- The MSDF vector atlas is no longer exempt in practice. Story #400 exempted it
  from the _compile_ gate for a good reason — running `identify` on an image
  this compiler encodes tests our own encoder, not an input. But its
  `AssetEntry` records the extent the bake asked for while its payload is a PNG
  the compiler writes, and those are two independent derivations of one number.
  The load gate compares them, which is a genuine check rather than a tautology.
  It passes today.
- Three tiny fixture images (under 700 bytes total) are duplicated into
  `dashpaint`, `dashc`, and `dashscene-validator` test trees. A crate cannot
  reach into a sibling's `tests/` directory and still build from its own
  published tarball, so each owns its copy.
- A payload whose header is intact but whose compressed data is truncated passes
  this gate. Only a decoder can find that, and the decoder is in the painter, on
  the far side of the boundary this record keeps. That is the correct division,
  and `a_payload_truncated_after_its_header_passes_because_this_gate_never_decodes`
  pins it so it is not later mistaken for a gap.
