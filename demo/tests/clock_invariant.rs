//! The clock invariant: **no crate at or below `LiveScene` reads a clock**
//! (story #572).
//!
//! R4's reproducibility clause rests on this. `LiveScene::tick` takes `dt` as
//! a parameter, so a test passes an explicit sequence and never involves a
//! host; `dashcue`'s per-step arithmetic is pinned to IEEE basic operations
//! plus `sqrt`, so an identical sequence is bit-identical across machines. A
//! helpful `Instant::now()` added anywhere in that stack would remove the
//! property silently, and nothing but this test would notice.
//!
//! The reasoning is recorded in
//! `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
//!
//! # Why the check is a source scan
//!
//! The invariant is about what the source may *contain*, not about what one
//! execution does, so no behavioural test can express it: a run that happens
//! not to call `Instant::now()` proves nothing about the call sites it did not
//! reach. A source scan states the rule directly, costs milliseconds, and
//! fails on the commit that breaks it rather than on the day a golden drifts.
//!
//! # Why it lives in `demo/`
//!
//! Because the host is the counterpart of the rule: `demo/src/shell.rs` is the
//! one place in this repository that reads a clock, and this is the assertion
//! that it stays the only one. Putting the scan inside any of the crates it
//! scans would make a crate police itself, and putting it in a sixteenth crate
//! would add a published crate for a test. `cargo test --workspace` runs it,
//! which is what CI runs.

use std::fs;
use std::path::{Path, PathBuf};

/// Every workspace member that can execute inside `LiveScene::tick`, as a path
/// relative to the workspace root.
///
/// **Maintain this list by hand.** It cannot be derived from Cargo's
/// dependency graph, because the entries the graph would miss are exactly the
/// ones the rule most needs: `dashscene-engine` reaches `tick` as an injected
/// `Box<dyn LayoutSolver>`, `dashscene-typeset` through the engine's measure
/// callback, and `showcase` supplies the injected solver itself — none of the
/// three is a library dependency of `dashlang`. Add an entry here when it
/// becomes reachable from `tick`.
///
/// Paths rather than crate names, because not every member lives under
/// `crates/`: the showcase scenes are a workspace member under `corpus/`
/// (story #574).
const RUNTIME_CRATES: [&str; 6] = [
    // `LiveScene` itself: the per-frame binding flush.
    "crates/dashlang",
    // The scheduler `tick` advances.
    "crates/dashcue",
    // `Txn`, `commit`, the double buffer and the generation stamp.
    "crates/dashscene-core",
    // The injected layout solver.
    "crates/dashscene-engine",
    // Shaping and measurement, reached through the engine.
    "crates/dashscene-typeset",
    // The showcase scenes' own `LayoutSolver`, which is what a scene injects
    // into its `LiveScene` and therefore what runs inside every solving tick.
    "corpus/showcase",
];

/// How a clock read is spelled.
///
/// Both are `std`. Nothing in this workspace depends on `chrono`, `time`,
/// `quanta` or `web_time`, so no third-party clock has a spelling to add; a
/// crate that took one on would need its call named here, which is a change
/// the manifest review would surface.
///
/// The type names `Instant` and `SystemTime` are deliberately *not* listed.
/// Naming a `Duration`-shaped type is not reading a clock, and a rule that
/// forbade the names would fail on a signature that merely accepted one.
const CLOCK_READS: [&str; 2] = ["Instant::now", "SystemTime::now"];

#[test]
fn no_crate_at_or_below_livescene_reads_a_clock() {
    let workspace = workspace_root();
    let mut offences: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for member in RUNTIME_CRATES {
        let src = workspace.join(member).join("src");
        let sources = rust_sources(&src);
        // A scan that scans nothing passes for free. That is the
        // `t2-check-has-no-teeth` failure the v0.13 test tiering exists to
        // remove, and a hand-maintained path list is exactly how a check
        // acquires it.
        assert!(
            !sources.is_empty(),
            "no Rust source found under {}: this test's crate list has gone stale, and a scan \
             that reads no files reports success without having checked anything",
            src.display()
        );
        for source in sources {
            scanned += 1;
            let text = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("reading {}: {error}", source.display()));
            for (index, line) in text.lines().enumerate() {
                for read in clock_reads_in(line) {
                    offences.push(format!(
                        "{}:{}: {read} — {}",
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
        "a crate at or below `LiveScene` reads a clock, which removes the reproducibility R4 \
         rests on. The host owns time: pass the value in as a parameter, the way \
         `LiveScene::tick` takes `dt`. See \
         docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md\n{}",
        offences.join("\n")
    );
    assert!(scanned > 0, "scanned no files at all");
}

/// Guards the guard.
///
/// A source scan whose matcher is wrong passes on every tree, including the
/// ones it exists to reject, and it does so silently. These four lines are the
/// mutation test, committed: two that must be caught, and two legitimate uses
/// that must not be.
#[test]
fn the_scan_catches_a_clock_read_and_leaves_legitimate_lines_alone() {
    assert_eq!(
        clock_reads_in("        let started = std::time::Instant::now();"),
        vec!["Instant::now"],
        "a qualified clock read must be caught"
    );
    assert_eq!(
        clock_reads_in("let stamp = SystemTime::now();"),
        vec!["SystemTime::now"],
        "an unqualified clock read must be caught"
    );
    assert!(
        clock_reads_in("        // never reach for Instant::now() here — the host owns time")
            .is_empty(),
        "prose about the rule is not a violation of it"
    );
    assert!(
        clock_reads_in("let step = Duration::from_millis(16);").is_empty(),
        "naming a duration is not reading a clock"
    );
}

/// The clock reads on one line of Rust source.
///
/// Comments are stripped first, because the crates this scans are expected to
/// *discuss* the rule and a doc comment that names the forbidden call is not a
/// call. The strip is a plain search for `//`, so a `//` inside a string
/// literal ends the scanned part of that line early. The only way that hides a
/// violation is a clock read placed after a string containing `//` on the same
/// line, which `rustfmt` does not produce and which no reviewer would miss.
fn clock_reads_in(line: &str) -> Vec<&'static str> {
    let code = match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    };
    CLOCK_READS
        .into_iter()
        .filter(|read| code.contains(read))
        .collect()
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
