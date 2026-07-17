# Story #284 — E7 design-source render oracle (tooling)

    status   wip (Superpowers spec + plan). Gardened into docs/design/goldens.md,
             docs/specification/05-qualification.md, and
             docs/decisions/effects-vocabulary-shadows.md on landing; this raw
             original moves to docs/archive/.
    branch   story/render-oracle off origin/main@8d58bdf
    issue    #284 (epic #42, v0.8 — fidelity)

## Problem

Guardrails G-11 / G-23 (`docs/technotes/engineering-guardrails.md`): fidelity
must be a measured number, not an asserted one, and a renderer that is its own
oracle cannot see its own drift. Today every golden diffs the `dashscene-skia`
reference painter against the project's own committed PNG — a self-oracle. Exit
criterion E7 (`docs/specification/05-qualification.md`) adds the missing half of
R6: a clean file must render within tolerance of its **design source** — Figma's
REST image export (`GET /images`).

This story delivers the E7 **tooling**, not the v0.9 exit assertion (#49).

## The #265 gate (explicit)

The real design-source images (Figma REST `GET /images` exports for the corpus
frames) are authored manually and tracked by issue #265 (parked). We therefore
CANNOT diff against a real Figma export today, and MUST NOT fabricate one or
claim the `sigma = blur/2` self-oracle debt is retired against a real capture —
that is the exact G-11 integrity failure this story exists to prevent.

Handling: build the tooling now; gate the real-capture assertion behind #265.

- The perceptual-diff harness and the per-rule bands are proven with controlled
  **synthetic** image pairs — no design source is pretended.
- The corpus-frame ↔ design-source manifest lists each frame's export slot,
  marked PENDING #265.
- The real-capture assertion is `#[ignore]`-gated with a named #265 reason and
  wired into an authored CI job (billing-blocked, will not execute).
- The `sigma = blur/2` debt is NOT retired: the manifest adds the BLUR_FALLOFF
  slot that will pin it, still pending #265.

## Scope

1. **Perceptual-diff harness** (`goldens/tooling/src/oracle.rs`): takes two PNGs
   - a per-rule `ToleranceBand`, returns an `OracleDiff` (measured differing
     count, fraction, max per-channel delta) and a pass/fail against the band.
     Decodes in the golden comparison space (unpremultiplied RGBA8888).
2. **Per-rule tolerance bands** (pinned constants, documented rationale):
   `AA_EDGE` (hard rect edges), `BLUR_FALLOFF` (soft shadow falloff / the
   `sigma = blur/2` mapping), `MSDF_TEXT` (glyph edges). Not one global budget:
   each rule fails differently, so each pins its own band (G-11).
3. **Corpus-frame ↔ design-source manifest** (`goldens/oracle/manifest.json` +
   README): per frame, the reference image (a committed golden), the design-
   source slot (PENDING #265), and the band that applies.
4. **Tests** (`goldens/tooling/tests/render_oracle.rs`): synthetic-pair harness
   validation (runs in the `test` job); a manifest-consistency test (bands
   known, reference images present, every frame pending #265); and the
   `#[ignore]`-gated real-capture assertion.
5. **CI job** (`.github/workflows/ci.yml`): a `render-oracle` job that runs the
   gated assertion with `--ignored`; authored, billing-blocked.
6. **Docs**: E7 status update (open → tooling landed, assertion pending);
   `goldens.md` design record; `sigma = blur/2` debt note update.

## The band values (pinned, with rationale)

A pixel counts as differing only when its largest per-channel absolute delta
(0..=255) exceeds the band's `channel_delta`; a frame passes when the differing
fraction is at or below the band's `differing_fraction`.

- **AA_EDGE** — `channel_delta = 40`, `differing_fraction = 0.02`. A hard rect
  edge anti-aliased against the design source: the reference painter's coverage
  rounding and Figma's server-side export resampling disagree on a thin 1–2 px
  edge band, where the per-pixel swing can be large. The fraction budget is the
  primary tolerance (edges occupy a small share of the canvas); `channel_delta`
  filters sub-threshold interior noise.
- **BLUR_FALLOFF** — `channel_delta = 24`, `differing_fraction = 0.12`. A
  blurred shadow spreads a small per-pixel disagreement across a wide falloff
  region — many pixels off by a little. The `sigma = blur/2` mapping is an
  approximation of Figma's blur, so the whole falloff can be systematically off
  by a small amount; a wider fraction with a moderate per-pixel threshold pins
  "the falloff shape is close" without demanding pixel identity.
- **MSDF_TEXT** — `channel_delta = 50`, `differing_fraction = 0.03`. MSDF glyph
  edges are sharp high-contrast transitions; the reference painter's MSDF
  resolve and Figma's font rasterizer disagree at glyph boundaries (hinting,
  gamma). Text ink is sparse, so a small fraction with a higher per-pixel
  threshold pins the glyph shapes without over-tolerating.

These initial values are engineering estimates from the AA/blur/MSDF edge
characteristics. They are pinned so the harness is falsifiable now; the first
real captures (#265) and the v0.9 exit gate (#49) confirm or retune them.

## Plan (TDD)

1. RED: `goldens/tooling/tests/render_oracle.rs` — synthetic-pair tests against
   the not-yet-existing `goldens::oracle` API. Verify they fail to compile /
   fail. → verify: `cargo test -p goldens --test render_oracle` fails.
2. GREEN: implement `goldens/tooling/src/oracle.rs` + `pub mod oracle;`. → verify:
   the synthetic tests pass.
3. Author `goldens/oracle/manifest.json` + README; add the manifest-consistency
   test and the `#[ignore]`-gated real-capture test. → verify: manifest test
   passes; ignored test is skipped.
4. Wire the `render-oracle` CI job; add it to the `ci` aggregate `needs`.
5. Garden docs; move this file to `docs/archive/`.
6. `just build`, `just verify`, `just wasm`; `just fmt`. → verify: exit 0.

## Alternatives considered

- **A single global perceptual budget** instead of per-rule bands. Rejected: a
  blurred shadow's wide soft falloff and a hard rect edge need different budgets
  (G-11 says so explicitly). One global number would either reject a correct
  blur or accept a broken edge.
- **Render each frame fresh in the oracle** rather than diffing the committed
  golden PNG. Deferred: the golden tests already prove each committed golden
  equals the fresh reference render, so the committed golden is a faithful
  stand-in, and reconstructing every scene here would duplicate the scene
  authoring scattered across the `v08_*` test files. The manifest points at the
  committed golden as the reference image; re-rendering can be added when the
  v0.9 assertion needs it.
- **Fabricate a stand-in design source** (hand-drawn or the golden itself) to
  make the assertion run today. Rejected outright: that is the self-oracle
  fidelity failure G-11 forbids. The assertion is gated on the real #265
  captures instead.
- **Make the gated CI job fail (red) until #265 lands.** Rejected: the story is
  the tooling, not the assertion; a permanently-red job is not a valid authored
  workflow. The assertion is `#[ignore]`-gated with a named reason and the
  manifest marks each frame PENDING #265, so the gate is visible, not silent.
