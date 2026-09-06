//! The shading's `DS_HAS_*` arms, and the `#pragma` that lets a player build
//! the variant with the arm removed.
//!
//! **A `#if` with no `#pragma` behind it is a fast path nothing can take.**
//! Unity compiles the variants a shader's `multi_compile` pragmas enumerate and
//! no others, so an arm guarded by a keyword no `.shader` declares is compiled
//! in exactly one form — the form with the keyword undefined. The clip loop
//! would then be removed from every document, clipped or not, and the picture
//! would be wrong wherever a clip box exists. Nothing else here would report
//! it: `Material.EnableKeyword` on a keyword the shader does not declare
//! selects nothing, silently.
//!
//! **And the converse costs.** R-E6 keeps every variant (`KeepAll`), so a
//! keyword declared by a shader whose class cannot reach the arm doubles that
//! shader's variant set for a compile-out that removes no code. The text class
//! is the case: its arm returns before the stroke branch, so `DS_HAS_STROKES`
//! there would double the text variants and change nothing.
//!
//! **The keywords are read from the shading, not listed here.** A `DS_HAS_*`
//! arm added to `DashsceneInstance.hlsl` and named nowhere else fails
//! [`the_reach_table_names_every_keyword_the_shading_guards`] rather than
//! passing unchecked, which is the direction a gate that can be incomplete has
//! to fail in.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use package_gate::cs_scan::{blank_comments_and_strings, member_body};

/// The hand-written shading. `Sdf.hlsl` beside it is generated from the WGSL
/// and guards nothing.
const SHADING: &str = "Runtime/Shaders/DashsceneInstance.hlsl";

/// Where the keywords are spelled on the C# side.
const KEYWORDS: &str = "Runtime/KindSetKeywords.cs";

/// Where they are applied to a material.
const PAINTER: &str = "Runtime/Engine/BrgPainter.cs";

/// Which `DASHSCENE_CLASS_*` a keyword's arm is reachable from.
///
/// **A table rather than a parse of the arm's position.** Deciding "does this
/// class reach this `#if`" from the source means matching `#ifdef
/// DASHSCENE_CLASS_TEXT` blocks against brace nesting in a language this crate
/// does not parse, and a gate that parses a foreign grammar loses to the
/// grammar. The table is small, it is stated with its reason, and
/// [`the_reach_table_names_every_keyword_the_shading_guards`] fails when the
/// shading grows an arm this does not name — so what the table cannot do is
/// silently omit one.
///
/// `None` means every class; `Some(set)` means those classes and no others.
fn reach() -> BTreeMap<&'static str, Option<BTreeSet<&'static str>>> {
    let mut table: BTreeMap<&'static str, Option<BTreeSet<&'static str>>> = BTreeMap::new();
    // The clip is multiplied into every kind's coverage, on the text arm as
    // much as on the three node classes.
    table.insert("DS_HAS_CLIPS", None);
    // The text arm shades a glyph run and returns before the stroke branch, so
    // no text fragment reaches the arm this removes.
    table.insert(
        "DS_HAS_STROKES",
        Some(
            ["LIT_CUTOUT", "LIT_OPAQUE", "UNLIT_OVERLAY"]
                .into_iter()
                .collect(),
        ),
    );
    table
}

/// The `DS_HAS_*` names the shading guards an arm with.
///
/// Read from preprocessor lines alone, with any trailing comment cut off: the
/// prose in this file names both keywords, and a scan over the whole text would
/// report a paragraph as an arm.
fn guarded() -> BTreeSet<String> {
    let source = shading();
    let mut names = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        if !(line.starts_with("#if") || line.starts_with("#elif")) {
            continue;
        }
        let line = line.split("//").next().unwrap_or("");
        let mut rest = line;
        while let Some(at) = rest.find("DS_HAS_") {
            rest = &rest[at..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            names.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    names
}

fn shading() -> String {
    package_gate::hlsl_sources()
        .into_iter()
        .find(|(path, _)| path.ends_with(SHADING))
        .unwrap_or_else(|| panic!("the package no longer ships {SHADING}"))
        .1
}

/// Every shader, as (path, class, source).
fn shaders() -> Vec<(String, String, String)> {
    let sources = package_gate::shader_sources();
    assert!(
        !sources.is_empty(),
        "the package ships no .shader, so every assertion below would hold over \
         an empty set."
    );
    sources
        .into_iter()
        .map(|(path, source)| {
            let class = package_gate::declared_class(&source).unwrap_or_else(|| {
                panic!("{path} defines no DASHSCENE_CLASS_* macro, so nothing ties it to a class")
            });
            (path, class, source)
        })
        .collect()
}

/// The reach table names exactly the keywords the shading guards.
///
/// **Both directions.** A keyword added to the shading and not here would be
/// checked by nothing below; a keyword named here and no longer in the shading
/// would make every `.shader` declare a `multi_compile` for an arm that no
/// longer exists, which is variants compiled for nothing.
///
/// The non-empty assertion is the one that matters most: until this story the
/// shading guarded no arm at all, and every per-keyword loop below would have
/// run zero times and passed.
#[test]
fn the_reach_table_names_every_keyword_the_shading_guards() {
    let guarded = guarded();
    assert!(
        !guarded.is_empty(),
        "{SHADING} guards no `#if DS_HAS_…` arm. The painter then toggles \
         keywords that remove nothing, and the specialisation story #1449 adds \
         exists only in the C#."
    );

    let named: BTreeSet<String> = reach().keys().map(|k| k.to_string()).collect();
    assert_eq!(
        guarded,
        named,
        "the reach table and {SHADING} name different keyword sets. In the \
         shading and not the table: {:?}; in the table and not the shading: {:?}",
        guarded.difference(&named).collect::<Vec<_>>(),
        named.difference(&guarded).collect::<Vec<_>>(),
    );
}

/// Every shader whose class reaches an arm declares the keyword that removes
/// it, in every one of its programs — and no other shader declares it.
///
/// **Per program, not per file**, for the reason `shader_pragmas.rs` states
/// about `DOTS_INSTANCING_ON`: a `#pragma` in one pass does not reach another.
///
/// **`multi_compile_local`, not `multi_compile`.** A global keyword takes a
/// slot in Unity's process-wide budget, and nothing here varies per anything
/// but a material.
#[test]
fn every_class_that_reaches_an_arm_declares_the_keyword_that_removes_it() {
    let table = reach();
    let shaders = shaders();

    for (keyword, classes) in &table {
        let pragma = format!("#pragma multi_compile_local _ {keyword}");
        let mut declared_by = 0usize;

        for (path, class, source) in &shaders {
            let reaches = classes
                .as_ref()
                .is_none_or(|set| set.contains(class.as_str()));

            let programs = package_gate::hlsl_programs(source);
            assert!(
                !programs.is_empty(),
                "{path} holds no HLSLPROGRAM block, so this check would hold \
                 that file over an empty set."
            );

            for (at, body) in programs {
                let present = body.lines().any(|line| line.trim() == pragma);
                if reaches {
                    assert!(
                        present,
                        "{path}: the HLSLPROGRAM at byte {at} does not declare \
                         `{pragma}`, and DASHSCENE_CLASS_{class} reaches the \
                         `{keyword}` arm. Unity then builds only the variant \
                         with `{keyword}` undefined, so the arm is removed from \
                         every document — a wrong picture rather than a failure, \
                         because `Material.EnableKeyword` on an undeclared \
                         keyword selects nothing and says nothing."
                    );
                    declared_by += 1;
                } else {
                    assert!(
                        !present,
                        "{path}: the HLSLPROGRAM at byte {at} declares \
                         `{pragma}`, and DASHSCENE_CLASS_{class} cannot reach \
                         the `{keyword}` arm. R-E6 keeps every variant, so this \
                         doubles the variant set this shader compiles for a \
                         removal that removes no code."
                    );
                }
            }
        }

        assert!(
            declared_by > 0,
            "no shader declares `{pragma}`, so the `{keyword}` arm is compiled \
             in one form on every class."
        );
    }
}

/// The keywords the C# toggles are the keywords the shading guards.
///
/// **Neither half is checked by the other.** The shading compiles whatever the
/// pragmas enumerate whether or not a painter ever enables one, and the painter
/// enables whatever string it holds whether or not a shader declares it — so a
/// rename on one side is a fast path that is never selected, with both halves
/// green.
#[test]
fn the_painter_spells_the_keywords_the_shading_guards() {
    let raw = package_gate::package_cs_files();
    let source = &raw
        .iter()
        .find(|(path, _)| path.ends_with(KEYWORDS))
        .unwrap_or_else(|| panic!("the package no longer ships {KEYWORDS}"))
        .1;

    for keyword in guarded() {
        assert!(
            source.contains(&format!("\"{keyword}\"")),
            "{KEYWORDS} does not spell `{keyword}`, which {SHADING} guards an \
             arm with. The painter cannot enable a keyword it cannot name, so \
             that arm is removed from every document."
        );
    }
}

/// `KeywordsFor` is a pure static, and it lives where a check without Unity can
/// execute it.
///
/// **The storage class is the point.** `unity/ffi-check` compiles
/// `Runtime/**` and excludes `Runtime/Engine/**` — every file there references
/// `UnityEngine`, which that project has no reference assemblies for — so a
/// `KeywordsFor` on `BrgPainter` could be read as text here and executed by
/// nothing. The mapping from bits to keywords is the half of this story that
/// can be wrong without a device, and `unity/ffi-check` is what drives it.
#[test]
fn keywords_for_is_a_pure_static_outside_the_engine_half() {
    let raw = package_gate::package_cs_files();
    let (_, source) = raw
        .iter()
        .find(|(path, _)| path.ends_with(KEYWORDS))
        .unwrap_or_else(|| panic!("the package no longer ships {KEYWORDS}"));

    let scanned = blank_comments_and_strings(source);
    let squeezed = package_gate::cs_scan::squeeze(&scanned);
    assert!(
        squeezed.contains("public static string[] KeywordsFor(uint bits)"),
        "{KEYWORDS} does not declare `public static string[] KeywordsFor(uint \
         bits)`. `unity/ffi-check` reaches it as a static on a type it \
         compiles; an instance member, or one on a type in Runtime/Engine/, is \
         a mapping no check without a Unity editor can execute."
    );

    // **Nothing UnityEngine, which is what keeps it compilable there.** A
    // `using UnityEngine;` added to this file removes it from `ffi-check`'s
    // compile set by failing that compile, which reads as an unrelated break.
    assert!(
        !scanned.contains("UnityEngine"),
        "{KEYWORDS} names UnityEngine. `unity/ffi-check` compiles this file \
         with no Unity reference assemblies, so the whole gate stops building."
    );

    // And the painter reaches the mapping rather than carrying a second one.
    let painter = raw
        .iter()
        .find(|(path, _)| path.ends_with(PAINTER))
        .unwrap_or_else(|| panic!("the package no longer ships {PAINTER}"))
        .1
        .as_str();
    let painter = blank_comments_and_strings(painter);
    let (start, end) = member_body(&painter, "private void ApplyKindSet(uint bits)");
    assert!(
        painter[start..=end].contains("KindSetKeywords.KeywordsFor("),
        "{PAINTER}'s `ApplyKindSet` does not call \
         `KindSetKeywords.KeywordsFor`. A second mapping in the engine half is \
         one `unity/ffi-check` cannot execute, so the bits it applies would be \
         checked by nothing."
    );
}
