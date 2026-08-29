//! The Unity sample's frame-cost instrument, held to the one it is stated
//! against.
//!
//! Issue #1329's third limb asks for "a per-frame figure whose definition is
//! stated against the instrument in `demo/src/shell.rs`", and issue #1347 says
//! why in terms: a Unity figure taken from `Time.deltaTime` or from the
//! profiler measures the engine's frame rather than the painter's work, so a
//! comparison built on one is between two harnesses and not between two
//! painters.
//!
//! **Nothing else here reads that sample.** `just unity-editor` compiles every
//! sample into a throwaway project and `just unity-demo` builds a player from
//! this one, and both need a Unity editor, so neither runs in CI. What this
//! crate can do without an editor is hold the two definitions to each other —
//! which is the half that goes wrong silently, because a sample size or a name
//! changed on one side compiles perfectly on the other.
//!
//! It is the same shape as `sdf_hlsl_is_generated`: a fact that lives in two
//! files, re-derived here rather than trusted.

use std::path::Path;

/// The Showcase sample's frame-cost source.
fn instrument() -> String {
    read("unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneFrameCost.cs")
}

fn read(relative: &str) -> String {
    let path = package_gate::root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// `pub const NAME: usize = <n>;` out of a Rust source.
///
/// Parsed rather than imported: `demo` and `demo-android` are binaries this
/// crate does not depend on, and adding a dependency to read one integer would
/// make the two hosts' build graphs answer to a gate over a Unity package.
fn rust_const_usize(source: &str, name: &str) -> Option<i64> {
    let needle = format!("const {name}: usize = ");
    let at = source.find(&needle)? + needle.len();
    let rest = &source[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

/// The three hosts report over the same number of frames.
///
/// **The reason this is a test and not a comment.** `demo/src/shell.rs` says
/// its 240 is the sample size `docs/technotes/frame-budget.md` states, and
/// `demo-android/src/timing.rs` repeats it saying "so the three are read in the
/// same units". The Unity sample is the fourth copy of that number, in a
/// language neither of the other two compiles, and the figure it produces is
/// the one issue #1347 sets beside the lean painter's. A sample size that
/// silently drifted would make the two figures cover different amounts of work
/// while reading as the same quantity.
#[test]
fn the_unity_sample_reports_over_the_same_number_of_frames_as_the_rust_hosts() {
    let desktop = read("demo/src/shell.rs");
    let android = read("demo-android/src/timing.rs");

    let desktop_sample = rust_const_usize(&desktop, "TIMING_SAMPLE")
        .expect("demo/src/shell.rs declares TIMING_SAMPLE as a usize const");
    let android_sample = rust_const_usize(&android, "TIMING_SAMPLE")
        .expect("demo-android/src/timing.rs declares TIMING_SAMPLE as a usize const");
    let unity = package_gate::cs_const_int(&instrument(), "TimingSample")
        .expect("DashsceneFrameCost declares `const int TimingSample`");

    assert_eq!(
        desktop_sample, android_sample,
        "demo and demo-android report over different numbers of frames, so the \
         Unity sample cannot match both. This test is about the Unity side and \
         has found a disagreement between the two Rust hosts instead."
    );
    assert_eq!(
        unity, desktop_sample,
        "the Unity sample reports over {unity} frames and demo/src/shell.rs \
         over {desktop_sample}. Issue #1347 sets the two figures beside each \
         other, so they have to cover the same amount of work."
    );
}

/// The Unity figure is not called `present`, and says what it excludes.
///
/// **One word must not name two quantities**, which is the rule
/// `demo-android/src/timing.rs` already states about its own rename: the
/// desktop host's `present` spans paint as well, so the Android host reports
/// `submit`. The Unity figure is a third quantity again — `AcquireFrame`,
/// `Draw` and the lease release, with the GPU execution and the swapchain
/// present outside it, because Unity owns both and this project does not.
#[test]
fn the_unity_figure_does_not_borrow_a_name_that_already_means_something_else() {
    let source = instrument();

    let reported = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .filter(|line| !line.trim_start().starts_with("///"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !reported.contains("present mean"),
        "DashsceneFrameCost reports a figure it calls `present`. The desktop \
         host prints `present` for paint plus present and the Android host \
         prints `submit` for the upload, encode, submit and swapchain; this \
         one is neither, so a third name is what keeps the three readable."
    );
    assert!(
        reported.contains("draw mean"),
        "DashsceneFrameCost does not report a `draw mean`. That is the name \
         the record states the Unity figure under, so the table and the record \
         disagree the moment it changes."
    );
    assert!(
        reported.contains("tick "),
        "DashsceneFrameCost does not report a `tick`. That is the one term the \
         Unity figure and the two Rust hosts genuinely share — the same \
         `ds_runtime_tick` across the same C ABI — so it is what makes the \
         comparison more than two unrelated numbers."
    );
}

/// Every reported row names the extent it was drawn at.
///
/// Issue #1236's rule, applied to the instrument that did not exist when it was
/// filed. Orientation changes the workload and not only the pixel count, so a
/// per-frame figure with no extent beside it is not comparable to any other —
/// and a Unity player rotates for exactly the reason #1346 exercises.
#[test]
fn every_reported_row_names_the_extent_it_was_drawn_at() {
    let source = instrument();
    assert!(
        source.contains("public int Width") && source.contains("public int Height"),
        "FrameCostSample carries no extent, so its rows would be the defect \
         issue #1236 records, one painter later."
    );
    assert!(
        source.contains("{1}x{2} over {3} frames"),
        "DashsceneFrameCost's reported line does not place the extent between \
         what was drawn and how many frames it covers. A figure recorded \
         without it cannot be set beside the lean painter's, which `frames.md` \
         now names per row."
    );
    assert!(
        source.contains(r#"entry + "@" + width + "x" + height"#),
        "the sample in hand is not keyed on the extent, so a rotation part-way \
         through would be averaged across rather than discarded. That is the \
         same boundary `shell.rs` clears on, and issue #1346 rotates a Unity \
         player on purpose."
    );
}

/// The reported line's format is pinned whole, and its readers are named.
///
/// **Three parsers outside this language depend on it.**
/// `measure/android/unity-lifecycle.sh` reads the extent out of it to decide
/// whether a lifecycle event reached the app, `measure/android/unity-frame-cost.sh`
/// reshapes every field of it into the table it publishes, and
/// `measure/android/record-check.py` re-derives the design record from it. A
/// review of PR #1377 changed ` at ` to ` @ ` in the format string and all four
/// tests here stayed green, because each asserted a fragment: the shell tests
/// stayed green too, because their stub writes its own copy of the line and so
/// agrees with the test rather than with the producer.
///
/// Pinning the literal is what makes a change to it a decision rather than an
/// accident — the three readers are named here so whoever makes that decision
/// knows what else moves.
#[test]
fn the_reported_line_format_is_pinned_whole_and_its_readers_are_named() {
    let source = instrument();

    const FORMAT: &str = r#""{0} at {1}x{2} over {3} frames — tick {4:F2} ms, "
                + "draw mean {5:F2} p50 {6:F2} p95 {7:F2} max {8:F2} ms "
                + "({9:F1} fps if unpaced)""#;
    assert!(
        source.contains(FORMAT),
        "DashsceneFrameCost's reported line format has changed. Three parsers \
         outside this language read it — measure/android/unity-lifecycle.sh, \
         measure/android/unity-frame-cost.sh and \
         measure/android/record-check.py — and each one silently stops \
         matching rather than failing. Move them with it, then update this \
         literal.\n\nexpected to find:\n{FORMAT}"
    );

    for reader in [
        "measure/android/unity-lifecycle.sh",
        "measure/android/unity-frame-cost.sh",
        "measure/android/record-check.py",
    ] {
        let path = package_gate::root().join(reader);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        assert!(
            text.contains(" at ") && text.contains(" over "),
            "{reader} is named here as a reader of the line format above and no \
             longer carries its ` at ` and ` over ` anchors, so it parses \
             something else."
        );
    }
}

/// The instrument names the file it is defined against, and that file is there.
///
/// **A definition stated against a path that has moved is not stated against
/// anything.** This is the one link issue #1329's third limb turns on, and the
/// cheapest way for it to rot is a rename somewhere else in the tree.
#[test]
fn the_instrument_names_the_definition_it_is_stated_against_and_it_exists() {
    let source = instrument();
    const AGAINST: &str = "demo/src/shell.rs";
    assert!(
        source.contains(AGAINST),
        "DashsceneFrameCost does not name {AGAINST}. Issue #1329's third limb \
         is a figure whose definition is stated against that instrument, so \
         the statement is the deliverable and not a courtesy."
    );
    assert!(
        Path::new(&package_gate::root().join(AGAINST)).is_file(),
        "DashsceneFrameCost states its definition against {AGAINST}, which is \
         not a file in this tree."
    );
}
