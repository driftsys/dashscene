//! The per-instance struct and the buffer of them — the lean painter's
//! output, and what layer 1 of epic #569's verification net is stated over.
//!
//! # Why this is the painter's output
//!
//! `docs/specification/03-target-hardware-rules.md` R-T4 bounds per-frame CPU
//! cost to "dirty-range instance-buffer upload from the rect table +
//! submission. Nothing else." If that is the whole of the painter's frame,
//! then the instance buffer *is* what the painter produces, and the GPU is a
//! pure function of it and of the boundary-B tables its rows index — so the
//! largest class of painter defect (a dropped clip, a wrong paint row, a wrong
//! draw order, a group applied to the wrong set) is a data defect, testable
//! bit-exactly on a runner with no GPU. What a layer-1 golden pins is the
//! rows, not the parameters behind them; a wrong *parameter* is a defect in
//! the table, which boundary B's own tests own.
//!
//! # One ordered stream, tagged by kind
//!
//! Every quad this painter draws is one [`Instance`] in one buffer, in draw
//! order — `docs/decisions/instance-buffer-contract.md` D1 for why, and D5 for
//! the order within one node.
//!
//! # Nothing here is wgpu-specific
//!
//! Story #578's second consumer is the Unity painter, which epic #569 plans as
//! instanced SDF quads too (how that painter reaches the GPU is still
//! **proposed** — `docs/decisions/unity-painter-uses-brg.md`). This struct
//! names no wgpu type, and the rules it follows are the ones story #578 set
//! for anything crossing a language seam: `#[repr(C)]`, fixed-width integers,
//! no `bool`, no payload enums, no nested collections.

use dashpaint::RectEntry;

/// Which primitive one [`Instance`] draws — the tag half of a tag-plus-row
/// form, the same idiom [`dashpaint::PaintKind`] uses.
///
/// `#[repr(u32)]` pins the discriminants as the width a shader reads them at.
/// It is documentation rather than a load-bearing guarantee: [`Instance::kind`]
/// is a plain `u32`, not this type, so that the struct stays a plain-old-data
/// row a consumer can cast to bytes without an enum's validity rule getting in
/// the way. Nothing transmutes an [`InstanceKind`], and the only conversions
/// are the explicit ones below.
///
/// Glyph quads are **not** here yet. A glyph's texel rectangle is a coordinate
/// in the painter's *residency* atlas, not the `atlas_px` boundary B carries,
/// and residency is story #581 — so packing one now would pin coordinates that
/// story is going to reassign. Story #582 adds the variant; where its
/// instances go in the order is already written down on
/// [`InstanceBuffer::spans`], so nothing about that placement is left to
/// decide.
#[repr(u32)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum InstanceKind {
    /// One drop or inner shadow of the rect. `row` is a
    /// [`dashpaint::PaintTable::all_shadows`] row and `tag` is its
    /// [`dashpaint::ShadowKind`].
    ///
    /// First variant because the derived [`Default`] needs one. It is not a
    /// safe resting value and nothing here pretends otherwise: a zeroed
    /// instance names shadow row 0, which is a real row of a real table. What
    /// makes [`Instance::default`] inert is its `opacity` of `0.0`.
    #[default]
    Shadow = 0,
    /// The already-composited backdrop beneath the rect, blurred. `row` is a
    /// [`dashpaint::PaintTable::all_blurs`] row and `tag` is its
    /// [`dashpaint::BlurKind`].
    Backdrop = 1,
    /// One fill layer of the rect — its own fill, or one of the layers
    /// stacked over it. `tag` is a [`dashpaint::PaintTag`] and `row` indexes
    /// the per-kind table that tag names.
    Fill = 2,
    /// The rect's outline stroke. `row` is a
    /// [`dashpaint::PaintTable::all_strokes`] row; `tag` is unused and zero.
    Stroke = 3,
}

impl InstanceKind {
    /// The value this kind occupies in [`Instance::kind`].
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// The name a layer-1 golden prints for this kind — stable, and the only
    /// place the spelling is decided.
    pub const fn name(self) -> &'static str {
        match self {
            InstanceKind::Shadow => "shadow",
            InstanceKind::Backdrop => "backdrop",
            InstanceKind::Fill => "fill",
            InstanceKind::Stroke => "stroke",
        }
    }

    /// The kind an [`Instance::kind`] names.
    ///
    /// # Panics
    ///
    /// Panics on a value no variant carries. The one path that reaches this is
    /// [`InstanceBuffer::dump`], where the alternative is a golden that prints
    /// a placeholder for a kind added without teaching the dump about it — a
    /// silent widening of what a golden covers, which is worse than a failing
    /// test.
    pub const fn from_u32(value: u32) -> Self {
        match value {
            0 => InstanceKind::Shadow,
            1 => InstanceKind::Backdrop,
            2 => InstanceKind::Fill,
            3 => InstanceKind::Stroke,
            _ => panic!("no instance kind carries this value"),
        }
    }
}

/// One quad the painter draws.
///
/// Sixty-four bytes, every member fixed-width, no implicit padding: eight
/// 4-byte scalars followed by two four-float vectors, so both vectors sit at a
/// 16-byte offset and a consumer that binds this as a storage-buffer element
/// needs no repacking.
///
/// # Two fields are index-plus-one, and zero means none
///
/// [`layer`](Self::layer) and [`shape`](Self::shape) both name an *optional*
/// row as `index + 1`, with `0` meaning "none".
/// `docs/decisions/optional-members-are-ranges-of-arity-one.md` chose a range
/// over a sentinel for boundary B; its reason is that boundary B is read by
/// every painter, so a skip rule is a rule each of them has to remember and can
/// diverge on. This struct is one painter's own upload format with one reader,
/// and a range here would cost a second 4-byte member to express an arity that
/// [`kind`](Self::kind) already fixes.
///
/// The bias, rather than a value at the top of the range: `0` is a valid row of
/// every table, so an unbiased index cannot say "none" at all. Biasing puts the
/// absent value at the bottom, where an unwritten member already sits.
///
/// [`row`](Self::row) is deliberately **not** biased — it is never absent, and
/// so a zeroed [`Instance`] does name row 0 of a real table. What makes such an
/// instance inert is its `opacity` of `0.0`, not its rows.
///
/// `Pod`/`Zeroable` since story #580: the frame path casts the rows to bytes
/// and uploads them, which is the whole of what R-T4 budgets for. Both derives
/// are checked, not asserted — `bytemuck` refuses a type with padding, so they
/// hold D2's "no implicit padding" claim by construction rather than by the
/// offset test alone.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    /// Which primitive this quad draws — an [`InstanceKind`].
    pub kind: u32,
    /// The kind-specific discriminator, as the value of the boundary-B enum it
    /// mirrors: a [`dashpaint::PaintTag`] for [`InstanceKind::Fill`], a
    /// [`dashpaint::ShadowKind`] for [`InstanceKind::Shadow`], a
    /// [`dashpaint::BlurKind`] for [`InstanceKind::Backdrop`], and zero for
    /// [`InstanceKind::Stroke`].
    ///
    /// Written by casting that enum, never by a second table of numbers here:
    /// a hand-written copy of the discriminants would survive a reorder in
    /// `dashpaint` and quietly change what this field means to a shader, and
    /// every layer-1 golden would stay green because it pins these numbers
    /// rather than those.
    pub tag: u32,
    /// The row this instance's parameters sit at, in the table `kind` and
    /// `tag` together name.
    pub row: u32,
    /// The baked-vector coverage mask that masks this instance, as a
    /// [`dashpaint::PaintTable::all_shapes`] row **plus one**; `0` for the
    /// implicit parametric shape.
    ///
    /// Carried on the backdrop instance as well as the fill, because a
    /// masked node's backdrop blur is confined to the field's coverage rather
    /// than to its box — the reference painter does the same, and the hero's
    /// frosted panel is exactly that node.
    pub shape: u32,
    /// First clip box, as an index into [`dashpaint::ClipTable::all_boxes`].
    pub clip_offset: u32,
    /// How many clip boxes to intersect, outermost first. Zero = unclipped.
    pub clip_count: u32,
    /// The render-target group layer this instance composites into: an index
    /// into the `groups` slice **plus one**, or `0` for the canvas.
    ///
    /// The innermost enclosing group, since groups nest. This is the field
    /// layer 1's "group applied to the wrong set" claim is stated over.
    pub layer: u32,
    /// The free-path alpha this instance's color is multiplied by —
    /// [`RectEntry::opacity`], carried through unchanged.
    pub opacity: f32,
    /// The quad in document space: `[x, y, w, h]` — the *silhouette* this
    /// instance is stated over.
    ///
    /// For a fill, a stroke and a backdrop that is the node's own box. For a
    /// **drop shadow** it is the node's box grown by the stroke outset, because
    /// a drop shadow casts from what the node draws rather than from its fill
    /// box (`docs/decisions/effects-vocabulary-shadows.md`) and no row this
    /// instance names carries the node's stroke. The remaining terms — the
    /// shadow's offset, its spread and its blur — stay on the shadow row and
    /// are resolved per-painter at draw time (P1). An inner shadow takes no
    /// outset, so its bounds are the node's box.
    pub bounds: [f32; 4],
    /// The rounded-box radii: `[top_left, top_right, bottom_right,
    /// bottom_left]`, grown alongside [`bounds`](Self::bounds) where that is,
    /// with a sharp corner staying sharp.
    ///
    /// Meaningless when [`shape`](Self::shape) names a coverage mask: a
    /// baked-vector node carries its outline in the baked geometry, and the
    /// parametric corners do not apply to it. The field is carried through
    /// rather than zeroed so that the value the node authored stays visible in
    /// a golden.
    ///
    /// The slot a glyph instance will reuse for its atlas texel rectangle
    /// (story #582) — four floats either way, which is what lets text join
    /// this stream without widening the struct.
    pub corners: [f32; 4],
}

impl Instance {
    /// The value [`layer`](Self::layer) and [`shape`](Self::shape) carry when
    /// they name nothing.
    pub const NONE: u32 = 0;
}

/// Where one rect's instances sit in the [`InstanceBuffer`]'s flat array.
///
/// The shape R-T4 asks for: a dirty rect index resolves to a byte range of the
/// buffer, so an upload is one contiguous copy per changed rect rather than a
/// scan.
///
/// The same `(offset, count)` form as every range on boundary B, with one
/// deliberate difference. Boundary B canonicalizes an empty range to `(0, 0)`
/// (`docs/decisions/optional-members-are-ranges-of-arity-one.md` D2) so that
/// two draws-nothing values compare equal. These spans cannot: they partition
/// the buffer, so an empty span still has to record where the next rect's
/// instances begin, and `(0, 0)` would break that. Two draws-nothing spans
/// therefore compare unequal, and nothing here compares them — the rect index
/// is the identity, not the span.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct InstanceSpan {
    /// First instance, as an index into [`InstanceBuffer::instances`].
    pub offset: u32,
    /// How many instances, in draw order. Zero for a rect that draws nothing
    /// — a layout-only container.
    pub count: u32,
}

/// The packed instance buffer: every quad of one frame, in draw order, plus
/// the per-rect index into it.
///
/// This is what [`crate::pack`] produces and what a layer-1 golden pins.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstanceBuffer {
    instances: Vec<Instance>,
    spans: Vec<InstanceSpan>,
}

impl InstanceBuffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every quad of the frame, in draw order. Index order is draw order:
    /// there is no separate depth field, because a second record of the same
    /// fact could disagree with the first.
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// Where each rect's instances sit, index-aligned with the rect table a
    /// frame was packed from — so `spans[i]` is rect `i`'s range, and a dirty
    /// index resolves without a search.
    ///
    /// A rect's instances are contiguous and in the order
    /// `docs/decisions/instance-buffer-contract.md` D5 states. Story #582's
    /// glyph instances go at the end of that list, so they widen a count and
    /// move no boundary — which is why this contract does not change when text
    /// arrives.
    ///
    /// A count of zero means the rect draws nothing *yet*: a layout-only
    /// container, and today also a text node, whose only ink is the glyph
    /// instances story #582 adds.
    pub fn spans(&self) -> &[InstanceSpan] {
        &self.spans
    }

    /// The instances rect `index` draws.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no rect, or if the span runs past the flat
    /// array — the mismatched-tables failure every range on boundary B panics
    /// by name for.
    pub fn rect_instances(&self, index: u32) -> &[Instance] {
        let span = self.spans.get(index as usize).unwrap_or_else(|| {
            panic!(
                "rect {index} has no span: the buffer holds {}",
                self.spans.len()
            )
        });
        let start = span.offset as usize;
        let end = start + span.count as usize;
        self.instances.get(start..end).unwrap_or_else(|| {
            panic!(
                "rect {index}'s span {start}..{end} runs past the buffer's {} instances",
                self.instances.len()
            )
        })
    }

    /// Empties the buffer, keeping its allocation — what a painter does at the
    /// top of a frame, so a steady-state frame reuses this buffer rather than
    /// growing a new one.
    pub fn clear(&mut self) {
        self.instances.clear();
        self.spans.clear();
    }

    /// Opens rect `index`'s span. Every instance pushed until the next
    /// [`begin_rect`](Self::begin_rect) belongs to it.
    ///
    /// # Panics
    ///
    /// Panics unless `index` is the next rect in order. The spans are
    /// index-aligned with the rect table, and a packer that skipped or
    /// repeated one would silently mis-attribute every instance after it.
    pub(crate) fn begin_rect(&mut self, index: u32) {
        assert_eq!(
            index as usize,
            self.spans.len(),
            "spans are index-aligned with the rect table; rect {index} arrived out of order"
        );
        let offset =
            u32::try_from(self.instances.len()).expect("instance buffer exceeds u32::MAX quads");
        self.spans.push(InstanceSpan { offset, count: 0 });
    }

    /// Appends one instance to the rect currently open.
    ///
    /// # Panics
    ///
    /// Panics if no rect is open — an instance belonging to no rect has no
    /// place in a buffer whose spans cover it exactly.
    pub(crate) fn push(&mut self, instance: Instance) {
        let span = self
            .spans
            .last_mut()
            .expect("push needs an open rect: call begin_rect first");
        span.count += 1;
        self.instances.push(instance);
    }

    /// The buffer as a layer-1 golden reads it: one header line, then one line
    /// per rect span, then one line per instance, in draw order.
    ///
    /// Text rather than the raw bytes, for one reason: a golden is reviewed
    /// truth, and a reviewer cannot read 64-byte rows in a diff. Floats print
    /// through `{:?}`, which is Rust's shortest representation that round-trips
    /// — so the text is exact, not rounded, and a one-bit change in a
    /// coordinate changes the line.
    pub fn dump(&self) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        let _ = writeln!(
            out,
            "instances {} rects {}",
            self.instances.len(),
            self.spans.len()
        );
        for (index, span) in self.spans.iter().enumerate() {
            let _ = writeln!(out, "rect {index} at {} count {}", span.offset, span.count);
        }
        for (index, instance) in self.instances.iter().enumerate() {
            let kind = InstanceKind::from_u32(instance.kind);
            let _ = writeln!(
                out,
                "{index:>4} {:<8} tag {} row {} shape {} clip {}..{} layer {} opacity {:?} \
                 bounds {:?} corners {:?}",
                kind.name(),
                instance.tag,
                instance.row,
                instance.shape,
                instance.clip_offset,
                instance.clip_offset + instance.clip_count,
                instance.layer,
                instance.opacity,
                instance.bounds,
                instance.corners,
            );
        }
        out
    }
}

/// A coverage-mask row as [`Instance::shape`] carries it: the row plus one, or
/// [`Instance::NONE`] when the entry names no mask.
///
/// Takes the row rather than the [`dashpaint::VectorField`] itself, because the
/// row is what a shader indexes; the field's parameters never reach the
/// instance buffer.
pub(crate) const fn shape_slot(row: Option<u32>) -> u32 {
    match row {
        Some(row) => row + 1,
        None => Instance::NONE,
    }
}

/// The box every instance of one rect is stated over, before a drop shadow's
/// stroke outset grows it.
pub(crate) const fn bounds_of(rect: &RectEntry) -> [f32; 4] {
    [rect.x, rect.y, rect.w, rect.h]
}
