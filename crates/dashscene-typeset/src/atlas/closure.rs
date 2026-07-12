//! Charset → glyph-id closure via cmap (DESIGN_1.md §7.2).
//!
//! v0.5 scope: nominal cmap lookups only. Contextual/ligature glyphs
//! that only shaping can discover are supplied via `extra_glyph_ids`
//! (the v0.6 charset story extends closure over GSUB).

use std::collections::BTreeSet;

/// The glyph-id set an atlas must cover, plus the charset entries the
/// font cannot represent (a named diagnostic surface, R6 — the caller
/// decides severity, nothing is dropped silently).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    /// Sorted, deduplicated; always contains glyph id 0 (`.notdef`) so
    /// painters can draw a visible fallback for unmapped input.
    pub glyph_ids: Vec<u16>,
    /// Charset codepoints without a cmap entry, ascending.
    pub missing_codepoints: Vec<u32>,
}

/// Resolves `charset` through the font's cmap and merges
/// `extra_glyph_ids`.
pub fn charset_closure(
    face: &ttf_parser::Face<'_>,
    charset: &BTreeSet<char>,
    extra_glyph_ids: &BTreeSet<u16>,
) -> Closure {
    let mut gids: BTreeSet<u16> = BTreeSet::new();
    gids.insert(0);
    let mut missing = Vec::new();
    for &c in charset {
        match face.glyph_index(c) {
            Some(gid) => {
                gids.insert(gid.0);
            }
            None => missing.push(c as u32),
        }
    }
    gids.extend(extra_glyph_ids.iter().copied());
    Closure {
        glyph_ids: gids.into_iter().collect(),
        missing_codepoints: missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
    );

    fn face(data: &[u8]) -> ttf_parser::Face<'_> {
        ttf_parser::Face::parse(data, 0).expect("fixture font parses")
    }

    #[test]
    fn resolves_covered_codepoints_to_sorted_unique_gids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['B', 'A', 'A', 'a'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert!(c.missing_codepoints.is_empty());
        // .notdef (0) is always included, plus one gid per distinct char.
        assert_eq!(c.glyph_ids.len(), 4);
        assert_eq!(c.glyph_ids[0], 0);
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, c.glyph_ids, "sorted and deduplicated");
        assert!(c.glyph_ids[1..].iter().all(|&g| g != 0));
    }

    #[test]
    fn reports_uncovered_codepoints_sorted() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        // Syriac letters — absent from a Latin/Greek/Cyrillic font.
        let charset: BTreeSet<char> = ['\u{0712}', '\u{0710}', 'A'].into_iter().collect();
        let c = charset_closure(&face, &charset, &BTreeSet::new());
        assert_eq!(c.missing_codepoints, vec![0x0710, 0x0712]);
        assert_eq!(c.glyph_ids.len(), 2); // .notdef + 'A'
    }

    #[test]
    fn merges_extra_glyph_ids() {
        let data = std::fs::read(FONT).expect("fixture font present");
        let face = face(&data);
        let charset: BTreeSet<char> = ['A'].into_iter().collect();
        let extras: BTreeSet<u16> = [700u16, 3].into_iter().collect();
        let c = charset_closure(&face, &charset, &extras);
        assert!(c.glyph_ids.contains(&700));
        assert!(c.glyph_ids.contains(&3));
        let mut sorted = c.glyph_ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, c.glyph_ids);
    }
}
