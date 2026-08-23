//! R-T5's mechanism, held to its source.
//!
//! `docs/specification/03-target-hardware-rules.md` R-T5 asks for the SDF
//! shader math to be single-sourced into both product painters' shading
//! languages. The mechanism is generation, and this is what makes it one: the
//! committed HLSL is re-derived here on every test run and compared byte for
//! byte against the file in the tree.
//!
//! **A hand edit to the generated file is what these tests exist to catch.** It
//! is the failure no review finds, because an edited file still compiles, still
//! draws, and is no longer the arithmetic the other painter evaluates.

use std::fs;

/// The whole of R-T5's mechanism: what is committed is what the WGSL produces.
#[test]
fn the_committed_hlsl_is_what_the_wgsl_compiles_to() {
    let root = package_gate::root();
    let wgsl = fs::read_to_string(root.join(package_gate::WGSL_PATH)).expect("the WGSL library");
    let generated = package_gate::generate_hlsl(&wgsl).expect("the WGSL compiles to HLSL");
    let committed = fs::read_to_string(root.join(package_gate::HLSL_PATH)).unwrap_or_else(|e| {
        panic!(
            "{} is missing ({e}). Run `just sdf-hlsl`.",
            package_gate::HLSL_PATH
        )
    });

    // **Not `assert_eq!` on the two strings.** This file is nine kilobytes,
    // and the standard comparison prints both of them in full — twenty
    // kilobytes of escaped source for a one-line difference, which buries the
    // line that actually moved. Reporting the first differing line instead is
    // what makes the failure readable, and the length line below is what keeps
    // it honest when the difference is a truncation rather than an edit.
    if committed != generated {
        let at = committed
            .lines()
            .zip(generated.lines())
            .position(|(a, b)| a != b);
        let detail = match at {
            Some(line) => format!(
                "first difference at line {}:\n  committed: {}\n  generated: {}",
                line + 1,
                committed.lines().nth(line).unwrap_or(""),
                generated.lines().nth(line).unwrap_or("")
            ),
            None => format!(
                "one is a prefix of the other: committed has {} lines, \
                 generated has {}",
                committed.lines().count(),
                generated.lines().count()
            ),
        };
        panic!(
            "{} is not what {} compiles to. Either the WGSL changed and the \
             HLSL was not regenerated, or the HLSL was edited by hand. Run \
             `just sdf-hlsl`; do not edit the HLSL.\n{detail}",
            package_gate::HLSL_PATH,
            package_gate::WGSL_PATH
        );
    }
}

/// Every function the WGSL declares survives into the HLSL.
///
/// **Without this the byte comparison above is satisfiable by two empty
/// files.** naga writes an empty module without complaint, so a WGSL emptied by
/// a bad merge would regenerate to a banner and nothing else, and the
/// comparison would agree with itself.
///
/// **The names are read out of the WGSL rather than listed here**, which is the
/// difference between checking the seventeen functions that existed when this
/// was written and checking the library. A function added to `sdf.wgsl` and not
/// translated fails here; a hand-written list would have gone quietly stale.
/// This crate has already been caught on the wrong side of that distinction
/// once, in `runtime_split.rs`.
///
/// The comparison allows a trailing underscore because that is what naga's
/// namer appends — for a name ending in a digit, or colliding with an HLSL
/// keyword. It is deliberately not a re-implementation of the rule: the test
/// below pins the two instances that fire today, and this one only requires
/// that each function arrives under one of the two spellings.
#[test]
fn every_function_the_wgsl_declares_survives_into_the_hlsl() {
    let root = package_gate::root();
    let wgsl = fs::read_to_string(root.join(package_gate::WGSL_PATH)).expect("the WGSL library");
    let committed = fs::read_to_string(root.join(package_gate::HLSL_PATH)).expect("the HLSL");

    let names = package_gate::wgsl_function_names(&wgsl);

    // **A floor, not a census — and it has to be near the population to mean
    // anything.** A first version said `>= 10` against a library of seventeen,
    // so deleting seven functions and regenerating passed both this and the
    // byte comparison. The number below is what the library carries; raising
    // it when a function is added is the point, and lowering it is a decision
    // someone has to write down.
    const DECLARED_TODAY: usize = 17;
    assert!(
        names.len() >= DECLARED_TODAY,
        "the WGSL shader library declares {} top-level functions and this gate \
         expects at least {DECLARED_TODAY}. A function was removed — say so \
         here deliberately — or the way they are declared changed and this gate \
         can no longer read them.",
        names.len()
    );

    for name in &names {
        assert!(
            committed.contains(&format!("{name}(")) || committed.contains(&format!("{name}_(")),
            "the generated HLSL has no `{name}` and no `{name}_`. The WGSL \
             declares it, so either naga renamed it by a rule beyond the \
             trailing underscore — the `gen_` prefix rule is the one that would \
             do it — or it did not translate at all."
        );
    }
}

/// The two names naga's namer moves, pinned as the checked instance.
///
/// The test above requires each function to arrive under one of two spellings
/// and does not say which. This says which, for the two that move today, so a
/// reader of the generated file has the rows and
/// `docs/design/unity-csharp-host.md` has the rule.
#[test]
fn the_two_renamed_symbols_are_the_ones_the_record_names() {
    let root = package_gate::root();
    let committed = fs::read_to_string(root.join(package_gate::HLSL_PATH)).expect("the HLSL");

    // `median3` ends with a digit; `sample` is an HLSL keyword. Different
    // clauses of one condition in `naga::proc::Namer::call`.
    assert!(
        committed.contains("float median3_("),
        "the generated HLSL has no `median3_`. naga renames an identifier \
         ending in a digit; if that stopped happening, every consumer keyed to \
         the renamed spelling is now wrong."
    );
    assert!(
        committed.contains("float msdf_coverage(float3 sample_,"),
        "the generated HLSL's `msdf_coverage` does not take `sample_`. naga \
         renames an identifier colliding with an HLSL keyword."
    );
}

/// The file this crate compiles is the file the lean painter includes.
///
/// This crate reads the WGSL off disk rather than through
/// `dashscene_gpu::SDF_WGSL`, so that it does not pull wgpu into a gate that
/// belongs in the sanity tier. That leaves one thing to assert: the path it
/// reads is the path that constant is `include_str!`'d from. Without it, the
/// two could name different files and every test above would still pass —
/// against a shader library no painter uses.
#[test]
fn the_path_this_gate_reads_is_the_one_dashscene_gpu_includes() {
    let root = package_gate::root();
    let shader_rs =
        fs::read_to_string(root.join(package_gate::WGSL_IS_THE_CRATE_S_OWN)).expect("shader.rs");

    // `shader.rs` sits in `crates/dashscene-gpu/src/`, so its `include_str!`
    // argument is `WGSL_PATH` with that prefix removed.
    let relative = package_gate::WGSL_PATH
        .strip_prefix("crates/dashscene-gpu/src/")
        .expect("the WGSL path is under dashscene-gpu's src/");
    let expected = format!("include_str!(\"{relative}\")");

    assert!(
        shader_rs.contains(&expected),
        "{} does not contain `{expected}`. This crate and the lean painter \
         would then be reading different shader libraries, and every other \
         test here would pass while R-T5 was false.",
        package_gate::WGSL_IS_THE_CRATE_S_OWN
    );
}

/// The shading that draws uses the generated file, rather than a copy of it.
///
/// **This is the assertion R-T5 actually needs, and the byte comparison above
/// is not it.** Everything above proves `Sdf.hlsl` is what the WGSL compiles
/// to. None of it proves anything *includes* it: deleting the `#include` from
/// `DashsceneInstance.hlsl` and pasting hand-written copies of
/// `rounded_box_sdf`, `coverage`, `stroke_coverage` and the gradient functions
/// in its place leaves the generated file committed, byte-identical and unread,
/// with R-T5's property false and every gate green.
///
/// So: the shading must include it, and must define none of the names it
/// provides.
#[test]
fn the_shading_includes_the_generated_file_and_redefines_nothing_in_it() {
    let root = package_gate::root();
    let wgsl = fs::read_to_string(root.join(package_gate::WGSL_PATH)).expect("the WGSL library");
    let names = package_gate::wgsl_function_names(&wgsl);

    let generated = std::path::Path::new(package_gate::HLSL_PATH)
        .file_name()
        .expect("the generated file has a name")
        .to_string_lossy()
        .into_owned();

    let sources = package_gate::hlsl_sources();
    let including: Vec<&String> = sources
        .iter()
        .filter(|(_, source)| source.contains(&format!("#include \"{generated}\"")))
        .map(|(path, _)| path)
        .collect();

    assert!(
        !including.is_empty(),
        "no .hlsl the package ships includes `{generated}`. The generated \
         file is then committed and unread, and whatever the shaders do shade \
         with is not the WGSL — which is exactly what R-T5 forbids."
    );

    for (path, source) in &sources {
        if path.ends_with(&generated) {
            continue;
        }
        for name in &names {
            // A definition, not a call: `float name(` at the start of a line is
            // how the generated file declares them and how a hand port would.
            let defined = source.lines().any(|line| {
                !line.starts_with("//")
                    && line.contains(&format!(" {name}("))
                    && line.ends_with(')')
            });
            assert!(
                !defined,
                "{path} appears to define `{name}`, which the generated \
                 {generated} already provides. A second definition is a hand \
                 port living beside the generated file, and R-T5 asks for one \
                 source rather than two that agree today."
            );
        }
    }
}

/// The heap layout constants agree across all three places that state them.
///
/// `paint.wgsl` declares `GRADIENT_WORDS`, the C# packer declares
/// `GradientWords`, and the Unity shading declares `DS_GRADIENT_WORDS`. **Three
/// copies of one number**, and the C# and the HLSL both carried a comment
/// saying this crate held them together while it did not: setting
/// `PaintHeap.GradientWords` to 11 made the packer stride the heap by eleven
/// words while the shader read at twelve, and every test passed.
#[test]
fn the_heap_row_widths_agree_across_the_three_places_that_state_them() {
    let root = package_gate::root();
    let wgsl = fs::read_to_string(root.join("crates/dashscene-gpu/src/shaders/paint.wgsl"))
        .expect("the lean painter's paint.wgsl");
    let hlsl = fs::read_to_string(
        root.join(package_gate::PACKAGE_PATH)
            .join("Runtime/Shaders/DashsceneInstance.hlsl"),
    )
    .expect("the Unity shading");
    let cs_files = package_gate::package_cs_files();
    let (_, heap) = cs_files
        .iter()
        .find(|(path, _)| path.ends_with("Runtime/PaintHeap.cs"))
        .expect("Runtime/PaintHeap.cs");

    // The gradient row is the one all three state. The clip and stroke rows are
    // stated by the C# and the HLSL; `paint.wgsl` expresses them as structs
    // rather than as a word count, so they are held to each other only.
    let wgsl_gradient = package_gate::wgsl_const_u32(&wgsl, "GRADIENT_WORDS")
        .expect("paint.wgsl declares GRADIENT_WORDS");
    let cs_gradient = package_gate::cs_const_int(heap, "GradientWords")
        .expect("PaintHeap.cs declares GradientWords");
    let hlsl_gradient = package_gate::hlsl_define_u32(&hlsl, "DS_GRADIENT_WORDS")
        .expect("the shading defines DS_GRADIENT_WORDS");

    assert_eq!(
        wgsl_gradient, cs_gradient,
        "paint.wgsl's GRADIENT_WORDS is {wgsl_gradient} and PaintHeap.cs's \
         GradientWords is {cs_gradient}. The packer would stride the heap by a \
         width the lean painter does not use, and the two painters are stated \
         over the same rows or over nothing."
    );
    assert_eq!(
        cs_gradient, hlsl_gradient,
        "PaintHeap.cs's GradientWords is {cs_gradient} and the shading's \
         DS_GRADIENT_WORDS is {hlsl_gradient}. The packer writes one stride \
         and the fragment stage reads another, so every gradient past the \
         first shades from the previous row's handles."
    );

    // **The stroke row's FIELD ORDER, which no width can see.** The fix this
    // pins moved the colour ahead of `(width, align)` to match `paint.wgsl`'s
    // `struct Stroke`. Reverting it in both the packer and the shader leaves
    // every row width unchanged, so the width assertions here pass over the
    // exact divergence they were written for — two painters agreeing with each
    // other and not with the row, which is what issue #828 exists to catch.
    let fields = package_gate::wgsl_struct_fields(&wgsl, "Stroke")
        .expect("paint.wgsl declares struct Stroke");
    assert_eq!(
        fields.first().map(String::as_str),
        Some("color"),
        "paint.wgsl's struct Stroke no longer begins with `color` — it declares \
         {fields:?}. The Unity packer writes the colour into word 0 to match \
         it, so this gate and that packer now disagree about the row."
    );
    assert!(
        hlsl.contains("colour = _DsStrokes[row * DS_STROKE_WORDS];"),
        "the Unity shading does not read the stroke colour from word 0. \
         `paint.wgsl`'s `struct Stroke` puts `color` first, so a shading that \
         reads word 1 is stated over a different row than the lean painter — \
         and every row WIDTH is identical either way."
    );
    assert!(
        hlsl.contains("float4 params = _DsStrokes[row * DS_STROKE_WORDS + 1u];"),
        "the Unity shading does not read the stroke's width and alignment from \
         word 1 of the row."
    );

    // The rows the C# and the HLSL state and `paint.wgsl` does not — it
    // expresses them as structs rather than as a word count, so these are held
    // to each other only. **`GlyphWords` has no third copy at all**: the lean
    // painter's glyph-run row carries a residency rectangle this painter has no
    // equivalent for, so the two rows are deliberately different widths and
    // `PaintHeap.GlyphWords` says why.
    for (cs_name, hlsl_name) in [
        ("ClipWords", "DS_CLIP_WORDS"),
        ("StrokeWords", "DS_STROKE_WORDS"),
        ("GlyphWords", "DS_GLYPH_WORDS"),
    ] {
        let a = package_gate::cs_const_int(heap, cs_name)
            .unwrap_or_else(|| panic!("PaintHeap.cs declares {cs_name}"));
        let b = package_gate::hlsl_define_u32(&hlsl, hlsl_name)
            .unwrap_or_else(|| panic!("the shading defines {hlsl_name}"));
        assert_eq!(
            a, b,
            "PaintHeap.cs's {cs_name} is {a} and the shading's {hlsl_name} is {b}"
        );
    }

    // **The kind tags, which decide which arm of the shader an instance takes
    // and are silent when they disagree.** `_DsPaint.x` is written from
    // `PaintKindTag` and branched on against `DS_KIND_*`: a glyph instance
    // whose tag the shader's text arm does not match falls through to the fill
    // arm, which reads `_DsCorners` as corner radii — where a glyph holds atlas
    // texels — and indexes the paint heap with a glyph-run row. Nothing else
    // compares the two, and every gate stays green.
    let kinds = [
        ("FillSolid", "DS_KIND_FILL_SOLID"),
        ("FillGradient", "DS_KIND_FILL_GRADIENT"),
        ("Stroke", "DS_KIND_STROKE"),
        ("Text", "DS_KIND_TEXT"),
    ];
    for (member, define) in kinds {
        let cs = package_gate::cs_enum_value(heap, "PaintKindTag", member)
            .unwrap_or_else(|| panic!("PaintHeap.cs declares PaintKindTag.{member}"));
        let hlsl_value = package_gate::hlsl_define_u32(&hlsl, define)
            .unwrap_or_else(|| panic!("the shading defines {define}"));
        assert_eq!(
            cs, hlsl_value,
            "PaintKindTag.{member} is {cs} and the shading's {define} is {hlsl_value}. The \
             packer writes one tag and the fragment stage branches on another, so that \
             kind takes the wrong arm and shades a plausible wrong picture."
        );
    }
    // **The NAMES, not the count**, and every member must carry an explicit
    // value. A count comparison passes over a member added without one — the
    // helper reports those as `None` for exactly this reason — and over a
    // rename that keeps the arity.
    let members = package_gate::cs_enum_members(heap, "PaintKindTag");
    for (member, value) in &members {
        assert!(
            value.is_some(),
            "PaintKindTag.{member} carries no explicit value, so what it is depends on \
             where it sits in the declaration and nothing holds it to a shader define."
        );
        assert!(
            kinds.iter().any(|(named, _)| named == member),
            "PaintKindTag.{member} is named by no entry in this list, so it is held to no \
             DS_KIND_* define at all — an instance carrying it takes whichever arm of the \
             shading matches by accident."
        );
    }
    for (member, _) in kinds {
        assert!(
            members.iter().any(|(name, _)| name == member),
            "this list names PaintKindTag.{member}, which the enum does not declare."
        );
    }

    // The stop count is the bound the packer clamps to and the shader clamps
    // to; they come from the C# and from the GENERATED file, so this holds the
    // packer to naga's output rather than to a second literal.
    let generated = fs::read_to_string(root.join(package_gate::HLSL_PATH)).expect("the HLSL");
    let cs_stops = package_gate::cs_const_int(heap, "MaxGradientStops")
        .expect("PaintHeap.cs declares MaxGradientStops");
    let shader_stops = package_gate::hlsl_static_const_u32(&generated, "MAX_GRADIENT_STOPS")
        .expect("the generated HLSL declares MAX_GRADIENT_STOPS");
    assert_eq!(
        cs_stops, shader_stops,
        "PaintHeap.cs clamps a gradient to {cs_stops} stops and the generated \
         shader library reads {shader_stops}. The packer would write past the \
         offset slots into the colour slots, or leave slots the shader reads \
         unwritten."
    );
}
