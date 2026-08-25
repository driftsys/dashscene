# Codec policy is a property of the target, not of the painter

    status   WIP — design-discussion capture (2026-08-25, user + Opus).
             **Nothing here is implemented and nothing is filed as a
             story.** It answers one question asked in one session — why
             can the lean painter only decode PNG, and what should each
             target actually be handed.

             **Reviewed 2026-08-25 before merge, and the review changed
             two of its rulings**: the desktop ASTC decode moves from the
             painter to the loader (§1.3), and the photographic classifier
             is withdrawn (§2.4). Both were reversals of things this
             repository already records. What survived is marked
             **promote**; those are decision-shaped and should become
             records in `docs/decisions/` rather than being gardened away.

             **On its citations.** Each claim about current behaviour
             names the symbol or the file it was read from, so a reader
             can check it. Claims about the world outside this repository
             are marked *(recollection)* and are not evidence. Where this
             file quotes, the quotation is verbatim; where it summarises,
             it does not use quotation marks.
    scope    `crates/dashscene-gpu` (its decoder set and `Painter::samples`),
             `crates/dashscene-core` (the load path, where a derived
             payload is decoded), `crates/dashpack` (a third shipping
             profile), the artifact set a product ships, and how a loader
             selects among them
    builds on `docs/decisions/backend-tiering-unity-skia-lean.md`,
             `docs/decisions/wgpu-is-the-lean-painter.md`,
             `docs/decisions/native-astc-codec-table.md`,
             `docs/decisions/compress-raster-only.md`,
             `docs/decisions/derivation-manifest-section.md`,
             `docs/decisions/profile-preview-decodes-in-the-loader.md`,
             `docs/specification/03-target-hardware-rules.md`
    opens on  two named blockers have no issue: the ASTC residency probe
             (§4.1) and the absence of any WebP encoder in the workspace
             (§2.3). Every other dependency named here is filed: #1356,
             #1357, #1292, #462, #553.

## Part 0 — the question, and the defect that prompted it

`dashscene-gpu` links one image decoder, `png`, and refuses JPEG and GIF by name
(`ResidencyError::NoDecoder`, issue #718). The justification is the Skia trim
profile, quoted from `docs/technotes/rendering-and-painters.md` §6: "No codecs
(libpng/libjpeg/libwebp) — runtime uploads pre-transcoded KTX2/Basis→ASTC/EAC
textures; decoders live in the offline asset pipeline." Inside
`crates/dashscene-gpu` that justification is repeated at `src/residency.rs:424`,
`src/residency.rs:1499` and `src/lib.rs:180`; several records under `docs/` give
it too. No count is stated here, because a count in prose is drift surface —
`grep -rn 'libwebp'` is the derivation.

**That is an entry-tier justification, and the crate is now the default desktop
painter.** `crates/dashscene-desktop` defaults `App::presenter` to the lean
painter, and `dashscene-skia` is absent from its manifest so a `winit` embedder
need not resolve a vendored C++ Skia build (issue #794's ruling). An embedded
budget became the desktop default for packaging reasons.

**There is a live defect behind this.** `dashc` accepts JPEG, and Figma emits
JPEG — `identify_jpeg`'s comment says "baseline / progressive — the only two
Figma's re-encoder ever emits", and issue #342 landed JPEG fills end to end. So
a Figma import containing a JPEG fill produces a `.dsb` the default desktop
painter cannot draw. It is refused by name rather than crashing, which #718
fixed, but the document is one this project produces and this painter rejects.
**Filed as issue #1357.**

**promote:** "no codecs" is a property of the **entry embedded target**, not of
the lean painter. Issue #718's refusal is correct on embedded and wrong on
desktop — the same code, two verdicts, because the target differs.

## Part 1 — one painter, three renderer targets

| renderer     | PNG/JPEG/GIF/WebP        | ASTC                                 | RGBA8 baked       |
| ------------ | ------------------------ | ------------------------------------ | ----------------- |
| **desktop**  | all, statically linked   | native, or decoded **in the loader** | yes               |
| **web**      | all, through the browser | **no**, deliberately                 | yes               |
| **embedded** | **none**                 | yes                                  | **yes, required** |

### 1.1 Embedded needs RGBA8, not only ASTC

Distance fields take no lossy rung under any profile
(`docs/decisions/compress-raster-only.md`), and story #432 measured why: at 4x4,
the finest ASTC footprint, the committed MSDF atlases still put 8.6044 % and
8.8753 % of their texels beyond a delta of 8 (`crates/dashpack/src/profile.rs`,
`AssetClass::lossy_rungs`). So `AssetClass::DistanceField` yields
`Contract::LosslessOnly`, whose terminal is `Rung::Uncompressed`, which ships as
RGBA8.

**Every glyph atlas on every embedded build is therefore RGBA8. Without it, no
text draws at all.** That is the whole of this section's argument, and it stands
alone.

An earlier draft added a second support that is false, and the correction is
worth keeping: it said LoFi reaches the uncompressed rung for image fills.
`crates/dashpack/src/profile.rs`, under `Profile::LoFi` in `contract`, says the
opposite — "Its band already accepts a lossy rung on every committed asset, so
the terminal is unreached and costs nothing". **And the first correction of that
was also wrong**, which is worth recording as its own lesson: it said HiFi
reaches the uncompressed rung instead. The committed table in
`crates/dashpack/tests/band_contract.rs` records `detail-noise` under HiFi as
`astc-4x4`, and `rung: "uncompressed"` appears there only for the two
distance-field atlases. **Neither shipping profile reaches the uncompressed rung
for an image fill** — HiFi's image-fill contract ends at
`Terminal::FinestLossy`, as `profile.rs` says and issue #553 confirms.

The false correction came from a **stale doc comment in the tree**:
`band_contract.rs`'s module documentation still says `detail-noise` ships
uncompressed under HiFi, contradicting its own table ninety lines below. That
drift is pre-existing and is not this file's to fix, but it is what a reader
checking this claim will hit first.

This does not weaken "no codecs": `ImageFormat::is_encoded()` is false for
`Rgba8Srgb` and `Rgba8Unorm`, which are uploaded as texels rather than decoded.
An embedded build still links zero decoders.

### 1.2 Web is all-but-ASTC on purpose

Not because ASTC is unavailable there. WebGPU exposes `texture-compression-astc`
where the hardware has it, which includes Android browsers and Apple Silicon
_(recollection)_, and `SampledFormats::of` queries it identically on wasm.

Three reasons to decline it anyway:

- **Download is the scarce resource on web.** ASTC is already high-entropy and
  compresses poorly under Zstd; WebP is a purpose-built transfer codec.
- **One artifact, no branching.** `texture-compression-astc` availability varies
  by browser, OS and GPU.
- **No exposure to the emulation trap**, which on web cannot be probed at all —
  WebGPU exposes no way to query a texture's real memory footprint
  _(recollection)_.

Cost: an Android browser gives up ASTC's bits per texel for RGBA8's 32.
Accepted, because the product target is the head unit rather than a phone
browser.

**The mechanism is undecided and it is the hard part of this row.** Every
browser image-decode API is promise-based, and `Residency::resident`
(`crates/dashscene-gpu/src/residency.rs`) is a synchronous function called from
the frame, returning `Result<Slot, ResidencyError>` with no pending state. The
justfile's `wasm-painter` recipe exists for exactly this hazard, and states it:
a blocking wait on the web path "would instead deadlock at runtime against the
very event loop that resolves the promise it waits on — which no native test can
catch". So this row needs an **async admission phase** that does not exist, and
that is a residency restructure rather than a dependency edit. Two things make
it worth doing: the extent is already known (`AssetEntry` records width and
height), so no decode is needed to size an atlas slot; and `wgpu` on web has
`copy_external_image_to_texture`, so an `ImageBitmap` can reach a texture
without RGBA materialising in wasm linear memory _(recollection)_.

### 1.3 Desktop reads everything — and the ASTC decode belongs in the loader

**This section reversed an accepted record in its first draft, and the record
wins.** `docs/decisions/profile-preview-decodes-in-the-loader.md` (accepted,
story #435) is titled "The profile preview decodes in the loader, not in the
painter", and its decision says "**The painter is unchanged.**" and that the
decode happens in the load path, between `dashbuf::open` and
`dashscene_core::load_document`.

Two constraints the first draft missed, both binding:

- **`dashscene-gpu` must not depend on the packer.** Stated in
  `goldens/tooling/Cargo.toml` beside the dependency edge it governs — that
  crate is the only workspace member depending on both `dashpack` and a GPU
  painter, and it is where that join is checked — and repeated in
  `goldens/tooling/tests/lean_painter_baked_assets.rs`. Reaching
  `dashpack::preview` from the painter breaks it.
- **`dashpack::preview` is itself behind a Cargo feature**
  (`crates/dashpack/Cargo.toml`), off by default so that a third party's parser
  stays out of the shipping tool. §1.7 rejects Cargo features for the painter's
  decode policy, so routing the painter through one would contradict this file.

**So the ruling is: the decoder stays where the record puts it.** A desktop
build decodes a block payload in the **load path** and binds the result through
`BoundPayload::derived`, the seam that already exists for this
(`crates/dashscene-core/src/load.rs`). The painter is unchanged, needs no C++
dependency, needs no new Cargo feature, and `Painter::samples` needs no desktop
special case.

The capability the first draft wanted survives — a desktop build can read a HiFi
or LoFi `.dsb` — and it is sited where this repository already decided such
decodes go. `dashpack::preview` is the decoder either way: it is bit-exact by
specification, and it recovers block footprint and colour space from the file's
own `VkFormat` rather than being told them, which
`goldens/tooling/tests/profile_preview_weld.rs` holds with its wrong-footprint
and wrong-colour-space mutations.

One thing the weld does not cover, and a runtime path would meet: it pins
bit-exactness against astcenc's reference decode for payloads this repository
encoded. Nothing pins it against a third-party ASTC file.

### 1.4 What a `samples` test can and cannot pin

`Painter::samples` is a **declaration**, not a capability — §1.5 turns on that,
and so does this. A test that iterates `ImageFormat` and asserts a desktop
painter samples every variant therefore pins **whether someone edited a
declaration**, not whether a decoder exists. Add a variant, add one `=> true`
arm, wire no decoder, and such a test goes green while the first draw still
fails.

An earlier draft prescribed exactly that test and claimed it would make a new
format fail loudly until someone wired a decoder. It would not. Two further
reasons it was wrong:

- **There is no generated variant list to iterate.** The analogy drawn was to
  `dashscene-validator`'s `every_schema_image_format_maps_to_a_paint_format`,
  which iterates `dashbuf::ImageFormat::ENUM_VALUES` — a **flatc-generated**
  list that grows when the schema does. `dashpaint::ImageFormat` has no such
  list; the only full-enum census in the tree is a hand-written array in
  `crates/dashpaint/tests/boundary_b.rs` (a second, partial one lists the six
  ASTC variants in `crates/dashscene-gpu/src/residency.rs`). A hand-written list
  is the defect AGENTS.md already records as issue #1252.
- **The declaration a desktop painter should make is contested**, see §1.5.

**What would pin it**: a test that drives a real payload of each format through
`Residency::resident` and asserts a texture landed. The PNG half of that already
exists in `crates/dashscene-gpu/tests/layer3_image_fills.rs`. And an
exhaustiveness anchor — a `match` that fails to compile on a new variant —
rather than an array anyone can forget to extend.

### 1.5 Desktop's risk, and the declaration conflict it opens

Reading everything means desktop can never catch a wrong-bank mistake: ship a
bank a device cannot use, and desktop shows a correct picture until the device
does not.

**promote:** let a desktop painter **declare what a target would**, narrower
than the truth, on purpose. That turns desktop from a viewer into a gate once
`Painter::samples` is wired into the load path (issue #1292).

**This conflicts with §1.4's fullest reading, and the conflict is not resolved
here.** A painter that declares everything it can decode, and a painter that
declares what a target would, cannot both be the default. Whoever builds this
decides which is the default and which is opt-in; a test over either must be
conditional on the mode.

Note that §1.3's loader siting keeps an existing assertion true:
`crates/dashscene-gpu/tests/layer3_image_fills.rs` has
`the_declaration_names_what_this_painter_can_actually_take`, asserting the
painter's ASTC answer equals the device's. Under §1.3 the painter is unchanged,
so that assertion survives — one more reason the loader siting is the cheaper
ruling.

### 1.6 Not four painters

`docs/decisions/wgpu-is-the-lean-painter.md` (story #577): "One Rust codebase
over `wgpu` covers native and web, which removes the reason to have a second,
different painter for the browser." A browser painter reverses that decision on
its own reasoning.

A per-target painter would also get ASTC wrong, because ASTC support is a
property of the **device**, not of the build target. `SampledFormats::of` reads
it from `Renderer::samples_astc`, which asks the device — and
`crates/dashscene-gpu/src/render.rs` states why the distinction matters: "a
feature the adapter advertises but that was not requested is not a feature the
device has, and it is the device the atlas is created on." A desktop browser and
an Android browser answer differently from the same wasm build. (§4.1's
complaint is about `adapter_report.rs`, which reads the **adapter**; that is a
different call and a different problem.)

Extra painters are expensive in this repository specifically. `goldens/`, the
render oracle, `conformance/layer2-probes.json` and `unity/hlsl-conformance`
exist to stop two painters drifting, and R-T5 generates the Unity HLSL from the
WGSL so a port cannot.

What would justify a new painter is a graphics API `wgpu` does not cover. The
wgpu record anticipates that as its point 5 — a direct-GLES backend over the
same instance buffer and the same shaders — and names the crate for the role
rather than the dependency so that contingency forces no rename.

**On collapsing mobile into embedded**: an earlier draft said the embedded
target _is_ Android. That is narrower than the recorded fleet.
`docs/decisions/native-astc-codec-table.md` names SA8255 and SA7255 (Adreno) and
**Renesas R-Car (PowerVR/IMG cores)** in waves 1-2, with NVIDIA DRIVE proposed
for wave 3. What is true, and is what the argument needs, is that every wave 1-2
target samples ASTC natively and takes one byte-identical bank — which the same
record states.

### 1.7 Mechanism: `cfg`, not a Cargo feature

Decode policy is `cfg(target_arch)`/`cfg(target_os)`. Cargo features are
additive within one invocation's dependency graph, so a `--workspace` build
unifies a test-only feature selection into the crate under test. That is the
exposure, and it is the reverse of what an earlier draft claimed. For a crate
whose value is what it does **not** link, it is the wrong failure direction
either way.

### 1.8 Rejected for desktop: lazy dynamic libraries, and OS decoders

- **Lazy dynamic libraries.** Rust has no stable ABI, so lazily loading the
  `png` crate does not exist as an option. It would mean `dlopen`ing C libraries
  through a hand-written shim: three platform naming regimes, version skew
  against what the goldens were taken through, and a search path that makes the
  CVE argument worse rather than better — all on the one target where binary
  size does not matter. _(recollection)_
- **OS decoders.** macOS has Image I/O and Windows has WIC; Linux has no
  standard system image-decoding API, so a bundled fallback ships anyway and the
  OS path becomes complexity for no saving. _(recollection)_

**Laziness belongs in the work, not the linking, and is already there.**
`Residency::resident` checks its cache before touching the payload and decodes
only on a miss — a review fix, because decoding a resident PNG on every frame
that drew it measured 20.4 % of frame time
(`crates/dashscene-gpu/src/residency.rs`).

## Part 2 — the packer ships three profiles

**HiFi, LoFi, Web.** RAW stays the null binding and is not a fourth output.
`crates/dashpack/src/profile.rs` describes it, under `Profile::Raw`, as not a
shipping profile but the qualification baseline, the oracle lane and the
developer preview, citing `docs/decisions/asset-quality-profile-naming.md`. That
record's own words are different — it calls RAW "the truth, the qualification
baseline, the null binding". Both are named here because an earlier draft
attributed the first wording to the record.

### 2.1 The Web profile: lossless for artwork, fixed lossy for photographs

**And no band.** A band exists to choose between rungs, and with fixed settings
there is no choice. `crates/dashpack/src/profile.rs` states the rule under
`Contract::LosslessOnly`: "pinning a number that no measurement can ever fail is
exactly the defect issue #422 recorded against `blur-falloff`, and the fix is to
not write the number."

**Open question, and it decides whether the Web profile is worth building: what
is the lossless rung for a distance field on Web?** `Contract::LosslessOnly`
today goes straight to `Rung::Uncompressed`, which is `ktx2::Format::Rgba8` — 32
bits per texel. So a `.web.dsb` would ship every glyph atlas uncompressed over
the wire, in a profile justified entirely by download size. Either WebP-lossless
is admissible as that rung, or the Web profile does not help the largest asset a
browser downloads. **Distance fields must stay lossless either way**; the
question is which lossless encoding.

Note the rule that makes them lossless is not structural: `contract` names
profiles by hand (`Profile::HiFi | Profile::LoFi if !class.admits_lossy()`), and
the fixture lists in `crates/dashpack/tests/band_contract.rs` and in
`profile.rs`'s own tests enumerate profiles by hand too. A `Profile::Web` arm
must exist, because the outer `match` is exhaustive — but nothing forces it to
be lossless for distance fields, and no existing test would see it. **Derive
those lists from an exhaustive match before adding a profile.**

### 2.2 WebP, not AVIF and not JPEG XL

**promote:** AVIF is **ruled out**. It is the slowest to decode of the
candidates, and its compression advantage is a photographic-content result that
does not materialise on flat UI artwork. JPEG XL is not portable — Chrome
removed it. _(recollection; a measurement on this corpus would supersede it.)_

**WebP lossless for artwork.** The content is flat fills, icons and sharp edges,
so the comparison is WebP lossless against PNG rather than lossy WebP against
JPEG. Lossy would ring on icon edges and on any text baked into an image, which
is why the canonical payload is PNG.

**The saving is download, not decode.** WebP's decode-speed reputation is a
lossy-mode claim; WebP lossless is a different algorithm and against libpng is
not reliably faster. _(recollection)_ Justify the change on bytes over the wire.

### 2.3 The photographic setting, and what would settle it

**Start at `cwebp -q 80 -m 6 -sharp_yuv`** — `cwebp` defaults to 75, and 80 to
85 is the common visually-lossless-enough recommendation; `-m 6` costs only
offline encode time, the same reasoning as `PACK_QUALITY = Quality::Thorough`;
`-sharp_yuv` helps hard edges in mixed content. _(All three are recollection of
published practice, not measured here.)_

**q=80 is a provisional constant, not a band, and its doc comment must say so.**
Every **band value** in `profile.rs` is measured and carries the measurement
that produced it — the module's own scope, and narrower than "every constant":
`PACK_QUALITY` is itself adopted from astcenc's guidance, which is why it is the
precedent cited above. The precedent is `Terminal::FinestLossy`, documented as a
provisional answer with issue #553 named as what removes it. The comment must
state: adopted from published practice, **not measured on this corpus**, and the
criterion that would settle it.

**The criterion: the lowest q clearing SSIMULACRA2 >= 90** on photographic
content — the threshold `profile.rs` already cites for the ASTC side.

**The measurement is blocked, and an earlier draft said it was not.** Three
things are needed and two exist:

- **The fixtures exist**, and they are the four `photo-*` payloads —
  `interior-render`, `coast-forest`, `snowy-forest`, `dawn-mountains` under
  `corpus/photo/`, committed by issue #455. The earlier draft named
  `import-image-fill` and `detail-noise` instead; `band_contract.rs` describes
  the Figma image fills as "a gradient and flat rectangles" and `detail-noise`
  as generated rather than committed. Calibrating a photographic quality setting
  on a gradient and an integer hash would settle on a q far below what the class
  needs.
- **The harness exists** — `goldens/tooling/tests/perceptual_calibration.rs`
  carries per-rung SSIMULACRA2, FLIP and PSNR figures (issue #544). It is a
  **calibration-tier** test, pinned in `.config/calibration-tier.txt` and run by
  `just calibrate`. The earlier draft said `just triptych`, which runs
  `profile_preview_oracle` instead.
- **No WebP encoder exists anywhere in the workspace.** No `webp` or `cwebp`
  dependency, no recipe, and `bootstrap` installs none. That is the blocker.

### 2.4 Classifying an asset as photographic — withdrawn

An earlier draft proposed using the canonical container as a free signal: JPEG
means photograph, PNG means artwork. **That is wrong, and the issue cited as its
evidence says so.** Issue #342's body states that Figma "re-encodes opaque
uploads to JPEG (a hand-placed PNG came back from the asset CDN as JFIF), so the
fixture needed an alpha channel purely to force PNG storage", and closes by
noting that "Figma converts any opaque upload to JPEG".

**The signal tracks opacity, not content.** Opaque UI artwork arrives as JPEG.
So the proposed classifier would route flat artwork with sharp edges into the
lossy path — the exact failure §2.2 gives as the reason not to use lossy WebP on
artwork.

**Consequence for the plan.** The Web profile's **lossless half can proceed**;
its **lossy-for-photographs half is blocked on issue #553**, the class split,
which `profile.rs` already names as what would remove `Terminal::FinestLossy`.
No cheaper signal is available, and a producer-declared kind would be a schema
and importer change rather than a free one.

### 2.5 What adding WebP touches

WebP would be a **resident** format only, never a canonical one, so the
accept-list is unchanged and no `identify_webp` is needed. The
lossless-canonical argument stands on its own: the packer's bands grade a
derived encoding against the canonical texels, so a lossy canonical would mean
grading against something already degraded.

**The packer cannot express a WebP rung at all, and this is the largest gap.**
`dashpack::Rung` has exactly two variants, `Astc(BlockSize)` and `Uncompressed`;
`Rung::format` maps them onto `ktx2::Format`, which has exactly two, `Rgba8` and
`Astc`. KTX2 has no WebP format. So Part 2's deliverable has no rung, no
container and no `Rung::format` arm. Whatever shape this takes — a payload that
is not KTX2 at all, or a new rung with its own container — it is a change to the
packer's output types, not an additive enum arm.

**On the document side, one change is needed and two are not.**
`dashpaint::ImageFormat::Webp` is required, because `BoundPayload::derived`
names the resident format by that enum. A `dashbuf` schema append is **not**:
`dashbuf::ImageFormat` is the type of `AssetEntry.format`, the **canonical**
container, and no resident format has a variant there — ASTC and RGBA8 are
`dashpaint::ImageFormat` only. Adding one would force the validator's
`as_paint_format` arm, whose `validate_asset_payloads` calls
`dashpaint::image_id::identify` on the payload — which is exactly the
`identify_webp` this section says is unnecessary.

**Two matches absorb a new variant without a compile error. A third is
exhaustive and fails loudly — but its doc comment invites the wrong fix:**

- `dashpaint::ImageFormat::from_u32` ends in
  `_ => panic!("no image format
  carries this value")`, and it is the one place
  the FFI `u32` is read back. A WebP `ImageEntry` crossing boundary B panics at
  run time rather than failing the build.
- `SampledFormats::contains` ends in a catch-all guarded by a `debug_assert!`.
  In a release build `samples(Webp)` returns whatever the device said about
  ASTC.
- `AtlasFormat::of` is exhaustive and **would** fail to compile — the safe one,
  and the third referred to above. Its doc comment invites the wrong fix,
  though: "Every encoded container answers `AtlasFormat::Rgba8`, because a
  painter that decodes one produces RGBA texels". Adding `Webp => Self::Rgba8`
  there uploads compressed WebP bytes into an RGBA8 texture as if they were
  texels.

**And `is_encoded()` is not a free correction.** `Painter::samples`'s default
body is `format.is_encoded()`, and `SkiaPainter` does not override `samples`. So
adding `Webp` to `is_encoded()` makes the Skia painter declare it samples WebP
without a line of its code changing — the direction the trait's own
documentation says could not be made safe. Whatever adds the variant must settle
that declaration at the same time.

## Part 3 — shipping: one bank per file, the loader picks the path

**No format change at all.** `ds_runtime_load_document_mapped` already takes a
path, so selection is the loader building a path string. `NotOneBindingsSection`
stays as it is: `crates/dashbuf/src/container.rs` refuses more than one manifest
because two manifests are two answers to one question.

| artifact                       | format | consumers              |
| ------------------------------ | ------ | ---------------------- |
| `<doc>.dsb`                    | RAW    | reference, development |
| `<doc>.hifi.dsb` / `.lofi.dsb` | ASTC   | **embedded + desktop** |
| `<doc>.web.dsb`                | WebP   | browser                |

Desktop shares the embedded bank, because §1.3 lets the loader decode ASTC where
the device cannot sample it. That removes a whole artifact family.

**Selection happens before anything is opened**: configuration gives the tier,
the build target gives the format family, the path is built, one file is opened,
and its one manifest resolves canonical to resident. The loader never compares
variants — the moment it does, the combined-file design has been built by
accident. The loader stays profile-blind, matching the envelope.

### 3.1 The variant goes in the stem; the extension is always `.dsb`

Every variant has the same signature, envelope, reader and `dashc check`. The
extension names the format, and the format has not changed. Splitting it would
break every `*.dsb` glob, MIME mapping and editor association, and would need
updating again for every future variant.

**The separator is undecided and must be settled before the rule is written.**
The table above uses a dot; the committed precedent,
`goldens/dsb/v03-paint-hifi.dsb`, uses a hyphen. Whoever writes the path
function picks one and renames the other, rather than leaving the cited
precedent as the only file not following the convention it is offered as
evidence for.

**Keep the naming rule in one function**, `(document, tier, family) -> path`.
Four copies of one rule — the loader, the packer, a packaging script and a
document — is this repository's most expensive recurring failure.

**The filename is how the loader finds the file, never how it trusts it.** A
rename or a stale deployment gives the wrong bytes under the right name.
`Painter::samples` is the intended check, and it works for a `.dsb` inside an
APK that has no filename at all — but it has no production caller today (§4.3),
so today nothing checks this.

### 3.2 Rejected: one file carrying every variant

It would work. R5's mapped load already reads only the ranges the shown root
wants, so an unused bank costs zero RAM, and HiFi/LoFi would even deduplicate —
content addressing makes an identical rung one blob referenced by two manifests.
It needs tagged manifest sections, a selection input at load, and blob
deduplication by resident hash.

Rejected for four reasons:

- **The download cost is unrecoverable**, and trimming recovers only storage.
- **It recurs on every OTA delta.** Every device pulls updates for banks it
  never reads.
- **Trimming is a rewrite, not a truncation.** Content hashes must be
  recomputed, so the trimmed file is no longer byte-identical to what shipped
  and any signature over it is invalidated. Deleting an unused file is `rm`.
- **A wrong quality manifest fails silently.** It draws, correctly, using more
  memory than provisioned, and there is no budget to detect it (issue #462).

**It earns itself under one condition**: the variant is unknown until install —
one image for a product line, per-unit runtime configuration.

### 3.3 Rejected for a known fleet: UASTC / Basis

UASTC transcodes to ASTC 4x4 only, which is 8.00 bits per texel. The ladder's
rungs are 0.89, 1.28, 2.00, 3.56, 5.12 and 8.00 bits per texel — arithmetic from
the block size, not measurements. What **is** measured is which rung each band
selects: on `import-image-fill`, HiFi rejects 12x12 at 19.1129 %, rejects 8x8 at
2.8012 % and accepts 6x6 at 0.2133 % (`crates/dashpack/src/profile.rs`). So the
honest comparison against a selected rung is 8.00 against 3.56, about 2.2 times
worse residency. It is free on the BC side, because BC7 is 8 bpp natively, so it
taxes the ASTC majority to serve a minority this project does not yet ship to.

It also optimises an axis this project deprioritises:
`docs/decisions/asset-quality-profile-naming.md` puts it as "file size a
constraint rather than the goal", and `docs/decisions/compress-raster-only.md`
makes the same point in its own words.

`docs/specification/03-target-hardware-rules.md` keeps the Basis path for the
case it is right for — "a target whose GPU is not known at pack time, or for a
fleet that must share one OTA image across GPU architectures with no common
native format" — and that remains correct.

### 3.4 Rejected: decoding ASTC to raw at install

The specified fallback for an unknown fleet is Basis/UASTC transcoded at install
— format to format, so the output stays compressed. Decoding to raw gives up all
of what ASTC bought, on the hardware least able to absorb it. It would also
require shipping an ASTC decoder to an embedded target, contradicting §1's
embedded row.

Raw is legitimate only as a **correctness** fallback — drawing at 32 bpp beats a
black screen — and if built it must be loud: a named diagnostic reporting that
the device exceeded the packed budget, rather than a quiet path that makes a
slow product look like a working one.

## Part 4 — the blockers: two measurements and two implementation gaps

### 4.1 The ASTC residency probe does not exist — and is unfiled

`crates/dashscene-gpu/examples/adapter_report.rs` reads
`wgpu::Features::TEXTURE_COMPRESSION_ASTC` off the **adapter** — the capability
bit `docs/decisions/native-astc-codec-table.md` says is not evidence:

> Tegra-class drivers do report ETC2 and ASTC support, for GLES conformance, but
> a driver that reports one of those formats can still decompress it to raw
> pixels at upload time rather than sampling the compressed blocks natively.
> [...] **Capability bits are not evidence — the probe is.**

So `just android-probe` answers "will ASTC draw", never "does ASTC cost what the
profile was packed against".

**The penalty scales with how well ASTC was doing.** Against RGBA8's 32 bpp the
ratios are 4x at the 4x4 rung, 9x at 6x6, 16x at 8x8 and 36x at 12x12. The
escalation ladder picks the coarsest rung that holds the band, so it maximises
the exposure.

**The probe**: create a `VkImage` with an ASTC format at a known extent, call
`vkGetImageMemoryRequirements`, and compare the reported size against blocks
times 16 bytes. Three constraints:

- **Raw `ash`, not `wgpu`**, which abstracts allocation and exposes no way to
  ask a texture what it cost in device memory _(recollection; not checked
  against the pinned `wgpu` source)_. It cannot live inside the painter.
- **The negative control must itself be block-compressed** — BC7 on desktop,
  ETC2 on a mobile GPU. An RGBA8 control returns a ratio of 1 without exercising
  the blocks-times-16 denominator, so it would pass with the block arithmetic
  wrong.
- **Report the number, never a verdict.** A ratio is a fact; "ASTC is native" is
  an interpretation that goes stale when a driver updates.

**Do not build a vendor capability matrix instead.** It is stale within a year,
and the cases already differ _(recollection)_: desktop AMD and NVIDIA expose no
ASTC and fail cleanly at image creation; Tegra reports it and emulates;
Mesa/RADV may emulate through a compute shader. One probe answers every case.

### 4.2 Issue #462 blocks validation, not selection

`docs/features.md` §7: "**A memory budget for a device** — planned (v1). No
number exists in the specification, so a build can succeed and still not fit,
and nothing detects it. A stated, accepted gap."

**This does not block per-SoC configured selection.** A configured choice needs
no budget — the budget is for validating that the choice fits. Today a unit
configured for HiFi that cannot hold HiFi has nothing telling anyone.

### 4.3 `Painter::samples` has no production caller — issue #1292

Open, `debt`, milestone `v1`. Its own body says there is no call site outside
tests today, and lists three candidate shapes for the C ABI channel. This file
states no census of the call sites: an earlier draft gave one, presented it as
verified, and it was short by one. `grep -rn '\.samples('` is the derivation.

Until it is wired, a wrong bank **loads cleanly** — envelope verified, gate
passed, document loaded — and fails at first draw of the first asset that uses
it. A named refusal since #718, but mid-frame rather than at load.

### 4.4 The packer cannot decode, and its binary packs nothing — issue #1356

The prerequisite for everything in Part 2, and the one item Part 5 calls a
prerequisite for the rest. `crates/dashpack/src/bank.rs`, under a heading titled
"What this does not do": "Decode. `Asset::image` is the canonical payload
already in 8-bit RGBA [...] the canonical-to-texels ingest is a later story."
`png` is a dev-dependency, for the band fixtures. `crates/dashpack/src/main.rs`
prints its pins and exits `FAILURE`.

**Two module doc comments disagree on where the ingest belongs**, and nothing
adjudicates: `crates/dashpack/src/bank.rs` says it "belongs with the format
identification `dashc` already does, not here";
`crates/dashpaint/src/image_id.rs` says "The packer's decode belongs in the
packer, which publishes after everything here."
`docs/decisions/dashc-identifies-images-never-decodes.md` does not settle it —
it is about identification, and it makes decode-never-in-the-compiler permanent,
which argues against `bank.rs`'s reading. Neither is a _record_ in this
repository's sense; both are source comments.

## Part 5 — what this implies for the plan

**This is not v0.22.** That slice is SVG as a second producer — four filed
stories in a stated dependency order (#1242, #848, #774, #1243), one
owner-supplied entry condition, and a purpose this work does not share: it tests
P5, that no producer's limitations define the format.

**Proposed as its own slice, at the v0.21 phase-end revision**, which is the
ritual's own moment for deciding what the remaining slices contain. Two items
are filed as debt independently of any slice, because they stand alone: the
packer ingest gap (§4.4) is issue #1356, and the desktop JPEG defect (Part 0) is
issue #1357. Both are on `v1`.

A plausible story breakdown, offered for that revision rather than as a plan:

1. The packer ingest — canonical bytes to RGBA, and a `dashpack` binary that
   packs (#1356). Prerequisite for everything else here.
2. The ASTC residency probe (§4.1), with its block-compressed negative control.
   Unfiled.
3. Per-target decode policy: the painter's decoder set by `cfg` (§1.7), and the
   desktop ASTC decode in the **loader** (§1.3).
4. The Web profile's output types in `dashpack` (§2.5) — the rung and its
   container, which is the part with no existing shape.
5. `dashpaint::ImageFormat::Webp`, the two matches that absorb it silently and
   the one that invites a wrong fix (§2.5), including the
   `is_encoded`/`SkiaPainter` consequence.
6. The Web profile itself (§2), lossless half only until #553 lands the class
   split; q=80 labelled provisional.
7. Loader-side variant selection and the one path-building function (§3).

The web row's async admission phase (§1.2) is a residency restructure and may
deserve its own story. `Painter::samples` being wired is issue #1292 and belongs
to whoever owns the C ABI channel; it is a dependency of item 7 rather than part
of it.
