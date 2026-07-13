//! dashc — compiler CLI entry point (native target only; the wasm32
//! target exposes the library surface directly, see `src/lib.rs`).
//!
//! `compile_figma` lowers Figma REST JSON into a `.dsb`, but this binary does
//! not expose it as a subcommand: the acceptance path for that entry point is
//! a library call, and #17's Deno importer consumes it through the wasm32
//! target, not this native CLI. Until a native subcommand earns its place,
//! the CLI's job is to *check* an existing `.dsb` — which is the same load
//! gate the runtime runs, so it earns its place on its own.

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
            eprintln!("dashc — the DSB compiler");
            eprintln!();
            eprintln!("  dashc check <file.dsb>   validate a document; exit 1 if it is blocked");
            eprintln!();
            eprintln!("Compiling Figma REST JSON has no CLI subcommand: `compile_figma` is a");
            eprintln!("library entry point, consumed by the Deno importer (#17) through the");
            eprintln!("wasm32 target, not by this native binary.");
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
