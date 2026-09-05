// The engine painter: the committed tables drawn through BatchRendererGroup.
//
// **Everything under `Runtime/Engine/` references UnityEngine**, which is what
// separates it from the rest of `Runtime/`. R-E10's netstandard2.1 check cannot
// compile a Unity-referencing type, so this directory is checked by a Unity
// editor instead — `docs/decisions/r-e10-is-checked-in-two-halves.md`, and
// `just unity-editor` is the recipe. Nothing that can be written without the
// engine belongs here: what the picture IS lives in `Runtime/FramePacker.cs`,
// where a check with no editor can execute it.
//
// **The lease does not cross into the culling callback.** Issue #1267 measured
// `OnPerformCulling` on Unity's main thread, so acquiring inside it is allowed
// — and this painter does not, because it does not need to. `Draw` packs the
// borrowed tables into arrays this object owns and the caller's lease can end
// the moment `Draw` returns. Whoever later moves that read into the callback
// inherits the rule the lease's own remarks state: release after Unity
// completes the `JobHandle`, not on return from the callback, because the
// workers are still reading the borrowed rows when the callback returns.

using System;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;
using UnityEngine;
using UnityEngine.Rendering;

namespace Driftsys.Dashscene
{
    /// Which rung of `docs/decisions/unity-painter-uses-brg.md` D3's ladder the
    /// painter is on.
    ///
    /// **Read from Unity rather than inferred** — D4. The value comes from
    /// `BatchRendererGroup.BufferTarget`, taken in a process that has a
    /// graphics device, which is R-E14.
    public enum BrgRung
    {
        /// Rung 1, instance data in a storage buffer. `RawBuffer`.
        RawBuffer,

        /// Rung 1, under the window and alignment the device reports.
        /// `ConstantBuffer`.
        ConstantBuffer,

        /// Rung 3 — instanced draws without BatchRendererGroup.
        /// `UnsupportedByUnderlyingGraphicsApi`.
        ///
        /// **Nothing is built for it.** D3 records rung 3 so the answer is not
        /// improvised under pressure, and says so; the painter reports this
        /// rung and draws nothing rather than pretending to be on rung 1.
        /// Descending below rung 1 is D3's trigger to raise the R-T4 conflict.
        InstancedWithoutBrg,
    }

    /// The painter could not be constructed, or could not draw.
    ///
    /// Separate from `DashsceneException`, which carries a `DsStatus` from the
    /// C ABI: nothing here comes from the library.
    public class DashscenePainterException : Exception
    {
        internal DashscenePainterException(string message)
            : base(message)
        {
        }
    }

    /// Draws committed dashscene frames through `BatchRendererGroup`.
    ///
    /// One painter draws one document with one material class. Create it after
    /// the runtime and before the first `Draw`; dispose it before the runtime.
    public sealed class BrgPainter : IDisposable
    {
        /// Bytes one instance occupies across the five per-instance properties.
        ///
        /// Five `float4`-sized properties. The same eighty bytes the lean
        /// painter's `Instance` occupies, which is not a coincidence: the
        /// layouts are the same because the pictures are meant to be.
        private const int BytesPerInstance = 5 * 16;

        /// The property ids the per-frame binding uses, resolved once.
        ///
        /// **`Material.SetBuffer(string, …)` hashes the name on every call**,
        /// and [`BindHeap`] makes four of those calls per material per frame
        /// plus one more for each glyph atlas — so a cascade of four faces
        /// paid twenty-four name lookups a frame where the process-wide
        /// binding paid five. R-T4 bounds what a frame may spend, and this is
        /// the same `Shader.PropertyToID` the metadata builders below already
        /// use.
        ///
        /// **Static, and what makes that safe is the order rather than the
        /// storage.** This class declares no static constructor, so it is
        /// `beforefieldinit` and the runtime may run the initializer at any
        /// point up to the first static access — which is the constructor's,
        /// on the host's thread. `OnPerformCulling` touches a static of this
        /// class too — the generic `Malloc` — and could therefore trigger it
        /// from a job thread; it cannot run first, because a `BatchRendererGroup`
        /// exists only once a painter has been constructed.
        /// `Shader.PropertyToID` needs no graphics device, which is why a
        /// rung-3 painter can be constructed with these already resolved.
        private static readonly int PaintsId =
            Shader.PropertyToID(PaintMaterialProperties.Paints);
        private static readonly int ClipBoxesId =
            Shader.PropertyToID(PaintMaterialProperties.ClipBoxes);
        private static readonly int StrokesId =
            Shader.PropertyToID(PaintMaterialProperties.Strokes);
        private static readonly int ScalarsId =
            Shader.PropertyToID(PaintMaterialProperties.Scalars);
        private static readonly int GlyphsId =
            Shader.PropertyToID(PaintMaterialProperties.Glyphs);
        private static readonly int CutoffId =
            Shader.PropertyToID(PaintMaterialProperties.Cutoff);
        private static readonly int AtlasId =
            Shader.PropertyToID(PaintMaterialProperties.Atlas);

        /// Bytes of shared, non-per-instance data at the head of every window:
        /// a zero `float4` and the two transforms.
        ///
        /// **The first sixteen bytes must be zero.** A metadata value of 0
        /// addresses byte 0, and that is what a property Unity asks for and
        /// this painter does not supply resolves to.
        private const int HeadBytes = 16 + 48 + 48;

        /// R-E20's bound, and a literal because it is one: the SRP core shader
        /// library declares `DOTSVisibleData unity_DOTSVisibleInstances[256]`
        /// against `kBRGVisibilityUBOShaderArraySize`, so it is a property of
        /// the shader rather than of the adapter — unlike R-E15's two, which
        /// are read from the running device below.
        private const int MaxInstancesPerDrawCommand = 256;

        private readonly FramePacker _packer = new FramePacker();
        private readonly MaterialClass _materialClass;

        private BatchRendererGroup _brg;
        private Material _material;
        private Mesh _mesh;
        private BatchMeshID _meshId;
        private BatchMaterialID _materialId;

        private GraphicsBuffer _instanceBuffer;
        private GraphicsBuffer _paintBuffer;
        private GraphicsBuffer _clipBuffer;
        private GraphicsBuffer _strokeBuffer;
        private GraphicsBuffer _glyphBuffer;

        /// The document's MSDF sheets, or [`TextAtlasSet.Empty`] until a host
        /// installs them.
        ///
        /// **`Empty` and not `null`**, so `Draw` has one shape to hand the
        /// packer: a document with no text and a host that has not called
        /// [`SetAtlases`] produce the same picture, and only the second is a
        /// mistake worth reporting — which `PackDiagnostic.GlyphRun` does, and
        /// only when the frame actually carries runs.
        private TextAtlasSet _atlases = TextAtlasSet.Empty;

        /// One texture, one material and one registration per atlas, in atlas
        /// order.
        ///
        /// **A material per sheet rather than per class.** A texture is a
        /// per-material binding, so a document naming two faces needs two
        /// materials over one shader, and the culling callback emits a draw
        /// command per instance, each one's atlas picking the material.
        private Texture2D[] _atlasTextures = Array.Empty<Texture2D>();
        private Material[] _textMaterials = Array.Empty<Material>();
        private BatchMaterialID[] _textMaterialIds = Array.Empty<BatchMaterialID>();

        /// Whether [`SetAtlases`] has been called since the last [`Draw`].
        ///
        /// **What tells a set installed FOR this document from one left over
        /// from the last.** `DsFrame.DocumentReplaced` says a load has happened
        /// and is cleared by the acquire that reports it, so it cannot say
        /// which side of that load a set came from — and the order the package
        /// documents puts the install on the reporting frame itself. This bit
        /// is what makes the drop in [`Draw`] fire on a stale set and not on a
        /// fresh one.
        private bool _atlasesInstalledSinceDraw;

        private BatchID[] _batches = Array.Empty<BatchID>();
        private int _batchCount;

        /// How many draw commands the last [`Draw`] laid out.
        ///
        /// Computed once per frame, equal to the instance count, and read by
        /// every camera's culling callback. It has to be exactly what the
        /// emission loop produces: Unity allocates the command array from it
        /// and that loop writes into it.
        private int _commandCount;

        /// Whether the short-frame warning has been reported.
        ///
        /// Latched: `OnPerformCulling` runs per camera per frame, and the
        /// condition it reports persists until a `Draw` completes.
        private bool _reportedShortFrame;

        private int _instancesPerBatch;
        private int _batchStrideBytes;

        private uint[] _staging = Array.Empty<uint>();
        private float[] _paintStaging = Array.Empty<float>();
        private float[] _clipStaging = Array.Empty<float>();
        private float[] _strokeStaging = Array.Empty<float>();
        private float[] _glyphStaging = Array.Empty<float>();
        private float _cutoff = 0.5f;
        private Bounds _globalBounds = new Bounds(Vector3.zero, Vector3.one * 10000.0f);
        private PackDiagnostics _lastDiagnostics;
        private bool _disposed;

        /// The pipeline instance R-E5 was last decided against, so the warning
        /// is reported once per instance rather than once per frame.
        ///
        /// **An instance rather than a `bool`, because the global is per
        /// instance.** URP assigns it in each pipeline instance's constructor,
        /// so a host that assigns a different render pipeline asset gets a
        /// fresh instance and a freshly assigned global. A quality-level switch
        /// does so only when the level names a different asset; naming the same
        /// one reconstructs nothing, and this then correctly says nothing. A
        /// latched `bool` would report the old, correct verdict once and stay
        /// silent when a new asset turned the batcher off — the painter would
        /// then draw nothing, for a reason it had already decided not to
        /// mention.
        ///
        /// Only identity is ever read from this, never a member, so a disposed
        /// pipeline here is harmless to correctness. It is still a strong
        /// reference to an instance this painter does not own, so `Dispose`
        /// clears it: a painter kept alive after its last `Draw` would
        /// otherwise be the last thing rooting a pipeline the host has
        /// replaced.
        private RenderPipeline _batcherReportedFor;

        /// The rung this painter is on, read from Unity at construction.
        public BrgRung Rung { get; }

        /// The window size the device reports, in bytes, or 0 under
        /// `RawBuffer`, which has no window.
        ///
        /// R-E15 requires this to be read from the running device and never
        /// compared against a literal. On the one adapter measured — Apple M3,
        /// Metal — the target was `RawBuffer`, so that adapter does not
        /// exercise this at all.
        public int ConstantBufferWindowBytes { get; }

        /// The window alignment the device reports, in bytes, or 0 under
        /// `RawBuffer`.
        public int ConstantBufferAlignmentBytes { get; }

        /// How many instances the last [`Draw`] emitted.
        public int InstanceCount { get; private set; }

        /// What the last [`Draw`] was handed and did not draw.
        public PackDiagnostics Diagnostics => _lastDiagnostics;

        /// Document space to world space.
        ///
        /// The document's own units, with y increasing downwards. The identity
        /// puts one document unit on one world unit with the document's origin
        /// at the world origin and its y axis pointing down, which is what a
        /// camera looking along +z sees upright.
        public Matrix4x4 DocumentToWorld { get; set; } = Matrix4x4.identity;

        /// The width, in document units, over which an anti-aliased edge ramps.
        ///
        /// One device pixel, expressed in the document's own units — so a
        /// document drawn at twice the scale halves this. Passed to the shader
        /// rather than taken from `fwidth`, because the layer-2 conformance
        /// harness that checks the same arithmetic has no derivatives at all.
        public float EdgeWidth { get; set; } = 1.0f;

        /// The coverage below which [`MaterialClass.LitCutout`] discards a
        /// fragment.
        ///
        /// Ignored by the other two classes, and set on the material rather
        /// than per instance: nothing on boundary B varies it per node, and a
        /// per-instance property costs sixteen bytes on every instance whether
        /// or not it differs between them.
        public float Cutoff
        {
            get => _cutoff;
            set
            {
                _cutoff = value;
                // Applied here as well as at construction, so a host that sets
                // this after the painter exists does not have to know that the
                // material was already made.
                _material?.SetFloat(CutoffId, value);
            }
        }

        /// The world-space bounds the group is culled against.
        ///
        /// Generous on purpose: a document is a flat sheet and this painter
        /// does not compute its extent per commit. A host that knows better
        /// sets it — and the setter applies it, which a plain auto-property
        /// would not: `SetGlobalBounds` is a call, so a value assigned after
        /// construction would otherwise be recorded and never reach Unity.
        public Bounds GlobalBounds
        {
            get => _globalBounds;
            set
            {
                _globalBounds = value;
                _brg?.SetGlobalBounds(value);
            }
        }

        /// Create a painter and take its rung.
        ///
        /// # Exceptions
        ///
        /// [`DashscenePainterException`] when the process holds no graphics
        /// device, when no render pipeline is active, when a shader is missing,
        /// or when `BufferTarget` returns a value Unity documents as one it
        /// never returns.
        public BrgPainter(MaterialClass materialClass = MaterialClass.UnlitOverlay)
        {
            _materialClass = materialClass;

            // R-E14: the read below is only a verdict in a process that has
            // obtained a graphics device. `unity-painter-uses-brg.md` D4 rules
            // that a read taken without one is not a verdict, and story #1125
            // measured that hazard producing a plausible answer rather than an
            // obvious absence — a `-nographics` run would report
            // `UnsupportedByUnderlyingGraphicsApi` and abandon BRG on a read
            // taken with no device.
            if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
            {
                throw new DashscenePainterException(
                    "this process has no graphics device (SystemInfo.graphicsDeviceType is "
                    + "Null), so BatchRendererGroup.BufferTarget cannot be read as a verdict. "
                    + "Run with a device — a windowed player or an editor with a graphics "
                    + "API — rather than under -nographics.");
            }

            // R-E4: a BatchRendererGroup requires a ScriptableRenderPipeline,
            // and Unity's own refusal is a log line rather than an exception —
            // so a painter that did not check would construct a group that
            // draws nothing and report success.
            if (GraphicsSettings.currentRenderPipeline == null)
            {
                throw new DashscenePainterException(
                    "no ScriptableRenderPipeline is active "
                    + "(GraphicsSettings.currentRenderPipeline is null), and BatchRendererGroup "
                    + "requires one. Assign a render pipeline asset in Graphics settings.");
            }

            var target = BatchRendererGroup.BufferTarget;
            switch (target)
            {
                case BatchBufferTarget.RawBuffer:
                    Rung = BrgRung.RawBuffer;
                    break;
                case BatchBufferTarget.ConstantBuffer:
                    Rung = BrgRung.ConstantBuffer;
                    ConstantBufferWindowBytes = BatchRendererGroup.GetConstantBufferMaxWindowSize();
                    ConstantBufferAlignmentBytes =
                        BatchRendererGroup.GetConstantBufferOffsetAlignment();
                    break;
                case BatchBufferTarget.UnsupportedByUnderlyingGraphicsApi:
                    // R-E19: this selects rung 3, not "draw nothing quietly".
                    // Nothing is built for rung 3, so the painter stops here
                    // and says which rung it is on — the `Debug.LogWarning`
                    // below is what says it, and `Draw`'s own remarks rely on
                    // it having been said. Descending below rung 1 is D3's
                    // trigger to raise the R-T4 conflict.
                    Rung = BrgRung.InstancedWithoutBrg;
                    // **The log is what makes this arm a report rather than a
                    // silent selection.** `Rung` is a public property, which is
                    // availability rather than a report: a host that never
                    // reads it sees a blank screen and a clean console.
                    // **R-E6's default produces a blank frame too and is NOT
                    // silent** — Unity itself logs `Trying to render a
                    // BatchRendererGroup batch with wrong cbuffer setup.
                    // Missing DOTS_INSTANCING_ON variant?` on every frame,
                    // measured 2026-08-23 on macOS/Metal, Unity 6000.3.22f1.
                    // So the two blank frames look identical on screen and
                    // differ entirely in the console, and without this line
                    // rung 3 is the one a host cannot tell apart from a bug in
                    // its own document (issue #1326).
                    Debug.LogWarning(
                        $"[dashscene] BatchRendererGroup.BufferTarget reports {target} on this "
                        + $"graphics API, so this painter is on rung {Rung} and draws nothing "
                        + "(docs/decisions/unity-painter-uses-brg.md D3, R-E19). Nothing is "
                        + "built for that rung: Draw returns without drawing and without "
                        + "throwing, so every frame is blank. Read BrgPainter.Rung to branch "
                        + "on this.");
                    // **It binds nothing either.** A rung-3 painter builds no
                    // group and no material, and `Draw` returns above
                    // `BindHeap`, so no buffer this object holds reaches a
                    // shader.
                    return;
                default:
                    // `Unknown` and anything else. Unity documents `Unknown` as
                    // "the default uninitialized value for this enum … Unity
                    // will never return this", so D4's table assigns it no
                    // rung. An undocumented value is not a rung selection, and
                    // no BatchRendererGroup is constructed.
                    throw new DashscenePainterException(
                        $"BatchRendererGroup.BufferTarget returned {target}, which "
                        + "docs/decisions/unity-painter-uses-brg.md D4 assigns no rung — "
                        + "Unity documents Unknown as a value it never returns. No "
                        + "BatchRendererGroup was constructed.");
            }

            // **R-E5 is NOT read here**, and that is issue #1317.
            // `GraphicsSettings.useScriptableRenderPipelineBatching` is a
            // verdict only once a pipeline INSTANCE exists, and this
            // constructor runs before one does in a host that builds a painter
            // in `Awake` of the FIRST frame — which the package's own
            // `Samples~/FrameLoop` does. A painter built later, once rendering
            // has begun, would read a decided global here, and a constructor
            // CAN tell the two apart with the same
            // `RenderPipelineManager.currentPipeline` test the read now uses —
            // issue #1317 names that shape as one of the two acceptable ones.
            // The read is moved rather than guarded here because a constructor
            // reads once and the global is reassigned by every later pipeline
            // instance, which is what `ReportBatcherOnce` re-decides against;
            // see its remarks.

            // **`Resources.Load`, not `Shader.Find`, and that is issue
            // #1313.** Unity strips a shader that no scene and no material
            // references out of a PLAYER build, and strips nothing in an
            // editor — so `Shader.Find` resolved in every gate this repository
            // had and returned null in the one configuration a customer ships.
            // Measured: a windowed macOS player, 6000.3.22f1, 2026-08-23,
            // threw the diagnostic below on a package that passed every other
            // check.
            //
            // A `Resources` folder is included in a build whether or not
            // anything references it, which is what makes the shader reachable
            // without asking each host to add it to Always Included Shaders.
            // The name doubles as the path: `PaintShaders.For` returns
            // `Dashscene/UnlitOverlay`, the shader's own declared name, and the
            // file sits at `Runtime/Resources/Dashscene/UnlitOverlay.shader`.
            // `unity/package-gate` holds the two together in both directions,
            // so a shader renamed and not moved fails a test rather than a
            // player.
            var shaderName = PaintShaders.For(materialClass);
            var shader = Resources.Load<Shader>(shaderName);
            if (shader == null)
            {
                throw new DashscenePainterException(
                    $"the shader '{shaderName}' was not found. It ships in this package at "
                    + $"Runtime/Resources/{shaderName}.shader and is loaded with "
                    + "Resources.Load, so it is included in a player build without the host "
                    + "configuring anything. Two things make it absent: a Git-URL install "
                    + "that lost the shader's .meta file, or one that lost the .meta of a "
                    + "folder on that path — both are R-E2, and Unity ignores an asset with "
                    + "no .meta inside an immutable package rather than generating one.");
            }

            // **Everything from here is disposable, and a throw would strand
            // it.** `_material` and `_mesh` carry `HideAndDontSave`, so they
            // survive scene loads and domain reloads for the life of the
            // process, and a constructed `BatchRendererGroup` stays registered
            // with a callback into an object no caller ever received. Nothing
            // can call `Dispose` on an object whose constructor threw.
            try
            {
                _material = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
                if (materialClass == MaterialClass.LitCutout)
                {
                    _material.SetFloat(CutoffId, _cutoff);
                }
                _mesh = UnitQuad();
                _brg = new BatchRendererGroup(OnPerformCulling, IntPtr.Zero);
                _brg.SetGlobalBounds(_globalBounds);
                _meshId = _brg.RegisterMesh(_mesh);
                _materialId = _brg.RegisterMaterial(_material);
            }
            catch
            {
                ReleaseUnityObjects();
                throw;
            }
        }

        /// Install the document's MSDF sheets, so its glyph runs can be drawn.
        ///
        /// **Once per load, not once per frame.** The set is installed by a
        /// load and replaced only by another, so a host reads it with
        /// `DashsceneRuntime.ReadAtlases` when a frame reports
        /// `DocumentReplaced` and calls this. Nothing here is per commit, which
        /// is why the sheets cross the C ABI on their own call rather than in
        /// the frame.
        ///
        /// Each sheet becomes one linear, unmipped texture and one material
        /// over `Dashscene/Text`, registered with the group. Passing `null` or
        /// an empty set releases what was installed and returns the painter to
        /// drawing no text.
        ///
        /// **A painter with no atlas set still draws every other node**, and
        /// reports `PackDiagnostic.GlyphRun` for a frame that carries runs. It
        /// does not throw: a document whose text cannot be shaded is a picture
        /// missing its text, which P4 asks be named rather than raised.
        ///
        /// # Exceptions
        ///
        /// [`DashscenePainterException`] when the text shader is missing, or
        /// for any reason `AtlasTexture.Decode` refuses a sheet — it does not
        /// decode, its decoded extent disagrees with the one its metrics
        /// declared, it carries a mip chain, or it came back in an sRGB
        /// format. That list lives on `Decode` and is not restated here, so a
        /// reason added there does not have to be added twice. **The painter is left with no atlas set in every one of
        /// those cases** rather than with a half-built one, so a host that
        /// catches and carries on draws its document without text rather than
        /// with some of it.
        ///
        /// [`ObjectDisposedException`] once the painter is disposed.
        public void SetAtlases(TextAtlasSet atlases)
        {
            ThrowIfDisposed();

            // **Nothing this painter packed earlier is drawn again**, and that
            // closes a real window rather than tidying: `OnPerformCulling` runs
            // when Unity renders, not when `Draw` returns, so a set installed
            // between the two would meet the PREVIOUS document's instances —
            // whose atlas indices are meaningful only against the set they were
            // packed with. The next `Draw` refills both.
            InstanceCount = 0;
            _commandCount = 0;

            // Released first, and unconditionally: a second call replaces the
            // set, and the textures the previous one minted are reachable from
            // nothing afterwards.
            ReleaseAtlases();

            if (atlases == null || atlases.Count == 0)
            {
                _atlases = TextAtlasSet.Empty;
                return;
            }

            if (_brg == null)
            {
                // A rung-3 painter constructs no group, so there is nothing to
                // register a material with. The rung was reported at
                // construction and `Draw` returns before packing anything, so
                // this is not a second failure to raise.
                _atlases = TextAtlasSet.Empty;
                return;
            }

            // `Resources.Load`, not `Shader.Find`, for the reason the
            // constructor gives at length: issue #1313 measured a player build
            // stripping a shader nothing references, where an editor strips
            // nothing. The name doubles as the path.
            var shaderName = PaintShaders.Text;
            var shader = Resources.Load<Shader>(shaderName);
            if (shader == null)
            {
                throw new DashscenePainterException(
                    $"the shader '{shaderName}' was not found. It ships in this package at "
                    + $"Runtime/Resources/{shaderName}.shader and is loaded with "
                    + "Resources.Load, so it is included in a player build without the host "
                    + "configuring anything. Two things make it absent: a Git-URL install "
                    + "that lost the shader's .meta file, or one that lost the .meta of a "
                    + "folder on that path — both are R-E2.");
            }

            // **Built into locals and assigned at the end.** A throw partway
            // through would otherwise leave the painter holding a set whose
            // later atlases have no material — and the culling callback indexes
            // that array by an instance's atlas, so it would read past its end
            // on the first frame carrying such a run.
            var textures = new Texture2D[atlases.Count];
            var materials = new Material[atlases.Count];
            var ids = new BatchMaterialID[atlases.Count];
            try
            {
                for (var i = 0; i < atlases.Count; i++)
                {
                    textures[i] = AtlasTexture.Decode(atlases[i], i);
                    materials[i] = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
                    materials[i].SetTexture(AtlasId, textures[i]);
                    ids[i] = _brg.RegisterMaterial(materials[i]);
                }
            }
            catch
            {
                for (var i = 0; i < atlases.Count; i++)
                {
                    if (ids[i] != default)
                    {
                        _brg.UnregisterMaterial(ids[i]);
                    }
                    if (materials[i] != null)
                    {
                        UnityEngine.Object.DestroyImmediate(materials[i]);
                    }
                    if (textures[i] != null)
                    {
                        UnityEngine.Object.DestroyImmediate(textures[i]);
                    }
                }
                throw;
            }

            _atlasTextures = textures;
            _textMaterials = materials;
            _textMaterialIds = ids;
            _atlases = atlases;
            // Set AFTER the assignments, so a throw above leaves the painter
            // with no set and no claim that one was installed for this frame.
            _atlasesInstalledSinceDraw = true;
        }

        /// Unregister, destroy and forget every text material and sheet.
        ///
        /// Called by [`SetAtlases`], by [`Dispose`], and by [`Draw`] when a
        /// frame reports a document this set was not installed for. Three
        /// callers freeing the same objects is the reason it is a method: one
        /// that freed a subset would leak exactly what nobody can reach.
        private void ReleaseAtlases()
        {
            for (var i = 0; i < _textMaterialIds.Length; i++)
            {
                // **Unregistered before the group is disposed, not after.** A
                // `BatchRendererGroup` disposed with a material registered
                // leaves Unity holding a handle to an object this painter is
                // about to destroy.
                if (_brg != null && _textMaterialIds[i] != default)
                {
                    _brg.UnregisterMaterial(_textMaterialIds[i]);
                }
            }
            foreach (var material in _textMaterials)
            {
                if (material != null)
                {
                    UnityEngine.Object.DestroyImmediate(material);
                }
            }
            foreach (var texture in _atlasTextures)
            {
                if (texture != null)
                {
                    UnityEngine.Object.DestroyImmediate(texture);
                }
            }
            _textMaterialIds = Array.Empty<BatchMaterialID>();
            _textMaterials = Array.Empty<Material>();
            _atlasTextures = Array.Empty<Texture2D>();
            _atlases = TextAtlasSet.Empty;
        }

        /// Report R-E5 once per pipeline instance, and only from a read that
        /// can decide it.
        ///
        /// **The read is a verdict only once a pipeline INSTANCE exists**,
        /// which is issue #1317. URP assigns
        /// `GraphicsSettings.useScriptableRenderPipelineBatching` from the
        /// asset's `useSRPBatcher` inside `UniversalRenderPipeline`'s own
        /// constructor — one line of `UniversalRenderPipeline.cs`, verified
        /// against URP 17.3.0, the version this package depends on — and Unity
        /// runs that constructor when it first creates a pipeline instance, at
        /// the first render. So the global is `false` in `Awake` of the first
        /// frame however the project is configured. Measured on `6000.3.22f1`,
        /// macOS/Metal, 2026-08-23, in a player whose asset had
        /// `useSRPBatcher` true: `False` at `Awake`, `True` four frames later.
        /// Reading it in this painter's constructor therefore warned on every
        /// correctly configured host, which is a diagnostic a developer learns
        /// to ignore.
        ///
        /// **`RenderPipelineManager.currentPipeline` is the guard, not a frame
        /// counter.** It is non-null exactly once Unity has constructed a
        /// pipeline instance, so the assignment above has already happened.
        /// The same test is used for "a pipeline is live" in
        /// `com.unity.render-pipelines.core` — in editor code, `DebugWindow`
        /// and `DisplayWindow` — and at run time in HDRP's
        /// `HDRenderPipeline`; SRP core's runtime half does not use it, so
        /// this is a precedent rather than a rule the packages enforce. While
        /// it is null the read cannot decide anything, so this
        /// says nothing and tries again next frame rather than saying the
        /// wrong thing. That is also why the instance is recorded inside the
        /// guard and not above it: a painter whose first `Draw` runs before the
        /// first render must still get its verdict on a later frame.
        ///
        /// **Two hosts this does not decide correctly, and neither is a host
        /// that merely never draws.** They fail in opposite directions, which
        /// is why they are not one bullet: the first is told nothing, the
        /// second is told something false.
        ///
        /// - A process that draws every frame and renders through no pipeline
        ///   gets none, because no instance is ever constructed and the guard
        ///   is closed forever. That is not hypothetical: `unity/render-gate`
        ///   records a batch-mode player doing exactly this, which is why it
        ///   drives rendering with `SubmitRenderRequest` rather than leaving it
        ///   to Unity. The old constructor read reported such a host, but
        ///   reported every host the same way whatever it was configured to, so
        ///   what is lost is a warning that was never evidence. R-E5 is
        ///   undecidable there, and saying nothing is the honest answer — but
        ///   nothing says "undecided" either, and that is the gap. Issue #1340
        ///   carries it.
        /// - A host on a pipeline that does not assign the global at all gets
        ///   the wrong verdict, not none: `currentPipeline` says an instance
        ///   exists, not that URP's constructor ran. The global would stay
        ///   `false` and this would warn about a project that meets R-E5,
        ///   which is issue #1317 on a different pipeline. This package
        ///   declares a dependency on
        ///   `com.unity.render-pipelines.universal` and R-E5 names URP's own
        ///   `m_UseSRPBatcher`, so that host is out of scope by construction;
        ///   the guard is deliberately not narrowed to a URP type, which would
        ///   make this file reference URP for a diagnostic.
        ///
        /// R-E5 is the host project's requirement rather than this painter's,
        /// so this reports instead of throwing: Unity's own refusal — "Please
        /// turn SRP Batcher ON to use the BatchRendererGroup API" — has never
        /// been observed in this repository, and a host meeting it late should
        /// see why.
        private void ReportBatcherOnce()
        {
            var pipeline = RenderPipelineManager.currentPipeline;
            if (pipeline == null || ReferenceEquals(pipeline, _batcherReportedFor))
            {
                return;
            }
            _batcherReportedFor = pipeline;

            if (!GraphicsSettings.useScriptableRenderPipelineBatching)
            {
                Debug.LogWarning(
                    "[dashscene] the SRP Batcher is off. BatchRendererGroup needs it "
                    + "(docs/specification/07-embedding-and-distribution.md R-E5): set "
                    + "m_UseSRPBatcher on the active render pipeline asset.");
            }
        }

        /// Pack and upload one committed frame.
        ///
        /// The lease's arrays are read here and nowhere else, so the caller may
        /// dispose it as soon as this returns.
        ///
        /// **It does not call `MarkDrawn`, and the caller must.** This painter
        /// packs and uploads; whether the frame reached a screen is decided by
        /// the culling callback and by Unity, later. A host that disposes the
        /// lease without marking it leaves every commit unshown, so a settled
        /// document reports `advanced` forever and the host re-acquires and
        /// re-packs frames that will never change.
        ///
        /// A painter on [`BrgRung.InstancedWithoutBrg`] returns without drawing
        /// and without throwing: the rung was already reported at construction
        /// — the `Debug.LogWarning` on that arm of the constructor's switch is
        /// the report — and throwing once per frame would bury it.
        public void Draw(FrameLease lease)
        {
            if (lease == null)
            {
                throw new ArgumentNullException(nameof(lease));
            }
            ThrowIfDisposed();

            if (_brg == null)
            {
                return;
            }

            // **After the rung-3 return, not before it.** A rung-3 painter
            // builds no group and draws nothing whatever the batcher is set
            // to, so warning about R-E5 there would add a second cause to a
            // blank frame that already has one.
            ReportBatcherOnce();

            // **A set installed for the PREVIOUS document is dropped.** An
            // atlas index is meaningful only against the set the load that
            // staged the runs installed; resolving a new document's runs
            // against the previous document's sheets would leave every index
            // that happens to be in range resolving, with `resolved = 1` and a
            // texture from the wrong document — the glyph ids still resolve,
            // the rectangles are still in range, and the text draws the wrong
            // letters rather than failing. That is the hazard
            // `the-glyph-atlas-crosses-the-c-abi-as-a-call.md` D2 closes for
            // the face order, reached here through a different door.
            //
            // **The test is `_atlasesInstalledSinceDraw`, not
            // `DocumentReplaced` alone**, and the difference is the whole of
            // this block. `DocumentReplaced` is raised by every load —
            // including the first — and is cleared by the acquire that reports
            // it, so it is true on exactly the frame a host following this
            // package's own instructions has just called `SetAtlases` for:
            // acquire, see the flag, read the atlases, install them, draw.
            // Dropping on the flag alone destroys the set two lines after it
            // was minted, no later frame raises the flag again, and the
            // document's text never draws — reported as "no atlas set was
            // installed", which is the opposite of what happened.
            //
            // So what is asked is "was this set installed for the document
            // this frame belongs to". A `SetAtlases` between the previous
            // `Draw` and this one answers yes, whichever side of the acquire
            // it happened on; no call at all, on a frame that reports a
            // replacement, answers no. A host that installs AFTER drawing
            // loses one frame of text and is told why.
            if (lease.DocumentReplaced && !_atlasesInstalledSinceDraw && _atlases.Count > 0)
            {
                ReleaseAtlases();
            }
            _atlasesInstalledSinceDraw = false;

            _packer.Pack(lease.Frame, _materialClass, _atlases);
            InstanceCount = _packer.InstanceCount;

            if (_packer.Diagnostics != _lastDiagnostics)
            {
                _lastDiagnostics = _packer.Diagnostics;
                if (!_lastDiagnostics.IsClean)
                {
                    // P4: every out-of-profile construct is a named diagnostic,
                    // never a silent drop. Logged on change rather than per
                    // frame — a document that carries a shadow carries it on
                    // every commit.
                    foreach (var line in _lastDiagnostics.Describe())
                    {
                        Debug.LogWarning($"[dashscene] {line}");
                    }
                }
            }

            UploadHeap();
            UploadInstances();
            BindHeap();

            // After `UploadInstances`, which is what settles `_batchCount` and
            // `_instancesPerBatch` — the two `InstancesInBatch` is counted
            // over. One draw command per visible instance (issue #1401), so
            // the command count is the instance count.
            _commandCount = 0;
            for (var b = 0; b < _batchCount; b++)
            {
                _commandCount += InstancesInBatch(b);
            }

            // The two counts agree again, so a later short frame is a new
            // event rather than the same one.
            _reportedShortFrame = false;
        }

        private unsafe JobHandle OnPerformCulling(
            BatchRendererGroup rendererGroup,
            BatchCullingContext cullingContext,
            BatchCullingOutput cullingOutput,
            IntPtr userContext)
        {
            var drawCommands = (BatchCullingOutputDrawCommands*)cullingOutput.drawCommands.GetUnsafePtr();

            // **Zeroed before the guard, not after it.** Unity hands this struct
            // over uninitialised, and this callback fires on every camera of
            // every frame — including the ones before the first `Draw` and any
            // frame whose document packs no instances. A first version returned
            // from the guard below without writing anything, leaving Unity to
            // read whatever was in that memory as a draw-command count and
            // pointer, while the comment two lines down asserted that could not
            // happen.
            *drawCommands = default;

            if (InstanceCount == 0 || _batchCount == 0)
            {
                return default;
            }

            // How many draw commands: one per visible instance
            // (issue #1401, D5), counted in `Draw`.
            //
            // **Counted in `Draw` rather than here.** One command per
            // instance (issue #1401, D5) makes this count the instance
            // count, which `Draw` already settles across its batches; this
            // callback runs once per camera per frame, so recomputing it
            // here would repeat that same walk for a value already known.
            // The emission loop below is the walk that has to happen.
            var commandCount = _commandCount;

            // Unity frees all of these. `Allocator.TempJob` is what the API
            // documents for a callback that returns a handle, and this one
            // returns `default` — the work is done here, on the thread issue
            // #1267 measured as Unity's main one.
            // **Allocated here, and every LENGTH stated after the emission
            // loop.** The four counts Unity reads describe what was written
            // rather than what was reserved, so a frame that stops early cannot
            // describe commands or floats it never produced. Setting them here
            // as well would be a dead store, and a reader would have to work
            // out which of the two assignments reaches Unity.
            drawCommands->drawCommands = Malloc<BatchDrawCommand>(commandCount);
            drawCommands->drawRanges = Malloc<BatchDrawRange>(1);
            drawCommands->visibleInstances = Malloc<int>(InstanceCount);
            // **The paint order, stated rather than assumed — issue #1389.**
            // `BatchRendererGroup` groups draw commands by material before it
            // draws them, which is ordinary renderer behaviour and is not
            // logged. This painter emits its instances in painter's-algorithm
            // order, and the two SHADERS a text document draws through —
            // `UnlitOverlay`, the class material, and `Text`, one material per
            // glyph atlas — declare `ZWrite Off` and `ZTest Always`, so on that
            // path sequence is the only thing that decides what covers what.
            // (`Text` is not a `MaterialClass`; the enum's other two values,
            // `LitOpaque` and `LitCutout`, declare `ZWrite On` and
            // `ZTest LEqual`. The keys below are set whichever class the
            // painter was built with, and were measured on `UnlitOverlay`
            // alone.) Emission order
            // alone does not survive the grouping: the document's backdrop is
            // the first row the packer writes and sits on the class material,
            // so it joined that material's group and was drawn over the glyphs.
            //
            // The keys below are what Unity's own producer uses for the same
            // problem: SRP core 17.3.0 flags every transparent material's
            // command with `HasSortingPosition`
            // (`Runtime/GPUDriven/InstanceCullingBatcherBurst.cs`) and writes
            // one `float3` per flagged command at float offset
            // `3 * commandIndex` (`Runtime/GPUDriven/InstanceCuller.cs`). A
            // flagged command below carries exactly one visible instance,
            // per issue #1401, with the measurements in the technote.
            //
            // **Setting the flag is what made glyphs reach the screen on the
            // typography scene (issue #1389), and under one instance per
            // command the keys' RANK is the draw order**: farthest first, so
            // command 0 draws first and the emission order is the picture —
            // measured on 2026-09-05 (issue #1402), on one graphics API. `just
            // unity-render`'s order phase pins the composite on every run;
            // negating the step below drew the backdrop last in a hand-run
            // arm, which the gate itself does not run, and the flag-off arm
            // composites in order on that fixture, which the order record
            // lists as owed.
            // `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
            // holds the claim and its limits and
            // `docs/technotes/batch-renderer-group.md` §5b the four arms. The
            // evidence lives there rather than here, so that revising it edits
            // one place.
            drawCommands->instanceSortingPositions = Malloc<float>(3 * commandCount);

            // **One base point for every command, and only the index varies.**
            // Using each run's own anchor would turn this back into depth
            // sorting of coplanar geometry — camera-angle dependent, with
            // near-ties. These are an order encoding, not geometry.
            var sortBase = DocumentToWorld.MultiplyPoint3x4(Vector3.zero);
            // **The view's position from `localToWorldMatrix`, not from
            // `lodParameters`.** This callback runs for every culling view, and
            // `LODParameters` is meaningful only for a camera one — a light,
            // picking or selection-outline pass can hand over a default, whose
            // `cameraPosition` is the world origin. The view matrix's fourth
            // column carries the position on every view type, and for a camera
            // view it is the same point.
            var viewPosition = (Vector3)cullingContext.localToWorldMatrix.GetColumn(3);
            var toView = viewPosition - sortBase;
            var distance = toView.magnitude;

            // A viewer at the sheet leaves no direction to encode along. The
            // document's own -z is where the unit quad faces, so it is the axis
            // a viewer must be on for the sheet to be visible at all.
            //
            // **Neither branch uses `Vector3.normalized`, and that is the
            // point.** Unity's `Normalize` returns the ZERO vector rather than
            // throwing when the magnitude is at or below its own `kEpsilon` of
            // 1e-5 — so a guard admitting anything shorter than that does not
            // guard: a host flattening the sheet with a z-scale of 5e-6, which
            // is exactly the strictly-2D case this fallback was written for,
            // passes a `sqrMagnitude > 1e-12f` test and still normalizes to
            // zero. Every key then lands on one point and the material
            // grouping this exists to escape returns, with no diagnostic.
            // Dividing by a magnitude this code has already tested against a
            // threshold of its own choosing has no hidden epsilon in it.
            //
            // **`back` is a deterministic default, and the rank does not turn
            // on it.** The keys run backwards from the sheet, so a viewer at
            // the base point is `(commandCount - 1 - c) * step` from key `c` —
            // falling in `c`, command 0 farthest — whichever unit direction is
            // chosen, and `back` and `forward` give identical distances there.
            var sortDir = Vector3.back;
            if (distance > 1e-4f)
            {
                sortDir = toView / distance;
            }
            else
            {
                var facing = DocumentToWorld.MultiplyVector(new Vector3(0.0f, 0.0f, -1.0f));
                var facingLength = facing.magnitude;
                if (facingLength > 1e-4f)
                {
                    sortDir = facing / facingLength;
                }
            }

            // **The step is bounded below, and the fold that would have needed
            // an upper bound is ruled out by construction instead.**
            //
            // Float32 carries about 1.2e-7 of RELATIVE precision, and what is
            // stored is a world coordinate around `sortBase` — so a document
            // placed ten thousand units from the world origin has an ULP of
            // about 1.2e-3 there, and a step sized only from a one-unit viewing
            // distance would round every key onto the same float. The magnitude
            // term and the floor are what keep the keys distinct.
            //
            // **There is no upper bound, because the keys never approach the
            // camera.** They are laid out BEHIND the sheet, running back toward
            // it: command `c` sits at `(commandCount - 1 - c)` steps on the far
            // side, so its distance from the camera is
            // `distance + (commandCount - 1 - c) * step` — strictly falling in
            // `c`, at any span, with command 0 farthest. An earlier version
            // walked them toward the camera instead, where distance is
            // `|distance - c * step|` and the rank folds back once the span
            // passes the viewing distance. Capping the span to keep that from
            // happening put the cap in conflict with the floor above — the cap
            // is smaller exactly when a near camera looks at a document far
            // from the world origin — and taking the smaller of the two rounded
            // every key onto one float, which is issue #1389 returning with no
            // diagnostic. Laying the keys out behind the sheet removes the
            // conflict rather than choosing a side of it: the rank is the same
            // one, and no span can reach the camera.
            var sortStep = Math.Max(Math.Max(distance, sortBase.magnitude), 1.0f) * 1e-5f;

            drawCommands->drawRanges[0] = new BatchDrawRange
            {
                drawCommandsBegin = 0,
                // `drawCommandsCount` is stated after the emission loop, with
                // the other four lengths, from what was actually written. Set
                // here it would be a dead store the reconciliation always
                // overwrites — and the initialiser zeroes it in any case.
                filterSettings = new BatchFilterSettings
                {
                    renderingLayerMask = 0xffffffff,
                    layer = 0,
                    motionMode = MotionVectorGenerationMode.Camera,
                    shadowCastingMode = ShadowCastingMode.Off,
                    receiveShadows = false,
                    staticShadowCaster = false,
                    // **False, though every command in this range now carries
                    // `HasSortingPosition`** — which is the property Unity
                    // documents this field as asserting, so the range
                    // under-declares itself on purpose. Setting it true was
                    // measured and changes no pixel
                    // (`docs/technotes/batch-renderer-group.md` §5c), and false
                    // is what Unity's own producer passes. It is left false so
                    // that nothing here claims an ordering guarantee the
                    // measurements do not support.
                    allDepthSorted = false,
                },
            };

            var visible = 0;
            var command = 0;
            for (var b = 0; b < _batchCount; b++)
            {
                var first = b * _instancesPerBatch;
                var limit = first + InstancesInBatch(b);
                // **Bounded by the allocated command count as well as by the
                // batch.** `commandCount` is `Draw`'s cached answer and the
                // loop's own bound comes from `InstanceCount`; the two agree on
                // every frame that completes `Draw`, but a frame that throws
                // between the two — `UploadHeap`, `UploadInstances` or
                // `BindHeap` — leaves a NEW instance count beside the PREVIOUS
                // command count, and this callback still runs. Without this
                // bound that frame writes past both `drawCommands` and
                // `instanceSortingPositions`, which is heap corruption in
                // unsafe code rather than a wrong picture. What is emitted is
                // reconciled after the loop.
                for (var at = first; at < limit && command < commandCount;)
                {
                    // **One visible instance per command — issue #1401.**
                    // Unity's sorted-transparent path was measured dropping
                    // a contiguous subset of draw commands for single
                    // frames when a HasSortingPosition command carried more
                    // than one instance; one instance per command is the
                    // shape docs/technotes/batch-renderer-group.md §3
                    // attributes to Unity's own GPU Resident Drawer, and the
                    // only one measured safe. Tables: §5d.
                    var end = at + 1;
                    var run = end - at;

                    // Every instance's index is relative to its BATCH, not to
                    // the buffer, because instance 0 of every batch is the
                    // first row of that batch's own property arrays.
                    //
                    // **On the ConstantBuffer rung that is because the metadata
                    // offsets are window-relative; on RawBuffer it is because
                    // `AddBatches` folds the batch's byte offset INTO those
                    // offsets** (issue #1389), which is the same destination
                    // reached two ways. An earlier version of this comment gave
                    // only the first reason, which stopped being true on the
                    // rung every measured adapter selects.
                    for (var i = 0; i < run; i++)
                    {
                        drawCommands->visibleInstances[visible + i] = at - first + i;
                    }

                    drawCommands->drawCommands[command] = new BatchDrawCommand
                    {
                        visibleOffset = (uint)visible,
                        visibleCount = (uint)run,
                        batchID = _batches[b],
                        materialID = MaterialOf(at),
                        meshID = _meshId,
                        submeshIndex = 0,
                        splitVisibilityMask = 0xff,
                        // Command 0 is farthest and each later command a step
                        // nearer, all of them behind the sheet. The renderer
                        // draws the farthest first — measured, see above the
                        // allocation — so the sign of the step below is the
                        // order, and negating it drew the backdrop last, over
                        // everything.
                        flags = BatchDrawCommandFlags.HasSortingPosition,
                        sortingPosition = 3 * command,
                    };

                    var sortAt = sortBase - sortDir * ((commandCount - 1 - command) * sortStep);
                    drawCommands->instanceSortingPositions[3 * command + 0] = sortAt.x;
                    drawCommands->instanceSortingPositions[3 * command + 1] = sortAt.y;
                    drawCommands->instanceSortingPositions[3 * command + 2] = sortAt.z;

                    visible += run;
                    command++;
                    at = end;
                }
            }

            // **Reported as emitted, not as counted.** These are the lengths
            // Unity reads, and on a frame where the cached count and the
            // instance count disagree the loop above stops early — so the
            // arrays are described by what was written rather than by what was
            // allocated. On every ordinary frame the two are equal and this
            // changes nothing.
            //
            // `drawRangeCount` is among them: a range describing zero commands
            // is the shape the rest of this reconciliation exists to avoid
            // handing over.
            drawCommands->drawCommandCount = command;
            drawCommands->visibleInstanceCount = visible;
            drawCommands->instanceSortingPositionFloatCount = 3 * command;
            drawCommands->drawRanges[0].drawCommandsCount = (uint)command;
            drawCommands->drawRangeCount = command > 0 ? 1 : 0;

            // **A short frame is a named diagnostic, never a silent drop** —
            // P4's rule, and this is the one path that can produce one. It
            // means `Draw` threw between settling `InstanceCount` and settling
            // `_commandCount`, so the picture is this frame's instances cut to
            // the previous frame's command count. Latched, because this
            // callback runs per camera per frame and the condition persists
            // until a `Draw` completes.
            // **The test is the instances left behind, not the command
            // shortfall.** `command < commandCount` is wrong in both
            // directions: it is false when the cached count is ZERO and this
            // frame's instances are all dropped — the total loss, reported by
            // nothing — and true whenever a smaller document follows a larger
            // one, where the loop ran to its own end and cut nothing.
            if (visible < InstanceCount && !_reportedShortFrame)
            {
                _reportedShortFrame = true;
                Debug.LogWarning(
                    $"[dashscene] the painter emitted {visible} of {InstanceCount} instance(s) in "
                    + $"{command} draw command(s), against {commandCount} counted, so this frame "
                    + "is cut short. Draw did not complete between settling the instance count "
                    + "and settling the command count; the picture is incomplete until a Draw "
                    + "completes.");
            }

            return default;
        }

        /// The material instance `at` draws with.
        ///
        /// A glyph draws with its atlas's text material and everything else
        /// with the class material. **Bounded rather than trusted**: the packer
        /// only writes an atlas it resolved against the set this painter was
        /// given, so an index past the material array means the two went out of
        /// step — and following it would read a `BatchMaterialID` out of a
        /// managed array's end.
        ///
        /// The fallback is the class material, and what that draws is **not
        /// predictable**: a `DS_KIND_TEXT` instance shaded by a node shader
        /// takes the final `else` of `DsShade`, which reads `_DsCorners` as
        /// corner radii where a glyph instance holds atlas texels, and indexes
        /// the paint heap with a glyph-run row. The point of the bound is that
        /// it is a wrong picture rather than an out-of-range managed read; it
        /// is not that the wrong picture is a tidy one.
        private BatchMaterialID MaterialOf(int at)
        {
            var atlas = _packer.InstanceAtlas[at];
            return atlas >= 0 && atlas < _textMaterialIds.Length
                ? _textMaterialIds[atlas]
                : _materialId;
        }

        /// How many instances batch `b` holds, of the ones being drawn.
        ///
        /// **Clamped at zero.** Capacity doubles, so the last batches are
        /// routinely empty — a negative here would be read as a count by the
        /// loop that writes visible instances.
        private int InstancesInBatch(int b)
        {
            var start = b * _instancesPerBatch;
            return Math.Max(Math.Min(_instancesPerBatch, InstanceCount - start), 0);
        }

        private static unsafe T* Malloc<T>(int count) where T : unmanaged
        {
            return (T*)UnsafeUtility.Malloc(
                UnsafeUtility.SizeOf<T>() * (long)Math.Max(count, 1),
                UnsafeUtility.AlignOf<T>(),
                Allocator.TempJob);
        }

        /// Grow the buffer if this frame needs more room, then upload.
        ///
        /// **Nothing is allocated on a frame that fits, and that is the
        /// allocation half of R-T4 rather than the whole of it.** The rule asks
        /// for a CPU frame cost of "dirty-range instance-buffer upload from the
        /// rect table + submission. Nothing else"; what goes up below is the
        /// whole staging array — every batch, including the capacity past the
        /// live instances — whatever `DsFrame.Dirty` carries, and nothing in
        /// this package reads that set. Issue #1306 carries it, and
        /// `docs/design/unity-csharp-host.md`'s gaps list states what the full
        /// repack costs on the document `just unity-render` draws.
        ///
        /// Capacity grows by doubling and never shrinks, and the batches are
        /// added once per growth rather than once per frame — a first version
        /// sized the buffer to the exact instance count, which reallocated the
        /// `GraphicsBuffer` and re-added every batch on any frame where a
        /// single node appeared or left.
        private void UploadInstances()
        {
            if (InstanceCount == 0)
            {
                // The batches stay. A frame with nothing in it draws nothing
                // because `OnPerformCulling` emits no command, not because the
                // batches were torn down and will be rebuilt next frame.
                return;
            }

            EnsureCapacity(InstanceCount);
            FillStaging();
            _instanceBuffer.SetData(_staging, 0, 0, _staging.Length);
        }

        /// How many instances one batch window holds.
        ///
        /// Under `ConstantBuffer` this is what the device's own window size
        /// leaves after the shared head — R-E15, read from the running device
        /// rather than compared against a literal. Under `RawBuffer` there is
        /// no window, so it is a capacity this painter chooses and doubles.
        private int InstancesPerBatch(int wanted)
        {
            if (Rung != BrgRung.ConstantBuffer)
            {
                var size = Math.Max(_instancesPerBatch, 64);
                while (size < wanted)
                {
                    size *= 2;
                }
                return size;
            }

            // **Sized from the ALIGNED window, not the raw one.** The stride
            // handed to `AddBatch` must be a multiple of the device's
            // alignment AND no larger than its window, so the budget a batch
            // may spend is the largest aligned value inside the window. A
            // first version sized from the raw window and clamped the stride
            // afterwards, which threw on exactly the shape it was written for:
            // a 16000-byte window with a 256-byte alignment fits 198 instances
            // by the raw figure and only 195 by the aligned one.
            var alignment = Math.Max(ConstantBufferAlignmentBytes, 1);
            var budget = ConstantBufferWindowBytes / alignment * alignment;
            var usable = budget - HeadBytes;
            var fit = usable / BytesPerInstance;
            if (fit < 1)
            {
                throw new DashscenePainterException(
                    $"the device reports a {ConstantBufferWindowBytes}-byte constant-buffer "
                    + $"window and a {alignment}-byte alignment, leaving {budget} usable "
                    + $"bytes, which do not hold one {BytesPerInstance}-byte instance after "
                    + $"{HeadBytes} bytes of shared data.");
            }

            // Also bounded by `MaxInstancesPerDrawCommand`. Not required by
            // R-E20 — a batch now holds one draw command per instance
            // regardless — but it keeps a batch's capacity bounded.
            return Math.Min(fit, MaxInstancesPerDrawCommand);
        }

        /// Make room for `wanted` instances, reallocating only if it is short.
        private void EnsureCapacity(int wanted)
        {
            var perBatch = InstancesPerBatch(wanted);
            if (_instanceBuffer != null
                && _instancesPerBatch == perBatch
                && _batchCount * perBatch >= wanted)
            {
                return;
            }

            // **Captured before `RemoveBatches` zeroes it.** The doubling below
            // amortises growth by starting from the count already allocated; a
            // first version read `_batchCount` after the call that sets it to
            // zero, so it always started from one and the comment described an
            // amortisation the code could not perform.
            var had = _batchCount;
            RemoveBatches();
            _instancesPerBatch = perBatch;
            _batchStrideBytes = BatchStrideBytes(perBatch);

            var needed = (wanted + perBatch - 1) / perBatch;
            var batches = Math.Max(had, 1);
            while (batches < needed)
            {
                batches *= 2;
            }

            // `_batchCount` is assigned only once the batches exist. Setting it
            // first and then throwing from `new GraphicsBuffer` would leave the
            // culling callback indexing an empty `_batches` array.
            AllocateInstanceBuffer(batches);
            AddBatches(batches);
            _batchCount = batches;
        }

        private int BatchStrideBytes(int perBatch)
        {
            var bytes = HeadBytes + perBatch * BytesPerInstance;
            if (Rung != BrgRung.ConstantBuffer)
            {
                return bytes;
            }

            // R-E15's second half: every window offset is aligned to the byte
            // count the device reports.
            //
            // **Rounding up can pass the window Unity forbids exceeding.**
            // `UnityEngine.CoreModule.xml` says of `AddBatch`'s `windowSize`:
            // "If this is a constant buffer, this value must be less or equal
            // to BatchRendererGroup.GetConstantBufferMaxWindowSize." A window
            // that is not a multiple of its own alignment rounds past itself —
            // 16000 with a 256-byte alignment gives 16128 — so the aligned
            // stride is clamped DOWN to the largest aligned value the window
            // holds. On the one adapter measured (16384 / 256) the two agree,
            // which is why nothing would have caught this at run time.
            var alignment = Math.Max(ConstantBufferAlignmentBytes, 1);
            var aligned = (bytes + alignment - 1) / alignment * alignment;

            // `InstancesPerBatch` sized this batch out of the aligned budget, so
            // the round-up above cannot pass the window. Asserted rather than
            // clamped: a clamp here would silently paper over the two going out
            // of step, which is the defect this pair already had once.
            // **A stride that is not a multiple of four cannot be expressed.**
            // `AllocateInstanceBuffer` and `FillStaging` both divide it by 4 to
            // reach a word index while `AddBatch` takes the untruncated byte
            // offset, so batch 1 onward would read its properties up to three
            // bytes from where the writer put them. A device reporting an
            // alignment that is not a multiple of 4 is the only way in.
            if (aligned % 4 != 0)
            {
                throw new DashscenePainterException(
                    $"the device's {alignment}-byte constant-buffer alignment gives a "
                    + $"{aligned}-byte batch stride, which is not a multiple of 4. The "
                    + "instance buffer is addressed in 4-byte words, so this stride cannot "
                    + "be expressed.");
            }

            var budget = ConstantBufferWindowBytes / alignment * alignment;
            if (aligned > budget)
            {
                throw new DashscenePainterException(
                    $"a batch stride of {aligned} bytes exceeds the {budget}-byte aligned "
                    + $"window the device allows ({ConstantBufferWindowBytes} bytes, "
                    + $"{alignment}-byte alignment). InstancesPerBatch and BatchStrideBytes "
                    + "have gone out of step.");
            }
            return aligned;
        }

        private void AllocateInstanceBuffer(int batches)
        {
            _instanceBuffer?.Dispose();
            var words = batches * _batchStrideBytes / 4;
            _instanceBuffer = new GraphicsBuffer(
                Rung == BrgRung.ConstantBuffer
                    ? GraphicsBuffer.Target.Constant
                    : GraphicsBuffer.Target.Raw,
                words,
                4);
            // Zeroed here and never again. The first sixteen bytes of every
            // window MUST be zero — that is the address a metadata value of 0
            // resolves to — and a `new uint[]` is already zero. Rows past
            // `InstanceCount` keep whatever a previous frame left, which no
            // draw command references.
            _staging = new uint[words];
        }

        /// Lay the instance data out as the metadata offsets say it is.
        ///
        /// Structure of arrays within each batch: property P's value for
        /// instance i sits at `offset(P) + i * 16`, which is what
        /// `ComputeDOTSInstanceDataAddress` computes on the shader side.
        private void FillStaging()
        {
            var toWorld = DocumentToWorld;
            // Inverted once, not once per batch. `Matrix4x4.inverse` is a real
            // computation and every batch writes the same value — a document is
            // one sheet.
            var toObject = toWorld.inverse;

            for (var b = 0; b < _batchCount; b++)
            {
                var baseWord = b * _batchStrideBytes / 4;

                // Bytes 0..16 stay zero — the address a metadata value of 0
                // resolves to. Then the two transforms, shared by every
                // instance in this window rather than repeated per instance:
                // a document is one sheet, so its object-to-world is one value
                // and ninety-six bytes an instance would be paid for nothing.
                WritePackedMatrix(baseWord + 4, toWorld);
                WritePackedMatrix(baseWord + 4 + 12, toObject);

                var propsWord = baseWord + HeadBytes / 4;
                var inBatch = InstancesInBatch(b);
                var first = b * _instancesPerBatch;

                WriteFloats(propsWord + 0 * _instancesPerBatch * 4, _packer.Quad, first, inBatch);
                WriteFloats(propsWord + 1 * _instancesPerBatch * 4, _packer.Corners, first, inBatch);
                WriteFloats(propsWord + 2 * _instancesPerBatch * 4, _packer.Shade, first, inBatch);
                WriteFloats(propsWord + 3 * _instancesPerBatch * 4, _packer.Pivot, first, inBatch);
                WriteUints(propsWord + 4 * _instancesPerBatch * 4, _packer.Paint, first, inBatch);
            }
        }

        private void WriteFloats(int word, float[] source, int firstInstance, int count)
        {
            for (var i = 0; i < count * 4; i++)
            {
                _staging[word + i] = Bits(source[firstInstance * 4 + i]);
            }
        }

        private void WriteUints(int word, uint[] source, int firstInstance, int count)
        {
            for (var i = 0; i < count * 4; i++)
            {
                _staging[word + i] = source[firstInstance * 4 + i];
            }
        }

        /// A `float4x4` as the `float3x4` BatchRendererGroup expects.
        ///
        /// Twelve floats, column-major, with the bottom row dropped: BRG's
        /// `unity_ObjectToWorld` is declared `float3x4` and a full matrix
        /// written here would put the next property's first word inside it.
        private void WritePackedMatrix(int word, Matrix4x4 m)
        {
            // Written straight into the staging buffer. A first version built a
            // `float[12]` here first — a heap allocation twice per batch per
            // frame, in the class whose own documentation says nothing is
            // allocated on a frame that fits.
            _staging[word] = Bits(m.m00);
            _staging[word + 1] = Bits(m.m10);
            _staging[word + 2] = Bits(m.m20);
            _staging[word + 3] = Bits(m.m01);
            _staging[word + 4] = Bits(m.m11);
            _staging[word + 5] = Bits(m.m21);
            _staging[word + 6] = Bits(m.m02);
            _staging[word + 7] = Bits(m.m12);
            _staging[word + 8] = Bits(m.m22);
            _staging[word + 9] = Bits(m.m03);
            _staging[word + 10] = Bits(m.m13);
            _staging[word + 11] = Bits(m.m23);
        }

        /// One float's bits, as the raw word the instance buffer carries.
        private static uint Bits(float value)
        {
            return unchecked((uint)BitConverter.SingleToInt32Bits(value));
        }

        private void AddBatches(int batches)
        {
            _batches = new BatchID[batches];
            var metadata = new NativeArray<MetadataValue>(7, Allocator.Temp);
            try
            {
                for (var b = 0; b < batches; b++)
                {
                    // **Which rung carries the batch's byte offset — issue
                    // #1389.** On `ConstantBuffer` the window carries it and
                    // the metadata stays window-relative. On `RawBuffer` there
                    // is no window: Unity requires BOTH window parameters to be
                    // zero and rejects the batch otherwise — with a log line
                    // rather than an exception — so the offset is folded into
                    // the metadata instead. Passing it as a window offset on
                    // this rung refused every batch after the first and left
                    // `_batches[b]` at its default.
                    //
                    // **Unreachable through the shipped path today**, because
                    // `InstancesPerBatch` doubles until one batch covers the
                    // whole document on this rung, so `b` is always zero and
                    // the offset is always zero. It was exercised by capping
                    // that capacity to 64 so eight batches were allocated —
                    // macOS/Metal, 2026-08-31: before this change Unity refused
                    // seven of the eight and the frame came back empty; after
                    // it, none were refused and the frame matched the
                    // single-batch one. That cap is not a shipped path, so no
                    // test claims this.
                    //
                    // The shared transforms carry NO per-instance bit, so every
                    // instance reads the same one — that is the whole reason a
                    // UI document costs eighty bytes an instance rather than a
                    // hundred and seventy-six.
                    var window = Rung == BrgRung.ConstantBuffer ? 0 : b * _batchStrideBytes;
                    metadata[0] = Shared("unity_ObjectToWorld", window + 16);
                    metadata[1] = Shared("unity_WorldToObject", window + 16 + 48);
                    var props = window + HeadBytes;
                    var stride = _instancesPerBatch * 16;
                    metadata[2] = PerInstance(PaintProperties.Quad, props);
                    metadata[3] = PerInstance(PaintProperties.Corners, props + stride);
                    metadata[4] = PerInstance(PaintProperties.Shade, props + 2 * stride);
                    metadata[5] = PerInstance(PaintProperties.Pivot, props + 3 * stride);
                    metadata[6] = PerInstance(PaintProperties.Paint, props + 4 * stride);

                    _batches[b] = _brg.AddBatch(
                        metadata,
                        _instanceBuffer.bufferHandle,
                        Rung == BrgRung.ConstantBuffer ? (uint)(b * _batchStrideBytes) : 0u,
                        Rung == BrgRung.ConstantBuffer ? (uint)_batchStrideBytes : 0u);
                }
            }
            finally
            {
                metadata.Dispose();
            }
        }

        private static MetadataValue PerInstance(string name, int byteOffset)
        {
            return new MetadataValue
            {
                NameID = Shader.PropertyToID(name),
                // The high bit is what makes an address per-instance: without
                // it every instance reads the value at this one offset.
                Value = 0x80000000 | (uint)byteOffset,
            };
        }

        private static MetadataValue Shared(string name, int byteOffset)
        {
            return new MetadataValue
            {
                NameID = Shader.PropertyToID(name),
                Value = (uint)byteOffset,
            };
        }

        private void UploadHeap()
        {
            Upload(ref _paintBuffer, ref _paintStaging, _packer.Paints, _packer.PaintFloats);
            Upload(ref _clipBuffer, ref _clipStaging, _packer.ClipBoxes, _packer.ClipFloats);
            Upload(ref _strokeBuffer, ref _strokeStaging, _packer.Strokes, _packer.StrokeFloats);
            Upload(ref _glyphBuffer, ref _glyphStaging, _packer.Glyphs, _packer.GlyphFloats);
        }

        /// Upload one heap table as `float4` rows.
        ///
        /// Rounded up to a whole `float4`, and **never zero-length**: a
        /// `StructuredBuffer` the shader declares must be bound even when the
        /// document has no gradient, or every draw that reads it is skipped
        /// with a binding error rather than drawing the fills that do exist.
        ///
        /// The buffer grows by doubling and the staging array with it, so a
        /// steady document allocates nothing here — the same R-T4 rule the
        /// instance buffer follows. `GraphicsBuffer.count` is the allocated row
        /// count rather than the used one, so the comparison below is against
        /// capacity.
        private static void Upload(
            ref GraphicsBuffer buffer,
            ref float[] staging,
            float[] source,
            int floats)
        {
            var rows = Math.Max((floats + 3) / 4, 1);
            if (buffer == null || buffer.count < rows)
            {
                // **Read before disposing.** A first version called
                // `buffer?.Dispose()` and then read `buffer?.count` to seed the
                // doubling — a property read on a released native object, which
                // is reached on the second growth of any heap table.
                var capacity = Math.Max(buffer?.count ?? 0, 1);
                buffer?.Dispose();
                while (capacity < rows)
                {
                    capacity *= 2;
                }
                buffer = new GraphicsBuffer(GraphicsBuffer.Target.Structured, capacity, 16);
                staging = new float[capacity * 4];
            }

            // Only the live floats are copied. What sits past them is whatever
            // a previous frame left, and no row index in this frame's instances
            // reaches it — the same reasoning the instance staging buffer uses
            // for the rows past `InstanceCount`.
            Array.Copy(source, staging, Math.Min(floats, source.Length));
            // **Only the live rows.** A first version pushed the whole doubled
            // capacity every frame — thousands of stale `float4`s for a
            // document that had once been large. Issue #1306 records the same
            // cost for the instance buffer, where the fix is a dirty range
            // rather than a length.
            buffer.SetData(staging, 0, 0, rows * 4);
        }

        /// Bind the paint heap on every material this painter draws with.
        ///
        /// **Per material and not process-wide, and that is issue #1297's
        /// fix.** `Shader.SetGlobalBuffer` and `Shader.SetGlobalVector` bind
        /// into one namespace the whole process shares, so two painters shaded
        /// from one heap and the last one to draw supplied the gradients,
        /// strokes and clip boxes every painter's fragments read. A painter
        /// binds its own materials here, and a second painter in the same
        /// process reaches none of them.
        ///
        /// **Every material, on every frame.** A heap buffer is reallocated
        /// when its table outgrows it, so a binding taken once at construction
        /// would name a freed buffer after the first growth — and
        /// [`SetAtlases`] mints text materials long after the constructor has
        /// run, so there is no earlier moment at which the set of materials is
        /// complete.
        private void BindHeap()
        {
            var scalars = new Vector4(
                EdgeWidth, _packer.SolidBase, _packer.GradientBase, 0.0f);
            BindHeapTo(_material, scalars);
            for (var i = 0; i < _textMaterials.Length; i++)
            {
                BindHeapTo(_textMaterials[i], scalars);
                // **The glyph rows go on the text materials alone**, because
                // the shading declares `_DsGlyphs` under `DASHSCENE_CLASS_TEXT`
                // and no other class can reach a glyph run. **The sheet itself
                // is not here** — `SetAtlases` binds it, one texture per
                // material, because a document may name more than one.
                _textMaterials[i].SetBuffer(GlyphsId, _glyphBuffer);
            }
        }

        /// Bind the three tables every class reads, and the scalars, on one
        /// material.
        private void BindHeapTo(Material material, Vector4 scalars)
        {
            material.SetBuffer(PaintsId, _paintBuffer);
            material.SetBuffer(ClipBoxesId, _clipBuffer);
            material.SetBuffer(StrokesId, _strokeBuffer);
            material.SetVector(ScalarsId, scalars);
        }

        private void RemoveBatches()
        {
            if (_brg == null)
            {
                return;
            }

            for (var b = 0; b < _batches.Length; b++)
            {
                if (_batches[b] != default)
                {
                    _brg.RemoveBatch(_batches[b]);
                }
            }
            _batches = Array.Empty<BatchID>();
            _batchCount = 0;
        }

        /// The unit quad every instance draws.
        ///
        /// `[0, 1] x [0, 1]` in the xy plane, so the vertex stage reads a
        /// corner as a fraction of the node's box and needs no per-instance
        /// mesh. Two triangles, wound so the quad faces -z; both material
        /// classes set `Cull Off`, so the winding is documentation rather than
        /// a correctness property.
        private static Mesh UnitQuad()
        {
            var mesh = new Mesh
            {
                name = "Dashscene Unit Quad",
                hideFlags = HideFlags.HideAndDontSave,
                vertices = new[]
                {
                    new Vector3(0.0f, 0.0f, 0.0f),
                    new Vector3(1.0f, 0.0f, 0.0f),
                    new Vector3(1.0f, 1.0f, 0.0f),
                    new Vector3(0.0f, 1.0f, 0.0f),
                },
                triangles = new[] { 0, 1, 2, 0, 2, 3 },
            };
            // The quad is placed by the vertex stage from the instance's own
            // box, so its own bounds say nothing useful. The group's global
            // bounds are what culling uses.
            mesh.bounds = new Bounds(Vector3.zero, Vector3.one);
            return mesh;
        }

        private void ThrowIfDisposed()
        {
            if (_disposed)
            {
                throw new ObjectDisposedException(nameof(BrgPainter));
            }
        }

        /// Releases the group, its buffers and the material and mesh it minted.
        ///
        /// **Order matters**: the batches name the instance buffer, so they go
        /// first. A `BatchRendererGroup` disposed with batches outstanding
        /// leaves Unity holding a handle to a buffer this object has freed.
        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            // The R-E5 latch is identity only, but it is a strong reference to
            // a pipeline this painter does not own; releasing it here keeps a
            // disposed painter from being that instance's last root.
            _batcherReportedFor = null;

            RemoveBatches();
            // **Before the group goes**, so every material this painter
            // registered is unregistered while the group that holds it still
            // exists.
            ReleaseAtlases();
            ReleaseUnityObjects();

            // **Nothing is unbound here, and that is what binding per material
            // buys.** While these were bound process-wide, a disposed painter
            // left `_DsPaints`, `_DsClipBoxes` and `_DsStrokes` naming released
            // native buffers, which anything drawing a `Dashscene/*` material
            // afterwards would sample — issue #1297's hazard with no live owner
            // at all. The only materials that name these buffers are this
            // painter's own, and `ReleaseAtlases` and `ReleaseUnityObjects`
            // above have destroyed every one of them before the buffers go.
            _instanceBuffer?.Dispose();
            _instanceBuffer = null;
            _paintBuffer?.Dispose();
            _paintBuffer = null;
            _clipBuffer?.Dispose();
            _clipBuffer = null;
            _strokeBuffer?.Dispose();
            _strokeBuffer = null;
            _glyphBuffer?.Dispose();
            _glyphBuffer = null;
        }

        /// Release the group, the material and the mesh, in that order.
        ///
        /// Shared by `Dispose` and by the constructor's failure path, which is
        /// the whole reason it is a method: the two must free the same set, and
        /// a constructor that freed a subset would leak exactly the objects
        /// nobody can reach.
        private void ReleaseUnityObjects()
        {
            _brg?.Dispose();
            _brg = null;

            if (_material != null)
            {
                UnityEngine.Object.DestroyImmediate(_material);
                _material = null;
            }
            if (_mesh != null)
            {
                UnityEngine.Object.DestroyImmediate(_mesh);
                _mesh = null;
            }
        }
    }
}
