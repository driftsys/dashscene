//! msdf-atlas-gen wrapper: discovery, version gate, canonical
//! invocation (R7: fixed argument order, pinned seed), layout parse.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::AtlasError;

/// The spike-validated version (docs/technotes/msdf-arabic-atlas-spike.md).
/// Anything else is a named error — R7 forbids silent generator drift.
pub const REQUIRED_TOOL_VERSION: &str = "1.4.0";

/// Finds msdf-atlas-gen — `MSDF_ATLAS_GEN` env override first, then
/// `PATH` — and enforces [`REQUIRED_TOOL_VERSION`].
pub fn find_tool_checked() -> Result<PathBuf, AtlasError> {
    let candidate = match std::env::var_os("MSDF_ATLAS_GEN") {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("msdf-atlas-gen"),
    };
    let output = Command::new(&candidate)
        .arg("-help")
        .output()
        .map_err(|e| {
            AtlasError::ToolMissing(format!(
                "{} ({e}); install with `brew install msdf-atlas-gen` or set MSDF_ATLAS_GEN",
                candidate.display()
            ))
        })?;
    let text = String::from_utf8_lossy(&output.stdout);
    let found = parse_banner_version(&text)
        .ok_or_else(|| AtlasError::ToolOutput("no version banner in -help output".to_string()))?;
    if found != REQUIRED_TOOL_VERSION {
        return Err(AtlasError::ToolVersion {
            found,
            required: REQUIRED_TOOL_VERSION,
        });
    }
    Ok(candidate)
}

/// Extracts `1.4.0` from `MSDF Atlas Generator by Viktor Chlumsky
/// v1.4.0 (with MSDFgen v1.13.0)`.
pub(crate) fn parse_banner_version(banner: &str) -> Option<String> {
    let line = banner
        .lines()
        .find(|l| l.contains("MSDF Atlas Generator"))?;
    let rest = line.split(" v").nth(1)?;
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!version.is_empty()).then_some(version)
}

/// Canonical argument vector — the exact list recorded in the blob's
/// provenance. Order is fixed; changing it changes the artifacts, so
/// treat any edit as a format decision.
pub(crate) fn build_args(
    font: &Path,
    glyphset: &Path,
    out_png: &Path,
    out_json: &Path,
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Vec<String> {
    [
        "-font",
        &font.display().to_string(),
        "-glyphset",
        &glyphset.display().to_string(),
        "-type",
        "msdf",
        "-size",
        &px_per_em.to_string(),
        "-pxrange",
        &px_range.to_string(),
        "-potr",
        "-yorigin",
        "bottom",
        "-nokerning",
        "-seed",
        &seed.to_string(),
        "-format",
        "png",
        "-imageout",
        &out_png.display().to_string(),
        "-json",
        &out_json.display().to_string(),
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[derive(Debug, Deserialize)]
pub(crate) struct Layout {
    pub atlas: LayoutAtlas,
    pub glyphs: Vec<LayoutGlyph>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutAtlas {
    pub width: u32,
    pub height: u32,
    pub distance_range: f64,
    pub y_origin: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutGlyph {
    pub index: u16,
    /// Em units — cross-checked against hmtx in `generate()`, not stored.
    pub advance: f64,
    pub plane_bounds: Option<LayoutBounds>,
    pub atlas_bounds: Option<LayoutBounds>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LayoutBounds {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}

pub(crate) fn parse_layout(json: &str) -> Result<Layout, AtlasError> {
    let layout: Layout = serde_json::from_str(json)
        .map_err(|e| AtlasError::ToolOutput(format!("layout JSON: {e}")))?;
    // The metrics blob's texel-bounds convention (FORMAT_VERSION 1) is
    // bottom-left origin; anything else means the invocation drifted.
    if layout.atlas.y_origin != "bottom" {
        return Err(AtlasError::ToolOutput(format!(
            "unexpected yOrigin {:?} (expected \"bottom\")",
            layout.atlas.y_origin
        )));
    }
    Ok(layout)
}

/// Runs the tool in a scratch directory and returns the raw PNG bytes
/// plus the parsed layout.
pub(crate) fn run(
    tool: &Path,
    font: &Path,
    glyph_ids: &[u16],
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Result<(Vec<u8>, Layout), AtlasError> {
    let dir = tempfile::tempdir()?;
    let glyphset = dir.path().join("glyphs.txt");
    let out_png = dir.path().join("atlas.png");
    let out_json = dir.path().join("atlas.json");
    let list = glyph_ids
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    std::fs::write(&glyphset, list)?;

    let args = build_args(
        font, &glyphset, &out_png, &out_json, px_per_em, px_range, seed,
    );
    let output = Command::new(tool).args(&args).output()?;
    if !output.status.success() {
        return Err(AtlasError::ToolFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let png = std::fs::read(&out_png)?;
    let layout = parse_layout(&std::fs::read_to_string(&out_json)?)?;
    Ok((png, layout))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_the_version_banner() {
        let banner = "MSDF Atlas Generator by Viktor Chlumsky v1.4.0 (with MSDFgen v1.13.0)\n----";
        assert_eq!(parse_banner_version(banner).as_deref(), Some("1.4.0"));
    }

    #[test]
    fn rejects_unrecognized_banner() {
        assert_eq!(parse_banner_version("some other tool"), None);
    }

    #[test]
    fn builds_canonical_args() {
        let args = build_args(
            Path::new("f.ttf"),
            Path::new("g.txt"),
            Path::new("a.png"),
            Path::new("a.json"),
            32,
            4,
            1,
        );
        let expect: Vec<String> = [
            "-font",
            "f.ttf",
            "-glyphset",
            "g.txt",
            "-type",
            "msdf",
            "-size",
            "32",
            "-pxrange",
            "4",
            "-potr",
            "-yorigin",
            "bottom",
            "-nokerning",
            "-seed",
            "1",
            "-format",
            "png",
            "-imageout",
            "a.png",
            "-json",
            "a.json",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(args, expect);
    }

    #[test]
    fn rejects_a_non_bottom_y_origin() {
        let sample = r#"{
            "atlas": { "type": "msdf", "distanceRange": 4, "size": 32,
                       "width": 128, "height": 64, "yOrigin": "top" },
            "glyphs": []
        }"#;
        assert!(parse_layout(sample).is_err());
    }

    #[test]
    fn parses_a_layout_json_sample() {
        let sample = r#"{
            "atlas": { "type": "msdf", "distanceRange": 4, "size": 32,
                       "width": 128, "height": 64, "yOrigin": "bottom" },
            "metrics": { "emSize": 1 },
            "glyphs": [
                { "index": 0, "advance": 0.6 },
                { "index": 36, "advance": 0.639,
                  "planeBounds": { "left": -0.01, "bottom": -0.02, "right": 0.65, "top": 0.72 },
                  "atlasBounds": { "left": 0.5, "bottom": 0.5, "right": 24.5, "top": 26.5 } }
            ]
        }"#;
        let layout = parse_layout(sample).expect("parses");
        assert_eq!(layout.atlas.width, 128);
        assert_eq!(layout.atlas.y_origin, "bottom");
        assert_eq!(layout.glyphs.len(), 2);
        assert_eq!(layout.glyphs[1].index, 36);
        assert!(layout.glyphs[0].plane_bounds.is_none());
        let ab = layout.glyphs[1].atlas_bounds.as_ref().unwrap();
        assert_eq!(ab.right, 24.5);
    }
}
