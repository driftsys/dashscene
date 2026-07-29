# goldens — the golden-image diff tooling and the v0.1 harness

    crate    goldens/tooling (package `goldens`)
    covers   v0.1 golden harness — the v0.1 slice's exit gate (story #6)
             + the v0.2 flex-vocabulary goldens (story #11)
             + the v0.3 paint-vocabulary golden (story #14)
             + the per-family paint goldens (story #18)
             + the subtree-clip golden (story #97)

## Purpose

`goldens` is the harness `docs/specification/05-qualification.md` names
as the v0.1 exit gate and `docs/technotes/rendering-and-painters.md` as
how CPU painters generate their own goldens: a scene
authored in the Rust DSL (`dashlang`), committed through
`dashscene-core`, painted by the Skia reference painter
(`dashscene-skia`), and compared pixel by pixel against a checked-in PNG — on
every `cargo test --workspace` run, with no recipe or CI wiring beyond
the workspace member. It is the harness every later slice re-goldens
against on a painter swap (`docs/technotes/rendering-and-painters.md`).

An unpublished workspace member at `goldens/tooling`; the checked-in
images live in `goldens/images/`.

## Public interface

All in `goldens/tooling/src/lib.rs`:

    pub fn assert_matches_golden(name: &str, png_bytes: &[u8])
    pub fn assert_matches_golden_within(name: &str, png_bytes: &[u8], max_differing_fraction: f64)
    pub fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4]

`assert_matches_golden` compares a render against the checked-in golden
`goldens/images/{name}.png` bit-exactly; `assert_matches_golden_within`
allows a bounded fraction of pixels to differ, for anti-aliased content
that is not bit-identical across CPU architectures (story #14; see
`docs/decisions/golden-comparison-space.md`). Their exact behavior
(update mode, failure artifacts, panics) is their rustdoc. `pixel` is
the shared RGBA8888 pixel-indexing helper golden tests use. The full workflow — running,
regenerating, inspecting a failure — is documented in
`goldens/README.md`, a shipped doc and the story's own acceptance
criterion; the comparison-space choice is
`docs/decisions/golden-comparison-space.md`. Neither is repeated here.

    pub mod oracle {
        pub fn diff(reference_png: &[u8], design_source_png: &[u8], band: &ToleranceBand) -> Result<OracleDiff, String>
        pub struct OracleDiff { differing: usize, total: usize, max_channel_delta: u8 }
        pub struct ToleranceBand { rule: &'static str, channel_delta: u8, differing_fraction: f64 }
        pub fn band_for(rule: &str) -> Option<&'static ToleranceBand>
        pub const AA_EDGE: ToleranceBand
        pub const BLUR_FALLOFF: ToleranceBand
        pub const MSDF_TEXT: ToleranceBand
    }

`goldens::oracle` (story #284) is the design-source render oracle — a
second, distinct diff path from `assert_matches_golden*` above. Its bands,
manifest, and gate are documented in their own section below.

## Golden scene (fixture)

`goldens/tooling/tests/v01.rs` builds one 64×64 scene through
`dashlang` exercising the whole v0.1 vocabulary (integer-aligned,
opaque colors); the element list lives in that file's comments, its one
home. Three direct pixel assertions — derived from the fixture colors
by the painter's own quantization — pin stacking, nesting, and dedup
independently of the image file.

`goldens/tooling/tests/v02_flex.rs` (story #11) adds four scenes, one
per v0.2 flex construct — nesting, sizing (hug-in-fill plus the equal
`Fill` split), clamping (min/max beats the flex distribution), and
alignment (every `MainAxisAlign`/`CrossAxisAlign` pairing). `dashlang`
is not used: it has no flex vocabulary
(`docs/decisions/negative-gap-lowering.md` D3, tracked as #118), so
each scene is authored directly against `dashscene-core`'s `Txn` and
solved by `dashscene-engine`'s `TaffySolver` via `commit_with`. Every
scene is dimensioned so each solved rect lands on an integer, so all
four goldens (`v02-nesting.png`, `v02-sizing.png`, `v02-clamping.png`,
`v02-alignment.png`) compare exact-match; each test also asserts its
solved rects before comparing the image. Granularity and scope
(fill weights are out of scope, tracked as #117) are
`docs/decisions/v02-flex-goldens-per-construct.md`.

`goldens/tooling/tests/v03.rs` (story #14) adds a 96×96 scene built
directly at boundary B (no producer stages the v0.3 vocabulary yet)
covering every paint kind on one canvas — all four gradient kinds,
stroke align, rounded corners, and every image scale mode against a
hand-rendered checker asset. Its gradients and curves are
anti-aliased, so it compares with a 1% differing-pixel tolerance
(cross-machine edge jitter); per-kind pixel semantics live in
`crates/dashscene-skia/tests/painter.rs` as bit-stable interior probes;
this golden pins the full rendering (`v03-paint.png`).

`goldens/tooling/tests/v03_families.rs` (story #18) adds three further
64×64 scenes, each isolating one construct family — gradients, strokes,
and images (`v03-gradients.png`, `v03-strokes.png`, `v03-images.png`) —
so a regression fails only the affected family's golden; scope and
tolerance are `docs/decisions/v03-paint-goldens-per-family.md`.

`goldens/tooling/tests/v03_clips.rs` (story #97) adds the clip golden
`v03-clips.png` — the one v0.3 scene authored through
`dashscene-core`'s producer API rather than hand-built at boundary B,
because clipping is the one construct a painter cannot be handed
directly: the ancestor relation exists only producer-side, and commit is
what resolves it (`docs/decisions/resolved-clip-regions-at-commit.md`).
Its four 64×64 panels cover a rounded clipping frame, a sharp clipping
frame that draws nothing itself, a nested sharp∩rounded chain, and an
unclipped control. Rounded clips are anti-aliased, so it compares at the
same 2% tolerance as the family goldens, with flat-interior probes
pinning each panel bit-stably.

The v0.8 masks + group-opacity goldens (story #44) are authored the same
way — through `dashscene-core`, since a mask and a group opacity are also
producer-side relations (`docs/decisions/masks-and-group-opacity.md`).
`v08-mask.png` is a rounded mask stenciling an oversized fill (the mask's
own color must not show); `v08-group-opacity-free.png` is two
non-overlapping children under a 0.5 group (the free path — no render
target); `v08-group-opacity-rt.png` is two overlapping children under a
0.5 group (the render-target path — the overlap is no darker than a single
child, which is what proves the subtree flattened before its alpha
applied). Each pins its distinguishing property with relational probes and
compares at the 2% tolerance.

The v0.8 shadow goldens (story #45,
`docs/decisions/effects-vocabulary-shadows.md`) are authored through
`dashscene-core` (`Prop::Shadows`) too. `v08-drop-shadow.png` is a rounded
amber card casting a soft drop shadow onto navy; `v08-inner-shadow.png` is a
rounded near-white panel with an inner shadow ringing its inside edges. A
blurred shadow is anti-aliased, so each compares at the 2% tolerance — but a
2% budget (~82 px on the 64×64 canvas) cannot on its own prove the golden
pins the shadow, so each test adds a sensitivity guard: it renders the same
scene with the shadow removed and asserts the two renders differ by far more
than the budget (1159 px for the drop, 748 px for the inner). That is the
demonstrated-sensitivity discipline `docs/decisions/golden-comparison-space.md`
requires; relational probes (the card fill unchanged behind its shadow, the
inner shadow darker at the edge than the center, the background untouched)
add exact machine-independent checks on top. `v08-stacked-shadows.png`
stacks two semi-transparent hard-edge drop shadows (a backmost blue and a
front red) and probes their overlap: it is red-over-blue only in Figma's
back-to-front `effects` order, so the probe flips and fails if the draw
loop reverses — the golden pins the stacking order, not just the presence
of a shadow.

`goldens/tooling/tests/v013_uncovered_shapes.rs` (issues #501, #495) adds
three v0.13 frames for behaviours that had no committed artifact at all —
each was a landed fix that changed real output while the whole golden and
oracle suite stayed green, because the shape appeared in no scene.

- `v013-hug-negative-margin.png` — a `Hug` column of four rows: three
  `Hug` rows over a `Hug` child with a negative main-axis margin (`0` as
  the control, then `-1` and `-16`), plus a fixed-width guard row where
  the #270 shrink factor must not apply. Integer-dimensioned, so it
  compares exact-match.
- `v013-baseline-hug-cross.png` — a HUG cross-axis `Baseline` row holding
  a 100-tall box, a 40 px text run and a `Fill` cross-sized child, with a
  following sibling underneath (#322). The text is anti-aliased MSDF, so
  it compares against a 400 px absolute budget.
- `v013-mask-effect-bleed.png` — two panels, each a mask larger than its
  maskee and a maskee whose hard-edged drop shadow reaches past the
  maskee's own box (#495). That overhang is where the two readings of the
  G-7 mask-bounds ruling produce different pixels: the landed reading
  shows it, the rejected one cuts it at the maskee edge. The left panel's
  parent does not clip; the right panel's does, and its box bounds x while
  the mask bounds y, so both boxes are visible in the picture.

Two of the three carry their sensitivity structurally: the canvas is sized
from the solved root, so reverting either layout fix renders a differently
sized image and the golden fails on its dimension check before any budget
applies. The mask frame compares exact-match and adds the explicit
sensitivity guard — a twin scene built as the rejected reading, asserted to
move far more than jitter could hide (measured: 1280 px of 18 432).

These are self-oracle frames, so they cover the producer surface, the
solver, commit-time resolution and paint — not the Figma lowering or the
`.dsb` round trip. Covering those as well would need a captured fixture per
shape, and authoring one is a manual Figma step
(`corpus/figma-fixtures/README.md`), so it is a separate, human-gated piece
of work rather than a residual of these frames.

## Design-source render oracle (E7 / G-11)

Every golden above diffs `dashscene-skia`'s render against the project's
_own_ previously committed PNG — a self-oracle, which by construction
cannot see the painter drifting away from what a design actually looks
like (`docs/technotes/engineering-guardrails.md` G-23). Story #284 adds a
second, distinct diff path: `goldens::oracle` (`goldens/tooling/src/oracle.rs`)
perceptually diffs a reference render against its **design source** —
Figma's REST `GET /images` export — decoded in the same unpremultiplied
RGBA8888 comparison space the self-oracle goldens use
(`docs/decisions/golden-comparison-space.md`). This is the tooling for
exit criterion E7 (`docs/specification/05-qualification.md`), the
falsifiable form of R6 that guardrail G-11 names.

`diff(reference_png, design_source_png, band)` counts a pixel as differing
only when its largest per-channel absolute delta exceeds the band's
`channel_delta`, and returns an `OracleDiff` carrying the measured
`differing` count, the `total` pixel count, and the largest per-channel
delta seen at any pixel — a result is a measured number, never a bare
pass/fail (G-11). `OracleDiff::passes(band)` checks the differing fraction
against `band.differing_fraction`. A dimension mismatch between the two
images is an `Err` naming both sizes, never a silent pass.

### Per-rule bands, not one global budget

G-11 requires per-rule tolerances: a hard rect edge, a blurred shadow's
soft falloff, and an MSDF glyph edge each disagree with a design-source
export differently, so one global budget would either reject a correct
blur or accept a broken edge. Three bands are pinned
(`goldens/tooling/src/oracle.rs`), each asserted exactly in
`goldens/tooling/tests/render_oracle.rs`
(`the_three_rule_bands_are_pinned_and_distinct`) so a retune is a
deliberate, reviewed change rather than a silent drift:

- **`AA_EDGE`** — `channel_delta = 40`, `differing_fraction = 0.02`. A hard
  rect edge anti-aliased against the design source disagrees on a thin
  1–2 px band, where the reference painter's coverage rounding and Figma's
  server-side export resampling can swing far apart per pixel. The
  fraction budget is the primary tolerance (an edge is a small share of
  the canvas); `channel_delta` filters sub-threshold interior noise.
  Governs the E3 exact-layout frames (wrap, grid spans, baseline).
- **`BLUR_FALLOFF`** — `channel_delta = 24`, `differing_fraction = 0.12`. A
  blurred shadow spreads a small per-pixel disagreement across a wide
  falloff region — many pixels off by a little. The `sigma = blur / 2`
  mapping (`docs/decisions/effects-vocabulary-shadows.md`) is an
  approximation of Figma's blur, so the whole falloff can be
  systematically off by a small amount; a wider fraction with a moderate
  per-pixel threshold pins "the falloff shape is close" without demanding
  pixel identity. Governs the drop- and inner-shadow frames, and is the
  band that will pin `sigma = blur / 2` against a real capture once #265
  lands.

  **It is the one band that also carries a gate** — `channel_delta = 40`,
  `differing_fraction = 0.01` — added at the ruling on issue #422. Its
  12 % budget is sound as a residual and could not work as a gate: a blur
  defect is a bounded-area error, so removing the effect outright still
  measured only 4.351 % (`v08-drop-shadow`) and 3.570 %
  (`v08-inner-shadow`), and both passed. The gate is on a different axis —
  a high threshold with a narrow budget, because a removed effect leaves
  few pixels but grossly wrong ones, where a falloff approximation leaves
  many pixels slightly wrong. Both terms bind and neither is redundant:
  the amplitude mutation recorded in
  `docs/technotes/2026-07-26-tolerance-band-coverage.md` fails the
  residual at 23.559 % while measuring 0.422 % at the gate's threshold.
  A frame passes only when it is inside both.
- **`MSDF_TEXT`** — `channel_delta = 50`, `differing_fraction = 0.03`. MSDF
  glyph edges are sharp, high-contrast transitions; the reference
  painter's MSDF resolve and Figma's font rasterizer disagree at glyph
  boundaries (hinting, gamma). Text ink is sparse, so a small fraction
  with a higher per-pixel threshold pins the glyph shapes without
  over-tolerating. Governs the text frames (Arabic, Latin).

These are engineering estimates from the AA/blur/MSDF edge
characteristics, pinned so the harness is falsifiable. All three bands are
now confirmed by real captures, none retuned: `AA_EDGE` (`v08-wrap`
0.000 %, `v08-grid-spans` 0.037 %), `BLUR_FALLOFF` (`v08-drop-shadow`
0.022 %, `v08-inner-shadow` 0.000 %), and `MSDF_TEXT` (`v05-text-latin`
0.033 %, `v06-text-arabic` 1.405 %) — every measured frame inside its
budget. Both `BLUR_FALLOFF` frames also measure 0.000 % at the gate's
threshold, so the gate added at #422 has its whole budget as headroom
rather than a share of an existing residual.

### The corpus-frame ↔ design-source manifest

`goldens/oracle/manifest.json` wires each corpus frame to the committed
Figma **fixture** the oracle imports and renders, the band that governs
it, and its design-source slot: `v08-wrap`, `v08-grid-spans`,
`v08-baseline` on `aa-edge`; `v08-drop-shadow`, `v08-inner-shadow` on
`blur-falloff`; `v06-text-arabic`, `v05-text-latin` on `msdf-text`. Six
frames carry a committed `designSource` (status `captured`); only
`v08-baseline` carries `null` and `pending-265`.
`goldens/tooling/tests/render_oracle.rs`'s manifest-consistency tests run
in the ordinary `test` job and assert every frame names a known band and,
when it declares a fixture, one that exists, and that a frame with no
design source is honestly marked `pending-265` — an assertion that checks
each frame's own state rather than "all frames are pending".
`goldens/oracle/README.md` documents the capture procedure.

### Measured now, and the pending follow-on

The assertion `the_reference_renders_match_their_design_source`
(`goldens/tooling/tests/render_oracle.rs`) imports each captured frame's
committed fixture, renders it, and diffs the render against the committed
Figma export within the frame's band. It is un-gated — hermetic (committed
fixture + committed export + in-process compile, no network) and fast — so
it runs in the ordinary `test` job, and its accounting asserts every frame
is measured or pending so none is silently dropped. The `render-oracle` CI
job (`.github/workflows/ci.yml`) re-runs the suite with `--nocapture` so
the measured per-frame numbers show in the log, and is wired into the `ci`
aggregate `needs`.

The oracle has **no text stager of its own** (story #542). It commits
through `TaffySolver::with_text`, so its glyph runs are the committed
scene's own — the one producer every other render path uses
(`docs/decisions/glyph-runs-cross-boundary-b.md`, "The producer story,
decided"). Measure and paint cannot diverge inside the instrument that
judges fidelity, because one commit does both.

That closed two divergences this file previously recorded. Runs now wrap at
the node's solved box width rather than at no width at all (issue #306):
staging at no width lets a width-fixed node be measured as N lines and
staged as one overflowing line. And the oracle no longer stages under
`TextShape::default()` while `goldens::render` staged under the node's
lowered axes — there is one policy, the lowered one, which is also what the
measure callback already used.

Neither move cost a re-baseline: **all seven frames hold their residual to
three decimals**, because every TEXT node in the seven fixtures hugs both
axes and authors `INTRINSIC_%` line height, zero letter spacing, and
LEFT/TOP alignment — so its lowered axes already _are_ the defaults and its
solved width already _is_ its shaped width. A future fixture authoring a
fixed line height, letter spacing, a non-Top vertical alignment, or a
width-fixed wrapping frame would move its frame, and that is the point at
which the cost would have appeared had it not been taken here.

The width-fixed case is pinned by
`a_width_fixed_text_node_stages_the_lines_the_measure_seam_wrapped`, on a
scene authored directly against `dashscene-core`, because a width-fixed
wrapping Figma frame would need a hand-authored file and a captured design
source, which G-11 does not allow to be fabricated.

All seven frames are measured today, each within its band. `v08-grid-spans` declares no
exclusion: with the text render path wired (#303) its `hug me` TEXT leaf sizes to
the shaped text instead of collapsing to 0x0, so the grid solves as Figma laid it
out — the whole 720x480 frame diffs 0.037 % (0.116 % before story #385 committed
Inter and matched the family by name; the cell is authored in `Inter`). The two shadow frames
(`v08-drop-shadow` 0.022 %, `v08-inner-shadow` 0.000 %) and the two text frames
(`v05-text-latin` 0.033 %, `v06-text-arabic` 1.405 %) render from fixtures the
fixture-author plugin builds (#304): the shadows pin `sigma = blur/2` against
Figma's own render, and the Noto text renders through the text path (#303).
`v06-text-arabic` caught a real line-height bug — the typesetter took a line's
height from the cascade's primary font — that story #314 fixed, bringing it from
3.300 % to 1.405 %. The `excludeRegions` mechanism (`oracle::diff_excluding`,
excluded pixels leaving both the numerator and the denominator) stays available
for a genuine structural divergence, though no frame declares one today.

The third text frame, `v08-baseline`, is a mixed-size baseline row — three
Noto Sans runs (`small` 12, `medium` 24, `LARGE` 40) baseline-aligned in a fixed
380x120 frame — replacing the earlier Inter-authored fixture whose HUG root
resized under a substituted font and could not be diffed. It caught a second real
bug the self-oracle goldens missed: the diff first measured 3.807 % against the
msdf-text band's 3 % budget because Taffy's high-level measure reports no
baseline for a leaf, so a text
leaf aligned on its box bottom, not its glyph baseline — the shorter runs sat a
descender too low (#272). A post-solve glyph-baseline correction in
`dashscene-engine` brought it to 1.816 %, measured in the `msdf-text` band because,
once the layout is correct, the residual is glyph edges and the small
reference-vs-Figma ascent-metric difference, the same nature as the other two text
frames; the baseline geometry itself is proven exactly by an engine unit test. No
design source may be fabricated, hand-drawn, or stood in for by the project's own
render; that is the exact self-oracle failure G-11 forbids, which is why an
un-captured frame's `designSource` stays `null`. With all seven frames measured,
E7 is **met** in `docs/specification/05-qualification.md`; the v0.9 exit gate (#49)
asserts it in CI alongside `E1`–`E6`.

## Profile-preview oracle (story #435)

The third manifest in the same pattern, and the only one with **no external
design source**: it renders each scene under RAW, then under HiFi and LoFi, and
diffs each production arm against RAW. Both arms are the same painter, the same
solver, the same typesetter and the same canvas, so the only variable is which
bytes the asset entries resolve to and a difference is the asset axis and
nothing else. That is a purer measurement than any comparison against an export,
which has to absorb rasterizer, resampling and gamma disagreement.

It exists because the packer's per-asset bands
(`crates/dashpack/tests/band_contract.rs`) measure texels in isolation and are
blind to the asset **in context** — banding read behind a caption, a block
boundary read against a stroke.

    goldens/oracle/
      profile-manifest.json     per scene: the canvas, the assets, and per
                                profile the band, the rungs the escalation
                                chose, the measured numbers, and the mutation
                                that fails the band

    goldens/tooling/tests/profile_preview_oracle.rs   the diff
    goldens/tooling/tests/profile_preview_weld.rs     the weld
    target/profile-preview/<scene>/                   the triptych and heatmaps

### How a derived scene renders at all

Under a production profile an asset's resident payload is a block-compressed
KTX2 file, which no image codec decodes. `goldens::profile::derive` packs a
document's assets under a profile and reassembles the file; `goldens::render`
then software-decodes each block payload back to RGBA with the same
version-pinned astcenc that encoded it, and re-wraps it losslessly as a PNG
before the painter sees it. **The painter is unchanged** and still only draws
RGBA, so P2 holds — the decode is the loader's, not the painter's
(`docs/decisions/profile-preview-decodes-in-the-loader.md`).

Both sides are behind the `profile-preview` feature, on by default so the
workspace suite covers them. With it off the harness still renders RAW and
refuses a block payload by name.

### The two scene bands

`PROFILE_HIFI_SCENE` and `PROFILE_LOFI_SCENE` carry `dashpack::profile`'s own
numbers exactly — 2 and 1 %, 8 and 5 % — and
`the_scene_bands_are_the_packers_bands` asserts that equality, so retuning a
pack band cannot silently leave the scene band behind. The profile's promise is
a per-asset band, and this oracle asks whether the profile keeps that promise
once the asset is composited.

They are deliberately **not** reachable from `band_for`, and the three
design-source bands are not reachable from `profile_band_for`
(`the_two_band_families_do_not_share_a_name_space`). One name space would let a
design-source frame be graded against a codec band, which at a threshold of 2
fails every frame, or a scene be graded against `blur-falloff`, which at 24
passes anything.

The design-source thresholds are 24 to 50 because they compare a CPU rasterizer
against a server-side export. Nothing here disagrees except the codec: HiFi's
whole-scene residual on `profile-photo` has a maximum per-channel delta of 3, so
every design-source threshold would report it as a perfect match.

### Every band ships the mutation that fails it

Issue #422 measured that a budget chosen in advance and never exercised is not a
gate. Each row in the manifest therefore carries the measured defect that
breaches its band — an escalation that stopped one rung early, built out of the
same public API the packer uses — and the oracle re-measures it every run and
asserts it **fails**:

| scene            | profile | rungs        | measured | mutation     | mutation measures |
| ---------------- | ------- | ------------ | -------- | ------------ | ----------------- |
| `profile-photo`  | HiFi    | 6x6, 8x8     | 0.2043 % | force 8x8    | 2.6627 %          |
| `profile-photo`  | LoFi    | 12x12, 8x8   | 0.0000 % | none, stated | —                 |
| `profile-stress` | HiFi    | uncompressed | 0.0000 % | force 4x4    | 51.8097 %         |
| `profile-stress` | LoFi    | 6x6          | 4.5166 % | force 8x8    | 9.7733 %          |

`profile-photo`'s LoFi row has no mutation and says so: that gradient survives
the cheapest rung on the ladder, so LoFi accepts 12x12 on its first attempt and
there is no coarser rung to stop at. `every_band_is_exercised_by_at_least_one_scene`
closes the loophole that would let every row make that excuse.

Three numbers per row are asserted exactly — the differing count, the fraction,
and the **maximum per-channel delta**. The last one is the knob an area budget
cannot supply: a budget cannot see a small number of pixels going badly wrong,
which is issue #422's finding in its general form. `profile-stress` under HiFi
escalates to the lossless rung, so its recorded maximum is exactly 0 — the
lossless identity proven through the whole chain, where any step altering one
texel moves it off zero.

The rungs the escalation chose are asserted too. A band says the scene still
looks right; the rung list says it looks right _for the recorded reason_, so a
packer that changed rung and happened to stay inside the band cannot pass with a
manifest that has quietly become fiction.

### Scenes are built in process

Neither scene is a committed fixture. `goldens/dsb/v03-paint.dsb` is the only
committed compiled document with an image, and that image is 16x16 — one ASTC
block at every footprint on the ladder — so all three arms of its triptych render
byte-identically and it cannot fail anything. `profile-photo` composes the
committed 380x380 `import-image-fill` payload with a caption, a stroke and a
second committed image as a badge, so no asset index in it is 0.
`profile-stress` generates its content from a deterministic integer hash, for
the reason story #432 recorded when it generated `detail-noise`: no committed
payload separates the two profiles' area budgets. Its amplitude was chosen by
measurement — at 4 the LoFi ladder still bottoms out and at 16 both profiles go
lossless.

### The triptych

Every run writes `raw.png`, `hifi.png`, `lofi.png` and a `-heat.png` beside each
production arm into `target/profile-preview/<scene>/`, and prints the banded
numbers. `just triptych` runs exactly that. A heatmap is scaled so the largest
delta present maps to white, with the scale factor printed beside it, because
these residuals are small enough that an unscaled map is a black square.

They are written rather than committed: a committed render of a scene whose
purpose is to show codec loss would need re-baselining for every unrelated
painter change, and the numbers above are the durable record.

### `just render --profile`

`render-dsb <in.dsb> <out.png> --profile raw|hifi|lofi` gives a designer the
same view of any imported file, and `just render <key> <root> <profile>` drives
it live. An unrecognised profile name is reported with the set that is accepted
and never resolved to a default.

### What a desk preview cannot show

Repeated wherever the preview is documented so a target bench confirms a short
list rather than discovering quality: GPU filtering behaviour, driver-level
effects (vendor bandwidth compression such as UBWC, and the NVIDIA case where
ASTC is emulated rather than sampled natively — the pack-time probe's job), and
where in a target pipeline the sRGB transfer function is applied.

## Testing

Unit tests in `src/lib.rs` cover the tooling's edge behavior against a
temporary, injected images root, so they exercise the panic and
actual-file paths without touching the repository's checked-in
goldens: matching pixels pass (clearing any stale failure artifact);
differing pixels and dimension mismatches panic with a report and write
the actual image; a missing golden panics naming the `UPDATE_GOLDENS`
workflow; a corrupt golden names itself rather than the render. (The
encoding-drift pass-with-note branch is currently exercised by no unit
test — constructing two byte-different encodings of identical pixels
from one pinned encoder is not practical; the branch exists for skia
version bumps.) `tests/v01.rs`
against the committed `goldens/images/v01-walking-skeleton.png` is the
harness's own acceptance path — a clean-checkout `cargo test` passing
against that image is the exit criterion itself.

## Trace

- Satisfies: issue #6 acceptance criteria;
  `docs/specification/05-qualification.md`'s v0.1 slice exit ("golden
  harness"), `docs/technotes/rendering-and-painters.md` (CPU painters
  generate their own goldens); issue #11's v0.2 flex goldens; issue
  #14's v0.3 golden; issue #97's clip golden; issue #284's design-source
  render oracle tooling (exit criterion E7, guardrail G-11); story #435's
  profile-preview oracle and weld (epic #345).
- Closes epic #1's story list (v0.1 walking skeleton, milestone 1).
- Closes epic #7's story list (v0.2 flex core) — issue #11 was its last
  open story.
- Related decisions: `docs/decisions/profile-preview-decodes-in-the-loader.md`,
  `docs/decisions/golden-comparison-space.md`
  (comparison space; resolves debt #86);
  `docs/decisions/reference-painter-antialiasing.md` (sub-pixel
  geometry policy; resolves debt #85, story #14 — anti-aliasing is on
  for every draw, and the v0.1 golden's unchanged pass is that
  decision's regression proof);
  `docs/decisions/v02-flex-goldens-per-construct.md` (v0.2 flex golden
  granularity and scope, story #11);
  `docs/decisions/render-oracle-tolerance-and-gating.md` (the
  design-source render oracle's per-rule bands, real-export-only rule, and
  #[ignore]-gated assertion, story #284).
