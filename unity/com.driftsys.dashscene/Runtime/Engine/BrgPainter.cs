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

        private BatchID[] _batches = Array.Empty<BatchID>();
        private int _batchCount;
        private int _instancesPerBatch;
        private int _batchStrideBytes;

        private uint[] _staging = Array.Empty<uint>();
        private float[] _paintStaging = Array.Empty<float>();
        private float[] _clipStaging = Array.Empty<float>();
        private float[] _strokeStaging = Array.Empty<float>();
        private float _cutoff = 0.5f;
        private Bounds _globalBounds = new Bounds(Vector3.zero, Vector3.one * 10000.0f);
        private PackDiagnostics _lastDiagnostics;
        private bool _disposed;

        /// How many painters bind the global paint heap. See the constructor.
        private static int _liveCount;

        /// Whether this painter incremented [`_liveCount`], so `Dispose`
        /// decrements exactly what the constructor added and a rung-3 painter
        /// leaves it untouched in both directions.
        private bool _counted;

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
                _material?.SetFloat(PaintMaterialProperties.Cutoff, value);
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
                    // and says which rung it is on — descending below rung 1 is
                    // D3's trigger to raise the R-T4 conflict.
                    Rung = BrgRung.InstancedWithoutBrg;
                    // **Not counted, and `Dispose` will not decrement it.** A
                    // rung-3 painter constructs no group and `Draw` returns
                    // before `BindGlobals`, so it binds nothing globally and
                    // cannot be the second party to issue #1297's collision.
                    // A first version returned above the increment while
                    // `Dispose` decremented unconditionally, driving the
                    // counter to -1; a second counted it here and warned about
                    // a collision it cannot take part in. `_counted` is what
                    // balances the two without either fault.
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

            if (!GraphicsSettings.useScriptableRenderPipelineBatching)
            {
                // R-E5 is the host project's requirement rather than this
                // painter's, so this reports instead of throwing: Unity's own
                // refusal — "Please turn SRP Batcher ON to use the
                // BatchRendererGroup API" — has never been observed in this
                // repository, and a host meeting it late should see why.
                Debug.LogWarning(
                    "[dashscene] the SRP Batcher is off. BatchRendererGroup needs it "
                    + "(docs/specification/07-embedding-and-distribution.md R-E5): set "
                    + "m_UseSRPBatcher on the active render pipeline asset.");
            }

            var shaderName = PaintShaders.For(materialClass);
            var shader = Shader.Find(shaderName);
            if (shader == null)
            {
                throw new DashscenePainterException(
                    $"the shader '{shaderName}' was not found. It ships in this package "
                    + "under Runtime/Shaders/; a Git-URL install that is missing it has "
                    + "lost a .meta file, which is R-E2.");
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
                    _material.SetFloat(PaintMaterialProperties.Cutoff, _cutoff);
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

            // **The paint heap is bound globally, so two painters share it.**
            // `Shader.SetGlobalBuffer` is what a BatchRendererGroup shader's
            // `StructuredBuffer` is reachable through, and the last painter to
            // draw wins — two documents in one process would each be shaded
            // from the other's gradients, strokes and clip boxes. Reported
            // rather than silently wrong, and issue #1297 carries the fix,
            // which needs a device to verify because the alternative binding
            // path cannot be told apart from "nothing drew" without one.
            _counted = true;
            CountLive();
        }

        /// Count this painter, and warn if it is not the only one.
        private static void CountLive()
        {
            if (System.Threading.Interlocked.Increment(ref _liveCount) > 1)
            {
                Debug.LogWarning(
                    "[dashscene] a second BrgPainter exists in this process. The paint heap is "
                    + "bound with Shader.SetGlobalBuffer, so the last painter to draw supplies "
                    + "the gradients, strokes and clip boxes every painter shades from. Draw one "
                    + "document per process until issue #1297 is fixed.");
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
        /// and without throwing: the rung was already reported at construction,
        /// and throwing once per frame would bury it.
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

            _packer.Pack(lease.Frame, _materialClass);
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
            BindGlobals();
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

            // How many draw commands: one per batch, split again where a batch
            // holds more visible instances than one command may carry.
            var commandCount = 0;
            for (var b = 0; b < _batchCount; b++)
            {
                commandCount += CommandsInBatch(b);
            }

            // Unity frees all of these. `Allocator.TempJob` is what the API
            // documents for a callback that returns a handle, and this one
            // returns `default` — the work is done here, on the thread issue
            // #1267 measured as Unity's main one.
            drawCommands->drawCommandCount = commandCount;
            drawCommands->drawCommands = Malloc<BatchDrawCommand>(commandCount);
            drawCommands->drawRangeCount = 1;
            drawCommands->drawRanges = Malloc<BatchDrawRange>(1);
            drawCommands->visibleInstanceCount = InstanceCount;
            drawCommands->visibleInstances = Malloc<int>(InstanceCount);
            drawCommands->instanceSortingPositions = null;
            drawCommands->instanceSortingPositionFloatCount = 0;

            drawCommands->drawRanges[0] = new BatchDrawRange
            {
                drawCommandsBegin = 0,
                drawCommandsCount = (uint)commandCount,
                filterSettings = new BatchFilterSettings
                {
                    renderingLayerMask = 0xffffffff,
                    layer = 0,
                    motionMode = MotionVectorGenerationMode.Camera,
                    shadowCastingMode = ShadowCastingMode.Off,
                    receiveShadows = false,
                    staticShadowCaster = false,
                    allDepthSorted = false,
                },
            };

            var visible = 0;
            var command = 0;
            for (var b = 0; b < _batchCount; b++)
            {
                var inBatch = InstancesInBatch(b);
                var emitted = 0;
                while (emitted < inBatch)
                {
                    var run = Math.Min(inBatch - emitted, MaxInstancesPerDrawCommand);

                    // Every instance's index is relative to its BATCH, not to
                    // the buffer: the metadata offsets are window-relative, so
                    // instance 0 of every batch is the first row of that
                    // window's own property arrays.
                    for (var i = 0; i < run; i++)
                    {
                        drawCommands->visibleInstances[visible + i] = emitted + i;
                    }

                    drawCommands->drawCommands[command] = new BatchDrawCommand
                    {
                        visibleOffset = (uint)visible,
                        visibleCount = (uint)run,
                        batchID = _batches[b],
                        materialID = _materialId,
                        meshID = _meshId,
                        submeshIndex = 0,
                        splitVisibilityMask = 0xff,
                        flags = BatchDrawCommandFlags.None,
                        sortingPosition = 0,
                    };

                    visible += run;
                    emitted += run;
                    command++;
                }
            }

            return default;
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

        /// How many draw commands batch `b` needs.
        ///
        /// **R-E20, and the reason it is a division rather than an assertion.**
        /// At most 256 visible instances per `BatchDrawCommand`, because the
        /// SRP core shader library declares an array of exactly that length. A
        /// painter that asserted the bound would refuse documents it can draw;
        /// splitting is what honours it.
        private int CommandsInBatch(int b)
        {
            var inBatch = InstancesInBatch(b);
            return (inBatch + MaxInstancesPerDrawCommand - 1) / MaxInstancesPerDrawCommand;
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
        /// **Nothing is allocated on a frame that fits**, which is R-T4: "CPU
        /// frame cost = dirty-range instance-buffer upload from the rect table
        /// + submission. Nothing else." Capacity grows by doubling and never
        /// shrinks, and the batches are added once per growth rather than once
        /// per frame — a first version sized the buffer to the exact instance
        /// count, which reallocated the `GraphicsBuffer` and re-added every
        /// batch on any frame where a single node appeared or left.
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

            // Also bounded by what one draw command may carry, so a batch never
            // needs a second command. Not required by R-E20 — the split in
            // `CommandsInBatch` honours it either way — but it keeps one batch
            // to one command, which is what a reader of a frame capture
            // expects.
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
                    // Window-relative offsets. The shared transforms carry NO
                    // per-instance bit, so every instance reads the same one —
                    // that is the whole reason a UI document costs eighty bytes
                    // an instance rather than a hundred and seventy-six.
                    metadata[0] = Shared("unity_ObjectToWorld", 16);
                    metadata[1] = Shared("unity_WorldToObject", 16 + 48);
                    var props = HeadBytes;
                    var stride = _instancesPerBatch * 16;
                    metadata[2] = PerInstance(PaintProperties.Quad, props);
                    metadata[3] = PerInstance(PaintProperties.Corners, props + stride);
                    metadata[4] = PerInstance(PaintProperties.Shade, props + 2 * stride);
                    metadata[5] = PerInstance(PaintProperties.Pivot, props + 3 * stride);
                    metadata[6] = PerInstance(PaintProperties.Paint, props + 4 * stride);

                    _batches[b] = _brg.AddBatch(
                        metadata,
                        _instanceBuffer.bufferHandle,
                        (uint)(b * _batchStrideBytes),
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

        private void BindGlobals()
        {
            Shader.SetGlobalBuffer(PaintGlobals.Paints, _paintBuffer);
            Shader.SetGlobalBuffer(PaintGlobals.ClipBoxes, _clipBuffer);
            Shader.SetGlobalBuffer(PaintGlobals.Strokes, _strokeBuffer);
            Shader.SetGlobalVector(
                PaintGlobals.Scalars,
                new Vector4(EdgeWidth, _packer.SolidBase, _packer.GradientBase, 0.0f));
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
            if (_counted)
            {
                System.Threading.Interlocked.Decrement(ref _liveCount);
            }

            RemoveBatches();
            ReleaseUnityObjects();

            // **Unbound before they are freed.** `BindGlobals` binds these
            // process-wide, so a disposed painter would otherwise leave
            // `_DsPaints`, `_DsClipBoxes` and `_DsStrokes` naming released
            // native buffers — which anything drawing a `Dashscene/*` material
            // afterwards would sample. That is issue #1297's hazard with no
            // live owner at all.
            Shader.SetGlobalBuffer(PaintGlobals.Paints, (GraphicsBuffer)null);
            Shader.SetGlobalBuffer(PaintGlobals.ClipBoxes, (GraphicsBuffer)null);
            Shader.SetGlobalBuffer(PaintGlobals.Strokes, (GraphicsBuffer)null);

            _instanceBuffer?.Dispose();
            _instanceBuffer = null;
            _paintBuffer?.Dispose();
            _paintBuffer = null;
            _clipBuffer?.Dispose();
            _clipBuffer = null;
            _strokeBuffer?.Dispose();
            _strokeBuffer = null;

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
