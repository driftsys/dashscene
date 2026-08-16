//! Font handle for the runtime pipeline: owns the font bytes and the
//! hhea vertical metrics — the same [`FontMetrics`] numbers the atlas metrics
//! blob records (docs/design/architecture.md: ttf-parser is the metrics
//! source), extracted through one shared function so the runtime and the
//! build-time blob cannot disagree.

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

/// One named family of the cascade: the name a document's
/// `TextStyle::family` is matched against, and the faces it resolves a
/// weight within (story #385).
///
/// The name is declared by whoever assembles the cascade, for the same
/// reason [`WeightedFont`] declares its own weight rather than reading
/// OS/2: it is explicit and auditable, and a face's own name table is
/// the wrong source. Inter's Medium and SemiBold faces declare name ID 1
/// as `Inter Medium` and `Inter Semi Bold` — the four-styles-per-family
/// convention — so reading the name per face would put those two weights
/// in families of their own and stop a document asking for `Inter` from
/// ever reaching them.
///
/// An empty name matches nothing and is never reported as a
/// substitution: it is what the unnamed constructors
/// ([`Typesetter::with_fonts`](super::Typesetter::with_fonts),
/// [`Typesetter::with_font_families`](super::Typesetter::with_font_families))
/// declare, so a cascade that predates family matching behaves exactly
/// as it did.
#[derive(Debug, Clone)]
pub struct FontFamily {
    /// The family name, matched ASCII-case-insensitively after trimming.
    pub name: String,
    /// The family's faces, in declared order. Never empty.
    pub faces: Vec<WeightedFont>,
}

impl FontFamily {
    /// A named family over an ordered set of weighted faces.
    pub fn new(name: impl Into<String>, faces: Vec<WeightedFont>) -> FontFamily {
        FontFamily {
            name: name.into(),
            faces,
        }
    }

    /// An unnamed family — the shape every pre-#385 cascade declared.
    pub fn unnamed(faces: Vec<WeightedFont>) -> FontFamily {
        FontFamily::new(String::new(), faces)
    }

    /// Whether this family answers to `requested`.
    pub fn matches(&self, requested: &str) -> bool {
        FontFamily::name_matches(&self.name, requested)
    }

    /// Whether a declared family name answers to a requested one, ignoring
    /// surrounding whitespace and ASCII case. An empty name on either side
    /// never matches: a cascade that declares no names has no family to
    /// prefer, and a document that names no family expresses no preference.
    ///
    /// ASCII case only, deliberately. A Unicode-aware fold would make the
    /// match depend on locale rules for a comparison whose inputs are font
    /// family names, and every family the corpus carries is ASCII.
    pub fn name_matches(name: &str, requested: &str) -> bool {
        let (name, requested) = (name.trim(), requested.trim());
        !name.is_empty() && !requested.is_empty() && name.eq_ignore_ascii_case(requested)
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
