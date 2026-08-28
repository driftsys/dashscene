//! The paint heap is bound on the painter's own materials, never process-wide.
//!
//! **Issue #1297 was `Shader.SetGlobalBuffer`.** `_DsPaints`, `_DsClipBoxes`,
//! `_DsStrokes` and `_DsGlyphs` were bound through the global namespace and
//! `_DsGlobals` through `Shader.SetGlobalVector`, all of which are process-wide
//! — so two `BrgPainter`s in one process shared one heap and the last one to
//! draw supplied the gradients, strokes and clip boxes every painter's
//! fragments shaded from. The painter reported it with a constructor warning
//! rather than drawing a wrong picture quietly, which was a diagnostic and not
//! a fix.
//!
//! **Text, for `painter_diagnostics.rs`'s reason.** `Runtime/Engine/` is
//! compiled by no CI job and constructed by no check that can run here, so what
//! this file asserts is that the calls are still written the way the design
//! says. `just unity-render` is what observes the consequence: it draws
//! `goldens/dsb/v03-paint.dsb` in a built player and judges five of its
//! thirteen sampled node centres against the instance's own colour, which a
//! material-bound `_DsPaints` that did not reach the fragment stage cannot
//! satisfy.
//!
//! **Every assertion about where a call sits is bounded by a member's own
//! braces**, through [`member_body`], and that is not tidiness. A first
//! version of this file asked only that `(PaintMaterialProperties.Paints,`
//! appear somewhere in `Runtime/`, and three review seats each defeated it by
//! running the suite green over a painter that binds nothing: one commented
//! out the single `BindHeap();` call in `Draw`, leaving the bindings intact
//! inside a method nothing reaches; one emptied the loop with `i < 0`, so no
//! text material was bound; one deleted
//! `BindHeapTo(_textMaterials[i], scalars);` outright.
//! `painter_diagnostics.rs` reaches for the same bound against three defeats
//! of its own — a call moved into a dead method after `Draw`, a `== null` kept
//! as a dead local, a switch arm whose `return` became `break` — which is
//! where the bound came from.
//!
//! **Two kinds of assertion here are deliberately not member-bounded**, and
//! each says so where it sits: an absence ("no `SetGlobal…` anywhere in the
//! compiled half") has no member to be bounded to, and the declarations the
//! ids are read from are file-scoped fields rather than members.

use package_gate::cs_scan::{blank_comments_and_strings, member_body};
use package_gate::{other_bound_names, package_cs_files};

/// The class in `Runtime/PaintBindings.cs` that names what a material carries.
const CLASS: &str = "PaintMaterialProperties";

const BINDINGS: &str = "Runtime/PaintBindings.cs";
const PAINTER: &str = "Runtime/Engine/BrgPainter.cs";

const DRAW: &str = "public void Draw(FrameLease lease)";
const BIND_HEAP: &str = "private void BindHeap()";
const BIND_HEAP_TO: &str = "private void BindHeapTo(Material material, Vector4 scalars)";
const DISPOSE: &str = "public void Dispose()";

/// The package's `Runtime/` C#, comments and string bodies blanked.
///
/// Blanked because the names this file looks for appear in prose here: the
/// comments recount what issue #1297 was, and an unblanked scan would report
/// the history as the defect.
fn runtime() -> Vec<(String, String)> {
    let files: Vec<(String, String)> = package_cs_files()
        .into_iter()
        .map(|(path, source)| {
            let blanked = blank_comments_and_strings(&source);
            (path, blanked)
        })
        .collect();
    assert!(!files.is_empty(), "the package ships no Runtime/ C#");
    files
}

/// The painter's source, blanked.
fn painter() -> String {
    runtime()
        .into_iter()
        .find(|(path, _)| path.ends_with(PAINTER))
        .unwrap_or_else(|| panic!("the package no longer ships {PAINTER}"))
        .1
}

/// One member's body, braces matched.
fn body(source: &str, signature: &str) -> String {
    let (start, end) = member_body(source, signature);
    source[start..=end].to_string()
}

/// Every `public const string` member of `Runtime/PaintBindings.cs`, as
/// (member, literal).
///
/// Read from the raw source because the literal is what the blanking removes.
/// A comment carrying the pattern would add a member, not drop one, and an
/// added member fails the assertions it is put through rather than passing
/// them — which is the direction a parse that can be wrong has to fail in.
fn bound_names() -> Vec<(String, String)> {
    let raw = package_cs_files();
    let source = &raw
        .iter()
        .find(|(path, _)| path.ends_with(BINDINGS))
        .unwrap_or_else(|| panic!("the package no longer ships {BINDINGS}"))
        .1;

    let mut out = Vec::new();
    let mut rest = source.as_str();
    while let Some(at) = rest.find("public const string ") {
        rest = &rest[at + "public const string ".len()..];
        let Some(eq) = rest.find(" = \"") else { break };
        let member = rest[..eq].to_string();
        rest = &rest[eq + " = \"".len()..];
        let Some(end) = rest.find('"') else { break };
        out.push((member, rest[..end].to_string()));
        rest = &rest[end..];
    }

    // **The parse is held against the gate's own reader**, so a member it
    // failed to see is a failure here rather than a name silently exempted
    // from every assertion below.
    let mut parsed: Vec<String> = out.iter().map(|(_, literal)| literal.clone()).collect();
    parsed.sort();
    parsed.dedup();
    assert_eq!(
        parsed,
        other_bound_names(&raw),
        "the members parsed out of {BINDINGS} are not the `_Ds…` literals \
         `package_gate::other_bound_names` reads from the same file"
    );
    out
}

/// Nothing in the package binds a paint name process-wide.
///
/// **The whole of `Runtime/`, not `BrgPainter.cs` alone.** The reported defect
/// was five calls in one member; the class is a global setter anywhere in the
/// compiled half, and a binding moved to a helper beside the painter would be
/// the same wrong picture reached through a different file.
///
/// **`SetGlobal`, not `Shader.SetGlobal`.** A `using static UnityEngine.Shader;`
/// with a bare `SetGlobalBuffer(…)` call is the same defect written in two
/// tokens fewer, and the qualified form is also not the only setter: `Shader`
/// exposes a `SetGlobal…` overload for every property type, so a name list
/// would go stale the first time one was used.
///
/// `Runtime/` outside the package is deliberately out of scope, and that is
/// `package_cs_files`'s own documented boundary: a host application setting a
/// global shader property is host code, and `unity/render-gate/`, `demo/` and
/// `Samples~/` are not the compiled half of the package.
#[test]
fn no_paint_binding_in_the_package_is_process_wide() {
    let mut offenders = Vec::new();
    for (path, source) in &runtime() {
        for (at, _) in source.match_indices("SetGlobal") {
            // **`BatchRendererGroup.SetGlobalBounds` is the one exemption**,
            // and it is named rather than pattern-matched away: it sets the
            // culling bounds of one group, is not a shader property, and
            // cannot be a channel through which two painters share anything.
            if source[at..].starts_with("SetGlobalBounds") {
                continue;
            }
            let line = source[..at].matches('\n').count() + 1;
            offenders.push(format!("{path}:{line}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "a `SetGlobal…` setter is process-wide, so two painters in one process \
         share whatever it binds — issue #1297. Found at: {}",
        offenders.join(", ")
    );
}

/// Every name the painter binds is resolved to a property id, in a static
/// field, and that field reaches a setter.
///
/// **Three assertions because the binding grew a third link.** It used to be
/// `name → setter`; it is now `name → id → setter`, and a test over either
/// link alone passes while the other is wrong. A review seat swapped the
/// initializers of `PaintsId` and `ClipBoxesId`: each name still resolved
/// exactly once, each setter call was still written verbatim, and `_DsPaints`
/// named the clip-box table.
///
/// **The storage class, not only the call.** The reason for an id at all is
/// that `Material.SetBuffer(string, …)` hashes its argument on every call and
/// the heap is bound per material per frame; a seat moved the resolution into
/// `BindHeapTo` as a local, which left the occurrence count at one and put the
/// hash back on the per-frame path.
///
/// **And that the id reaches a setter.** The first version of this file
/// asserted that through the name and the rewrite lost it: deleting
/// `materials[i].SetTexture(AtlasId, …)`, or both `SetFloat(CutoffId, …)`
/// calls, left an unused private field — which no job here compiles, so
/// nothing warns about it either.
#[test]
fn every_name_the_painter_binds_reaches_a_setter_through_a_static_property_id() {
    let source = painter();
    let squeezed: String = source.split_whitespace().collect::<Vec<_>>().join(" ");
    let names = bound_names();
    assert!(!names.is_empty(), "{BINDINGS} names nothing");

    for (member, literal) in &names {
        // The id's name is derived from the member's, so the declaration and
        // the call site cannot drift apart without one of the three
        // assertions below naming which.
        let field = format!("{member}Id");
        let declaration =
            format!("private static readonly int {field} = Shader.PropertyToID({CLASS}.{member});");
        assert!(
            squeezed.contains(&declaration),
            "{PAINTER} does not declare `{declaration}`. `{literal}` is \
             resolved somewhere else, or not at all, or into something other \
             than a static field — a local resolves the name again on every \
             call, which is the cost the id exists to remove."
        );

        let resolutions = source
            .matches(&format!("PropertyToID({CLASS}.{member})"))
            .count();
        assert_eq!(
            resolutions, 1,
            "{PAINTER} resolves `{CLASS}.{member}` {resolutions} time(s), not \
             once. Two resolutions of one name are two places to change it, \
             and a second one under a different field name is how the id and \
             the buffer come apart."
        );

        let uses = source.matches(&format!("({field}, ")).count();
        assert!(
            uses >= 1,
            "{PAINTER} declares `{field}` and passes it to no setter, so \
             `{literal}` binds nothing and the shading reads an unbound \
             buffer, an unbound sampler or a default."
        );
    }
}

/// `Draw` binds the heap, on the class material and on every text material.
///
/// **Three bounded assertions rather than one loose one**, because each of the
/// three was defeated on its own: the call site can be removed while the
/// bindings stay, the loop can be emptied while its body stays, and the class
/// material's binding can go while the loop's stays.
#[test]
fn draw_binds_the_heap_on_every_material_the_painter_registered() {
    let source = painter();

    let draw = body(&source, DRAW);
    let bound_at = draw.find("BindHeap();").unwrap_or_else(|| {
        panic!(
            "{PAINTER}'s `Draw` does not call `BindHeap()`. The bindings then \
             sit in a member nothing reaches, every draw reads an unbound \
             `StructuredBuffer`, and the frame is blank."
        )
    });
    let uploaded_at = draw
        .find("UploadHeap();")
        .unwrap_or_else(|| panic!("{PAINTER}'s `Draw` does not call `UploadHeap()`"));
    assert!(
        uploaded_at < bound_at,
        "{PAINTER}'s `Draw` binds the heap at {bound_at} and uploads it at \
         {uploaded_at}. `Upload` disposes and re-creates a `GraphicsBuffer` \
         when its table outgrows it, so a binding taken first names a freed \
         buffer from the first growth onward — which is the reason `BindHeap` \
         runs on every frame at all."
    );

    let bind = body(&source, BIND_HEAP);
    assert!(
        bind.contains("BindHeapTo(_material, scalars);"),
        "{PAINTER}'s `BindHeap` does not bind the class material."
    );
    assert!(
        bind.contains("for (var i = 0; i < _textMaterials.Length; i++)"),
        "{PAINTER}'s `BindHeap` does not walk `_textMaterials`. A text \
         material minted by `SetAtlases` then draws with `_DsPaints`, \
         `_DsClipBoxes` and `_DsStrokes` unbound — which needed no binding of \
         its own while the heap was global, and is the gap issue #1297's fix \
         opened."
    );
    assert!(
        bind.contains("BindHeapTo(_textMaterials[i], scalars);"),
        "{PAINTER}'s `BindHeap` walks `_textMaterials` and does not bind them."
    );
}

/// Each name is bound to the buffer that name means.
///
/// **The pairing, not the name.** Swapping the second arguments of two
/// `SetBuffer` calls, or `SolidBase` with `GradientBase` in the `Vector4`,
/// leaves every name bound and draws a wrong picture. What a wrong base costs
/// is measured, though by a smaller mutation than either of those: the poison
/// run in `docs/design/unity-csharp-host.md` wrote the solid base one row high
/// and 13 inked node centres fell to 11, with the per-instance colour
/// advantage going from 0.599 to -0.109. Seeing that took a Unity editor and a
/// player build. This is what sees it here.
#[test]
fn each_name_is_bound_to_the_buffer_it_means() {
    let bind_to = body(&painter(), BIND_HEAP_TO);

    for call in [
        "material.SetBuffer(PaintsId, _paintBuffer);",
        "material.SetBuffer(ClipBoxesId, _clipBuffer);",
        "material.SetBuffer(StrokesId, _strokeBuffer);",
        "material.SetVector(ScalarsId, scalars);",
    ] {
        assert!(
            bind_to.contains(call),
            "{PAINTER}'s `BindHeapTo` does not contain `{call}`. Each name \
             carries one table and a swapped pair is a wrong picture rather \
             than a failure."
        );
    }

    // Squeezed, because the argument list's wrapping is the formatter's and
    // not a property of the binding.
    let bind = body(&painter(), BIND_HEAP);
    let squeezed: String = bind.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        squeezed.contains("EdgeWidth, _packer.SolidBase, _packer.GradientBase, 0.0f"),
        "{PAINTER}'s `BindHeap` does not build `_DsGlobals` as `(aa, solid \
         base, gradient base, unused)`, which is the order the shading reads \
         it in."
    );

    // **Both sides of the contract, because only one of them is C#.** The
    // order above is meaningless on its own: swapping `_DsGlobals.y` and
    // `_DsGlobals.z` in the shading makes every solid fill read a gradient row
    // and every gradient read a solid one, with the painter unchanged and this
    // file — until this assertion — green.
    let shading: String = package_gate::hlsl_sources()
        .iter()
        .map(|(_, source)| source.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for (component, read) in [
        ("y", "_DsPaints[(uint)_DsGlobals.y + row]"),
        ("z", "(uint)_DsGlobals.z + row * DS_GRADIENT_WORDS"),
    ] {
        assert!(
            shading.contains(read),
            "the shading does not read `{read}`, so `_DsGlobals.{component}` \
             no longer indexes the table the painter writes into that \
             component. The two sides of this contract are a wrong picture \
             when they disagree, not a failure."
        );
    }
}

/// The glyph rows go on the text materials and on no other.
///
/// The shading declares `_DsGlyphs` under `DASHSCENE_CLASS_TEXT`, so the three
/// node classes never reach it — and binding a buffer a material's shader does
/// not declare is a silent no-op that reads as a binding.
#[test]
fn the_glyph_rows_are_bound_on_the_text_materials_alone() {
    let source = painter();
    let bind = body(&source, BIND_HEAP);

    assert!(
        bind.contains("_textMaterials[i].SetBuffer(GlyphsId, _glyphBuffer);"),
        "{PAINTER}'s `BindHeap` does not bind the glyph rows on each text \
         material."
    );
    // **Absent from `BindHeapTo`, rather than absent under one spelling.** A
    // review seat put `material.SetBuffer(GlyphsId, _glyphBuffer);` into
    // `BindHeapTo` — the natural place, since that is the shared per-material
    // bind — and an assertion naming the receiver `_material.` did not see it,
    // because the parameter is called `material`. No spelling of a receiver is
    // worth naming: the rows do not belong in the member every material goes
    // through.
    let bind_to = body(&source, BIND_HEAP_TO);
    assert!(
        !bind_to.contains("GlyphsId"),
        "{PAINTER}'s `BindHeapTo` names `GlyphsId`, and every material goes \
         through it — including the class material, whose shaders declare no \
         `_DsGlyphs`."
    );
    for (at, _) in bind.match_indices("GlyphsId") {
        let line_start = bind[..at].rfind('\n').map_or(0, |n| n + 1);
        let line = bind[line_start..at].trim();
        assert!(
            line.contains("_textMaterials["),
            "{PAINTER}'s `BindHeap` binds `GlyphsId` on a receiver that is not \
             `_textMaterials[…]`: `{line}`"
        );
    }
}

/// Every material naming a heap buffer is destroyed before that buffer is.
///
/// **This ordering replaced code.** `Dispose` used to unbind the four buffers
/// from the global namespace before freeing them, because a disposed painter
/// would otherwise leave `_DsPaints` naming released native memory for
/// anything drawing a `Dashscene/*` material afterwards. Per-material binding
/// removes the need — the only materials naming these buffers are this
/// painter's own — but only while they are destroyed first, and
/// `DestroyImmediate` is what makes "destroyed" synchronous.
#[test]
fn the_heap_buffers_are_freed_after_the_materials_that_name_them() {
    let dispose = body(&painter(), DISPOSE);

    let released = dispose
        .find("ReleaseUnityObjects();")
        .unwrap_or_else(|| panic!("{PAINTER}'s `Dispose` does not call `ReleaseUnityObjects()`"));
    let atlases = dispose
        .find("ReleaseAtlases();")
        .unwrap_or_else(|| panic!("{PAINTER}'s `Dispose` does not call `ReleaseAtlases()`"));
    // **Every buffer the painter frees, not the first one.** A review seat
    // moved three of the four heap buffers above the two release calls and
    // left `_paintBuffer` where it was, which an assertion over one buffer
    // cannot see.
    for buffer in [
        "_instanceBuffer",
        "_paintBuffer",
        "_clipBuffer",
        "_strokeBuffer",
        "_glyphBuffer",
    ] {
        let freed = dispose
            .find(&format!("{buffer}?.Dispose();"))
            .unwrap_or_else(|| panic!("{PAINTER}'s `Dispose` does not free `{buffer}`"));
        assert!(
            released < freed && atlases < freed,
            "{PAINTER}'s `Dispose` frees `{buffer}` at {freed}, before \
             destroying the materials that name it (ReleaseAtlases at \
             {atlases}, ReleaseUnityObjects at {released}). That is the hazard \
             the deleted global unbinds used to cover, reinstated with nothing \
             covering it."
        );
    }
}
