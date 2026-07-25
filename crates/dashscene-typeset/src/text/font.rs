//! Font handle for the runtime pipeline: owns the font bytes and the
//! hhea vertical metrics — the same [`FontMetrics`](crate::atlas::FontMetrics)
//! numbers the atlas metrics blob records (docs/design/architecture.md:
//! ttf-parser is the metrics source), extracted through one shared function so
//! the runtime and the build-time blob cannot disagree.

use std::sync::Arc;

use crate::atlas::FontMetrics;

use super::TypesetError;

/// One loaded font face. Owns the bytes; rustybuzz faces are
/// constructed on demand (the shaped-run cache sits in front of
/// shaping, so construction is off the hot path — see the design
/// record before optimizing this).
#[derive(Clone)]
pub struct Font {
    data: Arc<Vec<u8>>,
    index: u32,
    metrics: FontMetrics,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("bytes", &self.data.len())
            .field("index", &self.index)
            .field("metrics", &self.metrics)
            .finish()
    }
}

impl Font {
    /// Parses and validates the face once (rustybuzz's face wraps and
    /// derefs to ttf-parser's, so a single parse serves shaping and
    /// metrics); everything downstream can rely on the bytes being a
    /// valid font.
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Font, TypesetError> {
        let metrics = {
            let face = rustybuzz::Face::from_slice(&data, index)
                .ok_or_else(|| TypesetError::FontParse("not a parseable font face".to_string()))?;
            crate::atlas::font_metrics(&face)
        };
        Ok(Font {
            data: Arc::new(data),
            index,
            metrics,
        })
    }

    /// The blob-shared vertical metrics (hhea, font units).
    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }

    pub fn units_per_em(&self) -> u16 {
        self.metrics.units_per_em
    }

    /// hhea ascender, font units (positive).
    pub fn ascender(&self) -> i16 {
        self.metrics.ascender
    }

    /// hhea descender, font units (negative).
    pub fn descender(&self) -> i16 {
        self.metrics.descender
    }

    /// hhea line gap, font units.
    pub fn line_gap(&self) -> i16 {
        self.metrics.line_gap
    }

    /// Baseline-to-baseline distance, font units — the line metric
    /// layout uses and the measure callback (#29) needs.
    pub fn line_advance(&self) -> i32 {
        i32::from(self.metrics.ascender) - i32::from(self.metrics.descender)
            + i32::from(self.metrics.line_gap)
    }

    /// A shaping face over the owned bytes.
    pub(crate) fn face(&self) -> rustybuzz::Face<'_> {
        rustybuzz::Face::from_slice(&self.data, self.index).expect("validated at construction")
    }
}

/// One face of one family at one CSS-scale weight (story #368).
///
/// The weight is the face's *own* weight, declared by whoever assembled
/// the cascade — the typesetter does not read the font's OS/2 table for
/// it. That keeps the declaration explicit and auditable: the cascade
/// says which face stands for weight 700, and
/// [`Typesetter::with_font_families`](super::Typesetter::with_font_families)
/// matches a requested weight against exactly those declarations.
#[derive(Debug, Clone)]
pub struct WeightedFont {
    pub font: Font,
    /// CSS-scale weight, 100..=900 — the same scale the document carries
    /// (`dashbuf` `weight: ushort = 400`, `dashscene_core::TextStyle`).
    pub weight: u16,
}

impl WeightedFont {
    /// A face at the given CSS weight.
    pub fn new(font: Font, weight: u16) -> WeightedFont {
        WeightedFont { font, weight }
    }

    /// A face at weight 400 — the document's default and the weight every
    /// pre-#368 cascade implicitly declared.
    pub fn regular(font: Font) -> WeightedFont {
        WeightedFont::new(font, 400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_match_ttf_parser() {
        let data = std::fs::read(crate::atlas::TEST_FONT).expect("fixture font present");
        let direct = ttf_parser::Face::parse(&data, 0).expect("parses");
        let font = Font::from_bytes(data.clone(), 0).expect("loads");
        assert_eq!(font.units_per_em(), direct.units_per_em());
        assert_eq!(font.ascender(), direct.ascender());
        assert_eq!(font.descender(), direct.descender());
        assert_eq!(font.line_gap(), direct.line_gap());
        assert!(font.ascender() > 0);
        assert!(font.descender() < 0);
        assert_eq!(
            font.line_advance(),
            i32::from(direct.ascender()) - i32::from(direct.descender())
                + i32::from(direct.line_gap())
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(Font::from_bytes(vec![0xde, 0xad], 0).is_err());
    }
}
