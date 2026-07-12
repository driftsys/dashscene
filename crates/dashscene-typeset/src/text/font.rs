//! Font handle for the runtime pipeline: owns the font bytes, carries
//! the hhea vertical metrics (the same numbers the atlas metrics blob
//! records — DESIGN_1.md §2: ttf-parser is the metrics source).

use std::sync::Arc;

use super::TypesetError;

/// One loaded font face. Owns the bytes; rustybuzz faces are
/// constructed on demand (the shaped-run cache sits in front of
/// shaping, so construction is off the hot path — see the design
/// record before optimizing this).
#[derive(Clone)]
pub struct Font {
    data: Arc<Vec<u8>>,
    index: u32,
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("bytes", &self.data.len())
            .field("index", &self.index)
            .field("units_per_em", &self.units_per_em)
            .finish()
    }
}

impl Font {
    /// Parses and validates the face once; everything downstream can
    /// rely on the bytes being a valid font.
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<Font, TypesetError> {
        let face = ttf_parser::Face::parse(&data, index)
            .map_err(|e| TypesetError::FontParse(e.to_string()))?;
        let (units_per_em, ascender, descender, line_gap) = (
            face.units_per_em(),
            face.ascender(),
            face.descender(),
            face.line_gap(),
        );
        if rustybuzz::Face::from_slice(&data, index).is_none() {
            return Err(TypesetError::FontParse(
                "rustybuzz cannot read this face".to_string(),
            ));
        }
        Ok(Font {
            data: Arc::new(data),
            index,
            units_per_em,
            ascender,
            descender,
            line_gap,
        })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// hhea ascender, font units (positive).
    pub fn ascender(&self) -> i16 {
        self.ascender
    }

    /// hhea descender, font units (negative).
    pub fn descender(&self) -> i16 {
        self.descender
    }

    /// hhea line gap, font units.
    pub fn line_gap(&self) -> i16 {
        self.line_gap
    }

    /// A shaping face over the owned bytes.
    pub(crate) fn face(&self) -> rustybuzz::Face<'_> {
        rustybuzz::Face::from_slice(&self.data, self.index).expect("validated at construction")
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
    }

    #[test]
    fn rejects_garbage() {
        assert!(Font::from_bytes(vec![0xde, 0xad], 0).is_err());
    }
}
