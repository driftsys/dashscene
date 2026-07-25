//! Shared fixtures for this crate's integration tests. Each test
//! binary compiles its own copy of this module, so helpers unused by
//! one binary are still used by another — hence the `dead_code`
//! allowances.

use dashscene_typeset::text::{Font, Typesetter};

/// The committed corpus fixture font (see corpus/fonts/noto-sans/).
#[allow(dead_code)]
pub const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// The committed SemiBold (CSS weight 600) corpus fixture font, from the
/// same Noto Sans release as [`FONT`] (story #368).
#[allow(dead_code)]
pub const FONT_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-SemiBold.ttf"
);

/// The committed Bold (CSS weight 700) corpus fixture font, from the same
/// Noto Sans release as [`FONT`] (story #368).
#[allow(dead_code)]
pub const FONT_BOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Bold.ttf"
);

/// The committed Inter corpus fixture faces (see corpus/fonts/inter/) — the
/// family real Figma files are authored in, at the four CSS weights the live
/// targets use (story #385). Unhinted static CFF, unlike the Noto TrueType
/// faces above; the pipeline reads outlines through the same external baker
/// either way.
#[allow(dead_code)]
pub const FONT_INTER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Regular.otf"
);

/// The committed Inter Medium (CSS weight 500) face (story #385).
#[allow(dead_code)]
pub const FONT_INTER_MEDIUM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Medium.otf"
);

/// The committed Inter SemiBold (CSS weight 600) face (story #385).
#[allow(dead_code)]
pub const FONT_INTER_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-SemiBold.otf"
);

/// The committed Inter Bold (CSS weight 700) face (story #385).
#[allow(dead_code)]
pub const FONT_INTER_BOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Bold.otf"
);

/// The committed Arabic corpus fixture font (see
/// corpus/fonts/noto-sans-arabic/) — carries GSUB/GPOS/cmap.
#[allow(dead_code)]
pub const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);

/// A fixture font's bytes.
#[allow(dead_code)]
pub fn font_data(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("fixture font present")
}

/// A typesetter over a fixture font.
#[allow(dead_code)]
pub fn typesetter(path: &str) -> Typesetter {
    Typesetter::new(Font::from_bytes(font_data(path), 0).expect("loads"))
}

/// The nominal cmap glyph id of `c` in a fixture font.
#[allow(dead_code)]
pub fn cmap(path: &str, c: char) -> u16 {
    let data = font_data(path);
    ttf_parser::Face::parse(&data, 0)
        .unwrap()
        .glyph_index(c)
        .unwrap()
        .0
}
