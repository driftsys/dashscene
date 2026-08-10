//! The registry invariant: **every crate is in every machine-readable list
//! that enumerates crates** (story #795).
//!
//! Four lists, and the qualifier is load-bearing. Issue #445 named **seven**
//! registries, and four of those are prose — `crate-name-map.md`, `AGENTS.md`,
//! `architecture.md`, `glossary.md`. This suite does not read them: a crate
//! absent from a prose table is a documentation defect rather than a build or
//! publish failure, and matching prose reliably is not what a substring scan
//! does. The gap is stated here rather than left to be inferred from what the
//! code happens to check.
//!
//! Adding a workspace member means updating several files that each hold their
//! own copy of the crate list. Getting that wrong has happened twice:
//!
//! - **Issue #445** — `dashpack-astcenc-sys` landed touching only
//!   `Cargo.toml`, and seven other registries were found afterwards.
//! - **Story #795** — `#445` was closed as completed with one of its own items
//!   unfixed. `dashpack` and `dashpack-astcenc-sys` were still absent from
//!   `.git-std.toml`'s `[[version_files]]`, so `git std bump` would not have
//!   moved either, which is precisely the consequence #445 wrote down.
//!
//! A third recurrence is what this exists to stop. The failure is always the
//! same shape — a crate exists and a list does not know it — so the check is
//! one assertion applied to each of the four.
//!
//! # It derives the crate list rather than restating it
//!
//! `[workspace] members` is the source of truth, because Cargo will not build
//! without it: a crate missing *there* is not a registry drift, it is a crate
//! that does not exist. Every other list is checked against it.
//!
//! That is deliberate and it is the whole design. A test carrying its own list
//! of seventeen names would be the seventh registry, and it would drift exactly
//! as the other six can.
//!
//! # What each registry is worth, measured by mutating it
//!
//! Every registry below was emptied of one crate in turn and this suite re-run.
//! Four of the five failures are this suite's; the fifth is not, and saying so
//! is more useful than counting it as a catch:
//!
//! - `[[version_files]]`, `scopes` and the `publish` recipe — **caught here**,
//!   and by nothing else. Cargo does not read any of them.
//! - the publish **order** — caught here, and by nothing else until a real
//!   publish fails.
//! - `[workspace.dependencies]` — **Cargo refuses the workspace outright**
//!   ("`dependency.X` was not found in `workspace.dependencies`") for any crate
//!   something inherits. So this suite is not what protects that entry for most
//!   crates. It still protects the leaves — **`dashscene` and
//!   `dashscene-unity`**, the two nothing in the workspace depends on, so
//!   removing their entries would build cleanly and break only at publish. An
//!   earlier draft said four and added `dashpack` and `dashscene-desktop`;
//!   `goldens/tooling` takes the first and `demo` the second.
//!
//! # Why it lives in `demo/`
//!
//! The same reason `clock_invariant.rs`, `host_policy_invariant.rs` and
//! `integration_surface.rs` do: it is a check about repository files rather
//! than about a crate's behaviour, and `demo` is a host-target member that
//! `cargo test --workspace` reaches. Putting it inside a published crate would
//! ship a test about the repository to consumers.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The publishable crates, from `[workspace] members`.
///
/// Members under `crates/` only. `demo`, `demo-web`, `corpus/showcase`,
/// `goldens/tooling` and `measure/web-minimal` are `publish = false` by design,
/// so none belongs in the
/// version, dependency or publish registries — though `demo`, `corpus` and
/// `goldens` are `.git-std.toml` commit *scopes*, a different list serving a
/// different purpose and not checked here.
fn crates_from_members(workspace: &Path) -> BTreeSet<String> {
    let manifest = read(workspace.join("Cargo.toml"));
    let members = section(&manifest, "members = [", "]");
    members
        .lines()
        .filter_map(|line| line.trim().strip_prefix("\"crates/"))
        .filter_map(|line| line.split('"').next())
        .map(str::to_owned)
        .collect()
}

/// The directories under `crates/`, which must be exactly the members.
///
/// This is the one check that does not read a list at all, and it is what
/// catches the case the others cannot: a crate directory that was never added
/// to `members`, where every other registry would be consistent with a
/// workspace that has no such crate.
#[test]
fn every_directory_under_crates_is_a_workspace_member() {
    let workspace = workspace_root();
    let members = crates_from_members(&workspace);

    let mut on_disk = BTreeSet::new();
    for entry in fs::read_dir(workspace.join("crates")).expect("crates/ exists") {
        let path = entry.expect("a readable entry").path();
        if path.join("Cargo.toml").is_file() {
            on_disk.insert(
                path.file_name()
                    .expect("a named directory")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }

    assert_eq!(
        on_disk, members,
        "the directories under crates/ and [workspace] members disagree. A crate on disk and \
         not in members is invisible to Cargo; a member with no directory does not build."
    );
    assert!(
        !members.is_empty(),
        "a scan that reads nothing proves nothing"
    );
}

#[test]
fn every_crate_is_in_every_registry_that_enumerates_crates() {
    let workspace = workspace_root();
    let members = crates_from_members(&workspace);
    assert!(
        !members.is_empty(),
        "a scan that reads nothing proves nothing"
    );

    let root = read(workspace.join("Cargo.toml"));
    let git_std = read(workspace.join(".git-std.toml"));
    let justfile = read(workspace.join("justfile"));

    // Each registry, with the exact text that proves a crate is in it. The
    // text is the *entry* rather than the bare name, because the root
    // `Cargo.toml` names all 17 in its own publish-order commentary and a scan
    // for the name would read that as coverage.
    let mut missing: Vec<String> = Vec::new();
    missing.extend(absent_from(
        &root,
        &members,
        "[workspace.dependencies] in Cargo.toml",
        |name| format!("\n{name} = {{ path = \"crates/{name}\""),
    ));
    missing.extend(absent_from(
        &git_std,
        &members,
        ".git-std.toml scopes",
        |name| format!("\n    \"{name}\","),
    ));
    // Anchored on the crate name, because that is how the entries are written
    // and why: git-std splices one span per entry, so an unanchored entry would
    // move one requirement and leave the rest.
    missing.extend(absent_from(
        &git_std,
        &members,
        ".git-std.toml [[version_files]]",
        // Line-anchored, like the other three. Without the newline a
        // **commented-out** entry — `# regex = 'dashbuf = ...` — satisfied it,
        // so a crate could be dropped from the bump while this stayed green.
        |name| format!("\nregex = '{name} = "),
    ));
    missing.extend(absent_from(
        &justfile,
        &members,
        "the justfile publish recipe",
        |name| format!("cargo publish -p {name}\n"),
    ));

    assert!(
        missing.is_empty(),
        "a crate exists and a registry does not know it. That is the shape of issue #445, and \
         of the `[[version_files]]` gap story #795 found still open after #445 was closed as \
         completed — a crate absent from `[[version_files]]` is one `git std bump` will not \
         move, so it drifts out of the shared version flow at the first bump.\n{}",
        missing.join("\n")
    );
}

/// Every publishable crate carries the metadata a registry page is built from.
///
/// `description`, `license` and `repository` are inherited or present already
/// and Cargo refuses a publish without them. `keywords` and `categories` are
/// neither inherited nor required, so nothing but this notices when a new crate
/// arrives without them — which is how all **seventeen** crates came to have
/// neither until story #795.
///
/// **Category slugs are not checked, and cannot be here.** crates.io holds the
/// list, it moves, and a slug that does not exist is rejected at publish rather
/// than by anything local.
///
/// **Keyword rules are checked**, because unlike the slug list they are fixed
/// and published: at most five, at most twenty characters, `[A-Za-z0-9_-]`, and
/// an alphanumeric first character. crates.io rejects a violation at publish,
/// which is the worst moment to find out.
///
/// A non-empty value is required rather than the key merely being present:
/// `keywords = []` parses, reads as covered, and is no metadata at all.
#[test]
fn every_publishable_crate_carries_registry_metadata() {
    let workspace = workspace_root();
    let members = crates_from_members(&workspace);
    let mut missing: Vec<String> = Vec::new();

    assert!(
        !members.is_empty(),
        "a scan that reads nothing proves nothing"
    );

    for name in &members {
        let manifest = read(workspace.join("crates").join(name).join("Cargo.toml"));
        for field in ["keywords", "categories"] {
            // `= ["` and not `= [`: an empty array satisfies the second, and an
            // empty array is no metadata at all.
            if !manifest
                .lines()
                .any(|line| line.starts_with(&format!("{field} = [\"")))
            {
                missing.push(format!("{name} has no {field}"));
            }
        }
        for keyword in keywords_of(&manifest) {
            if keyword.len() > 20 {
                missing.push(format!("{name}: keyword {keyword:?} is over 20 characters"));
            }
            if !keyword.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                missing.push(format!(
                    "{name}: keyword {keyword:?} must start alphanumeric"
                ));
            }
            if !keyword
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                missing.push(format!(
                    "{name}: keyword {keyword:?} has an illegal character"
                ));
            }
        }
        let count = keywords_of(&manifest).count();
        if count > 5 {
            missing.push(format!(
                "{name} carries {count} keywords, and crates.io allows five"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "a crate is missing the metadata its registry page is built from. Neither field is \
         inherited and neither blocks a publish, so nothing else reports it.\n{}",
        missing.join("\n")
    );
}

/// The publish recipe is in dependency order.
///
/// Story #741 made `dashscene-web` depend on `dashscene-gpu`, and the recipe
/// had published the web crate first — correct while it was an empty
/// placeholder and wrong the moment it was not. That story moved it in the same
/// commit, so `main` never carried the inverted order. What `main` did carry was
/// a new dependency edge that made a previously-correct order wrong, with
/// nothing to notice had the move been forgotten: `cargo build` resolves through
/// `path` and never consults the order, so only a real publish would fail.
#[test]
fn the_publish_recipe_is_in_dependency_order() {
    let workspace = workspace_root();
    let members = crates_from_members(&workspace);
    let justfile = read(workspace.join("justfile"));

    let order: Vec<&str> = justfile
        .lines()
        .filter_map(|line| line.trim().strip_prefix("cargo publish -p "))
        .collect();
    let named: BTreeSet<String> = order.iter().map(|name| (*name).to_owned()).collect();
    assert_eq!(
        named, members,
        "the publish recipe and [workspace] members disagree"
    );
    assert_eq!(
        order.len(),
        members.len(),
        "the publish recipe names a crate twice"
    );

    let position = |name: &str| {
        order
            .iter()
            .position(|entry| *entry == name)
            .expect("every member is in the recipe, asserted above")
    };

    let mut wrong: Vec<String> = Vec::new();
    for name in &members {
        let manifest = read(workspace.join("crates").join(name).join("Cargo.toml"));
        for dependency in members.iter().filter(|other| *other != name) {
            let taken = manifest.lines().any(|line| takes(line, dependency));
            if taken && position(dependency) > position(name) {
                wrong.push(format!(
                    "{name} is published at position {} and depends on {dependency}, which is at {}",
                    position(name) + 1,
                    position(dependency) + 1
                ));
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "the publish recipe would publish a crate before something it depends on, which \
         crates.io refuses. Re-derive the order over every crate rather than moving the one \
         that changed.\n{}",
        wrong.join("\n")
    );
}

/// Guards the guard.
///
/// A registry check whose matcher is wrong passes on every tree. These are the
/// mutation test, committed: each registry's entry text must match the real
/// file for a crate that is in it, and must not match a name that merely
/// appears in prose.
#[test]
fn the_matchers_find_real_entries_and_not_prose() {
    let workspace = workspace_root();
    let root = read(workspace.join("Cargo.toml"));
    let git_std = read(workspace.join(".git-std.toml"));
    let justfile = read(workspace.join("justfile"));

    assert!(
        root.contains("\ndashbuf = { path = \"crates/dashbuf\""),
        "the workspace-dependency matcher must find a real entry"
    );
    assert!(
        !root.contains("\ndashfake = { path = \"crates/dashfake\""),
        "a crate that does not exist must not be found"
    );
    assert!(
        git_std.contains("\nregex = 'dashbuf = "),
        "the version-files matcher must find an anchored entry"
    );
    assert!(
        !git_std.contains("\nregex = 'dashfake = "),
        "an absent crate must not be found"
    );
    assert!(
        justfile.contains("cargo publish -p dashbuf\n"),
        "the publish matcher must find a real line"
    );
    assert!(
        !justfile.contains("cargo publish -p dashfake\n"),
        "an absent crate must not be found"
    );
    // The scopes matcher takes the quoted list entry, not the bare word, because
    // every crate name also appears in that file's own commentary.
    assert!(git_std.contains("\n    \"dashbuf\","));
    assert!(!git_std.contains("\n    \"dashfake\","));
}

/// The keywords a manifest declares, as written.
fn keywords_of(manifest: &str) -> impl Iterator<Item = &str> {
    manifest
        .lines()
        .find(|line| line.starts_with("keywords = ["))
        .unwrap_or("")
        .split('"')
        // A quoted TOML array alternates separator, value, separator, value —
        // so the values are the odd indices.
        .skip(1)
        .step_by(2)
}

/// One manifest line takes `dependency` from the workspace.
///
/// **Two spellings, and missing the second is what this exists for.** Most
/// crates write `dashbuf.workspace = true`, but a dependency that also needs
/// features or `optional` must use the table form — `dashbuf = { workspace =
/// true }` in `crates/dashpack/Cargo.toml`, and `dashpack = { workspace = true,
/// features = ["preview"], optional = true }` in `goldens/tooling`. Matching the
/// dotted form only left the order check blind to **both** of `dashpack`'s
/// internal dependencies, which are the exact crates issues #445 and #795 are
/// about. Found by review, not by this suite.
///
/// Anchored on the line start so `dashscene-core` does not read as `dashscene`.
fn takes(line: &str, dependency: &str) -> bool {
    let line = line.trim_start();
    line.starts_with(&format!("{dependency}.workspace"))
        || (line.starts_with(&format!("{dependency} = {{")) && line.contains("workspace = true"))
}

/// Every member whose entry is absent from `text`.
fn absent_from(
    text: &str,
    members: &BTreeSet<String>,
    registry: &str,
    entry: impl Fn(&str) -> String,
) -> Vec<String> {
    members
        .iter()
        .filter(|name| !text.contains(&entry(name)))
        .map(|name| format!("{name} is missing from {registry}"))
        .collect()
}

/// The text between `open` and the next `close`, or the whole file if absent.
fn section<'a>(text: &'a str, open: &str, close: &str) -> &'a str {
    let Some(start) = text.find(open) else {
        return text;
    };
    let rest = &text[start + open.len()..];
    match rest.find(close) {
        Some(end) => &rest[..end],
        None => rest,
    }
}

fn read(path: PathBuf) -> String {
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// The workspace root: `demo/`'s parent.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("demo/ has a parent, which is the workspace root")
        .to_path_buf()
}

/// Every publishable crate ships `LICENSE` and `NOTICE` inside its package.
///
/// Cargo packages only files under a crate's own directory, so a root-level
/// `LICENSE` and `NOTICE` reach no `.crate` at all. Under MIT that was
/// cosmetic. Under Apache-2.0 it is not: §4(a) requires giving recipients a
/// copy of the licence, and §4(d) requires carrying the attribution notices —
/// which for this workspace means Arm's, for the vendored astcenc sources.
///
/// The copies are byte-identical to the root files by construction;
/// `just licenses` regenerates them, and this test is what makes a drifted or
/// missing copy fail a build rather than ship.
#[test]
fn every_publishable_crate_packages_the_licence_and_notice() {
    let workspace = workspace_root();
    let licence = read(workspace.join("LICENSE"));
    let notice = read(workspace.join("NOTICE"));

    let mut missing = Vec::new();
    let mut drifted = Vec::new();
    for name in crates_from_members(&workspace) {
        let dir = workspace.join("crates").join(&name);
        if read(dir.join("Cargo.toml")).contains("publish = false") {
            continue;
        }
        for (file, want) in [("LICENSE", &licence), ("NOTICE", &notice)] {
            let path = dir.join(file);
            match fs::read_to_string(&path) {
                Err(_) => missing.push(format!("crates/{name}/{file}")),
                Ok(got) if got != **want => drifted.push(format!("crates/{name}/{file}")),
                Ok(_) => {}
            }
        }
    }

    assert!(
        missing.is_empty() && drifted.is_empty(),
        "run `just licenses`.\n  missing: {missing:?}\n  drifted from the root copy: {drifted:?}"
    );
}
