//! Build-time atlas pipeline (DESIGN_1.md §7.2, build-time half):
//! font + charset → MSDF glyph atlas keyed by GLYPH ID + metrics blob.
//!
//! The charset is an input parameter (per-locale charsets arrive with
//! the v0.6 charset story); glyph coverage beyond cmap (GSUB closure)
//! enters through `extra_glyph_ids` until then.

mod closure;
mod metrics;

pub use closure::{Closure, charset_closure};
pub use metrics::{
    AtlasInfo, AtlasMetrics, FORMAT_VERSION, FontMetrics, GeneratorInfo, GlyphEntry, font_metrics,
};

/// Everything that can go wrong in the pipeline — named and actionable
/// (P4 posture), std-only.
#[derive(Debug)]
pub enum AtlasError {
    /// Reading the font file failed.
    FontRead(std::path::PathBuf, std::io::Error),
    /// The font file is not a parseable TTF/OTF.
    FontParse(String),
    /// msdf-atlas-gen was not found (checked `MSDF_ATLAS_GEN`, then `PATH`).
    ToolMissing(String),
    /// The found tool is not the pinned version.
    ToolVersion {
        found: String,
        required: &'static str,
    },
    /// The tool ran and failed.
    ToolFailed { status: Option<i32>, stderr: String },
    /// The tool's output did not match expectations.
    ToolOutput(String),
    /// Metrics blob encode/decode failure.
    Metrics(String),
    /// Filesystem failure outside the font file.
    Io(std::io::Error),
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontRead(p, e) => write!(f, "cannot read font {}: {e}", p.display()),
            Self::FontParse(m) => write!(f, "cannot parse font: {m}"),
            Self::ToolMissing(hint) => write!(f, "msdf-atlas-gen not found: {hint}"),
            Self::ToolVersion { found, required } => {
                write!(f, "msdf-atlas-gen {found} found, {required} required")
            }
            Self::ToolFailed { status, stderr } => {
                write!(f, "msdf-atlas-gen failed (status {status:?}): {stderr}")
            }
            Self::ToolOutput(m) => write!(f, "unexpected msdf-atlas-gen output: {m}"),
            Self::Metrics(m) => write!(f, "metrics blob: {m}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for AtlasError {}

impl From<std::io::Error> for AtlasError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
