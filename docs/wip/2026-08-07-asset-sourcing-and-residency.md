# Asset sourcing and residency — side-loading, decoders, and what already exists

    status   WIP — design-discussion capture (2026-08-07, user + Opus).
             **Nothing here is implemented.** Its largest finding is that
             more of it exists than the question assumed: texture upload,
             residency, packing and eviction are built, and the gap is
             narrower and better-named than expected.

             Independent of the two animation captures beside it.
             Gardened when side-loading is built.
    scope    how an image that is not in the `.dsb` reaches the screen —
             the format fields that must exist first, which decoders to
             add and where, and the abstraction that should not be built
             yet
    builds on docs/technotes/runtime-content.md §2 (downloaded raster),
             §7 (the placeholder contract),
             docs/decisions/downloaded-raster-needs-no-vector-engine.md,
             docs/technotes/hifi-bank-size.md,
             docs/decisions/compress-raster-only.md,
             docs/decisions/native-astc-codec-table.md

## What already exists, so it is not rebuilt

Checked 2026-08-07 in `crates/dashscene-gpu/src/residency.rs`:

    enum Texels<'a> {
        Baked { format: AtlasFormat, bytes: &'a [u8] },  // borrowed, not copied
        Decoded { rgba: Vec<u8> },
    }

Both arms exist, alongside shelf packing (`etagere`), LRU eviction and a
decode counter. The comment on that enum is worth carrying forward: a baked
payload's bytes are **borrowed rather than copied**, and that borrow is what
`docs/specification/03-target-hardware-rules.md` means by "no transcode step
of any kind".

**PNG decodes today.** `png` is a real dependency of `dashscene-gpu` and
`decode_png` is on the residency path.

So texture support and upload are not the work. Decoders are.

## Side-loading is a named deferral, and the schema names both fields

Today `ImageFill.image` is a `uint32` index into `Document.assets`. There is
no URL, no slot, no external reference. `ImageFormat` is
`Png = 0, Jpeg = 1, Gif = 2` — **no WebP** — and animated GIF is refused
upstream at the importer by name (`figma-image-animated-gif`), so only
static single-frame GIF is admitted.

The `AssetEntry` comment states the deferral precisely:

> Two fields the asset-model record names are deliberately absent until they
> have a producer and a consumer: a **placeholder colour** (computing one
> needs pixel access dashc cannot have, and its consumer — placeholder
> activation while a payload is not resident — is **v1**), and a
> **flavor/locator bit** (every payload here is **resident-raw**, and the
> section table already says where). Both are **appends**, which is the
> R7-cheap change.

The locator bit is the field that would say "this payload lives elsewhere".
It is absent because nothing yet points anywhere. **Everything else in this
file is downstream of it** — until an asset can legitimately be absent,
there is nothing for a source to resolve, nothing to place a placeholder
for, and nothing to load lazily.

The runtime half is already decided too — `runtime-content.md` §2 gives
**decode → upload → bind** with small pure-Rust decoders, explicitly _not_
by re-enabling Skia's codecs so the Skia trim stays intact. And §7's
placeholder contract requires a **declared-size** box, never hug, so
late-arriving content cannot reflow the scene.

**That last one is a validator rule, not a runtime one.** A lazily loaded
node that can hug its content reflows the scene when the payload lands, and
that breaks any FLIP in flight. It has to be refused at compile time by
`dashscene-validator` as a named diagnostic.

## Decoders — three formats, three different amounts of work

The painter declines JPEG and GIF _deliberately_, and says so in its
`Cargo.toml`:

> Boundary B's encoded half is PNG, JPEG and GIF; `Painter::samples`
> declares that this painter takes PNG and neither of the other two, so only
> this crate is needed and `libjpeg`/`libgif` stay out of a build whose
> whole point is that it is lean (issue #718).

So:

| format    | work                                                                                                   |
| --------- | ------------------------------------------------------------------------------------------------------ |
| PNG       | none — works end to end                                                                                |
| JPEG, GIF | a pure-Rust decoder, then widen what `Painter::samples` declares                                       |
| WebP      | a **schema append** to `ImageFormat` first, then importer and packer, then the decoder, then `samples` |

**`Painter::samples` is a per-painter capability, which is what makes this
comfortable.** Adding JPEG to the lean painter does not impose it on Skia or
on a future Unity painter, and declining a format stays a declared answer
rather than a failure — which is why `Painter::paint` can be infallible.

**Keep the decoders pure Rust.** `runtime-content.md` §2 names `png` and
`image-webp`/`image`; for the other two, `jpeg-decoder` or `zune-jpeg` and
`gif` are the same-family choices, named here rather than there. That preserves
both halves of the recorded reasoning at once: no C libraries in a build
whose point is leanness, and the wasm32 target continuing to need nothing
extra.

## Delegating to system codecs — exactly one place

**Build time: never.** Determinism is load-bearing — golden images with
measured perceptual bands, `atlas-repro`, R7, and `just calibrate`
re-deriving committed tables. A system codec makes decoded texels a function
of the OS and its version, and everything downstream moves with it: the ASTC
encode, the band measurement that chooses the rung, the golden. This
repository already treats a 4-px-in-65536 divergence between arm64 and
x86_64 as a finding worth recording.

**Runtime on web: yes, and it is the clearly better option.** `createImageBitmap()`
decodes off the main thread, hardware-accelerated, covering everything the
browser supports, and `copyExternalImageToTexture` lands it in a WebGPU
texture. It costs **zero payload bytes**, in the one place payload size
actually hurts — the measured web build is 4.39 MB raw and **1.37 MB
brotli** (2026-08-07, release profile with `lto`, `strip`,
`codegen-units = 1`).

**Runtime on native: bundle pure Rust.** Linux has no system codec layer
worth depending on and CI runs on Linux, so a fallback ships regardless —
paying the bundle cost _and_ getting divergence between platforms. Five
targets means five API shapes (WIC, Image I/O, `AImageDecoder`) plus an
abstraction over them, on a slice already oversized with Android and iOS at
zero.

## Runtime ASTC encoding — not by default

Considered: decode a downloaded PNG, then encode it to ASTC in the
background to reclaim VRAM. Rejected as a default:

- **The encode is seconds, not milliseconds.** astcenc is a search. The
  measured datapoint in `Cargo.toml` is 597 s at `opt-level = 0` against
  **7.5 s at release for one band sweep**. Backgrounding moves that spike
  off the critical path; it does not remove it.
- **The encoder is not in the runtime.** `dashpack` is a build-time crate
  nothing in the workspace depends on. Putting astcenc in every shipped
  binary, including the wasm one, is a cost every target pays for a case
  that is occasional by definition.
- **WebP lossy is already lossy** — VP8, DCT over YUV — so encoding it to
  ASTC is lossy-on-lossy, which is the ground `compress-raster-only.md` used
  to defer BC4/BC7 for fields.

**If it is ever built**, it is a cache decision rather than an encode
decision: keyed by content hash (the model is already content-addressed with
BLAKE3) and **persisted**, so the cost is once per image ever rather than
once per session; at a fast preset, since its job is VRAM relief rather than
archival quality; and opt-in per asset class so transient content does not
pay.

**Prefer moving it upstream.** Ship KTX2 with block data rather than PNG and
there is no runtime encode at all. Runtime ASTC encoding is the packer's job
done late and worse.

## The size question, answered by measurement rather than argument

`docs/technotes/hifi-bank-size.md` already has the
numbers, and they do not point one way:

- A 380×380 image goes to **12.3 %** of canonical under HiFi and **4.6 %**
  under LoFi.
- A 16×16 image goes to **2.677×** — 64 bytes of blocks, Zstd-stored in 33,
  under 216 bytes of KTX2 framing, against a 93-byte PNG carrying neither.
- Distance fields get **bigger** (1.02–1.15×), because they may never be
  lossily encoded.

**But file size is the wrong axis for the small case**, and the note says as
much when it calls the 2.677 ratio "a property of the format, not a defect".
VRAM is one-sided: a PNG decodes to RGBA at **4 bytes per pixel, always**,
while ASTC stays compressed in memory at 1.0 (4×4), 0.444 (6×6), 0.25 (8×8)
and 0.111 (12×12) bytes per pixel. For the 16×16 case, keeping the PNG saves
156 bytes on disk and costs 960 bytes of VRAM, plus a decode and 16× the
upload bandwidth.

**Per-asset "keep the canonical" is not expressible today**, and that is
deliberate: `pack()` returns `Binding::Canonical` only from
`Contract::NullBinding`, which is RAW. The variant's own comment says it is
stated as a variant "so a caller cannot ask RAW for a derived payload and
receive a re-encoded one". `Rung::Uncompressed` is the existing escape valve
for exactness — it reproduces the canonical texels exactly, which is how
distance fields pass through — but it does not preserve the _container_.

The narrow case that would justify a per-asset opt-out is an asset that is
**cold** — rarely resident, loaded on demand — where disk beats VRAM because
VRAM is not being spent. Which is the locator bit again.

## The packer cannot decode in production

`png` is a **dev-dependency** of `dashpack`, and `bank.rs` takes `Rgba8`
from the caller. The bank-size note says it plainly: _"the packer takes
decoded texels and the canonical-to-texels ingest is a later story; only PNG
is decoded in tests today."_ That is also why the JPEG and GIF fixtures are
missing from its measured table.

So "decode PNG then encode ASTC" is the intended pipeline and it is half
built: the encode half exists and is measured; the ingest half is a named
story.

## What does not need work: time and ticks

Recorded because it was asked and the answer is a non-finding.

Time is already abstracted, and the abstraction is that the caller passes
`dt`. `docs/design/dashcue.md` states it: _"The scheduler never reads a
clock; the runtime calls `advance(dt)` once per frame with its own step
(P3)."_ `LiveScene::tick(dt, arena)` is the same, and `dashcue` has no
dependencies at all, so it could not read a clock.

That discipline solved the platform problem before it was one:
`std::time::Instant` does not work on `wasm32-unknown-unknown`, so a runtime
crate calling it would fail at runtime rather than at compile time. Every
clock call is in the host, and the hosts are already separate crates.

The hard part is solved in two layers: `dashcue` **sub-steps**
(`substeps = ceil(dt / h_max).max(1)`), so the semi-implicit Euler spring is
stable at any `dt`; and the hosts **clamp**, because the substep count scales
with `dt` so an unbounded `dt` is an unbounded substep count. Correctness in
the library, cost in the host.

**One small drift.** Both hosts clamp independently, each with its own
constant, and the only thing connecting them is a comment in `demo-web`
pointing at `demo`. With v0.17 adding Android and iOS hosts that becomes
four copies of one policy kept consistent only by a comment, with nothing
enforcing it. A shared constant, or a
small `frame_step(prev, now) -> f32`, is the right amount of sharing.

## The abstraction to build later, not now

What is wanted eventually is a **source** abstraction, not a platform one.
The interesting variation is not Linux versus web; it is where bytes come
from (embedded in the `.dsb`, a derived bank, a URL, a host callback), when
they arrive (cold load, on demand, never), and who decides eviction. The
residency layer already owns the second half, so the seam belongs there as a
source it pulls from, with platform differences falling out as
implementations.

**One design decision matters more than the rest: poll, do not await.** A
source is async by nature, but `Painter::paint` is infallible and
synchronous, and `just wasm-painter` exists as a CI gate specifically to
keep a blocking wait off the web path where it would deadlock. Given P3, the
residency layer should ask each frame what arrived and swap it in. That keeps
async out of the painter and matches the `tick(dt)` model already in place.

**And it should not be built until there are two real implementations.**
Today every payload is `resident-raw`. One real implementation plus one
imagined one produces a shape that fits neither — which is the discipline the
schema already applies to itself when it holds two fields absent "until they
have a producer and a consumer".

## Additional findings, noted incidentally

- **No size budget is recorded anywhere.** Searched `docs/` on 2026-08-07.
  With v0.17 being the embedding slice across five targets, payload size is
  the first question an integrator asks, and there is currently no number to
  answer with and nothing to regress against. The 1.37 MB brotli figure above
  includes `dashc`, which enters through `showcase` rather than `demo-web`
  itself and which an embedding application would drop.
- **Issue #453 is closed, and this is recorded so it is not re-checked.**
  `compress-raster-only.md` states that it exists to build an EAC-R11 encoder
  and, under that decision, "has no consumer". The codec table's `Field (SDF)`
  column has since been corrected to "none — canonical" on both waves, and
  #453 was closed against the v0.12 milestone. Nothing is orphaned.
- **ETC2 was never a candidate**, which is worth stating because its absence
  reads like a rejection. The committed targets are Adreno and PowerVR, both
  of which support ASTC; ETC2 is the GLES 3.0 fallback for hardware that does
  not. Where ASTC genuinely is unavailable — NVIDIA desktop architecture —
  the table's wave 3 answer is BC7 and BC1, not ETC2.

## Open questions

- **Who computes the placeholder colour?** `dashc` cannot — it has no pixel
  access. The importer does have the pixels. Nothing assigns the job.
- **Does eviction stay recency-keyed once loading is lazy?** Today the LRU
  evicts and the bytes are still in the document, so re-upload is local.
  With a remote source, eviction means a possible re-fetch, and recency may
  stop being the right key.
- **Does the web painter declare a different `samples` set** than the native
  one, given the browser decodes formats no bundled decoder would?
