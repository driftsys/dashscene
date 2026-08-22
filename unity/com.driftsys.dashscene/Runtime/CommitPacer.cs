// Committing below the display rate, without drifting off it.
//
// This is the arithmetic half of a host's frame loop, and it is here rather
// than in `Samples~/FrameLoop/` for one reason: nothing compiles a sample.
// `unity/package-compat` and `unity/ffi-check` both glob `Runtime/**/*.cs`, and
// no CI job runs a Unity editor, so logic left in the sample is checked by
// nobody. It is pure — no `UnityEngine` reference — so it costs the package
// nothing to keep it where the gates can reach it.

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
    /// **A class, not a struct.** `ShouldCommit` mutates the accumulators
    /// through an instance method, so a value type stored in a `readonly` field
    /// — an entirely reasonable thing for a host to write — would be defensively
    /// copied at every call, the accumulator would reset each frame, and the
    /// pacer would silently never commit.
    public sealed class CommitPacer
    {
        private readonly int _commitHz;

        /// Drives the cadence. Keeps its overshoot across commits.
        private float _residual;

        /// Wall time since the previous commit. Reported, then zeroed.
        private float _wallSinceCommit;

        /// `commitHz` of 0 or less commits once per frame.
        public CommitPacer(int commitHz)
        {
            _commitHz = commitHz;
            _residual = 0f;
            _wallSinceCommit = 0f;
        }

        /// Whether this frame carries a commit, and the time it covers.
        ///
        /// `dt` is the elapsed time since the previous COMMIT, not since the
        /// previous frame. Passing the frame delta while committing at a
        /// reduced rate would advance the scene at a fraction of real time.
        ///
        /// **Two accumulators, because one cannot do both jobs.** The residual
        /// decides WHEN to commit and must keep its overshoot: resetting it
        /// discards the remainder every cycle, so a requested 16 Hz on a 60 Hz
        /// display commits on a constant 4-frame interval — 15 Hz, and not the
        /// alternating cadence above. The wall accumulator decides WHAT TIME is
        /// reported and must not keep it: reporting the residual counts the
        /// carried remainder twice, and the scene then advances 10% faster than
        /// real time at 16 Hz and 16.7% at 25 Hz. Both were measured.
        ///
        /// `sum(dt)` over a run equals the elapsed wall time, which is what
        /// `unity/ffi-check` asserts. A cadence check alone cannot see the
        /// second defect, and did not.
        public bool ShouldCommit(float frameDelta, out float dt)
        {
            if (_commitHz <= 0)
            {
                dt = frameDelta;
                return true;
            }

            _residual += frameDelta;
            _wallSinceCommit += frameDelta;

            var period = 1f / _commitHz;
            if (_residual < period)
            {
                dt = 0f;
                return false;
            }

            dt = _wallSinceCommit;
            _wallSinceCommit = 0f;
            _residual -= period;
            return true;
        }

        /// The nearest rate at or below `commitHz` that divides `refreshHz`.
        ///
        /// Returns `commitHz` unchanged when it already divides. `refreshHz` of
        /// 0 or less, a `commitHz` of 0 or less, or a `commitHz` above the
        /// refresh rate returns `commitHz` — there is no reduced rate to
        /// advise. The search always terminates at 1, which divides everything.
        public static int NearestDivisor(int refreshHz, int commitHz)
        {
            if (refreshHz <= 0 || commitHz <= 0 || commitHz > refreshHz)
            {
                return commitHz;
            }

            // Terminates at 1 without a fallback below the loop: every integer
            // divides by 1, and the guard above has already established
            // refreshHz > 0. A trailing `return 1` here would be unreachable.
            var candidate = commitHz;
            while (refreshHz % candidate != 0)
            {
                candidate--;
            }

            return candidate;
        }
    }
}
