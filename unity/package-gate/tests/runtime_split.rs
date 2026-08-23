//! R-E10's split, and the two ways it could be widened into meaninglessness.
//!
//! `docs/specification/07-embedding-and-distribution.md` R-E10 requires every
//! C# type under `Runtime/` to compile against `netstandard.dll` 2.1.0, and
//! `unity/package-compat` is one of its two checks. That project has no Unity
//! reference assemblies, so a type referencing `UnityEngine` fails there
//! whatever its API compatibility level actually is — issue #1286 — and
//! `docs/decisions/r-e10-is-checked-in-two-halves.md` answers it by excluding
//! `Runtime/Engine/` from that project and covering it with a Unity editor
//! instead.
//!
//! **An exclusion is a hole, and these tests are its edges.** #1286 named the
//! cheap-looking repair itself: excluding the file that fails, which silently
//! narrows R-E10 to whatever is left. Two things have to hold for the split to
//! mean anything — the exclusion covers exactly the engine directory, and
//! everything inside that directory genuinely needs to be there.
//!
//! **The other direction is the compiler's**, and is deliberately not a test
//! here. A file outside the engine directory that references `UnityEngine`
//! fails `unity/package-compat` with `CS0246`, which is detection this crate
//! cannot improve on: a draft of that test searched the source text and
//! reported three files whose only mention of `UnityEngine` was a comment
//! saying they contain none. `just unity-abi` prints where such a file belongs
//! when the build fails, which is where a developer meets it.

/// **Every** project that globs `Runtime/` excludes the engine directory, and
/// nothing wider.
///
/// This is the test #1286's "cheap-looking repair" fails: a developer meeting
/// `CS0246` on a new engine-referencing file can make a project green by
/// widening its glob, and R-E10 then covers whatever is left with nothing
/// saying so.
///
/// **Stated over the class rather than over one project, and that is not
/// theoretical.** Story #1122 fixed `unity/package-compat` and left
/// `unity/ffi-check`, which carries the same glob for a different reason — its
/// question is whether the P/Invoke declarations match the library, and it
/// compiles the whole of `Runtime/` only because that is where they live. The
/// first version of this test named `package-compat`, passed, and `just
/// unity-ffi` went red for what read as an unrelated reason.
#[test]
fn every_project_globbing_runtime_excludes_the_engine_directory_and_nothing_wider() {
    const GLOB: &str = "../com.driftsys.dashscene/Runtime/**/*.cs";
    const EXPECTED: &str = "Exclude=\"../com.driftsys.dashscene/Runtime/Engine/**/*.cs\"";

    let projects = package_gate::csproj_files();
    assert!(
        !projects.is_empty(),
        "no .csproj under unity/. This test would then hold every project to \
         the exclusion, having found none."
    );

    let mut globbing = Vec::new();
    for (path, source) in &projects {
        if !source.contains(GLOB) {
            continue;
        }
        globbing.push(path.clone());

        assert!(
            source.contains(EXPECTED),
            "{path} globs `{GLOB}` and does not carry exactly `{EXPECTED}`. \
             That project has no Unity reference assemblies, so every file \
             under Runtime/Engine/ fails there with CS0246 — issue #1286. \
             Every project with this glob needs the same exclusion, for its \
             own reason."
        );

        // One exclusion, so the assertion above is about the whole hole rather
        // than about one of several.
        assert_eq!(
            source.matches("Exclude=").count(),
            1,
            "{path} carries more than one Exclude. The assertion above checks \
             one of them, so a second would be unexamined."
        );

        // **`Remove` is the same hole spelled differently.** `<Compile Remove=
        // "…FramePacker.cs" />` drops a file from the compile set while every
        // assertion above still passes: the `Exclude` is untouched and still
        // unique, and the item group is still non-empty. R-E10 would then cover
        // `Runtime/` minus whatever was quietly removed.
        assert!(
            !source.contains("Remove="),
            "{path} carries a `Remove=`, which drops files from the compile set \
             without touching the `Exclude` this test checks. R-E10 is stated \
             over `Runtime/` minus the engine directory and nothing else."
        );
    }

    assert!(
        !globbing.is_empty(),
        "no project under unity/ globs `{GLOB}`. R-E10's netstandard half is \
         stated over such a project, so either one was removed or the glob was \
         rewritten — either way this test is now checking nothing."
    );

    // **The netstandard project itself must be in the class.** Every assertion
    // above is over "projects carrying this glob", and respelling
    // `package-compat`'s include — `Runtime/*.cs`, non-recursive — would drop
    // it out of that class entirely while `ffi-check` kept the set non-empty.
    // R-E10's own named check would then silently stop covering any
    // subdirectory of `Runtime/`.
    assert!(
        globbing
            .iter()
            .any(|p| p.ends_with("package-compat/PackageCompat.csproj")),
        "unity/package-compat/PackageCompat.csproj does not carry \
         `{GLOB}`. That project is the one R-E10 names, so a respelled or \
         narrowed include there is the requirement quietly losing scope. \
         Projects that do carry it: {globbing:?}"
    );

    // Printed rather than asserted as a count: a count is a census that goes
    // stale when a project is added, which is the drift a fixed number invites.
    println!("projects globbing Runtime/: {globbing:?}");
}

/// Every file in the engine directory genuinely references the engine.
///
/// The exclusion's other edge. A file that compiles under netstandard2.1
/// belongs on the checked side, and moving it here would drop it out of the
/// half that runs on every pull request into the half that needs an editor
/// nobody has in CI.
#[test]
fn every_file_in_the_engine_directory_references_the_engine() {
    let engine: Vec<_> = package_gate::package_cs_files()
        .into_iter()
        .filter(|(path, _)| path.starts_with(package_gate::ENGINE_DIR))
        .collect();

    for (path, source) in &engine {
        let referenced = package_gate::ENGINE_TOKENS
            .iter()
            .find(|token| source.contains(**token));
        assert!(
            referenced.is_some(),
            "{path} is under Runtime/Engine/ and references none of \
             {:?}, so it would compile under netstandard2.1. Move it up into \
             Runtime/, where `unity/package-compat` checks it on every pull \
             request — the engine directory is checked only by an editor, \
             which no CI runner here has.",
            package_gate::ENGINE_TOKENS
        );
    }

    // **What this cannot see**: a file that mentions an engine token only in
    // a comment passes. That is a laxity rather than a false alarm — it lets
    // an engine-free file sit here undetected — and closing it would mean
    // stripping C# comments and string literals, a parse of the language this
    // gate deliberately does not attempt. It stops the accident (a file put
    // here because the netstandard build failed for some other reason), not a
    // deliberate evasion.
    //
    // Reported rather than asserted: an EMPTY engine directory is the
    // strongest state this split can be in, because `package-compat` then
    // covers the whole package. It is worth printing so a reader of a green
    // run knows which case they are looking at.
    println!(
        "engine-referencing files under {}: {}",
        package_gate::ENGINE_DIR,
        engine.len()
    );
}
