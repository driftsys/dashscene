//! **A pinned tool version is written in two files, and both must say the same
//! thing.**
//!
//! prim decides how every Markdown file in the tree is wrapped, so it is pinned
//! rather than tracked (`docs/decisions/house-style.md`, and the `audit` job in
//! `.github/workflows/ci.yml` for the case that must not be). The pin lives in
//! two places by necessity: `bootstrap` installs it on a developer's machine,
//! and the CI `prim` job installs it on a runner, and neither can read the
//! other.
//!
//! If only one moves, CI formats the tree with one version while every fresh
//! clone installs another — a CI-only red on a diff that changed nothing, which
//! is the exact failure the pin exists to prevent. `registry_consistency.rs`
//! exists for this same two-copies-drift class; this is the same idea applied to
//! a tool version rather than a crate list.
//!
//! The two are spelled differently on purpose and that is half the risk:
//! `bootstrap` carries the tag whole (`v0.2.4`), CI carries the bare version
//! (`0.2.4`) because it builds the URL as `v${v}`. This test compares them after
//! normalising the leading `v`, so the shapes may differ and the versions may
//! not.

use std::path::{Path, PathBuf};

#[test]
fn bootstrap_and_ci_pin_the_same_prim() {
    let root = workspace_root();
    let bootstrap = read(root.join("bootstrap"));
    let workflow = read(root.join(".github/workflows/ci.yml"));

    let from_bootstrap = value_after(&bootstrap, "PRIM_VERSION=\"${PRIM_VERSION:-")
        .expect("bootstrap declares PRIM_VERSION with a default");
    let from_ci = value_after(&workflow, "v=").expect("the prim job declares v=<version>");

    assert_eq!(
        from_bootstrap.trim_start_matches('v'),
        from_ci.trim_start_matches('v'),
        "bootstrap pins prim {from_bootstrap} and the CI prim job pins {from_ci}; \
         they install the formatter that decides this tree's Markdown, so they must agree"
    );
}

/// The checksum the CI job verifies belongs to the version it pins.
///
/// Not a second copy of the version — a third thing that must move with it. A
/// bump that edits `v=` and leaves the `sha=` behind fails the install with a
/// checksum mismatch rather than a version message, so this names the coupling
/// where a reader will look for it. Asserted structurally: both are present, on
/// their own lines, in the one step that installs prim.
#[test]
fn the_ci_prim_install_carries_a_version_and_a_checksum() {
    let workflow = read(workspace_root().join(".github/workflows/ci.yml"));
    let step = section(&workflow, "name: install prim", "\n      - uses:");

    assert!(
        step.lines().any(|l| l.trim().starts_with("v=")),
        "the install step pins a version"
    );
    let sha = step
        .lines()
        .find_map(|l| l.trim().strip_prefix("sha="))
        .expect("the install step carries a checksum");
    assert_eq!(
        sha.len(),
        64,
        "a sha256 is 64 hex characters, and this one is {}: {sha}",
        sha.len()
    );
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "the checksum is hex: {sha}"
    );
}

/// The first single-quoted or bare value following `needle`, to end of line.
fn value_after(text: &str, needle: &str) -> Option<String> {
    let at = text.find(needle)?;
    let rest = &text[at + needle.len()..];
    let end = rest.find(['"', '\n', ' ', '}']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// The text between `open` and the next `close`, or to the end if absent.
fn section<'a>(text: &'a str, open: &str, close: &str) -> &'a str {
    let Some(start) = text.find(open) else {
        return "";
    };
    let rest = &text[start..];
    match rest.find(close) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("demo/ has a parent, which is the workspace root")
        .to_path_buf()
}
