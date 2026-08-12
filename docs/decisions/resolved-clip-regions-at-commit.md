# Subtree clips resolve at commit into a per-rect clip-region table

    status   accepted (story #97, 2026-07-13)
    scope    dashpaint (boundary B), dashscene-core (commit), dashscene-skia
    resolves docs/decisions/subtree-clip-resolution-deferred.md

## Context

`Paint.clip` (`docs/design/architecture.md`) means "this node clips its children
to its own (rounded) box". Boundary B is a flat rect table: a painter has no
parent/child structure, and P2 forbids it re-deriving one, so no painter could
act on the `PaintEntry::clip` bool. Story #14 shipped it as a named
`unimplemented!` and deferred the resolution here
(`subtree-clip-resolution-deferred.md`, issue #97), deliberately leaving the
contract's shape open.

## Options

For the resolved shape:

1. A clip-region table: each rect references a region — the list of its clipping
   ancestors' (rounded) boxes, intersected.
2. Intersect the committed rects' geometry in core (option 2 of the deferred
   record).
3. Keep `PaintEntry::clip: bool` alongside a resolved region.
4. Put the resolved clip index on `PaintEntry` rather than `RectEntry`.
5. Pass the per-rect clip indices as a slice parallel to `rects` instead of a
   field in the entry.

## Choice

Option 1. Boundary B gains `ClipBox` (an axis-aligned box + per-corner radii),
`ClipRegion` (the boxes to intersect, outermost ancestor first; empty =
unclipped), `ClipTable` (dense, deduplicated, index 0 reserved for the unclipped
region), and `ClipIndex`. `RectEntry` gains `clip: ClipIndex` (24 bytes, still
`#[repr(C)]` blittable); `Painter::paint` gains a `clips: &ClipTable` parameter.
`PaintEntry::clip: bool` is **removed**.

`dashscene-core` resolves the regions on its existing DFS commit walk and gains
the intent to resolve: `Prop::Clip(bool)` and `Prop::Corners { .. }`.

## Why

- Option 2 stays rejected on story #14's grounds: geometry intersection is
  correct only for axis-aligned clips on solid fills, it distorts gradient
  frames (a gradient's frame is built from handles normalized to the node's own
  box, not a clipped sub-rect), and it cannot express a rounded clip at all —
  rounding a rect's bounds is not the same operation as intersecting with a
  rounded rect.
- The region is a **list** of boxes, not one pre-intersected box, because the
  intersection of two rounded rects is not a rounded rect. Pre-folding the sharp
  boxes would be exact but buys only a scissor-rect fast path no painter needs
  yet — and a painter can fold its own sharp prefix without knowing the tree, so
  nothing is lost by leaving it out.
- Against option 3: two representations of clipping in boundary B, only one of
  which a painter may act on, is the trap that produced the panic in the first
  place. Clipping intent belongs to the document (`dashbuf`'s `Paint.clip`) and
  the arena (`Prop::Clip`); boundary B carries the resolved result. `PaintEntry`
  now mirrors `dashbuf`'s `Paint` minus that bool, and that difference is the
  point: one is intent, the other is resolved.
- Against option 4: paint entries deduplicate by paint content, while a resolved
  region is a property of a node's _position in the tree_. Two
  identically-filled nodes under different clipping ancestors would have to
  occupy distinct paint entries, which destroys the pool's meaning.
- Against option 5: the clip is a per-rect fact, and splitting one per-rect
  record across two slices the painter must keep in step costs R-T4 a second
  buffer for no gain. Four bytes in the entry is the cheaper contract.
- `ClipIndex::UNCLIPPED` (index 0, reserved by `ClipTable::new`) keeps "every
  rect resolves" (`boundary-b-unification.md`) without a sentinel and without
  every hand-built fixture re-interning the empty region.
- Core gains `Prop::Corners` alongside `Prop::Clip` because a clip box takes the
  clipping node's corner radii: without it, no producer could author the rounded
  clip that this contract exists to express, and the option-2 failing it was
  rejected for would be untestable end to end.

## Consequences

- The `SkiaPainter` panic naming issue #97 is gone; a clipping node's
  descendants are clipped by `save` + `clip_rrect(Intersect)` per box.
- **Dirty set.** A rect is dirty when its entry bits changed, its resolved paint
  changed, **or its resolved clip region changed**. The third clause is
  load-bearing and mirrors the second: the clip table is re-interned every
  commit, so resizing a clipping frame leaves its subtree's clip _index_
  untouched while the region that index resolves to moves — the descendants' own
  entry bits are bit-identical and would otherwise be reported clean and never
  repainted. Regions compare by `f32::to_bits`, like every other clause.
- **Region interning** keys on
  `(parent's region index, parent's
  clip-box bits)`. Equal ancestor chains
  take equal keys by induction, so regions deduplicate by value at O(1) per node
  without hashing a chain-shaped key: one clipping ancestor's whole subtree
  shares one region entry.
- A clipping node does not clip _itself_ — only its descendants. Its own rounded
  corners still shape its own fill and stroke, as before.
- The paint interning key widens from the fill color to (fill color, corner
  radii), now that corners are authorable — two nodes may share a paint entry
  only if they resolve to the same `PaintEntry`.
- `dashlang` does not expose clip or corners; scenes needing them are authored
  directly against `dashscene-core`, as the v0.2 flex goldens already are (the
  DSL's vocabulary gap, #118).
- Rotated clips stay out of scope (`docs/roadmap.md` lists clip-on-rotated at
  v0.8): a `ClipBox` is axis-aligned, and gains a transform when that slice
  needs one.

## Trace

- Satisfies: `docs/design/architecture.md` (`Paint.clip`); issue #97 acceptance
  criteria; unblocks the clip half of issue #18's scope.
- Supersedes the "corner radii and clip live in the paint entry" sub-decision of
  `docs/decisions/paint-entry-composition.md` for clip (corners stay).
- Files: `docs/design/dashpaint.md`, `docs/design/dashscene-core-arena.md`,
  `docs/design/dashscene-skia.md`, `docs/design/goldens.md` (`v03-clips.png`).
- Related: `docs/decisions/boundary-b-unification.md` (every rect resolves),
  `docs/decisions/image-assets-cross-boundary-b.md` (the precedent for growing
  `Painter::paint`'s input), `docs/decisions/golden-comparison-space.md` (the
  golden's tolerance).
