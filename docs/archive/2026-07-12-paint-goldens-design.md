# v0.3 paint goldens — design

    story    #18 (epic #12, slice v0.3 — the paint-golden coverage story)
    branch   story/paint-goldens
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

Per-construct golden coverage of the v0.3 paint vocabulary (issue #18):
golden scenes that isolate each paint family so a visual regression
implicates one construct, not the whole fixture (DESIGN_1.md §8
bisect-by-construction).

## What already exists (story #14), and this story's residual scope

Story #14 shipped, and this story does NOT duplicate:

- per-kind painter unit tests in `crates/dashscene-skia/tests/painter.rs`
  asserting exact interior bytes for every construct — the correctness
  gate, bit-stable across machines; and
- one combined `v03-paint.png` golden covering every kind on one canvas
  — the all-together integration image.

This story adds **per-family isolation goldens** — three focused
scenes, each a single construct family — so a regression fails only the
affected family's golden and the reviewer sees the construct alone:

- `v03-gradients.png` — the four gradient kinds side by side, including
  an angular **gauge-style** case (issue #18's explicit ask): a
  multi-stop green→amber→red angular sweep read as a dial arc.
- `v03-strokes.png` — a rrect stroked at each of the three aligns
  (inside/center/outside), one on rounded corners.
- `v03-images.png` — the four image scale modes (Fill/Fit/Crop/Tile)
  against a hand-rendered checker asset, one drawn into a rounded box
  (the implemented rounded clip of a node's own content).

## Clips scope (constraint from #14)

Issue #18 lists "clips". Subtree `clipsContent` (clipping a node's
descendants to its box) is deferred to #97 and the painter panics on
`entry.clip`, so it cannot be goldened yet. What IS paintable and
goldened here is the rounded clipping of a node's own fill/image to its
(rounded) box — shown in `v03-strokes.png` and `v03-images.png`.
Subtree-clip goldens are noted as blocked on #97.

## Comparison tolerance (constraint from #14)

These goldens contain anti-aliased gradients and curves, which are not
bit-identical across CPU architectures
(`docs/decisions/golden-comparison-space.md`). They use
`assert_matches_golden_within` at **2%** — higher than the combined
golden's 1% because each is a smaller canvas (64×64), so the same
absolute edge jitter is a larger fraction. 2% is still far below any
real rendering change (the smallest construct fills a quarter of a
family strip, ≥25%). Each golden additionally carries a few
interior-probe exact-byte asserts (derived from the fixture colors by
the painter's quantization) to pin its key property bit-stably,
independent of the tolerance.

## Testing

Three new tests in `goldens/tooling/tests/v03_families.rs`, each
building its family scene at boundary B (hand-built — no producer
stages this vocabulary), painting through `SkiaPainter`, asserting a
few interior probes, then `assert_matches_golden_within(name, png,
0.02)`. Generated with `UPDATE_GOLDENS=1`, visually inspected, and
committed. `just build` green; the goldens pass on a clean CI checkout.

## Alternatives considered

- **One golden per construct (~12 files)** — rejected: the per-kind
  unit tests already bisect to a single construct; per-family goldens
  give visual isolation without 12 PNGs and 12 inspection passes.
- **Skip the story as covered by #14** — rejected: #14's combined
  golden is one image; a construct regression there requires reading
  which pixels moved. Per-family goldens are the DESIGN §8
  bisect-by-construction the slice asks for. Recorded as a scope
  reshape (see `docs/decisions/`), not a silent drop.
- **Golden the subtree clip** — blocked on #97 (the painter panics);
  noted, not attempted.

## Trace

- Satisfies: issue #18 acceptance criteria (goldens per v0.3 paint kind,
  green in CI); DESIGN_1.md §8 (bisect-by-construction), §10.1 NOW
  vocabulary.
- Depends on: #14 (painting), #6 (harness) — both merged.
- Blocked coverage: subtree clip goldens await #97.
