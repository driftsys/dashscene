// The ordered draw path: the overlay class and text, drawn in document order
// through one render-graph pass (story #1448).
//
// **Why a pass beside BatchRendererGroup.** The two blended classes write no
// depth and test none, so the order of their draws is the only thing that
// decides what covers what. BatchRendererGroup groups draw commands by
// material, and the sorting keys `BrgPainter.OnPerformCulling` writes to
// recover document order were measured rather than guaranteed
// (`docs/decisions/brg-draw-command-order-is-not-guaranteed.md`). Here the
// order of the draws IS document order: one `DrawProcedural` per contiguous
// run of instances that share a material, in instance order, with no key.
//
// **Injected by the package, not by a renderer feature.** A
// `ScriptableRendererFeature` takes effect only once it is added to the host's
// renderer asset, which is a step the package cannot make for its host — the
// class of defect issue #1313 was, a package that cannot draw as installed.
// This class enqueues its pass from `RenderPipelineManager.beginCameraRendering`
// on each game and scene camera instead, which URP runs before it records the
// camera, and which `RenderPipeline.SubmitRenderRequest` runs as well.

using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Rendering.RenderGraphModule;
using UnityEngine.Rendering.Universal;

namespace Driftsys.Dashscene
{
    /// One contiguous run of instances that draw with one material on the
    /// ordered path.
    ///
    /// `Offset` is the run's first row in the painter's `_DsInstances` buffer,
    /// which is its first instance index, and `Count` the number of instances.
    internal readonly struct OrderedRun
    {
        internal OrderedRun(Material material, int offset, int count)
        {
            Material = material;
            Offset = offset;
            Count = count;
        }

        internal Material Material { get; }

        internal int Offset { get; }

        internal int Count { get; }
    }

    /// Draws every registered painter's ordered runs after the transparent
    /// queue, in registration order and, within a painter, in run order.
    internal sealed class OrderedDrawPass : ScriptableRenderPass
    {
        /// The row of a draw's first instance, which the procedural variant
        /// adds to `SV_InstanceID`.
        ///
        /// Resolved once, for the reason `BrgPainter` resolves its heap names
        /// once: the string overloads hash the name on every call, and this
        /// one is set before every draw.
        private static readonly int FirstInstanceId =
            Shader.PropertyToID(OrderedDrawProperties.FirstInstance);

        /// The painters whose runs this pass draws, in registration order.
        ///
        /// **When to subscribe is the registry's answer**, executed by
        /// `unity/ffi-check` in `Runtime/PainterRegistry.cs`; this class only
        /// acts on it.
        private static readonly PainterRegistry<BrgPainter> s_painters =
            new PainterRegistry<BrgPainter>();

        /// The one instance every camera enqueues. A pass carries no per-frame
        /// state of its own: the runs are read from the painters when the
        /// render graph is recorded.
        private static readonly OrderedDrawPass s_pass = new OrderedDrawPass();

        private OrderedDrawPass()
        {
            // **After the transparent queue, which is not the same as being
            // in it.** A document draws over whatever the engine drew before
            // it. Under BatchRendererGroup the overlay class's commands were
            // sorted by distance among the scene's transparent objects; here
            // every ordered draw, which tests no depth, paints over every
            // transparent object the scene drew, at any distance — a
            // behaviour change `docs/design/unity-csharp-host.md`, "Two draw
            // paths", records.
            renderPassEvent = RenderPassEvent.AfterRenderingTransparents;
        }

        /// What one painter's raster pass reads when it executes.
        private sealed class PassData
        {
            internal List<OrderedRun> Runs;
            internal MaterialPropertyBlock Block;
            internal Matrix4x4 Sheet;
        }

        /// Start drawing `painter`'s runs. Called by the painter once it is
        /// built.
        ///
        /// **Subscribes on the first painter**, so a process with no painter
        /// pays nothing on any camera.
        public static void Register(BrgPainter painter)
        {
            if (s_painters.Add(painter))
            {
                RenderPipelineManager.beginCameraRendering += OnBeginCameraRendering;
            }
        }

        /// Stop drawing `painter`'s runs. Called by the painter's `Dispose`,
        /// so a disposed painter's destroyed materials are never drawn.
        public static void Unregister(BrgPainter painter)
        {
            if (s_painters.Remove(painter))
            {
                RenderPipelineManager.beginCameraRendering -= OnBeginCameraRendering;
            }
        }

        /// Forget every painter and the subscription when play mode starts.
        ///
        /// **For Enter Play Mode without a domain reload**, where statics keep
        /// their values from the previous session: a painter the previous
        /// session never disposed would stay registered, and this pass would
        /// keep drawing its runs — old content, or destroyed materials — in
        /// every later camera. `SubsystemRegistration` runs before any scene
        /// loads, so no painter of the new session has registered yet.
        /// Removing a handler that is not subscribed does nothing.
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void ResetOnEnterPlayMode()
        {
            RenderPipelineManager.beginCameraRendering -= OnBeginCameraRendering;
            s_painters.Clear();
        }

        /// Whether any registered painter has a run to draw.
        ///
        /// A painter that was built and never drew holds no run, so the pass is
        /// not enqueued for it and draws nothing.
        private static bool AnyRuns()
        {
            for (var i = 0; i < s_painters.Count; i++)
            {
                if (s_painters[i].OrderedRuns.Count > 0)
                {
                    return true;
                }
            }
            return false;
        }

        /// Enqueue the pass on a game or scene camera that has runs to draw.
        ///
        /// **Game and scene cameras only, and only those whose culling mask
        /// includes the document's layer.** Preview, reflection and the other
        /// camera types render for the editor or for an effect, not for the
        /// picture a host shows, and BatchRendererGroup's own culling callback
        /// is what decided those until now.
        private static void OnBeginCameraRendering(ScriptableRenderContext context, Camera camera)
        {
            if (camera.cameraType != CameraType.Game && camera.cameraType != CameraType.SceneView)
            {
                return;
            }

            // **The document's layer, as BatchRendererGroup filters it.** The
            // culling callback puts every command on `BrgPainter.DocumentLayer`,
            // so a camera whose culling mask excludes that layer draws none of
            // the document there. Without this test such a camera — a stacked
            // overlay camera is the common one — would draw the ordered runs,
            // compositing every translucent run a second time over a base
            // camera that already drew it.
            if ((camera.cullingMask & (1 << BrgPainter.DocumentLayer)) == 0) { return; }

            if (!AnyRuns()) { return; }

            // `GetUniversalAdditionalCameraData` adds the component to a camera
            // that has none, which URP's own rendering of that camera does as
            // well; a camera skipped here would draw no overlay node and no
            // text, with nothing reporting it.
            var renderer = camera.GetUniversalAdditionalCameraData().scriptableRenderer;
            renderer?.EnqueuePass(s_pass);
        }

        /// Record one raster pass per painter that has runs, in registration
        /// order, each drawing that painter's runs in the order they were cut.
        public override void RecordRenderGraph(RenderGraph renderGraph, ContextContainer frameData)
        {
            var resourceData = frameData.Get<UniversalResourceData>();

            for (var i = 0; i < s_painters.Count; i++)
            {
                var painter = s_painters[i];
                if (painter.OrderedRuns.Count == 0) { continue; }

                using var builder = renderGraph.AddRasterRenderPass<PassData>(
                    "Dashscene ordered draws", out var data);
                data.Runs = painter.OrderedRuns;
                data.Block = painter.OrderedBlock;
                data.Sheet = painter.OrderedSheet;

                // **Colour only.** Both blended shaders declare `ZWrite Off` and
                // `ZTest Always`, so no depth is read or written.
                builder.SetRenderAttachment(resourceData.activeColorTexture, 0);

                // **Never culled.** The pass writes the camera's colour, which
                // the graph would keep anyway; this says so, rather than
                // leaving the draw to an inference about the attachment.
                builder.AllowPassCulling(false);

                builder.SetRenderFunc(static (PassData d, RasterGraphContext ctx) =>
                {
                    // **In run order, and the first instance as an integer.**
                    // The order of these draws is the picture. `SetInteger` is
                    // the typed, non-deprecated setter for the `int`
                    // `_DsFirstInstance`. `SetInt` is the deprecated float path,
                    // and it was measured NOT to change the picture on Metal
                    // with editor 6000.3.23f1 (story #1448's mutation run:
                    // Unity converted the float to the int uniform), so the
                    // choice is pinned by the package-gate scans alone. The
                    // block's values are copied into the command as it is
                    // recorded, so setting the next run's value does not
                    // change this one's.
                    foreach (var run in d.Runs)
                    {
                        d.Block.SetInteger(FirstInstanceId, run.Offset);
                        ctx.cmd.DrawProcedural(d.Sheet, run.Material, 0, MeshTopology.Triangles, 6, run.Count, d.Block);
                    }
                });
            }
        }
    }
}
