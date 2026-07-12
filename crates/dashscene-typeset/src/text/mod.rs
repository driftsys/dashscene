//! Runtime text pipeline, Latin subset (DESIGN_1.md §7.2): shape
//! (rustybuzz, ligatures off) → greedy line break → positioned glyph
//! runs, with a font-unit shaped-run cache in front of shaping.
//!
//! Bidi splitting and Arabic shaping are the v0.6 stories; this module
//! is single-direction LTR by construction.

mod font;
mod layout;
mod shape;

pub use font::Font;
pub use shape::{ShapedGlyph, ShapedText};

use std::collections::HashMap;
use std::sync::Arc;

/// One glyph placed in document space (y-down, layout origin at the
/// top-left): the pen position on the line's baseline with the
/// shaping offsets applied. The painter combines this with the atlas
/// blob's y-up `plane_em` quad — that conversion is the painter's
/// (see `docs/design/atlas-pipeline.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
}

/// One positioned line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub glyphs: Vec<PositionedGlyph>,
    /// Pen advance over the line's glyphs, scaled.
    pub width: f32,
    /// Baseline position from the layout's top, y-down.
    pub baseline_y: f32,
}

/// A laid-out text block — DESIGN §7.2's positioned glyph runs. The
/// render size lives here once (one style per text node in v0.5);
/// the atlas page field arrives when multi-page atlases exist.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<Line>,
    /// Widest line.
    pub width: f32,
    /// Line count × the line advance (ascent − descent + line gap).
    pub height: f32,
    /// Render size (px per em in document units).
    pub size: f32,
}

/// Cache observability for tests and the measure callback's caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

/// The runtime pipeline facade: one font (fallback lists are the v0.6
/// charset story's), one shaped-run cache in front of shaping.
///
/// The cache stores font-unit shaped runs keyed by paragraph text —
/// shaping output is size-independent, so one entry serves every
/// render size (the design record explains how this refines DESIGN
/// §7.2's "string+style" key while the font is fixed per typesetter).
/// It is unbounded in v0.5: cockpit UI text is a bounded set, and an
/// eviction policy before a real producer shows growth would be
/// speculative.
#[derive(Debug)]
pub struct Typesetter {
    font: Font,
    cache: HashMap<Box<str>, Arc<ShapedText>>,
    hits: u64,
    misses: u64,
}

impl Typesetter {
    pub fn new(font: Font) -> Typesetter {
        Typesetter {
            font,
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    pub fn font(&self) -> &Font {
        &self.font
    }

    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
        }
    }

    /// Lays out `text` at `size` (px per em), wrapping greedily at
    /// spaces when `max_width` is given and breaking at `'\n'`
    /// always. Empty text produces an empty, zero-size layout.
    pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout {
        let scale = size / f32::from(self.font.units_per_em());
        let ascent = f32::from(self.font.ascender()) * scale;
        let advance = layout::line_advance(&self.font) as f32 * scale;
        let mut lines = Vec::new();
        if !text.is_empty() {
            for paragraph in text.split('\n') {
                let shaped = self.shaped(paragraph);
                for range in layout::break_lines(paragraph, &shaped, scale, max_width) {
                    let baseline = ascent + lines.len() as f32 * advance;
                    lines.push(layout::position_line(&shaped, range, scale, baseline));
                }
            }
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let height = lines.len() as f32 * advance;
        TextLayout {
            lines,
            width,
            height,
            size,
        }
    }

    fn shaped(&mut self, paragraph: &str) -> Arc<ShapedText> {
        if let Some(hit) = self.cache.get(paragraph) {
            self.hits += 1;
            return hit.clone();
        }
        self.misses += 1;
        let shaped = Arc::new(shape::shape(&self.font, paragraph));
        self.cache.insert(paragraph.into(), shaped.clone());
        shaped
    }
}

/// Everything that can go wrong in the runtime pipeline. Shaping and
/// layout are infallible over a valid [`Font`]: unknown codepoints
/// shape to `.notdef` (the painter's named diagnostic surface at
/// paint time), and empty text lays out empty.
#[derive(Debug)]
pub enum TypesetError {
    /// The bytes are not a parseable font face.
    FontParse(String),
}

impl std::fmt::Display for TypesetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontParse(m) => write!(f, "cannot parse font: {m}"),
        }
    }
}

impl std::error::Error for TypesetError {}
