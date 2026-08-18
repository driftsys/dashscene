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
//! # The one place the two now differ, and why
//!
//! This host reports **paint and present separately**; `demo/src/shell.rs`
//! reports one combined figure. The device measurement on 2026-08-17 is the
//! reason: at 1280x445 the showcase's simplest scene cost about 5 ms a frame and
//! **did not get cheaper when the surface lost 70% of its pixels**, which says
//! the cost is per-element rather than per-pixel — but a single wall-clock
//! number around `paint` *and* `present` cannot say whether that is this
//! project's own instance packing or the submit and swapchain path underneath
//! it. Those two have nothing in common as optimisation targets, so the
//! instrument stops averaging them together.
//!
//! **The two hosts' numbers are no longer directly comparable, and this line
//! used to claim they were.** `demo/src/shell.rs` prints `present mean` for a
//! quantity its own documentation defines as "the whole of the drawing: it is
//! `paint` plus whatever putting the frame on the window costs" — so putting a
//! device `present` beside a desktop `present` compares present-only against
//! paint-plus-present under one word. That is the error `shell.rs` warns about
//! directly: "Substituting any of them would put a different quantity under the
//! old one's name."
//!
//! So this host reports **`submit`** rather than `present`, and the desktop
//! host's `present` keeps its meaning. The two remain comparable by adding this
//! host's `paint` to its `submit`, which is what the desktop's single figure
//! spans — an addition a reader can do because both terms are printed, where
//! before they could only be conflated. Splitting the desktop host means
//! changing the winit frame loop that hands it the duration, which is a
//! different host's concern and not this measurement's.
//!
//! Two properties are preserved rather than reinvented. It reports **per
//! sample** rather than at exit, because the showcase advances through scenes
//! and a mean over all of them would describe none of them. And it names what
//! was being timed, because a sample taken across a scene change describes
//! neither side of it.

use std::time::Duration;

/// How many frames one report covers.
///
/// The sample size `docs/technotes/frame-budget.md` states for
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
    /// What packing the frame's instances cost, on the CPU, before anything is
    /// submitted. Mean and median only: this is a question about a magnitude,
    /// and the tail that matters for a deadline is `present`'s.
    pub paint_mean: f64,
    pub paint_p50: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    /// What the frame rate would be if nothing paced it.
    ///
    /// **Not the frame rate.** The loop is paced by vsync, so the observed rate
    /// is the display's until the work exceeds the budget; this is the rate the
    /// measured work alone would allow, which is what says how much headroom
    /// there is. All three measured terms are in it — tick, paint and present.
    pub fps_if_unpaced: f64,
}

impl Sample {
    /// The line.
    ///
    /// **No longer the form the desktop host prints**, and the module
    /// documentation carries why: that host's `present` spans paint as well, so
    /// one word would otherwise name two quantities.
    pub fn line(&self) -> String {
        format!(
            "{} over {} frames — tick {:.2} ms, paint mean {:.2} p50 {:.2}, \
             submit mean {:.2} p50 {:.2} p95 {:.2} max {:.2} ms \
             ({:.1} fps if unpaced)",
            self.scene,
            self.frames,
            self.tick_mean,
            self.paint_mean,
            self.paint_p50,
            self.mean,
            self.p50,
            self.p95,
            self.max,
            self.fps_if_unpaced,
        )
    }
}

/// Collects tick, paint and submit costs until a full sample is in hand.
#[derive(Default)]
pub struct Timing {
    tick: Vec<f64>,
    paint: Vec<f64>,
    /// **Submit, not the old combined `draw`.** Renamed with the split: this
    /// holds what `SurfaceRenderer::present` cost and no longer spans the
    /// packing above it.
    submit: Vec<f64>,
    /// What the sample in hand is a sample *of*. A sample is discarded when it
    /// changes part-way through, because a mean taken across that boundary
    /// describes neither side of it.
    of: Option<String>,
}

impl Timing {
    pub fn new() -> Self {
        Self {
            tick: Vec::with_capacity(TIMING_SAMPLE),
            paint: Vec::with_capacity(TIMING_SAMPLE),
            submit: Vec::with_capacity(TIMING_SAMPLE),
            of: None,
        }
    }

    /// Records one frame, and returns a report once a full sample is in hand.
    pub fn push(
        &mut self,
        scene: &str,
        tick: Duration,
        paint: Duration,
        present: Duration,
    ) -> Option<Sample> {
        if self.of.as_deref() != Some(scene) {
            self.tick.clear();
            self.paint.clear();
            self.submit.clear();
            self.of = Some(scene.to_owned());
        }
        self.tick.push(tick.as_secs_f64() * 1000.0);
        self.paint.push(paint.as_secs_f64() * 1000.0);
        self.submit.push(present.as_secs_f64() * 1000.0);
        self.report(scene)
    }

    fn report(&mut self, scene: &str) -> Option<Sample> {
        if self.submit.len() < TIMING_SAMPLE {
            return None;
        }
        let stat = |values: &mut Vec<f64>| {
            values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
            (mean, at(0.5), at(0.95), at(1.0))
        };
        let (tick_mean, ..) = stat(&mut self.tick);
        let (paint_mean, paint_p50, ..) = stat(&mut self.paint);
        let (mean, p50, p95, max) = stat(&mut self.submit);
        let frames = self.submit.len();
        self.tick.clear();
        self.paint.clear();
        self.submit.clear();
        Some(Sample {
            scene: scene.to_owned(),
            frames,
            tick_mean,
            paint_mean,
            paint_p50,
            mean,
            p50,
            p95,
            max,
            // Guarded: a sample of zero-cost frames would divide by zero, and a
            // device fast enough to make all **three** terms round to zero is a
            // device this would otherwise report `inf` for.
            fps_if_unpaced: if mean + paint_mean + tick_mean > 0.0 {
                1000.0 / (mean + paint_mean + tick_mean)
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
                timing.push("surfaces", ms(1), ms(2), ms(4)).is_none(),
                "reported early, at frame {frame}"
            );
        }
        let sample = timing
            .push("surfaces", ms(1), ms(2), ms(4))
            .expect("a full sample");
        assert_eq!(sample.frames, TIMING_SAMPLE);
        assert_eq!(sample.scene, "surfaces");
        // The next frame starts a fresh sample rather than reporting again.
        assert!(timing.push("surfaces", ms(1), ms(2), ms(4)).is_none());
    }

    /// The statistics are the ones the line claims. Uniform frames make every
    /// one of them the same number, which is the case where a wrong percentile
    /// index still looks plausible — so the second test varies them.
    #[test]
    fn uniform_frames_report_that_value_everywhere() {
        let mut timing = Timing::new();
        let mut last = None;
        for _ in 0..TIMING_SAMPLE {
            last = timing.push("surfaces", ms(2), ms(3), ms(8));
        }
        let sample = last.expect("a full sample");
        assert!((sample.tick_mean - 2.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.paint_mean - 3.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.paint_p50 - 3.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.mean - 8.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.p50 - 8.0).abs() < 1e-9, "{sample:?}");
        assert!((sample.max - 8.0).abs() < 1e-9, "{sample:?}");
        // 13 ms of work per frame — tick 2, paint 3, present 8 — is about
        // 76.9 fps if nothing paces it. **All three terms are in it**: a version
        // that forgot paint would report 100.
        assert!(
            (sample.fps_if_unpaced - 1000.0 / 13.0).abs() < 1e-6,
            "{sample:?}"
        );
    }

    /// **Paint and present are reported as themselves**, which is the whole
    /// reason the instrument carries two timers rather than their sum.
    ///
    /// Distinct values, and the assertion names which is which: a version that
    /// added them would report 12 for both, and one that swapped them would pass
    /// any test whose two terms were equal — which is why the fixtures here
    /// never are.
    #[test]
    fn paint_and_present_are_not_averaged_together() {
        let mut timing = Timing::new();
        let mut last = None;
        for _ in 0..TIMING_SAMPLE {
            last = timing.push("surfaces", ms(1), ms(3), ms(9));
        }
        let sample = last.expect("a full sample");
        assert!((sample.paint_mean - 3.0).abs() < 1e-9, "paint: {sample:?}");
        assert!((sample.mean - 9.0).abs() < 1e-9, "present: {sample:?}");
        // And the line says both, in that order.
        let line = sample.line();
        assert!(line.contains("paint mean 3.00"), "{line}");
        assert!(line.contains("submit mean 9.00"), "{line}");
    }

    /// A scene change clears the paint series with the others; a leak there
    /// would report the previous scene's packing against this one's present.
    #[test]
    fn a_scene_change_clears_the_paint_series_too() {
        let mut timing = Timing::new();
        for _ in 0..TIMING_SAMPLE - 1 {
            timing.push("surfaces", ms(1), ms(40), ms(1));
        }
        for _ in 0..TIMING_SAMPLE - 1 {
            assert!(timing.push("typography", ms(1), ms(2), ms(4)).is_none());
        }
        let sample = timing
            .push("typography", ms(1), ms(2), ms(4))
            .expect("a full sample of the second scene");
        assert!(
            (sample.paint_mean - 2.0).abs() < 1e-9,
            "the previous scene's paint leaked in: {sample:?}"
        );
    }

    /// One slow frame in a sample must reach `max` and must **not** move `p50`,
    /// which is the whole reason both are reported.
    #[test]
    fn a_single_slow_frame_moves_max_and_not_the_median() {
        let mut timing = Timing::new();
        let mut last = None;
        for frame in 0..TIMING_SAMPLE {
            let submit = if frame == 7 { ms(100) } else { ms(5) };
            last = timing.push("typography", ms(1), ms(2), submit);
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
            timing.push("surfaces", ms(1), ms(2), ms(50));
        }
        // One frame short, and now a different scene: the expensive frames must
        // not appear in what is reported for the new one.
        for _ in 0..TIMING_SAMPLE - 1 {
            assert!(timing.push("typography", ms(1), ms(2), ms(4)).is_none());
        }
        let sample = timing
            .push("typography", ms(1), ms(2), ms(4))
            .expect("a full sample of the second scene");
        assert_eq!(sample.scene, "typography");
        assert!(
            (sample.max - 4.0).abs() < 1e-9,
            "the previous scene's frames leaked in: {sample:?}"
        );
    }
}
