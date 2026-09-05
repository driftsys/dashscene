//! Golden-image diff tooling (docs/design/architecture.md, docs/roadmap.md v0.1): compares a
//! freshly rendered PNG against the checked-in golden under
//! `goldens/images/`, pixel by pixel in unpremultiplied RGBA8888 (the
//! comparison-space decision is
//! `docs/decisions/golden-comparison-space.md`; `SkiaPainter::rgba_bytes`
//! reads back in the same space — change the two together).
//!
//! Workflow documentation: `goldens/README.md`.

use std::env;
use std::io::ErrorKind;
use std::path::Path;

use skia_safe::{AlphaType, ColorType, Data, ImageInfo, images};

/// Two published perceptual scales, and PSNR for comparability — the
/// calibration behind `dashpack`'s tolerance bands (issue #544).
///
/// Unconditional, unlike [`profile`]: these are image mathematics over plain
/// RGBA8 buffers and reach neither the packer nor the block decoder.
pub mod metric;
pub mod oracle;
/// Rendering a document under a quality profile — the Gfx QA profile preview.
///
/// Behind the `profile-preview` feature, which is on by default: it links the
/// packer and the block decoder into this harness, and a build that does not
/// want them (a trimmed consumer of the reference painter) turns it off. The
/// reference painter itself is unchanged either way and never links a block
/// decoder — the decode happens in the loader, before any byte reaches it, so
/// P2 holds (story #435).
#[cfg(feature = "profile-preview")]
pub mod profile;
pub mod render;

/// Compares `png_bytes` against the checked-in golden `{name}.png`,
/// requiring an exact pixel match. Use this for content that renders
/// bit-identically across machines — integer-aligned, un-antialiased
/// geometry (solid fills). For anti-aliased content (gradients, curves)
/// use [`assert_matches_golden_within`].
///
/// With `UPDATE_GOLDENS=1` in the environment, validates that
/// `png_bytes` decodes and writes it as the new golden instead of
/// comparing.
///
/// # Panics
///
/// Panics when the golden is missing (run with `UPDATE_GOLDENS=1` to
/// create it) or when the dimensions or any pixel differ — the rendered
/// image is then written next to the golden as `{name}.actual.png`.
/// Encoded-byte drift with identical pixels passes with a note on
/// stderr: a golden is a picture, not a container format.
pub fn assert_matches_golden(name: &str, png_bytes: &[u8]) {
    run_golden(name, png_bytes, Budget::Fraction(0.0));
}

/// The pass criterion for a tolerance-based golden: how many differing
/// pixels are still a pass.
///
/// The tolerance exists to absorb cross-machine anti-aliasing jitter —
/// skia's coverage rounding at a fractional edge flips a handful of
/// boundary pixels across CPU architectures. That jitter scales with the
/// scene's anti-aliased **edge count**, an absolute number, not with the
/// canvas area. The two forms make that choice explicit:
///
/// - `Fraction` — a fraction of the whole canvas. Right when the inked
///   content is a large share of the canvas (solid-fill scenes: the v0.3
///   paint families), where a real regression moves several percent of
///   the canvas and the edge jitter is a small fraction of it.
/// - `Pixels` — an absolute differing-pixel count. Right for sparse
///   content (text), where the inked ink is a small fraction of the
///   canvas: a canvas fraction wide enough to clear the edge jitter can
///   exceed the entire inked footprint, so a regression that erases the
///   text passes. See `docs/decisions/golden-comparison-space.md`.
pub enum Budget {
    Fraction(f64),
    Pixels(usize),
}

/// Compares `png_bytes` against the checked-in golden, allowing up to
/// `max_differing_fraction` of the canvas pixels to differ (0.0 = exact).
///
/// A canvas fraction is the right tolerance when the inked content is a
/// large share of the canvas. For sparse content (text), prefer
/// [`assert_matches_golden_max_pixels`]: a fraction wide enough to clear
/// cross-machine edge jitter can exceed the whole inked footprint, so a
/// text-erasing regression would pass. See
/// `docs/decisions/golden-comparison-space.md`.
///
/// # Panics
///
/// As [`assert_matches_golden`], except a pixel difference within the
/// tolerance passes with a note on stderr; only dimension mismatches or
/// a differing fraction above `max_differing_fraction` fail.
pub fn assert_matches_golden_within(name: &str, png_bytes: &[u8], max_differing_fraction: f64) {
    run_golden(name, png_bytes, Budget::Fraction(max_differing_fraction));
}

/// The cross-architecture pixel budget every calibrated-budget golden
/// carries, in differing pixels.
///
/// One constant rather than one number per scene, because what a budget
/// absorbs is cross-architecture rasterisation jitter — a property of the
/// renderer and the machine, not of the picture. Measured across six
/// independent CI runners on 2026-08-02, the residual for the seven
/// goldens in this class was 0, 0, 0, 1, 2, 3 and 4 pixels, with **no
/// variance between runners**, and it did not scale with scene size,
/// glyph count or ink: the densest scene measured 0 and a sparse Arabic
/// one measured 4. A per-scene budget would model something that is not
/// per-scene.
///
/// 32 is the `v03-paint` anchor, the measured residual of the paint scene
/// every earlier budget was extrapolated from. It leaves at least 8x over
/// the largest measured floor (4 px) and at least 15x under the weakest
/// sensitivity guard (484 px, `v07-text-lowering`), so it absorbs the
/// jitter while still failing on lost ink.
///
/// The seven budgets this replaced — 1200, 500, 500, 440, 400, 200, 200 —
/// were not seven calibrations. They were seven multipliers of this same
/// anchor, each sitting about 1.5x under its guard and 110x to 1200x over
/// its floor, which is why a regression moving a few glyphs by a pixel
/// passed silently on every one of them (story #671).
///
/// **If a scene's measured floor ever rises above about 4 px, re-derive
/// this constant — do not exempt the scene.** Exempting one is how seven
/// unexplained numbers came about the first time.
pub const CROSS_ARCH_BUDGET_PX: usize = 32;

/// Compares `png_bytes` against the checked-in golden, allowing up to
/// `max_differing_pixels` pixels to differ.
///
/// The absolute form for sparse content (text): the budget is sized to
/// the scene's anti-aliased edge count, not to the canvas, so it stays
/// below the inked footprint and a regression that erases or moves the
/// text exceeds it. See `docs/decisions/golden-comparison-space.md`.
///
/// # Panics
///
/// As [`assert_matches_golden`], except a difference within the budget
/// passes with a note on stderr; only dimension mismatches or a differing
/// count above `max_differing_pixels` fail.
pub fn assert_matches_golden_max_pixels(name: &str, png_bytes: &[u8], max_differing_pixels: usize) {
    run_golden(name, png_bytes, Budget::Pixels(max_differing_pixels));
}

fn run_golden(name: &str, png_bytes: &[u8], budget: Budget) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../images");
    match env::var_os("UPDATE_GOLDENS") {
        None => compare_against(&root, name, png_bytes, budget),
        Some(value) if value == "1" => {
            // Never commit bytes the comparison path could not read
            // back: a broken render must fail here, not at the next
            // clean-checkout test run.
            decode_rgba(png_bytes, "the rendered image");
            let path = root.join(format!("{name}.png"));
            std::fs::write(&path, png_bytes).expect("write golden");
            remove_stale_actual(&root, name);
            eprintln!("UPDATE_GOLDENS: wrote {}", path.display());
        }
        Some(other) => panic!(
            "UPDATE_GOLDENS={} is not recognized — set UPDATE_GOLDENS=1 \
             (regenerating overwrites reviewed goldens, so only the \
             documented value is accepted)",
            other.to_string_lossy()
        ),
    }
}

/// Extracts the RGBA bytes of one pixel from a tightly packed
/// RGBA8888 buffer — the readback format of `SkiaPainter::rgba_bytes`
/// and [`assert_matches_golden`]'s comparison space.
pub fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    rgba[offset..offset + 4].try_into().expect("pixel in range")
}

/// What comparing two same-sized, tightly packed RGBA8888 buffers found.
///
/// Carries three numbers rather than one because a differing fraction alone
/// passes a systematic difference confined to one region of the frame:
/// [`Comparison::bounds`] and [`Comparison::max_channel_delta`] are what see
/// it. A golden comparison has a reviewed image on one side and can rely on a
/// fraction; two hosts drawing the same scene through different painters
/// cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// Pixels compared.
    ///
    /// The frame, for every caller that establishes two equal extents first —
    /// which is both of the in-tree ones. When the two buffers differ in
    /// length this is the shorter of them, because that is what the walk
    /// covers and what [`Comparison::fraction`] is therefore a fraction of.
    pub total: usize,
    /// Pixels counted as differing: some channel differs by more than the
    /// threshold the comparison was taken at. At threshold 0, which is every
    /// golden, that is every pixel whose four bytes are not identical.
    pub differing: usize,
    /// The first differing pixel in row-major order, as `(x, y)`.
    pub first: Option<(i32, i32)>,
    /// The differing pixels' bounding box, as `(min_x, min_y, max_x, max_y)`.
    pub bounds: Option<(i32, i32, i32, i32)>,
    /// The largest absolute single-channel difference anywhere in the frame.
    pub max_channel_delta: u8,
}

impl Comparison {
    /// The differing pixels as a fraction of the frame.
    ///
    /// `0.0` for an empty frame. One caller can now produce one —
    /// [`compare_rgba`] answers a width of zero with `total: 0` rather than
    /// dividing by it — and otherwise no caller does. Dividing by zero to say
    /// so would be worse than answering.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.differing as f64 / self.total as f64
    }
}

/// Compares two tightly packed RGBA8888 buffers of the same width, counting a
/// pixel as differing only when some channel differs by more than `threshold`.
///
/// **`threshold` is 0 for a golden**, where one side is a reviewed image
/// produced by the same painter and any difference is a change. It is not 0
/// for two painters: measured on a Pixel 5 on 2026-08-29, the lean painter and
/// the Unity BRG painter drawing the same document at the same extent differ by
/// one to three levels per channel on **99.98%** of the frame — a systematic
/// rounding difference that is invisible and that swamps any count taken at 0.
///
/// The caller establishes that the two are the same size — `compare_against`
/// against the golden's dimensions, and any other caller after decoding. When
/// it does not, this reports over the shorter of the two rather than over the
/// longer, so the fraction is of what was actually compared. A width of zero
/// or less reports nothing rather than dividing by it.
pub fn compare_rgba(left: &[u8], right: &[u8], width: i32, threshold: u8) -> Comparison {
    // **A zero width answers rather than dividing by it.** This is `pub`, and
    // `compare_against` is only one of its callers — the other decodes two
    // device captures, where a zero extent is a decode that went wrong rather
    // than a caller that should have known better. Nothing can be located
    // without a width, so there is nothing to report.
    // **This is a fail-open and the type cannot make it anything else.**
    // `Comparison` is not a `Result`, so "nothing could be compared" and
    // "nothing differs" are the same value: `fraction()` is 0.0 and both
    // budgets pass. A review asked for a `debug_assert!` here to bound it, and
    // that is refused: every test in this workspace runs in a debug build, so
    // it would reintroduce for them exactly the panic issue #1393 asked to
    // remove, and leave the function behaving differently in the two profiles
    // — which is worse for one whose whole contract is that it answers.
    //
    // What bounds it instead: neither in-tree caller can reach it — both take
    // the width from a decoded PNG, and `compare_against` short-circuits on
    // equal buffers first — and a caller that wants a refusal has
    // `compare_pngs`, which returns `Result`.
    if width <= 0 {
        return Comparison {
            total: 0,
            differing: 0,
            first: None,
            bounds: None,
            max_channel_delta: 0,
        };
    }
    // **Counted from the walk, not from the left buffer.** The walk stops at
    // the shorter of the two, so taking the total from `left` reported a right
    // buffer half its length as at most 50 % differing and understated
    // `fraction()` by 2x — which passes a `Budget::Fraction` check silently.
    let mut total = 0usize;
    let mut differing = 0usize;
    let mut first = None;
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    let mut max_channel_delta = 0u8;

    for (i, (a, b)) in left.chunks_exact(4).zip(right.chunks_exact(4)).enumerate() {
        total += 1;
        let delta = a
            .iter()
            .zip(b.iter())
            .map(|(channel_a, channel_b)| channel_a.abs_diff(*channel_b))
            .max()
            .unwrap_or(0);
        // **The maximum is taken over every pixel, not only counted ones.** A
        // threshold that also hid the largest delta would report a frame as
        // matching and give no number saying how nearly.
        max_channel_delta = max_channel_delta.max(delta);
        if delta <= threshold {
            continue;
        }
        differing += 1;
        let (x, y) = (i as i32 % width, i as i32 / width);
        if first.is_none() {
            first = Some((x, y));
        }
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
        });
    }

    Comparison {
        total,
        differing,
        first,
        bounds,
        max_channel_delta,
    }
}

/// The comparison body, with the images root injected (unit tests use
/// a temporary root; [`assert_matches_golden`] passes the repository's
/// `goldens/images/`).
fn compare_against(root: &Path, name: &str, png_bytes: &[u8], budget: Budget) {
    let golden_path = root.join(format!("{name}.png"));
    let golden_bytes = match std::fs::read(&golden_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => panic!(
            "golden {} is missing — generate and commit it with UPDATE_GOLDENS=1",
            golden_path.display()
        ),
        Err(error) => panic!("golden {} is unreadable: {error}", golden_path.display()),
    };

    let (golden_size, golden_pixels) = decode_rgba(&golden_bytes, "the checked-in golden");
    let (actual_size, actual_pixels) = decode_rgba(png_bytes, "the rendered image");

    if golden_size != actual_size {
        let actual_path = write_actual(root, name, png_bytes);
        panic!(
            "golden {} is {}x{} but the render is {}x{} — actual: {}",
            golden_path.display(),
            golden_size.0,
            golden_size.1,
            actual_size.0,
            actual_size.1,
            actual_path.display()
        );
    }

    if golden_pixels == actual_pixels {
        if golden_bytes != png_bytes {
            eprintln!(
                "golden {name}: pixels identical, encoded bytes differ (encoding drift, not a rendering change)"
            );
        }
        remove_stale_actual(root, name);
        return;
    }

    let (width, _) = golden_size;
    // The walk is `compare_rgba`'s, which the parity harness also uses over two
    // device captures. A golden comparison needs only the count and the first
    // coordinate; it ignores the bounds and the channel delta, which exist for
    // the caller that has no reviewed image on either side.
    // Threshold 0: one side is a reviewed image from this same painter, so any
    // difference at all is a change. The tolerance a golden carries is a count
    // of differing pixels, not a per-channel band.
    let comparison = compare_rgba(&golden_pixels, &actual_pixels, width, 0);
    let total = comparison.total;
    let differing = comparison.differing;
    let first = comparison.first;

    let fraction = comparison.fraction();
    let (within_budget, limit) = match budget {
        Budget::Fraction(max) => (fraction <= max, format!("{:.3}% tolerance", max * 100.0)),
        Budget::Pixels(max) => (differing <= max, format!("{max} px budget")),
    };
    if within_budget {
        // Within tolerance: cross-machine AA edge jitter, not a
        // rendering change. Clear any stale failure artifact.
        eprintln!(
            "golden {name}: {differing}/{total} pixel(s) differ ({:.3}%, within {limit}) — \
             cross-machine anti-aliasing jitter, accepted",
            fraction * 100.0,
        );
        remove_stale_actual(root, name);
        return;
    }

    let actual_path = write_actual(root, name, png_bytes);
    let (x, y) = first.expect("at least one differing pixel");
    panic!(
        "golden {name}: {differing}/{total} pixel(s) differ ({:.3}%, over {limit}), \
         first at ({x}, {y}) — golden: {}, actual: {}",
        fraction * 100.0,
        golden_path.display(),
        actual_path.display()
    );
}

fn write_actual(root: &Path, name: &str, png_bytes: &[u8]) -> std::path::PathBuf {
    let path = root.join(format!("{name}.actual.png"));
    std::fs::write(&path, png_bytes).expect("write actual image");
    path
}

/// A passing (or freshly regenerated) golden removes the failure
/// artifact of an earlier run, so a stale actual image cannot be
/// mistaken for a current failure.
fn remove_stale_actual(root: &Path, name: &str) {
    let path = root.join(format!("{name}.actual.png"));
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != ErrorKind::NotFound
    {
        panic!("stale {} could not be removed: {error}", path.display());
    }
}

/// Decodes a PNG to `((width, height), unpremultiplied RGBA8888 rows)`.
/// `label` names the image in failure messages (golden vs render).
fn decode_rgba(png_bytes: &[u8], label: &str) -> ((i32, i32), Vec<u8>) {
    match try_decode_rgba(png_bytes, label) {
        Ok(decoded) => decoded,
        // The two messages are the golden path's and are pinned by this
        // module's `should_panic` tests: `try_decode_rgba` formats them and
        // this re-raises them unchanged.
        Err(message) => panic!("{message}"),
    }
}

/// [`decode_rgba`] without the panic, for a caller comparing two images
/// neither of which is a reviewed golden.
fn try_decode_rgba(png_bytes: &[u8], label: &str) -> Result<((i32, i32), Vec<u8>), String> {
    let data = Data::new_copy(png_bytes);
    let Some(image) = images::deferred_from_encoded_data(data, None) else {
        return Err(format!("{label} is not a decodable PNG"));
    };
    let (width, height) = (image.width(), image.height());
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * height as usize];
    let read = image.read_pixels(
        &info,
        &mut pixels,
        row_bytes,
        (0, 0),
        skia_safe::image::CachingHint::Disallow,
    );
    if !read {
        return Err(format!(
            "{label} has a readable header but its pixel data does not decode (truncated or corrupt)"
        ));
    }
    Ok(((width, height), pixels))
}

/// Decodes a PNG to unpremultiplied RGBA8888 for [`pixel`], or `None`.
///
/// The comparison path's own decode, exposed so that a caller looking at *why*
/// two frames differ reads the same bytes the count was taken over.
pub fn decode_for_sampling(png_bytes: &[u8]) -> Option<((i32, i32), Vec<u8>)> {
    try_decode_rgba(png_bytes, "the image").ok()
}

/// Compares two encoded PNGs.
///
/// `Err` when either buffer does not decode, or when the two are different
/// sizes — which is a refusal rather than a difference: two frames of
/// different extents answer no question about whether two painters agree.
///
/// The counterpart of `assert_matches_golden_within` for a caller with no
/// reviewed image on either side, such as the Android host-parity harness
/// comparing one capture from each host.
pub fn compare_pngs(left: &[u8], right: &[u8], threshold: u8) -> Result<Comparison, String> {
    let (left_size, left_pixels) = try_decode_rgba(left, "the left image")?;
    let (right_size, right_pixels) = try_decode_rgba(right, "the right image")?;
    if left_size != right_size {
        return Err(format!(
            "the left image is {}x{} but the right is {}x{} — two extents answer \
             no question about whether the two agree",
            left_size.0, left_size.1, right_size.0, right_size.1
        ));
    }
    Ok(compare_rgba(
        &left_pixels,
        &right_pixels,
        left_size.0,
        threshold,
    ))
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use skia_safe::{Color4f, surfaces};
    use tempfile::TempDir;

    use super::{Budget, compare_against, compare_pngs, compare_rgba};

    /// A 2×2 PNG: three pixels of `base`, the bottom-right pixel of
    /// `corner`. Encoded directly through skia rather than through
    /// `SkiaPainter`, so a painter regression cannot mask a tooling
    /// regression (deliberate test isolation).
    fn tiny_png(base: Color4f, corner: Color4f) -> Vec<u8> {
        let mut surface = surfaces::raster_n32_premul((2, 2)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(base);
        let mut paint = skia_safe::Paint::new(corner, None);
        paint.set_anti_alias(false);
        canvas.draw_rect(skia_safe::Rect::from_xywh(1.0, 1.0, 1.0, 1.0), &paint);
        surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .expect("PNG encode")
            .as_bytes()
            .to_vec()
    }

    fn temp_root() -> TempDir {
        tempfile::tempdir().expect("temp images root")
    }

    const RED: Color4f = Color4f {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    const BLUE: Color4f = Color4f {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    #[test]
    fn identical_pixels_pass_and_clear_a_stale_actual_image() {
        let root = temp_root();
        let png = tiny_png(RED, BLUE);
        std::fs::write(root.path().join("case.png"), &png).unwrap();
        std::fs::write(root.path().join("case.actual.png"), b"stale").unwrap();

        compare_against(root.path(), "case", &png, Budget::Fraction(0.0));

        assert!(
            !root.path().join("case.actual.png").exists(),
            "a passing run removes the stale failure artifact"
        );
    }

    #[test]
    fn differing_pixels_panic_with_a_count_and_write_the_actual_image() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), tiny_png(RED, BLUE)).unwrap();
        let actual = tiny_png(RED, RED);

        let result = catch_unwind(|| {
            compare_against(root.path(), "case", &actual, Budget::Fraction(0.0));
        });

        let message = *result
            .expect_err("differing pixels must panic")
            .downcast::<String>()
            .expect("panic message is a String");
        assert!(
            message.contains("1/4 pixel(s) differ") && message.contains("first at (1, 1)"),
            "unexpected report: {message}"
        );
        assert_eq!(
            std::fs::read(root.path().join("case.actual.png")).expect("actual image written"),
            actual
        );
    }

    #[test]
    fn a_dimension_mismatch_panics_and_writes_the_actual_image() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), tiny_png(RED, BLUE)).unwrap();
        let mut surface = surfaces::raster_n32_premul((3, 3)).expect("surface");
        surface.canvas().clear(RED);
        let actual = surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .expect("PNG encode")
            .as_bytes()
            .to_vec();

        let result = catch_unwind(|| {
            compare_against(root.path(), "case", &actual, Budget::Fraction(0.0));
        });

        let message = *result
            .expect_err("a dimension mismatch must panic")
            .downcast::<String>()
            .expect("panic message is a String");
        assert!(
            message.contains("is 2x2 but the render is 3x3"),
            "unexpected report: {message}"
        );
        assert!(root.path().join("case.actual.png").exists());
    }

    #[test]
    #[should_panic(expected = "UPDATE_GOLDENS")]
    fn a_missing_golden_names_the_update_workflow() {
        let root = temp_root();
        compare_against(
            root.path(),
            "never-created",
            &tiny_png(RED, BLUE),
            Budget::Fraction(0.0),
        );
    }

    #[test]
    #[should_panic(expected = "not a decodable PNG")]
    fn a_corrupt_golden_names_itself_rather_than_the_render() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), b"not a png").unwrap();
        compare_against(
            root.path(),
            "case",
            &tiny_png(RED, BLUE),
            Budget::Fraction(0.0),
        );
    }

    #[test]
    fn a_difference_within_tolerance_passes_and_clears_a_stale_actual() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), tiny_png(RED, BLUE)).unwrap();
        std::fs::write(root.path().join("case.actual.png"), b"stale").unwrap();
        // One of the 2x2 image's four pixels differs = 25%.
        let actual = tiny_png(RED, RED);

        compare_against(root.path(), "case", &actual, Budget::Fraction(0.30));

        assert!(
            !root.path().join("case.actual.png").exists(),
            "a within-tolerance run clears the stale artifact"
        );
    }

    #[test]
    fn a_difference_above_tolerance_still_fails() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), tiny_png(RED, BLUE)).unwrap();
        let actual = tiny_png(RED, RED); // 25% differ

        let result = catch_unwind(|| {
            compare_against(root.path(), "case", &actual, Budget::Fraction(0.10));
        });

        let message = *result
            .expect_err("above-tolerance difference must panic")
            .downcast::<String>()
            .expect("panic message is a String");
        assert!(
            message.contains("over 10.000% tolerance"),
            "unexpected report: {message}"
        );
    }

    #[test]
    fn an_absolute_pixel_budget_passes_at_or_below_and_fails_above() {
        let root = temp_root();
        std::fs::write(root.path().join("case.png"), tiny_png(RED, BLUE)).unwrap();
        // One of the four pixels differs.
        let actual = tiny_png(RED, RED);

        // A one-pixel budget accepts the one differing pixel.
        compare_against(root.path(), "case", &actual, Budget::Pixels(1));

        // A zero-pixel budget rejects it, and the message names the
        // absolute budget rather than a percentage tolerance.
        let result = catch_unwind(|| {
            compare_against(root.path(), "case", &actual, Budget::Pixels(0));
        });
        let message = *result
            .expect_err("over-budget difference must panic")
            .downcast::<String>()
            .expect("panic message is a String");
        assert!(
            message.contains("over 0 px budget") && message.contains("1/4 pixel(s) differ"),
            "unexpected report: {message}"
        );
    }

    #[test]
    fn two_identical_pngs_compare_equal() {
        let png = tiny_png(RED, RED);
        let c = compare_pngs(&png, &png, 0).expect("both decode and match in size");
        assert_eq!(c.differing, 0);
        assert_eq!(c.total, 4);
    }

    #[test]
    fn two_differing_pngs_report_the_pixel_that_differs() {
        let left = tiny_png(RED, RED);
        let right = tiny_png(RED, BLUE);
        let c = compare_pngs(&left, &right, 0).expect("both decode and match in size");
        assert_eq!(c.differing, 1);
        assert_eq!(c.first, Some((1, 1)));
        assert_eq!(c.bounds, Some((1, 1, 1, 1)));
        assert!(c.max_channel_delta > 0);
    }

    #[test]
    fn an_undecodable_image_is_an_error_rather_than_a_panic() {
        let png = tiny_png(RED, RED);
        let error = compare_pngs(b"not a png", &png, 0).expect_err("undecodable left");
        assert!(
            error.contains("not a decodable PNG"),
            "the error names what failed: {error:?}"
        );
        let error = compare_pngs(&png, b"not a png", 0).expect_err("undecodable right");
        assert!(
            error.contains("not a decodable PNG"),
            "the right image is checked too: {error:?}"
        );
    }

    /// Two extents are refused rather than compared: a capture of one host at
    /// a different extent than the other answers no question about whether the
    /// two painters agree, and reporting a huge differing fraction for it
    /// would look like a rendering difference.
    #[test]
    fn two_extents_are_refused_rather_than_compared() {
        let small = tiny_png(RED, BLUE);
        let mut surface = surfaces::raster_n32_premul((4, 4)).expect("surface");
        surface.canvas().clear(RED);
        let large = surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .expect("PNG encode")
            .as_bytes()
            .to_vec();

        let error = compare_pngs(&small, &large, 0).expect_err("different extents");
        assert!(
            error.contains("2x2") && error.contains("4x4"),
            "the refusal names both extents: {error:?}"
        );
    }

    /// A `width` x `width` transparent-black RGBA8888 buffer.
    fn blank(width: usize) -> Vec<u8> {
        vec![0u8; width * width * 4]
    }

    #[test]
    fn identical_buffers_compare_equal() {
        let a = blank(4);
        let c = compare_rgba(&a, &a, 4, 0);
        assert_eq!(c.differing, 0);
        assert_eq!(c.total, 16);
        assert_eq!(c.fraction(), 0.0);
        assert_eq!(c.first, None);
        assert_eq!(c.bounds, None);
        assert_eq!(c.max_channel_delta, 0);
    }

    #[test]
    fn one_differing_pixel_is_located_and_bounded() {
        let a = blank(4);
        let mut b = a.clone();
        // pixel (2, 1), green channel: row 1 of a 4-wide image, column 2.
        b[(4 + 2) * 4 + 1] = 9;
        let c = compare_rgba(&a, &b, 4, 0);
        assert_eq!(c.differing, 1);
        assert_eq!(c.first, Some((2, 1)));
        assert_eq!(c.bounds, Some((2, 1, 2, 1)));
        assert_eq!(c.max_channel_delta, 9);
    }

    /// The case a differing fraction alone cannot see: a small, dense,
    /// systematic difference confined to one region of an otherwise identical
    /// frame. The fraction reads as noise; the bounding box and the channel
    /// delta are what report it.
    ///
    /// This is why [`Comparison`] carries three numbers rather than one.
    #[test]
    fn a_region_shifted_image_is_reported_by_its_bounds_not_its_fraction() {
        let width = 100usize;
        let a = blank(width);
        let mut b = a.clone();
        for y in 10..20 {
            for x in 30..40 {
                b[(y * width + x) * 4] = 255;
            }
        }
        let c = compare_rgba(&a, &b, width as i32, 0);
        assert_eq!(c.differing, 100);
        assert!(
            c.fraction() < 0.02,
            "the differing fraction alone reads as noise: {}",
            c.fraction()
        );
        assert_eq!(c.bounds, Some((30, 10, 39, 19)));
        assert_eq!(c.max_channel_delta, 255);
        // The *first* differing pixel in row-major order, not the last.
        // `compare_against`'s failure message says "first at (x, y)" and
        // points a reader at it, so recording the last one would send them to
        // the wrong corner of the region. A single-pixel case cannot tell the
        // two apart; this one can.
        assert_eq!(c.first, Some((30, 10)));
    }

    /// `threshold` is the reason this function is public, and every test above
    /// passes 0 — which is the one value at which the parameter can be ignored
    /// without any of them noticing.
    ///
    /// Two mutations this kills, both of which survive the tests above:
    /// replacing the `delta <= threshold` skip with `delta == 0`, so the
    /// parameter does nothing and the Android parity pair reads 99.98 %
    /// differing; and taking `max_channel_delta` only over counted pixels
    /// rather than over every pixel, which is what that field's own comment
    /// says it does not do.
    #[test]
    fn a_threshold_excludes_smaller_deltas_and_still_reports_the_largest() {
        let a = blank(4);
        let mut b = a.clone();
        // Three differing pixels on row 0, with red deltas of 1, 2 and 3.
        b[0] = 1;
        b[4] = 2;
        b[8] = 3;

        let strict = compare_rgba(&a, &b, 4, 0);
        assert_eq!(strict.differing, 3, "every delta counts at threshold 0");

        let banded = compare_rgba(&a, &b, 4, 2);
        assert_eq!(
            banded.differing, 1,
            "only the delta of 3 exceeds a threshold of 2"
        );
        assert_eq!(
            banded.first,
            Some((2, 0)),
            "the located pixel is the one that exceeded the threshold"
        );
        assert_eq!(banded.bounds, Some((2, 0, 2, 0)));
        assert_eq!(
            banded.max_channel_delta, 3,
            "the maximum is taken over every pixel, not only counted ones"
        );
        assert_eq!(banded.total, 16, "the total is the frame, not the count");
    }

    /// The golden path is strict on purpose and nothing pinned it: every
    /// fixture above differs by a saturated 255, so raising
    /// `compare_against`'s threshold from 0 to 3 would leave the whole
    /// workspace's goldens green while every one-to-three-level regression
    /// became invisible.
    #[test]
    fn a_golden_differing_by_one_level_still_counts_as_differing() {
        let a = blank(4);
        let mut b = a.clone();
        b[0] = 1;
        let c = compare_rgba(&a, &b, 4, 0);
        assert_eq!(
            c.differing, 1,
            "a golden has one painter on both sides, so one level is a change"
        );
    }

    /// Issue #1393, first half. `compare_rgba` is `pub` and `compare_pngs` is
    /// only one of its callers, so a zero width has to answer rather than
    /// divide.
    #[test]
    fn a_zero_width_is_refused_rather_than_dividing_by_it() {
        let a = vec![0u8; 16];
        let mut b = a.clone();
        b[0] = 255;
        let c = compare_rgba(&a, &b, 0, 0);
        assert_eq!(c.differing, 0, "nothing can be located without a width");
        assert_eq!(c.total, 0);
        assert_eq!(c.first, None);
        assert_eq!(c.bounds, None);
        assert_eq!(c.fraction(), 0.0);
    }

    /// Issue #1393, second half. The walk stops at the shorter buffer and
    /// `total` was taken from the left one, so a right buffer half the left's
    /// length reported at most 50 % differing and a `fraction()` understated
    /// by 2x — which silently passes a `Budget::Fraction` check.
    #[test]
    fn a_short_buffer_does_not_understate_the_fraction() {
        let a = vec![0u8; 8 * 4];
        let b = vec![255u8; 4 * 4];
        let c = compare_rgba(&a, &b, 4, 0);
        assert_eq!(
            c.differing, 4,
            "four pixels were compared, and all four differ"
        );
        assert_eq!(
            c.total, 4,
            "the total is what was compared, not what the longer buffer holds"
        );
        assert!(
            (c.fraction() - 1.0).abs() < f64::EPSILON,
            "every compared pixel differs, so the fraction is 1.0, not 0.5: {}",
            c.fraction()
        );
    }
}
