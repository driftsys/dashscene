//! The procedural variant the two blended classes draw through, and the editor
//! gate that compiles it.
//!
//! Story #1448 draws `UnlitOverlay` and `Text` with `DrawProcedural` in
//! document order rather than through the BatchRendererGroup. Those draws
//! select the `DASHSCENE_PROCEDURAL` variant of the same two shaders, so no
//! shader file is added. Three facts have to hold together:
//!
//! - **Both blended shaders declare the keyword**, in every program. A material
//!   that enables a keyword its shader does not declare selects nothing and
//!   says nothing, so a missing pragma draws the BRG variant with no mesh bound.
//! - **The lit shaders do not.** R-E6 keeps every variant, so a declaration on
//!   a class that never draws procedurally doubles its variant set for nothing.
//! - **`just unity-editor` compiles the variant.** Unity compiles variants
//!   lazily and `unity/package-gate` compiles nothing, so without an explicit
//!   `CompileVariant` for the keyword no gate in this repository builds the
//!   code path the ordered draws use. That is the trap
//!   `DashsceneEditorCompat.cs` records for `DOTS_INSTANCING_ON`.

use std::collections::BTreeSet;

const KEYWORD: &str = "DASHSCENE_PROCEDURAL";
const GATE: &str = "unity/editor-compat/DashsceneEditorCompat.cs";

/// The classes whose shaders compile the procedural variant.
const PROCEDURAL_CLASSES: [&str; 2] = ["TEXT", "UNLIT_OVERLAY"];

fn read(path: &str) -> String {
    let full = package_gate::root().join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

/// Every program of a blended shader declares the local keyword, and no
/// program of a lit shader names it in any pragma.
#[test]
fn the_blended_shaders_declare_the_procedural_variant_and_the_lit_shaders_do_not() {
    let pragma = format!("#pragma multi_compile_local _ {KEYWORD}");
    let sources = package_gate::shader_sources();
    assert!(
        !sources.is_empty(),
        "the package ships no .shader, so this check would hold over an empty set."
    );

    let mut declaring = BTreeSet::new();
    for (path, source) in &sources {
        let class = package_gate::declared_class(source)
            .unwrap_or_else(|| panic!("{path} defines no DASHSCENE_CLASS_* macro"));
        let procedural = PROCEDURAL_CLASSES.contains(&class.as_str());

        let programs = package_gate::hlsl_programs(source);
        assert!(
            !programs.is_empty(),
            "{path} holds no HLSLPROGRAM block, so this check would hold that \
             file over an empty set."
        );
        for (at, body) in programs {
            let pragma_lines: Vec<&str> = body
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("#pragma") && line.contains(KEYWORD))
                .collect();
            if procedural {
                assert_eq!(
                    pragma_lines,
                    vec![pragma.as_str()],
                    "{path}: the HLSLPROGRAM at byte {at} must declare `{pragma}` \
                     exactly once. DASHSCENE_CLASS_{class} draws through \
                     DrawProcedural, and without the variant the material's \
                     keyword selects nothing."
                );
                declaring.insert(class.clone());
            } else {
                assert!(
                    pragma_lines.is_empty(),
                    "{path}: the HLSLPROGRAM at byte {at} names `{KEYWORD}` in \
                     {pragma_lines:?}. DASHSCENE_CLASS_{class} stays on the \
                     BatchRendererGroup, and R-E6 keeps every variant, so the \
                     declaration doubles its variant set for nothing."
                );
            }
        }
    }

    let expected: BTreeSet<String> = PROCEDURAL_CLASSES.iter().map(|c| c.to_string()).collect();
    assert_eq!(
        declaring, expected,
        "the shaders declaring `{pragma}` are not exactly the two blended \
         classes. A class missing here is one with no shader file on disk."
    );
}

/// `just unity-editor` compiles the procedural keyword set, for the shaders that
/// declare it, and fails when a declaring shader yields no evidence.
///
/// **What the editor run itself proves**, measured in this story's review fix:
/// with `.TrimStart()` removed from the declaration scan it fails naming an
/// empty declaring set, and with the `DOTS_INSTANCING_ON` + procedural set
/// removed from the compile list it fails naming that set for both shaders.
/// This scan pins that each of the three refusals is still there to run: its
/// condition and the `failures.Add(` it guards, which is what makes the editor
/// run exit non-zero. It does not run the editor, so it cannot say that the
/// condition is true when it should be.
#[test]
fn the_editor_gate_compiles_the_procedural_variant() {
    let raw = read(GATE);
    // The keyword is a string literal, which the scan below blanks, so its
    // spelling is read with comments removed and string bodies kept.
    let literals = package_gate::cs_scan::blank_comments_only(&raw);
    assert!(
        literals.contains(&format!("const string ProceduralKeyword = \"{KEYWORD}\";")),
        "{GATE} does not spell `{KEYWORD}` as the `ProceduralKeyword` constant \
         outside its comments."
    );
    let squeezed_literals = package_gate::cs_scan::squeeze(&literals);
    for (needle, why) in [
        (
            "ProceduralShaders = { \"Text.shader\", \"UnlitOverlay.shader\" };",
            "the shaders that must declare the keyword are not the two blended \
             classes, by file name",
        ),
        (
            "RequiredProceduralSets = { \"DASHSCENE_PROCEDURAL\", \
             \"DOTS_INSTANCING_ON + DASHSCENE_PROCEDURAL\", };",
            "the keyword sets each declaring shader must yield bytes for are \
             not the keyword alone and with DOTS_INSTANCING_ON",
        ),
    ] {
        assert!(
            squeezed_literals.contains(needle),
            "{GATE} does not contain `{needle}` outside its comments: {why}."
        );
    }

    // Scanned, so that prose naming these tokens does not satisfy them.
    let gate =
        package_gate::cs_scan::squeeze(&package_gate::cs_scan::blank_comments_and_strings(&raw));
    for (needle, why) in [
        (
            "new[] { ProceduralKeyword }",
            "no variant is compiled with the procedural keyword set",
        ),
        (
            ".Contains(ProceduralKeyword)",
            "the gate does not decide from the shader source which shaders \
             declare the keyword",
        ),
        (
            "if (proceduralVariants == 0) { failures.Add(",
            "a shader that declares the keyword and yields no compiled \
             procedural variant does not fail the gate",
        ),
        (
            "foreach (var set in RequiredProceduralSets) { if (!proceduralBySet.TryGetValue(set, out var compiled) || compiled == 0) { failures.Add(",
            "a declaring shader that yields no variant for one of the two \
             keyword sets does not fail the gate",
        ),
        (
            "declaringShaders.Sort(StringComparer.Ordinal); if (!declaringShaders.SequenceEqual(ProceduralShaders)) { failures.Add(",
            "a declaration scan that found no shader, or the wrong ones, does \
             not fail the gate",
        ),
    ] {
        assert!(
            gate.contains(needle),
            "{GATE} does not contain `{needle}` outside its comments and \
             strings: {why}."
        );
    }
}
