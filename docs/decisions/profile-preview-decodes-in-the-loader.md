# The profile preview decodes in the loader, not in the painter

Status: accepted, v0.12 story #435.

Traces: `docs/specification/02-principles.md` (P2, P4),
`docs/decisions/asset-quality-profile-bands.md` (what a profile promises),
`docs/decisions/derivation-manifest-section.md` (how a derived payload is
resolved), `docs/decisions/image-assets-cross-boundary-b.md` (each painter
decodes with its own machinery), `docs/decisions/boundary-b-unification.md`,
`docs/design/goldens.md` (the profile-preview oracle as built).

## The problem

RAW, HiFi and LoFi are band contracts, and the packer measures each asset's
texels against its band before choosing a rung. That gate is per asset and
blind to the asset **in context**: banding read behind a caption, a block
boundary read against a stroke. Both are what a designer actually looks at, and
neither is visible in a texel diff of one image.

Validating them needed a target bench, and there is none — the launch fleet is a
codec table on paper (`docs/decisions/native-astc-codec-table.md`). Waiting for
hardware would mean the first person to see what a profile costs is the person
who cannot change it any more.

The premise that removes the wait: **ASTC decode is bit-exact by
specification.** Every conformant decoder returns the same texels for the same
block, so a software decode of a derived bank reconstructs the texels the target
GPU samples, and a desk render of that bank shows the quality the target ships.

## The decision

The reference loader software-decodes derived block payloads to RGBA before any
byte reaches the painter. Four parts.

**One pinned tool, both directions.** The decode is `dashpack::astc::decode` —
the same vendored, version-pinned astcenc that encoded the payload. Not a second
implementation: two codecs would leave a difference no test could attribute,
which is the reason `dashpack::astc` linked the reference decoder in the first
place (story #430).

**The painter is unchanged.** `dashscene-skia` gains nothing, links nothing new,
and still only draws RGBA. The decode happens in the load path, between
`dashbuf::open` and `dashscene_core::load_document`, and the decoded texels are
re-wrapped losslessly as a PNG so the painter receives a container its own codec
already accepts. P2 holds without an exception being carved for it: a painter
still never measures, wraps, kerns or moves anything. Boundary B is untouched —
`dashpaint::ImageAsset` gains no variant, and no paint-table type changed.

**RAW stays the null binding.** Under RAW the resident payload _is_ the
canonical payload, the loader passes it through untouched, and the render is
byte-for-byte what it was before this existed. That is what makes RAW the
reference arm of the comparison rather than a fourth thing being compared.

**The decoder is feature-gated.** `dashpack/preview` is off by default, so the
packer binary carries no reader on its emit path; `goldens/profile-preview` is
on by default, so the whole workspace test suite exercises it. A build without
it refuses a block payload by name rather than drawing whatever a lenient
decoder made of it (P4).

## What the weld test welds

The claim above is carried by
`goldens/tooling/tests/profile_preview_weld.rs`, in three separately falsifiable
legs. What it does **not** prove is stated first, because the shared codec makes
one obvious reading of it wrong: both sides run the same astcenc, so no
assertion there can catch a defect in the codec itself.

What it does prove is everything between the encoder and the painter.

- **The blocks that ship are the blocks the encoder made.** The ASTC payload
  recovered from an assembled `.dsb` is byte-identical to `astc::encode`'s
  output, read back with the independently written `ktx2` crate. This crosses
  the KTX2 writer, the Zstd level, cold-bank assembly, the section table and
  page alignment, and `dashbuf::open`'s resolution of a canonical hash through
  the derivation manifest.
- **The preview recovers its parameters from the file.** The block footprint and
  colour space come from the file's `VkFormat`
  (`dashpack::ktx2::Format::from_vk_format`), never from the caller. A preview
  that was told them would agree with the encoder even when the file said
  something else, and would render correctly on a desk what renders wrongly on
  the target. Mutations that decode at the wrong footprint and in the wrong
  colour space assert the result differs, so recovery is held by a measurement.
- **The PNG re-wrap is lossless.** Skia's own decode of the wrap equals the
  texels the block decode produced, so the wrap cannot quietly premultiply,
  resample or drop alpha.

## What a desk preview cannot show

Stated here, in `dashpack::preview`, in `goldens::profile`, in the oracle
manifest and in `just render`'s own help, so that a target bench confirms a
short list rather than discovering quality:

- **GPU filtering behaviour.** The texels are exact. What a sampler does with
  them between texel centres — bilinear taps, anisotropic footprints, mip
  selection — is the hardware's and is not modelled.
- **Driver-level effects.** Vendor bandwidth compression layered on top of the
  stored blocks (UBWC on Adreno), and the NVIDIA case where ASTC is emulated
  rather than sampled natively, with the residency cost that implies. Those
  belong to the pack-time probe.
- **sRGB conversion points.** Where a target applies the transfer function — in
  the sampler, in the shader, at the framebuffer — is a pipeline property. The
  preview decodes under the colour space the file records and returns 8-bit
  texels; it does not model where a target converts them.

## Alternatives considered

**Teach the painter to decode blocks.** The design capture's first sketch, with
`texture2ddecoder` behind a `profile-preview` feature in `dashscene-skia`. It
puts a codec inside the painter, which every future painter would then have to
carry or deliberately omit, and it makes a painter's dependency list a function
of a packer's format choices. Decoding in the loader keeps the format choice on
the producer side of boundary B, where the rest of it already lives.

**A pure-Rust block decoder welded to astcenc.** The capture proposed
`texture2ddecoder` welded to `astcenc -d` by byte equality. It is a stronger
weld — a genuine second implementation — but the story specifies one pinned tool
in both directions, and the reference decoder was linked in story #430 precisely
so a second one would not be needed. A second decoder can be added later without
changing anything decided here: it would slot in beside `astc::decode` and the
existing legs would then also cover the codec.

**Add a `Ktx2` variant to `dashpaint::ImageFormat`.** Widens boundary B for a
quality-assurance path. Boundary B is the contract every painter implements, and
a variant only the reference desk build can handle would be a construct most
painters must refuse — the shape P4 exists to prevent.

**Commit the triptych as goldens.** A committed render of a scene that exists to
show codec loss would have to be re-baselined for every unrelated painter
change, and the re-baseline would carry no information. The durable record is
the measured numbers in `goldens/oracle/profile-manifest.json`, which the oracle
asserts exactly; the images are written to `target/profile-preview/` on every
run (`just triptych`).
