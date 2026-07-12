//! Runtime text pipeline, Latin subset (DESIGN_1.md §7.2): shape
//! (rustybuzz, ligatures off) → greedy line break → positioned glyph
//! runs, with a font-unit shaped-run cache in front of shaping.
//!
//! Bidi splitting and Arabic shaping are the v0.6 stories; this module
//! is single-direction LTR by construction.

mod font;
mod shape;

pub use font::Font;
pub use shape::{ShapedGlyph, ShapedText};

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
