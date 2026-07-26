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

use std::collections::HashMap;

use dashpaint::{
    Atlas, BlurKind, ClipTable, CornerRadii, GlyphRun, GlyphRunTable, Gradient, GradientKind,
    GroupComposite, ImageAsset, ImageTable, MAX_GRADIENT_STOPS, PaintKind, PaintTable, Painter,
    RectEntry, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, VectorField,
};
use skia_safe::{
    AlphaType, BlendMode, BlurStyle, Canvas, ClipOp, Color4f, ColorType, Data, EncodedImageFormat,
    FilterMode, Image, ImageFilter, ImageInfo, MaskFilter, Matrix, MipmapMode, Path, PathFillType,
    Point, RRect, Rect, RuntimeEffect, SamplingOptions, Shader, TileMode, canvas::SaveLayerRec,
    gradient_shader, image_filters, images, surfaces,
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
        groups: &[GroupComposite],
        glyphs: &GlyphRunTable,
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
        // The render-target group opacities (`masks-and-group-opacity.md`):
        // a group's rect range `[start, end)` composites offscreen and the
        // layer blends at `alpha`. Groups arrive in ascending `start` order
        // (DFS pre-order) and nest, so one pointer walks the starts and a
        // stack of pending `end` indices closes the layers innermost first.
        let mut next_group = 0usize;
        let mut open_group_ends: Vec<u32> = Vec::new();
        // Baked-vector shapes (story B1): the MSDF resolve effect compiles
        // once (lazily — a vector-free scene pays nothing), and each atlas PNG
        // decodes once (the hero repeats one atlas across ~148 vectors).
        let mut field_effect: Option<RuntimeEffect> = None;
        let mut field_atlases: HashMap<u32, Image> = HashMap::new();
        for (i, rect) in source.iter().enumerate() {
            // Open every group that starts at this rect (at most one per
            // index — one opacity per node). `save_layer_alpha` begins the
            // offscreen composite.
            while next_group < groups.len() && groups[next_group].start == i as u32 {
                let group = groups[next_group];
                let alpha = (group.alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
                canvas.save_layer_alpha(None, alpha);
                open_group_ends.push(group.end);
                next_group += 1;
            }

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
            // How far the node's stroke pushes its rendered silhouette past
            // the fill box: an outside stroke by its full width, a center
            // stroke by half, an inside stroke not at all. A drop shadow
            // casts from that silhouette, not the bare fill box (P1).
            let outset = stroke_outset(entry.stroke.as_ref());
            // The backdrop blur runs before any of this node's own ink
            // (story #393, `docs/decisions/backdrop-blur-is-core-vocabulary.md`).
            // Boundary B states the guarantee over rects at a *lower index*,
            // so the backdrop this node samples is what those rects
            // composited — not this node's own drop shadow, which is part of
            // how the node paints rather than part of what lies behind it.
            // Painting in slice order satisfies the barrier for free: every
            // lower-index rect is already on the canvas here.
            //
            // A `BlurKind::Layer` blur is skipped: it is node-local, budgeted
            // at v1, and deliberately not part of this story
            // (the decision record's "`LAYER_BLUR` does not ride along").
            // Nothing in this tree emits one — `dashc` lowers only
            // `BACKGROUND_BLUR` — so this is a named gap, not a silent drop.
            //
            // Several backdrop blurs on one node apply in list order, each
            // over the result of the last, the same posture the shadow loops
            // below use for Figma's back-to-front `effects` array.
            for blur in entry
                .blurs
                .iter()
                .filter(|blur| blur.kind == BlurKind::Backdrop)
            {
                match &entry.shape {
                    // A baked-vector node's blur is confined to the field's
                    // coverage, not to its box — the hero's own frosted panel
                    // is exactly this shape, a VECTOR carrying
                    // `BACKGROUND_BLUR` (`crates/dashc/src/figma/mod.rs`), so
                    // blurring its whole box would frost a rectangle where the
                    // design has a rounded shape.
                    Some(field) => {
                        let effect = field_effect.get_or_insert_with(|| {
                            RuntimeEffect::make_for_shader(FIELD_MASK_SKSL, None)
                                .expect("field-mask resolve SkSL compiles")
                        });
                        let atlas = field_atlases
                            .entry(field.image)
                            .or_insert_with(|| decode_image(images.resolve(field.image)));
                        draw_backdrop_blur_field(
                            canvas,
                            rect,
                            field,
                            atlas,
                            effect,
                            blur.radius,
                            rect.opacity,
                        );
                    }
                    None => draw_backdrop_blur_box(canvas, &rrect, blur.radius, rect.opacity),
                }
            }
            // Drop shadows fall behind the fill (story #45,
            // `docs/decisions/effects-vocabulary-shadows.md`). They draw
            // inside this rect's clip-region save/restore, so an ancestor
            // clip bounds the shadow, and inside any open render-target
            // group layer, so a shadowed node under an overlapping partial
            // opacity composites its shadow in the layer. `rect.opacity`
            // carries the free-path group alpha.
            //
            // `entry.shadows` is in Figma's `effects` array order, which is
            // back-to-front (like Figma's `children`: `effects[0]` is the
            // backmost shadow, the last element renders on top). DFS draw
            // order composites a later draw over an earlier one, so painting
            // the list forward — first element first — reproduces that
            // stacking exactly; no reversal is needed.
            for shadow in entry.shadows.iter().filter(|s| s.kind == ShadowKind::Drop) {
                draw_drop_shadow(canvas, rect, &entry.corners, outset, shadow, rect.opacity);
            }
            if let Some(field) = &entry.shape {
                // A baked-vector shape (story B1): the fill is masked by the
                // field's coverage, not by the parametric box. The parametric
                // stroke and corners do not apply (a vector carries its
                // outline in the baked geometry).
                let effect = field_effect.get_or_insert_with(|| {
                    RuntimeEffect::make_for_shader(FIELD_MASK_SKSL, None)
                        .expect("field-mask resolve SkSL compiles")
                });
                let atlas = field_atlases
                    .entry(field.image)
                    .or_insert_with(|| decode_image(images.resolve(field.image)));
                draw_vector_field(canvas, rect, entry.fill.as_ref(), field, atlas, effect);
            } else {
                // A fill-less entry draws nothing (a layout-only node, or a
                // mask node whose shape is a stencil, not paint). Stacked
                // fills (story C1, debt #146) composite bottom to top on the
                // same box: `fill` first, then each of `extra_fills` in
                // order, an ordinary sequence of draws — a later one is
                // already an "over" composite onto the ones before it, so no
                // fill-specific blend logic is needed beyond drawing in
                // order. Empty `extra_fills` (every pre-C1 entry) draws
                // exactly the one fill it always has.
                if let Some(kind) = &entry.fill {
                    draw_fill_kind(canvas, rrect, rect, images, kind);
                }
                for kind in &entry.extra_fills {
                    draw_fill_kind(canvas, rrect, rect, images, kind);
                }
                if let Some(stroke) = &entry.stroke {
                    draw_stroke(canvas, &rrect, stroke, rect.opacity);
                }
            }
            // Inner shadows sit on top of the fill and stroke, clipped to
            // the node's own shape (story #45).
            for shadow in entry.shadows.iter().filter(|s| s.kind == ShadowKind::Inner) {
                draw_inner_shadow(canvas, &rrect, rect, &entry.corners, shadow, rect.opacity);
            }
            if clipped {
                canvas.restore();
            }

            // Close every group whose subtree ends after this rect,
            // innermost first (`end` values on the stack are non-increasing
            // from the top by the nesting).
            while open_group_ends.last() == Some(&(i as u32 + 1)) {
                canvas.restore();
                open_group_ends.pop();
            }
        }

        // Text is drawn after every rect: the v0.5 Latin subset composites
        // glyph runs over the rect table as foreground (boundary B's
        // paint contract). Runs arrive already shaped and positioned (P2),
        // and each glyph is one textured MSDF atlas quad.
        draw_glyph_runs(canvas, glyphs);
    }
}

/// The MSDF resolve shader. Samples the atlas (RGB distance channels),
/// takes the median as the signed distance, and turns it into coverage
/// over a screen-pixel range — the standard multi-channel SDF text
/// resolve (docs/technotes/rendering-and-painters.md: the SDF quad
/// renderer, driven from our own atlas). The child `msdf` maps device
/// coordinates back to the glyph's atlas texels via its local matrix, so
/// `main` samples at the point it is drawing.
const MSDF_SKSL: &str = r"
    uniform shader msdf;
    uniform float4 color;
    uniform float px_range;
    half4 main(float2 p) {
        float3 s = float3(msdf.eval(p).rgb);
        float sd = max(min(s.r, s.g), min(max(s.r, s.g), s.b));
        float coverage = clamp(px_range * (sd - 0.5) + 0.5, 0.0, 1.0);
        float alpha = color.a * coverage;
        return half4(half3(color.rgb * alpha), half(alpha));
    }
";

/// Draws every glyph run's quads. Each atlas image is decoded once (not
/// once per run that samples it); the resolve effect compiles once.
fn draw_glyph_runs(canvas: &Canvas, glyphs: &GlyphRunTable) {
    if glyphs.is_empty() {
        return;
    }
    let effect =
        RuntimeEffect::make_for_shader(MSDF_SKSL, None).expect("MSDF resolve SkSL compiles");
    let decoded: Vec<Image> = glyphs
        .atlases()
        .iter()
        .map(|atlas| {
            images::deferred_from_encoded_data(Data::new_copy(&atlas.image.bytes), None)
                .expect("atlas image decodes (a build artifact, validated upstream P4)")
        })
        .collect();
    for run in glyphs.runs() {
        let atlas = glyphs.atlas(run.atlas);
        let image = &decoded[run.atlas.0 as usize];
        // The MSDF field is a distance, not a color: linear filtering
        // interpolates the field (the point of MSDF's crisp edges);
        // nearest would step it. The surface carries no color space
        // (raster_n32_premul), so the channels sample raw — no sRGB
        // conversion mangling the distances.
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
        // The screen-pixel range scales the atlas distance range by the
        // ratio of render size to the size the atlas was baked at
        // (docs/design/atlas-pipeline.md).
        let px_range = atlas.distance_range_px * run.size / f32::from(atlas.px_per_em);
        // Fold the run's free-path group alpha into the fill (story #44):
        // the MSDF resolve modulates coverage by `color.a`, so multiplying
        // the alpha dims the whole run. The render-target group path and
        // clip/mask regions are not applied to runs (a documented
        // limitation on `GlyphRun::opacity`).
        let color = dashpaint::Color {
            a: run.color.a * run.opacity,
            ..run.color
        };
        let uniforms = msdf_uniforms(&effect, color, px_range);
        for quad in &run.glyphs {
            let Some(g) = atlas.glyph(quad.glyph_id) else {
                // No quad for this glyph id — an empty outline (space) or
                // a glyph outside the atlas charset. Painting nothing is
                // correct for the former; the latter is a coverage gap the
                // build-time closure owns (P4), not a per-frame decision.
                continue;
            };
            draw_glyph_quad(
                canvas, image, atlas, g, quad, run, &effect, &uniforms, sampling,
            );
        }
    }
}

/// Draws one glyph as a textured MSDF quad: maps the atlas texels to the
/// device quad, wraps the atlas sample in the resolve effect, and fills
/// the quad.
#[allow(clippy::too_many_arguments)]
fn draw_glyph_quad(
    canvas: &Canvas,
    image: &Image,
    atlas: &Atlas,
    glyph: &dashpaint::AtlasGlyph,
    quad: &dashpaint::GlyphQuad,
    run: &GlyphRun,
    effect: &RuntimeEffect,
    uniforms: &Data,
    sampling: SamplingOptions,
) {
    let size = run.size;
    let [pl, pb, pr, pt] = glyph.plane_em;
    // plane_em is y-up (baseline origin); document space is y-down, so
    // the top edge subtracts and the bottom edge subtracts a smaller (or
    // negative, for descenders) value.
    let dest = Rect::from_ltrb(
        quad.x + pl * size,
        quad.y - pt * size,
        quad.x + pr * size,
        quad.y - pb * size,
    );
    let [al, ab, ar, at] = glyph.atlas_px;
    // atlas_px is bottom-left origin; skia images are top-left, so flip y.
    let height = atlas.height as f32;
    let (src_left, src_top, src_right, src_bottom) = (al, height - at, ar, height - ab);

    let (src_w, src_h) = (src_right - src_left, src_bottom - src_top);
    if src_w <= 0.0 || src_h <= 0.0 {
        return;
    }
    let sx = dest.width() / src_w;
    let sy = dest.height() / src_h;
    // texel -> device: src maps onto dest.
    let mut local = Matrix::translate((-src_left, -src_top));
    local.post_scale((sx, sy), None);
    local.post_translate((dest.left, dest.top));

    let atlas_shader = image
        .to_shader((TileMode::Clamp, TileMode::Clamp), sampling, Some(&local))
        .expect("atlas image shader");
    let shader = effect
        .make_shader(uniforms.clone(), &[atlas_shader.into()], None)
        .expect("MSDF resolve shader");
    let mut paint = skia_safe::Paint::default();
    paint.set_shader(shader);
    // Coverage lives entirely in the shader; the quad itself is an opaque
    // carrier, so rect-edge anti-aliasing would only double up on the
    // transparent MSDF margin.
    paint.set_anti_alias(false);
    canvas.draw_rect(dest, &paint);
}

/// Packs the resolve effect's uniforms (`color` as float4, `px_range` as
/// float) by the offsets the compiled effect reports, so the byte layout
/// cannot drift from the SkSL declaration.
fn msdf_uniforms(effect: &RuntimeEffect, color: dashpaint::Color, px_range: f32) -> Data {
    let mut buf = vec![0u8; effect.uniform_size()];
    for uniform in effect.uniforms() {
        let offset = uniform.offset();
        match uniform.name() {
            "color" => {
                for (i, v) in [color.r, color.g, color.b, color.a].into_iter().enumerate() {
                    buf[offset + i * 4..offset + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
            "px_range" => {
                buf[offset..offset + 4].copy_from_slice(&px_range.to_le_bytes());
            }
            other => panic!("unexpected MSDF uniform {other}"),
        }
    }
    Data::new_copy(&buf)
}

/// The baked-vector coverage resolve (story B1). Samples the field atlas
/// (RGB distance channels), takes the median as the signed distance, and
/// turns it into a coverage value over a screen-pixel range — the same
/// multi-channel SDF reconstruction as [`MSDF_SKSL`], differing only in what
/// it modulates: it returns a white premultiplied coverage the painter blends
/// as an alpha mask (`BlendMode::DstIn`) over an already-drawn fill, so the
/// fill (solid or gradient) shows only inside the shape (P2 composition).
const FIELD_MASK_SKSL: &str = r"
    uniform shader field;
    uniform float px_range;
    half4 main(float2 p) {
        float3 s = float3(field.eval(p).rgb);
        float sd = max(min(s.r, s.g), min(max(s.r, s.g), s.b));
        float coverage = clamp(px_range * (sd - 0.5) + 0.5, 0.0, 1.0);
        return half4(coverage);
    }
";

/// Draws one baked-vector shape (story B1): the field masks the paint
/// entry's fill. The fill draws into an offscreen layer, then the field
/// coverage multiplies the layer's alpha (`DstIn`), so the fill — solid or
/// gradient — shows only inside the shape. The gradient's frame stays the
/// node box, so a gradient-filled vector composes day one.
///
/// The padded field quad (`plane_bounds`) maps to device space at unit scale,
/// origin at the node box top-left (`device = rect_origin + plane_bounds`).
/// The quad's margin reads as coverage 0, so drawing over exactly the quad
/// clips nothing wrongly.
fn draw_vector_field(
    canvas: &Canvas,
    rect: &RectEntry,
    fill: Option<&PaintKind>,
    field: &VectorField,
    atlas: &Image,
    effect: &RuntimeEffect,
) {
    // A shape with no fill has no ink to mask — a defensive guard; the
    // lowering always pairs a shape with a fill.
    let Some(fill) = fill else {
        return;
    };
    let Some((dest, coverage)) = field_coverage(rect, field, atlas, effect) else {
        return;
    };

    // Draw the fill into a layer, then multiply its alpha by the coverage.
    // The layer composites (SrcOver) over whatever is behind, so the masked
    // shape stacks correctly. `rect.opacity` is the free-path group alpha,
    // folded into the fill.
    canvas.save_layer_alpha(None, 255);
    match fill {
        PaintKind::Solid { color } => {
            let mut paint = solid_paint(*color);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rect(dest, &paint);
        }
        PaintKind::Gradient(gradient) => {
            let mut paint = gradient_paint(gradient, rect);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rect(dest, &paint);
        }
        // An image-filled vector is not in the measured census (B1 widens by
        // exactly what is measured); it draws nothing rather than an unmasked
        // rectangle. Masking an image fill is additive later work.
        PaintKind::Image { .. } => {}
    }
    let mut mask = skia_safe::Paint::default();
    mask.set_shader(coverage);
    mask.set_blend_mode(BlendMode::DstIn);
    mask.set_anti_alias(false);
    canvas.draw_rect(dest, &mask);
    canvas.restore();
}

/// The device quad a baked-vector shape occupies, and the shader that
/// resolves its field into a coverage mask over that quad.
///
/// Both draws that mask by a baked shape use it: the masked fill
/// ([`draw_vector_field`]) and the backdrop blur
/// ([`draw_backdrop_blur_field`], story #393). Stated once so the two cannot
/// disagree about where the shape is or how sharp its edge resolves.
///
/// The padded field quad (`plane_bounds`) maps to device space at unit scale,
/// origin at the node box top-left. `None` for a degenerate quad (no area),
/// which draws nothing rather than dividing by zero.
fn field_coverage(
    rect: &RectEntry,
    field: &VectorField,
    atlas: &Image,
    effect: &RuntimeEffect,
) -> Option<(Rect, Shader)> {
    let [left, top, right, bottom] = field.plane_bounds;
    let dest = Rect::from_ltrb(rect.x + left, rect.y + top, rect.x + right, rect.y + bottom);
    if dest.width() <= 0.0 || dest.height() <= 0.0 {
        return None;
    }

    // texel -> device: the shape's atlas sub-rect maps onto the padded quad.
    let [ax, ay, aw, ah] = field.atlas_rect;
    let (sx, sy) = (dest.width() / aw as f32, dest.height() / ah as f32);
    let mut local = Matrix::translate((-(ax as f32), -(ay as f32)));
    local.post_scale((sx, sy), None);
    local.post_translate((dest.left, dest.top));

    // Linear filtering interpolates the distance field (MSDF's crisp edges);
    // the raster surface carries no color space, so channels sample raw — the
    // same sampling the glyph atlas uses.
    let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
    let field_shader = atlas
        .to_shader((TileMode::Clamp, TileMode::Clamp), sampling, Some(&local))
        .expect("field atlas shader");
    // Screen-pixel range: the distance range (atlas texels) times device
    // pixels per texel — the glyph-atlas metric, with unit shape->device
    // scale, so `sx` is the pixels-per-texel factor.
    let px_range = field.distance_range * sx;
    let uniforms = field_mask_uniforms(effect, px_range);
    let coverage = effect
        .make_shader(uniforms, &[field_shader.into()], None)
        .expect("field-mask resolve shader");
    Some((dest, coverage))
}

/// Packs the field-mask effect's one `px_range` uniform by the offset the
/// compiled effect reports, so the byte layout cannot drift from the SkSL.
fn field_mask_uniforms(effect: &RuntimeEffect, px_range: f32) -> Data {
    let mut buf = vec![0u8; effect.uniform_size()];
    for uniform in effect.uniforms() {
        match uniform.name() {
            "px_range" => {
                let offset = uniform.offset();
                buf[offset..offset + 4].copy_from_slice(&px_range.to_le_bytes());
            }
            other => panic!("unexpected field-mask uniform {other}"),
        }
    }
    Data::new_copy(&buf)
}

/// Modulates a paint's alpha by the rect's free-path group opacity
/// (`docs/decisions/masks-and-group-opacity.md`). A `1.0` opacity leaves
/// the paint untouched; for a shader paint (gradient, image) the paint
/// alpha multiplies the shader's output, so the same call covers every
/// fill kind.
fn apply_opacity(paint: &mut skia_safe::Paint, opacity: f32) {
    if opacity != 1.0 {
        paint.set_alpha_f(paint.alpha_f() * opacity);
    }
}

/// The Gaussian sigma a blur radius maps to. Skia takes a sigma, not a
/// radius; the CSS/browser convention `sigma = radius / 2` is the one this
/// reference painter defines (story #45), and it is what the `blur-falloff`
/// tolerance band pins against Figma's own render. Stated once because a
/// shadow's blur (story #45) and a backdrop blur (story #393) are the same
/// mapping, and two copies of it could drift apart.
fn blur_sigma(radius: f32) -> f32 {
    radius / 2.0
}

/// The Gaussian blur a shadow's `blur` radius applies. A zero-radius
/// shadow uses no mask filter — a hard edge, not a degenerate blur.
fn blur_mask_filter(blur: f32) -> Option<MaskFilter> {
    (blur > 0.0)
        .then(|| MaskFilter::blur(BlurStyle::Normal, blur_sigma(blur), false))
        .flatten()
}

/// The image filter a backdrop blur of `radius` applies to the backdrop, or
/// `None` when the radius is not positive (nothing to blur — not a
/// degenerate filter, the same rule [`blur_mask_filter`] follows).
///
/// The guard is written as [`blur_mask_filter`]'s is — `radius > 0.0`, which
/// is false for a NaN — rather than as a `<= 0.0` rejection, which a NaN
/// passes. The document load path already refuses a non-finite radius
/// (`paint.blur.invalid-radius`), but the producer API stores `Prop::Blurs`
/// unchecked, so this is the last place the two can disagree, and a NaN
/// sigma reaching Skia has no defined result.
///
/// `TileMode::Clamp` at the filter's input edge: where the kernel reaches
/// past the backdrop Skia captured, the edge pixel extends rather than
/// fading toward transparent black, so a node frosting the canvas edge picks
/// up that edge's color instead of darkening. This is the edge-duplication
/// rule CSS's `backdrop-filter` specifies, for the same reason.
fn backdrop_blur_filter(radius: f32) -> Option<ImageFilter> {
    (radius > 0.0)
        .then(|| {
            let sigma = blur_sigma(radius);
            image_filters::blur((sigma, sigma), TileMode::Clamp, None, None)
        })
        .flatten()
}

/// The paint a backdrop-blur layer composites through: transparent-by-alpha
/// only, carrying the rect's free-path group alpha
/// (`docs/decisions/masks-and-group-opacity.md`) exactly as every other draw
/// in this painter does. At `opacity = 1.0` the blurred copy fully replaces
/// the region; below it, the copy composites over the sharp original, so a
/// dimmed node frosts proportionally.
fn backdrop_layer_paint(opacity: f32) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    apply_opacity(&mut paint, opacity);
    paint
}

/// Replaces the region a parametric (rounded-box) node covers with a blurred
/// copy of everything already composited beneath it — the backdrop blur of
/// story #393 (`docs/decisions/backdrop-blur-is-core-vocabulary.md`).
///
/// Skia has this natively: a `save_layer` whose `SaveLayerRec` carries a
/// backdrop [`ImageFilter`] initializes the new layer with the current
/// layer's contents passed through that filter, respecting the current clip.
/// Clipping to the node's own rounded box first is what confines the blurred
/// copy to the node's shape; Skia still reads the halo the kernel needs from
/// outside that clip, so the blur is built from the real backdrop rather than
/// from a clip-truncated copy of it (pinned by
/// `the_backdrop_blur_reads_past_the_node_box`).
///
/// Nothing is drawn into the layer, so its whole content is the blurred
/// backdrop and the immediate `restore` composites that over the sharp
/// original. The node's own shadows, fills and stroke then draw on top
/// through the ordinary path: a backdrop blur changes what is behind a node,
/// not how the node paints.
///
/// **Inside a [`GroupComposite`] the sample reads that group's layer, not the
/// canvas beneath it.** The layer Skia filters is the innermost open one, so
/// a render-target group is a backdrop root: a node inside it frosts its
/// in-group siblings and nothing further down. That is the settled reading of
/// the question boundary B left open, and the reason is in the decision
/// record — sampling through the group would composite the backdrop twice,
/// once directly and once inside the group's own alpha.
fn draw_backdrop_blur_box(canvas: &Canvas, shape: &RRect, radius: f32, opacity: f32) {
    let Some(filter) = backdrop_blur_filter(radius) else {
        return;
    };
    let layer = backdrop_layer_paint(opacity);
    canvas.save();
    canvas.clip_rrect(*shape, ClipOp::Intersect, true);
    canvas.save_layer(&SaveLayerRec::default().backdrop(&filter).paint(&layer));
    canvas.restore();
    canvas.restore();
}

/// [`draw_backdrop_blur_box`] for a baked-vector node (story B1): the blurred
/// backdrop is confined to the field's coverage rather than to a box.
///
/// This is the shape the live hero's frosted panel actually has — a Figma
/// VECTOR carrying `BACKGROUND_BLUR` (`crates/dashc/src/figma/mod.rs`) — so
/// it is the path the story's fidelity number depends on, not a generality.
///
/// A rounded-rect clip cannot express a baked outline, so two things confine
/// the blur rather than one. The canvas is clipped to the field's padded
/// quad, which bounds the layer; inside it, `BlendMode::DstIn` against the
/// coverage shader multiplies the layer's alpha by the shape's coverage,
/// clearing it outside the outline; the restore then composites what is left
/// over the sharp backdrop.
///
/// **The clip is load-bearing, not a duplicate of the mask.**
/// `SaveLayerRec::bounds` is a hint to Skia, not a guarantee: with a backdrop
/// filter the layer is allocated over the device clip, so without this clip
/// every layer pixel the `DstIn` rect does not cover keeps a full-opacity
/// blurred backdrop and composites on restore — one baked-vector node would
/// blur the whole frame. `the_baked_vector_blur_is_confined_to_its_quad`
/// pins it. The clip does not truncate the blur's input: Skia reads the halo
/// the kernel needs from outside it, which is the same property the box path
/// relies on.
fn draw_backdrop_blur_field(
    canvas: &Canvas,
    rect: &RectEntry,
    field: &VectorField,
    atlas: &Image,
    effect: &RuntimeEffect,
    radius: f32,
    opacity: f32,
) {
    let Some(filter) = backdrop_blur_filter(radius) else {
        return;
    };
    let Some((dest, coverage)) = field_coverage(rect, field, atlas, effect) else {
        return;
    };
    let layer = backdrop_layer_paint(opacity);
    canvas.save();
    canvas.clip_rect(dest, ClipOp::Intersect, false);
    // No `bounds` on the rec: Skia discards it whenever a backdrop filter is
    // set (`SkCanvas::internalSaveLayer` takes the user bounds as a hard
    // layer extent only on the trivial-restore path, which a backdrop
    // disables), so passing one would state a constraint that does not hold
    // and invite the clip above being removed as a duplicate.
    canvas.save_layer(&SaveLayerRec::default().backdrop(&filter).paint(&layer));
    let mut mask = skia_safe::Paint::default();
    mask.set_shader(coverage);
    mask.set_blend_mode(BlendMode::DstIn);
    mask.set_anti_alias(false);
    canvas.draw_rect(dest, &mask);
    canvas.restore();
    canvas.restore();
}

/// Per-corner radii adjusted by a spread delta: a corner grows with
/// positive spread (a drop shadow) or shrinks with negative (an inner
/// shadow's lit hole), floored at zero. A sharp corner (radius 0) stays
/// sharp, matching CSS's spread rule.
fn spread_corners(corners: &CornerRadii, delta: f32) -> CornerRadii {
    let adj = |r: f32| if r > 0.0 { (r + delta).max(0.0) } else { 0.0 };
    CornerRadii {
        top_left: adj(corners.top_left),
        top_right: adj(corners.top_right),
        bottom_right: adj(corners.bottom_right),
        bottom_left: adj(corners.bottom_left),
    }
}

/// How far the stroke pushes the node's rendered outline past its fill box,
/// the same geometry `draw_stroke` uses: an outside stroke by its full
/// width, a center stroke by half, an inside stroke not at all. A drop
/// shadow casts from that outline (P1), so it grows the shadow shape by
/// this amount before the spread and offset apply.
fn stroke_outset(stroke: Option<&Stroke>) -> f32 {
    match stroke {
        Some(s) => match s.align {
            StrokeAlign::Inside => 0.0,
            StrokeAlign::Center => s.width / 2.0,
            StrokeAlign::Outside => s.width,
        },
        None => 0.0,
    }
}

/// Draws a drop shadow: the node's rendered outline (its fill box grown by
/// `stroke_outset` for an outside/center stroke), offset and grown by
/// `spread` (the shadow-spread math of the seed §8.1), filled with the
/// shadow color under a Gaussian blur, behind the node. `opacity` is the
/// rect's free-path group alpha.
fn draw_drop_shadow(
    canvas: &Canvas,
    rect: &RectEntry,
    corners: &CornerRadii,
    stroke_outset: f32,
    shadow: &Shadow,
    opacity: f32,
) {
    // The silhouette grows by the stroke outset and the spread together;
    // the corners follow, and a sharp corner stays sharp under both.
    let grow = stroke_outset + shadow.spread;
    let shape = rrect_of(
        rect.x - grow + shadow.offset.x,
        rect.y - grow + shadow.offset.y,
        (rect.w + 2.0 * grow).max(0.0),
        (rect.h + 2.0 * grow).max(0.0),
        &spread_corners(corners, grow),
    );
    let mut paint = solid_paint(shadow.color);
    paint.set_anti_alias(true);
    paint.set_mask_filter(blur_mask_filter(shadow.blur));
    apply_opacity(&mut paint, opacity);
    canvas.draw_rrect(shape, &paint);
}

/// Draws an inner shadow: clip to the node's shape, then fill everything
/// except the (offset, spread-inset) inner shape with the shadow color
/// under a Gaussian blur, so the blur bleeds inward from the shape's edge
/// (`docs/decisions/effects-vocabulary-shadows.md`). The even-odd path is
/// an outer rect minus the inner rounded rect; the outer rect extends past
/// the clip by more than the blur radius, so the shadow saturates at the
/// shape edge rather than fading from the outer boundary.
fn draw_inner_shadow(
    canvas: &Canvas,
    shape: &RRect,
    rect: &RectEntry,
    corners: &CornerRadii,
    shadow: &Shadow,
    opacity: f32,
) {
    let s = shadow.spread;
    let hole = rrect_of(
        rect.x + s + shadow.offset.x,
        rect.y + s + shadow.offset.y,
        (rect.w - 2.0 * s).max(0.0),
        (rect.h - 2.0 * s).max(0.0),
        &spread_corners(corners, -s),
    );
    let margin =
        shadow.blur * 3.0 + s.abs() + shadow.offset.x.abs().max(shadow.offset.y.abs()) + 4.0;
    let outer = Rect::from_xywh(
        rect.x - margin,
        rect.y - margin,
        rect.w + 2.0 * margin,
        rect.h + 2.0 * margin,
    );
    let mut path = Path::new();
    path.add_rect(outer, None);
    path.add_rrect(hole, None);
    path.set_fill_type(PathFillType::EvenOdd);

    let mut paint = solid_paint(shadow.color);
    paint.set_anti_alias(true);
    paint.set_mask_filter(blur_mask_filter(shadow.blur));
    apply_opacity(&mut paint, opacity);

    canvas.save();
    canvas.clip_rrect(*shape, ClipOp::Intersect, true);
    canvas.draw_path(&path, &paint);
    canvas.restore();
}

/// Draws one fill kind into `rrect` — `entry.fill` or one of
/// `entry.extra_fills` (story C1, debt #146). Every caller draws the ordinary
/// (non-vector-masked) box in the same order the entry's fills stack, so a
/// later call composites over an earlier one with Skia's default "over"
/// blend — no per-fill blend mode is needed, because an advanced blend mode
/// on any fill is already refused upstream by name (the profile triage,
/// before it ever reaches a paint entry).
fn draw_fill_kind(
    canvas: &Canvas,
    rrect: RRect,
    rect: &RectEntry,
    images: &ImageTable,
    kind: &PaintKind,
) {
    match kind {
        PaintKind::Solid { color } => {
            let mut paint = solid_paint(*color);
            paint.set_anti_alias(true);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rrect(rrect, &paint);
        }
        PaintKind::Gradient(gradient) => {
            let mut paint = gradient_paint(gradient, rect);
            paint.set_anti_alias(true);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rrect(rrect, &paint);
        }
        PaintKind::Image {
            image,
            scale_mode,
            transform,
            tile_scale,
        } => {
            draw_image_fill(
                canvas,
                &rrect,
                rect,
                images.resolve(*image),
                *scale_mode,
                transform.as_ref(),
                *tile_scale,
                rect.opacity,
            );
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
fn draw_stroke(canvas: &Canvas, rrect: &RRect, stroke: &Stroke, opacity: f32) {
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
    apply_opacity(&mut paint, opacity);
    canvas.draw_rrect(stroked, &paint);
}

/// Decodes an encoded image asset with the Skia build's own codec —
/// `docs/decisions/image-assets-cross-boundary-b.md` names this each
/// painter's own machinery, and this build's `skia-safe` carries Png,
/// Jpeg, and Gif decode already (proven for Jpeg and Gif by this crate's
/// `tests::decode` module — a trimmed Skia build without codecs would need
/// the pure-Rust fallback `docs/decisions/downloaded-raster-needs-no-vector-engine.md`
/// describes, which the reference painter does not run).
fn decode_image(asset: &ImageAsset) -> Image {
    images::deferred_from_encoded_data(Data::new_copy(&asset.bytes), None)
        .expect("image asset decodes (validated upstream, P4)")
}

/// Draws an image fill clipped to the entry's (rounded) box.
#[allow(clippy::too_many_arguments)]
fn draw_image_fill(
    canvas: &Canvas,
    rrect: &RRect,
    rect: &RectEntry,
    asset: &ImageAsset,
    scale_mode: ScaleMode,
    transform: Option<&dashpaint::Mat23>,
    tile_scale: f32,
    opacity: f32,
) {
    let image = decode_image(asset);
    let (iw, ih) = (image.width() as f32, image.height() as f32);

    canvas.save();
    canvas.clip_rrect(*rrect, ClipOp::Intersect, true);

    // Nearest sampling: deterministic and exact for the v0.3 corpus;
    // filtering quality is a later, deliberate decision.
    let sampling = SamplingOptions::default();
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    apply_opacity(&mut paint, opacity);

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

#[cfg(test)]
mod tests {
    //! Proves `decode_image` actually decodes Jpeg and static Gif rather
    //! than assuming the pinned `skia-safe` build's codecs cover them
    //! (story #342) — a real 2x2 fixture through the real decode path, not
    //! a stub.

    use super::*;

    /// A real 2x2 JPEG (`convert`-encoded; lossy, so no pixel-color
    /// assertion — only size proves the decode).
    const JPEG_2X2: &[u8] = include_bytes!("../tests/fixtures/quadrant.jpg");

    /// A real 2x2 single-frame Gif.
    const GIF_2X2: &[u8] = include_bytes!("../tests/fixtures/quadrant.gif");

    #[test]
    fn decode_image_handles_a_real_jpeg() {
        let asset = ImageAsset {
            format: dashpaint::ImageFormat::Jpeg,
            bytes: JPEG_2X2.to_vec(),
        };
        let image = decode_image(&asset);
        assert_eq!((image.width(), image.height()), (2, 2));
    }

    #[test]
    fn decode_image_handles_a_real_static_gif() {
        let asset = ImageAsset {
            format: dashpaint::ImageFormat::Gif,
            bytes: GIF_2X2.to_vec(),
        };
        let image = decode_image(&asset);
        assert_eq!((image.width(), image.height()), (2, 2));
    }
}
