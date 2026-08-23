//! R-E11 and R-E12, over the shaders the painter registers.
//!
//! `docs/specification/07-embedding-and-distribution.md` requires
//! `#pragma target 4.5` or higher and `#pragma multi_compile _
//! DOTS_INSTANCING_ON` on every shader the painter passes to
//! `BatchRendererGroup.RegisterMaterial`, and **both requirements state that
//! the check must assert the set is not empty** — because until story #1122 no
//! shader existed under `unity/` and a check that only grepped would pass
//! having read nothing.
//!
//! Two sets have to agree for that non-empty assertion to mean anything: the
//! shaders on disk, and the names the C# registers. A shader nothing registers
//! would satisfy every pragma while the painter drew with something else.

/// The set is not empty, in both directions, and the two agree.
#[test]
fn every_shader_the_painter_names_exists_and_every_shader_is_named() {
    let sources = package_gate::shader_sources();
    assert!(
        !sources.is_empty(),
        "no .shader file under the package's Runtime/Shaders/. R-E11 and R-E12 \
         would then be met over an empty set, which is what their own text \
         forbids."
    );

    let registered = package_gate::registered_shader_names();
    assert!(
        !registered.is_empty(),
        "the package's C# names no shader. The painter registers materials by \
         name, so a gate over the files alone would pass while nothing drew."
    );

    let mut declared: Vec<String> = sources
        .iter()
        .map(|(path, source)| {
            package_gate::declared_shader_name(source)
                .unwrap_or_else(|| panic!("{path} declares no `Shader \"…\"` name"))
        })
        .collect();
    declared.sort();

    assert_eq!(
        declared, registered,
        "the shaders on disk and the names the C# registers are different \
         sets. A name the C# holds and no file declares is a null from \
         `Shader.Find`; a file no name reaches is a shader that passes every \
         pragma check and draws nothing."
    );
}

/// R-E11: every program declares `#pragma target 4.5` or higher.
#[test]
fn every_shader_program_declares_target_4_5_or_higher() {
    for (path, source) in package_gate::shader_sources() {
        let programs = package_gate::hlsl_programs(&source);
        assert!(
            !programs.is_empty(),
            "{path} holds no HLSLPROGRAM block, so R-E11 would be met over an \
             empty set for this file."
        );

        for (at, body) in programs {
            let target = body
                .lines()
                .filter_map(|line| line.trim().strip_prefix("#pragma target "))
                .map(|rest| rest.trim().to_string())
                .next()
                .unwrap_or_else(|| {
                    panic!("{path}: the HLSLPROGRAM at byte {at} declares no `#pragma target`")
                });

            let level: f32 = target
                .parse()
                .unwrap_or_else(|_| panic!("{path}: `#pragma target {target}` is not a number"));
            assert!(
                level >= 4.5,
                "{path}: `#pragma target {target}` is below 4.5, which is what \
                 BatchRendererGroup needs (R-E11)."
            );
        }
    }
}

/// R-E12: every program declares the DOTS instancing variant.
///
/// **Per program, not per file.** A `#pragma` in one pass does not reach
/// another, so a shader carrying three passes needs it three times — and a
/// check that read the file as one string would pass on a pragma present in
/// only the first.
#[test]
fn every_shader_program_declares_the_dots_instancing_variant() {
    const PRAGMA: &str = "#pragma multi_compile _ DOTS_INSTANCING_ON";

    for (path, source) in package_gate::shader_sources() {
        let programs = package_gate::hlsl_programs(&source);
        assert!(
            !programs.is_empty(),
            "{path} holds no HLSLPROGRAM block, so R-E12 would be met over an \
             empty set for this file."
        );

        for (at, body) in programs {
            assert!(
                body.lines().any(|line| line.trim() == PRAGMA),
                "{path}: the HLSLPROGRAM at byte {at} does not declare \
                 `{PRAGMA}`. Unity refuses a BatchRendererGroup pass without \
                 the variant, naming it (R-E12)."
            );
        }
    }
}

/// No shader smuggles a program past the two checks above.
///
/// `hlsl_programs` finds `HLSLPROGRAM`…`ENDHLSL` blocks, which is what a
/// Scriptable Render Pipeline shader uses. A `CGPROGRAM` block is a program
/// too, and it would be invisible to both checks above — they would report
/// zero programs, and the emptiness assertion inside each is what catches it.
/// This says so directly, so the failure names the reason.
#[test]
fn no_shader_uses_a_cgprogram_block() {
    for (path, source) in package_gate::shader_sources() {
        assert!(
            !source.contains("CGPROGRAM"),
            "{path} holds a CGPROGRAM block. R-E4 requires a Scriptable Render \
             Pipeline, whose shaders use HLSLPROGRAM — and the pragma checks \
             here read HLSLPROGRAM blocks, so a CGPROGRAM program would be \
             checked by nothing."
        );
    }
}

/// Every per-instance property the C# names is declared by every shader.
///
/// **A BatchRendererGroup binds a property by name**, through
/// `Shader.PropertyToID`. A name that exists on one side and not the other is
/// not a compile error and not a run-time error: the shader reads the
/// property's default and draws a plausible wrong picture. Both directions,
/// because either half missing is that same silent failure.
#[test]
fn the_per_instance_properties_are_the_same_set_on_both_sides() {
    let cs = package_gate::package_cs_files();
    let names = package_gate::instanced_property_names(&cs);
    let other = package_gate::other_bound_names(&cs);
    assert!(
        !names.is_empty(),
        "the package's C# declares no `_Ds…` property name. The painter binds \
         per-instance data by name, so a gate over the shaders alone would \
         pass while every instance read a default."
    );

    for (path, source) in package_gate::shader_sources() {
        let block = package_gate::properties_block(&source)
            .unwrap_or_else(|| panic!("{path} has no `Properties` block"));
        let declared = package_gate::ds_property_names(&block);

        for name in &names {
            assert!(
                declared.contains(name),
                "{path}: the `Properties` block does not declare `{name}`, \
                 which the package's C# binds per instance. Unity resolves \
                 that name to the property's default and draws a wrong \
                 picture rather than failing."
            );
        }

        for name in &declared {
            assert!(
                names.contains(name) || other.contains(name),
                "{path}: the `Properties` block declares `{name}`, which the \
                 package's C# names neither as a per-instance property \
                 (Runtime/PaintProperties.cs) nor as a global or per-material \
                 one (Runtime/PaintBindings.cs). Either the packer stopped \
                 writing it or the shader kept a property that moved."
            );
        }
    }
}

/// Each material class draws with **its own** shader, not merely with one of
/// the package's shaders.
///
/// **The set comparison above cannot see a swap.** `declared` and `registered`
/// are both sorted, so exchanging which C# constant holds which shader name
/// leaves them identical — and the consequence is not a failure but a wrong
/// picture: a lit-cutout painter drawing through the non-blending opaque shader
/// puts square corners where a pill was authored, with `FramePacker`'s refusal
/// never firing, because that refusal is gated on the material class rather
/// than on the shader actually bound.
///
/// The tie is the `DASHSCENE_CLASS_*` macro each shader defines. The C# member
/// name and the macro are the same words in two spellings, so no table has to
/// map them.
#[test]
fn each_material_class_draws_with_the_shader_that_declares_it() {
    let consts = package_gate::shader_consts();
    assert!(
        !consts.is_empty(),
        "Runtime/PaintHeap.cs declares no `Dashscene/…` shader constant."
    );

    let sources = package_gate::shader_sources();
    for (member, shader_name) in &consts {
        let (path, source) = sources
            .iter()
            .find(|(_, source)| {
                package_gate::declared_shader_name(source).as_deref() == Some(shader_name)
            })
            .unwrap_or_else(|| {
                panic!("no .shader declares `Shader \"{shader_name}\"`, which PaintShaders.{member} names")
            });

        let declared = package_gate::declared_class(source).unwrap_or_else(|| {
            panic!("{path} defines no DASHSCENE_CLASS_* macro, so nothing ties it to a class")
        });
        let expected = package_gate::screaming_snake(member);
        assert_eq!(
            declared, expected,
            "PaintShaders.{member} names `{shader_name}`, whose file {path} \
             defines DASHSCENE_CLASS_{declared} rather than \
             DASHSCENE_CLASS_{expected}. The two constants are exchanged: the \
             class would draw through the other class's shader, which is a \
             wrong picture rather than a failure."
        );
    }
}

/// Every per-instance name is declared where a BatchRendererGroup reads it.
///
/// **The `Properties` block is not that place.** A BRG binds through
/// `Shader.PropertyToID` against the `UNITY_DOTS_INSTANCED_PROP` declarations;
/// the `Properties` block is what the SRP Batcher reads. A name present in one
/// and absent from the other is the "reads the default and draws a plausible
/// wrong picture" failure, and the earlier test checks only the block.
#[test]
fn every_per_instance_name_is_declared_as_a_dots_instanced_prop() {
    let cs = package_gate::package_cs_files();
    let names = package_gate::instanced_property_names(&cs);
    assert!(
        !names.is_empty(),
        "no per-instance property names are declared"
    );

    let includes = package_gate::hlsl_sources();
    let props: Vec<(String, String)> = includes
        .iter()
        .flat_map(|(_, source)| package_gate::dots_instanced_props(source))
        .collect();
    let declared: Vec<String> = props.iter().map(|(name, _)| name.clone()).collect();
    assert!(
        !declared.is_empty(),
        "no UNITY_DOTS_INSTANCED_PROP is declared in any .hlsl under \
         Runtime/Shaders/. A BatchRendererGroup binds through those, so this \
         check would otherwise pass over a package that binds nothing."
    );

    for name in &names {
        assert!(
            declared.contains(name),
            "`{name}` is a per-instance property in Runtime/PaintProperties.cs \
             and is declared by no UNITY_DOTS_INSTANCED_PROP. Unity resolves \
             the metadata to nothing and every instance reads the default."
        );
    }
    for name in &declared {
        assert!(
            names.contains(name),
            "UNITY_DOTS_INSTANCED_PROP declares `{name}`, which \
             Runtime/PaintProperties.cs does not name — so the painter writes \
             no metadata for it and every instance reads the default."
        );
    }

    // **Every per-instance property is sixteen bytes wide, and the declared
    // type has to say so.** The painter lays each one out at
    // `HeadBytes + n * 16`, so a property declared `float` resolves its
    // metadata happily and reads the wrong sixteen bytes — the "right size,
    // wrong meaning" failure, on the surface `unity-abi` cannot see.
    for (name, ty) in &props {
        assert!(
            ty == "float4" || ty == "uint4" || ty == "int4",
            "UNITY_DOTS_INSTANCED_PROP declares `{name}` as `{ty}`. Every \
             per-instance property this painter writes occupies one 16-byte \
             slot, so a narrower type reads the wrong bytes rather than failing."
        );
    }
}

/// Every global the painter binds is declared by the shading it binds it for.
///
/// **Neither direction was checked before.** The globals never appear in a
/// `Properties` block — a `StructuredBuffer` is bound through
/// `Shader.SetGlobalBuffer` and cannot be a material property — so the
/// per-instance test consulted them only as an allow-list and never in the
/// direction that matters. Renaming `PaintGlobals.Paints` left every solid and
/// gradient fill shading from an unbound buffer with every gate green.
#[test]
fn every_global_the_painter_binds_is_declared_by_the_shading() {
    let cs = package_gate::package_cs_files();
    let bound = package_gate::other_bound_names(&cs);
    assert!(!bound.is_empty(), "Runtime/PaintBindings.cs names nothing");

    let includes = package_gate::hlsl_sources();
    assert!(!includes.is_empty(), "no .hlsl under Runtime/Shaders/");
    let shading: String = includes
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let cbuffer: String = includes
        .iter()
        .filter_map(|(_, source)| package_gate::per_material_cbuffer(source))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !cbuffer.is_empty(),
        "no .hlsl declares a `CBUFFER_START(UnityPerMaterial)` block. A \
         `Properties` entry outside that block makes the shader \
         SRP-Batcher-incompatible, which R-E5 requires and a compile does not \
         report — so the check below would pass over a package with no block \
         at all."
    );

    for name in &bound {
        // **Declared in the shading itself, and a mention in a `.shader` does
        // not count.** A first version allowed `shaders.contains(name)` as a
        // third disjunct, which `_DsCutoff` satisfied through its `Properties`
        // entry, its `clip()` call and a comment — so deleting its declaration
        // from the CBUFFER, the exact state the fix repaired, passed.
        let is_buffer = shading.contains(&format!("StructuredBuffer<float4> {name};"));
        let is_per_material = cbuffer.contains(&format!(" {name};"));
        let is_global_scalar = shading.contains(&format!("\nfloat4 {name};"));
        assert!(
            is_buffer || is_per_material || is_global_scalar,
            "`{name}` is bound by the painter (Runtime/PaintBindings.cs) and is \
             declared by no shader source as a StructuredBuffer, a \
             UnityPerMaterial member or a global scalar. The binding then \
             reaches nothing and the shading reads an unbound buffer or a \
             default."
        );
    }
}
