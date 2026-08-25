//! `scripts/is-code-change`, and the one job that deliberately does not consult
//! it.
//!
//! **The script had no test at all until issue #1361.** Its own header records
//! that the `--no-renames` defect was "found and fixed" by running it against a
//! scratch repository by hand, which is exactly the shape that does not stay
//! found: the next person to edit the `case` statement has nothing to run.
//!
//! Three things are pinned here, and they are different questions.
//!
//! **What the script classifies**, over a real repository rather than a mocked
//! diff, because the classification is a `git diff` away from the answer.
//!
//! **That the `test` job takes no condition at all** — no `if:`, no `needs:` —
//! which is issue #1361's fix. Asserting the absence of one expression would
//! not do: `if: false`, or a condition on a differently named output, re-gates
//! the job and leaves that expression absent.
//!
//! **That the tests which read files under `docs/` are the two this reasoning
//! names, and that the `test` job's own profile runs them.** The fix rests on
//! both, because ungating one job is sufficient only while that job runs every
//! such test. It runs `--workspace`, so it does — three of the eleven jobs
//! still gated also run tests, but each selects a subset this one has already
//! run. If a third record-reading test appears in a target only a gated job
//! runs, or if either of these two leaves the default profile, the fail-open
//! returns with nothing else noticing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root. `demo` is one level down.
///
/// Named as the five sibling test binaries in this directory name it, so a
/// grep for `workspace_root` while auditing how these meta-tests locate the
/// repository returns all six.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("demo sits one level below the repository root")
        .to_path_buf()
}

/// A scratch directory that removes itself however the test ends.
///
/// `Drop` rather than a call before each assertion: the likely panic is inside
/// the helpers — a `git` that fails, or the script exiting non-zero — and a
/// cleanup line after the assertion never runs for any of them.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        // **Kept when the test is failing.** A verdict that came out wrong is
        // exactly when the repository that produced it is worth looking at, and
        // the path is in the panic message. Removed on every other path,
        // including a panic inside the helpers, which a cleanup line placed
        // after the assertion would miss.
        if std::thread::panicking() {
            eprintln!("[ci_gating] kept for inspection: {}", self.0.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Scratch {
    /// Named from the process id and a counter rather than a random source: two
    /// cases in one process cannot collide, and no crate is needed for it.
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dashscene-is-code-change-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating the scratch repository");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

/// Runs `git` in `dir` and requires it to succeed.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        // **The developer's own git config is switched off, not worked
        // around.** Passing `-c user.name` and `-c user.email` pins identity
        // and nothing else: a global `commit.gpgsign = true` fails every commit
        // here with `No secret key`, and `core.hooksPath`, `init.templateDir`
        // and `core.autocrlf` reach in the same way. Pointing both config
        // levels at an empty file is what makes this fixture the same on every
        // machine.
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("running git {args:?}: {error}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().expect("a file has a parent"))
        .unwrap_or_else(|error| panic!("creating {}: {error}", path.display()));
    std::fs::write(&path, contents)
        .unwrap_or_else(|error| panic!("writing {}: {error}", path.display()));
}

/// A repository with one file of each kind the classification distinguishes.
fn scratch_repo() -> Scratch {
    let scratch = Scratch::new();
    let dir = scratch.path();
    git(dir, &["init", "-q", "-b", "main"]);
    // **Rename detection on, explicitly.** The rename case below measures
    // `--no-renames`, and a developer with `diff.renames=false` in their global
    // config would make that case pass against a script with the flag deleted —
    // measured, not reasoned about. Setting it here is what makes the case
    // about the script rather than about the machine.
    git(dir, &["config", "diff.renames", "true"]);
    write(dir, "README.md", "root markdown\n");
    write(dir, "docs/note.md", "a doc\n");
    write(dir, "docs/deep/nested/note.md", "a deeper doc\n");
    write(dir, "docs/capture.txt", "not markdown\n");
    write(dir, "crates/thing/README.md", "crate markdown\n");
    write(dir, "crates/thing/src/lib.rs", "// code\n");
    write(dir, "justfile", "default:\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "base"]);
    scratch
}

/// Applies `change` to a fresh repository and returns the script's verdict.
fn classify(change: impl FnOnce(&Path)) -> String {
    let scratch = scratch_repo();
    let dir = scratch.path();
    change(dir);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "change"]);
    run_script(dir, "HEAD~1...HEAD")
}

/// The script, run against `range` inside `dir`.
fn run_script(dir: &Path, range: &str) -> String {
    let script = workspace_root().join("scripts/is-code-change");
    assert!(
        script.is_file(),
        "{} is missing; this test is the only thing that runs it outside CI",
        script.display()
    );
    let out = Command::new(&script)
        .current_dir(dir)
        .arg(range)
        .output()
        .unwrap_or_else(|error| panic!("running {}: {error}", script.display()));
    assert!(
        out.status.success(),
        "the script exited {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn markdown_under_docs_is_documentation() {
    assert_eq!(classify(|d| write(d, "docs/note.md", "edited\n")), "false");
}

#[test]
fn markdown_under_docs_at_any_depth_is_documentation() {
    // `*` matches `/` in a `case` pattern, which is why one arm covers every
    // depth. A rewrite using something that does not would fail here.
    assert_eq!(
        classify(|d| write(d, "docs/deep/nested/note.md", "edited\n")),
        "false"
    );
}

#[test]
fn a_file_under_docs_that_is_not_markdown_is_code() {
    // **The only case that discriminates `docs/*.md` from `docs/*`.** Widening
    // that arm leaves every other case in this file identical, which was
    // measured. It is a live distinction: `docs/archive/` carries tracked
    // `.log`, `.txt` and `.pbtx` files today.
    assert_eq!(
        classify(|d| write(d, "docs/capture.txt", "edited\n")),
        "true"
    );
}

#[test]
fn markdown_at_the_repository_root_is_documentation() {
    assert_eq!(classify(|d| write(d, "README.md", "edited\n")), "false");
}

#[test]
fn a_rust_file_is_code() {
    assert_eq!(
        classify(|d| write(d, "crates/thing/src/lib.rs", "// edited\n")),
        "true"
    );
}

#[test]
fn markdown_under_crates_is_code() {
    // The script's own reasoning: a crate's Markdown can reach a doctest
    // through `include_str!`. No crate does that today.
    assert_eq!(
        classify(|d| write(d, "crates/thing/README.md", "edited\n")),
        "true"
    );
}

#[test]
fn a_file_at_the_root_that_is_not_markdown_is_code() {
    assert_eq!(
        classify(|d| write(d, "justfile", "default:\n  @echo\n")),
        "true"
    );
}

#[test]
fn documentation_beside_code_is_code() {
    assert_eq!(
        classify(|d| {
            write(d, "docs/note.md", "edited\n");
            write(d, "crates/thing/src/lib.rs", "// edited\n");
        }),
        "true"
    );
}

#[test]
fn a_deleted_source_file_is_code() {
    assert_eq!(
        classify(|d| std::fs::remove_file(d.join("crates/thing/src/lib.rs")).expect("removing")),
        "true"
    );
}

#[test]
fn a_rust_file_renamed_under_docs_is_code() {
    // **The defect `--no-renames` exists for.** `git diff --name-only` reports
    // only a rename's destination, so without the flag this lists
    // `docs/moved.md` alone and the gate skips the compile half over a source
    // file that is gone.
    let scratch = scratch_repo();
    let dir = scratch.path();
    git(dir, &["mv", "crates/thing/src/lib.rs", "docs/moved.md"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "rename"]);

    // The verdict, and the reason for it. Asserting only the verdict would pass
    // if the diff listed some other code file.
    let listed = Command::new("git")
        .current_dir(dir)
        .args(["diff", "--name-only", "--no-renames", "HEAD~1...HEAD"])
        .output()
        .expect("listing the diff");
    let listed = String::from_utf8_lossy(&listed.stdout);
    assert!(
        listed.contains("crates/thing/src/lib.rs") && listed.contains("docs/moved.md"),
        "with --no-renames the diff must list both paths, and lists: {listed}"
    );
    assert_eq!(run_script(dir, "HEAD~1...HEAD"), "true");
}

#[test]
fn the_range_is_taken_from_the_merge_base() {
    // Three dots, so a branch behind its base does not read the base's own
    // commits as its changes. Every other case here uses `HEAD~1...HEAD`, where
    // the merge base IS `HEAD~1` and two dots would agree — so this is the only
    // case where the dots discriminate.
    let scratch = scratch_repo();
    let dir = scratch.path();
    git(dir, &["checkout", "-qb", "branch"]);
    write(dir, "docs/note.md", "branch edits a doc\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "doc only"]);

    git(dir, &["checkout", "-q", "main"]);
    write(dir, "crates/thing/src/lib.rs", "// main moves on\n");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "code on main"]);

    // Three dots: only the branch's own commit, which is documentation.
    assert_eq!(run_script(dir, "main...branch"), "false");
    // Two dots would additionally report main's code commit as a change.
    let two_dots = Command::new("git")
        .current_dir(dir)
        .args(["diff", "--name-only", "main..branch"])
        .output()
        .expect("listing the two-dot diff");
    assert!(
        String::from_utf8_lossy(&two_dots.stdout).contains("crates/thing/src/lib.rs"),
        "this case only discriminates if two dots would see main's code commit"
    );
}

#[test]
fn an_empty_diff_is_code() {
    // **A second fail-closed arm, and a different one.** The script reaches
    // `true` from two states: `git diff` failed — the case below — and `git
    // diff` succeeded and listed nothing. Deleting the `[ -z "$changed" ]`
    // branch leaves the loop body unreached and prints `false`, which only this
    // case sees.
    let scratch = scratch_repo();
    assert_eq!(run_script(scratch.path(), "HEAD...HEAD"), "true");
}

#[test]
fn an_unreadable_range_is_code() {
    // Fails closed: a diff that cannot be read is not evidence that nothing
    // changed. `git diff` fails on the unknown revision, the script swallows it
    // and sees an empty list.
    let scratch = scratch_repo();
    assert_eq!(run_script(scratch.path(), "no-such-ref...HEAD"), "true");
}

#[test]
fn the_test_job_takes_no_condition_at_all() {
    // Issue #1361's fix, and the only thing that holds it.
    let text = workflow();
    let body = job_body(&text, "test");

    for key in ["if:", "needs:"] {
        let found: Vec<_> = body
            .lines()
            .filter(|line| line.trim_start().starts_with(key) && indent(line) == 4)
            .collect();
        assert!(
            found.is_empty(),
            "the `test` job declares `{key}` again: {found:?}. It must take no condition at \
             all — asserting the absence of one expression would pass over `if: false` or a \
             condition on a differently named output. Two tests parse files under `docs/`, so \
             any documentation-only condition here makes a docs diff able to take the suite \
             red with CI green. Issue #1361."
        );
    }

    // **Not vacuous**: the condition must still exist elsewhere, or a rename of
    // the detector's output would make the assertion above pass for free. On
    // `if:` lines specifically — a comment quoting the expression is not a
    // condition, and the `ci` job's comment block quotes it today.
    for name in ["clippy", "demo-build", "wasm-build", "android-build"] {
        let gated = job_body(&text, name).lines().any(|line| {
            line.trim_start().starts_with("if:") && line.contains("needs.changes.outputs.code")
        });
        assert!(
            gated,
            "the `{name}` job carries no `if:` naming the documentation-only detector, so the \
             assertion above proves nothing. Either the output was renamed, or the skip was \
             removed everywhere and this test needs rewriting."
        );
    }
}

#[test]
fn the_tests_that_read_a_record_are_the_two_this_reasoning_names() {
    // **The premise the fix rests on.** Ungating `test` closes the fail-open
    // only while every test that reads a file under `docs/` is one the `test`
    // job runs. A third one landing in a target that only a GATED job runs —
    // `atlas-repro`, `render-oracle` and `exit-gate-tests` all run tests and
    // all still skip — would restore it silently.
    let known = [
        "unity/package-gate/tests/plugin_meta.rs",
        "goldens/tooling/tests/worked_example.rs",
    ];
    let mut found = Vec::new();
    collect_doc_readers(&workspace_root(), &mut found);
    found.sort();
    found.dedup();

    let mut expected: Vec<String> = known.iter().map(|p| (*p).to_string()).collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the set of test sources naming a file under `docs/` that exists has changed. Every \
         one of them must be run by the `test` job, which runs the whole workspace; a new one \
         reachable only from a gated job reopens issue #1361. Add it here once you have \
         checked which job runs it."
    );

    // And neither is filtered out of the profile the `test` job runs.
    let nextest = std::fs::read_to_string(workspace_root().join(".config/nextest.toml"))
        .expect("reading .config/nextest.toml");
    let default = section(&nextest, "[profile.default]");
    for name in [
        "the_transcribed_rows_are_d3s_table",
        "the_guide_names_this_file",
    ] {
        assert!(
            !default.contains(name),
            "`{name}` is named in the default profile's filter, so the `test` job may not run \
             it — and that job is the only one issue #1361 left ungated."
        );
    }
}

/// Every `.rs` under `root` naming a `docs/…​.md` path that exists.
///
/// **Resolution is what makes this precise.** Two files mention a `docs/` path
/// in a message rather than opening one — a probe-table label and a diagnostic
/// string — and neither resolves to a file, so neither is collected. This
/// file's own fixtures use paths that exist in a scratch repository and not
/// here, for the same reason.
fn collect_doc_readers(root: &Path, out: &mut Vec<String>) {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if name == "target" || name == ".git" || name == "node_modules" {
                    continue;
                }
                walk(base, &path, out);
            } else if name.ends_with(".rs") {
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(_) => continue,
                };
                for literal in doc_literals(&text) {
                    let relative = literal.replace("../", "");
                    if base.join(&relative).is_file() {
                        let shown = path
                            .strip_prefix(base)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        out.push(shown);
                    }
                }
            }
        }
    }
    walk(root, root, out);
}

/// Quoted `docs/…​.md` paths in a Rust source, leading `../` included.
fn doc_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(offset) = text[i..].find('"') {
        let start = i + offset + 1;
        let Some(len) = text[start..].find('"') else {
            break;
        };
        let literal = &text[start..start + len];
        let trimmed = literal.trim_start_matches("../");
        if trimmed.starts_with("docs/") && literal.ends_with(".md") && !literal.contains('\\') {
            out.push(literal.to_string());
        }
        i = start + len + 1;
        if i >= bytes.len() {
            break;
        }
    }
    out
}

fn workflow() -> String {
    let path = workspace_root().join(".github/workflows/ci.yml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The `[section]` of a TOML file, up to the next one.
fn section<'a>(text: &'a str, header: &str) -> &'a str {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("no {header} in .config/nextest.toml"));
    let rest = &text[start + header.len()..];
    let end = rest.find("\n[").map_or(rest.len(), |offset| offset);
    &rest[..end]
}

/// The lines of one job in the workflow, from its `  <name>:` line to the next
/// key at that indent.
///
/// **Anchored after `jobs:`**, because the file has indent-two keys under `on:`,
/// `permissions:` and `concurrency:` — `push`, `pull_request`, `merge_group`,
/// `contents`, `group` among them. A bare search for `\n  test:\n` would find
/// one of those first if a job ever shared its name, return a few lines of an
/// unrelated block, and let an assertion about an absence pass over it.
///
/// Textual rather than parsed: this crate takes no YAML dependency, and the
/// caller checks the slice it gets back against an anchor unique to that job,
/// so a wrong slice fails loudly rather than reporting an absence.
fn job_body<'a>(text: &'a str, name: &str) -> &'a str {
    let jobs = text
        .find("\njobs:\n")
        .expect("ci.yml declares no `jobs:` mapping");
    let header = format!("\n  {name}:\n");
    let start = text[jobs..]
        .find(&header)
        .unwrap_or_else(|| panic!("ci.yml declares no `{name}` job, so this test reads nothing"))
        + jobs
        + 1;
    let rest = &text[start + header.len() - 1..];

    // The next line at indent two that is a key rather than content: block
    // scalars under `run: |` are indented further, and a `- ` list item is not
    // a key.
    let end = rest
        .match_indices('\n')
        .find(|(offset, _)| {
            let line = rest[offset + 1..].split('\n').next().unwrap_or_default();
            indent(line) == 2
                && line.trim_start().split(':').next().is_some_and(|key| {
                    !key.is_empty() && !key.contains(' ') && !key.starts_with('-')
                })
                && line.trim_end().ends_with(':')
        })
        .map_or(rest.len(), |(offset, _)| offset);
    &rest[..end]
}

#[test]
fn job_body_returns_the_job_it_was_asked_for() {
    // `job_body` is the parser two assertions above rest on, and a parser that
    // returned the wrong slice would make an assertion about an absence pass
    // for the wrong reason. Each anchor appears in exactly one job.
    let text = workflow();
    for (name, anchor) in [
        ("test", "shared-key: workspace-tests"),
        ("clippy", "just doc-links"),
        ("android-build", "just android-lint"),
    ] {
        assert!(
            job_body(&text, name).contains(anchor),
            "job_body({name}) does not contain `{anchor}`, so it returned some other part of \
             the file and every assertion resting on it means nothing."
        );
    }
}
