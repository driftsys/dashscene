//! `cargo run -p demo` — the showcase host (v0.14, epic #568).
//!
//! This is the first thing in the repository that draws into a window, since
//! story #572 the first thing that animates one, and since story #574 the
//! first thing that draws the whole v0 paint vocabulary.
//!
//! What the host is: the [`present`] seam and the Skia implementation behind
//! it (story #571), the [`shell`] frame loop that drives it (story #572),
//! [`scenes`], which chooses what it draws, and [`input`], which maps the
//! pointer and three keys onto what the chosen scene declares (story #573).
//! What it is **not** is the content: the scenes live in `corpus/showcase/`
//! (story #574), and [`document`] points this host at a compiled `.dsb` as a
//! further source (story #575).
//!
//! Nothing in this crate names a node, a signal or a colour. A scene carries
//! the name of the signal input drives and the function a key runs, and the
//! host passes both through without reading them (issue #625).

mod document;
mod input;
mod present;
mod scenes;
mod shell;

use std::error::Error;
use std::process::ExitCode;

use scenes::Selection;

fn main() -> ExitCode {
    // `--dsb` selects the loaded document rather than an authored showcase
    // scene. It sits beside the scene registry rather than inside it because a
    // `.dsb` is a different kind of source: the registry lists scenes authored
    // through the producer API, and the document is replayed through that same
    // API by the loader.
    if std::env::args().nth(1).as_deref() == Some("--dsb") {
        eprintln!("demo: document — a compiled .dsb replayed through the producer API");
        // A compiled document carries no signals, no bindings and no variant
        // table — issue #617 records that this is true of every `.dsb` in the
        // tree — so there is nothing for the pointer to drive and no action for
        // a key to run. An empty signal name is the honest value rather than a
        // special case in the input path: `LiveScene::signal_named` finds
        // nothing under it, which is exactly the right outcome.
        let entry = shell::SceneEntry {
            name: "document",
            build: document::scene,
            pulse: document::pulse,
            signal: "",
            action: None,
        };
        return finish(shell::run("dashscene — document", vec![entry]));
    }

    // One entry or several — the host takes a list either way, and its length
    // is what decides whether it advances (issue #628).
    let showing = match scenes::select(std::env::args().skip(1)) {
        Selection::Scene(scene) => vec![scene],
        Selection::All => showcase::SCENES.iter().collect(),
        Selection::Listed => return ExitCode::SUCCESS,
        Selection::Unknown => return ExitCode::FAILURE,
    };
    for scene in &showing {
        eprintln!("demo: scene {} — {}", scene.name, scene.summary);
    }

    let scenes = showing
        .into_iter()
        .map(|scene| shell::SceneEntry {
            name: scene.name,
            build: scene.build,
            pulse: scene.pulse,
            signal: scene.signal,
            action: scene.action,
        })
        .collect();

    finish(shell::run("dashscene — showcase", scenes))
}

/// Turns the loop's result into the process exit code, reporting the failure
/// first. Shared by the two ways the host can be pointed at something to draw,
/// so a `.dsb` run and a showcase run cannot report a failure differently.
fn finish(result: Result<(), Box<dyn Error>>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(error.as_ref());
            ExitCode::FAILURE
        }
    }
}

/// Prints the failure and every cause behind it, so a windowing-system error
/// arrives with the context that produced it rather than as one bare line.
fn report(error: &dyn Error) {
    eprintln!("demo: {error}");
    let mut cause = error.source();
    while let Some(next) = cause {
        eprintln!("demo:   caused by: {next}");
        cause = next.source();
    }
}
