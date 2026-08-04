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

/// What one [`Instance`] draws — the whole discriminant, sub-kind included.
///
/// # One enum, because two were a collision
///
/// This carried a `kind` and a separate `tag` whose meaning depended on it: a
/// `PaintTag` for a fill, a `ShadowKind` for a shadow, a `BlurKind` for a
/// backdrop. Their discriminants collide — `PaintTag::Solid`,
/// `ShadowKind::Inner` and `BlurKind::Backdrop` are all `1` — so a consumer
/// that read the tag without first checking the kind resolved a shadow against
/// the solid-fill table. Story #580's fragment shader did exactly that, and
/// painted a node's inner shadow with whatever colour sat at that row.
///
/// Merging them makes the mistake unrepresentable rather than forbidden. It is
/// the same argument `docs/decisions/optional-members-are-ranges-of-arity-one.md`
/// used against a sentinel: a rule every consumer has to remember is a rule
/// they can each get wrong, differently.
///
/// # It also removes a silent drift
///
/// The tag used to be written as `enum as u32` and read against a literal in
/// the shader. Reordering a variant in `dashpaint` changed the number, left the
/// literal alone, and nothing caught it — not the compiler, not the goldens,
/// which pin the packer's own output. The packer now maps by an exhaustive
/// `match` on the variant, so a reorder is harmless and a new variant is a
/// compile error.
///
/// `#[repr(u32)]` pins the discriminants as the width a shader reads them at.
/// [`Instance::kind`] is a plain `u32` so the row stays plain-old-data.
#[repr(u32)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum InstanceKind {
    /// A drop shadow: cast behind the node, from its stroked silhouette.
    /// `row` is a [`dashpaint::PaintTable::all_shadows`] row.
    ///
    /// First variant because the derived [`Default`] needs one. It is not a
    /// safe resting value and nothing pretends otherwise: a zeroed instance
    /// names shadow row 0, a real row of a real table. What makes
    /// [`Instance::default`] inert is its `opacity` of `0.0`.
    #[default]
    ShadowDrop = 0,
    /// An inner shadow: drawn over the node's ink, clipped to its shape.
    ShadowInner = 1,
    /// The already-composited backdrop beneath the node, blurred. `row` is a
    /// [`dashpaint::PaintTable::all_blurs`] row.
    ///
    /// One variant rather than two, because only `BlurKind::Backdrop` is
    /// packed: node-local layer blur is budgeted at v1 and nothing in this tree
    /// produces one. A layer blur arrives as its own variant, which is a
    /// compile error at every `match` until each is taught what to do with it.
    Backdrop = 2,
    /// A solid fill. `row` is a [`dashpaint::PaintTable::all_solids`] row.
    FillSolid = 3,
    /// A gradient fill. `row` is a [`dashpaint::PaintTable::all_gradients`] row.
    FillGradient = 4,
    /// An image fill. `row` is a [`dashpaint::PaintTable::all_images`] row.
    FillImage = 5,
    /// The node's outline stroke. `row` is a
    /// [`dashpaint::PaintTable::all_strokes`] row.
    Stroke = 6,
    /// One glyph of a positioned run. `row` is a [`dashpaint::GlyphRunTable`]
    /// run index, and [`Instance::corners`] is the glyph's rectangle in that
    /// run's atlas, in that atlas's own texels.
    ///
    /// One instance per glyph rather than per run: a run's glyphs are not one
    /// quad, and the alternative — a run instance that the shader expanded —
    /// would need the quad array in a shader stage and would put a loop in a
    /// fragment. Per glyph, each is an ordinary quad of the one stream, and
    /// everything already stated over that stream (draw order, clipping, the
    /// group layer, the dirty-range upload) applies to text without a second
    /// mechanism.
    Text = 7,
}

impl InstanceKind {
    /// The value this kind occupies in [`Instance::kind`].
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// True when this kind draws one of the node's shadows.
    pub const fn is_shadow(self) -> bool {
        matches!(self, InstanceKind::ShadowDrop | InstanceKind::ShadowInner)
    }

    /// True when this kind draws one of the node's fill layers.
    pub const fn is_fill(self) -> bool {
        matches!(
            self,
            InstanceKind::FillSolid | InstanceKind::FillGradient | InstanceKind::FillImage
        )
    }

    /// The name a layer-1 golden prints — stable, and the only place the
    /// spelling is decided.
    pub const fn name(self) -> &'static str {
        match self {
            InstanceKind::ShadowDrop => "shadow-drop",
            InstanceKind::ShadowInner => "shadow-inner",
            InstanceKind::Backdrop => "backdrop",
            InstanceKind::FillSolid => "fill-solid",
            InstanceKind::FillGradient => "fill-gradient",
            InstanceKind::FillImage => "fill-image",
            InstanceKind::Stroke => "stroke",
            InstanceKind::Text => "text",
        }
    }

    /// The kind an [`Instance::kind`] names.
    ///
    /// # Panics
    ///
    /// Panics on a value no variant carries, for the reason
    /// [`InstanceBuffer::dump`] gives.
    pub const fn from_u32(value: u32) -> Self {
        match value {
            0 => InstanceKind::ShadowDrop,
            1 => InstanceKind::ShadowInner,
            2 => InstanceKind::Backdrop,
            3 => InstanceKind::FillSolid,
            4 => InstanceKind::FillGradient,
            5 => InstanceKind::FillImage,
            6 => InstanceKind::Stroke,
            7 => InstanceKind::Text,
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
    /// outset.
    ///
    /// For an [`InstanceKind::Text`] instance it is the glyph's own quad in
    /// document space, already placed: `dashpaint::GlyphQuad`'s pen position
    /// plus the atlas glyph's `plane_em` scaled by the run's size. The painter
    /// places nothing else and moves nothing (P2).
    ///
    /// First, with `corners` after it, so both four-float vectors sit at a
    /// 16-byte offset. A consumer binding this as a storage-buffer element then
    /// repacks nothing.
    pub bounds: [f32; 4],
    /// The rounded-box radii: `[top_left, top_right, bottom_right,
    /// bottom_left]`, grown alongside [`bounds`](Self::bounds) where that is,
    /// with a sharp corner staying sharp.
    ///
    /// Meaningless when [`shape`](Self::shape) names a coverage mask: a
    /// baked-vector node carries its outline in the baked geometry. Carried
    /// through rather than zeroed so the authored value stays visible in a
    /// golden.
    ///
    /// **For an [`InstanceKind::Text`] instance this is the glyph's rectangle
    /// in its run's atlas** — `[x, y, w, h]` in that atlas's own texels, with a
    /// top-left origin, which is `dashpaint::VectorField::atlas_rect`'s
    /// convention rather than `dashpaint::AtlasGlyph::atlas_px`'s bottom-left
    /// `[l, b, r, t]`. The packer converts; the conversion is stated once,
    /// where the reference painter also states it.
    ///
    /// Texels of the *source* atlas, not of the residency atlas the payload was
    /// uploaded into. The packer runs with no device — that is what makes layer
    /// 1 testable on a runner with no GPU — so it cannot know where residency
    /// put the payload, and the row this instance names carries the mapping
    /// from one to the other.
    ///
    /// Four floats either way, which is what lets text join this stream without
    /// widening the struct. A glyph needs no rounded box, and an image fill
    /// could not have taken this route because it still needs one.
    pub corners: [f32; 4],
    /// What this quad draws — an [`InstanceKind`], sub-kind included.
    pub kind: u32,
    /// The row this instance's parameters sit at, in the table `kind` names.
    ///
    /// One field, one meaning per kind, and the kind is not separable from its
    /// sub-kind — which is the whole reason `kind` and `tag` were merged.
    pub row: u32,
    /// The baked-vector coverage mask that masks this instance, as a
    /// [`dashpaint::PaintTable::all_shapes`] row **plus one**; `0` for the
    /// implicit parametric shape.
    ///
    /// Carried on the backdrop instance as well as the fill, because a masked
    /// node's backdrop blur is confined to the field's coverage rather than to
    /// its box — the reference painter does the same, and the hero's frosted
    /// panel is exactly that node.
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
    /// [`dashpaint::RectEntry::opacity`], carried through unchanged.
    pub opacity: f32,
    /// Declared padding, and always zero.
    ///
    /// A struct carrying a four-float vector has an alignment of 16, so its
    /// array stride rounds up to 64 whatever its members add to. Without this
    /// word the Rust type would be 60 bytes and the shader's view of the same
    /// array would be 64 — every element after the first read from the wrong
    /// offset. `bytemuck::Pod` refuses a type with *implicit* padding, so
    /// declaring it is also what keeps that derive.
    ///
    /// It is public because `Pod` requires it, which makes it a field two
    /// otherwise-equal instances could differ in — the equality hazard
    /// `docs/decisions/sub-word-members-widen-rather-than-pad.md` rejected a
    /// `_pad` member for. Here there was no way to remove the hole, only to
    /// name it, so the packer writes zero and a test asserts every packed
    /// instance carries zero.
    pub _pad: u32,
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

/// One render-target group layer, as the painter composites it.
///
/// [`Instance::layer`] routes a quad to a layer; this says what to *do* with
/// the layer once its quads are drawn. Both halves live in the instance buffer
/// rather than the alpha travelling beside it as a second argument, for the
/// reason `docs/decisions/instance-buffer-contract.md` gives for every other
/// parameter: the packer already reads `dashpaint::GroupComposite` to assign
/// `layer`, so recording the rest of that row here keeps one producer and lets
/// layer 1 pin the whole group structure with no device.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Layer {
    /// The alpha this layer's composite blends at — `GroupComposite::alpha`,
    /// carried through unchanged.
    pub alpha: f32,
    /// The layer this one composites *into*: another layer's index **plus
    /// one**, or [`Instance::NONE`] for the frame's own target.
    ///
    /// The same bias [`Instance::layer`] uses, and for the same reason — one
    /// convention for "names a layer, or does not". Groups nest, so this is the
    /// enclosing group, and it is recorded rather than re-derived from the
    /// group ranges: the packer already holds the open stack that answers it,
    /// and a second derivation of the same fact could disagree with the first.
    pub parent: u32,
}

/// The packed instance buffer: every quad of one frame, in draw order, the
/// per-rect index into it, and the layers those quads composite through.
///
/// This is what [`crate::pack`] produces and what a layer-1 golden pins.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InstanceBuffer {
    instances: Vec<Instance>,
    spans: Vec<InstanceSpan>,
    layers: Vec<Layer>,
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
    /// glyph instances went at the end of that list, so they widened a count
    /// and moved no boundary — which is why this contract did not change when
    /// text arrived.
    ///
    /// A count of zero means the rect draws nothing: a layout-only container.
    /// A text node is no longer one of those — its glyphs are instances of the
    /// rect their run is anchored to.
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

    /// The frame's render-target group layers, index-aligned with the
    /// `dashpaint::GroupComposite` slice it was packed from — so an
    /// [`Instance::layer`] of `g + 1` is `layers()[g]`.
    ///
    /// Empty for a frame whose groups all took the free path, which is every
    /// scene where no group's contents overlap: `masks-and-group-opacity.md`
    /// resolves that case into per-rect `opacity` at commit and emits no group
    /// at all.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Empties the buffer, keeping its allocation — what a painter does at the
    /// top of a frame, so a steady-state frame reuses this buffer rather than
    /// growing a new one.
    pub fn clear(&mut self) {
        self.instances.clear();
        self.spans.clear();
        self.layers.clear();
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

    /// Appends one layer, and returns the value an [`Instance::layer`] carries
    /// to name it — the index **plus one**.
    ///
    /// # Panics
    ///
    /// Panics unless `index` is the next group in order. The layers are
    /// index-aligned with the group slice, so a packer that skipped or repeated
    /// one would route every instance after it into the wrong layer.
    pub(crate) fn push_layer(&mut self, index: usize, layer: Layer) -> u32 {
        assert_eq!(
            index,
            self.layers.len(),
            "layers are index-aligned with the group slice; group {index} arrived out of order"
        );
        self.layers.push(layer);
        u32::try_from(index).expect("group list exceeds u32::MAX") + 1
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
            "instances {} rects {} layers {}",
            self.instances.len(),
            self.spans.len(),
            self.layers.len(),
        );
        for (index, span) in self.spans.iter().enumerate() {
            let _ = writeln!(out, "rect {index} at {} count {}", span.offset, span.count);
        }
        // After the spans and before the instances, so a reviewer reads the
        // layer a following instance line names before reading the instances.
        for (index, layer) in self.layers.iter().enumerate() {
            let _ = writeln!(
                out,
                "layer {} alpha {:?} into {}",
                index + 1,
                layer.alpha,
                layer.parent,
            );
        }
        for (index, instance) in self.instances.iter().enumerate() {
            let kind = InstanceKind::from_u32(instance.kind);
            let _ = writeln!(
                out,
                "{index:>4} {:<13} row {} shape {} clip {}..{} layer {} opacity {:?} \
                 bounds {:?} corners {:?}",
                kind.name(),
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
