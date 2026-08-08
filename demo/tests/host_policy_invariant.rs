//! The host-policy invariant: **no host states the frame-delta clamp or the
//! generation gate for itself** (story #810).
//!
//! Both hosts used to carry both rules. The clamp was written twice in two
//! different units — `Duration::from_millis(100)` in `demo/src/shell.rs` and
//! `f64 = 0.1` in `demo-web/src/host.rs` — so keeping the two equal already
//! needed a unit conversion that nothing performed. The generation-and-`shown`
//! contract was duplicated the same way, and the web host documented its own
//! rule by pointing at the other host rather than at the record that binds
//! them.
//!
//! Between two `publish = false` demonstrations that is a minor flaw. Stories
//! #741 and #794 turn these hosts into two *published* integration crates,
//! at which point a duplicated rule becomes a semver-bound agreement that
//! nothing checks. So the rules moved to one owner —
//! `dashlang::LiveScene` — before the crates that would have inherited a copy
//! each (`docs/decisions/crate-name-map.md`, the `dashscene-desktop` section).
//!
//! The reasoning behind the clamp itself is
//! `docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md`.
//! That record's title is also the seam: **the host owns the clock, and
//! `LiveScene` owns the clamp.** A host still decides when its clock is
//! stopped — the first frame, and after a parked loop, both of which start
//! from zero — because that is a fact about the host's own timeline. What it
//! no longer decides is how large a step is too large.
//!
//! # Why the check is a source scan
//!
//! The same reason `clock_invariant.rs` gives, and this file is modelled on
//! it: the invariant is about what the source may *contain*, not about what
//! one execution does. A host that happens not to clamp on the frame a test
//! drives proves nothing about the constant sitting above it. A scan states
//! the rule directly and fails on the commit that breaks it.
//!
//! # Why it lives in `demo/`
//!
//! Because the hosts are what it polices, and `demo/` is a host. A test that
//! lived in `dashlang` would have the owner of the rule policing its own
//! consumers, and neither host crate can see the other.
//!
//! # What it does not catch
//!
//! Recorded because a scan that reads as exhaustive is worse than one whose
//! limit is written down. This matches three exact spellings, so **a host that
//! reintroduces either rule under a different name passes silently.** All
//! three of these were run against this scan and none of them failed it:
//!
//! - a differently-named constant —
//!   `const FRAME_DELTA_CLAMP: Duration = Duration::from_millis(100);`
//! - an inline clamp with no constant at all — `dt.min(Duration::from_millis(100))`
//! - a renamed gate — `presented_generation: Option<u64>`, with its own
//!   comparison and assignment
//!
//! What the scan does catch is a **revert**: the literal spellings both hosts
//! carried before story #810, which is the regression most likely to happen
//! and the one a rebase or a revert reintroduces verbatim. It is a tripwire on
//! the known path, not a proof about every path.
//!
//! Closing the gap properly needs something this scan is the wrong shape for —
//! a check on the hosts' *behaviour* rather than their text, which would have
//! to drive each host's frame loop with a stalled clock and observe the step
//! it took. That is worth doing when a host becomes a library that can be
//! driven from a test, which is what stories #741 and #794 make of them; it is
//! not constructible against a `winit` event loop and a `requestAnimationFrame`
//! callback today. `clock_invariant.rs` discloses its own gap the same way,
//! for the same reason.

use std::fs;
use std::path::{Path, PathBuf};

/// The hosts, as paths relative to the workspace root.
///
/// **Maintain this list by hand**, and add to it when a host appears. Stories
/// #741 and #794 add `crates/dashscene-web` and `crates/dashscene-desktop`,
/// which are these two hosts' integration halves and inherit this rule with
/// the code.
const HOSTS: [&str; 2] = ["demo", "demo-web"];

/// How a host would spell the rules if it reintroduced them.
///
/// `const MAX_FRAME_DELTA` is how both hosts spelled the clamp before story
/// #810, so a host declaring it again is the exact regression this catches.
///
/// The **declaration** is what is forbidden, not the name. A host is expected
/// to *use* `dashlang::MAX_FRAME_DELTA` — `demo/src/shell.rs` prints it in the
/// line that announces the frame loop, and using the owner's value is the
/// behaviour this test wants rather than one it should punish. An earlier
/// version of this scan matched the bare name and failed on exactly that line.
///
/// The generation gate is matched as **host state** rather than as the word
/// `shown`, which is deliberate and was not the first attempt. `shown` alone
/// is also the name R5 gives a document's shown root, and
/// `demo/src/document.rs` uses it in that unrelated sense — a scan for the
/// bare word reported those lines as violations. `self.shown` and a `shown:`
/// field or struct-literal binding are what a host holding its own gate
/// actually writes; a local named `shown` is not.
///
/// Nothing here forbids *mentioning* either rule, because comments are
/// stripped before matching and both hosts are expected to say what they call
/// and why.
/// Each spelling carries its own punctuation — `const `, `self.`, or a
/// trailing colon — so a plain substring match cannot collide with a longer
/// identifier, and `shown_count` or `frames_shown` are not the rule.
const HOST_POLICY_PATTERNS: [&str; 3] = ["const MAX_FRAME_DELTA", "self.shown", "shown:"];

#[test]
fn no_host_states_the_frame_delta_clamp_or_the_generation_gate_for_itself() {
    let workspace = workspace_root();
    let mut offences: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for host in HOSTS {
        let src = workspace.join(host).join("src");
        let sources = rust_sources(&src);
        // A scan that scans nothing passes for free — the same
        // `t2-check-has-no-teeth` failure `clock_invariant.rs` guards against,
        // and a hand-maintained path list is exactly how a check acquires it.
        assert!(
            !sources.is_empty(),
            "no Rust source found under {}: this test's host list has gone stale, and a scan \
             that reads no files reports success without having checked anything",
            src.display()
        );
        for source in sources {
            scanned += 1;
            let text = fs::read_to_string(&source)
                .unwrap_or_else(|error| panic!("reading {}: {error}", source.display()));
            for (index, line) in text.lines().enumerate() {
                for name in host_policy_definitions_in(line) {
                    offences.push(format!(
                        "{}:{}: {name} — {}",
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
        "a host states the frame-delta clamp or the generation gate for itself. Both belong to \
         `dashlang::LiveScene`: pass the raw delta to `tick`, which clamps it, and ask \
         `advanced()` and `mark_shown()` rather than keeping a `shown` of your own. Two hosts \
         with private copies is the duplication story #810 removed, and stories #741 and #794 \
         would publish it. See \
         docs/decisions/frame-delta-is-clamped-and-the-host-owns-the-clock.md\n{}",
        offences.join("\n")
    );
    assert!(scanned > 0, "scanned no files at all");
}

/// Guards the guard.
///
/// A source scan whose matcher is wrong passes on every tree, including the
/// ones it exists to reject, and it does so silently. These are the mutation
/// test, committed: the forms that must be caught, and the legitimate ones
/// that must not be.
#[test]
fn the_scan_catches_a_reintroduced_rule_and_leaves_legitimate_lines_alone() {
    assert_eq!(
        host_policy_definitions_in("const MAX_FRAME_DELTA: Duration = Duration::from_millis(100);"),
        vec!["const MAX_FRAME_DELTA"],
        "the native host's old clamp must be caught"
    );
    assert_eq!(
        host_policy_definitions_in("const MAX_FRAME_DELTA: f64 = 0.1;"),
        vec!["const MAX_FRAME_DELTA"],
        "the web host's old clamp must be caught, in its own unit"
    );
    // The regression that made this pattern a declaration rather than a name.
    // Using the owner's constant is the behaviour this test wants.
    assert!(
        host_policy_definitions_in("            (dashlang::MAX_FRAME_DELTA * 1000.0).round(),")
            .is_empty(),
        "using the owner's constant is the point, not a violation"
    );
    assert!(
        host_policy_definitions_in("use dashlang::{LiveScene, MAX_FRAME_DELTA};").is_empty(),
        "importing the owner's constant is not declaring one"
    );
    assert_eq!(
        host_policy_definitions_in("    shown: Option<u64>,"),
        vec!["shown:"],
        "a host field holding the shown generation must be caught"
    );
    assert_eq!(
        host_policy_definitions_in("        self.shown = Some(generation);"),
        vec!["self.shown"],
        "a host assigning its own shown generation must be caught"
    );
    assert!(
        host_policy_definitions_in("        // `LiveScene` clamps, so MAX_FRAME_DELTA is not ours")
            .is_empty(),
        "prose about the rule is not a violation of it"
    );
    assert!(
        host_policy_definitions_in("        if live.advanced() {").is_empty(),
        "calling the owner's gate is the point, not a violation"
    );
    assert!(
        host_policy_definitions_in("        live.mark_shown();").is_empty(),
        "calling the owner's gate is the point, not a violation"
    );
    assert!(
        host_policy_definitions_in("        let shown_count = self.presents;").is_empty(),
        "a longer identifier that merely contains the word is not the rule"
    );
    // The regression that made this matcher what it is. `demo/src/document.rs`
    // binds a local named `shown` for the *shown root* R5 bounds the load by —
    // an unrelated sense of the word, and the first version of this scan
    // reported both of its lines as violations.
    assert!(
        host_policy_definitions_in("    let shown = dashbuf::prefetch::first_root(&document)")
            .is_empty(),
        "R5's shown root is a different rule with the same word, and is not host policy"
    );
    assert!(
        host_policy_definitions_in(
            "        for index in dashbuf::prefetch::assets_of_root(&document, shown) {"
        )
        .is_empty(),
        "passing that local on is not host policy either"
    );
}

/// The host-policy rules defined or assigned on one line of Rust source.
///
/// Comments are stripped first, because both hosts are expected to *discuss*
/// these rules — a host that calls `LiveScene::advanced` should say what it is
/// calling. The strip is a plain search for `//`, on the same reasoning
/// `clock_invariant.rs` records for its own.
fn host_policy_definitions_in(line: &str) -> Vec<&'static str> {
    let code = match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    };
    HOST_POLICY_PATTERNS
        .into_iter()
        .filter(|pattern| code.contains(pattern))
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
