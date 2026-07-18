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
                        plus the figmaFileKey + figmaNodeId capture inputs
      design-source/    the committed Figma REST image exports (two captured)

Each `manifest.json` frame carries: `frame` (name and design-source basename),
`fixture` (the committed Figma fixture the oracle imports and renders, `null`
when the frame has no renderable fixture yet), `band` (the tolerance rule),
`figmaFileKey` and `figmaNodeId` (the capture inputs — the Figma file and node
the design source is rendered from, `null` until authored), `designSource` (the
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
(`v08-wrap` 0.000 %, `v08-grid-spans` 0.116 %), `blur-falloff` (`v08-drop-shadow`
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

Six frames are measured today, each within its band:

- `v08-wrap` (`lowering-wrap.json`, node `1:10`, 420x184) — 0.000 %.
- `v08-grid-spans` (`grid-basic.json`, node `1:11`, 720x480) — 0.116 % over the
  whole frame; its five structural cells match the export pixel-exact, and its
  `hug me` TEXT cell renders through the text render path (#303). The residual is
  the Latin glyph substitution (the fixture authors `Inter`; the oracle renders
  Noto Sans) plus MSDF edges, inside the 2 % aa-edge budget.
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

One frame stays `pending-265`:

- `v08-baseline` — a font gap, not the render path. Its fixture authors the Latin
  leaves in `Inter`, which the committed corpus does not provide; rendered in
  Noto Sans the HUG root measures 621x160 against Figma's 608x160 (Noto Sans is
  wider than Inter), a dimension mismatch that cannot be diffed. It becomes
  measurable once it renders in its authored font — a committed Inter atlas, or a
  Noto-authored re-capture of node `1:2`. (Its arabic leaf, `Noto Sans Arabic`,
  is a committed font and would render faithfully.)

The last frame is tracked by the parked issue **#265**; the v0.9 exit gate (#49)
is where E7 flips from `partial` to `met`, once `v08-baseline` is measured too.
E7 is `partial`, not `met`, in `docs/specification/05-qualification.md`.

No design source may be fabricated, hand-drawn, or stood in for by the
project's own render. That is the exact self-oracle fidelity failure G-11
forbids — which is why a pending frame's `designSource` stays `null` rather than
holding a placeholder.

## Capturing a design source

The export step is `importers/figma/src/render_oracle.ts`, run as
`deno task oracle-capture`. It fetches Figma's own render of each authored
frame; it never draws one (G-11).

1. Set the frame's `figmaFileKey` and `figmaNodeId` in `manifest.json` — the
   Figma file the frame lives in and the node id rendered as the design source —
   and its `fixture` to the committed Figma fixture the oracle imports and
   renders.
2. From `importers/figma/`, with `FIGMA_TOKEN` exported, run
   `deno task oracle-capture`. For each frame that names both keys it calls the
   Figma REST render `GET /v1/images/:key?ids=<nodeId>&format=png&scale=1`,
   downloads the returned PNG into `goldens/oracle/design-source/<frame>.png`,
   and flips that frame's `designSource` to the committed path and its `status`
   to `captured`. A frame with either key `null` is skipped and stays
   `pending-265`; a non-200, a non-null `err`, a missing node, or a non-PNG
   download fails that frame and writes nothing. Commit the written PNG and the
   `manifest.json` update together.
3. Run the assertion: `cargo test -p goldens --test render_oracle`.
   Tune nothing to make it pass — if a frame fails its band, that is a measured
   fidelity gap to fix in the painter or a band to re-pin with review.
