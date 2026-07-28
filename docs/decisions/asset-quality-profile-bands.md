# Decision: a quality profile is a measured band contract, and a distance field has no lossy rung

    status   accepted
    scope    the RAW/HiFi/LoFi band contracts, the per-asset encode-and-diff
             oracle, and the escalation ladder — `dashpack::band`,
             `dashpack::profile`, and the `AssetEntry.kind` the hard rules read
             (v0.12, story #432)
    source   docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md,
             "The kernel" points 2 and 3
    related  docs/decisions/asset-quality-profile-naming.md (the vocabulary),
             docs/decisions/native-astc-codec-table.md (the per-target codecs),
             docs/decisions/asset-model-content-addressed-blobs.md (what a
             binding is), docs/decisions/baked-vector-msdf-field.md,
             docs/technotes/2026-07-26-tolerance-band-coverage.md and issue
             #422 (what a band has to be able to fail), issue #544 and
             goldens/tooling/tests/perceptual_calibration.rs (section 5 —
             where these bands land on SSIMULACRA2 and FLIP), issue #549
             (the display geometry section 5 has to assume)

## Context

The asset-pipeline plan defines the three quality profiles as _band
contracts, not formats_: the packer encodes a candidate, diffs it against
the canonical payload, and escalates cheap → better → lossless until the
profile's band holds, so over-compression is structurally impossible. That
leaves three things to settle, and each of them is a number or a rule that
downstream work will be measured against.

**What the bands are.** The render oracle already pins three tolerance
bands, and the obvious move is to reuse their values. It is the wrong move:
those bands are wide because they compare a CPU rasterizer against a
server-side export and must absorb anti-aliasing, resampling, hinting and
gamma disagreement. A pack diff has none of that noise — both sides are the
same texel grid at the same size, and the only thing that can differ is
codec error.

**Whether the bands can fail.** Issue #422 measured that the render
oracle's `blur-falloff` band catches none of the six defects the frames it
governs exist to catch, because a 12 % area budget cannot be exceeded by a
bounded-area defect. The roadmap's v0.11-close revision records that this
second family of bands must be designed against that finding rather than by
analogy with the first.

**Whether a distance field may ever be encoded lossily.** The plan says, in
one sentence, that distance fields never enter a lossy path _and_ that
single-channel fields ride EAC-R11 — which is a lossy block format. Read
literally the two clauses conflict wherever a field is single-channel, and
`docs/decisions/native-astc-codec-table.md` carries the same conflict in
its "Field (SDF) encoding" column.

## Choice

### 1. A profile supplies a band; the class supplies the ladder

The escalation ladder belongs to the **asset class**, and the profile
supplies only the band that grades it. One ladder, two bands — which is what
makes "a band contract, not a format" true in the code rather than only in
prose. The image-fill ladder is the six square ASTC footprints in strictly
increasing bitrate — 12x12, 10x10, 8x8, 6x6, 5x5, 4x4 — followed by the
terminal rung, uncompressed 8-bit RGBA.

Only square footprints are rungs. ASTC's ten non-square footprints trade
horizontal resolution for vertical, which is a property of the content
rather than a step in quality; an anisotropic choice needs its own evidence
and would sit beside this ladder rather than inside it.

The terminal rung is what makes over-compression impossible rather than
unlikely: its payload _is_ the canonical texels, so it cannot fail a band and
the walk always ends inside the contract.

### 2. The two bands, and the mutation that fails each

| band              | per-texel threshold | area budget |
| ----------------- | ------------------- | ----------- |
| `hifi-image-fill` | 2                   | 1 %         |
| `lofi-image-fill` | 8                   | 5 %         |

**HiFi's threshold sits near the encoder's noise floor, not at a visibility
threshold.** The failure mode HiFi exists to prevent on this class is
banding across a smooth gradient — a _structured_ error of small amplitude
spread over a wide area, which a high per-texel threshold is blind to. That
is #422's finding pointed the other way: one number cannot both size a
residual and act as a gate. 2 of 255 is one quantisation step above
bit-exact, and the 1 % budget then says "all but a hundredth of the texels
are within one step". LoFi is four times the threshold and five times the
budget: 8 of 255 is roughly where a single texel's error stops being
invisible on an 8-bit panel.

Each band ships with the mutation that fails it, measured. The mutation is
**pin the ladder one rung coarser than the packer chose**, which is exactly
the defect the mechanism exists to prevent:

| band              | fixture             | chosen | mutation | measured      | budget |
| ----------------- | ------------------- | ------ | -------- | ------------- | ------ |
| `hifi-image-fill` | `import-image-fill` | 6x6    | 8x8      | **2.8012 %**  | 1 %    |
| `lofi-image-fill` | `detail-noise`      | 6x6    | 8x8      | **10.4401 %** | 5 %    |

Both are near misses on purpose — 2.8 and 2.1 times the budget, not fifty
times it. A mutation that fails by two orders of magnitude shows a band is
not vacuous but says nothing about whether the _number_ binds. These two do:
`widening_a_budget_changes_which_rung_ships` triples either budget and the
packer ships the coarser rung.

**Both knobs bind.** On `import-image-fill` the threshold rejects 12x12
(19.1129 %) and the _budget_ rejects 8x8 (2.8012 % against 1 %) and accepts
6x6 (0.2133 %). This is the property #422 found `blur-falloff` lacked.

### 3. A distance field has no lossy rung, under any profile

The **strict reading** is the rule: a distance field never enters a lossy
path, whatever the profile and whatever the codec. It is expressed
structurally — the class's lossy ladder is empty — so no measurement, and no
later edit to a band, can route a field onto a lossy rung.

It is measured rather than assumed. At the _finest_ ASTC footprint the
ladder offers, 4x4 at 8 bits per texel, the committed MSDF atlases still
fail both bands:

| atlas               | texels beyond delta 8 at 4x4 | peak per-channel error at 4x4 | peak at 12x12 |
| ------------------- | ---------------------------- | ----------------------------- | ------------- |
| `inter-ascii-atlas` | 8.6044 %                     | 84                            | 255           |
| `arabic-atlas`      | 8.8753 %                     | 70                            | 255           |

No lossy rung could have held either band for this content, so the strict
reading costs nothing a measurement would have bought back. A multi-channel
distance field is high-frequency by construction — each channel is a signed
distance with sharp median transitions — which is the content class block
compression is worst at.

### 4. `AssetEntry.kind`, and who sets it

The rule needs a key, so the schema gains `AssetKind { Image = 0,
DistanceField = 1 }` and `AssetEntry.kind`. `Image` is value 0, so the field
is omitted by `flatc` for every entry `dashc` writes today and **no committed
`.dsb` byte moved** — proven by `dashc`'s `the_fixture_emits_the_golden_dsb`,
which recompiles the committed golden through the current emitter, and by
`crates/dashbuf/tests/asset_kind.rs`, which proves the omission mechanism
itself.

The producer sets it, because the producer is the only place it is known: a
baked MSDF atlas is a PNG on the wire exactly as an image fill is, so nothing
downstream can tell them apart by inspecting the bytes. `dashc`'s vector bake
now mints its atlas asset as `DistanceField`; its Figma image fills stay
`Image`.

### 5. Where these bands land on two published perceptual scales

Every number in sections 1 to 4 is a per-texel threshold or an area budget, and
both are this project's own units. They say a band is a gate; they do not say
whether the rung a band chooses is _good_. Issue #544 measured that against two
published scales — **SSIMULACRA2** (JPEG XL; roughly 90 and above visually
lossless, 70 and above high quality, 50 medium, 30 low) and **FLIP** (NVIDIA;
mean error over the image, 0 identical, 1 maximally different), with PSNR
recorded for comparability and deciding nothing.

The whole ladder is walked rather than only the selected rung, because the
selected rung alone cannot say whether the cut is in the right place. The full
35-row table is `goldens/tooling/tests/perceptual_calibration.rs`, which pins
every figure; what follows is what it says.

**Both bands land on the published rung their name implies.**

| fixture             | LoFi took  | scores    | HiFi took    | scores     | HiFi rejected | scores |
| ------------------- | ---------- | --------- | ------------ | ---------- | ------------- | ------ |
| `import-image-fill` | astc-12x12 | **78.35** | astc-6x6     | **92.87**  | astc-8x8      | 87.82  |
| `block-stress`      | astc-6x6   | **78.57** | uncompressed | **100.00** | astc-4x4      | 87.69  |

HiFi's cut falls between a rejected 87.82 and an accepted 92.87 on the real
image fill, and between a rejected 87.69 and the lossless rung on the stress
fixture. SSIMULACRA2's visually-lossless threshold is 90, and the band brackets
it from both sides on both fixtures. That is worth stating plainly because it
was not designed: the threshold of 2 and the budget of 1 % were chosen from
texel deltas, with no knowledge of this scale. LoFi's selected rungs measure
78.35 and 78.57 against a high-quality threshold of 70.

The floors asserted are the **published** thresholds — HiFi at 90, LoFi at 70 —
not the measured values. A floor set to whatever the current fixtures happen to
score would be one more number internal to this project, which is the thing this
calibration exists to stop doing. Measured headroom is 2.87 and 8.35.

**The counterfactual for a distance field.** The ladder is walked for the two
MSDF atlases as well, as what the packer refuses rather than as a rung it could
select. At the _finest_ footprint the ladder offers, 4x4 at 8 bits per texel,
neither atlas reaches even HiFi's floor:

| atlas               | at astc-12x12 | at astc-4x4 | selected     |
| ------------------- | ------------- | ----------- | ------------ |
| `inter-ascii-atlas` | 17.91         | 86.12       | uncompressed |
| `arabic-atlas`      | 21.22         | 86.88       | uncompressed |

This is a second direction of evidence for section 3, and it is weaker than the
texel measurements beside it rather than stronger. SSIMULACRA2 and FLIP model
_colour_ perception, and an MSDF atlas carries signed distances — which is why
the class encodes in `ColorSpace::Linear`. Scoring one says how visible the loss
would be if those distances were colours. Nobody looks at the atlas; what is
looked at is the glyph a shader derives from it. The figures are recorded as
comparability, never as a perceptual claim about a rendered glyph — and the
caveat is part of the argument, because a per-asset perceptual metric being
unable to evaluate a distance field is one more reason the rule is structural.

**Three findings the measurement produced that were not being looked for.**

- **A band reading 0.0000 % can carry real loss.** The profile-preview oracle's
  `profile-photo` LoFi arm measures 0.0000 % differing, and the manifest already
  recorded that this scene cannot exercise the LoFi budget. The same arm scores
  81.17 on SSIMULACRA2 and 75.64 dB on alpha PSNR. A threshold of 8 is blind to
  it by construction. This is the clearest case for recording the perceptual
  columns beside the bands rather than instead of them.
- **FLIP depends on the viewing condition more than expected.** Reported at
  FLIP's shipped default of 67 pixels per degree and at 107.71 (an automotive
  centre display, 0.9 m from a 1920 px, 0.28 m wide panel), the two disagree by
  14 % to 32 % on real block-compression error — not the ~3 % an early
  bit-quantisation probe suggested. The panel geometry is a stated assumption
  and not a specified value, because
  `docs/specification/03-target-hardware-rules.md` pins no display geometry at
  all. Issue #549 carries that gap.
- **The ladder is not monotonic in quality when the footprint does not divide
  the extent.** `v03-paint` is 16x16: at astc-8x8 and finer it is exactly
  lossless, while astc-10x10 measures _worse_ than astc-12x12 (FLIP 0.1543
  against 0.1169). Both of the coarser footprints pad to partial blocks. Quality
  follows block alignment there, not bitrate.

**What is excluded, and why.** `v03-paint` has no SSIMULACRA2 figure. The metric
is multi-scale and refuses anything below 8x8; at 16x16 only two of its six
scales survive, and the score stops meaning what it means elsewhere. It keeps
its FLIP and PSNR columns, which have no such floor.

The floor is set at 64, above the metric's own 8x8, because a score that is
_produced_ is not the same as a score that is _comparable_. The probe behind
that judgement — the same 4-bit quantisation scoring 12.59 on a 380 px payload
and 92.86 on a 16 px one — is recorded in
`docs/archive/2026-07-28-perceptual-band-calibration-design.md`. Those two
figures come from that probe rather than from any check in this repository, and
are cited here on that footing: no test reproduces them.

## Why

- **The band has to be able to fail, and the number has to be the thing that
  fails it.** #422's finding was not that `blur-falloff` was too wide but
  that its budget was not the binding term for any defect it governs. Both
  bands here are held to the opposite by committed measurements, and by a
  test that refuses to let a band be pinned without one.
- **Classify from the measured residual, never from expectation.** The design
  capture and `native-astc-codec-table.md` expect HiFi to be "typically ASTC
  4x4". On the committed assets it measures 6x6, 8x8 and uncompressed, and
  never 4x4. The measurement is what is recorded. The codec table's HiFi and
  LoFi columns are the expected _outcome_ of a band, not a rule the packer
  applies — a profile that named its footprint would be a format, which is
  the thing this design is explicitly not.
- **A rule is safer than a check.** Expressing fields-never-lossy as an empty
  ladder means there is no lossy rung to reach, rather than a check that a
  later refactor could route around. The failure mode of getting this wrong
  is a silently degraded icon, which is worse than a size regression.
- **More than one asset per class, and one that escalates.** Debt #395 was a
  silent paint-entry collapse that survived because its fixture had exactly
  one instance, so every index in it was 0. Three image fills and two
  distance fields are measured here, `v03-paint` and `import-image-fill` both
  escalate, and `detail-noise` escalates through every lossy rung to the
  terminal one.
- **One vocabulary, welded rather than asserted.** `dashpack::band` and
  `goldens::oracle` cannot share an implementation — one takes decoded texels
  and must not link skia, the other takes PNG bytes and does. They are
  written twice and held together by `goldens/tooling/tests/asset_band_weld.rs`,
  which runs one image pair through both and asserts the three reported
  numbers are equal.

## Consequences

- The `Field (SDF) encoding` column of
  `docs/decisions/native-astc-codec-table.md` still reads `EAC-R11`, which is
  a lossy format and therefore contradicts this record. **Left open for the
  repository owner** and carried by issue #453, which owns the EAC-R11
  encoder. Until it is settled the strict reading holds, because its failure
  mode is a size regression rather than a silent quality loss.
- LoFi is defined and measured but not activated: HiFi ships first, and LoFi
  turns on when a measured budget or OTA constraint demands it.
- `dashpack` now depends on `dashbuf`, which is the coupling the
  asset-pipeline plan named when it placed the packer in this workspace.
- The vendored astcenc is built at `opt-level = 3` under the dev profile.
  Left unoptimised it runs about eighty times slower — 597 s against 7.5 s
  for one band sweep — which would make `just test` unusable. Both profiles
  produce byte-identical output, which the pinned measurements in
  `crates/dashpack/tests/band_contract.rs` check on every run.

## What this does not pin

Recorded because a green contract read as broader evidence than it is, is the
failure #422 documents.

- **LoFi's budget is not exercised by any committed _real_ asset.** The two
  real image fills are a gradient with flat rectangles, which ASTC reproduces
  almost exactly at every footprint. `detail-noise` is generated rather than
  committed precisely to make LoFi's budget the binding term; without it the
  number 5 % would be unexercised.
- **No asset lands on 4x4 or 5x5.** Those two rungs are in the ladder and are
  walked, but no committed fixture stops at either, so nothing here says the
  ladder's fine end behaves correctly in a case that matters.
- **Nothing measures a photograph.** The corpus has none. `detail-noise`
  stands in for high-frequency content and is honest about being synthetic.
  Section 5's perceptual figures inherit this exactly: they are measured on a
  gradient, two MSDF atlases and generated stress content, so they are a
  baseline that #455's representative fixtures will move. That the harness
  landed first is deliberate — the movement is then attributable to the
  content rather than to a new measurement.
- **Section 5's figures are not confirmed on a second architecture.** They are
  pinned at a fixed precision, which is what would make a disagreement visible,
  and every figure recorded so far was measured on aarch64-apple-darwin. A
  disagreement beyond the pinned precision is a finding to investigate rather
  than a number to re-record — the same standing this record's sibling
  measurements have, and the same argument
  `crates/dashpack/tests/band_contract.rs` makes for its digests.
- **No perceptual metric evaluates a rendered glyph.** Section 5's
  distance-field figures score an atlas of signed distances as if they were
  colours. What a reader would actually judge is the glyph a shader derives
  from that atlas, and nothing here measures it.
- **Nothing here measures in-context quality.** These are per-asset bands.
  Banding behind text, or block patterns against a stroke, are scene-level
  effects the per-asset oracle cannot see; that is the profile-preview
  oracle's job (story #435).
- **No budget check.** A profile must also fit the target's memory and
  bandwidth budget at pack time, which is a later story; nothing here refuses
  a bank for being too large.
