// The showcase's frame pacing: an explicit 60, stated before the first frame.
//
// **Why a number, and why this early.** With `Application.targetFrameRate` left
// at -1 Unity's Android player paces at 30 fps whatever the panel does, and
// until issue #1408 nothing in this package or the demo player set it — the
// Unity frame-cost table in `docs/design/android-toolchain.md` was taken under
// that cap, and its presented-rate section says which readings were not.
// `SubsystemRegistration` runs before the first scene loads and before the
// first frame; with this edit and the build's the compositor lists the app
// under `FrameRateOverrides` as asking for `60.00 Hz` rather than 30.
//
// **The trap, recorded beside the fix.** Reading the display's rate back is not
// an answer. Unity's init has already asked SurfaceFlinger for 30 Hz — read
// off the compositor on 2026-09-03 as the app's `setFrameRate` at 30.00 Hz —
// and under Android's per-app frame-rate override
// `Screen.currentResolution.refreshRateRatio` reports the rate the app was
// GRANTED, so an `Awake()` line that set the
// target from it read 30 and set 30 again (measured 2026-09-03 on the Pixel 5;
// the toolchain record's "The Unity host's presented rate" carries the three
// steps). A literal 60 set in `Awake()` was not among the arms measured, so
// what the record shows is that the read-back value was wrong, and this early
// site is the one that was read. `unity/package-gate`'s `frame_pacing` scan
// holds this method to the one statement below, refuses any other assignment
// of the target in the package's `Runtime/` and samples, and refuses a read of
// `refreshRateRatio` or `currentResolution` in this sample's code.
//
// **The other half is the build.** Asking for 60 with Unity's default pacing
// presented on every other vsync; `unity/demo/DemoBuild.cs` turns
// `PlayerSettings.Android.optimizedFramePacing` on for the Android player.
//
// **What this does to a player the sample is compiled into.** This runs in
// every player and every Editor play session that carries the sample, before
// the first scene, and sets the process-wide target — a project that imports
// the Showcase sample and sets its own target elsewhere gets this one first.
// 60 is the Pixel 5's measured mode (the budget record's D1); an Android
// panel above 60 Hz is capped by it. On desktop the value is ignored while
// `QualitySettings.vSyncCount` is not zero — Unity's default, and what the
// desktop demo player was measured at — so the cap is Android's unless a
// project turns vsync off; the Editor's Game view has it off by default,
// so a play session there is capped.

using UnityEngine;

namespace Driftsys.Dashscene.Samples
{
    /// <summary>The showcase's frame-rate target, set before the first frame.</summary>
    public static class DashsceneFramePacing
    {
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
        private static void PaceAtSixty()
        {
            Application.targetFrameRate = 60;
        }
    }
}
