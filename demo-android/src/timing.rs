//! Per-frame costs, in the same units and the same reported form the desktop
//! host uses (story #842, over story #586's instrument).
//!
//! # Why this is a second copy rather than a shared one
//!
//! Story #842 left "extract it or reimplement it" as its own call.
//! `demo/src/shell.rs`'s instrument is entangled with that host: it reads an
//! environment variable to arm itself, writes to `eprintln!`, and keys its
//! sample on `(scene, presenter)` because a key can swap the painter mid-run.
//! None of those is true here — an Android host has no environment to read, no
//! stderr anyone sees, and one painter — so sharing it would mean parameterising
//! three things to reuse the arithmetic.
//!
//! **What is shared is the shape, deliberately, and that is the part that
//! matters**: the same `TIMING_SAMPLE`, the same statistics, and the same line,
//! so a number from a device is read in the same units as a number from the
//! desktop and the two can be put side by side. If a third host wants this
//! again, that is the point at which extracting it pays.
//!
//! Two properties are preserved rather than reinvented. It reports **per
//! sample** rather than at exit, because the showcase advances through scenes
//! and a mean over all of them would describe none of them. And it names what
//! was being timed, because a sample taken across a scene change describes
//! neither side of it.

use std::time::Duration;

/// How many frames one report covers.
///
/// The sample size `docs/technotes/2026-07-31-v014-frame-budget.md` states for
/// its own measurement, and the one `demo/src/shell.rs` uses, so the three are
/// read in the same units.
pub const TIMING_SAMPLE: usize = 240;

/// One reported sample.
///
/// Returned rather than printed, so the arithmetic is testable without a device
/// and the platform half decides where a line goes — which on Android is
/// logcat, and is not `eprintln!`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub scene: String,
    pub frames: usize,
    pub tick_mean: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    /// What the frame rate would be if nothing paced it.
    ///
    /// **Not the frame rate.** The loop is paced by vsync, so the observed rate
    /// is the display's until the work exceeds the budget; this is the rate the
    /// measured work alone would allow, which is what says how much headroom
    /// there is.
    pub fps_if_unpaced: f64,
}

impl Sample {
    /// The line, in the form the desktop host prints.
    pub fn line(&self) -> String {
        format!(
            "{} over {} frames — tick {:.2} ms, draw mean {:.2} p50 {:.2} p95 {:.2} max {:.2} ms \
             ({:.1} fps if unpaced)",
            self.scene,
            self.frames,
            self.tick_mean,
            self.mean,
            self.p50,
            self.p95,
            self.max,
            self.fps_if_unpaced,
        )
    }
}

/// Collects tick and draw costs until a full sample is in hand.
#[derive(Default)]
pub struct Timing {
    tick: Vec<f64>,
    draw: Vec<f64>,
    /// What the sample in hand is a sample *of*. A sample is discarded when it
    /// changes part-way through, because a mean taken across that boundary
    /// describes neither side of it.
    of: Option<String>,
}

impl Timing {
    pub fn new() -> Self {
        Self {
            tick: Vec::with_capacity(TIMING_SAMPLE),
            draw: Vec::with_capacity(TIMING_SAMPLE),
            of: None,
        }
    }

    /// Records one frame, and returns a report once a full sample is in hand.
    pub fn push(&mut self, scene: &str, tick: Duration, draw: Duration) -> Option<Sample> {
        if self.of.as_deref() != Some(scene) {
            self.tick.clear();
            self.draw.clear();
            self.of = Some(scene.to_owned());
        }
        self.tick.push(tick.as_secs_f64() * 1000.0);
        self.draw.push(draw.as_secs_f64() * 1000.0);
        self.report(scene)
    }

    fn report(&mut self, scene: &str) -> Option<Sample> {
        if self.draw.len() < TIMING_SAMPLE {
            return None;
        }
        let stat = |values: &mut Vec<f64>| {
            values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
            (mean, at(0.5), at(0.95), at(1.0))
        };
        let (tick_mean, ..) = stat(&mut self.tick);
        let (mean, p50, p95, max) = stat(&mut self.draw);
        let frames = self.draw.len();
        self.tick.clear();
        self.draw.clear();
        Some(Sample {
            scene: scene.to_owned(),
            frames,
            tick_mean,
            mean,
            p50,
            p95,
            max,
            // Guarded: a sample of zero-cost frames would divide by zero, and a
            // device fast enough to make both terms round to zero is a device
            // this would otherwise report `inf` for.
            fps_if_unpaced: if mean + tick_mean > 0.0 {
                1000.0 / (mean + tick_mean)
            } else {
                f64::INFINITY
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_micros(value * 1000)
    }

    /// Nothing is reported until a full sample is in hand, and then exactly one
    /// report is.
    #[test]
    fn one_report_per_full_sample() {
        let mut timing = Timing::new();
        for frame in 0..TIMING_SAMPLE - 1 {
            assert!(
                timing.push("surfaces", ms(1), ms(4)).is_none(),
                "reported early, at frame {frame}"
            );
        }
        let sample = timing
            .push("surfaces", ms(1), ms(4))
            .expect("a full sample");
        assert_eq!(sample.frames, TIMING_SAMPLE);
        assert_eq!(sample.scene, "surfaces");
        // The next frame starts a fresh sample rather than reporting again.
        assert!(timing.push("surfaces", ms(1), ms(4)).is_none());
    }

    /// The statistics are the ones the line claims. Uniform frames make every
    /// one of them the same number, which is the case where a wrong percentile
    /// index still looks plausible — so the second test varies them.
    #[test]
    fn uniform_frames_report_that_value_everywhere() {
        let mut timing = Timing::new();
        let mut last = None;
        for _ in 0..TIMING_SAMPLE {
            last = timing.push("surfaces", ms(2), ms(8));
        }
        let sample = last.expect("a full sample");
        assert!((sample.tick_mean - 2.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.mean - 8.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.p50 - 8.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.max - 8.0).abs() < 1e-9, "{sample:?}");
        // 10 ms of work per frame is 100 fps if nothing paces it.
        assert!((sample.fps_if_unpaced - 100.0).abs() < 1e-6, "{sample:?}");
    }

    /// One slow frame in a sample must reach `max` and must **not** move `p50`,
    /// which is the whole reason both are reported.
    #[test]
    fn a_single_slow_frame_moves_max_and_not_the_median() {
        let mut timing = Timing::new();
        let mut last = None;
        for frame in 0..TIMING_SAMPLE {
            let draw = if frame == 7 { ms(100) } else { ms(5) };
            last = timing.push("typography", ms(1), draw);
        }
        let sample = last.expect("a full sample");
        assert!(
            (sample.p50 - 5.0).abs() < 1e-9,
            "the median moved: {sample:?}"
        );
        assert!(
            (sample.max - 100.0).abs() < 1e-9,
            "the max missed it: {sample:?}"
        );
        assert!(sample.mean > 5.0 && sample.mean < 6.0, "{sample:?}");
    }

    /// A scene change discards the part-sample rather than averaging across it.
    ///
    /// The showcase advances through scenes, so this is the ordinary case and
    /// not a corner: a mean taken across the boundary describes neither side.
    #[test]
    fn a_scene_change_starts_the_sample_again() {
        let mut timing = Timing::new();
        for _ in 0..TIMING_SAMPLE - 1 {
            timing.push("surfaces", ms(1), ms(50));
        }
        // One frame short, and now a different scene: the expensive frames must
        // not appear in what is reported for the new one.
        for _ in 0..TIMING_SAMPLE - 1 {
            assert!(timing.push("typography", ms(1), ms(4)).is_none());
        }
        let sample = timing
            .push("typography", ms(1), ms(4))
            .expect("a full sample of the second scene");
        assert_eq!(sample.scene, "typography");
        assert!(
            (sample.max - 4.0).abs() < 1e-9,
            "the previous scene's frames leaked in: {sample:?}"
        );
    }
}
