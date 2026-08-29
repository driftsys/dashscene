//! R-E7, R-E8 and R-E9's recorded status, held against the tree it describes.
//!
//! Those three bind the project that produces the **shipping** artifact, and
//! this repository contains no committed Unity project — so
//! `docs/specification/07-embedding-and-distribution.md` records them as
//! unchecked. What a status may not do is assert an absence the tree refutes,
//! and until this test it did: it said "nothing reads `PlayerSettings` for this
//! requirement" while `unity/android-probe/AndroidProbeBuild.cs` set each of
//! the three and read it back, and `just unity-android` asserted R-E8's
//! equality on the APK it built. A false absence is worse than a missing status
//! marker, because it invites a second implementation of a check that is
//! already written.
//!
//! **Both directions, and that is the point.** The status names the file that
//! reads `PlayerSettings`; that file must still contain the read. A status
//! naming a file that stopped doing the thing is the same defect pointing the
//! other way, and it is the direction a prose-only fix leaves open.
//!
//! **What this cannot say is whether the status is the right one.** It holds
//! the prose to the presence of the code it names, not to a judgement about
//! whether a set-then-read-back amounts to a check — that judgement is issue
//! #1353's, and the paragraph naming the issue is what this test pins instead.

use std::fs;

const SPEC: &str = "docs/specification/07-embedding-and-distribution.md";
const PROBE: &str = "unity/android-probe/AndroidProbeBuild.cs";

/// Each requirement, and the read-back STATEMENT the probe takes for it.
///
/// **The whole statement, not the property name.** A first version named the
/// expression alone, which works for R-E7 — `SetScriptingBackend` writes and
/// `GetScriptingBackend` reads — and is vacuous for the other two, whose write
/// and read are the same text: deleting both read-backs and keeping the two
/// assignments left the test green. Reviewed on 2026-08-29.
const READS: [(&str, &str); 3] = [
    (
        "R-E7",
        "var backend = PlayerSettings.GetScriptingBackend(NamedBuildTarget.Android);",
    ),
    (
        "R-E8",
        "var architectures = PlayerSettings.Android.targetArchitectures;",
    ),
    (
        "R-E9",
        "var minSdk = (int)PlayerSettings.Android.minSdkVersion;",
    ),
];

/// The method the three read-backs live in, and the one call that runs it.
///
/// Pinning the reads without pinning the call leaves them reachable from
/// nothing: deleting `SetAndroidPlayerSettings(failures);` from `Build` keeps
/// every read in the file and stops all three from ever executing, while the
/// specification goes on describing them.
const READER: &str = "SetAndroidPlayerSettings";

/// The paragraph a requirement opens, up to the blank line that ends it.
fn paragraph(spec: &str, marker: &str) -> String {
    let opener = format!("**{marker}** —");
    let start = spec.find(&opener).unwrap_or_else(|| {
        panic!(
            "{SPEC} has no paragraph opening `{opener}`. Every requirement in \
             that file opens one, so either the marker was renamed or the file \
             was restructured — and this test then checks nothing."
        )
    });
    let rest = &spec[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    rest[..end].to_string()
}

fn read(path: &str) -> String {
    let full = package_gate::root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// The three statuses name the file that reads `PlayerSettings`, and it reads.
#[test]
fn every_host_project_requirement_names_the_file_that_reads_player_settings() {
    let spec = read(SPEC);
    // **A read is a claim about code.** The probe's own header discusses
    // `PlayerSettings` at length in order to deny that it checks these three,
    // so searching the raw text would find that denial and call it a read.
    let probe = package_gate::cs_scan::blank_comments_and_strings(&read(PROBE));

    // The reads are pinned inside the method that takes them, and that method
    // is pinned to being called.
    let (from, to) = package_gate::cs_scan::member_body(
        &probe,
        &format!("private static void {READER}(List<string> failures)"),
    );
    let reader_body = &probe[from..to];
    assert!(
        probe.contains(&format!("{READER}(failures);")),
        "{PROBE} never calls `{READER}`, so the three read-backs {SPEC} \
         describes are reachable from nothing and execute on no run."
    );

    for (marker, expression) in READS {
        assert!(
            reader_body.contains(expression),
            "{PROBE}'s `{READER}` does not contain `{expression}` outside \
             its comments, and {SPEC}'s {marker} names that file as what reads \
             `PlayerSettings` for the requirement. Either the read moved, in \
             which case the status has to name where it moved to, or it was \
             deleted and the status is now an overclaim."
        );

        let para = paragraph(&spec, marker);
        assert!(
            para.contains(PROBE),
            "{SPEC}'s {marker} does not name `{PROBE}`, which sets and reads \
             back the value this requirement is about. A status that does not \
             say what examines the requirement leaves a reader to conclude \
             nothing does — which is the false absence this test exists for.\n\
             {para}"
        );

        // The sentence this test was written against. It is asserted as an
        // absence, so the naming assertion above is what carries the weight;
        // this catches the specific regression of putting it back.
        assert!(
            !para.contains("nothing reads `PlayerSettings`"),
            "{SPEC}'s {marker} says nothing reads `PlayerSettings` for it, and \
             `{PROBE}` reads `{expression}`. The requirement may still be \
             unchecked — a check that writes the value it then reads cannot \
             fail — but that is a different sentence from this one.\n{para}"
        );

        assert!(
            // `**Unchecked` rather than `**Unchecked**`: R-E8's status opens
            // `**Unchecked as stated, and asserted on one artifact.**`, and
            // what must not happen is the word flipping to `Checked`.
            para.contains("**Unchecked"),
            "{SPEC}'s {marker} no longer opens its status with `**Unchecked`. A write-then-\
             read cannot fail for the requirement and the probe project is not \
             the shipping one, so nothing in this tree discharges these three — \
             and the status word is the one thing a reader scanning for what is \
             outstanding actually sees.\n{para}"
        );

        assert!(
            para.contains("#1353"),
            "{SPEC}'s {marker} names no issue. These three bind the shipping \
             project, which this repository does not contain, so the gap is \
             open and issue #1353 is where what would close it is written \
             down.\n{para}"
        );
    }
}

/// R-E8's status names the one assertion here that reads an artifact.
///
/// **The only one of the three with two independently produced answers.**
/// `just unity-android` unzips the APK its own build produced and requires
/// exactly one ABI directory, which is R-E8's equality read off a build product
/// rather than off the assignment that configured it. A status that folded that
/// in with the other two would understate what is checked, and deleting the
/// assertion while the status still claimed it would overstate it.
#[test]
fn r_e8_s_status_names_the_equality_asserted_on_the_built_apk() {
    let justfile = read("justfile");
    let para = paragraph(&read(SPEC), "R-E8");

    // **The refusal, not the line that computes it, and inside the recipe.**
    // Two earlier versions were vacuous. The first searched for `other_abis`,
    // and renaming the assignment left that name's two later uses in place. The
    // second searched for the exclusion, which lives on the assignment line —
    // so deleting the `if [ -n … ]; then … exit 1; fi` block that acts on it
    // stayed green while a fat APK started passing. Both were caught on
    // 2026-08-29 by running the mutation rather than by reading the assertion.
    let recipe = package_gate::recipe_code(&justfile, "unity-android");
    for pattern in [
        "lib/arm64-v8a/libdashscene_ffi\\.so",
        "grep -v '^lib/arm64-v8a/'",
        "if [ -n \"${other_abis}\" ]; then",
    ] {
        assert!(
            recipe.contains(pattern),
            "`unity-android`'s body carries no `{pattern}` outside its \
             comments, and {SPEC}'s R-E8 says that recipe asserts the equality \
             on the APK it built. Three parts are needed: the required \
             directory, the exclusion that computes what else is there, and the \
             refusal that acts on it — a check for arm64-v8a alone passes a fat \
             APK, which is exactly the configuration the equality forbids."
        );
    }

    // The `exit 1` beneath that guard, so the block cannot become an `echo`.
    let refusal = recipe
        .split("if [ -n \"${other_abis}\" ]; then")
        .nth(1)
        .expect("the guard is asserted above");
    // **The closing keyword at the recipe's indentation, not the bigram.**
    // `split("fi")` truncated at the first word containing those two letters —
    // "verification", "configuration", "the fix" — turning this red with a
    // message about a refusal that was still there.
    let block = refusal
        .split_once("\n    fi")
        .map(|(block, _)| block)
        .unwrap_or_else(|| panic!("the `other_abis` guard never closes:\n{refusal}"));
    assert!(
        block.contains("exit 1"),
        "`unity-android` finds a second ABI directory and does not exit \
         non-zero, so R-E8's equality is reported and not enforced:\n{block}"
    );

    assert!(
        para.contains("just unity-android") && para.contains("arm64-v8a"),
        "{SPEC}'s R-E8 does not name `just unity-android` and the ABI \
         directory it asserts. That assertion is the strongest evidence any of \
         these three requirements carries, and a status that omits it reads as \
         if the requirement were examined only by the code that configures \
         it.\n{para}"
    );
}
