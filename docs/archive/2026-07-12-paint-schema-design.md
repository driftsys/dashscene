# v0.3 paint vocabulary — dashbuf schema + dashpaint types — design

    story    #13 (epic #12, slice v0.3 — early start, depends on epic #1 crates
             dashbuf + dashpaint only, both on main)
    branch   story/paint-schema
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

Extend the paint vocabulary from v0.1's single solid fill to the v0.3 set
(`DESIGN_1.md` §10.1 NOW, §11 v0.3): the four gradient types, rounded-rect
corner radii, stroke with align, image fills with scale modes, and
axis-aligned + rounded clip. Two deliverables, no painter work (painting
these lands at #14):

- `dashbuf.fbs` schema growth, strictly additive (append-only field ids,
  R7): session A's story #2 reads `Node.paint` from main today and must
  not break.
- `dashpaint` paint-table types mirroring the same vocabulary.

## dashbuf schema additions

New enums (all `uint8`): `GradientKind { Linear, Radial, Angular,
Diamond }`, `StrokeAlign { Inside, Center, Outside }`, `ScaleMode
{ Fill, Fit, Crop, Tile }`, `ImageFormat { Png }`.

New structs: `Vec2 { x, y: float32 }`, `GradientStop { offset: float32,
color: Color }`, `CornerRadii { top_left, top_right, bottom_right,
bottom_left: float32 }`.

New tables and the fill union:

    table Gradient {
      kind: GradientKind;
      // Figma-style normalized handle positions in the node's box:
      // gradient origin, primary-axis end, secondary-axis end. One
      // geometry model serves all four kinds (P1: intent, not results).
      handle_origin: Vec2;
      handle_primary: Vec2;
      handle_secondary: Vec2;
      stops: [GradientStop];
    }

    table ImageFill {
      image: uint32;          // index into Document.images
      scale_mode: ScaleMode;
    }

    union Fill { SolidFill, Gradient, ImageFill }

    table Stroke {
      width: float32;
      align: StrokeAlign;
      color: Color;           // v0.3 strokes are solid-only (see decision)
    }

    table Image {
      format: ImageFormat;    // embedded encoded bytes; Png only for now
      bytes: [ubyte];
    }

`Node` gains four appended optional fields: `fill: Fill`, `stroke:
Stroke`, `corners: CornerRadii`, `clip: bool = false`. `Document` gains
`images: [Image]`.

Precedence rule (schema comment now, validator rule when
`dashscene-validator` gets its real profile work): when `fill` is
present it supersedes `paint`; `paint` remains the v0.1 walking-skeleton
solid shorthand and is removed only in a coordinated cleanup once the
v0.1 stories no longer write it.

## dashpaint additions

New value types, mirroring the schema shape for shape (still no
dependency between the crates — conversion stays the producer's job, per
`docs/decisions/dashpaint-owns-boundary-b-types.md`):

    #[repr(C)] Vec2 { x, y: f32 }                       // Copy
    #[repr(C)] GradientStop { offset: f32, color: Color } // Copy
    GradientKind { Linear, Radial, Angular, Diamond }    // Copy
    Gradient { kind: GradientKind, handles: [Vec2; 3],
               stops: Vec<GradientStop> }                // Clone
    ScaleMode { Fill, Fit, Crop, Tile }                  // Copy
    StrokeAlign { Inside, Center, Outside }              // Copy
    Stroke { width: f32, align: StrokeAlign, color: Color } // Copy
    CornerRadii { top_left, top_right, bottom_right,
                  bottom_left: f32 }                     // Copy, Default

`PaintKind` grows two variants and keeps the pinned `Solid` shape
exactly:

    enum PaintKind {
        Solid { color: Color },          // pinned v0.1 contract, unchanged
        Gradient(Gradient),
        Image { image: u32, scale_mode: ScaleMode },
    }

`PaintKind` loses `Copy` (a gradient owns its stops vector) but keeps
`Clone`, `Debug`, `PartialEq`.

The paint-table entry becomes a composition, per `DESIGN_1.md` §5's
paint-table row ("paint-kind enum, fill/stroke/effect params"):

    struct PaintEntry {
        fill: Option<PaintKind>,   // None = paint-less node (closes #55)
        stroke: Option<Stroke>,
        corners: CornerRadii,      // Default: all zero = sharp corners
        clip: bool,                // clips children to the (rounded) box
    }

    impl PaintEntry {
        fn solid(color: Color) -> Self;   // the v0.1 shorthand
    }

`PaintTable` keeps its name and its `push`/`get`/`resolve`/`len`/
`is_empty` API but stores `PaintEntry` instead of bare `PaintKind`.
`RectEntry`, `Color`, and the `Painter` trait are untouched.

## Contract-evolution note (why this does not break session A)

The pinned v0.1 cross-session contract fixes the data shapes session A
produces: `RectEntry { x, y, w, h, paint }`, solid color as 4×f32 RGBA,
and the type names. It does not freeze `PaintTable`'s entry composition,
and `dashscene-core` does not depend on `dashpaint` at all until
story #4 unifies the types — story #4 is this same session's next story, so
the wiring lands with full knowledge of this shape. The schema side is
strictly additive; `Node.paint` keeps working unchanged.

## Testing

dashbuf — new `crates/dashbuf/tests/paint_roundtrip.rs`, one focused
test per construct (every new paint kind and field, per the acceptance
criteria):

1. Gradient fill round-trips for each of the four kinds: kind
   discriminant, all three handle positions, stop offsets and colors.
2. Image fill round-trips: image index, every scale mode; `Document.
   images` round-trips format and bytes.
3. Stroke round-trips: width, every align value, color.
4. Corners + clip round-trip; absent corners read back as `None`
   (producer treats as all-zero), absent clip defaults to `false`.
5. Fill-union discrimination: `fill_type()` plus the three
   `fill_as_*` accessors, and a node carrying the legacy `paint` field
   alongside no `fill` still reads back (v0.1 regression stays green in
   the existing `roundtrip.rs`).

dashpaint — extend `crates/dashpaint/tests/boundary_b.rs`:

1. `PaintEntry::solid` fills only the fill slot (stroke `None`, sharp
   corners, no clip).
2. A paint-less entry (`PaintEntry::default()`) pushes and resolves —
   the #55 representation.
3. A full entry (gradient fill + stroke + corners + clip) pushes and
   resolves with equality.
4. The existing table/painter tests migrate from `PaintKind` to
   `PaintEntry` (the `RecordingPainter` matches on the resolved entry's
   fill).

`just build` green is the gate.

## Alternatives considered

- **Replace `Node.paint` with the `Fill` union instead of adding a
  second field** — rejected: session A's in-flight story #2 consumes
  `Node.paint` from main right now, and R7's schema discipline is
  append-only ids. Cost accepted: two ways to express a solid fill until
  a coordinated cleanup; the precedence rule is documented and will be a
  validator diagnostic.
- **Four separate `PaintKind` gradient variants (LinearGradient,
  RadialGradient, …)** — rejected: all four share one payload (handles +
  stops); a `kind` field matches the schema, keeps matches small, and
  adds kinds without new variants.
- **Per-kind gradient geometry (center/radius for radial, angle for
  angular, …)** — rejected: Figma's own model is three normalized handle
  positions for every gradient type; one geometry model round-trips the
  importer's data losslessly (P5: the IR has its own spec, but P1 wants
  intent — handles are the intent, resolved geometry is per-painter
  math).
- **Stroke as a full recursive `Fill` (gradient/image strokes)** —
  deferred, not chosen: `DESIGN_1.md` §10.1's NOW list needs stroke
  align, not stroke gradients; a solid color covers the v0 corpus. When
  a real file needs more, the field can widen additively; until then the
  importer diagnoses it (R6) — a named diagnostic, not a silent drop.
- **`PaintKind::None` variant instead of `fill: Option<PaintKind>`** —
  rejected: an `Option` is idiomatic Rust, keeps `PaintKind` meaning
  "a way to fill", and gives debt #55's paint-less node a representation
  that cannot be confused with a drawable kind.
- **Corner radii / clip on `RectEntry` instead of the paint entry** —
  rejected: `RectEntry`'s layout is pinned (and blittable per §7.3);
  corners and clip are paint-side shape parameters (`DESIGN_1.md` §5
  lists them in the paint table's "carries" column via effect params),
  and grouping them with fill/stroke keeps one dedup pool.
- **Image assets crossing boundary B in this story** — deferred to #14:
  painters need decoded pixels, which is painter-input plumbing; the
  schema (`Document.images`) and the fill reference (`image: u32`) are
  this story's scope. Recorded as an open item for #14 in the design
  record.

## Trace

- Satisfies: issue #13 acceptance criteria; `DESIGN_1.md` §10.1 NOW
  vocabulary, §11 v0.3 slice, R7 additive schema evolution.
- Resolves: #55 (paint-less node representation).
- Blocks: #14, #15, #16.
