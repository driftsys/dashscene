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

pub mod oracle;

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
    let total = golden_pixels.len() / 4;
    let mut differing = 0usize;
    let mut first = None;
    for (i, (golden, actual)) in golden_pixels
        .chunks_exact(4)
        .zip(actual_pixels.chunks_exact(4))
        .enumerate()
    {
        if golden != actual {
            differing += 1;
            if first.is_none() {
                first = Some((i as i32 % width, i as i32 / width));
            }
        }
    }

    let fraction = differing as f64 / total as f64;
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
    let data = Data::new_copy(png_bytes);
    let image = images::deferred_from_encoded_data(data, None)
        .unwrap_or_else(|| panic!("{label} is not a decodable PNG"));
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
    assert!(
        read,
        "{label} has a readable header but its pixel data does not decode (truncated or corrupt)"
    );
    ((width, height), pixels)
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use skia_safe::{Color4f, surfaces};
    use tempfile::TempDir;

    use super::{Budget, compare_against};

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
}
