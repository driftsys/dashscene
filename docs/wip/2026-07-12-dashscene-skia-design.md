# dashscene-skia v0.1 — first Painter impl + boundary-B unification — design

    story    #4 (epic #1, slice v0.1)
    branch   story/dashscene-skia
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

Two deliverables that belong together, because the second is what the
first is wired through:

1. The first `Painter` implementation: `dashscene-skia`, skia-safe CPU
   raster only (bit-exact, deterministic — `DESIGN_1.md` §8.1), painting
   a scene committed by `dashscene-core` and encoding it to PNG.
2. The boundary-B reconciliation both story #2 and story #3 explicitly
   deferred to this story: single ownership of the shared types, the
   paint-less-node crossing (debt #55), and the `PaintIndex` newtype
   evaluation (debt #54).

## Decision 1 — the boundary-B types live in dashpaint

`dashscene-core` deletes its mirror types (`committed::{Color,
RectEntry, Paint}` and `NO_PAINT`) and depends on `dashpaint`.
`CommittedScene.paints` becomes a `dashpaint::PaintTable`, so
`Arena::committed()` yields exactly the two values `Painter::paint`
takes: `scene.rects()` (`&[RectEntry]`) and `scene.paints()`
(`&PaintTable`). Core re-exports the types it consumes (`Color`,
`PaintEntry`, `PaintIndex`, `PaintKind`, `PaintTable`, `RectEntry`) so
producers keep a one-crate import surface.

Publish order (`SCOPE_DECISIONS.md` §7, the workspace `Cargo.toml`
comment, and the `justfile` publish recipe) moves `dashpaint` before
`dashscene-core`: dashbuf → dashpaint → dashscene-core → ….

Why this direction and not the reverse (core owns, dashpaint depends on
core): a painter crate that depends on the producer/runtime crate would
drag the arena, staging, and (from v0.2) Taffy into every painter
build — against §8's "painters only color" and R3's lean-painter goal.
The reverse dependency is exactly the pipeline direction of §4:
producers → runtime → painters, with the boundary types defined at the
boundary they describe.

## Decision 2 — every rect resolves; the NO_PAINT sentinel dies at boundary B (closes #55)

Core's commit interns `PaintEntry::default()` (fill `None`) for
unfilled nodes instead of emitting `RectEntry.paint = u32::MAX`. Every
committed rect entry resolves through `PaintTable::resolve`; an entry
with no fill draws nothing by definition — not by a per-painter skip
rule.

Why: `resolve`'s panic stays the single failure mechanism
(`docs/decisions/painter-trait-infallible-slice-input.md`); painters
have no sentinel special case, so all backends treat unfilled nodes
identically by construction (§8 bisect-by-construction); and from v0.3
vocabulary onward a fill-less entry is a real entry anyway (it can
carry stroke or clip). Cost: at most one extra pooled entry per commit
(the interner deduplicates all unfilled nodes to it). `NO_PAINT`
disappears from core's public API; the `u32::MAX` guard in `add_node`
stays (it still bounds `NodeId` against `dashbuf`'s `NO_PARENT`
sentinel, and it keeps every paint index representable).

`dashbuf`'s `Node.paint_entry` keeps its `u32::MAX` NO_PAINT sentinel:
that is the document format's "node references no pool entry", a
different level than the committed runtime output (a document node
without paint still gets a resolved rect whose entry is the shared
empty one).

## Decision 3 — adopt the PaintIndex newtype (closes #54)

`dashpaint` gains `#[repr(transparent)] pub struct PaintIndex(pub u32)`.
`RectEntry.paint: PaintIndex`; `PaintTable::push` returns `PaintIndex`;
`get`/`resolve` take it. Layout is unchanged (`repr(transparent)`
inside the `repr(C)` entry — still blittable, still 20 bytes), so the
pinned v0.1 data shape holds; what changes is the Rust type, and this
story is the sanctioned renegotiation point both prior decision records
name. Why now: core's `commit` builds node ids, DFS indices, and paint
indices in one function — the exact cross-index confusion debt #54
records — and unification already touches every line that would need
the change later.

## The painter

`dashscene-skia` depends on `dashpaint` and `skia-safe` only
(`dashscene-core` is a dev-dependency for the scene-building test — a
painter never sees the arena, P2). skia-safe 0.81 builds from prebuilt
binaries (verified in this environment, 15 s).

    pub struct SkiaPainter { surface: skia_safe::Surface }

    impl SkiaPainter {
        /// CPU raster surface (N32 premul). Panics if width/height
        /// are not positive.
        pub fn new(width: i32, height: i32) -> Self;
        /// PNG-encode the current surface contents.
        pub fn png_bytes(&mut self) -> Vec<u8>;
        /// RGBA8888 readback of the current surface contents (tests
        /// and future golden tooling).
        pub fn rgba_bytes(&mut self) -> Vec<u8>;
    }

    impl Painter for SkiaPainter {
        fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable);
    }

`paint` semantics (v0.1 vocabulary):

- Clears to transparent, then draws every rect in slice order (slice
  order defines stacking; painting back-to-front is this
  implementation's choice).
- Per rect: `paints.resolve(entry.paint)`; a `Solid` fill draws an
  axis-aligned rect with anti-aliasing off (bit-exact goldens want no
  coverage math on axis-aligned edges); a `None` fill draws nothing.
- v0.3 vocabulary that this painter cannot draw yet — gradient/image
  fills, strokes, non-zero corners, clip — panics via `unimplemented!`
  naming story #14. Not a silent drop (P4): v0.1 producers cannot emit
  these (core's `Prop` has only solid fill), so hitting the panic means
  a producer outran the painter.

## Testing

`crates/dashscene-skia/tests/painter.rs`, wiring `dashscene-core` →
`dashscene-skia` end to end (this test is the story's acceptance
criterion and the first cross-crate exercise of boundary B):

1. `paints_a_core_committed_scene_with_exact_pixels` — Arena: red root
   rect (0,0 4×4) with a blue child (authored offset 1,1, size 2×2);
   commit; paint into a 4×4 surface; RGBA readback asserts the exact
   bytes: blue where the child covers, red elsewhere in the root,
   transparent outside — non-trivial pixel output, asserted exactly
   (CPU raster is deterministic).
2. `an_unfilled_node_draws_nothing` — a layout-only parent (no fill)
   with a filled child: parent area outside the child stays
   transparent; the child paints. Pins decision 2's crossing.
3. `encodes_png` — `png_bytes()` starts with the 8-byte PNG signature
   and is non-empty beyond it.
4. `unimplemented_vocabulary_panics_by_name` — `#[should_panic]`: a
   hand-built `PaintEntry` with a gradient fill panics naming
   story #14. Pins the honest-failure contract until #14 lands.

Unit coverage that moves with the types: `dashpaint`'s and core's
existing tests migrate mechanically (`PaintIndex` in table tests, core
tests assert `PaintEntry::solid` pool contents instead of `Paint`, the
unfilled-node test asserts resolution to the shared empty entry instead
of the sentinel).

## Alternatives considered

- **Core keeps ownership; dashpaint depends on core** — rejected, see
  decision 1 (dependency direction against §4/§8; every painter would
  build the runtime).
- **Both keep their own types + a conversion layer at the seam** —
  rejected: a permanent per-frame translation for identical shapes,
  and two definitions of one contract drift (story #2's and #3's
  records each flagged this state as temporary).
- **Keep the NO_PAINT sentinel and make painters skip it** — rejected:
  every painter re-implements the skip (the divergence-per-backend
  failure `painter-trait-infallible-slice-input.md` records), and the
  sentinel value is unrepresentable in `PaintIndex` terms once indices
  are typed.
- **`PaintKind::None` instead of the empty entry** — rejected already
  in `paint-entry-composition.md`; unchanged.
- **Defer PaintIndex again** — rejected, see decision 3 (this story
  touches every affected line anyway; deferring re-creates the churn
  later for no gain).
- **Anti-aliased fills in the reference painter** — deferred, not
  chosen: AA belongs to the vocabulary discussion when non-axis-aligned
  or rounded geometry lands (#14 corners); for v0.1's axis-aligned
  rects, AA off keeps goldens bit-exact and machine-independent.
- **A `Pixmap`/`tiny-skia` painter instead of skia-safe** — out of
  scope: `DESIGN_1.md` §8.1 names skia-safe as the reference painter;
  tiny-skia is the parked wasm path (§8.4).

## Trace

- Satisfies: issue #4 acceptance criteria; `DESIGN_1.md` §8.1 (CPU
  raster reference painter), §4 boundary B.
- Resolves: debt #54 (PaintIndex), debt #55 (paint-less crossing).
- Updates: `SCOPE_DECISIONS.md` §7 publish order;
  `docs/decisions/core-committed-output-shape.md`,
  `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/paint-entry-composition.md` (all deferred parts of
  the boundary to this story).
- Blocks: #6 (golden harness), #14 (v0.3 painting).
