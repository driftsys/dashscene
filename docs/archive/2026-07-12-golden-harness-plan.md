# Golden harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `goldens` workspace crate whose test renders the dashlang-built v0.1 scene through `SkiaPainter` and compares it against a checked-in PNG, with regeneration and diff workflows documented.

**Architecture:** One unpublished crate at `goldens/tooling` (lib = diff tooling with an injectable images root for unit tests; integration test = the golden scene). The golden image is committed under `goldens/images/`.

**Tech Stack:** Rust, skia-safe (decode/encode — already in tree). Gate: `just build`.

## Global Constraints

- No new external dependencies.
- The golden test must fail loudly when the golden is missing (never
  auto-create outside `UPDATE_GOLDENS=1`).
- Fixtures: integer coordinates, opaque colors only (debt #85 untouched).
- Commits: conventional; scope `goldens` (added to `.git-std.toml`) for
  the harness + image, `repo` for the workspace/config wiring if split.

---

### Task 1: crate wiring + diff tooling (TDD on the tooling)

**Files:**

- Modify: `Cargo.toml` (workspace members + `goldens` in
  `[workspace.dependencies]`? not needed — nothing depends on it),
  `.git-std.toml` (scope list + one `[[version_files]]`? no — the crate
  is unpublished and unversioned; scope list only), `.gitignore`
  (`goldens/images/*.actual.png`)
- Create: `goldens/tooling/Cargo.toml` (package `goldens`,
  `publish = false`, deps: `skia-safe.workspace = true`; dev-deps:
  `dashlang`, `dashscene-core`, `dashscene-skia` all workspace)
- Create: `goldens/tooling/src/lib.rs`

**Interfaces:**

- Produces: `goldens::assert_matches_golden(name: &str, png_bytes: &[u8])`
  and the internal `compare_against(root: &Path, name, png_bytes)`
  used by unit tests.

- [ ] **Step 1 (RED):** unit tests in `src/lib.rs` `#[cfg(test)]`:
      matching pixels pass (and encoding-only drift passes), differing
      pixels panic with a count and write `{name}.actual.png`, missing
      golden panics mentioning `UPDATE_GOLDENS` — all against a temp images
      root, using tiny PNGs encoded via skia in the test. Run: FAIL to
      compile.
- [ ] **Step 2 (GREEN):** implement decode (skia `Image::from_encoded`
  - `read_pixels` to unpremul RGBA8888), compare, actual-write, update
    mode, panic messages. Run: PASS.
- [ ] **Step 3:** commit
      `feat(goldens): add the golden diff tooling crate`.

---

### Task 2: the golden scene test + the committed golden

**Files:**

- Create: `goldens/tooling/tests/v01.rs`
- Create: `goldens/images/v01-walking-skeleton.png` (generated via
  `UPDATE_GOLDENS=1 cargo test -p goldens`, then committed)

**Interfaces:**

- Consumes: `dashlang::{scene, node, anon, rgba}`,
  `dashscene_core::Arena`, `dashscene_skia::SkiaPainter`,
  `goldens::assert_matches_golden`.

- [ ] **Step 1 (RED):** write the test building the design doc's 64×64
      scene, painting it, and calling
      `assert_matches_golden("v01-walking-skeleton", &painter.png_bytes())`.
      Run without the golden: FAIL (missing golden, names UPDATE_GOLDENS).
- [ ] **Step 2 (GREEN):** `UPDATE_GOLDENS=1 cargo test -p goldens`,
      inspect the produced PNG (visually or via the pixel asserts below),
      `git add goldens/images/v01-walking-skeleton.png`; re-run without the
      env var: PASS. Also add two direct pixel asserts in the test (e.g.
      background pixel and overlap-order pixel) so the scene's key
      properties are pinned independently of the image file.
- [ ] **Step 3:** mutate one fixture color, run, verify the golden
      test FAILS with a differing-pixel report and writes the .actual.png;
      restore; PASS. Delete the stray actual file.
- [ ] **Step 4:** commit
      `feat(goldens): golden-test the v0.1 walking-skeleton scene`.

---

### Task 3: README + records

**Files:**

- Create: `goldens/README.md` — what lives here; how the test runs
  (workspace test, CI); regeneration (`UPDATE_GOLDENS=1`); failure
  inspection (`.actual.png`); determinism rationale (CPU raster, pinned
  skia); the comparison-space decision (unpremul RGBA, opaque fixtures,
  encoding drift informational).
- Create: `docs/decisions/golden-comparison-space.md` (context/options/
  choice/why from the design doc; closes #86).
- Modify: `docs/decisions/README.md` (index entry).

- [ ] **Step 1:** write both + index; `just build` green; commit
      `docs(goldens): document the golden workflow and comparison space`.
