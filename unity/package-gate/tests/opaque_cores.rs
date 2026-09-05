//! R-T2 in the Unity painter: opaque cores front-to-back with depth, and a
//! blended fringe (story #1412).
//!
//! `docs/specification/03-target-hardware-rules.md` R-T2: split each SDF quad
//! into an opaque core drawn front-to-back with depth writes, so hidden-surface
//! rejection kills covered pixels, and a thin blended anti-aliasing fringe.
//! The shape this painter takes, and what each scan below pins:
//!
//! - every fully opaque fill on the overlay class is emitted twice — a core on
//!   `Dashscene/OverlayCore`, which writes depth in the geometry queue and
//!   keeps only fragments the shape and its clip cover completely, and the
//!   blended instance as before;
//! - every instance carries its paint ordinal in `_DsShade.w`, and the vertex
//!   stage moves it toward the viewer by that ordinal, so a core and its fringe
//!   share one depth and a later-painted node is nearer;
//! - the overlay and text passes test depth with `ZTest Less` and write none:
//!   the fringe's interior, at the core's own depth, is rejected, and a blended
//!   fragment under a core the document painted later is rejected too;
//! - the cores travel in a draw range of their own without `HasSortingPosition`,
//!   ordered by the depth test rather than by the sort keys.
//!
//! Text scans, for the reason every scan over `Runtime/Engine/` gives: no CI
//! job compiles it. `just unity-render`'s ink and order phases are what draw
//! the result, and `just unity-editor` is what compiles the new pass for the
//! fleet's two graphics APIs.

use package_gate::cs_scan::{blank_comments_and_strings, member_body, squeeze};
use package_gate::{
    hlsl_programs, package_cs_files, painter_source, resources_shader_path, shader_consts,
};

const CULLING: &str = "private unsafe JobHandle OnPerformCulling(";

fn shader(name: &str) -> String {
    let path = package_gate::root().join(resources_shader_path(name));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn runtime_file(suffix: &str) -> String {
    package_cs_files()
        .into_iter()
        .find(|(path, _)| path.ends_with(suffix))
        .map(|(_, source)| source)
        .unwrap_or_else(|| panic!("the package no longer ships {suffix}"))
}

fn pass_of(source: &str, shader_name: &str) -> String {
    let programs = hlsl_programs(source);
    assert_eq!(programs.len(), 1, "{shader_name} carries one program");
    let at = source
        .find("Pass")
        .unwrap_or_else(|| panic!("{shader_name} has no Pass"));
    source[at..].to_string()
}

/// The two blended passes test depth and write none.
///
/// **`Less`, not `LEqual`.** A core and its fringe share one ordinal, so their
/// depths are bit-identical; `Less` is what rejects the fringe's interior over
/// its own core while passing its antialiasing band, where no core wrote.
#[test]
fn the_overlay_and_text_passes_test_depth_and_write_none() {
    for name in ["Dashscene/UnlitOverlay", "Dashscene/Text"] {
        let pass = squeeze(&blank_comments_and_strings(&pass_of(&shader(name), name)));
        assert!(
            pass.contains("ZTest Less ") || pass.ends_with("ZTest Less"),
            "{name}'s pass does not declare `ZTest Less`, so a blended fragment under a \
             core the document painted later would be drawn over it, and a fringe's \
             interior would be shaded twice (R-T2, story #1412)."
        );
        assert!(
            !pass.contains("ZTest Always"),
            "{name}'s pass still declares `ZTest Always`."
        );
        assert!(
            pass.contains("ZWrite Off"),
            "{name}'s pass writes depth. D3 of the order record rejects depth writes on \
             the blended path: a translucent node would then cut what is behind it."
        );
    }
}

/// The core pass exists, is named, writes depth in the geometry queue, does
/// not blend, and discards anything under full coverage.
#[test]
fn the_core_pass_writes_depth_in_the_geometry_queue_and_keeps_only_full_coverage() {
    let consts = shader_consts();
    assert!(
        consts
            .iter()
            .any(|(member, value)| member == "OverlayCore" && value == "Dashscene/OverlayCore"),
        "PaintShaders declares no `OverlayCore = \"Dashscene/OverlayCore\"`; found {consts:?}"
    );
    let source = shader("Dashscene/OverlayCore");
    let pass = squeeze(&blank_comments_and_strings(&pass_of(
        &source,
        "Dashscene/OverlayCore",
    )));
    for needle in [
        "ZWrite On",
        "ZTest LEqual",
        "#define DASHSCENE_CLASS_OVERLAY_CORE",
        "clip(",
    ] {
        assert!(
            pass.contains(needle),
            "Dashscene/OverlayCore's pass lacks `{needle}`:\n{pass}"
        );
    }
    assert!(
        !pass.contains("Blend "),
        "Dashscene/OverlayCore's pass blends; a core is opaque by definition."
    );
    assert!(
        source.contains("\"Queue\" = \"Geometry\""),
        "Dashscene/OverlayCore is not in the Geometry queue, so its cores would not be drawn \
         before the blended instances they reject."
    );
}

/// Every instance carries its paint ordinal, and the vertex stage moves it
/// toward the viewer by that ordinal.
#[test]
fn every_instance_carries_its_paint_ordinal_and_the_vertex_stage_applies_it() {
    let packer = squeeze(&blank_comments_and_strings(&runtime_file(
        "Runtime/FramePacker.cs",
    )));
    let writes = packer.matches("_shade[f + 3] = ordinal;").count();
    assert_eq!(
        writes, 2,
        "FramePacker writes `_shade[f + 3] = ordinal;` {writes} time(s); one rect emission \
         and one glyph emission are expected, so every instance carries the ordinal a core \
         and its fringe share."
    );
    assert!(
        !packer.contains("_shade[f + 3] = 0.0f;"),
        "FramePacker still writes a zero into `_shade[f + 3]` somewhere: that instance \
         would sit at the sheet's own depth, behind every ordinal."
    );

    let hlsl = package_gate::root()
        .join("unity/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl");
    let hlsl = std::fs::read_to_string(&hlsl).expect("DashsceneInstance.hlsl is readable");
    let vertex_at = hlsl.find("DsVaryings DsVertex(").expect("DsVertex exists");
    let vertex = &hlsl[vertex_at..];
    for needle in ["DS_ORDINAL_SPAN", "shade.w", "TransformWViewToHClip"] {
        assert!(
            vertex.contains(needle),
            "DsVertex does not use `{needle}`: the ordinal in `_DsShade.w` never reaches \
             the depth, so no core rejects anything."
        );
    }
}

/// The cores travel in their own draw range, without the sorting flag.
#[test]
fn cores_travel_in_their_own_range_without_the_sorting_flag() {
    let source = painter_source();
    let (s, e) = member_body(&source, CULLING);
    let culling = squeeze(&source[s..=e]);
    assert!(
        culling.contains("Malloc<BatchDrawRange>(2)"),
        "OnPerformCulling allocates one draw range; the cores need a range of their own \
         so the flagged, sorted commands and the depth-ordered core commands do not share \
         one sort."
    );
    assert!(
        culling.contains("flags = BatchDrawCommandFlags.None,"),
        "OnPerformCulling emits no command without `HasSortingPosition`; the cores carry \
         no sorting key, because the depth test orders them (issue #1404)."
    );
    assert!(
        culling.contains("flags = BatchDrawCommandFlags.HasSortingPosition,"),
        "the blended commands lost their sorting flag."
    );
}
