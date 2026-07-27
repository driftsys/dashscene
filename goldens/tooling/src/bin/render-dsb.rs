//! `render-dsb <in.dsb> <out.png> [--profile raw|hifi|lofi]` — load a committed
//! `.dsb` and render it through the v0 Skia reference painter to a PNG. A thin
//! wrapper over [`goldens::render::render_dsb`]; the live entry point is
//! `just render` (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).
//!
//! `--profile` is the Gfx QA profile preview (story #435): the document's
//! assets are packed under that quality profile in memory, the derived bank is
//! assembled, and the reference painter renders it — so a designer sees the
//! same view of an imported file that the profile-preview oracle measures. RAW
//! is the null binding and renders the file unchanged, which is what makes it
//! the reference arm rather than a fourth thing.
//!
//! What this view cannot show, so a target bench confirms a short list rather
//! than discovering quality: GPU filtering behaviour, driver-level effects
//! (vendor bandwidth compression such as UBWC, and the NVIDIA case where ASTC
//! is emulated rather than sampled natively), and where in a target pipeline
//! the sRGB transfer function is applied.

use std::process::ExitCode;

/// The usage line, in one place so every way of getting the arguments wrong
/// prints the same text.
const USAGE: &str = "usage: render-dsb <in.dsb> <out.png> [--profile raw|hifi|lofi]";

fn main() -> ExitCode {
    let mut positional: Vec<String> = Vec::new();
    let mut profile_name: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => match args.next() {
                Some(value) => profile_name = Some(value),
                None => {
                    eprintln!("render-dsb: --profile needs a value\n{USAGE}");
                    return ExitCode::FAILURE;
                }
            },
            other if other.starts_with("--") => {
                eprintln!("render-dsb: unknown option {other}\n{USAGE}");
                return ExitCode::FAILURE;
            }
            other => positional.push(other.to_string()),
        }
    }
    let [input, output] = positional.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };

    let dsb = match std::fs::read(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("render-dsb: cannot read {input}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let png = match render(&dsb, profile_name.as_deref()) {
        Ok(png) => png,
        Err(message) => {
            eprintln!("render-dsb: {message}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = std::fs::write(output, &png) {
        eprintln!("render-dsb: cannot write {output}: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!("render-dsb: wrote {output} ({} bytes)", png.len());
    ExitCode::SUCCESS
}

/// Renders under the named profile, or unchanged when no profile was asked
/// for.
///
/// An unrecognised name is reported alongside the set that is accepted, and
/// never resolved to a default: silently rendering RAW when the caller asked
/// for LoFi would answer a question nobody asked.
#[cfg(feature = "profile-preview")]
fn render(dsb: &[u8], profile_name: Option<&str>) -> Result<Vec<u8>, String> {
    let Some(name) = profile_name else {
        return Ok(goldens::render::render_dsb(dsb));
    };
    let profile = goldens::profile::profile_named(name).ok_or_else(|| {
        format!(
            "{name} is not a profile — expected one of {}",
            goldens::profile::PROFILE_NAMES.join(", ")
        )
    })?;
    goldens::profile::render_under(dsb, profile).map_err(|error| error.to_string())
}

/// The same entry point in a build without the profile preview: RAW renders,
/// and asking for a profile is refused by name rather than answered with the
/// wrong picture.
#[cfg(not(feature = "profile-preview"))]
fn render(dsb: &[u8], profile_name: Option<&str>) -> Result<Vec<u8>, String> {
    if let Some(name) = profile_name {
        return Err(format!(
            "--profile {name} needs the `profile-preview` feature, which this build \
             does not have"
        ));
    }
    Ok(goldens::render::render_dsb(dsb))
}
