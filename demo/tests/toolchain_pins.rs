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
//! `bootstrap` through `scripts/prim-min-version` instead of carrying its own.
//!
//! The comparison must exist in one place too. `bootstrap` and the CI job both
//! call `scripts/prim-version-at-least`, so they cannot disagree about a
//! version, and the tests below execute that script rather than read it. What
//! the tests cannot execute — the download and install steps, which need the
//! network — is asserted on the text of the lines that do it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `scripts/<name>` under bash with `args`.
fn run(name: &str, args: &[&str]) -> Output {
    let script = workspace_root().join("scripts").join(name);
    assert!(script.is_file(), "{} is missing", script.display());
    Command::new("bash")
        .arg(&script)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("running {}: {e}", script.display()))
}

/// The exit code of `scripts/prim-version-at-least have min`.
fn at_least(have: &str, min: &str) -> Option<i32> {
    run("prim-version-at-least", &[have, min]).status.code()
}

#[test]
fn a_version_equal_to_the_minimum_is_accepted() {
    assert_eq!(at_least("0.9.1", "0.9.1"), Some(0));
}

#[test]
fn a_greater_version_is_accepted() {
    assert_eq!(at_least("0.9.2", "0.9.1"), Some(0));
    assert_eq!(at_least("1.0.0", "0.9.1"), Some(0));
}

#[test]
fn a_lower_version_is_refused() {
    assert_eq!(at_least("0.9.0", "0.9.1"), Some(1));
    assert_eq!(at_least("0.2.3", "0.9.1"), Some(1));
}

/// A lexical comparison gets both of these wrong.
#[test]
fn fields_compare_as_numbers_not_as_text() {
    assert_eq!(at_least("0.10.0", "0.9.1"), Some(0));
    assert_eq!(at_least("0.9.2", "0.9.10"), Some(1));
}

#[test]
fn a_leading_v_is_ignored_on_either_side() {
    assert_eq!(at_least("v0.9.1", "0.9.1"), Some(0));
    assert_eq!(at_least("0.9.1", "v0.9.1"), Some(0));
    assert_eq!(at_least("v0.9.0", "v0.9.1"), Some(1));
}

/// A suffix is stripped before the comparison, so it neither aborts the script
/// nor changes the answer.
#[test]
fn a_pre_release_or_build_suffix_is_ignored() {
    assert_eq!(at_least("0.9.2-beta", "0.9.1"), Some(0));
    assert_eq!(at_least("0.9.1-rc1", "0.9.1"), Some(0));
    assert_eq!(at_least("0.9.1+abc", "0.9.1"), Some(0));
    assert_eq!(at_least("0.9.0-rc1", "0.9.1"), Some(1));
}

/// Empty or malformed input is an error, never an acceptance, so a `prim
/// --version` whose output changed shape fails closed in both callers.
#[test]
fn empty_or_malformed_input_fails() {
    for (have, min) in [
        ("", "0.9.1"),
        ("0.9.1", ""),
        ("unknown", "0.9.1"),
        ("0.9.x", "0.9.1"),
    ] {
        let code = at_least(have, min);
        assert!(
            code.is_some() && code != Some(0) && code != Some(1),
            "prim-version-at-least {have:?} {min:?} exited {code:?}, \
             not the usage error 2"
        );
    }
}

/// The reader prints what `bootstrap` declares, read from the file that is
/// passed to it.
#[test]
fn the_minimum_reader_prints_the_declared_minimum() {
    let scratch = Scratch::new();
    let copy = scratch.0.join("bootstrap");
    std::fs::write(
        &copy,
        "set -u\nPRIM_MIN_VERSION=\"${PRIM_MIN_VERSION:-1.22.333}\"\n",
    )
    .expect("writing a scratch bootstrap");

    let out = run("prim-min-version", &[copy.to_str().expect("utf-8 path")]);
    assert!(out.status.success(), "the reader failed: {out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1.22.333");
}

/// With no argument it reads this repository's `bootstrap`, and agrees with the
/// declaration found by text.
#[test]
fn the_minimum_reader_reads_this_repositorys_bootstrap() {
    let bootstrap = read(workspace_root().join("bootstrap"));
    let declared = value_after(&bootstrap, "PRIM_MIN_VERSION=\"${PRIM_MIN_VERSION:-")
        .expect("bootstrap declares PRIM_MIN_VERSION with a default");

    let out = run("prim-min-version", &[]);
    assert!(out.status.success(), "the reader failed: {out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), declared);

    let parts: Vec<&str> = declared.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "PRIM_MIN_VERSION is a bare X.Y.Z version, without a leading v: {declared}"
    );
}

#[test]
fn the_minimum_reader_fails_when_the_declaration_is_missing() {
    let scratch = Scratch::new();
    let copy = scratch.0.join("bootstrap");
    std::fs::write(&copy, "set -u\nJQ_VERSION=\"${JQ_VERSION:-1.8.2}\"\n")
        .expect("writing a scratch bootstrap");

    let out = run("prim-min-version", &[copy.to_str().expect("utf-8 path")]);
    assert!(!out.status.success(), "the reader succeeded: {out:?}");
    assert!(out.stdout.is_empty(), "the reader printed a value: {out:?}");
}

/// The CI install step, asserted line by line because it is shell inside YAML
/// and cannot be called from here.
#[test]
fn the_ci_prim_install_uses_the_shared_scripts() {
    let workflow = read(workspace_root().join(".github/workflows/ci.yml"));
    let step = section(&workflow, "name: install prim", "\n      - uses:");
    assert!(!step.is_empty(), "ci.yml has a step named `install prim`");
    let lines: Vec<&str> = step.lines().map(str::trim).collect();

    assert!(
        lines.contains(&r#"min="$(scripts/prim-min-version)""#),
        "the install step reads the minimum through scripts/prim-min-version"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("v=") || l.starts_with("min=0") || l.contains("sort -V")),
        "the install step carries no version or comparison of its own"
    );
    let check = lines
        .iter()
        .position(|l| l.starts_with("scripts/prim-version-at-least ") && l.contains("|| {"))
        .expect("the install step checks the version with scripts/prim-version-at-least");
    let install = lines
        .iter()
        .position(|l| l.starts_with("install -m 0755"))
        .expect("the install step installs prim");
    assert!(
        check < install,
        "the version is checked before prim is installed"
    );

    let verify: Vec<&&str> = lines.iter().filter(|l| l.contains("sha256sum")).collect();
    assert_eq!(
        verify,
        [&r#"echo "${sha}  prim.tgz" | sha256sum -c --strict -"#],
        "the download is verified by one strict sha256sum line that nothing can mask"
    );
    assert!(
        !step.contains("latest/download/"),
        "the tarball and its checksum come from one resolved tag, not two `latest` redirects"
    );
    assert!(
        step.contains("releases/download/${tag}/"),
        "the install step downloads from the resolved tag"
    );
}

/// The same for `install_prim` in `bootstrap`, whose install path needs the
/// network and a writable install directory.
#[test]
fn bootstrap_install_prim_uses_the_shared_script() {
    let bootstrap = read(workspace_root().join("bootstrap"));
    let body = section(&bootstrap, "\ninstall_prim() {", "\n}\n");
    assert!(!body.is_empty(), "bootstrap defines install_prim");
    let lines: Vec<&str> = body.lines().map(str::trim).collect();

    assert!(
        !bootstrap.contains("version_at_least()"),
        "bootstrap carries no comparison of its own"
    );
    let checks: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("\"${PRIM_AT_LEAST}\""))
        .map(|(i, _)| i)
        .collect();
    let candidate = lines
        .iter()
        .position(|l| l.contains(r#""${tmp}/prim" --version"#))
        .expect("install_prim reads the version of the downloaded prim");
    let install = lines
        .iter()
        .position(|l| l.starts_with("install -m 0755"))
        .expect("install_prim installs prim");
    assert!(
        checks.iter().any(|&c| c > candidate && c < install),
        "install_prim checks the downloaded prim before it replaces the installed one"
    );
    assert!(
        !body.contains("latest/download/"),
        "the tarball and its checksum come from one resolved tag, not two `latest` redirects"
    );
    assert!(
        body.contains("releases/download/${tag}/"),
        "install_prim downloads from the resolved tag"
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

/// A scratch directory removed on drop. Named from the process id and a
/// counter, as `ci_gating.rs` does, so no crate is needed for it.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dashscene-toolchain-pins-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating a scratch directory");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
