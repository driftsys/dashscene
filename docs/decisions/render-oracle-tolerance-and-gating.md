# The design-source render oracle uses per-rule bands and a gated assertion

    status   accepted (story #284, 2026-07-17)
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

1b — per-rule bands. 2b — real export only, pending #265. 3b — diff the
committed reference golden. 4b — `#[ignore]`-gated assertion, loud pending
report.

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
  estimates from the AA/blur/MSDF edge characteristics; the first real
  captures (#265) and the v0.9 exit gate (#49) confirm or retune them.
- **Real export only (2b).** No design source may be fabricated,
  hand-drawn, or stood in for by the project's own render. That is the
  exact self-oracle fidelity failure G-11 forbids — a renderer graded
  against itself cannot measure its own drift. So the diff runs against a
  real Figma REST export, and until #265 lands each frame's `designSource`
  is `null` with status `pending-265`. The `sigma = blur/2` mapping
  (`docs/decisions/effects-vocabulary-shadows.md`) stays a self-oracle
  constant, not retired against a real capture, until the `BLUR_FALLOFF`
  band measures it.
- **Diff the committed golden (3b).** The self-oracle golden tests already
  prove each committed golden equals the fresh reference render, so the
  committed golden is a faithful stand-in for the render side, and
  reconstructing every scene in the oracle would duplicate the scene
  authoring spread across the `v08_*` test files. The manifest points at
  the committed golden as the reference image; re-rendering can be added
  when the v0.9 assertion needs it.
- **Gated assertion, not a red job (4b).** This story is the tooling, not
  the assertion, so a permanently-red CI job would misreport unfinished
  scope as a failure. Instead the real-capture assertion
  (`the_reference_renders_match_their_design_source`) is `#[ignore]`-gated
  with a named #265 reason, and an authored `render-oracle` CI job runs it
  with `--ignored`. With no committed design source it measures nothing and
  prints a pending summary naming every frame — a loud pending state, never
  a silent green. The test-locks that keep the report honest (every frame
  is measured or pending, and `pending` is exactly the null-source frames)
  live in the same test. E7 stays **open (tooling landed)**, not met, and
  is asserted at the v0.9 exit gate (#49).
