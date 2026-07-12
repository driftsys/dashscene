//! Build-time atlas pipeline (DESIGN_1.md §7.2, build-time half):
//! font + charset → MSDF glyph atlas keyed by GLYPH ID + metrics blob.
//!
//! The charset is an input parameter (per-locale charsets arrive with
//! the v0.6 charset story); glyph coverage beyond cmap (GSUB closure)
//! enters through `extra_glyph_ids` until then.

mod closure;
mod metrics;
mod tool;

pub use closure::{Closure, charset_closure};
pub use metrics::{
    AtlasInfo, AtlasMetrics, FORMAT_VERSION, FontMetrics, GeneratorInfo, GlyphEntry, font_metrics,
};
pub use tool::{REQUIRED_TOOL_VERSION, find_tool_checked};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// File names inside an atlas bundle directory.
pub const ATLAS_IMAGE_FILE: &str = "atlas.png";
pub const ATLAS_METRICS_FILE: &str = "atlas.metrics";

/// Inputs of one atlas generation. Defaults per the Q-1 decision
/// record: 32 px/em, pxrange 4, seed 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtlasSpec {
    pub font_path: PathBuf,
    /// The declared charset (DESIGN_1.md §6.1: coverage comes from
    /// declared charsets, never from document text).
    pub charset: BTreeSet<char>,
    /// Glyphs only shaping can discover (ligatures, contextual forms)
    /// until GSUB closure lands with the v0.6 charset story.
    pub extra_glyph_ids: BTreeSet<u16>,
    pub px_per_em: u16,
    pub px_range: u16,
    pub seed: u64,
}

impl AtlasSpec {
    pub fn new(font_path: impl Into<PathBuf>, charset: BTreeSet<char>) -> Self {
        Self {
            font_path: font_path.into(),
            charset,
            extra_glyph_ids: BTreeSet::new(),
            px_per_em: 32,
            px_range: 4,
            seed: 1,
        }
    }
}

/// The two build artifacts, in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasBundle {
    /// The tool's PNG bytes, untouched (R7: no recompression).
    pub image_png: Vec<u8>,
    pub metrics: AtlasMetrics,
}

/// Runs the whole pipeline: closure → tool → metrics.
pub fn generate(spec: &AtlasSpec) -> Result<AtlasBundle, AtlasError> {
    let tool = tool::find_tool_checked()?;
    let data = std::fs::read(&spec.font_path)
        .map_err(|e| AtlasError::FontRead(spec.font_path.clone(), e))?;
    let face =
        ttf_parser::Face::parse(&data, 0).map_err(|e| AtlasError::FontParse(e.to_string()))?;
    let closure = closure::charset_closure(&face, &spec.charset, &spec.extra_glyph_ids);

    let (image_png, layout) = tool::run(
        &tool,
        &spec.font_path,
        &closure.glyph_ids,
        spec.px_per_em,
        spec.px_range,
        spec.seed,
    )?;

    let glyphs = build_glyph_entries(&face, &closure.glyph_ids, &layout)?;
    // Provenance args use canonical names, never caller paths: an
    // absolute font path would make the blob machine-dependent and
    // break cross-machine byte-identity (R7) by construction.
    let font_name = spec
        .font_path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| spec.font_path.clone());
    let args = tool::build_args(
        &font_name,
        Path::new("glyphs.txt"),
        Path::new(ATLAS_IMAGE_FILE),
        Path::new("atlas.json"),
        spec.px_per_em,
        spec.px_range,
        spec.seed,
    );
    let metrics = AtlasMetrics {
        format_version: FORMAT_VERSION,
        generator: GeneratorInfo {
            tool_version: REQUIRED_TOOL_VERSION.to_string(),
            args,
        },
        font: metrics::font_metrics(&face),
        atlas: AtlasInfo {
            width: layout.atlas.width,
            height: layout.atlas.height,
            px_per_em: spec.px_per_em,
            distance_range_px: layout.atlas.distance_range as f32,
        },
        glyphs,
        missing_codepoints: closure.missing_codepoints,
    };
    Ok(AtlasBundle { image_png, metrics })
}

/// Joins the requested gid list with the tool layout; every requested
/// gid must appear (P4: absence is an error, not a silent gap), and
/// hmtx must agree with the tool's advance within 1e-3 em.
fn build_glyph_entries(
    face: &ttf_parser::Face<'_>,
    glyph_ids: &[u16],
    layout: &tool::Layout,
) -> Result<Vec<GlyphEntry>, AtlasError> {
    use std::collections::BTreeMap;
    let by_index: BTreeMap<u16, &tool::LayoutGlyph> =
        layout.glyphs.iter().map(|g| (g.index, g)).collect();
    let upem = f64::from(face.units_per_em());
    let mut out = Vec::with_capacity(glyph_ids.len());
    for &gid in glyph_ids {
        let lg = by_index
            .get(&gid)
            .ok_or_else(|| AtlasError::ToolOutput(format!("glyph {gid} missing from layout")))?;
        let advance_units = face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0);
        let hmtx_em = f64::from(advance_units) / upem;
        if (hmtx_em - lg.advance).abs() > 1e-3 {
            return Err(AtlasError::ToolOutput(format!(
                "glyph {gid}: hmtx advance {hmtx_em} vs tool {}",
                lg.advance
            )));
        }
        let to4 =
            |b: &tool::LayoutBounds| [b.left as f32, b.bottom as f32, b.right as f32, b.top as f32];
        out.push(GlyphEntry {
            glyph_id: gid,
            advance_units,
            plane_em: lg.plane_bounds.as_ref().map(&to4),
            atlas_px: lg.atlas_bounds.as_ref().map(&to4),
        });
    }
    Ok(out)
}

impl AtlasBundle {
    /// Writes `atlas.png` + `atlas.metrics` into `dir`.
    pub fn write_to_dir(&self, dir: &Path) -> Result<(), AtlasError> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(ATLAS_IMAGE_FILE), &self.image_png)?;
        std::fs::write(dir.join(ATLAS_METRICS_FILE), self.metrics.to_bytes())?;
        Ok(())
    }

    /// Loads a bundle written by [`AtlasBundle::write_to_dir`].
    pub fn load_from_dir(dir: &Path) -> Result<Self, AtlasError> {
        let image_png = std::fs::read(dir.join(ATLAS_IMAGE_FILE))?;
        let metrics = AtlasMetrics::from_bytes(&std::fs::read(dir.join(ATLAS_METRICS_FILE))?)?;
        Ok(Self { image_png, metrics })
    }
}

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
