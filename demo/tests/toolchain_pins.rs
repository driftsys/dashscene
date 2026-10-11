//! **prim has a minimum version, written once, in `bootstrap`.**
//!
//! prim decides how every Markdown file in the tree is wrapped
//! (`docs/decisions/house-style.md`). This repository requires a minimum prim
//! rather than one exact version: `bootstrap` accepts any installed prim at or
//! above `PRIM_MIN_VERSION` and installs the latest release when the installed
//! one is older, and the CI `prim` job installs the latest release and fails if
//! it is older than the same minimum.
//!
//! The minimum must exist in one place only. A second copy in the workflow is
//! the two-copies drift this repository keeps hitting (`registry_consistency.rs`
//! guards the same class for a crate list), so the CI job reads the value out of
//! `bootstrap` instead of carrying its own.

use std::path::{Path, PathBuf};

#[test]
fn bootstrap_declares_a_prim_minimum() {
    let bootstrap = read(workspace_root().join("bootstrap"));
    let min = value_after(&bootstrap, "PRIM_MIN_VERSION=\"${PRIM_MIN_VERSION:-")
        .expect("bootstrap declares PRIM_MIN_VERSION with a default");

    let parts: Vec<&str> = min.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "PRIM_MIN_VERSION is a bare X.Y.Z version, without a leading v: {min}"
    );
}

/// The CI install step takes the minimum from `bootstrap`, verifies the
/// download against the release's checksum, and compares versions.
///
/// Asserted on the text of the one step that installs prim, because the step is
/// shell inside YAML and cannot be called from here. Each needle is a distinct
/// failure if it goes missing: a second copy of the minimum, an unchecked
/// download, or a version that is never compared.
#[test]
fn the_ci_prim_install_reads_the_minimum_from_bootstrap() {
    let workflow = read(workspace_root().join(".github/workflows/ci.yml"));
    let step = section(&workflow, "name: install prim", "\n      - uses:");
    assert!(!step.is_empty(), "ci.yml has a step named `install prim`");

    assert!(
        !step.lines().any(|l| l.trim().starts_with("v=")),
        "the install step carries no version of its own"
    );
    assert!(
        step.contains("PRIM_MIN_VERSION") && step.contains("bootstrap"),
        "the install step reads PRIM_MIN_VERSION from bootstrap"
    );
    assert!(
        step.contains("releases/latest/download/"),
        "the install step downloads the latest release"
    );
    assert!(
        step.contains("sha256sum -c --strict"),
        "the install step verifies the download against its checksum"
    );
    assert!(
        step.contains("sort -V"),
        "the install step compares the installed version with the minimum"
    );
}

/// The first value following `needle`, up to a quote, newline, space or brace.
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
