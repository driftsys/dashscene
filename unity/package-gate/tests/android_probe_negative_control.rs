//! `just unity-android-negative`'s table, held to the sources it mutates.
//!
//! **A negative control is a gate, and a gate over an editor-only recipe stops
//! matching its sources where nobody is looking.** The mutations that show `just unity-android` can
//! fail need an editor with Android Build Support and an attached device, so
//! months can pass between runs while the probe sources move underneath them.
//! When a `find` stops matching, the recipe copies an UNMUTATED source and runs
//! a positive gate that reports success — a negative control that proves the
//! opposite of what it claims. The recipe refuses that at run time; this
//! refuses it on every pull request, where it costs nothing.
//!
//! Issue #1370 is why the table exists at all: before it, the four mutations
//! were prose in a commit message, and `implementing-a-change` rules that
//! mutation evidence expires when a later round changes the code. It had
//! expired — the recorded evidence for `runtime-throws` said the recipe named
//! R-E21, and the marker loop had since moved in front of that block.
//!
//! **What this cannot say is that a mutation still turns the recipe red.** Only
//! a device answers that. It says the table can still be applied, and that
//! every diagnostic it waits for is text something here still prints.

use std::fs;

const TABLE: &str = "unity/android-probe/negative-control.tsv";
const PROBE_DIR: &str = "unity/android-probe";
const COLUMNS: [&str; 6] = ["name", "file", "find", "replace", "expect", "marker"];

struct Row {
    name: String,
    file: String,
    find: String,
    replace: String,
    expect: String,
    marker: String,
}

fn read(path: &str) -> String {
    let full = package_gate::root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// The data rows, with the header checked rather than skipped by position.
fn rows() -> Vec<Row> {
    let table = read(TABLE);
    let mut lines = table
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty());

    let header: Vec<&str> = lines
        .next()
        .expect("the table has no header row")
        .split('\t')
        .collect();
    assert_eq!(
        header, COLUMNS,
        "{TABLE}'s header names different columns, or names them in a different \
         order. The recipe reads the fields positionally, so a reordered header \
         is a table every row of which means something else."
    );

    let rows: Vec<Row> = lines
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(
                f.len(),
                COLUMNS.len(),
                "a row of {TABLE} has {} tab-separated fields, not {}. The \
                 recipe's `read` would bind the wrong text to the wrong \
                 column, and a `find` bound to an `expect` matches nothing.\n{line}",
                f.len(),
                COLUMNS.len()
            );
            for (column, value) in COLUMNS.iter().zip(&f) {
                // **No empty field, and this is not tidiness.** `unity-android-
                // negative` reads the row with `IFS=$'\\t' read`, and a tab is
                // IFS *whitespace* in bash, so runs of tabs collapse: an empty
                // column silently binds every later column to the wrong
                // variable, and the recipe then substitutes an `expect` string
                // into the source. `split('\\t')` here sees the empty field, so
                // the two disagree — measured on 2026-08-29. The `-` convention
                // for "no marker" is what avoids it today, by accident.
                assert!(
                    !value.is_empty(),
                    "a row of {TABLE} has an empty `{column}`. bash's `read` \
                     merges adjacent tabs, so that row reaches the recipe with \
                     every later column shifted one to the left. Write `-` \
                     where a column has no value.\n{line}"
                );
            }
            Row {
                name: f[0].to_string(),
                file: f[1].to_string(),
                find: f[2].to_string(),
                replace: f[3].to_string(),
                expect: f[4].to_string(),
                marker: f[5].to_string(),
            }
        })
        .collect();

    assert!(
        !rows.is_empty(),
        "{TABLE} holds no mutation. Every assertion below is stated over its \
         rows, so an empty table passes this file entirely — and \
         `unity-android-negative` would report that zero mutations proved the \
         gate has teeth."
    );
    rows
}

/// Every mutation can still be applied to the source it names.
#[test]
fn every_mutation_still_matches_its_source_exactly_once() {
    let rows = rows();
    for row in &rows {
        let path = format!("{PROBE_DIR}/{}", row.file);
        let source = read(&path);

        // Raw text, not scanned source: the recipe substitutes over the bytes,
        // so a `find` matching only inside a comment is still a `find` that
        // works. What matters here is that it matches, once.
        let hits = source.matches(&row.find).count();
        assert_eq!(
            hits, 1,
            "{}: its `find` appears {hits} time(s) in {path}, not once:\n  {}\n\
             At zero the recipe would copy an unmutated source and run a \
             POSITIVE gate, reporting that a negative control passed. Above one \
             it would change more than the row describes.",
            row.name, row.find
        );

        assert_ne!(
            row.replace, row.find,
            "{}: `replace` is `find`, so applying the row changes nothing.",
            row.name
        );
        // **The recipe substitutes with bash pattern semantics.**
        // `${content//${find}/${replace}}` treats `find` as a glob, while this
        // test and both of the recipe's own guards count literal matches with
        // `grep -oF`. A `find` carrying `*`, `?`, `[` or a backslash therefore
        // mutates text nobody counted, and the planted-replacement guard still
        // passes. No row needs one, so refusing them is cheaper than making the
        // recipe substitute literally.
        for meta in ['*', '?', '[', '\\'] {
            assert!(
                !row.find.contains(meta),
                "{}: its `find` contains `{meta}`, which bash's pattern \
                 substitution reads as a glob while every count here and in the \
                 recipe is literal. The mutation would land somewhere nobody \
                 checked.\n  {}",
                row.name,
                row.find
            );
        }

        // The row name becomes a path component under `target/`, and the recipe
        // `rm -rf`s it.
        assert!(
            row.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{}: a row name is used as a directory under target/ and that \
             directory is removed with `rm -rf`, so it is held to lowercase \
             ASCII, digits and hyphens.",
            row.name
        );

        assert!(
            !source.contains(&row.replace),
            "{}: `replace` is already in {path}, so the mutation cannot be \
             told from the healthy source. Either the mutation was committed \
             by accident, or the row now describes what the file already does.",
            row.name
        );
    }
    // **The two rows the table's prose says reach different checks**, by name.
    // Not a count — that goes stale the moment another mutation is worth having
    // — but deleting `r-e8-fat-apk` left every test here green while the APK
    // refusal R-E8's status calls its strongest evidence went back to being
    // exercised by no row.
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    for required in ["r-e8-membership", "r-e8-fat-apk"] {
        assert!(
            names.contains(&required),
            "{TABLE} has no `{required}` row. Those two are the pair its own \
             header describes: one fails inside the editor on the read-back, \
             the other lets the build run so the APK's ABI refusal is what \
             fires. Rows now: {names:?}"
        );
    }

    // Printed rather than asserted: a fixed count is a census that goes stale
    // the moment another mutation is worth having.
    println!(
        "mutations in {TABLE}: {:?}",
        rows.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
}

/// Every diagnostic a row waits for is text this repository still prints.
///
/// **The half a `find` check cannot cover.** A row whose `find` still matches
/// and whose `expect` was reworded fails the run for the right reason and is
/// reported as failing for the wrong one — which reads as a broken gate and
/// invites the control itself to be deleted.
#[test]
fn every_expected_diagnostic_is_still_emitted_somewhere() {
    let justfile = read("justfile");
    let sources: Vec<String> = ["AndroidProbeBuild.cs", "DashsceneAndroidProbe.cs"]
        .iter()
        .map(|f| read(&format!("{PROBE_DIR}/{f}")))
        .collect();

    // **Comment lines do not count as emitted.** A diagnostic reworded in an
    // `echo` or a `failures.Add` but left quoted in the comment above it would
    // otherwise satisfy this, and the row would then report that the recipe
    // "went red for some other reason" — which reads as a broken gate. Whole
    // lines only: a `#` inside `${#header}` is shell, not prose.
    let code = |text: &str, prefix: &str| -> String {
        text.lines()
            .filter(|l| !l.trim_start().starts_with(prefix))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let justfile = code(&justfile, "#");
    let sources: Vec<String> = sources.iter().map(|s| code(s, "//")).collect();

    for row in rows() {
        let emitted =
            justfile.contains(&row.expect) || sources.iter().any(|s| s.contains(&row.expect));
        assert!(
            emitted,
            "{}: nothing in the justfile or the probe sources contains its \
             `expect`:\n  {}\nSo no run can print it, and this row can only \
             ever report that the recipe failed for some other reason.",
            row.name, row.expect
        );

        if row.marker != "-" {
            assert!(
                justfile.contains(&row.marker),
                "{}: the justfile names no marker `{}`. The recipe waits for \
                 `never reported '<marker>'`, which `unity-android` composes \
                 from its own list — a marker not in that list is one the \
                 recipe never reports as missing.",
                row.name,
                row.marker
            );
        }
    }
}

/// The recipe and the seam it drives are both still there.
#[test]
fn the_recipe_reads_this_table_and_unity_android_takes_its_sources_from_it() {
    let justfile = read("justfile");

    // **Scoped to the recipe bodies, and to code rather than comments.** A
    // first version searched the whole file, and every needle it used also
    // occurred in a comment or in the other recipe — so deleting the line each
    // needle stood for left it green. Measured on 2026-08-29.
    let negative = package_gate::recipe_code(&justfile, "unity-android-negative");
    let android = package_gate::recipe_code(&justfile, "unity-android");

    assert!(
        negative.contains(TABLE),
        "`unity-android-negative` does not read {TABLE}. That table is then \
         used by nothing, and every assertion in this file is checking a \
         document no recipe reads."
    );

    // **The seam, as a parameter.** Without it the recipe would have to edit a
    // committed file to mutate the probe, which is how a mutation gets
    // committed by accident. A parameter rather than an environment variable so
    // that nothing exported in a shell can redirect an ordinary
    // `just unity-android` at uncommitted sources months later.
    assert!(
        android.contains("probe_src={{ quote(probe_src) }}"),
        "`unity-android` no longer takes its probe sources from its \
         `probe_src` parameter, so `unity-android-negative` builds the \
         COMMITTED sources and every mutation it applied is discarded — a \
         control that always passes."
    );
    assert!(
        android.contains(r#"cp "${probe_src}/AndroidProbeBuild.cs""#)
            && android.contains(r#"cp "${probe_src}/DashsceneAndroidProbe.cs""#),
        "`unity-android` reads its `probe_src` parameter and then copies \
         something else into the project, so the mutated sources reach no build."
    );
    assert!(
        negative.contains(r#"just unity-android "{{ unity_version }}" "{{ timeout }}" "${work}""#),
        "`unity-android-negative` no longer passes the mutated directory to \
         `unity-android` as its third argument, so every row builds the \
         committed probe and reports that a mutation turned the gate red."
    );
}
