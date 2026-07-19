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

/// Horizontal alignment of a line within the container width (story #310).
/// `Left` is the default and reproduces the pre-#310 flush placement: an LTR
/// line stays at x = 0, an RTL line flushes right by direction. `Center` and
/// `Right` shift every line by the container's free space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// The additive shaping knobs [`Typesetter::layout_with`] honors (story #310,
/// #341): a fixed line height, letter spacing, horizontal alignment, and
/// standard ligatures forced off. The [`Default`] reproduces the previous
/// behavior exactly — an auto line height (font metrics), no tracking,
/// flush-by-direction placement, and the per-run context's own ligature
/// posture (`shape::RunContext`) — so [`Typesetter::layout`] delegating with
/// the default is byte-for-byte the pre-#310 output (the E7 guard).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShape {
    /// A fixed line advance in pixels, or `None` for the intrinsic advance
    /// (ascent − descent + line gap of the fonts that shaped each line).
    pub line_height_px: Option<f32>,
    /// Letter spacing (tracking) added after each glyph, in pixels.
    pub letter_spacing: f32,
    /// Horizontal alignment within the container width.
    pub align: TextAlign,
    /// Forces standard ligatures (`liga`/`clig`) off for every run in this
    /// layout, regardless of the run's own context (story #341: Figma's
    /// OpenType `LIGA: 0`). A non-Arabic run already shapes with these
    /// features off by default (`docs/decisions/
    /// liga-clig-off-until-gsub-closure.md`), so `false` here is a no-op for
    /// it; an Arabic-context run's other default features (digit shapes,
    /// joining, `rlig`/`ccmp`, …) are unaffected either way.
    pub ligatures_off: bool,
}

impl Default for TextShape {
    fn default() -> Self {
        TextShape {
            line_height_px: None,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            ligatures_off: false,
        }
    }
}

/// The vertical shift a fixed line height applies to a line's baseline.
/// Figma (like CSS inline layout) centers the line's intrinsic box within the
/// fixed line box, so half the leading — `line_height - intrinsic` — sits
/// above the ascent: negative leading (a line height below the intrinsic
/// advance) lifts the baseline, positive lowers it. The default auto line
/// height applies none. Pinned against Figma's own `GET /images` render by
/// the #332 import oracle.
fn half_leading(shape: &TextShape, intrinsic: f32) -> f32 {
    shape
        .line_height_px
        .map_or(0.0, |line_height| (line_height - intrinsic) / 2.0)
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
///
/// `ligatures_off` (story #341) is a per-call knob, not a property of the
/// text, so the same paragraph text can shape two different ways depending
/// on it — a single map keyed by text alone would hand back a stale entry
/// shaped under the other setting. `cache_ligatures_off` holds that second
/// posture's entries; `cache` (the `false` posture, which every existing
/// call site uses) is untouched by the split.
#[derive(Debug)]
pub struct Typesetter {
    /// Primary font first, then fallbacks in cascade order. Always at
    /// least one element.
    fonts: Vec<Font>,
    cache: HashMap<Box<str>, Arc<ShapedText>>,
    cache_ligatures_off: HashMap<Box<str>, Arc<ShapedText>>,
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
            cache_ligatures_off: HashMap::new(),
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
        self.layout_with(text, size, max_width, TextShape::default())
    }

    /// [`layout`](Self::layout) with the additive shaping knobs (story #310):
    /// `shape.line_height_px` overrides the per-line advance and centers each
    /// line's intrinsic box within the fixed box ([`half_leading`], Figma's
    /// model), `shape.letter_spacing` tracks each glyph in both the measured
    /// width and the placement pen, and `shape.align` shifts each line within
    /// `max_width` (or the widest line when `None`). With
    /// [`TextShape::default`] this is exactly `layout`'s previous behavior, so
    /// every existing call site — the E7 oracle and goldens included — renders
    /// identically.
    pub fn layout_with(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
    ) -> TextLayout {
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
                let shaped = self.shaped(paragraph, &bidi, shape.ligatures_off);
                // An empty chunk has no bidi paragraph: one empty line.
                if bidi.paragraphs.is_empty() {
                    // A blank line has no glyphs; measure its box by the
                    // primary font. A fixed line height overrides the advance
                    // and centers the intrinsic box within it (half-leading,
                    // story #310 corrected by #332).
                    let (ascent, intrinsic) = self.line_box(std::iter::empty(), &scales);
                    lines.push(Line {
                        glyphs: Vec::new(),
                        width: 0.0,
                        baseline_y: pen_y + ascent + half_leading(&shape, intrinsic),
                    });
                    pen_y += shape.line_height_px.unwrap_or(intrinsic);
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
                    for range in layout::break_lines(
                        paragraph,
                        &shaped,
                        gs..ge,
                        &scales,
                        max_width,
                        shape.letter_spacing,
                    ) {
                        // The line's box comes from the fonts that shaped
                        // its glyphs, not the primary (story #219). A fixed
                        // line height overrides the advance and centers the
                        // intrinsic box within it — half-leading, Figma's
                        // model: the baseline sits at ascent plus half the
                        // leading, lifted under negative leading and lowered
                        // under positive (story #310, corrected by the #332
                        // import oracle against Figma's own render).
                        let (ascent, intrinsic) = self.line_box(
                            shaped.glyphs[range.clone()].iter().map(|g| g.font as usize),
                            &scales,
                        );
                        let baseline = pen_y + ascent + half_leading(&shape, intrinsic);
                        pen_y += shape.line_height_px.unwrap_or(intrinsic);
                        rtl_lines.push(para.level.is_rtl());
                        lines.push(layout::position_line(
                            &bidi,
                            para,
                            &shaped,
                            range,
                            &scales,
                            baseline,
                            shape.letter_spacing,
                        ));
                    }
                }
            }
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        // Horizontal alignment within the container (story #310). `Left` is the
        // default and reproduces the pre-#310 flush placement: an LTR line stays
        // at x = 0, an RTL-base line flushes right by direction. `Center` and
        // `Right` shift every line by the container's free space, regardless of
        // direction. A line wider than the container overflows leftward past
        // x = 0, mirroring the prior LTR overflow.
        let container = max_width.unwrap_or(width);
        for (line, rtl) in lines.iter_mut().zip(&rtl_lines) {
            let shift = match shape.align {
                TextAlign::Left if *rtl => container - line.width,
                TextAlign::Left => 0.0,
                TextAlign::Center => (container - line.width) / 2.0,
                TextAlign::Right => container - line.width,
            };
            if shift != 0.0 {
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

    /// The cache key is the paragraph text plus `ligatures_off`: resolved
    /// bidi levels and the per-run context (Arabic/Plain, `shape::
    /// RunContext`) are a pure function of the text alone, but
    /// `ligatures_off` is an authored knob the text carries no trace of, so
    /// the same text shaped under each posture must not collide in one
    /// entry (story #341). `ligatures_off` selects which of the two maps to
    /// probe and fill; the default (`false`) path is `cache`, unchanged from
    /// before this knob existed.
    fn shaped(
        &mut self,
        paragraph: &str,
        bidi: &BidiInfo<'_>,
        ligatures_off: bool,
    ) -> Arc<ShapedText> {
        let hit = if ligatures_off {
            self.cache_ligatures_off.get(paragraph).cloned()
        } else {
            self.cache.get(paragraph).cloned()
        };
        if let Some(shaped) = hit {
            self.hits += 1;
            return shaped;
        }
        self.misses += 1;
        let shaped = Arc::new(shape::shape_paragraph(&self.fonts, bidi, ligatures_off));
        if ligatures_off {
            self.cache_ligatures_off
                .insert(paragraph.into(), shaped.clone());
        } else {
            self.cache.insert(paragraph.into(), shaped.clone());
        }
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
