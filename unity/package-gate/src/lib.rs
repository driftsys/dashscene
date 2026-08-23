//! The Unity package's shading, held to its two sources.
//!
//! Two questions, both answerable with no Unity editor and no GPU, which is
//! why they are a Rust test rather than a step inside `just unity-abi`:
//!
//! - **R-T5** ([`docs/specification/03-target-hardware-rules.md`]) asks for the
//!   SDF shader math to be single-sourced into both product painters' shading
//!   languages. [`generate_hlsl`] is that mechanism: it compiles
//!   `crates/dashscene-gpu/src/shaders/sdf.wgsl` — the file the lean painter's
//!   pipelines and the layer-2 harness both include — to HLSL with `naga`, the
//!   translator wgpu already runs that same file through. The committed
//!   [`HLSL_PATH`] is the output, and `tests/` re-derives it.
//!
//! - **R-E11 and R-E12** ([`docs/specification/07-embedding-and-distribution.md`])
//!   require `#pragma target 4.5` or higher and
//!   `#pragma multi_compile _ DOTS_INSTANCING_ON` on every shader the painter
//!   registers, and both state that the check must assert the set is not empty.
//!   [`shader_sources`] and [`registered_shader_names`] are the two halves of
//!   that set, and the tests hold them to each other.
//!
//! Nothing here runs at package build time. The generator is a `just` recipe a
//! developer runs after editing the WGSL; the test is what makes forgetting it
//! a failure rather than a silent divergence.
//!
//! [`docs/specification/03-target-hardware-rules.md`]: https://github.com/driftsys/dashscene/blob/main/docs/specification/03-target-hardware-rules.md
//! [`docs/specification/07-embedding-and-distribution.md`]: https://github.com/driftsys/dashscene/blob/main/docs/specification/07-embedding-and-distribution.md

use std::path::{Path, PathBuf};

/// The WGSL shader library, relative to the repository root.
///
/// `dashscene-gpu` exposes the same file as `dashscene_gpu::SDF_WGSL`. It is
/// read from disk here rather than taken through that constant so this crate
/// does not depend on a crate that pulls in wgpu — the gate compiles in
/// seconds and runs in the sanity tier, and a wgpu dependency would put it
/// behind a two-minute build.
///
/// The two are held together by [`WGSL_IS_THE_CRATE_S_OWN`]'s test, which
/// asserts this path is the one `shader.rs` includes.
pub const WGSL_PATH: &str = "crates/dashscene-gpu/src/shaders/sdf.wgsl";

/// The generated HLSL, relative to the repository root.
pub const HLSL_PATH: &str = "unity/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl";

/// The package directory the shaders and the C# live under.
pub const PACKAGE_PATH: &str = "unity/com.driftsys.dashscene";

/// The package's UPM name, which is also the first segment of every absolute
/// `#include "Packages/…"` path its shaders make.
pub const PACKAGE_NAME: &str = "com.driftsys.dashscene";

/// The `include_str!` in `dashscene-gpu` that must name [`WGSL_PATH`].
///
/// Named as a constant so the test that checks it reads as an assertion about
/// the other crate rather than as a string buried in a test body.
pub const WGSL_IS_THE_CRATE_S_OWN: &str = "crates/dashscene-gpu/src/shader.rs";

/// The repository root, derived from this crate's manifest directory.
///
/// `unity/package-gate` is two levels down, so the root is two parents up. A
/// crate moved without updating this resolves to a directory with no
/// `Cargo.toml` in it, which [`root`] refuses rather than reporting a missing
/// shader.
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("unity/package-gate sits two levels below the repository root")
        .to_path_buf();
    assert!(
        root.join("Cargo.toml").is_file(),
        "{} has no Cargo.toml, so it is not the repository root — has this \
         crate moved? Every path in this crate is stated relative to that root.",
        root.display()
    );
    root
}

/// What a generated file says about itself, before the generated text.
///
/// **The command is in it on purpose.** A reader who finds this file in a
/// package they installed has no repository to look in, and the first thing
/// they need is that editing it is pointless.
fn banner() -> String {
    format!(
        "// GENERATED FILE — do not edit.\n\
         //\n\
         // Compiled from\n\
         //     {wgsl}\n\
         // by `naga`, the translator wgpu runs that same file through for the\n\
         // lean painter. Regenerate with:\n\
         //\n\
         //     just sdf-hlsl\n\
         //\n\
         // `docs/specification/03-target-hardware-rules.md` R-T5 asks for this\n\
         // math to be single-sourced into both product painters' shading\n\
         // languages. Editing this file breaks that in the one direction no\n\
         // review catches: it would still compile, still draw, and no longer\n\
         // be the same arithmetic the other painter evaluates. The test in\n\
         // `unity/package-gate` re-derives it on every run and fails if it is\n\
         // not what the WGSL produces.\n\
         //\n\
         // Two names differ from the WGSL, and both are naga's namer rather\n\
         // than a port: `median3` is emitted as `median3_` because the name\n\
         // ends with a digit, and `msdf_coverage`'s `sample` parameter as\n\
         // `sample_` because HLSL reserves it. Argument order is untouched.\n\
         // The namer has other rules that do not fire on this file —\n\
         // docs/design/unity-csharp-host.md carries them.\n\
         \n",
        wgsl = WGSL_PATH
    )
}

/// Compile the WGSL shader library to HLSL.
///
/// # The options are naga's defaults, with one departure
///
/// `shader_model` is set to 5.0 rather than left at naga's 5.1, because
/// `#pragma target 4.5` is what R-E11 requires of the shaders that include this
/// and 4.5 is Unity's spelling of Shader Model 5.0. Nothing else is changed —
/// in particular `restrict_indexing` and `force_loop_bounding` stay on, because
/// they are on for the lean painter too. Turning either off here would make the
/// generated code *differ* from what the same module compiles to for the other
/// painter, which is the property this whole file exists to keep.
///
/// # Errors
///
/// The WGSL failing to parse, failing validation, or failing to write as HLSL.
/// All three are defects in the source rather than conditions to handle: the
/// same file is parsed and validated by `wgpu` on every frame the lean painter
/// draws.
pub fn generate_hlsl(wgsl: &str) -> Result<String, String> {
    let module = naga::front::wgsl::parse_str(wgsl)
        .map_err(|e| format!("{WGSL_PATH} is not valid WGSL: {e}"))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| format!("{WGSL_PATH} does not validate: {e}"))?;

    let options = naga::back::hlsl::Options {
        shader_model: naga::back::hlsl::ShaderModel::V5_0,
        ..naga::back::hlsl::Options::default()
    };
    let pipeline = naga::back::hlsl::PipelineOptions::default();

    let mut body = String::new();
    naga::back::hlsl::Writer::new(&mut body, &options, &pipeline)
        .write(&module, &info, None)
        .map_err(|e| format!("{WGSL_PATH} does not translate to HLSL: {e}"))?;

    Ok(format!("{}{}", banner(), body))
}

/// Every `.shader` the package ships, as (path relative to the root, source).
///
/// Sorted by path, so a diff between two runs is stable and a test that reports
/// the set reports it in one order.
///
/// **Collected from the whole package rather than from one directory**, which
/// is a correctness difference and not tidiness. The scope includes
/// `Samples~/` and any other hidden directory, deliberately: a shader there is
/// one `every_shader_sits_where_resources_load_will_find_it` reports as
/// shipped and unnamed, which is the answer wanted — do not narrow this back to
/// `Runtime/`.
///
/// **[`package_cs_files`] does NOT share that scope**, and the difference is
/// load-bearing rather than an oversight to tidy: it walks `Runtime/` only, so
/// every question this crate asks about C# is a question about the compiled
/// half of the package. The shaders moved once
/// already — issue #1313 put them under `Runtime/Resources/` so a player build
/// keeps them — and a collector pointed at a directory answers "R-E11 and R-E12
/// hold over what I found there", which is not the requirement. Where they are
/// allowed to sit is a separate assertion,
/// `every_shader_sits_where_resources_load_will_find_it`, so moving one is a
/// named failure rather than an invisible one.
///
/// # Panics
///
/// If the package directory cannot be read. An absent directory is not the same
/// as an empty set and must not be reported as one — R-E11 and R-E12 both
/// require the non-empty assertion, and a gate whose input has moved would
/// otherwise pass having read nothing.
pub fn shader_sources() -> Vec<(String, String)> {
    collect_package_ext("shader")
}

/// Every `.hlsl` the package ships, as (path relative to the root, source).
///
/// The shading the `.shader` files include. R-E11 and R-E12 are about the
/// `.shader` programs; **R-T5 is about these**, because this is where a hand
/// port would live. Collected from the whole package for
/// [`shader_sources`]'s reason.
pub fn hlsl_sources() -> Vec<(String, String)> {
    collect_package_ext("hlsl")
}

/// Every file with `ext` anywhere under the package.
fn collect_package_ext(ext: &str) -> Vec<(String, String)> {
    let dir = root().join(PACKAGE_PATH);
    assert!(
        dir.is_dir(),
        "{} is not a directory. The package's shading is what R-E11, R-E12 and \
         R-T5 are stated over; a missing directory is a moved gate, not an \
         empty set.",
        dir.display()
    );
    let mut out = Vec::new();
    collect_ext(&dir, ext, &mut out);
    out.sort();
    out
}

/// Where `Resources.Load` finds the shader a material class names, relative to
/// the repository root.
///
/// **The shader's own declared name doubles as its Resources path**, so
/// `PaintShaders.For` is the only string the painter needs and there is no
/// second constant to drift. `Dashscene/UnlitOverlay` is the name in
/// `Shader "…"`, the argument to `Resources.Load<Shader>`, and the path under
/// `Runtime/Resources/`.
pub fn resources_shader_path(shader_name: &str) -> String {
    format!("{PACKAGE_PATH}/Runtime/Resources/{shader_name}.shader")
}

/// Collect every file with `ext` under `dir`, **recursively**.
///
/// Recursive on purpose: a first version used `read_dir` and would have missed
/// a shader in a subdirectory entirely — resolved at run time, and invisible to
/// R-E11, R-E12 and every other check here, with the non-empty assertions still
/// satisfied by its siblings.
fn collect_ext(dir: &Path, ext: &str, out: &mut Vec<(String, String)>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            collect_ext(&path, ext, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some(ext) {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let relative = path
            .strip_prefix(root())
            .expect("a path under the root")
            .to_string_lossy()
            .into_owned();
        out.push((relative, source));
    }
}

/// Every shader name the package's C# names.
///
/// Read from `PaintShaders`' own `const string` declarations, through
/// [`shader_consts`]. **Not** from `Dashscene/` literals anywhere in the
/// package: that is what a first version did, and it counted a mention in a
/// comment, so a shader dropped from `PaintShaders` but still named in prose
/// nearby kept the two sets equal.
///
/// The painter is what holds the other end. A null from `Resources.Load` is a
/// named diagnostic there rather than a null dereference, so a name this
/// function collects that resolves to nothing is caught at run time as well as
/// here.
pub fn registered_shader_names() -> Vec<String> {
    let mut names: Vec<String> = shader_consts().into_iter().map(|(_, v)| v).collect();
    names.sort();
    names.dedup();
    names
}

/// The `PaintShaders` constants, as (C# member name, shader name).
///
/// Read from `const string` declarations in `Runtime/PaintHeap.cs` rather than
/// from any `"Dashscene/…"` literal anywhere in the package. **A first version
/// scanned every `.cs` for the prefix**, which counted a mention in a comment —
/// so a shader dropped from `PaintShaders` but still named in prose nearby kept
/// the two sets equal and the gate green.
pub fn shader_consts() -> Vec<(String, String)> {
    let files = package_cs_files();
    let (_, source) = files
        .iter()
        .find(|(path, _)| path.ends_with("Runtime/PaintHeap.cs"))
        .expect("the package has Runtime/PaintHeap.cs, where PaintShaders lives");

    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("public const string ") else {
            continue;
        };
        let Some((member, value)) = rest.split_once(" = \"") else {
            continue;
        };
        let Some(value) = value.split('"').next() else {
            continue;
        };
        if !value.starts_with("Dashscene/") {
            continue;
        }
        out.push((member.trim().to_string(), value.to_string()));
    }
    out.sort();
    out
}

/// The `DASHSCENE_CLASS_*` macro a `.shader` defines, without the prefix.
///
/// One per shader. The include refuses to compile with none defined and, since
/// the review of story #1122, with more than one — a first version guarded only
/// the first case, so a shader could define two and this function would return
/// whichever appeared first.
pub fn declared_class(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("#define DASHSCENE_CLASS_")
            .map(|rest| rest.trim().to_string())
    })
}

/// `UnlitOverlay` as `UNLIT_OVERLAY` — the C# member name in the macro's
/// spelling, so the two can be compared without a table mapping them.
pub fn screaming_snake(camel: &str) -> String {
    let mut out = String::new();
    for (i, c) in camel.char_indices() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_uppercase());
    }
    out
}

/// The `UNITY_DOTS_INSTANCED_PROP(<type>, <name>)` names a source declares.
///
/// **This is what a BatchRendererGroup actually binds**, through
/// `Shader.PropertyToID` against the `MaterialPropertyMetadata` block — not the
/// `Properties` block, which is what the SRP Batcher reads. A name in one and
/// not the other is the silent default-read this crate exists to catch.
pub fn dots_instanced_props(source: &str) -> Vec<(String, String)> {
    const NEEDLE: &str = "UNITY_DOTS_INSTANCED_PROP(";
    let mut props = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find(NEEDLE) {
        rest = &rest[at + NEEDLE.len()..];
        let Some(end) = rest.find(')') else { break };
        let inside = &rest[..end];
        if let Some((ty, name)) = inside.split_once(',') {
            // **The type is kept, not discarded.** A first version returned
            // names alone, so `UNITY_DOTS_INSTANCED_PROP(float, _DsQuad)` passed
            // — the metadata resolves and every instance reads the wrong sixteen
            // bytes. That is the same "right size, wrong meaning" class
            // `AGENTS.md` records `unity-abi` as unable to catch, reproduced on
            // the instanced-property surface.
            props.push((name.trim().to_string(), ty.trim().to_string()));
        }
        rest = &rest[end..];
    }
    props.sort();
    props.dedup();
    props
}

/// The body between `CBUFFER_START(UnityPerMaterial)` and `CBUFFER_END`.
///
/// A `Properties` entry must appear there or the SRP Batcher refuses the shader
/// — R-E5 requires the batcher, and a compile does not report the refusal.
pub fn per_material_cbuffer(source: &str) -> Option<String> {
    let at = source.find("CBUFFER_START(UnityPerMaterial)")?;
    let rest = &source[at..];
    let end = rest.find("CBUFFER_END")?;
    Some(rest[..end].to_string())
}

/// The field names a WGSL `struct NAME { … }` declares, in order.
///
/// Used to hold a heap row's **field order** — not merely its width — against
/// the packer that writes it and the shader that reads it.
pub fn wgsl_struct_fields(source: &str, name: &str) -> Option<Vec<String>> {
    let at = source.find(&format!("struct {name} {{"))?;
    let rest = &source[at..];
    let end = rest.find('}')?;
    let fields = rest[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            let (field, _) = line.split_once(':')?;
            let field = field.trim();
            if field.is_empty() || field.starts_with("//") {
                return None;
            }
            Some(field.to_string())
        })
        .collect();
    Some(fields)
}

/// Every `.cs` under `Runtime/`, as (path relative to the root, source).
///
/// Recursive, so `Runtime/Engine/` is included. Sorted by path.
///
/// # Panics
///
/// If the directory cannot be read, or if it holds no C# at all — the second
/// because every question this crate asks about the package's C# would
/// otherwise answer vacuously.
pub fn package_cs_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_cs(&root().join(PACKAGE_PATH).join("Runtime"), &mut out);
    assert!(
        !out.is_empty(),
        "the package's Runtime/ holds no C# at all, so every question this \
         crate asks about it would answer vacuously."
    );
    out.sort();
    out
}

fn collect_cs(dir: &Path, out: &mut Vec<(String, String)>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            collect_cs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("cs") {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let relative = path
                .strip_prefix(root())
                .expect("a path under the root")
                .to_string_lossy()
                .into_owned();
            out.push((relative, source));
        }
    }
}

/// The directory under `Runtime/` that holds every engine-referencing file.
///
/// `docs/decisions/r-e10-is-checked-in-two-halves.md`: `unity/package-compat`
/// compiles everything under `Runtime/` EXCEPT this directory, because it has
/// no Unity reference assemblies and a `UnityEngine` type fails to resolve
/// there whatever its API compatibility level actually is.
pub const ENGINE_DIR: &str = "unity/com.driftsys.dashscene/Runtime/Engine/";

/// The tokens that make a C# file engine-referencing.
///
/// A file naming any of these cannot compile in `unity/package-compat`. The
/// list is short on purpose: it is what the gate searches for, so a token
/// added here without a file using it makes the gate stricter, and a namespace
/// missing from it makes it laxer — which is why the test asserts in the
/// direction that catches the second.
pub const ENGINE_TOKENS: [&str; 3] = ["UnityEngine", "UnityEditor", "Unity."];

/// The `Shader "…"` name a `.shader` source declares.
///
/// The first line of a Unity shader: the name it declares, which is also the
/// path `Resources.Load` resolves it by.
pub fn declared_shader_name(source: &str) -> Option<String> {
    let at = source.find("Shader \"")?;
    let rest = &source[at + "Shader \"".len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Every `HLSLPROGRAM`…`ENDHLSL` block in a shader source, as (index, body).
///
/// **Per block, not per file**, because R-E11 and R-E12 are properties of a
/// compiled program: a `#pragma` in one pass does not reach another. A shader
/// carrying three material classes as three passes has three programs, and a
/// check that read the file as one string would pass on a pragma present in
/// only the first.
pub fn hlsl_programs(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut rest = source;
    let mut base = 0usize;
    while let Some(open) = rest.find("HLSLPROGRAM") {
        let after = &rest[open + "HLSLPROGRAM".len()..];
        let Some(close) = after.find("ENDHLSL") else {
            break;
        };
        out.push((base + open, after[..close].to_string()));
        base += open + "HLSLPROGRAM".len() + close;
        rest = &after[close..];
    }
    out
}

/// The `Properties { … }` block of a shader source, without its braces.
///
/// Matched by counting braces from the block's own opening one, so a nested
/// brace inside an attribute or a default value does not end it early.
pub fn properties_block(source: &str) -> Option<String> {
    let at = source.find("Properties")?;
    let open = source[at..].find('{')? + at;
    let mut depth = 0usize;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[open + 1..open + offset].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// The `_Ds…` property names a `Properties` block declares.
///
/// A shader property is declared as `_Name("label", Type) = default`, so the
/// name is the first token of a line. Only names beginning `_Ds` are
/// collected: this package's own, and the ones the C# is held against.
pub fn ds_property_names(block: &str) -> Vec<String> {
    let mut names: Vec<String> = block
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with("_Ds") {
                return None;
            }
            let end = line.find('(')?;
            Some(line[..end].trim().to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The per-instance property names the package's C# declares.
///
/// Read as the `"_Ds…"` string literals in `Runtime/PaintProperties.cs` alone,
/// and that narrowing is the whole design: the package keeps one file per
/// binding kind precisely so this function needs no list of exceptions. A
/// global buffer name or a per-material property name lives in
/// `Runtime/PaintBindings.cs` and is invisible here, which is what stops the
/// gate demanding it of every shader.
pub fn instanced_property_names(files: &[(String, String)]) -> Vec<String> {
    ds_literals_in(files, "Runtime/PaintProperties.cs")
}

/// The names that are bound some other way: the global buffers and the
/// per-material properties.
///
/// What a shader is ALLOWED to declare beyond the per-instance set. A shader
/// declaring a name in neither set is what the gate reports.
pub fn other_bound_names(files: &[(String, String)]) -> Vec<String> {
    ds_literals_in(files, "Runtime/PaintBindings.cs")
}

/// Every `"_Ds…"` string literal in one of the package's C# files.
fn ds_literals_in(files: &[(String, String)], relative: &str) -> Vec<String> {
    let source = files
        .iter()
        .find(|(path, _)| path.ends_with(relative))
        .map(|(_, source)| source.as_str())
        .unwrap_or_else(|| {
            panic!("the package has no {relative}, which is where this gate reads its names")
        });

    let mut names = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("\"_Ds") {
        rest = &rest[at + 1..];
        let Some(end) = rest.find('"') else { break };
        names.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    names.sort();
    names.dedup();
    names
}

/// Every `.csproj` under `unity/`, as (path relative to the root, source).
///
/// The .NET checks beside the package. Sorted by path.
///
/// # Panics
///
/// If `unity/` cannot be read. An absent directory is not an empty set, and a
/// gate whose input has moved must not report a pass over nothing.
pub fn csproj_files() -> Vec<(String, String)> {
    let root = root();
    let dir = root.join("unity");
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));

    let mut out = Vec::new();
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if !path.is_dir() {
            continue;
        }
        let inner = std::fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for file in inner {
            let file = file.expect("a readable directory entry").path();
            if file.extension().and_then(|e| e.to_str()) != Some("csproj") {
                continue;
            }
            let source = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
            let relative = file
                .strip_prefix(&root)
                .expect("a path under the root")
                .to_string_lossy()
                .into_owned();
            out.push((relative, source));
        }
    }
    out.sort();
    out
}

/// Every top-level `fn` name the WGSL shader library declares.
///
/// Read from the source rather than listed, so a function added to the library
/// cannot be invisible to the tests that check the translation. That is the
/// distinction between asserting an instance and asserting a class, and this
/// crate has already been caught on the wrong side of it once — a check keyed
/// to `unity/package-compat` passed while `unity/ffi-check`, which carries the
/// same glob, was broken.
pub fn wgsl_function_names(wgsl: &str) -> Vec<String> {
    let mut names: Vec<String> = wgsl
        .lines()
        .filter_map(|line| {
            // Top-level only: a nested `fn` is indented, and this file has
            // none. Anchoring on column zero keeps a `fn` inside a comment out
            // as well, since every comment here starts with `//`.
            let rest = line.strip_prefix("fn ")?;
            let end = rest.find('(')?;
            Some(rest[..end].trim().to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `const NAME: u32 = <n>u;` in a WGSL source.
pub fn wgsl_const_u32(source: &str, name: &str) -> Option<i64> {
    let needle = format!("const {name}: u32 = ");
    let at = source.find(&needle)? + needle.len();
    let rest = &source[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

/// `public const int Name = <n>;` in a C# source.
pub fn cs_const_int(source: &str, name: &str) -> Option<i64> {
    let needle = format!("const int {name} = ");
    let at = source.find(&needle)? + needle.len();
    let rest = &source[at..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

/// `#define NAME <n>u` in an HLSL source.
pub fn hlsl_define_u32(source: &str, name: &str) -> Option<i64> {
    source.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(&format!("#define {name} "))?;
        let digits: String = rest
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse().ok()
    })
}

/// `static const uint NAME = <n>u;` in an HLSL source.
pub fn hlsl_static_const_u32(source: &str, name: &str) -> Option<i64> {
    let needle = format!("static const uint {name} = ");
    let at = source.find(&needle)? + needle.len();
    let rest = &source[at..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
