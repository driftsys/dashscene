//! Golden-image diff tooling (DESIGN_1.md §8, §11 v0.1): compares a
//! freshly rendered PNG against the checked-in golden under
//! `goldens/images/`, pixel by pixel in unpremultiplied RGBA8888 (the
//! comparison-space decision is
//! `docs/decisions/golden-comparison-space.md`).
//!
//! Workflow documentation: `goldens/README.md`.

use std::env;
use std::path::Path;

use skia_safe::{AlphaType, ColorType, Data, ImageInfo, images};

/// Compares `png_bytes` against the checked-in golden `{name}.png`.
///
/// With `UPDATE_GOLDENS=1` in the environment, writes `png_bytes` as
/// the new golden instead of comparing.
///
/// # Panics
///
/// Panics when the golden is missing (run with `UPDATE_GOLDENS=1` to
/// create it) or when any pixel differs — the rendered image is then
/// written next to the golden as `{name}.actual.png`. Encoded-byte
/// drift with identical pixels passes with a note on stderr: a golden
/// is a picture, not a container format.
pub fn assert_matches_golden(name: &str, png_bytes: &[u8]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../images");
    if env::var_os("UPDATE_GOLDENS").is_some_and(|v| v == "1") {
        let path = root.join(format!("{name}.png"));
        std::fs::write(&path, png_bytes).expect("write golden");
        eprintln!("UPDATE_GOLDENS: wrote {}", path.display());
        return;
    }
    compare_against(&root, name, png_bytes);
}

/// The comparison body, with the images root injected (unit tests use
/// a temporary root; [`assert_matches_golden`] passes the repository's
/// `goldens/images/`).
fn compare_against(root: &Path, name: &str, png_bytes: &[u8]) {
    let golden_path = root.join(format!("{name}.png"));
    let Ok(golden_bytes) = std::fs::read(&golden_path) else {
        panic!(
            "golden {} is missing — generate and commit it with UPDATE_GOLDENS=1",
            golden_path.display()
        );
    };

    let (golden_size, golden_pixels) = decode_rgba(&golden_bytes);
    let (actual_size, actual_pixels) = decode_rgba(png_bytes);
    assert_eq!(
        golden_size,
        actual_size,
        "golden {} is {}x{} but the render is {}x{}",
        golden_path.display(),
        golden_size.0,
        golden_size.1,
        actual_size.0,
        actual_size.1
    );

    if golden_pixels == actual_pixels {
        if golden_bytes != png_bytes {
            eprintln!(
                "golden {name}: pixels identical, encoded bytes differ (encoding drift, not a rendering change)"
            );
        }
        return;
    }

    let (width, _) = golden_size;
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
    let actual_path = root.join(format!("{name}.actual.png"));
    std::fs::write(&actual_path, png_bytes).expect("write actual image");
    let (x, y) = first.expect("at least one differing pixel");
    panic!(
        "golden {name}: {differing} differing pixel(s), first at ({x}, {y}) — \
         golden: {}, actual: {}",
        golden_path.display(),
        actual_path.display()
    );
}

/// Decodes a PNG to `((width, height), unpremultiplied RGBA8888 rows)`.
fn decode_rgba(png_bytes: &[u8]) -> ((i32, i32), Vec<u8>) {
    let data = Data::new_copy(png_bytes);
    let image = images::deferred_from_encoded_data(data, None).expect("decodable PNG");
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
    assert!(read, "PNG decodes to RGBA8888");
    ((width, height), pixels)
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::PathBuf;

    use skia_safe::{Color4f, surfaces};

    use super::compare_against;

    /// A 2×2 PNG: three pixels of `base`, the bottom-right pixel of
    /// `corner`.
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

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("goldens-test-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp images root");
        root
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
    fn identical_pixels_pass() {
        let root = temp_root("identical");
        let png = tiny_png(RED, BLUE);
        std::fs::write(root.join("case.png"), &png).unwrap();

        compare_against(&root, "case", &png);
    }

    #[test]
    fn differing_pixels_panic_with_a_count_and_write_the_actual_image() {
        let root = temp_root("differing");
        std::fs::write(root.join("case.png"), tiny_png(RED, BLUE)).unwrap();
        let actual = tiny_png(RED, RED);

        let result = catch_unwind(AssertUnwindSafe(|| {
            compare_against(&root, "case", &actual);
        }));

        let message = *result
            .expect_err("differing pixels must panic")
            .downcast::<String>()
            .expect("panic message is a String");
        assert!(
            message.contains("1 differing pixel(s), first at (1, 1)"),
            "unexpected report: {message}"
        );
        assert_eq!(
            std::fs::read(root.join("case.actual.png")).expect("actual image written"),
            actual
        );
    }

    #[test]
    #[should_panic(expected = "UPDATE_GOLDENS")]
    fn a_missing_golden_names_the_update_workflow() {
        let root = temp_root("missing");
        compare_against(&root, "never-created", &tiny_png(RED, BLUE));
    }
}
