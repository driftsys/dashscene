# dashscene-skia v0.3 vocabulary painting — design

    story    #14 (epic #12, slice v0.3)
    branch   story/skia-paint-v03
    date     2026-07-12
    status   working memory — garden into docs/ records before the PR lands

## Purpose

Teach the reference painter the v0.3 paint vocabulary (`DESIGN_1.md`
§8.1, §10.1; issue #14): the four gradient kinds (diamond via SkSL),
stroke align via path expansion, image fills with scale modes, rounded
corners, and clipping — removing the `unimplemented!` tripwires story #4
left. All lowerings are non-structural.

## Decision 1 — image assets cross boundary B as an ImageTable (closes the #13/#4 open item)

`dashpaint` gains:

    ImageFormat { Png }                       // mirrors dashbuf
    ImageAsset { format: ImageFormat, bytes: Vec<u8> }
    ImageTable — push/get/resolve/len/is_empty over ImageAsset,
                 indexed by the u32 in PaintKind::Image (same API
                 shape as PaintTable)

and the trait widens:

    fn paint(&mut self, rects: &[RectEntry], paints: &PaintTable,
             images: &ImageTable);

Encoded bytes, not decoded pixels: each painter decodes with its own
machinery (Skia natively; the lean painter later wants GPU-native
formats, which arrive as new `ImageFormat` variants). The signature
change is the in-workspace widening the trait decision record
anticipated. An empty table is valid input for image-less scenes.

## Decision 2 — anti-aliasing on; sub-pixel geometry is analytic coverage (closes debt #85)

The reference painter enables anti-aliasing for every draw. CPU-raster
AA is deterministic for the pinned skia version, so goldens stay
bit-exact; integer-aligned axis-aligned edges have coverage exactly 0
or 1, so v0.1-style scenes are unaffected by construction (the
committed v0.1 golden must survive this story unchanged — verified by
the existing golden test). Fractional and rounded geometry renders
with analytic coverage instead of story #4's edge snapping. The lean
painter's SDF AA model will differ (§8.3 accepts permanent pixel
non-identity across painters); cross-backend identity remains
structural (rect tables), pixel truth is per-painter goldens.

## Decision 3 — subtree clipsContent is runtime resolution, not painting (follow-up issue)

Boundary B is a flat rect table: a painter cannot know which rects are
a clipping node's descendants, and P2 forbids it re-deriving the tree.
`Paint.clip` ("clips children to the node's box") therefore needs
`dashscene-core` to resolve ancestor clips into painter-consumable data
at commit — a contract extension (a resolved clip region per rect) that
is its own story, filed as a follow-up and out of scope here. This
story implements what a painter legitimately owns:

- rounded corners shape the entry's own fill and stroke (drawn as an
  rrect); and
- image content is clipped to the entry's (rounded) node box (cover
  and tile overflow must not leak).

`entry.clip = true` keeps a named `unimplemented!` pointing at the
follow-up issue — never a silent misrender (P4).

## Decision 4 — the schema gains the two fields Figma image fills need

`dashbuf.fbs`, additive: `ImageFill` gains
`transform: Mat23` (new struct — row-major 2×3 affine in normalized
image space; identity when absent) used by `ScaleMode::Crop` (Figma
sends `imageTransform` with CROP), and `tile_scale: float32 = 1.0`
used by `ScaleMode::Tile` (Figma's `scalingFactor`). Without these the
two modes are unpaintable as authored. Mirrored in `dashpaint`'s
`PaintKind::Image` as `transform: Option<Mat23>` (a new `#[repr(C)]`
struct) and `tile_scale: f32`.

## Gradient lowering — one geometry model

Handles are normalized to the node box (`paint-entry-composition.md`).
Build one affine frame per gradient: unit space → handle frame → node
box. Then:

- linear: skia linear gradient (0,0)→(1,0) under the frame;
- radial: skia radial gradient (unit circle under the frame — Figma's
  elliptical radials come from the frame's axes);
- angular: skia sweep gradient around the frame origin;
- diamond: an SkSL runtime effect shading `t = |x| + |y|` in unit
  space under the frame (not a Skia primitive, §8.1).

Stops map to colors[]/positions[]; tile mode Clamp. Degenerate frames
(zero-area) fall back to the first stop's color — deterministic, and
the importer/validator owns rejecting them upstream (P4).

## Stroke align lowering

Skia strokes are center-only (§8.1). Inside/outside lower by geometry
expansion: stroke the rrect inset (inside) or outset (outside) by
width/2, with corner radii adjusted by the same amount (clamped at 0),
then center-stroke with the authored width. Solid stroke color, per
the v0.3 schema.

## Image scale modes

Decoded via skia from the `ImageTable` entry; drawn clipped to the
entry's rounded box:

- Fill: aspect-preserving cover, centered;
- Fit: aspect-preserving contain, centered (uncovered area stays
  whatever is beneath);
- Crop: the normalized `transform` maps the image into the box;
- Tile: repeat at `tile_scale`, anchored at the box origin.

## Testing

Unit-level render tests per paint kind in
`crates/dashscene-skia/tests/painter.rs` (hand-built boundary-B input —
no producer can stage this vocabulary yet, and the painter contract
needs no producer): assert exact bytes at stop extremes and mode-
distinguishing pixels (gradient t=0/t=1 regions, stroke bands
inside/on/outside the outline, image quadrant placement per mode,
rounded-corner coverage vs the square corner). One new golden
(`v03-paint.png`, hand-built scene covering every kind) pins the full
rendering; the v0.1 golden must pass unchanged (decision 2's proof).
The `resolved_color_bits` tripwire in core stays — it guards producer
staging, which this story does not extend.

## Alternatives considered

- **Decoded pixels in the ImageTable** — rejected: forces one decoded
  format on every backend; encoded+format-tagged lets the lean painter
  consume GPU-native containers later (§9 texture policy).
- **Painter-side image registration (out-of-band)** — rejected: boundary
  B is "the entire painter input" (§7.3); a side channel breaks
  bisect-by-construction.
- **Geometry-intersecting subtree clips in core's committed rects** —
  rejected for now: correct only for axis-aligned clips on solid
  fills; distorts gradient frames and cannot express rounded clips.
  The follow-up story defines the real resolved-clip contract.
- **AA off with snapped rounded corners** — rejected: stair-stepped
  corners misrepresent the vocabulary the story exists to paint; AA on
  is equally deterministic on pinned CPU raster (#85).
- **Crop as Fill until a transform exists** — rejected: a silent
  misrender (P4); the additive schema field is small and matches
  Figma's actual data.

## Trace

- Satisfies: issue #14 acceptance criteria; `DESIGN_1.md` §8.1
  lowerings, §10.1 NOW vocabulary, §11 v0.3.
- Resolves: debt #85 (sub-pixel policy); the #13/#4 image-crossing
  open item (both design records' "Open for story #14" sections close).
- Files: a follow-up issue for subtree clipsContent resolution in core.
- Blocks: #18; #44/#45 (v0.8) build on this surface.
