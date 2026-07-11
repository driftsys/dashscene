# dashpaint v0.1 — painter trait + paint table (boundary B) — design

    story    #3 (epic #1, slice v0.1)
    branch   story/dashpaint
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

`dashpaint` defines boundary B (`DESIGN_1.md` §4, §7.3, §8): the complete
input a painter consumes, and the trait every painter implements. For the
v0.1 walking skeleton that input is a rect table plus a paint table with
one paint kind (solid fill). Principle P2 applies: a painter only colors —
it never measures, wraps, kerns, or moves anything.

## Contract (pinned across sessions A and B)

These shapes are the agreed boundary-B contract for v0.1. Session A's
`dashscene-core` produces them; this crate defines them. Do not change
them without updating both stories.

- `RectEntry { x: f32, y: f32, w: f32, h: f32, paint: u32 }` — one entry
  per document node; the rect-table index IS the document DFS node index
  (`DESIGN_1.md` §5), so no per-entry id field exists. The issue text's
  "(id, x, y, w, h)" sketch is superseded by this pinned shape.
- `paint` is an index into the paint table.
- Solid fill color is 4×f32 RGBA, the same shape as `dashbuf`'s `Color`
  struct (`crates/dashbuf/schema/dashbuf.fbs`).
- The generation stamp from `DESIGN_1.md` §7.3 belongs to the double
  buffer that `dashscene-core` owns, not to individual rect entries. It
  is out of scope for this crate.

## Public API

All types live in `dashpaint` with no dependencies (in particular, no
`dashscene-core` and no `dashbuf`).

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Color { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct RectEntry { pub x: f32, pub y: f32, pub w: f32, pub h: f32, pub paint: u32 }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum PaintKind { Solid { color: Color } }

    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct PaintTable { /* Vec<PaintKind>, private */ }

    impl PaintTable {
        pub fn new() -> Self;
        /// Appends an entry; returns its index (the value `RectEntry.paint` holds).
        pub fn push(&mut self, kind: PaintKind) -> u32;
        pub fn get(&self, index: u32) -> Option<&PaintKind>;
        pub fn len(&self) -> usize;
        pub fn is_empty(&self) -> bool;
    }

    pub trait Painter {
        /// Paint every rect, in slice order, resolving each `RectEntry.paint`
        /// index against `paints`.
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
    }

Notes:

- `Color` and `RectEntry` are `#[repr(C)]` because `DESIGN_1.md` §7.3
  calls rect entries "blittable" and R-T4 plans dirty-range instance-buffer
  uploads from the rect table. Fixing the layout now costs nothing.
- `paint()` is infallible. Vocabulary and index validity are validated
  upstream (P4) — by the time a rect table reaches a painter there is no
  legitimate failure. An out-of-range `paint` index is a broken contract
  between crates, not a runtime condition; implementations may panic on it.
- The trait is object-safe. Backend selection is whole-scene (R3), so
  `Box<dyn Painter>` must work.
- Slice order is paint order (back-to-front): DFS order already encodes
  document stacking for v0.1's fixed-rect scenes.

## Testing (acceptance criteria from issue #3)

Unit tests inside the crate, against hand-built fixtures — no
`dashscene-core`:

1. `PaintTable` push returns sequential indices starting at 0; `get`
   resolves them; `get` past the end returns `None`.
2. A `RecordingPainter` test double implements `Painter`, resolves each
   rect's paint index, and records `(RectEntry, Color)` pairs; a
   hand-built two-rect, two-paint fixture asserts the recorded output —
   order, geometry, and resolved colors.
3. The same painter driven through `Box<dyn Painter>` proves object
   safety.

`just build` green is the gate (test + clippy -D warnings + fmt + dprint +
markdownlint).

## Alternatives considered

- **Reuse `dashbuf::Color` instead of defining one** — rejected. Painters
  sit downstream of the runtime, not of the document format; a `dashbuf`
  dependency couples boundary B to the file format (against the layering
  in `DESIGN_1.md` §4 and P5's spirit) and leaks the flatbuffers
  dependency into every painter. The contract is "same shape", not "same
  Rust type"; the runtime converts.
- **The reverse ownership: types live in `dashscene-core`, `dashpaint`
  depends on it** — deferred, not chosen here. The pinned contract for
  story #4 explicitly revisits single ownership of these types once both
  crates exist; today `dashpaint` standing alone keeps story A and story B
  independent.
- **A `Scene`/`FrameInput` wrapper struct as the `paint()` parameter** —
  rejected for v0.1 (YAGNI). Glyph runs (v0.5) and the dirty set will
  change the signature, but that is a cheap in-workspace refactor; a
  wrapper today is speculative structure.
- **`paint()` returning `Result`** — rejected. No fallible operation
  exists in the v0.1 contract, and P4 places validation upstream; adding
  an error channel would be error handling for an impossible scenario.

## Trace

- Satisfies: `DESIGN_1.md` §8 painter trait (boundary B), §7.3 output
  shape; issue #3 acceptance criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness).
