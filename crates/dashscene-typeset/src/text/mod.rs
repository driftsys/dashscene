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
    /// Index into the typesetter's font list of the font this glyph was
    /// shaped with — the fallback cascade's result (story #219). A
    /// mixed-script line carries glyphs from more than one font, so the
    /// boundary-B stager groups consecutive same-font glyphs into one
    /// glyph run per atlas. Zero for a single-font typesetter.
    pub font: u16,
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

/// The runtime pipeline facade: an ordered font list (primary first,
/// story #219 — the runtime resolves one document font reference to a
/// fallback list), one shaped-run cache in front of shaping.
///
/// The cache stores font-unit shaped runs keyed by paragraph text —
/// shaping output is size-independent, so one entry serves every
/// render size. The font list is fixed per typesetter (runtime
/// configuration, not a per-call axis), so the cascade — which font
/// each codepoint resolves to — is a pure function of the text; the
/// key stays the text alone, exactly as in the single-font case (the
/// design record explains how this refines DESIGN §7.2's "string+style"
/// key). It is unbounded: cockpit UI text is a bounded set, and an
/// eviction policy before a real producer shows growth would be
/// speculative.
#[derive(Debug)]
pub struct Typesetter {
    /// Primary font first, then fallbacks in cascade order. Always at
    /// least one element.
    fonts: Vec<Font>,
    cache: HashMap<Box<str>, Arc<ShapedText>>,
    hits: u64,
    misses: u64,
}

impl Typesetter {
    /// A single-font typesetter — the pre-#219 constructor. Equivalent
    /// to [`with_fonts`](Self::with_fonts) with a one-element list; every
    /// glyph is tagged font 0.
    pub fn new(font: Font) -> Typesetter {
        Self::with_fonts(vec![font])
    }

    /// A typesetter over an ordered fallback list, primary font first.
    /// Each level run splits by coverage: a codepoint goes to the first
    /// font in `fonts` that covers it, and a codepoint no font covers
    /// stays in the primary as `.notdef` (P4).
    ///
    /// # Panics
    ///
    /// Panics if `fonts` is empty — a typesetter has no primary font to
    /// fall back to.
    pub fn with_fonts(fonts: Vec<Font>) -> Typesetter {
        assert!(!fonts.is_empty(), "a typesetter needs at least one font");
        Typesetter {
            fonts,
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// The primary font — the first in the cascade. A single-font
    /// typesetter has only this one; a multi-font typesetter falls back
    /// past it by coverage (story #219). Line metrics are taken per line
    /// from the fonts that actually shaped that line ([`layout`](Self::layout)
    /// through [`line_box`](Self::line_box)), not from the primary alone.
    pub fn font(&self) -> &Font {
        &self.fonts[0]
    }

    /// The ordered font list, primary first — the cascade a
    /// [`PositionedGlyph::font`] indexes. A boundary-B stager maps each
    /// font to its atlas in this order.
    pub fn fonts(&self) -> &[Font] {
        &self.fonts
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
        // Per-font pixel scale: each glyph's advances and offsets are in
        // its own font's units, so a different-upem fallback is scaled by
        // its own upem, not the primary's (story #219).
        let scales: Vec<f32> = self
            .fonts
            .iter()
            .map(|f| size / f32::from(f.units_per_em()))
            .collect();
        // Lines stack down the page: `pen_y` accumulates the line boxes
        // above the current line, and each box is measured from the fonts
        // that actually shaped its own glyphs ([`line_box`]), not the
        // primary — a line shaped by a taller fallback gets a taller box
        // (story #219, applied per line).
        let mut pen_y = 0.0f32;
        let mut lines = Vec::new();
        // Parallel to `lines`: the owning paragraph's base direction.
        let mut rtl_lines = Vec::new();
        if !text.is_empty() {
            for paragraph in text.split('\n') {
                let bidi = BidiInfo::new(paragraph, None);
                let shaped = self.shaped(paragraph, &bidi);
                // An empty chunk has no bidi paragraph: one empty line.
                if bidi.paragraphs.is_empty() {
                    // A blank line has no glyphs; measure its box by the
                    // primary font.
                    let (ascent, advance) = self.line_box(std::iter::empty(), &scales);
                    lines.push(Line {
                        glyphs: Vec::new(),
                        width: 0.0,
                        baseline_y: pen_y + ascent,
                    });
                    pen_y += advance;
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
                    for range in layout::break_lines(paragraph, &shaped, gs..ge, &scales, max_width)
                    {
                        // The line's box comes from the fonts that shaped
                        // its glyphs, not the primary (story #219).
                        let (ascent, advance) = self.line_box(
                            shaped.glyphs[range.clone()].iter().map(|g| g.font as usize),
                            &scales,
                        );
                        let baseline = pen_y + ascent;
                        pen_y += advance;
                        rtl_lines.push(para.level.is_rtl());
                        lines.push(layout::position_line(
                            &bidi, para, &shaped, range, &scales, baseline,
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
        let height = pen_y;
        TextLayout {
            lines,
            width,
            height,
            size,
        }
    }

    /// The vertical metrics of one line, taken from the fonts that
    /// actually shaped its glyphs — not the cascade's primary alone.
    /// Story #219 established the per-font principle for glyph scale
    /// (each glyph scaled by its own font's units-per-em); a line box
    /// has the same shape of dependency. The box spans the tallest
    /// ascender and the deepest descender across the line's fonts, plus
    /// the widest line gap, so a line shaped entirely by a taller
    /// fallback is measured by that fallback rather than by a shorter
    /// primary.
    ///
    /// `fonts_on_line` yields the font index of each glyph on the line;
    /// repeats are harmless, since the maximum and minimum ignore them.
    /// An empty iterator is a blank line, which carries no glyphs and is
    /// measured by the primary font. Returns the line's ascent (the
    /// distance from the top of the box down to the baseline) and its
    /// baseline-to-baseline advance, both in pixels.
    fn line_box(&self, fonts_on_line: impl Iterator<Item = usize>, scales: &[f32]) -> (f32, f32) {
        let metrics = |f: usize| {
            let scale = scales[f];
            (
                f32::from(self.fonts[f].ascender()) * scale,
                f32::from(self.fonts[f].descender()) * scale,
                f32::from(self.fonts[f].line_gap()) * scale,
            )
        };
        // Seed from the primary so a blank line (no glyphs) is measured
        // by it; the first shaping font on the line replaces the seed.
        let (mut ascent, mut descent, mut gap) = metrics(0);
        let mut seen = false;
        for f in fonts_on_line {
            let (a, d, g) = metrics(f);
            if seen {
                ascent = ascent.max(a);
                descent = descent.min(d);
                gap = gap.max(g);
            } else {
                (ascent, descent, gap) = (a, d, g);
                seen = true;
            }
        }
        // The descender is negative, so subtracting it adds the depth
        // below the baseline: advance = ascent + |descent| + line gap.
        (ascent, ascent - descent + gap)
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
        let shaped = Arc::new(shape::shape_paragraph(&self.fonts, bidi));
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
