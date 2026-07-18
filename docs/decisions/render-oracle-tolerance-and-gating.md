# The design-source render oracle uses per-rule bands and a fixture-import reference

    status   accepted (story #284, 2026-07-17; choices 3 and 4 revised at
             E7 productionization, 2026-07-18)
    scope    goldens/oracle, goldens/tooling; exit criterion E7, guardrail
             G-11; binds the v0.9 exit gate (#49) and the manual-capture
             work (#265)

## Context

Every golden in this repo diffs the `dashscene-skia` reference painter
against the project's own previously committed PNG — a self-oracle, which
by construction cannot see the painter drifting away from what a design
actually looks like (`docs/technotes/engineering-guardrails.md` G-23,
G-11). Exit criterion E7 (`docs/specification/05-qualification.md`) adds
the missing half of R6: a clean file must render within tolerance of its
**design source** — Figma's REST `GET /images` export.

Story #284 delivers the E7 tooling, not the v0.9 exit assertion (#49).
The tooling needed four choices settled: how to size the tolerance, what
to diff the render against, whether a design source may be stood in for,
and how to ship the real-capture assertion while the real captures do not
yet exist. The real captures are authored manually and tracked by the
parked issue #265.

Productionizing E7 revised two of those choices once the first real
captures existed: the render side is now a fresh import-and-render of the
committed fixture (choice 3, revised from 3b to 3a), and the assertion runs
un-gated for captured frames (choice 4, revised from the tooling-phase
gate). Choices 1 and 2 stand. The revised text below records the as-built
system; the tooling-phase choice it superseded is named in each `Why`
bullet.

## Options

1. **Tolerance shape.** (a) One global perceptual budget for every frame,
   or (b) a per-rule band, each rule pinning its own `channel_delta` and
   `differing_fraction`.
2. **Design source.** (a) Fabricate a stand-in — hand-draw an export or
   reuse the project's own golden — so the assertion runs today, or (b)
   diff only against a real Figma REST export, and mark each frame pending
   until one is captured.
3. **What the render side is.** (a) Re-render each corpus frame fresh
   inside the oracle, or (b) diff the committed reference golden that the
   self-oracle goldens already prove equals the fresh render.
4. **Shipping the assertion.** (a) A CI job that fails (red) until #265
   lands, or (b) an `#[ignore]`-gated assertion with a named #265 reason,
   run with `--ignored` by an authored job that reports each pending frame.

## Choice

1b — per-rule bands. 2b — real export only, pending a capture. 3a — import
and render the committed fixture fresh in the oracle (revised from 3b at
productionization). 4 — un-gated assertion once a real capture exists
(revised from the tooling-phase 4b gate).

## Why

- **Per-rule bands (1b).** G-11 requires per-rule tolerances. A hard rect
  edge, a blurred shadow's soft falloff, and an MSDF glyph edge each
  disagree with a design-source export differently: a blur spreads a small
  per-pixel disagreement across a wide area, while an edge disagrees
  sharply over a thin band. One global budget would either reject a
  correct blur or accept a broken edge. Three bands are pinned in
  `goldens/tooling/src/oracle.rs` (`AA_EDGE`, `BLUR_FALLOFF`, `MSDF_TEXT`),
  each asserted exactly in `goldens/tooling/tests/render_oracle.rs` so a
  retune is a deliberate, reviewed change. Their values are engineering
  estimates from the AA/blur/MSDF edge characteristics. The two committed
  layout captures confirm `AA_EDGE` (`v08-wrap` 0.000 %, `v08-grid-spans`
  0.000 % over its five structural cells, its one text-driven cell excluded
  as a disclosed structural divergence — `goldens/oracle/manifest.json`,
  `goldens/oracle/README.md`); only `BLUR_FALLOFF` and `MSDF_TEXT` remain
  to be confirmed or retuned once their frames become renderable (#265, the
  v0.9 exit gate #49).
- **Real export only (2b).** No design source may be fabricated,
  hand-drawn, or stood in for by the project's own render. That is the
  exact self-oracle fidelity failure G-11 forbids — a renderer graded
  against itself cannot measure its own drift. So the diff runs against a
  real Figma REST export, and until a frame is captured its `designSource`
  is `null` with status `pending-265`. The `sigma = blur/2` mapping
  (`docs/decisions/effects-vocabulary-shadows.md`) stays a self-oracle
  constant, not retired against a real capture, until the `BLUR_FALLOFF`
  band measures it.
- **Import and render the fixture (3a).** The reference is our own fresh
  render of the imported fixture, not a pre-committed corpus golden. A
  committed golden is itself a self-oracle artifact, so diffing it against
  the design source would grade one committed picture against another and
  could hide a lowering-or-solve regression that moved both. The oracle
  instead imports each measured frame's committed Figma fixture
  (`corpus/figma-fixtures/<name>.json`) through `dashc::compile_figma`,
  re-solves it with the one `TaffySolver`, and renders it — the same path a
  producer takes — then diffs that render against the export. The
  tooling-phase choice (3b, diff the committed golden) was adopted before a
  real capture existed; importing the fixture in process is hermetic and
  fast (~0.05 s/frame), so it duplicates nothing and supersedes 3b.
- **Un-gated once captured (revised 4).** The tooling phase (#284) shipped
  the assertion `#[ignore]`-gated with a named #265 reason, run with
  `--ignored` by the `render-oracle` job, because no real design source
  existed and a permanently-red job would misreport unfinished scope. With
  the two layout captures committed, the assertion
  (`the_reference_renders_match_their_design_source`) is un-gated: it is
  hermetic (committed fixture + committed export + in-process compile, no
  network) and fast, so it runs in the ordinary `test` job. Its accounting
  test-locks keep the report honest — every frame is measured or pending,
  and `pending` is exactly the null-source frames — so an un-gated green
  cannot hide an unmeasured frame. The `render-oracle` CI job now re-runs
  the suite with `--nocapture` so the measured per-frame numbers show in the
  log. E7 is **partial**, not met: the two layout frames are measured, the
  shadow and text/baseline frames stay pending a renderable fixture and the
  text render path, and E7 flips to met at the v0.9 exit gate (#49) once
  every frame is measured.
