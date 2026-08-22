// Committing below the display rate, without drifting off it.
//
// This is the arithmetic half of a host's frame loop, and it is here rather
// than in `Samples~/FrameLoop/` for one reason: nothing compiles a sample.
// `unity/package-compat` and `unity/ffi-check` both glob `Runtime/**/*.cs`, and
// no CI job runs a Unity editor, so logic left in the sample is checked by
// nobody. It is pure — no `UnityEngine` reference — so it costs the package
// nothing to keep it where the gates can reach it.

using System;

namespace Driftsys.Dashscene
{
    /// Decides which frames carry a commit when committing below the display
    /// rate.
    ///
    /// **Pick a divisor of the display rate** (issue #851). At 60 Hz a 16 Hz
    /// commit lands on 4, 4, 4, 3 frames alternating — an uneven cadence on top
    /// of a low rate. 15 and 20 divide exactly.
    ///
    /// The other half of that rule is not arithmetic and cannot live here:
    /// **anchored content must read the committed rects, never its own
    /// interpolator.** A host-side tween running at full rate inside a
    /// reduced-rate surface produces relative motion that looks worse than
    /// either rate alone.
    public struct CommitPacer
    {
        private readonly int _commitHz;
        private float _sinceCommit;

        /// `commitHz` of 0 or less commits once per frame.
        public CommitPacer(int commitHz)
        {
            _commitHz = commitHz;
            _sinceCommit = 0f;
        }

        /// Whether this frame carries a commit, and the time it covers.
        ///
        /// `dt` is the elapsed time since the previous COMMIT, not since the
        /// previous frame. Passing the frame delta while committing at a
        /// reduced rate would advance the scene at a fraction of real time.
        ///
        /// **The accumulator keeps its overshoot.** Resetting it to zero after
        /// a commit discards the remainder every cycle, so any rate that does
        /// not divide the frame rate runs slower than configured: at 60 Hz a
        /// requested 16 Hz then commits on a constant 4-frame interval, which
        /// is 15 Hz and is not the alternating cadence described above, and 25
        /// becomes 20. Subtracting the period is what makes the average come
        /// out at the rate that was asked for.
        public bool ShouldCommit(float frameDelta, out float dt)
        {
            if (_commitHz <= 0)
            {
                dt = frameDelta;
                return true;
            }

            _sinceCommit += frameDelta;
            var period = 1f / _commitHz;
            if (_sinceCommit < period)
            {
                dt = 0f;
                return false;
            }

            dt = _sinceCommit;
            _sinceCommit -= period;
            return true;
        }

        /// The nearest rate at or below `commitHz` that divides `refreshHz`.
        ///
        /// Returns `commitHz` unchanged when it already divides, and 1 when
        /// nothing else does. `refreshHz` of 0 or less, or a `commitHz` above
        /// it, returns `commitHz` — there is no reduced rate to advise.
        public static int NearestDivisor(int refreshHz, int commitHz)
        {
            if (refreshHz <= 0 || commitHz <= 0 || commitHz > refreshHz)
            {
                return commitHz;
            }

            for (var candidate = commitHz; candidate >= 1; candidate--)
            {
                if (refreshHz % candidate == 0)
                {
                    return candidate;
                }
            }

            return 1;
        }
    }
}
