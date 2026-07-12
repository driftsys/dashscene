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
        // Gate on the leading version field before decoding the body:
        // a future-version blob must produce the version error, not a
        // decode error from a mismatched layout.
        let (version, _) = postcard::take_from_bytes::<u32>(bytes)
            .map_err(|e| AtlasError::Metrics(format!("blob decode failed: {e}")))?;
        if version != FORMAT_VERSION {
            return Err(AtlasError::Metrics(format!(
                "unsupported blob format version {version} (supported: {FORMAT_VERSION})"
            )));
        }
        let (m, rest) = postcard::take_from_bytes::<AtlasMetrics>(bytes)
            .map_err(|e| AtlasError::Metrics(format!("blob decode failed: {e}")))?;
        if !rest.is_empty() {
            return Err(AtlasError::Metrics(format!(
                "{} trailing bytes after the blob",
                rest.len()
            )));
        }
        // Enforce the documented field contracts at the parse boundary,
        // not only in the producer: consumers are entitled to
        // binary-search `glyphs`, and a silent violation would drop
        // glyphs without a diagnostic (P4).
        if !m.glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id) {
            return Err(AtlasError::Metrics(
                "glyph entries not sorted and unique by glyph id".to_string(),
            ));
        }
        if !m.missing_codepoints.windows(2).all(|w| w[0] < w[1]) {
            return Err(AtlasError::Metrics(
                "missing codepoints not ascending".to_string(),
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &str = crate::atlas::TEST_FONT;

    fn sample() -> AtlasMetrics {
        AtlasMetrics {
            format_version: FORMAT_VERSION,
            generator: GeneratorInfo {
                tool_version: "1.4.0".into(),
                args: vec!["-type".into(), "msdf".into()],
            },
            font: FontMetrics {
                units_per_em: 1000,
                ascender: 1069,
                descender: -293,
                line_gap: 0,
            },
            atlas: AtlasInfo {
                width: 128,
                height: 128,
                px_per_em: 32,
                distance_range_px: 4.0,
            },
            glyphs: vec![
                GlyphEntry {
                    glyph_id: 0,
                    advance_units: 600,
                    plane_em: None,
                    atlas_px: None,
                },
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
    fn rejects_unknown_format_version_by_name() {
        let mut m = sample();
        m.format_version = FORMAT_VERSION + 1;
        let bytes = m.to_bytes();
        let err = AtlasMetrics::from_bytes(&bytes).unwrap_err();
        assert!(
            err.to_string().contains("unsupported blob format version"),
            "want the version error, got: {err}"
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(AtlasMetrics::from_bytes(&[0xff, 0x00, 0x13]).is_err());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = sample().to_bytes();
        bytes.extend_from_slice(&[0xAB; 22]);
        let err = AtlasMetrics::from_bytes(&bytes).unwrap_err();
        assert!(
            err.to_string().contains("trailing bytes"),
            "want the trailing-bytes error, got: {err}"
        );
    }

    #[test]
    fn rejects_unsorted_glyph_entries() {
        let mut m = sample();
        m.glyphs.reverse();
        let bytes = m.to_bytes();
        let err = AtlasMetrics::from_bytes(&bytes).unwrap_err();
        assert!(
            err.to_string().contains("not sorted"),
            "want the sortedness error, got: {err}"
        );
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
