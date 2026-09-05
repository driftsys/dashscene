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
//! readings and says the same thing from the other side. Issue #1399 is the
//! ticket for the gap.
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

/// Prints the two images' pixels at `x,y`, as decimal RGBA.
fn sample(left: &[u8], right: &[u8], at: &str) -> ExitCode {
    let Some((x, y)) = at.split_once(',') else {
        eprintln!("compare-images: COMPARE_AT must be `x,y`, got {at:?}");
        return ExitCode::from(2);
    };
    let (Ok(x), Ok(y)) = (x.trim().parse::<usize>(), y.trim().parse::<usize>()) else {
        eprintln!("compare-images: COMPARE_AT must be two whole numbers, got {at:?}");
        return ExitCode::from(2);
    };

    // Decoded through the same path the comparison uses, so a difference seen
    // here is a difference the comparison saw.
    match goldens::decode_for_sampling(left).zip(goldens::decode_for_sampling(right)) {
        Some((((width, _), left_px), ((_, _), right_px))) => {
            let l = goldens::pixel(&left_px, width as usize, x, y);
            let r = goldens::pixel(&right_px, width as usize, x, y);
            println!("({x},{y}) left {l:?} right {r:?}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("compare-images: one of the images did not decode");
            ExitCode::from(2)
        }
    }
}
