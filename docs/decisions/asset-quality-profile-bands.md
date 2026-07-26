# Decision: a quality profile is a measured band contract, and a distance field has no lossy rung

    status   accepted
    scope    the RAW/HiFi/Lite band contracts, the per-asset encode-and-diff
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
             #422 (what a band has to be able to fail)

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
| `lite-image-fill` | 8                   | 5 %         |

**HiFi's threshold sits near the encoder's noise floor, not at a visibility
threshold.** The failure mode HiFi exists to prevent on this class is
banding across a smooth gradient — a _structured_ error of small amplitude
spread over a wide area, which a high per-texel threshold is blind to. That
is #422's finding pointed the other way: one number cannot both size a
residual and act as a gate. 2 of 255 is one quantisation step above
bit-exact, and the 1 % budget then says "all but a hundredth of the texels
are within one step". Lite is four times the threshold and five times the
budget: 8 of 255 is roughly where a single texel's error stops being
invisible on an 8-bit panel.

Each band ships with the mutation that fails it, measured. The mutation is
**pin the ladder one rung coarser than the packer chose**, which is exactly
the defect the mechanism exists to prevent:

| band              | fixture             | chosen | mutation | measured      | budget |
| ----------------- | ------------------- | ------ | -------- | ------------- | ------ |
| `hifi-image-fill` | `import-image-fill` | 6x6    | 8x8      | **2.8012 %**  | 1 %    |
| `lite-image-fill` | `detail-noise`      | 6x6    | 8x8      | **10.4401 %** | 5 %    |

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
  Lite columns are the expected _outcome_ of a band, not a rule the packer
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
- Lite is defined and measured but not activated: HiFi ships first, and Lite
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

- **Lite's budget is not exercised by any committed _real_ asset.** The two
  real image fills are a gradient with flat rectangles, which ASTC reproduces
  almost exactly at every footprint. `detail-noise` is generated rather than
  committed precisely to make Lite's budget the binding term; without it the
  number 5 % would be unexercised.
- **No asset lands on 4x4 or 5x5.** Those two rungs are in the ladder and are
  walked, but no committed fixture stops at either, so nothing here says the
  ladder's fine end behaves correctly in a case that matters.
- **Nothing measures a photograph.** The corpus has none. `detail-noise`
  stands in for high-frequency content and is honest about being synthetic.
- **Nothing here measures in-context quality.** These are per-asset bands.
  Banding behind text, or block patterns against a stroke, are scene-level
  effects the per-asset oracle cannot see; that is the profile-preview
  oracle's job (story #435).
- **No budget check.** A profile must also fit the target's memory and
  bandwidth budget at pack time, which is a later story; nothing here refuses
  a bank for being too large.
