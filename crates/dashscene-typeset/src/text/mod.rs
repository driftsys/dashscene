//! Runtime text pipeline (docs/design/architecture.md): bidi split
//! (UAX #9 level runs) → shape per run (rustybuzz, features and digit
//! shapes resolved from the run's context — `shape::RunContext`) →
//! greedy line break → positioned glyph runs, reordered per line for
//! display, with a font-unit shaped-run cache in front of shaping.

mod font;
mod layout;
mod shape;

pub use font::Font;

// The digit-shape mapping and the Arabic-strong predicate are the one
// definition the atlas closure shares (atlas/closure.rs), so coverage
// derivation cannot drift from production shaping.
pub(crate) use shape::{arabic_indic_digit, is_arabic_strong};

use shape::ShapedText;

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use unicode_bidi::{BidiClass, BidiInfo, ParagraphInfo};

// ShapedText/ShapedGlyph stay crate-private: they are the cache-value
// representation, and publishing them before a consumer exists (#29/
// #30) would freeze it into the public API.

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

/// A laid-out text block — docs/design/architecture.md's positioned glyph runs. The
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
    /// always. Each paragraph resolves its base direction per UAX #9
    /// (first strong character): an RTL paragraph's lines sit
    /// flush-right within `max_width` (or within the widest line when
    /// `None`); LTR lines stay flush-left at x = 0. `width` stays the
    /// widest line's pen advance — the measure contract — so an RTL
    /// line's glyph positions can reach up to `max_width`, past
    /// `width`. Empty text produces an empty, zero-size layout.
    pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout {
        let scale = size / f32::from(self.font.units_per_em());
        let ascent = f32::from(self.font.ascender()) * scale;
        let advance = self.font.line_advance() as f32 * scale;
        let mut lines = Vec::new();
        // Parallel to `lines`: the owning paragraph's base direction.
        let mut rtl_lines = Vec::new();
        if !text.is_empty() {
            for paragraph in text.split('\n') {
                let bidi = BidiInfo::new(paragraph, None);
                let shaped = self.shaped(paragraph, &bidi);
                // An empty chunk has no bidi paragraph: one empty line.
                if bidi.paragraphs.is_empty() {
                    lines.push(Line {
                        glyphs: Vec::new(),
                        width: 0.0,
                        baseline_y: ascent + lines.len() as f32 * advance,
                    });
                    rtl_lines.push(false);
                    continue;
                }
                // Lines are produced per bidi paragraph: '\n' is split
                // above, and the other UAX #9 class-B separators (CR,
                // NEL, U+2029, …) end a paragraph here, so no line
                // ever spans two paragraphs — each line reorders and
                // aligns under its own paragraph's level.
                for para in &bidi.paragraphs {
                    let content = paragraph_content(&bidi, para);
                    let gs = shaped
                        .glyphs
                        .partition_point(|g| (g.cluster as usize) < content.start);
                    let ge = shaped
                        .glyphs
                        .partition_point(|g| (g.cluster as usize) < content.end);
                    for range in layout::break_lines(paragraph, &shaped, gs..ge, scale, max_width) {
                        let baseline = ascent + lines.len() as f32 * advance;
                        rtl_lines.push(para.level.is_rtl());
                        lines.push(layout::position_line(
                            &bidi, para, &shaped, range, scale, baseline,
                        ));
                    }
                }
            }
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        // Flush-right placement for RTL-base paragraphs. A line wider
        // than the container (an overflowing word) overflows leftward
        // past x = 0, mirroring LTR overflow.
        let container = max_width.unwrap_or(width);
        for (line, rtl) in lines.iter_mut().zip(&rtl_lines) {
            if *rtl {
                let shift = container - line.width;
                for g in &mut line.glyphs {
                    g.x += shift;
                }
            }
        }
        let height = lines.len() as f32 * advance;
        TextLayout {
            lines,
            width,
            height,
            size,
        }
    }

    /// The cache key stays the paragraph text alone: resolved bidi
    /// levels are a pure function of that text, so one entry serves
    /// every layout of the paragraph.
    fn shaped(&mut self, paragraph: &str, bidi: &BidiInfo<'_>) -> Arc<ShapedText> {
        if let Some(hit) = self.cache.get(paragraph) {
            self.hits += 1;
            return hit.clone();
        }
        self.misses += 1;
        let shaped = Arc::new(shape::shape_paragraph(&self.font, bidi));
        self.cache.insert(paragraph.into(), shaped.clone());
        shaped
    }
}

/// A paragraph's content range: its bidi range minus the trailing
/// block-separator character (class B — CR, NEL, U+2029, …), which
/// ends the paragraph and renders on no line, exactly like the '\n'
/// the caller splits on. Every byte of a multi-byte separator carries
/// class B, so trimming by byte is trimming by char.
fn paragraph_content(bidi: &BidiInfo<'_>, para: &ParagraphInfo) -> Range<usize> {
    let mut end = para.range.end;
    while end > para.range.start && bidi.original_classes[end - 1] == BidiClass::B {
        end -= 1;
    }
    para.range.start..end
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
