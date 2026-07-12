# Typeset Latin (#28) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dashscene-typeset::text` — text + size in, positioned glyph
runs out (shape via rustybuzz with ligatures off, greedy line break,
baseline positioning), with a font-unit shaped-run cache.

**Architecture:** see `docs/wip/2026-07-12-typeset-latin-design.md`.
Pipeline: `Typesetter::layout(text, size, max_width)` → split at
`'\n'` → per-paragraph cache lookup (`ShapedText`, font units) →
greedy word wrap → scale/position.

**Tech Stack:** rustybuzz 0.20 (workspace), existing corpus Noto Sans
fixture font.

## Global Constraints

- Shaping features: `liga` and `clig` disabled (atlas closure is
  cmap-only, #27 seam); `kern` stays default-on.
- Coordinates: document space, y-down, layout origin top-left;
  HarfBuzz y-up offsets negated at positioning.
- `PositionedGlyph` carries per-glyph x/y with offsets applied (GPOS
  marks, spike #25 finding).
- Cache: font-unit `ShapedText` keyed by paragraph text; hit/miss
  counters observable.
- House style: std-only error enum, doc comments citing DESIGN §7.2,
  commits scoped `dashscene-typeset` with the Claude trailer.

---

### Task 1: Font + shaping (font units, liga off)

**Files:**

- Modify: `crates/dashscene-typeset/Cargo.toml` — add
  `rustybuzz.workspace = true`
- Create: `crates/dashscene-typeset/src/text/mod.rs` (module shell +
  `TypesetError`)
- Create: `crates/dashscene-typeset/src/text/font.rs`
- Create: `crates/dashscene-typeset/src/text/shape.rs`
- Modify: `crates/dashscene-typeset/src/lib.rs` — `pub mod text;`
- Test: in-module `#[cfg(test)]` in font.rs and shape.rs (the crate's
  `TEST_FONT` const is `#[cfg(test)]`-visible)

**Interfaces (produced):**

```rust
pub struct Font { /* Arc<Vec<u8>>, index, cached hhea metrics */ }
impl Font {
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Font, TypesetError>;
    pub fn units_per_em(&self) -> u16;
    pub fn ascender(&self) -> i16;   // hhea, font units
    pub fn descender(&self) -> i16;  // negative
    pub fn line_gap(&self) -> i16;
    pub(crate) fn face(&self) -> rustybuzz::Face<'_>;
}
pub struct ShapedGlyph { pub glyph_id: u16, pub cluster: u32,
    pub x_advance: i32, pub x_offset: i32, pub y_offset: i32 }
pub struct ShapedText { pub glyphs: Vec<ShapedGlyph> }
pub(crate) fn shape(font: &Font, text: &str) -> ShapedText;
pub enum TypesetError { FontParse(String) }
```

- [ ] **Step 1: failing tests** — in `font.rs`:
      `metrics_match_ttf_parser` (load `TEST_FONT`, compare upem/ascender/
      descender against a direct `ttf_parser::Face`), `rejects_garbage`
      (`Font::from_bytes(vec![0xde, 0xad], 0)` errs). In `shape.rs`:
      `shapes_av_with_kerning` (gids equal the cmap gids for 'A'/'V',
      total x_advance < plain hmtx sum), `liga_disabled_keeps_fi_two_glyphs`
      ("fi" shapes to 2 glyphs whose gids equal cmap('f')/cmap('i')),
      `clusters_are_byte_indices` ("ab" clusters == [0, 1]),
      `empty_text_shapes_to_nothing`.
- [ ] **Step 2: run to see the compile failure**
      (`cargo test -p dashscene-typeset text::`)
- [ ] **Step 3: implement** — `Font::from_bytes` parses with
      `ttf_parser::Face::parse` (extract metrics) AND validates
      `rustybuzz::Face::from_slice` succeeds, storing `Arc<Vec<u8>>`;
      `face()` re-slices with `.expect("validated at construction")`.
      `shape()` builds `rustybuzz::UnicodeBuffer`, `push_str`,
      `guess_segment_properties`, shapes with
      `[Feature::new(ty::Tag::from_bytes(b"liga"), 0, ..), (b"clig", 0)]`,
      copies infos+positions into `ShapedGlyph` (u16 gid via `as u16` —
      glyph ids are u16 in TrueType; document).
- [ ] **Step 4: green** — `cargo test -p dashscene-typeset text::`
- [ ] **Step 5: commit**
      `feat(dashscene-typeset): add Font and liga-off rustybuzz shaping`

---

### Task 2: layout (greedy break + positioning) + cache + facade

**Files:**

- Create: `crates/dashscene-typeset/src/text/layout.rs`
- Modify: `crates/dashscene-typeset/src/text/mod.rs` — `Typesetter`
  with cache + public types
- Test: `crates/dashscene-typeset/tests/typeset_latin.rs`
  (integration; needs only the font, no external tool)

**Interfaces (produced — the #29/#30 surface):**

```rust
pub struct Typesetter { /* font, cache: HashMap<Box<str>, Arc<ShapedText>>, hits, misses */ }
impl Typesetter {
    pub fn new(font: Font) -> Typesetter;
    pub fn font(&self) -> &Font;
    pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout;
    pub fn cache_stats(&self) -> CacheStats;
}
pub struct CacheStats { pub hits: u64, pub misses: u64 }
pub struct TextLayout { pub lines: Vec<Line>, pub width: f32, pub height: f32, pub size: f32 }
pub struct Line { pub glyphs: Vec<PositionedGlyph>, pub width: f32, pub baseline_y: f32 }
pub struct PositionedGlyph { pub glyph_id: u16, pub x: f32, pub y: f32 }
```

- [ ] **Step 1: failing integration tests** (names pin behavior):
      `newline_forces_a_break`; `greedy_wrap_breaks_at_the_space`
      ("Hello world" at a width that fits only "Hello": 2 lines, line 2's
      first gid == cmap('w'), the broken-at space glyph appears on
      neither line); `spaces_inside_a_line_still_advance` ("a b" single
      line: 3 glyphs, width > width("ab"));
      `single_line_width_is_the_scaled_advance_sum` ("ll" at size 20:
      width == 2 × hmtx('l') × 20 / upem, within 1e-4);
      `baselines_advance_by_the_line_metric` (two lines: first baseline ==
      ascent×scale, second == first + (ascent − descent + line_gap)×scale);
      `a_word_wider_than_max_width_overflows` (one long word, tiny width:
      1 line); `empty_text_lays_out_empty` (no lines, zero size);
      `cache_hits_across_sizes_and_counts` (same text twice + a third
      size: misses == 1, hits == 2; new text: misses == 2);
      `offsets_reach_positioned_glyphs` (shape a string, assert
      `PositionedGlyph.y == baseline_y` for offset-less Latin, documenting
      the negation convention).
- [ ] **Step 2: run to see the failure**
- [ ] **Step 3: implement**

`layout.rs` core (complete):

```rust
//! Greedy line breaking + baseline positioning (DESIGN §7.2, Latin
//! subset). Break opportunities: after runs of ASCII space, and at
//! '\n' (handled by the caller splitting paragraphs). A word wider
//! than the maximum width overflows its line; mid-word breaking is
//! not a v0.5 problem.

use super::font::Font;
use super::shape::ShapedText;
use super::{Line, PositionedGlyph};

/// One paragraph's glyphs split into lines of glyph ranges.
/// `shaped.glyphs[range]` are the glyphs of each line, with
/// trailing break spaces excluded.
pub(crate) fn break_lines(
    text: &str,
    shaped: &ShapedText,
    scale: f32,
    max_width: Option<f32>,
) -> Vec<std::ops::Range<usize>> {
    let Some(max_width) = max_width else {
        return vec![0..shaped.glyphs.len()];
    };
    // Tokenize into (is_space, glyph range) runs via cluster → byte.
    let bytes = text.as_bytes();
    let is_space = |gi: usize| bytes[shaped.glyphs[gi].cluster as usize] == b' ';
    let mut tokens: Vec<(bool, std::ops::Range<usize>, f32)> = Vec::new();
    let mut i = 0;
    while i < shaped.glyphs.len() {
        let space = is_space(i);
        let start = i;
        let mut advance = 0f32;
        while i < shaped.glyphs.len() && is_space(i) == space {
            advance += shaped.glyphs[i].x_advance as f32 * scale;
            i += 1;
        }
        tokens.push((space, start..i, advance));
    }
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    let mut line_end = 0usize; // exclusive, trailing spaces trimmed
    let mut line_width = 0f32;
    let mut pending_space: Option<(std::ops::Range<usize>, f32)> = None;
    for (space, range, advance) in tokens {
        if space {
            pending_space = Some((range, advance));
            continue;
        }
        let space_w = pending_space.as_ref().map_or(0.0, |(_, w)| *w);
        let fits = line_width + space_w + advance <= max_width;
        if line_width == 0.0 || fits {
            if let Some((sr, sw)) = pending_space.take() {
                if line_width > 0.0 {
                    line_end = sr.end; // keep the mid-line space glyphs
                    line_width += sw;
                } // else: leading space after a break — dropped
            }
            if line_width == 0.0 {
                line_start = range.start;
            }
            line_end = range.end;
            line_width += advance;
        } else {
            lines.push(line_start..line_end);
            pending_space = None; // the broken-at space vanishes
            line_start = range.start;
            line_end = range.end;
            line_width = advance;
        }
    }
    lines.push(line_start..line_end);
    lines
}

/// Positions one line's glyphs on its baseline. HarfBuzz offsets are
/// y-up; document space is y-down, so the y offset is subtracted.
pub(crate) fn position_line(
    shaped: &ShapedText,
    range: std::ops::Range<usize>,
    scale: f32,
    baseline_y: f32,
) -> Line {
    let mut glyphs = Vec::with_capacity(range.len());
    let mut pen_x = 0f32;
    for g in &shaped.glyphs[range] {
        glyphs.push(PositionedGlyph {
            glyph_id: g.glyph_id,
            x: pen_x + g.x_offset as f32 * scale,
            y: baseline_y - g.y_offset as f32 * scale,
        });
        pen_x += g.x_advance as f32 * scale;
    }
    Line { glyphs, width: pen_x, baseline_y }
}

pub(crate) fn line_advance(font: &Font) -> i32 {
    i32::from(font.ascender()) - i32::from(font.descender()) + i32::from(font.line_gap())
}
```

`Typesetter::layout` (complete):

```rust
    pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout {
        let scale = size / f32::from(self.font.units_per_em());
        let ascent = f32::from(self.font.ascender()) * scale;
        let advance = layout::line_advance(&self.font) as f32 * scale;
        let mut lines = Vec::new();
        if !text.is_empty() {
            for paragraph in text.split('\n') {
                let shaped = self.shaped(paragraph);
                let baseline_base = ascent + lines.len() as f32 * advance;
                // (recompute per line below; paragraphs stack lines)
                for (i, range) in layout::break_lines(paragraph, &shaped, scale, max_width)
                    .into_iter()
                    .enumerate()
                {
                    let baseline = ascent + (lines.len()) as f32 * advance;
                    let _ = (i, baseline_base);
                    lines.push(layout::position_line(&shaped, range, scale, baseline));
                }
            }
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let height = lines.len() as f32 * advance;
        TextLayout { lines, width, height, size }
    }

    fn shaped(&mut self, paragraph: &str) -> std::sync::Arc<ShapedText> {
        if let Some(hit) = self.cache.get(paragraph) {
            self.hits += 1;
            return hit.clone();
        }
        self.misses += 1;
        let shaped = std::sync::Arc::new(shape::shape(&self.font, paragraph));
        self.cache.insert(paragraph.into(), shaped.clone());
        shaped
    }
```

(Clean the leftover `baseline_base` scaffolding when writing the real
code — baseline is computed per pushed line from `lines.len()`.)

- [ ] **Step 4: green** — `cargo test -p dashscene-typeset` (all atlas
      tests still pass; text tests green), clippy clean.
- [ ] **Step 5: commit**
      `feat(dashscene-typeset): add the Latin layout pipeline — greedy wrap, baselines, run cache`

---

### Task 3: story wrap-up (process)

- [ ] `just build` green.
- [ ] sdd-gardening (subagent): new `docs/design/typeset-latin.md` (or
      extend a text section in the crate's design records as the gardener
      judges), decision records for liga-off-until-#34 and the
      font-unit/text-only cache key; archive wip.
- [ ] `/code-review` on the diff; findings → PR checklist; criticals
      fixed; `debt` issues for minors.
- [ ] Rebase on main, PR, CI green, merge, close #28, tick epic #24.
