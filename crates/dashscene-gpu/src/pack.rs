//! The packer: boundary B's tables in, one ordered [`InstanceBuffer`] out.
//!
//! This is the whole of the painter's per-frame CPU work as
//! `docs/specification/03-target-hardware-rules.md` R-T4 defines it, and it is
//! the only place a table is read. Everything downstream — pipelines, bind
//! groups, shaders — is a pure function of what this produces, which is what
//! makes layer 1 (`docs/decisions/instance-buffer-contract.md`) able to catch
//! a translation defect on a runner with no GPU.
//!
//! # The order is the reference painter's, deliberately
//!
//! `docs/decisions/instance-buffer-contract.md` D5 states the per-node order
//! and why it is copied from `dashscene-skia` rather than re-derived from
//! boundary B. The code below is the one other place it is written, because it
//! is the place that produces it.
//!
//! # Where a glyph run goes
//!
//! Story #582 appends a run's glyph instances to the span of the rect the run
//! is anchored to, after that rect's inner shadows — the position
//! `dashscene-skia` draws an anchored run at, which puts the run inside the
//! rect's clip region (issue #275) and inside every group layer enclosing it
//! (issue #274). Nothing about the span contract changed: the instances go at
//! the end of a rect's list, so they widen a count and move no boundary.
//!
//! The instance carries the glyph's rectangle in the run's **source** atlas,
//! in that atlas's own texels. Where residency put that atlas is a device
//! question and this packer has no device — which is exactly what makes layer 1
//! runnable on a runner with no GPU — so the row the instance names carries the
//! mapping, and it is resolved once per run rather than once per glyph.
//!
//! # What this packer does not emit, by name
//!
//! - **`BlurKind::Layer` blurs.** Node-local layer blur is budgeted at v1 and
//!   nothing in this tree produces one (`dashc` lowers only
//!   `BACKGROUND_BLUR`). The reference painter skips it by the same filter.
//!   A named gap, not a silent drop (P4).
//! - **The flatten-for-a-stroked-node layer** `dashscene-skia` opens when a
//!   node carries both a fill and a stroke below full opacity (debt #277).
//!   What this packer emits for such a node is not a missing layer but a
//!   different alpha: three instances each at the group alpha, where the
//!   reference draws three opaque instances into one layer composited at that
//!   alpha. Where an Inside or Center stroke overlaps its own fill the two
//!   differ. Story #583 owns group compositing and decides it for this painter.
//! - **A masked node's stacked layers and its stroke**, and a masked node
//!   whose fill is an image. The reference painter draws a baked-vector node's
//!   own `fill` through the coverage field and nothing else, and draws nothing
//!   at all for an image fill ("an image-filled vector is not in the measured
//!   census; it draws nothing rather than an unmasked rectangle"). Both are
//!   matched here rather than decided here.

use dashpaint::{
    BlurKind, ClipRegion, ClipTable, GlyphRun, GlyphRunTable, GroupComposite, ImageTable,
    PaintKind, PaintTable, PaintTag, RectEntry, Shadow, ShadowKind, Stroke, StrokeAlign,
};

use crate::instance::{Instance, InstanceBuffer, InstanceKind, Layer, bounds_of, shape_slot};

/// Packs one frame of boundary-B tables into `out`, which is emptied first.
///
/// `images` is part of boundary B and is accepted so that the packer's
/// signature is the painter's, not a subset of it; an image fill's payload is
/// a device question and no instance reads this table.
///
/// # Panics
///
/// Panics through boundary B's own resolvers when a rect names a paint or clip
/// index its tables do not hold. Indices are validated upstream (P4), so a
/// miss is a broken contract between crates and never a frame to skip.
///
/// Panics when a glyph run is anchored to a rect the table does not hold, for
/// the same reason and by the same rule the reference painter panics under.
#[allow(clippy::too_many_arguments)]
pub fn pack(
    out: &mut InstanceBuffer,
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    clips: &ClipTable,
    groups: &[GroupComposite],
    glyphs: &GlyphRunTable,
) {
    let _ = images;
    out.clear();

    // Runs arrive ordered by anchor — commit stable-sorts them
    // (`docs/decisions/glyph-runs-cross-boundary-b.md`) — so one forward cursor
    // walks them alongside the rects. Both properties the cursor rests on are
    // checked below rather than assumed: the order, at each step, and that
    // every run was consumed, at the end. A run the cursor walked past would
    // draw nothing, which is a picture missing its text and no diagnostic.
    let mut next_run = 0usize;
    let runs = glyphs.runs();

    // Two properties make one forward pointer plus a stack enough, and they
    // come from different places. Nesting is boundary B's, stated on
    // `GroupComposite` and in `docs/decisions/masks-and-group-opacity.md`.
    // Ascending `start` is not: it comes from `dashscene-core`'s pre-order
    // emission (`crates/dashscene-core/src/arena.rs`, the walk over `order`),
    // which is what the reference painter relies on too. Both are asserted
    // below rather than assumed, because a violation of either is silent —
    // every instance after it carries a wrong `layer`, which is the "group
    // applied to the wrong set" defect layer 1 exists to catch.
    // The open stack holds `(layer slot, group end)` — the slot being the
    // `Instance::layer` value, already biased, rather than the group index it
    // came from. Biasing once where the layer is created means the two places
    // that read the stack cannot disagree about whether it is biased.
    let mut next_group = 0usize;
    let mut open: Vec<(u32, u32)> = Vec::new();

    for (index, rect) in rects.iter().enumerate() {
        let i = u32::try_from(index).expect("rect table exceeds u32::MAX entries");
        while open.last().is_some_and(|&(_, end)| end <= i) {
            open.pop();
        }
        // A group whose `start` is behind the walk can never be opened by the
        // forward pointer, so it and every group after it would be skipped in
        // silence.
        debug_assert!(
            next_group >= groups.len() || groups[next_group].start >= i,
            "group {next_group} starts at {} but the walk is already at rect {i}; groups arrive \
             in ascending start order",
            groups[next_group].start,
        );
        while next_group < groups.len() && groups[next_group].start == i {
            // A group that ended past its parent's end would leave the stack in
            // an order where `last` is not the innermost, and every instance
            // after it would carry the wrong layer while every test passed.
            debug_assert!(
                open.last()
                    .is_none_or(|&(_, end)| groups[next_group].end <= end),
                "group {next_group} runs past the group enclosing it; boundary B's groups nest",
            );
            // Read the enclosing layer *before* pushing this one: a group
            // composites into whatever was innermost when it opened, and after
            // the push that is the group itself.
            let parent = open.last().map_or(Instance::NONE, |&(slot, _)| slot);
            let slot = out.push_layer(
                next_group,
                Layer {
                    alpha: groups[next_group].alpha,
                    parent,
                },
            );
            open.push((slot, groups[next_group].end));
            next_group += 1;
        }
        let layer = open.last().map_or(Instance::NONE, |&(slot, _)| slot);

        out.begin_rect(i);
        pack_rect(out, rect, paints, clips, layer);

        // The run's glyphs, after this rect's own ink — the position the
        // reference painter draws an anchored run at, which is what puts it
        // inside this rect's clip and inside every group layer enclosing it.
        debug_assert!(
            next_run >= runs.len() || runs[next_run].rect >= i,
            "glyph run {next_run} is anchored to rect {} but the walk is already at rect {i}; \
             commit orders the run table by anchor",
            runs[next_run].rect,
        );
        while next_run < runs.len() && runs[next_run].rect == i {
            let row = u32::try_from(next_run).expect("glyph run table exceeds u32::MAX runs");
            pack_run(out, glyphs, row, clips.region(rect.clip), layer);
            next_run += 1;
        }
    }

    // A group starting past the rect table is never opened, so it composites
    // nothing and — since the layer table is index-aligned with the group slice
    // — every later group would be recorded at the wrong index. The same
    // failure the run cursor asserts below, one table over, and the same reason
    // it cannot be reported from inside the walk (P4).
    assert_eq!(
        next_group,
        groups.len(),
        "group {next_group} starts at rect {} of a {}-rect table: a group's range is meaningful \
         only against the rect table of the commit it came from",
        groups.get(next_group).map_or(0, |group| group.start),
        rects.len(),
    );

    // A run anchored past the rect table draws nothing and is the one failure
    // the cursor cannot report from inside the walk. The reference painter
    // asserts the same thing, by name, for the same reason (P4).
    assert_eq!(
        next_run,
        runs.len(),
        "glyph run {next_run} is anchored to rect {} of a {}-rect table: a run's anchor is \
         meaningful only against the rect table of the commit it came from",
        runs.get(next_run).map_or(0, |run| run.rect),
        rects.len(),
    );
}

/// One instance per glyph run `row` places, in the run's own draw order.
///
/// The geometry is the reference painter's, resolved here rather than
/// re-derived: `plane_em` is y-up in ems from the baseline and document space
/// is y-down, so the top of the quad is `y - top * size` and the bottom is
/// `y - bottom * size`. Getting that flip wrong moves every glyph by its own
/// height, which reads as a baseline offset rather than as a transposition.
///
/// A glyph id the atlas has no quad for draws nothing and produces no instance
/// — an empty outline such as a space, or a glyph outside the atlas's charset.
/// That is `dashpaint::Atlas::glyph`'s own contract, not a filter invented here.
fn pack_run(
    out: &mut InstanceBuffer,
    glyphs: &GlyphRunTable,
    row: u32,
    clip: ClipRegion,
    layer: u32,
) {
    let run: &GlyphRun = &glyphs.runs()[row as usize];
    let atlas = glyphs.atlas(run.atlas);
    let height = atlas.height as f32;
    for quad in glyphs.quads(run) {
        let Some(glyph) = atlas.glyph(quad.glyph_id) else {
            continue;
        };
        let [left, bottom, right, top] = glyph.plane_em;
        let size = run.size;
        let x = quad.x + left * size;
        let y = quad.y - top * size;
        // `atlas_px` is `[left, bottom, right, top]` with a bottom-left origin;
        // `Instance::corners` is `[x, y, w, h]` with a top-left origin, which is
        // the convention `dashpaint::VectorField::atlas_rect` already uses. The
        // flip is against the atlas's own height, so an atlas of a different
        // height places the same glyph at a different row — which is why the
        // height comes from the run's atlas and not from a constant.
        let [al, ab, ar, at] = glyph.atlas_px;
        out.push(Instance {
            kind: InstanceKind::Text.as_u32(),
            row,
            shape: Instance::NONE,
            clip_offset: clip.offset,
            clip_count: clip.count,
            layer,
            // The run's own free-path alpha, which is what the reference
            // painter multiplies the fill colour by. It mirrors the anchor
            // rect's `opacity` and is kept as its own field until that fold-in
            // lands (`docs/decisions/glyph-runs-cross-boundary-b.md`).
            opacity: run.opacity,
            // A glyph's ink is the field inside its own quad.
            outset: 0.0,
            bounds: [x, y, (right - left) * size, (top - bottom) * size],
            corners: [al, height - at, ar - al, at - ab],
        });
    }
}

/// Every instance one rect draws, in the reference painter's per-node order.
fn pack_rect(
    out: &mut InstanceBuffer,
    rect: &RectEntry,
    paints: &PaintTable,
    clips: &ClipTable,
    layer: u32,
) {
    let entry = paints.resolve(rect.paint);
    let clip = clips.region(rect.clip);
    // How far the node's stroke pushes its rendered silhouette past the fill
    // box. A drop shadow casts from that silhouette rather than from the bare
    // fill box (`docs/decisions/effects-vocabulary-shadows.md`, and
    // `dashscene-skia`'s `stroke_outset`), and it is the one term of the
    // shadow's geometry that no row of any table carries — a shadow instance
    // names a shadow row, not a stroke row. So it is resolved here, into the
    // drop-shadow instance's own bounds. Spread, offset and blur stay on the
    // row, where a shader reads them.
    let outset = stroke_outset(paints.stroke(entry));
    // A `ShapeRange` of arity one, so its offset is the row when it names
    // anything at all — the accessor decides whether it does, rather than this
    // reading `count` and duplicating the arity rule.
    let shape = shape_slot(paints.shape(entry).map(|_| entry.shape.offset));

    // What every instance of this rect shares. `kind` and `row` are set on every
    // push below rather than defaulted here: an instance taking them from this
    // template would draw a drop shadow of row 0, which is a real shadow of a
    // real table and so paints something wrong rather than nothing.
    let base = Instance {
        kind: InstanceKind::ShadowDrop.as_u32(),
        row: 0,
        shape: Instance::NONE,
        clip_offset: clip.offset,
        clip_count: clip.count,
        layer,
        opacity: rect.opacity,
        // Zero, and every kind that draws past its own bounds overrides it: the
        // stroke and the drop shadow. A fill, a glyph and a backdrop draw inside
        // the box this template states, so the default is the value they keep.
        outset: 0.0,
        bounds: bounds_of(rect),
        corners: [
            entry.corners.top_left,
            entry.corners.top_right,
            entry.corners.bottom_right,
            entry.corners.bottom_left,
        ],
    };

    // 1. The backdrop, blurred, beneath the node's own ink. A masked node's
    //    blur is confined to the field's coverage, so `shape` rides along.
    for (offset, blur) in paints.blurs(entry).iter().enumerate() {
        if blur.kind != BlurKind::Backdrop {
            continue;
        }
        out.push(Instance {
            kind: InstanceKind::Backdrop.as_u32(),
            row: entry.blurs.offset + row_offset(offset),
            shape,
            ..base
        });
    }

    // 2. Drop shadows, in the document's back-to-front `effects` order, each
    //    cast from the node's stroked silhouette rather than from its fill box.
    for (offset, shadow) in paints.shadows(entry).iter().enumerate() {
        if shadow.kind != ShadowKind::Drop || !inks(shadow, rect.opacity) {
            continue;
        }
        out.push(shadow_instance(
            &base,
            entry.shadows.offset + row_offset(offset),
            shadow,
            outset,
        ));
    }

    // 3. The node's ink. A baked-vector node's fill is masked by its coverage
    //    field, and the parametric stroke and stacked layers do not apply — a
    //    vector carries its outline in the baked geometry.
    if shape != Instance::NONE {
        // An image-filled vector draws nothing rather than an unmasked
        // rectangle, which is the reference painter's own choice: masking an
        // image fill is additive later work, and B1 widened the vocabulary by
        // exactly what was measured. Matched here rather than decided here.
        if matches!(entry.fill.tag, PaintTag::Solid | PaintTag::Gradient) {
            out.push(Instance {
                shape,
                ..fill_instance(&base, entry.fill)
            });
        }
    } else {
        if entry.fill.tag != PaintTag::None {
            out.push(fill_instance(&base, entry.fill));
        }
        for kind in paints.extra_fills(entry) {
            out.push(fill_instance(&base, *kind));
        }
        if paints.stroke(entry).is_some() {
            out.push(Instance {
                kind: InstanceKind::Stroke.as_u32(),
                row: entry.stroke.offset,
                // The same number the drop shadow's silhouette grew by, in its
                // other role: how far this stroke's own band reaches past the
                // fill box its instance is stated over.
                outset,
                ..base
            });
        }
    }

    // 4. Inner shadows, over the fill and the stroke. An inner shadow is
    //    clipped to the node's own shape and takes no stroke outset — the
    //    reference painter passes it none.
    for (offset, shadow) in paints.shadows(entry).iter().enumerate() {
        if shadow.kind != ShadowKind::Inner || !inks(shadow, rect.opacity) {
            continue;
        }
        out.push(shadow_instance(
            &base,
            entry.shadows.offset + row_offset(offset),
            shadow,
            0.0,
        ));
    }
}

/// One fill instance naming `kind`'s row in the per-kind table its tag names.
///
/// The tag is mapped by an exhaustive `match`, never cast. A cast would put
/// `dashpaint`'s discriminant into the buffer, where a reordered variant
/// changes the number and every consumer's constant keeps its old meaning —
/// silently, because the goldens pin what this packer wrote. A `match` is
/// indifferent to the numbers and a new variant is a compile error here.
///
/// # Panics
///
/// Panics on [`PaintTag::None`]: a fill-less entry emits no instance at all,
/// and a stacked layer naming no fill is a corrupt list rather than an empty
/// one — which `PaintTable::check_fills` already refuses by name.
fn fill_instance(base: &Instance, kind: PaintKind) -> Instance {
    let instance_kind = match kind.tag {
        PaintTag::Solid => InstanceKind::FillSolid,
        PaintTag::Gradient => InstanceKind::FillGradient,
        PaintTag::Image => InstanceKind::FillImage,
        PaintTag::None => panic!("a fill-less entry emits no fill instance"),
    };
    Instance {
        kind: instance_kind.as_u32(),
        row: kind.index,
        ..*base
    }
}

/// One shadow instance, over the silhouette that shadow casts from.
///
/// `outset` grows the box and its corners: it is the stroke outset for a drop
/// shadow and zero for an inner one. The spread, the offset and the blur stay
/// on the row this names, so a shader reads them there — and
/// [`shadow_ink_reach`] says how far past the silhouette they take the ink, so
/// the vertex stage can build a quad that does not clip it.
fn shadow_instance(base: &Instance, row: u32, shadow: &Shadow, outset: f32) -> Instance {
    // Mapped, never cast, for the reason `fill_instance` gives.
    let kind = match shadow.kind {
        ShadowKind::Drop => InstanceKind::ShadowDrop,
        ShadowKind::Inner => InstanceKind::ShadowInner,
    };
    Instance {
        kind: kind.as_u32(),
        row,
        bounds: grow(base.bounds, outset),
        corners: grow_corners(base.corners, outset),
        outset: shadow_ink_reach(shadow),
        ..*base
    }
}

/// Whether a shadow puts any ink on the frame at all — issue #285's early-out,
/// implemented natively rather than reproduced and then filed again.
///
/// A shadow whose colour is fully transparent, or one on a node whose free-path
/// alpha is zero, draws nothing at any blur or spread. The reference painter
/// still builds a paint, a Gaussian mask filter and a rasterisation for it,
/// which is the waste #285 reports against `dashscene-skia`; here the instance
/// is not emitted, so nothing downstream — no upload, no quad, no fragment —
/// pays for it either.
///
/// Written as `> 0.0` rather than as a `<= 0.0` rejection, which a NaN passes.
/// The same guard `dashscene-skia`'s `backdrop_blur_filter` gives its reason
/// for: the load path refuses a non-finite colour, but the producer API stores
/// effects unchecked, so this is the last place the two can disagree.
///
/// Skipping a shadow does not move any other shadow's row. A row is the
/// shadow's position in its entry's own list
/// (`a_shadow_row_is_its_position_in_the_entrys_own_list`), which is the
/// `enumerate` index, and that is unchanged by whether the instance is pushed.
fn inks(shadow: &Shadow, opacity: f32) -> bool {
    shadow.color.a * opacity > 0.0
}

/// How far a shadow's ink reaches past the silhouette its instance is stated
/// over, on every side.
///
/// **Zero for an inner shadow**, whose ink is clipped to the node's own shape
/// however far its offset and blur reach — the reference painter clips it to
/// exactly that shape, and the fragment stage does the same.
///
/// A **drop shadow** is drawn displaced from that silhouette: outward by its
/// spread, along its offset, and blurred. Its quad has to cover all three, so
/// they add. The offset is one displacement rather than a growth on both sides,
/// but the quad grows symmetrically, so the larger axis is taken and both sides
/// get it — the cheap over-estimate, on the side of the trade
/// [`Instance::outset`] documents: only the lower bound is a correctness
/// property.
///
/// **Three sigma is the blur's whole support here**, not a truncation of it:
/// `blurred_rounded_box` in `shaders/sdf.wgsl` integrates over `p.y ± 3 sigma`
/// and reports zero coverage where that window clears the box, so past this
/// reach the shader itself draws nothing. The x integral is a difference of
/// error functions rather than a windowed sum, and it does leave a tail out
/// there — bounded by 0.0014 of full coverage at three sigma, a third of a code
/// point of 255, which is below what an eight-bit output can carry.
///
/// Floored at zero: a negative spread shrinks the shadow, and a quad smaller
/// than the instance's own bounds is not what this member means.
fn shadow_ink_reach(shadow: &Shadow) -> f32 {
    if shadow.kind == ShadowKind::Inner {
        return 0.0;
    }
    let offset = shadow.offset.x.abs().max(shadow.offset.y.abs());
    (shadow.spread + 3.0 * blur_sigma(shadow.blur) + offset).max(0.0)
}

/// The Gaussian sigma a blur radius maps to — [`dashpaint::BLUR_SIGMA_PER_RADIUS`],
/// which is Figma's measured constant and the reference painter's own mapping
/// (`docs/decisions/blur-sigma-is-figmas-mapping.md`).
///
/// Named here so that the packer, which sizes a shadow's quad by the blur's
/// support, and `render::paint_heap`, which writes the sigma the fragment stage
/// integrates, apply one mapping. Two call sites multiplying by the constant
/// themselves would be two chances to write `radius / 2`, which is the CSS
/// convention this project measured and rejected.
pub(crate) fn blur_sigma(radius: f32) -> f32 {
    radius * dashpaint::BLUR_SIGMA_PER_RADIUS
}

/// How far a stroke pushes the node's rendered silhouette past its fill box:
/// an Outside stroke by its full width, a Center stroke by half, an Inside
/// stroke not at all.
///
/// The same geometry `dashscene-skia`'s `stroke_outset` computes, and the one
/// term of a drop shadow's silhouette that no table row carries.
fn stroke_outset(stroke: Option<&Stroke>) -> f32 {
    match stroke {
        Some(stroke) => match stroke.align {
            StrokeAlign::Inside => 0.0,
            StrokeAlign::Center => stroke.width / 2.0,
            StrokeAlign::Outside => stroke.width,
        },
        None => 0.0,
    }
}

/// `bounds` grown by `delta` on every side. The identity at `delta == 0`, so
/// an unstroked node's shadow keeps the node's own box exactly.
fn grow(bounds: [f32; 4], delta: f32) -> [f32; 4] {
    if delta == 0.0 {
        return bounds;
    }
    let [x, y, w, h] = bounds;
    [x - delta, y - delta, w + 2.0 * delta, h + 2.0 * delta]
}

/// Corner radii grown by `delta`. A sharp corner stays sharp, which is what
/// `dashscene-skia`'s `spread_corners` does and what keeps a square-cornered
/// node's shadow square.
fn grow_corners(corners: [f32; 4], delta: f32) -> [f32; 4] {
    if delta == 0.0 {
        return corners;
    }
    corners.map(|radius| if radius > 0.0 { radius + delta } else { 0.0 })
}

/// A position within one entry's slice of a flat array, as the row index type.
fn row_offset(offset: usize) -> u32 {
    u32::try_from(offset).expect("an entry's effect list exceeds u32::MAX elements")
}
