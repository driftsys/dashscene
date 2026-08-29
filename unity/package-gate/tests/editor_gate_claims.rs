//! What three documents say `just unity-editor` asks, held to the file it asks
//! it in — and to the call that makes it ask on every run.
//!
//! **The gate itself is in the class issue #1350 describes**: nothing on a pull
//! request compiles `unity/editor-compat/DashsceneEditorCompat.cs`, so a check
//! deleted from it is found by a developer minutes into an editor run — and by
//! then three documents have been claiming it for however long. This is text
//! over a file no CI job compiles, which is what `unity/package-gate` is for.
//!
//! It pins the missing-symbol pair specifically because that pair is what
//! issue #1322 turned from an assertion into a measurement, and because the
//! positive control is the half a well-meaning edit removes: it looks
//! redundant, and without it a library that failed to load reports the same
//! pass.

use std::fs;

const GATE: &str = "unity/editor-compat/DashsceneEditorCompat.cs";
const RECORD: &str = "docs/design/unity-csharp-host.md";
const README: &str = "unity/README.md";
const SKILL: &str = ".claude/skills/project-gates/SKILL.md";
const HEADER: &str = "crates/dashscene-ffi/include/dashscene.h";

/// The check's own method, and the constant naming the symbol it looks for.
const CHECK: &str = "CheckMissingSymbolRaises";
const MISSING: &str = "ds_no_library_exports_this_symbol";

fn read(path: &str) -> String {
    let full = package_gate::root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// Both imports are still declared, and both are still called.
#[test]
fn the_editor_gate_still_asks_what_a_missing_entry_point_raises() {
    // Scanned, not raw: this file's own prose names every one of these tokens
    // while explaining them, so a search over raw text would find the
    // explanation of a check that had been deleted.
    let gate = package_gate::cs_scan::blank_comments_and_strings(&read(GATE));

    for needle in [
        // The two imports.
        "extern uint ds_abi_version()",
        "extern uint ds_missing()",
        // Both called — a declared `[DllImport]` nothing calls binds nothing,
        // which is the defect issue #1308 records for the package itself.
        "Probe.ds_abi_version()",
        "Probe.ds_missing()",
        // The type the whole translation rests on.
        "catch (EntryPointNotFoundException)",
        // **The call site.** Every needle above lives inside `Probe` and inside
        // the method's own body, so deleting this one line left them all in
        // place, made the method dead code — which C# does not diagnose for a
        // private member — and stopped the gate asking the question while three
        // documents went on saying it does. Measured on 2026-08-29.
        "CheckMissingSymbolRaises(failures);",
    ] {
        assert!(
            gate.contains(needle),
            "{GATE} no longer contains `{needle}` outside its comments. \
             {RECORD}, {README} and {SKILL} all state that this gate observes a \
             missing entry point raising EntryPointNotFoundException on Mono, \
             and issue #1322 rests on that observation being taken on every run."
        );
    }

    // **The failure edges, not only the call edges.** Turning either
    // `failures.Add` into a `Debug.Log` leaves every needle above in place and
    // makes a runtime that binds an absent entry point report a pass.
    let raw = read(GATE);
    let (from, to) = package_gate::cs_scan::member_body(
        &gate,
        &format!("private static string {CHECK}(List<string> failures)"),
    );
    let body = &gate[from..to];
    // **The same range over the RAW source.** `blank_comments_and_strings`
    // preserves every byte's position, so one member_body call indexes both —
    // and the two assertions below are about string literals, which the scanned
    // text blanks along with the comments.
    let raw_body = &raw[from..to];
    // **Each edge by identity, not a census of them.** Counting `failures.Add`
    // was the first version and it pinned nothing about WHICH branch records a
    // failure: swapping the two `ds_missing` outcomes — `Debug.Log` on the
    // RETURNED path and `failures.Add` in the catch — keeps the count at four
    // and inverts the gate, so it passes exactly when a runtime binds an absent
    // entry point and fails when it correctly refuses. Measured on 2026-08-29.
    let (returned, caught) = body
        .split_once("catch (EntryPointNotFoundException)")
        .unwrap_or_else(|| {
            panic!(
                "{CHECK} no longer catches EntryPointNotFoundException by name, \
                 which is the one type every forwarder in Native.cs catches."
            )
        });
    assert!(
        returned.contains("Probe.ds_missing();") && returned.contains("failures.Add("),
        "{CHECK} calls `Probe.ds_missing()` and does not record a failure when \
         it RETURNS. A runtime that binds an absent entry point would then be \
         reported as a pass, which is issue #1322's whole subject."
    );
    let arm = caught
        .split_once("catch (Exception e)")
        .map_or(caught, |(a, _)| a);
    assert!(
        !arm.contains("failures.Add("),
        "{CHECK} records a failure in the `EntryPointNotFoundException` arm — \
         the arm the healthy run takes. The gate is inverted:\n{arm}"
    );
    assert!(
        raw_body.contains(r#"Type.GetType("Mono.Runtime") == null"#),
        "{CHECK} no longer refuses a runtime on `Mono.Runtime` being ABSENT. \
         Inverted, it fails on Mono and passes silently on everything else, \
         while three records say the reading was taken on Mono."
    );
    assert!(
        raw_body.contains("SKIPPED"),
        "{CHECK} no longer reports a SKIP in the clause the summary line \
         prints. On a host where D3 ships no library an editor can load it \
         makes no call at all, and a skip that does not say so reads as a pass."
    );

    // The constant has to name a symbol nothing exports, or the check measures
    // nothing while still passing — and no pull request compiles this file.
    // Raw rather than scanned: the constant's value IS a string literal, and
    // `blank_comments_and_strings` blanks those along with the comments.
    assert!(
        raw.contains(MISSING),
        "{GATE} no longer names `{MISSING}`. The check is only a measurement \
         while the entry point it asks for is one no library exports."
    );
    assert!(
        !read(HEADER).contains(MISSING),
        "`{MISSING}` is now declared in {HEADER}, so the shipped library may \
         export it — and {GATE} would then be asking what a runtime does with a \
         symbol that is present. Rename the constant."
    );
    assert!(
        read(HEADER).contains("ds_abi_version"),
        "{HEADER} no longer declares `ds_abi_version`, which {GATE} calls as \
         its positive control. Without an exported symbol beside the missing \
         one, a library that failed to load reports the same pass."
    );
}

/// The three documents that state the measurement still state it.
///
/// The other direction of the same pin: a check kept and a record that stopped
/// naming it is a gate nobody knows they have, and the next reader files
/// issue #1322 again.
#[test]
fn the_records_that_claim_that_measurement_still_name_it() {
    // **A phrase unique to the new claim, and all THREE documents.**
    // `EntryPointNotFoundException` alone was vacuous for the design record: it
    // already occurred there twice, about `unity/ffi-check` and issue #1308, so
    // deleting the whole measurement section left this green. And the module
    // doc above says three documents while a first version iterated over two.
    // Both measured on 2026-08-29.
    // **A phrase unique to THIS claim in each file, because the obvious tokens
    // are not.** `EntryPointNotFoundException` was already in the design record
    // twice, about `unity/ffi-check` and issue #1308; and `Mono` occurs in all
    // three for older reasons — `MonoImporter` in the record, and a sentence
    // about `ffi-check` running on CoreCLR "rather than on Mono or IL2CPP" in
    // the skill. Deleting the whole measurement section survived both. So each
    // file is held to a phrase that exists only because of this measurement.
    for (path, unique) in [
        (RECORD, MISSING),
        (README, "no library exports"),
        (SKILL, "a symbol the loaded"),
    ] {
        let text = read(path);
        assert!(
            text.contains("EntryPointNotFoundException"),
            "{path} no longer names `EntryPointNotFoundException`, and {GATE} \
             still measures it. A gate no record names is one the next reader \
             re-files."
        );
        assert!(
            text.contains(unique),
            "{path} no longer carries `{unique}`, which appears there only \
             because of issue #1322's measurement. The record has stopped \
             stating what {GATE} still does on every run."
        );
    }
}
