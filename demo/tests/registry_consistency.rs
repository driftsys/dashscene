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
//! # Four checks that are not about crates (issues #1167, #1175, story #1125)
//!
//! Four tests in this file are not registry checks and the paragraphs above do
//! not describe them. **They are named rather than located**, because an
//! earlier version of this paragraph pointed at "the two tests at the end" and
//! story #1125 then added two more after them:
//!
//! - `the_android_linker_variable_is_named_only_by_the_android_env_recipe`
//! - `every_listed_recipe_describes_itself_in_a_sentence`
//! - `the_unity_package_version_is_bumped_and_matches_the_workspace`
//! - `the_unity_package_meta_files_are_all_or_nothing`
//!
//! The first two are here because this file already reads and parses the
//! justfile in three places, and issue #1167 asked whether a justfile assertion
//! should overload a test named for the demo registry or take a binary of its
//! own: the answer taken was to overload it. The last two are story #1125's and
//! carry their own note where they sit.
//!
//! **That threshold has now been passed**, and moving them out into a file
//! named for what they check rather than for the registry is the obvious next
//! step. It is not taken here because this pull request's subject is the Unity
//! package's deployment model, not this file's organisation.
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
//!   `dashpaint-abi`**, the two nothing in the workspace depends on, so
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
    missing.extend(absent_from_list(&git_std, &members, ".git-std.toml scopes"));
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
            // Emptiness rather than the `= ["` spelling: an empty array is no
            // metadata at all, and `array_values` reports it as none either way.
            if array_values(&manifest, field).is_empty() {
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
        let count = keywords_of(&manifest).len();
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
    // every crate name also appears in that file's own commentary. Trimmed, so
    // the indentation stays the formatter's business — see `absent_from_list`.
    assert!(git_std.lines().any(|l| l.trim() == "\"dashbuf\","));
    assert!(!git_std.lines().any(|l| l.trim() == "\"dashfake\","));
}

/// The keywords a manifest declares.
fn keywords_of(manifest: &str) -> Vec<&str> {
    array_values(manifest, "keywords")
}

/// The string values of a top-level array field, inline or expanded.
///
/// **Layout-independent on purpose, and this is the second matcher in this file
/// to need it.** prim formats every manifest and expands an inline array as soon
/// as its line passes 80 columns — measured: a five-keyword line at 87 columns
/// becomes one value per line. Reading only the inline spelling failed twice
/// over. The presence check above wanted `keywords = ["`, which an expanded
/// array does not start with, so a crate with five valid keywords reported as
/// having none. And this function matched the bare `keywords = [`, whose line
/// carries no quotes at all, so it yielded nothing and every rule below it —
/// length, charset, the five-keyword ceiling — stopped checking while the test
/// stayed green. The longest `keywords` line in the tree is 69 columns, so that
/// was eleven columns away.
///
/// Takes the text from the opening bracket to the first `]`, which is one span
/// in both layouts, and reads the odd indices of its `"` split: a quoted TOML
/// array alternates separator, value, separator, value. An empty array yields
/// no values, which is what the presence check reads as absent metadata.
fn array_values<'a>(manifest: &'a str, field: &str) -> Vec<&'a str> {
    let open = format!("{field} = [");
    let mut from = 0;
    while let Some(rel) = manifest[from..].find(&open) {
        let at = from + rel;
        // Line-anchored, so a key ending in `field` cannot match.
        if at == 0 || manifest.as_bytes()[at - 1] == b'\n' {
            let rest = &manifest[at + open.len()..];
            let end = rest.find(']').unwrap_or(rest.len());
            return rest[..end].split('"').skip(1).step_by(2).collect();
        }
        from = at + open.len();
    }
    Vec::new()
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

/// Members whose quoted list entry appears on no line of `text`.
///
/// Line-oriented, where the other registries are matched by substring, because
/// this one is the only entry whose indentation a formatter decides.
/// `.editorconfig` asks for two spaces in a `.toml`; `.git-std.toml` was written
/// with four and nothing enforced the difference until prim replaced dprint,
/// which formats TOML. The matcher spelled the width out and failed on the
/// reindent, reporting every crate as missing from a list that still held all
/// of them. Trimming makes the width the formatter's business and leaves this
/// test checking what it means to check.
///
/// Still the quoted entry rather than the bare name: every crate name also
/// appears in that file's own commentary, which is what the width was doing
/// half of the work of excluding.
fn absent_from_list(text: &str, members: &BTreeSet<String>, registry: &str) -> Vec<String> {
    members
        .iter()
        .filter(|name| {
            let entry = format!("\"{name}\",");
            !text.lines().any(|line| line.trim() == entry)
        })
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
/// **This asserts against `cargo package --list`, not against the directory.**
/// Checking the files exist on disk would pass while an `include` key in a
/// manifest quietly excluded them from the package, which is the obligation
/// the test exists to enforce. `just licenses` regenerates the copies.
#[test]
fn every_publishable_crate_packages_the_licence_and_notice() {
    let workspace = workspace_root();
    let members = crates_from_members(&workspace);
    assert!(
        !members.is_empty(),
        "a scan that reads nothing proves nothing: `members` parsed empty"
    );

    let mut missing = Vec::new();
    for name in members {
        let dir = workspace.join("crates").join(&name);
        if read(dir.join("Cargo.toml")).contains("publish = false") {
            continue;
        }
        let listed = std::process::Command::new(env!("CARGO"))
            .args(["package", "--list", "--allow-dirty", "-p", &name])
            .current_dir(&workspace)
            .output()
            .expect("cargo package --list");
        let listed = String::from_utf8_lossy(&listed.stdout);
        for file in ["LICENSE", "NOTICE"] {
            if !listed.lines().any(|l| l.trim() == file) {
                missing.push(format!("{name}: {file} not in `cargo package --list`"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "run `just licenses`.\n  {}",
        missing.join("\n  ")
    );
}

/// The NDK cross-compiling wiring is written in exactly one recipe (issue
/// #1167).
///
/// PR #1162 consolidated three inlined copies into `_android-env` (issue
/// #1101). **Nothing failed if a fourth appeared**, and the third arrived
/// exactly that way: PR #1098 added `android-lint` with its own inlined copy,
/// one screen below a header arguing that a duplicated package list "is the
/// drift issue #903 keeps producing".
///
/// The check needs neither a device nor an NDK, which is why it can live in the
/// sanity tier: the linker variable's *name* is what a copy has to spell, so
/// the assertion is over the justfile's text.
///
/// Here rather than in a file of its own for the reason issue #1167 leaves
/// open: this file already reads and parses the justfile in three other tests,
/// and a second test binary costs a link per run for one assertion.
#[test]
fn the_android_linker_variable_is_named_only_by_the_android_env_recipe() {
    const VARIABLE: &str = "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER";
    let justfile = read(workspace_root().join("justfile"));
    let exempt = recipe_lines(&justfile, "_android-env");

    // Guards the matcher itself: a rename that this test did not follow would
    // otherwise make the assertion below vacuously true, and a `_android-env`
    // that stopped being found would exempt nothing rather than everything.
    assert!(
        justfile.contains(VARIABLE),
        "{VARIABLE} is named nowhere in the justfile — if the wiring was renamed, \
         rename it here too rather than deleting this test"
    );
    assert!(
        !exempt.is_empty(),
        "the `_android-env` recipe was not found, so this test would report every \
         mention of {VARIABLE} as a stray"
    );

    let stray: Vec<usize> = justfile
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(VARIABLE))
        .map(|(i, _)| i + 1)
        .filter(|line| !exempt.contains(line))
        .collect();

    assert!(
        stray.is_empty(),
        "{VARIABLE} is named outside the `_android-env` recipe, at justfile \
         line(s) {stray:?}. The NDK wiring is written once and consumed by \
         assigning `just _android-env`'s output and then eval-ing the variable — \
         never `eval \"$(just _android-env)\"`, which swallows a missing NDK, and \
         never a second copy, which is the drift issues #1101 and #903 were each \
         filed for."
    );
}

/// The 1-based line numbers a recipe's header and body occupy.
///
/// A `just` recipe's body is its header plus the indented lines under it: an
/// unindented line ends it, **including a comment**, which is the next recipe's
/// documentation. That is stricter than "not a comment" and is the difference
/// that matters here — `_android-env` is followed immediately by `_android-sdk`'s
/// header block, and treating comments as part of the body would exempt twenty
/// lines this test exists to police. Blank lines inside a body are kept, since
/// `just` allows them.
///
/// Empty if there is no such recipe, which the caller asserts on rather than
/// reading as "nothing to exempt".
fn recipe_lines(justfile: &str, recipe: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    let mut inside = false;
    for (i, text) in justfile.lines().enumerate() {
        let header =
            text.starts_with(recipe) && text[recipe.len()..].starts_with([' ', ':']) && !inside;
        if header {
            inside = true;
        } else if inside && !text.is_empty() && !text.starts_with([' ', '\t']) {
            inside = false;
        }
        if inside {
            lines.insert(i + 1);
        }
    }
    lines
}

/// Every recipe `just --list` shows describes itself in a sentence, not in the
/// tail of one (issue #1175).
///
/// `just` takes a recipe's description from the **last comment line** above it.
/// This justfile writes long explanatory headers, so for most recipes that line
/// was the tail of a wrapped sentence: `test # a tier and each silently skip the
/// same three tests.`, `lint # check.`, `package # most of the value.` — and
/// `just --list` is the discovery surface, being what `just` prints with no
/// arguments.
///
/// # The predicate, and the half of it that is not obvious
///
/// **Shape**: a description starts with a capital and ends with a full stop.
/// That catches most wrapped tails, because a sentence broken across a line
/// break resumes in lower case.
///
/// **Continuation**: it does not catch all of them, because a tail can resume
/// on a capitalised token. `deno-capture`'s did — `FIGMA_TOKEN
/// (docs/…/figma-access-plan-and-pat-policy.md). Never commit the token.` passes
/// the shape test and is still the back half of "Needs FIGMA_TOKEN …". Issue
/// #1175's own `awk` command misses it for the same reason, which is why this
/// counted **23** fragments on `main` where that command counted 22.
///
/// So the description is also refused when the prose line above it does not end
/// a sentence. An **indented** comment line does not count as prose: several
/// recipes end their block with an example block (`reprobe`, `render`), and a
/// command line is not a wrapped sentence.
///
/// It is not a test of whether a summary is *good*, and does not pretend to be.
#[test]
fn every_listed_recipe_describes_itself_in_a_sentence() {
    let justfile = read(workspace_root().join("justfile"));
    let lines: Vec<&str> = justfile.lines().collect();

    let mut fragments = Vec::new();
    let (mut listed, mut described) = (0usize, 0usize);
    for (i, line) in lines.iter().enumerate() {
        let Some(name) = listed_recipe_name(line) else {
            continue;
        };
        listed += 1;

        // The description is the last comment line above the header, skipping
        // any `[attribute]` lines between the two — `just` reads through those,
        // and this file already uses seven of them.
        let mut j = i;
        while j > 0 && lines[j - 1].starts_with('[') {
            j -= 1;
        }
        let Some(text) = j.checked_sub(1).and_then(|k| lines[k].strip_prefix("# ")) else {
            // No comment block at all: `just --list` shows the recipe with no
            // description, so there is no fragment to find.
            continue;
        };
        described += 1;
        let text = text.trim();

        let shaped = text.starts_with(|c: char| c.is_ascii_uppercase()) && text.ends_with('.');
        // The line above the description, when it is prose rather than an
        // indented example.
        let continues = j
            .checked_sub(2)
            .and_then(|k| lines[k].strip_prefix("# "))
            .filter(|above| !above.starts_with(' '))
            .is_some_and(|above| !above.trim_end().ends_with(['.', ':', ';', '!', '?']));

        if !shaped {
            fragments.push(format!("{name} -> {text}"));
        } else if continues {
            fragments.push(format!("{name} -> (continues the line above) {text}"));
        }
    }

    // Two guards, because there are two matchers and either can stop matching.
    // Counting only the headers would let every description silently go
    // unread — spell them `#Text` instead of `# Text` and `strip_prefix`
    // returns `None` for all of them — while the test still reported no
    // fragments.
    assert!(
        listed > 40,
        "only {listed} listed recipes were found in the justfile, which means this \
         test's header matcher stopped matching rather than that the file shrank"
    );
    assert!(
        described * 10 >= listed * 9,
        "only {described} of {listed} listed recipes had a description this test \
         could read, which means its comment matcher stopped matching rather than \
         that the descriptions were deleted"
    );
    assert!(
        fragments.is_empty(),
        "these recipes' `just --list` descriptions are a sentence fragment rather \
         than a summary — write a one-line summary as the LAST comment line above \
         the recipe, with the explanation above it (issue #1175):\n  {}",
        fragments.join("\n  ")
    );
}

/// The name `just --list` shows for `line`, if it shows one.
///
/// A header is unindented, is not a comment, an assignment or a setting, and
/// names the recipe before its parameters or its colon. Two details `just`
/// decides and this has to follow: a name starting with `_` is **hidden** from
/// the listing, and a leading `@` — which only silences the echo — is not part
/// of the name and does not hide anything.
fn listed_recipe_name(line: &str) -> Option<&str> {
    let body = line.strip_prefix('@').unwrap_or(line);
    if line.starts_with([' ', '\t', '#']) || !line.contains(':') || line.contains(":=") {
        return None;
    }
    let name = body.split([' ', ':']).next()?;
    let listed = !name.is_empty()
        && !name.starts_with('_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    listed.then_some(name)
}

// Two checks over the Unity package, added by story #1125. Neither is a
// registry check and the module header does not describe them; they are here
// because this file already reads `.git-std.toml` and the workspace root, and
// because the failure they catch is the same shape as the registry one — a
// declaration that exists and a list that does not know it.

/// The Unity package's version is moved by `git std bump`, and matches the
/// workspace.
///
/// `the-package-and-its-library-are-one-versioned-artifact.md` D1 rules that
/// the package version tracks the Cargo workspace, and the whole mechanism is
/// one `[[version_files]]` entry. That entry is **outside** every assertion in
/// this file's registry tests, which iterate `[workspace] members` and anchor
/// on `\nregex = '{name} = ` — the package is not a crate and its entry is
/// spelled differently. Deleting it leaves every tier green while `git std
/// bump` silently stops moving `package.json`, which is exactly the failure
/// story #795 exists to have caught once already.
#[test]
fn the_unity_package_version_is_bumped_and_matches_the_workspace() {
    let workspace = workspace_root();
    let git_std = fs::read_to_string(workspace.join(".git-std.toml")).expect(".git-std.toml");
    assert!(
        git_std.contains("path = \"unity/com.driftsys.dashscene/package.json\""),
        ".git-std.toml has no [[version_files]] entry for the Unity package, so `git std bump` \
         will not move it and its version will drift from the workspace's."
    );

    let manifest = fs::read_to_string(workspace.join("Cargo.toml")).expect("Cargo.toml");
    let workspace_version = manifest
        .split_once("\n[workspace.package]")
        .and_then(|(_, rest)| rest.split_once("version = \""))
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(v, _)| v.to_owned())
        .expect("[workspace.package] version");

    let package = fs::read_to_string(workspace.join("unity/com.driftsys.dashscene/package.json"))
        .expect("the Unity package manifest");
    let package_version = package
        .split_once("\"version\": \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(v, _)| v.to_owned())
        .expect("a version in package.json");

    assert_eq!(
        package_version, workspace_version,
        "the Unity package version and the Cargo workspace version disagree, which D1 of \
         the-package-and-its-library-are-one-versioned-artifact.md says they never do."
    );
}

/// If the Unity package ships any `.meta` file, it ships one for everything
/// Unity imports.
///
/// `07-embedding-and-distribution.md` R-E2. A Git-URL package lands in
/// `Library/PackageCache` and is **immutable**, where Unity does not generate a
/// missing `.meta` — it ignores the asset, so a file without one is not
/// imported at all.
///
/// **Deliberately conditional.** The package ships zero `.meta` files today, so
/// an unconditional form could not land green; this catches the state that
/// actually arrives — story #1121 adding them by hand and missing one, which
/// leaves the package installing and delivering nothing with every tier green.
/// It becomes non-vacuous the moment the first one is committed.
#[test]
fn the_unity_package_meta_files_are_all_or_nothing() {
    let workspace = workspace_root();
    const PACKAGE: &str = "unity/com.driftsys.dashscene";

    // **Tracked files, not the working tree.** R-E2 says a *committed* `.meta`,
    // and a stray untracked one left by a local editor would otherwise flip
    // this test into demanding full coverage of a state nobody pushed.
    let listing = std::process::Command::new("git")
        .args(["ls-files", "-z", PACKAGE])
        .current_dir(&workspace)
        .output()
        .expect("git ls-files runs");
    assert!(listing.status.success(), "git ls-files failed");
    let prefix = format!("{PACKAGE}/");
    let tracked: Vec<String> = String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| p.trim_start_matches(&prefix).to_owned())
        .collect();

    // The four shapes Unity's importer hides. Each applies to one kind of entry
    // — `cvs` names a folder, `.tmp` an extension — so applying either to the
    // wrong kind would exempt a path R-E2 requires a `.meta` for.
    fn hidden(relative: &str) -> bool {
        let mut components: Vec<&str> = relative.split('/').collect();
        let file = components.pop().unwrap_or_default();
        let dir_hidden = components
            .iter()
            .any(|c| c.starts_with('.') || c.ends_with('~') || c.eq_ignore_ascii_case("cvs"));
        dir_hidden || file.starts_with('.') || file.ends_with('~') || file.ends_with(".tmp")
    }

    let mut imported = BTreeSet::new();
    let mut metas = BTreeSet::new();
    for relative in &tracked {
        if hidden(relative) {
            continue;
        }
        if let Some(subject) = relative.strip_suffix(".meta") {
            metas.insert(subject.to_owned());
        } else {
            imported.insert(relative.clone());
            // Unity imports folders too, and each needs its own `.meta`.
            let mut parts: Vec<&str> = relative.split('/').collect();
            parts.pop();
            while !parts.is_empty() {
                imported.insert(parts.join("/"));
                parts.pop();
            }
        }
    }

    assert!(
        !imported.is_empty(),
        "a scan that reads nothing proves nothing"
    );
    if metas.is_empty() {
        return; // R-E2 is unmet and known to be; nothing to compare.
    }

    let missing: Vec<_> = imported.difference(&metas).cloned().collect();
    let orphaned: Vec<_> = metas.difference(&imported).cloned().collect();
    assert!(
        missing.is_empty() && orphaned.is_empty(),
        "the Unity package ships .meta files but not for everything Unity imports, so the \
         entries without one are ignored in an immutable package (R-E2).\n  no .meta: \
         {missing:?}\n  .meta with no subject: {orphaned:?}"
    );
}
