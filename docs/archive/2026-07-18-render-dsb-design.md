# Sf-1 — render an emitted .dsb to a PNG (`just render`)

    status   design (working memory); human-approved 2026-07-18 (live-render path)
    story    Sf-1 of the "full real-file import" epic (ledger .superpowers/sdd/epic-progress.md)
    scope    goldens/tooling (a public render helper + a binary), justfile
    base     main 8ce351c

## Why

Both epic targets now emit a `.dsb`. The exit criterion's second half is "renders
through Skia." The human chose the **live-render** path (nothing third-party is
committed — `figma-corpus-self-authored-only.md`): a `just render <key> [root]`
recipe that imports a live Figma file, renders the emitted `.dsb` through the v0
Skia painter, and writes a PNG to `/tmp` for review. This proves both targets
render (and exercises the real image-fill path for the first time — the hero's
`.dsb` embeds its image bytes).

## What exists (grounded)

The full render pipeline works and is exercised by the **test-only**
`render_fixture` (`goldens/tooling/tests/render_oracle.rs:672-719`):
`.dsb` → `dashscene_core::load_document` → re-commit through
`TaffySolver::with_typesetter` (runs the measure seam) → stage `GlyphRun`s for
TEXT nodes (`stage_text`, atlases in cascade order `[ascii, arabic]`) →
`SkiaPainter::paint(rects, paints, images, clips, groups, glyphs, None)` →
`png_bytes()`. The v0 Skia painter is complete for both targets' vocabulary
(solid/gradients/**image fills w/ byte decode**/clips/corners/strokes/shadows/
MSDF text).

Key difference from `render_fixture`: it re-compiles a fixture with an **empty
images map**, so image fills never render. Sf-1 **loads the emitted `.dsb`
directly**, which already embeds image bytes (`dashbuf` `Image { bytes }`), so
`load_document` populates `scene.images()` and image fills paint.

## Design

### 1. A public render helper (additive — do NOT touch `render_fixture`)

Add a public function to `goldens/tooling/src` (e.g. `render.rs`):

    pub fn render_dsb(dsb: &[u8]) -> Vec<u8>   // returns PNG bytes

It reproduces `render_fixture`'s load → solve-with-typesetter → stage-text →
paint → png sequence, but takes `.dsb` bytes (no compile step). Expose the
font-resource loading it needs — the Noto cascade typesetter (`oracle_typesetter`)
and the atlas dirs (`ATLAS_ASCII_DIR`/`ATLAS_ARABIC_DIR`, `load_atlas`) — as
public `src` helpers (move or re-expose from the test module; keep the test's
own call sites working). The root paint size comes from `scene.rects()[0]`
(the root node's solved box), as `render_fixture` does.

**E7 safety:** do NOT modify `render_fixture`, `render_oracle.rs`, the
`goldens/oracle/manifest.json`, or the bands. Add the helper alongside; leave the
E7 gate byte-identical. (A later cleanup could refactor `render_fixture` onto the
shared helper — out of scope here to avoid touching the live E7 test while the
v0.9 track is active.)

### 2. A binary/example that renders a .dsb file

Add `goldens/tooling/src/bin/render-dsb.rs` (or an example): reads a `.dsb` path
and an output PNG path from argv, calls `render_dsb`, writes the PNG. Keep it a
thin wrapper.

### 3. The `just render` recipe

A recipe sibling to `reprobe` (justfile): `render key root=""`:

- Depends on `wasm`; reads FIGMA_TOKEN from the keychain (never printed — only
  its length), exports it.
- `deno task import <key> [--root <root>] -o <tmp>.dsb` (partial-emit default) —
  emit the document (in-scope tmp path like the reprobe recipe, copied to
  `/tmp/render.dsb`).
- `cargo run -p <goldens-tooling-crate> --bin render-dsb -- /tmp/render.dsb /tmp/render.png`.
- Print `/tmp/render.png` (and its size). Public files are live-only — the `.dsb`
  and `.png` live in `/tmp`, never committed; the in-scope tmp is cleaned like
  reprobe's.
- Document the two epic targets as usage examples in a comment.

## Guardrails

- **License / corpus:** third-party files are live-only. Nothing (no `.dsb`, no
  `.png`, no file JSON) is committed. `/tmp` outputs + any in-scope scratch are
  gitignored/cleaned.
- **E7 (v0.9 exit gate) untouched:** the render oracle test, manifest, design
  sources, and bands are not modified. Only additive `src` + a new binary +
  a recipe.
- **P1/P2:** unchanged — this is a consumer of the existing render stack, not a
  change to it. #327 (text render-wiring) is a SEPARATE follow-up: first-light
  text renders here with default axes (auto line-height, left align); if the live
  render shows it visibly wrong, that motivates #327 next (not part of Sf-1).

## Verification

- `just build` green (the new src helper + binary compile; the E7 oracle test
  still passes unchanged — the render is byte-identical because `render_fixture`
  is untouched).
- Live: `just render MRk9I5cYY6yJa8JhljzkBn 2411:10795` and
  `just render S30AJmYfnDKGeSQmzuXEUk 1973:6580` each write a PNG. Confirm both
  are non-trivial (first-light: frames+text; hero: a full landing page with
  images). Report the paths + sizes; the orchestrator surfaces the PNGs to the
  human. NEVER echo the token.

## Alternatives considered

- **Refactor `render_fixture` onto the shared helper** (dedup). Deferred: it
  touches the live E7 test while the v0.9 track is editing that file — collision
  - regression risk on the exit gate. Additive-alongside is safer now.
- **A `dashc` example.** Rejected: `dashc` is the compiler and does not depend on
  core/engine/typeset/skia; only `goldens/tooling` has the full render stack.

## Test strategy

- A unit test that `render_dsb` on a tiny known `.dsb` (e.g. a one-frame solid
  fill, built via `compile_figma` in the test) returns non-empty PNG bytes of the
  expected dimensions — proves the public helper drives the pipeline. (The E7
  oracle already covers render fidelity; this only guards the new entry point.)
- The live `just render` on both targets is the empirical proof (manual, not a
  committed test — targets are third-party/live-only).
