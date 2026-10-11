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
//!
//! Story #1450 adds two facts the variant shares with the mesh path: `DsVertex`
//! decodes an occlusion piece and selects each quad edge the same way whichever
//! path supplied the corner, and `DsProceduralCorner` returns exactly the
//! corners `BrgPainter.UnitQuad` holds, in its index order.

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

const HLSL: &str = "unity/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl";

/// The squeezed body of the HLSL function whose signature is `signature`,
/// from the signature to its closing brace at column zero.
fn hlsl_function(path: &str, signature: &str) -> String {
    let raw = read(path);
    assert!(
        !raw.contains("/*"),
        "{path} has a block comment; this scan drops line comments only"
    );
    let code = raw
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let start = code
        .find(signature)
        .unwrap_or_else(|| panic!("{path} no longer declares `{signature}`"));
    let end = start
        + code[start..]
            .find("\n}")
            .unwrap_or_else(|| panic!("{path}'s `{signature}` never closes"));
    package_gate::cs_scan::squeeze(&code[start..end])
}

/// `DsVertex` decodes an occlusion piece and places its quad by selection,
/// on both draw paths (story #1450).
///
/// **The decode is `row * 16 + grow bits`, with row 0 meaning the whole box**,
/// so the extent table is indexed by `row - 1`: dropping the `- 1u` reads the
/// next piece's edges and still compiles. **The corner is selected, not
/// interpolated**: two pieces of one rect meet where one's `hi` is the other's
/// `lo`, and `lo + corner * (hi - lo)` reproduces `hi` only up to rounding. The
/// corner source is the mesh on the BatchRendererGroup path and
/// `DsProceduralCorner` on the `DASHSCENE_PROCEDURAL` path, and the selection
/// after it is shared. `just unity-render`'s seam and edge probes draw the
/// result on Metal; this text pins it for every API that gate does not run.
#[test]
fn ds_vertex_decodes_a_piece_and_selects_its_corners_on_both_paths() {
    let body = hlsl_function(HLSL, "DsVaryings DsVertex(DsAttributes input)");
    for needle in [
        "uint piece = (uint)shade.w;",
        "float4 edges = _DsExtents[piece / DS_EXTENT_INDEX_SCALE - 1u];",
        "uint grow = piece % DS_EXTENT_INDEX_SCALE;",
        "(grow & DS_GROW_LEFT) != 0u ? edges.x - margin : edges.x,",
        "(grow & DS_GROW_TOP) != 0u ? edges.y - margin : edges.y);",
        "(grow & DS_GROW_RIGHT) != 0u ? edges.z + margin : edges.z,",
        "(grow & DS_GROW_BOTTOM) != 0u ? edges.w + margin : edges.w);",
        "#ifdef DASHSCENE_PROCEDURAL float2 corner = DsProceduralCorner(input.vertexID); \
         #else float2 corner = input.positionOS.xy; #endif \
         float2 local = float2(corner.x > 0.5 ? hi.x : lo.x, corner.y > 0.5 ? hi.y : lo.y);",
    ] {
        assert!(
            body.contains(needle),
            "{HLSL}'s DsVertex no longer contains `{needle}` in code."
        );
    }
}

/// `BrgPainter.UnitQuad` and `DsProceduralCorner` hand `DsVertex` the same
/// corners, each exactly 0 or 1 on each axis.
///
/// **`DsVertex` selects an edge with `corner > 0.5`**, so a corner has to be
/// exactly 0 or 1: a quad centred on the origin (-0.5 .. 0.5) would select
/// `lo` for every corner and collapse every quad to a point. The procedural
/// variant rebuilds the mesh from the vertex id, so its six corners must be
/// the mesh's vertices taken in the mesh's index order, or the two paths
/// wind and place their quads differently.
#[test]
fn the_unit_quad_and_the_procedural_corner_agree_on_exact_corners() {
    const PAINTER: &str = "unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs";
    let painter = package_gate::cs_scan::blank_comments_and_strings(&read(PAINTER));
    let (s, e) = package_gate::cs_scan::member_body(&painter, "private static Mesh UnitQuad()");
    let body = package_gate::cs_scan::squeeze(&painter[s..=e]);

    let vertices: Vec<(f32, f32)> = body
        .split("new Vector3(")
        .skip(1)
        .map(|rest| {
            let args: Vec<f32> = rest[..rest.find(')').unwrap()]
                .split(',')
                .map(|a| {
                    a.trim()
                        .trim_end_matches('f')
                        .parse::<f32>()
                        .unwrap_or_else(|_| {
                            panic!("{PAINTER}'s UnitQuad vertex `{a}` is not a literal")
                        })
                })
                .collect();
            (args[0], args[1])
        })
        .collect();
    assert_eq!(
        vertices,
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
        "{PAINTER}'s UnitQuad is no longer the four exact corners of [0, 1] x [0, 1] \
         in this order."
    );
    let triangles: Vec<usize> = {
        let at = body
            .find("triangles = new[] {")
            .unwrap_or_else(|| panic!("{PAINTER}'s UnitQuad sets no triangle list"));
        let rest = &body[at + "triangles = new[] {".len()..];
        rest[..rest.find('}').unwrap()]
            .split(',')
            .map(|i| i.trim().parse().unwrap())
            .collect()
    };
    assert_eq!(triangles, [0, 1, 2, 0, 2, 3]);

    // The HLSL function, pinned as text and mirrored here: the mirror is only
    // as good as the pin, so a change to either fails one of the two asserts.
    let corner = hlsl_function(HLSL, "float2 DsProceduralCorner(uint vertexId)");
    assert_eq!(
        corner,
        "float2 DsProceduralCorner(uint vertexId) { \
         uint q = vertexId < 3u ? vertexId : (vertexId == 3u ? 0u : vertexId - 2u); \
         return float2((q == 1u || q == 2u) ? 1.0 : 0.0, q >= 2u ? 1.0 : 0.0);",
        "{HLSL}'s DsProceduralCorner changed; this mirror must change with it."
    );
    for vertex_id in 0u32..6 {
        let q = if vertex_id < 3 {
            vertex_id
        } else if vertex_id == 3 {
            0
        } else {
            vertex_id - 2
        };
        let procedural = (
            if q == 1 || q == 2 { 1.0 } else { 0.0 },
            if q >= 2 { 1.0 } else { 0.0 },
        );
        assert_eq!(
            procedural, vertices[triangles[vertex_id as usize]],
            "vertex {vertex_id}: DsProceduralCorner and the mesh disagree"
        );
    }
}
