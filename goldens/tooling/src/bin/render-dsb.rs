//! `render-dsb <in.dsb> <out.png>` — load a committed `.dsb` and render it
//! through the v0 Skia reference painter to a PNG. A thin wrapper over
//! [`goldens::render::render_dsb`]; the live entry point is `just render`
//! (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: render-dsb <in.dsb> <out.png>");
        return ExitCode::FAILURE;
    };
    let dsb = match std::fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("render-dsb: cannot read {input}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let png = goldens::render::render_dsb(&dsb);
    if let Err(error) = std::fs::write(&output, &png) {
        eprintln!("render-dsb: cannot write {output}: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!("render-dsb: wrote {output} ({} bytes)", png.len());
    ExitCode::SUCCESS
}
