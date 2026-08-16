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

pub mod retention;

use std::collections::HashMap;
use std::sync::Arc;

use dashpaint::{
    Atlas, BlurKind, ClipTable, CornerRadii, Fill, GlyphRun, GlyphRunTable, Gradient, GradientKind,
    GradientView, GroupComposite, ImageTable, MAX_GRADIENT_STOPS, PaintKind, PaintTable, Painter,
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
    ///
    /// Render-target group composites are retained the same way (issue
    /// #278): a group whose rect range no dirty index touches blends last
    /// frame's layer again instead of redrawing its subtree. The rule is
    /// [`retention::GroupCache`]'s, and it extends the differential test — a dirty
    /// set that misses a rect inside a group now leaves a stale *layer*,
    /// not only a stale entry.
    ///
    /// A dirty index the rect table does not have is skipped, not a panic
    /// (debt #181). `Painter::paint` states no precondition on the set's
    /// indices — it calls the set advisory and says ignoring it is always
    /// correct — so a caller honoring that contract can hand over a stale
    /// index, and the only reading of "advisory" that stays true of a
    /// surplus one is to ignore it. Skipping cannot change the picture
    /// either: an index past the end names no rect, so there is nothing it
    /// could have refreshed.
    Retained,
}

/// The reference painter: draws boundary-B input onto a CPU raster
/// surface (N32 premultiplied).
pub struct SkiaPainter {
    surface: skia_safe::Surface,
    mode: DirtyMode,
    /// The simulated instance buffer. Empty in `Full` mode.
    retained: Vec<RectEntry>,
    /// The retained render-target group composites (issue #278). The layer
    /// handle is a raster [`Image`] snapshotted from the offscreen surface
    /// the group's subtree drew into; the rule for when one may be blended
    /// again is [`retention`]'s and names no Skia type. Always empty in
    /// `Full` mode, which rebuilds every layer by construction.
    group_layers: retention::GroupCache<Image>,
    /// The decoded image assets, kept for as long as this painter lives
    /// (issue #639). Independent of [`DirtyMode`]: a decode is not a draw,
    /// and both modes read the same table.
    images: ImageCache,
    /// The decoded MSDF glyph atlases and the compiled resolve shader, kept
    /// for as long as this painter lives (issue #644). The sibling of
    /// [`ImageCache`] for the text half of the painter input, and
    /// [`DirtyMode`]-independent for the same reason.
    msdf: MsdfCache,
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
    /// **The surface carries no colour space, and that is decided rather than
    /// incidental.** Two independent requirements need it, and they agree:
    ///
    /// - MSDF distance channels sample raw, with no sRGB transfer applied to
    ///   what is a distance rather than a colour
    ///   (`docs/decisions/q1-msdf-below-14px.md`, and the comment at the
    ///   sampling site below).
    /// - Blur therefore averages raw sRGB-encoded channel values rather than
    ///   linear light, which is what Figma's own `BACKGROUND_BLUR` does —
    ///   measured, not assumed
    ///   (`docs/decisions/blur-blends-in-srgb-encoded-space.md`).
    ///
    /// Attaching a linear working colour space would break both at once. Both
    /// `backdrop-blur` oracle frames fail on that change, at 5.429 % and
    /// 4.866 % against a 2 % budget.
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
            group_layers: retention::GroupCache::new(),
            images: ImageCache::default(),
            msdf: MsdfCache::default(),
        }
    }

    /// How many render-target group composites this painter has built since
    /// it was created (issue #278).
    ///
    /// In [`DirtyMode::Retained`] a group whose range stays clean is built
    /// once and blended on every later frame, so a scene with one stable
    /// group reads `1` after any number of frames. In [`DirtyMode::Full`]
    /// this stays `0`: that mode composites through Skia's own
    /// `save_layer` and retains nothing, which is what keeps it correct
    /// without consulting the dirty set.
    pub fn group_composites_built(&self) -> u64 {
        self.group_layers.builds()
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

    /// The current surface contents as tightly packed **premultiplied**
    /// RGBA8888 rows, written into `buffer` — the presentation readback.
    ///
    /// Both halves of the name are the difference from
    /// [`SkiaPainter::rgba_bytes`], and both matter once this runs every
    /// frame rather than once per golden (issue #603).
    ///
    /// - **Premultiplied** is the surface's own alpha type: the raster is
    ///   `raster_n32_premul`, so Skia converts the channel order and nothing
    ///   else. Asking for `Unpremul` makes Skia divide every channel by
    ///   alpha, which a host presenting onto an opaque window has to undo by
    ///   multiplying it back. That round trip is not only wasted work, it is
    ///   lossy in one direction: the integer division truncates, so every
    ///   semi-transparent pixel reached the window up to one code point
    ///   darker per channel than the value this surface holds.
    /// - **Into a caller's buffer**, because a presenter holds one across
    ///   frames and reuses it. `rgba_bytes` returns a fresh allocation, which
    ///   is 9.2 MB at 1920x1200 and is the right shape for a caller that runs
    ///   once.
    ///
    /// `buffer` is resized to `width * height * 4` bytes and every one of
    /// them is overwritten. When the extent has not changed since the last
    /// call, the resize is a no-op and nothing is allocated.
    pub fn read_premul_into(&mut self, buffer: &mut Vec<u8>) {
        let width = self.surface.width();
        let height = self.surface.height();
        let info = ImageInfo::new(
            (width, height),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = width as usize * 4;
        buffer.resize(row_bytes * height as usize, 0);
        let read = self.surface.read_pixels(&info, buffer, row_bytes, (0, 0));
        assert!(read, "CPU raster surfaces read back");
    }
}

/// The decoded image assets, keyed by [`ImageTable`] index, for as long as
/// the painter lives (issue #639).
///
/// # Why the painter and not `paint()`
///
/// Issue #101's cache was a `paint()` local, which is right for the golden
/// path where `paint()` runs once. A frame loop calls `paint()` sixty times a
/// second, and a local cache inflates every PNG again on each of them —
/// measured at 20.4 % of the `surfaces` frame
/// (`docs/technotes/frame-budget.md`). Keeping the decodes on
/// the painter is the other fix issue #101 named.
///
/// # Why the whole table is the key
///
/// An `ImageTable` index alone is **not** an identity. The host's scene reel
/// rebuilds the arena on a scene change and keeps one painter across it
/// (`demo/src/present.rs` owns `SkiaPainter`, `demo/src/shell.rs` replaces the
/// `Arena`), so two unrelated documents both have an asset at index 0. A cache
/// keyed on the index alone would draw the outgoing scene's picture in the
/// incoming scene's box, silently.
///
/// A content hash would be the honest key, and the document has one —
/// `dashbuf`'s `AssetEntry.hash`, BLAKE3-256 over the canonical payload. It
/// does not reach here: the loader consumes it to bind each entry to its
/// payload, and what crosses boundary B is [`dashpaint::ImageAsset`], which carries a
/// format and bytes and no hash. So the identity is established the only other
/// way available at this level — the painter keeps the table it decoded from
/// and compares the incoming one against it by value, byte for byte. Equal
/// tables hold the same payloads in the same order, so every decode still
/// stands; any difference clears the lot.
///
/// # Memory
///
/// **This grows with the document and nothing evicts from it.** Every distinct
/// asset the current table names is held until the table changes or the
/// painter is dropped, and the retained bytes are up to two copies of each
/// asset's **encoded** payload: one in `source`, one inside the [`Image`],
/// which `decode_image` builds over its own `Data::new_copy`. The `surfaces`
/// showcase scene measures 200 873 B of encoded assets and so retains
/// 401 746 B here.
///
/// "Up to two" since story #596: `source` is a clone of the incoming
/// [`ImageTable`], and cloning a table whose pool is a mapped region takes a
/// refcount rather than the bytes
/// (`docs/decisions/assets-borrow-from-the-mapping.md`). A mapped document
/// therefore retains one copy here — the one inside the [`Image`] — and the
/// figures above are the owned case, which is what the `surfaces` scene is.
///
/// The **decoded** pixels are not held here. `decode_image` returns a lazy
/// image, and Skia keeps the raster it produces in its own global resource
/// cache, which is limited (33 554 432 B on this build,
/// `skia_safe::graphics::resource_cache_total_bytes_limit`) and purges under
/// pressure. `surfaces` decodes to 1 062 016 B, about 3 % of that limit, so
/// its decodes survive between frames — a document whose images decode past
/// the limit would still be re-inflated, and this cache cannot prevent that.
///
/// There is no memory budget anywhere in this project to size any of it
/// against (issue #462), and issue #614 records the sibling risk on the
/// retained group composites. Both are open; neither is decided here, and this
/// cache deliberately invents neither a budget nor an eviction policy.
#[derive(Default)]
struct ImageCache {
    /// The table the entries in `decoded` were decoded from — the painter's
    /// own copy, so the comparison has something to compare against after the
    /// caller's document is gone.
    source: ImageTable,
    decoded: HashMap<u32, Image>,
}

impl ImageCache {
    /// Points the cache at this frame's table, dropping every decode if it is
    /// not the table the decodes came from.
    ///
    /// The comparison walks the encoded payloads and is therefore not free:
    /// 200 873 B for the `surfaces` showcase scene, once per frame, against
    /// the 2.22 ms of decoding it removes there. It is linear in the table's
    /// encoded size, so a document with far more asset bytes pays more for it.
    fn begin_frame(&mut self, images: &ImageTable) {
        if self.source == *images {
            return;
        }
        self.decoded.clear();
        self.source = images.clone();
    }

    /// The decoded asset at `index`, decoding it on the first request since
    /// the table last changed.
    ///
    /// Resolves against the kept table rather than the caller's, which is
    /// sound because [`ImageCache::begin_frame`] has already established that
    /// the two are equal byte for byte — that is the whole point of keeping a
    /// copy. `paint()` calls it first, before anything draws.
    ///
    /// # Panics
    ///
    /// Panics on an out-of-range index, exactly as [`ImageTable::resolve`]
    /// does — indices are validated upstream (P4).
    fn get(&mut self, index: u32) -> &Image {
        let source = &self.source;
        self.decoded
            .entry(index)
            .or_insert_with(|| decode_image(source.resolve(index)))
    }
}

impl Painter for SkiaPainter {
    /// The reference painter draws [`RectEntry::rotation`] (story #770): it
    /// turns the canvas about the rect's anchor around the node's own ink.
    ///
    /// The default is `false`, so declaring this is what separates a painter
    /// that rotates from one that would quietly draw the node upright.
    fn rotates(&self) -> bool {
        true
    }

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
        //
        // `incremental` records which arm ran, because the group cache asks
        // the same question one level up: on the full-refresh arm the dirty
        // set does not describe the difference between the two tables, so
        // no group composite may be reused either.
        let mut incremental = false;
        if self.mode == DirtyMode::Retained {
            match dirty {
                Some(indices) if self.retained.len() == rects.len() => {
                    incremental = true;
                    // An index past the end of the table names no rect, so
                    // it is skipped rather than indexed (debt #181) — see
                    // `DirtyMode::Retained`. The two lengths are equal in
                    // this arm, so one bound covers both reads.
                    for &i in indices {
                        let i = i as usize;
                        if let Some(entry) = rects.get(i) {
                            self.retained[i] = *entry;
                        }
                    }
                }
                _ => {
                    self.retained.clear();
                    self.retained.extend_from_slice(rects);
                }
            }
        }
        // `None` outside the incremental arm — and always in `Full` mode,
        // which retains nothing and so never stores a layer to reuse.
        self.group_layers
            .begin_frame(groups, if incremental { dirty } else { None });
        // Keeps this frame's decodes if this frame's table is the one they
        // came from, and drops all of them otherwise (issue #639). Called
        // before anything draws, so no draw can read a decode the incoming
        // table does not stand behind.
        self.images.begin_frame(images);

        // Disjoint field borrows: `retained`, `group_layers`, `images` and
        // `msdf` are read while `surface` is borrowed mutably.
        let source: &[RectEntry] = match self.mode {
            DirtyMode::Full => rects,
            DirtyMode::Retained => &self.retained,
        };
        let retain_groups = self.mode == DirtyMode::Retained;
        let (layer_width, layer_height) = (self.surface.width(), self.surface.height());
        let group_layers = &mut self.group_layers;
        let image_cache = &mut self.images;
        let msdf_cache = &mut self.msdf;

        let base_canvas = self.surface.canvas();
        base_canvas.clear(skia_safe::colors::TRANSPARENT);
        // The render-target group opacities (`masks-and-group-opacity.md`):
        // a group's rect range `[start, end)` composites offscreen and the
        // layer blends at `alpha`. Groups arrive in ascending `start` order
        // (DFS pre-order) and nest, so one pointer walks the starts and a
        // stack of pending layers closes them innermost first.
        //
        // Two realisations of the same composite, one per mode:
        //
        // - `Full` uses Skia's `save_layer_alpha`, which owns the layer and
        //   discards it at `restore`. Nothing is retained and nothing can
        //   be, which is exactly why that mode is correct without reading
        //   the dirty set — it is the reference and the goldens' renderer.
        // - `Retained` draws each group's subtree into its own offscreen
        //   surface, blends the snapshot, and keeps it (issue #278). A
        //   later frame whose dirty set leaves the group's range alone
        //   blends the same snapshot again and skips the subtree entirely.
        //
        // Only one of the two stacks is ever non-empty.
        let mut next_group = 0usize;
        let mut open_group_ends: Vec<u32> = Vec::new();
        let mut open_layers: Vec<OpenLayer> = Vec::new();
        // Baked-vector shapes (story B1): the MSDF resolve effect compiles
        // once (lazily — a vector-free scene pays nothing). The atlas PNG a
        // field samples is an ordinary `ImageTable` entry, so it comes from
        // `image_cache` along with every other asset — one cache, one key
        // space, one decode per asset (issues #101 and #639).
        let mut field_effect: Option<RuntimeEffect> = None;
        // The run-by-anchor index, built once, over the atlases and shader
        // the painter already holds — re-decoding and re-compiling only what
        // this frame changed (issue #644). A text-free scene builds nothing.
        let msdf = msdf_cache.frame(glyphs, source);
        // A manual index rather than `enumerate`, because a reused group
        // composite advances it past the whole subtree it covers.
        let mut i = 0usize;
        while i < source.len() {
            // Open every group that starts at this rect (at most one per
            // index — one opacity per node), and begin its offscreen
            // composite.
            let mut reused_through: Option<usize> = None;
            while next_group < groups.len() && groups[next_group].start == i as u32 {
                let group = groups[next_group];
                next_group += 1;
                if !retain_groups {
                    base_canvas.save_layer_alpha(None, u32::from(layer_alpha(group.alpha)));
                    open_group_ends.push(group.end);
                    continue;
                }
                if let Some(layer) = group_layers.reuse(&group) {
                    blend_layer(
                        target_canvas(base_canvas, &mut open_layers),
                        layer,
                        group.alpha,
                    );
                    reused_through = Some(group.end as usize);
                    break;
                }
                open_layers.push(OpenLayer {
                    surface: offscreen_layer(layer_width, layer_height),
                    group,
                });
            }
            if let Some(end) = reused_through {
                // The reused layer already holds everything its range drew,
                // nested groups and anchored glyph runs included, so the
                // whole range is skipped and every group inside it with it.
                //
                // A composite is only ever stored by the closing arm below,
                // which runs at rect `end - 1`, so a stored range always has
                // `end > start`. The loop therefore advances; the assertion
                // states that rather than leaving the reader to derive it.
                debug_assert!(end > i, "a reused composite must advance the walk");
                while next_group < groups.len() && (groups[next_group].start as usize) < end {
                    next_group += 1;
                }
                i = end;
                continue;
            }
            // The surface this rect draws into: the innermost open group's
            // offscreen in `Retained` mode, the base surface otherwise
            // (`Full` mode keeps its layers on the base canvas itself).
            let canvas = target_canvas(base_canvas, &mut open_layers);

            let rect = &source[i];
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
            // The node's rotation (story #770). It goes on *after* the clip
            // above and comes off before that clip is released, so it turns
            // this node's own ink — fill, stroke, both shadow passes, and the
            // glyph runs anchored to it — and leaves the ancestor clip region
            // where it is. That region was resolved by core in absolute space
            // and belongs to ancestors, which are not turning.
            //
            // The pivot is the anchor in canvas space: the anchor is a point
            // in the node's own frame, where `(0, 0)` is its top-left, so it
            // is the rect's origin plus the anchor
            // (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
            //
            // Radians in, degrees out: Skia's `rotate` takes degrees and turns
            // clockwise, which is this repository's y-down convention already,
            // so the angle needs a unit conversion and no sign flip.
            let rotated = rect.rotation != 0.0;
            if rotated {
                canvas.save();
                canvas.rotate(
                    rect.rotation.to_degrees(),
                    Some(Point::new(
                        rect.x + rect.rotation_anchor.x,
                        rect.y + rect.rotation_anchor.y,
                    )),
                );
            }
            let rrect = rounded_box(rect, &entry.corners);
            // How far the node's stroke pushes its rendered silhouette past
            // the fill box: an outside stroke by its full width, a center
            // stroke by half, an inside stroke not at all. A drop shadow
            // casts from that silhouette, not the bare fill box (P1).
            let outset = stroke_outset(paints.stroke(entry));
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
            // **Asked once, above the loop** (issue #1044). `ImageCache::get`
            // decodes on first request and `field_coverage` answers `None` for
            // a field with no area, so fetching first paid a full decode to
            // draw nothing; the lean painter never did, its gate sitting above
            // residency.
            //
            // `field_quad` rather than `VectorField::draws` since issue #1160:
            // the same question with the node origin in it, which the shared
            // predicate cannot see. A field the origin collapses used to pass
            // here and be refused inside the draw, below the fetch.
            //
            // Above the loop because neither the shape nor the quad varies per
            // blur — a node may carry several backdrops, each over the result of
            // the last — and because a field that covers nothing draws no
            // backdrop at all rather than one empty one per blur.
            let shape = paints.shape(entry);
            if shape.is_none_or(|field| field_quad(rect, field).is_some()) {
                for blur in paints
                    .blurs(entry)
                    .iter()
                    .filter(|blur| blur.kind == BlurKind::Backdrop)
                {
                    match shape {
                        // A baked-vector node's blur is confined to the field's
                        // coverage, not to its box — the hero's own frosted
                        // panel is exactly this shape, a VECTOR carrying
                        // `BACKGROUND_BLUR` (`crates/dashc/src/figma/mod.rs`),
                        // so blurring its whole box would frost a rectangle
                        // where the design has a rounded shape.
                        Some(field) => {
                            let effect = field_effect.get_or_insert_with(|| {
                                RuntimeEffect::make_for_shader(FIELD_MASK_SKSL, None)
                                    .expect("field-mask resolve SkSL compiles")
                            });
                            let atlas = image_cache.get(field.image);
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
                        None => {
                            draw_backdrop_blur_box(canvas, &rrect, blur.radius, rect.opacity);
                        }
                    }
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
            for shadow in paints
                .shadows(entry)
                .iter()
                .filter(|s| s.kind == ShadowKind::Drop)
            {
                draw_drop_shadow(canvas, rect, &entry.corners, outset, shadow, rect.opacity);
            }
            if let Some(field) = paints.shape(entry) {
                // A baked-vector shape (story B1): the fill is masked by the
                // field's coverage, not by the parametric box. The parametric
                // stroke and corners do not apply (a vector carries its
                // outline in the baked geometry).
                //
                // **Asked before the atlas is fetched** (issue #1044), and
                // inside this branch rather than on the `if let`: a masked
                // entry whose field covers nothing draws nothing, where the
                // `else` below would draw its parametric fill over the whole
                // box.
                //
                // `field_quad` rather than `VectorField::draws` since issue
                // #1160 — the same question with the node origin in it. See the
                // backdrop arm above.
                if field_quad(rect, field).is_some() {
                    let effect = field_effect.get_or_insert_with(|| {
                        RuntimeEffect::make_for_shader(FIELD_MASK_SKSL, None)
                            .expect("field-mask resolve SkSL compiles")
                    });
                    let atlas = image_cache.get(field.image);
                    draw_vector_field(canvas, rect, paints, entry.fill, field, atlas, effect);
                }
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
                // A stroke that lies over its own fill must not be dimmed
                // separately from it (debt #277). Folding `rect.opacity` into
                // each draw would composite a dimmed stroke over an
                // already-dimmed fill — alpha over alpha — where an Inside or
                // Center aligned stroke overlaps, while the composite path
                // flattens the node and dims once. Flatten this node the same
                // way: one layer at the group alpha, its contents drawn
                // opaque.
                //
                // Narrow on purpose. The layer costs an offscreen, so it is
                // opened only for the shape that actually disagrees — a node
                // carrying both a stroke and at least one fill, below full
                // opacity. Everything else keeps the folded path unchanged.
                let has_fill =
                    entry.fill != PaintKind::NONE || !paints.extra_fills(entry).is_empty();
                let flatten = has_fill && paints.stroke(entry).is_some() && rect.opacity != 1.0;
                let (draw_rect, layered) = if flatten {
                    canvas.save_layer_alpha_f(None, rect.opacity);
                    (
                        &RectEntry {
                            opacity: 1.0,
                            ..*rect
                        },
                        true,
                    )
                } else {
                    (rect, false)
                };
                // `draw_fill_kind` is called unconditionally: `paints.fill`
                // resolves `PaintKind::NONE` to `Fill::None`, whose arm draws
                // nothing, so a fill-less entry still paints exactly nothing
                // here — the same outcome the old `if let Some(kind)` guard
                // gave when `fill` was `Option::None`.
                draw_fill_kind(canvas, rrect, draw_rect, image_cache, paints, entry.fill);
                for kind in paints.extra_fills(entry) {
                    draw_fill_kind(canvas, rrect, draw_rect, image_cache, paints, *kind);
                }
                if let Some(stroke) = paints.stroke(entry) {
                    draw_stroke(canvas, &rrect, stroke, draw_rect.opacity);
                }
                if layered {
                    canvas.restore();
                }
            }
            // Inner shadows sit on top of the fill and stroke, clipped to
            // the node's own shape (story #45).
            for shadow in paints
                .shadows(entry)
                .iter()
                .filter(|s| s.kind == ShadowKind::Inner)
            {
                draw_inner_shadow(canvas, &rrect, rect, &entry.corners, shadow, rect.opacity);
            }
            // Every run anchored to this rect, in table order: after the
            // rect's own ink, still inside its clip save, and still inside
            // every group layer enclosing it — the group closes below. That
            // placement is the whole of issues #274 and #275.
            if let Some(msdf) = msdf.as_ref() {
                msdf.draw_anchored(canvas, glyphs, i as u32);
            }
            // The rotation comes off before the clip it was applied inside.
            if rotated {
                canvas.restore();
            }
            if clipped {
                canvas.restore();
            }

            // Close every group whose subtree ends after this rect,
            // innermost first (`end` values on the stack are non-increasing
            // from the top by the nesting). In `Retained` mode closing a
            // group snapshots its offscreen, blends the snapshot into
            // whatever encloses it, and stores it for the next frame.
            while open_group_ends.last() == Some(&(i as u32 + 1)) {
                base_canvas.restore();
                open_group_ends.pop();
            }
            while open_layers
                .last()
                .is_some_and(|layer| layer.group.end == i as u32 + 1)
            {
                let mut layer = open_layers.pop().expect("checked by the loop condition");
                let composite = layer.surface.image_snapshot();
                blend_layer(
                    target_canvas(base_canvas, &mut open_layers),
                    &composite,
                    layer.group.alpha,
                );
                group_layers.store(&layer.group, composite);
            }
            i += 1;
        }
    }
}

/// A render-target group whose subtree is still being drawn, in
/// [`DirtyMode::Retained`] (issue #278).
struct OpenLayer {
    /// The offscreen the group's rect range draws into, at full alpha.
    surface: skia_safe::Surface,
    group: GroupComposite,
}

/// The surface draws go to: the innermost open group's offscreen, or the
/// base surface when none is open.
fn target_canvas<'a>(base: &'a Canvas, open: &'a mut [OpenLayer]) -> &'a Canvas {
    match open.last_mut() {
        Some(layer) => layer.surface.canvas(),
        None => base,
    }
}

/// An offscreen for one group composite, at the full surface size.
///
/// Full size rather than the group's device bounds, so the snapshot blends
/// back at the origin with no transform and no resampling — a group's ink
/// reaches past its rect range through shadows and blurs, so a tight bound
/// would have to be derived from the effects rather than from the geometry,
/// and getting it wrong moves pixels. `raster_n32_premul` hands back
/// zero-initialised (fully transparent) pixels, which is the state
/// `save_layer` starts a layer in.
fn offscreen_layer(width: i32, height: i32) -> skia_safe::Surface {
    surfaces::raster_n32_premul((width, height)).expect("raster surface allocation")
}

/// Blends a group composite onto `canvas` at the group's alpha.
///
/// The device-aligned counterpart of what `save_layer_alpha`'s `restore`
/// does: one source-over draw of the layer at the origin, modulated by the
/// same 8-bit alpha. Nearest sampling and no antialiasing, because the draw
/// is a 1:1 pixel copy — there is no geometry to smooth and no scale to
/// filter.
fn blend_layer(canvas: &Canvas, composite: &Image, alpha: f32) {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(false);
    paint.set_alpha(layer_alpha(alpha));
    canvas.draw_image(composite, (0, 0), Some(&paint));
}

/// A group's alpha as the 8-bit value the composite blends at.
fn layer_alpha(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
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

/// The decoded MSDF glyph atlases and the compiled resolve shader, kept for
/// as long as the painter lives (issue #644).
///
/// # Why the painter and not `paint()`
///
/// Both were built inside `MsdfFrame::new`, whose doc comment called them
/// "the expensive parts" while placing them on the path a frame loop runs
/// sixty times a second. Every scene loads the same three atlases regardless
/// of what it draws, so a document carrying one glyph run re-inflated 226 508
/// encoded bytes per frame — about 2 ms
/// (`docs/technotes/frame-budget.md`).
///
/// This is the same defect issue #639 fixed for the [`ImageTable`], in a
/// different table: the atlases hang off the [`GlyphRunTable`] and are not
/// `ImageTable` entries, so that cache could not reach them.
///
/// # Why the atlas set is the key, and how it is compared
///
/// An [`AtlasIndex`](dashpaint::AtlasIndex) alone is not an identity, for the
/// reason [`ImageCache`] gives at length: the host's reel keeps one painter
/// across a scene change, so two unrelated documents both have an atlas at
/// index 0. The set is what identifies the decodes.
///
/// Unlike the image table, the set arrives behind an [`Arc`] that commit
/// shares rather than rebuilds, so the comparison has a fast path the sibling
/// cache does not: [`Arc::ptr_eq`] settles the steady-state frame in constant
/// time, and comparing contents is the fallback for an equal set rebuilt
/// behind a fresh allocation (the reel returning to a scene it has shown
/// before). Holding the handle is also what makes keeping it free —
/// `GlyphRunTable::atlas_set` records why both properties hold.
///
/// # Memory
///
/// **This grows with the atlas set and nothing evicts from it**, the same
/// posture — and the same open question, issue #462 — as [`ImageCache`]. The
/// retained bytes are one [`Arc`] clone of the set (shared, not copied) plus
/// one copy of each atlas's **encoded** payload inside its [`Image`], which
/// `decode_image` builds over its own `Data::new_copy`: 226 508 B for the
/// eight-atlas cascade the goldens harness and the showcase scenes both load.
/// The decoded texels are Skia's own resource cache's, as before.
#[derive(Default)]
struct MsdfCache {
    /// The atlas set the entries in `decoded` were decoded from.
    source: Arc<Vec<Atlas>>,
    decoded: Vec<Image>,
    /// Compiled on the first text-bearing frame and never invalidated — the
    /// shader is a constant, so no input can stale it. A scene that never
    /// carries a glyph run never compiles it, which is the posture the old
    /// lazy entry point had.
    effect: Option<RuntimeEffect>,
}

impl MsdfCache {
    /// This frame's MSDF setup, decoding and compiling only what this frame
    /// changed, or `None` for a text-free scene.
    ///
    /// Taking the whole frame's setup through one call is deliberate: the
    /// compiled shader and the decoded atlases are only in step with `glyphs`
    /// because the same `glyphs` selected them, and splitting this into a
    /// prepare call and a build call would put that invariant between two
    /// call sites where a later edit could separate them.
    ///
    /// # Panics
    ///
    /// Panics if a run's anchor is out of range for `rects`. A run and the
    /// rect table it is read against come from one commit, so a miss is a
    /// broken contract between crates (P4) — and under the z-interleave the
    /// alternative is worse than a wrong clip: a run bucketed at an index
    /// the loop never visits would simply never be drawn, which is exactly
    /// the silent drop P4 forbids. Checked here, once, rather than at the
    /// draw site, so an unreachable run cannot hide behind a rect table that
    /// merely happens to be short.
    fn frame(&mut self, glyphs: &GlyphRunTable, rects: &[RectEntry]) -> Option<MsdfFrame<'_>> {
        if glyphs.is_empty() {
            return None;
        }
        let mut by_anchor: HashMap<u32, Vec<usize>> = HashMap::new();
        for (index, run) in glyphs.runs().iter().enumerate() {
            assert!(
                (run.rect as usize) < rects.len(),
                "glyph run anchored at rect {} out of range ({} rects): a run and the rect table \
                 it is read against come from one commit (P4)",
                run.rect,
                rects.len()
            );
            by_anchor.entry(run.rect).or_default().push(index);
        }
        self.refresh(glyphs.atlas_set());
        Some(MsdfFrame {
            effect: self.effect.get_or_insert_with(|| {
                RuntimeEffect::make_for_shader(MSDF_SKSL, None).expect("MSDF resolve SkSL compiles")
            }),
            decoded: &self.decoded,
            by_anchor,
        })
    }

    /// Points the cache at `atlases`, re-decoding only when that is not the
    /// set the decodes came from.
    ///
    /// The handle is adopted in the equal-contents case too, so a reel
    /// returning to a scene pays the comparison once and takes the pointer
    /// fast path on every later frame of it.
    fn refresh(&mut self, atlases: &Arc<Vec<Atlas>>) {
        if Arc::ptr_eq(&self.source, atlases) {
            return;
        }
        if self.source != *atlases {
            self.decoded = atlases
                .iter()
                .map(|atlas| decode_image(atlas.image().as_ref()))
                .collect();
        }
        self.source = Arc::clone(atlases);
    }
}

/// One frame's MSDF drawing state: the run index this frame's runs produced,
/// over the atlases and shader the painter already holds.
///
/// `None` for a text-free scene, which then compiles nothing and decodes
/// nothing — the posture the old lazy entry point had.
struct MsdfFrame<'a> {
    effect: &'a RuntimeEffect,
    decoded: &'a [Image],
    /// Run indices by anchor rect. Built once so the rect loop can ask for
    /// "the runs at index i" without scanning the table per rect. This is
    /// the part that genuinely changes every frame — the runs do.
    by_anchor: HashMap<u32, Vec<usize>>,
}

impl MsdfFrame<'_> {
    /// Draws every run anchored at rect `index`, in table order, at the
    /// canvas's current clip and layer state.
    ///
    /// The caller places it, and that placement is the whole of issues #274
    /// and #275: called from inside the rect loop after the rect's own ink,
    /// the run lands inside that rect's clip save and inside every group
    /// layer still open around it, at the right z position.
    fn draw_anchored(&self, canvas: &Canvas, glyphs: &GlyphRunTable, index: u32) {
        let Some(anchored) = self.by_anchor.get(&index) else {
            return;
        };
        for &run_index in anchored {
            self.draw_run(canvas, glyphs, &glyphs.runs()[run_index]);
        }
    }

    fn draw_run(&self, canvas: &Canvas, glyphs: &GlyphRunTable, run: &GlyphRun) {
        let atlas = glyphs.atlas(run.atlas);
        let image = &self.decoded[run.atlas.0 as usize];
        // The MSDF field is a distance, not a color: linear filtering
        // interpolates the field (the point of MSDF's crisp edges);
        // nearest would step it. The surface carries no color space
        // (raster_n32_premul), so the channels sample raw — no sRGB
        // conversion mangling the distances.
        let sampling = SamplingOptions::new(FilterMode::Linear, MipmapMode::None);
        // The screen-pixel range scales the atlas distance range by the
        // ratio of render size to the size the atlas was baked at
        // (docs/design/atlas-pipeline.md).
        let px_range = atlas.distance_range_px() * run.size / f32::from(atlas.px_per_em());
        // Fold the run's free-path group alpha into the fill (story #44):
        // the MSDF resolve modulates coverage by `color.a`, so multiplying
        // the alpha dims the whole run.
        let color = dashpaint::Color {
            a: run.color.a * run.opacity,
            ..run.color
        };
        let uniforms = msdf_uniforms(self.effect, color, px_range);
        for quad in glyphs.quads(run) {
            let Some(g) = atlas.glyph(quad.glyph_id) else {
                // No quad for this glyph id — an empty outline (space) or
                // a glyph outside the atlas charset. Painting nothing is
                // correct for the former; the latter is a coverage gap the
                // build-time closure owns (P4), not a per-frame decision.
                continue;
            };
            draw_glyph_quad(
                canvas,
                image,
                atlas,
                g,
                quad,
                run,
                self.effect,
                &uniforms,
                sampling,
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
    let height = atlas.height() as f32;
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
    paints: &PaintTable,
    fill: PaintKind,
    field: &VectorField,
    atlas: &Image,
    effect: &RuntimeEffect,
) {
    // A shape with no fill has no ink to mask — a defensive guard; the
    // lowering always pairs a shape with a fill.
    if fill == PaintKind::NONE {
        return;
    }
    let Some((dest, coverage)) = field_coverage(rect, field, atlas, effect) else {
        return;
    };

    // Draw the fill into a layer, then multiply its alpha by the coverage.
    // The layer composites (SrcOver) over whatever is behind, so the masked
    // shape stacks correctly. `rect.opacity` is the free-path group alpha,
    // folded into the fill.
    //
    // The layer is bounded to `dest` (debt #358). Both draws inside it are
    // `draw_rect(dest)`, so nothing outside that quad can be written, and an
    // unbounded layer allocated the whole surface per shape — about 148 of
    // them on the hero. This is an allocation bound on a CPU reference
    // painter, not a correctness gate: with no backdrop filter on the rec,
    // Skia takes the bounds as the layer extent and clips to it, and the
    // clip is one every draw already respects.
    canvas.save_layer(&SaveLayerRec::default().bounds(&dest));
    match paints.fill(fill) {
        // Unreachable: the guard above already returned for
        // `PaintKind::NONE`, so `paints.fill` never resolves to this arm
        // here. Present because `Fill` matches exhaustively.
        Fill::None => {}
        Fill::Solid(color) => {
            let mut paint = solid_paint(color);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rect(dest, &paint);
        }
        Fill::Gradient(gradient) => {
            let mut paint = gradient_paint(gradient, rect);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rect(dest, &paint);
        }
        // An image-filled vector is not in the measured census (B1 widens by
        // exactly what is measured); it draws nothing rather than an unmasked
        // rectangle. Masking an image fill is additive later work.
        Fill::Image(_) => {}
    }
    let mut mask = skia_safe::Paint::default();
    mask.set_shader(coverage);
    mask.set_blend_mode(BlendMode::DstIn);
    mask.set_anti_alias(false);
    canvas.draw_rect(dest, &mask);
    canvas.restore();
}

/// The device quad a baked-vector shape occupies, or `None` for a shape that
/// draws nothing.
///
/// The padded field quad (`plane_bounds`) maps to device space at unit scale,
/// origin at the node box top-left, so `dest` is the plane bounds offset by
/// [`RectEntry::x`] and [`RectEntry::y`].
///
/// **`None` is two separate answers taken in order**, and the whole reason
/// this function exists is that they belong in one place (issue #1160). Until
/// then [`VectorField::draws`] was asked at both of [`Painter::paint`]'s two
/// masked arms — hoisted above `ImageCache::get` at issue #1044 — while the
/// device-quad guard stayed inside [`field_coverage`], below the fetch. So a
/// field the second guard refused still paid for its atlas. Both call sites
/// now ask `field_quad(rect, field).is_some()`, which is the same question
/// with the origin in it.
///
/// **Both #1044 hoists stay**, and this is what makes that consistent rather
/// than a reversal: the consolidation is the two guards living in one named
/// function, not the ask moving back down into the draw. The predicate is
/// still asked before the fetch, and the device-quad guard has joined it
/// there.
///
/// It runs twice for a field that draws — once at the call site and once
/// inside [`field_coverage`] — which is about eight arithmetic operations
/// against a `save_layer` and a shader construction on the same path.
///
/// # The shared predicate first (issues #1000 and #1144)
///
/// [`VectorField::draws`] decides whether a field draws at all, and both
/// painters call it. A zero atlas extent is a **legal** state rather than an
/// out-of-domain one, which is why `PaintTable::push_with` does not refuse it
/// (`docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md`).
///
/// **It is asked before the atlas is fetched** (issue #1044). Neither
/// [`field_coverage`] nor either draw fetches an atlas — all three are handed an
/// already-decoded `&Image` — so the ask belongs in the two arms of
/// [`Painter::paint`] that own the [`ImageCache`]. `ImageCache::get` decodes on
/// first request, and asking only below the fetch paid a full decode for a field
/// that paints nothing. [`field_coverage`] calls this function again anyway,
/// because that is what makes "a degenerate field draws nothing" this painter's
/// own answer rather than a property of two call sites.
///
/// **Called rather than restated**, which it was until issue #1144.
/// `dashscene-skia` does not depend on `dashscene-gpu`, so the two carried
/// byte-identical expressions kept in step by prose — and that convention failed
/// twice. Issue #1000 was the two painters disagreeing about which fields draw:
/// without the predicate [`field_coverage`] returned `Some` for a field with no
/// atlas extent and `sx` became an infinity. On the masked-fill path that resolved to
/// zero coverage and drew nothing anyway, so the two agreed by accident. On the
/// **backdrop-blur** path they did not — `draw_backdrop_blur_field` clips to the
/// coverage shader and opens a backdrop layer, and with an infinity in the
/// shader's local matrix the layer composited over the backdrop and **erased
/// it**, measured at 32 of 64 pixels on a bar-under-frosted-node scene at 8x8.
///
/// Issue #1034 was the restated predicate being wrong in **both**: ordering the
/// plane bounds admits an infinity, and testing each bound for finiteness still
/// admits two large bounds whose difference overflows. Each fix meant editing
/// two copies and saying so in a comment. Both painters depend on `dashpaint`
/// and both take a `&VectorField`, so the predicate lives there now and the
/// agreement is structural.
///
/// Whether such a field should instead be *refused* at
/// `PaintTable::push_with`, beside the `distance_range` check, is the half that
/// stays open on #1034: nothing produces a non-finite quad, so it arrives from
/// an authored or corrupted `.dsb` rather than from the importer, which is the
/// out-of-domain shape that seam exists for.
///
/// # Then this painter's own device quad
///
/// `dest` is the plane bounds offset by the node origin, and the predicate
/// above cannot see it: with a large `rect.x` a positive `right - left` can
/// cancel to a zero-width device quad, which [`field_coverage`] would then
/// divide by to build its texel-to-device scale.
///
/// **`dashscene-gpu` writes the same quantity, and this section said it had no
/// such case until issue #1185.** `gpu_shape` does derive `px_range` from
/// `right - left` directly, with no origin in it — but the masked-fill pipeline
/// builds an origin-offset quad in `paint.wgsl`'s vertex stage
/// (`lo = inst.bounds.xy + field.plane.xy`,
/// `hi = inst.bounds.xy + field.plane.zw`, `quad = vec4f(lo, hi - lo)`) and
/// `msdf_sample` divides by `quad.zw`. Since #1185 that stage refuses a quad
/// with no area through `params2.w`, so both painters now carry a local floor
/// and this one is not alone.
///
/// **What that pipeline does not reproduce is the cancellation**, at least on
/// the one adapter this workspace can measure. On Metal, the compiled shader
/// evaluates the pair to `plane.zw - plane.xy` — the origin folded out of the
/// subtraction — where an `f32` evaluation of what is written would give zero.
/// So the lean painter's guard is reachable through its **text** arm, whose
/// zero-area quad comes from the CPU, and not through the masked arm the issue
/// was filed for. Issue #1195 carries the measurement and what would settle it.
/// None of that reaches this function: Skia computes `dest` on the CPU, in
/// Rust, where the cancellation is exactly what happens.
///
/// # Which half of the shared predicate decides, and which is restatement
///
/// **The `atlas_rect` terms and the finite-extent terms decide; the
/// positive-extent terms do not.** Measured by deleting each in turn and running
/// this crate's suite: deleting either atlas term, or either `is_finite`, fails
/// `a_coverage_mask_with_no_area_draws_nothing_through_the_backdrop`, and
/// deleting `width > 0.0`, `height > 0.0`, or both together fails nothing.
///
/// No fixture can isolate **the two positive-extent terms**, and the reason is a
/// one-way implication rather than an equivalence. Adding the node origin to
/// both ends of an interval cannot make a non-positive width positive, so
/// wherever `width > 0.0` is false the guard below is also true and refuses the
/// field on its own. The converse does not hold, and that is what the guard
/// below exists for: a large `rect.x` can collapse a positive `right - left` to
/// a zero-width device quad, which the predicate accepts and the guard refuses.
///
/// A non-finite extent is what breaks that symmetry, and it is why those terms
/// pin where the positive ones cannot: `inf - 0.0` is still positive, so the
/// device-quad guard lets it through and only the predicate stops it.
///
/// So neither check is redundant, and the measurement says something narrower
/// than that they are: **on the fixtures this crate can build**, every
/// positive-extent case is refused twice over, so no test tells which refusal
/// ran.
///
fn field_quad(rect: &RectEntry, field: &VectorField) -> Option<Rect> {
    if !field.draws() {
        return None;
    }
    let [left, top, right, bottom] = field.plane_bounds;
    let dest = Rect::from_ltrb(rect.x + left, rect.y + top, rect.x + right, rect.y + bottom);
    // Spelled as a negated positive rather than `<= 0.0`, so a NaN is refused
    // rather than admitted — `NaN <= 0.0` is false and waved the quad through.
    // The quad's **extent** reaching here is finite, because
    // `VectorField::draws` requires that of the two differences rather than of
    // the four bounds — testing the bounds individually still admits two large
    // ones whose difference overflows (issue #1034). Note what that does and
    // does not give: the extent is finite, and an individual bound may still be
    // enormous.
    //
    // `dest` adds the **node origin** and nothing refuses a non-finite one:
    // `check_rect_extent` covers a rect's `w` and `h` and not its `x` and `y`,
    // and it belongs to `validate_scene`, which has no production caller. Two
    // origins reach this line with a width that is not positive, and they are
    // different mechanisms:
    //
    // - A **NaN** origin makes both ends NaN and their difference NaN.
    // - A **large finite** origin makes them **cancel**. Not overflow, which is
    //   what four documents said until issue #1160 measured it:
    //   `3.0e38f32 + 8.0 == 3.0e38` is true and `f32::MAX + 8.0 == f32::MAX` is
    //   true, so neither end reaches an infinity and there is no `inf - inf`.
    //   What happens is that the field extent falls below one ulp of the origin,
    //   so both ends round to the same float and the width is exactly zero.
    //
    // The second is a **ratio** of the two operands rather than a property of
    // either — an origin of `1e8` against an 8-unit field admits, and an origin
    // of `65536.0` against a 0.001-unit field collapses — which bounds what
    // issue #1048 can do upstream. A finiteness rule over `RectEntry::x` and
    // `y` covers the NaN route and not this one, so this floor stays necessary
    // after #1048 lands.
    //
    // **This changes no picture, and the claim is narrower than it looks.**
    // Measured with the admitting spelling: a NaN device quad reaches the
    // resolve shader and Skia draws nothing regardless, on the masked fill and
    // the backdrop blur alike. So it is not the erasure issue #1000 closed —
    // that needed a *finite* quad and an infinite scale. What this spelling buys
    // is that "a degenerate quad draws nothing", which this painter's contract
    // states, is decided here rather than resting on Skia's undocumented
    // handling of a NaN rectangle. It is the idiom `PaintTable::push_with` uses
    // two seams away, for the same reason.
    if !(dest.width() > 0.0 && dest.height() > 0.0) {
        return None;
    }
    Some(dest)
}

/// The device quad a baked-vector shape occupies, and the shader that
/// resolves its field into a coverage mask over that quad.
///
/// Both draws that mask by a baked shape use it: the masked fill
/// ([`draw_vector_field`]) and the backdrop blur
/// ([`draw_backdrop_blur_field`], story #393). Stated once so the two cannot
/// disagree about where the shape is or how sharp its edge resolves.
///
/// `None` for a field that draws nothing, which is [`field_quad`]'s answer and
/// carries that function's whole argument about the two guards behind it. What
/// is left here is the mapping: the field's atlas sub-rect onto that quad, and
/// the screen-pixel range its edge resolves over.
///
fn field_coverage(
    rect: &RectEntry,
    field: &VectorField,
    atlas: &Image,
    effect: &RuntimeEffect,
) -> Option<(Rect, Shader)> {
    // Both refusals, in one place and already asked at both call sites
    // (issue #1160). Asking again here is what keeps "a degenerate field draws
    // nothing" this function's own answer rather than a property of its
    // callers.
    let dest = field_quad(rect, field)?;

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

/// Sigma per unit of blur radius — Figma's measured constant.
///
/// The value's one home is `dashpaint`, on boundary B, since story #584 gave
/// the lean painter the same mapping to apply: two painters restating a
/// measured number is exactly the drift the scale-mode and gradient-kind pins
/// exist to catch. This alias keeps the name the measurements below are written
/// against; the mapping it belongs to is documented on [`blur_sigma`].
const FIGMA_BLUR_SIGMA_PER_RADIUS: f32 = dashpaint::BLUR_SIGMA_PER_RADIUS;

/// The Gaussian sigma a blur radius maps to. Skia takes a sigma, not a
/// radius, so a painter has to choose the mapping.
///
/// **This is Figma's mapping, measured against Figma's own renders, not the
/// CSS/browser `radius / 2` convention** (issue #412,
/// `docs/decisions/blur-sigma-is-figmas-mapping.md`). Every frame this
/// project measures blur on fits better here than at `radius / 2`, by mean
/// per-pixel delta against Figma's `GET /images` export:
///
/// | frame                        | at 0.4375 | at 0.5 |
/// | ---------------------------- | --------- | ------ |
/// | `v08-drop-shadow`, outside   | 0.1016    | 0.7264 |
/// | `v08-inner-shadow`, inside   | 0.6937    | 3.6956 |
/// | `backdrop-blur`, panel       | 1.187     | 2.704  |
///
/// Stated once because a shadow's blur (story #45) and a backdrop blur
/// (story #393) are the same mapping, and two copies of it could drift
/// apart. That single-mapping claim is now measured rather than assumed:
/// the shadow frames and the backdrop frame agree on the same value, so
/// nothing here needs to split per effect.
///
/// Skia quantises sigma into integer box-blur windows, so this constant is
/// only as precise as the radii it was fitted against. At the shadow
/// fixtures' radius 6 every scale in `[0.40, 0.475]` renders identically;
/// the backdrop fixture's radius 16 is what resolves the value, where
/// 0.4375 is a distinct minimum against 0.40 (1.622) and 0.45 (1.930).
/// Two radii, both self-authored — a third would need a new fixture.
fn blur_sigma(radius: f32) -> f32 {
    radius * FIGMA_BLUR_SIGMA_PER_RADIUS
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

/// The paint a backdrop-blur layer composites through, carrying the rect's
/// free-path group alpha (`docs/decisions/masks-and-group-opacity.md`) exactly
/// as every other draw in this painter does.
///
/// **At `opacity = 1.0` the layer replaces the region rather than compositing
/// over it** (`BlendMode::Src`), which is what a backdrop filter means. The
/// default `SrcOver` was indistinguishable from replacement for an opaque
/// backdrop — an opaque blurred copy hides the original — and wrong for a
/// partially transparent one, because the blurred copy is then also partially
/// transparent and the sharp original showed through beneath it. The blur's
/// alpha falloff was lost and its alpha edge stayed hard (debt #405). RGB was
/// correct throughout; only alpha was affected.
///
/// Below `1.0` the copy still composites over the sharp original, so a dimmed
/// node frosts proportionally. That is deliberate and is the CSS model this
/// project follows: the filtered backdrop is composited at the element's alpha
/// over the unfiltered one, of which replacement is the `alpha = 1` case.
/// `docs/decisions/backdrop-blur-is-core-vocabulary.md` settles the
/// surrounding question — an opacity below 1 makes a group a backdrop root, so
/// the isolating case is handled by isolation rather than by this blend mode.
///
/// Both blur paths share this paint. `Src` replaces whatever the draw's
/// coverage reaches, so each path is responsible for making that coverage the
/// region it means to replace — a rounded-rect clip on the parametric path, a
/// clip shader carrying the field coverage on the baked-vector one.
fn backdrop_layer_paint(opacity: f32) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    if opacity == 1.0 {
        paint.set_blend_mode(BlendMode::Src);
    } else {
        apply_opacity(&mut paint, opacity);
    }
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
/// A rounded-rect clip cannot express a baked outline, so the field coverage
/// enters as a **clip shader** instead. Skia carries a clip shader as draw
/// coverage rather than as a mask over the drawn pixels, and it applies
/// coverage to a blend as `lerp(dst, blend(src, dst), coverage)`. With `Src`
/// on the layer paint that resolves to `lerp(dst, blurred, coverage)`: the
/// covered region is replaced, the uncovered region keeps the original
/// backdrop exactly, and the shape's antialiased edge crossfades between them.
/// The parametric path's rounded-rect clip does the same job by the same
/// mechanism, so the two paths now have the same structure and differ only in
/// which clip carries the coverage.
///
/// **Masking inside the layer instead is what made this path composite rather
/// than replace** (debt #503). `BlendMode::DstIn` against the coverage shader
/// clears the layer outside the outline, but the restore then still has to
/// composite what is left, and `SrcOver` leaves the destination weighted
/// `1 - blurred_alpha × coverage` where a replacement weights it
/// `1 - coverage`. Those agree only where the backdrop is opaque, which every
/// scene measured before debt #503 was. Promoting the mask to a clip is what
/// moves the coverage into the term that the blend already lerps by.
///
/// **`BlendMode::Src` alone does not transfer from the box path.** `Src`
/// replaces everything the coverage reaches, and the rect clip's coverage is
/// the field's padded bounding box, so setting it without the clip shader
/// writes transparent around the shape and erases the real backdrop there —
/// measured at 22.6 % differing on the `vector-backdrop-blur` import frame
/// against a 2 % band. `Src` is sound only where the coverage equals the
/// region being replaced, which is precisely what the clip shader arranges.
///
/// **The rect clip is now an allocation bound, not a correctness gate.** It
/// used to be one: while the coverage was applied as a `DstIn` mask inside the
/// layer, every layer pixel that mask did not cover kept a full-opacity
/// blurred backdrop and composited on restore, so without the rect one
/// baked-vector node blurred the whole frame — the leak
/// `the_baked_vector_blur_is_confined_to_its_quad` was written for. The clip
/// shader prevents that leak by construction, because `lerp(dst, blurred, 0)`
/// is `dst`, so removing the rect now renders every scene in that test
/// identically, whole-canvas, rather than turning it red. It stays because
/// `SaveLayerRec::bounds` is a hint to Skia rather than a guarantee — with a
/// backdrop filter the layer is allocated over the device clip, and a clip
/// shader does not tighten device clip bounds — so the rect is what keeps a
/// frosted node from allocating and blurring a frame-sized layer. That is a
/// cost bound, and no pixel test can pin it; the same standing as the
/// `has_fill` guard in `draw_rects`.
///
/// Neither clip truncates the blur's input: Skia reads the halo the kernel
/// needs from outside both, which `the_baked_vector_blur_reads_past_its_quad`
/// pins for the rect and now for the clip shader with it — the property the
/// box path relies on holds for a shader clip too, which is what made the
/// nested-layer alternative unnecessary.
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
    canvas.clip_shader(coverage, ClipOp::Intersect);
    // No `bounds` on the rec: Skia discards it whenever a backdrop filter is
    // set (`SkCanvas::internalSaveLayer` takes the user bounds as a hard
    // layer extent only on the trivial-restore path, which a backdrop
    // disables), so passing one would state a constraint that does not hold
    // and invite the clip above being removed as a duplicate.
    canvas.save_layer(&SaveLayerRec::default().backdrop(&filter).paint(&layer));
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
///
/// `image_cache` decodes each `ImageTable` index at most once (issue #101 for
/// rects sharing one index, issue #639 for frames sharing one table), and it
/// carries the table the decodes came from, so nothing here needs one.
fn draw_fill_kind(
    canvas: &Canvas,
    rrect: RRect,
    rect: &RectEntry,
    image_cache: &mut ImageCache,
    paints: &PaintTable,
    kind: PaintKind,
) {
    match paints.fill(kind) {
        // The fill-less node (story #578): `entry.fill` reaches here as
        // `PaintKind::NONE` on every call the main loop makes unconditionally,
        // and this arm is where that draws exactly nothing, the same outcome
        // the old `if let Some(kind) = &entry.fill` guard gave.
        Fill::None => {}
        Fill::Solid(color) => {
            let mut paint = solid_paint(color);
            paint.set_anti_alias(true);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rrect(rrect, &paint);
        }
        Fill::Gradient(gradient) => {
            let mut paint = gradient_paint(gradient, rect);
            paint.set_anti_alias(true);
            apply_opacity(&mut paint, rect.opacity);
            canvas.draw_rrect(rrect, &paint);
        }
        Fill::Image(image) => {
            let decoded = image_cache.get(image.image);
            draw_image_fill(
                canvas,
                &rrect,
                rect,
                decoded,
                image.scale_mode,
                &image.transform,
                image.tile_scale,
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
fn gradient_paint(gradient: GradientView<'_>, rect: &RectEntry) -> skia_safe::Paint {
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

    let frame = gradient_frame(gradient.gradient, rect);
    if frame.invert().is_none() {
        return solid_paint(first_stop.color);
    }

    let colors: Vec<Color4f> = gradient.stops.iter().map(|s| to_color4f(s.color)).collect();
    let positions: Vec<f32> = gradient.stops.iter().map(|s| s.offset).collect();

    let shader = match gradient.gradient.kind {
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

#[cfg(test)]
thread_local! {
    /// Test-only decode counter (issue #101): `paint()`'s image-fill cache
    /// should call `decode_image` at most once per `ImageTable` index, and
    /// this is how `tests::paint_decodes_a_shared_image_table_index_once_per_call`
    /// observes that without reaching into the cache itself. Compiled out
    /// of every non-test build — this is not a runtime metric.
    ///
    /// Thread-local rather than a shared global counter: the default
    /// `#[test]` harness runs each test function on its own thread, and
    /// this crate's other `decode_image` callers
    /// (`decode_image_handles_a_real_jpeg` and `..._static_gif`) would
    /// otherwise bump a counter this test did not mean to observe, flakily,
    /// whenever tests happen to run concurrently.
    static DECODE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Decodes an encoded image asset with the Skia build's own codec —
/// `docs/decisions/image-assets-cross-boundary-b.md` names this each
/// painter's own machinery, and this build's `skia-safe` carries Png,
/// Jpeg, and Gif decode already (proven for Jpeg and Gif by this crate's
/// `tests::decode` module — a trimmed Skia build without codecs would need
/// the pure-Rust fallback `docs/decisions/downloaded-raster-needs-no-vector-engine.md`
/// describes, which the reference painter does not run).
fn decode_image(asset: dashpaint::ImageRef<'_>) -> Image {
    #[cfg(test)]
    DECODE_CALLS.with(|c| c.set(c.get() + 1));
    // This painter declares only the source-encoded formats — it takes
    // `Painter::samples`'s default — so a baked payload reaching here means the
    // binding ignored the declaration rather than that this function is
    // incomplete. Named rather than decoded as if it were a container: Skia
    // would report "unknown image format" for an ASTC block payload, which
    // says nothing about why it arrived.
    assert!(
        asset.format.is_encoded(),
        "this painter was handed a {:?} payload, which it declared it cannot sample \
         (Painter::samples); a baked derivation is selected for a painter that can upload it",
        asset.format
    );
    images::deferred_from_encoded_data(Data::new_copy(asset.bytes), None)
        .expect("image asset decodes (validated upstream, P4)")
}

/// Draws an image fill clipped to the entry's (rounded) box. `image` is
/// already decoded — the caller (`draw_fill_kind`) owns the per-`paint()`
/// decode cache, so this function decodes nothing (issue #101).
#[allow(clippy::too_many_arguments)]
fn draw_image_fill(
    canvas: &Canvas,
    rrect: &RRect,
    rect: &RectEntry,
    image: &Image,
    scale_mode: ScaleMode,
    transform: &dashpaint::Mat23,
    tile_scale: f32,
    opacity: f32,
) {
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
            canvas.draw_image_rect_with_sampling_options(image, None, dest, sampling, &paint);
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
            // uv_image = T · uv_box (normalized spaces; T is Mat23::IDENTITY
            // for a fill that crops nothing). The shader's local matrix maps
            // image pixel space to device space: box ∘ T⁻¹ ∘ image-normalize.
            let t = *transform;
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
    //! a stub. Also proves `paint()`'s per-`ImageTable`-index decode cache
    //! (issue #101), via `DECODE_CALLS`.

    use super::*;
    // Test-only: the painter itself no longer names the owning type, since a
    // table hands out `ImageRef`. The tests still build assets to put in one.
    use dashpaint::{ImageAsset, Vec2};

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
        let image = decode_image(asset.as_ref());
        assert_eq!((image.width(), image.height()), (2, 2));
    }

    #[test]
    fn decode_image_handles_a_real_static_gif() {
        let asset = ImageAsset {
            format: dashpaint::ImageFormat::Gif,
            bytes: GIF_2X2.to_vec(),
        };
        let image = decode_image(asset.as_ref());
        assert_eq!((image.width(), image.height()), (2, 2));
    }

    /// A minimal real (decodable) 1x1 PNG of `color`, rendered through the
    /// painter itself rather than a committed binary fixture — the same
    /// approach `goldens/tooling/tests/common::checker_asset` uses for its 4x4
    /// checker.
    fn one_pixel_png(color: dashpaint::Color) -> Vec<u8> {
        let mut painter = SkiaPainter::new(1, 1);
        let mut paints = PaintTable::new();
        let solid = paints.push_solid(color);
        let rects = [RectEntry {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            paint: solid,
            clip: dashpaint::ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }];
        painter.paint(
            &rects,
            &paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &[],
            &GlyphRunTable::new(),
            None,
        );
        painter.png_bytes()
    }

    /// Two rects filled from the same `ImageTable` index must decode that
    /// index's asset exactly once per `paint()` call, not once per rect
    /// (issue #101). The scene reduces the v0.3 golden's own shape — two
    /// paint entries sharing one checker asset — to a single shared pixel,
    /// so the test needs no external fixture.
    ///
    /// Falsifiable: reverting `draw_fill_kind`'s `image_cache` lookup back
    /// to a bare `decode_image(images.resolve(*image))` call per fill makes
    /// `DECODE_CALLS` read 2 here, and the assertion below fails.
    #[test]
    fn paint_decodes_a_shared_image_table_index_once_per_call() {
        let mut images = ImageTable::new();
        let asset = images.push(ImageAsset {
            format: dashpaint::ImageFormat::Png,
            bytes: one_pixel_png(GREY),
        });

        let mut paints = PaintTable::new();
        let image_kind = paints.intern_fill(&dashpaint::FillSpec::Image(dashpaint::ImageFill {
            image: asset,
            scale_mode: ScaleMode::Fill,
            transform: dashpaint::Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        let image_fill = || dashpaint::PaintEntry {
            fill: image_kind,
            ..dashpaint::PaintEntry::default()
        };
        let paint_a = paints.push(image_fill());
        let paint_b = paints.push(image_fill());

        let rect = |x: f32, paint| RectEntry {
            x,
            y: 0.0,
            w: 2.0,
            h: 2.0,
            paint,
            clip: dashpaint::ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        };
        let rects = [rect(0.0, paint_a), rect(2.0, paint_b)];

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(4, 2);
        painter.paint(
            &rects,
            &paints,
            &images,
            &ClipTable::new(),
            &[],
            &GlyphRunTable::new(),
            None,
        );

        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            1,
            "two rects sharing one ImageTable index must decode its asset exactly once per \
             paint() call, not once per rect"
        );
    }

    /// The colour the pre-issue-#639 fixture used. Named only so the two
    /// tests below can pick colours that are obviously not it.
    const GREY: dashpaint::Color = dashpaint::Color {
        r: 0.4,
        g: 0.5,
        b: 0.6,
        a: 1.0,
    };
    const RED: dashpaint::Color = dashpaint::Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    const BLUE: dashpaint::Color = dashpaint::Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    /// A one-asset table holding a 1x1 PNG of `color`.
    fn one_image_table(color: dashpaint::Color) -> ImageTable {
        let mut images = ImageTable::new();
        images.push(ImageAsset {
            format: dashpaint::ImageFormat::Png,
            bytes: one_pixel_png(color),
        });
        images
    }

    /// One rect filling a 2x2 surface from `ImageTable` index 0.
    fn one_image_rect() -> (PaintTable, [RectEntry; 1]) {
        let mut paints = PaintTable::new();
        let image_kind = paints.intern_fill(&dashpaint::FillSpec::Image(dashpaint::ImageFill {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: dashpaint::Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        let paint = paints.push(dashpaint::PaintEntry {
            fill: image_kind,
            ..dashpaint::PaintEntry::default()
        });
        let rects = [RectEntry {
            x: 0.0,
            y: 0.0,
            w: 2.0,
            h: 2.0,
            paint,
            clip: dashpaint::ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }];
        (paints, rects)
    }

    /// Paints `rects` against `images` and returns the top-left pixel as
    /// unpremultiplied RGBA bytes.
    fn paint_and_read(
        painter: &mut SkiaPainter,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
    ) -> [u8; 4] {
        painter.paint(
            rects,
            paints,
            images,
            &ClipTable::new(),
            &[],
            &GlyphRunTable::new(),
            None,
        );
        let pixels = painter.rgba_bytes();
        [pixels[0], pixels[1], pixels[2], pixels[3]]
    }

    /// Sixty frames of one scene decode its asset once, not sixty times
    /// (issue #639). The last frame paints an *independently built* table
    /// holding the same bytes, because the host rebuilds its arena on a
    /// resize and the reel returns to a scene it has shown before — an
    /// equal table must be recognised as the same table, or the cache would
    /// be thrown away on every rebuild.
    ///
    /// Falsifiable: moving the cache back into `paint()` — a
    /// `HashMap<u32, Image>` declared as a local instead of
    /// `self.images` — makes `DECODE_CALLS` read 61 here.
    #[test]
    fn paint_decodes_an_image_table_index_once_across_frames() {
        let images = one_image_table(GREY);
        let (paints, rects) = one_image_rect();

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(2, 2);
        for _ in 0..60 {
            paint_and_read(&mut painter, &rects, &paints, &images);
        }
        let rebuilt = one_image_table(GREY);
        assert_eq!(rebuilt, images, "the rebuilt table holds the same bytes");
        paint_and_read(&mut painter, &rects, &paints, &rebuilt);

        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            1,
            "sixty-one paints of one asset must decode it once, not once per paint()"
        );
    }

    /// A painter that outlives the document it painted must not draw the old
    /// document's image (issue #639). This is the shape the host's scene reel
    /// has: `demo/src/shell.rs` replaces the `Arena` on a scene change while
    /// `demo/src/present.rs` keeps one `SkiaPainter`, so two unrelated
    /// documents both hand it an asset at index 0.
    ///
    /// Falsifiable: deleting `self.images.begin_frame(images)` from `paint()`
    /// makes the second read return the first document's red instead of the
    /// second's blue — a wrong picture with nothing else to notice it,
    /// which is why this is asserted on the pixel and not only on the
    /// decode count.
    #[test]
    fn a_different_asset_table_is_not_the_table_that_was_decoded() {
        let (paints, rects) = one_image_rect();
        let first = one_image_table(RED);
        let second = one_image_table(BLUE);
        assert_ne!(first, second, "the two documents hold different bytes");

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(2, 2);
        let drawn_first = paint_and_read(&mut painter, &rects, &paints, &first);
        let drawn_second = paint_and_read(&mut painter, &rects, &paints, &second);

        assert_eq!(
            drawn_first,
            [255, 0, 0, 255],
            "the first document's image fill draws the first document's image"
        );
        assert_eq!(
            drawn_second,
            [0, 0, 255, 255],
            "a second document's image fill must draw the second document's image at index 0, \
             not the first document's"
        );
        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            2,
            "each of the two documents decodes its own asset once"
        );
    }

    /// The white the MSDF resolve reads as full coverage: the shader takes
    /// the median of the three distance channels, so `(1, 1, 1)` is a signed
    /// distance of 1 and `clamp(px_range * 0.5 + 0.5, 0, 1)` saturates. A run
    /// over this atlas paints its own fill colour.
    const ATLAS_INK: dashpaint::Color = dashpaint::Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    /// The black the same resolve reads as zero coverage — distance 0 gives
    /// `clamp(-px_range * 0.5 + 0.5, 0, 1)`, which is 0 for any `px_range`
    /// of 1 or more. A run over this atlas paints nothing, which is how the
    /// test below tells one atlas set from the other by pixel.
    const ATLAS_VOID: dashpaint::Color = dashpaint::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// A one-atlas, one-run table whose single glyph covers the whole 2x2
    /// surface, sampling a 1x1 atlas of `atlas_color` and filling with
    /// `ink`.
    fn one_atlas_table(atlas_color: dashpaint::Color, ink: dashpaint::Color) -> GlyphRunTable {
        let mut glyphs = GlyphRunTable::new();
        let atlas = glyphs.push_atlas(
            Atlas::new(
                ImageAsset {
                    format: dashpaint::ImageFormat::Png,
                    bytes: one_pixel_png(atlas_color),
                },
                1,
                1,
                1,
                2.0,
                vec![dashpaint::AtlasGlyph {
                    glyph_id: 0,
                    // y-up, baseline origin: a 2-em quad sitting above the
                    // baseline, which the run's 2.0 size places over the surface.
                    plane_em: [0.0, 0.0, 1.0, 1.0],
                    atlas_px: [0.0, 0.0, 1.0, 1.0],
                }],
            )
            .expect("a test atlas states a non-zero px_per_em"),
        );
        glyphs.push_run(
            GlyphRun {
                rect: 0,
                atlas,
                size: 2.0,
                color: ink,
                glyphs: dashpaint::GlyphRange::UNASSIGNED,
                opacity: 1.0,
            },
            &[dashpaint::GlyphQuad {
                glyph_id: 0,
                x: 0.0,
                y: 2.0,
            }],
        );
        glyphs
    }

    /// One rect anchoring a glyph run, drawing no ink of its own.
    fn one_text_rect() -> (PaintTable, [RectEntry; 1]) {
        let mut paints = PaintTable::new();
        let paint = paints.push(dashpaint::PaintEntry::default());
        let rects = [RectEntry {
            x: 0.0,
            y: 0.0,
            w: 2.0,
            h: 2.0,
            paint,
            clip: dashpaint::ClipIndex::UNCLIPPED,
            opacity: 1.0,
            rotation: 0.0,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }];
        (paints, rects)
    }

    /// Paints `glyphs` and returns the top-left pixel as unpremultiplied
    /// RGBA bytes.
    fn paint_text_and_read(
        painter: &mut SkiaPainter,
        rects: &[RectEntry],
        paints: &PaintTable,
        glyphs: &GlyphRunTable,
    ) -> [u8; 4] {
        painter.paint(
            rects,
            paints,
            &ImageTable::new(),
            &ClipTable::new(),
            &[],
            glyphs,
            None,
        );
        let pixels = painter.rgba_bytes();
        [pixels[0], pixels[1], pixels[2], pixels[3]]
    }

    /// Sixty frames of one text scene decode its atlas once, not sixty times
    /// (issue #644). The last frame paints an *independently built* table
    /// holding an equal atlas set behind a fresh allocation, which is what
    /// the host's reel produces when it returns to a scene it has shown
    /// before — the pointer fast path misses there, and comparing contents
    /// must still recognise it.
    ///
    /// Falsifiable: moving the decode back into the per-frame path — a
    /// `decoded` built inside `MsdfCache::frame` instead of behind
    /// `refresh`'s identity check — makes `DECODE_CALLS` read 61 here.
    #[test]
    fn paint_decodes_a_glyph_atlas_once_across_frames() {
        let glyphs = one_atlas_table(ATLAS_INK, RED);
        let (paints, rects) = one_text_rect();

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(2, 2);
        for _ in 0..60 {
            paint_text_and_read(&mut painter, &rects, &paints, &glyphs);
        }
        let rebuilt = one_atlas_table(ATLAS_INK, RED);
        assert_eq!(
            rebuilt.atlases(),
            glyphs.atlases(),
            "the rebuilt table holds an equal atlas set"
        );
        assert!(
            !Arc::ptr_eq(rebuilt.atlas_set(), glyphs.atlas_set()),
            "behind a different allocation, so the pointer fast path misses \
             and the contents comparison is what is under test"
        );
        paint_text_and_read(&mut painter, &rects, &paints, &rebuilt);

        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            1,
            "sixty-one paints of one atlas set must decode it once, not once per paint()"
        );
    }

    /// A painter that outlives the document it painted must not sample the
    /// old document's atlas (issue #644) — the text-side twin of
    /// `a_different_asset_table_is_not_the_table_that_was_decoded`, and the
    /// same host shape: one painter, two documents, an atlas at index 0 in
    /// both.
    ///
    /// The two atlases differ in what the MSDF resolve makes of them: the
    /// first paints red, the second paints nothing at all. So a stale decode
    /// shows up as ink on a surface that should have stayed clear.
    ///
    /// Falsifiable: deleting the `self.source != *atlases` arm from
    /// `MsdfCache::refresh` — keeping only the `Arc::ptr_eq` early return —
    /// makes the second read return the first document's red.
    #[test]
    fn a_different_atlas_set_is_not_the_set_that_was_decoded() {
        let (paints, rects) = one_text_rect();
        let first = one_atlas_table(ATLAS_INK, RED);
        let second = one_atlas_table(ATLAS_VOID, RED);
        assert_ne!(
            first.atlases(),
            second.atlases(),
            "the two documents hold different atlas bytes"
        );

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(2, 2);
        let drawn_first = paint_text_and_read(&mut painter, &rects, &paints, &first);
        let drawn_second = paint_text_and_read(&mut painter, &rects, &paints, &second);

        assert_eq!(
            drawn_first,
            [255, 0, 0, 255],
            "a run over a full-coverage atlas paints its own fill colour"
        );
        assert_eq!(
            drawn_second[3], 0,
            "a second document's run must sample the second document's atlas, which resolves to \
             zero coverage — reading the first document's decode paints red on a clear surface"
        );
        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            2,
            "each of the two documents decodes its own atlas once"
        );
    }

    /// A text-free scene compiles no shader and decodes no atlas, which is
    /// the posture the lazy entry point had before the cache moved onto the
    /// painter — worth pinning, because a cache built eagerly in `paint()`
    /// would make every image-only scene pay for text it never draws.
    #[test]
    fn a_text_free_scene_decodes_no_atlas() {
        let (paints, rects) = one_text_rect();

        DECODE_CALLS.with(|c| c.set(0));
        let mut painter = SkiaPainter::new(2, 2);
        paint_text_and_read(&mut painter, &rects, &paints, &GlyphRunTable::new());

        assert_eq!(
            DECODE_CALLS.with(|c| c.get()),
            0,
            "a scene with no glyph runs decodes nothing"
        );
    }

    /// **A coverage field that draws nothing decodes no atlas** (issue #1044).
    ///
    /// `ImageCache::get` decodes on first request, and `field_coverage` answers
    /// `None` for a field with no area — so asking after the fetch paid a full
    /// decode for a field that paints nothing. `dashscene-gpu` never did: its
    /// gate sits above residency, and `dashscene_gpu::field_draws`' own doc is
    /// where that reason is written — "checked **before** the payload is made
    /// resident rather than after".
    ///
    /// Both call sites are covered, because they are separate paths that each
    /// fetch their own atlas: the masked fill, and the backdrop blur.
    ///
    /// **Each path is measured both ways**, which is what makes the zero mean
    /// anything. A test asserting only `DECODE_CALLS == 0` passes just as well
    /// against a painter that deleted the path: the drawing case asserting
    /// **one** is what says the decode was skipped rather than never reachable.
    /// Found by review — the first version had only the zeros.
    ///
    /// **What the backdrop row's `1` does not establish**, stated because the
    /// mutation says so rather than because the shape suggests it: an entry
    /// carrying a backdrop also carries its shape, so the masked-fill path runs
    /// for it too and fetches the same image index first — and `ImageCache`
    /// dedupes by index, so the count is one however many paths asked. Deleting
    /// the backdrop path outright therefore still reads one and this test still
    /// passes. Measured. What the backdrop row does pin is the **hoist**: with
    /// its gate forced open the degenerate case reads one and fails, because the
    /// fill path's own gate still holds. Isolating the backdrop path's decode
    /// would need an entry with a shape whose fill path does not run, which this
    /// painter has no way to build — `paints.shape(entry)` drives both.
    ///
    /// **The picture is pinned elsewhere** — the two `DRAWS_NOTHING` sweeps in
    /// `tests/painter.rs` already assert that such a field paints nothing on
    /// both paths, and `a_sound_coverage_mask_still_draws` that a sound one
    /// does. This adds the cost, which no pixel can show.
    ///
    /// # The origin row (issue #1160)
    ///
    /// The third row carries a **sound** field at a node origin of `f32::MAX`,
    /// where the device quad cancels to nothing. `VectorField::draws` accepts
    /// it — the predicate reads `plane_bounds` alone — and until #1160 only
    /// `field_coverage`'s own device-quad guard refused it, below the fetch. So
    /// it paid for its atlas to draw nothing, which is exactly what #1044 fixed
    /// for the other route.
    ///
    /// It reads **1 on `main` and 0 after**, so it falsifies rather than
    /// decorates. `a_frosted_node_with_an_out_of_domain_origin_draws_nothing` in
    /// `tests/painter.rs` covers the same origins for the *picture* and cannot
    /// cover this: `DECODE_CALLS` is a `cfg(test)` thread-local in this file,
    /// which an integration test cannot see.
    #[test]
    fn a_coverage_field_that_draws_nothing_decodes_no_atlas() {
        let mut images = ImageTable::new();
        let image = images.push(ImageAsset {
            format: dashpaint::ImageFormat::Png,
            bytes: one_pixel_png(RED),
        });
        let sound = dashpaint::VectorField {
            image,
            atlas_rect: [0, 0, 1, 1],
            plane_bounds: [0.0, 0.0, 8.0, 8.0],
            distance_range: 4.0,
        };
        // The same field with no atlas extent, so it draws nothing on either
        // path. One member apart, so the two cases differ in the answer and in
        // nothing else.
        let degenerate = dashpaint::VectorField {
            atlas_rect: [0, 0, 0, 0],
            ..sound
        };
        assert!(
            sound.draws() && !degenerate.draws(),
            "the two fixtures must differ in exactly this predicate's answer",
        );

        for (what, blurs) in [
            ("a masked fill", &[][..]),
            (
                "a backdrop blur",
                &[dashpaint::Blur {
                    kind: dashpaint::BlurKind::Backdrop,
                    radius: 8.0,
                }][..],
            ),
        ] {
            for (why, field, origin, expected) in [
                ("draws", sound, 0.0, 1),
                ("has no atlas extent", degenerate, 0.0, 0),
                // A sound field whose *device* quad cancels: `f32::MAX + 8.0`
                // is `f32::MAX`, so both ends of the 8-unit plane round to the
                // same float and the quad has zero width. Not an overflow to an
                // infinity, which four documents said until issue #1160
                // measured it.
                ("is collapsed by its node origin", sound, f32::MAX, 0),
            ] {
                let mut paints = PaintTable::new();
                let fill = paints.intern_fill(&dashpaint::FillSpec::Solid { color: RED });
                let entry = paints.push_with(
                    dashpaint::PaintEntry {
                        fill,
                        ..dashpaint::PaintEntry::default()
                    },
                    dashpaint::EntryParts {
                        shape: Some(field),
                        blurs,
                        ..dashpaint::EntryParts::default()
                    },
                );
                let rects = [RectEntry {
                    x: origin,
                    y: origin,
                    w: 8.0,
                    h: 8.0,
                    paint: entry,
                    clip: dashpaint::ClipIndex::UNCLIPPED,
                    opacity: 1.0,
                    rotation: 0.0,
                    rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
                }];

                DECODE_CALLS.with(|c| c.set(0));
                let mut painter = SkiaPainter::new(8, 8);
                painter.paint(
                    &rects,
                    &paints,
                    &images,
                    &ClipTable::new(),
                    &[],
                    &GlyphRunTable::new(),
                    None,
                );

                assert_eq!(
                    DECODE_CALLS.with(|c| c.get()),
                    expected,
                    "{what} whose coverage field {why} must decode {expected} atlas: both refusals \
                     are asked before `ImageCache::get`, not inside the draw call below it — and \
                     the drawing case is what says this path decodes at all",
                );
            }
        }
    }
}
