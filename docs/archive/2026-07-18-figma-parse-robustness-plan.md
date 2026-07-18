# S2 — Figma REST parse robustness: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:test-driven-development.
> This plan is executed inline, task-by-task, in the same session (no subagent
> dispatch) — one conventional commit per task, `dashc` scope.

**Goal:** convert the three serde-strict Figma REST enums (`PaintTag`,
`ScaleMode`, `StrokeAlign` in `crates/dashc/src/figma/rest.rs`) to tolerant
`String` fields, and add one named catch-all diagnostic per enum at its walk
site in `crates/dashc/src/figma/mod.rs`, so an unknown Figma vocabulary value
(`PATTERN`, `STRETCH`, an unrecognized `strokeAlign`) degrades to a named
`figma.unsupported` (skip-with-warning under `EmitPolicy::Partial`, a blocking
error under `Strict`) instead of a hard `serde` parse crash that aborts the
whole compile.

**Architecture:** parse-side only. `rest.rs`'s three enum fields become
`String`, matching the file's ~20 other open-vocabulary `String` fields
(`Node::kind`, `Effect::kind`, …). The walk (`mod.rs`) already has a
`Result<_, CompileError::Unsupported>` → `blockers.push(what)` →
`Walk::unsupported_at` → `figma.unsupported` pipeline for every other
out-of-vocabulary construct (see `text_of`'s `blockers.push(format!("a {other}
line height"))` at mod.rs:1235 for the exact precedent this story extends to
paint kind, scale mode, and stroke align). No new plumbing.

**Tech Stack:** Rust, serde/serde_json, the existing `dashc` integration test
harness (`crates/dashc/tests/figma_lowering.rs`, `cargo test -p dashc`).

## Global Constraints

- R7 byte-reproducibility: a known-variant input must parse to the same
  value and emit byte-identically — no fixture/golden test may change
  observed behavior for `SOLID`/`GRADIENT_*`/`IMAGE`, `FILL`/`FIT`/`CROP`/`TILE`,
  or `INSIDE`/`CENTER`/`OUTSIDE`.
- P4: every unknown value becomes a **named** diagnostic carrying the actual
  value (not just "unknown"). Never use `#[serde(other)]` (it loses the value).
- P5: the producer (this walk) owns the diagnostic; parse only carries the
  string through.
- Parse-side only: do NOT touch `triage.rs`, `emit.rs`, the wasm ABI, the
  `.dsb`/dashbuf schema, or `importers/figma/render_oracle.ts`.
- Do NOT model `STRETCH` or `PATTERN` in `dashpaint`/`dashbuf` — diagnose only.
- Match existing code style (see the `text_of` blockers pattern above); keep
  changes surgical.
- Commit scope: `dashc` (see `.git-std.toml` — `dashc` is a valid scope; type
  `fix`, since this is a crash → graceful-degrade bug fix).

---

### Task 1: `Paint.kind` (`PaintTag`) → tolerant `String`

**Files:**

- Modify: `crates/dashc/src/figma/rest.rs` (delete the `PaintTag` enum at
  lines ~409-418; change `Paint.kind: PaintTag` at line 376 to `String`)
- Modify: `crates/dashc/src/figma/mod.rs`:
  - import at line 57: drop `PaintTag` from
    `use crate::figma::rest::{FigmaFile, Node, Paint, PaintTag};`
  - `image_refs` at line 380: `paint.kind == PaintTag::Image` →
    `paint.kind == "IMAGE"`
  - `paint_kind` (~992-1096): the `match paint.kind { PaintTag::Solid => …,
    PaintTag::GradientLinear | … => …, PaintTag::Image => … }` becomes a
    string match with a named catch-all arm
  - `stroke_of`'s `match stroke.kind { PaintTag::Solid => …, _ => Err(…"a
    non-solid stroke") }` (~1145-1156): syntax-only conversion to a string
    match; the existing generic wildcard message is unchanged (out of this
    story's scope — the design names only the three walk sites below)
  - `text_fill_of`'s `match fill.kind { PaintTag::Solid => …, _ => … }`
    (~1334-1346): same syntax-only conversion, message unchanged
- Modify: `crates/dashc/tests/figma_lowering.rs`:
  - import at line 17: drop `PaintTag` from
    `use dashc_wasm::figma::rest::{FigmaFile, PaintTag};`
  - `an_image_fill_carries_only_a_ref` (line 125):
    `assert_eq!(fill.kind, PaintTag::Image);` →
    `assert_eq!(fill.kind, "IMAGE");`
  - add the two new tests below (new section at end of file)

**Interfaces:**

- Consumes: `document_json`, `compile_figma_with_bindings_and_policy`,
  `EmitPolicy`, `Profile::Core`, `Severity`, `dashc_wasm::figma::rule::UNSUPPORTED`
  — all already imported in `figma_lowering.rs`.
- Produces: `paint_kind` now matches on `&str`; any value outside
  `SOLID`/`GRADIENT_LINEAR`/`GRADIENT_RADIAL`/`GRADIENT_ANGULAR`/
  `GRADIENT_DIAMOND`/`IMAGE` returns
  `Err(CompileError::Unsupported { what: format!("a {other} paint"), .. })`.

- [x] **Step 1: Write the two failing tests** (append to
      `crates/dashc/tests/figma_lowering.rs`, after
      `partial_still_refuses_a_no_content_file`)

```rust
// -- Unknown paint-vocabulary values degrade to a diagnostic, never a parse
// crash (story S2). Each fixture nests the unsupported construct one level
// under a valid root, mirroring frame_with_vector_child: the root itself must
// still lower, or Partial's "never ship zero content" gate (figma.no-content)
// would fire instead of the warning under test.

/// A child FRAME whose only fill is a Figma paint type this file did not
/// model until now (`PATTERN`, a repeating source-node tile — story S2 says
/// diagnose, never model).
fn frame_with_pattern_fill_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "swatch",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "PATTERN" }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_paint_type_naming_it() {
    let json = frame_with_pattern_fill_child().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_paint_type_naming_it() {
    let json = frame_with_pattern_fill_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown paint type");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message,
        "a PATTERN paint is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering strict_refuses_an_unknown_paint_type_naming_it partial_skips_and_warns_on_an_unknown_paint_type_naming_it`

Expected: compile failure at first (the test file still imports `PaintTag`
that is about to be removed — if editing test file first, temporarily leave
the old `PaintTag` import/assert alone and only add the new tests; the new
tests fail at runtime with `unknown variant PATTERN, expected one of SOLID,
GRADIENT_LINEAR, …` — a `serde_json::Error`, so `compile_figma_with_bindings_and_policy`
returns `Err(CompileError::Parse(_))`, and
`partial_skips_and_warns_on_an_unknown_paint_type_naming_it`'s `.expect(…)`
panics: "partial-emit returns a document even with an unknown paint type:
Parse(…unknown variant `PATTERN`…)". That panic **is** the red state this
story fixes.

- [x] **Step 3: Convert `rest.rs`**

In `crates/dashc/src/figma/rest.rs`:

```rust
// Paint.kind, was:
pub kind: PaintTag,
// becomes:
pub kind: String,
```

Delete the `PaintTag` enum (the 6-variant block starting
`#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]` /
`#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` / `pub enum PaintTag { … }`).

- [x] **Step 4: Convert `mod.rs`**

Import line 57:

```rust
use crate::figma::rest::{FigmaFile, Node, Paint};
```

`image_refs` (was line 380):

```rust
if paint.kind == "IMAGE"
    && let Some(image_ref) = &paint.image_ref
{
```

`paint_kind` (was lines 998-1046) — string match, named catch-all:

```rust
match paint.kind.as_str() {
    "SOLID" => {
        let color = paint
            .color
            .ok_or_else(|| unsupported("a SOLID with no color"))?;
        Ok(PaintKind::Solid {
            color: color_of(color, paint.opacity),
        })
    }
    "GRADIENT_LINEAR" | "GRADIENT_RADIAL" | "GRADIENT_ANGULAR" | "GRADIENT_DIAMOND" => {
        let handles = &paint.gradient_handle_positions;
        let [origin, primary, secondary] = handles[..] else {
            return Err(unsupported("a gradient without three handles"));
        };
        Ok(PaintKind::Gradient(Gradient {
            kind: match paint.kind.as_str() {
                "GRADIENT_LINEAR" => GradientKind::Linear,
                "GRADIENT_RADIAL" => GradientKind::Radial,
                "GRADIENT_ANGULAR" => GradientKind::Angular,
                _ => GradientKind::Diamond,
            },
            handle_origin: Vec2 { x: origin.x, y: origin.y },
            handle_primary: Vec2 { x: primary.x, y: primary.y },
            handle_secondary: Vec2 { x: secondary.x, y: secondary.y },
            stops: paint
                .gradient_stops
                .iter()
                .map(|s| GradientStop {
                    offset: s.position,
                    color: color_of(s.color, paint.opacity),
                })
                .collect(),
        }))
    }
    "IMAGE" => {
        // unchanged body — see Task 2 for the scale_mode match inside it
        …
    }
    other => Err(unsupported(&format!("a {other} paint"))),
}
```

(Keep the `IMAGE` arm's body byte-for-byte in this task — Task 2 edits only
its inner `scale_mode` match.)

`stroke_of`'s stroke-color match (was lines 1145-1156) — syntax-only, message
unchanged:

```rust
let color = match stroke.kind.as_str() {
    "SOLID" => stroke.color.ok_or_else(|| CompileError::Unsupported {
        path: path.to_string(),
        what: "a SOLID stroke with no color".to_string(),
    })?,
    _ => {
        return Err(CompileError::Unsupported {
            path: path.to_string(),
            what: "a non-solid stroke".to_string(),
        });
    }
};
```

`text_fill_of` (was lines 1334-1346) — syntax-only, message unchanged:

```rust
match fill.kind.as_str() {
    "SOLID" => {
        let Some(color) = fill.color else {
            blockers.push("a text SOLID fill with no color".to_string());
            return None;
        };
        Some(color_of(color, fill.opacity))
    }
    _ => {
        blockers.push("a non-solid text fill".to_string());
        None
    }
}
```

- [x] **Step 5: Fix the now-broken existing test**

In `crates/dashc/tests/figma_lowering.rs`:

```rust
// import line 17, was: use dashc_wasm::figma::rest::{FigmaFile, PaintTag};
use dashc_wasm::figma::rest::FigmaFile;

// an_image_fill_carries_only_a_ref, was: assert_eq!(fill.kind, PaintTag::Image);
assert_eq!(fill.kind, "IMAGE");
```

- [x] **Step 6: Run the full `dashc` test suite**

Run: `cargo test -p dashc`
Expected: PASS, including the two new tests and every existing
`figma_lowering`/`round_trip`/`abi`/`component_lowering`/`flex_lowering`/
`text_lowering`/`bindings_lowering` test (known-variant fixtures unchanged).

- [x] **Step 7: `cargo clippy` check**

Run: `cargo clippy -p dashc --all-targets -- -D warnings`
Expected: no warnings (no leftover unused `PaintTag` import anywhere).

- [x] **Step 8: Commit**

```bash
git add crates/dashc/src/figma/rest.rs crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "fix(dashc): degrade an unknown Figma paint type to a named diagnostic

An unknown Paint.kind (e.g. a PATTERN fill) used to fail the whole serde
parse, aborting the compile before the walk ever saw the rest of the file.
Paint.kind is now a tolerant String, like the file's other open-vocabulary
fields, and the walk names the unknown value in a figma.unsupported
diagnostic: a skip-with-warning under EmitPolicy::Partial, a blocking error
under Strict. Known variants (SOLID, GRADIENT_*, IMAGE) lower unchanged."
```

---

### Task 2: `Paint.scale_mode` (`ScaleMode`) → tolerant `String`

**Files:**

- Modify: `crates/dashc/src/figma/rest.rs` (delete the `ScaleMode` enum at
  lines ~420-427; change `Paint.scale_mode: Option<ScaleMode>` at line 393 to
  `Option<String>`)
- Modify: `crates/dashc/src/figma/mod.rs`: `paint_kind`'s `IMAGE` arm
  (~1069-1077), the `scale_mode` match inside `PaintKind::Image { .. }`
- Modify: `crates/dashc/tests/figma_lowering.rs`: add the two new tests below

**Interfaces:**

- Consumes: `IMAGE_REF`, `images()` (both already defined in
  `figma_lowering.rs`, lines ~169-179) — the STRETCH test must resolve the
  `imageRef` through `images()` or `paint_kind` returns
  `CompileError::UnresolvedImage` before ever reaching the `scale_mode`
  match (struct-literal fields evaluate left to right: `image` resolves
  before `scale_mode`).
- Produces: an unknown `scale_mode` value returns
  `Err(CompileError::Unsupported { what: format!("an image scaleMode {other}"), .. })`.

- [x] **Step 1: Write the two failing tests** (append after Task 1's tests)

```rust
/// A child FRAME whose image fill carries `scaleMode: STRETCH` — a
/// non-uniform scale-to-fill Figma supports that `dashpaint::ScaleMode`
/// does not (story S2 says diagnose, never model). Needs a resolvable
/// imageRef, or paint_kind fails with UnresolvedImage before ever reaching
/// the scaleMode match.
fn frame_with_stretch_image_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "photo",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "fills": [{ "type": "IMAGE", "scaleMode": "STRETCH", "imageRef": IMAGE_REF }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_scale_mode_naming_it() {
    let json = frame_with_stretch_image_child().to_string();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images(),
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_scale_mode_naming_it() {
    let json = frame_with_stretch_image_child().to_string();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images(),
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown scaleMode");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message,
        "an image scaleMode STRETCH is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering strict_refuses_an_unknown_scale_mode_naming_it partial_skips_and_warns_on_an_unknown_scale_mode_naming_it`
Expected: `.expect(…)` panics — the parse still hard-crashes on `STRETCH`
(`unknown variant STRETCH, expected one of FILL, FIT, CROP, TILE`) since
`ScaleMode` in `rest.rs` has not been converted yet. This is the exact crash
named in the design doc's "Why" section.

- [x] **Step 3: Convert `rest.rs`**

```rust
// Paint.scale_mode, was:
pub scale_mode: Option<ScaleMode>,
// becomes:
pub scale_mode: Option<String>,
```

Delete the `ScaleMode` enum (the `Fill, Fit, Crop, Tile` block).

- [x] **Step 4: Convert `mod.rs`**

Inside `paint_kind`'s `"IMAGE"` arm:

```rust
Ok(PaintKind::Image {
    image,
    scale_mode: match paint
        .scale_mode
        .as_deref()
        .ok_or_else(|| unsupported("an IMAGE fill with no scaleMode"))?
    {
        "FILL" => ScaleMode::Fill,
        "FIT" => ScaleMode::Fit,
        "CROP" => ScaleMode::Crop,
        "TILE" => ScaleMode::Tile,
        other => return Err(unsupported(&format!("an image scaleMode {other}"))),
    },
    transform: paint.image_transform.map(|[[a, b, tx], [c, d, ty]]| Mat23 {
        a, b, c, d, tx, ty,
    }),
    tile_scale: paint.scaling_factor.unwrap_or(1.0),
})
```

(`image` and the trailing fields are unchanged from Task 1 — only the
`scale_mode` match body changes.)

- [x] **Step 5: Run the full `dashc` test suite**

Run: `cargo test -p dashc`
Expected: PASS. In particular `an_image_fill_resolves_through_the_caller_supplied_map`,
`a_cropped_image_fill_lowers_its_crop_transform`, and
`a_tiled_image_fill_lowers_its_tile_scale` (all in `figma_lowering.rs`) must
still assert `ScaleMode::Fit`/`Crop`/`Tile` unchanged — these are the R7
regression guard for this task.

- [x] **Step 6: `cargo clippy` check**

Run: `cargo clippy -p dashc --all-targets -- -D warnings`
Expected: no warnings.

- [x] **Step 7: Commit**

```bash
git add crates/dashc/src/figma/rest.rs crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "fix(dashc): degrade an unknown Figma image scaleMode to a named diagnostic

An unknown Paint.scale_mode (e.g. STRETCH, a non-uniform scale-to-fill Figma
supports that dashpaint::ScaleMode does not model) used to fail the whole
serde parse — the exact crash that stopped the hero fixture from reaching
dashc. Paint.scale_mode is now a tolerant Option<String>; the walk names the
unknown value in a figma.unsupported diagnostic instead of aborting. Known
variants (FILL, FIT, CROP, TILE) lower unchanged."
```

---

### Task 3: `Node.stroke_align` (`StrokeAlign`) → tolerant `String`, and close out the module doc

**Files:**

- Modify: `crates/dashc/src/figma/rest.rs`:
  - delete the `StrokeAlign` enum (lines ~429-435)
  - change `Node.stroke_align: Option<StrokeAlign>` (line 49) to `Option<String>`
  - rewrite the module doc rationale at lines 8-10
- Modify: `crates/dashc/src/figma/mod.rs`: `stroke_of`'s `align` match
  (~1161-1165)
- Modify: `crates/dashc/tests/figma_lowering.rs`: add the two new tests below

**Interfaces:**

- Consumes: `dashpaint::StrokeAlign` (the _document_ stroke-align enum,
  imported from `dashpaint` at mod.rs:40 — unaffected, distinct type from the
  REST one being deleted).
- Produces: an unknown `strokeAlign` value returns
  `Err(CompileError::Unsupported { what: format!("a {other} stroke alignment"), .. })`.

- [x] **Step 1: Write the two failing tests** (append after Task 2's tests)

```rust
/// A child FRAME whose stroke carries a strokeAlign Figma might add that
/// this file has never modeled (synthetic — no captured fixture has one).
fn frame_with_unknown_stroke_align_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "odd-border",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
            "strokeAlign": "MIDDLE",
            "strokes": [{ "type": "SOLID", "color": { "r": 1.0, "g": 0.0, "b": 0.0, "a": 1.0 } }],
        }],
    }))
}

#[test]
fn strict_refuses_an_unknown_stroke_align_naming_it() {
    let json = frame_with_unknown_stroke_align_child().to_string();
    let images = BTreeMap::new();
    let result = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Strict,
    );
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_skips_and_warns_on_an_unknown_stroke_align_naming_it() {
    let json = frame_with_unknown_stroke_align_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .expect("partial-emit returns a document even with an unknown strokeAlign");
    assert!(!bytes.is_empty(), "a document is emitted");

    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    let [warning] = warnings[..] else {
        panic!("expected exactly one figma.unsupported, got {warnings:?}");
    };
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.message,
        "a MIDDLE stroke alignment is not in the document vocabulary yet",
        "the diagnostic must name the actual value (P4)",
    );
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dashc --test figma_lowering strict_refuses_an_unknown_stroke_align_naming_it partial_skips_and_warns_on_an_unknown_stroke_align_naming_it`
Expected: `.expect(…)` panics — `unknown variant MIDDLE, expected one of
INSIDE, CENTER, OUTSIDE`.

- [x] **Step 3: Convert `rest.rs`**

```rust
// Node.stroke_align, was:
pub stroke_align: Option<StrokeAlign>,
// becomes:
pub stroke_align: Option<String>,
```

Delete the `StrokeAlign` enum (the `Inside, Center, Outside` block).

Rewrite the module doc at lines 8-10, replacing:

```rust
//! Enum-valued fields deserialize into real enums, so an unknown value fails
//! the parse rather than silently lowering to a default. A silent default is
//! the silent drop P4 forbids.
```

with:

```rust
//! Every Figma-vocabulary field with a small closed set of values — a
//! paint's `type`, an image fill's `scaleMode`, a stroke's `align` — stays a
//! `String`, like the file's other open-vocabulary fields (`Node::kind`,
//! `Effect::kind`). An unknown value used to deserialize into a Rust enum and
//! fail the whole parse; the walk's named catch-all diagnostic is not a
//! silent default (P4), so a `String` field plus a walk-side verdict is now
//! this file's only pattern — parse never refuses on a value it does not
//! recognize.
```

- [x] **Step 4: Convert `mod.rs`**

```rust
align: match node.stroke_align.as_deref().unwrap_or("INSIDE") {
    "INSIDE" => StrokeAlign::Inside,
    "CENTER" => StrokeAlign::Center,
    "OUTSIDE" => StrokeAlign::Outside,
    other => {
        return Err(CompileError::Unsupported {
            path: path.to_string(),
            what: format!("a {other} stroke alignment"),
        });
    }
},
```

- [x] **Step 5: Run the full `dashc` test suite**

Run: `cargo test -p dashc`
Expected: PASS. `all_three_stroke_aligns_lower` and
`a_hidden_second_stroke_is_not_a_second_visible_stroke` (both in
`figma_lowering.rs`) are the R7 regression guard: `StrokeAlign::Inside/Center/Outside`
must still lower unchanged.

- [x] **Step 6: `cargo clippy` check**

Run: `cargo clippy -p dashc --all-targets -- -D warnings`
Expected: no warnings — confirms no leftover `rest::ScaleMode`/`rest::StrokeAlign`/
`PaintTag` reference anywhere in the crate.

Also grep to be sure:

```bash
grep -rn "PaintTag\|rest::ScaleMode\|rest::StrokeAlign" crates/dashc/src crates/dashc/tests
```

Expected: no output.

- [x] **Step 7: Commit**

```bash
git add crates/dashc/src/figma/rest.rs crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "fix(dashc): degrade an unknown Figma strokeAlign to a named diagnostic

An unknown Node.stroke_align used to fail the whole serde parse. It is now a
tolerant Option<String>, and the walk names the unknown value in a
figma.unsupported diagnostic. This closes the last of the three serde-strict
Figma enums (Paint.kind, Paint.scale_mode, Node.stroke_align); rest.rs's
module doc now describes the walk-catch-all pattern all three follow. Known
variants (INSIDE, CENTER, OUTSIDE) lower unchanged."
```

---

### Task 4: workspace-wide verification and the empirical hero/first-light re-probe

Not a code task — no commit. This is the story's acceptance gate.

- [x] **Step 1: `just build`**

Run: `just build`
Expected: green (workspace assemble + full check — `cargo test --workspace`,
clippy, fmt, dprint, markdownlint).

- [x] **Step 2: Rebuild wasm**

Run: `just wasm`
Expected: green — `dashc` builds for `wasm32-unknown-unknown` with the same
parse-side changes.

- [x] **Step 3: Re-probe the HERO under Partial**

```bash
FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w)
cd importers/figma
deno task import S30AJmYfnDKGeSQmzuXEUk --root 1973:6580 -o .probe.dsb
```

Never echo `$FIGMA_TOKEN` — only its length or the HTTP status if something
fails. Record:

- Exit code and whether `.probe.dsb` was written.
- If it emits: capture the sorted-unique warning list (this is the
  previously-masked frontier — the key deliverable).
- If it does not emit: capture the exact error, and whether it is
  `UnresolvedImage` (expected next hard blocker per the design's predicted
  frontier) or something else.
- If the token is missing or the API returns 401/403: report the status and
  move on — this is not a code defect in this story.

Clean up afterward: `rm -f .probe.dsb .probe.vars.json` (never commit probe
output).

- [x] **Step 4: Re-probe FIRST-LIGHT (no-regression check)**

```bash
just reprobe MRk9I5cYY6yJa8JhljzkBn 2411:10795
```

(Or the equivalent `deno task import` invocation if `just reprobe` does not
exist in this tree — check `justfile` first.) Expected: still emits, same as
before this story (S2 touches only unknown-enum parse paths; first-light's
known-variant fixture should show zero behavior change).

- [x] **Step 5: Write the final report**

Write to
`/private/tmp/claude-501/-Users-sebastientasson-Workspace-driftsys-dashscene-staging/2da6e6de-7893-4aa0-8061-7e0a885fcb7a/scratchpad/s2-report.md`:
per-task TDD evidence (test names, red→green), files changed, `just build`
result, the HERO re-probe outcome (emits? frontier warnings, or the next hard
error), the first-light no-regression confirmation, guardrail confirmations
(R7, no triage.rs/emit.rs/ABI/schema/oracle touch, no `#[serde(other)]`, no
`.dsb`/probe output committed), and any concerns.
