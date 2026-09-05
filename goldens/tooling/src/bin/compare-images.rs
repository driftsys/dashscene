//! Compares two PNGs and prints what differs, as JSON.
//!
//! How a shell harness reaches [`goldens::compare_pngs`]. A binary rather than
//! a test, because the two images are device captures that exist only during a
//! run and neither is a reviewed golden.
//!
//! **It has no caller in the tree yet**, and the harness that will call it —
//! `measure/android/host-parity.sh` — is issue #1329's remaining limb and does
//! not exist. The Android host-parity captures of 2026-08-29 were compared by
//! running this binary by hand; `docs/design/android-toolchain.md` records the
//! readings and says the same thing from the other side.
//!
//! **What holds part of the surface without one**, and what still holds none
//! of it. [`sample_pair`] has the tests at the foot of this file, and
//! `compare_rgba` and `compare_pngs` have `goldens`' own — `compare_rgba`
//! including a case at a non-zero threshold, which is the parameter this
//! binary's whole reason for existing turns on. **`main` has none**: its
//! argument arity, the `COMPARE_THRESHOLD` fallback described below, the JSON
//! keys and the exit codes are a contract with no test behind it, and a
//! harness reading this file is relying on prose.
//!
//! Issue #1399 is the ticket for the missing caller, and issue #1392 is what
//! the absence of one cost here: the right image was indexed with the left
//! image's width, in the diagnostic path a mismatched pair is most likely to
//! reach.
//!
//! ```text
//! compare-images lean.png unity.png
//! {"differing":1234,"total":2527200,"fraction":0.000488,
//!  "bounds":[30,10,39,19],"max_channel_delta":255,"threshold":0}
//! ```
//!
//! `threshold` is echoed back because it is read from `COMPARE_THRESHOLD` and
//! an unparseable value falls back to 0 — which reads 99.98 % differing on two
//! painters, so a caller needs the value in the output rather than in its own
//! assumption.
//!
//! Exit 0 when the two decode and share an extent, whatever they show — a
//! difference is a result, not a failure. Exit 2 when they cannot be compared
//! at all, with the reason on stderr; the caller decides what a difference
//! means, and this does not carry a tolerance.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [left, right] = args.as_slice() else {
        eprintln!("usage: compare-images <left.png> <right.png>");
        return ExitCode::from(2);
    };

    let read = |path: &str| match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            eprintln!("compare-images: {path}: {error}");
            None
        }
    };
    let (Some(left_bytes), Some(right_bytes)) = (read(left), read(right)) else {
        return ExitCode::from(2);
    };

    // `--at x,y` prints the two pixels at one coordinate instead of comparing.
    // A whole-frame count says how much differs and never what: two frames that
    // look identical and differ in 99.98% of their pixels are a systematic
    // per-pixel difference, and the only way to see which is to read one.
    if let Some(at) = std::env::var_os("COMPARE_AT") {
        return sample(&left_bytes, &right_bytes, &at.to_string_lossy());
    }

    let threshold: u8 = std::env::var("COMPARE_THRESHOLD")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    match goldens::compare_pngs(&left_bytes, &right_bytes, threshold) {
        Ok(comparison) => {
            let bounds = match comparison.bounds {
                Some((min_x, min_y, max_x, max_y)) => {
                    format!("[{min_x},{min_y},{max_x},{max_y}]")
                }
                None => "null".to_owned(),
            };
            println!(
                "{{\"differing\":{},\"total\":{},\"fraction\":{:.6},\"bounds\":{},\"max_channel_delta\":{},\"threshold\":{}}}",
                comparison.differing,
                comparison.total,
                comparison.fraction(),
                bounds,
                comparison.max_channel_delta,
                threshold
            );
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("compare-images: {why}");
            ExitCode::from(2)
        }
    }
}

/// The two images' pixels at `x,y` as decimal RGBA, or why they could not be
/// sampled.
///
/// **The refusals `compare_pngs` makes, this makes too**, including naming
/// which of the two failed to decode. It used the LEFT image's width to index
/// the RIGHT one and skipped the extent check altogether, because
/// `COMPARE_AT` returns before `compare_pngs` ever runs —
/// so on a mismatched pair the right sample came from a different row, and
/// near the end of the buffer `goldens::pixel` panicked on the slice index —
/// the range panic, before its own `expect("pixel in range")` message could be
/// produced — for exit 101, where every other failure here is exit 2 with a
/// reason.
fn sample_pair(left: &[u8], right: &[u8], at: &str) -> Result<String, String> {
    let Some((x, y)) = at.split_once(',') else {
        return Err(format!("COMPARE_AT must be `x,y`, got {at:?}"));
    };
    let (Ok(x), Ok(y)) = (x.trim().parse::<usize>(), y.trim().parse::<usize>()) else {
        return Err(format!("COMPARE_AT must be two whole numbers, got {at:?}"));
    };

    // Decoded through the same path the comparison uses, so a difference seen
    // here is a difference the comparison saw.
    let (Some(((left_w, left_h), left_px)), Some(((right_w, right_h), right_px))) = (
        goldens::decode_for_sampling(left),
        goldens::decode_for_sampling(right),
    ) else {
        // Named, as `compare_pngs` names it: "one of them" sends a reader to
        // check both files.
        let which = if goldens::decode_for_sampling(left).is_none() {
            "left"
        } else {
            "right"
        };
        return Err(format!("the {which} image is not a decodable PNG"));
    };
    if (left_w, left_h) != (right_w, right_h) {
        return Err(format!(
            "the two images are {left_w}x{left_h} and {right_w}x{right_h}; \
             one coordinate names two different pixels, so there is nothing \
             to compare at it"
        ));
    }
    // `try_decode_rgba` allocates `width * 4 * height`, so a negative extent
    // would have failed there; these are non-negative by construction.
    let (width, height) = (
        left_w.unsigned_abs() as usize,
        left_h.unsigned_abs() as usize,
    );
    if x >= width || y >= height {
        return Err(format!("({x},{y}) is outside {width}x{height}"));
    }

    let l = goldens::pixel(&left_px, width, x, y);
    let r = goldens::pixel(&right_px, width, x, y);
    Ok(format!("({x},{y}) left {l:?} right {r:?}"))
}

/// Prints [`sample_pair`], or its reason on stderr.
fn sample(left: &[u8], right: &[u8], at: &str) -> ExitCode {
    match sample_pair(left, right, at) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("compare-images: {why}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sample_pair;

    /// A solid PNG of the given extent, so two of them can differ in size.
    fn png(width: i32, height: i32, colour: skia_safe::Color4f) -> Vec<u8> {
        let mut surface = skia_safe::surfaces::raster_n32_premul((width, height)).expect("surface");
        surface.canvas().clear(colour);
        surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .expect("PNG encode")
            .as_bytes()
            .to_vec()
    }

    /// Issue #1392. Two extents is the case the sampling path assumed away
    /// while the comparison path refused it, and the two have to agree —
    /// otherwise `COMPARE_AT`, which a reader reaches precisely when a
    /// comparison looks wrong, answers where the comparison would not.
    ///
    /// Every recorded use of this tool has been at one extent: the six
    /// 2026-08-29 Android captures were all 2340x1080, which is what made them
    /// comparable at all. The mismatch is the case a reader hits when
    /// something has already gone wrong, not the ordinary one.
    #[test]
    fn two_extents_are_refused_rather_than_sampled_with_one_width() {
        let left = png(8, 8, skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0));
        let right = png(4, 8, skia_safe::Color4f::new(1.0, 0.0, 0.0, 1.0));
        let why = sample_pair(&left, &right, "1,1").expect_err("two extents are refused");
        assert!(why.contains("8x8") && why.contains("4x8"), "{why}");
    }

    /// The panic this replaces: `goldens::pixel` slices `rgba[o..o + 4]`, so an
    /// out-of-range coordinate aborted with exit 101 and a backtrace, where the
    /// module header promises exit 2 with a reason.
    #[test]
    fn a_coordinate_outside_the_extent_is_a_reason_and_not_a_panic() {
        let image = png(4, 4, skia_safe::Color4f::new(0.0, 1.0, 0.0, 1.0));
        let why = sample_pair(&image, &image, "5000,5000").expect_err("refused");
        assert!(why.contains("outside 4x4"), "{why}");
    }

    /// A PNG whose every pixel differs from every other, so a sample can only
    /// be right for one coordinate in one image.
    ///
    /// **Two solid fills cannot pin this**, which a first version of this test
    /// used: reading the right image out of the left buffer, which is issue
    /// #1392 itself, still printed the right colour, and so did any `x`/`y`
    /// swap or offset slip.
    fn ramp(width: i32, height: i32, tint: u8) -> Vec<u8> {
        let mut surface = skia_safe::surfaces::raster_n32_premul((width, height)).expect("surface");
        let canvas = surface.canvas();
        for y in 0..height {
            for x in 0..width {
                let colour = skia_safe::Color4f::new(
                    f32::from(u8::try_from(x).expect("small")) / 255.0,
                    f32::from(u8::try_from(y).expect("small")) / 255.0,
                    f32::from(tint) / 255.0,
                    1.0,
                );
                let mut paint = skia_safe::Paint::new(colour, None);
                paint.set_anti_alias(false);
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(x as f32, y as f32, 1.0, 1.0),
                    &paint,
                );
            }
        }
        surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, None)
            .expect("PNG encode")
            .as_bytes()
            .to_vec()
    }

    /// The successful path, pinned by value rather than by label.
    ///
    /// Kills: sampling the right image out of the left buffer (issue #1392),
    /// and transposing `x` and `y`.
    #[test]
    fn each_image_is_sampled_at_the_asked_coordinate_from_its_own_bytes() {
        let left = ramp(4, 4, 0);
        let right = ramp(4, 4, 255);
        let line = sample_pair(&left, &right, "2,3").expect("both decode and agree");
        assert_eq!(
            line, "(2,3) left [2, 3, 0, 255] right [2, 3, 255, 255]",
            "the red channel is x, the green is y and the blue tells the two \
             images apart, so this string is wrong under any of the three \
             slips it exists to catch"
        );
    }

    #[test]
    fn a_malformed_coordinate_says_so() {
        let image = png(2, 2, skia_safe::Color4f::new(0.0, 0.0, 0.0, 1.0));
        assert!(sample_pair(&image, &image, "3").is_err());
        assert!(sample_pair(&image, &image, "a,b").is_err());
        assert!(sample_pair(&image, &image, "-1,0").is_err());
    }
}
