//! Runtime that resolves the model — the Taffy layout solve
//! (docs/design/architecture.md; variants, FLIP, and the measure callback land at
//! their own slices).
//!
//! [`TaffySolver`] implements `dashscene-core`'s `LayoutSolver` seam:
//! it maps the arena's layout intent to a Taffy tree (one tree per
//! root — roots are independent coordinate islands), solves, and
//! returns absolute rects. Taffy is the sole solver (P2); layout mode
//! `None` is a passthrough expressed as absolutely-positioned children,
//! not a second engine.
//!
//! The tree is **retained** across solves (issue #164). The first solve
//! builds it and reports every node; a later solve marks only the nodes
//! whose layout intent changed (via `set_style`, which clears a node and
//! its ancestors), lets Taffy recompute just those subtrees, and reads
//! back only the rects whose absolute position or size actually moved.
//! A commit whose changes are all paint-only marks nothing and performs
//! no solve at all. A structural change — the node count grew — rebuilds
//! the tree, since the arena's DFS indices have shifted underneath it.

pub mod flip;

// `Channel` is the document binding vocabulary and lives in
// `dashscene-core` since story #167 (one channel set for binding rows,
// reactive bindings, and FLIP tracks — debt #208); re-exported here so a
// FLIP consumer keeps one import path for the key and its channel.
pub use dashscene_core::Channel;
pub use flip::{VariantFlip, decode_prop_key, prop_key};

use std::sync::Arc;

use dashpaint::image_id::identify;
use dashscene_core::{
    Arena, Atlas, AtlasGlyph, AtlasIndex, AxisSizing, GlyphQuad, GlyphRange, GlyphRun, GridTrack,
    ImageAsset, ImageFormat, Layout, LayoutMode, LayoutSolver, NodeId, SolvedRect, StagedRun,
    TextAlignV, TextStyle,
};
use dashscene_typeset::atlas::AtlasMetrics;
use dashscene_typeset::text::{Font, FontFamily, TextShape, Typesetter, WeightedFont};
use rustc_hash::FxHashSet;
use taffy::prelude::*;
use taffy::{AlignContent, AlignItems, AlignSelf, GridPlacement, JustifyContent, Position};

/// The text a host hands a document load, so its text can be measured and drawn.
///
/// **A `.dsb` carries neither of these**, and that is a ruling rather than a
/// gap: `docs/decisions/font-resolution-order.md` step 1 would embed a font,
/// and its own Consequences record why nothing implements it — the render path
/// consumes an `AtlasBundle` and the MSDF baker is an external pinned binary,
/// so nothing turns embedded font bytes into glyphs at load time. A rasterised
/// atlas must never be embedded at all: it is a result, and P1 forbids results
/// in the document.
///
/// So the host supplies both, and this is the type it supplies them in (story
/// #863). A load given [`None`] instead builds `TaffySolver::new()` and lays
/// its text out as empty leaves — which is what every load path did before this
/// story, and is now a choice a caller makes rather than one made for it.
///
/// `atlases` must be in the cascade's font-slot order: a shaped glyph carries
/// the slot of the face that shaped it, and that slot indexes this list
/// directly. A list in any other order samples the wrong face rather than
/// failing. `corpus/showcase/src/resources.rs` builds a cascade and its atlases
/// together for exactly that reason, and is the worked example.
#[derive(Debug)]
#[non_exhaustive]
pub struct TextResources {
    /// The typesetter text is shaped and measured through.
    pub typesetter: Typesetter,
    /// The atlases staged runs sample, in the cascade's font-slot order.
    ///
    /// Shared rather than owned, because every producer of one already holds an
    /// `Arc` and the solver stores an `Arc`: taking a `Vec` here would deep-copy
    /// the whole set — three atlases with their texel payloads, about a megabyte
    /// — between two `Arc`s, on every call. `demo` rebuilds its scene on every
    /// resize step of a window drag.
    pub atlases: Arc<Vec<Atlas>>,
}

impl TextResources {
    /// The pair, in the order the cascade declares its faces.
    ///
    /// # Panics
    ///
    /// In a debug build, if `atlases` is neither empty nor one entry per face.
    /// The list is indexed by the slot of the face that shaped a glyph, so a
    /// short list resolves an index past its end and a reordered one samples
    /// the wrong face — neither fails on its own, which is why this is checked
    /// where the pair is made rather than discovered in a picture (P4).
    ///
    /// Empty is allowed and is not the same mistake: it is the measure-only
    /// solver, which shapes text for layout and stages no runs.
    pub fn new(typesetter: Typesetter, atlases: Arc<Vec<Atlas>>) -> Self {
        debug_assert!(
            atlases.is_empty() || atlases.len() == typesetter.fonts().len(),
            "an atlas per face, in the cascade's font-slot order, or none at all: {} atlases \
             against {} faces",
            atlases.len(),
            typesetter.fonts().len(),
        );
        Self {
            typesetter,
            atlases,
        }
    }
}

/// One face a host supplies, with the atlas its shaped glyphs sample.
///
/// Owned bytes rather than borrowed, because the caller this was written for is
/// a C ABI whose pointers are valid only for the length of the call. It is no
/// longer the only one: `corpus/showcase` builds these directly, and
/// `dashscene-desktop` and `dashscene-web` re-export this type so an embedder
/// can too (issue #992). Owned suits all three — a caller assembling a
/// descriptor in Rust hands over bytes it no longer needs.
#[derive(Debug)]
pub struct FaceBytes {
    /// The family this face belongs to. Faces sharing a name become one
    /// family, in first-appearance order, however they are ordered here.
    /// "Sharing a name" is [`FontFamily::name_matches`] — trimmed and
    /// ASCII-case-insensitive, the same test a document's `TextStyle::family`
    /// is resolved by — and the family takes the first spelling to appear.
    pub family: String,
    /// The CSS weight this face stands for.
    ///
    /// **Unchecked here.** A CSS weight is `1..=1000` and `dashscene-ffi`
    /// refuses one outside it by name, but this constructor does not: the value
    /// reaches `WeightedFont::new` as given, and a face declared at 0 then
    /// resolves against every request as if the caller had meant it. So the
    /// same descriptor is refused on the C route and accepted on this one.
    /// Which side should hold the rule is issue #1206.
    pub weight: u16,
    /// The font file's bytes.
    pub font: Vec<u8>,
    /// Which face within a collection. Zero for a single-face file.
    pub face_index: u32,
    /// The committed sheet, or [`None`] for a measure-only cascade. Either
    /// every face carries one or none does.
    pub atlas: Option<AtlasBytes>,
}

/// A committed atlas as its two files' bytes — what `corpus/atlas/*/` holds
/// and what the MSDF tool emits.
///
/// **Nothing bakes one of these at run time.**
/// `dashscene_typeset::atlas::generate` shells out to an external pinned
/// binary and reads its font from a path, so a host arrives with a sheet or
/// it gets no glyphs.
#[derive(Debug)]
pub struct AtlasBytes {
    /// The sheet, PNG-encoded.
    pub png: Vec<u8>,
    /// The postcard `AtlasMetrics` beside it.
    pub metrics: Vec<u8>,
}

/// Why a set of [`FaceBytes`] is not a cascade.
///
/// Every variant names the entry it came from, because the caller assembling
/// the list is a host that cannot see this one.
///
/// [`Display`](std::fmt::Display) is what a host reads: `dashscene-ffi` puts
/// it straight into `ds_last_error_message`, where every other message on
/// that path is prose. So each variant's message is a sentence, and nothing
/// on this path formats the error with `{:?}`.
#[derive(Debug)]
#[non_exhaustive]
pub enum TextResourcesError {
    /// No faces at all. `Typesetter::with_named_font_families` asserts on
    /// this, so it is caught rather than reached.
    NoFaces,
    /// A face declared a family name that is empty once trimmed, which no
    /// document could ever ask for: `FontFamily::name_matches` trims both
    /// sides and returns false when either is empty, so no `TextStyle::family`
    /// selects such a family.
    ///
    /// Unrequestable is not unreachable. `Typesetter::probe_order` builds
    /// `(0..families.len())` and only *promotes* a matched family to the
    /// head, so every family stays in the cascade and shapes whatever the
    /// ones ahead of it do not cover — which is exactly what a
    /// `FontFamily::unnamed` family is, the whole pre-#385 cascade shape. So
    /// a face here would still draw, as an unlabelled coverage fallback at
    /// whatever position the caller happened to list it. A host declaring a
    /// cascade is naming its families; a face it can never name back is a
    /// mistake in the descriptor rather than a fallback it asked for.
    ///
    /// Not a panic guard; the assertion in `with_named_font_families`
    /// inspects a family's faces, never its name.
    EmptyFamily { index: usize },
    /// A face's bytes are not a parseable font.
    Font { index: usize, message: String },
    /// A face's sheet is unusable: the metrics did not decode, the PNG's
    /// header did not parse or does not carry the extent the metrics
    /// declare, or a glyph is described by exactly one of its two quads.
    Atlas { index: usize, message: String },
    /// Some faces carry a sheet and some do not. The list is indexed by font
    /// slot, so a short one resolves past its end — which is why this is
    /// rejected rather than padded or truncated.
    MixedAtlases,
}

impl std::fmt::Display for TextResourcesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFaces => write!(f, "no font faces were supplied"),
            Self::EmptyFamily { index } => write!(
                f,
                "face {index}: the family name is empty once trimmed, so no document could \
                 ever request it"
            ),
            Self::Font { index, message } => {
                write!(f, "face {index}: the font bytes are unusable: {message}")
            }
            Self::Atlas { index, message } => write!(f, "face {index}: {message}"),
            Self::MixedAtlases => write!(
                f,
                "some faces carry an atlas and some do not; the atlas list is indexed by font \
                 slot, so it is every face or none"
            ),
        }
    }
}

impl std::error::Error for TextResourcesError {}

impl TextResources {
    /// Assembles a cascade and its atlases from bytes a host supplied.
    ///
    /// **The two lists are built from one walk**, which is the whole point.
    /// Faces are grouped by family in first-appearance order and
    /// `Typesetter::with_named_font_families` flattens family-major over
    /// exactly that order, so a face's atlas lands at the slot its glyphs
    /// will carry however the caller ordered the argument. Building the
    /// atlas list separately is what would let a caller mis-order it, and a
    /// mis-ordered list samples the wrong face rather than failing.
    pub fn from_faces(faces: Vec<FaceBytes>) -> Result<Self, TextResourcesError> {
        if faces.is_empty() {
            return Err(TextResourcesError::NoFaces);
        }
        let sheets = faces.iter().filter(|face| face.atlas.is_some()).count();
        if sheets != 0 && sheets != faces.len() {
            return Err(TextResourcesError::MixedAtlases);
        }

        // Group by family name, keeping first appearance. Indices only, so
        // the owned bytes move once, below, in the flatten's order.
        let mut names: Vec<&str> = Vec::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        for (index, face) in faces.iter().enumerate() {
            // Trimmed, because `FontFamily::name_matches` trims before
            // comparing: a name of only spaces is as unselectable as "".
            if face.family.trim().is_empty() {
                return Err(TextResourcesError::EmptyFamily { index });
            }
            // Grouped on the predicate the typesetter SELECTS with, not on
            // string equality. `Typesetter::probe_order` promotes only the
            // first family whose name matches and leaves the rest in cascade
            // order, so "Inter" and "inter" as two families would send a
            // request for Inter at 600 into the first one — which holds only
            // the 400 face — and bold would render regular, reported through
            // `Typesetter::weight_substitutions` where no C caller can read
            // it.
            match names
                .iter()
                .position(|name| FontFamily::name_matches(name, &face.family))
            {
                Some(slot) => members[slot].push(index),
                None => {
                    names.push(&face.family);
                    members.push(vec![index]);
                }
            }
        }
        let names: Vec<String> = names.into_iter().map(str::to_string).collect();

        let mut taken: Vec<Option<FaceBytes>> = faces.into_iter().map(Some).collect();
        let mut families = Vec::with_capacity(names.len());
        let mut atlases = Vec::new();
        for (name, group) in names.into_iter().zip(members) {
            let mut weighted = Vec::with_capacity(group.len());
            for index in group {
                let face = taken[index]
                    .take()
                    .expect("each index is grouped exactly once");
                let font = Font::from_bytes(face.font, face.face_index).map_err(|error| {
                    TextResourcesError::Font {
                        index,
                        message: format!("{error}"),
                    }
                })?;
                if let Some(sheet) = face.atlas {
                    atlases.push(atlas_from_bytes(sheet, index)?);
                }
                weighted.push(WeightedFont::new(font, face.weight));
            }
            families.push(FontFamily::new(name, weighted));
        }
        Ok(Self::new(
            Typesetter::with_named_font_families(families),
            Arc::new(atlases),
        ))
    }
}

/// A committed sheet's two files, as the boundary-B atlas a staged run
/// samples.
///
/// A glyph described by neither a plane quad nor an atlas quad is dropped:
/// that is an empty outline, such as the space, which occupies advance and
/// paints nothing. It is the filter `corpus/showcase` reaches through this
/// function rather than restating (issue #962), and the reason
/// `Atlas::new`'s sorted-and-unique assertion still holds:
/// `AtlasMetrics::glyphs` is sorted by glyph id and filtering preserves
/// order. A glyph carrying exactly one of the two is **refused** rather than
/// dropped — `AtlasMetrics::from_bytes` does not check the pair agrees, and
/// these are host bytes that passed no `dashc` gate, so dropping one would
/// leave `Atlas::glyph`'s binary search missing that character with nothing
/// reported.
///
/// # What the header check buys, and what it does not
///
/// The PNG is copied through unread everywhere else, so its **header** is
/// read here through the same [`identify`] every other writer in this
/// workspace uses: the signature, the `IHDR` chunk type and length, and the
/// extent. That catches a file that is not a PNG at all, a re-muxed one
/// whose first chunk is not `IHDR`, and — against the metrics — a sheet
/// whose extent disagrees with the quads that normalise over it, which is
/// the silent case: `TexelPayload::of` takes the extent from the decode
/// while `gpu_glyph_run` normalises with the metrics extent, so a
/// disagreement samples the wrong texels rather than failing.
///
/// **A body that fails to decode is still a panic at the first draw.**
/// `dashscene_gpu`'s `decode_png` calls `next_frame(..).expect(..)`, so a
/// correctly-headed PNG with a truncated or CRC-corrupt `IDAT` passes
/// everything here and unwinds there; across the C ABI `guard` catches it
/// and reports `DsStatus::Panic`. Closing that would mean decoding the
/// whole sheet at load, which is a real cost and a separate decision.
fn atlas_from_bytes(sheet: AtlasBytes, index: usize) -> Result<Atlas, TextResourcesError> {
    let atlas_error = |message: String| TextResourcesError::Atlas { index, message };
    let metrics = AtlasMetrics::from_bytes(&sheet.metrics)
        .map_err(|error| atlas_error(format!("atlas_metrics did not decode: {error}")))?;

    // `identify` rather than a second reading of the same bytes: it checks
    // the chunk type is IHDR and the chunk length is 13, which a signature
    // test and two fixed offsets do not, and it names the format it found.
    let header = identify(&sheet.png)
        .map_err(|error| atlas_error(format!("atlas_png is unusable: {error}")))?;
    if header.format != ImageFormat::Png {
        return Err(atlas_error(format!(
            "atlas_png is {:?} and a committed sheet is PNG",
            header.format
        )));
    }
    if header.width != metrics.atlas.width || header.height != metrics.atlas.height {
        return Err(atlas_error(format!(
            "atlas_png is {} x {} and its metrics declare {} x {}",
            header.width, header.height, metrics.atlas.width, metrics.atlas.height
        )));
    }

    let mut glyphs = Vec::with_capacity(metrics.glyphs.len());
    for glyph in &metrics.glyphs {
        match (glyph.plane_em, glyph.atlas_px) {
            (Some(plane_em), Some(atlas_px)) => glyphs.push(AtlasGlyph {
                glyph_id: u32::from(glyph.glyph_id),
                plane_em,
                atlas_px,
            }),
            // Neither is an empty outline, and dropping it is what makes the
            // space legal.
            (None, None) => {}
            (plane_em, _) => {
                let (present, absent) = if plane_em.is_some() {
                    ("plane_em", "atlas_px")
                } else {
                    ("atlas_px", "plane_em")
                };
                return Err(atlas_error(format!(
                    "atlas_metrics glyph {} carries {present} and no {absent}; a glyph is \
                     described by both or by neither",
                    glyph.glyph_id
                )));
            }
        }
    }
    // `Atlas::new` is fallible since issue #724, and refuses four values: a
    // zero `px_per_em`, a `distance_range_px` that is not finite and positive
    // (issue #964), a glyph id above `u16::MAX` (issue #966), and a zero width
    // or height (issue #1001). All four come from the metrics blob a host
    // supplied, so each is a host-data error on this path rather than a broken
    // contract between crates, and is reported as one instead of being
    // unwrapped.
    //
    // **The extent is a behaviour change on this path, and a deliberate one.**
    // `identify_png` does not refuse a zero IHDR extent, so a sheet declaring
    // 0 x 0 in both its header and its metrics passes the agreement check above
    // and used to build an atlas: `dashscene-gpu` then skipped every run naming
    // it and the document rendered with no text at all, silently. Now the load
    // fails here and the host is told which blob is wrong. P4 is the rule that
    // makes that the right way round — a named diagnostic rather than a silent
    // drop — and it is the same argument the three refusals above already made.
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: sheet.png,
        },
        metrics.atlas.width,
        metrics.atlas.height,
        metrics.atlas.px_per_em,
        metrics.atlas.distance_range_px,
        glyphs,
    )
    .map_err(|error| atlas_error(format!("{error}")))
}

/// How a solver reaches its [`Typesetter`]: lent by the caller, or held.
///
/// Lending is the original seam and stays the default
/// (`docs/decisions/measure-callback-typesetter-seam.md`): the caller keeps one
/// typesetter for the whole runtime, so layout and paint read one shaped-run
/// cache and cannot disagree about a glyph's size.
///
/// Holding exists for the one caller that cannot lend — a host that loads a
/// document. `LiveScene` takes a `Box<dyn LayoutSolver>` and keeps it for the
/// life of the scene, so the solver in that box is `'static` and a borrowed
/// typesetter cannot travel in it. The alternative in the tree before story
/// #863 was a wrapper type that owned the typesetter and built a fresh
/// `TaffySolver` per call, which works and **throws the retained tree away on
/// every solve** — issue #164's whole saving, paid back per frame. Holding the
/// typesetter inside the solver keeps the tree.
///
/// `corpus/showcase` was the last of those wrappers in the tree and is one no
/// longer: it hands each scene a single `owning` solver (issue #950). What that
/// cost it is the invariant a retained tree carries — one solver has to see
/// every commit into the arena that **solves**, in order, because such a commit
/// consumes the layout-dirty set the next solve would have patched from. A
/// commit that replays geometry the producer resolved for itself leaves the set
/// alone and is therefore not a second producer in this sense
/// (`LayoutSolver::consumes_layout_dirty`, issue #1148). See
/// `docs/decisions/one-solver-per-live-scene.md`.
#[derive(Debug)]
enum Text<'a> {
    Lent(&'a mut Typesetter),
    Held(Box<Typesetter>),
}

impl Text<'_> {
    fn get_mut(&mut self) -> &mut Typesetter {
        match self {
            Text::Lent(ts) => ts,
            Text::Held(ts) => ts,
        }
    }
}

/// The Taffy implementation of `dashscene-core`'s `LayoutSolver`.
///
/// **The typesetter is lent by default and may be held.** Lending is the
/// original seam: the caller keeps one [`Typesetter`] for the whole runtime and
/// lends it here for the solve, so the measure callback and the painter (#30)
/// read one shaped-run cache and cannot disagree about a glyph's size
/// ([`with_typesetter`](TaffySolver::with_typesetter),
/// [`with_text`](TaffySolver::with_text)). Holding is
/// [`owning`](TaffySolver::owning), for the caller that has nothing to lend —
/// a host loading a document, whose scene outlives every local it could lend
/// (story #863). A solver built with [`new`](TaffySolver::new) carries no
/// typesetter and solves a text-free scene; text nodes in such a scene are
/// simply not measured, and size as empty leaves.
///
/// A solver retains its Taffy tree across solves (issue #164), so it is
/// bound to one arena for its lifetime: reusing a solver against a
/// different arena would read a mismatched tree. The runtime keeps one
/// solver per arena, the same way it keeps one typesetter.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct TaffySolver<'a> {
    typesetter: Option<Text<'a>>,
    /// The atlases staged runs sample, in the cascade's font-slot order.
    /// Empty for a solver that stages no text. Shared rather than copied,
    /// because commit rebuilds the run table every frame and the atlas set
    /// behind it does not change.
    atlases: Arc<Vec<Atlas>>,
    /// The retained tree and its per-node bookkeeping. `None` until the
    /// first solve builds it.
    state: Option<TreeState>,
    /// How many Taffy layout computations this solver has run. A commit
    /// whose changes are all paint-only leaves this unchanged — the whole
    /// point of the retained tree (issue #164). Read via
    /// [`solves`](TaffySolver::solves).
    solves: u64,
}

/// The #272 baseline corrections one solve produced: a cross-axis offset for
/// each child of a baseline-aligned text row, and for no other node.
///
/// **Dense and stamped, the shape [`TreeState::on_path`] uses**, and for the
/// same reasons (issue #1111): nothing is allocated per frame, so the per-frame
/// band still reads 0 bytes, and a lookup is one indexed read at any scene size.
///
/// The sparse `FxHashMap` this replaced allocated nothing on a scene with no
/// baseline row either — `hashbrown` answers an empty map from a length check
/// and never hashes — but on a scene carrying one it put a hash and a probe on
/// the readback path for **every node the readback visits**, where the vector it
/// had itself replaced cost an index. That is issue #1153: the map did not add
/// a cost, it moved one, from a per-frame allocation the band could see to a
/// per-node probe the band cannot. This is the third shape, and it is the only
/// one with neither. `docs/technotes/frame-budget.md` carries what the probe
/// measured, which is that it was never visible at the frame level.
///
/// **The slots are allocated by the first collection, not by the constructor**,
/// so a solver with no typesetter keeps the empty map's zero heap. That is not
/// a corner: `TaffySolver::new()` is what `dashscene-desktop` builds and what
/// the per-frame band's own fixture runs, and on that path `baseline_pass`
/// returns before recording anything, so a table sized by the document would be
/// bytes allocated at every rebuild for a lookup that can only miss.
///
/// It is **not** the same mechanism as `on_path` and should not be merged with
/// it. That one marks and tests for membership with a payload of nothing, this
/// one carries a value, and the two differ in what a wrapped stamp costs: a
/// wrapped `on_path` reads every node as marked and the readback over-descends
/// for one solve, where a wrapped stamp here would answer 0.0 as a real
/// correction. [`Self::begin`] handles the wrap for that reason and `on_path`
/// does not need to.
#[derive(Debug, Default)]
struct BaselineOffsets {
    /// `(stamp, offset)` per [`NodeId`] slot, or **empty** until a collection
    /// sizes it. A slot whose stamp is not [`Self::stamp`] carries no
    /// correction, and its `f32` is whatever an earlier collection left there.
    slots: Vec<(u32, f32)>,
    /// The stamp the collection in progress writes and [`Self::y_or`] compares
    /// against. Bumped by [`Self::begin`] rather than cleared, so the previous
    /// collection's entries read as absent without touching them — including
    /// the **two** collections one [`baseline_pass`] can run.
    stamp: u32,
}

impl BaselineOffsets {
    /// Start a fresh collection over `node_count` nodes: everything the
    /// previous one wrote reads as absent from here on.
    ///
    /// Sizing and bumping are one call because a collection needs both and
    /// neither is meaningful alone — a stamp bumped over slots that are about
    /// to be replaced says nothing, and slots sized without a bump would keep
    /// the last collection's entries live.
    ///
    /// **The wrap is handled rather than assumed away.** Zero is what the
    /// unwritten slots carry, so a stamp that wraps back to it would make every
    /// node without a correction report one of 0.0 — the whole shown subtree
    /// placed at local y = 0. It takes 2^32 **collections** to get there, and a
    /// solve runs one or two of them — two whenever a #322 floor moves, which is
    /// every frame of an animating HUG baseline row, the shape #322 exists for.
    /// So the floor is 2^31 solves: about 414 days at 60 Hz rather than never,
    /// on hardware meant to run for months. The cost of refusing to depend on
    /// that is one comparison per collection.
    fn begin(&mut self, node_count: usize) {
        if self.slots.len() != node_count {
            // A node count that moved has already forced a rebuild, so this
            // runs once per tree rather than per frame.
            self.slots = vec![(0, 0.0); node_count];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.slots.fill((0, 0.0));
            self.stamp = 1;
        }
    }

    /// Record `node`'s corrected cross-axis offset for this collection.
    fn insert(&mut self, node: NodeId, offset: f32) {
        self.slots[node.index()] = (self.stamp, offset);
    }

    /// `node`'s corrected cross-axis offset, or `taffy_y` where it has none.
    ///
    /// The readback's whole expression, stated once rather than at each of the
    /// two call sites — which is the other half of issue #1153.
    ///
    /// `get` rather than an index, so a node past the end answers `taffy_y` the
    /// way an absent key did. That is the state a solver with no typesetter is
    /// in for its whole life, and it keeps a rect misplaced rather than a panic
    /// on the frame path if the two ever disagree for another reason.
    ///
    /// The `debug_assert` is the tripwire that keeps the release fallback from
    /// hiding the disagreement, the same posture `taffy_of`'s `UNBUILT` sentinel
    /// takes: a **sized** table shorter than the arena is a structural bug, and
    /// this names it where it happened rather than as a row silently placed at
    /// Taffy's y on every later frame.
    fn y_or(&self, node: NodeId, taffy_y: f32) -> f32 {
        debug_assert!(
            self.slots.is_empty() || node.index() < self.slots.len(),
            "a sized baseline table must cover every node: {node:?} against {} slots",
            self.slots.len()
        );
        match self.slots.get(node.index()) {
            Some(&(stamp, offset)) if stamp == self.stamp => offset,
            _ => taffy_y,
        }
    }

    /// Drop the table, so nothing a previous collection recorded reads back.
    ///
    /// The zero-heap state a solver with no typesetter stays in, reached from
    /// wherever a solve ends without collecting.
    fn forget(&mut self) {
        self.slots = Vec::new();
    }
}

/// The retained Taffy tree plus the maps that let an incremental solve
/// find each arena node, walk to its ancestors, and tell whether its
/// resolved rect moved since last time. All the per-node vectors are
/// indexed by [`NodeId`] slot; `roots` and `prev_root_origin` follow
/// arena root order.
#[derive(Debug)]
struct TreeState {
    tree: TaffyTree<TextContext>,
    /// The Taffy node standing for each arena node.
    taffy_of: Vec<taffy::NodeId>,
    /// Each arena node's parent (its root has `None`).
    parent_of: Vec<Option<NodeId>>,
    /// The Taffy roots, in arena root order.
    roots: Vec<taffy::NodeId>,
    /// The previous solve's Taffy-relative layout per node, as bit
    /// patterns: `[location.x, location.y, size.width, size.height]`. Bits
    /// so the compare stays deterministic where `f32` equality is not.
    prev_rel: Vec<[u32; 4]>,
    /// The previous solve's authored root origin per root — the offset the
    /// readback adds, which Taffy does not model, so a root move is
    /// detected here rather than in the tree.
    prev_root_origin: Vec<[u32; 2]>,
    /// The cross-axis floors the #322 baseline pass has injected into the
    /// Taffy styles, in tree order. Held so a later solve can tell a floor
    /// that is still wanted from one that is stale: a row that stops
    /// needing a floor must have it removed, and the row's own style is not
    /// restyled when only a text child changed.
    baseline_floors: Vec<(NodeId, f32)>,
    /// The #272 baseline corrections the last solve produced. Retained for the
    /// same reason `on_path` below is, and — unlike it — **sized by the first
    /// collection that has a typesetter rather than here**, so a solver with no
    /// typesetter never allocates it at all (issue #1153).
    baseline_offsets: BaselineOffsets,
    /// Which nodes are on a path to a changed node, stamped with the solve
    /// that put them there rather than cleared between solves.
    ///
    /// Retained rather than allocated per frame, and dense rather than a set.
    /// A per-frame vector sized by the document is what issue #1111 removed;
    /// a set is bounded by the dirty closure and so is cheaper on the small
    /// frames this runtime is built for, but it rehashes its way up on the
    /// large ones — and a frame that dirties a large subtree is the frame a
    /// fixed budget is judged on. Stamping a retained vector is neither:
    /// nothing is allocated per frame, so the per-frame band still reads 0,
    /// and the marking stays one indexed write per node with no hashing at
    /// any dirty-set size.
    on_path: Vec<u32>,
    /// The stamp `on_path` carries for the solve in progress. Incremented per
    /// solve, so the previous solve's marks read as absent without a clear.
    pass: u32,
    /// The node count when the tree was built. A mismatch is a structural
    /// change and forces a rebuild.
    node_count: usize,
    /// The shown root this tree was last read back for. A mismatch means the
    /// newly shown root's subtree has never been reported, so the pruned
    /// readback below cannot be used for it (story #838).
    shown: Option<NodeId>,
}

impl<'a> TaffySolver<'a> {
    /// A solver with no typesetter — for scenes without text-driven
    /// sizing. A hug-sized text node solved this way is not measured
    /// (it has no font to shape with) and sizes as an empty leaf.
    pub fn new() -> Self {
        Self {
            typesetter: None,
            atlases: Arc::new(Vec::new()),
            state: None,
            solves: 0,
        }
    }

    /// A solver that measures text nodes against `typesetter`'s
    /// shaped-run cache. The borrow keeps the cache single-sourced: the
    /// same `Typesetter` the caller lends here is the one the painter
    /// reads at paint time (#30).
    ///
    /// It carries no atlases, so it measures text and stages none —
    /// layout without paint. A caller that wants the committed scene to
    /// carry drawable glyph runs builds the solver with
    /// [`with_text`](TaffySolver::with_text) instead.
    pub fn with_typesetter(typesetter: &'a mut Typesetter) -> Self {
        Self {
            typesetter: Some(Text::Lent(typesetter)),
            atlases: Arc::new(Vec::new()),
            state: None,
            solves: 0,
        }
    }

    /// A solver that measures text nodes *and* stages their glyph runs at
    /// commit — the one text producer (P2). The runs it stages are laid
    /// out under each node's lowered text axes, which is the same
    /// `TextShape` the measure callback sizes the node with, so the lines
    /// a painter draws are the lines the solve measured.
    ///
    /// `atlases` are the atlases those runs sample, **in the cascade's
    /// font-slot order**: a shaped glyph carries the slot of the face that
    /// shaped it, and that slot indexes this list directly. A list in any
    /// other order samples the wrong face rather than failing, so it is
    /// built next to the typesetter's own font list and from the same
    /// order.
    /// Takes anything that becomes an `Arc<Vec<Atlas>>`, so a caller already
    /// holding one shares it rather than deep-copying the set. A `Vec<Atlas>`
    /// still works unchanged. The distinction matters because issue #621 made
    /// `stage_text` a per-frame call rather than a per-solve one, and an atlas
    /// carries its own PNG payload — the showcase's three faces are about 226 kB,
    /// which a copy per frame would put in the frame loop of every host.
    pub fn with_text(typesetter: &'a mut Typesetter, atlases: impl Into<Arc<Vec<Atlas>>>) -> Self {
        Self {
            typesetter: Some(Text::Lent(typesetter)),
            atlases: atlases.into(),
            state: None,
            solves: 0,
        }
    }

    /// The boxed solver a document load hands to `dashlang::attach_live`, from
    /// what the host supplied.
    ///
    /// One function, so the integration crates that take a [`TextResources`]
    /// cannot disagree about what [`None`] means (story #863). All three call
    /// it, `dashscene-ffi` included since story #947 — its own
    /// [`TextResources`] is assembled by [`TextResources::from_faces`] from
    /// bytes that crossed a C boundary, and reaches this the same way the
    /// other two do.
    ///
    /// It means the pre-#863 solver: every text node measures as an empty leaf
    /// and no glyph run reaches the painter, which is correct for a document
    /// with no text and wrong for one with it.
    ///
    /// [`owning`](TaffySolver::owning) rather than a wrapper that owns the
    /// typesetter, for the reason that method records: the scene keeps this box
    /// for its whole life, and a solver rebuilt per call throws the retained
    /// tree away on every frame.
    pub fn boxed(text: Option<TextResources>) -> Box<dyn LayoutSolver> {
        match text {
            Some(text) => Box::new(TaffySolver::owning(text)),
            None => Box::new(TaffySolver::new()),
        }
    }

    /// A solver that **owns** its typesetter and atlas set, for a caller that
    /// cannot lend one.
    ///
    /// The lifetime is free — this is a `TaffySolver<'static>` — so it can go
    /// into the `Box<dyn LayoutSolver>` that `dashlang::attach_live` keeps for
    /// the life of a scene. That is the whole reason it exists: a host loading
    /// a `.dsb` has nothing to borrow from, because the scene outlives every
    /// local it could lend (story #863, issue #379's
    /// `docs/decisions/font-resolution-order.md`).
    ///
    /// It is otherwise [`with_text`](TaffySolver::with_text) exactly: text is
    /// measured through the typesetter and every text node's glyph runs are
    /// staged against `atlases`, which must be in the cascade's font-slot
    /// order. **The retained tree is retained**, which is what distinguishes
    /// this from wrapping a solver in a type that owns the typesetter and
    /// builds a fresh `TaffySolver` per call.
    ///
    /// The seam record's reason for lending — one typesetter for the whole
    /// runtime, so layout and paint cannot disagree about a glyph's size —
    /// still holds here, and is satisfied differently: this solver *is* the
    /// runtime's only text producer, staging runs at commit that a painter
    /// then reads out of the committed table rather than out of a cache.
    pub fn owning(text: TextResources) -> TaffySolver<'static> {
        TaffySolver {
            typesetter: Some(Text::Held(Box::new(text.typesetter))),
            atlases: text.atlases,
            state: None,
            solves: 0,
        }
    }

    /// How many Taffy layout computations this solver has run. It stays
    /// put across a paint-only commit — the retained tree is not
    /// recomputed when no layout intent changed (issue #164) — so a test
    /// can assert a paint-only frame did no solve.
    pub fn solves(&self) -> u64 {
        self.solves
    }
}

impl LayoutSolver for TaffySolver<'_> {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        let TaffySolver {
            typesetter,
            state,
            solves,
            ..
        } = self;
        // A grown node count means the arena's DFS indices shifted under
        // the retained tree; rebuild rather than patch.
        let structural = state
            .as_ref()
            .is_none_or(|s| s.node_count != arena.node_count());
        if structural {
            let (new_state, out) = rebuild(typesetter.as_mut().map(Text::get_mut), arena, solves);
            *state = Some(new_state);
            out
        } else {
            let state = state.as_mut().expect("non-structural implies a built tree");
            incremental(state, typesetter.as_mut().map(Text::get_mut), arena, solves)
        }
    }

    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        Arc::clone(&self.atlases)
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        // A solver with no atlas set stages nothing. A run whose atlas
        // index resolves against an empty table is not a run any painter
        // can draw, so producing one would be worse than producing none:
        // `TaffySolver::with_typesetter` is deliberately that solver, and
        // it measures text without painting it.
        if self.atlases.is_empty() {
            return Vec::new();
        }
        let Some(ts) = self.typesetter.as_mut().map(Text::get_mut) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // The shown roots, not every root: a run staged under a root the
        // committed table does not hold would be anchored on a rect index that
        // names another node (story #838).
        for &root in arena.shown_roots() {
            stage_subtree(arena, root, ts, geometry, &mut out);
        }
        out
    }
}

/// Stages every text node in `node`'s subtree, parent before child, so the
/// runs come back in document DFS order — which is rect-table index order.
fn stage_subtree(
    arena: &Arena,
    node: NodeId,
    ts: &mut Typesetter,
    geometry: &dyn Fn(NodeId) -> SolvedRect,
    out: &mut Vec<StagedRun>,
) {
    // A node is a text leaf exactly when it carries both authored
    // characters and a text style; either alone is a plain node, the same
    // test the measure seam applies.
    if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
        for (run, quads) in text_runs(ts, geometry(node), text, style) {
            out.push(StagedRun { node, run, quads });
        }
    }
    for &child in arena.children(node) {
        stage_subtree(arena, child, ts, geometry, out);
    }
}

/// Shapes one text node and places every glyph in absolute document space,
/// starting a new run wherever the fallback cascade switched fonts so each
/// run samples the atlas of the face that actually shaped it (story #219).
///
/// The painter moves nothing (P2), so the node's box origin is added here
/// and the block is shifted down by the vertical alignment's share of the
/// box's free space. The layout runs within the solved box width, so the
/// line breaks are the ones the solve measured.
fn text_runs(
    ts: &mut Typesetter,
    r: SolvedRect,
    text: &str,
    style: &TextStyle,
) -> Vec<(GlyphRun, Vec<GlyphQuad>)> {
    let laid = ts.layout_styled(
        text,
        style.size,
        Some(r.w),
        text_shape(style),
        style.weight,
        &style.family,
    );
    let voff = vertical_offset(r.h, laid.height, style.text_align_v);
    // The quads travel beside their run rather than inside it: a run's
    // `glyphs` is a range into the table's flat array, and a stager has no
    // table to index (story #578).
    let mut runs: Vec<(GlyphRun, Vec<GlyphQuad>)> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = AtlasIndex(u32::from(g.font));
            let quad = GlyphQuad {
                // Widened at boundary B, where a u16 leading a struct of
                // 4-byte members would only be two bytes of padding.
                glyph_id: u32::from(g.glyph_id),
                x: r.x + g.x,
                y: r.y + voff + g.y,
            };
            match runs.last_mut() {
                Some((run, quads)) if run.atlas == atlas => quads.push(quad),
                _ => runs.push((
                    GlyphRun {
                        // Commit stamps the anchor from the node this run was
                        // staged for; whatever is written here is overwritten.
                        rect: 0,
                        atlas,
                        size: style.size,
                        color: style.color,
                        // Assigned by `GlyphRunTable::push_run`, after commit
                        // has sorted the runs by anchor — so no offset a
                        // stager computed could survive anyway.
                        glyphs: GlyphRange::UNASSIGNED,
                        opacity: 1.0,
                    },
                    vec![quad],
                )),
            }
        }
    }
    runs
}

/// The y-offset that places a text block of height `content_height` within
/// a node box of height `box_height` under `align` (story #310).
///
/// Vertical alignment is block placement, not paint (P2) and not a measured
/// extent (P1) — the document carries the intent as an enum and the stager
/// resolves it here. `Top` is the zero-offset default; `Center` and
/// `Bottom` distribute the box's free space, which goes negative when the
/// text overflows its box, mirroring the overflow horizontal placement
/// already allows.
fn vertical_offset(box_height: f32, content_height: f32, align: TextAlignV) -> f32 {
    let slack = box_height - content_height;
    match align {
        TextAlignV::Top => 0.0,
        TextAlignV::Center => slack / 2.0,
        TextAlignV::Bottom => slack,
    }
}

#[cfg(test)]
mod baseline_offsets_tests {
    use super::BaselineOffsets;
    use dashscene_core::Arena;

    /// Four nodes of a real arena, because [`dashscene_core::NodeId`] has no
    /// public constructor and the table is indexed by the slot one carries.
    fn four_nodes() -> (Arena, Vec<dashscene_core::NodeId>) {
        let mut arena = Arena::new();
        let ids = {
            let mut txn = arena.open();
            let root = txn.add_node(None, None);
            let ids = vec![
                root,
                txn.add_node(Some(root), None),
                txn.add_node(Some(root), None),
                txn.add_node(Some(root), None),
            ];
            txn.commit();
            ids
        };
        (arena, ids)
    }

    /// A unit test rather than one beside the others in `tests/`, because
    /// reaching a wrapped stamp through the solver would take 2^32 solves.
    ///
    /// The wrap is the one case where the stamp's own encoding can lie: zero is
    /// what an unwritten slot carries, so a stamp back at zero makes every node
    /// without a correction report one of 0.0 — the whole shown subtree placed
    /// at local y = 0, on a runtime whose hosts are meant to run for weeks.
    #[test]
    fn a_wrapped_stamp_does_not_invent_a_correction() {
        let (_arena, ids) = four_nodes();
        let mut offsets = BaselineOffsets::default();
        offsets.begin(ids.len());
        offsets.insert(ids[1], 12.0);

        // The collection that wraps. Reaching it any other way is the 2^32
        // solves above.
        offsets.stamp = u32::MAX;
        offsets.begin(ids.len());

        assert_eq!(
            offsets.y_or(ids[2], 17.0),
            17.0,
            "a slot no collection has written must keep Taffy's y across the wrap"
        );
        assert_eq!(
            offsets.y_or(ids[1], 17.0),
            17.0,
            "and so must one an earlier collection wrote and this one did not"
        );
    }

    /// The ordinary case the wrap arm must not disturb: what the collection in
    /// progress wrote is read back, and what an earlier one wrote is not.
    #[test]
    fn a_collection_reads_back_its_own_entries_and_no_earlier_ones() {
        let (_arena, ids) = four_nodes();
        let mut offsets = BaselineOffsets::default();
        offsets.begin(ids.len());
        offsets.insert(ids[1], 12.0);
        assert_eq!(offsets.y_or(ids[1], 17.0), 12.0);

        offsets.begin(ids.len());
        assert_eq!(
            offsets.y_or(ids[1], 17.0),
            17.0,
            "the previous collection's entry must read as absent"
        );
        offsets.insert(ids[3], 5.0);
        assert_eq!(offsets.y_or(ids[3], 17.0), 5.0);
    }

    /// **A solver with no typesetter sizes no table**, which is the property the
    /// early return in `baseline_pass` exists for: `TaffySolver::new()` is what
    /// `dashscene-desktop` builds and what the per-frame band's own fixture
    /// runs, and sizing there would be 8 bytes per document node at every
    /// rebuild for a lookup that can only miss.
    ///
    /// Reaching into the solver's own state rather than asserting on the rects,
    /// because there is nothing to see in the rects: the answer is the same
    /// either way and only the allocation differs. Every other test in the
    /// workspace is blind to it, the per-frame band included — that one measures
    /// steady-state frames, where the table is already sized.
    ///
    /// **Two things hold the property and this fails only when both go.**
    /// `begin` sits below the early return, and the return calls `forget`
    /// first, so moving `begin` above it costs a transient allocate-and-drop at
    /// rebuild rather than a retained table, and this still passes. Remove
    /// `forget` as well and it fails. That is the intended order: `forget`
    /// makes the placement an optimisation rather than the invariant.
    #[test]
    fn a_solver_with_no_typesetter_sizes_no_baseline_table() {
        use crate::TaffySolver;
        use dashscene_core::{CrossAxisAlign, LayoutMode, Prop};

        let mut arena = Arena::new();
        let mut solver = TaffySolver::new();
        {
            let mut txn = arena.open();
            // The shape that WOULD record corrections, so the emptiness below is
            // the missing typesetter and not a scene with nothing to correct.
            let row = txn.add_node(None, None);
            txn.set_prop(row, Prop::Mode(LayoutMode::Horizontal));
            txn.set_prop(row, Prop::CrossAlign(CrossAxisAlign::Baseline));
            let text = txn.add_node(Some(row), None);
            txn.set_prop(text, Prop::Text("LARGE".to_string()));
            txn.commit_with(&mut solver);
        }

        let state = solver
            .state
            .as_ref()
            .expect("the solve built a retained tree");
        assert_eq!(
            state.baseline_offsets.slots.capacity(),
            0,
            "a solver with no typesetter corrects nothing, so it must size no table"
        );
    }

    /// A node past the end answers with Taffy's y rather than panicking — the
    /// state a solver with no typesetter is in for its whole life, since
    /// nothing ever sizes its table.
    #[test]
    fn an_unsized_table_answers_every_node_with_taffys_y() {
        let (_arena, ids) = four_nodes();
        let offsets = BaselineOffsets::default();
        assert_eq!(offsets.y_or(ids[0], 17.0), 17.0);
        assert_eq!(
            offsets.slots.capacity(),
            0,
            "and it allocated nothing to do it — capacity rather than length, because a \
             reserved-then-cleared table is empty and has still allocated"
        );
    }
}

#[cfg(test)]
mod atlas_build_tests {
    use super::{AtlasBytes, TextResourcesError, atlas_from_bytes};
    use dashscene_typeset::atlas::AtlasMetrics;

    /// `Atlas::new` refuses `px_per_em == 0` (issue #724), because every
    /// painter divides by it. On this path that is a **host data** error
    /// rather than a broken contract between crates, so it must arrive as
    /// `TextResourcesError::Atlas` and not as an unwrap.
    ///
    /// A unit test rather than one beside the others in `tests/`, because
    /// `atlas_from_bytes` is private: an integration test reaches it only
    /// through `from_faces`, which would need a parseable face beside the
    /// malformed sheet to say anything about the sheet at all.
    #[test]
    fn an_atlas_rendered_at_zero_texels_per_em_is_refused() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/atlas/inter-ascii"
        );
        let png = std::fs::read(format!("{dir}/atlas.png")).expect("the committed sheet");
        let raw = std::fs::read(format!("{dir}/atlas.metrics")).expect("its metrics");

        // A real sheet with one field zeroed, so the PNG signature and the
        // extent check both still pass and this reaches `Atlas::new`.
        let mut metrics = AtlasMetrics::from_bytes(&raw).expect("the committed metrics decode");
        metrics.atlas.px_per_em = 0;

        let error = atlas_from_bytes(
            AtlasBytes {
                png,
                metrics: metrics.to_bytes(),
            },
            0,
        )
        .expect_err("an atlas with no scale to map its distances through is refused");
        assert!(
            matches!(error, TextResourcesError::Atlas { index: 0, .. }),
            "reported as a host atlas error naming its face: {error:?}"
        );
    }
}

#[cfg(test)]
mod stager_tests {
    use super::{TextAlignV, vertical_offset};

    /// Story #310: the stager offsets a text block within its node box by the
    /// vertical alignment. `Top` sits at the origin; `Center` and `Bottom`
    /// distribute the box's free space (100 − 40 = 60).
    #[test]
    fn vertical_offset_places_the_block_within_the_box() {
        assert_eq!(vertical_offset(100.0, 40.0, TextAlignV::Top), 0.0);
        assert_eq!(vertical_offset(100.0, 40.0, TextAlignV::Center), 30.0);
        assert_eq!(vertical_offset(100.0, 40.0, TextAlignV::Bottom), 60.0);
    }

    /// Text taller than its box overflows rather than clamping, the same way
    /// horizontal placement already allows overflow: the offset goes negative.
    #[test]
    fn an_overflowing_block_offsets_negative() {
        assert_eq!(vertical_offset(40.0, 100.0, TextAlignV::Center), -30.0);
        assert_eq!(vertical_offset(40.0, 100.0, TextAlignV::Bottom), -60.0);
    }
}

/// The `taffy_of` slot value that means "`build` has not written this
/// node yet" (issue #200). Taffy hands out node ids from a slot map, so
/// `u64::MAX` names no node any tree can allocate. An unwritten slot is
/// caught by the `debug_assert` in [`rebuild`]; with assertions off it
/// aborts at the first Taffy read instead, which is still a stop rather
/// than the silent wrong answer the old zero placeholder gave — zero is
/// the id of the first node built, so an unwritten slot read that node's
/// rect and reported it as another node's.
const UNBUILT: taffy::NodeId = taffy::NodeId::new(u64::MAX);

/// Build the whole tree from scratch, solve it, and report every node —
/// the first solve, or one after a structural change (issue #164).
fn rebuild(
    typesetter: Option<&mut Typesetter>,
    arena: &Arena,
    solves: &mut u64,
) -> (TreeState, Vec<(NodeId, SolvedRect)>) {
    let n = arena.node_count();
    let mut tree: TaffyTree<TextContext> = TaffyTree::new();
    // R7: the committed table is an f32 passthrough of the solve —
    // Taffy's default whole-pixel rounding is off.
    tree.disable_rounding();

    // A sentinel for every slot; `build` overwrites each, since every node
    // is reachable from a root (issue #200, and the `debug_assert` below
    // says so out loud).
    let mut taffy_of: Vec<taffy::NodeId> = vec![UNBUILT; n];
    let mut parent_of: Vec<Option<NodeId>> = vec![None; n];
    let mut roots = Vec::with_capacity(arena.roots().len());
    for &root in arena.roots() {
        let taffy_root = build(
            &mut tree,
            &mut taffy_of,
            &mut parent_of,
            arena,
            root,
            None,
            None,
        );
        roots.push(taffy_root);
    }
    // The tripwire itself: name the structural bug where it happened,
    // rather than at whichever later read hits the unwritten slot.
    debug_assert!(
        !taffy_of.contains(&UNBUILT),
        "every arena node is reachable from a root, so `build` must write every taffy_of slot"
    );

    // The tree is built over **every** root above and computed over the shown
    // ones here. Keeping the whole tree is what makes a later change of shown
    // root cheap — no rebuild, because every root's nodes are already in it —
    // and computing only the shown ones is the per-frame cost story #836
    // measured at one Taffy computation per root (story #838).
    let shown_taffy = shown_taffy_roots(arena, &roots);
    let typesetter = compute_all(&mut tree, shown_taffy, typesetter, solves);

    // #272 baseline correction: re-place text children of baseline rows on
    // their glyph baseline, and (#322) re-solve when that placement needs a
    // HUG row to be taller than Taffy made it. Needs the typesetter —
    // without it text nodes measure to zero and there is no baseline to
    // correct.
    let mut baseline_floors = Vec::new();
    // Declared here rather than with the rest of `TreeState` below, because the
    // pass that fills it runs before that struct exists. Empty until that pass
    // sizes it, and it never does on a solver with no typesetter.
    let mut baseline_offsets = BaselineOffsets::default();
    baseline_pass(
        &mut tree,
        &taffy_of,
        shown_taffy,
        arena,
        typesetter,
        &mut baseline_floors,
        &mut baseline_offsets,
        solves,
    );

    let mut prev_rel = vec![[0u32; 4]; n];
    // One entry per **arena** root, in arena root order, so a root's slot does
    // not move when the shown root changes. The readback below skips the
    // unshown ones; their origins stay as recorded and are re-read when they
    // are shown again.
    let mut prev_root_origin = Vec::with_capacity(arena.roots().len());
    let mut out = Vec::new();
    for &root in arena.roots() {
        let origin = arena.layout(root);
        prev_root_origin.push([origin.x.to_bits(), origin.y.to_bits()]);
    }
    for &root in arena.shown_roots() {
        // Roots are their own coordinate islands: the subtree translates
        // by the root's authored offset.
        let origin = arena.layout(root);
        read_back_full(
            &tree,
            &taffy_of,
            &mut prev_rel,
            &baseline_offsets,
            arena,
            root,
            (origin.x, origin.y),
            &mut out,
        );
    }

    let state = TreeState {
        tree,
        baseline_offsets,
        // One stamp per node, allocated with the tree and reused by every
        // solve after it. Zero is "never marked", and the first solve stamps
        // with 1 (issue #1111).
        on_path: vec![0; n],
        pass: 0,
        taffy_of,
        parent_of,
        roots,
        prev_rel,
        prev_root_origin,
        baseline_floors,
        node_count: n,
        shown: arena.shown_root(),
    };
    (state, out)
}

/// Re-solve only what the change forced: restyle the nodes whose layout
/// intent changed (and their children, for a mode change), recompute the
/// dirtied subtrees, and read back only the rects that moved (issue #164).
fn incremental(
    state: &mut TreeState,
    typesetter: Option<&mut Typesetter>,
    arena: &Arena,
    solves: &mut u64,
) -> Vec<(NodeId, SolvedRect)> {
    // A change of shown root is not an ordinary frame. The newly shown root's
    // subtree has never been reported through this solver, so `commit_with`
    // would find no rect for any of its nodes and refuse the commit — and the
    // pruned readback cannot supply them, because nothing about those nodes
    // *moved*. Report the whole of the new subtree, the way the first solve
    // does, and leave the retained tree alone: it already holds every root
    // (story #838).
    let shown_changed = state.shown != arena.shown_root();

    // The nodes whose layout intent changed since the last commit.
    let dirty: FxHashSet<NodeId> = arena.layout_dirty().iter().copied().collect();
    // The paint-only fast path: nothing to solve. A root move is layout
    // intent (an X/Y change), so an empty set means no geometry changed
    // anywhere, and every rect carries forward unchanged. A shown-root change
    // is not covered by it: no layout intent changed, and the answer is still
    // a different subtree.
    if dirty.is_empty() && !shown_changed {
        return Vec::new();
    }

    // Restyle each dirty node. A node's child-side style depends on its
    // parent's mode, so a mode change on the parent restyles its children
    // too; recompute them unconditionally (bounded by the change).
    for &node in &dirty {
        let taffy_node = state.taffy_of[node.index()];
        let parent_layout = state.parent_of[node.index()].map(|p| arena.layout(p));
        let node_layout = arena.layout(node);
        state
            .tree
            .set_style(
                taffy_node,
                style_for(
                    &node_layout,
                    arena.grid_tracks(node),
                    parent_layout.as_ref(),
                ),
            )
            .expect("restyling a retained node cannot fail");
        state
            .tree
            .set_node_context(taffy_node, text_context(arena, node))
            .expect("setting a retained node's context cannot fail");
        for &child in arena.children(node) {
            let taffy_child = state.taffy_of[child.index()];
            state
                .tree
                .set_style(
                    taffy_child,
                    style_for(
                        &arena.layout(child),
                        arena.grid_tracks(child),
                        Some(&node_layout),
                    ),
                )
                .expect("restyling a retained child cannot fail");
        }
    }

    // Borrowed, not cloned. `shown_taffy_roots` hands back a subslice of
    // `state.roots`, and everything below it mutates `state.tree` — a
    // *different* field, so the borrow checker takes both at once and the
    // clone the comment here used to justify was never needed. It was 8 bytes
    // per root in the document on every frame, which the per-frame band reads
    // as document-scaled cost whatever the shown root is (issue #1111).
    let shown_taffy = shown_taffy_roots(arena, &state.roots);
    let typesetter = compute_all(&mut state.tree, shown_taffy, typesetter, solves);

    // #272 baseline correction (see `rebuild`): re-place a baseline row's text
    // children on their glyph baseline. The corrected y is folded into
    // `rel_bits`, so the pruned read-back re-emits a child whose baseline shift
    // moved it — including when a sibling changed the tallest baseline.
    baseline_pass(
        &mut state.tree,
        &state.taffy_of,
        shown_taffy,
        arena,
        typesetter,
        &mut state.baseline_floors,
        &mut state.baseline_offsets,
        solves,
    );

    // A subtree can hold a changed node without moving at its own root
    // (a fixed-size frame with a shifted child): mark every dirty node and
    // its ancestors so the readback descends to reach them, on top of
    // descending wherever a rect actually moved.
    //
    // Stamped, not allocated. The vector is retained on the state and each
    // solve marks with a fresh stamp, so the previous solve's marks read as
    // absent and nothing is cleared or allocated per frame (issue #1111). The
    // `break` still stops each walk at the first already-marked ancestor, so
    // the work is the size of the dirty closure rather than the sum of the
    // chains — the vector's size is the document's, but it is paid once at
    // rebuild and never again.
    state.pass = state.pass.wrapping_add(1);
    let pass = state.pass;
    for &node in &dirty {
        let mut cursor = Some(node);
        while let Some(current) = cursor {
            if state.on_path[current.index()] == pass {
                break;
            }
            state.on_path[current.index()] = pass;
            cursor = state.parent_of[current.index()];
        }
    }

    let mut out = Vec::new();
    for (root_i, &root) in arena.roots().iter().enumerate() {
        // By identity: the shown root is a node of this arena, so "is this root
        // shown" is one comparison against the root in hand — no scan and no
        // index (issue #943). `shown_taffy_roots` below answers a different
        // question — *where* in the parallel Taffy list the shown root sits —
        // and pays a scan for it, because only a position can index that list.
        let shown = arena.shown_root().is_none_or(|shown| shown == root);
        if !shown {
            continue;
        }
        let origin = arena.layout(root);
        let cur_origin = [origin.x.to_bits(), origin.y.to_bits()];
        let root_moved = state.prev_root_origin[root_i] != cur_origin;
        state.prev_root_origin[root_i] = cur_origin;
        if shown_changed {
            // Every node of the newly shown subtree, not the ones that moved:
            // "moved" is measured against `prev_rel`, and a subtree nothing has
            // reported has no previous rect for the commit to carry forward.
            read_back_full(
                &state.tree,
                &state.taffy_of,
                &mut state.prev_rel,
                &state.baseline_offsets,
                arena,
                root,
                (origin.x, origin.y),
                &mut out,
            );
        } else {
            read_back_pruned(
                &state.tree,
                &state.taffy_of,
                &mut state.prev_rel,
                &state.baseline_offsets,
                &state.on_path,
                pass,
                arena,
                root,
                (origin.x, origin.y),
                root_moved,
                &mut out,
            );
        }
    }
    state.shown = arena.shown_root();
    out
}

/// The Taffy roots standing for the arena's **shown** roots, in arena root
/// order.
///
/// `taffy_roots` is one entry per arena root, in that order, so this is the
/// same positional split `Arena::shown_roots` makes over the arena's own list —
/// written twice because the two lists hold different types, and kept in the
/// one shape so the solve and the readback cannot disagree about it. A slice
/// rather than a `Vec`: the answer is always a contiguous run of `taffy_roots`,
/// either all of it or one element.
///
/// The shown root is a [`NodeId`], so its position is looked up in
/// `arena.roots()` — the list `taffy_roots` is parallel to — rather than used as
/// an index directly. A node that is no root of this arena selects nothing, the
/// same answer `Arena::shown_roots` gives it (issue #943).
fn shown_taffy_roots<'t>(arena: &Arena, taffy_roots: &'t [taffy::NodeId]) -> &'t [taffy::NodeId] {
    match arena.shown_root() {
        None => taffy_roots,
        Some(shown) => match arena.roots().iter().position(|&root| root == shown) {
            Some(at) => taffy_roots.get(at..=at).unwrap_or(&[]),
            None => &[],
        },
    }
}

/// Compute the layout of every root, counting each as one solve. Text
/// nodes size to their shaped runs; the typesetter is reborrowed per root
/// so its one shaped-run cache serves every root (and, at #30, the
/// painter). A solver with no typesetter measures every text node to zero
/// — the same result Taffy's default (no-op) measure gives.
/// Returns the typesetter it was lent, so the caller can run the #272
/// baseline-correction pass over the freshly solved tree without reborrowing.
fn compute_all<'t>(
    tree: &mut TaffyTree<TextContext>,
    roots: &[taffy::NodeId],
    mut typesetter: Option<&'t mut Typesetter>,
    solves: &mut u64,
) -> Option<&'t mut Typesetter> {
    for &taffy_root in roots {
        tree.compute_layout_with_measure(
            taffy_root,
            Size::MAX_CONTENT,
            |known, available, _node, context, _style| match (context, typesetter.as_deref_mut()) {
                (Some(text), Some(ts)) => measure_text(known, available, text, ts),
                _ => Size::ZERO,
            },
        )
        .expect("taffy tree built from the arena is always valid");
        *solves += 1;
    }
    typesetter
}

/// A text node's measure input, attached to its Taffy leaf: the
/// paragraph text and the render size (px per em in document units).
/// The text is owned so the tree outlives the arena borrow; shaping
/// itself is not repeated, because `measure_text` reads the
/// typesetter's shaped-run cache
/// (`docs/decisions/shaped-run-cache-font-units.md`).
#[derive(Debug)]
struct TextContext {
    text: String,
    size: f32,
    /// The measure-affecting shaping axes (fixed line height, letter
    /// spacing, horizontal align, standard-ligatures-off) from the node's
    /// `TextStyle` (story #327, #341). Vertical alignment is not here: it
    /// is block placement, not a measured extent, so it lives in the
    /// stager, not the solve.
    shape: TextShape,
    /// The node's CSS-scale weight (story #368). Weight is a measure input,
    /// not only a paint one: a heavier face has its own advances, so a bold
    /// run measured at Regular's advances would size a box the text then
    /// overflows. The typesetter resolves it to one of its faces; a cascade
    /// offering only weight 400 resolves every request there and the box is
    /// measured exactly as before this field existed.
    weight: u16,
    /// The node's font family (story #385). A measure input for the same
    /// reason weight is, and a stronger one: a different family has its
    /// own advances, its own kerning and its own metrics, so a run
    /// measured in a family the document did not ask for sizes a box the
    /// text does not fit. A cascade that declares no names, or one with no
    /// family answering to this one, resolves by coverage and measures
    /// exactly as before this field existed.
    family: String,
}

/// The measure context for a node, present only when the node carries
/// both text content and a text style — the well-formed text node. A
/// node missing either is a plain leaf, not a measured text node (a
/// text node with no style has no size to shape at).
fn text_context(arena: &Arena, node: NodeId) -> Option<TextContext> {
    let text = arena.text(node)?;
    let style = arena.text_style(node)?;
    Some(TextContext {
        text: text.to_string(),
        size: style.size,
        shape: text_shape(style),
        weight: style.weight,
        family: style.family.clone(),
    })
}

/// The measure-affecting shaping axes of a node's text style (story #327,
/// #341): a fixed line height, letter spacing, horizontal alignment, and
/// standard-ligatures-off (a ligated glyph's advance generally differs from
/// its component glyphs' combined advance, so it can move the measured
/// extent). Vertical alignment is placement (the stager), not a measured
/// extent, so it is not carried here. A default-axis style maps to
/// [`TextShape::default`], so the solve stays byte-identical to the
/// pre-#327 `layout()` path (the E7 guard).
fn text_shape(style: &dashscene_core::TextStyle) -> TextShape {
    TextShape {
        line_height_px: style.line_height_px,
        letter_spacing: style.letter_spacing,
        align: match style.text_align {
            dashscene_core::TextAlign::Left => dashscene_typeset::text::TextAlign::Left,
            dashscene_core::TextAlign::Center => dashscene_typeset::text::TextAlign::Center,
            dashscene_core::TextAlign::Right => dashscene_typeset::text::TextAlign::Right,
        },
        ligatures_off: style.ligatures_off,
    }
}

/// Measure a text node against the shaped-run cache. `known` is what
/// Taffy has already fixed for the node; `available` is the space it
/// offers. The wrap width is the fixed width if Taffy set one, else a
/// definite available width, else probe-dependent: a max-content probe
/// imposes no wrap, so the paragraph lays out on one line and the node
/// hugs its natural width; a min-content probe measures at wrap width
/// zero, which the greedy breaker turns into one word per line — width
/// = the widest word, the box wrappable text can never shrink below
/// (debt #177). A known axis is returned unchanged; only an unfixed
/// axis takes the shaped measurement.
fn measure_text(
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    context: &TextContext,
    typesetter: &mut Typesetter,
) -> Size<f32> {
    let max_width = known.width.or(match available.width {
        AvailableSpace::Definite(width) => Some(width),
        AvailableSpace::MinContent => Some(0.0),
        AvailableSpace::MaxContent => None,
    });
    let laid = typesetter.layout_styled(
        &context.text,
        context.size,
        max_width,
        context.shape,
        context.weight,
        &context.family,
    );
    Size {
        width: known.width.unwrap_or(laid.width),
        height: known.height.unwrap_or(laid.height),
    }
}

/// Build the Taffy subtree for `node`; record its Taffy id and parent.
fn build(
    tree: &mut TaffyTree<TextContext>,
    taffy_of: &mut [taffy::NodeId],
    parent_of: &mut [Option<NodeId>],
    arena: &Arena,
    node: NodeId,
    parent: Option<&Layout>,
    parent_id: Option<NodeId>,
) -> taffy::NodeId {
    let layout = arena.layout(node);
    let style = style_for(&layout, arena.grid_tracks(node), parent);
    // A text node carries a measure context so Taffy sizes it from its
    // shaped runs; every other node is a plain leaf whose measure is a
    // no-op.
    let taffy_node = match text_context(arena, node) {
        Some(context) => tree.new_leaf_with_context(style, context),
        None => tree.new_leaf(style),
    }
    .expect("taffy node allocation cannot fail");
    taffy_of[node.index()] = taffy_node;
    parent_of[node.index()] = parent_id;
    for &child in arena.children(node) {
        let taffy_child = build(
            tree,
            taffy_of,
            parent_of,
            arena,
            child,
            Some(&layout),
            Some(node),
        );
        tree.add_child(taffy_node, taffy_child)
            .expect("taffy child insertion cannot fail");
    }
    taffy_node
}

/// Map one node's layout intent to a Taffy style, in the context of
/// its parent's layout (child sizing is axis-relative). `tracks` is the
/// node's grid track lists (rows, columns) — meaningful when its mode
/// is `Grid`, empty otherwise.
fn style_for(
    layout: &Layout,
    tracks: (&[GridTrack], &[GridTrack]),
    parent: Option<&Layout>,
) -> Style {
    let mut style = Style::default();

    // The authored gaps, axis-split (v0.8, story #43): `gap` is the
    // main-axis spacing — horizontal for every mode but `Vertical` —
    // and `cross_gap` the other axis's, following `gap` when unset
    // (the v0.2 both-axes mapping, unchanged for old documents). The
    // cross half is inert without wrap lines or grid rows.
    let cross_gap = layout.cross_gap.unwrap_or(layout.gap);
    let gap = if layout.mode == LayoutMode::Vertical {
        Size {
            width: length(cross_gap),
            height: length(layout.gap),
        }
    } else {
        Size {
            width: length(layout.gap),
            height: length(cross_gap),
        }
    };
    let padding = Rect {
        left: length(layout.padding.left),
        top: length(layout.padding.top),
        right: length(layout.padding.right),
        bottom: length(layout.padding.bottom),
    };

    // Container side: how this node lays out its own children.
    match layout.mode {
        LayoutMode::None => {
            // Children are absolutely positioned by their authored
            // offsets (the passthrough); Block is the inert display.
            style.display = Display::Block;
        }
        LayoutMode::Horizontal | LayoutMode::Vertical | LayoutMode::Wrap => {
            style.display = Display::Flex;
            style.flex_direction = if layout.mode == LayoutMode::Vertical {
                FlexDirection::Column
            } else {
                // Wrap is a horizontal wrapping row (story #43).
                FlexDirection::Row
            };
            if layout.mode == LayoutMode::Wrap {
                style.flex_wrap = FlexWrap::Wrap;
                // Figma packs wrap lines at the cross start; taffy's
                // default (None = stretch) would move lines in a
                // fixed-height container.
                style.align_content = Some(AlignContent::FLEX_START);
            }
            style.gap = gap;
            style.padding = padding;
            style.justify_content = Some(match layout.main_align {
                dashscene_core::MainAxisAlign::Start => JustifyContent::FLEX_START,
                dashscene_core::MainAxisAlign::Center => JustifyContent::CENTER,
                dashscene_core::MainAxisAlign::End => JustifyContent::FLEX_END,
                dashscene_core::MainAxisAlign::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            });
            // Never Stretch at the container level: Fill children opt
            // into stretching via align_self; Fixed/Hug children keep
            // their own cross size under any alignment. Baseline (Q-4)
            // aligns a row's children on their flex baselines — a
            // leaf's baseline is its bottom edge, a nested row
            // propagates its first line's — and degrades to start in a
            // column (taffy computes baselines for rows only). A text
            // leaf's box-bottom baseline is corrected to its glyph
            // baseline after the solve (#272, `collect_baseline_offsets`).
            style.align_items = Some(match layout.cross_align {
                dashscene_core::CrossAxisAlign::Start => AlignItems::FLEX_START,
                dashscene_core::CrossAxisAlign::Center => AlignItems::CENTER,
                dashscene_core::CrossAxisAlign::End => AlignItems::FLEX_END,
                dashscene_core::CrossAxisAlign::Baseline => AlignItems::BASELINE,
            });
        }
        LayoutMode::Grid => {
            style.display = Display::Grid;
            style.grid_template_rows = tracks.0.iter().map(template_track).collect();
            style.grid_template_columns = tracks.1.iter().map(template_track).collect();
            style.gap = gap;
            style.padding = padding;
            // main_align/cross_align are not mapped here: grid children
            // place by cell, and their in-cell alignment comes from
            // their own sizing (the child side below).
        }
    }

    // Child side: how this node sizes within its parent.
    let dimension = |sizing: AxisSizing, size: f32| match sizing {
        AxisSizing::Fixed => Dimension::length(size),
        // Fill's main-axis growth is expressed via flex_basis/grow
        // below; its size stays auto on both axes.
        AxisSizing::Hug | AxisSizing::Fill => Dimension::AUTO,
    };
    style.size = Size {
        width: dimension(layout.sizing_h, layout.width),
        height: dimension(layout.sizing_v, layout.height),
    };
    let bound = |v: Option<f32>| v.map_or(Dimension::AUTO, Dimension::length);
    style.min_size = Size {
        width: bound(layout.min_width),
        height: bound(layout.min_height),
    };
    style.max_size = Size {
        width: bound(layout.max_width),
        height: bound(layout.max_height),
    };

    match parent.map(|p| p.mode) {
        // Root: nothing more to map (location handled at readback).
        // Margin is flex-flow vocabulary with no meaning here, and
        // Taffy ignores a root's own margin regardless.
        None => {}
        // Passthrough parent: place by the authored offset. Fill has
        // no free-space axis under a None parent and behaves as Hug
        // (the validator diagnoses it at its own slice, P4). Margin is
        // inert — placement is the authored offset, matching
        // `commit()`'s fixed resolution, which ignores margin.
        Some(LayoutMode::None) => {
            style.position = Position::Absolute;
            style.inset = Rect {
                left: LengthPercentageAuto::length(layout.x),
                top: LengthPercentageAuto::length(layout.y),
                right: LengthPercentageAuto::AUTO,
                bottom: LengthPercentageAuto::AUTO,
            };
        }
        Some(mode @ (LayoutMode::Horizontal | LayoutMode::Vertical | LayoutMode::Wrap)) => {
            // Outer margin applies only in flex flow (negative allowed
            // — it expresses overlap, the target of the negative-gap
            // lowering).
            style.margin = Rect {
                left: LengthPercentageAuto::length(layout.margin.left),
                top: LengthPercentageAuto::length(layout.margin.top),
                right: LengthPercentageAuto::length(layout.margin.right),
                bottom: LengthPercentageAuto::length(layout.margin.bottom),
            };
            // Axis-relative sizing: the parent's main axis maps to
            // flex_basis/grow/shrink; the cross axis maps to size (set
            // above) and align_self. Wrap flows horizontally, so its
            // main axis is Horizontal's.
            let (main_sizing, main_size, cross_sizing) = if mode == LayoutMode::Vertical {
                (layout.sizing_v, layout.height, layout.sizing_h)
            } else {
                (layout.sizing_h, layout.width, layout.sizing_v)
            };
            match main_sizing {
                AxisSizing::Fixed => {
                    // Debt #236: taffy 0.12's intrinsic (hug) pass divides a
                    // shrink-0 item's negative contribution diff by
                    // `max(1, shrink * inner_basis)` (= 1) but multiplies it
                    // back by `max(1, shrink) * inner_basis`, so a negative
                    // main-axis margin is amplified by the item's inner flex
                    // basis and the hug sum collapses. Rebate the negative
                    // margin into the basis — the contribution (clamped size
                    // + margins) then equals the basis, the diff is zero, and
                    // the broken reconstruction is never entered. Taffy
                    // floors a basis at the item's own padding sum (review
                    // finding R2), so a rebate below that floor anchors at
                    // padding + 1 instead: the inner flex basis is then
                    // exactly 1, where the branch's two scaled-shrink
                    // formulas agree, and the reconstruction stays exact for
                    // any overlap depth (R3). A min-size floor at the
                    // authored size — clamped by an authored max (R1), maxed
                    // with an authored min — restores the real size in the
                    // definite pass, so positions and sizes are unchanged.
                    // Positive margins take the diff > 0 path, whose two
                    // formulas agree, and need no rebate. Full arithmetic:
                    // docs/decisions/negative-margin-hug-rebate.md.
                    let (margin_sum, authored_min, authored_max) = if mode == LayoutMode::Vertical {
                        (
                            layout.margin.top + layout.margin.bottom,
                            layout.min_height,
                            layout.max_height,
                        )
                    } else {
                        (
                            layout.margin.left + layout.margin.right,
                            layout.min_width,
                            layout.max_width,
                        )
                    };
                    if margin_sum < 0.0 {
                        // The padding taffy sees: style_for maps authored
                        // padding for container modes only, and there is no
                        // border vocabulary.
                        let padding_sum = if layout.mode == LayoutMode::None {
                            0.0
                        } else if mode == LayoutMode::Vertical {
                            layout.padding.top + layout.padding.bottom
                        } else {
                            layout.padding.left + layout.padding.right
                        };
                        let rebated = main_size + margin_sum;
                        style.flex_basis = Dimension::length(if rebated >= padding_sum {
                            rebated
                        } else {
                            padding_sum + 1.0
                        });
                        let clamped = authored_max.map_or(main_size, |m| main_size.min(m));
                        let floor = authored_min.map_or(clamped, |m| m.max(clamped));
                        let min = if mode == LayoutMode::Vertical {
                            &mut style.min_size.height
                        } else {
                            &mut style.min_size.width
                        };
                        *min = Dimension::length(floor);
                    } else {
                        style.flex_basis = Dimension::length(main_size);
                    }
                    style.flex_grow = 0.0;
                    style.flex_shrink = 0.0;
                }
                AxisSizing::Hug => {
                    style.flex_basis = Dimension::AUTO;
                    style.flex_grow = 0.0;
                    // Issue #270, the residual the `Fixed` rebate above could
                    // not cover: a `Hug` child's flex basis is content-derived,
                    // so there is no authored size to rebate a negative margin
                    // into. The same taffy 0.12 branch amplifies it — for a
                    // shrink-0 item the branch divides the negative diff by
                    // `max(1, 0 * inner_basis)` = 1 and multiplies it back by
                    // `max(1, 0) * inner_basis`, so the item contributes
                    // `basis + inner_basis * margin_sum` instead of
                    // `basis + margin_sum`. At `flex_shrink = 1` the same two
                    // expressions are `max(1, inner_basis)` and `inner_basis`,
                    // which agree for any inner basis of 1 or more, so the
                    // contribution is exact. The switch is confined to the
                    // broken pass: it applies only when the parent hugs this
                    // axis, and a hug parent is sized to its content sum, so
                    // the definite pass has no negative free space for a
                    // shrink factor to act on.
                    let margin_sum = if mode == LayoutMode::Vertical {
                        layout.margin.top + layout.margin.bottom
                    } else {
                        layout.margin.left + layout.margin.right
                    };
                    let parent_hugs_main = parent.is_some_and(|p| {
                        let parent_main_sizing = if mode == LayoutMode::Vertical {
                            p.sizing_v
                        } else {
                            p.sizing_h
                        };
                        parent_main_sizing == AxisSizing::Hug
                    });
                    style.flex_shrink = if margin_sum < 0.0 && parent_hugs_main {
                        1.0
                    } else {
                        0.0
                    };
                }
                AxisSizing::Fill => {
                    style.flex_basis = Dimension::length(0.0);
                    style.flex_grow = 1.0;
                    style.flex_shrink = 1.0;
                }
            }
            if cross_sizing == AxisSizing::Fill {
                style.align_self = Some(AlignSelf::STRETCH);
            }
        }
        Some(LayoutMode::Grid) => {
            // Margin applies inside the cell, like flex flow.
            style.margin = Rect {
                left: LengthPercentageAuto::length(layout.margin.left),
                top: LengthPercentageAuto::length(layout.margin.top),
                right: LengthPercentageAuto::length(layout.margin.right),
                bottom: LengthPercentageAuto::length(layout.margin.bottom),
            };
            // Placement: the 0-based anchor becomes taffy's 1-based
            // start line; the end is always the span (default 1). An
            // absent anchor auto-places in document order. The schema's
            // anchors are ushort and taffy's lines are i16, so the
            // conversion saturates — never a wrap to an end-counted
            // line, never a debug overflow (review finding R5); a span
            // of 0 floors at 1 (R6). The load gate bounds both for
            // documents; this is the engine's own hardening for direct
            // producers.
            let placement = |anchor: Option<u16>, span_tracks: u16| taffy::Line {
                start: anchor.map_or(GridPlacement::Auto, |a| {
                    line(i16::try_from(i32::from(a) + 1).unwrap_or(i16::MAX))
                }),
                end: span(span_tracks.max(1)),
            };
            style.grid_row = placement(layout.grid_row, layout.grid_row_span);
            style.grid_column = placement(layout.grid_column, layout.grid_column_span);
            // In-cell alignment comes from the sizing intent: Fill
            // stretches over the cell area, Fixed and Hug keep their
            // own size at the cell origin (what the captured grid
            // shows — taffy's default would stretch a hug child).
            let alignment = |sizing: AxisSizing| {
                Some(if sizing == AxisSizing::Fill {
                    AlignSelf::STRETCH
                } else {
                    AlignSelf::START
                })
            };
            style.justify_self = alignment(layout.sizing_h);
            style.align_self = alignment(layout.sizing_v);
        }
    }

    // Overrides both sides above: Taffy's Display::None hides the node
    // from its parent's flow and hides its whole subtree regardless of
    // any descendant's own style (issue #165).
    if !layout.visible {
        style.display = Display::None;
    }

    style
}

/// One authored grid track as a taffy template component. `Fixed` is a
/// document-unit length; `Fraction` is Figma's `minmax(0, Nfr)` — the
/// zero minimum, not `fr`'s implied min-content one, so a fraction
/// track divides exactly the free space the captured grid divides.
fn template_track(track: &GridTrack) -> taffy::GridTemplateComponent<String> {
    match *track {
        GridTrack::Fixed(v) => length(v),
        GridTrack::Fraction(weight) => minmax(length(0.0_f32), fr(weight)),
    }
}

/// Emit every node's absolute rect and record its relative layout — the
/// full readback a rebuild uses (issue #164).
#[allow(clippy::too_many_arguments)]
fn read_back_full(
    tree: &TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    prev_rel: &mut [[u32; 4]],
    baseline_offsets: &BaselineOffsets,
    arena: &Arena,
    node: NodeId,
    parent_origin: (f32, f32),
    out: &mut Vec<(NodeId, SolvedRect)>,
) {
    let layout = tree
        .layout(taffy_of[node.index()])
        .expect("layout was computed for the whole tree");
    // A baseline-corrected child (#272) overrides Taffy's cross-axis offset.
    let local_y = baseline_offsets.y_or(node, layout.location.y);
    let x = parent_origin.0 + layout.location.x;
    let y = parent_origin.1 + local_y;
    prev_rel[node.index()] = rel_bits(
        layout.location.x,
        local_y,
        layout.size.width,
        layout.size.height,
    );
    out.push((
        node,
        SolvedRect {
            x,
            y,
            w: layout.size.width,
            h: layout.size.height,
        },
    ));
    for &child in arena.children(node) {
        read_back_full(
            tree,
            taffy_of,
            prev_rel,
            baseline_offsets,
            arena,
            child,
            (x, y),
            out,
        );
    }
}

/// Emit only the rects that moved or resized since the previous solve,
/// pruning subtrees that neither shifted nor hold a changed node
/// (issue #164). `parent_moved` is whether this node's parent origin
/// changed — if so, this node shifts even when its own relative layout
/// did not.
#[allow(clippy::too_many_arguments)]
fn read_back_pruned(
    tree: &TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    prev_rel: &mut [[u32; 4]],
    baseline_offsets: &BaselineOffsets,
    on_path: &[u32],
    pass: u32,
    arena: &Arena,
    node: NodeId,
    parent_origin: (f32, f32),
    parent_moved: bool,
    out: &mut Vec<(NodeId, SolvedRect)>,
) {
    let layout = tree
        .layout(taffy_of[node.index()])
        .expect("layout was computed for the whole tree");
    // A baseline-corrected child (#272) overrides Taffy's cross-axis offset;
    // the corrected `y` is folded into `rel_bits`, so a change to a sibling's
    // baseline shift is detected and re-emitted like any other move.
    let local_y = baseline_offsets.y_or(node, layout.location.y);
    let x = parent_origin.0 + layout.location.x;
    let y = parent_origin.1 + local_y;
    let cur = rel_bits(
        layout.location.x,
        local_y,
        layout.size.width,
        layout.size.height,
    );
    let prev = prev_rel[node.index()];
    prev_rel[node.index()] = cur;

    let rel_changed = cur != prev;
    // The absolute rect changed if the parent shifted or the node's own
    // relative layout (position or size) changed.
    let rect_changed = parent_moved || rel_changed;
    if rect_changed {
        out.push((
            node,
            SolvedRect {
                x,
                y,
                w: layout.size.width,
                h: layout.size.height,
            },
        ));
    }

    // This node's own origin moved if the parent shifted or its relative
    // position changed (a pure resize leaves the origin put).
    let origin_moved = parent_moved || cur[0] != prev[0] || cur[1] != prev[1];
    // Descend when the subtree could hold a change: this node shifted or
    // resized, or it is on the path to a node whose intent changed. A
    // node that neither moved nor guards a dirty descendant has an
    // unchanged subtree, and Taffy left its layouts untouched.
    if rect_changed || on_path[node.index()] == pass {
        for &child in arena.children(node) {
            read_back_pruned(
                tree,
                taffy_of,
                prev_rel,
                baseline_offsets,
                on_path,
                pass,
                arena,
                child,
                (x, y),
                origin_moved,
                out,
            );
        }
    }
}

/// One node's Taffy-relative layout as bit patterns — position and size,
/// the values a readback compares to decide whether a rect moved.
/// The change-detection key for a node's relative layout: its local
/// position and size. `local_y` is the cross-axis offset after the #272
/// baseline correction, so a re-placed baseline child is seen by the
/// incremental read-back exactly like any other move — when a sibling's
/// baseline shift changes the whole row re-emits.
fn rel_bits(local_x: f32, local_y: f32, w: f32, h: f32) -> [u32; 4] {
    [
        local_x.to_bits(),
        local_y.to_bits(),
        w.to_bits(),
        h.to_bits(),
    ]
}

/// #272: after the solve, re-place the children of a baseline row on one
/// glyph baseline. Taffy's high-level measure reports no baseline for a
/// leaf, so Taffy aligns box bottoms (`baseline.unwrap_or(height)`); a text
/// leaf's real first-line baseline — the `line.baseline_y` the typesetter
/// already computes — sits a descender above its box bottom, so a mixed-size
/// row of box-bottom-aligned runs drops the shorter runs too low. This walks
/// the tree and, for every `Horizontal` row whose cross
/// alignment is `Baseline` and that holds at least one text child, records
/// each child's corrected cross-axis (local y): the child sits so its
/// baseline meets the row's baseline line, the content-box top plus the
/// tallest participating baseline. A non-text child keeps the box bottom
/// Taffy uses for it (recomputed to the same place). Rows with no text child,
/// and every other mode or alignment, record nothing at all — an unrecorded
/// node keeps Taffy's y, so a baseline row of plain boxes solves
/// exactly as before. The relation is sparse, and it is nonetheless a slot per
/// node rather than a map: [`BaselineOffsets`] carries why, and what issue #1153
/// measured of the map that stood here between issue #1111 and it.
///
/// The walk visits every node, but only shapes at a baseline text row, which
/// is rare. `baseline_y` is the first line's placed baseline, not a bare font
/// metric: under a fixed line height it is the ascent plus half the leading
/// (the typesetter's half-leading placement, #332), so it must come from
/// `layout_with` under the node's own `text_shape` — the same call the render
/// stager makes — never recomputed from font metrics alone. Laying out at the
/// solved width also keys a wrapped run off its real first line.
///
/// Limitation: a nested container inside a baseline text row is taken by its
/// box bottom, not its own first line's baseline (Taffy's `Layout` does not
/// expose the computed baseline). No corpus scene nests under a text
/// baseline row; the general nested case is tracked as follow-up debt.
fn collect_baseline_offsets(
    tree: &TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    arena: &Arena,
    typesetter: &mut Typesetter,
    node: NodeId,
    offsets: &mut BaselineOffsets,
    floors: &mut Vec<(NodeId, f32)>,
) {
    let layout = arena.layout(node);
    let children = arena.children(node);
    if layout.mode == LayoutMode::Horizontal
        && layout.cross_align == dashscene_core::CrossAxisAlign::Baseline
        && !children.is_empty()
    {
        let mut has_text = false;
        // The children that take part in baseline alignment, as
        // `(child, baseline, cross size)`. A `Fill` cross-sized child is
        // mapped `align_self: STRETCH`, and taffy excludes a stretched item
        // from baseline alignment, so it keeps the place and the size taffy
        // gave it and is not counted here either.
        let mut participants = Vec::with_capacity(children.len());
        for &child in children {
            if arena.layout(child).sizing_v == AxisSizing::Fill {
                continue;
            }
            let child_layout = tree
                .layout(taffy_of[child.index()])
                .expect("layout was computed for the whole tree");
            let baseline = match (arena.text(child), arena.text_style(child)) {
                (Some(text), Some(style)) => {
                    has_text = true;
                    // The #272 baseline pass measures the same run the
                    // measure callback did, so it must resolve the same
                    // face: a bold child's first baseline sits at the bold
                    // face's ascent, not Regular's (story #368), and an
                    // Inter child's at Inter's, not Noto's (story #385).
                    let laid = typesetter.layout_styled(
                        text,
                        style.size,
                        Some(child_layout.size.width),
                        text_shape(style),
                        style.weight,
                        &style.family,
                    );
                    laid.lines
                        .first()
                        .map_or(child_layout.size.height, |line| line.baseline_y)
                }
                // A non-text child keeps Taffy's leaf baseline: the box bottom.
                _ => child_layout.size.height,
            };
            participants.push((child, baseline, child_layout.size.height));
        }
        if has_text {
            let max_baseline = participants
                .iter()
                .map(|&(_, baseline, _)| baseline)
                .fold(f32::NEG_INFINITY, f32::max);
            // The cross extent the re-placed children need, as a border-box
            // size: the lowest re-placed bottom edge plus the bottom padding.
            let mut extent = f32::NEG_INFINITY;
            for &(child, baseline, cross_size) in &participants {
                // Local y within the row's border box: the content-box top
                // plus the gap between this child's baseline and the tallest.
                let local_y = layout.padding.top + (max_baseline - baseline);
                offsets.insert(child, local_y);
                extent = extent.max(local_y + cross_size + layout.padding.bottom);
            }
            // #322: a HUG cross size was taken from the box bottoms taffy
            // aligned, which is a descender short of where the glyph-aligned
            // text now ends. Record the floor the row needs; `baseline_pass`
            // feeds it back through the solver so the row's ancestors and
            // following siblings re-place around the taller row.
            if layout.sizing_v == AxisSizing::Hug {
                floors.push((node, extent));
            }
        }
    }
    for &child in children {
        collect_baseline_offsets(tree, taffy_of, arena, typesetter, child, offsets, floors);
    }
}

/// Run the #272 baseline correction over a freshly solved tree, and give
/// the solver a second turn when the correction needs a HUG row to be
/// taller than Taffy sized it (#322). Records into `offsets` the corrected
/// cross-axis offset of each node that has one; a node it does not record keeps
/// Taffy's.
///
/// `offsets` is retained rather than returned, and stamped rather than cleared
/// — see [`BaselineOffsets`] for why, and for what issue #1153 found in the
/// sparse map this replaced.
///
/// The re-solve is what makes #322 a layout fix rather than a rect patch:
/// the row's cross size feeds its own placement in its parent, its
/// following siblings' offsets, and any hugging ancestor's size, and Taffy
/// is the one solver that propagates all three (P2). The floor is injected
/// as the row's Taffy `min_size` on the cross axis and recomputed every
/// solve, so a row that stops needing it — smaller text, or no text left in
/// it — gets it removed rather than carrying a stale one on the retained
/// tree.
///
/// Exactly one extra solve is ever run. The floor is `max(baseline-aligned
/// child bottoms)`, and neither a child's baseline nor its cross size
/// depends on the row's own cross size, so the second solve computes the
/// same floor the first one did and there is nothing left to iterate on.
///
/// Eight arguments rather than a struct grouping `floors` and `offsets`: both
/// are fields of [`TreeState`] on the incremental path and locals on the rebuild
/// path, so a struct here would be one the caller assembles and takes apart
/// again. The two readbacks below carry the same `allow` for a different reason
/// — most of their arguments are recursion state.
///
/// `shown_taffy` is the Taffy roots standing for `Arena::shown_roots`, not for
/// every root, and both collection loops below walk the same set. It has to be
/// the shown set for two reasons and neither is cost: an unshown root's nodes
/// were never computed, so `collect_baseline_offsets` would read a zeroed
/// layout and shape its text at width 0 — inventing a floor from nothing — and
/// the re-solve would then compute every root in the document, which is the
/// per-frame cost story #838 exists to remove, on exactly the text scenes the
/// band cannot see (it runs `TaffySolver::new()` and returns above).
#[allow(clippy::too_many_arguments)]
fn baseline_pass(
    tree: &mut TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    shown_taffy: &[taffy::NodeId],
    arena: &Arena,
    typesetter: Option<&mut Typesetter>,
    floors: &mut Vec<(NodeId, f32)>,
    offsets: &mut BaselineOffsets,
    solves: &mut u64,
) {
    // Without a typesetter every text node measures to zero, so there is no
    // glyph baseline to correct and no row to grow — and a solver's typesetter
    // is fixed when it is built, so a solver taking this arm takes it on every
    // solve and has never recorded a correction to go stale. Returning above
    // `begin` is what keeps the empty table empty on that path.
    let Some(ts) = typesetter else {
        // A no-op on the table this path actually holds, which is the empty
        // one. It is here so the property does not rest on the sentence above:
        // a solver that ever went from lending a typesetter to not would
        // otherwise keep the last correcting solve's offsets readable for the
        // rest of its life, and nothing would report it.
        offsets.forget();
        return;
    };
    // Once per solve, and before the walk rather than inside it: the case that
    // needs it is a solve that reaches the walk and records nothing — every
    // baseline row gone — where the previous solve's corrections would
    // otherwise still read as present. That is
    // `a_row_that_stops_being_baseline_aligned_drops_its_corrections`.
    offsets.begin(arena.node_count());

    let mut wanted = Vec::new();
    for &root in arena.shown_roots() {
        collect_baseline_offsets(tree, taffy_of, arena, ts, root, offsets, &mut wanted);
    }
    if wanted == *floors {
        return;
    }

    // Put every previously floored row back to its authored min size, then
    // apply the floors this solve wants. Both lists hold only HUG baseline
    // text rows, so they are short, and this path is only taken when they
    // differ — which already costs a re-solve.
    for &(node, _) in floors.iter() {
        set_cross_floor(tree, taffy_of, arena, node, None);
    }
    for &(node, required) in &wanted {
        set_cross_floor(tree, taffy_of, arena, node, Some(required));
    }
    floors.clone_from(&wanted);

    let ts = compute_all(tree, shown_taffy, Some(ts), solves)
        .expect("the typesetter was lent, not lost");
    // A second collection, over the re-solved tree. **Defensive, and no test
    // can detect its removal**, which is why it says so rather than reading as
    // load-bearing: which nodes the walk records depends only on arena data —
    // the row's mode and cross alignment, its children, each child's
    // `sizing_v`, and whether any child carries text — and the #322 re-solve
    // changes none of that. So the second walk overwrites every slot the first
    // wrote and only the values differ. The bump is here because that argument
    // is about today's collector and the cost of not depending on it is one
    // increment.
    offsets.begin(arena.node_count());
    let mut settled = Vec::new();
    for &root in arena.shown_roots() {
        collect_baseline_offsets(tree, taffy_of, arena, ts, root, offsets, &mut settled);
    }
    debug_assert_eq!(
        settled, wanted,
        "the #322 cross-size floor must not depend on the row's own cross size"
    );
}

/// Set — or, with `floor` of `None`, clear — one node's #322 cross-size
/// floor on the retained tree. Clearing restores the authored min size,
/// which is what [`style_for`] maps for the node.
fn set_cross_floor(
    tree: &mut TaffyTree<TextContext>,
    taffy_of: &[taffy::NodeId],
    arena: &Arena,
    node: NodeId,
    floor: Option<f32>,
) {
    let authored = arena.layout(node).min_height;
    let min_height = match floor {
        Some(required) => Dimension::length(authored.map_or(required, |m| required.max(m))),
        None => authored.map_or(Dimension::AUTO, Dimension::length),
    };
    let taffy_node = taffy_of[node.index()];
    let mut style = tree
        .style(taffy_node)
        .expect("a retained node always has a style")
        .clone();
    style.min_size.height = min_height;
    tree.set_style(taffy_node, style)
        .expect("restyling a retained node cannot fail");
}
