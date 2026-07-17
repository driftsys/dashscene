# goldens/oracle — the design-source render oracle (E7 / G-11)

Every other golden in this repo diffs the `dashscene-skia` reference painter
against the project's own committed PNG — a self-oracle, which by construction
cannot see the painter drifting away from what a design actually looks like
(`docs/technotes/engineering-guardrails.md` G-23). This directory adds the
missing half of R6 (exit criterion E7): a perceptual diff of the reference
render against its **design source** — Figma's REST `GET /images` export — with
per-rule tolerance bands.

    goldens/oracle/
      manifest.json     per-frame wiring: reference golden -> design source -> band
      design-source/    the committed Figma REST image exports (PENDING #265)

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
`docs/design/goldens.md`. The values are pinned so the harness is falsifiable
now; the first real captures (#265) and the v0.9 exit gate (#49) confirm or
retune them. A retune is a deliberate, reviewed change — the band values are
asserted in `goldens/tooling/tests/render_oracle.rs`
(`the_three_rule_bands_are_pinned_and_distinct`), so a silent drift fails the
test.

## The #265 gate

The real design-source images are authored manually — a Figma REST `GET /images`
export of each corpus frame's Figma file — and are tracked by the parked
manual-Figma-authoring issue **#265**. Until they land:

- Every `manifest.json` frame's `designSource` is `null` and its `status` is
  `pending-265`.
- The assertion that a frame's render matches its export
  (`the_reference_renders_match_their_design_source`) is `#[ignore]`-gated with a
  named #265 reason. It does not run in the ordinary `test` job.
- The `render-oracle` CI job runs the gated assertion with `--ignored`; with no
  committed design source it reports every frame as pending #265 and measures
  nothing — a loud pending summary, never a silent green.
- E7 stays **open (tooling landed)** in `docs/specification/05-qualification.md`,
  not `met`. The v0.9 exit gate (#49) is where E7 is asserted.

No design source may be fabricated, hand-drawn, or stood in for by the
project's own render. That is the exact self-oracle fidelity failure G-11
forbids.

## Adding a design source (when #265 lands)

1. Export the frame from its Figma file via the REST `GET /images` endpoint at
   the same pixel dimensions as the reference golden.
2. Commit it as `goldens/oracle/design-source/<frame>.png`.
3. Set the frame's `designSource` in `manifest.json` to that path and its
   `status` to `captured`.
4. Run the gated assertion: `cargo test -p goldens --test render_oracle -- --ignored`.
   Tune nothing to make it pass — if a frame fails its band, that is a measured
   fidelity gap to fix in the painter or a band to re-pin with review.
