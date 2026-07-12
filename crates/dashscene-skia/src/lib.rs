//! Skia reference painter — v0 native painter, reference forever (DESIGN_1.md §8.1).
//!
//! CPU raster only: deterministic, bit-exact output — the golden
//! generator (§8). One [`Painter`] implementation over `skia-safe`;
//! v0.1 vocabulary (solid fills, fill-less entries). The v0.3
//! vocabulary this painter cannot draw yet panics by name — story #14
//! implements it (P4: never a silent drop).

use dashpaint::{PaintKind, PaintTable, Painter, RectEntry};
use skia_safe::{AlphaType, Color4f, ColorType, EncodedImageFormat, ImageInfo, Rect, surfaces};

/// The reference painter: draws boundary-B input onto a CPU raster
/// surface (N32 premultiplied).
pub struct SkiaPainter {
    surface: skia_safe::Surface,
}

impl SkiaPainter {
    /// A CPU raster surface of the given pixel size.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not positive.
    pub fn new(width: i32, height: i32) -> Self {
        assert!(
            width > 0 && height > 0,
            "surface dimensions must be positive, got {width}x{height}"
        );
        let surface =
            surfaces::raster_n32_premul((width, height)).expect("raster surface allocation");
        Self { surface }
    }

    /// The current surface contents, PNG-encoded.
    pub fn png_bytes(&mut self) -> Vec<u8> {
        let image = self.surface.image_snapshot();
        let data = image
            .encode(None, EncodedImageFormat::PNG, None)
            .expect("CPU raster images PNG-encode");
        data.as_bytes().to_vec()
    }

    /// The current surface contents as tightly packed RGBA8888 rows
    /// (unpremultiplied) — test and golden-tooling readback.
    pub fn rgba_bytes(&mut self) -> Vec<u8> {
        let width = self.surface.width();
        let height = self.surface.height();
        let info = ImageInfo::new(
            (width, height),
            ColorType::RGBA8888,
            AlphaType::Unpremul,
            None,
        );
        let row_bytes = width as usize * 4;
        let mut pixels = vec![0u8; row_bytes * height as usize];
        let read = self
            .surface
            .read_pixels(&info, &mut pixels, row_bytes, (0, 0));
        assert!(read, "CPU raster surfaces read back");
        pixels
    }
}

impl Painter for SkiaPainter {
    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable) {
        let canvas = self.surface.canvas();
        canvas.clear(Color4f::new(0.0, 0.0, 0.0, 0.0));
        for rect in rects {
            let entry = paints.resolve(rect.paint);
            if entry.stroke.is_some()
                || entry.clip
                || entry.corners != dashpaint::CornerRadii::default()
            {
                unimplemented!("strokes, clip, and corner radii are painted from story #14 onward");
            }
            match &entry.fill {
                // A fill-less entry draws nothing (a layout-only node).
                None => {}
                Some(PaintKind::Solid { color }) => {
                    // Anti-aliasing off: axis-aligned rects need no
                    // coverage math, and goldens want bit-exact edges.
                    let mut paint = skia_safe::Paint::new(
                        Color4f::new(color.r, color.g, color.b, color.a),
                        None,
                    );
                    paint.set_anti_alias(false);
                    canvas.draw_rect(Rect::from_xywh(rect.x, rect.y, rect.w, rect.h), &paint);
                }
                Some(PaintKind::Gradient(_)) | Some(PaintKind::Image { .. }) => {
                    unimplemented!("gradient and image fills are painted from story #14 onward");
                }
            }
        }
    }
}
