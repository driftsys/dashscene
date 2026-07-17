//! The in-memory dashscene document — what a producer lowers *into*, and
//! what the emitter writes *out of*.
//!
//! It is deliberately not a second vocabulary. The paint types are
//! `dashpaint`'s (boundary B), so the one paint vocabulary spans the
//! document, the runtime, and the painter, and a lowering cannot invent a
//! construct no painter can draw.
//!
//! What it adds over `dashpaint` is the *document's* shape: a flattened DFS
//! node list whose array index is the rect-table index (docs/design/dashbuf.md), layout
//! intent (never results — P1), and the pools nodes reference by index.

use dashpaint::{Color, ImageAsset, PaintEntry};

/// A node's authored box. Intent, not a result (P1): under a flex parent the
/// solver owns placement and these offsets are ignored, and the width/height
/// are the datum only an axis sized [`AxisSizing::Fixed`] reads.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Box2D {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A flex container's direction. Mode `None` — the schema's
/// `LayoutMode::None` — is spelled as `Node::container: None`: the absence
/// the schema encodes as an absent table, `Option` encodes as `None`.
///
/// `Wrap` and `Grid` append at v0.8 (story #43): `Wrap` is a horizontal
/// wrapping row (Figma's `layoutWrap` exists for horizontal auto-layout
/// only) and `Grid` places children by cell into the container's track
/// lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Horizontal,
    Vertical,
    Wrap,
    Grid,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// `Baseline` appends at v0.8 (Q-4): counter-axis baseline alignment for a
/// horizontal row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Baseline,
}

/// How one grid track sizes (v0.8, story #43) — the schema's `GridTrack`
/// table as a plain enum, mirroring `dashscene-core`'s. `Fixed` is a
/// document-unit length; `Fraction` is a flexible weight over the free
/// space (Figma's `minmax(0, Nfr)` serialized track).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrack {
    Fixed(f32),
    Fraction(f32),
}

/// How a node sizes itself along one axis. `Fixed` reads the [`Box2D`]
/// width/height as its datum; `Hug` wraps content; `Fill` stretches into
/// the parent's free space.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AxisSizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}

/// Insets named per edge, mirroring the schema's `EdgeInsets` struct.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Container-side flex intent — the schema's `LayoutContainer` table.
/// Present only on a node that lays its children out (mode H/V/Wrap/Grid).
///
/// Not `Copy`: the grid track lists are variable-length. The two v0.8
/// track vectors are empty for a non-grid container, which the emitter
/// writes as the schema's absent field, so a pre-v0.8 document emits
/// byte-identically.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutContainer {
    pub mode: LayoutMode,
    /// The main-axis gap. Never negative in an emitted document under
    /// mode H/V: a negative authored gap is lowered to child margins
    /// before it gets here (`docs/decisions/negative-gap-lowering.md`).
    /// For `Grid` this is the column gap (Figma's `gridColumnGap`); for
    /// `Wrap` the between-chips gap (`itemSpacing`).
    pub gap: f32,
    pub padding: EdgeInsets,
    pub main_align: MainAxisAlign,
    pub cross_align: CrossAxisAlign,
    /// The cross-axis gap (v0.8, story #43): the spacing between wrap
    /// lines (Figma's `counterAxisSpacing`) and between grid rows
    /// (`gridRowGap`). `None` = follows `gap`, preserving the v0.2
    /// both-axes mapping for every H/V document.
    pub cross_gap: Option<f32>,
    /// The grid row tracks, top to bottom (v0.8, story #43). Empty for a
    /// non-grid container.
    pub grid_rows: Vec<GridTrack>,
    /// The grid column tracks, left to right (v0.8, story #43). Empty for
    /// a non-grid container.
    pub grid_columns: Vec<GridTrack>,
}

/// Child-side flex intent — the schema's `LayoutConstraints` table. `None`
/// on [`Node`] means fully default: `Fixed` sizing, unconstrained min/max,
/// zero margin, auto grid placement, unit spans.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConstraints {
    pub sizing_h: AxisSizing,
    pub sizing_v: AxisSizing,
    /// `None` = unconstrained — absence of intent, not a sentinel (P1).
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    /// Outer margin in the parent's flex flow. Negative values express
    /// overlap — the negative-gap lowering's target.
    pub margin: EdgeInsets,
    /// Grid placement (v0.8, story #43), meaningful under a `Grid`
    /// parent: the 0-based anchor cell (Figma's `gridRowAnchorIndex` /
    /// `gridColumnAnchorIndex`). `None` = auto-placement in document
    /// order. An anchor of `Some(0)` is the first cell, distinct from
    /// absent.
    pub grid_row: Option<u16>,
    pub grid_column: Option<u16>,
    /// The number of tracks the child spans (Figma's `gridRowSpan` /
    /// `gridColumnSpan`). The schema default is 1.
    pub grid_row_span: u16,
    pub grid_column_span: u16,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        // Hand-written so the spans default to 1 (the schema default),
        // not the 0 a derive would give — otherwise a non-grid child's
        // constraints would never equal the default and would emit a
        // needless table (R7).
        Self {
            sizing_h: AxisSizing::default(),
            sizing_v: AxisSizing::default(),
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            margin: EdgeInsets::default(),
            grid_row: None,
            grid_column: None,
            grid_row_span: 1,
            grid_column_span: 1,
        }
    }
}

/// A text node's authored style — the schema's `TextStyle` table (story
/// #26). Intent only (P1): family, em size, CSS-scale weight, and fill
/// color. Never glyph data, line breaks, or resolved metrics — shaping and
/// placement are the runtime's (`docs/design/typeset-latin.md`). The
/// vocabulary carries exactly these four axes; a non-default alignment, line
/// height, or letter spacing has nothing to lower into and is a named
/// diagnostic at the walk (P4), never carried here as though it rendered.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: String,
    /// Em size in document units.
    pub size: f32,
    /// CSS-scale weight, 100 to 900. Lowered verbatim from Figma's
    /// `fontWeight`; the range check is the validator's (`#41`/`#129`).
    pub weight: u16,
    /// The fill color. Always set by the lowering (a text node with no solid
    /// fill is refused at the walk), so an emitted style always carries one.
    pub color: Color,
}

/// One node of the document. `parent` is an index into [`Document::nodes`], and
/// the array is in DFS order, so a parent's index is always lower than its
/// children's.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: Option<String>,
    pub parent: Option<u32>,
    pub box2d: Box2D,
    /// The node's style. `None` draws nothing (a layout-only container).
    pub paint: Option<Paint>,
    /// Container-side flex intent. `None` = mode `None` (a passthrough:
    /// children place by their authored offsets).
    pub container: Option<LayoutContainer>,
    /// Child-side flex intent. `None` = fully default constraints.
    pub constraints: Option<LayoutConstraints>,
    /// The node's authored characters. `None` for a non-text node. The
    /// emitter interns it into `Document.strings` and points `Node.text` at
    /// it (the same pooling the paint entries get).
    pub text: Option<String>,
    /// The node's text style. `None` for a non-text node. Interned into
    /// `Document.text_styles`. A text node carries both `text` and
    /// `text_style`, or neither.
    pub text_style: Option<TextStyle>,
    /// Node/group alpha in `[0, 1]` (story #44,
    /// `docs/decisions/masks-and-group-opacity.md`). `1.0` is fully
    /// opaque, the schema default the emitter omits.
    pub opacity: f32,
    /// Whether this node masks the siblings that follow it. Default
    /// `false`.
    pub mask: bool,
    /// Whether the node is drawn and takes part in layout (Figma's
    /// `visible`). Default `true`; `false` lowers to Taffy Display::None
    /// (debt #143).
    pub visible: bool,
}

impl Default for Node {
    fn default() -> Self {
        // Hand-written so `opacity` defaults to fully opaque and `visible`
        // to shown, matching the schema defaults rather than the numeric
        // and boolean zeroes a derive would give.
        Self {
            name: None,
            parent: None,
            box2d: Box2D::default(),
            paint: None,
            container: None,
            constraints: None,
            text: None,
            text_style: None,
            opacity: 1.0,
            mask: false,
            visible: true,
        }
    }
}

/// A node's style: the boundary-B paint entry, plus the clip intent the
/// document pools alongside it.
///
/// Clip travels with the paint entry because the schema pools it there
/// (`Paint.clip`). The arena instead carries clip as *node* intent
/// (`Prop::Clip`, issue #97) — so two nodes sharing a style but differing in
/// clip need two pool entries here, which is why the pool key below includes
/// it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paint {
    pub entry: PaintEntry,
    pub clip: bool,
}

/// One scalar prop slot a binding targets — the schema's
/// `BindingChannel` as a plain type (story #167).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingChannel {
    X,
    Y,
    Width,
    Height,
    Gap,
    FillR,
    FillG,
    FillB,
    FillA,
    /// Node/group opacity (story #44, debt #253).
    Opacity,
}

/// A binding's transform — the schema's `BindingTransform` union as a
/// plain type, plus `Custom`: `dashlang`'s closure escape hatch, carried
/// as its opaque closure id. A `Custom` transform cannot serialize (the
/// closure lives in a producer-side table), so a document that carries
/// one is refused by name at [`crate::compile`] (`binding.custom-transform`),
/// never emitted approximately (P4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BindingTransform {
    Identity,
    Scale(f32),
    MapRange {
        in_lo: f32,
        in_hi: f32,
        out_lo: f32,
        out_hi: f32,
    },
    Clamp {
        lo: f32,
        hi: f32,
    },
    Custom(u32),
}

/// One declared signal: the runtime lookup name and the initial value
/// its bindings seed from (story #167). The Figma producer names every
/// signal (a variable's mode-qualified name); the name is what a runtime
/// looks the signal up by after loading.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalDecl {
    pub name: String,
    pub initial: f32,
}

/// One binding row: `signal` (index into [`Document::signals`]) drives
/// `channel` on `node` (index into [`Document::nodes`]) through
/// `transform`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Binding {
    pub signal: u32,
    pub node: u32,
    pub channel: BindingChannel,
    pub transform: BindingTransform,
}

/// One dashscene document, ready to emit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    /// Flattened DFS node tree: array index = rect-table index (docs/design/dashbuf.md).
    pub nodes: Vec<Node>,
    /// The image assets an image fill references by index.
    pub images: Vec<ImageAsset>,
    /// The signal declarations bindings reference by index (story #167).
    pub signals: Vec<SignalDecl>,
    /// The binding rows joining signals to node channels (story #167).
    pub bindings: Vec<Binding>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a node and returns its index. The caller appends in DFS
    /// order; `emit` does not reorder.
    pub fn push(&mut self, node: Node) -> u32 {
        let index = u32::try_from(self.nodes.len()).expect("document exceeds u32::MAX nodes");
        self.nodes.push(node);
        index
    }

    /// Appends an image asset and returns its index.
    pub fn push_image(&mut self, asset: ImageAsset) -> u32 {
        let index = u32::try_from(self.images.len()).expect("document exceeds u32::MAX images");
        self.images.push(asset);
        index
    }
}
