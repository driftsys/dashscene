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
//! (story #574), and story #575 points this host at a compiled `.dsb` document
//! as a further source.
//!
//! Nothing in this crate names a node, a signal or a colour. A scene carries
//! the name of the signal input drives and the function a key runs, and the
//! host passes both through without reading them (issue #625).

mod input;
mod present;
mod scenes;
mod shell;

use std::error::Error;
use std::process::ExitCode;

use scenes::Selection;

fn main() -> ExitCode {
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

    match shell::run("dashscene — showcase", scenes) {
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
