# Asset pipeline: canonical store, quality profiles, and baked delivery

    status   WIP — design-discussion capture (2026-07-19, user + Fable);
             feeds future decision records when the relevant slice opens.
             Nothing here is implemented; v0 ships inline PNG bytes only.
    scope    the asset pipeline from import to painter: image fills, baked
             vectors (MSDF), distance-field atlases; the .dsb cold sections;
             the packer and quality profiles; per-target GPU codecs
    builds on docs/decisions/image-assets-cross-boundary-b.md,
             docs/decisions/asset-model-content-addressed-blobs.md,
             docs/decisions/dsb-sectioned-container.md,
             docs/technotes/rendering-and-painters.md (§1 quad model),
             docs/specification/03-target-hardware-rules.md

## The kernel

1. **Canonical store, derived banks.** Every asset has one canonical
   payload — the original imported bytes (PNG/JPEG as Figma served them;
   provenance + byte-reproducible corpus), or lossless KTX2 for generated
   assets (baked vector MSDFs, shadow bakes) where no original exists.
   Production encodings are **derivations** produced by a packer, keyed by
   hash, mapped to canonical in a derivation manifest. dashc stays
   deterministic and lossless; the packer owns every lossy step.

2. **Three quality profiles, defined as band-contracts, not formats.**
   - **RAW** — the truth. Not a shipping profile: the
     qualification baseline, oracle lane, dev preview. It is the _null
     binding_: a regular `.dsb` resolved against canonical payloads; no
     manifest, no extra machinery. v0's inline-bytes `.dsb` is exactly this.
   - **HiFi** — premium production target (SA8255-class). Tight per-class
     bands; typically ASTC 4x4.
   - **Lite** — entry production target (SA7255-class). Looser bands;
     typically ASTC 6x6/8x8. Defined now, built when a measured budget or
     OTA constraint demands it (ship one production profile first).
     A profile is a set of per-asset-class tolerance bands. The packer
     escalates per asset (cheap -> better -> lossless) until the band holds,
     so over-compression is structurally impossible and entry hardware never
     silently shows banding. Distance fields never enter a lossy path
     (validator rule); single-channel fields ride EAC-R11 (or BC4 on BC
     targets).

3. **Per-asset choice: kind rules -> measured bands -> recorded override.**
   Hard rules by AssetEntry kind first (fields: never lossy). Then the
   packer runs a per-asset oracle: encode a candidate, diff against
   canonical in the differing-pixel-within-band vocabulary (the E7/#332
   pattern), escalate until the profile's band holds. Designer overrides
   (annotator-plugin role, e.g. "never below UASTC/4x4") are honored and
   recorded. Every choice lands in the signed derivation manifest —
   auditable, and a re-pack that changes a classification is a manifest
   diff. Budgets close the loop: a profile must fit the target's
   memory/bandwidth budget at pack time (validator error, never a silent
   quality cut).

4. **Shipped form: one file, one mmap; profiles are cold-bank assemblies.**
   Per the sectioned-container decision, the shipped `.dsb` is a thin
   container: hot sections (tree/tables/asset entries) at the head, the
   chosen profile's payloads page-aligned in cold sections at the tail.
   "One mmap of the whole file, once"; untouched cold pages never fault.
   The content-addressed store is the _pipeline_ view (packer, dedup,
   distribution cache); assembly inlines the chosen bank. External blobs
   stay the exception for streamed/lazy content. Intended invariant (to
   confirm at design time): hot sections byte-identical across assemblies
   of one document — the signed intent — with per-assembly differences
   confined to the envelope's section table and the cold bytes. Multi-bank
   files (2-3 profiles in one file) are possible and load-time-neutral
   under mmap, but cost flash/OTA size; default is one bank per target,
   multi-bank reserved for dev/demo toggles. Needs a size analysis on a
   real corpus (packer report; the hero is the natural test corpus).

5. **Qualification: a factorized matrix of banded diffs, anchored at
   Skia+RAW.** Making the asset set a _bound input_ (not a lane
   property) factorizes qualification into axes measured one at a time:
   - Skia+RAW vs Figma GET /images — design-source truth (E7 and the
     #332 import oracle; exists today).
   - backend+RAW vs Skia+RAW — painter divergence, assets held
     constant. Production painters consume the RAW bank through the
     raw-upload path they already need for runtime-downloaded images.
   - backend+profile vs backend+RAW — asset/compression cost, painter
     held constant.
   - backend+profile vs Skia+RAW — end to end; if it exceeds what the
     two axes explain, that residual is an interaction effect (e.g. a
     sampling artifact only on block-compressed data) — a detector, not
     noise.
     Every band gets its own rationale, in the same differing-pixel
     vocabulary as the existing oracles.

## Targets and codec plan

Format support follows GPU architecture, not market segment or API;
capability bits are not evidence — the probe is.

- **Wave 1-2 (committed): Adreno-ASTC + PowerVR-ASTC.** SA8255 (HiFi
  default) / SA7255 (Lite default); Renesas R-Car (PowerVR/IMG cores).
  ASTC is a vendor-neutral bitstream, so both vendors can share
  byte-identical banks: one packer output for the whole launch fleet.
  Encoder: astcenc, version-pinned (the msdf-atlas-gen precedent). No
  Basis, no install/prefetch transcode — "no transcoder in the trusted
  load path" satisfied by having no transcoder at all.
- **Wave 3 (maybe): NVIDIA-BC7.** DRIVE Orin/Thor GPUs are
  desktop-architecture (Ampere/Blackwell): native BC1-7 only. Tegra-class
  drivers _report_ ETC2/ASTC for GLES conformance but decompress to raw at
  upload (silent 32 bpp — the exact trap the pack-time probe must catch:
  allocate the compressed texture, measure actual residency; 8 bpp proves
  native). Adding NVIDIA is a derivation, not a redesign: a BC codec-table
  row (BC7 HiFi / BC1 Lite / BC4 fields), banks re-derived from canonical
  under the same bands, one new qualification column.
- **No universal hardware codec exists** (ASTC = mobile IP universe;
  BC = desktop/NVIDIA; intersection empty). The shared standards live one
  level up: KTX2 as the container everywhere, and Basis UASTC as the
  _contingency_ distribution codec (one bitstream transcoding to both ASTC
  and BC7) if a genuinely mixed fleet must share one OTA image. The
  hardware rules currently prescribe the Basis path universally; this plan
  proposes recording the fixed-target native-codec variant as a refinement
  to 03-target-hardware-rules.md, with Basis kept for the unknown-GPU case.

Naming convention: **RAW capitalized is always the profile; lowercase
"raw" is always an encoding** (prefer "uncompressed" where it could
confuse). The retired names (Lossless, Access, Master, Eco) stay out of
the vocabulary.

## Reference-painter profile preview (Gfx QA pre-validation)

The Skia reference backend renders all three profiles, so quality loss is
validated on desk before any target bench exists. Premise: ASTC/BC decode
is bit-exact by specification — a software decode of a HiFi/Lite bank
reconstructs the same texels the target GPU samples. Mechanism: the
reference loader software-decodes block payloads to RGBA using the same
version-pinned astcenc that encoded them (one pinned tool, both
directions); the painter is unchanged (it draws RGBA; P2 holds). RAW stays
the null binding.

Workflow: per corpus scene, render the triptych (Skia+RAW, Skia+HiFi,
Skia+Lite) + diff heatmaps + banded numbers. Skia+profile vs Skia+RAW is
the purest asset-axis measurement (no backend in the loop) and catches
scene-level, in-context artifacts the per-asset pack bands cannot (banding
behind text, block patterns against strokes). A "profile preview oracle"
(third manifest in the E7/import-oracle pattern, scene-level band per
profile) makes it regression-checked, and `just render --profile <p>`
gives designers the same view per imported file. Desk cannot show: GPU
filtering behavior, driver-level effects (UBWC; the NVIDIA
emulation/residency trap — probe's job), sRGB conversion points — the
bench confirms exactly that short list, rather than discovering quality.

## Dependency plan (who decodes/encodes what, with which tools)

- **dashc: identification + header parse — never decode.** Producer
  symmetry (P5) means image support is a property of the format, and every
  producer — the Figma importer, dashlang, anything future — supplies
  tagged bytes through the same compile contract (the `images` map). The
  compile gate therefore owns the shared P4 validation: magic-byte
  identification (is this really the tagged format?) and header parsing
  (intrinsic width/height for the asset entry) — hand-rolled, a few
  hundred lines, the same complexity class as the container's section
  table. Today's isPng check lives only Deno-side, which validates the
  Figma path but not dashlang's — moving identification into dashc closes
  that asymmetry, and it is ONE implementation for all producers: dashlang
  reaches it through the native compile API, the Figma importer through
  dashc.wasm (the recorded importer split), the packer through the
  workspace; the TS isPng demotes to a courtesy pre-flight. Implementation
  choice (two-way door, decide at slice time): an own ~200-line module
  scoped to exactly the PNG/JPEG/GIF closure (zero third-party on the emit
  path; the P4 accept-list is ours, not a library's), or the `imagesize`
  crate version-pinned after audit (pure-Rust header sniffing, no decode).
  Decode (entropy coding, pixel reconstruction — the CVE-bearing part)
  never enters the compiler.
- **Packer encode: standalone workspace binary, no external CLIs** (user
  requirement 2026-07-19). astcenc vendored and linked via a thin -sys
  crate (its C API is ~5 entry points; version-pinned by the vendored
  commit — stricter than a binary on PATH); the same linked astcenc
  provides the reference decode for the weld test, fully in-process. KTX2
  written by an own writer (narrow slice: single-level 2D LDR + Zstd
  supercompression + DFD; round-trip-validated against the `ktx2` reader
  crate). Zstd via the `zstd` crate (vendored libzstd, cargo-built). BC7
  in wave 3 via vendored bc7enc-family source, same pattern. Caveat on
  record: no production-grade pure-Rust ASTC encoder exists — standalone
  means vendored-and-linked, not rewritten. This deliberately diverges
  from the msdf-atlas-gen external-binary precedent: the packer runs on
  every pack and its outputs must reproduce on every build machine;
  in-tree vendored source serves that better (atlas-gen may stay as-is —
  different cadence — or migrate later via its decision record).
  Reproducible banks require pinned encoder versions — and version-locked
  _decoders_ too (JPEG decode output varies across implementations;
  Cargo.lock is the mechanism on the crate side).
- **Packer decode (canonical -> RGBA for encoding + the band oracle; GIF
  frames for the animation bake): the image-rs family** — `png`,
  `zune-jpeg`, `gif`, `image-webp` (or the umbrella `image` crate with
  exactly those features). Pure Rust; the pattern the downloaded-raster
  decision already records.
- **dashscene-skia (reference): Skia's own codecs** for PNG/JPEG/static
  GIF (in the default skia-safe build; WebP one feature flag away);
  `texture2ddecoder` + `ktx2` + `ruzstd` for ASTC/BC profile preview,
  behind a `profile-preview` cargo feature so a trimmed/shipping Skia
  build excludes them. Boundary B holds: bank bytes cross unchanged; the
  decoder is this painter's own machinery. The goldens tooling inherits
  the capability from the painter — one decode implementation, welded to
  the pinned encoder by a byte-equality test (decode corpus banks with
  both texture2ddecoder and `astcenc -d`; assert identical, covering the
  sRGB block modes actually emitted).
- **Deno importer: format identification only** (magic-byte checks
  isPng/isJpeg/isGif); never decodes.
- **Product painters (lean, Unity): nothing, ever.** Blocks upload; zero
  image parsers in the trusted load path.
- WebP enters only with the runtime-download story (Figma's REST can
  never serve it; the ingest closure is PNG/JPEG/GIF).

## Packer home: an in-workspace crate, not a new repo

The packer lives in this workspace as its own bin+lib crate (working name
**dashpack** — to be registered and added to
docs/decisions/crate-name-map.md when the slice opens; not among the 12
squatted names). Rationale: the recorded bar for a separate repo is
toolchain incompatibility (the Unity decision) — the packer is plain
cargo; its coupling is deep (compiles against dashbuf's ImageFormat /
AssetTable / manifest schemas; its band oracle reuses goldens::oracle;
the weld and profile-preview tests span packer output and the reference
painter); and the workspace already absorbs heavier builds (skia-bindings)
than a vendored astcenc -sys crate. The standalone-tool requirement is
met by the binary artifact (`cargo build -p dashpack`), not by repo
ownership. Extraction bar, recorded now: revisit only if an external
consumer needs the source tree, not just the binary; publishing as its
own crate happens at staging->public promotion regardless.

## Vectors: path -> MSDF, with Skia as the bake's oracle

The vector bake is **path geometry -> multi-channel distance field**, never
Skia-render-to-bitmap (a fixed-resolution raster loses crisp-at-scale; the
to-texture path stays the runtime-content escape hatch per the ThorVG
decision). Figma's REST supplies fillGeometry/strokeGeometry as
pre-expanded filled outlines, so stroke expansion is already done; the
field supplies coverage, the paint entry supplies color/gradient (one
shader multiply). Skia's role is verification, not baking: the reference
painter draws the original path directly (truth) vs the MSDF quad (what
products show) — a per-asset bake-fidelity oracle in the band vocabulary,
with escalation on the field's px-per-em / distance range, per-profile
resolution as a legitimate HiFi/Lite knob, and a named
diagnostic + escape hatch (ThorVG texture) for shapes too detailed to
field — never a silently degraded icon.

Tooling: dashc builds to wasm32 (the Deno importer), so vendored C++
msdfgen cannot live in the compiler. Generation uses a pure-Rust msdfgen
port (`fdsm` candidate) — wasm-clean, standalone — welded to vendored
msdfgen in the packer by a field-equality test (the
texture2ddecoder<->astcenc pattern). Near-term (#340): bake in dashc, the
.dsb embeds the atlas (the technote's recorded "(dashc)" placement, the
glyph precedent). End-state fork, recorded open: the path as canonical
intent with the field as a packer-derived generated asset (per-profile
px-per-em, banked like every other derivation) — P1-purist, not
foreclosed by the near-term shape.

## Animated content: bake the frames, let dashcue own the clock

Two classes, deliberately opposite treatments:

- **Content animation (animated GIF fills; bakeable Lottie):**
  frame-by-frame bake at pack time, exactly the recorded Lottie strategy
  (docs/decisions/lottie-bake-when-possible.md; ThorVG as the offline
  frame renderer for vector sources). An animated GIF is the easy
  sibling — its frames are already raster: the packer decodes them
  (needs the GIF row of the image-format work, #342), dedups identical
  frames by hash, and bakes a sprite sheet in the profile's codec. The
  document carries the description: frame count, per-frame durations
  (GIF delays map to keyframe times), loop mode. **dashcue drives it**:
  a keyframe track stepping the paint's frame index / UV window on the
  runtime's clock — P3 exactly (nothing producer-side runs in the frame
  loop; the runtime owns time), and the painter stays trivial (it draws
  a quad with a UV window; it never knows about time).
- **Authored interaction animation (Figma smart animate, prototype
  transitions):** never baked to frames — that would flatten intent to
  results (P1). It lowers to what dashcue already is: variant
  transitions, springs, FLIP over the semantic tree.

Costs and open points: an animated fill is N frames of texture budget —
the pack-time budget check applies, frame dedup helps loops; the
temporal axis may want its own profile knob (e.g. Lite decimates to
15 fps) — but frame decimation is an _approximation_, so it needs an
explicit decision against the skip-never-approximate line (a disclosed,
banded temporal degrade vs refusal); the dashcue track kind targeting a
paint's frame index / UV window is a small schema addition for that
slice; video fills stay out of the bake model entirely (unbounded
length) — they are runtime content (escape-hatch class), refused by
name until that story lands.

## Why baking wins (the compounding argument)

Vectors baked to MSDF delete the need for a runtime vector engine
(per-frame tessellation/coverage, mid-frame RT flushes on tiling GPUs,
path-complexity-dependent frame cost) — replaced by one sample + ALU,
resolution-independent, constant-cost. Images baked to GPU-native delete
decode from the boot-critical load path (order 5-20 ms/megapixel on
embedded cores), cut residency and sampled bandwidth 4-8x on a shared bus,
and remove decoders (CVE surface) from the trusted path. Steady-state FPS
is the honest shrug: a non-texture-bound UI does not render visibly faster.
The price: disk/OTA bytes, packer machinery, and escape hatches for
genuinely runtime content (ThorVG-to-texture; raw upload). Both bakes are
the same move — shape/format intelligence at build time, runtime consumes
only what fixed-function GPU hardware natively understands — and each bake
kept out of the painter is what keeps the painter small enough to qualify.

## Open points (deliberately unresolved)

- **AssetEntry hash semantics**: canonical identity vs resident-payload
  hash — decides whether hot sections are assembly-invariant; couples to
  signing and the derivation manifest. (The asset-model decision says "the
  content hash of the payload"; the tier-neutral reading needs it to be
  canonical + a signed derivation manifest.)
- **Derivation-manifest signing** and its relation to the signed root.
- **Unity ingest wrapping**: Unity consumes the packed set (runtime KTX2
  loader plugin vs build-time wrap into Unity textures) — invariant either
  way: input is the packed bank keyed to canonical, never a separate
  export. Belongs to the deferred dashscene-unity design.
- **Band values per class per profile**: pinned empirically by the packer
  oracle + review, never invented (the E7 band discipline).
- **Bank size analysis**: HiFi/Lite/multi-bank growth on a real corpus.
- **Lite profile activation**: ship HiFi first; turn Lite on when a
  measured budget/OTA constraint demands.
- **Per-core verification at slice time**: PowerVR ASTC LDR on the actual
  R-Car parts; Adreno UBWC interaction (expected non-issue for sampled
  ASTC); NVIDIA residency probe; LPDDR budget numbers per program config.
- **MSDF atlas shipping form on lean targets** (multi-channel field: raw
  in KTX2 vs EAC per-channel split) — the fields-never-lossy rule
  constrains but does not fully decide it.
- **fadvise policy** for multi-bank files (fault only the bound bank).
- **AssetEntry placeholder color**: computing it needs pixel access, which
  dashc cannot have (identification-only line). Candidates:
  producer-supplied (trivial for dashlang; importer-derived or default for
  Figma), a neutral default, or packer back-fill (awkward: hot-section
  data should not change after compile). Decide at the asset-model slice.
