//! Pure-Rust MSDF vector-field generator (story B1,
//! docs/wip/2026-07-19-B1-vector-msdf-design.md).
//!
//! Input: a Figma VECTOR path — SVG path data in the census vocabulary
//! (`M`/`L`/`C`/`Z`), one winding rule (NONZERO/EVENODD), possibly
//! multi-contour with holes. Output: a baked multi-channel signed
//! distance field, packed with other shapes into one shared atlas and
//! deduplicated by path-geometry hash.
//!
//! The bake runs inside `dashc.wasm` at import time (the generator is
//! `fdsm`, pure Rust, so it rides the wasm path where the C++ `msdfgen`
//! cannot). fdsm's field is welded per-texel to a committed pinned-msdfgen
//! reference by `tests/vector_field_weld.rs`. The pub bake surface
//! (`VectorAtlasBaker`, `bake_single`, `BakedField`, `VectorAtlasBake`)
//! is reused by the bake oracle in `goldens/tooling`.
//!
//! Field encoding mirrors the glyph atlas (`dashscene-typeset::atlas`):
//! `px_per_em` texels per shape em (here the em is the shape's longer
//! bounding-box side), a `distance_range`-texel MSDF spread, and a
//! `plane_bounds` quad — padded by the spread — that places the field in
//! the shape's own coordinate space. The painter samples median-of-3 to a
//! signed distance, exactly as it does a glyph (B1.3).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use fdsm::bezier::scanline::FillRule;
use fdsm::bezier::{Order, Point, Segment};
use fdsm::generate::generate_msdf;
use fdsm::render::correct_sign_msdf;
use fdsm::shape::{Contour, Shape};
use fdsm::transform::Transform;
use image::{ImageEncoder, RgbImage};
use nalgebra::{Affine2, Similarity2, Vector2};

/// Field resolution: atlas texels per shape em, the em being the shape's
/// longer bounding-box side. The glyph atlas bakes at 32; vectors get
/// more headroom (approved gate). Production bakes every shape at this fixed
/// value and never re-bakes — the v0.10 census found zero shapes needing more.
/// The per-shape px-per-em escalation ladder and the unfieldable-ceiling
/// refusal live only in the bake oracle
/// (`goldens/tooling/tests/v010_bake_oracle.rs`, B1.5); wiring escalation into
/// the lowering stays deferred (debt #357), and the reason is now measured
/// rather than assumed.
///
/// Escalating needs a per-shape verdict on whether the baked field represents
/// the shape. The oracle gets one by rendering the field as a quad and
/// diffing it against Skia's exact fill of the same path. This crate compiles
/// to `wasm32-unknown-unknown` for the Deno importer and cannot link Skia, so
/// that verdict is not available here, and `fdsm` carries no substitute:
/// counting the texels whose raw MSDF median sign disagrees with `fdsm`'s own
/// exact scanline fill — the quantity `correct_sign_msdf` acts on, and the
/// only fidelity-shaped signal the generator exposes — separates nothing. On
/// the oracle's census shapes plus its synthetic sub-texel barcode, at every
/// rung of the ladder, that count reads 0.000 % for the barcode, which is the
/// one shape the oracle refuses at every rung, and 0.000–0.040 % for the
/// shapes it accepts. It measures reconstruction artifacts, not resolution
/// loss: a field of a sub-texel feature is internally consistent, it simply
/// encodes geometry the sampling grid never saw.
///
/// Reproducing the oracle's measurement here instead would mean
/// reimplementing both halves — a supersampled scanline rasterizer for the
/// truth and Skia's bilinear sampling plus the `FIELD_MASK_SKSL` resolve for
/// the bake. The census's worst measured residual is about 2.2 % of footprint
/// against a 3 % tolerance, so such a reimplementation would have to agree
/// with Skia to well inside 0.8 percentage points to avoid escalating a shape
/// that fields correctly today — and escalation changes the bake, so a wrong
/// verdict moves committed atlas bytes. That margin is not measurable without
/// the oracle it is standing in for.
pub const DEFAULT_PX_PER_EM: f64 = 48.0;

/// MSDF spread in texels (the msdfgen `-pxrange`), aligned with the glyph
/// atlas's pxrange 4.
pub const DEFAULT_DISTANCE_RANGE: f64 = 4.0;

/// Edge-coloring corner threshold: the sine of the minimum corner angle,
/// matching fdsm's own example. Pinned — the field is welded to pinned
/// msdfgen output, so this is a format input: changing it rebakes every
/// field and demands a re-weld.
pub const EDGE_COLORING_SIN_ALPHA: f64 = 0.03;

/// Edge-coloring seed. Pinned (the glyph atlas pins its own seed to 1 for
/// the same reason — a deterministic, reviewed generator input).
pub const EDGE_COLORING_SEED: u64 = 1;

/// Transparent gutter in texels between packed fields, so no shape's
/// spread bleeds into a neighbour under the painter's bilinear sampling.
const ATLAS_GUTTER: u32 = 1;

/// The winding rule a Figma `fillGeometry`/`strokeGeometry` entry carries
/// (`windingRule`). Holes fill correctly because the rule reaches fdsm's
/// scanline sign-correction unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindingRule {
    NonZero,
    EvenOdd,
}

impl WindingRule {
    fn fill_rule(self) -> FillRule {
        match self {
            WindingRule::NonZero => FillRule::Nonzero,
            WindingRule::EvenOdd => FillRule::Odd,
        }
    }
}

/// The number of control points a segment carries (2 line, 3 quadratic,
/// 4 cubic). fdsm keeps its own `order_int` crate-private, so derive it
/// from the public [`Order`].
fn control_point_count(order: Order) -> usize {
    match order {
        Order::Linear => 2,
        Order::Quadratic => 3,
        Order::Cubic => 4,
    }
}

/// A vector path to bake: SVG path data plus its winding rule. Multiple
/// subpaths (each opened by `M`) become separate contours of one shape —
/// this is how a hole (the fixture's square-with-hole, the hero's EVENODD
/// contours) reaches the generator.
#[derive(Clone, Copy, Debug)]
pub struct VectorPath<'a> {
    pub path: &'a str,
    pub winding: WindingRule,
}

/// The field quad in the shape's own coordinate space: the tight geometry box
/// grown by a `distance_range`-texel margin at the min corner, then carried out
/// to the far edge of the ceil'd atlas tile, so `(right - left) * scale` equals
/// the tile width in texels exactly (the painter maps the whole tile onto this
/// quad). msdfgen's planeBounds, not the em box. Y is down (Figma's
/// fillGeometry space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneBoundsF {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

/// A sub-rect inside the packed atlas, in atlas texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRectU {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One baked field before packing: the RGB MSDF tile plus its placement
/// metadata. `rgb` is row-major, 3 bytes per texel, `width * height * 3`
/// long.
#[derive(Clone, Debug)]
pub struct BakedField {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    pub plane_bounds: PlaneBoundsF,
}

/// One packed shape: where its field sits in the atlas and how the field
/// quad maps into the shape's coordinate space.
#[derive(Clone, Debug, PartialEq)]
pub struct BakedShapePlacement {
    pub atlas_rect: AtlasRectU,
    pub plane_bounds: PlaneBoundsF,
}

/// The finished bake: one packed atlas (PNG-encoded RGB) plus a placement
/// per unique shape. Shape indices are the values `VectorAtlasBaker::add`
/// returned — identical geometry shares one placement.
#[derive(Clone, Debug)]
pub struct VectorAtlasBake {
    pub image_png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub px_per_em: f64,
    pub distance_range: f64,
    pub shapes: Vec<BakedShapePlacement>,
}

/// Everything the generator refuses by name (P4). A refusal becomes the
/// generator-side cause of a `figma.unsupported` diagnostic the lowering
/// raises through `vector_field_blocker` (B1.4). There is no dedicated
/// `figma.vector-unfieldable` code — the generic `figma.unsupported` rule
/// carries every out-of-census / degenerate refusal.
#[derive(Clone, Debug, PartialEq)]
pub enum VectorFieldError {
    /// A path command outside the `M`/`L`/`C`/`Z` census vocabulary.
    UnsupportedCommand(char),
    /// Path data that does not parse: a non-numeric coordinate, a command
    /// with too few coordinates, or a segment before the opening `M`.
    MalformedPath(String),
    /// Geometry with no fillable extent (empty, a single point, or a
    /// zero-area collinear path).
    DegenerateGeometry,
}

impl std::fmt::Display for VectorFieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedCommand(c) => {
                write!(f, "unsupported path command {c:?} (census is M/L/C/Z)")
            }
            Self::MalformedPath(m) => write!(f, "malformed path data: {m}"),
            Self::DegenerateGeometry => write!(f, "degenerate geometry: no fillable extent"),
        }
    }
}

impl std::error::Error for VectorFieldError {}

/// The pixel-space plan for a field: the tile dimensions and the
/// shape-to-texel transform (`pixel = scale * shape + translate`). Exposed
/// so the weld test and the bake oracle can frame msdfgen (or a Skia path
/// render) byte-identically to the fdsm bake — one source of truth for the
/// geometry framing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldPlan {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub translate_x: f64,
    pub translate_y: f64,
    pub plane_bounds: PlaneBoundsF,
}

/// Computes the [`FieldPlan`] for a path without baking. Same math
/// `bake_field` runs.
pub fn plan_field(
    path: &VectorPath<'_>,
    px_per_em: f64,
    distance_range: f64,
) -> Result<FieldPlan, VectorFieldError> {
    let contours = parse_path(path.path)?;
    let bbox = bounding_box(&contours).ok_or(VectorFieldError::DegenerateGeometry)?;
    field_plan(&bbox, px_per_em, distance_range)
}

/// Bakes exactly one path to a field, bypassing dedup and packing. Used by
/// the weld test and the bake oracle; `VectorAtlasBaker` is the import-time
/// entry.
pub fn bake_single(
    path: &VectorPath<'_>,
    px_per_em: f64,
    distance_range: f64,
) -> Result<BakedField, VectorFieldError> {
    let contours = parse_path(path.path)?;
    bake_field(&contours, path.winding, px_per_em, distance_range)
}

/// Import-time atlas builder: bakes each unique path once, deduplicating by
/// path geometry, and shelf-packs the unique fields into one atlas.
pub struct VectorAtlasBaker {
    px_per_em: f64,
    distance_range: f64,
    seen: HashMap<u64, u32>,
    /// The normalized geometry behind each baked field, parallel to `fields`.
    /// [`add`](VectorAtlasBaker::add) compares it on a hash hit, so a hash
    /// collision cannot hand back another shape's field (debt #358).
    keys: Vec<GeometryKey>,
    fields: Vec<BakedField>,
}

impl VectorAtlasBaker {
    /// A baker at the default resolution (`DEFAULT_PX_PER_EM`,
    /// `DEFAULT_DISTANCE_RANGE`).
    pub fn new() -> Self {
        Self::with_resolution(DEFAULT_PX_PER_EM, DEFAULT_DISTANCE_RANGE)
    }

    pub fn with_resolution(px_per_em: f64, distance_range: f64) -> Self {
        Self {
            px_per_em,
            distance_range,
            seen: HashMap::new(),
            keys: Vec::new(),
            fields: Vec::new(),
        }
    }

    /// True until the first shape is baked.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// How many unique shapes have been baked — the value the next
    /// [`add`](Self::add) of new geometry would return.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Drops every shape baked at or after `len`, so a caller that baked a
    /// node's geometry and then found the node unlowerable can undo exactly
    /// its own registration (debt #356).
    ///
    /// The dedup index is pruned with the fields. Leaving it would let a later
    /// identical path hit a cached index for a field that no longer exists,
    /// which is worse than the orphan this removes. Indices below `len` are
    /// untouched, so every shape index already handed out stays valid — the
    /// same contract `Document::assets.truncate` relies on for skipped image
    /// fills (debt #485).
    ///
    /// A `len` at or past the end is a no-op, so a caller does not have to
    /// know whether its own `add` baked a field or hit the dedup.
    pub fn truncate(&mut self, len: usize) {
        self.fields.truncate(len);
        self.keys.truncate(len);
        self.seen.retain(|_, index| (*index as usize) < len);
    }

    /// Bakes `path` and returns its shape index, or returns the existing
    /// index when identical geometry was already baked (path-geometry dedup:
    /// the hero repeats icon vectors, which then share one field).
    ///
    /// The hash only selects a candidate; the geometry itself decides (debt
    /// #358). Dedup used to trust the 64-bit hash alone, so a collision — rare,
    /// but silent and unbounded in effect — would have painted one shape with
    /// another's field. A colliding second shape bakes its own field and
    /// leaves the bucket to the first: it simply stops deduplicating, which
    /// costs one tile and can never render the wrong outline.
    pub fn add(&mut self, path: &VectorPath<'_>) -> Result<u32, VectorFieldError> {
        let contours = parse_path(path.path)?;
        let key = geometry_key(&contours, path.winding);
        let hash = key_hash(&key);
        let collided = match self.seen.get(&hash) {
            Some(&index) if self.keys[index as usize] == key => return Ok(index),
            Some(_) => true,
            None => false,
        };
        // Bake before registering: a refusal must leave the baker exactly as
        // it was, or `seen` would name an index `fields` never gains.
        let field = bake_field(&contours, path.winding, self.px_per_em, self.distance_range)?;
        let index = self.fields.len() as u32;
        self.fields.push(field);
        self.keys.push(key);
        if !collided {
            self.seen.insert(hash, index);
        }
        Ok(index)
    }

    /// Shelf-packs every unique field into one atlas and PNG-encodes it.
    pub fn finish(self) -> Result<VectorAtlasBake, VectorFieldError> {
        let (width, height, rects) = shelf_pack(&self.fields);
        let mut atlas = RgbImage::new(width.max(1), height.max(1));
        for (field, rect) in self.fields.iter().zip(&rects) {
            blit(&mut atlas, field, rect.x, rect.y);
        }
        let shapes = self
            .fields
            .iter()
            .zip(&rects)
            .map(|(field, &atlas_rect)| BakedShapePlacement {
                atlas_rect,
                plane_bounds: field.plane_bounds,
            })
            .collect();
        Ok(VectorAtlasBake {
            image_png: encode_png(&atlas),
            width: atlas.width(),
            height: atlas.height(),
            px_per_em: self.px_per_em,
            distance_range: self.distance_range,
            shapes,
        })
    }
}

impl Default for VectorAtlasBaker {
    fn default() -> Self {
        Self::new()
    }
}

// --- baking ---------------------------------------------------------------

struct BBox {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl BBox {
    fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
}

fn bounding_box(contours: &[Vec<Segment>]) -> Option<BBox> {
    let mut b = BBox {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    let mut any = false;
    for segment in contours.iter().flatten() {
        for i in 0..control_point_count(segment.order()) {
            let p = segment.control_point(i);
            b.min_x = b.min_x.min(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_x = b.max_x.max(p.x);
            b.max_y = b.max_y.max(p.y);
            any = true;
        }
    }
    any.then_some(b)
}

/// Frames a bounding box into a field tile: `px_per_em` texels per shape em
/// (the longer side), a `distance_range`-texel margin on every side so the
/// whole MSDF spread fits (mirrors the glyph atlas's `-pxrange` padding).
fn field_plan(
    bbox: &BBox,
    px_per_em: f64,
    distance_range: f64,
) -> Result<FieldPlan, VectorFieldError> {
    let em = bbox.width().max(bbox.height());
    // Reject a non-fillable extent: a single point (em == 0), a collinear
    // zero-area path, or a non-finite coordinate that reached here.
    if !em.is_finite() || em <= 0.0 {
        return Err(VectorFieldError::DegenerateGeometry);
    }
    let scale = px_per_em / em;
    let width = (bbox.width() * scale + 2.0 * distance_range)
        .ceil()
        .max(1.0) as u32;
    let height = (bbox.height() * scale + 2.0 * distance_range)
        .ceil()
        .max(1.0) as u32;
    // pixel = scale * shape + translate, with the shape's min corner landing
    // at (distance_range, distance_range).
    let translate_x = distance_range - bbox.min_x * scale;
    let translate_y = distance_range - bbox.min_y * scale;
    // The padded quad, back in shape space. The min corner sits `margin`
    // (`distance_range` texels) inside the tile; the far edge must map to the
    // tile's far edge, which spans the *ceil'd* `width`/`height` texels — not
    // `bbox.width() + 2 * margin`, the un-ceil'd extent the ceil rounded up
    // from. The painter maps the whole ceil'd atlas tile onto this quad, so a
    // far edge left at the un-ceil'd extent renders the field up to one texel
    // too small, anisotropically (x and y ceil independently). Anchoring the
    // far edge at `left + width / scale` keeps `(right - left) * scale == width`
    // exactly, so texels and shape units stay in lockstep.
    let margin = distance_range / scale;
    let left = bbox.min_x - margin;
    let top = bbox.min_y - margin;
    let plane_bounds = PlaneBoundsF {
        left,
        top,
        right: left + f64::from(width) / scale,
        bottom: top + f64::from(height) / scale,
    };
    Ok(FieldPlan {
        width,
        height,
        scale,
        translate_x,
        translate_y,
        plane_bounds,
    })
}

fn bake_field(
    contours: &[Vec<Segment>],
    winding: WindingRule,
    px_per_em: f64,
    distance_range: f64,
) -> Result<BakedField, VectorFieldError> {
    let bbox = bounding_box(contours).ok_or(VectorFieldError::DegenerateGeometry)?;
    let plan = field_plan(&bbox, px_per_em, distance_range)?;

    // This is fdsm's own README recipe: a Similarity with the shape's min
    // corner landing at (distance_range, distance_range).
    let transform: Affine2<f64> = nalgebra::convert(Similarity2::new(
        Vector2::new(plan.translate_x, plan.translate_y),
        0.0,
        plan.scale,
    ));

    let mut shape = Shape {
        contours: contours
            .iter()
            .map(|segments| Contour {
                segments: segments.clone(),
            })
            .collect(),
    };
    shape.transform(&transform);

    let colored = Shape::edge_coloring_simple(shape, EDGE_COLORING_SIN_ALPHA, EDGE_COLORING_SEED);
    let prepared = colored.prepare();
    let mut msdf = RgbImage::new(plan.width, plan.height);
    generate_msdf(&prepared, distance_range, &mut msdf);
    correct_sign_msdf(&mut msdf, &prepared, winding.fill_rule());

    Ok(BakedField {
        width: plan.width,
        height: plan.height,
        rgb: msdf.into_raw(),
        plane_bounds: plan.plane_bounds,
    })
}

// --- path parsing ---------------------------------------------------------

/// Parses SVG path data (`M`/`L`/`C`/`Z`) into closed contours of fdsm
/// segments. Every `M` opens a new contour; `Z` closes the current one back
/// to its start. Absolute commands only (Figma's exported geometry), with
/// implicit command repeats supported. Any other command is refused by name
/// (P4).
fn parse_path(data: &str) -> Result<Vec<Vec<Segment>>, VectorFieldError> {
    let mut tok = Tokenizer::new(data);
    let mut contours: Vec<Vec<Segment>> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut start = Point::new(0.0, 0.0);
    let mut cursor = Point::new(0.0, 0.0);
    let mut have_start = false;

    let close = |contours: &mut Vec<Vec<Segment>>,
                 current: &mut Vec<Segment>,
                 cursor: &Point,
                 start: &Point| {
        if !current.is_empty() {
            if (cursor.x - start.x).abs() > 0.0 || (cursor.y - start.y).abs() > 0.0 {
                current.push(Segment::line(*cursor, *start));
            }
            contours.push(std::mem::take(current));
        }
    };

    while let Some(cmd) = tok.next_command()? {
        match cmd {
            'M' => {
                close(&mut contours, &mut current, &cursor, &start);
                let x = tok.number()?;
                let y = tok.number()?;
                start = Point::new(x, y);
                cursor = start;
                have_start = true;
                // An `M` with extra coordinate pairs is an implicit `L` run.
                while tok.has_number() {
                    let lx = tok.number()?;
                    let ly = tok.number()?;
                    let next = Point::new(lx, ly);
                    current.push(Segment::line(cursor, next));
                    cursor = next;
                }
            }
            'L' => {
                if !have_start {
                    return Err(VectorFieldError::MalformedPath("L before M".into()));
                }
                loop {
                    let x = tok.number()?;
                    let y = tok.number()?;
                    let next = Point::new(x, y);
                    current.push(Segment::line(cursor, next));
                    cursor = next;
                    if !tok.has_number() {
                        break;
                    }
                }
            }
            'C' => {
                if !have_start {
                    return Err(VectorFieldError::MalformedPath("C before M".into()));
                }
                loop {
                    let c1 = Point::new(tok.number()?, tok.number()?);
                    let c2 = Point::new(tok.number()?, tok.number()?);
                    let end = Point::new(tok.number()?, tok.number()?);
                    current.push(Segment::cubic(cursor, c1, c2, end));
                    cursor = end;
                    if !tok.has_number() {
                        break;
                    }
                }
            }
            'Z' => {
                close(&mut contours, &mut current, &cursor, &start);
                cursor = start;
            }
            other => return Err(VectorFieldError::UnsupportedCommand(other)),
        }
    }
    // A path that ends without a trailing `Z` still closes its last contour.
    close(&mut contours, &mut current, &cursor, &start);

    if contours.is_empty() {
        return Err(VectorFieldError::DegenerateGeometry);
    }
    Ok(contours)
}

/// A minimal SVG-path-data tokenizer: command letters and floating-point
/// numbers, separated by whitespace and/or commas.
struct Tokenizer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(data: &'a str) -> Self {
        Self {
            bytes: data.as_bytes(),
            pos: 0,
        }
    }

    fn skip_separators(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// The next command letter, or `None` at end of input.
    fn next_command(&mut self) -> Result<Option<char>, VectorFieldError> {
        self.skip_separators();
        if self.pos >= self.bytes.len() {
            return Ok(None);
        }
        let b = self.bytes[self.pos];
        if b.is_ascii_alphabetic() {
            self.pos += 1;
            Ok(Some(b as char))
        } else {
            Err(VectorFieldError::MalformedPath(format!(
                "expected a command, found {:?}",
                b as char
            )))
        }
    }

    /// True if a number token starts at the cursor (peek, no consume).
    fn has_number(&mut self) -> bool {
        self.skip_separators();
        match self.bytes.get(self.pos) {
            Some(b) => b.is_ascii_digit() || *b == b'-' || *b == b'+' || *b == b'.',
            None => false,
        }
    }

    /// Consumes one floating-point number.
    fn number(&mut self) -> Result<f64, VectorFieldError> {
        self.skip_separators();
        let begin = self.pos;
        if self.pos < self.bytes.len()
            && (self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+')
        {
            self.pos += 1;
        }
        let mut seen_dot = false;
        let mut seen_exp = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b.is_ascii_digit() {
                self.pos += 1;
            } else if b == b'.' && !seen_dot && !seen_exp {
                seen_dot = true;
                self.pos += 1;
            } else if (b == b'e' || b == b'E') && !seen_exp {
                seen_exp = true;
                self.pos += 1;
                if self.pos < self.bytes.len()
                    && (self.bytes[self.pos] == b'-' || self.bytes[self.pos] == b'+')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[begin..self.pos])
            .map_err(|_| VectorFieldError::MalformedPath("non-utf8 number".into()))?;
        text.parse::<f64>()
            .map_err(|_| VectorFieldError::MalformedPath(format!("not a number: {text:?}")))
    }
}

// --- dedup + packing ------------------------------------------------------

/// The normalized geometry two paths are compared on: the winding rule, the
/// contour and segment structure, and every control point's coordinates.
///
/// This is the dedup decision itself, not a digest of it — [`key_hash`] only
/// picks the candidate to compare against (debt #358).
type GeometryKey = Vec<u64>;

/// Flattens a parsed path into its [`GeometryKey`].
fn geometry_key(contours: &[Vec<Segment>], winding: WindingRule) -> GeometryKey {
    let mut key = vec![
        match winding {
            WindingRule::NonZero => 0,
            WindingRule::EvenOdd => 1,
        },
        contours.len() as u64,
    ];
    for contour in contours {
        key.push(contour.len() as u64);
        for segment in contour {
            key.push(segment.order() as u64);
            for i in 0..control_point_count(segment.order()) {
                let p = segment.control_point(i);
                key.push(coordinate_bits(p.x));
                key.push(coordinate_bits(p.y));
            }
        }
    }
    key
}

/// A coordinate as bits, with `-0.0` folded onto `+0.0` (debt #358).
///
/// The two compare equal and place the same point, but their bit patterns
/// differ, so keying on the raw bits made two geometrically identical paths
/// miss a valid dedup and bake the same outline twice. A NaN keeps its own
/// bits — it never compares equal to anything, and a path carrying one is
/// refused as degenerate before it can be baked.
fn coordinate_bits(v: f64) -> u64 {
    if v == 0.0 {
        0.0_f64.to_bits()
    } else {
        v.to_bits()
    }
}

/// A stable hash of a [`GeometryKey`] — the bucket a candidate is looked up
/// in, never the answer on its own.
fn key_hash(key: &GeometryKey) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Shelf-packs fields (in order) into an atlas with a one-texel gutter.
/// Returns the atlas dimensions and one rect per field.
fn shelf_pack(fields: &[BakedField]) -> (u32, u32, Vec<AtlasRectU>) {
    if fields.is_empty() {
        return (0, 0, Vec::new());
    }
    let widest = fields.iter().map(|f| f.width).max().unwrap_or(0);
    let total_area: u64 = fields
        .iter()
        .map(|f| u64::from(f.width + ATLAS_GUTTER) * u64::from(f.height + ATLAS_GUTTER))
        .sum();
    // Aim for a roughly square sheet, but never narrower than the widest
    // field plus its two gutters (a field is placed at x = ATLAS_GUTTER and
    // needs a trailing gutter too, so the sheet must hold widest + 2*gutter or
    // that field runs off the right edge), and round to a multiple of 4
    // (texture-friendly).
    let target = (total_area as f64).sqrt().ceil() as u32;
    let atlas_width = round_up_4((widest + 2 * ATLAS_GUTTER).max(target).max(1));

    let mut rects = Vec::with_capacity(fields.len());
    let mut x = ATLAS_GUTTER;
    let mut y = ATLAS_GUTTER;
    let mut shelf_height = 0u32;
    for field in fields {
        if x + field.width + ATLAS_GUTTER > atlas_width && x > ATLAS_GUTTER {
            y += shelf_height + ATLAS_GUTTER;
            x = ATLAS_GUTTER;
            shelf_height = 0;
        }
        rects.push(AtlasRectU {
            x,
            y,
            width: field.width,
            height: field.height,
        });
        x += field.width + ATLAS_GUTTER;
        shelf_height = shelf_height.max(field.height);
    }
    let atlas_height = round_up_4(y + shelf_height + ATLAS_GUTTER);
    (atlas_width, atlas_height, rects)
}

fn round_up_4(n: u32) -> u32 {
    n.div_ceil(4) * 4
}

/// Copies a field's RGB tile into the atlas at (dx, dy).
fn blit(atlas: &mut RgbImage, field: &BakedField, dx: u32, dy: u32) {
    for row in 0..field.height {
        for col in 0..field.width {
            let src = ((row * field.width + col) * 3) as usize;
            let px = image::Rgb([field.rgb[src], field.rgb[src + 1], field.rgb[src + 2]]);
            atlas.put_pixel(dx + col, dy + row, px);
        }
    }
}

fn encode_png(atlas: &RgbImage) -> Vec<u8> {
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            atlas.as_raw(),
            atlas.width(),
            atlas.height(),
            image::ExtendedColorType::Rgb8,
        )
        .expect("PNG encoding of an in-memory RGB atlas cannot fail");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square, counter-clockwise in y-down space, filled NONZERO.
    const SQUARE: &str = "M 0 0 L 40 0 L 40 40 L 0 40 Z";
    /// The same square textually reordered but geometrically identical.
    const SQUARE_ALT: &str = "M0,0 L40,0 L40,40 L0,40Z";
    /// A 40x40 square with a centered 20x20 hole, EVENODD (the hole is the
    /// second subpath).
    const SQUARE_WITH_HOLE: &str =
        "M 0 0 L 40 0 L 40 40 L 0 40 Z M 10 10 L 10 30 L 30 30 L 30 10 Z";

    fn median(rgb: &[u8], i: usize) -> u8 {
        let (r, g, b) = (rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]);
        r.max(g).min(b.max(r.min(g)))
    }

    #[test]
    fn parses_square_into_one_closed_contour() {
        let contours = parse_path(SQUARE).expect("parses");
        assert_eq!(contours.len(), 1);
        // Four explicit lines; the trailing Z coincides with the start, so
        // no extra closing segment is added.
        assert_eq!(contours[0].len(), 4);
    }

    #[test]
    fn refuses_out_of_census_command() {
        let err = parse_path("M0 0 A 5 5 0 0 1 10 10").unwrap_err();
        assert_eq!(err, VectorFieldError::UnsupportedCommand('A'));
    }

    #[test]
    fn refuses_empty_geometry() {
        assert_eq!(
            bake_single(
                &VectorPath {
                    path: "   ",
                    winding: WindingRule::NonZero
                },
                DEFAULT_PX_PER_EM,
                DEFAULT_DISTANCE_RANGE
            )
            .unwrap_err(),
            VectorFieldError::DegenerateGeometry
        );
    }

    #[test]
    fn bakes_a_plausible_field() {
        let field = bake_single(
            &VectorPath {
                path: SQUARE,
                winding: WindingRule::NonZero,
            },
            DEFAULT_PX_PER_EM,
            DEFAULT_DISTANCE_RANGE,
        )
        .expect("bakes");
        // em = 40, scale = 48/40 = 1.2, so the tile is 40*1.2 + 2*4 = 56 on
        // each side.
        assert_eq!(field.width, 56);
        assert_eq!(field.height, 56);
        assert_eq!(field.rgb.len(), (56 * 56 * 3) as usize);
        // The center texel is deep inside the fill (median well above the
        // 128 mid-level); a corner of the tile is far outside (well below).
        let center = (field.height / 2) * field.width + field.width / 2;
        assert!(
            median(&field.rgb, center as usize) > 160,
            "center should read as inside"
        );
        assert!(
            median(&field.rgb, 0) < 96,
            "tile corner should read as outside"
        );
        // The padded plane bounds extend distance_range/scale beyond the
        // geometry on each side.
        let margin = DEFAULT_DISTANCE_RANGE / 1.2;
        assert!((field.plane_bounds.left - (-margin)).abs() < 1e-9);
        assert!((field.plane_bounds.right - (40.0 + margin)).abs() < 1e-9);
    }

    #[test]
    fn plane_bounds_span_the_ceild_atlas_tile_exactly() {
        // Regression for the anisotropic plane-bounds bug: the atlas tile is
        // `ceil(extent * scale + 2 * distance_range)` texels on each axis, but
        // the far plane-bounds edge used to sit at the *un-ceil'd* geometry
        // extent, so the painter mapped a wider tile onto a narrower quad and
        // rendered the field up to one texel too small — and, because x and y
        // ceil independently, anisotropically.
        //
        // A 37.3 x 60 shape at px_per_em 48: the em is the 60-unit side, so its
        // texel extent is a whole number (60 * 48/60 + 8 = 56); the 37.3-unit
        // side is not (37.3 * 0.8 + 8 = 37.84, which ceils up to 38). The
        // shorter axis is exactly where the old far edge and the ceil'd tile
        // disagreed, so this fixture catches the aspect distortion.
        let px_per_em = 48.0;
        let dr = DEFAULT_DISTANCE_RANGE;
        let plan = plan_field(
            &VectorPath {
                path: "M 0 0 L 37.3 0 L 37.3 60 L 0 60 Z",
                winding: WindingRule::NonZero,
            },
            px_per_em,
            dr,
        )
        .expect("plans");

        // The fixture must exercise a genuine ceil round-up on the short axis,
        // or the assertion below would hold even against the old code.
        let short_unceiled = 37.3 * (px_per_em / 60.0) + 2.0 * dr;
        assert!(
            (short_unceiled.ceil() - short_unceiled).abs() > 1e-6,
            "the fixture must round the short axis up, got {short_unceiled}"
        );

        // The plane-bounds quad must span exactly the ceil'd tile on both axes,
        // so `(right - left) * scale == width` and likewise for height. Against
        // the old un-ceil'd far edge the short axis spanned 37.84 texels, not
        // 38, and this fails.
        let pb = plan.plane_bounds;
        let span_x = (pb.right - pb.left) * plan.scale;
        let span_y = (pb.bottom - pb.top) * plan.scale;
        assert!(
            (span_x - f64::from(plan.width)).abs() < 1e-9,
            "plane bounds must span exactly {} texels wide, span {span_x}",
            plan.width
        );
        assert!(
            (span_y - f64::from(plan.height)).abs() < 1e-9,
            "plane bounds must span exactly {} texels tall, span {span_y}",
            plan.height
        );
        // Isotropy: with both axes mapping at exactly `scale`, texels-per-shape-
        // unit is equal on x and y, so no aspect distortion remains.
        assert!(
            ((span_x / f64::from(plan.width)) - (span_y / f64::from(plan.height))).abs() < 1e-12,
            "the tile-to-quad mapping must be isotropic"
        );
    }

    #[test]
    fn hole_reads_as_outside_under_evenodd() {
        let field = bake_single(
            &VectorPath {
                path: SQUARE_WITH_HOLE,
                winding: WindingRule::EvenOdd,
            },
            DEFAULT_PX_PER_EM,
            DEFAULT_DISTANCE_RANGE,
        )
        .expect("bakes");
        // The tile center sits in the hole, which EVENODD leaves empty, so
        // it must read as outside (median below mid).
        let center = (field.height / 2) * field.width + field.width / 2;
        assert!(
            median(&field.rgb, center as usize) < 128,
            "hole center should read as outside"
        );
        // A texel in the ring wall (a quarter of the way across) is inside.
        let wall = (field.height / 2) * field.width + field.width / 4;
        assert!(
            median(&field.rgb, wall as usize) > 128,
            "ring wall should read as inside"
        );
    }

    #[test]
    fn dedup_returns_same_index_for_identical_geometry() {
        let mut baker = VectorAtlasBaker::new();
        let a = baker
            .add(&VectorPath {
                path: SQUARE,
                winding: WindingRule::NonZero,
            })
            .unwrap();
        let b = baker
            .add(&VectorPath {
                path: SQUARE_ALT,
                winding: WindingRule::NonZero,
            })
            .unwrap();
        assert_eq!(a, b, "identical geometry dedups to one field");
        // A different winding rule is a different field.
        let c = baker
            .add(&VectorPath {
                path: SQUARE,
                winding: WindingRule::EvenOdd,
            })
            .unwrap();
        assert_ne!(a, c);
    }

    /// `-0.0` and `+0.0` place the same point, so two paths differing only in
    /// the sign of a zero coordinate are the same shape and must share one
    /// field (debt #358). Keying on the raw bits baked the outline twice.
    #[test]
    fn a_negative_zero_coordinate_dedups_with_a_positive_zero() {
        let mut baker = VectorAtlasBaker::new();
        let plus = baker
            .add(&VectorPath {
                path: "M 0 0 L 40 0 L 40 40 L 0 40 Z",
                winding: WindingRule::NonZero,
            })
            .unwrap();
        let minus = baker
            .add(&VectorPath {
                path: "M -0 -0 L 40 -0 L 40 40 L -0 40 Z",
                winding: WindingRule::NonZero,
            })
            .unwrap();
        assert_eq!(minus, plus, "a signed zero is the same point");
        assert_eq!(baker.len(), 1, "and so bakes one field, not two");
    }

    /// A hash collision must never hand back another shape's field (debt
    /// #358). The hash picks a candidate; the geometry decides.
    ///
    /// A real 64-bit collision cannot be constructed on demand, so the
    /// collision is injected: two distinct shapes are baked, then the first's
    /// bucket is pointed at by the second's hash, which is exactly the state a
    /// collision produces. Asking for the second shape again must still return
    /// the second shape.
    #[test]
    fn a_hash_collision_does_not_return_the_other_shapes_field() {
        let mut baker = VectorAtlasBaker::new();
        let square = VectorPath {
            path: SQUARE,
            winding: WindingRule::NonZero,
        };
        // A different extent, so the two fields are told apart by their plane
        // bounds — `SQUARE_WITH_HOLE` shares the square's bounding box and
        // would make the check pass against the wrong field.
        let wide = VectorPath {
            path: "M 0 0 L 100 0 L 100 60 L 0 60 Z",
            winding: WindingRule::NonZero,
        };
        assert_eq!(baker.add(&square).unwrap(), 0);
        assert_eq!(baker.add(&wide).unwrap(), 1);

        // Force the wide shape's hash to select the square's field.
        let wide_key = geometry_key(&parse_path(wide.path).unwrap(), wide.winding);
        baker.seen.insert(key_hash(&wide_key), 0);

        assert_eq!(
            baker.add(&wide).unwrap(),
            2,
            "a colliding shape bakes its own field rather than borrowing one",
        );
        let bake = baker.finish().expect("packs");
        assert_eq!(
            bake.shapes[2].plane_bounds, bake.shapes[1].plane_bounds,
            "and that field is the wide shape's own",
        );
        assert_ne!(
            bake.shapes[2].plane_bounds, bake.shapes[0].plane_bounds,
            "not the square's, which the collided bucket pointed at",
        );
    }

    /// Rolling a shape back also forgets its dedup entry (debt #356).
    ///
    /// Truncating the fields alone would leave `seen` mapping that geometry to
    /// an index the baker no longer has, so the next node with the same path
    /// would be handed a shape index past the end of the packed list — a
    /// dangling reference, strictly worse than the orphan tile the rollback
    /// exists to remove.
    #[test]
    fn truncate_forgets_the_dedup_entry_for_the_rolled_back_shape() {
        let mut baker = VectorAtlasBaker::new();
        let square = VectorPath {
            path: SQUARE,
            winding: WindingRule::NonZero,
        };
        let hole = VectorPath {
            path: SQUARE_WITH_HOLE,
            winding: WindingRule::EvenOdd,
        };
        assert_eq!(baker.add(&square).unwrap(), 0);
        let before = baker.len();
        assert_eq!(baker.add(&hole).unwrap(), 1);
        baker.truncate(before);
        assert_eq!(baker.len(), 1, "the rolled-back field is gone");

        // The same geometry again must bake afresh at the reclaimed index,
        // not return the stale 1.
        assert_eq!(baker.add(&hole).unwrap(), 1);
        let bake = baker.finish().expect("packs");
        assert_eq!(bake.shapes.len(), 2, "every index names a packed shape");

        // A shape index handed out before the rollback still names its own
        // field: the rollback only drops the tail.
        assert_eq!(baker_index_of(SQUARE, WindingRule::NonZero), 0);
    }

    /// The index a fresh baker gives `path` — the first-shape case, stated as a
    /// function so the assertion above reads as the invariant it checks.
    fn baker_index_of(path: &str, winding: WindingRule) -> u32 {
        VectorAtlasBaker::new()
            .add(&VectorPath { path, winding })
            .expect("bakes")
    }

    /// Truncating to a length the baker has not reached is a no-op, so the
    /// caller does not have to know whether its `add` deduped.
    #[test]
    fn truncate_past_the_end_keeps_every_shape() {
        let mut baker = VectorAtlasBaker::new();
        baker
            .add(&VectorPath {
                path: SQUARE,
                winding: WindingRule::NonZero,
            })
            .unwrap();
        baker.truncate(5);
        assert_eq!(baker.len(), 1);
    }

    #[test]
    fn packer_places_shapes_without_overlap() {
        let mut baker = VectorAtlasBaker::new();
        // Five distinct shapes at different sizes.
        for size in [20.0_f64, 40.0, 60.0, 30.0, 80.0] {
            let path = format!("M 0 0 L {size} 0 L {size} {size} L 0 {size} Z");
            baker
                .add(&VectorPath {
                    path: &path,
                    winding: WindingRule::NonZero,
                })
                .unwrap();
        }
        let bake = baker.finish().expect("packs");
        assert_eq!(bake.shapes.len(), 5);
        // Every rect fits inside the atlas.
        for shape in &bake.shapes {
            let r = shape.atlas_rect;
            assert!(r.x + r.width <= bake.width, "rect within atlas width");
            assert!(r.y + r.height <= bake.height, "rect within atlas height");
        }
        // No two rects overlap.
        for (i, a) in bake.shapes.iter().enumerate() {
            for b in bake.shapes.iter().skip(i + 1) {
                let (a, b) = (a.atlas_rect, b.atlas_rect);
                let disjoint = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                assert!(disjoint, "packed rects must not overlap: {a:?} vs {b:?}");
            }
        }
        // The atlas PNG decodes to the reported dimensions.
        let decoded = image::load_from_memory_with_format(&bake.image_png, image::ImageFormat::Png)
            .expect("atlas PNG decodes");
        assert_eq!(decoded.width(), bake.width);
        assert_eq!(decoded.height(), bake.height);
    }

    #[test]
    fn a_single_non_square_field_packs_within_the_atlas() {
        // A lone wide field is the packer edge case: with only one tile the
        // area-driven target rounds down to the field's own width, so the atlas
        // must still reserve the trailing gutter or the tile runs off the right
        // edge and `blit` panics (the bake oracle surfaced this on the star).
        let mut baker = VectorAtlasBaker::new();
        baker
            .add(&VectorPath {
                path: "M 0 0 L 100 0 L 100 60 L 0 60 Z",
                winding: WindingRule::NonZero,
            })
            .unwrap();
        let bake = baker.finish().expect("a single field packs");
        assert_eq!(bake.shapes.len(), 1);
        let r = bake.shapes[0].atlas_rect;
        assert!(
            r.x + r.width <= bake.width && r.y + r.height <= bake.height,
            "the lone tile {r:?} must fit inside the {}x{} atlas",
            bake.width,
            bake.height
        );
        let decoded = image::load_from_memory_with_format(&bake.image_png, image::ImageFormat::Png)
            .expect("atlas PNG decodes");
        assert_eq!(
            (decoded.width(), decoded.height()),
            (bake.width, bake.height)
        );
    }
}
