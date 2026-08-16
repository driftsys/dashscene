//! The Figma REST subset the v0.3 lowering reads.
//!
//! Deliberately partial: only the fields the v0.3 paint vocabulary needs.
//! Every shape here is pinned by `corpus/figma-fixtures/v03-paint.json`, not
//! by a reading of Figma's documentation — the lowering was deferred out of
//! #16 precisely so it would never be written against a guess (P5).
//!
//! Every Figma-vocabulary field with a small closed set of values — a
//! paint's `type`, an image fill's `scaleMode`, a stroke's `align` — stays a
//! `String`, like the file's other open-vocabulary fields (`Node::kind`,
//! `Effect::kind`). An unknown value used to deserialize into a Rust enum and
//! fail the whole parse; the walk's named catch-all diagnostic is not a
//! silent default (P4), so a `String` field plus a walk-side verdict is now
//! this file's only pattern — parse never refuses on a value it does not
//! recognize.

use serde::{Deserialize, Deserializer};

/// A `GET /v1/files/:key` response. Only `document` is read.
#[derive(Debug, Deserialize)]
pub struct FigmaFile {
    pub document: Node,
}

/// One node of the Figma tree.
///
/// `kind` stays a `String` rather than an enum: Figma's node vocabulary is
/// open (TEXT, VECTOR, INSTANCE, …) and v0.3 handles only `FRAME`. The walk
/// rejects the rest by name, so a new Figma node type is a loud error here
/// rather than a parse failure of the whole file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Figma's stable node id (`"1:23"`), unique across the file and
    /// pinned by every capture. A diagnostic path uses it to tell two
    /// same-named siblings apart (debt #150). Optional so a synthetic
    /// test document does not have to invent one.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub fills: Vec<Paint>,
    #[serde(default)]
    pub strokes: Vec<Paint>,
    /// A `VECTOR` node's filled geometry (story B1) — Figma's `fillGeometry`,
    /// present only when the file was fetched with `geometry=paths`. Each
    /// entry is one closed path (`M`/`L`/`C`/`Z`) plus its winding rule; the
    /// node's fill is the concatenation of them, holes riding as EVENODD
    /// subpaths. Absent (an empty vector) for a non-vector node or a
    /// geometry-free fetch.
    #[serde(default)]
    pub fill_geometry: Vec<Geometry>,
    /// A `VECTOR` node's stroke outline (story B1) — Figma's `strokeGeometry`,
    /// the already-expanded closed outline of the stroke. The fieldable
    /// geometry for a stroke-only vector (a hairline arrow), whose
    /// `fillGeometry` is degenerate.
    #[serde(default)]
    pub stroke_geometry: Vec<Geometry>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub stroke_weight: Option<f32>,
    #[serde(default)]
    pub stroke_align: Option<String>,
    /// The stroke's shape family. Every frame in `v03-paint.json` carries
    /// `{"strokeType": "BASIC"}`, which is what pins the shape of this field.
    /// `dashpaint::Stroke` is solid and uniform-width, so the walk rejects any
    /// other stroke type loudly rather than repainting it as a plain solid
    /// stroke (P4).
    #[serde(default)]
    pub complex_stroke_properties: Option<ComplexStrokeProperties>,
    /// The dash pattern, in pixels — pinned by `lowering-variant-topology.json`,
    /// whose root carries `"strokeDashes": [10, 5]`. A continuous stroke omits
    /// the field.
    ///
    /// That same node also carries `{"strokeType": "BASIC"}`, which is the load-
    /// bearing detail: Figma expresses a dash pattern **without** changing the
    /// stroke type, so the `complex_stroke_properties` gate alone would never
    /// catch a dashed stroke. `dashpaint::Stroke` has no dash vocabulary, so
    /// this is the only gate that stops a dashed border being repainted as a
    /// continuous one (P4).
    #[serde(default)]
    pub stroke_dashes: Option<Vec<f32>>,
    /// Mutually exclusive with `rectangle_corner_radii`; Figma nulls the
    /// other.
    #[serde(default)]
    pub corner_radius: Option<f32>,
    /// `[top_left, top_right, bottom_right, bottom_left]` — the same order as
    /// `dashpaint::CornerRadii`'s fields.
    #[serde(default)]
    pub rectangle_corner_radii: Option<[f32; 4]>,
    #[serde(default)]
    pub corner_smoothing: Option<f32>,
    /// An `ELLIPSE` node's arc parameters
    /// (`docs/decisions/figma-ellipse-as-circle.md`). A full ellipse sweeps
    /// `2π` from angle `0` with `innerRadius 0`; a partial sweep is an arc
    /// (pie) and a non-zero inner radius is a ring (donut), neither of which
    /// the rounded-rect lowering can express, so each is a named diagnostic
    /// (P4). Absent means a full ellipse — Figma's default. Pinned by
    /// `lowering-negative-gap.json`.
    #[serde(default)]
    pub arc_data: Option<ArcData>,
    #[serde(default)]
    pub clips_content: bool,
    #[serde(default)]
    pub blend_mode: Option<String>,
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Figma's auto-layout mode: `NONE`, `HORIZONTAL`, `VERTICAL`, or the
    /// newer `GRID`. `HORIZONTAL`/`VERTICAL` lower into the document's flex
    /// vocabulary (story #140). `GRID` is refused: the document and the
    /// runtime gain grid at v0.8 (#43), so there is nothing to lower it
    /// into — and inside a grid frame every `absolute_bounding_box` is
    /// Figma's own solver output, which P1 forbids writing as intent.
    ///
    /// A `String` for the same reason `kind` is: the vocabulary is open —
    /// `GRID` is recent — and a mode this lowering has not seen must be a loud
    /// error, not a parse failure of the whole file.
    #[serde(default)]
    pub layout_mode: Option<String>,
    /// `NO_WRAP` or `WRAP`, present on every captured auto-layout frame.
    /// Wrap is v0.8 layout-fidelity vocabulary (`docs/roadmap.md`), so
    /// `WRAP` is refused rather than flattened onto a single line.
    #[serde(default)]
    pub layout_wrap: Option<String>,
    /// Main-axis gap between children, in pixels. Negative values are
    /// legal Figma vocabulary (overlap) and lower to child margins
    /// (`docs/decisions/negative-gap-lowering.md`). Pinned by
    /// `lowering-negative-gap.json` (`itemSpacing: -16`).
    #[serde(default)]
    pub item_spacing: Option<f32>,
    // The four padding edges. Figma omits an edge that is zero.
    #[serde(default)]
    pub padding_left: Option<f32>,
    #[serde(default)]
    pub padding_right: Option<f32>,
    #[serde(default)]
    pub padding_top: Option<f32>,
    #[serde(default)]
    pub padding_bottom: Option<f32>,
    /// Main-axis alignment: absent (= `MIN`), `CENTER`, `MAX`, or
    /// `SPACE_BETWEEN`. Open vocabulary, same posture as `layout_mode`.
    #[serde(default)]
    pub primary_axis_align_items: Option<String>,
    /// Cross-axis alignment: absent (= `MIN`), `CENTER`, `MAX`, or
    /// `BASELINE`. `BASELINE` lowers to `CrossAxisAlign::Baseline` at
    /// v0.8 (Q-4, story #264); pinned by `lowering-baseline.json`.
    #[serde(default)]
    pub counter_axis_align_items: Option<String>,
    /// The cross-axis gap between wrap lines — Figma's `counterAxisSpacing`.
    /// Lowers to the container's `cross_gap` for a `WRAP` frame (story
    /// #264). Pinned by `lowering-wrap.json` (`counterAxisSpacing: 16`).
    #[serde(default)]
    pub counter_axis_spacing: Option<f32>,
    /// How wrap lines distribute along the cross axis — Figma's
    /// `counterAxisAlignContent`: `AUTO` (the packed default the runtime
    /// carries) or `SPACE_BETWEEN`. `SPACE_BETWEEN` has no vocabulary and
    /// is refused by name (story #264, P4). Pinned `AUTO` by
    /// `lowering-wrap.json`.
    #[serde(default)]
    pub counter_axis_align_content: Option<String>,
    /// Per-axis sizing: `FIXED`, `HUG`, or `FILL` — the modern encoding,
    /// present on every node the captures place in an auto-layout context.
    /// Absent means fixed (a node outside auto-layout). The older
    /// container-side encoding (`primaryAxisSizingMode`/
    /// `counterAxisSizingMode`) carries no information these two do not,
    /// so it is not read.
    #[serde(default)]
    pub layout_sizing_horizontal: Option<String>,
    #[serde(default)]
    pub layout_sizing_vertical: Option<String>,
    // Authored min/max clamps. Absent = unconstrained. Pinned by
    // `grid-basic.json` (`fill-minmax` carries minWidth/maxWidth).
    #[serde(default)]
    pub min_width: Option<f32>,
    #[serde(default)]
    pub max_width: Option<f32>,
    #[serde(default)]
    pub min_height: Option<f32>,
    #[serde(default)]
    pub max_height: Option<f32>,
    // ---- GRID container-side fields (story #264) ---------------------
    // Pinned by `grid-basic.json`. The gaps are separate per axis, and
    // the track sizes are Figma's own serialized track strings (e.g.
    // "160px minmax(0,1fr) minmax(0,1fr)"), not a plain count — a count
    // cannot express the 160px first column. `gridColumnCount` /
    // `gridRowCount` are not read: the sizing strings carry one entry per
    // track, so the counts are redundant (the REST subset stays partial —
    // only what the lowering needs).
    /// The row gap — lowers to the container's `cross_gap` under `GRID`.
    #[serde(default)]
    pub grid_row_gap: Option<f32>,
    /// The column gap — lowers to the container's `gap` under `GRID`.
    #[serde(default)]
    pub grid_column_gap: Option<f32>,
    /// Per-track sizing, left-to-right / top-to-bottom, as a
    /// whitespace-separated CSS-like track string. `Npx` is a fixed
    /// length; `minmax(0,Nfr)` is a fraction weight. Any other token is a
    /// named refusal (story #264, P4).
    #[serde(default)]
    pub grid_columns_sizing: Option<String>,
    #[serde(default)]
    pub grid_rows_sizing: Option<String>,
    // ---- GRID child-side placement (story #264) ---------------------
    /// The 0-based anchor cell. Absent = auto-placement in document order.
    #[serde(default)]
    pub grid_row_anchor_index: Option<u16>,
    #[serde(default)]
    pub grid_column_anchor_index: Option<u16>,
    /// The number of tracks the child spans. Absent = 1.
    #[serde(default)]
    pub grid_row_span: Option<u16>,
    #[serde(default)]
    pub grid_column_span: Option<u16>,
    /// `ABSOLUTE` takes a child out of its auto-layout parent's flow and
    /// places it by its box. The document has no vocabulary for an
    /// absolutely-placed flex child, and treating one as in-flow would
    /// reflow everything after it — so it is refused (P4). Absent means
    /// `AUTO` (in flow). No capture carries it; the shape is Figma's
    /// documented enum, flagged as capture-unpinned in
    /// `docs/technotes/figma-rest-shapes.md`.
    #[serde(default)]
    pub layout_positioning: Option<String>,
    /// `true` makes strokes consume layout space (CSS border-like) — the
    /// strokes-in-layout Figma≠CSS difference. The lowering keeps strokes
    /// out of layout (the schema has no border vocabulary), so `true` is
    /// refused rather than solved to a different size. Absent = `false`.
    #[serde(default)]
    pub strokes_included_in_layout: Option<bool>,
    /// `true` reverses the paint order of an auto-layout frame's children.
    /// Document order is paint order, so a reversed stack has no lowering
    /// short of reordering nodes — refused (P4). Absent = `false`.
    #[serde(default)]
    pub item_reverse_z_index: Option<bool>,
    /// Page-absolute. The lowering subtracts the parent's origin to get the
    /// parent-relative intent `Document` wants. Never `absoluteRenderBounds`,
    /// which is a *result* (P1).
    #[serde(default)]
    pub absolute_bounding_box: Option<Rect>,
    /// **Radians**, and Figma omits the field entirely when it is zero, so
    /// `None` and `Some(0.0)` both mean unrotated.
    ///
    /// This doc comment said *Degrees* until story #770, and it never
    /// mattered because the lowering only compared the value with zero. It
    /// became a factor-of-57.3 error the moment the value was lowered
    /// rather than refused.
    ///
    /// Verified against `corpus/figma-fixtures/node-fx.json`, whose
    /// `rotated-15deg` RECTANGLE carries `-0.26179940325453416` — −15° in
    /// radians — beside a `relativeTransform` whose cosine is 0.9659258 and
    /// sine 0.2588191, the cosine and sine of 15°.
    ///
    /// The sign needs no flip on the way in. Figma's matrix is
    /// `[[cos, +sin, tx], [−sin, cos, ty]]` for this field's negation, which
    /// is the same matrix as this repository's y-down, clockwise-positive
    /// convention evaluated at the field's own value.
    ///
    /// **Read through [`Node::turn`], never directly.** A turn can also
    /// arrive in `relative_transform`, and the two blockers and the origin
    /// derivation must read the same source (issue #878).
    #[serde(default)]
    pub rotation: Option<f32>,
    /// The top two rows of the node's 2D transform relative to its parent,
    /// row-major `[[m00, m01, tx], [m10, m11, ty]]` — the same six components
    /// as `Paint::image_transform` and as `dashpaint::Mat23`. Absent means
    /// identity.
    ///
    /// Parsed for one thing only: the turn its linear part carries, read
    /// through [`Node::turn`]. Its `tx`/`ty` are not read — a node's position
    /// comes from `absolute_bounding_box` — and its scale is read only for
    /// the sign of the determinant, never for a magnitude, because the
    /// document has no vocabulary for a scale
    /// (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`,
    /// "Scale and skew are not in this slice").
    #[serde(default)]
    pub relative_transform: Option<[[f32; 3]; 2]>,
    /// The node's own width and height, **before** any rotation — what
    /// `absolute_bounding_box` stops reporting the moment a node turns
    /// (story #770).
    ///
    /// For the `rotated-15deg` fixture above this is 100 × 100 while the
    /// bounding box reads 122.474 × 122.474, because a bounding box is the
    /// axis-aligned bounds of the *rotated* shape — a result, and 22.5 %
    /// high at 15°. A rotated node's extent comes from here.
    #[serde(default)]
    pub size: Option<Vector>,
    /// Whether this node masks its following siblings (story #44). A
    /// box-shaped outline mask lowers into `Node.mask`; a soft (alpha or
    /// luminance) mask, or a mask whose shape the box vocabulary cannot
    /// express, is refused by name (P4).
    #[serde(default)]
    pub is_mask: Option<bool>,
    /// A mask node's compositing type — Figma's `maskType`: `ALPHA`
    /// (the masking layer's alpha channel), `LUMINANCE` (its brightness),
    /// or `VECTOR` (its vector geometry) — measured against the live REST
    /// API on file OAXcoWO5j5NghXV3ZKw9QV (issue #517); `OUTLINE` is not a
    /// value the REST API has been observed to emit. Only a geometry/vector
    /// mask lowers to a hard box clip; alpha and luminance are soft masks
    /// the clip-region vocabulary cannot express (story #44 M6). Absent on
    /// a synthetic node, which lowers as the geometric default.
    #[serde(default)]
    pub mask_type: Option<String>,
    /// Whether a `SECTION`'s children are hidden (#309). The document has no
    /// vocabulary for a hidden-contents section, so the walk refuses one by
    /// name rather than silently rendering children Figma hides. Absent =
    /// `false`.
    #[serde(default)]
    pub section_contents_hidden: Option<bool>,
    /// A `TEXT` node's authored characters (story #160). The runtime shapes
    /// and breaks them; the document carries the codepoints, never the
    /// rendered lines (P1). Pinned by `lowering-baseline.json` and
    /// `lowering-hug-in-fill.json`.
    #[serde(default)]
    pub characters: Option<String>,
    /// A `TEXT` node's base style — Figma's `TypeStyle`.
    #[serde(default)]
    pub style: Option<TextStyle>,
    /// Per-character style overrides, keyed by the ids in
    /// `character_style_overrides`. A single-style text node carries an empty
    /// table; a non-empty one means multiple style segments, which the
    /// single-style `TextStyle` cannot express — a named diagnostic (P4).
    /// Pinned empty by both text captures.
    #[serde(default)]
    pub style_override_table: serde_json::Map<String, serde_json::Value>,
    /// The component an `INSTANCE` currently shows (story #773). For an
    /// instance of a component *set* this names one member `COMPONENT`, which
    /// is what selects the lowered `VariantSet`'s `active_member`.
    ///
    /// Story #242 stated that the walk does not read this field, because the
    /// closure had already validated the reference and the baked subtree is
    /// the authored content. That stays true of the *walk*; the variant-table
    /// pass reads it, because which member an instance shows is not
    /// recoverable from the baked children alone.
    #[serde(default)]
    pub component_id: Option<String>,
    /// The node's prototype interactions — the Plugin API's `reactions`,
    /// which REST serializes under this name (story #773).
    ///
    /// Empty on every capture committed before
    /// `prototype-smart-animate.json`, which is why nothing pinned the shape
    /// until #882. What is read here and what is deliberately not is
    /// `docs/technotes/figma-rest-shapes.md`
    /// §"The prototype-interaction shapes"; the flat
    /// `transitionNodeID`/`transitionDuration`/`transitionEasing` triple is
    /// **not** in this struct, and that is the point — it is lossy, it cannot
    /// express the trigger or the navigation, and where an interaction says
    /// there is no transition the triple invents one.
    #[serde(default)]
    pub interactions: Vec<Interaction>,
}

impl Node {
    /// This node's turn in **radians**, from whichever field carries it —
    /// the one value every rotated-node rule reads (issue #878).
    ///
    /// `rotation` is the ordinary source and lowers unconverted. It is read
    /// first, and `relative_transform`'s off-diagonal only where `rotation`
    /// is absent or zero. No capture pins that Figma always populates
    /// `rotation` for a node whose matrix carries a turn, and none could:
    /// `corpus/figma-fixtures/` holds one rotated node, and it carries both
    /// encodings. Reading both is the defensive posture, because a turn read
    /// as zero is not only a missed refusal — `rotated_bounds_offset` derives
    /// the node's own origin from this value, so the node would also lower at
    /// the wrong position and the wrong extent (P4).
    ///
    /// What the matrix is read for is `matrix_turn` below, which states the
    /// two shapes it declines to read as a turn and why.
    pub fn turn(&self) -> f32 {
        match self.rotation {
            Some(rotation) if rotation != 0.0 => rotation,
            _ => self.relative_transform.map_or(0.0, matrix_turn),
        }
    }
}

/// Below this many radians a derived turn is float residue rather than
/// authored intent, and reads as zero.
///
/// It exists only on the matrix path. `rotation` needs no tolerance: Figma
/// omits it entirely when it is zero, so an unrotated node carries no value
/// to be residue. A `relativeTransform` is written for every node whether it
/// turns or not, so an unrotated one is an identity matrix that a round trip
/// through Figma's own arithmetic could leave a residue in — and the
/// consequence of reading that residue as a turn is a refused node, which
/// under `EmitPolicy::Strict` withholds the whole document.
///
/// The threshold is derived rather than picked: at 1e-6 rad the far corner of
/// a 4096 px node moves 0.004 px, so no turn this rule discards can reach a
/// pixel on any surface this runtime targets. Every unrotated node across the
/// committed captures carries an exact zero, so nothing in the corpus is
/// within orders of magnitude of it either way.
const TURN_EPSILON: f32 = 1e-6;

/// The turn a `relativeTransform` carries, in radians, or `0.0` where it
/// carries none this document can express.
///
/// The **determinant** decides whether the matrix turns at all. A positive
/// determinant is a rotation, with or without a positive scale, and its angle
/// is `atan2(m10, m00)` — the same derivation, and the same sign, that
/// `rotation` itself reports.
/// `corpus/figma-fixtures/node-fx.json`'s `rotated-15deg` carries
/// `-0.26179940325453416` beside an `m10` of `-0.2588190734386444` and an
/// `m00` of `0.9659258723258972`, whose `atan2` is that angle to every digit
/// an `f32` holds.
///
/// A **negative** determinant is a mirror, and reads as `0.0`. The document
/// has no vocabulary for a mirror, so reporting one as the angle `atan2`
/// gives it — a half-turn for a horizontal flip — would draw a new wrong
/// picture rather than repair one. A zero determinant is a collapsed matrix
/// and carries no angle at all.
///
/// The determinant is what separates a mirror from a **half-turn**, which the
/// off-diagonal alone cannot: both `[[-1, 0], [0, 1]]` and `[[-1, 0],
/// [0, -1]]` have zero off-diagonals, and only the second is a rotation. An
/// off-diagonal test would have let a frame turned 180° lower upright, which
/// is the exact silent wrong picture issue #878 is about.
fn matrix_turn(m: [[f32; 3]; 2]) -> f32 {
    let [[m00, _, _], [m10, _, _]] = m;
    if handedness(m) != Handedness::Upright {
        return 0.0;
    }
    let turn = m10.atan2(m00);
    if turn.abs() < TURN_EPSILON { 0.0 } else { turn }
}

/// Which way a `relativeTransform` faces, as its determinant reports it.
///
/// One classification, and `matrix_turn` reads it too, because the two are
/// halves of one answer: `matrix_turn` reports an angle only for `Upright`, so
/// anything else leaves the whole orientation uncarried and a caller that must
/// not drop it in silence (P4) needs to see which case it is.
///
/// **The boundary is exact zero, deliberately.** A tolerance here was tried
/// and reverted: the determinant is an *area* scale, so any band around zero
/// is a band of real matrices, and a 1e-6 threshold discarded the rotation of
/// a node at uniform scale 0.001 — reporting no turn for a node that plainly
/// turns, which is the silent wrong picture issue #878 exists to prevent. The
/// residue argument that motivated it has no population behind it either:
/// every matrix in `corpus/figma-fixtures/` is a pure rotation, whose
/// determinant is 1.0 to every digit an `f32` holds.
#[derive(PartialEq)]
enum Handedness {
    /// A rotation, with or without a positive scale. Its angle is the one
    /// `matrix_turn` reports.
    Upright,
    /// A mirror. The document has no vocabulary for one, so no angle is
    /// reported and the whole orientation is uncarried.
    Mirrored,
    /// No area at all, so no handedness and no angle. The node's own zero
    /// extent is what names it.
    Collapsed,
}

fn handedness(m: [[f32; 3]; 2]) -> Handedness {
    let [[m00, m01, _], [m10, m11, _]] = m;
    let determinant = m00 * m11 - m01 * m10;
    if determinant > 0.0 {
        Handedness::Upright
    } else if determinant < 0.0 {
        Handedness::Mirrored
    } else {
        Handedness::Collapsed
    }
}

/// Whether a matrix encloses any area — false only for a collapsed one, which
/// is neither upright nor mirrored and whose real difference from a member
/// that does is its extent, not its handedness.
pub(super) fn has_area(m: Option<[[f32; 3]; 2]>) -> bool {
    m.is_none_or(|m| handedness(m) != Handedness::Collapsed)
}

/// Whether a matrix carries an angle `Node::turn` reports. Only an upright one
/// does, so this is the test a caller uses to ask whether the *rest* of the
/// linear part reached anything.
pub(super) fn carries_its_angle(m: Option<[[f32; 3]; 2]>) -> bool {
    m.is_none_or(|m| handedness(m) == Handedness::Upright)
}

/// Whether a matrix mirrors — the one handedness the document has no vocabulary
/// for at all.
///
/// Deliberately narrower than `!carries_its_angle`, which is also true of a
/// collapsed matrix: a collapsed one encloses no area, and its own zero extent
/// is what names it rather than its handedness (`has_area` above is that test).
/// The walk refuses on this alone, so widening it would refuse a node for the
/// wrong reason.
pub(super) fn is_mirrored(m: Option<[[f32; 3]; 2]>) -> bool {
    m.is_some_and(|m| handedness(m) == Handedness::Mirrored)
}

/// Whether two `relativeTransform`s face the same way — the same handedness
/// and the same angle, with the scale each carries divided out.
///
/// Each column of the linear part is scaled to unit length, which is exactly
/// the magnitude `absoluteBoundingBox` already carries and `Props` already
/// compares. What survives is the orientation, which nothing else can see once
/// a mirror makes [`matrix_turn`] report `0.0` — so this is the third reading
/// of the same six numbers, and it lives beside the other two because the row
/// and column convention they share is what all three encode.
///
/// An absent matrix is the identity. A column with no length at all is a
/// collapsed matrix, which has no orientation to compare: it matches only
/// another collapsed one, and the node's own zero extent is what names it.
pub(super) fn same_orientation(a: Option<[[f32; 3]; 2]>, b: Option<[[f32; 3]; 2]>) -> bool {
    const IDENTITY: [[f32; 3]; 2] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    // `handedness` decides degeneracy here too. Testing the column lengths
    // instead would disagree with `has_area` on a matrix whose columns are
    // long but parallel — zero area, two usable columns — and the diagnostic
    // would then name the one property such a pair agrees on.
    let oriented = |m: [[f32; 3]; 2]| {
        let [[m00, m01, _], [m10, m11, _]] = m;
        let (x, y) = (m00.hypot(m10), m01.hypot(m11));
        (handedness(m) != Handedness::Collapsed).then(|| [m00 / x, m10 / x, m01 / y, m11 / y])
    };
    match (
        oriented(a.unwrap_or(IDENTITY)),
        oriented(b.unwrap_or(IDENTITY)),
    ) {
        // `TURN_EPSILON` itself, not a second copy of it: for a small
        // angle the difference between two unit-length components *is* that
        // angle in radians, so the two are one quantity and a refit of its
        // derivation has to move both.
        (Some(a), Some(b)) => a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= TURN_EPSILON),
        // Two collapsed matrices agree; a collapsed one and a real one do not.
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// One prototype interaction: what starts it, and what it does (story #773).
///
/// The Plugin API's `Reaction` carries a deprecated singular `action` beside
/// `actions`; the string appears zero times across both captures, so REST
/// emits the plural only and no fallback is read.
#[derive(Debug, Deserialize)]
pub struct Interaction {
    /// Absent for a reaction with no trigger. `kind` stays a `String` for
    /// the same reason `Node::kind` does: the trigger vocabulary is open.
    #[serde(default)]
    pub trigger: Option<Trigger>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// What starts an interaction — `ON_CLICK`, `AFTER_TIMEOUT`, `ON_KEY_DOWN`, …
#[derive(Debug, Deserialize)]
pub struct Trigger {
    #[serde(rename = "type")]
    pub kind: String,
}

/// One thing an interaction does. `NODE` is the only kind with a lowering;
/// `URL`, `SET_VARIABLE` and `CONDITIONAL` are refused by name (P4), and
/// `CONDITIONAL` nests `Action[]` recursively inside `conditionalBlocks`,
/// which is not read: the whole action is refused, so its branches have
/// nothing to lower into.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: String,
    /// The node a `NODE` action targets. For `CHANGE_TO` that is one member
    /// `COMPONENT` of the set the acting node belongs to.
    #[serde(default)]
    pub destination_id: Option<String>,
    /// `CHANGE_TO` (a variant switch — the one navigation with a lowering),
    /// `NAVIGATE`, `OVERLAY`, `SCROLL_TO`, …
    #[serde(default)]
    pub navigation: Option<String>,
    /// Absent, or explicitly `null`, when the action carries no transition
    /// (`refused-on-key-down` pins the `null` spelling).
    #[serde(default)]
    pub transition: Option<Transition>,
}

/// How a `NODE` action animates. `SMART_ANIMATE` is the only kind with a
/// lowering: it interpolates whatever differs between the two variants,
/// which is what a `VariantTransition`'s per-prop tracks express.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub easing: Option<TransitionEasing>,
    /// **Seconds**, and `dashcue`'s `TweenSpec::duration` is seconds too, so
    /// this lowers unscaled.
    ///
    /// `@figma/rest-api-spec` documents this field as milliseconds, which is
    /// wrong — a 0.3 s transition returns `0.30000001192092896` here and
    /// `300` in the flat `transitionDuration` beside it. Dividing by 1000 on
    /// the strength of that comment would animate every transition in under a
    /// millisecond, and both fields are `number`, so nothing would object
    /// (`docs/technotes/figma-rest-shapes.md`).
    #[serde(default)]
    pub duration: Option<f32>,
}

/// A transition's easing. The four spring presets (`GENTLE`, `QUICK`,
/// `BOUNCY`, `SLOW`) arrive as a bare `{"type": …}` with no
/// `easingFunctionSpring`, so the parameters a `dashcue` `Spring` needs are
/// not in the payload; `CUSTOM_CUBIC_BEZIER` by contrast arrives with its
/// four control points populated. Neither shape lowers this slice, so
/// neither is read beyond its name.
#[derive(Debug, Deserialize)]
pub struct TransitionEasing {
    #[serde(rename = "type")]
    pub kind: String,
}

/// An `ELLIPSE` node's `arcData` — its pie/ring parameters.
///
/// Angles are in radians. A full ellipse is `startingAngle 0`,
/// `endingAngle 2π`, `innerRadius 0` (a fraction of the radius, `0.0`–`1.0`).
/// Pinned by `lowering-negative-gap.json`, whose ellipses are all full.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcData {
    pub starting_angle: f32,
    pub ending_angle: f32,
    pub inner_radius: f32,
}

/// One contour of a `VECTOR` node's `fillGeometry`/`strokeGeometry` (story
/// B1): an SVG path string in the census vocabulary (`M`/`L`/`C`/`Z`) and its
/// fill rule. Figma emits these only when the file is fetched with
/// `geometry=paths`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub path: String,
    /// Figma's `windingRule`: `NONZERO` or `EVENODD`. Absent defaults to
    /// NONZERO (Figma always emits it alongside path geometry).
    #[serde(default)]
    pub winding_rule: Option<String>,
}

/// The stroke's shape family.
///
/// `stroke_type` stays a `String` for the same reason `Node::kind` does: the
/// vocabulary is open (`BASIC`, `DASHED`, and the variable-width types), and
/// only `BASIC` lowers, so an unrecognized value must be a loud error rather
/// than a parse failure of the whole file.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexStrokeProperties {
    #[serde(default)]
    pub stroke_type: Option<String>,
}

/// A `TEXT` node's base style — Figma's `TypeStyle` (story #160).
///
/// The document's `TextStyle` carries `fontFamily`, `fontSize`, `fontWeight`,
/// and the fill color only (the runtime consumes family and size; the painter
/// the color — `docs/design/typeset-latin.md`). Every other axis here is read
/// solely to *diagnose* it: a non-default alignment, line height, letter
/// spacing, decoration, case transform, italic, hyperlink, or OpenType flag
/// has nothing to lower into, and lowering the text without it would drop the
/// designer's intent in silence (P4). The default values (`LEFT`/`TOP`,
/// `INTRINSIC_%` line height, zero letter spacing, upright, no decoration) are
/// pinned by both text captures, which lower cleanly.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub font_family: String,
    /// `Regular`, `Bold`, `Italic`, `Bold Italic`, … The document font
    /// reference is family + weight; an italic style has no vocabulary and is
    /// diagnosed.
    #[serde(default)]
    pub font_style: Option<String>,
    /// CSS-scale weight (100–900). Lowered verbatim; the range check is the
    /// validator's (#41/#129). Absent means 400.
    #[serde(default)]
    pub font_weight: Option<f32>,
    /// Em size in document units.
    pub font_size: f32,
    /// `LEFT` (the default — the runtime flushes LTR left, RTL right, by
    /// direction), `CENTER`, `RIGHT`, or `JUSTIFIED`.
    #[serde(default)]
    pub text_align_horizontal: Option<String>,
    /// `TOP` (default), `CENTER`, or `BOTTOM`.
    #[serde(default)]
    pub text_align_vertical: Option<String>,
    /// `WIDTH_AND_HEIGHT`, `HEIGHT`, `NONE`, or `TRUNCATE`. The sizing modes
    /// map through `layoutSizingHorizontal`/`layoutSizingVertical` (the modern
    /// per-axis pair, `docs/decisions/figma-flex-lowering.md` D1);
    /// `TRUNCATE` is the one value that pair cannot express — an ellipsis has
    /// no vocabulary, so it is diagnosed.
    ///
    /// Absent and `null` both normalize to `NONE` here, at the parse boundary
    /// (debt #339): `NONE` is the REST default and Figma omits the field for a
    /// fixed-box label, so absent *is* `NONE`
    /// (`docs/decisions/figma-text-lowering.md` D2). Reading that default once,
    /// where the field enters the compiler, is what keeps a later consumer from
    /// treating absent as some other mode — the #332 bug class, where absent
    /// was mapped to auto-size and a fixed label collapsed to its content.
    #[serde(
        default = "text_auto_resize_none",
        deserialize_with = "text_auto_resize_or_none"
    )]
    pub text_auto_resize: String,
    /// `NONE` (default), `UNDERLINE`, or `STRIKETHROUGH`.
    #[serde(default)]
    pub text_decoration: Option<String>,
    /// `ORIGINAL` (default), `UPPER`, `LOWER`, `TITLE`, … A case transform
    /// rewrites the rendered glyphs, so it is diagnosed rather than dropped.
    #[serde(default)]
    pub text_case: Option<String>,
    /// Pixels. Zero (the default) lowers cleanly; the runtime tracks no
    /// letter spacing, so a non-zero value is diagnosed. Figma's REST flattens
    /// this to a number (pinned by the captures).
    #[serde(default)]
    pub letter_spacing: Option<f32>,
    /// `INTRINSIC_%` is Figma's "Auto" — the font's natural line advance,
    /// which is what the runtime uses. `PIXELS` is a fixed line height that
    /// lowers into `line_height_px`; `FONT_SIZE_%` and `PERCENT` are
    /// percentage line heights with no vocabulary and stay diagnosed.
    #[serde(default)]
    pub line_height_unit: Option<String>,
    /// The line height in document units, meaningful when `line_height_unit`
    /// is `PIXELS`. Figma serializes it alongside the unit.
    #[serde(default)]
    pub line_height_px: Option<f32>,
    /// A hyperlink on the whole text run. No vocabulary, so diagnosed.
    #[serde(default)]
    pub hyperlink: Option<serde_json::Value>,
    /// OpenType feature flags. Exactly `{"LIGA": 0}` (standard ligatures off)
    /// lowers into `ligatures_off` (story #341) — the one flag measured on a
    /// real file's text; any other flag, value, or combination has no
    /// vocabulary (`liga`/`clig` posture is otherwise the runtime's per-run
    /// default, not authored,
    /// `docs/decisions/liga-clig-off-until-gsub-closure.md`) and is
    /// diagnosed.
    #[serde(default)]
    pub opentype_flags: serde_json::Map<String, serde_json::Value>,
}

/// The `textAutoResize` value of a style that carries none: Figma's own REST
/// default (see [`TextStyle::text_auto_resize`]).
fn text_auto_resize_none() -> String {
    "NONE".to_string()
}

/// A present `textAutoResize`, with JSON `null` normalized to `NONE`.
///
/// `serde(default)` covers an absent field; this covers the other spelling of
/// the same absence. Figma writes `null` for an unset optional field
/// (`strokeDashes` and `fontPostScriptName` both arrive that way in the
/// captures), and refusing the whole file over one would be a far worse
/// failure than the mode it stands for.
fn text_auto_resize_or_none<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(text_auto_resize_none))
}

/// One fill or stroke.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paint {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub blend_mode: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Multiplies the paint's alpha. Absent means fully opaque.
    #[serde(default)]
    pub opacity: Option<f32>,
    #[serde(default)]
    pub color: Option<Color>,
    /// Origin, primary-axis end, secondary-axis end — normalized to the
    /// node's box. `dashpaint::Gradient` stores this convention verbatim.
    #[serde(default)]
    pub gradient_handle_positions: Vec<Vector>,
    #[serde(default)]
    pub gradient_stops: Vec<GradientStop>,
    #[serde(default)]
    pub scale_mode: Option<String>,
    /// The content hash of an image asset. The bytes are **not** in this
    /// JSON; the caller resolves the ref (design D1).
    #[serde(default)]
    pub image_ref: Option<String>,
    /// The crop rectangle of a `scaleMode: CROP` image fill: a 2x3 affine in
    /// normalized image space, row-major — `[[a, b, tx], [c, d, ty]]`, the
    /// same six components as `dashpaint::Mat23`. Absent means identity.
    #[serde(default)]
    pub image_transform: Option<[[f32; 3]; 2]>,
    /// The tile magnification of a `scaleMode: TILE` image fill. Absent means
    /// 1.0 — the image tiles at its natural size.
    #[serde(default)]
    pub scaling_factor: Option<f32>,
}

/// An effect. `kind` stays a `String` for the same reason `Node::kind` does:
/// Figma's effect vocabulary is open, and the triage table (not the parser)
/// decides which band each one falls in.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Present on `LAYER_BLUR`. `PROGRESSIVE` moves it from the LATER band to
    /// the REJECT band, so the effect type alone cannot decide the verdict.
    #[serde(default)]
    pub blur_type: Option<String>,
    /// A shadow effect's blend mode. `NORMAL` (the default) lowers; anything
    /// else is an advanced blend mode with no vocabulary, diagnosed like a
    /// paint blend mode (story #45).
    #[serde(default)]
    pub blend_mode: Option<String>,
    /// A shadow effect's color (`DROP_SHADOW`/`INNER_SHADOW`). Absent on a
    /// blur effect, which is triaged rather than lowered.
    #[serde(default)]
    pub color: Option<Color>,
    /// A shadow effect's `{x, y}` offset in pixels. Absent means centered.
    #[serde(default)]
    pub offset: Option<Vector>,
    /// A shadow effect's Gaussian blur radius in pixels — Figma's `radius`.
    /// Absent means a hard-edged shadow.
    #[serde(default)]
    pub radius: Option<f32>,
    /// A shadow effect's spread in pixels — grows the shadow shape. Absent
    /// means zero.
    #[serde(default)]
    pub spread: Option<f32>,
}

/// Non-premultiplied, 0.0–1.0 per channel — the same convention as
/// `dashpaint::Color`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

/// Figma calls the stop's location `position`; `dashpaint` calls it `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}
