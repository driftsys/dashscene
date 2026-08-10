//! msdf-atlas-gen wrapper: discovery, version gate, canonical
//! invocation (R7: fixed argument order, pinned seed), layout parse.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use serde::Deserialize;

use super::AtlasError;

/// The spike-validated version (docs/technotes/arabic-atlas-coverage.md).
/// Anything else is a named error — R7 forbids silent generator drift.
pub const REQUIRED_TOOL_VERSION: &str = "1.4.0";

/// Environment variable overriding tool discovery (a path to the
/// msdf-atlas-gen binary); `PATH` is searched otherwise.
pub const TOOL_ENV: &str = "MSDF_ATLAS_GEN";

/// Environment variable that turns tool absence from a test self-skip
/// into a hard failure. CI sets it, so a skipped test cannot be
/// reported as passing there.
pub const REQUIRE_TOOL_ENV: &str = "DASHSCENE_REQUIRE_ATLAS_TOOL";

/// Scratch file names inside the tool's working directory; also the
/// canonical names recorded in blob provenance.
pub(crate) const GLYPHSET_SCRATCH: &str = "glyphs.txt";
pub(crate) const LAYOUT_SCRATCH: &str = "atlas.json";

/// Finds msdf-atlas-gen — [`TOOL_ENV`] override first, then `PATH` —
/// and enforces [`REQUIRED_TOOL_VERSION`]. A successful probe is
/// cached for the process lifetime (the environment cannot change the
/// answer mid-process); failures are re-probed on every call.
pub fn find_tool_checked() -> Result<PathBuf, AtlasError> {
    static CHECKED: OnceLock<PathBuf> = OnceLock::new();
    if let Some(p) = CHECKED.get() {
        return Ok(p.clone());
    }
    let candidate = match std::env::var_os(TOOL_ENV) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("msdf-atlas-gen"),
    };
    let output = Command::new(&candidate)
        .arg("-help")
        .output()
        .map_err(|e| {
            AtlasError::ToolMissing(format!(
                "{} ({e}); install with `brew install msdf-atlas-gen` or set {TOOL_ENV}",
                candidate.display()
            ))
        })?;
    let text = String::from_utf8_lossy(&output.stdout);
    let found = parse_banner_version(&text).ok_or_else(|| {
        // A binary that spawns but cannot run (loader error, wrong
        // tool) lands here — surface its own evidence, not just ours.
        AtlasError::ToolOutput(format!(
            "no version banner in -help output (exit: {:?}, stderr: {})",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    })?;
    if found != REQUIRED_TOOL_VERSION {
        return Err(AtlasError::ToolVersion {
            found,
            required: REQUIRED_TOOL_VERSION,
        });
    }
    let _ = CHECKED.set(candidate.clone());
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

/// One invocation, built once: the argv actually executed (`OsString`,
/// so non-UTF-8 paths survive) and the provenance vector recorded in
/// the blob (canonical file names, never machine paths). Building both
/// from one pass makes recorded == executed hold by construction (R7).
/// The flag order is fixed; changing it changes the artifacts, so
/// treat any edit as a format decision.
pub(crate) struct Invocation {
    pub exec: Vec<OsString>,
    pub provenance: Vec<String>,
}

pub(crate) fn build_invocation(
    font: &Path,
    glyphset: &Path,
    out_png: &Path,
    out_json: &Path,
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Invocation {
    fn plain(inv: &mut Invocation, s: &str) {
        inv.exec.push(s.into());
        inv.provenance.push(s.to_string());
    }
    fn path(inv: &mut Invocation, flag: &str, real: &Path, canonical: String) {
        plain(inv, flag);
        inv.exec.push(real.as_os_str().to_owned());
        inv.provenance.push(canonical);
    }

    let mut inv = Invocation {
        exec: Vec::new(),
        provenance: Vec::new(),
    };
    let font_name = font
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| font.display().to_string());

    path(&mut inv, "-font", font, font_name);
    path(
        &mut inv,
        "-glyphset",
        glyphset,
        GLYPHSET_SCRATCH.to_string(),
    );
    for s in [
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
    ] {
        plain(&mut inv, s);
    }
    path(
        &mut inv,
        "-imageout",
        out_png,
        super::ATLAS_IMAGE_FILE.to_string(),
    );
    path(&mut inv, "-json", out_json, LAYOUT_SCRATCH.to_string());
    inv
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

/// Runs the tool in a scratch directory and returns the raw PNG bytes,
/// the parsed layout, and the provenance args of the invocation that
/// produced them.
pub(crate) fn run(
    tool: &Path,
    font: &Path,
    glyph_ids: &[u16],
    px_per_em: u16,
    px_range: u16,
    seed: u64,
) -> Result<(Vec<u8>, Layout, Vec<String>), AtlasError> {
    let dir = tempfile::tempdir()?;
    let glyphset = dir.path().join(GLYPHSET_SCRATCH);
    let out_png = dir.path().join(super::ATLAS_IMAGE_FILE);
    let out_json = dir.path().join(LAYOUT_SCRATCH);
    let list = glyph_ids
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    std::fs::write(&glyphset, list)?;

    let inv = build_invocation(
        font, &glyphset, &out_png, &out_json, px_per_em, px_range, seed,
    );
    let output = Command::new(tool).args(&inv.exec).output()?;
    if !output.status.success() {
        return Err(AtlasError::ToolFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let png = std::fs::read(&out_png)?;
    let layout = parse_layout(&std::fs::read_to_string(&out_json)?)?;
    Ok((png, layout, inv.provenance))
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
    fn builds_canonical_invocation() {
        let inv = build_invocation(
            Path::new("/tmp/fonts/f.ttf"),
            Path::new("/scratch/g.txt"),
            Path::new("/scratch/a.png"),
            Path::new("/scratch/a.json"),
            32,
            4,
            1,
        );
        // Provenance holds canonical names, never machine paths.
        let expect: Vec<String> = [
            "-font",
            "f.ttf",
            "-glyphset",
            "glyphs.txt",
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
            "atlas.png",
            "-json",
            "atlas.json",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(inv.provenance, expect);
        // The executed argv mirrors the provenance one-to-one, with the
        // real paths in the path slots.
        assert_eq!(inv.exec.len(), inv.provenance.len());
        assert_eq!(inv.exec[1], std::ffi::OsString::from("/tmp/fonts/f.ttf"));
        assert_eq!(inv.exec[3], std::ffi::OsString::from("/scratch/g.txt"));
        assert_eq!(inv.exec[19], std::ffi::OsString::from("/scratch/a.png"));
        assert_eq!(inv.exec[21], std::ffi::OsString::from("/scratch/a.json"));
        assert_eq!(inv.exec[4], std::ffi::OsString::from("-type"));
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
