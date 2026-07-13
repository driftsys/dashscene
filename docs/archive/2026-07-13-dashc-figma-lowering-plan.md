# dashc Figma REST lowering — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lower the captured `v03-paint` Figma REST fixture into `Scd` and
compile it to a `.dsb` that loads in `dashscene-core` and renders through the
Skia painter.

**Architecture:** A new `figma` module inside `dashc` — the only Figma-aware
code in the Rust tree. It parses a partial REST model with serde, walks the
frame tree depth-first into `Scd`, and calls `dashscene-validator`'s import
gate (`triage`) for every construct outside the NOW band. `Scd`, `emit`, and
`compile` are unchanged. Image bytes arrive as a caller-supplied
`imageRef` map, so `dashc` performs no network I/O and stays wasm-clean.

**Tech Stack:** Rust 2024, serde + serde_json (already workspace-vendored),
`dashscene-validator` for verdicts, `dashpaint` for the paint vocabulary,
`dashscene-skia` (dev-dependency) for the render assertion.

Design: `docs/wip/2026-07-13-dashc-figma-lowering-design.md`.

## Global Constraints

- Edition 2024, `resolver = "3"`, `-D warnings` under clippy.
- `dashc`'s lib target is **`dashc_wasm`**, not `dashc` — tests
  `use dashc_wasm::{...}`.
- `dashc` must keep building for `wasm32-unknown-unknown` (`just wasm`, a
  required CI job). No network, no filesystem, no `std::time` in the lib.
- P1 — the document carries intent, never results. `absoluteRenderBounds` is
  never read.
- P4 — every out-of-profile construct is a named diagnostic, never a silent
  drop. Anything the v0.3 `Scd` cannot express fails loudly.
- P5 — the producer owns the Figma mapping; the validator owns the verdict.
- Commit style: conventional commits. Allowed scopes come from
  `.git-std.toml` (`corpus` is **not** a scope; use `repo`).
- Run `just fmt` before each commit; the pre-commit hook runs
  `cargo fmt --all` and `dprint fmt`.

## Signature refinements over the design doc

The design doc named `lower -> (Scd, Vec<Diagnostic>)` and
`compile_figma -> Result<Vec<u8>, Report>`. Writing the code against those
showed they cannot represent three real outcomes, so both widen:

- A **JSON parse failure** is not a `Report`.
- An **unresolved `imageRef`** (the caller's map lacks the hash) cannot be
  faked — the load gate rejects a zero-byte asset (`asset.image-no-bytes`).
- A construct the v0.3 `Scd` **cannot express** (a stacked fill, a shadow, a
  non-`FRAME` node) has no `Construct` variant, so it cannot be a
  `Diagnostic`. Dropping it silently violates P4, so it must be a hard error.

And `Result<Vec<u8>, Report>` would **discard warnings on the success path**,
which also violates P4. Success therefore returns the bytes _and_ the report.

```rust
pub enum CompileError {
    Parse(serde_json::Error),
    Unsupported { path: String, what: String },
    UnresolvedImage { path: String, image_ref: String },
    Diagnostics(Report),
}

pub fn lower(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Scd, Vec<Diagnostic>), CompileError>;

pub fn compile_figma(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Vec<u8>, Report), CompileError>;
```

## File structure

| File                                               | Responsibility                                        |
| -------------------------------------------------- | ----------------------------------------------------- |
| `crates/dashscene-validator/src/lib.rs`            | modify — `Report` gains `FromIterator` + `Extend`     |
| `crates/dashscene-validator/tests/triage.rs`       | modify — assembly tests                               |
| `crates/dashc/Cargo.toml`                          | modify — add `serde`, `serde_json`                    |
| `crates/dashc/src/figma/rest.rs`                   | create — the REST subset (serde types only)           |
| `crates/dashc/src/figma/triage.rs`                 | create — Figma construct → `Construct`                |
| `crates/dashc/src/figma/mod.rs`                    | create — `CompileError`, the DFS walk, paint lowering |
| `crates/dashc/src/lib.rs`                          | modify — `mod figma`, re-exports, `compile_figma`     |
| `crates/dashc/tests/figma_lowering.rs`             | create — fixture-driven acceptance                    |
| `crates/dashc/src/main.rs`, `docs/design/dashc.md` | modify — stale "not captured" text                    |

---

### Task 1: `Report` gains public assembly

`triage` returns a bare `Diagnostic`, but `Report::push` is `pub(crate)`, so
`dashc` cannot report what it triages. This closes the gap.

**Files:**

- Modify: `crates/dashscene-validator/src/lib.rs` (near `impl Report`, ~line 279)
- Test: `crates/dashscene-validator/tests/triage.rs`

**Interfaces:**

- Produces: `impl FromIterator<Diagnostic> for Report`,
  `impl Extend<Diagnostic> for Report`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashscene-validator/tests/triage.rs`:

```rust
#[test]
fn a_producer_assembles_a_report_from_its_own_diagnostics() {
    // The import gate hands back bare Diagnostics. A producer (dashc) must be
    // able to turn its own findings into the one Report type both gates use,
    // or P4's "never a silent drop" has no channel to speak on.
    let found = vec![
        triage(Construct::LayerBlur, Profile::Core, NodePath::new(1, "/a")),
        triage(
            Construct::NoiseOrTextureEffect,
            Profile::Core,
            NodePath::new(2, "/b"),
        ),
    ];

    let report: Report = found.into_iter().collect();

    assert_eq!(report.diagnostics().len(), 2);
    assert!(report.has_errors(), "the noise effect is an error");
    assert!(report.has(rule::LAYER_BLUR));
    assert!(report.has(rule::NOISE_OR_TEXTURE_EFFECT));
}

#[test]
fn a_report_merges_a_second_gates_diagnostics() {
    // compile_figma merges the import gate's findings with the load gate's
    // Report before deciding whether to emit.
    let mut report: Report = vec![triage(
        Construct::CornerSmoothing,
        Profile::Core,
        NodePath::new(0, "/a"),
    )]
    .into_iter()
    .collect();

    assert!(!report.has_errors(), "corner smoothing only warns");

    report.extend([triage(
        Construct::ProgressiveBlur,
        Profile::Core,
        NodePath::new(1, "/b"),
    )]);

    assert_eq!(report.diagnostics().len(), 2);
    assert!(report.has_errors(), "progressive blur is an error");
}
```

Extend the existing `use` line at the top of the file to import `Report` and
`rule`:

```rust
use dashscene_validator::{
    Construct, Diagnostic, Location, NodePath, Profile, Report, Severity, rule, triage,
};
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashscene-validator --test triage`
Expected: FAIL — `a value of type`Report`cannot be built from an iterator
over elements of type`Diagnostic``, and `no method named `extend``.

- [ ] **Step 3: Implement**

In `crates/dashscene-validator/src/lib.rs`, directly after the `impl Report`
block that ends with `pub(crate) fn push`:

```rust
/// A producer assembles its own findings into a `Report`.
///
/// The import gate (`triage`) hands back one `Diagnostic` at a time, and the
/// producer that owns the Figma mapping (`dashc`, P5) is the only code that
/// knows when it is done finding them. Without this, a producer could triage
/// a construct and then have no way to report it — a silent drop by
/// construction, which P4 forbids.
impl FromIterator<Diagnostic> for Report {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        Self {
            diagnostics: iter.into_iter().collect(),
        }
    }
}

/// Merges one gate's diagnostics into another's — `dashc` folds the load
/// gate's `Report` into the import gate's before deciding whether to emit.
impl Extend<Diagnostic> for Report {
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, iter: I) {
        self.diagnostics.extend(iter);
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashscene-validator --test triage`
Expected: PASS — 8 tests (the 6 existing plus the 2 new).

- [ ] **Step 5: Commit**

```bash
just fmt
git add crates/dashscene-validator/
git commit -m "feat(dashscene-validator): let a producer assemble a Report

triage hands back a bare Diagnostic and Report::push is pub(crate), so
dashc could triage a construct and then have no way to report it. P4
forbids a silent drop, so the producer needs the channel."
```

---

### Task 2: the Figma REST subset

**Files:**

- Modify: `crates/dashc/Cargo.toml`
- Create: `crates/dashc/src/figma/rest.rs`
- Modify: `crates/dashc/src/lib.rs` (add `mod figma;`)
- Create: `crates/dashc/src/figma/mod.rs` (module declarations only, for now)

**Interfaces:**

- Produces: `rest::{FigmaFile, Node, Paint, PaintTag, ScaleMode, StrokeAlign,
  Effect, Color, Rect, Vector, GradientStop}`.

- [ ] **Step 1: Add the dependencies**

In `crates/dashc/Cargo.toml`, under `[dependencies]`:

```toml
serde.workspace = true
serde_json.workspace = true
```

Both are already declared in the root `[workspace.dependencies]` and are in
`Cargo.lock`; `serde` already carries the `derive` feature.

- [ ] **Step 2: Write the failing test**

Create `crates/dashc/tests/figma_lowering.rs`:

```rust
//! The Figma REST front end, end to end (story #139):
//!
//!     Figma REST JSON → lower → Scd → validate → emit → .dsb
//!                                                        ↓
//!                                    dashscene-core → Skia painter
//!
//! Every assertion here is pinned by the captured corpus, not by a reading of
//! Figma's documentation: `v03-paint.json` is the emission fixture and
//! `effects-2025.json` is the diagnostic fixture (SCOPE_DECISIONS §8).

use dashc_wasm::figma::rest::{FigmaFile, PaintTag};

/// The designated input for this story (corpus/figma-fixtures/manifest.json).
const V03_PAINT: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");

/// The diagnostic fixture: every construct in it is REJECT-band, so it can
/// never emit a `.dsb`.
const EFFECTS_2025: &str = include_str!("../../../corpus/figma-fixtures/effects-2025.json");

fn parse(json: &str) -> FigmaFile {
    serde_json::from_str(json).expect("the captured fixture parses")
}

#[test]
fn the_fixture_parses_into_the_rest_subset() {
    let file = parse(V03_PAINT);

    let canvas = &file.document.children[0];
    assert_eq!(canvas.kind, "CANVAS");

    let root = &canvas.children[0];
    assert_eq!(root.name, "v03-paint");
    assert!(root.clips_content);

    let bbox = root.absolute_bounding_box.expect("the root frame has a box");
    assert_eq!((bbox.width, bbox.height), (960.0, 680.0));
}

#[test]
fn corner_radius_and_rectangle_corner_radii_are_mutually_exclusive() {
    // Figma nulls whichever does not apply. A lowering that read both would
    // be guessing; the capture settles it.
    let file = parse(V03_PAINT);
    let uniform = find(&file, "corners-uniform");
    let per_corner = find(&file, "corners-per-corner");

    assert_eq!(uniform.corner_radius, Some(16.0));
    assert_eq!(uniform.rectangle_corner_radii, None);

    assert_eq!(per_corner.corner_radius, None);
    assert_eq!(
        per_corner.rectangle_corner_radii,
        Some([0.0, 24.0, 4.0, 48.0]),
    );
}

#[test]
fn stroke_weight_and_align_are_present_even_with_no_stroke() {
    // The trap: a lowering that gated the stroke on `strokeWeight` being
    // present would give every unstroked frame a 1px stroke.
    let file = parse(V03_PAINT);
    let unstroked = find(&file, "fill-solid");

    assert!(unstroked.strokes.is_empty());
    assert_eq!(unstroked.stroke_weight, Some(1.0));
    assert_eq!(unstroked.stroke_align.is_some(), true);
}

#[test]
fn an_image_fill_carries_only_a_ref() {
    // No bytes anywhere in the file JSON — the whole reason the caller
    // supplies an imageRef→bytes map (design D1).
    let file = parse(V03_PAINT);
    let node = find(&file, "image-fit");

    let fill = &node.fills[0];
    assert_eq!(fill.kind, PaintTag::Image);
    assert_eq!(
        fill.image_ref.as_deref(),
        Some("390616a0e7321eddb464388366d9a2a1bcb7f4c3"),
    );
    assert!(fill.color.is_none(), "an image fill carries no color");
}

#[test]
fn progressive_blur_is_a_layer_blur_carrying_a_blur_type() {
    // The type alone cannot decide the band: plain LAYER_BLUR warns,
    // LAYER_BLUR + blurType PROGRESSIVE rejects.
    let file = parse(EFFECTS_2025);
    let node = find(&file, "progressive-blur");

    let effect = &node.effects[0];
    assert_eq!(effect.kind, "LAYER_BLUR");
    assert_eq!(effect.blur_type.as_deref(), Some("PROGRESSIVE"));
}

/// Depth-first search for a node by name. Panics if absent — a fixture that
/// lost a node should fail loudly, not skip the assertion.
fn find<'a>(file: &'a FigmaFile, name: &str) -> &'a dashc_wasm::figma::rest::Node {
    fn walk<'a>(
        node: &'a dashc_wasm::figma::rest::Node,
        name: &str,
    ) -> Option<&'a dashc_wasm::figma::rest::Node> {
        if node.name == name {
            return Some(node);
        }
        node.children.iter().find_map(|child| walk(child, name))
    }
    walk(&file.document, name).unwrap_or_else(|| panic!("fixture has no node named {name}"))
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p dashc --test figma_lowering`
Expected: FAIL to compile — `could not find`figma`in`dashc_wasm``.

- [ ] **Step 4: Write the REST subset**

Create `crates/dashc/src/figma/rest.rs`:

```rust
//! The Figma REST subset the v0.3 lowering reads.
//!
//! Deliberately partial: only the fields the v0.3 paint vocabulary needs.
//! Every shape here is pinned by `corpus/figma-fixtures/v03-paint.json`, not
//! by a reading of Figma's documentation — the lowering was deferred out of
//! #16 precisely so it would never be written against a guess (P5).
//!
//! Enum-valued fields deserialize into real enums, so an unknown value fails
//! the parse rather than silently lowering to a default. A silent default is
//! the silent drop P4 forbids.

use serde::Deserialize;

/// A `GET /v1/files/:key` response. Only `document` is read.
#[derive(Debug, Deserialize)]
pub struct FigmaFile {
    pub document: Node,
}

/// One node of the Figma tree.
///
/// `kind` stays a `String` rather than an enum: Figma's node vocabulary is
/// open (TEXT, VECTOR, INSTANCE, …) and v0.3 handles only `FRAME`. The walk
/// rejects the rest by name, so a new Figma node type is a loud error here
/// rather than a parse failure of the whole file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub fills: Vec<Paint>,
    #[serde(default)]
    pub strokes: Vec<Paint>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub stroke_weight: Option<f32>,
    #[serde(default)]
    pub stroke_align: Option<StrokeAlign>,
    /// Mutually exclusive with `rectangle_corner_radii`; Figma nulls the
    /// other.
    #[serde(default)]
    pub corner_radius: Option<f32>,
    /// `[top_left, top_right, bottom_right, bottom_left]` — the same order as
    /// `dashpaint::CornerRadii`'s fields.
    #[serde(default)]
    pub rectangle_corner_radii: Option<[f32; 4]>,
    #[serde(default)]
    pub corner_smoothing: Option<f32>,
    #[serde(default)]
    pub clips_content: bool,
    #[serde(default)]
    pub blend_mode: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Page-absolute. The lowering subtracts the parent's origin to get the
    /// parent-relative intent `Scd` wants. Never `absoluteRenderBounds`,
    /// which is a *result* (P1).
    #[serde(default)]
    pub absolute_bounding_box: Option<Rect>,
}

/// One fill or stroke.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paint {
    #[serde(rename = "type")]
    pub kind: PaintTag,
    #[serde(default)]
    pub blend_mode: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Multiplies the paint's alpha. Absent means fully opaque.
    #[serde(default)]
    pub opacity: Option<f32>,
    pub color: Option<Color>,
    /// Origin, primary-axis end, secondary-axis end — normalized to the
    /// node's box. `dashpaint::Gradient` stores this convention verbatim.
    #[serde(default)]
    pub gradient_handle_positions: Vec<Vector>,
    #[serde(default)]
    pub gradient_stops: Vec<GradientStop>,
    #[serde(default)]
    pub scale_mode: Option<ScaleMode>,
    /// The content hash of an image asset. The bytes are **not** in this
    /// JSON; the caller resolves the ref (design D1).
    #[serde(default)]
    pub image_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaintTag {
    Solid,
    GradientLinear,
    GradientRadial,
    GradientAngular,
    GradientDiamond,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScaleMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

/// An effect. `kind` stays a `String` for the same reason `Node::kind` does:
/// Figma's effect vocabulary is open, and the triage table (not the parser)
/// decides which band each one falls in.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Present on `LAYER_BLUR`. `PROGRESSIVE` moves it from the LATER band to
    /// the REJECT band, so the effect type alone cannot decide the verdict.
    #[serde(default)]
    pub blur_type: Option<String>,
}

/// Non-premultiplied, 0.0–1.0 per channel — the same convention as
/// `dashpaint::Color`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

/// Figma calls the stop's location `position`; `dashpaint` calls it `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}
```

Create `crates/dashc/src/figma/mod.rs`:

```rust
//! The Figma REST front end — the only Figma-aware code in the Rust tree.
//!
//! Figma compatibility is a property of one producer (P5), so nothing
//! downstream of this module knows what a `FRAME` or an `imageRef` is.

pub mod rest;
```

In `crates/dashc/src/lib.rs`, beside the existing `mod emit; mod scd;`:

```rust
pub mod figma;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p dashc --test figma_lowering`
Expected: PASS — 5 tests.

- [ ] **Step 6: Commit**

```bash
just fmt
git add crates/dashc/
git commit -m "feat(dashc): parse the Figma REST subset the v0.3 lowering needs

Every shape is pinned by the captured v03-paint fixture. The tests assert
the two traps a guess would have fallen into: cornerRadius and
rectangleCornerRadii are mutually exclusive, and strokeWeight is present
even when the strokes array is empty."
```

---

### Task 3: the triage mapping

The producer owns the mapping; the validator owns the verdict (P5).

**Files:**

- Create: `crates/dashc/src/figma/triage.rs`
- Modify: `crates/dashc/src/figma/mod.rs` (add `mod triage;`)

**Interfaces:**

- Consumes: `rest::{Node, Paint, Effect}` (Task 2).
- Produces:
  `pub(crate) fn constructs_of(node: &Node) -> Result<Vec<Construct>, String>`
  — the constructs a node carries that sit outside the NOW band. `Err(what)`
  names a construct the v0.3 `Scd` cannot express at all, which the caller
  turns into `CompileError::Unsupported`.

- [ ] **Step 1: Write the failing test**

Create `crates/dashc/src/figma/triage.rs` with only the tests at the bottom
(the module body comes in Step 3), or add this to the file once written. For
TDD, write the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::figma::rest::FigmaFile;

    const EFFECTS_2025: &str =
        include_str!("../../../../corpus/figma-fixtures/effects-2025.json");
    const V03_PAINT: &str = include_str!("../../../../corpus/figma-fixtures/v03-paint.json");

    fn find<'a>(file: &'a FigmaFile, name: &str) -> &'a Node {
        fn walk<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
            if node.name == name {
                return Some(node);
            }
            node.children.iter().find_map(|child| walk(child, name))
        }
        walk(&file.document, name).expect("fixture has the node")
    }

    fn file(json: &str) -> FigmaFile {
        serde_json::from_str(json).expect("the fixture parses")
    }

    #[test]
    fn noise_and_texture_are_the_reject_band() {
        let f = file(EFFECTS_2025);
        assert_eq!(
            constructs_of(find(&f, "noise")).unwrap(),
            vec![Construct::NoiseOrTextureEffect],
        );
        assert_eq!(
            constructs_of(find(&f, "texture")).unwrap(),
            vec![Construct::NoiseOrTextureEffect],
        );
    }

    #[test]
    fn a_layer_blur_rejects_only_when_it_is_progressive() {
        // The discrimination the capture forced: the effect type alone cannot
        // decide the band.
        let f = file(EFFECTS_2025);
        assert_eq!(
            constructs_of(find(&f, "progressive-blur")).unwrap(),
            vec![Construct::ProgressiveBlur],
        );
    }

    #[test]
    fn the_paint_fixture_carries_no_out_of_profile_construct() {
        // v03-paint is entirely NOW-band, so it must triage to nothing at all
        // — otherwise it could never emit, and the manifest says emits: true.
        let f = file(V03_PAINT);
        for name in [
            "fill-solid",
            "gradient-angular",
            "image-fit",
            "stroke-outside",
            "corners-uniform",
            "corners-per-corner",
            "clip-frame",
        ] {
            assert_eq!(
                constructs_of(find(&f, name)).unwrap(),
                vec![],
                "{name} must be in the NOW band",
            );
        }
    }

    #[test]
    fn a_shadow_is_unsupported_rather_than_silently_dropped() {
        // Baked shadows are NOW-band per DESIGN §10.1, but Scd cannot express
        // them, so there is no Construct to triage. P4 forbids dropping it in
        // silence, so it fails loudly instead.
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "card",
            "type": "FRAME",
            "effects": [{ "type": "DROP_SHADOW", "visible": true }],
        }))
        .unwrap();

        assert_eq!(constructs_of(&node), Err("effect DROP_SHADOW".to_string()));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p dashc figma::triage`
Expected: FAIL to compile — `cannot find function`constructs_of``.

- [ ] **Step 3: Implement the mapping**

The body of `crates/dashc/src/figma/triage.rs`, above the test module:

```rust
//! Figma constructs mapped onto `dashscene-validator`'s `Construct`.
//!
//! The producer owns the mapping, the validator owns the verdict (P5). A
//! `figma` module inside the validator was rejected on exactly those grounds
//! (`docs/decisions/validator-three-gates.md`).
//!
//! Only vocabulary *outside* the NOW band appears here. DESIGN §10.1's NOW
//! band — the four gradient kinds, image fills and scale modes, axis-aligned
//! and rounded clip — is simply the schema, and needs no verdict.

use dashscene_validator::Construct;

use crate::figma::rest::{Effect, Node};

/// The out-of-profile constructs `node` carries.
///
/// `Err(what)` names a construct the v0.3 `Scd` cannot express at all. It has
/// no `Construct` variant, so it cannot become a `Diagnostic` — and P4
/// forbids dropping it in silence, so the caller fails the compile loudly.
pub(crate) fn constructs_of(node: &Node) -> Result<Vec<Construct>, String> {
    let mut found = Vec::new();

    for effect in node.effects.iter().filter(|e| e.visible != Some(false)) {
        found.push(effect_construct(effect)?);
    }

    // Figma carries a blendMode on the node and on every paint. Both are
    // triaged: a paint-level blend mode is just as invisible a drop.
    if !is_plain_blend(node.blend_mode.as_deref()) {
        found.push(Construct::AdvancedBlendMode);
    }
    for paint in node.fills.iter().chain(node.strokes.iter()) {
        if !is_plain_blend(paint.blend_mode.as_deref()) {
            found.push(Construct::AdvancedBlendMode);
        }
    }

    if node.corner_smoothing.is_some_and(|s| s > 0.0) {
        found.push(Construct::CornerSmoothing);
    }

    Ok(found)
}

fn effect_construct(effect: &Effect) -> Result<Construct, String> {
    match effect.kind.as_str() {
        "NOISE" | "TEXTURE" => Ok(Construct::NoiseOrTextureEffect),
        // A progressive blur serializes as a LAYER_BLUR carrying
        // `blurType: PROGRESSIVE` — pinned by effects-2025.json. Plain layer
        // blur only warns; progressive blur is an error.
        "LAYER_BLUR" => Ok(match effect.blur_type.as_deref() {
            Some("PROGRESSIVE") => Construct::ProgressiveBlur,
            _ => Construct::LayerBlur,
        }),
        "BACKGROUND_BLUR" => Ok(Construct::BackdropBlur),
        // Shadows are NOW-band, but Scd cannot express them yet. No Construct
        // fits, so it fails loudly rather than vanishing.
        other => Err(format!("effect {other}")),
    }
}

/// `PASS_THROUGH` is a frame's default and `NORMAL` a paint's; anything else
/// is an advanced blend mode.
fn is_plain_blend(mode: Option<&str>) -> bool {
    matches!(mode, None | Some("NORMAL") | Some("PASS_THROUGH"))
}
```

Add to `crates/dashc/src/figma/mod.rs`:

```rust
pub(crate) mod triage;
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashc figma::triage`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
just fmt
git add crates/dashc/
git commit -m "feat(dashc): map Figma constructs onto the validator's vocabulary

The producer owns the mapping, the validator owns the verdict (P5). A
progressive blur serializes as a LAYER_BLUR carrying blurType PROGRESSIVE,
so the effect type alone cannot decide the band — the capture settled that,
a guess would not have."
```

---

### Task 4: the lowering walk

**Files:**

- Modify: `crates/dashc/src/figma/mod.rs` (the walk, `CompileError`)
- Test: `crates/dashc/tests/figma_lowering.rs`

**Interfaces:**

- Consumes: `rest::*` (Task 2), `triage::constructs_of` (Task 3).
- Produces:
  `pub enum CompileError { Parse, Unsupported, UnresolvedImage, Diagnostics }`
  and
  `pub fn lower(&FigmaFile, Profile, &BTreeMap<String, ImageAsset>)
   -> Result<(Scd, Vec<Diagnostic>), CompileError>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashc/tests/figma_lowering.rs`, and extend its imports:

```rust
use std::collections::BTreeMap;

use dashc_wasm::figma::{CompileError, lower};
use dashpaint::{
    CornerRadii, GradientKind, ImageAsset, ImageFormat, PaintKind, ScaleMode, StrokeAlign,
};
use dashscene_validator::Profile;

/// A 1x1 red PNG — the smallest asset that actually decodes. The fixture's
/// image fill is an `imageRef` with no bytes anywhere in the JSON, so the
/// caller supplies them (design D1). In production that is the Deno importer
/// resolving `GET /images`; here it is this constant.
fn png_pixel() -> Vec<u8> {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    PNG.to_vec()
}

const IMAGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";

fn images() -> BTreeMap<String, ImageAsset> {
    BTreeMap::from([(
        IMAGE_REF.to_string(),
        ImageAsset {
            format: ImageFormat::Png,
            bytes: png_pixel(),
        },
    )])
}

fn lowered() -> dashc_wasm::Scd {
    let (scd, diagnostics) = lower(&parse(V03_PAINT), Profile::Core, &images())
        .expect("the paint fixture is entirely NOW-band");
    assert!(
        diagnostics.is_empty(),
        "v03-paint must triage clean, or it could never emit",
    );
    scd
}

/// The node named `name`, and its index in the rect table.
fn node<'a>(scd: &'a dashc_wasm::Scd, name: &str) -> (u32, &'a dashc_wasm::ScdNode) {
    scd.nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.name.as_deref() == Some(name))
        .map(|(i, n)| (i as u32, n))
        .unwrap_or_else(|| panic!("no lowered node named {name}"))
}

#[test]
fn the_root_frame_drops_its_page_position() {
    // Where a frame sits on the Figma canvas is a page-layout artifact, not
    // intent (P1).
    let scd = lowered();
    let (index, root) = node(&scd, "v03-paint");

    assert_eq!(index, 0, "the root is the first rect-table entry");
    assert_eq!(root.parent, None);
    assert_eq!((root.box2d.x, root.box2d.y), (0.0, 0.0));
    assert_eq!((root.box2d.width, root.box2d.height), (960.0, 680.0));
}

#[test]
fn a_childs_box_is_relative_to_its_parent() {
    // Figma's absoluteBoundingBox is page-absolute; Scd's Box2D is
    // parent-relative intent. The overflow child is the sharpest case: it sits
    // at an absolute x of -28 inside a parent at 32, so it must land at -60.
    let scd = lowered();
    let (_, child) = node(&scd, "overflow-child");

    assert_eq!((child.box2d.x, child.box2d.y), (-60.0, -30.0));
    assert_eq!((child.box2d.width, child.box2d.height), (520.0, 180.0));
}

#[test]
fn a_clipping_frame_carries_the_clip_intent() {
    let scd = lowered();
    let (clip_index, clip_frame) = node(&scd, "clip-frame");
    let (_, child) = node(&scd, "overflow-child");

    assert!(clip_frame.paint.as_ref().expect("has paint").clip);
    assert_eq!(child.parent, Some(clip_index));
}

#[test]
fn all_three_stroke_aligns_lower() {
    // absoluteRenderBounds differs from absoluteBoundingBox for CENTER and
    // OUTSIDE by exactly the stroke expansion. It is a *result*, so P1 says
    // the lowering must never read it — the box plus the align is the intent.
    let scd = lowered();

    for (name, align) in [
        ("stroke-inside", StrokeAlign::Inside),
        ("stroke-center", StrokeAlign::Center),
        ("stroke-outside", StrokeAlign::Outside),
    ] {
        let (_, n) = node(&scd, name);
        let stroke = n.paint.as_ref().unwrap().entry.stroke.expect("has a stroke");

        assert_eq!(stroke.align, align, "{name}");
        assert_eq!(stroke.width, 8.0, "{name}");
        // The box is the authored one, not the render bounds.
        assert_eq!((n.box2d.width, n.box2d.height), (200.0, 140.0), "{name}");
    }
}

#[test]
fn an_unstroked_frame_gets_no_stroke() {
    // strokeWeight is 1 on every node in the fixture, stroked or not.
    let scd = lowered();
    let (_, n) = node(&scd, "fill-solid");

    assert!(n.paint.as_ref().unwrap().entry.stroke.is_none());
}

#[test]
fn both_corner_forms_lower() {
    let scd = lowered();

    let (_, uniform) = node(&scd, "corners-uniform");
    assert_eq!(
        uniform.paint.as_ref().unwrap().entry.corners,
        CornerRadii {
            top_left: 16.0,
            top_right: 16.0,
            bottom_right: 16.0,
            bottom_left: 16.0,
        },
    );

    let (_, per_corner) = node(&scd, "corners-per-corner");
    assert_eq!(
        per_corner.paint.as_ref().unwrap().entry.corners,
        CornerRadii {
            top_left: 0.0,
            top_right: 24.0,
            bottom_right: 4.0,
            bottom_left: 48.0,
        },
    );
}

#[test]
fn all_four_gradient_kinds_lower() {
    let scd = lowered();

    for (name, kind) in [
        ("gradient-linear", GradientKind::Linear),
        ("gradient-radial", GradientKind::Radial),
        ("gradient-angular", GradientKind::Angular),
        ("gradient-diamond", GradientKind::Diamond),
    ] {
        let (_, n) = node(&scd, name);
        let Some(PaintKind::Gradient(g)) = &n.paint.as_ref().unwrap().entry.fill else {
            panic!("{name} did not lower to a gradient");
        };

        assert_eq!(g.kind, kind, "{name}");
        assert_eq!(g.stops.len(), 3, "{name}");
        // Figma calls it `position`, dashpaint calls it `offset`.
        assert_eq!(g.stops[1].offset, 0.5, "{name}");
    }
}

#[test]
fn an_image_fill_resolves_through_the_caller_supplied_map() {
    let scd = lowered();
    let (_, n) = node(&scd, "image-fit");

    let Some(PaintKind::Image {
        image, scale_mode, ..
    }) = &n.paint.as_ref().unwrap().entry.fill
    else {
        panic!("image-fit did not lower to an image fill");
    };

    assert_eq!(*scale_mode, ScaleMode::Fit);
    assert_eq!(scd.images[*image as usize].bytes, png_pixel());
}

#[test]
fn an_unresolved_image_ref_fails_loudly() {
    // The load gate rejects a zero-byte asset (asset.image-no-bytes), so the
    // lowering cannot invent one. Better a named error than a fabricated pixel.
    let empty = BTreeMap::new();
    let err = lower(&parse(V03_PAINT), Profile::Core, &empty).unwrap_err();

    let CompileError::UnresolvedImage { image_ref, path } = err else {
        panic!("expected an UnresolvedImage error");
    };
    assert_eq!(image_ref, IMAGE_REF);
    assert!(path.contains("image-fit"), "the error names the node: {path}");
}

#[test]
fn the_reject_fixture_triages_every_construct_as_an_error() {
    let (_, diagnostics) = lower(&parse(EFFECTS_2025), Profile::Core, &images())
        .expect("the effects fixture lowers; its constructs are diagnosed, not fatal");

    let rules: Vec<&str> = diagnostics.iter().map(|d| d.rule).collect();

    assert!(rules.contains(&"profile.noise-or-texture-effect"));
    assert!(rules.contains(&"profile.progressive-blur"));
    assert!(
        diagnostics
            .iter()
            .all(|d| d.severity == dashscene_validator::Severity::Error),
        "every construct in effects-2025 is REJECT-band",
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering`
Expected: FAIL to compile — `cannot find function`lower``.

- [ ] **Step 3: Implement the walk**

Replace `crates/dashc/src/figma/mod.rs` with:

```rust
//! The Figma REST front end — the only Figma-aware code in the Rust tree.
//!
//! Figma compatibility is a property of one producer (P5), so nothing
//! downstream of this module knows what a `FRAME` or an `imageRef` is: the
//! walk lowers Figma's vocabulary into `Scd`, and `Scd` is Figma-agnostic.
//!
//! The lowering does no I/O. `dashc` compiles to `wasm32-unknown-unknown`, so
//! it cannot fetch — and Figma serializes an image fill as a bare `imageRef`
//! with no bytes. The caller resolves refs and passes them in.

pub mod rest;
pub(crate) mod triage;

use std::collections::BTreeMap;
use std::fmt;

use dashpaint::{
    Color, CornerRadii, Gradient, GradientKind, GradientStop, ImageAsset, PaintEntry, PaintKind,
    ScaleMode, Stroke, StrokeAlign, Vec2,
};
use dashscene_validator::{Diagnostic, NodePath, Profile, Report};

use crate::figma::rest::{self, FigmaFile, Node, Paint, PaintTag};
use crate::scd::{Box2D, Paint as ScdPaint, Scd, ScdNode};

/// Why a Figma file could not be compiled at all.
///
/// Distinct from a `Diagnostic`, which is a verdict *about* a document that
/// was understood. These are the cases where lowering cannot proceed.
#[derive(Debug)]
pub enum CompileError {
    /// The input was not the Figma REST JSON it claimed to be.
    Parse(serde_json::Error),
    /// A construct the v0.3 `Scd` cannot express. It has no `Construct`
    /// variant, so it cannot be a diagnostic — and P4 forbids dropping it in
    /// silence, so it stops the compile instead. Each one is tracked as debt.
    Unsupported { path: String, what: String },
    /// An image fill whose `imageRef` the caller did not resolve. The load
    /// gate rejects a zero-byte asset, so no placeholder can be invented.
    UnresolvedImage { path: String, image_ref: String },
    /// The document carried at least one error-severity diagnostic, so R6
    /// blocks emission.
    Diagnostics(Report),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "not valid Figma REST JSON: {e}"),
            Self::Unsupported { path, what } => {
                write!(f, "{path}: {what} is not in the v0.3 vocabulary")
            }
            Self::UnresolvedImage { path, image_ref } => {
                write!(f, "{path}: no image supplied for imageRef {image_ref}")
            }
            Self::Diagnostics(report) => write!(f, "{report}"),
        }
    }
}

impl std::error::Error for CompileError {}

/// Lowers a parsed Figma file into an `Scd` plus the diagnostics its
/// out-of-profile constructs earned.
///
/// `images` maps an `imageRef` to its bytes. Figma's `GET /file` carries no
/// image bytes, and `dashc` cannot fetch them (wasm), so whoever *can* fetch
/// — the Deno importer — resolves them and passes them here.
pub fn lower(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Scd, Vec<Diagnostic>), CompileError> {
    let root = root_frame(&file.document)?;
    let origin = box_of(root, "")?;

    let mut walk = Walk {
        scd: Scd::new(),
        diagnostics: Vec::new(),
        image_of: BTreeMap::new(),
        profile,
        images,
    };
    // The root drops its page position: (0, 0, w, h).
    walk.visit(root, None, (origin.x, origin.y), "")?;

    Ok((walk.scd, walk.diagnostics))
}

/// The first `FRAME` under the first `CANVAS`.
///
/// v0.3 exports one root frame. Declared roots plus a reachability closure
/// (DESIGN §6.1) is the v0.7 story; until then the rule is positional and
/// stated rather than inferred.
fn root_frame(document: &Node) -> Result<&Node, CompileError> {
    document
        .children
        .iter()
        .find(|n| n.kind == "CANVAS")
        .and_then(|canvas| canvas.children.iter().find(|n| n.kind == "FRAME"))
        .ok_or_else(|| CompileError::Unsupported {
            path: "/".to_string(),
            what: "a document with no root FRAME under its first CANVAS".to_string(),
        })
}

fn box_of(node: &Node, path: &str) -> Result<rest::Rect, CompileError> {
    node.absolute_bounding_box
        .ok_or_else(|| CompileError::Unsupported {
            path: path.to_string(),
            what: format!("node {} has no absoluteBoundingBox", node.name),
        })
}

struct Walk<'a> {
    scd: Scd,
    diagnostics: Vec<Diagnostic>,
    /// Interns `imageRef` → image-table index, so two nodes sharing one image
    /// share one asset.
    image_of: BTreeMap<String, u32>,
    profile: Profile,
    images: &'a BTreeMap<String, ImageAsset>,
}

impl Walk<'_> {
    /// Depth-first, parent before child: `Scd::push` order is the rect-table
    /// index, and `emit` does not reorder.
    ///
    /// `parent_origin` is the parent's *absolute* origin — what turns Figma's
    /// page-absolute box into the parent-relative intent `Scd` wants.
    fn visit(
        &mut self,
        node: &Node,
        parent: Option<u32>,
        parent_origin: (f32, f32),
        prefix: &str,
    ) -> Result<(), CompileError> {
        let path = format!("{prefix}/{}", node.name);

        if node.kind != "FRAME" {
            return Err(CompileError::Unsupported {
                path,
                what: format!("node type {}", node.kind),
            });
        }
        if node.visible == Some(false) {
            return Err(CompileError::Unsupported {
                path,
                what: "a hidden node".to_string(),
            });
        }
        if node.opacity.is_some_and(|o| o < 1.0) {
            return Err(CompileError::Unsupported {
                path,
                what: "node opacity".to_string(),
            });
        }

        let bbox = box_of(node, &path)?;
        // Built before the push: `paint_of` borrows `self` mutably (it interns
        // image assets), so it cannot run inside the `push` argument.
        let paint = self.paint_of(node, &path)?;
        let index = self.scd.push(ScdNode {
            name: Some(node.name.clone()),
            parent,
            box2d: Box2D {
                x: bbox.x - parent_origin.0,
                y: bbox.y - parent_origin.1,
                width: bbox.width,
                height: bbox.height,
            },
            paint,
        });

        // The import gate: the producer maps, the validator decides (P5).
        let constructs = triage::constructs_of(node).map_err(|what| CompileError::Unsupported {
            path: path.clone(),
            what,
        })?;
        for construct in constructs {
            self.diagnostics.push(dashscene_validator::triage(
                construct,
                self.profile,
                NodePath::new(index, path.clone()),
            ));
        }

        for child in &node.children {
            self.visit(child, Some(index), (bbox.x, bbox.y), &path)?;
        }
        Ok(())
    }

    fn paint_of(&mut self, node: &Node, path: &str) -> Result<Option<ScdPaint>, CompileError> {
        let entry = PaintEntry {
            fill: self.fill_of(node, path)?,
            stroke: self.stroke_of(node, path)?,
            corners: corners_of(node),
        };

        // A layout-only container draws nothing but still occupies a rect-table
        // slot. A clipping frame with no paint still needs its clip intent.
        if entry == PaintEntry::default() && !node.clips_content {
            return Ok(None);
        }
        Ok(Some(ScdPaint {
            entry,
            clip: node.clips_content,
        }))
    }

    fn fill_of(&mut self, node: &Node, path: &str) -> Result<Option<PaintKind>, CompileError> {
        let mut visible = node.fills.iter().filter(|p| p.visible != Some(false));
        let Some(fill) = visible.next() else {
            return Ok(None);
        };
        if visible.next().is_some() {
            // PaintEntry.fill is one Option<PaintKind>; Figma's fills is an
            // array. Stacking is an Scd expressiveness gap (debt), not a
            // triage gap — and a silent drop would violate P4.
            return Err(CompileError::Unsupported {
                path: path.to_string(),
                what: "more than one visible fill".to_string(),
            });
        }
        self.paint_kind(fill, path).map(Some)
    }

    fn paint_kind(&mut self, paint: &Paint, path: &str) -> Result<PaintKind, CompileError> {
        let unsupported = |what: &str| CompileError::Unsupported {
            path: path.to_string(),
            what: what.to_string(),
        };

        match paint.kind {
            PaintTag::Solid => {
                let color = paint.color.ok_or_else(|| unsupported("a SOLID with no color"))?;
                Ok(PaintKind::Solid {
                    color: color_of(color, paint.opacity),
                })
            }
            PaintTag::GradientLinear
            | PaintTag::GradientRadial
            | PaintTag::GradientAngular
            | PaintTag::GradientDiamond => {
                let handles = &paint.gradient_handle_positions;
                let [origin, primary, secondary] = handles[..] else {
                    return Err(unsupported("a gradient without three handles"));
                };
                Ok(PaintKind::Gradient(Gradient {
                    kind: match paint.kind {
                        PaintTag::GradientLinear => GradientKind::Linear,
                        PaintTag::GradientRadial => GradientKind::Radial,
                        PaintTag::GradientAngular => GradientKind::Angular,
                        _ => GradientKind::Diamond,
                    },
                    handle_origin: Vec2 {
                        x: origin.x,
                        y: origin.y,
                    },
                    handle_primary: Vec2 {
                        x: primary.x,
                        y: primary.y,
                    },
                    handle_secondary: Vec2 {
                        x: secondary.x,
                        y: secondary.y,
                    },
                    stops: paint
                        .gradient_stops
                        .iter()
                        // Figma calls the location `position`; dashpaint calls
                        // it `offset`.
                        .map(|s| GradientStop {
                            offset: s.position,
                            color: color_of(s.color, paint.opacity),
                        })
                        .collect(),
                }))
            }
            PaintTag::Image => {
                let image_ref = paint
                    .image_ref
                    .as_deref()
                    .ok_or_else(|| unsupported("an IMAGE fill with no imageRef"))?;

                let image = match self.image_of.get(image_ref) {
                    Some(index) => *index,
                    None => {
                        let asset = self.images.get(image_ref).ok_or_else(|| {
                            CompileError::UnresolvedImage {
                                path: path.to_string(),
                                image_ref: image_ref.to_string(),
                            }
                        })?;
                        let index = self.scd.push_image(asset.clone());
                        self.image_of.insert(image_ref.to_string(), index);
                        index
                    }
                };

                Ok(PaintKind::Image {
                    image,
                    scale_mode: match paint
                        .scale_mode
                        .ok_or_else(|| unsupported("an IMAGE fill with no scaleMode"))?
                    {
                        rest::ScaleMode::Fill => ScaleMode::Fill,
                        rest::ScaleMode::Fit => ScaleMode::Fit,
                        rest::ScaleMode::Crop => ScaleMode::Crop,
                        rest::ScaleMode::Tile => ScaleMode::Tile,
                    },
                    transform: None,
                    tile_scale: 1.0,
                })
            }
        }
    }

    fn stroke_of(&mut self, node: &Node, path: &str) -> Result<Option<Stroke>, CompileError> {
        // strokeWeight and strokeAlign are present even when `strokes` is
        // empty (pinned by the fixture), so the stroke is gated on the array,
        // never on the weight.
        let Some(stroke) = node.strokes.iter().find(|p| p.visible != Some(false)) else {
            return Ok(None);
        };

        let color = match stroke.kind {
            PaintTag::Solid => stroke.color.ok_or_else(|| CompileError::Unsupported {
                path: path.to_string(),
                what: "a SOLID stroke with no color".to_string(),
            })?,
            // v0.3 strokes are solid-only (dashpaint::Stroke).
            _ => {
                return Err(CompileError::Unsupported {
                    path: path.to_string(),
                    what: "a non-solid stroke".to_string(),
                });
            }
        };

        Ok(Some(Stroke {
            width: node.stroke_weight.unwrap_or(1.0),
            align: match node.stroke_align.unwrap_or(rest::StrokeAlign::Inside) {
                rest::StrokeAlign::Inside => StrokeAlign::Inside,
                rest::StrokeAlign::Center => StrokeAlign::Center,
                rest::StrokeAlign::Outside => StrokeAlign::Outside,
            },
            color: color_of(color, stroke.opacity),
        }))
    }
}

/// `cornerRadius` and `rectangleCornerRadii` are mutually exclusive — Figma
/// nulls whichever does not apply. `rectangleCornerRadii` is
/// `[top_left, top_right, bottom_right, bottom_left]`, matching
/// `CornerRadii`'s field order.
fn corners_of(node: &Node) -> CornerRadii {
    if let Some([top_left, top_right, bottom_right, bottom_left]) = node.rectangle_corner_radii {
        return CornerRadii {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        };
    }
    let r = node.corner_radius.unwrap_or(0.0);
    CornerRadii {
        top_left: r,
        top_right: r,
        bottom_right: r,
        bottom_left: r,
    }
}

/// Figma's paint `opacity` multiplies the color's alpha. Ignoring it would be
/// a silent drop (P4); it is two lines, so it is not one.
fn color_of(color: rest::Color, opacity: Option<f32>) -> Color {
    Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a * opacity.unwrap_or(1.0),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashc --test figma_lowering`
Expected: PASS — 15 tests.

- [ ] **Step 5: Commit**

```bash
just fmt
cargo clippy -p dashc --all-targets -- -D warnings
git add crates/dashc/
git commit -m "feat(dashc): lower Figma REST JSON into an Scd

Geometry, paint, and the import gate. Figma's absoluteBoundingBox is
page-absolute, so the walk subtracts the parent's origin; absoluteRenderBounds
is never read, because it is a result rather than intent (P1).

Anything the v0.3 Scd cannot express — a stacked fill, a shadow, a non-FRAME
node — fails loudly rather than vanishing (P4)."
```

---

### Task 5: the emission gate

**Files:**

- Modify: `crates/dashc/src/lib.rs`
- Test: `crates/dashc/tests/figma_lowering.rs`

**Interfaces:**

- Consumes: `figma::lower`, `figma::CompileError` (Task 4); `emit` (existing);
  `Report: FromIterator + Extend` (Task 1).
- Produces: `pub fn compile_figma(&str, Profile, &BTreeMap<String, ImageAsset>)
  -> Result<(Vec<u8>, Report), CompileError>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashc/tests/figma_lowering.rs`:

```rust
use dashc_wasm::compile_figma;
use dashpaint::Painter;
use dashscene_core::{Arena, load_document};
use dashscene_skia::SkiaPainter;

#[test]
fn the_fixture_compiles_loads_and_renders() {
    // Story #139's acceptance criterion, end to end.
    let (bytes, report) =
        compile_figma(V03_PAINT, Profile::Core, &images()).expect("v03-paint compiles");

    assert!(report.is_empty(), "the paint fixture is entirely NOW-band");

    let document = dashbuf::root_as_document(&bytes).expect("a valid buffer");
    let mut arena = Arena::new();
    load_document(&document, &mut arena);

    let scene = arena.committed();
    assert_eq!(scene.rects().len(), 14, "13 frames plus the root");

    let mut painter = SkiaPainter::new(960, 680);
    painter.paint(scene.rects(), scene.paints(), scene.images(), scene.clips());
    let png = painter.png_bytes();

    assert!(!png.is_empty(), "the fixture rasterizes");
    assert_eq!(&png[1..4], b"PNG", "and it is a PNG");
}

#[test]
fn emission_from_the_fixture_is_byte_reproducible() {
    // R7: same input → byte-identical document.
    let (first, _) = compile_figma(V03_PAINT, Profile::Core, &images()).unwrap();
    let (second, _) = compile_figma(V03_PAINT, Profile::Core, &images()).unwrap();

    assert_eq!(first, second, "emission is not deterministic");
}

#[test]
fn the_reject_fixture_is_refused_rather_than_emitted() {
    // effects-2025 is a DIAGNOSTIC fixture (SCOPE §8): everything in it is
    // REJECT-band, so under R6 it can never emit a .dsb. The report must name
    // each construct — never a silent drop (P4).
    let err = compile_figma(EFFECTS_2025, Profile::Core, &images())
        .expect_err("a REJECT-band document must never emit");

    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };

    assert!(report.has_errors());
    assert!(report.has("profile.noise-or-texture-effect"));
    assert!(report.has("profile.progressive-blur"));
}

#[test]
fn malformed_json_is_a_parse_error_not_a_panic() {
    let err = compile_figma("{ not json", Profile::Core, &images()).unwrap_err();
    assert!(matches!(err, CompileError::Parse(_)));
}
```

`assert_eq!(scene.rects().len(), 14, ...)` — verify the real count first with
`jq '[.document] | .. | objects | select(.type=="FRAME")] | length'
corpus/figma-fixtures/v03-paint.json` and use that number.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering`
Expected: FAIL to compile — `cannot find function`compile_figma``.

- [ ] **Step 3: Implement the gate**

In `crates/dashc/src/lib.rs`, after the existing `compile`:

```rust
/// Compiles Figma REST JSON to a `.dsb`.
///
/// Two gates, one report. The **import gate** (`triage`) runs while lowering,
/// on constructs that have no representation in the `.dsb` schema at all; the
/// **load gate** (`validate_document`) runs on the emitted document. An error
/// from either blocks emission (R6). Warnings do not block, so they come back
/// with the bytes — dropping them on the success path would be the silent drop
/// P4 forbids.
///
/// `images` resolves the `imageRef` of every image fill; see `figma::lower`.
pub fn compile_figma(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
) -> Result<(Vec<u8>, Report), CompileError> {
    let file: FigmaFile = serde_json::from_str(json).map_err(CompileError::Parse)?;
    let (scd, found) = figma::lower(&file, profile, images)?;

    let mut report: Report = found.into_iter().collect();

    let bytes = emit(&scd);
    let document = dashbuf::root_as_document(&bytes)
        .expect("the emitter always produces a structurally valid buffer");
    report.extend(
        dashscene_validator::validate_document(&document)
            .diagnostics()
            .iter()
            .cloned(),
    );

    if report.has_errors() {
        return Err(CompileError::Diagnostics(report));
    }
    Ok((bytes, report))
}
```

And the imports/re-exports at the top of `crates/dashc/src/lib.rs`:

```rust
use std::collections::BTreeMap;

use dashpaint::ImageAsset;
use dashscene_validator::{Profile, Report};

pub mod figma;
mod emit;
mod scd;

pub use emit::emit;
pub use figma::{CompileError, lower, rest::FigmaFile};
pub use scd::{Box2D, Paint, Scd, ScdNode};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashc`
Expected: PASS — the 6 existing `round_trip` tests plus 19 in
`figma_lowering`.

- [ ] **Step 5: Verify the wasm build still holds**

Run: `just wasm`
Expected: success. `serde_json` is `std`-but-wasm-clean, and nothing added
does I/O.

- [ ] **Step 6: Commit**

```bash
just fmt
git add crates/dashc/
git commit -m "feat(dashc): compile Figma REST JSON to a .dsb

Two gates, one report: the import gate runs while lowering, the load gate on
the emitted document, and an error from either blocks emission (R6). Warnings
come back with the bytes rather than being dropped on the success path."
```

---

### Task 6: retire the stale docs, file the debt

**Files:**

- Modify: `crates/dashc/src/lib.rs`, `crates/dashc/src/main.rs` (crate docs)
- Modify: `crates/dashc/tests/round_trip.rs` (module doc)
- Modify: `docs/design/dashc.md`

- [ ] **Step 1: Fix the stale "not captured" claims**

Three files say the v0.3 fixture has not been captured. PR #142 made that
false. Search and replace the claim, not just the words:

Run: `grep -rn "not been captured\|holds only its manifest\|not yet wired" crates/dashc/ docs/design/dashc.md`

Rewrite each to describe the shipped state: the fixture is
`corpus/figma-fixtures/v03-paint.json`, and `compile_figma` lowers it.

- [ ] **Step 2: Run the full build**

Run: `just build`
Expected: green — this is what CI runs.

- [ ] **Step 3: Commit**

```bash
just fmt
git add crates/dashc/ docs/design/dashc.md
git commit -m "docs(dashc): the v0.3 Figma fixture is captured and lowered

The crate docs still said the fixture did not exist and the Figma path was
not wired. PR #142 captured it and this branch wired it."
```

- [ ] **Step 4: File the debt issues**

Each is a construct the v0.3 `Scd` cannot express, which the lowering
currently rejects loudly rather than dropping in silence. One issue each,
labeled `debt`, linked to #139:

```bash
gh issue create --label debt --title "debt(dashc): Scd cannot express stacked fills, so the Figma lowering rejects them" --body "PaintEntry.fill is one Option<PaintKind>; Figma's fills is an array. Every node in the v03-paint fixture carries exactly one visible fill, so #139 is unaffected, but a stacked-fill node is CompileError::Unsupported rather than lowered.

Same genre as #140: an Scd expressiveness gap, not a triage gap. Rejecting loudly keeps P4, but a real Figma file will hit it.

Found while implementing #139."

gh issue create --label debt --title "debt(dashc): the Figma lowering rejects node opacity, rotation, and hidden nodes" --body "Scd has no field for node opacity or rotation, and no way to represent a hidden node without shifting the DFS indices every later node depends on. The lowering returns CompileError::Unsupported for each rather than dropping them silently (P4).

Found while implementing #139."

gh issue create --label debt --title "debt(dashc): baked shadows are NOW-band but Scd cannot express them" --body "DESIGN §10.1 puts baked drop/inner shadows in the NOW band, but Scd has no effects vocabulary, so there is no Construct to triage a DROP_SHADOW onto and no field to lower it into. The lowering returns CompileError::Unsupported.

Effects enter the schema at v0.8; this is the tracking issue for the Figma side.

Found while implementing #139."

gh issue create --label debt --title "debt(dashscene-validator): variable-width stroke is REJECT-band with no Construct variant" --body "SCOPE_DECISIONS §8 lists variable-width stroke among the REJECT-band 2025 Figma Draw effects, but dashscene_validator::Construct has no variant for it, so a producer cannot triage it.

It is 'pendingManual' in the fixture manifest and absent from effects-2025.json, so #139 could not test it.

Found while implementing #139."
```

- [ ] **Step 5: Run the story's definition of done**

Per AGENTS.md: `/code-review` on the diff, every finding captured as a
checklist in the PR description, critical findings fixed before merge, one
`debt` issue per minor finding.

Run: `just verify`
Expected: commit-message lint over the branch range passes, then `just build`
green.

## Self-review

**Spec coverage.** D1 (image map) → Tasks 2, 4. D2 (`Report` assembly) →
Task 1. D3 (no `Scd` widening) → no task touches `Scd`, by construction.
D4 (negative gap stays in core) → no task touches `arena.rs`. The lowering
table → Task 4. The triage table → Task 3. The emission gate → Task 5. Known
gaps → Task 6 Step 4. Acceptance 1–4 → Task 5; acceptance 5 (`just wasm`,
`just build`) → Task 5 Step 5 and Task 6 Step 2.

**Type consistency.** `PaintTag` is the REST fill-type enum throughout (not
`PaintKindTag`, an earlier draft name). `rest::ScaleMode`/`rest::StrokeAlign`
are always qualified to distinguish them from `dashpaint`'s same-named types.
`CompileError` is the single error type across `lower` and `compile_figma`.
`constructs_of` returns `Result<Vec<Construct>, String>` in Task 3 and is
consumed as such in Task 4.

**Known soft spot.** The rect count in Task 5
(`scene.rects().len() == 14`) is asserted from a `jq` count, not assumed —
the step says to verify it first.
