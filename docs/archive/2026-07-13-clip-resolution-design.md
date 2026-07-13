# Spec — subtree clip resolution at commit (story #97)

Working memory for story #97. Gardened into `docs/decisions/` and
`docs/design/` before the PR opens.

## Problem

`Paint.clip` (DESIGN_1.md §8.1) means "this node clips its children to
its own (rounded) box". Boundary B is a flat rect table: a painter has
no parent/child structure, and P2 forbids it re-deriving one. So the
reference painter cannot paint a clipping node, and today panics by
name on `entry.clip` (`docs/decisions/subtree-clip-resolution-deferred.md`,
issue #97).

`dashscene-core` therefore has to resolve the ancestor-clip relation at
commit, into per-rect data a painter consumes without knowing the tree.

## Requirements

- R1 — Every committed rect carries the clip that applies to it,
  resolved from its clipping ancestors, with no tree walk left for the
  painter.
- R2 — The resolved form expresses rounded clips exactly (the reason
  story #14 rejected geometry intersection), and expresses a chain of
  nested clipping ancestors exactly.
- R3 — The committed rect table stays blittable plain data (`#[repr(C)]`,
  R-T4 instance-buffer uploads): the per-rect clip reference is an index.
- R4 — The dirty set marks a rect dirty when the clip that applies to it
  changed, even when the rect's own geometry and paint did not (resizing
  a clipping ancestor is the case).
- R5 — Every rect resolves — an unclipped rect references a real region,
  not a sentinel (`docs/decisions/boundary-b-unification.md`).
- R6 — A producer can author clip intent: core stages "this node clips
  its children", and a rounded clipping frame is authorable end to end
  (otherwise R2's rounded case has no producer).
- R7 — The reference painter paints the resolved clip; the named panic
  goes away.

## Contract (boundary B, `dashpaint`)

    #[repr(C)]
    pub struct ClipBox { x, y, w, h: f32, corners: CornerRadii }

    #[repr(transparent)]
    pub struct ClipIndex(pub u32);
    impl ClipIndex { pub const UNCLIPPED: ClipIndex = ClipIndex(0); }

    pub struct ClipRegion { /* private: Vec<ClipBox> */ }
    impl ClipRegion {
        pub fn unclipped() -> Self;
        pub fn new(boxes: Vec<ClipBox>) -> Self;
        pub fn boxes(&self) -> &[ClipBox];   // outermost ancestor first
        pub fn is_unclipped(&self) -> bool;
    }

    pub struct ClipTable { /* private: Vec<ClipRegion> */ }
    impl ClipTable { new, push -> ClipIndex, get, resolve, len }

    pub struct RectEntry { x, y, w, h: f32, paint: PaintIndex, clip: ClipIndex }

    pub trait Painter {
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
                 images: &ImageTable, clips: &ClipTable);
    }

- A `ClipRegion` is the **intersection** of its boxes. An empty box list
  is the unclipped region.
- `ClipTable` index 0 is always the unclipped region (`ClipTable::new()`
  seeds it), so an unclipped rect references `ClipIndex::UNCLIPPED` and
  still resolves (R5).
- `PaintEntry::clip: bool` is **removed**. Boundary B carries the
  resolved region; the "clips its children" bool stays document intent
  (`dashbuf`'s `Paint.clip`) and arena intent (`Prop::Clip`).
- `RectEntry` grows to 24 bytes, still `#[repr(C)]` blittable (R3).

## Commit-time resolution (`dashscene-core`)

- Intent: `Prop::Clip(bool)` and `Prop::Corners { .. }` on the node.
  Corners feed both the node's `PaintEntry` and its clip box (R6).
- Resolution rides the existing DFS walk (parent before child):
  `region(node) = region(parent)` when the parent does not clip, and
  `region(parent) + [parent's box]` when it does. A node is not clipped
  by its own clip — only its descendants are.
- Interning: the key is `(parent's region index, parent's clip-box
  bits)`, or `None` for the unclipped region. Because equal chains
  inductively get equal indices, this dedups by value at O(1) per node
  with no chain-shaped hash key; sibling subtrees under one clipping
  ancestor share one region entry.
- Dirty set: a rect is dirty when its entry bits changed **or** its
  resolved paint changed **or** its resolved clip region changed — the
  clip table is re-interned every commit, exactly like the paint table,
  so a stable index can reference a different region (R4). Region
  comparison is by `f32::to_bits`, like every other diff clause.
- The paint interning key widens from the fill color to (fill color,
  corner radii) now that corners are authorable.

## Painter (`dashscene-skia`)

Per rect: `clips.resolve(rect.clip)`; if the region is not unclipped,
`save()`, `clip_rrect(Intersect, anti-alias)` per box, draw, `restore()`.
The `unimplemented!` naming issue #97 goes away.

## Alternatives considered

1. **Intersect the committed rects' geometry in core** (option 2 of
   `subtree-clip-resolution-deferred.md`). Rejected there and not
   resurrected: correct only for axis-aligned clips on solid fills, it
   distorts gradient frames (a gradient's frame is built from handles
   normalized to the node's own box, not a clipped sub-rect), and it
   cannot express rounded clips.
2. **Keep `PaintEntry::clip: bool` alongside the resolved region.**
   Rejected: two representations of clipping in boundary B, only one of
   which a painter may act on — the trap that produced the panic. The
   intent bool belongs to the document and the arena, not to the
   resolved painter input.
3. **Put the clip index on `PaintEntry` instead of `RectEntry`.**
   Rejected: paint entries are deduplicated by paint content, and a
   resolved region is a property of a node's _position in the tree_.
   Two identically-filled nodes under different clipping ancestors
   would force distinct paint entries, destroying the pool's meaning.
4. **Pre-intersect the ancestor boxes into one region box.** Exact only
   when every box is sharp; a rounded∩rounded is not a rounded rect. A
   correct implementation needs the box list anyway, so the sharp-prefix
   fold buys one clip op for a painter that wants a scissor rect — a
   painter-local optimization needing no tree. Left out (P2 is
   satisfied either way, and nothing needs it yet).
5. **A parallel `clips: &[ClipIndex]` slice alongside `rects`.** Keeps
   `RectEntry` at 20 bytes, but splits one per-rect record across two
   slices the painter must keep in step, and costs the R-T4 upload a
   second buffer. The index belongs in the entry.
6. **Bundle boundary B into one `SceneInput` struct** instead of a
   fourth `paint` parameter. A larger change to a pinned contract than
   this story needs; the parameter follows the `ImageTable` precedent
   (story #14). The growing arity is worth a debt item.
