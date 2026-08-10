# Calibrating the profile bands against SSIMULACRA2 and FLIP — design

    status   archived 2026-07-28, gardened. Working memory for issue #544,
             kept verbatim below the line. The durable record is
             docs/decisions/asset-quality-profile-bands.md section 5, which
             is the authority; the harness is
             goldens/tooling/tests/perceptual_calibration.rs and the scene
             half is in goldens/oracle/profile-manifest.json.

             Two things this design got wrong, corrected by the measurement
             and recorded here so the file is not read as accurate: FLIP
             turned out to move 14 % to 32 % between the two viewing
             conditions rather than the ~3 % section 8 predicted from a
             bit-quantisation probe (issue #549), and the scene half landed
             in the existing profile-preview oracle rather than in the new
             test file, because that oracle already renders the arms.
    issue    #544 (debt, milestone v0.13)
    refs     #455 (representative fixtures — this must land first),
             #435 (the profile-preview oracle and `just triptych`),
             #422 (a band has to be able to fail),
             docs/decisions/asset-quality-profile-bands.md (the bands),
             docs/technotes/tolerance-band-coverage.md (classify
             from the measured residual, never from expectation),
             docs/wip/2026-07-28-photorealistic-3d-content.md (target content)

## The problem

`docs/decisions/asset-quality-profile-bands.md` sets HiFi at a per-texel
threshold of 2 with a 1 % area budget, and LoFi at 8 with 5 %. Both are
measured gates and each ships with a mutation that fails it. What neither
number says is where the rung it chooses lands on a scale a reader outside this
repository would recognise. A reviewer asked whether LoFi is good enough has
nothing to anchor to.

Two published perceptual scales answer that:

- **SSIMULACRA2**, from the JPEG XL work, currently the best-correlated still
  image metric. Its scale is published: roughly 90 and above visually lossless,
  70 and above high quality, 50 medium, 30 low.
- **FLIP** (NVIDIA), designed for comparing _rendered_ images — it models what a
  viewer notices when alternating between two images. Half the target content is
  3D renders, so this is the matched metric rather than a generic one.

PSNR is recorded for comparability and decides nothing.

## Sequencing

This lands **before** the representative fixtures of #455. Calibrating against
the current unrepresentative assets establishes the harness and the baseline;
the fixtures then move the numbers, and the movement is the finding. In the
other order a new harness and new content change the numbers together and
neither can be attributed.

## Feasibility, measured before designing

Both metrics have maintained Rust implementations that build in this workspace.
A spike outside the repository ran them over two committed corpus payloads,
degraded by bit quantisation as a stand-in for codec loss.

| fixture                     | degradation     | SSIMULACRA2 | FLIP @ 67 ppd | FLIP @ 108 ppd | PSNR  |
| --------------------------- | --------------- | ----------- | ------------- | -------------- | ----- |
| `import-image-fill` 380x380 | none            | 100.00      | 0.000000      | 0.000000       | inf   |
| `import-image-fill` 380x380 | quantise 2 bits | 89.03       | 0.051204      | 0.049703       | 41.56 |
| `import-image-fill` 380x380 | quantise 4 bits | 12.59       | 0.190805      | 0.189334       | 27.63 |
| `v03-paint` 16x16           | quantise 4 bits | **92.86**   | 0.140732      | 0.134487       | 27.50 |

Three results shape the design.

1. **SSIMULACRA2 is deterministic run to run**, to twelve decimal places on this
   machine. Cross-architecture agreement is still unmeasured.
2. **SSIMULACRA2 is not meaningful at 16x16.** The metric refuses anything below
   8x8 and stops rescaling there, so a 16x16 payload reaches two of its six
   scales. The same 4-bit quantisation scores 12.59 on the 380 px payload and
   92.86 on the 16 px one.
3. **FLIP is nearly insensitive to viewing distance** across the range that
   separates a desk from a dashboard: 0.051204 at 67 ppd against 0.049703 at
   108 ppd, a difference of about 3 % of the value.

## Design

### 1. Where the harness lives

One new test file, `goldens/tooling/tests/perceptual_calibration.rs`, carrying
both halves of the calibration, with the metric implementations in a new
`goldens/tooling/src/metric.rs`.

`goldens` already depends on `dashpack` with its `preview` feature, already
walks the ASTC ladder in `tests/profile_preview_oracle.rs`, and already holds
the RAW, HiFi and LoFi renders the scene half needs. So both halves reach
everything they need without a new coupling.

**Alternative considered — per-asset figures in `crates/dashpack/tests/`,
beside the bands they calibrate.** That reads better and was rejected on cost.
It forces one of two things: a dev-dependency cycle (`dashpack` dev-depends on
`goldens`, which depends on `dashpack`), which Cargo permits but which would
build Skia to run the packer's own tests; or a second copy of the metric glue,
held to the first by a weld test in the shape of
`goldens/tooling/tests/asset_band_weld.rs`. The weld exists there because
`dashpack::band` must not link Skia and `goldens::oracle` must; no such
constraint applies to a test-only metric, so a second implementation would be
duplication without a reason. `crates/dashpack/tests/band_contract.rs` gains a
pointer comment to the calibration instead.

### 2. The ladder is walked, not only the chosen rung

Scoring only the rung the packer selected answers "how good is HiFi" and leaves
"is the band's cut in the right place" unanswered. So for each fixture the
table records **every rung** — 12x12, 10x10, 8x8, 6x6, 5x5, 4x4, uncompressed —
and marks which rung each profile's band accepted.

That converts "HiFi is a threshold of 2 with a 1 % budget" into "HiFi accepted
the rung scoring N and rejected the rung scoring M", which is a claim a reader
can evaluate without trusting this repository's conventions. It is also what
the issue asks for when it says the useful question is where loss becomes
visible, rather than three points on a ladder.

For the two MSDF atlases the ladder is walked as an explicit **counterfactual** —
what the packer refuses, never a rung it could select. `dashpack::profile`
expresses fields-never-lossy structurally, as an empty lossy ladder, and
nothing here changes that. The counterfactual puts a published-scale number
behind the decision record's "no lossy rung could have held either band".

### 3. Fixtures

| fixture             | extent  | kind           | ladder walked as | scored by SSIMULACRA2 |
| ------------------- | ------- | -------------- | ---------------- | --------------------- |
| `import-image-fill` | 380x380 | image          | selectable rungs | yes                   |
| generated stress    | 256x256 | image          | selectable rungs | yes                   |
| `inter-ascii-atlas` | 512x256 | distance field | counterfactual   | yes                   |
| `arabic-atlas`      | 512x256 | distance field | counterfactual   | yes                   |
| `v03-paint`         | 16x16   | image          | selectable rungs | **no** — see below    |

The generated high-frequency payload is the one the profile-preview oracle
already builds for its `profile-stress` scene. It moves into
`goldens/tooling/tests/common/` so both tests share one generator rather than a
second copy that has to agree with the first.

**`v03-paint` is excluded from the SSIMULACRA2 column, with the reason
measured** — the 92.86 against 12.59 above. It keeps its FLIP and PSNR columns,
which have no minimum extent. Excluding it silently, or scoring it and
reporting the number as if it meant the same thing as the others, are both
worse than naming the exclusion and the measurement behind it.

### 4. Alpha is measured separately

SSIMULACRA2 and FLIP are both defined over RGB and neither reads an alpha
channel. `dashpack::band::diff` compares alpha like any other channel, and on
an image fill a codec that drops coverage is one of the more visible failures.
Rather than leave that blind spot open, the table carries a separate alpha PSNR
column. The limitation is recorded in `goldens::metric`'s module documentation,
where a later reader arrives.

### 5. The scene half

For each scene in `goldens/oracle/profile-manifest.json` and each production
arm, the same four metrics are computed on the profile render against the RAW
render, and recorded beside that arm's existing band numbers. Both arms are the
same painter, solver, typesetter and canvas, so the only variable is which
bytes the asset entries resolve to.

### 6. What is asserted

Every recorded score is asserted at a fixed precision — 2 decimals for
SSIMULACRA2 and PSNR, 4 for the FLIP mean. The rounding is the tolerance, which
is how the repository already pins band fractions such as 2.8012 %. A
disagreement on another architecture beyond that precision is a finding to
investigate, not a number to re-record — the same argument
`crates/dashpack/tests/band_contract.rs` makes for its BLAKE3 digests, and the
first opportunity to test it here.

On top of the pinned values sits the falsifiable claim: a **floor per band**
against the published scale, asserted for every fixture.

### 7. The floors are chosen after measurement, not before

This is the load-bearing rule of the whole design.

If HiFi's accepted rung measures 96 across the fixtures, then "at or above 90,
visually lossless" is a sound floor to pin. If it measures 85, the finding is
that the band is looser than the rung it implies, and that goes into the
decision record and a new issue. The floor is **not** quietly lowered to
whatever the measurement happened to pass, and the fixture is **not** adjusted
to fit the band. If a band picks a rung that scores badly on both metrics, that
is evidence the band is wrong, and the direction of the fix is to retune the
band against the asset.

This is `docs/technotes/tolerance-band-coverage.md`'s rule: classify
from the measured residual, never from expectation.

### 8. FLIP viewing conditions

The headline column is FLIP's published default of 67 pixels per degree, which
is exactly a 0.7 m viewing distance on a 3840 px, 0.7 m wide monitor. Using the
published default keeps our figures comparable to every published FLIP number.

A second column reports about 107.7 ppd, derived from an automotive centre
display: 0.9 m viewing distance, 1920 px across a 0.28 m wide panel.

**That geometry is not specified anywhere in this repository.**
`docs/specification/03-target-hardware-rules.md` pins GPU class, render-pass
rules and texture policy, and no display geometry at all. So the number is
recorded as a stated assumption of the calibration, and a follow-up issue
proposes pinning panel geometry in the hardware rules. A test is the wrong
place to invent specification.

The spike already shows what the second column buys: the two agree to about
3 %, so FLIP's answer does not depend on which of these two viewing conditions
is assumed. That is the finding the column exists to produce, and it is worth
recording once even though it argues the column could later be dropped.

### 9. Documents

- A calibration section in `docs/decisions/asset-quality-profile-bands.md`
  carrying the table and what it says about each band, with "What this does not
  pin" updated. The record is edited in place rather than added beside, per the
  working-memory lifecycle rule.
- Viewing-condition rules for human review, written into the `just triptych`
  recipe comment and the preview oracle's module header — native pixels, no
  browser or DPI scaling, integer nearest-neighbour if zoom is needed, blind and
  randomised order, the full ladder rather than three points, ITU-R BT.500 and
  ITU-T P.910 as the standard protocols. Smooth scaling averages block artifacts
  away, so a viewer who rescales the image is reporting on the resampler rather
  than on the codec.

### 10. Dependencies

Two dev-dependencies on `goldens` only. Nothing reaches a shipped crate.

| crate         | version | license                                      | notes                                      |
| ------------- | ------- | -------------------------------------------- | ------------------------------------------ |
| `ssimulacra2` | 0.5.1   | BSD-2-Clause                                 | pure Rust, from rust-av                    |
| `nv-flip`     | 0.1.2   | MIT OR Apache-2.0 OR Zlib                    | wraps `nv-flip-sys`                        |
| `nv-flip-sys` | 0.1.1   | (MIT OR Apache-2.0 OR Zlib) AND BSD-3-Clause | vendors NVIDIA's CPU FLIP inside the crate |

All are compatible with this repository's MIT licence. `NOTICE` covers vendored
source trees **inside this repository**; `nv-flip-sys` vendors its C++ within
its own published crate, which makes it an ordinary dependency rather than a
vendored tree, so no `NOTICE` entry is expected. This is called out so the
question is answered rather than skipped.

Cold build of both crates measured 7 s. Runtime is dominated by the extra
astcenc ladder encodes, roughly the scale of `band_contract.rs`'s existing
7.5 s sweep.

## Success criteria

1. `just build` green, with the calibration test running under
   `cargo test --workspace`.
2. Every rung of every fixture has a recorded, asserted SSIMULACRA2, FLIP (two
   ppd), PSNR and alpha PSNR figure, except where an exclusion is named with its
   measurement.
3. Each band carries a floor claim against the published scale, chosen from the
   measurement, and the claim fails if a fixture drops below it.
4. Each scene arm carries the same four metrics in
   `goldens/oracle/profile-manifest.json`.
5. `docs/decisions/asset-quality-profile-bands.md` carries the table and says
   what each band's cut means on the published scales.
6. The triptych's viewing conditions are documented where a reviewer arrives.
7. A follow-up issue exists for pinning panel geometry in the hardware rules.

## What this design does not pin

- **It does not make the fixtures representative.** Every figure it produces is
  measured on a gradient, two MSDF atlases and generated stress content. That is
  the point of the sequencing: the numbers are a baseline that #455's fixtures
  will move.
- **It does not measure cross-architecture agreement.** The pinned precision
  makes a disagreement visible; only a run on another architecture can settle
  whether one exists.
- **It does not measure a human.** SSIMULACRA2 and FLIP are models of
  perception, not observers. The triptych and its viewing conditions remain the
  place a person is asked, and no score here substitutes for that.
- **It does not gate the packer.** The floors fail a test; they do not change
  which rung the packer selects. Retuning a band remains a recorded decision.
