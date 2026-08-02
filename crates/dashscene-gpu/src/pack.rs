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
//! # What this packer does not emit, by name
//!
//! - **Glyph quads.** A glyph's texel rectangle is a coordinate in the
//!   painter's residency atlas rather than the `atlas_px` boundary B carries,
//!   and residency is story #581. Story #582 appends glyph instances to the
//!   anchor rect's span, after its inner shadows — the position
//!   `dashscene-skia` already draws an anchored run at.
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
    BlurKind, ClipTable, GlyphRunTable, GroupComposite, ImageTable, PaintKind, PaintTable,
    PaintTag, RectEntry, Shadow, ShadowKind, Stroke, StrokeAlign,
};

use crate::instance::{Instance, InstanceBuffer, InstanceKind, bounds_of, shape_slot};

/// Packs one frame of boundary-B tables into `out`, which is emptied first.
///
/// `images` and `glyphs` are part of boundary B and are accepted so that the
/// packer's signature is the painter's, not a subset of it; neither
/// contributes an instance yet — see the module documentation for which story
/// each belongs to.
///
/// # Panics
///
/// Panics through boundary B's own resolvers when a rect names a paint or clip
/// index its tables do not hold. Indices are validated upstream (P4), so a
/// miss is a broken contract between crates and never a frame to skip.
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
    let _ = (images, glyphs);
    out.clear();

    // Two properties make one forward pointer plus a stack enough, and they
    // come from different places. Nesting is boundary B's, stated on
    // `GroupComposite` and in `docs/decisions/masks-and-group-opacity.md`.
    // Ascending `start` is not: it comes from `dashscene-core`'s pre-order
    // emission (`crates/dashscene-core/src/arena.rs`, the walk over `order`),
    // which is what the reference painter relies on too. Both are asserted
    // below rather than assumed, because a violation of either is silent —
    // every instance after it carries a wrong `layer`, which is the "group
    // applied to the wrong set" defect layer 1 exists to catch.
    let mut next_group = 0usize;
    let mut open: Vec<(usize, u32)> = Vec::new();

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
            open.push((next_group, groups[next_group].end));
            next_group += 1;
        }
        let layer = open
            .last()
            .map(|&(group, _)| u32::try_from(group).expect("group list exceeds u32::MAX") + 1)
            .unwrap_or(Instance::NONE);

        out.begin_rect(i);
        pack_rect(out, rect, paints, clips, layer);
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

    // What every instance of this rect shares. The three kind-specific members
    // are set on every push below rather than defaulted here: an instance whose
    // `kind` came from this template would draw a shadow of row 0, which is a
    // real shadow of a real table and so paints something wrong rather than
    // nothing.
    let base = Instance {
        kind: InstanceKind::Shadow.as_u32(),
        tag: 0,
        row: 0,
        shape: Instance::NONE,
        clip_offset: clip.offset,
        clip_count: clip.count,
        layer,
        opacity: rect.opacity,
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
            tag: blur.kind as u32,
            row: entry.blurs.offset + row_offset(offset),
            shape,
            ..base
        });
    }

    // 2. Drop shadows, in the document's back-to-front `effects` order, each
    //    cast from the node's stroked silhouette rather than from its fill box.
    for (offset, shadow) in paints.shadows(entry).iter().enumerate() {
        if shadow.kind != ShadowKind::Drop {
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
                kind: InstanceKind::Fill.as_u32(),
                tag: entry.fill.tag as u32,
                row: entry.fill.index,
                shape,
                ..base
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
                tag: 0,
                row: entry.stroke.offset,
                ..base
            });
        }
    }

    // 4. Inner shadows, over the fill and the stroke. An inner shadow is
    //    clipped to the node's own shape and takes no stroke outset — the
    //    reference painter passes it none.
    for (offset, shadow) in paints.shadows(entry).iter().enumerate() {
        if shadow.kind != ShadowKind::Inner {
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
fn fill_instance(base: &Instance, kind: PaintKind) -> Instance {
    Instance {
        kind: InstanceKind::Fill.as_u32(),
        tag: kind.tag as u32,
        row: kind.index,
        ..*base
    }
}

/// One shadow instance, over the silhouette that shadow casts from.
///
/// `outset` grows the box and its corners: it is the stroke outset for a drop
/// shadow and zero for an inner one. The spread, the offset and the blur stay
/// on the row this names, so a shader reads them there.
fn shadow_instance(base: &Instance, row: u32, shadow: &Shadow, outset: f32) -> Instance {
    Instance {
        kind: InstanceKind::Shadow.as_u32(),
        tag: shadow.kind as u32,
        row,
        bounds: grow(base.bounds, outset),
        corners: grow_corners(base.corners, outset),
        ..*base
    }
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
