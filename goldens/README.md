# goldens

CI golden images and the diff tooling (`docs/design/architecture.md`, `docs/roadmap.md` v0.1):
scenes rendered through the reference painter (`dashscene-skia`, CPU
raster) and compared pixel-for-pixel against checked-in PNGs.

    goldens/
      images/     the checked-in goldens ({name}.png) and, on failure,
                  the rendered actual images ({name}.actual.png —
                  gitignored, never committed)
      dsb/        golden .dsb documents ({name}.dsb): what the compiler
                  emits, compared exactly rather than with a pixel
                  tolerance (see goldens/dsb/README.md)
      tooling/    the `goldens` crate: diff tooling (src/lib.rs) and
                  the golden tests (tests/)
      oracle/     the design-source render oracle (exit criterion E7,
                  guardrail G-11): a perceptual diff of a fresh
                  import-and-render of the committed Figma fixture
                  against its Figma design source, not against this
                  crate's own previous output — see
                  goldens/oracle/README.md. The assertion runs un-gated
                  in the ordinary test job for captured frames; issue
                  #265 tracks capturing the remaining frames, not the
                  mechanism

## Running

The golden tests are ordinary workspace tests — `just test`,
`cargo test --workspace`, or `cargo test -p goldens` all run them, and
the CI `test` job picks them up with no extra wiring.

## When a golden test fails

The failure message names the differing-pixel count, the first
differing coordinate, and two paths: the checked-in golden and the
freshly rendered `{name}.actual.png` written next to it. Open both to
compare. If the rendering change is intended, regenerate (below), review
the image diff in the PR, and commit; if not, the painter (or an
upstream crate) regressed.

## Regenerating goldens

    UPDATE_GOLDENS=1 cargo test -p goldens

writes the current render over every golden the run touches; narrow it
to one golden with cargo's test-name filter
(`UPDATE_GOLDENS=1 cargo test -p goldens <test_name>`). Inspect each
image before committing — a golden is reviewed truth; never commit one
unreviewed. A missing golden never auto-creates on a normal run: CI on
a clean checkout fails loudly if a golden was not committed.

## Determinism

The reference painter is CPU raster (deterministic by construction,
`docs/design/architecture.md`) at an exactly pinned skia-safe version (`=0.81.0` in
the workspace `Cargo.toml`; bumping it is a deliberate, re-goldened
change). On one machine every render is bit-identical.

Across CPU architectures, bit-exactness holds only for integer-aligned,
un-antialiased geometry — solid fills. Anti-aliasing is on since v0.3
(`docs/decisions/reference-painter-antialiasing.md`), and skia's
coverage rounding at a fractional edge is not bit-identical across
architectures, so gradient and curve edges jitter by a handful of
pixels between machines. Two comparison functions handle this:

- `assert_matches_golden(name, png)` — exact; use it for solid,
  integer-aligned content (the v0.1 golden, and the four v0.2 flex
  goldens, whose scenes are dimensioned so every solved rect lands on
  an integer — see `docs/decisions/v02-flex-goldens-per-construct.md`).
- `assert_matches_golden_within(name, png, max_fraction)` — allows up
  to `max_fraction` of the canvas pixels to differ; use it for
  anti-aliased content whose ink is a large share of the canvas. The
  combined v0.3 golden uses 1%; the smaller per-family v0.3 goldens use
  2% (edge jitter is a larger fraction of a smaller canvas). This is
  `docs/technotes/rendering-and-painters.md`'s tolerance-based perceptual diff.
- `assert_matches_golden_max_pixels(name, png, max_pixels)` — allows up
  to `max_pixels` to differ, an absolute count; use it for sparse
  content (text). A canvas fraction wide enough to clear the jitter can
  exceed the whole inked footprint of sparse text, so a text-erasing
  regression would pass; the absolute budget is calibrated against the
  smallest regression the scene can express — for a scene with several
  text runs, one run vanishing, not the total erase. The v0.6 Arabic
  golden uses 440 px (issue #532) and pairs it with a glyph-id-level
  guard test. Commit the calibration as a test beside the golden, so the
  budget is a gate rather than an estimate. See
  `docs/decisions/golden-comparison-space.md`, "Text goldens".

Per-kind correctness is pinned separately by the painter's
interior-probe unit tests (and, for text, the golden's companion
glyph-id guard), which are bit-stable across machines. See
`docs/decisions/golden-comparison-space.md`.

GPU painters (v1+) will use tolerance-based perceptual diffs throughout
— that tooling will be added here.

## Comparison space

Goldens compare decoded pixels in unpremultiplied RGBA8888 — never
encoded bytes (an encoder change is not a rendering change; the tooling
notes byte drift on stderr and passes). Rationale and consequences:
`docs/decisions/golden-comparison-space.md`. The one fact golden
authors need: a semi-transparent fill bakes skia's premultiplication
quantization into the golden — the stored value is the quantized one,
not the authored one.
