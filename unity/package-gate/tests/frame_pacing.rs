//! The Android player's frame pacing, held to the two edits that lift the
//! 30 fps cap.
//!
//! Issue #1408: the Unity showcase player on Android presented at 30 fps and
//! nothing in this repository decided that. `Application.targetFrameRate` left
//! at -1 is 30 on Unity's Android player whatever the panel does, and with
//! Unity's default pacing a player asked for 60 presented on every other vsync.
//! Two edits lift the cap, and each is pinned here because no CI job compiles
//! either file: the sample compiles in the players `just unity-demo` and
//! `just unity-demo-android` build and in `just unity-editor`'s project, and
//! `unity/demo/DemoBuild.cs` runs inside the first two, all of which need an
//! editor:
//!
//! - the Showcase sample sets an explicit `60` in a method marked
//!   `[RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]`,
//!   before the first frame;
//! - `unity/demo/DemoBuild.cs` sets `PlayerSettings.Android.optimizedFramePacing`
//!   on the Android player.
//!
//! **The trap is pinned as an absence.** Reading the display's rate back is not
//! an answer: Unity's init has already asked the compositor for 30 Hz, and under
//! Android's per-app frame-rate override `Screen.currentResolution.refreshRateRatio`
//! reports the rate the app was granted — so an `Awake()` line that set the
//! target from it set 30 again (`docs/design/android-toolchain.md`, "The Unity
//! host's presented rate"). The target is assigned once across the package's
//! `Runtime/` and every sample, and only inside the method that runs before
//! the first frame.
//!
//! **What a text scan can and cannot hold.** Every scan below is over blanked
//! source — `package_gate::cs_scan` — so a line moved into a comment reads as
//! removed, except the one that reads the sample's prose for the trap's name.
//! The early method's body is held to exactly one statement, compared with
//! its spaces removed, so a guard around the assignment or a second write
//! after it is refused rather than searched for and a respacing is accepted;
//! the pacing file and the whole of `DemoBuild.cs` may carry no preprocessor
//! directive, because the scan does not evaluate `#if` and a line compiled
//! out of the Android player would otherwise read as present — the other
//! package sources are counted, not directive-checked, which errs towards a
//! red. What the scan still keys on is the qualified spelling
//! `Application.targetFrameRate` followed by `=`: a compound assignment
//! (`-= 30`), a deconstruction, an alias (`using App = UnityEngine.Application`
//! or `using static`), or spaces around the dot are not counted — issue #1435
//! carries the class. `OnDemandRendering.renderFrameInterval`, Unity's other
//! process-wide divisor of the presented rate, is refused by name.

use package_gate::cs_scan::{assignment_count, blank_comments_and_strings, member_body, squeeze};

/// Under `package_gate::PACKAGE_PATH`.
const SHOWCASE_DIR: &str = "Samples~/Showcase";

const DEMO_BUILD: &str = "unity/demo/DemoBuild.cs";

const EARLY: &str =
    "[RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]";

const TARGET: &str = "Application.targetFrameRate";

const PACING: &str = "PlayerSettings.Android.optimizedFramePacing";

/// The one statement the early method may hold, with every space removed.
const STATEMENT: &str = "Application.targetFrameRate=60;";

fn read(relative: &str) -> String {
    let path = package_gate::root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `.cs` the package compiles into a player or an editor project:
/// `Runtime/` and every sample, each walked recursively, as (path relative
/// to the root, blanked source).
///
/// **Both trees, recursively, as an upper bound.** `just unity-demo` and
/// `just unity-demo-android` compile the package's `Runtime/` (the whole
/// package is embedded) and `Samples~/Showcase/*.cs` by a flat glob; `just
/// unity-editor` compiles every sample recursively and builds no player. The
/// scan covers the union, so an assignment a recipe would compile is never
/// outside it; a file the flat glob does not copy is scanned too, which errs
/// towards a red.
fn package_sources() -> Vec<(String, String)> {
    let mut files = package_gate::cs_files_under("Runtime");
    files.extend(package_gate::cs_files_under("Samples~"));
    files
        .into_iter()
        .map(|(path, source)| (path, blank_comments_and_strings(&source)))
        .collect()
}

/// The pacing file's own path: the demo recipes copy `Samples~/Showcase/*.cs`
/// by a flat glob, so a file one directory down would be scanned here and
/// compiled into no player.
const PACING_FILE: &str = "unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFramePacing.cs";

/// The brace depth at `at` inside `body`, whose first character is the
/// opening brace of a method: 1 is the method's own top level.
fn depth_at(body: &str, at: usize) -> usize {
    let mut depth = 0usize;
    for c in body[..at].chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// The brace-delimited block that follows `needle`, braces included.
fn block_after<'a>(text: &'a str, needle: &str, what: &str) -> &'a str {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("{what}: no `{needle}`"));
    let open = start
        + text[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{what}: `{needle}` opens no block"));
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("{what}: the block after `{needle}` never closes")
}

/// Refuses a preprocessor directive inside a scanned region: `#if` is not
/// evaluated here, so a directive would let a line the Android player never
/// compiles read as present.
fn assert_no_directive(path: &str, region: &str, what: &str) {
    for line in region.lines() {
        assert!(
            !line.trim_start().starts_with('#'),
            "{path}: {what} holds a preprocessor directive, `{}`, which this scan \
             does not evaluate — a line under it may be compiled out of the \
             Android player while reading as present here (issue #1408).",
            line.trim()
        );
    }
}

/// The sample asks for 60 before its first frame, in a static method whose
/// body is that one statement, and says why a read-back would not do.
///
/// The assignment is looked for inside the body of the method carrying the
/// `SubsystemRegistration` attribute — not anywhere in the file — because the
/// point of issue #1408's first edit is WHEN it runs: `Awake()` is after Unity's
/// init has asked the compositor for 30 Hz. Unity registers the attribute on a
/// static method only, so `static` is held too.
#[test]
fn the_showcase_sample_asks_for_sixty_before_its_first_frame() {
    let sources = package_sources();
    let carrying: Vec<&(String, String)> = sources
        .iter()
        .filter(|(path, scanned)| path.contains(SHOWCASE_DIR) && scanned.contains(EARLY))
        .collect();
    assert_eq!(
        carrying.len(),
        1,
        "expected exactly one file in the Showcase sample to carry `{EARLY}`, found \
         {}: {:?}. Without it the Android player paces at 30 fps (issue #1408).",
        carrying.len(),
        carrying.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    let (path, scanned) = carrying[0];
    assert_eq!(
        path, PACING_FILE,
        "{path} carries `{EARLY}`, but the demo recipes copy `Samples~/Showcase/*.cs` by a \
         flat glob, so only a file directly under the sample reaches a player (issue #1408)."
    );
    assert_no_directive(path, scanned, "the file that sets the target");
    let source = read(path);

    let attribute = scanned.find(EARLY).expect("the attribute the filter found");
    let (open, close) = member_body(scanned, EARLY);
    let declaration = squeeze(&scanned[attribute + EARLY.len()..open]);
    assert!(
        declaration.contains("static ") && declaration.trim_end().ends_with("()"),
        "{path}: the method carrying `{EARLY}` is not a static method with no \
         parameters — `{declaration}` — and Unity invokes the attribute on those only, \
         so it would never run."
    );
    let body = &scanned[open..=close];
    let inner: String = body[1..body.len() - 1].split_whitespace().collect();
    assert_eq!(
        inner,
        STATEMENT,
        "{path}: the `SubsystemRegistration` method's body is not the one \
         statement `{TARGET} = 60;`. A guard around it, a second write after it, \
         or anything else in the body is refused, because a player whose body \
         does not run that statement paces at 30 fps (issue #1408). Its body:\n{}",
        squeeze(body)
    );

    // **The trap, named beside the fix.** Read from the un-blanked source
    // because it is prose: this pins that the file's comment names the
    // read-back, not that the code avoids it — the second test holds the code.
    assert!(
        source.contains("refreshRateRatio"),
        "{path}: the file that sets the target does not name \
         `refreshRateRatio`, so the trap issue #1408 records — that reading the \
         display's rate back gives the 30 Hz the app was granted — is not \
         recorded beside the fix."
    );
}

/// The target is assigned once across the package's `Runtime/` and samples,
/// and only in the early method; the Showcase sample's code reads no display
/// rate back.
///
/// **An absence, so the mutation is required**: add
/// `Application.targetFrameRate = Mathf.RoundToInt((float)Screen.currentResolution.refreshRateRatio.value);`
/// to an `Awake()` in the sample, or `Application.targetFrameRate = 30;` to a
/// `Start()` under `Runtime/`, and this must go red. The first line is the
/// 2026-09-03 trap, and it compiles.
///
/// The read-back refusal covers the Showcase sample alone: the FrameLoop
/// sample reads `refreshRateRatio` to advise a commit-rate divisor, which is a
/// use, not a target — under the override it advises from the granted rate,
/// which issue #1434 carries rather than this scan refusing it.
#[test]
fn the_target_is_assigned_once_in_the_package_and_only_before_the_first_frame() {
    let mut total = 0;
    for (path, scanned) in package_sources() {
        assert!(
            !scanned.contains("OnDemandRendering"),
            "{path} reaches `OnDemandRendering`, Unity's other process-wide divisor of the \
             presented rate; a `renderFrameInterval` of 2 halves the rate with the target \
             untouched (issue #1408)."
        );
        if path.contains(SHOWCASE_DIR) {
            for read_back in ["refreshRateRatio", "currentResolution"] {
                assert!(
                    !scanned.contains(read_back),
                    "{path} reads `{read_back}` in code. Under Android's per-app \
                     frame-rate override that reports the rate the app was \
                     GRANTED, which is the 30 Hz Unity's init already asked for \
                     (issue #1408)."
                );
            }
        }
        let here = assignment_count(&scanned, TARGET);
        total += here;
        if here == 0 {
            continue;
        }
        assert!(
            scanned.contains(EARLY),
            "{path} assigns `{TARGET}` {here} time(s) and carries no \
             `SubsystemRegistration` method. An assignment anywhere else runs \
             after Unity's init has asked the compositor for 30 Hz (issue #1408)."
        );
        let (open, close) = member_body(&scanned, EARLY);
        let inside = assignment_count(&scanned[open..=close], TARGET);
        assert_eq!(
            here, inside,
            "{path} assigns `{TARGET}` {here} time(s), of which {inside} inside \
             the `SubsystemRegistration` method. An assignment anywhere else runs \
             after Unity's init has asked the compositor for 30 Hz."
        );
    }
    assert_eq!(
        total, 1,
        "the package's Runtime/ and samples assign `{TARGET}` {total} time(s); \
         exactly one, inside the `SubsystemRegistration` method, is the shape \
         issue #1408 measured."
    );
}

/// The Android player is built with optimized frame pacing on, and reads the
/// setting back.
///
/// Inside `SetAndroidPlayerSettings`, which `Build` runs for the Android target
/// alone — and that call is held too, because pinning the setting without the
/// call leaves it reachable from nothing (`host_project_status.rs` records the
/// same for the probe's build script). Off — Unity's default — a player asked
/// for 60 presented on every other vsync, 87 of 125 intervals at 33.4 ms with
/// each frame ready about 21 ms before it was shown (issue #1408).
#[test]
fn the_android_player_is_built_with_optimized_frame_pacing_on() {
    let scanned = blank_comments_and_strings(&read(DEMO_BUILD));
    // The whole file, so a `#if`/`#else` twin of a method is refused with the
    // rest: the scan pins the first declaration it finds.
    assert_no_directive(DEMO_BUILD, &scanned, "the build script");
    assert_eq!(
        scanned.matches("void SetAndroidPlayerSettings(").count(),
        1,
        "{DEMO_BUILD} declares `SetAndroidPlayerSettings` other than once; the scan pins \
         the first declaration."
    );
    assert_eq!(
        assignment_count(&scanned, PACING),
        1,
        "{DEMO_BUILD} assigns `{PACING}` other than exactly once; a later \
         assignment would be the value the player is built with (issue #1408)."
    );
    // **The declaration, not the call.** `Build()` calls
    // `SetAndroidPlayerSettings(failures)` first, and a scan keyed on the bare
    // name took the `if` block after that call as the body — measured while
    // writing this test, red for the wrong reason.
    let (open, close) = member_body(&scanned, "void SetAndroidPlayerSettings(");
    let squeezed = squeeze(&scanned[open..=close]);
    let setter = format!("{PACING} = true;");
    let at = squeezed.find(&setter).unwrap_or_else(|| {
        panic!(
            "{DEMO_BUILD}: `SetAndroidPlayerSettings` does not set `{PACING} = true`. \
             Off, a player asked for 60 presents on every other vsync (issue #1408). \
             Its body:\n{squeezed}"
        )
    });
    assert_eq!(
        depth_at(&squeezed, at),
        1,
        "{DEMO_BUILD}: `{setter}` sits inside a block of `SetAndroidPlayerSettings` rather \
         than at its top level, so a condition decides whether the player is paced. Its \
         body:\n{squeezed}"
    );
    let read_back = format!("if (!{PACING})");
    let at = squeezed.find(&read_back).unwrap_or_else(|| {
        panic!(
            "{DEMO_BUILD}: `SetAndroidPlayerSettings` does not read `{PACING}` back after \
             setting it. Its body:\n{squeezed}"
        )
    });
    assert_eq!(
        depth_at(&squeezed, at),
        1,
        "{DEMO_BUILD}: the read-back of `{PACING}` sits inside a block rather than at the \
         method's top level. Its body:\n{squeezed}"
    );
    let refusal = block_after(&squeezed, &read_back, DEMO_BUILD);
    assert!(
        refusal.contains("failures.Add("),
        "{DEMO_BUILD}: a false read-back of `{PACING}` adds no failure, so the build \
         proceeds with the pacing off and only the log says so: `{refusal}`"
    );

    let (open, close) = member_body(&scanned, "void Build(");
    let build = squeeze(&scanned[open..=close]);
    let android = block_after(&build, "if (BuildingForAndroid)", DEMO_BUILD);
    assert!(
        android.contains("SetAndroidPlayerSettings(failures);"),
        "{DEMO_BUILD}: `Build`'s Android branch no longer calls \
         `SetAndroidPlayerSettings(failures)`, so the pacing setting is reachable from \
         nothing. The branch:\n{android}"
    );
    let call = build
        .find("SetAndroidPlayerSettings(failures);")
        .expect("the call the branch holds");
    let player = build.find("BuildPlayer(failures);").unwrap_or_else(|| {
        panic!("{DEMO_BUILD}: `Build` no longer calls `BuildPlayer(failures)`.")
    });
    assert!(
        call < player,
        "{DEMO_BUILD}: `Build` applies the Android settings after it builds the player."
    );
}
