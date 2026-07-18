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
    /// Whether this node masks its following siblings (story #44). A
    /// box-shaped outline mask lowers into `Node.mask`; a soft (alpha or
    /// luminance) mask, or a mask whose shape the box vocabulary cannot
    /// express, is refused by name (P4).
    #[serde(default)]
    pub is_mask: Option<bool>,
    /// A mask node's compositing type — Figma's `maskType`: `ALPHA`
    /// (the masking layer's alpha channel), `LUMINANCE` (its brightness),
    /// or `OUTLINE` (its vector geometry). Only a geometry/outline mask
    /// lowers to a hard box clip; alpha and luminance are soft masks the
    /// clip-region vocabulary cannot express (story #44 M6). Absent on a
    /// synthetic node, which lowers as the geometric default.
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
}

/// An `ELLIPSE` node's `arcData` — its pie/ring parameters.
///
/// Angles are in radians. A full ellipse is `startingAngle 0`,
/// `endingAngle 2π`, `innerRadius 0` (a fraction of the radius, `0.0`–`1.0`).
/// Pinned by `lowering-negative-gap.json`, whose ellipses are all full.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcData {
    pub starting_angle: f32,
    pub ending_angle: f32,
    pub inner_radius: f32,
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
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub font_family: String,
    #[serde(default)]
    pub font_post_script_name: Option<String>,
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
    #[serde(default)]
    pub text_auto_resize: Option<String>,
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
    /// which is what the runtime uses. `FONT_SIZE_%`, `PERCENT`, or `PIXELS`
    /// are fixed line heights with no vocabulary.
    #[serde(default)]
    pub line_height_unit: Option<String>,
    /// A hyperlink on the whole text run. No vocabulary, so diagnosed.
    #[serde(default)]
    pub hyperlink: Option<serde_json::Value>,
    /// OpenType feature flags. A non-empty set has no vocabulary
    /// (`liga`/`clig` posture is the runtime's per-run default, not authored,
    /// `docs/decisions/liga-clig-off-until-gsub-closure.md`), so it is
    /// diagnosed.
    #[serde(default)]
    pub opentype_flags: serde_json::Map<String, serde_json::Value>,
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
