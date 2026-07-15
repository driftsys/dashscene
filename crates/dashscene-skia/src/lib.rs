//! Skia reference painter — v0 native painter, reference forever (docs/design/architecture.md).
//!
//! CPU raster only: deterministic, bit-exact output — the golden
//! generator (§8). One [`Painter`] implementation over `skia-safe`,
//! covering the v0.3 vocabulary: solid fills, the four gradient kinds
//! (diamond via SkSL — not a Skia primitive), image fills with scale
//! modes, stroke align via geometry expansion (Skia strokes are
//! center-only), rounded corners. Anti-aliasing is on for every draw
//! (`docs/decisions/reference-painter-antialiasing.md`): deterministic
//! for the pinned skia version, and a no-op on integer-aligned
//! axis-aligned edges. Subtree clipping arrives already resolved — each
//! rect carries the clip region `dashscene-core` computed from its
//! clipping ancestors at commit (issue #97), and the painter only
//! intersects it.

use dashpaint::{
    ClipTable, CornerRadii, Gradient, GradientKind, ImageAsset, ImageTable, MAX_GRADIENT_STOPS,
    PaintKind, PaintTable, Painter, RectEntry, ScaleMode, Stroke, StrokeAlign,
};
use skia_safe::{
    AlphaType, Canvas, ClipOp, Color4f, ColorType, Data, EncodedImageFormat, ImageInfo, Matrix,
    Point, RRect, Rect, RuntimeEffect, SamplingOptions, Shader, TileMode, gradient_shader, images,
    surfaces,
};

/// How a painter treats the advisory dirty set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DirtyMode {
    /// Redraw every rect from the caller's table. Ignores `dirty`, and is
    /// always correct — the reference behavior, and what the golden images
    /// are rendered with.
    #[default]
    Full,
    /// Model R-T4: keep a persistent copy of the rect table, refresh only
    /// the entries `dirty` names, and redraw every quad from that copy.
    /// This simulates a product painter's instance buffer, so a dirty set
    /// that omits a changed rect leaves a stale entry and renders a stale
    /// pixel — which is what makes the two modes a differential test of the
    /// dirty set (`goldens/tooling/tests/dirty_oracle.rs`).
    Retained,
}

/// The reference painter: draws boundary-B input onto a CPU raster
/// surface (N32 premultiplied).
pub struct SkiaPainter {
    surface: skia_safe::Surface,
    mode: DirtyMode,
    /// The simulated instance buffer. Empty in `Full` mode.
    retained: Vec<RectEntry>,
}

impl SkiaPainter {
    /// A CPU raster surface of the given pixel size, in [`DirtyMode::Full`].
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not positive.
    pub fn new(width: i32, height: i32) -> Self {
        Self::with_mode(width, height, DirtyMode::Full)
    }

    /// A CPU raster surface of the given pixel size, in `mode`.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not positive.
    pub fn with_mode(width: i32, height: i32, mode: DirtyMode) -> Self {
        assert!(
            width > 0 && height > 0,
            "surface dimensions must be positive, got {width}x{height}"
        );
        let surface =
            surfaces::raster_n32_premul((width, height)).expect("raster surface allocation");
        Self {
            surface,
            mode,
            retained: Vec::new(),
        }
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
    ///
    /// Opaque colors round-trip byte-exact. A semi-transparent color is
    /// stored premultiplied (8-bit quantized) and divided back out here,
    /// which can shift a channel by one code point — deterministic for a
    /// fixed skia version, but not equal to direct quantization of the
    /// authored color. The golden comparison space is
    /// `docs/decisions/golden-comparison-space.md`.
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
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        dirty: Option<&[u32]>,
    ) {
        // Refresh the simulated instance buffer, then draw from it. A full
        // refresh when the caller has no dirty set, or when the node count
        // changed (every index is new, so the whole buffer re-uploads —
        // the first-frame and structural-change path).
        if self.mode == DirtyMode::Retained {
            match dirty {
                Some(indices) if self.retained.len() == rects.len() => {
                    for &i in indices {
                        let i = i as usize;
                        self.retained[i] = rects[i];
                    }
                }
                _ => {
                    self.retained.clear();
                    self.retained.extend_from_slice(rects);
                }
            }
        }

        // Disjoint field borrows: `retained` is read while `surface` is
        // borrowed mutably.
        let source: &[RectEntry] = match self.mode {
            DirtyMode::Full => rects,
            DirtyMode::Retained => &self.retained,
        };

        let canvas = self.surface.canvas();
        canvas.clear(skia_safe::colors::TRANSPARENT);
        for rect in source {
            let entry = paints.resolve(rect.paint);
            let region = clips.resolve(rect.clip);
            // The region is already ancestor-resolved (core, at commit —
            // issue #97): intersect its boxes and draw. Which node each
            // box came from is not the painter's business (P2).
            let clipped = !region.is_unclipped();
            if clipped {
                canvas.save();
                for clip_box in region.boxes() {
                    let rrect = rrect_of(
                        clip_box.x,
                        clip_box.y,
                        clip_box.w,
                        clip_box.h,
                        &clip_box.corners,
                    );
                    canvas.clip_rrect(rrect, ClipOp::Intersect, true);
                }
            }
            let rrect = rounded_box(rect, &entry.corners);
            match &entry.fill {
                // A fill-less entry draws nothing (a layout-only node).
                None => {}
                Some(PaintKind::Solid { color }) => {
                    let mut paint = solid_paint(*color);
                    paint.set_anti_alias(true);
                    canvas.draw_rrect(rrect, &paint);
                }
                Some(PaintKind::Gradient(gradient)) => {
                    let mut paint = gradient_paint(gradient, rect);
                    paint.set_anti_alias(true);
                    canvas.draw_rrect(rrect, &paint);
                }
                Some(PaintKind::Image {
                    image,
                    scale_mode,
                    transform,
                    tile_scale,
                }) => {
                    draw_image_fill(
                        canvas,
                        &rrect,
                        rect,
                        images.resolve(*image),
                        *scale_mode,
                        transform.as_ref(),
                        *tile_scale,
                    );
                }
            }
            if let Some(stroke) = &entry.stroke {
                draw_stroke(canvas, &rrect, stroke);
            }
            if clipped {
                canvas.restore();
            }
        }
    }
}

fn solid_paint(color: dashpaint::Color) -> skia_safe::Paint {
    skia_safe::Paint::new(to_color4f(color), None)
}

fn to_color4f(color: dashpaint::Color) -> Color4f {
    Color4f::new(color.r, color.g, color.b, color.a)
}

/// The entry's box as a rounded rect (sharp when all radii are zero —
/// `CornerRadii::default()`); skia clamps oversized radii.
fn rounded_box(rect: &RectEntry, corners: &CornerRadii) -> RRect {
    rrect_of(rect.x, rect.y, rect.w, rect.h, corners)
}

/// A box with per-corner radii as a skia rounded rect — the entry's own
/// box (`rounded_box`) and a resolved clip box share this shaping.
fn rrect_of(x: f32, y: f32, w: f32, h: f32, corners: &CornerRadii) -> RRect {
    let bounds = Rect::from_xywh(x, y, w, h);
    if *corners == CornerRadii::default() {
        return RRect::new_rect(bounds);
    }
    let radii = [
        Point::new(corners.top_left, corners.top_left),
        Point::new(corners.top_right, corners.top_right),
        Point::new(corners.bottom_right, corners.bottom_right),
        Point::new(corners.bottom_left, corners.bottom_left),
    ];
    RRect::new_rect_radii(bounds, &radii)
}

/// The affine frame mapping gradient unit space into device space:
/// (0,0) → the origin handle, (1,0) → the primary handle, (0,1) → the
/// secondary handle, all in the entry's box (handles are normalized to
/// it).
fn gradient_frame(gradient: &Gradient, rect: &RectEntry) -> Matrix {
    let dev = |v: dashpaint::Vec2| (rect.x + v.x * rect.w, rect.y + v.y * rect.h);
    let (ox, oy) = dev(gradient.handle_origin);
    let (px, py) = dev(gradient.handle_primary);
    let (sx, sy) = dev(gradient.handle_secondary);
    Matrix::new_all(px - ox, sx - ox, ox, py - oy, sy - oy, oy, 0.0, 0.0, 1.0)
}

/// Builds the skia paint for a gradient fill. A degenerate frame (no
/// area) falls back to the first stop's color — deterministic; the
/// validator owns rejecting it upstream (P4).
fn gradient_paint(gradient: &Gradient, rect: &RectEntry) -> skia_safe::Paint {
    assert!(
        gradient.stops.len() <= MAX_GRADIENT_STOPS,
        "gradient stop budget exceeded: {} stops, budget {MAX_GRADIENT_STOPS} \
         (validated upstream once profiles land, P4)",
        gradient.stops.len()
    );
    let first_stop = gradient
        .stops
        .first()
        .expect("a gradient carries at least one stop (the schema requires stops)");

    let frame = gradient_frame(gradient, rect);
    if frame.invert().is_none() {
        return solid_paint(first_stop.color);
    }

    let colors: Vec<Color4f> = gradient.stops.iter().map(|s| to_color4f(s.color)).collect();
    let positions: Vec<f32> = gradient.stops.iter().map(|s| s.offset).collect();

    let shader = match gradient.kind {
        GradientKind::Linear => gradient_shader::linear(
            (Point::new(0.0, 0.0), Point::new(1.0, 0.0)),
            &colors[..],
            Some(&positions[..]),
            TileMode::Clamp,
            None,
            Some(&frame),
        ),
        GradientKind::Radial => gradient_shader::radial(
            Point::new(0.0, 0.0),
            1.0,
            &colors[..],
            Some(&positions[..]),
            TileMode::Clamp,
            None,
            Some(&frame),
        ),
        GradientKind::Angular => gradient_shader::sweep(
            Point::new(0.0, 0.0),
            &colors[..],
            Some(&positions[..]),
            TileMode::Clamp,
            None,
            None,
            Some(&frame),
        ),
        GradientKind::Diamond => diamond_shader(&colors, &positions, &frame),
    };

    match shader {
        Some(shader) => {
            let mut paint = skia_safe::Paint::default();
            paint.set_shader(shader);
            paint
        }
        // Skia refused gradient geometry the frame check did not catch
        // — same deterministic fallback.
        None => solid_paint(first_stop.color),
    }
}

/// Diamond gradient: not a Skia primitive (docs/technotes/rendering-and-painters.md). An SkSL
/// shader computes t = |x| + |y| in gradient unit space and samples a
/// 1D ramp child — a linear gradient along x — so the stop machinery
/// stays Skia's.
fn diamond_shader(colors: &[Color4f], positions: &[f32], frame: &Matrix) -> Option<Shader> {
    const SKSL: &str = r"
        uniform shader ramp;
        half4 main(float2 p) {
            float t = clamp(abs(p.x) + abs(p.y), 0.0, 1.0);
            return ramp.eval(float2(t, 0.5));
        }
    ";
    let effect = RuntimeEffect::make_for_shader(SKSL, None).expect("diamond SkSL compiles");
    // The ramp is only sampled, never drawn; t maps along its x axis.
    let ramp = gradient_shader::linear(
        (Point::new(0.0, 0.0), Point::new(1.0, 0.0)),
        colors,
        Some(positions),
        TileMode::Clamp,
        None,
        None,
    )?;
    effect.make_shader(Data::new_empty(), &[ramp.into()], Some(frame))
}

/// Stroke align by geometry expansion (docs/technotes/rendering-and-painters.md): Skia strokes
/// are center-only, so inside/outside strokes offset the stroked
/// geometry by half the width (corner radii adjust with it) and
/// center-stroke that.
fn draw_stroke(canvas: &Canvas, rrect: &RRect, stroke: &Stroke) {
    let half = stroke.width / 2.0;
    let stroked = match stroke.align {
        StrokeAlign::Center => *rrect,
        StrokeAlign::Inside => rrect.with_inset((half, half)),
        StrokeAlign::Outside => rrect.with_outset((half, half)),
    };
    let mut paint = solid_paint(stroke.color);
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(stroke.width);
    canvas.draw_rrect(stroked, &paint);
}

/// Draws an image fill clipped to the entry's (rounded) box.
fn draw_image_fill(
    canvas: &Canvas,
    rrect: &RRect,
    rect: &RectEntry,
    asset: &ImageAsset,
    scale_mode: ScaleMode,
    transform: Option<&dashpaint::Mat23>,
    tile_scale: f32,
) {
    let image = images::deferred_from_encoded_data(Data::new_copy(&asset.bytes), None)
        .expect("image asset decodes (validated upstream, P4)");
    let (iw, ih) = (image.width() as f32, image.height() as f32);

    canvas.save();
    canvas.clip_rrect(*rrect, ClipOp::Intersect, true);

    // Nearest sampling: deterministic and exact for the v0.3 corpus;
    // filtering quality is a later, deliberate decision.
    let sampling = SamplingOptions::default();
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);

    match scale_mode {
        ScaleMode::Fill | ScaleMode::Fit => {
            let scale = if scale_mode == ScaleMode::Fill {
                (rect.w / iw).max(rect.h / ih) // cover
            } else {
                (rect.w / iw).min(rect.h / ih) // contain
            };
            let (dw, dh) = (iw * scale, ih * scale);
            let dest = Rect::from_xywh(
                rect.x + (rect.w - dw) / 2.0,
                rect.y + (rect.h - dh) / 2.0,
                dw,
                dh,
            );
            canvas.draw_image_rect_with_sampling_options(&image, None, dest, sampling, &paint);
        }
        ScaleMode::Tile => {
            // Repeat at tile_scale magnification, anchored at the box
            // origin.
            let mut local = Matrix::scale((tile_scale, tile_scale));
            local.post_translate((rect.x, rect.y));
            let shader = image
                .to_shader(
                    Some((TileMode::Repeat, TileMode::Repeat)),
                    sampling,
                    Some(&local),
                )
                .expect("image shader");
            paint.set_shader(shader);
            canvas.draw_rrect(*rrect, &paint);
        }
        ScaleMode::Crop => {
            // uv_image = T · uv_box (normalized spaces; identity when
            // absent). The shader's local matrix maps image pixel space
            // to device space: box ∘ T⁻¹ ∘ image-normalize.
            let t = transform.copied().unwrap_or(dashpaint::Mat23 {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: 0.0,
                ty: 0.0,
            });
            let uv_transform = Matrix::new_all(t.a, t.b, t.tx, t.c, t.d, t.ty, 0.0, 0.0, 1.0);
            let inverted = uv_transform
                .invert()
                .expect("image crop transform is invertible (validated upstream, P4)");
            let mut local = Matrix::scale((1.0 / iw, 1.0 / ih));
            local.post_concat(&inverted);
            local.post_scale((rect.w, rect.h), None);
            local.post_translate((rect.x, rect.y));
            let shader = image
                .to_shader(
                    Some((TileMode::Clamp, TileMode::Clamp)),
                    sampling,
                    Some(&local),
                )
                .expect("image shader");
            paint.set_shader(shader);
            canvas.draw_rrect(*rrect, &paint);
        }
    }

    canvas.restore();
}
