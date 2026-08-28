// The Android probe's runtime half: take the read `unity-painter-uses-brg.md`
// D4 selects a rung from, on a device, and say so in one parseable line.
//
// **Not part of the package.** `just unity-android` copies it into a throwaway
// project beside `AndroidProbeBuild.cs`.
//
// **The order of the two reads is the whole design of this file.**
// `BatchRendererGroup.BufferTarget` is read and reported BEFORE the painter is
// constructed, because constructing the painter is what can fail: a stripped
// shader throws, and rung 3 draws nothing. A probe that read the value off
// `BrgPainter.Rung` alone would report nothing at all in exactly the cases
// worth reporting, and the run would look like a build failure rather than
// like the answer it is.

using System;
using Driftsys.Dashscene;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Reads BufferTarget on a device and reports it, then quits.</summary>
public sealed class DashsceneAndroidProbe : MonoBehaviour
{
    /// The prefix `just unity-android` greps logcat for.
    ///
    /// **One line, machine-readable.** The recipe fails when it does not appear,
    /// so this string is the contract between the two halves rather than a
    /// convenience for a human reader.
    private const string ReadTag = "[android-probe] READ";

    private DashsceneRuntime _runtime;
    private BrgPainter _painter;

    private void Start()
    {
        // R-E14: this read is a verdict only in a process that has a graphics
        // device. A player on a device has one; a `-nographics` batchmode run
        // does not, and D4 records that such a run can return the value the
        // table maps to rung 3 and abandon BatchRendererGroup on a read taken
        // with no device at all. `SystemInfo.graphicsDeviceType` is reported
        // beside the value so a reader can tell the two apart.
        var api = SystemInfo.graphicsDeviceType;

        // **Guarded, though these are the reads this file exists to take.** An
        // unguarded throw here aborts `Start` before the report, and the recipe
        // then says "the player reported no read" — indistinguishable from a
        // launch failure. The `ConstantBuffer` branch is exactly the one no
        // adapter has yet selected, so it is the least-exercised code in this
        // file and the most likely to surprise.
        var target = BatchBufferTarget.Unknown;
        var window = 0;
        var alignment = 0;
        try
        {
            target = BatchRendererGroup.BufferTarget;
            if (target == BatchBufferTarget.ConstantBuffer)
            {
                window = BatchRendererGroup.GetConstantBufferMaxWindowSize();
                alignment = BatchRendererGroup.GetConstantBufferOffsetAlignment();
            }
        }
        catch (Exception e)
        {
            Debug.LogError(
                $"[android-probe] the BufferTarget read itself threw: {e.GetType().Name}: "
                + $"{e.Message}. The line below reports {target}, which is NOT a verdict.");
        }

        // The read, before anything that can throw.
        Debug.Log(
            $"{ReadTag} api={api} BufferTarget={target} window={window} alignment={alignment} "
            + $"device={SystemInfo.deviceModel} gpu={SystemInfo.graphicsDeviceName}");

        // The rung the painter actually selects from that value. Separate line,
        // because this one can be preceded by a throw.
        try
        {
            _painter = new BrgPainter();
            Debug.Log(
                $"[android-probe] rung={_painter.Rung} "
                + $"window={_painter.ConstantBufferWindowBytes} "
                + $"alignment={_painter.ConstantBufferAlignmentBytes}");
        }
        catch (Exception e)
        {
            // **Reported, not swallowed.** A painter that cannot construct on
            // the target is a finding about the target, and it is the one this
            // probe exists to surface. Issue #1313's stripped shaders land here.
            Debug.LogError(
                $"[android-probe] the painter did not construct: {e.GetType().Name}: "
                + $"{e.Message}");
        }

        // The runtime is constructed after the read rather than before it, so a
        // library that fails to load cannot suppress the graphics answer.
        try
        {
            _runtime = new DashsceneRuntime();
            Debug.Log("[android-probe] runtime constructed, so the shipped .so loaded");
        }
        catch (Exception e)
        {
            Debug.LogError(
                $"[android-probe] the runtime did not construct: {e.GetType().Name}: "
                + $"{e.Message}. R-E21 is the requirement a missing library breaks.");
        }

        Debug.Log("[android-probe] DONE");

        // **Quit, so the disposal below actually runs.** Without this the app
        // sits on the device after the recipe returns and `OnDestroy` is dead
        // code — and this type's own summary said "then quits", which was
        // false until now.
        Application.Quit();
    }

    private void OnDestroy()
    {
        _painter?.Dispose();
        _runtime?.Dispose();
    }
}
