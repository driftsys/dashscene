# goldens

CI golden images and the diff tooling (`DESIGN_1.md` §8, §11 v0.1):
scenes rendered through the reference painter (`dashscene-skia`, CPU
raster) and compared pixel-for-pixel against checked-in PNGs.

    goldens/
      images/     the checked-in goldens ({name}.png) and, on failure,
                  the rendered actual images ({name}.actual.png —
                  gitignored, never committed)
      tooling/    the `goldens` crate: diff tooling (src/lib.rs) and
                  the golden tests (tests/)

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

writes the current render over the golden. Inspect the image before
committing — the golden is the reviewed truth, not a rubber stamp. A
missing golden never auto-creates on a normal run: CI on a clean
checkout fails loudly if a golden was not committed.

## Determinism

Goldens are bit-exact because the reference painter is CPU raster
(deterministic by construction, `DESIGN_1.md` §8) with anti-aliasing
off and a pinned skia-safe version. GPU painters (v1+) will use
tolerance-based perceptual diffs instead — that tooling grows here.

## Comparison space

Goldens compare decoded pixels in unpremultiplied RGBA8888 — never
encoded bytes (an encoder change is not a rendering change; the tooling
notes byte drift on stderr and passes). Consequences, decided in
`docs/decisions/golden-comparison-space.md`:

- Opaque colors round-trip byte-exact.
- A semi-transparent fill bakes skia's premultiplication quantization
  into the golden: deterministic, bit-exact, but the stored value is
  the quantized one, not the authored one.
- v0.1 fixtures use opaque colors and integer coordinates only; the
  sub-pixel geometry policy is open as debt #85.
