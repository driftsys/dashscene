//! Shared fixtures for this crate's integration tests.

/// The committed corpus fixture font (see corpus/fonts/noto-sans/).
pub const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// The committed Arabic corpus fixture font (see
/// corpus/fonts/noto-sans-arabic/) — carries GSUB/GPOS/cmap.
#[allow(dead_code)]
pub const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
