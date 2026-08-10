# Atlas Pipeline (#27) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dashscene-typeset::atlas` — font + charset in, byte-reproducible
MSDF atlas (`atlas.png`, keyed by glyph id) + metrics blob
(`atlas.metrics`) out.

**Architecture:** Thin, deterministic wrapper around a version-pinned
external `msdf-atlas-gen` binary: cmap closure (ttf-parser) → tool
invocation with canonical args → metrics assembly (ttf-parser
authoritative for font metrics/advances; tool JSON for plane/atlas
bounds) → postcard-serialized blob. See
`docs/wip/2026-07-12-atlas-pipeline-design.md`.

**Tech Stack:** ttf-parser 0.25, serde + serde_json, postcard, tempfile,
external msdf-atlas-gen 1.4.0.

## Global Constraints

- Atlas keyed by glyph id, never codepoint (DESIGN §7.2).
- Tool version pinned: exactly `1.4.0`; refuse anything else with a
  named error.
- Fixed generator args: `-type msdf -size <px_per_em> -pxrange
  <px_range> -potr -yorigin bottom -nokerning -seed <seed>`; defaults
  32 / 4 / 1 (Q-1 decision record).
- All output vectors sorted (glyphs by gid, missing codepoints
  ascending); glyph id 0 (`.notdef`) always included.
- Tool-dependent tests self-skip when the tool is absent, but panic if
  `DASHSCENE_REQUIRE_ATLAS_TOOL` is set (CI sets it).
- House style: std-only error enum (no thiserror), doc comments citing
  DESIGN sections, `edition = "2024"`, clippy `-D warnings` clean.
- Commits: conventional, scope `dashscene-typeset` (or `ci` / `repo`
  where appropriate), `Co-Authored-By: Claude Fable 5
  <noreply@anthropic.com>` trailer.

---

### Task 1: font asset + charset→gid closure

**Files:**

- Create: `corpus/fonts/noto-sans/NotoSans-Regular.ttf` (from
  notofonts/latin-greek-cyrillic release `NotoSans-v2.015`, the
  `unhinted/ttf` build — the runtime never uses TT hints)
- Create: `corpus/fonts/noto-sans/OFL.txt` (from the same zip)
- Create: `corpus/fonts/noto-sans/README.md`
- Modify: `Cargo.toml` (workspace) — no new deps this task; ttf-parser
  already reserved
- Modify: `crates/dashscene-typeset/Cargo.toml` — add
  `ttf-parser.workspace = true`
- Create: `crates/dashscene-typeset/src/atlas/mod.rs` (module shell +
  `AtlasError` skeleton variants needed so far)
- Create: `crates/dashscene-typeset/src/atlas/closure.rs`
- Modify: `crates/dashscene-typeset/src/lib.rs` — `pub mod atlas;`
- Test: `crates/dashscene-typeset/src/atlas/closure.rs` (`#[cfg(test)]`
  in-module; pure, no tool)

**Interfaces:**

- Consumes: nothing prior.
- Produces:
  `pub struct Closure { pub glyph_ids: Vec<u16>, pub missing_codepoints: Vec<u32> }`;
  `pub fn charset_closure(face: &ttf_parser::Face<'_>, charset: &BTreeSet<char>, extra_glyph_ids: &BTreeSet<u16>) -> Closure`;
  font fixture path used by every later test:
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf")`.

- [ ] **Step 1: Fetch and commit the font asset**

```bash
cd "$(git rev-parse --show-toplevel)"
curl -sL -o /tmp/notosans.zip https://github.com/notofonts/latin-greek-cyrillic/releases/download/NotoSans-v2.015/NotoSans-v2.015.zip
unzip -o -q /tmp/notosans.zip -d /tmp/notosans
mkdir -p corpus/fonts/noto-sans
cp /tmp/notosans/NotoSans/unhinted/ttf/NotoSans-Regular.ttf corpus/fonts/noto-sans/
cp /tmp/notosans/NotoSans/OFL.txt corpus/fonts/noto-sans/
```

(Adjust the inner zip layout if it differs — the invariant is: the
`unhinted/ttf` Regular weight plus the OFL license file, both committed.)

`corpus/fonts/noto-sans/README.md`:

```markdown
# Noto Sans (Latin/Greek/Cyrillic)

    source   github.com/notofonts/latin-greek-cyrillic
    release  NotoSans-v2.015
    build    unhinted/ttf (the runtime never uses TT hints)
    license  OFL 1.1 — see OFL.txt

Test and golden fixture font for the text stack (#27, #28, #29, #30).
Do not modify the file; replace it wholesale (and update this README)
when a version bump is deliberate.
```

- [ ] **Step 2: Write the failing closure test**

`crates/dashscene-typeset/src/atlas/closure.rs` (bottom of the new file;
the impl above it starts as `todo!()`-free — write the test first, the
module shell only declares the items):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
    );

    fn face(data: &[u8]) -> ttf_parser::Face<'_> {
        ttf_parser::Face::parse(data, 0).expect("fixture font parses")
    }

    #[test]
    fn resolves_covered_codepoints_to_sorted_unique_gids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['B', 'A', 'A', 'a'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert!(c.missing_codepoints.is_empty());
        // .notdef (0) is always included, plus one gid per distinct char.
        assert_eq!(c.glyph_ids.len(), 4);
        assert_eq!(c.glyph_ids[0], 0);
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, c.glyph_ids, "sorted and deduplicated");
        assert!(c.glyph_ids[1..].iter().all(|&g| g != 0));
    }

    #[test]
    fn reports_uncovered_codepoints_sorted() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        // Syriac letters — absent from a Latin/Greek/Cyrillic font.
        let charset: BTreeSet<char> = ['\u{0712}', '\u{0710}', 'A'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert_eq!(c.missing_codepoints, vec![0x0710, 0x0712]);
        assert_eq!(c.glyph_ids.len(), 2); // .notdef + 'A'
    }

    #[test]
    fn merges_extra_glyph_ids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['A'].into_iter().collect();
        let extras: BTreeSet<u16> = [700u16, 3].into_iter().collect();
        let c = charset_closure(&face, &charset, &extras);
        assert!(c.glyph_ids.contains(&700));
        assert!(c.glyph_ids.contains(&3));
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, c.glyph_ids);
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p dashscene-typeset atlas::closure`
Expected: compile error — `charset_closure` not defined.

- [ ] **Step 4: Implement the module shell and closure**

`crates/dashscene-typeset/src/atlas/mod.rs`:

```rust
//! Build-time atlas pipeline (DESIGN_1.md §7.2, build-time half):
//! font + charset → MSDF glyph atlas keyed by GLYPH ID + metrics blob.
//!
//! The charset is an input parameter (per-locale charsets arrive with
//! the v0.6 charset story); glyph coverage beyond cmap (GSUB closure)
//! enters through `extra_glyph_ids` until then.

mod closure;

pub use closure::{charset_closure, Closure};
```

`crates/dashscene-typeset/src/atlas/closure.rs` (above the tests):

```rust
//! Charset → glyph-id closure via cmap (DESIGN_1.md §7.2).
//!
//! v0.5 scope: nominal cmap lookups only. Contextual/ligature glyphs
//! that only shaping can discover are supplied via `extra_glyph_ids`
//! (the v0.6 charset story extends closure over GSUB).

use std::collections::BTreeSet;

/// The glyph-id set an atlas must cover, plus the charset entries the
/// font cannot represent (a named diagnostic surface, R6 — the caller
/// decides severity, nothing is dropped silently).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    /// Sorted, deduplicated; always contains glyph id 0 (`.notdef`) so
    /// painters can draw a visible fallback for unmapped input.
    pub glyph_ids: Vec<u16>,
    /// Charset codepoints without a cmap entry, ascending.
    pub missing_codepoints: Vec<u32>,
}

/// Resolves `charset` through the font's cmap and merges
/// `extra_glyph_ids`.
pub fn charset_closure(
    face: &ttf_parser::Face<'_>,
    charset: &BTreeSet<char>,
    extra_glyph_ids: &BTreeSet<u16>,
) -> Closure {
    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0);
    let mut missing = Vec::new();
    for &c in charset {
        match face.glyph_index(c) {
            Some(gid) => {
                gids.insert(gid.0);
            }
            None => missing.push(c as u32),
        }
    }
    gids.extend(extra_glyph_ids.iter().copied());
    Closure {
        glyph_ids: gids.into_iter().collect(),
        missing_codepoints: missing,
    }
}
```

`crates/dashscene-typeset/src/lib.rs` — replace the stub body comment,
keep the crate doc line, add `pub mod atlas;`.

`crates/dashscene-typeset/Cargo.toml` `[dependencies]`:

```toml
ttf-parser.workspace = true
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p dashscene-typeset atlas::closure`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add corpus/fonts/noto-sans crates/dashscene-typeset Cargo.lock
git commit -m "feat(dashscene-typeset): add cmap charset→glyph-id closure + Noto Sans fixture font"
```

---

### Task 2: metrics model + canonical blob round-trip

**Files:**

- Modify: `Cargo.toml` (workspace `[workspace.dependencies]`) — add
  `serde = { version = "1", features = ["derive"] }` and
  `postcard = { version = "1", features = ["use-std"] }`
- Modify: `crates/dashscene-typeset/Cargo.toml` — add both
- Create: `crates/dashscene-typeset/src/atlas/metrics.rs`
- Modify: `crates/dashscene-typeset/src/atlas/mod.rs` — `mod metrics;`
  re-exports + `AtlasError`
- Test: in-module `#[cfg(test)]` (pure, no tool)

**Interfaces:**

- Consumes: fixture font path from Task 1.
- Produces (used by Tasks 3–5):

```rust
pub const FORMAT_VERSION: u32 = 1;
pub struct AtlasMetrics { pub format_version: u32, pub generator: GeneratorInfo,
    pub font: FontMetrics, pub atlas: AtlasInfo, pub glyphs: Vec<GlyphEntry>,
    pub missing_codepoints: Vec<u32> }
pub struct GeneratorInfo { pub tool_version: String, pub args: Vec<String> }
pub struct FontMetrics { pub units_per_em: u16, pub ascender: i16,
    pub descender: i16, pub line_gap: i16 }
pub struct AtlasInfo { pub width: u32, pub height: u32, pub px_per_em: u16,
    pub distance_range_px: f32 }
pub struct GlyphEntry { pub glyph_id: u16, pub advance_units: u16,
    pub plane_em: Option<[f32; 4]>, pub atlas_px: Option<[f32; 4]> }
impl AtlasMetrics { pub fn to_bytes(&self) -> Vec<u8>;
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AtlasError>; }
pub fn font_metrics(face: &ttf_parser::Face<'_>) -> FontMetrics;
```

- `AtlasError` (in `mod.rs`) grows: `Metrics(String)` variant.

- [ ] **Step 1: Write the failing tests**

`crates/dashscene-typeset/src/atlas/metrics.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
    );

    fn sample() -> AtlasMetrics {
        AtlasMetrics {
            format_version: FORMAT_VERSION,
            generator: GeneratorInfo {
                tool_version: "1.4.0".into(),
                args: vec!["-type".into(), "msdf".into()],
            },
            font: FontMetrics { units_per_em: 1000, ascender: 1069, descender: -293, line_gap: 0 },
            atlas: AtlasInfo { width: 128, height: 128, px_per_em: 32, distance_range_px: 4.0 },
            glyphs: vec![
                GlyphEntry { glyph_id: 0, advance_units: 600, plane_em: None, atlas_px: None },
                GlyphEntry {
                    glyph_id: 36,
                    advance_units: 639,
                    plane_em: Some([-0.01, -0.02, 0.65, 0.72]),
                    atlas_px: Some([0.5, 0.5, 24.5, 26.5]),
                },
            ],
            missing_codepoints: vec![0x0710],
        }
    }

    #[test]
    fn blob_round_trips() {
        let m = sample();
        let bytes = m.to_bytes();
        let back = AtlasMetrics::from_bytes(&bytes).expect("valid blob");
        assert_eq!(m, back);
    }

    #[test]
    fn blob_bytes_are_canonical() {
        assert_eq!(sample().to_bytes(), sample().to_bytes());
    }

    #[test]
    fn rejects_unknown_format_version() {
        let mut m = sample();
        m.format_version = FORMAT_VERSION + 1;
        let bytes = m.to_bytes();
        assert!(AtlasMetrics::from_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(AtlasMetrics::from_bytes(&[0xff, 0x00, 0x13]).is_err());
    }

    #[test]
    fn extracts_font_metrics_from_fixture() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = ttf_parser::Face::parse(&data, 0).expect("parses");
        let fm = font_metrics(&face);
        assert_eq!(fm.units_per_em, 1000);
        assert!(fm.ascender > 0);
        assert!(fm.descender < 0);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dashscene-typeset atlas::metrics`
Expected: compile error — types not defined.

- [ ] **Step 3: Implement**

`crates/dashscene-typeset/src/atlas/metrics.rs`:

```rust
//! The metrics blob: everything a painter or the typesetter needs to
//! consume an atlas (DESIGN_1.md §7.2). Serialized with postcard;
//! vectors are pre-sorted so the encoding is canonical (R7).
//!
//! Fixed by `FORMAT_VERSION` 1 (not stored per-field): atlas kind is
//! MSDF; plane bounds are y-up, em units, baseline origin; atlas texel
//! bounds have a bottom-left origin (`-yorigin bottom`). The painter's
//! screen-pixel range is `distance_range_px * screen_px_per_em /
//! px_per_em`.

use serde::{Deserialize, Serialize};

use super::AtlasError;

/// Bump on any breaking change to the blob layout.
pub const FORMAT_VERSION: u32 = 1;

/// Provenance: rerunning `args` against the same font and tool version
/// must reproduce the artifacts byte-for-byte (R7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorInfo {
    pub tool_version: String,
    pub args: Vec<String>,
}

/// Font-wide vertical metrics in raw font units (hhea — the same
/// numbers FreeType reads); consumers normalize by `units_per_em`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontMetrics {
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub line_gap: i16,
}

/// Parameters of the generated atlas image.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AtlasInfo {
    pub width: u32,
    pub height: u32,
    pub px_per_em: u16,
    pub distance_range_px: f32,
}

/// One atlas entry, keyed by glyph id (DESIGN_1.md §7.2 — contextual
/// forms are just glyphs). `None` bounds ⇔ empty outline (e.g. space):
/// the glyph advances but paints nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlyphEntry {
    pub glyph_id: u16,
    /// Horizontal advance in raw font units (hmtx, authoritative —
    /// DESIGN_1.md §2 names ttf-parser as the metrics source).
    pub advance_units: u16,
    /// Quad bounds in ems: `[left, bottom, right, top]`, y-up,
    /// baseline origin.
    pub plane_em: Option<[f32; 4]>,
    /// Texel bounds in the atlas image: `[left, bottom, right, top]`,
    /// bottom-left origin.
    pub atlas_px: Option<[f32; 4]>,
}

/// The whole blob (`atlas.metrics`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtlasMetrics {
    pub format_version: u32,
    pub generator: GeneratorInfo,
    pub font: FontMetrics,
    pub atlas: AtlasInfo,
    /// Sorted by `glyph_id`, unique.
    pub glyphs: Vec<GlyphEntry>,
    /// Charset codepoints the font's cmap cannot represent, ascending
    /// (R6: a named diagnostic surface, never a silent drop).
    pub missing_codepoints: Vec<u32>,
}

impl AtlasMetrics {
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("postcard encoding of plain data cannot fail")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AtlasError> {
        let m: AtlasMetrics = postcard::from_bytes(bytes)
            .map_err(|e| AtlasError::Metrics(format!("blob decode failed: {e}")))?;
        if m.format_version != FORMAT_VERSION {
            return Err(AtlasError::Metrics(format!(
                "unsupported blob format version {} (supported: {FORMAT_VERSION})",
                m.format_version
            )));
        }
        Ok(m)
    }
}

/// Extracts the blob's font-wide metrics from a parsed face.
pub fn font_metrics(face: &ttf_parser::Face<'_>) -> FontMetrics {
    FontMetrics {
        units_per_em: face.units_per_em(),
        ascender: face.ascender(),
        descender: face.descender(),
        line_gap: face.line_gap(),
    }
}
```

`mod.rs` gains:

```rust
mod metrics;

pub use metrics::{
    font_metrics, AtlasInfo, AtlasMetrics, FontMetrics, GeneratorInfo, GlyphEntry,
    FORMAT_VERSION,
};

/// Everything that can go wrong in the pipeline — named and actionable
/// (P4 posture), std-only.
#[derive(Debug)]
pub enum AtlasError {
    /// Reading the font file failed.
    FontRead(std::path::PathBuf, std::io::Error),
    /// The font file is not a parseable TTF/OTF.
    FontParse(String),
    /// msdf-atlas-gen was not found (checked `MSDF_ATLAS_GEN`, then `PATH`).
    ToolMissing(String),
    /// The found tool is not the pinned version.
    ToolVersion { found: String, required: &'static str },
    /// The tool ran and failed.
    ToolFailed { status: Option<i32>, stderr: String },
    /// The tool's output did not match expectations.
    ToolOutput(String),
    /// Metrics blob encode/decode failure.
    Metrics(String),
    /// Filesystem failure outside the font file.
    Io(std::io::Error),
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontRead(p, e) => write!(f, "cannot read font {}: {e}", p.display()),
            Self::FontParse(m) => write!(f, "cannot parse font: {m}"),
            Self::ToolMissing(hint) => write!(f, "msdf-atlas-gen not found: {hint}"),
            Self::ToolVersion { found, required } => {
                write!(f, "msdf-atlas-gen {found} found, {required} required")
            }
            Self::ToolFailed { status, stderr } => {
                write!(f, "msdf-atlas-gen failed (status {status:?}): {stderr}")
            }
            Self::ToolOutput(m) => write!(f, "unexpected msdf-atlas-gen output: {m}"),
            Self::Metrics(m) => write!(f, "metrics blob: {m}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for AtlasError {}

impl From<std::io::Error> for AtlasError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashscene-typeset atlas::metrics`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/dashscene-typeset
git commit -m "feat(dashscene-typeset): add the atlas metrics blob (postcard, canonical)"
```

---

### Task 3: tool wrapper — discovery, version gate, invocation, JSON parse

**Files:**

- Modify: `Cargo.toml` (workspace deps) — add `serde_json = "1"`,
  `tempfile = "3"`
- Modify: `crates/dashscene-typeset/Cargo.toml` — add both
- Create: `crates/dashscene-typeset/src/atlas/tool.rs`
- Modify: `crates/dashscene-typeset/src/atlas/mod.rs` — `mod tool;` +
  re-export `find_tool_checked`, `REQUIRED_TOOL_VERSION`
- Test: in-module (pure parts) + tool-gated integration parts arrive in
  Task 4's integration file.

**Interfaces:**

- Consumes: `AtlasError` from Task 2.
- Produces:

```rust
pub const REQUIRED_TOOL_VERSION: &str = "1.4.0";
/// Finds the binary (env `MSDF_ATLAS_GEN`, else PATH) and enforces the
/// pinned version. Returns the path.
pub fn find_tool_checked() -> Result<std::path::PathBuf, AtlasError>;
pub(crate) fn parse_banner_version(banner: &str) -> Option<String>;
pub(crate) fn build_args(font: &Path, glyphset: &Path, out_png: &Path,
    out_json: &Path, px_per_em: u16, px_range: u16, seed: u64) -> Vec<String>;
pub(crate) struct Layout { pub atlas: LayoutAtlas, pub glyphs: Vec<LayoutGlyph> }
pub(crate) struct LayoutAtlas { pub width: u32, pub height: u32,
    pub size: f64, pub distance_range: f64, pub y_origin: String }
pub(crate) struct LayoutGlyph { pub index: u16, pub advance: f64,
    pub plane_bounds: Option<LayoutBounds>, pub atlas_bounds: Option<LayoutBounds> }
pub(crate) struct LayoutBounds { pub left: f64, pub bottom: f64,
    pub right: f64, pub top: f64 }
/// Runs the tool over the glyph ids; returns raw PNG bytes + layout.
pub(crate) fn run(tool: &Path, font: &Path, glyph_ids: &[u16],
    px_per_em: u16, px_range: u16, seed: u64)
    -> Result<(Vec<u8>, Layout), AtlasError>;
```

- [ ] **Step 1: Write the failing pure-function tests**

In `tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_the_version_banner() {
        let banner =
            "MSDF Atlas Generator by Viktor Chlumsky v1.4.0 (with MSDFgen v1.13.0)\n----";
        assert_eq!(parse_banner_version(banner).as_deref(), Some("1.4.0"));
    }

    #[test]
    fn rejects_unrecognized_banner() {
        assert_eq!(parse_banner_version("some other tool"), None);
    }

    #[test]
    fn builds_canonical_args() {
        let args = build_args(
            Path::new("f.ttf"),
            Path::new("g.txt"),
            Path::new("a.png"),
            Path::new("a.json"),
            32,
            4,
            1,
        );
        let expect: Vec<String> = [
            "-font", "f.ttf", "-glyphset", "g.txt", "-type", "msdf", "-size", "32",
            "-pxrange", "4", "-potr", "-yorigin", "bottom", "-nokerning", "-seed", "1",
            "-format", "png", "-imageout", "a.png", "-json", "a.json",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(args, expect);
    }

    #[test]
    fn parses_a_layout_json_sample() {
        let sample = r#"{
            "atlas": { "type": "msdf", "distanceRange": 4, "size": 32,
                       "width": 128, "height": 64, "yOrigin": "bottom" },
            "metrics": { "emSize": 1 },
            "glyphs": [
                { "index": 0, "advance": 0.6 },
                { "index": 36, "advance": 0.639,
                  "planeBounds": { "left": -0.01, "bottom": -0.02, "right": 0.65, "top": 0.72 },
                  "atlasBounds": { "left": 0.5, "bottom": 0.5, "right": 24.5, "top": 26.5 } }
            ]
        }"#;
        let layout = parse_layout(sample).expect("parses");
        assert_eq!(layout.atlas.width, 128);
        assert_eq!(layout.atlas.y_origin, "bottom");
        assert_eq!(layout.glyphs.len(), 2);
        assert_eq!(layout.glyphs[1].index, 36);
        assert!(layout.glyphs[0].plane_bounds.is_none());
        let ab = layout.glyphs[1].atlas_bounds.as_ref().unwrap();
        assert_eq!(ab.right, 24.5);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dashscene-typeset atlas::tool`
Expected: compile error.

- [ ] **Step 3: Implement `tool.rs`**

```rust
//! msdf-atlas-gen wrapper: discovery, version gate, canonical
//! invocation (R7: fixed argument order, pinned seed), layout parse.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::AtlasError;

/// The spike-validated version (docs/technotes/msdf-arabic-atlas-spike.md).
/// Anything else is a named error — R7 forbids silent generator drift.
pub const REQUIRED_TOOL_VERSION: &str = "1.4.0";

/// Finds msdf-atlas-gen — `MSDF_ATLAS_GEN` env override first, then
/// `PATH` — and enforces [`REQUIRED_TOOL_VERSION`].
pub fn find_tool_checked() -> Result<PathBuf, AtlasError> {
    let candidate = match std::env::var_os("MSDF_ATLAS_GEN") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("msdf-atlas-gen"),
    };
    let output = Command::new(&candidate).arg("-help").output().map_err(|e| {
        AtlasError::ToolMissing(format!(
            "{} ({e}); install with `brew install msdf-atlas-gen` or set MSDF_ATLAS_GEN",
            candidate.display()
        ))
    })?;
    let text = String::from_utf8_lossy(&output.stdout);
    let found = parse_banner_version(&text).ok_or_else(|| {
        AtlasError::ToolOutput("no version banner in -help output".to_string())
    })?;
    if found != REQUIRED_TOOL_VERSION {
        return Err(AtlasError::ToolVersion { found, required: REQUIRED_TOOL_VERSION });
    }
    Ok(candidate)
}

/// Extracts `1.4.0` from `MSDF Atlas Generator by Viktor Chlumsky
/// v1.4.0 (with MSDFgen v1.13.0)`.
pub(crate) fn parse_banner_version(banner: &str) -> Option<String> {
    let line = banner.lines().find(|l| l.contains("MSDF Atlas Generator"))?;
    let rest = line.split(" v").nth(1)?;
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!version.is_empty()).then_some(version)
}

/// Canonical argument vector — the exact list recorded in the blob's
/// provenance. Order is fixed; changing it changes the artifacts' hash
/// story, so treat any edit as a format decision.
pub(crate) fn build_args(
    font: &Path,
    glyphset: &Path,
    out_png: &Path,
    out_json: &Path,
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Vec<String> {
    [
        "-font",
        &font.display().to_string(),
        "-glyphset",
        &glyphset.display().to_string(),
        "-type",
        "msdf",
        "-size",
        &px_per_em.to_string(),
        "-pxrange",
        &px_range.to_string(),
        "-potr",
        "-yorigin",
        "bottom",
        "-nokerning",
        "-seed",
        &seed.to_string(),
        "-format",
        "png",
        "-imageout",
        &out_png.display().to_string(),
        "-json",
        &out_json.display().to_string(),
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct Layout {
    pub atlas: LayoutAtlas,
    pub glyphs: Vec<LayoutGlyph>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutAtlas {
    pub width: u32,
    pub height: u32,
    /// px per em.
    pub size: f64,
    pub distance_range: f64,
    pub y_origin: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutGlyph {
    pub index: u16,
    /// Em units — cross-checked against hmtx in tests, not stored.
    pub advance: f64,
    pub plane_bounds: Option<LayoutBounds>,
    pub atlas_bounds: Option<LayoutBounds>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LayoutBounds {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

pub(crate) fn parse_layout(json: &str) -> Result<Layout, AtlasError> {
    serde_json::from_str(json)
        .map_err(|e| AtlasError::ToolOutput(format!("layout JSON: {e}")))
}

/// Runs the tool in a scratch directory and returns the raw PNG bytes
/// plus the parsed layout.
pub(crate) fn run(
    tool: &Path,
    font: &Path,
    glyph_ids: &[u16],
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Result<(Vec<u8>, Layout), AtlasError> {
    let dir = tempfile::tempdir()?;
    let glyphset = dir.path().join("glyphs.txt");
    let out_png = dir.path().join("atlas.png");
    let out_json = dir.path().join("atlas.json");
    let list = glyph_ids
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    std::fs::write(&glyphset, list)?;

    let args = build_args(font, &glyphset, &out_png, &out_json, px_per_em, px_range, seed);
    let output = Command::new(tool).args(&args).output()?;
    if !output.status.success() {
        return Err(AtlasError::ToolFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let png = std::fs::read(&out_png)?;
    let layout = parse_layout(&std::fs::read_to_string(&out_json)?)?;
    Ok((png, layout))
}
```

`mod.rs` gains `mod tool;` and
`pub use tool::{find_tool_checked, REQUIRED_TOOL_VERSION};`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashscene-typeset atlas::tool`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/dashscene-typeset
git commit -m "feat(dashscene-typeset): wrap msdf-atlas-gen — discovery, version gate, canonical args"
```

---

### Task 4: `generate()` + `AtlasBundle` + gated integration tests

**Files:**

- Modify: `crates/dashscene-typeset/src/atlas/mod.rs` — `AtlasSpec`,
  `AtlasBundle`, `generate`, bundle IO, file-name constants
- Create: `crates/dashscene-typeset/tests/atlas_pipeline.rs`
  (integration; every tool-dependent test lives here)
- Test: same file

**Interfaces:**

- Consumes: Tasks 1–3 (`charset_closure`, metrics types, tool wrapper).
- Produces (Task 5 + #30 rely on these):

```rust
pub const ATLAS_IMAGE_FILE: &str = "atlas.png";
pub const ATLAS_METRICS_FILE: &str = "atlas.metrics";
pub struct AtlasSpec { pub font_path: PathBuf, pub charset: BTreeSet<char>,
    pub extra_glyph_ids: BTreeSet<u16>, pub px_per_em: u16, pub px_range: u16,
    pub seed: u64 }
impl AtlasSpec { pub fn new(font_path: impl Into<PathBuf>,
    charset: BTreeSet<char>) -> Self } // defaults: 32 / 4 / 1
pub struct AtlasBundle { pub image_png: Vec<u8>, pub metrics: AtlasMetrics }
pub fn generate(spec: &AtlasSpec) -> Result<AtlasBundle, AtlasError>;
impl AtlasBundle {
    pub fn write_to_dir(&self, dir: &Path) -> Result<(), AtlasError>;
    pub fn load_from_dir(dir: &Path) -> Result<Self, AtlasError>;
}
```

- [ ] **Step 1: Write the failing integration tests**

`crates/dashscene-typeset/tests/atlas_pipeline.rs`:

```rust
//! Tool-dependent pipeline tests. They self-skip when msdf-atlas-gen
//! is absent, but fail when `DASHSCENE_REQUIRE_ATLAS_TOOL` is set —
//! CI sets it, so a skip can never masquerade as green there.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dashscene_typeset::atlas::{generate, AtlasBundle, AtlasSpec};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// Returns false (and prints why) when the pinned tool is unavailable
/// and the environment tolerates that; panics when CI demands it.
fn tool_available() -> bool {
    match dashscene_typeset::atlas::find_tool_checked() {
        Ok(_) => true,
        Err(e) if std::env::var_os("DASHSCENE_REQUIRE_ATLAS_TOOL").is_some() => {
            panic!("DASHSCENE_REQUIRE_ATLAS_TOOL is set but: {e}")
        }
        Err(e) => {
            eprintln!("skipping tool-dependent test: {e}");
            false
        }
    }
}

fn ascii_charset() -> BTreeSet<char> {
    (0x20u8..=0x7e).map(char::from).collect()
}

fn ascii_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT), ascii_charset())
}

#[test]
fn generates_ascii_atlas_with_full_coverage() {
    if !tool_available() {
        return;
    }
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
    let m = &bundle.metrics;
    assert!(m.missing_codepoints.is_empty());
    // 95 ASCII chars resolve to 95 distinct gids, plus .notdef.
    assert_eq!(m.glyphs.len(), 96);
    assert!(m.glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id));
    // space advances but paints nothing; every other glyph has bounds.
    let space_gid = {
        let data = std::fs::read(FONT).unwrap();
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        face.glyph_index(' ').unwrap().0
    };
    for g in &m.glyphs {
        assert!(g.advance_units > 0 || g.glyph_id == 0);
        if g.glyph_id == space_gid {
            assert!(g.plane_em.is_none() && g.atlas_px.is_none());
        } else if g.glyph_id != 0 {
            assert!(g.plane_em.is_some() && g.atlas_px.is_some(), "gid {}", g.glyph_id);
        }
    }
    assert_eq!(m.atlas.px_per_em, 32);
    assert_eq!(m.atlas.distance_range_px, 4.0);
    assert!(m.atlas.width > 0 && m.atlas.height > 0);
    assert!(!bundle.image_png.is_empty());
    assert_eq!(&bundle.image_png[1..4], b"PNG");
}

#[test]
fn double_run_is_byte_identical() {
    if !tool_available() {
        return;
    }
    let a = generate(&ascii_spec()).expect("first run");
    let b = generate(&ascii_spec()).expect("second run");
    assert_eq!(a.image_png, b.image_png, "atlas.png must be byte-identical (R7)");
    assert_eq!(
        a.metrics.to_bytes(),
        b.metrics.to_bytes(),
        "atlas.metrics must be byte-identical (R7)"
    );
}

#[test]
fn bundle_write_load_round_trips() {
    if !tool_available() {
        return;
    }
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
    let dir = tempfile::tempdir().expect("tempdir");
    bundle.write_to_dir(dir.path()).expect("writes");
    let back = AtlasBundle::load_from_dir(dir.path()).expect("loads");
    assert_eq!(bundle.image_png, back.image_png);
    assert_eq!(bundle.metrics, back.metrics);
}

#[test]
fn missing_codepoints_are_reported_not_dropped() {
    if !tool_available() {
        return;
    }
    let mut spec = ascii_spec();
    spec.charset.insert('\u{0710}'); // Syriac alaph — not in Noto Sans LGC
    let bundle = generate(&spec).expect("pipeline runs");
    assert_eq!(bundle.metrics.missing_codepoints, vec![0x0710]);
}
```

(The hmtx-vs-tool advance cross-check lives inside `generate()` — see
Step 3 — because the tool layout is not persisted: the pipeline returns
`ToolOutput` if hmtx and the tool's advance disagree beyond 1e-3 em, so
every passing integration test exercises it. Parameter drift such as an
accidental `-fontscale` therefore cannot pass silently.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dashscene-typeset --test atlas_pipeline`
Expected: compile error — `AtlasSpec`, `generate` missing. Also add
`tempfile` and `ttf-parser` to `[dev-dependencies]` of
`crates/dashscene-typeset/Cargo.toml`.

- [ ] **Step 3: Implement in `atlas/mod.rs`**

```rust
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// File names inside an atlas bundle directory.
pub const ATLAS_IMAGE_FILE: &str = "atlas.png";
pub const ATLAS_METRICS_FILE: &str = "atlas.metrics";

/// Inputs of one atlas generation. Defaults per the Q-1 decision
/// record: 32 px/em, pxrange 4, seed 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasSpec {
    pub font_path: PathBuf,
    /// The declared charset (DESIGN_1.md §6.1: coverage comes from
    /// declared charsets, never from document text).
    pub charset: BTreeSet<char>,
    /// Glyphs only shaping can discover (ligatures, contextual forms)
    /// until GSUB closure lands with the v0.6 charset story.
    pub extra_glyph_ids: BTreeSet<u16>,
    pub px_per_em: u16,
    pub px_range: u16,
    pub seed: u64,
}

impl AtlasSpec {
    pub fn new(font_path: impl Into<PathBuf>, charset: BTreeSet<char>) -> Self {
        Self {
            font_path: font_path.into(),
            charset,
            extra_glyph_ids: BTreeSet::new(),
            px_per_em: 32,
            px_range: 4,
            seed: 1,
        }
    }
}

/// The two build artifacts, in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasBundle {
    /// The tool's PNG bytes, untouched (R7: no recompression).
    pub image_png: Vec<u8>,
    pub metrics: AtlasMetrics,
}

/// Runs the whole pipeline: closure → tool → metrics.
pub fn generate(spec: &AtlasSpec) -> Result<AtlasBundle, AtlasError> {
    let tool = tool::find_tool_checked()?;
    let data = std::fs::read(&spec.font_path)
        .map_err(|e| AtlasError::FontRead(spec.font_path.clone(), e))?;
    let face = ttf_parser::Face::parse(&data, 0)
        .map_err(|e| AtlasError::FontParse(e.to_string()))?;
    let closure = closure::charset_closure(&face, &spec.charset, &spec.extra_glyph_ids);

    let (image_png, layout) = tool::run(
        &tool,
        &spec.font_path,
        &closure.glyph_ids,
        spec.px_per_em,
        spec.px_range,
        spec.seed,
    )?;

    let glyphs = build_glyph_entries(&face, &closure.glyph_ids, &layout)?;
    // Provenance args use canonical names, never caller paths: an
    // absolute font path would make the blob machine-dependent and
    // break cross-machine byte-identity (R7) by construction.
    let font_name = spec
        .font_path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| spec.font_path.clone());
    let args = tool::build_args(
        &font_name,
        Path::new("glyphs.txt"),
        Path::new(ATLAS_IMAGE_FILE),
        Path::new("atlas.json"),
        spec.px_per_em,
        spec.px_range,
        spec.seed,
    );
    let metrics = AtlasMetrics {
        format_version: FORMAT_VERSION,
        generator: GeneratorInfo {
            tool_version: REQUIRED_TOOL_VERSION.to_string(),
            args,
        },
        font: metrics::font_metrics(&face),
        atlas: AtlasInfo {
            width: layout.atlas.width,
            height: layout.atlas.height,
            px_per_em: spec.px_per_em,
            distance_range_px: layout.atlas.distance_range as f32,
        },
        glyphs,
        missing_codepoints: closure.missing_codepoints,
    };
    Ok(AtlasBundle { image_png, metrics })
}

/// Joins the requested gid list with the tool layout; every requested
/// gid must appear (P4: absence is an error, not a silent gap), and
/// hmtx must agree with the tool's advance within 1e-3 em.
fn build_glyph_entries(
    face: &ttf_parser::Face<'_>,
    glyph_ids: &[u16],
    layout: &tool::Layout,
) -> Result<Vec<GlyphEntry>, AtlasError> {
    use std::collections::BTreeMap;
    let by_index: BTreeMap<u16, &tool::LayoutGlyph> =
        layout.glyphs.iter().map(|g| (g.index, g)).collect();
    let upem = f64::from(face.units_per_em());
    let mut out = Vec::with_capacity(glyph_ids.len());
    for &gid in glyph_ids {
        let lg = by_index.get(&gid).ok_or_else(|| {
            AtlasError::ToolOutput(format!("glyph {gid} missing from layout"))
        })?;
        let advance_units = face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0);
        let hmtx_em = f64::from(advance_units) / upem;
        if (hmtx_em - lg.advance).abs() > 1e-3 {
            return Err(AtlasError::ToolOutput(format!(
                "glyph {gid}: hmtx advance {hmtx_em} vs tool {}",
                lg.advance
            )));
        }
        let to4 = |b: &tool::LayoutBounds| {
            [b.left as f32, b.bottom as f32, b.right as f32, b.top as f32]
        };
        out.push(GlyphEntry {
            glyph_id: gid,
            advance_units,
            plane_em: lg.plane_bounds.as_ref().map(to4),
            atlas_px: lg.atlas_bounds.as_ref().map(to4),
        });
    }
    Ok(out)
}

impl AtlasBundle {
    /// Writes `atlas.png` + `atlas.metrics` into `dir`.
    pub fn write_to_dir(&self, dir: &Path) -> Result<(), AtlasError> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(ATLAS_IMAGE_FILE), &self.image_png)?;
        std::fs::write(dir.join(ATLAS_METRICS_FILE), self.metrics.to_bytes())?;
        Ok(())
    }

    /// Loads a bundle written by [`AtlasBundle::write_to_dir`].
    pub fn load_from_dir(dir: &Path) -> Result<Self, AtlasError> {
        let image_png = std::fs::read(dir.join(ATLAS_IMAGE_FILE))?;
        let metrics = AtlasMetrics::from_bytes(&std::fs::read(dir.join(ATLAS_METRICS_FILE))?)?;
        Ok(Self { image_png, metrics })
    }
}
```

(Provenance args note: recorded paths are the canonical bundle-relative
names, not the scratch-dir paths, so blobs stay machine-independent.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashscene-typeset` (tool present locally via brew)
Expected: closure + metrics + tool unit tests pass; all 5 integration
tests pass (not skipped) — confirm the "skipping" line is absent.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-typeset
git commit -m "feat(dashscene-typeset): assemble the atlas pipeline — generate() + bundle IO"
```

---

### Task 5: committed fixture + regeneration example + fixture test

**Files:**

- Create: `crates/dashscene-typeset/examples/generate_fixture.rs`
- Create: `crates/dashscene-typeset/tests/fixtures/ascii/atlas.png`
  (generated)
- Create: `crates/dashscene-typeset/tests/fixtures/ascii/atlas.metrics`
  (generated)
- Modify: `crates/dashscene-typeset/tests/atlas_pipeline.rs` — add the
  fixture-compare test

**Interfaces:**

- Consumes: Task 4's `generate`, `AtlasBundle::write_to_dir`,
  `AtlasSpec::new`, `ascii_charset()` shape.
- Produces: the committed fixture that CI regenerates — the
  cross-machine R7 evidence.

- [ ] **Step 1: Write the failing fixture test**

Append to `tests/atlas_pipeline.rs`:

```rust
#[test]
fn committed_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let fixture_dir = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/ascii"
    ));
    let committed = AtlasBundle::load_from_dir(&fixture_dir)
        .expect("committed fixture loads — regenerate with `cargo run -p dashscene-typeset --example generate_fixture`");
    let fresh = generate(&ascii_spec()).expect("pipeline runs");
    assert_eq!(
        committed.image_png, fresh.image_png,
        "committed atlas.png no longer reproducible (R7) — \
         if the toolchain legitimately changed, regenerate the fixture \
         and record why"
    );
    assert_eq!(committed.metrics.to_bytes(), fresh.metrics.to_bytes());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p dashscene-typeset --test atlas_pipeline committed_fixture`
Expected: FAIL — fixture directory does not exist yet.

- [ ] **Step 3: Write the example and generate the fixture**

`crates/dashscene-typeset/examples/generate_fixture.rs`:

```rust
//! Regenerates the committed ASCII test fixture:
//! `cargo run -p dashscene-typeset --example generate_fixture`
//!
//! Only rerun this when the pipeline parameters or the tool version
//! change deliberately — the fixture is the R7 cross-machine evidence.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dashscene_typeset::atlas::{generate, AtlasSpec};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let font = root.join("../../corpus/fonts/noto-sans/NotoSans-Regular.ttf");
    let out = root.join("tests/fixtures/ascii");
    let charset: BTreeSet<char> = (0x20u8..=0x7e).map(char::from).collect();
    let bundle = generate(&AtlasSpec::new(font, charset)).expect("pipeline");
    bundle.write_to_dir(&out).expect("write fixture");
    println!(
        "wrote {} ({} glyphs, {}x{})",
        out.display(),
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}
```

Run: `cargo run -p dashscene-typeset --example generate_fixture`
Expected: prints the fixture path + 96 glyphs; files appear.

- [ ] **Step 4: Run the fixture test to verify it passes**

Run: `cargo test -p dashscene-typeset --test atlas_pipeline committed_fixture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-typeset
git commit -m "feat(dashscene-typeset): commit the ASCII fixture atlas + regeneration example"
```

---

### Task 6: CI — `atlas-repro` job with a cached tool build

**Files:**

- Modify: `.github/workflows/ci.yml` — new job + aggregate `needs`

**Interfaces:**

- Consumes: the env gate from Task 4 (`DASHSCENE_REQUIRE_ATLAS_TOOL`),
  the fixture from Task 5.
- Produces: CI enforcement of R7, including the cross-machine
  (macOS-generated fixture vs Linux-generated) byte-identity check.

- [ ] **Step 1: Add the job**

Insert after the `wasm-build` job:

```yaml
atlas-repro:
  name: atlas-repro
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: cache msdf-atlas-gen
      id: cache-tool
      uses: actions/cache@v4
      with:
        path: ~/.local/bin/msdf-atlas-gen
        key: msdf-atlas-gen-v1.4-${{ runner.os }}
    - name: build msdf-atlas-gen
      if: steps.cache-tool.outputs.cache-hit != 'true'
      # Upstream publishes Windows binaries only; build the pinned tag
      # once and cache it (spike technote: version must stay 1.4.0).
      run: |
        sudo apt-get update
        sudo apt-get install -y cmake libfreetype-dev
        git clone --depth 1 --branch v1.4 --recurse-submodules \
          https://github.com/Chlumsky/msdf-atlas-gen.git "$RUNNER_TEMP/mag"
        cmake -S "$RUNNER_TEMP/mag" -B "$RUNNER_TEMP/mag/build" \
          -DCMAKE_BUILD_TYPE=Release -DMSDF_ATLAS_USE_VCPKG=OFF \
          -DMSDF_ATLAS_USE_SKIA=OFF -DMSDF_ATLAS_NO_ARTERY_FONT=ON \
          -DMSDF_ATLAS_INSTALL=OFF
        cmake --build "$RUNNER_TEMP/mag/build" -j"$(nproc)"
        mkdir -p ~/.local/bin
        install -m 0755 "$(find "$RUNNER_TEMP/mag/build" -name msdf-atlas-gen -type f)" \
          ~/.local/bin/msdf-atlas-gen
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - name: atlas pipeline tests (tool required)
      run: cargo test -p dashscene-typeset
      env:
        DASHSCENE_REQUIRE_ATLAS_TOOL: "1"
        MSDF_ATLAS_GEN: /home/runner/.local/bin/msdf-atlas-gen
```

And extend the aggregate:

```yaml
needs: [changes, fmt, dprint, clippy, test, wasm-build, convco, deno, atlas-repro]
```

- [ ] **Step 2: Verify locally what can be verified**

Run: `just build`
Expected: green (the plain `test` job path — tool-dependent tests still
pass locally because brew provides the tool; the CI YAML itself is
exercised on push).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci(ci): enforce atlas byte-reproducibility with a cached msdf-atlas-gen build"
```

**Execution note:** if the Linux-built generator does not reproduce the
macOS-generated fixture byte-for-byte, that is the spike's open
cross-machine question answering "no" — do not force it green. Record
the finding (issue + decision record: per-platform fixtures or a pinned
generation platform) and gate the fixture test accordingly.

---

### Task 7: story wrap-up (process, not code)

- [ ] `just build` green in the worktree.
- [ ] sdd-gardening: move both wip docs to `docs/archive/`, write
      `docs/design/atlas-pipeline.md` (as-built), decision records
      (external pinned binary; postcard blob; cmap-only closure), update
      `specs/SCOPE_DECISIONS.md` §11 pointer if needed.
- [ ] `/code-review` on the diff; findings → PR checklist; criticals
      fixed; `debt` issues for minors.
- [ ] PR → CI green (including `atlas-repro`) → merge → close #27 →
      tick epic #24 checkbox.
