# render an emitted .dsb to a PNG (`just render`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:test-driven-development to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a public `render_dsb(dsb: &[u8]) -> Vec<u8>` helper, a `render-dsb`
binary, and a `just render <key> [root]` recipe that imports a live Figma file,
renders the emitted `.dsb` through the v0 Skia painter, and writes a PNG to `/tmp`.

**Architecture:** `render_dsb` mirrors the test-only `render_fixture`
(`goldens/tooling/tests/render_oracle.rs`) — load `.dsb` → re-solve with a
typesetter-backed `TaffySolver` (runs the measure seam) → stage glyph runs for
every TEXT node → `SkiaPainter::paint` → PNG — but takes emitted `.dsb` bytes
directly (no compile step) so embedded image bytes render. It lives in a NEW
`goldens/tooling/src/render.rs`, self-contained (its own copy of the Noto
cascade typesetter + atlas loaders) so the E7 oracle test file stays
byte-identical. A thin `src/bin/render-dsb.rs` wraps it; a `just render` recipe
(sibling to `reprobe`) drives the live import + render.

**Tech Stack:** Rust 2024, the `goldens` crate (package name `goldens`),
`dashbuf`/`dashscene-core`/`dashscene-engine`/`dashscene-typeset`/`dashpaint`/
`dashscene-skia` for the render stack, `dashc`(`dashc_wasm`)/`dashscene-validator`/
`serde_json` for the test's in-process compile, Deno importer + `just`, Skia.

## Global Constraints

- **E7 safety (paramount):** do NOT modify `render_fixture`,
  `goldens/tooling/tests/render_oracle.rs`, `goldens/tooling/tests/common/mod.rs`,
  `goldens/oracle/manifest.json`, `goldens/oracle/design-source/*`, or the bands
  in `goldens/tooling/src/oracle.rs`. Additive only. The E7 exit gate stays
  byte-identical. This is why `render.rs` carries its own copy of the loader
  helpers instead of moving them out of the live test file.
- **No third-party content committed:** public Figma files are live-only. No
  `.dsb`, `.png`, or file JSON is committed. `/tmp` outputs are never committed;
  in-scope scratch is cleaned like `reprobe`'s.
- **Never echo FIGMA_TOKEN:** print only its character length or an HTTP status.
- **Commit scopes (git-std allowlist):** `goldens` for tooling src/bin, `repo`
  for the justfile recipe, `docs` for this working memory. `edition = "2024"`.
- **Every commit leaves `just build` green.**

---

## File Structure

- `goldens/tooling/Cargo.toml` (modify) — promote the render-pipeline crates from
  `[dev-dependencies]` to `[dependencies]` (they are now used by `src/render.rs`,
  not only tests): `dashbuf`, `dashpaint`, `dashscene-core`, `dashscene-engine`,
  `dashscene-skia`, `dashscene-typeset`. `dashc`, `dashscene-validator`,
  `serde_json`, `dashlang`, `tempfile`, `dashcue` stay dev-only.
- `goldens/tooling/src/render.rs` (create) — `pub fn render_dsb`, the public
  loader helpers (`oracle_typesetter`, `load_atlas`, `ATLAS_ASCII_DIR`,
  `ATLAS_ARABIC_DIR`), private stagers (`stage_text`, `text_runs`, `origin_of`,
  `FONT_LATIN`, `FONT_ARABIC`), and the unit test.
- `goldens/tooling/src/lib.rs` (modify) — add `pub mod render;`.
- `goldens/tooling/src/bin/render-dsb.rs` (create) — thin argv wrapper.
- `justfile` (modify) — the `render key root=""` recipe.
- `docs/wip/2026-07-18-render-dsb-{design,plan}.md` — working memory (this plan +
  the approved design), committed under `docs`.

---

### Task 1: Working memory (design + plan) committed

**Files:**

- Modify (track): `docs/wip/2026-07-18-render-dsb-design.md` (already written by the
  orchestrator, currently untracked)
- Create: `docs/wip/2026-07-18-render-dsb-plan.md` (this file)

- [ ] **Step 1: Commit the design + plan**

```bash
git add docs/wip/2026-07-18-render-dsb-design.md docs/wip/2026-07-18-render-dsb-plan.md
git commit -m "docs(docs): add render-dsb design and TDD plan working memory"
```

---

### Task 2: `render_dsb` public helper

**Files:**

- Modify: `goldens/tooling/Cargo.toml`
- Create: `goldens/tooling/src/render.rs`
- Modify: `goldens/tooling/src/lib.rs` (add `pub mod render;`)

**Interfaces:**

- Produces: `pub fn render_dsb(dsb: &[u8]) -> Vec<u8>` (PNG bytes);
  `pub fn oracle_typesetter() -> dashscene_typeset::text::Typesetter`;
  `pub fn load_atlas(dir: &str) -> dashpaint::Atlas`;
  `pub const ATLAS_ASCII_DIR: &str`; `pub const ATLAS_ARABIC_DIR: &str`.
- Consumes: `dashc_wasm::compile_figma`, `dashscene_validator::Profile`,
  `serde_json::json!` (unit test only, to build a one-frame `.dsb`).

- [ ] **Step 1: Promote the render-pipeline crates to real dependencies**

Edit `goldens/tooling/Cargo.toml` so `[dependencies]` gains the render stack and
`[dev-dependencies]` keeps only the test-only crates:

```toml
[dependencies]
skia-safe.workspace = true
# render_dsb (src/render.rs) loads a committed `.dsb` and renders it through the
# full stack, so the render-pipeline crates are real dependencies now, not
# dev-only: dashbuf parses the buffer, dashscene-core loads it, dashscene-engine
# re-solves through the typesetter measure seam, dashscene-typeset shapes,
# dashpaint carries the boundary-B tables, dashscene-skia paints.
dashbuf.workspace = true
dashpaint.workspace = true
dashscene-core.workspace = true
dashscene-engine.workspace = true
dashscene-skia.workspace = true
dashscene-typeset.workspace = true

[dev-dependencies]
dashlang.workspace = true
tempfile.workspace = true
dashcue.workspace = true
# The render_dsb unit test and the v0.7 text-lowering golden (#160) drive dashc's
# compile_figma to build a `.dsb` in-process; the validator supplies the compile
# profile. Both are test-only.
dashc.workspace = true
dashscene-validator.workspace = true
# serde_json builds the render_dsb unit test's inline Figma fixture and derives
# the v0.7 ellipse golden's (#239) captured negative-gap fixture in place.
serde_json.workspace = true
```

- [ ] **Step 2: Register the module**

Add to `goldens/tooling/src/lib.rs`, next to `pub mod oracle;`:

```rust
pub mod render;
```

- [ ] **Step 3: Write the failing unit test (RED)**

Create `goldens/tooling/src/render.rs` with a `todo!()` body and the unit test:

```rust
//! Load a committed `.dsb` and render it through the v0 Skia reference painter —
//! the public render entry point behind `just render` and the `render-dsb`
//! binary (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).
//!
//! This mirrors the test-only `render_fixture` in `tests/render_oracle.rs` (the
//! E7 design-source oracle) with one deliberate difference: it takes emitted
//! `.dsb` *bytes* directly rather than recompiling a fixture with an empty
//! images map, so an embedded image fill (`dashbuf` `Image { bytes }`) is present
//! in `scene.images()` and paints. The E7 oracle and its helpers are left
//! byte-identical (docs/wip design, E7 safety), so this module carries its own
//! copy of the font/atlas resource loaders rather than moving them out of the
//! live test file.

use dashbuf::root_as_document;
use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageAsset,
    ImageFormat, Painter,
};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, Typesetter};

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);

/// The committed ASCII glyph-atlas fixture directory (Noto Sans, font 0).
pub const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
/// The committed Arabic glyph-atlas fixture directory (Noto Sans Arabic, font 1).
pub const ATLAS_ARABIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// The one coverage cascade every TEXT node is measured and staged through:
/// Noto Sans primary (font 0), Noto Sans Arabic fallback (font 1). The font
/// index a shaped glyph carries indexes both this cascade and the atlas list
/// built in the same order (`[ascii, arabic]`). A copy of the E7 oracle's
/// `oracle_typesetter` (kept here so the live oracle test stays byte-identical).
pub fn oracle_typesetter() -> Typesetter {
    let latin = Font::from_bytes(
        std::fs::read(FONT_LATIN).expect("corpus Latin font present"),
        0,
    )
    .expect("Noto Sans parses");
    let arabic = Font::from_bytes(
        std::fs::read(FONT_ARABIC).expect("corpus Arabic font present"),
        0,
    )
    .expect("Noto Sans Arabic parses");
    Typesetter::with_fonts(vec![latin, arabic])
}

/// Converts a committed build-time atlas fixture at `dir` into a boundary-B
/// [`Atlas`]: only glyphs that paint (bounded outlines) carry a quad. A copy of
/// the goldens `common` helper (kept here so the E7 oracle test stays
/// byte-identical).
pub fn load_atlas(dir: &str) -> Atlas {
    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"));
    let m = &bundle.metrics;
    let glyphs = m
        .glyphs
        .iter()
        .filter_map(|g| {
            Some(AtlasGlyph {
                glyph_id: g.glyph_id,
                plane_em: g.plane_em?,
                atlas_px: g.atlas_px?,
            })
        })
        .collect();
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: bundle.image_png.clone(),
        },
        m.atlas.width,
        m.atlas.height,
        m.atlas.px_per_em,
        m.atlas.distance_range_px,
        glyphs,
    )
}

/// The resolved box origin of a committed node.
fn origin_of(arena: &Arena, node: NodeId) -> (f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.x, rect.y)
}

/// Shapes `text` at `size`, places every glyph in absolute document space, and
/// splits a new run wherever the cascade switched fonts. A copy of the E7
/// oracle's `text_runs`.
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    origin: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
) -> Vec<GlyphRun> {
    let laid = ts.layout(text, size, None);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + g.y,
            };
            match runs.last_mut() {
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
                    atlas,
                    size,
                    color,
                    glyphs: vec![quad],
                    opacity: 1.0,
                }),
            }
        }
    }
    runs
}

/// Walks the committed arena and stages glyph runs for every TEXT node. A copy
/// of the E7 oracle's `stage_text`.
fn stage_text(arena: &Arena, ts: &mut Typesetter, atlases: &[AtlasIndex]) -> Vec<GlyphRun> {
    fn walk(
        arena: &Arena,
        node: NodeId,
        ts: &mut Typesetter,
        atlases: &[AtlasIndex],
        out: &mut Vec<GlyphRun>,
    ) {
        if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
            let origin = origin_of(arena, node);
            out.extend(text_runs(ts, atlases, origin, text, style.size, style.color));
        }
        for &child in arena.children(node) {
            walk(arena, child, ts, atlases, out);
        }
    }
    let mut out = Vec::new();
    for &root in arena.roots() {
        walk(arena, root, ts, atlases, &mut out);
    }
    out
}

/// Loads a committed `.dsb`, re-solves it through the one typesetter-backed
/// `TaffySolver` (so TEXT nodes size to their shaped extent), stages a glyph run
/// for every TEXT node, and renders the committed scene with the Skia reference
/// painter — returning the PNG. The canvas is sized to the root node's solved
/// box (`scene.rects()[0]`). Unlike `render_fixture`, embedded image-fill bytes
/// carried by the `.dsb` are loaded into `scene.images()` and paint.
pub fn render_dsb(dsb: &[u8]) -> Vec<u8> {
    todo!("implement in Step 5")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dashc_wasm::compile_figma;
    use dashscene_validator::Profile;

    use super::render_dsb;

    /// A one-page Figma REST document whose root FRAME is `root`.
    fn document_json(root: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "document": {
                "name": "Document",
                "type": "DOCUMENT",
                "children": [{
                    "name": "Page 1",
                    "type": "CANVAS",
                    "children": [root],
                }],
            },
        })
    }

    #[test]
    fn render_dsb_returns_a_png_of_the_root_box_size() {
        // A single 100x60 frame with one solid fill — no text, no image — is the
        // smallest scene that exercises load -> solve -> paint -> png. It has no
        // fixture on disk: it is compiled in-process into a `.dsb`, exactly the
        // bytes `render_dsb` consumes at runtime.
        let root = serde_json::json!({
            "name": "one-frame",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        });
        let json = document_json(root).to_string();
        let (dsb, report) = compile_figma(&json, Profile::Core, &BTreeMap::new())
            .expect("the one-frame fixture compiles");
        assert!(report.is_empty(), "the one-frame fixture lowers clean: {report}");

        let png = render_dsb(&dsb);

        assert!(!png.is_empty(), "render_dsb returns non-empty PNG bytes");
        let data = skia_safe::Data::new_copy(&png);
        let image = skia_safe::images::deferred_from_encoded_data(data, None)
            .expect("the rendered bytes decode as a PNG");
        assert_eq!(
            (image.width(), image.height()),
            (100, 60),
            "the PNG is sized to the root frame's solved box"
        );
    }
}
```

- [ ] **Step 4: Run the test to verify it fails (RED)**

Run: `cargo test -p goldens --lib render::tests::render_dsb_returns_a_png_of_the_root_box_size -- --nocapture`
Expected: FAIL — the test panics inside `render_dsb` with `not yet implemented` (`todo!`).

- [ ] **Step 5: Implement `render_dsb` (GREEN)**

Replace the `todo!` body:

```rust
pub fn render_dsb(dsb: &[u8]) -> Vec<u8> {
    let document = root_as_document(dsb).expect("a valid .dsb buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit an empty transaction through a typesetter-backed solver
    // so a full solve runs the measure seam (the pattern the text goldens use).
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    // Stage glyph runs for every TEXT node. The atlases are pushed in the
    // cascade's font order (`[ascii, arabic]`), so the font index a shaped glyph
    // carries selects its atlas.
    let mut glyphs = GlyphRunTable::new();
    let ascii = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    for run in stage_text(&arena, &mut ts, &[ascii, arabic]) {
        glyphs.push_run(run);
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &glyphs,
        None,
    );
    painter.png_bytes()
}
```

- [ ] **Step 6: Run the test to verify it passes (GREEN)**

Run: `cargo test -p goldens --lib render::tests::render_dsb_returns_a_png_of_the_root_box_size`
Expected: PASS.

- [ ] **Step 7: Verify the E7 oracle test still passes (unchanged)**

Run: `cargo test -p goldens --test render_oracle the_reference_renders_match_their_design_source -- --nocapture`
Expected: PASS, same measured lines as before (render_oracle.rs untouched).

- [ ] **Step 8: Commit**

```bash
git add goldens/tooling/Cargo.toml goldens/tooling/src/lib.rs goldens/tooling/src/render.rs
git commit -m "feat(goldens): add render_dsb, a public helper that loads and renders a .dsb"
```

---

### Task 3: `render-dsb` binary

**Files:**

- Create: `goldens/tooling/src/bin/render-dsb.rs`

**Interfaces:**

- Consumes: `goldens::render::render_dsb`.

- [ ] **Step 1: Write the binary**

Create `goldens/tooling/src/bin/render-dsb.rs`:

```rust
//! `render-dsb <in.dsb> <out.png>` — load a committed `.dsb` and render it
//! through the v0 Skia reference painter to a PNG. A thin wrapper over
//! [`goldens::render::render_dsb`]; the live entry point is `just render`
//! (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: render-dsb <in.dsb> <out.png>");
        return ExitCode::FAILURE;
    };
    let dsb = match std::fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("render-dsb: cannot read {input}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let png = goldens::render::render_dsb(&dsb);
    if let Err(error) = std::fs::write(&output, &png) {
        eprintln!("render-dsb: cannot write {output}: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!("render-dsb: wrote {output} ({} bytes)", png.len());
    ExitCode::SUCCESS
}
```

- [ ] **Step 2: Verify it builds and runs on the one-frame `.dsb`**

Run (drives the binary end-to-end through a tmp `.dsb` produced by the test path):

```bash
cargo build -p goldens --bin render-dsb
```

Expected: builds clean. (End-to-end runtime coverage of the binary comes from
the live `just render` in Task 5; the unit test in Task 2 already covers
`render_dsb` itself.)

- [ ] **Step 3: Commit**

```bash
git add goldens/tooling/src/bin/render-dsb.rs
git commit -m "feat(goldens): add the render-dsb binary (<in.dsb> <out.png>)"
```

---

### Task 4: `just render` recipe

**Files:**

- Modify: `justfile` (add the `render` recipe, sibling to `reprobe`)

- [ ] **Step 1: Add the recipe**

Append after the `reprobe` recipe in `justfile`:

```just
# Live render: import a Figma file to a .dsb, render it through the v0 Skia
# reference painter, and write a PNG to /tmp for review — the "renders through
# Skia" half of the full-real-file-import exit criterion (story Sf-1,
# docs/wip/2026-07-18-render-dsb-design.md). Depends on `wasm` (the importer
# loads dashc_wasm.wasm). Reads FIGMA_TOKEN from the macOS keychain
# (`security add-generic-password -a "$USER" -s figma-pat -w <token>`); the
# token is read, never printed — only its length. `root` is optional. Public
# Figma files are live-only: the .dsb and .png land in /tmp, never committed;
# the in-scope scratch is cleaned on exit, like reprobe's.
#
# Epic targets:
#   just render MRk9I5cYY6yJa8JhljzkBn 2411:10795  # first-light
#   just render S30AJmYfnDKGeSQmzuXEUk 1973:6580    # hero
render key root="": wasm
    #!/usr/bin/env bash
    set -euo pipefail
    token=$(security find-generic-password -a "$USER" -s figma-pat -w)
    export FIGMA_TOKEN="$token"
    echo "render: FIGMA_TOKEN loaded (${#token} chars)" >&2

    root_flag=""
    if [ -n "{{root}}" ]; then
        root_flag="--root {{root}}"
    fi

    tmp_dsb="importers/figma/.render-tmp.dsb"
    # The importer writes a sidecar next to `-o`'s output
    # (`<out minus .dsb>.vars.json`) — cleaned alongside the .dsb, not read here.
    tmp_vars="importers/figma/.render-tmp.vars.json"
    trap 'rm -f "$tmp_dsb" "$tmp_vars"' EXIT

    (cd importers/figma && deno task import "{{key}}" $root_flag -o .render-tmp.dsb)

    cp "$tmp_dsb" /tmp/render.dsb
    dsb_size=$(wc -c < /tmp/render.dsb | tr -d ' ')
    echo "render: imported /tmp/render.dsb (${dsb_size} bytes)" >&2

    cargo run --quiet -p goldens --bin render-dsb -- /tmp/render.dsb /tmp/render.png
    png_size=$(wc -c < /tmp/render.png | tr -d ' ')
    echo "RENDERED — wrote /tmp/render.png (${png_size} bytes)"
```

- [ ] **Step 2: Verify the recipe is parsed by `just`**

Run: `just --list | grep render`
Expected: the `render` recipe is listed with its `key root=""` parameters.

- [ ] **Step 3: Commit**

```bash
git add justfile
git commit -m "feat(repo): add the just render recipe to render a live Figma import"
```

---

### Task 5: Full build + live render of both epic targets

**Files:** none (verification only).

- [ ] **Step 1: Full build gate**

Run: `just build`
Expected: green — new src + binary compile, clippy clean, fmt clean, and the E7
oracle test still passes unchanged.

- [ ] **Step 2: Live render — first-light**

Run (token from the keychain; never echoed):

```bash
FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w) \
  just render MRk9I5cYY6yJa8JhljzkBn 2411:10795
cp /tmp/render.png /tmp/first-light.png
```

Expected: `RENDERED — wrote /tmp/render.png (<n> bytes)`. Record dimensions,
byte size, and a one-line sanity description of `/tmp/first-light.png`.

- [ ] **Step 3: Live render — hero**

```bash
FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w) \
  just render S30AJmYfnDKGeSQmzuXEUk 1973:6580
cp /tmp/render.png /tmp/hero.png
```

Expected: `RENDERED — wrote /tmp/render.png (<n> bytes)`. Record dimensions,
byte size, and a one-line sanity description of `/tmp/hero.png`. If a render is
blank/garbled or the API returns 401/403, report that — do not hide it.

- [ ] **Step 4: Confirm no third-party content is staged**

Run: `git status --short`
Expected: no `.dsb`/`.png`/file-JSON tracked or staged; only the intended source,
binary, justfile, and docs changes are on the branch.

---

## Self-Review

- **Spec coverage:** design §1 (public helper) → Task 2; §2 (binary) → Task 3;
  §3 (recipe) → Task 4; the unit test in the design's "Test strategy" → Task 2
  Step 3/6; the live-render verification → Task 5. E7 safety guardrail → the
  self-contained `render.rs` + the Task 2 Step 7 oracle re-run + Task 5 Step 4.
- **Placeholder scan:** none — every code step carries complete code; the one
  `todo!()` is the deliberate RED stub, replaced in Step 5.
- **Type consistency:** `render_dsb(&[u8]) -> Vec<u8>` is used identically by the
  unit test, the binary, and the recipe. Loader helper names (`oracle_typesetter`,
  `load_atlas`, `ATLAS_ASCII_DIR`, `ATLAS_ARABIC_DIR`) match the E7 oracle's for
  auditable 1:1 correspondence.

## Deviation from the design (recorded)

The design §1 suggests "move or re-expose from the test module" for the loader
helpers. The task's CRITICAL E7 SAFETY directive is stronger — "do NOT modify
render_oracle.rs ... ADD alongside ... leave the E7 gate byte-identical" — so
`render.rs` carries its own copy of `oracle_typesetter`, `load_atlas`,
`stage_text`, `text_runs`, and `origin_of` rather than moving them out of the
live test file. Cost: ~90 duplicated lines, which a later cleanup can dedup once
the v0.9 E7 track is idle (the design's own "Alternatives considered" already
defers that refactor for the same reason).
