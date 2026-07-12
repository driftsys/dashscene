//! rustybuzz shaping into font-unit glyph runs (DESIGN_1.md §7.2).
//!
//! Ligatures (`liga`, `clig`) are disabled: the atlas closure is
//! cmap-only in v0.5 (`docs/decisions/atlas-closure-cmap-plus-extras.md`),
//! so a ligature glyph would shape to an id the atlas cannot cover.
//! They return together with GSUB charset closure (the v0.6 charset
//! story) as one coordinated change. Kerning stays on — it moves pen
//! positions and needs no atlas coverage.

use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, Feature, UnicodeBuffer};

use super::font::Font;

/// One shaped glyph in font units, offsets preserved (GPOS positions
/// marks through offsets — spike #25 finding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// Byte index of the source character (cluster) in the shaped text.
    pub cluster: u32,
    pub x_advance: i32,
    pub x_offset: i32,
    /// HarfBuzz convention: y-up. Positioning negates it into document
    /// space (y-down).
    pub y_offset: i32,
}

/// A shaped, unpositioned run in font units — the cache value. Size
/// independence is what lets one entry serve every render size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
}

/// Shapes `text` as one forced-LTR run: guessing direction would
/// silently shape RTL input into visual order and corrupt greedy
/// wrapping, so until bidi itemization replaces this entry point (the
/// v0.6 story), every run is LTR by construction — deterministic, and
/// visibly wrong for RTL text rather than quietly reordered.
pub(crate) fn shape(font: &Font, text: &str) -> ShapedText {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    buffer.set_direction(Direction::LeftToRight);
    let features = [
        Feature::new(Tag::from_bytes(b"liga"), 0, ..),
        Feature::new(Tag::from_bytes(b"clig"), 0, ..),
    ];
    let glyphs = rustybuzz::shape(&font.face(), &features, buffer);
    let infos = glyphs.glyph_infos();
    let positions = glyphs.glyph_positions();
    ShapedText {
        glyphs: infos
            .iter()
            .zip(positions)
            .map(|(i, p)| ShapedGlyph {
                // TrueType glyph ids are u16; rustybuzz widens to u32.
                glyph_id: i.glyph_id as u16,
                cluster: i.cluster,
                x_advance: p.x_advance,
                x_offset: p.x_offset,
                y_offset: p.y_offset,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        let data = std::fs::read(crate::atlas::TEST_FONT).expect("fixture font present");
        Font::from_bytes(data, 0).expect("loads")
    }

    fn cmap(font_data: &[u8], c: char) -> u16 {
        ttf_parser::Face::parse(font_data, 0)
            .unwrap()
            .glyph_index(c)
            .unwrap()
            .0
    }

    fn hmtx(font_data: &[u8], c: char) -> u16 {
        let face = ttf_parser::Face::parse(font_data, 0).unwrap();
        let gid = face.glyph_index(c).unwrap();
        face.glyph_hor_advance(gid).unwrap()
    }

    #[test]
    fn shapes_av_with_kerning() {
        let data = std::fs::read(crate::atlas::TEST_FONT).unwrap();
        let shaped = shape(&font(), "AV");
        assert_eq!(shaped.glyphs.len(), 2);
        assert_eq!(shaped.glyphs[0].glyph_id, cmap(&data, 'A'));
        assert_eq!(shaped.glyphs[1].glyph_id, cmap(&data, 'V'));
        let shaped_total: i32 = shaped.glyphs.iter().map(|g| g.x_advance).sum();
        let plain_total = i32::from(hmtx(&data, 'A')) + i32::from(hmtx(&data, 'V'));
        assert!(
            shaped_total < plain_total,
            "kerning must tighten AV: {shaped_total} vs {plain_total}"
        );
    }

    #[test]
    fn liga_disabled_keeps_fi_two_glyphs() {
        let data = std::fs::read(crate::atlas::TEST_FONT).unwrap();
        let shaped = shape(&font(), "fi");
        assert_eq!(shaped.glyphs.len(), 2, "liga must be off");
        assert_eq!(shaped.glyphs[0].glyph_id, cmap(&data, 'f'));
        assert_eq!(shaped.glyphs[1].glyph_id, cmap(&data, 'i'));
    }

    #[test]
    fn clusters_are_byte_indices() {
        let shaped = shape(&font(), "ab");
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, vec![0, 1]);
    }

    #[test]
    fn empty_text_shapes_to_nothing() {
        assert!(shape(&font(), "").glyphs.is_empty());
    }
}
