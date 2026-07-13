//! dashc — compiler CLI entry point (native target only; the wasm32
//! target exposes the library surface directly, see `src/lib.rs`).
//!
//! The Figma front end is not wired yet: lowering Figma REST JSON into
//! `Scd` needs a captured fixture to build against, and the v0.3 fixture has
//! not been captured (`corpus/figma-fixtures/` holds only its manifest).
//! Until then the CLI's job is to *check* an existing `.dsb` — which is the
//! same load gate the runtime runs, so it earns its place on its own.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => match args.next() {
            Some(path) => check(&path),
            None => {
                eprintln!("dashc check <file.dsb>");
                ExitCode::from(2)
            }
        },
        _ => {
            eprintln!("dashc — the SCD compiler");
            eprintln!();
            eprintln!("  dashc check <file.dsb>   validate a document; exit 1 if it is blocked");
            eprintln!();
            eprintln!("Compiling Figma REST JSON is not wired yet: story #16 ships the SCD");
            eprintln!("model, the deterministic emitter, and the load path, while the Figma");
            eprintln!("lowering waits on a captured fixture.");
            ExitCode::from(2)
        }
    }
}

/// Runs the load gate over a `.dsb` and reports.
///
/// An error blocks the document (DESIGN §5, R6); a warning does not, but a
/// strict build refuses it (waivers are v0.7, issue #41).
fn check(path: &str) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("dashc: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    // The flatbuffer verifier first: it checks structure, and the load gate
    // assumes a structurally valid buffer.
    let document = match dashbuf::root_as_document(&bytes) {
        Ok(document) => document,
        Err(e) => {
            eprintln!("dashc: {path} is not a valid .dsb buffer: {e}");
            return ExitCode::from(1);
        }
    };

    let report = dashscene_validator::validate_document(&document);
    print!("{report}");

    if report.has_errors() {
        eprintln!(
            "dashc: {path} is blocked by {} error(s)",
            report.errors().count()
        );
        ExitCode::from(1)
    } else {
        println!("dashc: {path} is valid");
        ExitCode::SUCCESS
    }
}
