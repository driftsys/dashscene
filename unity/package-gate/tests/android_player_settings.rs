//! R-E7, R-E8 and R-E9 held against every Android player this repository
//! builds.
//!
//! Issue #1353. Those three bind "the host project", and **this repository
//! contains no committed Unity project** — every project here is scaffolded
//! under `target/` by a recipe and thrown away. So the requirement as stated
//! has no artifact to bind to, and
//! `docs/specification/07-embedding-and-distribution.md` records all three as
//! unchecked; `host_project_status.rs` is what holds it to saying so.
//!
//! What this file checks is the nearest thing that does exist: **every
//! committed C# that builds an Android player**. That is a different subject
//! and a weaker claim.
//!
//! # What it catches that nothing else did
//!
//! `AndroidProbeBuild.cs` sets each of the three and reads it back, so a value
//! the object rejected is caught — for that one project. **Nothing at all
//! watched `unity/demo/DemoBuild.cs`**, whose player is the one
//! `just unity-demo-android` installs on the device this project measures on,
//! and that recipe requires only that `lib/arm64-v8a/libdashscene_ffi.so` is
//! present — it has no second-ABI refusal, so **a fat APK passes it**. R-E8 is
//! therefore the one of the three whose breach reaches a device: Mono on
//! arm64 fails in the editor or in Gradle, and a low minimum SDK fails at
//! install. A second ABI slice just makes the artifact twice the size,
//! silently.
//!
//! # How it reads C#, and why not with `contains`
//!
//! Through [`blank_comments_and_strings`], and scoped to the body of the one
//! member that applies the settings. A first version of this file used raw
//! `text.contains` over the whole source, which
//! `package_gate::cs_scan`'s own module documentation records the measured
//! failure of — a needle inside a `Debug.Log` string hid a missing read and
//! the gate passed. It also fails the other way: a comment naming
//! `AndroidArchitecture.All` to explain why it is rejected would have turned
//! the scan red against correct code, so the two build scripts could no longer
//! document the values the rule forbids.
//!
//! **Neither this nor `cs_scan` understands `#if`.** A block disabled by a
//! mistyped symbol still reads as present. Nothing here closes that.
//!
//! # What it cannot do
//!
//! It reads source, not `PlayerSettings`; it cannot see whether a setting
//! survives into the built APK — `just unity-android` asserts R-E8 on the APK
//! and is the only thing that does — and it cannot see a value an editor wrote
//! through the Inspector, which is exactly what a committed host project would
//! carry.
//!
//! **The predicate has one known blind spot.** A build script that sets its
//! target through `EditorUserBuildSettings.activeBuildTarget`, the way
//! `unity/render-gate/RenderGateBuild.cs` does, never names
//! `BuildTarget.Android` and so is invisible here. Nothing builds an Android
//! player that way today, and a recipe passing `-buildTarget` would be the
//! change that makes it possible.

use std::fs;
use std::path::{Path, PathBuf};

use package_gate::cs_scan::{assignment_count, blank_comments_and_strings, member_body};

/// The one copy of the Android SDK floor.
const FLOOR_VARIABLE: &str = "ANDROID_API";

/// The environment variable the recipes pass the floor in.
const FLOOR_ENV: &str = "DASHSCENE_ANDROID_API";

/// The member both build scripts apply the three settings in.
const APPLIER: &str = "SetAndroidPlayerSettings";

/// Every `.cs` under `unity/`, recursively.
fn unity_cs(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            unity_cs(&path, out);
        } else if path.extension().is_some_and(|e| e == "cs")
            && let Ok(text) = fs::read_to_string(&path)
        {
            out.push((path, text));
        }
    }
}

/// One committed Android player build: its path, and the body of the member
/// that applies the three settings, with comments and string literals blanked.
struct Build {
    path: String,
    /// Comments and string literals blanked — for counting assignments and
    /// matching code.
    applier: String,
    /// The same byte range of the untouched source, for the one thing the
    /// blanked view cannot answer: which environment variable is named.
    /// `blank_comments_and_strings` replaces characters in place, so the
    /// offsets `member_body` returns are valid in both.
    applier_raw: String,
}

/// The committed C# that builds an Android player.
///
/// **Derived, not listed.** A hard-coded list would be a second copy of the
/// population, and the defect this file exists for is a build script nobody
/// remembered to check.
fn android_player_builds() -> Vec<Build> {
    let root = package_gate::root();
    let mut all = Vec::new();
    unity_cs(&root.join("unity"), &mut all);

    let mut out: Vec<Build> = all
        .into_iter()
        .filter_map(|(path, text)| {
            // **Raw first, blanked second.** The raw pass is a superset and
            // costs nothing; the blanked pass is what decides, so a file
            // naming both needles only in a comment is still excluded. It is
            // ordered this way because `blank_comments_and_strings` panics on
            // two committed files — `unity/hlsl-conformance/ProbeJson.cs`'s
            // `case '"':` and `DashsceneHlslConformance.cs`'s `@"..."`
            // verbatim strings. That is its documented limit rather than a
            // defect — "On a verbatim string … the painter uses neither
            // today" — and neither file is an Android player build, so the
            // raw filter keeps them out of its way.
            if !text.contains("BuildPipeline.BuildPlayer") || !text.contains("BuildTarget.Android")
            {
                return None;
            }
            let scanned = blank_comments_and_strings(&text);
            if !scanned.contains("BuildPipeline.BuildPlayer")
                || !scanned.contains("BuildTarget.Android")
            {
                return None;
            }
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();

            // **The settings have to be applied, not merely written.**
            // Deleting the one call leaves every token in the file and every
            // needle satisfied, while the player builds on Unity's defaults —
            // which `host_project_status.rs` pins for the probe and this did
            // not, until a review found it.
            assert!(
                scanned.contains(&format!(
                    "private static void {APPLIER}(List<string> failures)"
                )),
                "{shown} builds an Android player and declares no `{APPLIER}`, \
                 so this file cannot find where it sets the three"
            );
            assert!(
                scanned.contains(&format!("{APPLIER}(failures);")),
                "{shown} never calls `{APPLIER}`, so R-E7, R-E8 and R-E9 are \
                 written and never applied and the player takes Unity's \
                 defaults"
            );
            let (open, close) = member_body(
                &scanned,
                &format!("private static void {APPLIER}(List<string> failures)"),
            );
            Some(Build {
                path: shown,
                applier: scanned[open..=close].to_owned(),
                applier_raw: text[open..=close].to_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    assert!(
        !out.is_empty(),
        "no committed C# builds an Android player, so either this scan's \
         predicate is wrong or the Android build scripts have gone — either \
         way it is asserting nothing"
    );
    out
}

/// R-E7 — the Android scripting backend is IL2CPP.
///
/// Unity ships no arm64 Mono runtime, so an arm64 Android player cannot be
/// Mono. That is caught by the editor or by Gradle rather than on a device;
/// what reaches a device is the other two.
#[test]
fn every_android_player_build_selects_il2cpp() {
    for build in android_player_builds() {
        let Build { path, applier, .. } = build;
        assert!(
            applier.contains(
                "PlayerSettings.SetScriptingBackend(\n            NamedBuildTarget.Android, ScriptingImplementation.IL2CPP);"
            ) || applier.contains(
                "PlayerSettings.SetScriptingBackend(NamedBuildTarget.Android, ScriptingImplementation.IL2CPP);"
            ),
            "{path}'s `{APPLIER}` does not set the Android scripting backend \
             to IL2CPP (R-E7):\n{applier}"
        );
        assert_eq!(
            applier.matches("SetScriptingBackend").count(),
            1,
            "{path}'s `{APPLIER}` sets the scripting backend more than once, \
             so the last one wins and this file pins the wrong one"
        );
    }
}

/// R-E8 — the target architecture is exactly ARM64.
///
/// Compared for equality rather than membership: a second ABI slice doubles
/// the native payload of an artifact whose only native binary is the one this
/// project ships.
#[test]
fn every_android_player_build_targets_arm64_and_nothing_else() {
    for build in android_player_builds() {
        let Build { path, applier, .. } = build;
        assert!(
            applier.contains("targetArchitectures = AndroidArchitecture.ARM64;"),
            "{path}'s `{APPLIER}` does not set targetArchitectures to exactly \
             ARM64 (R-E8)"
        );
        // **Counted, because the pinned assignment can be undone on the next
        // line.** `targetArchitectures |= (AndroidArchitecture)1;` leaves the
        // needle above intact and ships an `armeabi-v7a` slice —
        // `unity/android-probe/negative-control.tsv`'s `r-e8-fat-apk` row is
        // that exact shape. `assignment_count` refuses `==` and sees a bare
        // `=`; the compound spellings it cannot see are named below.
        assert_eq!(
            assignment_count(&applier, "targetArchitectures"),
            1,
            "{path}'s `{APPLIER}` assigns targetArchitectures more than once \
             (R-E8)"
        );
        for compound in [
            "targetArchitectures |=",
            "targetArchitectures|=",
            "targetArchitectures &=",
            "targetArchitectures ^=",
        ] {
            assert!(
                !applier.contains(compound),
                "{path}'s `{APPLIER}` carries `{compound}`, which widens the \
                 architecture set without an assignment `assignment_count` \
                 can see (R-E8)"
            );
        }
    }
}

/// R-E9 — the minimum SDK is the `justfile`'s `ANDROID_API`, or higher.
///
/// Every Android target is built through
/// `aarch64-linux-android<ANDROID_API>-clang`, including the
/// `libdashscene_ffi.so` a Unity host loads, so a player declaring a lower
/// minimum than the library it loads installs on a device the library cannot
/// run on.
#[test]
fn every_android_player_build_takes_its_sdk_floor_from_the_justfile() {
    for build in android_player_builds() {
        let Build {
            path,
            applier,
            applier_raw,
        } = build;
        // **The one assertion taken over the raw slice**, because the
        // variable's name is a string literal and the blanked view removes it
        // by design. Both build scripts also name `DASHSCENE_ANDROID_API` a
        // second time inside a `failures.Add(...)` message — a string
        // literal, not a comment — so a scan of the whole raw file for the
        // bare name matched a file whose read had been replaced by a literal.
        // Measured, by doing exactly that. Scoping to the applier's body and
        // matching the call is what closes it.
        let read = format!("GetEnvironmentVariable(\"{FLOOR_ENV}\")");
        assert!(
            applier_raw.contains(&read),
            "{path}'s `{APPLIER}` does not call {read} (R-E9)"
        );
        // **The assignment's whole right-hand side**, not the absence of
        // today's literal. A first version asserted
        // `!contains("(AndroidSdkVersions)33")`, which is keyed to the current
        // floor: `minSdkVersion = AndroidSdkVersions.AndroidApi24;` passed it
        // while declaring a minimum below the one the `.so` was compiled
        // against.
        assert!(
            applier.contains("PlayerSettings.Android.minSdkVersion = (AndroidSdkVersions)floor;"),
            "{path}'s `{APPLIER}` does not assign minSdkVersion from the \
             parsed floor (R-E9):\n{applier}"
        );
        assert_eq!(
            assignment_count(&applier, "minSdkVersion"),
            1,
            "{path}'s `{APPLIER}` assigns minSdkVersion more than once, so a \
             literal after the floor would win (R-E9)"
        );
    }
}

/// The floor has one copy, and this is what keeps it to one.
///
/// The build scripts read `DASHSCENE_ANDROID_API` and never a literal, which
/// the test above holds them to. That is only half the chain: **the recipes
/// have to pass the `justfile`'s own `ANDROID_API` into it.** Nothing pinned
/// that, so `DASHSCENE_ANDROID_API=29` in one recipe would have given the
/// floor a second copy with every gate green.
#[test]
fn every_recipe_passes_the_justfiles_own_floor() {
    let justfile = fs::read_to_string(package_gate::root().join("justfile"))
        .expect("the justfile is readable");
    assert!(
        justfile
            .lines()
            .any(|line| line.starts_with(&format!("{FLOOR_VARIABLE} := "))),
        "the justfile no longer declares `{FLOOR_VARIABLE} := `"
    );

    let passes: Vec<&str> = justfile
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with(&format!("{FLOOR_ENV}=")))
        .collect();
    assert!(
        !passes.is_empty(),
        "no recipe passes {FLOOR_ENV}, so the build scripts that read it get \
         nothing and refuse"
    );
    for line in passes {
        assert_eq!(
            line,
            &format!("{FLOOR_ENV}={{{{ {FLOOR_VARIABLE} }}}} \\"),
            "a recipe passes {FLOOR_ENV} as something other than the \
             justfile's own {FLOOR_VARIABLE}, which gives the floor a second \
             copy"
        );
    }
}

/// The specification names this scan beside each requirement it touches.
///
/// **Per marker, and the paragraph is what is searched.** A first version
/// wrote `paragraph.contains(..) || spec.contains(..)`, which reduces to the
/// second operand — so deleting the sentences added to R-E8 and R-E9 left the
/// test green on R-E7's. `host_project_status.rs` owns the other half, that
/// all three still say **Unchecked**; this does not restate it.
#[test]
fn the_specification_names_this_scan_beside_each_requirement() {
    let spec = fs::read_to_string(
        package_gate::root().join("docs/specification/07-embedding-and-distribution.md"),
    )
    .expect("the specification is readable");
    for marker in ["R-E7", "R-E8", "R-E9"] {
        let opener = format!("**{marker}** —");
        let start = spec
            .find(&opener)
            .unwrap_or_else(|| panic!("{marker} no longer opens a paragraph"));
        let rest = &spec[start..];
        let end = rest.find("\n\n").unwrap_or(rest.len());
        assert!(
            rest[..end].contains("android_player_settings"),
            "{marker}'s paragraph does not name what holds the committed \
             build scripts to it. A reader checking what is outstanding needs \
             both halves: the requirement is unchecked as stated, and this is \
             what does cover the scripts."
        );
    }
}
