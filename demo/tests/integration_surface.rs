//! The integration surface: **the five pieces live in the integration crate,
//! and not in the demonstration** (story #741, epic #793).
//!
//! Epic #793's definition of done is explicit that `demo-web` *consuming*
//! `dashscene-web` is not the check — that would pass with two of the five
//! moved and three left inline. The check is that none of the five is still in
//! the demonstration, and that "a test or a lint that names those five and
//! fails when one is found outside the integration crate is the deliverable; a
//! reviewer's judgement is not."
//!
//! This is that test.
//!
//! # Why a source scan, and what that costs
//!
//! The same reasoning `clock_invariant.rs` and `host_policy_invariant.rs`
//! record: the property is about where code *lives*, which no behavioural test
//! can express. And the same limitation, stated here rather than left implied —
//! **this matches one spelling per piece.** A demonstration that reimplemented
//! the frame loop by calling `requestAnimationFrame` through a differently
//! named binding, or refetched a byte range without going through
//! `dashbuf::prefix`, would pass.
//!
//! What it does catch is the regression that actually threatens: a piece
//! drifting back into the demonstration because that is where it is convenient
//! to change it, which is how both hosts came to hold private copies of the
//! frame policy before story #810 moved it.

use std::fs;
use std::path::{Path, PathBuf};

/// The integration crate: where each piece must be.
const INTEGRATION: &str = "crates/dashscene-web/src";

/// The demonstration: where none of them may be.
const DEMONSTRATION: &str = "demo-web/src";

/// The five pieces epic #793 names, each with one spelling that is present
/// where the piece is implemented.
///
/// The spellings are chosen to be the call that *does* the thing rather than a
/// name that describes it, so that moving the code moves the marker with it.
const PIECES: [(&str, &str); 5] = [
    (
        "the canvas-to-surface handoff",
        // `SurfaceRenderer::for_canvas` is the handoff itself.
        "for_canvas",
    ),
    (
        "the requestAnimationFrame loop",
        // The browser call that schedules the next frame.
        "request_animation_frame",
    ),
    (
        "the generation-and-shown contract",
        // Story #810 moved the rule to `dashlang::LiveScene`; what stays here
        // is the loop that reads it, and this is the call that closes the gate.
        "mark_shown",
    ),
    (
        "rebuilding on resize, reporting document_replaced",
        // The renderer is told the generations restart. A host that rebuilds
        // without this leaves the device applying ranges against a chain that
        // no longer connects.
        "document_replaced",
    ),
    (
        "the byte-range .dsb load",
        // Reading the envelope from a prefix rather than parsing a whole file.
        "prefix::plan",
    ),
];

#[test]
fn each_of_the_five_integration_pieces_is_in_the_integration_crate() {
    let workspace = workspace_root();
    let sources = rust_sources(&workspace.join(INTEGRATION));
    assert!(
        !sources.is_empty(),
        "no Rust source found under {INTEGRATION}: a scan that reads no files reports success \
         without having checked anything"
    );
    let code = concatenated_code(&sources);

    let missing: Vec<&str> = PIECES
        .into_iter()
        .filter(|(_, spelling)| !code.contains(spelling))
        .map(|(piece, _)| piece)
        .collect();

    assert!(
        missing.is_empty(),
        "the integration crate does not implement every piece epic #793 names. Missing: {}.\n\
         Either the piece is not there, or its spelling changed and this test's marker needs \
         to change with it — do not delete the entry.",
        missing.join("; ")
    );
}

#[test]
fn none_of_the_five_is_still_in_the_demonstration() {
    let workspace = workspace_root();
    let sources = rust_sources(&workspace.join(DEMONSTRATION));
    assert!(
        !sources.is_empty(),
        "no Rust source found under {DEMONSTRATION}: this test's paths have gone stale"
    );

    let mut offences: Vec<String> = Vec::new();
    for source in sources {
        let text = fs::read_to_string(&source)
            .unwrap_or_else(|error| panic!("reading {}: {error}", source.display()));
        for (index, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            for (piece, spelling) in PIECES {
                if code.contains(spelling) {
                    offences.push(format!(
                        "{}:{}: {piece} — {}",
                        source.display(),
                        index + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offences.is_empty(),
        "an integration piece is back in the demonstration. All five belong to `dashscene-web`, \
         which `demo-web` consumes: an embedder must be able to draw a `.dsb` in a browser \
         without copying code out of the demonstration (epic #793).\n{}",
        offences.join("\n")
    );
}

/// Guards the guard.
///
/// A scan whose matcher is wrong passes on every tree, including the ones it
/// exists to reject. These are the mutation test, committed.
#[test]
fn the_scan_catches_a_piece_moving_back_and_leaves_prose_alone() {
    assert!(
        strip_comment("        self.renderer.document_replaced();").contains("document_replaced"),
        "a call must be caught"
    );
    assert!(
        !strip_comment("    // the renderer is told through document_replaced")
            .contains("document_replaced"),
        "prose naming a piece is not the piece"
    );
    assert!(
        !strip_comment("/// How often the scene's signal is advanced, in milliseconds.")
            .contains("mark_shown"),
        "ordinary English near a marker is not a match"
    );
    // Every piece's spelling must be distinct, or one entry could satisfy the
    // presence test for another and the count would be a fiction.
    for (index, (_, spelling)) in PIECES.into_iter().enumerate() {
        for (other_index, (_, other)) in PIECES.into_iter().enumerate() {
            if index != other_index {
                assert!(
                    !spelling.contains(other),
                    "{spelling:?} contains {other:?}, so one piece would satisfy another"
                );
            }
        }
    }
}

/// One line of Rust with any trailing comment removed.
///
/// Both crates are expected to *discuss* these pieces — `demo-web`'s own module
/// doc says which five it no longer holds, which is exactly the sentence a
/// naive scan would fail on.
fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Every source under `directory`, with comments stripped, joined.
fn concatenated_code(sources: &[PathBuf]) -> String {
    let mut code = String::new();
    for source in sources {
        let text = fs::read_to_string(source)
            .unwrap_or_else(|error| panic!("reading {}: {error}", source.display()));
        for line in text.lines() {
            code.push_str(strip_comment(line));
            code.push('\n');
        }
    }
    code
}

/// Every `.rs` file under `directory`, recursively.
fn rust_sources(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// The workspace root: `demo/`'s parent.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("demo/ has a parent, which is the workspace root")
        .to_path_buf()
}
