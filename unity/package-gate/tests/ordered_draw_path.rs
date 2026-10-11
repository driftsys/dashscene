//! The ordered draw path: how the overlay class and text reach the screen in
//! document order, outside BatchRendererGroup (story #1448).
//!
//! **What is pinned.** The pass is injected by the package itself, from
//! `RenderPipelineManager.beginCameraRendering`, so a host draws the overlay
//! class without adding a renderer feature to its renderer asset — the #1313
//! class of defect, a package that cannot draw as installed. The instance rows
//! the procedural variant reads are uploaded by their own member, never by
//! `UploadInstances`, and ranged on a partial commit the way the BRG buffer
//! is. The materials on the path carry the `DASHSCENE_PROCEDURAL` keyword, and
//! no other material does.
//!
//! **Text, for the reason every scan beside this one gives.** No CI job
//! compiles `Runtime/Engine/`. `just unity-editor` compiles it against URP and
//! `just unity-render` draws through it: its order phase composites a fixture
//! whose picture is decided by draw order, which is the behaviour this text
//! cannot observe.

use package_gate::cs_scan::{blank_comments_and_strings, member_body, squeeze};
use package_gate::{PAINTER_PATH as PAINTER, package_cs_files, painter_source, root};

const PASS: &str = "Runtime/Engine/OrderedDrawPass.cs";
const ROUTING: &str = "Runtime/DrawRouting.cs";
const UPLOAD: &str = "private void UploadOrdered()";
const ASMDEF: &str = "unity/com.driftsys.dashscene/Runtime/Driftsys.Dashscene.Runtime.asmdef";

/// The pass's source, comments and string bodies blanked.
fn pass_source() -> String {
    let files = package_cs_files();
    let source = &files
        .iter()
        .find(|(path, _)| path.ends_with(PASS))
        .unwrap_or_else(|| panic!("the package no longer ships {PASS}"))
        .1;
    blank_comments_and_strings(source)
}

/// `DrawRouting`'s source, comments and string bodies blanked.
fn routing_source() -> String {
    let files = package_cs_files();
    let source = &files
        .iter()
        .find(|(path, _)| path.ends_with(ROUTING))
        .unwrap_or_else(|| panic!("the package no longer ships {ROUTING}"))
        .1;
    blank_comments_and_strings(source)
}

fn squeezed_body(source: &str, signature: &str) -> String {
    let (s, e) = member_body(source, signature);
    squeeze(&source[s..=e])
}

/// The pass enqueues itself on game and scene cameras, and only while a
/// registered painter has runs to draw.
#[test]
fn the_pass_is_injected_by_the_package_for_game_and_scene_cameras() {
    let pass = pass_source();
    let squeezed = squeeze(&pass);

    assert!(
        !squeezed.contains("ScriptableRendererFeature"),
        "{PASS} declares a renderer feature. A feature must be added to every \
         host's renderer asset, which the package cannot do for the host."
    );
    assert!(
        squeezed.contains("renderPassEvent = RenderPassEvent.AfterRenderingTransparents;"),
        "{PASS} no longer records after the transparent queue."
    );

    // **When to subscribe is `PainterRegistry`'s answer**, which
    // `unity/ffi-check` executes: `Add` is true exactly on the first painter
    // and `Remove` exactly on the last. What is pinned here is that the pass
    // acts on those answers and on nothing else — each body is that one
    // statement, so a second subscription or an unconditional one is a
    // different body.
    assert!(
        squeezed.contains(
            "private static readonly PainterRegistry<BrgPainter> s_painters = new PainterRegistry<BrgPainter>();"
        ),
        "{PASS} no longer keeps its painters in a `PainterRegistry`, whose \
         subscription answers `unity/ffi-check` executes."
    );
    let register = squeezed_body(&pass, "public static void Register(BrgPainter painter)");
    assert_eq!(
        register,
        "{ if (s_painters.Add(painter)) { RenderPipelineManager.beginCameraRendering += OnBeginCameraRendering; } }",
        "{PASS}'s Register no longer subscribes exactly when the registry \
         reports its first painter."
    );
    let unregister = squeezed_body(&pass, "public static void Unregister(BrgPainter painter)");
    assert_eq!(
        unregister,
        "{ if (s_painters.Remove(painter)) { RenderPipelineManager.beginCameraRendering -= OnBeginCameraRendering; } }",
        "{PASS}'s Unregister no longer unsubscribes exactly when the registry \
         reports its last painter left."
    );
    assert_eq!(
        (
            squeezed.matches("beginCameraRendering +=").count(),
            squeezed.matches("s_painters.Add(").count(),
        ),
        (1, 1),
        "{PASS} subscribes, or registers a painter, outside `Register`."
    );

    // **Reset under Enter Play Mode without a domain reload**, where statics
    // survive from the previous session: an undisposed painter would stay
    // registered and keep drawing old content or destroyed materials.
    assert!(
        squeezed.contains(
            "[RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)] \
             private static void ResetOnEnterPlayMode()"
        ),
        "{PASS} no longer resets its statics at `SubsystemRegistration`, so a \
         session entered without a domain reload keeps the last one's painters."
    );
    let reset = squeezed_body(&pass, "private static void ResetOnEnterPlayMode()");
    assert_eq!(
        reset,
        "{ RenderPipelineManager.beginCameraRendering -= OnBeginCameraRendering; s_painters.Clear(); }",
        "{PASS}'s reset no longer unsubscribes and forgets every painter."
    );

    let begin = squeezed_body(
        &pass,
        "private static void OnBeginCameraRendering(ScriptableRenderContext context, Camera camera)",
    );
    // **Each guard with its `return`, and every guard before the enqueue.**
    // A condition that survives with its `return` deleted filters nothing, so
    // the needle is the whole statement; and a guard after `EnqueuePass` is
    // one the pass has already passed.
    let enqueue = begin
        .find(".EnqueuePass(s_pass);")
        .unwrap_or_else(|| panic!("{PASS}'s OnBeginCameraRendering no longer enqueues the pass"));
    for guard in [
        "{ if (camera.cameraType != CameraType.Game && camera.cameraType != CameraType.SceneView) { return; }",
        "if ((camera.cullingMask & (1 << BrgPainter.DocumentLayer)) == 0) { return; }",
        "if (!AnyRuns()) { return; }",
    ] {
        let at = begin.find(guard).unwrap_or_else(|| {
            panic!("{PASS}'s OnBeginCameraRendering no longer returns on `{guard}`.")
        });
        assert!(
            at < enqueue,
            "{PASS}'s OnBeginCameraRendering tests `{guard}` after it enqueues the pass."
        );
    }
    assert_eq!(
        begin.matches("return;").count(),
        3,
        "{PASS}'s OnBeginCameraRendering returns other than on its three guards."
    );
    assert!(
        begin.contains("camera.GetUniversalAdditionalCameraData().scriptableRenderer"),
        "{PASS}'s OnBeginCameraRendering no longer enqueues on the camera's URP renderer."
    );

    // **The layer is one constant on both paths.** BRG's filter and the
    // pass's mask test name `DocumentLayer`, so a camera that culls the
    // document's layer draws it on neither path.
    let painter = painter_source();
    let squeezed_painter = squeeze(&painter);
    assert!(
        squeezed_painter.contains("internal const int DocumentLayer = 0;")
            && squeezed_painter.contains("layer = DocumentLayer,"),
        "{PAINTER} no longer puts BRG's commands on `DocumentLayer`, the \
         layer the ordered pass tests the camera's culling mask against."
    );

    let record = squeezed_body(
        &pass,
        "public override void RecordRenderGraph(RenderGraph renderGraph, ContextContainer frameData)",
    );
    for needle in [
        "for (var i = 0; i < s_painters.Count; i++) { var painter = s_painters[i]; \
         if (painter.OrderedRuns.Count == 0) { continue; }",
        "builder.SetRenderAttachment(resourceData.activeColorTexture, 0);",
        "builder.AllowPassCulling(false);",
    ] {
        assert!(
            record.contains(needle),
            "{PASS}'s RecordRenderGraph no longer contains `{needle}`."
        );
    }
}

/// A painter registers once it is built and unregisters when it is disposed,
/// and releasing an atlas set drops the runs cut against it.
#[test]
fn a_painter_registers_while_it_lives_and_drops_stale_runs() {
    let painter = painter_source();

    let (s, e) = member_body(
        &painter,
        "public BrgPainter(MaterialClass materialClass = MaterialClass.UnlitOverlay)",
    );
    let constructor = &painter[s..=e];
    assert_eq!(
        constructor
            .matches("OrderedDrawPass.Register(this);")
            .count(),
        1,
        "{PAINTER}'s constructor does not register the painter with the ordered pass once."
    );
    // **After the group is built, so a rung-3 painter is never registered**
    // (R7): its constructor returns in the `UnsupportedByUnderlyingGraphicsApi`
    // arm, before this point.
    let built = constructor
        .find("_brg = new BatchRendererGroup(")
        .expect("the group is built in the constructor");
    let registered = constructor.find("OrderedDrawPass.Register(this);").unwrap();
    assert!(
        built < registered,
        "{PAINTER}'s constructor registers before the group is built."
    );

    let dispose = squeezed_body(&painter, "public void Dispose()");
    assert!(
        dispose.contains("OrderedDrawPass.Unregister(this);"),
        "{PAINTER}'s Dispose no longer unregisters from the ordered pass, so \
         the pass would draw with a disposed painter's destroyed materials."
    );

    // **In `ReleaseAtlases`, which `SetAtlases`, `Dispose` and a stale set's
    // drop in `Draw` all go through.**
    // **Its first statement, so no condition can skip it.**
    let release = squeezed_body(&painter, "private void ReleaseAtlases()");
    assert!(
        release.starts_with("{ _runs.Clear(); "),
        "{PAINTER}'s ReleaseAtlases no longer drops the runs as its first, \
         unconditional statement. A run names a text material by reference, \
         and the set is about to be destroyed."
    );
    let cleared = release.find("_runs.Clear();").unwrap();
    let destroyed = release.find("DestroyImmediate(").unwrap();
    assert!(
        cleared < destroyed,
        "{PAINTER}'s ReleaseAtlases destroys the text materials before it drops \
         the runs that name them."
    );
}

/// The interleaved rows are uploaded by their own member, whole on a full
/// pack and by dirty range on a partial one.
///
/// **Its own member, never `UploadInstances`** (R5): `StreamLayout` is
/// batch-window arithmetic with a head per batch, and the interleaved buffer
/// has neither, so a dirty range is the words
/// `[Offset * 20, (Offset + Count) * 20)`. `dirty_range_upload.rs` pins
/// `UploadInstances` exactly and is left to do so.
#[test]
fn the_ordered_rows_are_uploaded_whole_or_by_dirty_range() {
    let painter = painter_source();
    let upload = squeezed_body(&painter, UPLOAD);

    let decision = "var ranged = InstanceUpload.CanSendRanges( _packer.LastPackWasPartial, \
                    _haveUploadedOrdered, _uploadedOrderedGeneration, _packer.PackedGeneration, \
                    ReferenceEquals(held, _orderedBuffer), true);";
    assert!(
        upload.contains(decision),
        "{PAINTER}'s {UPLOAD} no longer delegates the ranged decision as \
         `{decision}`. The interleaved buffer has no head, so `sameHead` is \
         true; every other reading is the ordered buffer's own."
    );
    assert!(
        upload.contains("var held = _orderedBuffer; EnsureOrderedCapacity(InstanceCount);"),
        "{PAINTER}'s {UPLOAD} no longer reads the buffer before it can be \
         replaced, so a reallocated buffer could be sent only its dirty rows."
    );

    let (s, e) = member_body(&painter, UPLOAD);
    let body = &painter[s..=e];
    let (ws, we) = member_body(body, "if (!ranged)");
    let whole = squeeze(&body[ws..=we]);
    assert!(
        whole.contains("Interleave(0, InstanceCount);")
            && whole.contains("SendOrdered(0, InstanceCount);"),
        "{PAINTER}'s {UPLOAD} no longer interleaves and sends every live row \
         on the whole path."
    );
    let (rs, re) = member_body(body, "for (var r = 0; r < ranges.Count; r++)");
    let ranged = squeeze(&body[rs..=re]);
    let interleave = ranged
        .find("Interleave(range.Offset, range.Count);")
        .unwrap_or_else(|| panic!("{PAINTER}'s ranged loop no longer interleaves the dirty rows"));
    let send = ranged
        .find("SendOrdered(range.Offset, range.Count);")
        .unwrap_or_else(|| panic!("{PAINTER}'s ranged loop no longer sends the dirty rows"));
    assert!(
        interleave < send,
        "{PAINTER}'s ranged loop sends the rows before it writes them."
    );

    // **One writer of the buffer**, as `UploadRows` is for the BRG buffer, so
    // what `LastUpload` reports is what was sent.
    let send_body = squeezed_body(&painter, "private void SendOrdered(int first, int count)");
    assert!(
        send_body.contains(
            "_orderedBuffer.SetData(_orderedStaging, first * OrderedRowWords, first * OrderedRowWords, count * OrderedRowWords);"
        ),
        "{PAINTER}'s SendOrdered no longer sends rows `[first, first + count)` \
         at 20 words a row."
    );
    assert_eq!(
        painter.matches("_orderedBuffer.SetData(").count(),
        1,
        "{PAINTER} writes the ordered buffer outside SendOrdered."
    );

    let (us, ue) = member_body(&painter, "private void UploadInstances()");
    assert!(
        !painter[us..=ue].contains("_ordered"),
        "{PAINTER}'s UploadInstances touches the ordered buffer."
    );

    // **An ordered painter reports this upload**, so the render gate's
    // `JudgeInstanceUpload` reads the transfer this painter actually makes.
    assert!(
        upload
            .contains("if (PathFor(_materialClass) == DrawPath.Ordered) { LastUpload = upload; }"),
        "{PAINTER}'s {UPLOAD} no longer reports itself as `LastUpload` on an \
         ordered painter."
    );
    // **One constant, owned by `DrawRouting`**, which `unity/ffi-check`
    // executes; the painter's own is that one by name.
    let row_words = package_gate::cs_const_int(&routing_source(), "OrderedRowWords");
    assert_eq!(
        row_words,
        Some(20),
        "{ROUTING}'s `OrderedRowWords` is not 20: five `float4`/`uint4` \
         properties, 80 bytes, the `DsInstance` stride."
    );
    assert!(
        squeeze(&painter)
            .contains("private const int OrderedRowWords = DrawRouting.OrderedRowWords;"),
        "{PAINTER}'s `OrderedRowWords` is no longer `DrawRouting.OrderedRowWords`, \
         so the buffer's stride and the interleave's row could disagree."
    );
}

/// A grown ordered buffer is the one the next draw reads.
///
/// **When to grow, and to what, is `DrawRouting.OrderedCapacity`'s**, which
/// `unity/ffi-check` executes. This test pins that the painter asks it with
/// the buffer's own capacity and returns on its `0`, rather than growing
/// regardless or never.
///
/// **The buffer is created in one place, and bound on the block in the same
/// unbroken sequence.** The block is what the pass draws with, so a buffer
/// created and not bound leaves the block naming the one just disposed — a
/// draw from freed memory, which no gate here observes: `just unity-render`
/// draws one static document and never grows the buffer.
#[test]
fn a_grown_ordered_buffer_is_the_one_the_block_binds() {
    let painter = painter_source();
    let grow = squeezed_body(&painter, "private void EnsureOrderedCapacity(int wanted)");
    let replaced = "_orderedBuffer?.Dispose(); _orderedBuffer = new GraphicsBuffer( \
                    GraphicsBuffer.Target.Structured, capacity, OrderedRowWords * 4); \
                    _orderedStaging = new uint[capacity * OrderedRowWords]; \
                    _orderedBlock.SetBuffer(InstancesId, _orderedBuffer); }";
    assert!(
        grow.ends_with(replaced),
        "{PAINTER}'s EnsureOrderedCapacity no longer ends by replacing the \
         buffer and binding the new one on the block, in one unbroken \
         sequence: `{replaced}`."
    );
    assert_eq!(
        grow,
        format!(
            "{{ var capacity = DrawRouting.OrderedCapacity(_orderedBuffer?.count ?? 0, wanted); \
             if (capacity == 0) {{ return; }} {replaced}"
        ),
        "{PAINTER}'s EnsureOrderedCapacity is no longer: ask \
         `DrawRouting.OrderedCapacity` for the capacity of the current buffer, \
         return on 0, then replace the buffer and bind the new one, once each."
    );
    for (needle, what) in [
        (
            "_orderedBuffer = new GraphicsBuffer(",
            "creates the ordered buffer",
        ),
        (
            "_orderedBlock.SetBuffer(",
            "binds a buffer on the ordered block",
        ),
    ] {
        assert_eq!(
            painter.matches(needle).count(),
            1,
            "{PAINTER} {what} outside EnsureOrderedCapacity, so a buffer can \
             exist that the block does not name, or the block can name one \
             that is not this painter's current buffer."
        );
    }
}

/// The interleaved row is `DsInstance` in field order, and the procedural
/// draw reads the row its run names, placed by the sheet matrix.
///
/// **A permutation keeps every width.** Writing Shade where Corners belongs
/// is still twenty words a row, and only a draw on the one graphics API `just
/// unity-render` measures could see it, if the fixture happens to read both.
#[test]
fn the_ordered_row_is_ds_instance_in_field_order() {
    const HLSL: &str = "unity/com.driftsys.dashscene/Runtime/Shaders/DashsceneInstance.hlsl";
    let hlsl = std::fs::read_to_string(root().join(HLSL))
        .unwrap_or_else(|e| panic!("cannot read {HLSL}: {e}"));

    let open = hlsl
        .find("struct DsInstance")
        .unwrap_or_else(|| panic!("{HLSL} no longer declares `struct DsInstance`"));
    let body = &hlsl[open..];
    let body = &body[body.find('{').unwrap() + 1..body.find("};").unwrap()];
    let fields: Vec<String> = body
        .lines()
        .filter_map(|line| line.trim().strip_suffix(';'))
        .map(|decl| decl.split_whitespace().last().unwrap().to_string())
        .collect();
    assert_eq!(
        fields,
        ["Quad", "Corners", "Shade", "Pivot", "Paint"],
        "{HLSL}'s `DsInstance` fields changed. The interleave below and this \
         list must move together."
    );

    // **The painter hands the five streams to `DrawRouting.InterleaveRows` in
    // this order and nothing else**; `unity/ffi-check` executes the rows it
    // writes, and the scan below reads which stream lands at which offset.
    let painter = painter_source();
    assert_eq!(
        squeezed_body(&painter, "private void Interleave(int first, int count)"),
        "{ DrawRouting.InterleaveRows( _packer.Quad, _packer.Corners, _packer.Shade, \
         _packer.Pivot, _packer.Paint, _orderedStaging, first, count); }",
        "{PAINTER}'s Interleave is no longer one call that hands the packer's \
         five streams, in `DsInstance`'s order, to `DrawRouting.InterleaveRows`."
    );

    // `staging[row + K + k] = …stream[at + k]…;`, K the word offset of the
    // stream in the row, absent for the first.
    let routing = routing_source();
    let params: Vec<String> = {
        let head = squeezed_body(&routing, "internal static void InterleaveRows(");
        assert!(
            head.contains("staging["),
            "{ROUTING}'s InterleaveRows writes no staging row"
        );
        let sig = squeeze(&routing);
        let at = sig.find("InterleaveRows(").unwrap();
        let list = &sig[at + "InterleaveRows(".len()..];
        list[..list.find(')').unwrap()]
            .split(',')
            .map(|p| p.trim().to_string())
            .collect()
    };
    assert_eq!(
        params,
        [
            "float[] quad",
            "float[] corners",
            "float[] shade",
            "float[] pivot",
            "uint[] paint",
            "uint[] staging",
            "int first",
            "int count"
        ],
        "{ROUTING}'s InterleaveRows parameters changed; the painter's call and \
         this scan must move together."
    );
    let interleave = squeezed_body(&routing, "internal static void InterleaveRows(");
    let mut written: Vec<(usize, String)> = Vec::new();
    for statement in interleave.split(';') {
        const TARGET: &str = "staging[row + ";
        let Some(at) = statement.find(TARGET) else {
            continue;
        };
        let lhs = &statement[at + TARGET.len()..];
        let offset = if lhs.starts_with("k]") {
            0
        } else {
            let (k, _) = lhs.split_once(" + k]").unwrap_or_else(|| {
                panic!(
                    "{ROUTING}'s InterleaveRows writes a row word it cannot place: `{statement}`"
                )
            });
            k.parse::<usize>().unwrap_or_else(|_| {
                panic!(
                    "{ROUTING}'s InterleaveRows writes at an offset that is not a number: `{statement}`"
                )
            })
        };
        let rhs = statement.split_once("] = ").map(|(_, r)| r).unwrap_or("");
        let stream = rhs
            .trim_start_matches("Bits(")
            .split('[')
            .next()
            .unwrap_or("");
        let field = match stream {
            "quad" => "Quad",
            "corners" => "Corners",
            "shade" => "Shade",
            "pivot" => "Pivot",
            "paint" => "Paint",
            _ => panic!(
                "{ROUTING}'s InterleaveRows writes a row word from no packer stream: `{statement}`"
            ),
        };
        written.push((offset, field.to_string()));
    }
    written.sort();
    let expected: Vec<(usize, String)> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| (i * 4, f.clone()))
        .collect();
    assert_eq!(
        written, expected,
        "{ROUTING}'s InterleaveRows does not write the five streams at `DsInstance`'s \
         field offsets, four words each, in field order."
    );

    // **The row a run's instance reads**: its run's first row plus the
    // instance id, the first set by the pass from `run.Offset`.
    //
    // **Read from the code of the `DASHSCENE_PROCEDURAL` span that declares
    // `DsInstance`**, with comments dropped: a needle over the whole file
    // matches a read commented out or one outside the keyword.
    assert!(
        !hlsl.contains("/*"),
        "{HLSL} has a block comment; this scan drops line comments only"
    );
    let span_start = hlsl[..open]
        .rfind("#ifdef DASHSCENE_PROCEDURAL")
        .unwrap_or_else(|| {
            panic!("{HLSL}'s `DsInstance` is not under `#ifdef DASHSCENE_PROCEDURAL`")
        });
    let span_end = open
        + hlsl[open..]
            .find("\n#endif")
            .expect("the procedural span never ends");
    let code: String = hlsl[span_start..span_end]
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.lines()
            .skip(1)
            .map(str::trim_start)
            .all(|line| !line.starts_with('#') || line.starts_with("#define ")),
        "{HLSL}'s procedural span now holds a preprocessor directive other than \
         `#define` (`#if`, `#elif`, `#else`, ...); this scan reads every line of the span as live \
         code and assumes the first `#endif` closes it."
    );
    let read = squeeze(&code);
    assert!(
        read.contains(
            "void DsLoad(uint instanceId) { ds_Instance = _DsInstances[(uint)_DsFirstInstance + instanceId]; }"
        ),
        "{HLSL}'s procedural DsLoad no longer adds the run's first row \
         `_DsFirstInstance` to the instance id in code."
    );

    // **Each name reads its own field, whole** (story #1450). `DsVertex` and
    // `DsShade` read the row through these five names, and the occlusion piece
    // word is `_DsShade.w`: a `#define` that names the wrong field, or rebuilds
    // `Shade` with a constant fourth lane, compiles and keeps every row width,
    // and every piece of an overlay rect would then draw as its whole box.
    for field in &fields {
        let name = format!("_Ds{field}");
        let defines: Vec<&str> = code
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.starts_with("#define ")
                    && line.split_whitespace().nth(1) == Some(name.as_str())
            })
            .collect();
        assert_eq!(
            defines.len(),
            1,
            "{HLSL}'s procedural span defines `{name}` {} time(s), not once.",
            defines.len()
        );
        let replacement = defines[0]
            .split_whitespace()
            .skip(2)
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            replacement,
            format!("ds_Instance.{field}"),
            "{HLSL}'s procedural `{name}` is not the row's own `{field}` field, \
             read whole."
        );
    }

    // **Each stage loads the row of its own instance id**: the vertex stage
    // from the attribute, the fragment stage from the varying the vertex stage
    // wrote. `DsLoad(0)` would read row 0 for every instance.
    let stages = squeeze(
        &hlsl
            .lines()
            .map(|line| line.split("//").next().unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    for needle in [
        "DsVaryings DsVertex(DsAttributes input) { #ifdef DASHSCENE_PROCEDURAL DsLoad(input.instanceID); DsVaryings output",
        "float4 DsShade(DsVaryings input) { #ifdef DASHSCENE_PROCEDURAL DsLoad(input.instance); #else",
    ] {
        assert!(
            stages.contains(needle),
            "{HLSL} no longer contains `{needle}` in code: a stage's procedural \
             branch must load the row of its own instance."
        );
    }

    // **The sheet the runs are drawn with is the document-to-world `Draw`
    // cut them under**, the matrix the BRG path's heads carry.
    let squeezed_painter = squeeze(&painter);
    for needle in [
        "_orderedSheet = DocumentToWorld; CutRuns();",
        "internal Matrix4x4 OrderedSheet => _orderedSheet;",
    ] {
        assert!(
            squeezed_painter.contains(needle),
            "{PAINTER} no longer contains `{needle}`."
        );
    }
    assert_eq!(
        package_gate::cs_scan::assignment_count(&painter, "_orderedSheet"),
        2,
        "{PAINTER} assigns `_orderedSheet` other than in its identity \
         initializer and once in Draw."
    );
    let pass = squeeze(&pass_source());
    assert!(
        pass.contains("data.Sheet = painter.OrderedSheet;"),
        "{PASS} no longer draws a painter's runs with that painter's sheet."
    );
}

/// The ordered materials carry `DASHSCENE_PROCEDURAL`, and a lit class
/// material does not.
#[test]
fn the_ordered_materials_and_only_those_take_the_procedural_variant() {
    let raw = package_cs_files()
        .into_iter()
        .find(|(path, _)| path.ends_with(PAINTER))
        .unwrap()
        .1;
    assert!(
        squeeze(&raw)
            .contains("private const string ProceduralKeyword = \"DASHSCENE_PROCEDURAL\";"),
        "{PAINTER} no longer names the keyword the two blended shaders declare."
    );

    let painter = painter_source();
    // **Through `Declares`, the one `LocalKeyword` construction site, and the
    // refusal is a throw.** `SetKeyword` on an invalid `LocalKeyword` does
    // nothing, and the material would draw without a mesh through the mesh
    // variant — a blank picture. A warning in place of the throw would let
    // the constructor finish and the painter report success.
    let enable = squeezed_body(
        &painter,
        "private static void EnableProcedural(Material material)",
    );
    let refused = "{ if (!Declares(material, ProceduralKeyword, out var keyword)) { throw new \
                   DashscenePainterException(";
    assert!(
        enable.starts_with(refused),
        "{PAINTER}'s EnableProcedural no longer refuses, by throwing, a shader \
         whose `Declares` says it lacks the keyword: expected `{refused}`."
    );
    assert!(
        enable.ends_with("material.SetKeyword(keyword, true); }"),
        "{PAINTER}'s EnableProcedural no longer sets the keyword as a LOCAL \
         keyword on the material, after the refusal."
    );
    assert_eq!(
        painter.matches("new LocalKeyword(").count(),
        1,
        "{PAINTER} constructs a `LocalKeyword` outside `Declares`."
    );

    let (s, e) = member_body(
        &painter,
        "public BrgPainter(MaterialClass materialClass = MaterialClass.UnlitOverlay)",
    );
    let constructor = squeeze(&painter[s..=e]);
    assert!(
        constructor.contains(
            "if (PathFor(materialClass) == DrawPath.Ordered) { EnableProcedural(_material); }"
        ),
        "{PAINTER}'s constructor no longer enables the procedural variant on \
         the class material of an ordered class, and on that class alone."
    );
    assert!(
        constructor.contains(
            "if (PathFor(materialClass) == DrawPath.Brg) { _materialId = _brg.RegisterMaterial(_material); }"
        ),
        "{PAINTER}'s constructor registers the class material with the group \
         on a class that does not draw through it."
    );

    let set_atlases = squeezed_body(&painter, "public void SetAtlases(TextAtlasSet atlases)");
    assert!(
        set_atlases.contains("EnableProcedural(materials[i]);"),
        "{PAINTER}'s SetAtlases no longer enables the procedural variant on \
         every text material."
    );
    assert!(
        !painter.contains("RegisterMaterial(materials[i])"),
        "{PAINTER} registers a text material with the group. Text draws on the \
         ordered path in every painter."
    );
}

/// The runtime assembly references URP and SRP core, which the pass is
/// written against.
#[test]
fn the_runtime_assembly_references_the_pipeline_it_records_into() {
    let asmdef = std::fs::read_to_string(root().join(ASMDEF))
        .unwrap_or_else(|e| panic!("cannot read {ASMDEF}: {e}"));
    for reference in [
        "\"Unity.RenderPipelines.Universal.Runtime\"",
        "\"Unity.RenderPipelines.Core.Runtime\"",
    ] {
        assert!(
            asmdef.contains(reference),
            "{ASMDEF} no longer references {reference}, which `{PASS}` needs."
        );
    }
}
