//! The Figma REST subset the v0.3 lowering reads.
//!
//! Deliberately partial: only the fields the v0.3 paint vocabulary needs.
//! Every shape here is pinned by `corpus/figma-fixtures/v03-paint.json`, not
//! by a reading of Figma's documentation — the lowering was deferred out of
//! #16 precisely so it would never be written against a guess (P5).
//!
//! Enum-valued fields deserialize into real enums, so an unknown value fails
//! the parse rather than silently lowering to a default. A silent default is
//! the silent drop P4 forbids.

use serde::Deserialize;

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
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub stroke_weight: Option<f32>,
    #[serde(default)]
    pub stroke_align: Option<StrokeAlign>,
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
    /// `BASELINE`. `BASELINE` is v0.8 vocabulary (Q-4) and is refused;
    /// pinned by `lowering-baseline.json`.
    #[serde(default)]
    pub counter_axis_align_items: Option<String>,
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
    /// `ABSOLUTE` takes a child out of its auto-layout parent's flow and
    /// places it by its box. The document has no vocabulary for an
    /// absolutely-placed flex child, and treating one as in-flow would
    /// reflow everything after it — so it is refused (P4). Absent means
    /// `AUTO` (in flow). No capture carries it; the shape is Figma's
    /// documented enum, flagged as capture-unpinned in
    /// `docs/technotes/figma-rest-shapes-the-capture-pinned.md`.
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
    /// Degrees. Figma omits the field entirely when it is zero. `Document` has no
    /// rotation vocabulary, so the walk rejects a non-zero value loudly
    /// rather than lowering it as though the node were axis-aligned (P4).
    #[serde(default)]
    pub rotation: Option<f32>,
    /// Whether this node masks its following siblings. `Document` has no mask
    /// vocabulary, so the walk rejects a mask node loudly rather than
    /// painting it as an ordinary frame (P4).
    #[serde(default)]
    pub is_mask: Option<bool>,
}

/// The stroke's shape family.
///
/// `stroke_type` stays a `String` for the same reason `Node::kind` does: the
/// vocabulary is open (`BASIC`, `DASHED`, and the variable-width types), and
/// only `BASIC` lowers, so an unrecognized value must be a loud error rather
/// than a parse failure of the whole file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexStrokeProperties {
    #[serde(default)]
    pub stroke_type: Option<String>,
}

/// One fill or stroke.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paint {
    #[serde(rename = "type")]
    pub kind: PaintTag,
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
    pub scale_mode: Option<ScaleMode>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaintTag {
    Solid,
    GradientLinear,
    GradientRadial,
    GradientAngular,
    GradientDiamond,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScaleMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

/// An effect. `kind` stays a `String` for the same reason `Node::kind` does:
/// Figma's effect vocabulary is open, and the triage table (not the parser)
/// decides which band each one falls in.
#[derive(Debug, Deserialize)]
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
