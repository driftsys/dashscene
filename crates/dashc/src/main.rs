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
            eprintln!("dashc — the dashscene compiler");
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
/// An error blocks the document (docs/design/architecture.md, R6); a warning does not, but a
/// strict build refuses it (waivers are v0.7, issue #41).
fn check(path: &str) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("dashc: cannot read {path}: {e}");
            return ExitCode::from(2);
        }
    };

    // The envelope first: magic, version, the section table against the root
    // hash, the ui section's own content hash, then the flatbuffers verifier
    // over that section, then the null binding that resolves each asset entry
    // to its blob (`docs/design/dsb-container-format.md`).
    // `dashbuf::open_verified` runs all of it in that order and hands back both
    // halves of the document — the entries and the payloads they name. The
    // eager reader is the right one here because this command is checking a
    // file rather than drawing it, which is the distinction story #597 gave the
    // two readers their names for.
    //
    // The failures are reported apart. A pre-envelope `.dsb` is not the same
    // complaint as a valid envelope carrying a bad buffer, nor as a file whose
    // asset entry names a payload it does not carry, and a person holding the
    // wrong kind of broken file needs to be told which.
    //
    // A container failure does not end the check. This command exists to report
    // everything wrong with a file, so losing every referential finding in it
    // because one asset binding failed would make it least useful exactly when
    // it is needed most. The failure is named, and the document gate still runs
    // — without its asset half, which has no payloads to work with.
    let mut file_is_broken = false;
    let (document, payloads) = match dashbuf::open_verified(&bytes) {
        Ok((document, payloads)) => (document, Some(payloads)),
        Err(dashbuf::OpenError::Document(e)) => {
            // The ui section is not a document, so no gate can run at all.
            eprintln!("dashc: {path} does not carry a valid document: {e}");
            return ExitCode::from(1);
        }
        // Every other failure leaves the ui section possibly readable, so they
        // share one recovery. They are still reported apart: a broken envelope
        // and a derivation manifest that does not resolve are different repairs
        // for whoever is holding the file.
        Err(
            e @ (dashbuf::OpenError::Container(_)
            | dashbuf::OpenError::Bindings(_)
            | dashbuf::OpenError::BindingHashLength { .. }
            | dashbuf::OpenError::BindingRepeated { .. }),
        ) => {
            file_is_broken = true;
            match &e {
                dashbuf::OpenError::Container(
                    dashbuf::container::ContainerError::NoBlobForHash,
                ) => eprintln!(
                    "dashc: {path}: an asset entry names a payload the file does not carry: {e}"
                ),
                dashbuf::OpenError::Container(_) => {
                    eprintln!("dashc: {path} is not a valid .dsb file: {e}");
                }
                _ => eprintln!(
                    "dashc: {path}: the derivation manifest does not bind this file's \
                     assets: {e}"
                ),
            }
            // The ui section may still be readable even though the file as a
            // whole is not, in which case every document rule can still report.
            let Some(document) = dashbuf::container::ui_document(&bytes)
                .ok()
                .and_then(|ui| dashbuf::root_as_document(ui).ok())
            else {
                return ExitCode::from(1);
            };
            (document, None)
        }
    };

    // Both halves of the load gate. The second needs the payloads, which is
    // why it is a separate call and why this path opens the file rather than
    // reading the ui section alone (story #437, debt #416): it is what
    // catches an asset entry whose recorded format or extent disagrees with
    // the bytes it names, whichever writer produced them.
    let mut report = dashscene_validator::validate_document(&document);
    if let Some(payloads) = payloads.as_deref() {
        report.extend(
            dashscene_validator::validate_asset_payloads(&document, payloads)
                .diagnostics()
                .iter()
                .cloned(),
        );
    }
    print!("{report}");

    if report.has_errors() {
        eprintln!(
            "dashc: {path} is blocked by {} error(s)",
            report.errors().count()
        );
        ExitCode::from(1)
    } else if file_is_broken {
        // Every rule the document gate could run passed, but the file's own
        // structure is broken, so it must not be reported as valid.
        ExitCode::from(1)
    } else {
        println!("dashc: {path} is valid");
        ExitCode::SUCCESS
    }
}
