//! R-E7, R-E8 and R-E9 held against every Android player this repository
//! builds.
//!
//! Issue #1353. Those three bind "the host project", and **this repository
//! contains no committed Unity project** — every project here is scaffolded
//! under `target/` by a recipe and thrown away. So the requirement as stated
//! has no artifact to bind to, and `docs/specification/07-embedding-and-distribution.md`
//! records all three as unchecked for that reason.
//!
//! What this file checks instead is the nearest thing that does exist: **every
//! committed C# that builds an Android player**. That is a different subject
//! and a weaker claim, and the specification says which is which rather than
//! letting this stand in for the requirement.
//!
//! **What it catches that nothing else did.** `AndroidProbeBuild.cs` sets each
//! of the three and reads it back, so a value the object rejected is caught
//! there — for that one project. Nothing at all watched `DemoBuild.cs`, and
//! nothing watches a third Android build script that has not been written yet.
//! A build script that ships an `armeabi-v7a` slice, or Mono, or a hard-coded
//! minimum SDK, is a shipped-artifact defect on the target this project
//! measures on, and until this file the first thing to notice would have been
//! a device.
//!
//! **What it cannot do.** It reads source, not `PlayerSettings`. It cannot see
//! whether a setting survives to the built APK — `just unity-android` asserts
//! R-E8 on the APK it produced and is the only thing that does — and it cannot
//! see a value written by an editor through the Inspector, which is exactly
//! what a committed host project would carry and why the requirement is not
//! met by this.

use std::fs;
use std::path::{Path, PathBuf};

/// The one copy of the Android SDK floor, read from the `justfile` rather than
/// written here — which is the same rule the two build scripts follow, and the
/// reason this can compare them to it.
const FLOOR_VARIABLE: &str = "ANDROID_API";

/// The environment variable the recipes pass the floor in.
const FLOOR_ENV: &str = "DASHSCENE_ANDROID_API";

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

/// The committed C# that builds an Android player.
///
/// **Derived, not listed.** A file that calls `BuildPipeline.BuildPlayer` and
/// names `BuildTarget.Android` is one; a hard-coded list here would be a
/// second copy of the population, and the defect this file exists for is a
/// build script nobody remembered to check.
fn android_player_builds() -> Vec<(String, String)> {
    let mut all = Vec::new();
    unity_cs(&package_gate::root().join("unity"), &mut all);
    let root = package_gate::root();
    let mut out: Vec<(String, String)> = all
        .into_iter()
        .filter(|(_, text)| {
            text.contains("BuildPipeline.BuildPlayer") && text.contains("BuildTarget.Android")
        })
        .map(|(path, text)| {
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            (shown, text)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
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
/// Mono. This is not a preference.
#[test]
fn every_android_player_build_selects_il2cpp() {
    for (path, text) in android_player_builds() {
        assert!(
            text.contains("ScriptingImplementation.IL2CPP"),
            "{path} builds an Android player and does not select IL2CPP \
             (R-E7). Unity ships no arm64 Mono runtime, so the player would \
             not run on the target at all."
        );
        assert!(
            !text.contains("ScriptingImplementation.Mono2x"),
            "{path} names Mono for a target that has no arm64 Mono runtime"
        );
    }
}

/// R-E8 — the target architecture is exactly ARM64.
///
/// Compared for equality rather than membership: a value that also carries
/// `ARMv7` fails, because a second ABI slice doubles the native payload in an
/// artifact whose only native binary is the one this project ships.
#[test]
fn every_android_player_build_targets_arm64_and_nothing_else() {
    for (path, text) in android_player_builds() {
        assert!(
            text.contains("targetArchitectures = AndroidArchitecture.ARM64;"),
            "{path} does not set targetArchitectures to exactly ARM64 (R-E8)"
        );
        // The union spellings, which are what "exactly" excludes. Named one at
        // a time so a reader knows what the list covers.
        for wrong in [
            "AndroidArchitecture.ARMv7",
            "AndroidArchitecture.X86_64",
            "AndroidArchitecture.All",
        ] {
            assert!(
                !text.contains(wrong),
                "{path} names {wrong}, so the architecture is not exactly \
                 ARM64 (R-E8). An emulator run wants X86_64 and this is where \
                 that decision has to be taken deliberately rather than found."
            );
        }
    }
}

/// R-E9 — the minimum SDK is the `justfile`'s `ANDROID_API`, or higher.
///
/// **The floor has one copy and this is what holds it to one.** Every Android
/// target is built through `aarch64-linux-android<ANDROID_API>-clang`,
/// including the `libdashscene_ffi.so` a Unity host loads, so a player
/// declaring a lower minimum than the library it loads would install on a
/// device the library cannot run on. A literal in a build script is a second
/// copy that drifts the first time the variable moves.
#[test]
fn every_android_player_build_takes_its_sdk_floor_from_the_justfile() {
    let justfile = fs::read_to_string(package_gate::root().join("justfile"))
        .expect("the justfile is readable");
    let declared = justfile
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{FLOOR_VARIABLE} := ")))
        .map(|value| value.trim().trim_matches('"').to_owned())
        .unwrap_or_else(|| panic!("the justfile no longer declares `{FLOOR_VARIABLE} := `"));
    assert!(
        declared.parse::<u32>().is_ok(),
        "{FLOOR_VARIABLE} is `{declared}`, which is not a number"
    );

    for (path, text) in android_player_builds() {
        // **The call, not the name.** Both build scripts discuss
        // `DASHSCENE_ANDROID_API` in a comment as well as reading it, so
        // matching the bare name passed a file whose read had been replaced by
        // a literal — measured, by doing exactly that. A needle found anywhere
        // in a file pins nothing, which is the trap `recipe_body`'s own doc
        // records for the justfile.
        let read = format!("GetEnvironmentVariable(\"{FLOOR_ENV}\")");
        assert!(
            text.contains(&read),
            "{path} does not call {read} (R-E9), so its minimum SDK is either \
             absent or a second copy of the floor the justfile declares as \
             {declared}"
        );
        assert!(
            text.contains("PlayerSettings.Android.minSdkVersion"),
            "{path} reads {FLOOR_ENV} and does not apply it to \
             minSdkVersion (R-E9)"
        );
        assert!(
            !text.contains(&format!("(AndroidSdkVersions){declared}")),
            "{path} hard-codes the floor as {declared} rather than taking it \
             from {FLOOR_ENV}; the two copies drift the first time the \
             justfile's {FLOOR_VARIABLE} moves"
        );
    }
}

/// The specification says what this file does and does not check.
///
/// Without this, the three requirements could be quietly marked met by a scan
/// that reads source and never touches `PlayerSettings` — which is the shape
/// of defect issue #1353 was opened about in the first place, from the other
/// direction: a status asserting an absence the tree refutes.
#[test]
fn the_specification_still_says_these_three_are_unchecked_as_stated() {
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
        let paragraph = &rest[..end];
        assert!(
            paragraph.contains("Unchecked"),
            "{marker}'s paragraph no longer says it is unchecked. This file \
             scans committed build scripts and reads no PlayerSettings, so it \
             does not meet the requirement as stated — if something now does, \
             say so there and say what it is."
        );
        assert!(
            paragraph.contains("android_player_settings")
                || spec.contains("android_player_settings"),
            "{marker} is unchecked as stated, and the specification does not \
             name what DOES hold the committed build scripts to it. A reader \
             checking what is outstanding needs both halves."
        );
    }
}
