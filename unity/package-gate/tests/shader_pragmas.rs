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
//!
//! **And where they sit is a third assertion**, added with issue #1313: the
//! painter loads a shader by its own declared name through `Resources.Load`, so
//! a shader outside `Runtime/Resources/` is one a player build strips and one
//! that load returns null for.

/// The set is not empty, in both directions, and the two agree.
#[test]
fn every_shader_the_painter_names_exists_and_every_shader_is_named() {
    let sources = package_gate::shader_sources();
    assert!(
        !sources.is_empty(),
        "the package ships no .shader at all. R-E11 and R-E12 would then be \
         met over an empty set, which is what their own text forbids."
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
         `Resources.Load`; a file no name reaches is a shader that passes every \
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
                 (Runtime/PaintProperties.cs) nor as a per-material one \
                 (Runtime/PaintBindings.cs). Either the packer stopped writing \
                 it or the shader kept a property that moved."
            );
        }
    }
}

/// Each shader constant names **its own** shader, not merely one of the
/// package's.
///
/// **"Material class" is what this was written for and is no longer the whole
/// set.** `shader_consts` reads every `Dashscene/…` constant in `PaintHeap.cs`,
/// and story #1123 added `PaintShaders.Text`, which is deliberately not a
/// class — so the tie below now covers four constants and three classes. The
/// mechanism is unchanged: each constant's shader must define the
/// `DASHSCENE_CLASS_*` macro spelled from that constant's own name.
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
        "no UNITY_DOTS_INSTANCED_PROP is declared in any .hlsl the package \
         ships. A BatchRendererGroup binds through those, so this check would \
         otherwise pass over a package that binds nothing."
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

/// Every name the painter binds is declared by the shading it binds it for.
///
/// **Neither direction was checked before.** The per-instance test consults
/// these names only as an allow-list — it asks that a `Properties` entry be a
/// name the C# knows, never that a name the C# binds be declared anywhere — so
/// renaming `PaintMaterialProperties.Paints` left every solid and gradient
/// fill shading from an unbound buffer with every gate green.
///
/// **A `Properties` entry is not what this asks for**, and the two are
/// separate questions: `every_per_material_member_is_declared_by_every_shader`
/// below asks that one, over the `UnityPerMaterial` members alone. Three of
/// the names here do carry a `Properties` entry — `_DsGlobals` in all four
/// shaders, `_DsCutoff` in all four, `_DsAtlas` in `Text.shader` — and the
/// four `StructuredBuffer`s carry none, because a buffer cannot have one.
#[test]
fn every_name_the_painter_binds_is_declared_by_the_shading() {
    let cs = package_gate::package_cs_files();
    let bound = package_gate::other_bound_names(&cs);
    assert!(!bound.is_empty(), "Runtime/PaintBindings.cs names nothing");

    let includes = package_gate::hlsl_sources();
    assert!(!includes.is_empty(), "the package ships no .hlsl");
    let shading: String = includes
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // **The block's own comments are cut out first.** `is_per_material` below
    // asks whether the block contains ` name;`, and the block's prose names
    // every member it discusses — so a comment reading `the painter writes
    // _DsGlobals; every class reads it` satisfied the arm for a member whose
    // declaration had been deleted. That is the same fail-open this file
    // already records for the third disjunct it removed: "a mention in a
    // `.shader` does not count".
    let cbuffer: String = includes
        .iter()
        .filter_map(|(_, source)| package_gate::per_material_cbuffer(source))
        .flat_map(|block| {
            block
                .lines()
                .map(|line| line.split("//").next().unwrap_or("").to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !cbuffer.trim().is_empty(),
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
        // **A texture is neither of the two above and cannot be made into
        // one.** It is not a `StructuredBuffer`, and it cannot be a
        // `UnityPerMaterial` member — a sampler has no place in a constant
        // buffer, and putting one there makes the shader
        // SRP-Batcher-incompatible. Before story #1123 the package bound no
        // texture, so the arms that remained were the whole set; the atlas a
        // glyph run samples is the first texture, and it is bound per material
        // because a document may name more than one sheet.
        //
        // **Both halves, and that is the strengthening rather than a
        // formality.** `TEXTURE2D(name)` alone would be satisfied by a
        // declaration nothing can sample: URP's `SAMPLE_TEXTURE2D*` macros take
        // the sampler by name, and on a platform where `TEXTURE2D` expands to a
        // plain `Texture2D` a missing `SAMPLER` is a compile error only in the
        // pass that samples it — which the text class is and the other three
        // are not, so a file-joined search that accepted the texture alone
        // would pass over a package whose sampler had been deleted.
        //
        // **`sampler` then the name, with no separator of its own.** Unity's
        // convention pairs `_MainTex` with `sampler_MainTex`, and every name
        // here already begins with an underscore — a `sampler_` prefix would
        // look for `sampler__DsAtlas` and fail on a correct declaration, which
        // is what the first cut of this check did.
        let is_texture = shading.contains(&format!("TEXTURE2D({name});"))
            && shading.contains(&format!("SAMPLER(sampler{name});"));
        // **A scalar declared at global scope was a fourth arm until issue
        // #1297.** A uniform outside every CBUFFER lands in `$Globals`, which
        // is one namespace for the process — the shading half of the collision
        // that issue names — so the form is no longer accepted here, and
        // `_DsGlobals` moving into `UnityPerMaterial` is what this arm's
        // removal holds in place.
        assert!(
            is_buffer || is_per_material || is_texture,
            "`{name}` is bound by the painter (Runtime/PaintBindings.cs) and is \
             declared by no shader source as a StructuredBuffer, a \
             UnityPerMaterial member, or a TEXTURE2D with its SAMPLER. The \
             binding then reaches nothing and the shading reads an unbound \
             buffer, an unbound sampler or a default."
        );
    }
}

/// Every shader sits where `Resources.Load` will find it, and nowhere else.
///
/// **This is issue #1313's fix held in place, and no other check in this
/// repository can hold it.** `BrgPainter` resolved its shaders with
/// `Shader.Find` until 2026-08-23, which works in an editor and returns null in
/// a player: Unity strips a shader that no scene and no material references out
/// of a player build. Every gate here passed while the package could not draw
/// as installed, because both sides of every assertion are derived from the
/// tree and stripping happens at build time. `just unity-render` is what
/// observes the consequence; this is what keeps the arrangement that avoids it.
///
/// A `Resources` folder is included in a build whether or not anything
/// references it, and the painter now loads through one. The load argument is
/// the shader's own declared name — `Dashscene/UnlitOverlay` — so the file has
/// to sit at `Runtime/Resources/Dashscene/UnlitOverlay.shader` and nowhere
/// else. **Both directions**, because a shader that stays behind is one
/// `Resources.Load` returns null for, and a shader in `Resources/` that no
/// class names is one nothing loads.
#[test]
fn every_shader_sits_where_resources_load_will_find_it() {
    let consts = package_gate::shader_consts();
    assert!(
        !consts.is_empty(),
        "Runtime/PaintHeap.cs declares no `Dashscene/…` shader constant, so this \
         check would hold the layout over an empty set."
    );

    let sources = package_gate::shader_sources();
    let paths: Vec<&str> = sources.iter().map(|(path, _)| path.as_str()).collect();

    let mut expected = Vec::new();
    for (member, shader_name) in &consts {
        let want = package_gate::resources_shader_path(shader_name);
        assert!(
            paths.contains(&want.as_str()),
            "PaintShaders.{member} names `{shader_name}`, and no shader sits at \
             {want}. The painter loads it with \
             `Resources.Load<Shader>(\"{shader_name}\")`, which resolves that \
             path and nothing else — so this is a null from `Resources.Load` \
             and the painter's R-E2 diagnostic at run time. The shaders the \
             package does ship are {paths:?}."
        );
        expected.push(want);
    }

    for (path, _) in &sources {
        assert!(
            expected.contains(path),
            "{path} is a shader the package ships and no material class names \
             it at that path. A shader outside `Runtime/Resources/` is stripped \
             from a player build (issue #1313), and one inside it that \
             `PaintShaders` does not name is included in every build and loaded \
             by nothing. The paths the classes name are {expected:?}."
        );
    }
}

/// The painter loads its shaders through `Resources`, and nothing under
/// `Runtime/` calls `Shader.Find`.
///
/// **The layout test above is half of issue #1313's fix and this is the other
/// half.** Moving the files into `Runtime/Resources/` only helps if the painter
/// loads them from there: restoring `Shader.Find` leaves every shader where it
/// is, passes every other check in this crate, and returns null in a player
/// again. Measured — that mutation was made and the whole suite stayed green
/// before this test existed.
///
/// **The two halves are stated over different scopes, and neither scope is an
/// oversight.** The positive half names one file, because pinning it there is
/// what stopped the check tracking the spelling of a local. The negative half
/// is stated over all of `Runtime/`, so a `Shader.Find` written anywhere in
/// the package fails whether or not the painter still loads correctly.
#[test]
fn the_package_loads_its_shaders_through_resources_and_never_finds_them() {
    let files = package_gate::package_cs_files();
    let runtime: Vec<&(String, String)> = files
        .iter()
        .filter(|(path, _)| path.contains("/Runtime/"))
        .collect();
    assert!(
        !runtime.is_empty(),
        "the package ships no C# under Runtime/, so this check would hold the \
         load mechanism over an empty set."
    );

    // **The file, not a count, and not the argument's spelling.** Matching
    // `Resources.Load<Shader>(shaderName)` coupled this gate to the name of a
    // local: renaming it failed the test with a message naming a defect that
    // did not exist, and a second loader written with a different argument name
    // went uncounted.
    const LOADER: &str = "Runtime/Engine/BrgPainter.cs";
    let painter = runtime
        .iter()
        .find(|(path, _)| path.ends_with(LOADER))
        .unwrap_or_else(|| panic!("the package no longer ships {LOADER}"));
    assert!(
        painter.1.contains("Resources.Load<Shader>("),
        "{LOADER} does not load a shader through `Resources`. That call is what \
         makes the shaders survive a player build (issue #1313); the layout \
         test above only puts the files where it can find them."
    );

    for (path, source) in &runtime {
        assert!(
            !source.contains("Shader.Find("),
            "{path} calls `Shader.Find`. Unity strips a shader that no scene \
             and no material references out of a player build, so that call \
             resolves in an editor and returns null in the one configuration a \
             consumer ships — which is issue #1313, measured rather than \
             reasoned. Load through `Resources` instead."
        );
    }
}

/// Every absolute `#include` a shader makes resolves to a file the package
/// ships.
///
/// **New coupling, and the move introduced it.** The three `.shader` files used
/// to include their shading from the same directory; they now sit under
/// `Runtime/Resources/` and reach `Runtime/Shaders/` through the
/// `Packages/<name>/…` form Unity resolves at compile time. That hardcodes the
/// package's own directory name in three files, and a typo or a rename is
/// caught by nothing until someone runs a gate that needs a Unity editor —
/// which `docs/design/unity-csharp-host.md` records happening once already,
/// with three shaders reported clean while every one of them failed on a
/// non-existent include path.
#[test]
fn every_absolute_shader_include_resolves_to_a_file_the_package_ships() {
    // **The package name is read out of the line rather than matched against**,
    // which is the difference between catching a typo and skipping it. Matching
    // `Packages/com.driftsys.dashscene/` and skipping everything else means a
    // misspelled package segment — the likeliest typo of all — never reaches
    // the assertion. Measured: `Packages/com.driftsys.dashsceneX/…` left every
    // test green before this.
    const OPEN: &str = "#include \"Packages/";

    // Everything the package legitimately includes from another package: the
    // one its `package.json` declares a dependency on, and nothing else. URP's
    // own library includes from `com.unity.render-pipelines.core` in turn, but
    // that is URP's file rather than one this gate reads — listing that package
    // here would permit an include this package's manifest does not entitle it
    // to make.
    const FOREIGN: [&str; 1] = ["com.unity.render-pipelines.universal"];

    let mut checked = 0;
    // Held per shader rather than over the set: a shader that includes NOTHING
    // cannot compile, and a count over all of them is satisfied by its
    // siblings. Measured — deleting the include from one shader left the suite
    // green.
    let mut own_include = std::collections::BTreeMap::new();

    let sources: Vec<(String, String)> = package_gate::shader_sources()
        .into_iter()
        .chain(package_gate::hlsl_sources())
        .collect();

    for (path, source) in &sources {
        own_include.entry(path.clone()).or_insert(0);
        for line in source.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix(OPEN) else {
                continue;
            };
            let Some(body) = rest.strip_suffix('"') else {
                panic!("{path}: `{line}` does not end in a quote");
            };
            let Some((package, relative)) = body.split_once('/') else {
                panic!("{path}: `{line}` names a package and no file inside it");
            };

            if FOREIGN.contains(&package) {
                continue;
            }
            assert_eq!(
                package,
                package_gate::PACKAGE_NAME,
                "{path} includes from `Packages/{package}/`, which is neither \
                 this package nor a package it depends on. Unity resolves this \
                 at shader compile time, so only a gate that needs an editor \
                 would report it."
            );

            let target = package_gate::root()
                .join(package_gate::PACKAGE_PATH)
                .join(relative);
            assert!(
                target.is_file(),
                "{path} includes `{relative}`, and the package ships no such \
                 file. Unity resolves this path at shader compile time, so \
                 nothing before `just unity-editor` or `just unity-render` — \
                 both of which need an editor — would report it."
            );
            checked += 1;
            *own_include.get_mut(path).unwrap() += 1;
        }
    }

    assert!(
        checked > 0,
        "no file makes an absolute `Packages/{}/…` include, so this check held \
         over an empty set. The material classes reach their shading that way \
         since issue #1313 moved them out of `Runtime/Shaders/`.",
        package_gate::PACKAGE_NAME
    );

    // Every `.shader` that declares a material class reaches its shading this
    // way, so each must make at least one such include of its own.
    for (path, source) in &sources {
        if !path.ends_with(".shader") || package_gate::declared_class(source).is_none() {
            continue;
        }
        assert!(
            own_include[path] > 0,
            "{path} declares a material class and includes nothing from \
             `Packages/{}/`. Its shading lives in `Runtime/Shaders/`, so a \
             shader that reaches none of it cannot compile — and a count over \
             all the shaders together is satisfied by its siblings.",
            package_gate::PACKAGE_NAME
        );
    }
}

/// Every `UnityPerMaterial` member is declared by every shader that includes
/// the block it lives in.
///
/// **This is the rule issue #1297's fix discovered, and it is measured rather
/// than reasoned.** The converse — a `Properties` entry must appear in
/// `UnityPerMaterial` or the SRP Batcher refuses the shader — was already
/// stated in `DashsceneInstance.hlsl` and is what the test above rests on.
/// This direction was found by breaking it: `_DsGlobals` moved into the
/// CBUFFER for issue #1297 and was left out of the four `Properties` blocks,
/// and every draw command in a built player was then refused with
/// `UnityPerMaterial var is not declared in shader property section`, ink at 0
/// of 13 sampled node centres. `just unity-render`, 6000.3.23f1, macOS/Metal,
/// Apple M3, 2026-08-29.
///
/// **No offline gate saw that state.** Three review seats each deleted one
/// `_DsGlobals` line and ran the whole of `package-gate` green over the exact
/// tree that draws nothing — which is why this test exists and why it is worth
/// its cost: the only other thing that catches the class needs a Unity editor,
/// a player build and a device, and runs in no CI job.
///
/// **Every member of every including shader, not `_DsGlobals` alone.** Naming
/// the one member that was measured would fix the instance and leave the
/// class: `_DsCutoff` sat in this same block for all four shaders and in one
/// `Properties` block, and drew only because nothing in the other three reads
/// it — an explanation neither run measured, and one that a graphics API whose
/// reflection keeps an unreferenced member would falsify. All four shaders now
/// declare both.
///
/// **Scoped to the shaders that include the shading**, because the block is
/// the shading's. A shader shipped in `Samples~/` that includes none of it has
/// no `UnityPerMaterial` members of this package's and is not asked for any.
#[test]
fn every_per_material_member_is_declared_by_every_shader() {
    let includes = package_gate::hlsl_sources();
    // Comments cut out first, for the reason the sibling test above states:
    // the block's prose names every member it discusses, and this parse must
    // not read a mention as a declaration or a `#` in prose as a directive.
    let cbuffer: String = includes
        .iter()
        .filter_map(|(_, source)| package_gate::per_material_cbuffer(source))
        .flat_map(|block| {
            block
                .lines()
                .map(|line| line.split("//").next().unwrap_or("").to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !cbuffer.trim().is_empty(),
        "no .hlsl declares a `CBUFFER_START(UnityPerMaterial)` block, so this \
         gate would demand nothing of any shader"
    );

    // A member is `<type> <name>;`. **The parse fails closed**, which is the
    // whole of it: a first version filtered on `line.ends_with(';')` after
    // trimming, so `float _DsCutoff;  // only the cutout reads it` was not a
    // declaration, dropped out of the demanded set, and let two review seats
    // run this suite green over the tree that draws 0 of 13 centres. A parse
    // that can be wrong has to be wrong in the direction that fails.
    //
    // So the comment is cut off FIRST, and then every remaining line naming a
    // `_Ds` token must yield exactly one member — a line this cannot read is a
    // failure here rather than a member nothing demands.
    // **A preprocessor directive inside the block is refused, not skipped.**
    // The shading scopes `_DsGlyphs` and `_DsAtlas` by class already, so
    // guarding a CBUFFER member the same way is a reasonable future change —
    // and this parser would then demand a member of shaders that never compile
    // it. Failing here is what makes that change land with its own gate rather
    // than against one that quietly asks the wrong thing.
    assert!(
        !cbuffer.contains('#'),
        "the `UnityPerMaterial` block carries a preprocessor directive, and \
         this gate reads it as one member set for every shader. Scoping a \
         member by class needs this parser to scope with it."
    );

    let mut members: Vec<String> = Vec::new();
    for raw in cbuffer.lines() {
        let line = raw.trim();
        if line.is_empty() || !line.contains("_Ds") {
            continue;
        }
        assert_eq!(
            line.matches(';').count(),
            1,
            "the `UnityPerMaterial` block line `{line}` declares no member or \
             more than one, and this parse reads one per line. Two members on \
             one line would leave the first demanded of nobody."
        );
        let name = line
            .strip_suffix(';')
            .and_then(|declaration| declaration.split_whitespace().next_back())
            .filter(|name| name.starts_with("_Ds"))
            .unwrap_or_else(|| {
                panic!(
                    "the `UnityPerMaterial` block line `{line}` names a `_Ds` \
                     token and this parse read no member from it. Every shader \
                     must declare every member, so a member this cannot see is \
                     one no shader is held to."
                )
            });
        members.push(name.to_string());
    }

    // **Held against a set derived elsewhere**, so a parse that reads nothing
    // fails rather than demanding nothing. The five per-instance names are in
    // this block by construction — they are its non-instanced fallback, which
    // is what `the_per_instance_properties_are_the_same_set_on_both_sides`
    // reads it for.
    let instanced = package_gate::instanced_property_names(&package_gate::package_cs_files());
    assert!(
        !instanced.is_empty(),
        "Runtime/PaintProperties.cs names nothing"
    );
    for name in &instanced {
        assert!(
            members.contains(name),
            "the `UnityPerMaterial` block parsed to {members:?}, which does not \
             include the per-instance name `{name}` that block declares as its \
             non-instanced fallback. The parse has stopped reading the block."
        );
    }

    let shaders = package_gate::shader_sources();
    assert!(!shaders.is_empty(), "the package ships no .shader");
    let mut checked = 0usize;
    for (path, source) in &shaders {
        if !source.contains("DashsceneInstance.hlsl") {
            // **Every shipped shader includes it, and that is asserted rather
            // than assumed.** `Runtime/Resources/` is where a shader the
            // painter loads has to sit —
            // `every_shader_sits_where_resources_load_will_find_it` holds that
            // — so one there that includes none of the shading is a shader
            // this gate would pass over silently. A shader elsewhere in the
            // package, `Samples~/` included, is not the painter's and is not
            // asked.
            assert!(
                !path.contains("Runtime/Resources/"),
                "{path} is a shader the painter loads and includes none of the \
                 shading, so this gate cannot say whether its `Properties` \
                 block covers the `UnityPerMaterial` members it draws with"
            );
            continue;
        }
        checked += 1;
        let block = package_gate::properties_block(source)
            .unwrap_or_else(|| panic!("{path} declares no `Properties` block"));
        let declared = package_gate::ds_property_names(&block);
        for member in &members {
            assert!(
                declared.contains(member),
                "{path} includes the shading's `UnityPerMaterial` block, which \
                 declares `{member}`, and its `Properties` block does not. A \
                 member the property section omits makes the pass \
                 SRP-Batcher-incompatible and a BatchRendererGroup refuses \
                 every draw command that uses it — a blank frame, which no \
                 compile and no other gate here reports. Declared: {declared:?}"
            );
        }
    }
    assert!(
        checked > 0,
        "no shipped .shader includes DashsceneInstance.hlsl, so this gate \
         checked nothing"
    );
}
