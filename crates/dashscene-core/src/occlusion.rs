//! The occlusion pass: which part of each rect no later opaque rect covers.
//!
//! On the fill-bound GPUs this project targets, the cost of a frame is the
//! area the fragment stage shades, and a rect drawn under a later opaque rect
//! is shaded and then overwritten. R-T2
//! (docs/specification/03-target-hardware-rules.md) asks for the opaque core
//! of a quad to reject the pixels it covers. Story #1450 meets that on the
//! shared side, before any painter runs: this pass cuts each rect into the
//! [`Piece`]s that are still visible, so both painters shade only those. The
//! one exception is the Unity painter's `LitOpaque` material class: a frame
//! with a node that class refuses for its coverage (a corner radius, a clip, a
//! stroke or an opacity below 1) is packed with every rect whole
//! (`FramePacker.AnyCoverageRefused`).
//!
//! The pass has three steps, and the commit runs them in this order:
//!
//! 1. [`cores`] finds every *opaque core* — a rect whose base fill is a solid
//!    colour at alpha 1, or a gradient whose stops are all at alpha 1, on a
//!    node at opacity 1 that is not rotated, has no shape, has no blur, and
//!    is not inside a [`GroupComposite`] composited below alpha 1 — and the
//!    *interior* it occludes: its box shrunk by its largest corner radius
//!    plus the host's anti-aliasing band, intersected with each of its clip
//!    boxes shrunk the same way by their own radii. Inside that interior
//!    every pixel the core paints is opaque, whatever painter draws it.
//! 2. [`pieces`] cuts each rect by the interior of every core **later** in the
//!    rect table — slice order is stacking order (`dashpaint::Painter::paint`),
//!    so only a later rect draws over an earlier one. A rect whose ink reaches
//!    past its box ([`occludable`] is false) and a rotated rect are kept
//!    whole, and so is a rect whose piece list grows past
//!    [`MAX_PIECES_PER_RECT`] after any one cut. A core never cuts a rect across a
//!    [`barriers`] rect that lies between the two: that rect samples the
//!    composited backdrop beneath it, so everything before it must still be
//!    whole when it is drawn.
//! 3. [`changed_rects`] compares two commits' pieces, because a core that moves
//!    changes the pieces of the rects beneath it while their own entries stay
//!    the same. The commit (`Txn::commit_with`) adds every rect it names to
//!    the dirty set, and holds the pieces on the `CommittedScene`
//!    ([`crate::CommittedScene::pieces`]).
//!
//! The pass lives in this crate rather than beside `RectEntry` in
//! `dashpaint`, because the commit that runs it needs the previous commit's
//! pieces and the dirty set, and both live here.
//!
//! Only the base [`PaintEntry::fill`] decides whether a rect is a core: it is
//! the bottom layer (`PaintEntry::extra_fills` paint over it), and any layer
//! composited over an opaque pixel leaves it opaque.

use dashpaint::Piece;

use crate::committed::{
    ClipBox, ClipTable, Color, CornerRadii, Fill, GradientStop, GroupComposite, PaintEntry,
    PaintKind, PaintTable, RectEntry, StrokeAlign,
};

/// The most pieces one rect is cut into. [`pieces`] tests the count after
/// each core's cut, so a rect whose piece list grows past this count after
/// any one cut keeps its whole quad and is not occluded at all, even when a
/// later cut would bring the count back under it. Each piece is one more
/// instance to pack, and on the Unity path one more draw command, so past
/// this count the extra instances cost more than the shaded area they remove.
///
/// Set to 32 from the per-scene piece counts that
/// `crates/dashscene-gpu/tests/piece_count.rs` pins. At a cap of 8, each showcase scene's opaque full-screen background
/// lay under more cuts than the cap, kept its whole quad, and the pass removed
/// almost nothing (0.0033, 0.0004 and 0.0424 Mpx on `surfaces`, `typography`
/// and `layout`). At 32 no rect in those scenes reaches the cap: the drawn
/// rects become 59, 18 and 90 pieces and the pass removes 0.7428, 0.2777 and
/// 2.4636 Mpx, the same as with no cap at all. Those areas are boxes and
/// pieces as `piece_count.rs` sums them, with no anti-aliasing margin;
/// `crates/dashscene-gpu/tests/shaded_area.rs` adds the margin each packed
/// quad carries and reads 0.7430 Mpx on `surfaces` and the same two figures
/// on the other scenes.
pub const MAX_PIECES_PER_RECT: usize = 32;

/// An axis-aligned rectangle in the rect table's absolute space, as its four
/// edges: a core's interior, or one piece of a rect while [`pieces`] is
/// cutting it.
///
/// **Edges, not an origin and a size.** [`subtract`]
/// only ever copies an edge, through `min` and `max`, and never computes one,
/// so every edge of every piece is bit for bit an edge of the rect's box or of
/// a core's interior. An origin and a size would re-derive the far edge as
/// `x + w`, which rounds differently from the edge it was cut against: two
/// pieces that meet would then leave a hairline gap or overlap, and a
/// translucent rect would show a seam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occluder {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// What a fill's alpha allows: [`classify`] takes this as a value, so a test of
/// the geometry builds no [`PaintTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillAlpha {
    /// No fill: the rect paints nothing of its own to occlude with.
    None,
    /// Every pixel of the fill is at alpha 1.
    Opaque,
    /// Some pixel of the fill may be below alpha 1.
    Translucent,
}

/// A solid colour is opaque only at alpha exactly 1.
pub fn solid_alpha(color: Color) -> FillAlpha {
    if color.a == 1.0 {
        FillAlpha::Opaque
    } else {
        FillAlpha::Translucent
    }
}

/// A gradient is opaque only when it has stops and every one is at alpha 1:
/// interpolating between opaque colours stays opaque.
pub fn stops_alpha(stops: &[GradientStop]) -> FillAlpha {
    if !stops.is_empty() && stops.iter().all(|stop| stop.color.a == 1.0) {
        FillAlpha::Opaque
    } else {
        FillAlpha::Translucent
    }
}

/// The alpha of the fill `kind` names in `paints`. An image fill is
/// [`FillAlpha::Translucent`] whatever its pixels hold: this pass does not
/// read image payloads.
pub fn fill_alpha(paints: &PaintTable, kind: PaintKind) -> FillAlpha {
    match paints.fill(kind) {
        Fill::None => FillAlpha::None,
        Fill::Solid(color) => solid_alpha(color),
        Fill::Gradient(view) => stops_alpha(view.stops),
        Fill::Image(_) => FillAlpha::Translucent,
    }
}

/// The interior `rect` occludes, or `None` when it occludes nothing.
///
/// `None` unless the rect is at opacity 1, unrotated, and `alpha` is
/// [`FillAlpha::Opaque`]. The interior is the box shrunk on every side by the
/// largest of `corners` plus `aa`, intersected with each of `clip_boxes`
/// shrunk by its own largest corner plus `aa`. A rounded corner of radius `r`
/// lies outside the box shrunk by `r`, and a clip edge is anti-aliased like a
/// rect edge, so neither leaves a partly covered pixel inside the result.
/// `None` when the result has no area, and `None` when any corner radius of
/// the rect or of a clip box is NaN.
pub fn classify(
    rect: &RectEntry,
    corners: CornerRadii,
    alpha: FillAlpha,
    clip_boxes: &[ClipBox],
    aa: f32,
) -> Option<Occluder> {
    // A NaN opacity or rotation is refused: the opacity test names NaN, and
    // NaN != 0.0 holds for the rotation.
    if alpha != FillAlpha::Opaque
        || rect.opacity < 1.0
        || rect.opacity.is_nan()
        || rect.rotation != 0.0
    {
        return None;
    }
    let mut interior = inset(rect.x, rect.y, rect.w, rect.h, corners, aa);
    // Refused before the clip intersection, whose `max` and `min` would drop
    // the NaN edges `inset` returns for a NaN radius and leave the clip's
    // interior standing in for the core's.
    if interior[0].is_nan() {
        return None;
    }
    for clip in clip_boxes {
        let [x0, y0, x1, y1] = inset(clip.x, clip.y, clip.w, clip.h, clip.corners, aa);
        // `max` and `min` below drop a NaN operand, so a clip box `inset`
        // refused is refused here.
        if x0.is_nan() {
            return None;
        }
        interior = [
            interior[0].max(x0),
            interior[1].max(y0),
            interior[2].min(x1),
            interior[3].min(y1),
        ];
    }
    let [x0, y0, x1, y1] = interior;
    // Negated so that a NaN edge yields no interior.
    if !(x1 > x0 && y1 > y0) {
        return None;
    }
    Some(Occluder {
        left: x0,
        top: y0,
        right: x1,
        bottom: y1,
    })
}

/// A box shrunk by its largest corner radius plus `aa`, as `[x0, y0, x1, y1]`.
///
/// Every edge is NaN when any radius is NaN, so that [`classify`] refuses the
/// box: `f32::max` drops a NaN operand and would otherwise inset the box as if
/// that corner were square, while the painters' SDF yields NaN coverage there.
fn inset(x: f32, y: f32, w: f32, h: f32, corners: CornerRadii, aa: f32) -> [f32; 4] {
    if [
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ]
    .iter()
    .any(|radius| radius.is_nan())
    {
        return [f32::NAN; 4];
    }
    let radius = corners
        .top_left
        .max(corners.top_right)
        .max(corners.bottom_right)
        .max(corners.bottom_left);
    let d = radius + aa;
    [x + d, y + d, x + w - d, y + h - d]
}

/// Whether a rect painted from `entry` may be cut into pieces: false when its
/// ink can reach past its box, because a piece is a sub-rectangle of the box
/// and ink outside the box would be lost with the part it was cut from.
///
/// False for a shape (its field's plane bounds are not the box), for an
/// outside or centred stroke, for any shadow, and for any blur. An inside
/// stroke stays inside the box and does not prevent it.
pub fn occludable(entry: &PaintEntry, paints: &PaintTable) -> bool {
    if entry.shape.count != 0 {
        return false;
    }
    if paints
        .stroke(entry)
        .is_some_and(|stroke| stroke.align != StrokeAlign::Inside)
    {
        return false;
    }
    paints.shadows(entry).is_empty() && paints.blurs(entry).is_empty()
}

/// Every opaque core in `rects`, in rect order, with the interior it
/// occludes.
///
/// Refuses, before [`classify`] judges the rest: a rect with a shape (its
/// silhouette is a field, not its box), a rect with a blur, and a rect inside
/// a [`GroupComposite`] range `[start, end)` composited below alpha 1 (its
/// pixels reach the target through that alpha).
pub fn cores(
    rects: &[RectEntry],
    paints: &PaintTable,
    clips: &ClipTable,
    groups: &[GroupComposite],
    aa: f32,
) -> Vec<(u32, Occluder)> {
    let mut translucent = vec![false; rects.len()];
    for group in groups
        .iter()
        .filter(|group| group.alpha < 1.0 || group.alpha.is_nan())
    {
        let end = (group.end as usize).min(rects.len());
        let start = (group.start as usize).min(end);
        translucent[start..end].fill(true);
    }
    let mut found = Vec::new();
    for (index, rect) in rects.iter().enumerate() {
        if translucent[index] {
            continue;
        }
        let entry = paints.resolve(rect.paint);
        if entry.shape.count != 0 || !paints.blurs(entry).is_empty() {
            continue;
        }
        let alpha = fill_alpha(paints, entry.fill);
        let boxes = clips.resolve(rect.clip).boxes();
        if let Some(interior) = classify(rect, entry.corners, alpha, boxes, aa) {
            found.push((index as u32, interior));
        }
    }
    found
}

/// Appends to `out` the parts of `a` outside `b`: none when `b` covers `a`,
/// `a` itself when they do not overlap, otherwise up to four strips in this
/// order — the strip above `b` and the strip below it, each `a`'s full width,
/// then the strips left and right of `b`, each the overlap's height.
pub fn subtract(a: Occluder, b: Occluder, out: &mut Vec<Occluder>) {
    // Every edge below is one of `a`'s or `b`'s, selected by `min` or `max`:
    // nothing here computes a coordinate, which is what keeps the edges of
    // adjacent pieces identical (see `Occluder`).
    let left = a.left.max(b.left);
    let top = a.top.max(b.top);
    let right = a.right.min(b.right);
    let bottom = a.bottom.min(b.bottom);
    if !(right > left && bottom > top) {
        out.push(a);
        return;
    }
    if top > a.top {
        out.push(Occluder { bottom: top, ..a });
    }
    if bottom < a.bottom {
        out.push(Occluder { top: bottom, ..a });
    }
    if left > a.left {
        out.push(Occluder {
            left: a.left,
            top,
            right: left,
            bottom,
        });
    }
    if right < a.right {
        out.push(Occluder {
            left: right,
            top,
            right: a.right,
            bottom,
        });
    }
}

/// The visible pieces of every rect, in rect order: each rect's box minus the
/// interior of every core later than it in `cores` (which [`cores`] returns
/// in rect order). A rect with no later core over it yields one piece, its
/// box; a rect a later core covers completely yields none.
///
/// `occludable(index)` answers [`occludable`] for rect `index`; a rect it
/// refuses is kept whole, and so is a rotated rect. A rect whose piece list
/// grows past [`MAX_PIECES_PER_RECT`] after any one cut is also kept whole,
/// even when a later cut would bring the count back under the cap.
///
/// `barriers` is [`barriers`]'s answer, in ascending order: the rects that
/// sample the composited backdrop beneath them. A core later than a barrier
/// never cuts a rect earlier than that barrier, because the barrier's blur
/// reads that rect's pixels inside the core's interior before the core is
/// drawn over them (`dashpaint::Painter::paint`, "the backdrop barrier"). Pass
/// an empty slice for a scene with no backdrop sampling.
pub fn pieces(
    rects: &[RectEntry],
    cores: &[(u32, Occluder)],
    barriers: &[u32],
    occludable: impl Fn(usize) -> bool,
) -> Vec<Piece> {
    let mut out = Vec::with_capacity(rects.len());
    let mut current = Vec::new();
    let mut next = Vec::new();
    for (index, rect) in rects.iter().enumerate() {
        // The box's right and bottom edges are computed here once, as
        // `x + w` and `y + h` in `f32` — the expression a painter evaluates
        // for the box — and every piece edge on the box boundary is a copy of
        // these.
        let whole = Occluder {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.w,
            bottom: rect.y + rect.h,
        };
        current.clear();
        current.push(whole);
        if rect.rotation == 0.0 && occludable(index) {
            let later = cores.partition_point(|&(core, _)| core as usize <= index);
            // Only the cores before the next barrier: that barrier samples
            // this rect as it is composited, so a core past it must not
            // remove any part of this rect.
            let until = match barriers.get(barriers.partition_point(|&b| b as usize <= index)) {
                Some(&barrier) => cores.partition_point(|&(core, _)| core < barrier),
                None => cores.len(),
            };
            for &(_, interior) in &cores[later..until.max(later)] {
                next.clear();
                for &part in &current {
                    subtract(part, interior, &mut next);
                }
                std::mem::swap(&mut current, &mut next);
                if current.len() > MAX_PIECES_PER_RECT {
                    current.clear();
                    current.push(whole);
                    break;
                }
                if current.is_empty() {
                    break;
                }
            }
        }
        out.extend(current.iter().map(|part| Piece {
            rect: index as u32,
            left: part.left,
            top: part.top,
            right: part.right,
            bottom: part.bottom,
        }));
    }
    out
}

/// Every rect in `rects` that samples the backdrop beneath it
/// ([`PaintTable::samples_backdrop`]), in ascending order: the barriers
/// [`pieces`] does not let a core cut across.
pub fn barriers(rects: &[RectEntry], paints: &PaintTable) -> Vec<u32> {
    rects
        .iter()
        .enumerate()
        .filter(|(_, rect)| paints.samples_backdrop(paints.resolve(rect.paint)))
        .map(|(index, _)| index as u32)
        .collect()
}

/// Appends to `out`, in ascending order, every rect index whose pieces differ
/// between `previous` and `current` — a different count or a different
/// extent. Both slices are in rect order, as [`pieces`] returns them. A rect
/// with no piece in either (covered completely both times) is unchanged.
pub fn changed_rects(previous: &[Piece], current: &[Piece], out: &mut Vec<u32>) {
    let (mut p, mut c) = (previous, current);
    while !p.is_empty() || !c.is_empty() {
        let rect = match (p.first(), c.first()) {
            (Some(a), Some(b)) => a.rect.min(b.rect),
            (Some(a), None) => a.rect,
            (None, Some(b)) => b.rect,
            (None, None) => unreachable!("the loop condition holds one non-empty slice"),
        };
        let p_len = p.iter().take_while(|piece| piece.rect == rect).count();
        let c_len = c.iter().take_while(|piece| piece.rect == rect).count();
        if p[..p_len] != c[..c_len] {
            out.push(rect);
        }
        p = &p[p_len..];
        c = &c[c_len..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::committed::{
        Blur, BlurKind, ClipIndex, EntryParts, FillSpec, ImageFill, Mat23, PaintIndex, ScaleMode,
        Shadow, ShadowKind, Stroke, StrokeAlign, Vec2, VectorField,
    };

    /// An occluder from an origin and a size, for the tests whose values are
    /// exact in `f32`; `adjacent_pieces_share_bit_identical_edges` builds its
    /// edges directly.
    fn o(x: f32, y: f32, w: f32, h: f32) -> Occluder {
        Occluder {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }

    /// A piece from an origin and a size, on the same terms as [`o`].
    fn piece(rect: u32, x: f32, y: f32, w: f32, h: f32) -> Piece {
        Piece {
            rect,
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32, opacity: f32, rotation: f32) -> RectEntry {
        RectEntry {
            x,
            y,
            w,
            h,
            paint: PaintIndex(0),
            clip: ClipIndex::UNCLIPPED,
            opacity,
            rotation,
            rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
        }
    }

    fn painted(paint: PaintIndex, x: f32, y: f32, w: f32, h: f32) -> RectEntry {
        RectEntry {
            paint,
            ..rect(x, y, w, h, 1.0, 0.0)
        }
    }

    fn whole(index: u32, r: &RectEntry) -> Piece {
        piece(index, r.x, r.y, r.w, r.h)
    }

    const SQUARE: CornerRadii = CornerRadii {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };
    const R8: CornerRadii = CornerRadii {
        top_left: 8.0,
        top_right: 8.0,
        bottom_right: 8.0,
        bottom_left: 8.0,
    };
    /// The largest `f32` below 1, one unit in the last place under it. Not
    /// `1.0 - f32::EPSILON`: below 1 the spacing is half of `f32::EPSILON`, so
    /// that value is the second largest.
    const BELOW_ONE: f32 = f32::from_bits(1.0f32.to_bits() - 1);

    const OPAQUE: Color = Color {
        r: 0.2,
        g: 0.4,
        b: 0.6,
        a: 1.0,
    };
    const HALF: Color = Color {
        r: 0.2,
        g: 0.4,
        b: 0.6,
        a: 0.5,
    };

    /// A piece's edges as `[left, top, right, bottom]`.
    fn edges(p: &Piece) -> [f32; 4] {
        [p.left, p.top, p.right, p.bottom]
    }

    /// A core interior from its four edges.
    fn interior(left: f32, top: f32, right: f32, bottom: f32) -> Occluder {
        Occluder {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Every piece edge is a value copied from the
    /// rect's own edges or from a core interior's edges, never re-derived by
    /// arithmetic. So two pieces that meet share an edge bit for bit (no
    /// hairline gap and no double blend on a translucent rect), and an edge on
    /// the box boundary equals the box's edge bit for bit, which is what lets
    /// a packer test "this edge lies on the box edge" with `==`.
    ///
    /// The coordinates are chosen to be inexact in `f32`, so a re-derived edge
    /// (`x + (right - x)`) rounds away from the edge it was derived from.
    #[test]
    fn adjacent_pieces_share_bit_identical_edges() {
        let r = rect(0.1, 0.1, 1000.3, 1000.3, 1.0, 0.0);
        // The box's own edges: right and bottom are `x + w` and `y + h`,
        // computed once in exactly the expression a shader evaluates.
        let (left, top, right, bottom) = (r.x, r.y, r.x + r.w, r.y + r.h);
        // A core in the box's upper-left half: the right strip's width
        // (`right - 366.9`) is not exact in `f32`, so storing it and adding it
        // back to 366.9 does not give `right` again. Found by an `f32` search.
        let (cl, ct, cr, cb) = (133.7_f32, 133.7_f32, 366.9_f32, 366.9_f32);
        let rects = [r, rect(cl, ct, cr - cl, cb - ct, 1.0, 0.0)];
        let p = pieces(&rects, &[(1, interior(cl, ct, cr, cb))], &[], |_| true);
        let under: Vec<[u32; 4]> = p
            .iter()
            .filter(|p| p.rect == 0)
            .map(|p| edges(p).map(f32::to_bits))
            .collect();
        let bits = |e: [f32; 4]| e.map(f32::to_bits);
        assert_eq!(
            under,
            vec![
                bits([left, top, right, ct]),
                bits([left, cb, right, bottom]),
                bits([left, ct, cl, cb]),
                bits([cr, ct, right, cb]),
            ],
            "every piece edge is the box's own edge or the core interior's, bit for bit"
        );
    }

    #[test]
    fn subtracting_a_disjoint_rect_keeps_the_whole() {
        let mut out = vec![];
        subtract(o(0., 0., 10., 10.), o(20., 20., 5., 5.), &mut out);
        assert_eq!(out, vec![o(0., 0., 10., 10.)]);
    }

    #[test]
    fn subtracting_a_full_cover_leaves_nothing() {
        let mut out = vec![];
        subtract(o(2., 2., 4., 4.), o(0., 0., 10., 10.), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn subtracting_an_off_centre_rect_leaves_exactly_these_four_pieces() {
        // outer 10x7, hole at (2,1) 3x2: strips are top, bottom (full width),
        // then left and right beside the hole
        let mut out = vec![];
        subtract(o(0., 0., 10., 7.), o(2., 1., 3., 2.), &mut out);
        assert_eq!(
            out,
            vec![
                o(0., 0., 10., 1.),
                o(0., 3., 10., 4.),
                o(0., 1., 2., 2.),
                o(5., 1., 5., 2.)
            ]
        );
    }

    #[test]
    fn an_occluder_over_a_corner_leaves_two_pieces() {
        // b covers a's top-left 3x3: no top strip, bottom (0,3,10,7), no left,
        // right (3,0,7,3)
        let mut out = vec![];
        subtract(o(0., 0., 10., 10.), o(-5., -5., 8., 8.), &mut out);
        assert_eq!(out, vec![o(0., 3., 10., 7.), o(3., 0., 7., 3.)]);
    }

    #[test]
    fn an_occluder_spanning_the_full_width_leaves_a_top_and_a_bottom_strip() {
        let mut out = vec![];
        subtract(o(0., 0., 10., 10.), o(-1., 4., 12., 2.), &mut out);
        assert_eq!(out, vec![o(0., 0., 10., 4.), o(0., 6., 10., 4.)]);
    }

    #[test]
    fn an_occluder_touching_an_edge_leaves_one_strip() {
        let mut out = vec![];
        subtract(o(0., 0., 10., 10.), o(0., 0., 10., 4.), &mut out);
        assert_eq!(out, vec![o(0., 4., 10., 6.)]);
    }

    #[test]
    fn a_translucent_fill_is_not_a_core_and_an_opaque_one_is() {
        assert!(
            classify(
                &rect(0., 0., 10., 10., 1.0, 0.0),
                SQUARE,
                FillAlpha::Translucent,
                &[],
                1.0
            )
            .is_none()
        );
        assert!(matches!(
            solid_alpha(Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.5
            }),
            FillAlpha::Translucent
        ));
        assert!(matches!(
            solid_alpha(Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0
            }),
            FillAlpha::Opaque
        ));
        assert!(matches!(
            fill_alpha(&PaintTable::default(), PaintKind::NONE),
            FillAlpha::None
        ));
    }

    #[test]
    fn a_gradient_is_a_core_only_when_every_stop_is_opaque() {
        let opaque = [
            GradientStop {
                offset: 0.0,
                color: Color {
                    r: 0.,
                    g: 0.,
                    b: 0.,
                    a: 1.0,
                },
            },
            GradientStop {
                offset: 1.0,
                color: Color {
                    r: 1.,
                    g: 1.,
                    b: 1.,
                    a: 1.0,
                },
            },
        ];
        assert!(matches!(stops_alpha(&opaque), FillAlpha::Opaque));
        let one_translucent = [
            opaque[0],
            GradientStop {
                offset: 1.0,
                color: Color {
                    r: 1.,
                    g: 1.,
                    b: 1.,
                    a: 0.99,
                },
            },
        ];
        assert!(matches!(
            stops_alpha(&one_translucent),
            FillAlpha::Translucent
        ));
    }

    /// An image fill is judged translucent whatever its pixels hold: the pass
    /// does not read image payloads.
    #[test]
    fn an_image_fill_is_never_opaque() {
        let mut paints = PaintTable::new();
        let kind = paints.intern_fill(&FillSpec::Image(ImageFill {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        assert_eq!(fill_alpha(&paints, kind), FillAlpha::Translucent);
        let solid = paints.intern_fill(&FillSpec::Solid { color: OPAQUE });
        assert_eq!(fill_alpha(&paints, solid), FillAlpha::Opaque);
    }

    #[test]
    fn a_rect_with_a_shape_or_under_a_translucent_group_is_not_a_core() {
        let mut paints = PaintTable::new();
        let opaque = paints.push_solid(OPAQUE);
        let fill = paints.intern_fill(&FillSpec::Solid { color: OPAQUE });
        let shaped = paints.push_with(
            PaintEntry {
                fill,
                ..PaintEntry::default()
            },
            EntryParts {
                shape: Some(VectorField {
                    image: 0,
                    atlas_rect: [0, 0, 16, 16],
                    plane_bounds: [0.0, 0.0, 40.0, 40.0],
                    distance_range: 4.0,
                }),
                ..EntryParts::default()
            },
        );
        let blurred = paints.push_with(
            PaintEntry {
                fill,
                ..PaintEntry::default()
            },
            EntryParts {
                blurs: &[Blur {
                    kind: BlurKind::Backdrop,
                    radius: 4.0,
                }],
                ..EntryParts::default()
            },
        );
        let clips = ClipTable::new();

        // The control: the same box with a plain opaque fill is a core, so
        // each refusal below is the property it names and nothing else.
        let plain = [painted(opaque, 0., 0., 40., 40.)];
        assert_eq!(
            cores(&plain, &paints, &clips, &[], 1.0),
            vec![(0, o(1., 1., 38., 38.))]
        );

        let rects = [
            painted(shaped, 0., 0., 40., 40.),
            painted(blurred, 0., 0., 40., 40.),
            painted(opaque, 0., 0., 40., 40.),
        ];
        assert_eq!(
            cores(&rects, &paints, &clips, &[], 1.0),
            vec![(2, o(1., 1., 38., 38.))],
            "a shaped rect and a blurred rect are not cores"
        );

        let two = [
            painted(opaque, 0., 0., 40., 40.),
            painted(opaque, 0., 0., 40., 40.),
        ];
        let translucent = [GroupComposite {
            start: 0,
            end: 1,
            alpha: 0.5,
        }];
        assert_eq!(
            cores(&two, &paints, &clips, &translucent, 1.0),
            vec![(1, o(1., 1., 38., 38.))],
            "rect 0 is inside the group composited at 0.5; rect 1 is past its exclusive end"
        );
        let full = [GroupComposite {
            start: 0,
            end: 2,
            alpha: 1.0,
        }];
        assert_eq!(
            cores(&two, &paints, &clips, &full, 1.0).len(),
            2,
            "a group composited at alpha 1 refuses nothing"
        );
    }

    #[test]
    fn a_core_reads_its_clip_from_the_clip_table() {
        let mut paints = PaintTable::new();
        let opaque = paints.push_solid(OPAQUE);
        let mut clips = ClipTable::new();
        let clip = clips.push(&[ClipBox {
            x: 5.,
            y: 5.,
            w: 20.,
            h: 20.,
            corners: SQUARE,
        }]);
        let rects = [RectEntry {
            clip,
            ..painted(opaque, 0., 0., 40., 40.)
        }];
        assert_eq!(
            cores(&rects, &paints, &clips, &[], 1.0),
            vec![(0, o(6., 6., 18., 18.))]
        );
    }

    #[test]
    fn a_rect_whose_ink_reaches_past_its_box_is_never_occluded() {
        let mut paints = PaintTable::new();
        // Translucent, so none of these rects is a core itself and the one
        // core in the scene is the last rect.
        let fill = paints.intern_fill(&FillSpec::Solid { color: HALF });
        let entry = PaintEntry {
            fill,
            ..PaintEntry::default()
        };
        let stroked = |paints: &mut PaintTable, align| {
            paints.push_with(
                entry,
                EntryParts {
                    stroke: Some(Stroke {
                        width: 2.0,
                        align,
                        color: OPAQUE,
                    }),
                    ..EntryParts::default()
                },
            )
        };
        let outside = stroked(&mut paints, StrokeAlign::Outside);
        let center = stroked(&mut paints, StrokeAlign::Center);
        let inside = stroked(&mut paints, StrokeAlign::Inside);
        let shadowed = paints.push_with(
            entry,
            EntryParts {
                shadows: &[Shadow {
                    kind: ShadowKind::Drop,
                    offset: Vec2 { x: 2.0, y: 2.0 },
                    blur: 4.0,
                    spread: 0.0,
                    color: HALF,
                }],
                ..EntryParts::default()
            },
        );
        let blurred = paints.push_with(
            entry,
            EntryParts {
                blurs: &[Blur {
                    kind: BlurKind::Backdrop,
                    radius: 4.0,
                }],
                ..EntryParts::default()
            },
        );
        let shaped = paints.push_with(
            entry,
            EntryParts {
                shape: Some(VectorField {
                    image: 0,
                    atlas_rect: [0, 0, 16, 16],
                    plane_bounds: [-4.0, -4.0, 14.0, 14.0],
                    distance_range: 4.0,
                }),
                ..EntryParts::default()
            },
        );
        let plain = paints.push(entry);

        let is = |index: PaintIndex| occludable(paints.resolve(index), &paints);
        assert!(!is(outside), "an outside stroke");
        assert!(!is(center), "a centred stroke");
        assert!(!is(shadowed), "a drop shadow");
        assert!(!is(blurred), "a blur");
        assert!(!is(shaped), "a shape");
        assert!(is(inside), "an inside stroke stays inside the box");
        assert!(is(plain), "a plain fill");

        // Every rect sits under one later core that covers all of it; only the
        // occludable ones are cut away.
        let core = paints.push_solid(OPAQUE);
        let paints_in_order = [outside, center, inside, shadowed, blurred, shaped, plain];
        let mut rects: Vec<RectEntry> = paints_in_order
            .iter()
            .map(|&p| painted(p, 10., 10., 10., 10.))
            .collect();
        rects.push(painted(core, 0., 0., 40., 40.));
        let found = cores(&rects, &paints, &ClipTable::new(), &[], 1.0);
        assert_eq!(found, vec![(7, o(1., 1., 38., 38.))]);
        let p = pieces(&rects, &found, &[], |i| {
            occludable(paints.resolve(rects[i].paint), &paints)
        });
        let survivors: Vec<u32> = p.iter().map(|p| p.rect).collect();
        assert_eq!(
            survivors,
            vec![0, 1, 3, 4, 5, 7],
            "the inside-stroked rect (2) and the plain rect (6) are fully covered"
        );
        for piece in &p {
            assert_eq!(*piece, whole(piece.rect, &rects[piece.rect as usize]));
        }
    }

    #[test]
    fn a_node_below_opacity_one_is_not_a_core() {
        assert!(
            classify(
                &rect(0., 0., 10., 10., 0.5, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            )
            .is_none()
        );
    }

    /// The alpha and opacity tests are exact: `BELOW_ONE`, the largest
    /// `f32` below 1, is not a core. Any threshold below 1 would pass the 0.5 fixtures the other
    /// tests use.
    #[test]
    fn a_fill_or_node_just_below_alpha_one_is_not_a_core() {
        let near = Color {
            a: BELOW_ONE,
            ..OPAQUE
        };
        assert_eq!(solid_alpha(near), FillAlpha::Translucent, "a solid fill");
        let stop = |offset, color| GradientStop { offset, color };
        assert_eq!(
            stops_alpha(&[stop(0.0, near), stop(1.0, OPAQUE)]),
            FillAlpha::Translucent,
            "a gradient whose first stop is just below alpha 1"
        );
        assert_eq!(
            stops_alpha(&[stop(0.0, OPAQUE), stop(1.0, OPAQUE)]),
            FillAlpha::Opaque,
            "the control: the same gradient with both stops at alpha 1"
        );
        let square = rect(0., 0., 40., 40., 1.0, 0.0);
        assert!(classify(&square, SQUARE, FillAlpha::Opaque, &[], 1.0).is_some());
        assert!(
            classify(
                &rect(0., 0., 40., 40., BELOW_ONE, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            )
            .is_none(),
            "a node at opacity just below 1"
        );
    }

    #[test]
    fn a_gradient_with_no_stops_is_not_opaque() {
        assert_eq!(stops_alpha(&[]), FillAlpha::Translucent);
    }

    #[test]
    fn a_nan_opacity_or_a_nan_group_alpha_is_not_a_core() {
        assert!(
            classify(
                &rect(0., 0., 40., 40., f32::NAN, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            )
            .is_none(),
            "a node at NaN opacity"
        );
        let mut paints = PaintTable::new();
        let opaque = paints.push_solid(OPAQUE);
        let rects = [painted(opaque, 0., 0., 40., 40.)];
        let clips = ClipTable::new();
        assert_eq!(
            cores(&rects, &paints, &clips, &[], 1.0).len(),
            1,
            "the control: with no group the rect is a core"
        );
        let nan = [GroupComposite {
            start: 0,
            end: 1,
            alpha: f32::NAN,
        }];
        assert!(
            cores(&rects, &paints, &clips, &nan, 1.0).is_empty(),
            "a rect inside a group composited at NaN alpha"
        );
        let near = [GroupComposite {
            start: 0,
            end: 1,
            alpha: BELOW_ONE,
        }];
        assert!(
            cores(&rects, &paints, &clips, &near, 1.0).is_empty(),
            "a rect inside a group composited just below alpha 1"
        );
    }

    /// The interior is inset by the LARGEST of the four radii, wherever it
    /// sits. Each case puts the largest radius at a different corner.
    #[test]
    fn a_core_with_four_different_radii_is_inset_by_the_largest() {
        let r = |top_left, top_right, bottom_right, bottom_left| CornerRadii {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        };
        for corners in [
            r(8., 2., 4., 6.),
            r(2., 8., 4., 6.),
            r(2., 4., 8., 6.),
            r(2., 4., 6., 8.),
        ] {
            assert_eq!(
                classify(
                    &rect(0., 0., 40., 40., 1.0, 0.0),
                    corners,
                    FillAlpha::Opaque,
                    &[],
                    1.0
                ),
                Some(o(9., 9., 22., 22.)),
                "{corners:?}: inset by 8 plus the band of 1"
            );
        }
    }

    /// `f32::max` drops a NaN operand, so a NaN radius must be refused
    /// explicitly: the painters' SDF yields NaN coverage at that corner, and
    /// an interior computed as if the corner were square would cut a hole.
    #[test]
    fn a_core_or_a_clip_with_a_nan_radius_occludes_nothing() {
        let square = rect(0., 0., 40., 40., 1.0, 0.0);
        let clip = |corners| {
            [ClipBox {
                x: 5.,
                y: 5.,
                w: 20.,
                h: 20.,
                corners,
            }]
        };
        // Each of the four radii in turn, so a guard that tests only one
        // corner is caught.
        let nans = [
            CornerRadii {
                top_left: f32::NAN,
                ..SQUARE
            },
            CornerRadii {
                top_right: f32::NAN,
                ..SQUARE
            },
            CornerRadii {
                bottom_right: f32::NAN,
                ..SQUARE
            },
            CornerRadii {
                bottom_left: f32::NAN,
                ..SQUARE
            },
        ];
        assert!(
            classify(&square, SQUARE, FillAlpha::Opaque, &clip(SQUARE), 1.0).is_some(),
            "the control: a square core with a square clip is a core"
        );
        for corners in nans {
            assert!(
                classify(&square, corners, FillAlpha::Opaque, &[], 1.0).is_none(),
                "{corners:?} on the core"
            );
            assert!(
                classify(&square, corners, FillAlpha::Opaque, &clip(SQUARE), 1.0).is_none(),
                "{corners:?} on the core, which also has a clip box: the clip's `max` and \
                 `min` must not drop the core's NaN edges"
            );
            assert!(
                classify(&square, SQUARE, FillAlpha::Opaque, &clip(corners), 1.0).is_none(),
                "{corners:?} on the core's clip box"
            );
        }
    }

    #[test]
    fn a_square_core_occludes_its_box_shrunk_by_the_band() {
        assert_eq!(
            classify(
                &rect(0., 0., 40., 40., 1.0, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            ),
            Some(o(1., 1., 38., 38.))
        );
    }

    #[test]
    fn a_rounded_core_occludes_its_box_shrunk_by_the_radius_and_the_band() {
        assert_eq!(
            classify(
                &rect(0., 0., 40., 40., 1.0, 0.0),
                R8,
                FillAlpha::Opaque,
                &[],
                1.0
            ),
            Some(o(9., 9., 22., 22.))
        );
    }

    #[test]
    fn a_clipped_core_occludes_only_inside_its_clip_inset_like_a_rect() {
        // a clip edge is anti-aliased and a clip box has corners of its own:
        // inset by its largest radius plus aa
        let square = [ClipBox {
            x: 5.,
            y: 5.,
            w: 20.,
            h: 20.,
            corners: SQUARE,
        }];
        assert_eq!(
            classify(
                &rect(0., 0., 40., 40., 1.0, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &square,
                1.0
            ),
            Some(o(6., 6., 18., 18.))
        );
        let rounded = [ClipBox {
            x: 5.,
            y: 5.,
            w: 20.,
            h: 20.,
            corners: CornerRadii {
                top_left: 4.0,
                top_right: 4.0,
                bottom_right: 4.0,
                bottom_left: 4.0,
            },
        }];
        assert_eq!(
            classify(
                &rect(0., 0., 40., 40., 1.0, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &rounded,
                1.0
            ),
            Some(o(10., 10., 10., 10.))
        );
    }

    #[test]
    fn a_core_too_small_for_its_own_band_occludes_nothing() {
        assert!(
            classify(
                &rect(0., 0., 2., 2., 1.0, 0.0),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            )
            .is_none()
        );
    }

    #[test]
    fn a_rotated_rect_neither_occludes_nor_is_occluded() {
        assert!(
            classify(
                &rect(0., 0., 40., 40., 1.0, 0.3),
                SQUARE,
                FillAlpha::Opaque,
                &[],
                1.0
            )
            .is_none()
        );
        let rects = [
            rect(0., 0., 10., 10., 1.0, 0.3),
            rect(0., 0., 10., 10., 1.0, 0.0),
        ];
        let p = pieces(&rects, &[(1, o(1., 1., 8., 8.))], &[], |_| true);
        assert_eq!(
            p,
            vec![piece(0, 0., 0., 10., 10.), piece(1, 0., 0., 10., 10.)],
            "the rotated rect stays whole, and so does the last rect"
        );
    }

    #[test]
    fn a_rect_that_would_split_past_the_cap_keeps_its_whole_quad() {
        // Later cores in a row across a wide rect. The first cut leaves four
        // pieces and each further cut splits the right strip in two, so `k`
        // cuts leave `k + 3` pieces: `MAX - 2` cuts leave one past the cap.
        let past = MAX_PIECES_PER_RECT - 2;
        let mut rects = vec![rect(0., 0., 10. * past as f32 + 10., 10., 1.0, 0.0)];
        let mut cores = vec![];
        for i in 0..past {
            rects.push(rect(10. * i as f32 + 2., 2., 4., 6., 1.0, 0.0));
            cores.push((i as u32 + 1, o(10. * i as f32 + 3., 3., 2., 4.)));
        }
        let p = pieces(&rects, &cores, &[], |_| true);
        assert_eq!(
            p.iter().filter(|p| p.rect == 0).collect::<Vec<_>>(),
            vec![&piece(0, 0., 0., rects[0].w, 10.)]
        );
        // The control: one cut fewer leaves exactly the cap, and it is cut.
        let p = pieces(&rects[..past], &cores[..past - 1], &[], |_| true);
        assert_eq!(
            p.iter().filter(|p| p.rect == 0).count(),
            MAX_PIECES_PER_RECT
        );
    }

    #[test]
    fn the_commit_dirties_a_rect_whose_pieces_changed() {
        let before = vec![piece(0, 0., 0., 10., 1.), piece(1, 2., 1., 3., 2.)];
        let after = vec![piece(0, 0., 0., 10., 2.), piece(1, 2., 2., 3., 2.)];
        let mut out = vec![];
        changed_rects(&before, &after, &mut out);
        assert_eq!(out, vec![0, 1]);
        let mut same = vec![];
        changed_rects(&before, &before, &mut same);
        assert!(same.is_empty());
    }

    #[test]
    fn a_rect_whose_piece_count_changed_or_that_vanished_is_changed() {
        let a = piece(0, 0., 0., 10., 10.);
        let b = Piece { rect: 1, ..a };
        let c = Piece { rect: 2, ..a };
        let split = [
            Piece {
                rect: 1,
                bottom: 4.,
                ..a
            },
            Piece {
                rect: 1,
                top: 6.,
                ..a
            },
        ];
        // rect 1 goes from one piece to two; rect 2 is fully covered after.
        let before = [a, b, c];
        let after = [a, split[0], split[1]];
        let mut out = vec![];
        changed_rects(&before, &after, &mut out);
        assert_eq!(out, vec![1, 2]);
        let mut back = vec![];
        changed_rects(&after, &before, &mut back);
        assert_eq!(back, vec![1, 2]);
    }

    #[test]
    fn an_earlier_core_never_occludes_a_later_rect() {
        let rects = [
            rect(0., 0., 10., 10., 1.0, 0.0),
            rect(0., 0., 10., 10., 1.0, 0.0),
        ];
        let p = pieces(&rects, &[(0, o(1., 1., 8., 8.))], &[], |_| true);
        assert_eq!(
            p,
            vec![piece(0, 0., 0., 10., 10.), piece(1, 0., 0., 10., 10.)]
        );
    }

    #[test]
    fn a_later_core_cuts_the_rect_beneath_it_into_pieces() {
        let rects = [
            rect(0., 0., 10., 7., 1.0, 0.0),
            rect(2., 1., 3., 2., 1.0, 0.0),
        ];
        let p = pieces(&rects, &[(1, o(2., 1., 3., 2.))], &[], |_| true);
        let under: Vec<_> = p
            .iter()
            .filter(|p| p.rect == 0)
            .map(|p| Occluder {
                left: p.left,
                top: p.top,
                right: p.right,
                bottom: p.bottom,
            })
            .collect();
        assert_eq!(
            under,
            vec![
                o(0., 0., 10., 1.),
                o(0., 3., 10., 4.),
                o(0., 1., 2., 2.),
                o(5., 1., 5., 2.)
            ]
        );
        assert_eq!(
            p.iter().filter(|p| p.rect == 1).collect::<Vec<_>>(),
            vec![&piece(1, 2., 1., 3., 2.)],
            "the core itself is whole"
        );
    }

    #[test]
    fn a_scene_with_no_core_keeps_every_rect_as_one_piece() {
        let mut paints = PaintTable::new();
        let translucent = paints.push_solid(HALF);
        let rects = [
            painted(translucent, 0., 0., 40., 40.),
            painted(translucent, 5., 5., 10., 10.),
            painted(translucent, 0., 0., 40., 40.),
        ];
        let found = cores(&rects, &paints, &ClipTable::new(), &[], 1.0);
        assert!(found.is_empty(), "a fill at alpha 0.5 is not a core");
        let p = pieces(&rects, &found, &[], |_| true);
        let expected: Vec<Piece> = rects
            .iter()
            .enumerate()
            .map(|(i, r)| whole(i as u32, r))
            .collect();
        assert_eq!(p, expected);
    }

    #[test]
    fn a_later_core_that_covers_the_whole_rect_leaves_it_no_piece() {
        let rects = [
            rect(10., 10., 10., 10., 1.0, 0.0),
            rect(0., 0., 40., 40., 1.0, 0.0),
        ];
        let p = pieces(&rects, &[(1, o(1., 1., 38., 38.))], &[], |_| true);
        assert_eq!(p, vec![whole(1, &rects[1])]);
    }

    #[test]
    fn a_translucent_rect_on_top_occludes_nothing() {
        let mut paints = PaintTable::new();
        let opaque = paints.push_solid(OPAQUE);
        let translucent = paints.push_solid(HALF);
        let rects = [
            painted(opaque, 0., 0., 40., 40.),
            painted(translucent, 0., 0., 40., 40.),
        ];
        let found = cores(&rects, &paints, &ClipTable::new(), &[], 1.0);
        assert_eq!(found, vec![(0, o(1., 1., 38., 38.))]);
        let p = pieces(&rects, &found, &[], |_| true);
        assert_eq!(p, vec![whole(0, &rects[0]), whole(1, &rects[1])]);
    }

    #[test]
    fn a_rounded_core_cuts_only_its_inset_interior() {
        let mut paints = PaintTable::new();
        let fill = paints.intern_fill(&FillSpec::Solid { color: OPAQUE });
        let rounded = paints.push(PaintEntry {
            fill,
            corners: R8,
            ..PaintEntry::default()
        });
        let plain = paints.push_solid(HALF);
        let rects = [
            painted(plain, 0., 0., 40., 40.),
            painted(rounded, 0., 0., 40., 40.),
        ];
        let found = cores(&rects, &paints, &ClipTable::new(), &[], 1.0);
        assert_eq!(found, vec![(1, o(9., 9., 22., 22.))]);
        let p = pieces(&rects, &found, &[], |_| true);
        let under: Vec<_> = p
            .iter()
            .filter(|p| p.rect == 0)
            .map(|p| Occluder {
                left: p.left,
                top: p.top,
                right: p.right,
                bottom: p.bottom,
            })
            .collect();
        assert_eq!(
            under,
            vec![
                o(0., 0., 40., 9.),
                o(0., 31., 40., 9.),
                o(0., 9., 9., 22.),
                o(31., 9., 9., 22.)
            ]
        );
    }

    /// A rect that samples the backdrop beneath it (a backdrop blur) is a
    /// barrier: every rect before it must be composited, whole, when it is
    /// drawn. A core after the barrier therefore never cuts a rect before
    /// it, because the blur's kernel reads pixels inside that core's
    /// interior before the core covers them.
    #[test]
    fn a_core_never_cuts_a_rect_across_a_backdrop_barrier() {
        let mut paints = PaintTable::new();
        let translucent = paints.push_solid(HALF);
        let core = paints.push_solid(OPAQUE);
        let blurred = |paints: &mut PaintTable, kind| {
            let fill = paints.intern_fill(&FillSpec::Solid { color: HALF });
            paints.push_with(
                PaintEntry {
                    fill,
                    ..PaintEntry::default()
                },
                EntryParts {
                    blurs: &[Blur { kind, radius: 4.0 }],
                    ..EntryParts::default()
                },
            )
        };
        let backdrop = blurred(&mut paints, BlurKind::Backdrop);
        let layer = blurred(&mut paints, BlurKind::Layer);
        let run = |rects: &[RectEntry]| {
            let found = cores(rects, &paints, &ClipTable::new(), &[], 1.0);
            let barriers = barriers(rects, &paints);
            let p = pieces(rects, &found, &barriers, |i| {
                occludable(paints.resolve(rects[i].paint), &paints)
            });
            (barriers, p)
        };
        let count = |p: &[Piece], rect: u32| p.iter().filter(|p| p.rect == rect).count();

        // Rect 0 beneath, rect 1 the backdrop blur, rect 2 the core over rect 0.
        let across = [
            painted(translucent, 0., 0., 40., 40.),
            painted(backdrop, 50., 50., 10., 10.),
            painted(core, 0., 0., 40., 40.),
        ];
        let (barriers, p) = run(&across);
        assert_eq!(
            barriers,
            vec![1],
            "the backdrop-blurred rect is the one barrier"
        );
        assert_eq!(
            p,
            vec![
                whole(0, &across[0]),
                whole(1, &across[1]),
                whole(2, &across[2])
            ],
            "the core lies past the barrier, so the rect before the barrier stays whole"
        );

        // Control: a layer blur samples nothing beneath it and is no barrier.
        let layered = [across[0], painted(layer, 50., 50., 10., 10.), across[2]];
        let (barriers, p) = run(&layered);
        assert!(barriers.is_empty());
        assert_eq!(
            count(&p, 0),
            4,
            "with no barrier the core cuts the rect into four strips"
        );

        // A core before the barrier still cuts the rect beneath it, and a rect
        // after the barrier is still cut by a later core.
        let both_sides = [
            painted(translucent, 0., 0., 40., 40.),
            painted(core, 0., 0., 40., 40.),
            painted(backdrop, 50., 50., 10., 10.),
            painted(translucent, 0., 0., 40., 40.),
            painted(core, 0., 0., 40., 40.),
        ];
        let (barriers, p) = run(&both_sides);
        assert_eq!(barriers, vec![2]);
        assert_eq!(
            count(&p, 0),
            4,
            "rect 0 is cut by the core at 1, which lies before the barrier"
        );
        assert_eq!(
            count(&p, 1),
            1,
            "the core at 1 lies before the barrier, so the core at 4 leaves it whole"
        );
        assert_eq!(
            count(&p, 3),
            4,
            "rect 3 lies after the barrier and is cut by the core at 4"
        );
        assert_eq!(count(&p, 4), 1);
    }

    /// An image fill is never a core (its pixels are not read), but an
    /// image-filled rect may itself be cut: a piece restricts only its quad,
    /// and the image is still sampled against the box.
    #[test]
    fn an_image_filled_rect_is_cut_but_never_cuts() {
        let mut paints = PaintTable::new();
        let image = paints.intern_fill(&FillSpec::Image(ImageFill {
            image: 0,
            scale_mode: ScaleMode::Fill,
            transform: Mat23::IDENTITY,
            tile_scale: 1.0,
        }));
        let image = paints.push(PaintEntry {
            fill: image,
            ..PaintEntry::default()
        });
        let core = paints.push_solid(OPAQUE);
        let rects = [
            painted(core, 0., 0., 40., 40.),
            painted(image, 0., 0., 40., 40.),
            painted(core, 0., 0., 40., 40.),
        ];
        let found = cores(&rects, &paints, &ClipTable::new(), &[], 1.0);
        assert_eq!(
            found.iter().map(|&(i, _)| i).collect::<Vec<_>>(),
            vec![0, 2],
            "the image-filled rect is no core"
        );
        let p = pieces(&rects, &found, &[], |i| {
            occludable(paints.resolve(rects[i].paint), &paints)
        });
        assert_eq!(
            p.iter().filter(|p| p.rect == 1).count(),
            4,
            "the image-filled rect is cut by the core above it"
        );
    }
}
