# goldens/oracle — the design-source render oracle (E7 / G-11)

Every other golden in this repo diffs the `dashscene-skia` reference painter
against the project's own committed PNG — a self-oracle, which by construction
cannot see the painter drifting away from what a design actually looks like
(`docs/technotes/engineering-guardrails.md` G-23). This directory adds the
missing half of R6 (exit criterion E7): a perceptual diff of the reference
render against its **design source** — Figma's REST `GET /images` export — with
per-rule tolerance bands.

The reference is not a pre-committed corpus golden. Per frame the oracle imports
the committed Figma **fixture** (`corpus/figma-fixtures/<name>.json`), compiles
it in-process through `dashc`'s `compile_figma` (`Profile::Core`), loads the
emitted `.dsb`, re-solves it with the one `TaffySolver` — running the typesetter
measure seam so TEXT nodes size to their shaped extent (#303) — and renders the
committed scene with the Skia reference painter, sized to the root's solved
rect, with text painted from a glyph-run table staged over the committed Noto
atlases. That fresh render is diffed against the committed Figma export of the
same node at the same size. Importing the fixture and rendering it is exactly
what a producer does, so the diff measures the reference painter against Figma's
own render of the same scene.

    goldens/oracle/
      manifest.json     per-frame wiring: fixture -> design source -> band,
                        plus the figmaNodeId capture input
      design-source/    the committed Figma REST image exports (two captured)

Each `manifest.json` frame carries: `frame` (name and design-source basename),
`fixture` (the committed Figma fixture the oracle imports and renders, `null`
when the frame has no renderable fixture yet), `band` (the tolerance rule),
`figmaNodeId` (the node the design source is rendered from, `null` until
authored — the Figma file it lives in is not repeated here, see below),
`designSource` (the
committed export path, `null` until captured), `status` (`pending-265` until
captured, then `captured`), and optionally `excludeRegions` (a list of
`{ x, y, w, h }` rectangles whose pixels the diff drops from both the differing
count and the total — for one genuine, disclosed structural divergence that the
area budget must not silently absorb; see the per-frame `note`).

## The harness and the bands

The diff harness and the pinned bands are the `goldens::oracle` module
(`goldens/tooling/src/oracle.rs`). A pixel counts as differing only when its
largest per-channel absolute delta (0..=255) exceeds the band's `channel_delta`;
a frame passes when the differing fraction is at or below the band's
`differing_fraction`.

Fidelity is not one global budget (G-11): a hard rect edge, a blurred shadow's
soft falloff, and an MSDF glyph edge each disagree with the design source
differently, so each rule pins its own band:

| Band           | `channel_delta` | `differing_fraction` | Governs                            |
| -------------- | --------------- | -------------------- | ---------------------------------- |
| `aa-edge`      | 40              | 0.02                 | hard rect edges (E3 layout)        |
| `blur-falloff` | 24              | 0.12                 | soft shadow falloff (sigma=blur/2) |
| `msdf-text`    | 50              | 0.03                 | MSDF glyph edges (text)            |

The rationale for each value is in the module's rustdoc and in
`docs/design/goldens.md`. The values are pinned so the harness is falsifiable.
All three bands are now confirmed by real captures, none retuned: `aa-edge`
(`v08-wrap` 0.000 %, `v08-grid-spans` 0.037 %), `blur-falloff` (`v08-drop-shadow`
0.022 %, `v08-inner-shadow` 0.000 %), and `msdf-text` (`v05-text-latin` 0.033 %,
`v06-text-arabic` 1.405 %) — every measured frame inside its budget. A retune is a
deliberate, reviewed change — the band values are asserted in
`goldens/tooling/tests/render_oracle.rs`
(`the_three_rule_bands_are_pinned_and_distinct`), so a silent drift fails the
test.

## Measured now, and the pending follow-on

The design-source assertion
(`the_reference_renders_match_their_design_source`) runs in the ordinary `test`
job — un-gated. It is hermetic (committed fixture + committed export +
in-process compile, no network) and fast (~0.05 s/frame), and the `render-oracle`
CI job re-runs it with `--nocapture` so the per-frame numbers show in the log.

All seven frames are measured today, each within its band:

- `v08-wrap` (`lowering-wrap.json`, node `1:10`, 420x184) — 0.000 %.
- `v08-grid-spans` (`grid-basic.json`, node `1:11`, 720x480) — 0.037 % over the
  whole frame; its five structural cells match the export pixel-exact, and its
  `hug me` TEXT cell renders through the text render path (#303). The residual is
  MSDF glyph edges, inside the 2 % aa-edge budget. It measured 0.116 % until
  story #385 committed Inter and taught the cascade to match a family by name:
  the cell is authored in `Inter`, so until then it rendered in Noto Sans and the
  substituted letterforms were most of the difference. This is the frame that
  measures Inter fidelity against Figma's own render.
- `v08-drop-shadow` (`drop-shadow.json`, node `1:2`, 96x96) — 0.022 %, and
  `v08-inner-shadow` (`inner-shadow.json`, node `1:2`, 96x96) — 0.000 %. One
  shadowed card each (fixtures authored by the fixture-author plugin, #304); the
  first real measurement of `sigma = blur/2` against Figma, near-pixel-exact —
  the blur-falloff band.
- `v05-text-latin` (`text-latin.json`, node `1:2`, 480x200) — 0.033 %, and
  `v06-text-arabic` (`text-arabic.json`, node `1:2`, 520x240) — 1.405 %. Noto
  text authored in the committed atlas fonts (#304), rendered through the text
  path (#303) — the msdf-text band. The Arabic frame caught a real line-height
  bug (the typesetter took a line's height from the cascade's primary font);
  story #314 fixed it, bringing Arabic from 3.300 % to 1.405 %.

- `v08-baseline` (`text-baseline.json`, node `1:2`, 380x120) — 1.816 %. Three
  baseline-aligned Noto Sans runs at 12, 24 and 40. It first measured 3.807 %
  against this band's 3 % budget, and the cause was a real engine defect rather
  than a font gap: Taffy's high-level measure API reports no baseline for a
  leaf, so its flexbox falls back to the box bottom and a `BASELINE` row
  aligned box bottoms instead of glyph baselines (#272). A post-solve
  glyph-baseline
  correction fixed it and changed no golden.

  This frame was once blocked on the Inter gap — its Latin leaves were authored
  in `Inter`, which the corpus did not carry, and rendered in Noto Sans the HUG
  root measured 621x160 against Figma's 608x160, a dimension mismatch that
  cannot be diffed. It was re-captured Noto-authored instead, so it does not
  exercise Inter today; `v08-grid-spans` is the frame that does.

All seven frames are measured, so E7 is met and the v0.9 exit gate (#49) is
closed.

No design source may be fabricated, hand-drawn, or stood in for by the
project's own render. That is the exact self-oracle fidelity failure G-11
forbids — which is why a pending frame's `designSource` stays `null` rather than
holding a placeholder.

## Capturing a design source

The export step is `importers/figma/src/render_oracle.ts`, run as
`deno task oracle-capture`. It fetches Figma's own render of each authored
frame; it never draws one (G-11).

1. Set the frame's `figmaNodeId` in `manifest.json` — the node rendered as the
   design source — and its `fixture` to the committed Figma fixture the oracle
   imports and renders. The Figma file key is **not** set here: the capture tool
   joins the fixture's name against `corpus/figma-fixtures/manifest.json` and
   takes the key recorded there (debt #338). Recording it twice meant the
   fixture JSON and the design-source PNG could come from different files and
   the diff would be wrong by construction.
2. From `importers/figma/`, with `FIGMA_TOKEN` exported, run
   `deno task oracle-capture`. For each frame that names a `figmaNodeId` and
   whose fixture resolves to a file key it calls the Figma REST render
   `GET /v1/images/:key?ids=<nodeId>&format=png&scale=1`,
   downloads the returned PNG into `goldens/oracle/design-source/<frame>.png`,
   and flips that frame's `designSource` to the committed path and its `status`
   to `captured`. A frame with a null `figmaNodeId`, or whose fixture is absent
   from the corpus manifest or still carries its placeholder key, is skipped and
   stays `pending-265`; a non-200, a non-null `err`, a missing node, or a non-PNG
   download fails that frame and writes nothing. Commit the written PNG and the
   `manifest.json` update together.
3. Run the assertion: `cargo test -p goldens --test render_oracle`.
   Tune nothing to make it pass — if a frame fails its band, that is a measured
   fidelity gap to fix in the painter or a band to re-pin with review.

## The profile-preview oracle (story #435)

The third manifest here, and the only one with **no design source**. The other
two ask whether the reference painter agrees with Figma; this one asks what a
quality profile costs, by rendering each scene under RAW, then under HiFi and
LoFi, and diffing each production arm against RAW.

    goldens/oracle/
      profile-manifest.json    per scene and profile: the band, the rungs the
                               escalation chose, the measured numbers, and the
                               mutation that fails the band

Both arms are the same painter, the same solver, the same typesetter and the
same canvas, so a difference is the asset axis and nothing else — no export
pipeline and no resampling in the loop. It catches what the packer's per-asset
bands cannot: the asset **in context**, banding read behind a caption and block
boundaries read against a stroke.

The E7 gate's files and the import oracle's files are never read or written by
it. It reuses `goldens::oracle`'s diff and band **type**, and pins its own two
bands — `profile-hifi-scene` and `profile-lofi-scene` — which carry
`dashpack::profile`'s numbers exactly. The two band families deliberately do not
share a name space: a design-source frame graded against a codec band would fail
at a threshold of 2, and a scene graded against `blur-falloff` would pass at 24.

No frame is captured and there is no `status` field, because there is nothing to
capture: the reference arm is produced in the same run. The scenes are compiled
in process rather than committed — the only committed compiled document with an
image has a 16x16 image, which is one ASTC block at every footprint, so its
triptych renders byte-identically and could not fail anything.

`goldens/tooling/tests/profile_preview_oracle.rs` is the diff, and it writes the
triptych plus a diff heatmap per production arm to
`target/profile-preview/<scene>/` on every run (`just triptych`).
`goldens/tooling/tests/profile_preview_weld.rs` holds the premise the whole path
rests on. Full detail, including the measured table and each band's failing
mutation: `docs/design/goldens.md` and
`docs/decisions/profile-preview-decodes-in-the-loader.md`.

## The import-fidelity oracle (issue #332)

The full real-file-import epic ends at a quantitative question: does an
**imported** file render inside a measured band of Figma's own render? Its two
real targets are third-party Community files, and
`docs/decisions/figma-corpus-self-authored-only.md` forbids committing their
JSON or their render — they are checked live only (`just render`). The
committed, license-clean half lives beside the E7 oracle, deliberately
separate from it:

    goldens/oracle/
      import-manifest.json     per-frame wiring, same shape as manifest.json
                               (status pending-332 until captured)
      import-design-source/    the committed Figma REST exports for the
                               import frames

The E7 gate's files — `manifest.json`, `design-source/`, the
`render_oracle.rs` test — are never read or written by the import oracle. The
diff harness and the three pinned bands (`goldens::oracle`) are reused
**read-only**; a band is never retuned here. The reference render is
`goldens::render::render_dsb` — the Sf-1 production path — because unlike the
E7 test's own stager it paints embedded image-fill bytes and honors the
lowered text axes, the two capabilities these frames exist to measure. The
assertion is `goldens/tooling/tests/import_oracle.rs`; the capture step is
`deno task import-oracle-capture` (the same export mechanism as
`oracle-capture`, pointed at the import manifest).

Two self-authored frames cover the two vocabulary paths the real import
proved live but no E7 frame measures, both measured within their band:

- `import-image-fill` (`import-image-fill.json`, node `1:2`, 400x200) —
  **0.329 %** on `aa-edge`. One frame whose only paint is an IMAGE fill
  (scaleMode `FILL`) of a self-generated 380x380 PNG — gradients, two
  hard-edged rectangles, and a semi-transparent square — embedded into the
  `.dsb` at compile and decoded by the painter. The 380x380 image in the
  400x200 box means `FILL` scales the paint up to cover and crops it, so the
  measurement includes the scale-and-crop path. The first committed
  measurement of the image decode -> embed -> paint path against Figma; the
  residual is rect-edge anti-aliasing and sub-threshold resampling noise.
- `import-text-axes` (`import-text-axes.json`, node `2:2`, 400x200) —
  **1.029 %** on `msdf-text` (1.829 % until #336 dropped the trailing
  letter-spacing step from the measured width, PR #372). One Noto Sans
  Regular 24 TEXT node exercising
  the #310 axes end-to-end: PIXELS line height 18, letter spacing 1.2, RIGHT +
  BOTTOM alignment in a fixed box larger than its content. This frame caught
  two real bugs on first measurement (the G-11 pattern finding real bugs
  again, after #314 and #272): an absent `textAutoResize` mis-lowered as
  auto-size
  (`dashc`), and a fixed line height placing the baseline at the full
  intrinsic ascent instead of centering the intrinsic box (half-leading,
  `dashscene-typeset`) — together first measured 2.822 %, structurally
  misplaced. Fixed, the run lands where Figma renders it. The residual was
  then glyph edges plus ~1 px of horizontal placement from the trailing
  letter-spacing step Figma excludes from the measured width (#336); once
  that step was dropped (PR #372) the residual is glyph edges alone.
