//! Paint table (fill/stroke/effect params, token refs, material class) + the painter trait — boundary B (docs/design/architecture.md).
//!
//! Vocabulary scope: the v0.3 slice (docs/roadmap.md, drawn from the
//! docs/specification/04-figma-vocabulary-profile.md NOW list) — solid fills,
//! the four gradient kinds, image fills with scale modes, stroke with
//! align, rounded corners, and clip. The rect-table index is the document
//! DFS node index (docs/design/dashbuf.md); `RectEntry.paint` indexes the
//! [`PaintTable`] and `RectEntry.clip` indexes the [`ClipTable`].
//!
//! Clipping crosses this boundary already resolved: `dashscene-core`
//! turns "this node clips its children" into a per-rect [`ClipRegion`]
//! at commit (issue #97), because a flat rect table carries no ancestors
//! for a painter to walk (P2).
//!
//! [`image_id`] sits here for a reason that is about publish order rather
//! than about painting: identifying an image's container and reading its
//! intrinsic extent is needed by every crate that writes an asset entry
//! (`dashc`, `dashpack`) and by the gate that checks them
//! (`dashscene-validator`), and this crate is the earliest one all of them
//! share. It header-parses and never decodes — see the module's own
//! documentation and
//! `docs/decisions/image-header-parser-lives-in-dashpaint.md`.

pub mod image_id;

use std::fmt;
use std::sync::Arc;

/// An RGBA color, 4×f32 — the same shape as `dashbuf`'s `Color` struct.
///
/// `#[repr(C)]` fixes the layout now: solid-fill colors are per-frame
/// painter input, and docs/specification/03-target-hardware-rules.md (R-T4) plans instance-buffer uploads
/// of that input, even though nothing uploads it yet.
///
/// `PartialEq` is derived, so it compares each field with `f32`'s own
/// `==` — IEEE 754 equality, not a comparison of the stored bits (debt
/// #53). Two consequences follow, both confirmed by a runtime test
/// (`tests/boundary_b.rs`) rather than asserted from the standard:
///
/// - A `NaN` channel makes the whole `Color` unequal to everything,
///   including a bit-for-bit copy of itself — `PartialEq` is derived,
///   so it inherits `f32`'s non-reflexivity (`NaN != NaN`).
/// - `0.0` and `-0.0` compare equal channel-by-channel even though their
///   bit patterns differ, so two `Color`s that compare equal are not
///   guaranteed to be byte-identical.
///
/// Nothing in this crate relies on either behavior today: every equality
/// check in the test suite is over finite constants. A future
/// equality-based dedup or dirty-diff over colors (dashscene-core's
/// dirty set, an R-T4 upload path) would need to choose its comparison
/// deliberately rather than assume this `PartialEq` is a byte-equality
/// proxy.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// An index into the [`PaintTable`] — the type of [`RectEntry::paint`]
/// and the return of [`PaintTable::push`].
///
/// `#[repr(transparent)]` over `u32`: [`RectEntry`] stays blittable and
/// its layout unchanged, while a node index or any other bare `u32`
/// cannot be passed where a paint index belongs without an explicit
/// wrap.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaintIndex(pub u32);

/// An index into the [`ClipTable`] — the type of [`RectEntry::clip`]
/// and the return of [`ClipTable::push`].
///
/// `#[repr(transparent)]` over `u32`, for the same reason as
/// [`PaintIndex`]: [`RectEntry`] stays blittable, and no bare `u32` can
/// pass for a clip index without an explicit wrap.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipIndex(pub u32);

impl ClipIndex {
    /// The region every unclipped rect references. [`ClipTable::new`]
    /// reserves index 0 for it, so an unclipped rect still resolves —
    /// this is a real entry, not a sentinel
    /// (`docs/decisions/boundary-b-unification.md`).
    pub const UNCLIPPED: ClipIndex = ClipIndex(0);
}

/// One resolved rectangle — boundary B's per-node unit (docs/design/architecture.md).
///
/// The rect-table index of this entry is the document DFS node index, so
/// there is no id field. `paint` resolves in the [`PaintTable`], `clip`
/// in the [`ClipTable`].
///
/// `#[repr(C)]`: docs/design/architecture.md calls rect entries blittable, and R-T4
/// plans dirty-range instance-buffer uploads straight from the rect table.
///
/// `PartialEq` is derived over `x`, `y`, `w`, `h` and `opacity` as `f32`
/// (IEEE 754 `==`, not a bitwise compare), and over `paint`/`clip` as
/// exact integer indices — the same NaN and -0.0 semantics documented on
/// [`Color`] apply here for the same reason and with the same caveat: a
/// NaN geometry field breaks reflexivity, and -0.0/0.0 compare equal
/// despite differing bits, confirmed by a runtime test rather than
/// assumed (debt #53).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectEntry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub paint: PaintIndex,
    /// The clip that applies to this rect, resolved from its clipping
    /// ancestors by `dashscene-core` at commit — a painter never
    /// re-derives the tree (P2). [`ClipIndex::UNCLIPPED`] when no
    /// ancestor clips.
    pub clip: ClipIndex,
    /// The effective *free*-path group alpha for this rect, in `[0, 1]`
    /// — the product of the enclosing group opacities that resolved to
    /// the free path (`docs/decisions/masks-and-group-opacity.md`),
    /// including the node's own opacity when it is free. A painter
    /// multiplies the rect's paint alpha by this value. `1.0` when no
    /// free-path group opacity applies. The alpha of a *render-target*
    /// group is not folded in here — it is applied once when the group's
    /// [`GroupComposite`] layer composites, so rects inside such a group
    /// carry only their in-layer free alpha.
    pub opacity: f32,
    /// This rect's rotation in radians, about [`RectEntry::rotation_anchor`].
    /// `0.0` is unrotated, which is what every rect carried before story
    /// #770.
    ///
    /// The rect's `x`/`y`/`w`/`h` describe the node's box **before** the
    /// rotation, and a painter turns that box when it draws
    /// (`docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
    /// They are not the rotated silhouette's bounds: taking them for that is
    /// the mistake the Figma lowering made in the other direction, reading a
    /// rotated node's `absoluteBoundingBox` as its box.
    ///
    /// A painter that cannot rotate says so through [`Painter::rotates`]
    /// rather than ignoring this field.
    pub rotation: f32,
    /// The point [`RectEntry::rotation`] turns about, in the rect's own
    /// coordinate space — `(0.0, 0.0)` is the rect's top-left, not its
    /// centre. Meaningless when `rotation` is `0.0`, and canonically
    /// `(0.0, 0.0)` there.
    pub rotation_anchor: Vec2,
}

/// A 2D point or vector, in the coordinate space its context names.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

/// A row-major 2×3 affine transform: maps (x, y) to
/// (a·x + b·y + tx, c·x + d·y + ty). Same shape as `dashbuf`'s `Mat23`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat23 {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Mat23 {
    /// The transform that maps every point to itself — what an image fill
    /// carries when it names no crop transform of its own. Story #578
    /// removed [`ImageFill::transform`]'s `Option`, which had no C
    /// representation, and this is the value its `None` meant.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
}

/// One gradient color stop; `offset` is normalized 0..1 along the
/// gradient's primary axis.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

/// The four gradient kinds of docs/specification/04-figma-vocabulary-profile.md (angular serves gauges).
/// `#[repr(u8)]` is checked: [`Gradient`] carries this enum and is on
/// `dashscene-unity`'s `extern "C"` surface since story #578, so removing
/// the attribute stops the workspace compiling — verified by mutation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    Angular,
    Diamond,
}

/// The most stops one gradient may carry.
///
/// A property of the paint vocabulary, not of any one backend: the lean
/// painter pays for stops in uniform slots, so the ceiling is what every
/// painter can be held to. It lives here, on boundary B, because the
/// painter that panics above it (`dashscene-skia`) and the validator that
/// rejects it upstream (`dashscene-validator`, P4) have to agree on the
/// number — two copies of an `8` that silently disagree would make the
/// validator's guarantee false.
pub const MAX_GRADIENT_STOPS: usize = 8;

/// Where one gradient's stops sit in the [`PaintTable`]'s flat stop array.
///
/// The fill-side twin of [`ShadowRange`] and [`GlyphRange`], for the same
/// two reasons: a `Vec` per gradient has no C representation, and a flat
/// array plus a range uploads as one buffer copy (story #578).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StopRange {
    /// First stop, as an index into [`PaintTable::all_stops`].
    pub offset: u32,
    /// How many stops, along the primary axis. At least one and at most
    /// [`MAX_GRADIENT_STOPS`] for a gradient the table holds.
    pub count: u32,
}

impl StopRange {
    /// The range a gradient carries before [`PaintTable::intern_fill`]
    /// assigns it one. Unlike [`ShadowRange::NONE`] this is not also a
    /// resting value: every gradient in a table names at least one stop,
    /// so a zero count here means "not yet interned", never "no stops".
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// A gradient fill. One geometry model serves all four kinds: three
/// normalized handle positions in the node's box — the gradient origin,
/// the primary-axis end, and the secondary-axis end (Figma's
/// gradientHandlePositions). Handles are intent; resolved geometry is
/// per-painter math (P1).
///
/// The stops are a [`StopRange`] into the table's flat array since story
/// #578; read them with [`PaintTable::stops`]. A gradient carries at least
/// one and at most [`MAX_GRADIENT_STOPS`] stops — validated upstream (P4),
/// and painters may assume it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub handle_origin: Vec2,
    pub handle_primary: Vec2,
    pub handle_secondary: Vec2,
    pub stops: StopRange,
}

/// Figma image-fill scale modes.
///
/// `#[repr(u8)]` is checked: [`ImageFill`] carries this enum and is on
/// `dashscene-unity`'s `extern "C"` surface since story #578, so removing
/// the attribute stops the workspace compiling — verified by mutation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    Fill,
    Fit,
    Crop,
    Tile,
}

/// What an image payload is — a source-encoded container a painter decodes,
/// or a baked texel block format it uploads.
///
/// # Two halves, one enum
///
/// `Png`, `Jpeg` and `Gif` are the source-encoded containers (story #342 for
/// the latter two — Figma re-encodes opaque uploads to Jpeg, and Gif covers
/// static single-frame fills; the importer refuses animated Gif by name before
/// it reaches here). A painter holding one of these decodes it.
///
/// Everything after them is a **baked** payload: the block format `dashpack`
/// derived, ready to upload with no decode of any kind. That is what
/// `docs/specification/03-target-hardware-rules.md` requires of product assets
/// — "native ASTC directly, with no Basis and no transcode step of any kind" —
/// and until story #640 boundary B could not express it, so the packer's output
/// could not reach a painter at all (issue #640).
///
/// # Why one flat enum rather than `Baked(TexelFormat)`
///
/// The nested form reads better and is what issue #640 proposed. It is not
/// used, for the reason [`crate::ImageEntry`] gives: this value crosses the FFI
/// gate as one `u32`, and a nested enum needs a mapping in each direction that
/// is a second place for the correspondence to be written. `InstanceKind` in
/// `dashscene-gpu` was two fields for the same reason and their discriminants
/// collided; one flat discriminant makes that unrepresentable rather than
/// forbidden.
///
/// # Why the colour space is part of the format
///
/// It is part of the *format* in KTX2 and in Vulkan — `ASTC_6x6_SRGB_BLOCK` and
/// `ASTC_6x6_UNORM_BLOCK` are two formats, not one format with a flag — and
/// `dashpack::Rung::format` already names them that way. Carrying it beside the
/// block size would be a second record of one fact.
///
/// The variants are exactly the rungs `dashpack`'s image-fill ladder can stop
/// at (`crates/dashpack/src/profile.rs`, `IMAGE_FILL_RUNGS`), in each of the
/// two colour spaces its asset classes use. A format the packer cannot produce
/// is deliberately absent: a painter that matched on one would be writing a
/// branch nothing can reach.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png = 0,
    Jpeg = 1,
    Gif = 2,
    Astc4x4Srgb = 3,
    Astc4x4Unorm = 4,
    Astc5x5Srgb = 5,
    Astc5x5Unorm = 6,
    Astc6x6Srgb = 7,
    Astc6x6Unorm = 8,
    Astc8x8Srgb = 9,
    Astc8x8Unorm = 10,
    Astc10x10Srgb = 11,
    Astc10x10Unorm = 12,
    Astc12x12Srgb = 13,
    Astc12x12Unorm = 14,
    Rgba8Srgb = 15,
    Rgba8Unorm = 16,
}

impl ImageFormat {
    /// True when a painter must decode this payload before it can use it.
    ///
    /// The whole of what separates the two halves, and the predicate a
    /// capability declaration is written against — see
    /// [`Painter::samples`].
    pub const fn is_encoded(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Gif)
    }

    /// The ASTC block this format's payload is made of, or `None` for a format
    /// that has no blocks.
    ///
    /// `Rgba8` is baked and blockless: it is the terminal rung of every ladder
    /// (`dashpack::Rung::Uncompressed`), uploaded as texels rather than
    /// decoded, so it is neither encoded nor blocked.
    pub const fn block(self) -> Option<(u32, u32)> {
        match self {
            Self::Astc4x4Srgb | Self::Astc4x4Unorm => Some((4, 4)),
            Self::Astc5x5Srgb | Self::Astc5x5Unorm => Some((5, 5)),
            Self::Astc6x6Srgb | Self::Astc6x6Unorm => Some((6, 6)),
            Self::Astc8x8Srgb | Self::Astc8x8Unorm => Some((8, 8)),
            Self::Astc10x10Srgb | Self::Astc10x10Unorm => Some((10, 10)),
            Self::Astc12x12Srgb | Self::Astc12x12Unorm => Some((12, 12)),
            Self::Png | Self::Jpeg | Self::Gif | Self::Rgba8Srgb | Self::Rgba8Unorm => None,
        }
    }

    /// How many bytes a payload in this format occupies at `width` x `height`
    /// texels, or `None` for an encoded container, whose length is a property
    /// of its compression rather than of its extent.
    ///
    /// An ASTC payload is one 16-byte block per footprint, with the last block
    /// of a row or column covering texels past the edge — so the block counts
    /// round **up**, and an extent that is not a multiple of the footprint
    /// still costs a whole block. `Rgba8` is four bytes per texel.
    ///
    /// The same arithmetic `dashpack` checks a derivation against when it reads
    /// one back (`dashpack::preview`, `PreviewError::PayloadLen`). It is written
    /// twice because `dashpaint` depends on no crate — the same trade
    /// `ImageFormat` itself makes against `dashpack::Rung::format` — and both
    /// copies are stated over the format's own footprint rather than over a
    /// number, so neither can drift without the footprint drifting.
    ///
    /// `None` for an encoded format rather than a guess: a PNG's length says
    /// nothing about its extent, which is the whole reason the extent is
    /// carried on the row at all.
    pub const fn payload_len(self, width: u32, height: u32) -> Option<u64> {
        match self.block() {
            Some((bx, by)) => {
                let blocks =
                    (width as u64).div_ceil(bx as u64) * (height as u64).div_ceil(by as u64);
                Some(blocks * 16)
            }
            None if self.is_encoded() => None,
            // The blockless baked half: `Rgba8` in either colour space.
            None => Some(width as u64 * height as u64 * 4),
        }
    }

    /// The value [`ImageEntry::format`] carries.
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// The format a stored entry names.
    ///
    /// # Panics
    ///
    /// Panics on a value no variant carries. An entry's format is written by
    /// [`ImageTable::push`] from this same enum, so a miss is a corrupt table
    /// rather than input to validate — the contract every other row of
    /// boundary B panics under.
    pub const fn from_u32(value: u32) -> Self {
        match value {
            0 => Self::Png,
            1 => Self::Jpeg,
            2 => Self::Gif,
            3 => Self::Astc4x4Srgb,
            4 => Self::Astc4x4Unorm,
            5 => Self::Astc5x5Srgb,
            6 => Self::Astc5x5Unorm,
            7 => Self::Astc6x6Srgb,
            8 => Self::Astc6x6Unorm,
            9 => Self::Astc8x8Srgb,
            10 => Self::Astc8x8Unorm,
            11 => Self::Astc10x10Srgb,
            12 => Self::Astc10x10Unorm,
            13 => Self::Astc12x12Srgb,
            14 => Self::Astc12x12Unorm,
            15 => Self::Rgba8Srgb,
            16 => Self::Rgba8Unorm,
            _ => panic!("no image format carries this value"),
        }
    }
}

/// One image asset on the way **into** an [`ImageTable`] — bytes plus what
/// they are.
///
/// The producer half of the three types this table splits into, and the only
/// one that owns its bytes: the table stores them in one pool and hands out
/// [`ImageRef`], which borrows. That split is what lets the stored row be
/// `#[repr(C)]` without making every caller that builds an asset assemble a
/// range by hand (`docs/decisions/instance-buffer-contract.md` took the same
/// shape for the same reason).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAsset {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

impl ImageAsset {
    /// This asset as a painter reads one, with the extent read out of the
    /// payload's own header.
    ///
    /// An [`Atlas`] owns its payload directly rather than through an
    /// [`ImageTable`], so the two ways a payload reaches a painter meet here
    /// and a consumer writes one signature rather than two.
    ///
    /// # Panics
    ///
    /// Panics on a baked payload, and on an encoded one whose header does not
    /// parse — for the reasons [`ImageTable::push`] gives, which is the other
    /// half of this pair.
    pub fn as_ref(&self) -> ImageRef<'_> {
        let (width, height) = identified_extent(self.format, &self.bytes);
        ImageRef {
            format: self.format,
            bytes: &self.bytes,
            width,
            height,
        }
    }
}

/// The extent of an **encoded** payload, from its own header.
///
/// The one place [`image_id::identify`] is called on the way into boundary B,
/// so that the derived extent and the refusals around it are written once.
///
/// # The empty payload is not an error here
///
/// A payload whose binding supplied nothing has no header and so no extent, and
/// it is stored at `0 x 0` rather than refused. `dashscene-validator`'s
/// `image.no-bytes` rule is what names it, over a table it is handed already
/// built — so a panic here would replace a named diagnostic with a crash, which
/// is the opposite of what P4 asks for. [`ImageEntry::len`] already says a
/// zero-length payload is a value rather than a sentinel; its extent follows.
///
/// # Panics
///
/// Panics on a baked payload, naming the path that carries an extent instead:
/// ASTC blocks and raw RGBA8 texels have no header, and a payload length does
/// not determine an extent — `ceil(w/6) * ceil(h/6) * 16` maps many extents
/// onto one length — so a derived value here would be a guess.
///
/// Panics on a non-empty encoded payload whose header does not parse. Those
/// bytes are refused upstream — `dashc`'s image-identity gate reads the same
/// header, and `dashscene-validator`'s `asset.extent-mismatch` compares it
/// against the document — so one arriving here is a broken contract between
/// crates rather than input to validate, and takes the same panic
/// [`ImageTable::resolve`] takes for an index (P4).
fn identified_extent(format: ImageFormat, bytes: &[u8]) -> (u32, u32) {
    assert!(
        format.is_encoded(),
        "a {format:?} payload states no extent of its own: bake it through \
         ImageTable::push_baked, which takes the extent the document records"
    );
    if bytes.is_empty() {
        return (0, 0);
    }
    let header = image_id::identify(bytes)
        .unwrap_or_else(|error| panic!("a {format:?} payload's header parses (P4): {error}"));
    (header.width, header.height)
}

/// One image asset as the table **stores** it: what it is, and where its bytes
/// are in the pool.
///
/// `#[repr(C)]`, fixed-width, no owning members — the rule story #600 holds
/// every boundary-B row to, and the one row that did not meet it until story
/// #640. `format` is a plain `u32` rather than the enum so the layout is a
/// number a C or C# reader can hold; [`ImageFormat::from_u32`] is the one place
/// it is read back.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageEntry {
    /// An [`ImageFormat`] discriminant.
    pub format: u32,
    /// First byte of the payload in the table's pool.
    pub offset: u32,
    /// How many bytes. A zero-length payload is a real value — an asset whose
    /// binding supplied nothing — and is not a sentinel for anything.
    pub len: u32,
    /// The payload's intrinsic extent in texels.
    ///
    /// # Why the row carries it rather than each painter recovering it
    ///
    /// A painter needs the extent before it draws anything: a
    /// [`ScaleMode::Fill`], `Fit` or `Crop` fill is stated over the image's
    /// intrinsic size, and a painter that puts the payload in a texture atlas
    /// needs it to allocate the rectangle at all.
    ///
    /// The encoded half could be recovered per painter —
    /// `dashscene-skia` reads it off the decoded image, and
    /// [`image_id::identify`] reads it out of the header with no decode. The
    /// baked half cannot be recovered by anyone: ASTC blocks and raw RGBA8
    /// texels carry no header, and a payload length does not determine an
    /// extent. Since one half has to be carried, carrying both is what keeps
    /// every painter reading one field rather than two paths (issue #716).
    pub width: u32,
    pub height: u32,
}

/// One image asset as a painter **reads** it: what it is, its extent, and its
/// bytes, borrowed from the table's pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageRef<'a> {
    pub format: ImageFormat,
    pub bytes: &'a [u8],
    /// The payload's intrinsic extent in texels — see [`ImageEntry::width`].
    pub width: u32,
    pub height: u32,
}

/// A contiguous byte region an [`ImageTable`] may point into without owning it
/// — a memory-mapped `.dsb` in practice (story #596,
/// `docs/decisions/assets-borrow-from-the-mapping.md`).
///
/// The trait exists so that `dashpaint`, which depends on nothing and must keep
/// depending on nothing, can hold a region whose type it does not know.
/// `dashbuf::map::MappedFile` satisfies it through the blanket implementation
/// below, and so does anything else that can hand out its bytes.
///
/// `Send + Sync` is required rather than convenient: the arena holds
/// `Arc<ImageTable>`, so a table crosses threads by construction (D4). Story
/// (#597) did not add the loader thread this originally anticipated — the hosts
/// build their scene before the frame loop starts, so the faults are already
/// off the frame thread — but `dashbuf::residency::BlobResidency` is `Send + Sync`
/// for the same reason, and a thread is the next step rather than a different
/// design.
pub trait Region: Send + Sync {
    /// The whole region. An [`ImageEntry`]'s `offset` is an index into this.
    fn bytes(&self) -> &[u8];
}

impl<T: AsRef<[u8]> + Send + Sync> Region for T {
    fn bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

/// Where an [`ImageTable`]'s payload bytes live: bytes it allocated, or a
/// region it does not own.
///
/// **One or the other, never both** (D1). A per-row choice would mean a base
/// pointer per row, which widens the one row the FFI gate pins, to serve a case
/// nothing in v0 has. A table that has taken one arm refuses the other by name
/// rather than growing a third state.
#[derive(Clone)]
enum Pool {
    /// Bytes this table allocated and copied into — what a producer building a
    /// table by hand gets.
    Owned(Vec<u8>),
    /// A region this table points into and does not own — what the loader
    /// builds from a mapped file.
    Mapped(Arc<dyn Region>),
}

impl Pool {
    /// The whole pool. An entry's `offset` indexes this in both arms.
    fn bytes(&self) -> &[u8] {
        match self {
            Pool::Owned(blobs) => blobs,
            Pool::Mapped(region) => region.bytes(),
        }
    }
}

impl Default for Pool {
    fn default() -> Self {
        Pool::Owned(Vec::new())
    }
}

impl fmt::Debug for Pool {
    /// A pool prints its shape and its size, never its bytes. A mapped pool is
    /// the whole file, and a `Debug` that dumped megabytes of payload would
    /// make every `{:?}` on a scene useless.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pool::Owned(blobs) => write!(f, "Owned({} bytes)", blobs.len()),
            Pool::Mapped(region) => write!(f, "Mapped({} bytes)", region.bytes().len()),
        }
    }
}

/// The image-asset table — the runtime side, carrying decoded-ready bytes.
/// A document names its assets by content hash (`dashbuf`'s `Document.assets`)
/// and the loader binds each to its payload; by the time a table reaches a
/// painter the bytes are here:
/// dense, indexed by [`ImageFill::image`]. Part of the
/// painter input since the v0.3 vocabulary
/// (`docs/decisions/image-assets-cross-boundary-b.md`).
///
/// Since story #596 the pool is either owned or mapped
/// (`docs/decisions/assets-borrow-from-the-mapping.md`). Nothing a painter sees
/// changes: [`ImageEntry`] keeps its twenty-byte `#[repr(C)]` shape with
/// `offset` relative to the pool in both arms, and [`ImageTable::resolve`]
/// hands back the same [`ImageRef`].
#[derive(Debug, Clone, Default)]
pub struct ImageTable {
    /// Every asset's bytes: concatenated in the pool this table allocated, or
    /// in place in a region it borrows. One base for a whole frame's payloads
    /// rather than one per asset, which is what lets [`ImageEntry`] be a
    /// fixed-width row.
    pool: Pool,
    entries: Vec<ImageEntry>,
}

/// Two tables are equal when they hold the same rows over the same payload
/// bytes.
///
/// Compared row by row rather than pool against pool, which is what makes one
/// behaviour serve both arms. An owned pool is exactly its rows concatenated,
/// so this is the comparison the derive did; a mapped pool is a whole file, of
/// which the rows are a part, and comparing pools there would call two tables
/// different because their files differ elsewhere — and would walk megabytes to
/// do it, every frame, in `dashscene-skia`'s frame cache.
///
/// `offset` is deliberately **not** compared. It says where a payload sits in
/// this table's own pool, which is the one field that must differ between an
/// owned table and a mapped one holding the same picture. Two owned tables
/// built from the same payloads always agree on it anyway, since rows are
/// appended in order, so nothing that used to compare unequal now compares
/// equal.
///
/// The identity shortcut a mapped pool makes possible — equal handle plus equal
/// rows, with no byte walk at all — is deliberately not taken here. It is real
/// and it is debt #752 (D8), because it makes `PartialEq` two behaviours and
/// needs its own test for the mixed comparison.
impl PartialEq for ImageTable {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self
                .entries
                .iter()
                .enumerate()
                .zip(other.entries.iter().enumerate())
                .all(|((index, row), (other_index, other_row))| {
                    row.format == other_row.format
                        && row.len == other_row.len
                        && row.width == other_row.width
                        && row.height == other_row.height
                        && self.payload(index) == other.payload(other_index)
                })
    }
}

impl ImageTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an **encoded** asset and returns its index — the value an
    /// [`ImageFill::image`] field holds to reference it.
    ///
    /// Takes the owning [`ImageAsset`] and copies its bytes into the pool, so
    /// that a caller assembling a table writes what it means rather than an
    /// offset it has to compute.
    ///
    /// The extent is read out of the payload's own header rather than taken
    /// from the caller. Deriving it here rather than widening [`ImageAsset`]
    /// is the same producer/table/reader split
    /// `docs/decisions/baked-texel-payloads-cross-boundary-b.md` D4 took: the
    /// producer keeps its shape, and the fixed-width row the table stores gains
    /// what only the table can fill in.
    ///
    /// # Panics
    ///
    /// Panics on a baked payload, which states no extent of its own — use
    /// [`push_baked`](Self::push_baked). See `identified_extent` for why a
    /// baked extent cannot be derived from the bytes.
    pub fn push(&mut self, asset: ImageAsset) -> u32 {
        let (width, height) = identified_extent(asset.format, &asset.bytes);
        self.push_row(asset, width, height)
    }

    /// Appends a **baked** asset whose extent the caller states, and returns
    /// its index.
    ///
    /// The loader is the caller: a baked payload only ever reaches boundary B
    /// through a binding (`dashscene_core::BoundPayload`), and the extent is
    /// the one the document records for the canonical asset. A derivation
    /// preserves it — `dashpack`'s rungs are block footprints, not mip levels —
    /// so the document's number describes the derived payload as exactly as it
    /// describes the canonical one.
    ///
    /// # Panics
    ///
    /// Panics on an encoded payload. Its extent is in its own header, and a
    /// caller stating a second copy of it is a caller who can state a wrong
    /// one — the divergence `dashscene-validator`'s `asset.extent-mismatch`
    /// exists to refuse upstream.
    ///
    /// Panics when the payload is not the length the format and the extent
    /// require ([`ImageFormat::payload_len`]). This is the baked half's version
    /// of the check the encoded half gets for free: `identify` refuses bytes
    /// that are not the container they claim to be, and until this assertion a
    /// baked binding could state any extent at all beside any bytes at all.
    /// That is exactly the disagreement
    /// `docs/decisions/baked-texel-payloads-cross-boundary-b.md` D5 closed for
    /// the *format* and could not close for the extent, because the extent was
    /// not carried yet.
    pub fn push_baked(&mut self, asset: ImageAsset, width: u32, height: u32) -> u32 {
        assert!(
            !asset.format.is_encoded(),
            "a {:?} payload carries its own extent: push it through ImageTable::push, which \
             reads the header rather than trusting a second copy of the number",
            asset.format
        );
        let expected = asset
            .format
            .payload_len(width, height)
            .expect("a baked format sizes its payload");
        assert_eq!(
            asset.bytes.len() as u64,
            expected,
            "a {:?} payload at {width}x{height} is {expected} bytes, and {} were bound: the \
             extent and the bytes describe different images",
            asset.format,
            asset.bytes.len()
        );
        self.push_row(asset, width, height)
    }

    /// The half both push paths share, once the extent is settled.
    ///
    /// # Panics
    ///
    /// On a mapped table. Copying a payload into a pool this table does not own
    /// is the mixed table D1 refuses, and refusing it here by name is what
    /// keeps that a stated limitation rather than a silent one (P4).
    fn push_row(&mut self, asset: ImageAsset, width: u32, height: u32) -> u32 {
        let Pool::Owned(blobs) = &mut self.pool else {
            panic!(
                "this image table borrows a mapped region, and a payload cannot be copied into a \
                 pool it does not own: build a mapped table with ImageTable::mapped and add rows \
                 with push_mapped, or an owned one with ImageTable::new \
                 (docs/decisions/assets-borrow-from-the-mapping.md D1)"
            )
        };
        let index =
            u32::try_from(self.entries.len()).expect("image table exceeds u32::MAX entries");
        let offset = u32::try_from(blobs.len()).expect("image pool exceeds u32::MAX bytes");
        let len = u32::try_from(asset.bytes.len()).expect("image payload exceeds u32::MAX bytes");
        blobs.extend_from_slice(&asset.bytes);
        self.entries.push(ImageEntry {
            format: asset.format.as_u32(),
            offset,
            len,
            width,
            height,
        });
        index
    }

    /// A table whose payloads live in `region` and are never copied out of it —
    /// the loader's arm (D1).
    ///
    /// Rows are added with [`push_mapped`](Self::push_mapped), by range. The
    /// region is the whole file, so a row's `offset` is a file offset.
    pub fn mapped(region: Arc<dyn Region>) -> Self {
        Self {
            pool: Pool::Mapped(region),
            entries: Vec::new(),
        }
    }

    /// Appends a row naming `len` bytes at `offset` in the mapped region, and
    /// returns its index.
    ///
    /// Nothing is read and nothing is copied: the row is the range. `format`
    /// and the extent come from the caller because a range carries neither —
    /// the loader has the document's entry, which states both.
    ///
    /// # Panics
    ///
    /// On an owned table, for the reason `push_row` panics on
    /// a mapped one.
    ///
    /// On an `offset` or `len` past `u32::MAX`, which is the 4 GiB cap
    /// [`ImageEntry`]'s `u32` offset implies (D7). It is named here rather than
    /// truncated into a plausible offset, because a truncated offset draws the
    /// wrong picture instead of failing.
    ///
    /// On a **baked** payload whose range is not the length its format and
    /// extent require ([`ImageFormat::payload_len`]) — the same check
    /// [`push_baked`](Self::push_baked) makes, for the same reason and at the
    /// same cost. A baked payload carries no header, so nothing downstream can
    /// tell that a stated extent and the bytes beside it describe different
    /// images; and this path deliberately reads no header even for an encoded
    /// payload, so it is the only place the disagreement can be named.
    ///
    /// The check is arithmetic over numbers already in hand, so it costs no
    /// page fault — which is what makes it belong here rather than being the
    /// price of the mapped path.
    ///
    /// On a range that runs past the end of the region, which would otherwise
    /// be discovered by a painter slicing out of bounds one frame later.
    pub fn push_mapped(
        &mut self,
        format: ImageFormat,
        offset: u64,
        len: u64,
        width: u32,
        height: u32,
    ) -> u32 {
        assert!(
            matches!(self.pool, Pool::Mapped(_)),
            "this image table owns its pool, and a range into a mapped region means nothing in \
             it: build a mapped table with ImageTable::mapped \
             (docs/decisions/assets-borrow-from-the-mapping.md D1)"
        );
        let end = offset
            .checked_add(len)
            .unwrap_or_else(|| panic!("payload range {offset}..+{len} overflows"));
        let region = self.pool.bytes().len() as u64;
        assert!(
            end <= region,
            "payload range {offset}..{end} runs past the {region}-byte region it names"
        );
        let offset = u32::try_from(offset).unwrap_or_else(|_| {
            panic!(
                "payload offset {offset} is past {}: a mapped document is capped at 4 GiB by \
                 ImageEntry's u32 offset (docs/decisions/assets-borrow-from-the-mapping.md D7)",
                u32::MAX
            )
        });
        let len = u32::try_from(len)
            .unwrap_or_else(|_| panic!("payload length {len} is past {}", u32::MAX));
        if !format.is_encoded() {
            let expected = format
                .payload_len(width, height)
                .expect("a baked format sizes its payload");
            assert_eq!(
                u64::from(len),
                expected,
                "a {format:?} payload at {width}x{height} is {expected} bytes, and the range \
                 names {len}: the extent and the bytes describe different images"
            );
        }
        let index =
            u32::try_from(self.entries.len()).expect("image table exceeds u32::MAX entries");
        self.entries.push(ImageEntry {
            format: format.as_u32(),
            offset,
            len,
            width,
            height,
        });
        index
    }

    /// The bytes row `index` names, or an empty slice for a row that is not
    /// there — the shared half of [`get`](Self::get) and [`PartialEq`].
    fn payload(&self, index: usize) -> &[u8] {
        let Some(entry) = self.entries.get(index) else {
            return &[];
        };
        let start = entry.offset as usize;
        &self.pool.bytes()[start..start + entry.len as usize]
    }

    /// The asset at `index`, borrowing its bytes from the pool.
    pub fn get(&self, index: u32) -> Option<ImageRef<'_>> {
        let entry = self.entries.get(index as usize)?;
        Some(ImageRef {
            format: ImageFormat::from_u32(entry.format),
            bytes: self.payload(index as usize),
            width: entry.width,
            height: entry.height,
        })
    }

    /// Resolves an image index. Panics on an out-of-range index —
    /// indices are validated upstream (P4), same contract as
    /// [`PaintTable::resolve`].
    pub fn resolve(&self, index: u32) -> ImageRef<'_> {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "image index {index} out of range ({} assets): image indices are validated upstream (P4)",
                self.entries.len()
            )
        })
    }

    /// Every stored row, for a consumer that walks the table rather than
    /// resolving one index — and the value the FFI gate is stated over.
    pub fn all_entries(&self) -> &[ImageEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Stroke placement relative to the node's outline. Painters that only
/// stroke on center lower Inside/Outside by path expansion
/// (docs/technotes/rendering-and-painters.md).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

/// A stroke. v0.3 strokes are solid-only (see
/// `docs/decisions/paint-entry-composition.md`); the color widens to a
/// fill additively if a real file ever needs gradient strokes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub align: StrokeAlign,
    pub color: Color,
}

/// Per-corner radii in document units; all zero (the default) = sharp
/// corners.
///
/// `#[repr(C)]` because [`ClipBox`] embeds one and is itself `#[repr(C)]`.
/// A `repr(C)` struct with a `repr(Rust)` field does not have a fixed
/// layout — the field's own layout is unspecified — so `ClipBox` was
/// making a promise it did not keep. Found by `dashscene-unity`'s
/// `improper_ctypes_definitions` gate on its first run (story #600); the
/// attribute changes no layout any current target actually produces, which
/// is why nothing moved when it was added.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

/// One clipping box of a resolved [`ClipRegion`]: an axis-aligned box in
/// the same absolute space as [`RectEntry`], rounded by `corners` (all
/// zero = a sharp rect).
///
/// This is a clipping *ancestor*'s box, already resolved — not the
/// clipped rect's own geometry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub corners: CornerRadii,
}

/// The clip that applies to one rect: the **intersection** of its boxes,
/// which are the boxes of its clipping ancestors, outermost first. No
/// boxes = unclipped.
///
/// `dashscene-core` resolves this at commit (docs/design/architecture.md
/// `Paint.clip` — "clips its children to its box"): boundary B is a flat
/// rect table, so a painter has no ancestors to walk and P2 forbids it
/// re-deriving them. The box list is kept rather than pre-intersected
/// because the intersection of two rounded rects is not a rounded rect.
///
/// # A range, not a list (story #578)
///
/// This is `(offset, count)` into the [`ClipTable`]'s one flat box array,
/// not a `Vec<ClipBox>` of its own. Two reasons, and they are the same
/// reason:
///
/// - **It has a C representation.** A `Vec` does not, and boundary B is a
///   language-neutral data contract because G2 names a C# backend
///   (`docs/design/architecture.md`). `crates/dashscene-unity`'s
///   `improper_ctypes_definitions` gate holds this.
/// - **It uploads.** A flat array plus a range is one buffer copy; a
///   `Vec` per region is a pointer chase per rect, which is what R-T4
///   bounds the frame's CPU cost against.
///
/// Reading the boxes needs the table that owns them, so
/// [`ClipTable::resolve`] hands back a [`ClipView`] rather than this type
/// directly. This is what crosses the seam; that is what code reads.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ClipRegion {
    /// First box, as an index into [`ClipTable::all_boxes`].
    pub offset: u32,
    /// How many boxes, outermost ancestor first. Zero = unclipped.
    pub count: u32,
}

impl ClipRegion {
    /// The region of a rect no ancestor clips — the one [`ClipTable`]
    /// reserves at [`ClipIndex::UNCLIPPED`].
    pub const fn unclipped() -> Self {
        Self {
            offset: 0,
            count: 0,
        }
    }

    /// True when no ancestor clips this rect (no boxes).
    pub const fn is_unclipped(&self) -> bool {
        self.count == 0
    }
}

/// One region together with the boxes it names — what a painter reads.
///
/// [`ClipRegion`] is a range and cannot answer "which boxes" on its own.
/// Rather than make every call site carry the table alongside the range,
/// [`ClipTable::resolve`] returns this, so `clips.resolve(rect.clip).boxes()`
/// reads exactly as it did before the flattening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipView<'a> {
    region: ClipRegion,
    boxes: &'a [ClipBox],
}

impl<'a> ClipView<'a> {
    /// The boxes to intersect, outermost ancestor first.
    pub fn boxes(&self) -> &'a [ClipBox] {
        self.boxes
    }

    /// True when no ancestor clips this rect (no boxes).
    pub fn is_unclipped(&self) -> bool {
        self.region.is_unclipped()
    }

    /// The range itself — the value that crosses boundary B, for a caller
    /// packing an instance buffer rather than drawing.
    pub fn region(&self) -> ClipRegion {
        self.region
    }
}

/// The clip table: dense, indexed by [`RectEntry::clip`]. Index 0 is
/// always the unclipped region ([`ClipIndex::UNCLIPPED`]), so every rect
/// resolves; regions are deduplicated by `dashscene-core`, so the
/// subtree under one clipping ancestor shares one entry.
///
/// The regions are ranges into `boxes`, one flat array for the whole table
/// (story #578). Prefixes are **not** shared: a region `[A]` and a region
/// `[A, B]` store `A` twice. Sharing them would mean a suffix-matching
/// allocator on a path that runs per commit, to save bytes on a table whose
/// regions are already deduplicated by value upstream — and a painter
/// uploading this wants one contiguous run per region regardless.
///
/// Story #583 measured that argument rather than leaving it an argument
/// (issue #133, closed won't-fix). On `dacef5e`, across the twenty Figma
/// fixtures that compile clean and all three showcase scenes: the deepest
/// ancestor chain anywhere is **3**, it is **1** in every scene the lean
/// painter draws, and the duplication costs **19 boxes — 608 bytes — across
/// the whole corpus**. The third claim above was a prediction when it was
/// written and now has a consumer confirming it: `clip_coverage` in
/// `dashscene-gpu` loops one contiguous run per region, so prefix sharing
/// would have to be flattened away again before upload, and a parent-pointer
/// chain would replace a run of at most three boxes with a dependent pointer
/// chase per fragment. These are the depths that were measurable then, not a
/// bound on what a deeper imported design could produce.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipTable {
    entries: Vec<ClipRegion>,
    boxes: Vec<ClipBox>,
}

impl Default for ClipTable {
    fn default() -> Self {
        Self {
            entries: vec![ClipRegion::unclipped()],
            boxes: Vec::new(),
        }
    }
}

// A `ClipTable` is never empty — index 0 is the reserved unclipped
// region — so an `is_empty` that always answers false would be a trap.
#[allow(clippy::len_without_is_empty)]
impl ClipTable {
    /// A table holding only the reserved unclipped region.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a region over `boxes` (outermost ancestor first) and returns
    /// its index — the value a [`RectEntry::clip`] field holds to reference
    /// it.
    ///
    /// Takes the boxes rather than a [`ClipRegion`] because the range is
    /// only meaningful against this table's flat array, so only this table
    /// can produce a correct one.
    pub fn push(&mut self, boxes: &[ClipBox]) -> ClipIndex {
        let index = u32::try_from(self.entries.len()).expect("clip table exceeds u32::MAX entries");
        let offset =
            u32::try_from(self.boxes.len()).expect("clip table exceeds u32::MAX clip boxes");
        let count = u32::try_from(boxes.len()).expect("a clip region exceeds u32::MAX boxes");
        self.boxes.extend_from_slice(boxes);
        self.entries.push(ClipRegion { offset, count });
        ClipIndex(index)
    }

    pub fn get(&self, index: ClipIndex) -> Option<ClipView<'_>> {
        let region = *self.entries.get(index.0 as usize)?;
        Some(self.view(region))
    }

    /// Resolves a rect's clip index. This is the lookup painters use.
    ///
    /// Panics on an out-of-range index, for the same reason as
    /// [`PaintTable::resolve`]: a miss is a broken contract between
    /// crates, and a painter must never skip a rect's clip silently
    /// (P4).
    pub fn resolve(&self, index: ClipIndex) -> ClipView<'_> {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "clip index {} out of range ({} regions): clip indices are validated upstream (P4)",
                index.0,
                self.entries.len()
            )
        })
    }

    /// The stored range for an index, without its boxes — what a caller
    /// packing an instance buffer writes, rather than what a painter draws.
    ///
    /// Panics on an out-of-range index, same contract as
    /// [`resolve`](Self::resolve).
    pub fn region(&self, index: ClipIndex) -> ClipRegion {
        self.resolve(index).region()
    }

    /// Every box in the table, in one flat array. A [`ClipRegion`]'s
    /// `offset` and `count` index into this.
    pub fn all_boxes(&self) -> &[ClipBox] {
        &self.boxes
    }

    /// Region count, including the reserved unclipped region at index 0.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Pairs a stored range with the boxes it names.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat array.
    ///
    /// No public path reaches this today: only [`push`](Self::push) writes
    /// ranges, and it writes them from the array's own length, so every
    /// stored range is in bounds by construction. It is a named panic rather
    /// than the slice indexer's because the failure it describes — a range
    /// and the array it indexes built apart from each other — is the one
    /// defect this shape introduces that a `Vec` per region could not have,
    /// and because the next story to write ranges here will not be `push`.
    fn view(&self, region: ClipRegion) -> ClipView<'_> {
        let start = region.offset as usize;
        let end = start + region.count as usize;
        let boxes = self.boxes.get(start..end).unwrap_or_else(|| {
            panic!(
                "clip region {}..{} runs past the table's {} boxes: a region and the array it \
                 indexes must be built together",
                start,
                end,
                self.boxes.len()
            )
        });
        ClipView { region, boxes }
    }
}

/// Which per-kind table a [`PaintKind`] indexes.
///
/// The tag half of the tag-plus-index form story #578 gave the fill
/// vocabulary. A payload-carrying enum has no clean C form — it is a
/// tag-plus-union, which means explicit-layout structs in C# and which
/// Burst handles badly — and the repo already mandates integer indices for
/// cross-table references (`docs/decisions/dsb-sectioned-container.md`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PaintTag {
    /// The paint-less node: a layout-only container draws nothing but still
    /// occupies its rect-table slot. First variant, so a zeroed
    /// [`PaintKind`] is the fill-less one and `Default` agrees.
    #[default]
    None,
    Solid,
    Gradient,
    Image,
}

/// One way to fill a rect: which kind, and which row of that kind's table
/// in the [`PaintTable`] holds its parameters. Shadows are a separate
/// per-entry list ([`PaintEntry::shadows`]), not a fill kind; masks resolve
/// into clip regions, not paint.
///
/// Read it with [`PaintTable::fill`], which returns the borrowed [`Fill`]
/// view — the form painters match on. Producers do not build one of these
/// directly: they hand a [`FillSpec`] to the table, which interns the
/// parameters and returns the index.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PaintKind {
    pub tag: PaintTag,
    /// Row in the table named by `tag` — [`PaintTable::all_solids`],
    /// [`PaintTable::all_gradients`] or [`PaintTable::all_images`].
    /// Meaningless, and zero, when `tag` is [`PaintTag::None`].
    pub index: u32,
}

impl PaintKind {
    /// The fill a paint-less node carries (story #578). Replaced
    /// `PaintEntry`'s `Option<PaintKind>`, which has no C representation:
    /// `Option<T>` needs a niche and this struct has none.
    pub const NONE: Self = Self {
        tag: PaintTag::None,
        index: 0,
    };
}

/// An image fill's parameters, as one table row.
///
/// `transform` lost its `Option` in story #578 — `Option<Mat23>` has no C
/// representation, and [`Mat23::IDENTITY`] is exactly what the `None` meant
/// (an image fill that names no crop transform samples the whole image).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageFill {
    /// Index into the [`ImageTable`].
    pub image: u32,
    pub scale_mode: ScaleMode,
    /// Normalized image-space transform for [`ScaleMode::Crop`];
    /// [`Mat23::IDENTITY`] for a fill that crops nothing.
    pub transform: Mat23,
    /// Tile magnification for [`ScaleMode::Tile`].
    pub tile_scale: f32,
}

/// A fill as a producer authors it, with its gradient stops still owned.
///
/// The producer-side twin of [`PaintKind`]: a producer cannot know where
/// its parameters will land in a table it has not entered, so it describes
/// the fill and the table assigns the index. Same split as
/// [`GlyphRunTable::push_run`]'s, and the reason is the same — an index
/// assigned anywhere but the table it indexes is an index that will be
/// silently replaced.
#[derive(Debug, Clone, PartialEq)]
pub enum FillSpec {
    Solid {
        color: Color,
    },
    Gradient {
        /// Handles and kind. Its [`Gradient::stops`] must be
        /// [`StopRange::NONE`] — the table assigns the range.
        gradient: Gradient,
        /// At least one and at most [`MAX_GRADIENT_STOPS`] stops.
        stops: Vec<GradientStop>,
    },
    Image(ImageFill),
}

/// A fill as a painter reads it: the table row, borrowed, with a
/// gradient's stops resolved to the slice they name.
///
/// The form [`PaintTable::fill`] returns, and the one painters match on —
/// the flattened [`PaintKind`] is for uploading, not for reading. Without
/// this view every painter would repeat the same tag-match plus bounds
/// check, and the fill vocabulary has enough call sites in this workspace
/// for that repetition to be where a wrong table gets indexed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill<'a> {
    /// The paint-less node. An arm rather than an `Option` around the enum,
    /// so a painter's `match` is exhaustive over it and cannot forget the
    /// case — the same reason `docs/decisions/boundary-b-unification.md`
    /// gives for every rect resolving.
    None,
    Solid(Color),
    Gradient(GradientView<'a>),
    Image(&'a ImageFill),
}

impl Fill<'_> {
    /// The producer-side description of this fill — what re-interning it
    /// into another table takes.
    ///
    /// `dashscene-core`'s table compaction is the caller: it rebuilds a
    /// fresh [`PaintTable`] from the entries still referenced, and a fill
    /// index names a row in the table that interned it, so a re-homed entry
    /// has to re-intern rather than carry its index over.
    pub fn to_spec(&self) -> Option<FillSpec> {
        match self {
            Fill::None => None,
            Fill::Solid(color) => Some(FillSpec::Solid { color: *color }),
            Fill::Gradient(view) => Some(FillSpec::Gradient {
                gradient: Gradient {
                    stops: StopRange::NONE,
                    ..*view.gradient
                },
                stops: view.stops.to_vec(),
            }),
            Fill::Image(image) => Some(FillSpec::Image(**image)),
        }
    }
}

/// A gradient and the stops its [`StopRange`] names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientView<'a> {
    pub gradient: &'a Gradient,
    pub stops: &'a [GradientStop],
}

/// Whether a shadow falls behind the node (a drop shadow) or inside it
/// (an inner shadow). Mirrors `dashbuf`'s `ShadowKind`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowKind {
    Drop,
    Inner,
}

/// One drop or inner shadow (v0.8, story #45,
/// `docs/decisions/effects-vocabulary-shadows.md`). Authored intent —
/// offset, blur radius, spread, and color. The resolved shadow geometry
/// (spread-expanded, offset, blurred) is per-painter math the painter
/// derives from the rect's box and the entry's corners at draw time (P1);
/// this carries only the parameters.
///
/// `blur` is the Gaussian blur radius in document units (non-negative);
/// `spread` grows the shadow shape — a drop shadow outward, an inner
/// shadow's lit hole inward — and may be negative. Ranges are validated
/// upstream (`dashscene-validator`, P4).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub kind: ShadowKind,
    pub offset: Vec2,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

/// Which content a blur applies to (v0.11, story #393,
/// `docs/decisions/backdrop-blur-is-core-vocabulary.md`).
///
/// The distinction is not cosmetic. `Layer` is node-local like every effect
/// before it — the node's own composited content is blurred. `Backdrop` is
/// the first effect that requires a painter to read what is *already*
/// composited beneath the node, seen through the node's own transparency,
/// which is why it carries an ordering guarantee the other effects do not
/// (see [`Painter::paint`]).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurKind {
    Layer,
    Backdrop,
}

/// The Gaussian sigma one unit of blur radius maps to — Figma's measured
/// constant (`docs/decisions/blur-sigma-is-figmas-mapping.md`, issue #412).
///
/// **Not a contract term, and the distinction matters.** A blend space two
/// painters disagree on produces visibly different pixels from the same
/// document, which is why [`Blur`] states that one as a requirement. This is
/// the reference painter's *measured* value instead: a painter should match it
/// where it reasonably can, and one approximating the blur on constrained
/// hardware will not match it exactly.
///
/// It lives here, on boundary B, because two painters had it — `dashscene-skia`
/// measured it and `dashscene-gpu` would otherwise restate it — and a constant
/// stated twice with nothing holding the copies together is the failure the
/// scale-mode and gradient-kind tests exist to catch. One radius maps through
/// one number, whoever is drawing.
///
/// The mapping applies to a shadow's blur (story #45) and to a backdrop blur
/// (story #393) alike; that single-mapping claim is measured rather than
/// assumed, and `dashscene_skia::blur_sigma` carries the table it was fitted
/// against.
pub const BLUR_SIGMA_PER_RADIUS: f32 = 0.4375;

/// One blur (v0.11, story #393). Authored intent: which content is blurred
/// and by how much. `radius` is the Gaussian blur radius in document units,
/// non-negative, carried verbatim from the document — the sigma mapping
/// ([`BLUR_SIGMA_PER_RADIUS`], Figma's measured constant —
/// `docs/decisions/blur-sigma-is-figmas-mapping.md`) is derived at draw time
/// rather than carried in the document (P1), exactly as it is for
/// [`Shadow::blur`]. Unlike the blend space below, the constant is the
/// reference painter's measured value rather than a contract term: a painter
/// should match it where it reasonably can, and one approximating the blur on
/// constrained hardware will not match it exactly.
///
/// Only `Backdrop` is produced today. Layer blur is budgeted at v1 and needs
/// no change here when it lands, which is the reason the kind exists now
/// rather than being inferred from context.
///
/// # The blend space is part of this contract
///
/// **A painter must average the blur kernel over raw sRGB-encoded channel
/// values, not over linear light**
/// (`docs/decisions/blur-blends-in-srgb-encoded-space.md`). Unlike the sigma
/// mapping above, this is *not* per-painter math: two painters that blur in
/// different spaces produce visibly different pixels from the same document,
/// so leaving it free would break the premise that this boundary is a
/// contract. The divergence is large, not marginal — across a saturated
/// colour seam the two spaces disagree by roughly 50 code points at the
/// midpoint of the transition.
///
/// The value is Figma's, measured rather than chosen: over the
/// `backdrop-blur` oracle frame, sRGB-encoded blending sits a mean of 1.187
/// code points from Figma's own render at its best-fitting sigma against
/// 10.363 for linear light, and it fits better at every sigma from 0.20 to
/// 0.60 · radius than linear light does at its own best. Both blur oracle
/// frames fail a linear-light blend, at 5.429 % and 4.866 % against a 2 %
/// budget.
///
/// For the reference painter this falls out of allocating a surface with no
/// colour space attached, which is also what keeps MSDF distance channels
/// sampling raw — one allocation, two requirements, no conflict. A painter
/// built on a pipeline that is linear by default has to convert deliberately.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blur {
    pub kind: BlurKind,
    pub radius: f32,
}

/// A resolved baked-vector coverage mask (story B1,
/// `docs/wip/2026-07-19-B1-vector-msdf-design.md`) — the runtime form of a
/// Figma VECTOR node's shape. A paint entry carrying `Some(VectorField)`
/// samples this multi-channel signed-distance field as a coverage mask and
/// composes it with the entry's ordinary `fill` (solid or gradient), so the
/// painter never rasterizes a path (P2). `None` is the implicit parametric
/// (rounded-rect) shape — unchanged from every pre-B1 entry.
///
/// The atlas image lives in the [`ImageTable`] the same way an image fill's
/// bytes do (`image` indexes it); it is a lossless PNG of the packed RGB
/// distance channels. `atlas_rect` is this shape's sub-rect in that image, in
/// texels (`[x, y, width, height]`, top-left origin). `plane_bounds` is the
/// padded field quad in the shape's own coordinate space, relative to the
/// node box origin, y-down (`[left, top, right, bottom]`) — the painter maps
/// it to device space by `device = rect_origin + plane_bounds` (unit scale).
///
/// Plain mirror of `dashbuf`'s `VectorAtlas` + `VectorShape` tables, resolved
/// (atlas → image index, `distance_range` folded in) at load time, so the
/// painter needs no pool walk.
///
/// Carries no `px_per_em`, unlike the glyph [`Atlas`] (debt #358). A glyph
/// run renders at a size the bake resolution cannot imply — the same atlas
/// serves many run sizes — so the painter divides by `Atlas::px_per_em` to
/// find the scale. A vector field has no equivalent free variable: its
/// `atlas_rect`-to-device-quad ratio already is the scale, so a bake
/// resolution carried alongside it would be redundant for every reader that
/// only paints.
///
/// # Why the domain check is not a constructor here
///
/// [`distance_range`](Self::distance_range) has a domain — finite and greater
/// than zero — and it is refused at [`PaintTable::push_with`] rather than by a
/// constructor on this type (issue #986). The reason is cost, not
/// impossibility: private fields plus accessors would work for this type, and
/// would mean rewriting every literal and every read of it outside `dashpaint`
/// — including both painters' draw paths. Keeping the fields public and
/// adding a constructor
/// beside them would not be a check at all — that is the shape PR #983's review
/// rejected on [`Atlas`], where `px_per_em` had to become private before its
/// check meant anything.
///
/// `docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md` records
/// the choice, including what it gives up. [`Atlas`] holds its own invariant in
/// a constructor because it already had one to put the check in; this type has
/// none, and the table it enters is a seam every field passes through.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorField {
    /// Index into the [`ImageTable`] — the packed MSDF atlas PNG.
    pub image: u32,
    /// This shape's sub-rect in the atlas, in texels: `[x, y, width, height]`,
    /// top-left origin.
    ///
    /// A zero width or height means the field draws nothing —
    /// [`draws`](Self::draws) rejects it and both painters take that answer.
    /// **Not at the same point**: `dashscene-gpu` asks before making the atlas
    /// resident, so a rejected field costs it no upload, while `dashscene-skia`
    /// has already decoded the atlas by the time `field_coverage` asks. That is
    /// a legal state, not an out-of-domain one, so nothing refuses it.
    pub atlas_rect: [u32; 4],
    /// The padded field quad in shape space, node-box-relative, y-down:
    /// `[left, top, right, bottom]`.
    ///
    /// A quad whose width or height is not finite and positive means the field
    /// draws nothing, decided by the same [`draws`](Self::draws) predicate
    /// `atlas_rect` names.
    ///
    /// **The test is on the extent, not on the four bounds** (issue #1034).
    /// Ordering them alone rejected a NaN — every comparison against one is
    /// false — and admitted an infinity, since `f32::INFINITY > left` is true;
    /// and requiring each bound finite still admitted `[-3.0e38, 0.0, 3.0e38,
    /// 8.0]`, whose difference overflows `f32`. Each of those drove both
    /// painters' screen-pixel range to an infinity, and the reference painter's
    /// backdrop erased what was beneath the node.
    ///
    /// **Rejected by the painters, refused by no seam.** [`PaintTable::push_with`]
    /// does not refuse such a quad and `dashscene-validator` has no rule over
    /// this member, so it is a legal draws-nothing state exactly as a zero
    /// `atlas_rect` extent is. Whether it should instead be out of domain, beside
    /// the [`distance_range`](Self::distance_range) check, is the half of issue
    /// #1034 that stays open.
    pub plane_bounds: [f32; 4],
    /// The MSDF distance range in atlas texels (msdfgen `-pxrange`).
    ///
    /// Finite and greater than zero for every field that reached a painter:
    /// [`PaintTable::push_with`] refuses anything else (issue #986). It is the
    /// coverage-mask twin of [`Atlas::distance_range_px`], and out of that
    /// domain it paints the same three wrong pictures that constructor's
    /// documentation enumerates.
    pub distance_range: f32,
}

impl VectorField {
    /// Whether this field draws at all: a quad of finite, positive extent over
    /// an atlas rectangle with texels in it.
    ///
    /// **The one predicate both painters take** (issue #1144). It was restated
    /// in each of them and kept in step by prose, and that convention failed
    /// twice: issue #1000 was the two disagreeing about which fields draw, and
    /// issue #1034 was the restated predicate being wrong in both. Each fix was
    /// "change both copies and say so in a comment". Here the agreement is
    /// structural — `dashscene-skia` does not depend on `dashscene-gpu`, but
    /// both depend on this crate and both already take a `&VectorField`.
    ///
    /// # What it decides, and what it deliberately does not
    ///
    /// This answers "draws nothing", which is a **legal** state. It is not a
    /// domain check: [`PaintTable::push_with`] is where an out-of-domain operand
    /// is refused, and a zero [`atlas_rect`](Self::atlas_rect) extent is
    /// deliberately not refused there, because a field with no texels to sample
    /// is something a producer may legitimately emit.
    ///
    /// # Why the extent and not the four bounds
    ///
    /// **The subtraction is the operation whose domain matters**, so it is the
    /// one tested. Both painters scale by the quad's extent rather than
    /// dividing by it — `dashscene-gpu` computes `distance_range * (right -
    /// left) / aw` and `dashscene-skia` computes `dest.width() / aw`, each
    /// dividing by the *atlas* rectangle — and a non-finite extent carries
    /// straight through either multiplication into the screen-pixel range and
    /// into the shader's local matrix.
    ///
    /// `is_finite` over the difference rejects all three ways out at once: a NaN
    /// bound makes it a NaN, an infinite bound makes it infinite or a NaN, and
    /// two large finite bounds whose difference overflows make it infinite.
    /// Testing the bounds individually admits the last of those, which is how
    /// issue #1034's first fix was still wrong. For finite bounds `width > 0.0`
    /// and `right > left` agree exactly, so nothing that drew before stops
    /// drawing.
    ///
    /// # The rejection is silent
    ///
    /// No diagnostic names a field this rejects, at this seam or any other,
    /// which is issue #1021 and P4's complaint about all of them rather than a
    /// property this predicate adds.
    #[must_use]
    pub fn draws(&self) -> bool {
        let [left, top, right, bottom] = self.plane_bounds;
        let (width, height) = (right - left, bottom - top);
        width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0
            && self.atlas_rect[2] > 0
            && self.atlas_rect[3] > 0
    }
}

/// Where one entry's shadows sit in the [`PaintTable`]'s flat shadow array.
///
/// The effect-side twin of [`ClipRegion`] and [`GlyphRange`], for the same
/// two reasons: a `Vec` per entry has no C representation, and a flat array
/// plus a range uploads as one buffer copy (story #578).
///
/// A distinct type rather than one shared span type, following the rule
/// [`PaintIndex`], [`ClipIndex`] and [`AtlasIndex`] already follow here: a
/// range into the shadow array cannot be passed where a range into the blur
/// array belongs.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShadowRange {
    /// First shadow, as an index into [`PaintTable::all_shadows`].
    pub offset: u32,
    /// How many shadows, in paint order. Zero = none.
    pub count: u32,
}

impl ShadowRange {
    /// The range an entry carries before [`PaintTable::push_with`]
    /// assigns it one, and the range of an entry with no shadows. Both mean
    /// "names nothing", which is why one value serves both.
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// Where one entry's blurs sit in the [`PaintTable`]'s flat blur array.
/// Sibling of [`ShadowRange`]; see it for why these are two types.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BlurRange {
    /// First blur, as an index into [`PaintTable::all_blurs`].
    pub offset: u32,
    /// How many blurs. Zero = none.
    pub count: u32,
}

impl BlurRange {
    /// The range an entry carries before [`PaintTable::push_with`]
    /// assigns it one, and the range of an entry with no blurs.
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// Where one entry's stacked fill layers sit in the [`PaintTable`]'s flat
/// fill array. Sibling of [`ShadowRange`]; see it for why these are
/// separate types.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FillRange {
    /// First layer, as an index into [`PaintTable::all_extra_fills`].
    pub offset: u32,
    /// How many layers, bottom to top. Zero = a single-fill or fill-less
    /// entry.
    pub count: u32,
}

impl FillRange {
    /// The range an entry carries before [`PaintTable::push_with`] assigns
    /// it one, and the range of an entry with no stacked layers.
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// Where one entry's stroke sits in the [`PaintTable`]'s flat stroke array.
///
/// A range rather than an index-plus-sentinel for a member that is at most
/// one, because an empty range needs no skip rule at the read site — the
/// property `docs/decisions/boundary-b-unification.md` chose over a
/// sentinel painters must each remember to test. `count` is 0 or 1;
/// anything higher is refused upstream (P4), the same way
/// [`MAX_GRADIENT_STOPS`] bounds a gradient's stops rather than the type
/// doing it.
///
/// It also leaves the stroke half of debt #146 expressible without a second
/// migration: a node that one day stacks strokes needs a wider arity bound
/// here, not a different shape.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StrokeRange {
    /// The stroke, as an index into [`PaintTable::all_strokes`].
    pub offset: u32,
    /// 1 for a stroked entry, 0 for an unstroked one.
    pub count: u32,
}

impl StrokeRange {
    /// The range an unstroked entry carries, and what an entry arrives with
    /// before [`PaintTable::push_with`] assigns one.
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// Where one entry's baked-vector coverage mask sits in the
/// [`PaintTable`]'s flat shape array. Sibling of [`StrokeRange`], with the
/// same 0-or-1 arity and the same reason for being a range.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShapeRange {
    /// The field, as an index into [`PaintTable::all_shapes`].
    pub offset: u32,
    /// 1 for a masked entry, 0 for the implicit parametric shape.
    pub count: u32,
}

impl ShapeRange {
    /// The range an unmasked entry carries.
    pub const NONE: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// One paint-table entry (docs/design/dashbuf.md's paint-table row: paint-kind
/// enum plus fill/stroke params): what a rect is filled with, how its
/// outline is stroked, how its corners round, and the shadows it casts.
///
/// [`PaintKind::NONE`] as the fill is the paint-less node — a layout-only
/// container draws nothing but still occupies its rect-table slot
/// (index = DFS node index).
///
/// Whether a node clips its children (`Paint.clip`, docs/design/architecture.md)
/// is *intent*, and does not appear here: `dashscene-core` resolves it
/// at commit into the [`ClipTable`] each [`RectEntry::clip`] references
/// (issue #97). The intent itself lives in the document (`dashbuf`'s
/// `Paint.clip`) and in the arena (`Prop::Clip`).
///
/// # Every member is fixed-width, and that is the point
///
/// Story #578 replaced this type's `Option`s and its one `Vec` with row
/// references into the table's flat arrays. Sixty-four bytes, seven
/// members, no pointer to chase and no niche to depend on — which is what
/// lets it cross an `extern "C"` seam and what an instance-buffer upload
/// wants. The producer-side shape, with its lists still owned, is
/// [`EntryParts`].
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PaintEntry {
    /// What the rect is filled with, or [`PaintKind::NONE`]. Read it with
    /// [`PaintTable::fill`].
    pub fill: PaintKind,
    /// Fills stacked over `fill`, bottom to top (story C1, debt #146):
    /// `fill` is the bottom (first visible) layer, and this names every
    /// layer above it, in the order a node's fills paint. A layer's own
    /// opacity is already folded into its color or stops the same way
    /// `fill`'s is. [`FillRange::NONE`] for a single-fill or fill-less
    /// entry; read it with [`PaintTable::extra_fills`].
    pub extra_fills: FillRange,
    /// The node's outline stroke, or [`StrokeRange::NONE`]. Read it with
    /// [`PaintTable::stroke`].
    pub stroke: StrokeRange,
    pub corners: CornerRadii,
    /// The node's drop and inner shadows, in paint order (v0.8, story
    /// #45). Shadows are an effect, not a fill or stroke, so they carry no
    /// arity limit — a node stacks as many as it authors, the same posture
    /// `extra_fills` brought to the fill side (story C1, debt #146).
    ///
    /// A range into the table's flat shadow array since story #578; read it
    /// with [`PaintTable::shadows`]. [`ShadowRange::NONE`] for a node with
    /// no shadows, which is the default.
    pub shadows: ShadowRange,
    /// The node's blurs (v0.11, story #393). [`BlurRange::NONE`] for a node
    /// with no blur, so every pre-v0.11 entry is unchanged. Carried beside
    /// `shadows` because a blur is an effect on the same node and dedups
    /// with the rest of the entry the same way.
    ///
    /// A `BlurKind::Backdrop` entry here is also what declares that the node
    /// samples the already-composited backdrop; there is deliberately no
    /// separate flag saying so, because two records of one fact can
    /// disagree.
    pub blurs: BlurRange,
    /// The baked-vector coverage mask (story B1). A named field masks
    /// `fill` by that field's coverage — a Figma VECTOR shape.
    /// [`ShapeRange::NONE`] is the implicit parametric shape, so every
    /// pre-B1 entry is unchanged. Skipped for a fill-less entry (no ink to
    /// mask). Read it with [`PaintTable::shape`].
    pub shape: ShapeRange,
}

/// Everything an entry owns that lives in the table's flat arrays, as a
/// producer holds it: owned, and with no index into a table it has not
/// entered.
///
/// The producer-side twin of [`PaintEntry`], the same split [`FillSpec`]
/// has from [`PaintKind`]. [`PaintTable::push_with`] copies these in and
/// assigns every range.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntryParts<'a> {
    /// At most [`MAX_GRADIENT_STOPS`]-independent: any number of layers,
    /// bottom to top.
    pub extra_fills: &'a [PaintKind],
    pub stroke: Option<Stroke>,
    pub shape: Option<VectorField>,
    pub shadows: &'a [Shadow],
    pub blurs: &'a [Blur],
}

/// The paint table (docs/design/dashbuf.md): dense, indexed by `RectEntry.paint`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintTable {
    entries: Vec<PaintEntry>,
    /// Every entry's shadows, in one flat array (story #578). A
    /// [`ShadowRange`] indexes into this.
    shadows: Vec<Shadow>,
    /// Every entry's blurs, likewise, named by a [`BlurRange`].
    blurs: Vec<Blur>,
    /// Every entry's stacked fill layers, flat; a [`FillRange`] indexes
    /// into this. Not the same array as the per-kind tables below: this
    /// holds *references* in paint order, those hold the parameters.
    extra_fills: Vec<PaintKind>,
    /// Every entry's stroke, flat; a [`StrokeRange`] indexes into this.
    strokes: Vec<Stroke>,
    /// Every entry's baked-vector coverage mask; a [`ShapeRange`] indexes
    /// into this.
    shapes: Vec<VectorField>,
    /// The per-kind fill tables a [`PaintKind`] indexes (story #578), one
    /// per [`PaintTag`]. Deduplicated on the way in by
    /// [`intern_fill`](Self::intern_fill), so a scene's distinct fills are
    /// what these hold however many entries reference them.
    solids: Vec<Color>,
    gradients: Vec<Gradient>,
    /// Every gradient's stops, flat; a [`StopRange`] indexes into this.
    stops: Vec<GradientStop>,
    images: Vec<ImageFill>,
}

impl PaintTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a fill's parameters and returns the [`PaintKind`] naming
    /// them — the value a [`PaintEntry::fill`] holds.
    ///
    /// Deduplicated: an identical fill returns the index it already has.
    /// This is where fills differ from shadows and blurs, which
    /// [`push_with`](Self::push_with) copies per entry
    /// without dedup. A shadow list belongs to one entry and has no
    /// identity beyond it; a fill is a shared value, and the retained
    /// interner in `dashscene-core` re-stages the same fills on every
    /// commit — appending each time would grow these tables without bound
    /// across a session's frames, which the entry-level interner cannot
    /// see because two equal fills would reach it as two different indices.
    ///
    /// # Panics
    ///
    /// Panics if a gradient arrives with a [`StopRange`] already assigned —
    /// a range that would be silently replaced, refused by name for the
    /// reason [`push_with`](Self::push_with) gives.
    ///
    /// It deliberately does **not** check the stop count against
    /// [`MAX_GRADIENT_STOPS`]. That is a vocabulary rule, and P4 puts
    /// vocabulary rules in `dashscene-validator` as named diagnostics
    /// (`GRADIENT_NO_STOPS`, `GRADIENT_STOP_BUDGET`). Asserting it here
    /// would move the rule's enforcement into a panic that fires first and
    /// makes those diagnostics unreachable — the validator could no longer
    /// report what it exists to report.
    pub fn intern_fill(&mut self, spec: &FillSpec) -> PaintKind {
        match spec {
            FillSpec::Solid { color } => {
                let index = match self.solids.iter().position(|c| c == color) {
                    Some(i) => i,
                    None => {
                        self.solids.push(*color);
                        self.solids.len() - 1
                    }
                };
                PaintKind {
                    tag: PaintTag::Solid,
                    index: Self::fill_index(index),
                }
            }
            FillSpec::Gradient { gradient, stops } => {
                assert_eq!(
                    gradient.stops,
                    StopRange::NONE,
                    "intern_fill assigns a gradient's stop range; the gradient must arrive with \
                     StopRange::NONE, not offsets into some other table"
                );
                let existing = self
                    .gradients
                    .iter()
                    .position(|g| Self::same_gradient(g, self.stops_of(g), gradient, stops));
                let index = match existing {
                    Some(i) => i,
                    None => {
                        // Through `flat_range` rather than two `fill_index`
                        // calls: a count and an offset that each fit a `u32`
                        // can still name an end past one, which is the shape
                        // issue #1014 names in `push_with` and this is the
                        // second instance of it. Not `span`, whose empty-range
                        // canonicalization would move a no-stop gradient's
                        // offset and is a property entry equality depends on.
                        let (offset, count) = Self::flat_range(self.stops.len(), stops.len());
                        self.stops.extend_from_slice(stops);
                        self.gradients.push(Gradient {
                            stops: StopRange { offset, count },
                            ..*gradient
                        });
                        self.gradients.len() - 1
                    }
                };
                PaintKind {
                    tag: PaintTag::Gradient,
                    index: Self::fill_index(index),
                }
            }
            FillSpec::Image(image) => {
                let index = match self.images.iter().position(|i| i == image) {
                    Some(i) => i,
                    None => {
                        self.images.push(*image);
                        self.images.len() - 1
                    }
                };
                PaintKind {
                    tag: PaintTag::Image,
                    index: Self::fill_index(index),
                }
            }
        }
    }

    /// Whether an interned gradient and a candidate describe the same fill
    /// — handles and kind, and the stops themselves rather than the ranges
    /// naming them, since the candidate has no range yet.
    fn same_gradient(
        interned: &Gradient,
        interned_stops: &[GradientStop],
        candidate: &Gradient,
        candidate_stops: &[GradientStop],
    ) -> bool {
        interned.kind == candidate.kind
            && interned.handle_origin == candidate.handle_origin
            && interned.handle_primary == candidate.handle_primary
            && interned.handle_secondary == candidate.handle_secondary
            && interned_stops == candidate_stops
    }

    /// One range's `(offset, count)`, with an empty range canonicalized to
    /// `(0, 0)`.
    ///
    /// An empty range names nothing, so where it would have started carries
    /// no information — but it is still *observable*, and two entries that
    /// both draw nothing would compare unequal for differing in it. That
    /// breaks entry equality, and with it every comparison against
    /// [`PaintEntry::default`], for a difference that means nothing. Every
    /// `NONE` const in this crate is `(0, 0)`, and this is what keeps an
    /// assigned empty range equal to one.
    fn span(array_len: usize, count: usize) -> (u32, u32) {
        if count == 0 {
            return (0, 0);
        }
        Self::flat_range(array_len, count)
    }

    /// `(offset, count)` for `count` rows about to be appended at `array_len`,
    /// refusing a range whose **end** runs past `u32::MAX`.
    ///
    /// The sum, not the two lengths separately (issue #1014). Converting each
    /// alone accepts an array at `u32::MAX - 999` taking a 1000-row part: both
    /// conversions succeed, the append takes the array past what a `u32` offset
    /// can address, and the range handed back names rows the next caller cannot
    /// reach. The refusal then lands on whichever later push happens to tip the
    /// offset itself over, a call away from the cause.
    ///
    /// Unreachable in practice on the target hardware — every array this bounds
    /// is a boundary-B row array, and four billion of any of them is orders of
    /// magnitude past a fixed memory budget. It is checked because it is a
    /// contract these methods state.
    fn flat_range(array_len: usize, count: usize) -> (u32, u32) {
        let offset = Self::fill_index(array_len);
        let count = Self::fill_index(count);
        if offset.checked_add(count).is_none() {
            panic!(
                "a part of {count} rows appended at offset {offset} would end past u32::MAX: \
                 boundary B names a flat array's rows with a u32 offset and a u32 count, so \
                 nothing past that end has a range that can be expressed (story #578)"
            );
        }
        (offset, count)
    }

    /// A row index, offset or count as the flattened types carry them.
    ///
    /// `u32` rather than `usize` because these cross boundary B, where a
    /// pointer-sized integer differs by target (story #578).
    fn fill_index(len: usize) -> u32 {
        u32::try_from(len).expect("a paint table's fill rows exceed u32::MAX")
    }

    /// The fill `kind` names, borrowed, with a gradient's stops resolved.
    ///
    /// # Panics
    ///
    /// Panics on an index past the end of the table `kind.tag` names. An
    /// out-of-range fill index is a broken contract between crates, exactly
    /// as [`resolve`](Self::resolve)'s is — most often a [`PaintKind`]
    /// interned into one table and read from another.
    pub fn fill(&self, kind: PaintKind) -> Fill<'_> {
        match kind.tag {
            PaintTag::None => Fill::None,
            PaintTag::Solid => {
                Fill::Solid(*self.solids.get(kind.index as usize).unwrap_or_else(|| {
                    panic!(
                        "solid fill {} out of range: the table holds {}",
                        kind.index,
                        self.solids.len()
                    )
                }))
            }
            PaintTag::Gradient => {
                let gradient = self.gradients.get(kind.index as usize).unwrap_or_else(|| {
                    panic!(
                        "gradient fill {} out of range: the table holds {}",
                        kind.index,
                        self.gradients.len()
                    )
                });
                Fill::Gradient(GradientView {
                    gradient,
                    stops: self.stops_of(gradient),
                })
            }
            PaintTag::Image => {
                Fill::Image(self.images.get(kind.index as usize).unwrap_or_else(|| {
                    panic!(
                        "image fill {} out of range: the table holds {}",
                        kind.index,
                        self.images.len()
                    )
                }))
            }
        }
    }

    /// The stops `gradient`'s [`StopRange`] names.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat stop array, for the reason
    /// [`fill`](Self::fill) gives.
    pub fn stops(&self, gradient: &Gradient) -> &[GradientStop] {
        self.stops_of(gradient)
    }

    fn stops_of(&self, gradient: &Gradient) -> &[GradientStop] {
        let start = gradient.stops.offset as usize;
        let end = start + gradient.stops.count as usize;
        self.stops.get(start..end).unwrap_or_else(|| {
            panic!(
                "stop range {start}..{end} out of range: the table holds {}",
                self.stops.len()
            )
        })
    }

    /// The fill layers stacked over `entry`'s own fill, bottom to top.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat array, for the reason
    /// [`shadows`](Self::shadows) gives.
    pub fn extra_fills(&self, entry: &PaintEntry) -> &[PaintKind] {
        let start = entry.extra_fills.offset as usize;
        let end = start + entry.extra_fills.count as usize;
        self.extra_fills.get(start..end).unwrap_or_else(|| {
            panic!(
                "fill range {start}..{end} runs past the table's {} stacked fills: an entry and \
                 the table it is read against must be the same one",
                self.extra_fills.len()
            )
        })
    }

    /// The stroke `entry` carries, or `None` for an unstroked entry.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat array, or names more than one
    /// stroke — an arity the vocabulary does not have (debt #146's stroke
    /// half is untouched), and one no painter is written for.
    pub fn stroke(&self, entry: &PaintEntry) -> Option<&Stroke> {
        assert!(
            entry.stroke.count <= 1,
            "entry names {} strokes; the vocabulary is single-stroke",
            entry.stroke.count
        );
        let start = entry.stroke.offset as usize;
        let end = start + entry.stroke.count as usize;
        self.strokes
            .get(start..end)
            .unwrap_or_else(|| {
                panic!(
                    "stroke range {start}..{end} runs past the table's {} strokes: an entry and \
                     the table it is read against must be the same one",
                    self.strokes.len()
                )
            })
            .first()
    }

    /// The baked-vector coverage mask `entry` is masked by, or `None` for
    /// the implicit parametric shape.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat array, or names more than one
    /// field — a node has at most one coverage mask.
    pub fn shape(&self, entry: &PaintEntry) -> Option<&VectorField> {
        assert!(
            entry.shape.count <= 1,
            "entry names {} coverage masks; a node has at most one",
            entry.shape.count
        );
        let start = entry.shape.offset as usize;
        let end = start + entry.shape.count as usize;
        self.shapes
            .get(start..end)
            .unwrap_or_else(|| {
                panic!(
                    "shape range {start}..{end} runs past the table's {} shapes: an entry and \
                     the table it is read against must be the same one",
                    self.shapes.len()
                )
            })
            .first()
    }

    /// Every entry's stacked fill layers, flat — what a [`FillRange`]
    /// indexes.
    pub fn all_extra_fills(&self) -> &[PaintKind] {
        &self.extra_fills
    }

    /// Every entry's stroke, flat — what a [`StrokeRange`] indexes.
    pub fn all_strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Every entry's coverage mask, flat — what a [`ShapeRange`] indexes.
    pub fn all_shapes(&self) -> &[VectorField] {
        &self.shapes
    }

    /// Every interned solid color, in index order.
    pub fn all_solids(&self) -> &[Color] {
        &self.solids
    }

    /// Every interned gradient, in index order.
    pub fn all_gradients(&self) -> &[Gradient] {
        &self.gradients
    }

    /// Every gradient's stops, flat — what a [`StopRange`] indexes.
    pub fn all_stops(&self) -> &[GradientStop] {
        &self.stops
    }

    /// Every interned image fill, in index order.
    pub fn all_images(&self) -> &[ImageFill] {
        &self.images
    }

    /// Appends a bare entry — one that names no stacked fills, no stroke,
    /// no effects and no coverage mask — and returns its index, the value a
    /// [`RectEntry::paint`] field holds to reference it.
    ///
    /// Most entries are this. [`push_with`](Self::push_with) is for the
    /// rest.
    ///
    /// # Panics
    ///
    /// Panics if `entry` already names any of them, which this cannot
    /// honour: it is given no arrays to copy from, so accepting the entry
    /// would leave its ranges pointing at whatever happened to sit at those
    /// offsets. Refused by name (P4).
    pub fn push(&mut self, entry: PaintEntry) -> PaintIndex {
        assert_eq!(
            (
                entry.extra_fills,
                entry.stroke,
                entry.shadows,
                entry.blurs,
                entry.shape,
            ),
            (
                FillRange::NONE,
                StrokeRange::NONE,
                ShadowRange::NONE,
                BlurRange::NONE,
                ShapeRange::NONE,
            ),
            "push takes a bare entry; an entry naming stacked fills, a stroke, effects or a \
             coverage mask goes through push_with, which is given the arrays to copy them from"
        );
        self.push_entry(entry)
    }

    /// Appends an entry over `parts`, which are copied into the table's
    /// flat arrays and named by the ranges this assigns.
    ///
    /// # Panics
    ///
    /// Panics unless the entry arrives with every range at `NONE`, for the
    /// reason [`GlyphRunTable::push_run`] gives: a caller cannot know where
    /// its parts will land in a table it has not entered, so a range
    /// arriving here is one that will be replaced, and replacing it
    /// silently is how a producer comes to believe its own offsets were
    /// used.
    ///
    /// Panics if `parts.shape`'s [`distance_range`](VectorField::distance_range)
    /// is not finite and greater than zero (issue #986). It is the coverage-mask
    /// path's copy of the operand [`Atlas::new`] refuses for glyphs, and the
    /// three ways out of that domain reach the same three plausible wrong
    /// pictures: zero paints uniform half coverage, a negative value inverts
    /// it — interiors transparent, exteriors opaque — and a NaN or an infinity
    /// reaches the implementation-defined WGSL `clamp`. Both painters compute
    /// it, `dashscene-skia` as `distance_range * sx` and `dashscene-gpu` as
    /// `distance_range * (right - left) / aw`.
    ///
    /// This is the seam because the table's `shapes` array is private and
    /// [`push`](Self::push) refuses an entry that names a shape, so this is the
    /// only way a field reaches a painter. Why not a constructor on
    /// [`VectorField`] is in that type's own documentation.
    ///
    /// # What this seam does not cover
    ///
    /// `atlas_rect`'s width and height are divisors in both painters and are
    /// **not** refused here, deliberately: [`VectorField::draws`] treats a zero
    /// extent as a field that draws nothing, so it is a legal state rather than
    /// an out-of-domain one. Issue #1000 closed the painter half of that
    /// instead, and issue #1144 made the predicate one function both painters
    /// call — so the two agree on such a field without this seam deciding
    /// anything about it.
    ///
    /// **`plane_bounds` is not refused here either, and for the same reason.**
    /// [`VectorField::draws`] rejects a quad whose width or height is not finite
    /// and positive, so both painters agree that such a field draws nothing —
    /// but agreeing to draw nothing is not the same as refusing, and no seam
    /// refuses this one. Whether it should, beside the `distance_range` check
    /// above, is the half of issue #1034 that stays open: nothing produces a
    /// non-finite quad, so it arrives from an authored or corrupted `.dsb`
    /// rather than from the importer.
    ///
    /// # Atomic on its own refusals, and what that does not reach
    ///
    /// **Every refusal above runs before the first flat array is extended**
    /// (issue #1012), so a caller that catches an unwind from any of them holds
    /// the table exactly as it was. The five `extend` calls and the entry's own
    /// append are the last statements in the method and none of them refuses
    /// anything.
    ///
    /// That is a statement about this method, not about a commit. **The
    /// production caller has already mutated this table by the time it calls
    /// here**: `intern_paint` in `dashscene-core` takes `Arc::make_mut` and
    /// interns the entry's own fill and every stacked layer through
    /// [`intern_fill`](Self::intern_fill), growing `solids`, `gradients`,
    /// `stops` and `images` first. A refusal here leaves those interned rows
    /// behind. They are inert — an interned fill is deduplicated by content, so
    /// the next commit that stages the same fill reuses the row rather than
    /// appending beside it — and `dashscene-core`'s `compact_paints` rebuilds
    /// the table from the entries that survived. What is gone is the claim that
    /// a refusal *here* is what leaves the table grown.
    pub fn push_with(&mut self, mut entry: PaintEntry, parts: EntryParts<'_>) -> PaintIndex {
        assert_eq!(
            (
                entry.extra_fills,
                entry.stroke,
                entry.shadows,
                entry.blurs,
                entry.shape,
            ),
            (
                FillRange::NONE,
                StrokeRange::NONE,
                ShadowRange::NONE,
                BlurRange::NONE,
                ShapeRange::NONE,
            ),
            "push_with assigns an entry's ranges; the entry must arrive with every range at \
             NONE, not offsets into some other table"
        );
        // An `if` around a `panic!` rather than an `assert!`, for the reason
        // `GlyphRunTable::push_run`'s guard gives. Above the five `extend`
        // calls, as every refusal in this method now is; see the `# Panics`
        // note for what that does and does not reach.
        if let Some(shape) = parts.shape {
            let range = shape.distance_range;
            if !(range.is_finite() && range > 0.0) {
                panic!(
                    "a coverage mask's distance range is {range}, which is not finite and greater \
                     than zero: every painter scales it into the screen-pixel range it samples \
                     coverage over, and each way out of that domain paints a plausible wrong \
                     picture (P4)"
                );
            }
        }
        let (offset, count) = Self::span(self.extra_fills.len(), parts.extra_fills.len());
        entry.extra_fills = FillRange { offset, count };
        let (offset, count) = Self::span(self.strokes.len(), usize::from(parts.stroke.is_some()));
        entry.stroke = StrokeRange { offset, count };
        let (offset, count) = Self::span(self.shadows.len(), parts.shadows.len());
        entry.shadows = ShadowRange { offset, count };
        let (offset, count) = Self::span(self.blurs.len(), parts.blurs.len());
        entry.blurs = BlurRange { offset, count };
        let (offset, count) = Self::span(self.shapes.len(), usize::from(parts.shape.is_some()));
        entry.shape = ShapeRange { offset, count };
        // The last refusal, and it is here rather than after the extends so
        // that every refusal in this method runs before the first array grows
        // (issue #1012). `push_entry` would raise these two after the copies,
        // leaving the table carrying rows no entry names, so this takes them
        // through `check_entry` and appends the entry itself below.
        let index = self.check_entry(&entry, parts.extra_fills);
        self.extra_fills.extend_from_slice(parts.extra_fills);
        self.strokes.extend(parts.stroke);
        self.shadows.extend_from_slice(parts.shadows);
        self.blurs.extend_from_slice(parts.blurs);
        self.shapes.extend(parts.shape);
        self.entries.push(entry);
        PaintIndex(index)
    }

    /// Interns a solid color and appends the entry that fills a rect with
    /// it and nothing else — the v0.1 walking-skeleton shorthand, and what
    /// most pushes in this workspace are.
    pub fn push_solid(&mut self, color: Color) -> PaintIndex {
        let fill = self.intern_fill(&FillSpec::Solid { color });
        self.push(PaintEntry {
            fill,
            ..PaintEntry::default()
        })
    }

    fn push_entry(&mut self, entry: PaintEntry) -> PaintIndex {
        let index = self.check_entry(&entry, self.extra_fills(&entry));
        self.entries.push(entry);
        PaintIndex(index)
    }

    /// Every refusal appending `entry` can raise, and the index it will take
    /// if none of them fires.
    ///
    /// Separate from the append so that [`push_with`](Self::push_with) can run
    /// both refusals *before* it extends the first flat array (issue #1012).
    /// That caller has not copied its parts in yet, so `entry.extra_fills`
    /// names rows the array does not hold and reading them back through it
    /// would panic here for the wrong reason — it passes the slice it is about
    /// to copy as `layers` instead.
    ///
    /// `push_entry` resolves the entry's own range instead, which for its one
    /// caller is always empty: [`push`](Self::push) refuses an entry arriving
    /// with any range assigned. It is read rather than passed as `&[]` so that
    /// this stays a check over whatever the entry names, and not a second place
    /// stating what that assert already does.
    fn check_entry(&self, entry: &PaintEntry, layers: &[PaintKind]) -> u32 {
        self.check_fills(entry, layers);
        u32::try_from(self.entries.len()).expect("paint table exceeds u32::MAX entries")
    }

    /// Refuses an entry whose fills name rows this table does not hold.
    ///
    /// The fill-side counterpart of the [`ShadowRange::NONE`] assert: a
    /// fill index is legitimately assigned before the entry is pushed, so
    /// it cannot be refused by being present — but it can be refused for
    /// naming a row that is not there, which is what a [`PaintKind`]
    /// interned into a different table looks like. Catching it here names
    /// the producer that mismatched them; letting it through moves the
    /// panic to whichever painter reads the entry first, or, if the other
    /// table happened to be longer, silently paints the wrong fill (P4).
    fn check_fills(&self, entry: &PaintEntry, layers: &[PaintKind]) {
        // `layers` is [`check_entry`]'s argument rather than a list resolved
        // here, and for a caller that has already copied its layers in that
        // list comes from the accessor: it panics by name when a range runs
        // past its array, and this function exists to refuse exactly that kind
        // of mismatched entry. Resolving it any other way would validate
        // however many layers happened to be in reach and wave the rest
        // through, which is the silent drop P4 forbids — in the one place
        // written to catch it.
        for (position, kind) in std::iter::once(&entry.fill).chain(layers).enumerate() {
            let len = match kind.tag {
                // The entry's own fill may name no row — that is the
                // paint-less node, and there is nothing to be in range of.
                // A stacked *layer* may not: a layer exists to add ink, and
                // one naming no fill is a corrupt list rather than an empty
                // one. `dashscene-core`'s table compaction refuses the same
                // state by name.
                PaintTag::None if position == 0 => continue,
                PaintTag::None => panic!(
                    "stacked fill layer {} names no fill; a layer with nothing to paint is a \
                     corrupt list, not an empty one",
                    position - 1
                ),
                PaintTag::Solid => self.solids.len(),
                PaintTag::Gradient => self.gradients.len(),
                PaintTag::Image => self.images.len(),
            };
            assert!(
                (kind.index as usize) < len,
                "entry names {:?} fill {}, but this table holds {len}: a fill index belongs to \
                 the table that interned it",
                kind.tag,
                kind.index
            );
        }
    }

    /// The shadows `entry` casts, in paint order.
    ///
    /// # Panics
    ///
    /// Panics if the range runs past the flat array. Only
    /// [`push_with`](Self::push_with) writes ranges, and it
    /// writes them from the array's own length, so no public path reaches
    /// this today — but an entry read against a table it did not come from
    /// would, which is the hazard a range has and a `Vec` did not.
    pub fn shadows(&self, entry: &PaintEntry) -> &[Shadow] {
        let start = entry.shadows.offset as usize;
        let end = start + entry.shadows.count as usize;
        self.shadows.get(start..end).unwrap_or_else(|| {
            panic!(
                "shadow range {}..{} runs past the table's {} shadows: an entry and the table it \
                 is read against must be the same one",
                start,
                end,
                self.shadows.len()
            )
        })
    }

    /// The blurs `entry` applies. Same contract as
    /// [`shadows`](Self::shadows).
    pub fn blurs(&self, entry: &PaintEntry) -> &[Blur] {
        let start = entry.blurs.offset as usize;
        let end = start + entry.blurs.count as usize;
        self.blurs.get(start..end).unwrap_or_else(|| {
            panic!(
                "blur range {}..{} runs past the table's {} blurs: an entry and the table it is \
                 read against must be the same one",
                start,
                end,
                self.blurs.len()
            )
        })
    }

    /// True when a rect painted from `entry` reads the already-composited
    /// backdrop beneath it, rather than being built from the node's own
    /// geometry alone — that is, when any of its blurs is a
    /// [`BlurKind::Backdrop`]
    /// (`docs/decisions/backdrop-blur-is-core-vocabulary.md`). A
    /// [`BlurKind::Layer`] blur is node-local and does not count.
    ///
    /// This is the property [`Painter::paint`]'s ordering guarantee is
    /// stated over: every rect beneath a rect whose entry answers `true` is
    /// composited before that rect is drawn. A painter finds its barriers by
    /// resolving the paint index it already resolves per rect.
    ///
    /// Moved here from `PaintEntry` by story #578: the answer is derived
    /// from the entry's blurs, and the blurs now live in this table. Still
    /// derived rather than stored, for the reason it always was — a flag
    /// beside `blurs` would be a second copy of one fact, and a struct of
    /// public fields has nothing to keep the two agreeing. Deriving it also
    /// widens the guarantee by itself: a further backdrop-sampling effect
    /// extends this answer, and no painter's barrier handling changes.
    pub fn samples_backdrop(&self, entry: &PaintEntry) -> bool {
        self.blurs(entry)
            .iter()
            .any(|blur| matches!(blur.kind, BlurKind::Backdrop))
    }

    /// Every shadow in the table, in one flat array — the shape an
    /// instance-buffer upload wants.
    pub fn all_shadows(&self) -> &[Shadow] {
        &self.shadows
    }

    /// Every blur in the table, likewise.
    pub fn all_blurs(&self) -> &[Blur] {
        &self.blurs
    }

    pub fn get(&self, index: PaintIndex) -> Option<&PaintEntry> {
        self.entries.get(index.0 as usize)
    }

    /// Resolves a rect's paint index. This is the lookup painters use.
    ///
    /// Panics on an out-of-range index: indices are validated upstream
    /// (P4), so a miss is a broken contract between crates, and the
    /// panic for that case is centralized here — a painter must never
    /// skip a rect silently.
    pub fn resolve(&self, index: PaintIndex) -> &PaintEntry {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "paint index {} out of range ({} entries): paint indices are validated upstream (P4)",
                index.0,
                self.entries.len()
            )
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One glyph placed in absolute document space (y-down, layout origin
/// at the top-left): the pen position on its line's baseline, with the
/// shaping offsets already applied.
///
/// Plain mirror of `dashscene-typeset`'s `PositionedGlyph` (dashpaint
/// depends on no crate — the same reason [`Color`] mirrors `dashbuf`'s
/// `Color`). Whoever stages the run adds the text node's resolved box
/// origin, so positions reach the painter absolute and the painter never
/// moves anything (P2). The painter combines each position with the
/// atlas glyph's y-up `plane_em` quad to place the textured quad.
///
/// # `glyph_id` is `u32`, and that is what removes the padding
///
/// A glyph id is a `u16` by the OpenType specification, and
/// `dashscene-typeset` carries it as one. Here it is widened, because a
/// 2-byte member leading a struct of 4-byte members costs two bytes of
/// padding and saves none: `{u16, f32, f32}` and `{u32, f32, f32}` are both
/// 12 bytes at alignment 4. Story #578's rules for anything crossing a
/// language seam call for explicit padding; removing the need for any is
/// better than declaring it, and it avoids a public padding member that two
/// otherwise-equal quads could differ in — the equality hazard
/// `docs/decisions/optional-members-are-ranges-of-arity-one.md` D2 removed
/// from the ranges.
///
/// The value's domain is unchanged: OpenType ids are 16-bit, every producer
/// widens with `u32::from`, and nothing here widens what a font may express.
/// What the `u16` used to give for free — an out-of-domain id being
/// unrepresentable — is replaced by a check rather than by the type, and the
/// check is at [`GlyphRunTable::push_run`] (issue #985), which is the one seam
/// every quad passes through to reach a painter. [`Atlas::new`] refuses the same
/// id on [`AtlasGlyph`], the atlas side of the same widening; between them the
/// two sides of [`Atlas::glyph`]'s lookup agree on the domain.
///
/// **The check is not on this type, and for this type cannot be.** A checked
/// constructor holds only if the fields are private, and `glyph_id` cannot be:
/// `neither_glyph_type_carries_padding` — the test
/// `docs/decisions/sub-word-members-widen-rather-than-pad.md` names as what
/// holds the no-padding property — reads `offset_of!(GlyphQuad, glyph_id)` and
/// `size_of_val(&quad.glyph_id)` from `dashscene-unity`, another crate. Private
/// fields make both a compile error, so the constructor would be bought by
/// deleting the assertion that holds the widening.
///
/// The trade, and why widening beat declaring the padding, are in
/// `docs/decisions/sub-word-members-widen-rather-than-pad.md`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    pub glyph_id: u32,
    pub x: f32,
    pub y: f32,
}

/// One atlas glyph's placement geometry, keyed by glyph id. Only glyphs
/// that paint appear: an empty outline (a space) has no quad and is
/// omitted, so a `glyph_id` absent from an [`Atlas`] draws nothing.
///
/// Plain mirror of `dashscene-typeset`'s `GlyphEntry` bounds. Both
/// rectangles are `[left, bottom, right, top]`.
///
/// `glyph_id` is a `u32` for the reason [`GlyphQuad::glyph_id`] gives, and
/// with the same effect: `{u16, [f32; 4], [f32; 4]}` padded to 36 bytes and
/// `{u32, [f32; 4], [f32; 4]}` is 36 bytes with nothing padded. Both
/// rectangles keep the offsets they already had, so a consumer's declaration
/// of them is unchanged — only the leading member stops being two bytes
/// followed by two a C header would have to name.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasGlyph {
    pub glyph_id: u32,
    /// Quad bounds in ems, y-up, baseline origin (the metrics blob's
    /// `plane_em`).
    pub plane_em: [f32; 4],
    /// Texel bounds in the atlas image, bottom-left origin (the metrics
    /// blob's `atlas_px`).
    pub atlas_px: [f32; 4],
}

/// An index into a [`GlyphRunTable`]'s atlas list — the type of
/// [`GlyphRun::atlas`] and the return of [`GlyphRunTable::push_atlas`].
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasIndex(pub u32);

/// The MSDF glyph atlas a painter samples: the image, its parameters,
/// and per-glyph placement.
///
/// Plain mirror of the `dashscene-typeset` atlas metrics blob (dashpaint
/// depends on no crate). The build-time pipeline produces the metrics; a
/// stager converts them into this boundary-B shape, the same way an
/// image fill's bytes reach the painter as an [`ImageAsset`].
#[derive(Debug, Clone, PartialEq)]
pub struct Atlas {
    /// The MSDF atlas image (RGB distance channels), an encoded asset.
    pub image: ImageAsset,
    /// Atlas image size in texels.
    ///
    /// Private for the reason [`px_per_em`](Self::px_per_em) is, and it is the
    /// third divisor on this type (issue #1001): `dashscene-gpu`'s
    /// `gpu_glyph_run` divides by both to map a source texel into the residency
    /// texture. `pub` here would let any holder write `atlas.width = 0` after
    /// construction and reach that divide exactly as before [`Atlas::new`]'s
    /// check existed. Read them through [`width`](Self::width) and
    /// [`height`](Self::height).
    width: u32,
    height: u32,
    /// The size, in texels per em, the atlas was rendered at.
    ///
    /// Private, and that is what makes [`Atlas::new`]'s refusal of zero an
    /// invariant rather than a formality: `pub` here would let any holder write
    /// `atlas.px_per_em = 0` after construction and reach both painters' divide
    /// exactly as before the check existed (issue #724). Read it through
    /// [`px_per_em`](Self::px_per_em).
    px_per_em: u16,
    /// The MSDF distance range in atlas texels. The painter's
    /// screen-pixel range is `distance_range_px * render_size /
    /// px_per_em` (`plane_em` and `atlas_px` bake the range into the
    /// bounds, so this scales the sharpness of the edge, not the size).
    ///
    /// Private for the reason [`px_per_em`](Self::px_per_em) is, and it is the
    /// other operand of that same expression: `pub` here would let any holder
    /// write `atlas.distance_range_px = 0.0` after construction and paint every
    /// glyph as a uniform half-alpha box, exactly as before [`Atlas::new`]'s
    /// check existed (issue #964). Read it through
    /// [`distance_range_px`](Self::distance_range_px).
    distance_range_px: f32,
    /// Placement per glyph, sorted and unique by `glyph_id` (the metrics
    /// blob's own invariant — painters may binary-search it).
    glyphs: Vec<AtlasGlyph>,
}

/// Why an [`Atlas`] could not be built.
///
/// It is an enum rather than a unit struct because a further reason to refuse an
/// atlas belongs beside the existing ones rather than in a new type — which is
/// what issue #1001 then did, twice over: [`ZeroExtent`](Self::ZeroExtent) is a
/// variant here rather than a check at some other seam, where the sibling
/// degenerate case it used to be grouped with — a payload with no bytes — is
/// still named at its own.
///
/// The first two are the two operands of one expression, `distance_range_px *
/// size / px_per_em`, and between them they fix its domain. The third is about
/// a single glyph rather than the whole run. The fourth is a second expression's
/// only operand: the atlas extent a painter normalises a source texel by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AtlasBuildError {
    /// `px_per_em` was zero: the atlas states no scale to map its distances
    /// through, and every painter divides by it (issue #724).
    ZeroPxPerEm,
    /// `distance_range_px` was not finite and greater than zero, so the
    /// screen-pixel range every painter derives from it is not a range
    /// (issue #964).
    DistanceRangeOutOfDomain,
    /// A glyph id exceeded `u16::MAX`, which no OpenType font can produce
    /// (issue #966).
    GlyphIdAboveU16Max,
    /// `width` or `height` was zero: the atlas states an image with no texels
    /// to sample, and a painter mapping a source texel into it divides by both
    /// (issue #1001).
    ZeroExtent,
}

impl std::fmt::Display for AtlasBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroPxPerEm => write!(
                f,
                "an atlas rendered at zero texels per em has no scale to map its distances \
                 through; every painter divides by px_per_em"
            ),
            Self::DistanceRangeOutOfDomain => write!(
                f,
                "an atlas distance range must be finite and greater than zero; every painter \
                 scales it into the screen-pixel range it samples coverage over"
            ),
            Self::GlyphIdAboveU16Max => write!(
                f,
                "a glyph id above u16::MAX cannot come from a font: OpenType ids are 16-bit"
            ),
            Self::ZeroExtent => write!(
                f,
                "an atlas with no texels on an axis has nothing to sample; a painter mapping a \
                 source texel into it divides by both"
            ),
        }
    }
}

impl std::error::Error for AtlasBuildError {}

impl Atlas {
    /// An atlas over `glyphs`, which must be sorted and unique by
    /// `glyph_id` — the metrics blob guarantees it, so [`glyph`](Self::glyph)
    /// binary-searches.
    ///
    /// # Errors
    ///
    /// - [`AtlasBuildError::ZeroPxPerEm`] when `px_per_em` is zero (issue #724).
    /// - [`AtlasBuildError::DistanceRangeOutOfDomain`] when
    ///   `distance_range_px` is not finite and greater than zero (issue #964).
    /// - [`AtlasBuildError::GlyphIdAboveU16Max`] when any glyph id exceeds
    ///   `u16::MAX` (issue #966).
    /// - [`AtlasBuildError::ZeroExtent`] when `width` or `height` is zero
    ///   (issue #1001).
    ///
    /// The **extent** is the simplest of the four and the last to arrive: it is
    /// a third divisor, on a second expression. `dashscene-gpu`'s
    /// `gpu_glyph_run` maps a source-atlas texel into the residency texture by
    /// dividing its `uv` by both axes, so a zero on either is a division by
    /// zero. Both axes separately, because they are independent divisors and a
    /// check written against the pair would admit a 64 x 0 atlas.
    ///
    /// The first two are the two operands of one expression, and every painter
    /// computes it: `px_range = distance_range_px * size / px_per_em`. Between
    /// them they fix its domain, and each way out of that domain reaches *a
    /// plausible wrong picture* — text that still looks like text — rather than
    /// nothing drawn:
    ///
    /// - `px_per_em` is the **divisor**, so zero makes `px_range` an infinity
    ///   and `msdf_coverage` degenerates to a hard edge: `clamp(inf + 0.5, 0, 1)`
    ///   is 1 inside and 0 outside, with a NaN at the single sample whose median
    ///   is exactly 0.5, where WGSL leaves `clamp` implementation-defined. So the
    ///   glyph is aliased rather than antialiased, and the two painters diverge.
    /// - `distance_range_px` is the **numerator**, and it has three ways out
    ///   where a `u16` divisor had one. Zero makes `px_range` zero, so
    ///   `clamp(signed_distance * 0 + 0.5, 0, 1)` is exactly 0.5 for every
    ///   sample and each glyph paints as a uniform half-alpha box. A negative
    ///   value inverts coverage — interiors transparent, exteriors opaque. A NaN
    ///   or an infinity reaches the same implementation-defined `clamp` as
    ///   above, which is why the domain is finite-and-positive rather than
    ///   merely positive: an infinity is positive.
    ///
    /// There is deliberately **no upper bound** on `distance_range_px`. One
    /// would have to relate the range to the atlas extent, and no measurement in
    /// this repository supplies that number; an over-wide range also saturates
    /// towards the same aliased edge the infinity case already names, rather than
    /// into a class of its own.
    ///
    /// The third is about a single **glyph**, and it refuses an *atlas row*
    /// naming an id no font can produce — an id the `u16` these were widened
    /// from made unrepresentable
    /// (`docs/decisions/sub-word-members-widen-rather-than-pad.md`).
    ///
    /// It is **not** the silent drop that record describes. The two are
    /// different sides of the same widening: a row this constructor accepted
    /// would be *found* by [`glyph`](Self::glyph), which binary-searches the
    /// rows the atlas holds, so it paints. What paints nothing is a
    /// [`GlyphQuad`] naming an id the atlas has no row for, and both painters
    /// `continue` past it — deliberately, because an empty outline and a
    /// charset gap take that same path. This constructor never sees a
    /// `GlyphQuad`, so nothing here reaches that side; issue #985 put its check
    /// at [`GlyphRunTable::push_run`] instead, which refuses the same
    /// unrepresentable id while leaving that deliberate `continue` reachable.
    ///
    /// P4 forbids discovering a limit at draw time, so every one of these has to
    /// hold in release. A `debug_assert!` compiles out and leaves exactly the
    /// silent degrade it was added to remove — and an `assert!` would hold, but
    /// no test tier in this repository runs `--release`, so nothing would fail if
    /// it were weakened back to a `debug_assert!`. A `Result` runs in every
    /// profile and is pinned by a test in every profile.
    ///
    /// The one assertion left below is the odd one out, and legitimately: sorted-
    /// unique glyph ids are a real invariant of the metrics blob, and
    /// `AtlasMetrics::from_bytes` refuses a blob that breaks it **in every
    /// profile**, at the parse boundary. This is its second line of defence
    /// rather than its only one.
    ///
    /// Refused where the values enter boundary B, which fixes both painters at
    /// once and is why neither of them carries a guard. `px_per_em` and
    /// `distance_range_px` are both private, so this is the only way to set
    /// either and the checks hold for every atlas that exists — neither a struct
    /// literal nor a later assignment can reach them. Since issue #1001 the
    /// extent is private for the same reason, so every value this type feeds a
    /// painter's arithmetic is checked here.
    ///
    /// **[`image`](Self::image) is the one field still public**, and it is not
    /// a divisor — but the argument above does reach it: a holder replacing the
    /// payload after construction leaves `width` and `height` describing the old
    /// one, and `dashscene-engine`'s own doc records what that costs
    /// (`TexelPayload::of` takes the extent from the decode while
    /// `gpu_glyph_run` normalises with the metrics extent, so a disagreement
    /// samples the wrong texels rather than failing). That is issue #1074's
    /// shape rather than this one's, and it is filed.
    ///
    /// This is a statement about *this* type, not about boundary B.
    /// [`VectorField::distance_range`] is the same operand on the coverage-mask
    /// path, and issue #986 refused it at [`PaintTable::push_with`] rather than
    /// as a sibling variant here, because that type has far more construction
    /// sites than this one and the table they all pass through is a seam.
    ///
    /// "Between them they fix its domain" above is scoped to the two operands
    /// *this type* owns. The expression's third is [`GlyphRun::size`], which
    /// nothing in this crate refuses; on the document path
    /// `dashscene-validator`'s `text.style-size-out-of-range` does, with the
    /// same predicate.
    ///
    /// An embedder supplying its own font depends on `dashpaint` directly, as
    /// `dashscene-desktop` and `dashscene-web` both say of their `Atlas`
    /// re-export: naming is as far as that re-export goes. So the errors are
    /// nameable wherever the constructor is.
    pub fn new(
        image: ImageAsset,
        width: u32,
        height: u32,
        px_per_em: u16,
        distance_range_px: f32,
        glyphs: Vec<AtlasGlyph>,
    ) -> Result<Self, AtlasBuildError> {
        if px_per_em == 0 {
            return Err(AtlasBuildError::ZeroPxPerEm);
        }
        if width == 0 || height == 0 {
            return Err(AtlasBuildError::ZeroExtent);
        }
        // `is_finite` rejects both NaN and the infinities; `> 0.0` is false for
        // NaN as well, so the pair is not redundant only because of the
        // infinities — a positive infinity passes the comparison.
        if !distance_range_px.is_finite() || distance_range_px <= 0.0 {
            return Err(AtlasBuildError::DistanceRangeOutOfDomain);
        }
        debug_assert!(
            glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id),
            "atlas glyphs must be sorted and unique by glyph id"
        );
        if !glyphs.iter().all(|g| g.glyph_id <= u32::from(u16::MAX)) {
            return Err(AtlasBuildError::GlyphIdAboveU16Max);
        }
        Ok(Self {
            image,
            width,
            height,
            px_per_em,
            distance_range_px,
            glyphs,
        })
    }

    /// The size, in texels per em, this atlas was rendered at. Never zero —
    /// [`Atlas::new`] refuses that, and the field is private so nothing can
    /// reintroduce it (issue #724).
    pub fn px_per_em(&self) -> u16 {
        self.px_per_em
    }

    /// The MSDF distance range in atlas texels. Always finite and greater than
    /// zero — [`Atlas::new`] refuses anything else, and the field is private so
    /// nothing can reintroduce it (issue #964).
    pub fn distance_range_px(&self) -> f32 {
        self.distance_range_px
    }

    /// The atlas image's width in texels. Never zero — [`Atlas::new`] refuses
    /// that, and the field is private so nothing can reintroduce it
    /// (issue #1001).
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The atlas image's height in texels. Never zero, for the reason
    /// [`width`](Self::width) gives.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The placement for `glyph_id`, or `None` when the atlas has no quad
    /// for it (an empty-outline glyph such as a space, or a glyph outside
    /// the atlas's charset — which paints nothing).
    pub fn glyph(&self, glyph_id: u32) -> Option<&AtlasGlyph> {
        self.glyphs
            .binary_search_by_key(&glyph_id, |g| g.glyph_id)
            .ok()
            .map(|i| &self.glyphs[i])
    }
}

/// Where one run's quads sit in the [`GlyphRunTable`]'s flat quad array.
///
/// The glyph-side twin of [`ClipRegion`], and there for the same two
/// reasons: a `Vec` per run has no C representation, and a flat array plus
/// a range uploads as one buffer copy where a `Vec` per run is a pointer
/// chase (story #578).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct GlyphRange {
    /// First quad, as an index into [`GlyphRunTable::all_quads`].
    pub offset: u32,
    /// How many quads, in draw order.
    pub count: u32,
}

impl GlyphRange {
    /// The range a run carries before [`GlyphRunTable::push_run`] assigns
    /// it one. A staged run names no quads in a table it has not entered
    /// yet, and this is the only value `push_run` accepts.
    pub const UNASSIGNED: Self = Self {
        offset: 0,
        count: 0,
    };
}

/// One positioned glyph run: a sequence of placed glyphs that share a
/// render size, a fill color, and an atlas (one style per text node in
/// the v0.5 Latin subset — `docs/design/architecture.md` §7.2).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphRun {
    /// The rect-table index of the text node this run was shaped from —
    /// the run's *anchor*, stamped by commit
    /// (`docs/decisions/glyph-runs-cross-boundary-b.md`, "The producer
    /// story, decided").
    ///
    /// One field carries three facts a painter cannot otherwise recover:
    /// the run's clip is `rects[run.rect].clip`, its group membership is
    /// the [`GroupComposite`] whose `[start, end)` range contains
    /// `run.rect`, and its z position is immediately after that rect. A
    /// separate clip index mirroring [`RectEntry::clip`] was considered
    /// and rejected: it is derivable, and two fields can disagree.
    ///
    /// Like [`RectEntry::paint`] and [`RectEntry::clip`], this is
    /// meaningful only against the rect table of the commit it came from
    /// — never cached across commits.
    pub rect: u32,
    /// The atlas the glyphs are sampled from.
    pub atlas: AtlasIndex,
    /// Render size in document units (px per em).
    pub size: f32,
    /// The text fill; the MSDF coverage modulates this color.
    pub color: Color,
    /// The placed glyphs, in draw order, as a range into the
    /// [`GlyphRunTable`]'s one flat quad array — read it with
    /// [`GlyphRunTable::quads`] (story #578).
    ///
    /// A `Vec` here has no C representation and would be a pointer chase
    /// per run on the path R-T4 bounds; the same reasoning as
    /// [`ClipRegion`], and the same shape.
    ///
    /// [`GlyphRunTable::push_run`] assigns this. Whatever a caller writes
    /// is refused rather than overwritten, because a range is only
    /// meaningful against the array the table owns — see that method.
    pub glyphs: GlyphRange,
    /// The run's free-path group alpha in `[0, 1]`, mirroring
    /// [`RectEntry::opacity`] (story #44,
    /// `docs/decisions/masks-and-group-opacity.md`): a group opacity that
    /// took the free path folds into it, and the painter multiplies the
    /// run's fill alpha by it. `1.0` when no free-path group opacity
    /// applies.
    ///
    /// This is the **free** path's alpha only. A run now draws at its
    /// [`rect`](Self::rect) anchor's index, inside that rect's clip region
    /// (issue #275) and inside every render-target group layer enclosing it
    /// (issue #274), so the render-target path reaches a run through the
    /// layer rather than through this field. Both limitations this field's
    /// documentation used to name are gone, and the
    /// `paint.text-outside-group` gate that reported the second is retired.
    ///
    /// The field is now derivable from `rects[rect].opacity` and is kept
    /// only until that fold-in lands
    /// (`docs/decisions/glyph-runs-cross-boundary-b.md`).
    pub opacity: f32,
}

/// The glyph-run table (`docs/design/architecture.md` §7.3): the
/// positioned glyph runs plus the atlases they reference — the text half
/// of the painter input, a sibling of the rect table. Empty for a
/// text-free scene, so every existing caller passes
/// [`GlyphRunTable::new`].
///
/// Runs cross boundary B already shaped, wrapped, and positioned: the
/// one typesetter did that once, and the painter only draws the quads
/// (P2). The atlases are carried with the runs because a run's glyph ids
/// are meaningless without the atlas that places them, the same way an
/// image fill needs its [`ImageTable`] entry.
///
/// The atlases are held behind an [`Arc`] because commit rebuilds this
/// table every frame while the atlas set behind it is a build artifact
/// that does not change: the eight atlases the goldens harness loads are
/// about 460 KB together, and copying them per commit would be per-frame
/// cost that R-T4 bounds to the dirty-range upload and submission. The
/// same posture `CommittedScene` takes with its paint and clip tables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphRunTable {
    atlases: Arc<Vec<Atlas>>,
    runs: Vec<GlyphRun>,
    /// Every run's quads, in one flat array (story #578). A
    /// [`GlyphRun::glyphs`] range indexes into this. Runs are appended in
    /// draw order and never rewritten, so each range is a contiguous run
    /// and the array is in the same order as `runs`.
    quads: Vec<GlyphQuad>,
}

impl GlyphRunTable {
    /// An empty table — no runs, no atlases.
    pub fn new() -> Self {
        Self::default()
    }

    /// A table over an existing atlas set, sharing it rather than copying
    /// it — how commit builds the table from the atlases its solver
    /// supplies once.
    pub fn with_atlases(atlases: Arc<Vec<Atlas>>) -> Self {
        Self {
            atlases,
            runs: Vec::new(),
            quads: Vec::new(),
        }
    }

    /// Appends an atlas and returns its index — the value a
    /// [`GlyphRun::atlas`] field holds to reference it.
    ///
    /// Copies the atlas set first when it is shared with another table
    /// ([`Arc::make_mut`]), so a caller building a table by hand is
    /// unaffected by the sharing.
    pub fn push_atlas(&mut self, atlas: Atlas) -> AtlasIndex {
        let atlases = Arc::make_mut(&mut self.atlases);
        let index = u32::try_from(atlases.len()).expect("glyph-run table exceeds u32::MAX atlases");
        atlases.push(atlas);
        AtlasIndex(index)
    }

    /// Appends a positioned run over `quads`, which are copied into the
    /// table's flat array and named by the range this assigns.
    ///
    /// # Panics
    ///
    /// Panics unless `run.glyphs` is [`GlyphRange::UNASSIGNED`]. A caller
    /// cannot know where its quads will land in a table it has not entered,
    /// so a range arriving here is a range that will be replaced — and
    /// silently replacing it is how a producer comes to believe its own
    /// offsets were used. Refused by name instead (P4).
    ///
    /// Panics if any quad's `glyph_id` exceeds `u16::MAX`, which no OpenType
    /// font can produce (issue #985). This is the [`GlyphQuad`] side of the
    /// widening `docs/decisions/sub-word-members-widen-rather-than-pad.md`
    /// records, and the side that carries the silent drop: such a quad matches
    /// no [`Atlas`] row, so [`Atlas::glyph`] returns `None` and both painters
    /// `continue` past it with no diagnostic (P4). [`Atlas::new`] refuses the
    /// same id on an [`AtlasGlyph`]; between them the two sides of the lookup
    /// now agree on the domain, where before only the atlas side did.
    ///
    /// It refuses **an id no font can express**, not "an id this atlas has no
    /// row for". The `continue` those painters take stays reachable and stays
    /// correct: an empty outline such as a space has no atlas row by design,
    /// and a codepoint outside the atlas charset is a build-time coverage gap
    /// the closure owns rather than a per-frame decision. Only the
    /// unrepresentable id is refused here.
    ///
    /// Also panics if the flat array *would* exceed `u32::MAX` quads — the
    /// length the append produces, not the two lengths going into it, which is
    /// what `quad_range` checks and issue #1014 was.
    ///
    /// # What this seam does not cover
    ///
    /// [`GlyphRun::size`] is the third operand of the expression the other two
    /// are guarded for — every painter computes `px_range = distance_range_px *
    /// size / px_per_em` — and no seam in this crate refuses it. On the
    /// document path it is refused upstream: `dashscene-validator`'s
    /// `text.style-size-out-of-range` rejects a `TextStyle.size` that is not
    /// finite and greater than zero, which is the value `dashscene-engine`
    /// copies into this field. What is left uncovered is the producer that
    /// stages a text style directly and never runs that gate — the same
    /// residual gap every public field on these rows has.
    pub fn push_run(&mut self, mut run: GlyphRun, quads: &[GlyphQuad]) {
        assert_eq!(
            run.glyphs,
            GlyphRange::UNASSIGNED,
            "push_run assigns a run's quad range; a staged run must carry \
             GlyphRange::UNASSIGNED, not offsets into some other array"
        );
        // An `if` around a `panic!` rather than an `assert!`; the record
        // `docs/decisions/boundary-b-domain-checks-sit-at-the-table-seam.md`
        // gives the reason, which is that an `if` has no debug-only spelling a
        // later edit could weaken it to.
        //
        // The test is a bitwise OR rather than the obvious `any()` because this
        // runs once per run per commit, on the frame path: the OR has no early
        // exit and no per-quad counter, so it vectorizes and folds into the
        // memory traffic `extend_from_slice` below already pays, where a search
        // over a 12-byte stride does not. An id above `u16::MAX` is exactly an
        // id with a bit above bit 15 set, so OR-ing every id and testing that
        // once is the same predicate over the slice. The search still runs, but
        // only on the failing path, where it recovers the quad's index.
        if quads.iter().fold(0u32, |seen, quad| seen | quad.glyph_id) > u32::from(u16::MAX) {
            let (at, quad) = quads
                .iter()
                .enumerate()
                .find(|(_, quad)| quad.glyph_id > u32::from(u16::MAX))
                .expect("the fold found a bit set that no id below u16::MAX can set");
            panic!(
                "glyph quad {at} names id {}, above u16::MAX: no OpenType font can produce it, so \
                 no atlas has a row for it and every painter would skip the quad without a \
                 diagnostic (P4)",
                quad.glyph_id
            );
        }
        let (offset, count) = Self::quad_range(self.quads.len(), quads.len());
        self.quads.extend_from_slice(quads);
        run.glyphs = GlyphRange { offset, count };
        self.runs.push(run);
    }

    /// `(offset, count)` for `count` quads about to be appended to a flat array
    /// already `array_len` long, refusing a range whose **end** runs past
    /// `u32::MAX`.
    ///
    /// The sum, not the two lengths separately (issue #1014). Converting each
    /// alone accepts an array at `u32::MAX - 999` taking a 1000-quad run: both
    /// conversions succeed, `extend_from_slice` takes the array past what a
    /// `u32` offset can address, and the run is given a [`GlyphRange`] as though
    /// it were valid. The push that "would exceed `u32::MAX`" does not refuse,
    /// and the *next* one does — one call away from its cause.
    ///
    /// The [`PaintTable`] side of this is `PaintTable::flat_range`, which the
    /// five ranges [`PaintTable::push_with`] assigns go through. Both are
    /// unreachable on the target hardware, where four billion 12-byte quads is
    /// about 48 GB against a fixed memory budget; they are checked because they
    /// are a contract these methods state.
    fn quad_range(array_len: usize, count: usize) -> (u32, u32) {
        let offset = u32::try_from(array_len).expect("glyph-run table exceeds u32::MAX quads");
        let count = u32::try_from(count).expect("a glyph run exceeds u32::MAX quads");
        if offset.checked_add(count).is_none() {
            panic!(
                "a run of {count} quads appended at offset {offset} would take the flat array \
                 past u32::MAX: boundary B names a run's quads with a u32 offset and a u32 \
                 count, so nothing past that end has a range that can be expressed (story #578)"
            );
        }
        (offset, count)
    }

    /// The quads `run` draws, in draw order.
    ///
    /// # Panics
    ///
    /// Panics if the run's range runs past the flat array — the same
    /// contract, and the same reasoning, as [`ClipTable::all_boxes`]'s
    /// side of a [`ClipRegion`]. Only [`push_run`](Self::push_run) writes
    /// ranges, and it writes them from the array's own length, so no
    /// public path reaches this today.
    pub fn quads(&self, run: &GlyphRun) -> &[GlyphQuad] {
        let start = run.glyphs.offset as usize;
        let end = start + run.glyphs.count as usize;
        self.quads.get(start..end).unwrap_or_else(|| {
            panic!(
                "glyph run range {}..{} runs past the table's {} quads: a run and the array it \
                 indexes must be built together",
                start,
                end,
                self.quads.len()
            )
        })
    }

    /// Every quad in the table, in one flat array. A [`GlyphRange`]'s
    /// `offset` and `count` index into this — the shape an instance-buffer
    /// upload wants.
    pub fn all_quads(&self) -> &[GlyphQuad] {
        &self.quads
    }

    /// The runs to draw, in order.
    pub fn runs(&self) -> &[GlyphRun] {
        &self.runs
    }

    /// Resolves a run's atlas index. Panics on an out-of-range index,
    /// the same contract as [`PaintTable::resolve`]: a miss is a broken
    /// contract between crates, validated upstream (P4).
    pub fn atlas(&self, index: AtlasIndex) -> &Atlas {
        self.atlases.get(index.0 as usize).unwrap_or_else(|| {
            panic!(
                "atlas index {} out of range ({} atlases): atlas indices are validated upstream (P4)",
                index.0,
                self.atlases.len()
            )
        })
    }

    /// The atlases the runs reference, in [`AtlasIndex`] order — so a
    /// painter can prepare each atlas (decode, upload) once, rather than
    /// once per run that samples it.
    pub fn atlases(&self) -> &[Atlas] {
        self.atlases.as_slice()
    }

    /// The shared handle behind [`atlases`](Self::atlases), for a painter
    /// that keeps its prepared atlases across frames and needs to know
    /// whether this frame's set is still the one it prepared (issue #644).
    ///
    /// Two properties follow from returning the handle rather than the
    /// slice, and a painter needs both:
    ///
    /// - **Holding it costs nothing.** Commit rebuilds this table every
    ///   frame, so a painter that kept a `Vec<Atlas>` copy to compare
    ///   against would duplicate the whole set — the copy this field is an
    ///   [`Arc`] to avoid in the first place.
    /// - **[`Arc::ptr_eq`] against it is a sound identity test.** A shared
    ///   set is immutable: [`push_atlas`](Self::push_atlas) goes through
    ///   [`Arc::make_mut`], which copies while any other holder exists, so
    ///   the holder's pointer cannot come to name different contents. Nor
    ///   can a freed allocation be reused at the same address, because the
    ///   holder is itself keeping it alive.
    ///
    /// Pointer inequality does **not** imply the contents differ — an
    /// equal set rebuilt behind a fresh allocation compares unequal here —
    /// so it is a fast path, not a verdict. Comparing contents is the
    /// fallback.
    pub fn atlas_set(&self) -> &Arc<Vec<Atlas>> {
        &self.atlases
    }

    /// True when the table carries no runs.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

/// One render-target group opacity, resolved at commit
/// (`docs/decisions/masks-and-group-opacity.md`). A node whose opacity is
/// below 1 and whose painted subtree overlaps cannot have its alpha pushed
/// per-rect (the overlap would blend twice), so its subtree — the contiguous
/// rect range `[start, end)` in DFS order — composites offscreen and the
/// layer composites at `alpha`. A group is fully nested inside any group
/// that encloses it, so the ranges form a proper nesting (never a partial
/// overlap). The non-overlapping *free* path carries its alpha in
/// [`RectEntry::opacity`] instead and produces no `GroupComposite` at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupComposite {
    /// First rect index in the group's subtree (the group node itself).
    pub start: u32,
    /// One past the last rect index in the group's subtree.
    pub end: u32,
    /// The alpha the group's offscreen layer composites at, in `[0, 1]`.
    pub alpha: f32,
}

/// Boundary B (docs/design/architecture.md): the one trait every paint backend
/// implements. A painter only colors — it never measures, wraps, kerns,
/// or moves anything (P2).
pub trait Painter {
    /// Paints every rect, resolving each [`RectEntry::paint`] index
    /// against `paints` (use [`PaintTable::resolve`]) and each
    /// [`RectEntry::clip`] index against `clips` (use
    /// [`ClipTable::resolve`]); image fills resolve their asset in
    /// `images` (an empty table is valid input for image-less scenes).
    ///
    /// A rect draws only inside its resolved [`ClipRegion`] — the
    /// intersection of the region's boxes. The region is already
    /// ancestor-resolved: a painter clips against the boxes it is given
    /// and never asks which node they came from (P2).
    ///
    /// Slice order defines stacking: a later entry composites over an
    /// earlier one (DFS order encodes document stacking). The composited
    /// result is the contract; iteration order is the implementation's
    /// choice (the lean painter draws opaque cores front-to-back,
    /// docs/specification/03-target-hardware-rules.md R-T2) — with one
    /// exception, below.
    ///
    /// **The backdrop barrier.** A rect whose paint entry answers
    /// [`PaintTable::samples_backdrop`] reads what is already composited
    /// beneath it, so every rect at a lower index MUST be composited
    /// before that rect is drawn
    /// (`docs/decisions/backdrop-blur-is-core-vocabulary.md`). Such a
    /// rect is a barrier in any reorder, and that is the whole of the
    /// narrowing: the licence above still applies on either side of it. A
    /// painter that iterates in slice order satisfies the guarantee
    /// without doing anything, because it already composites
    /// back-to-front into one target; only a painter that reorders pays
    /// for the barrier. The guarantee fixes the order alone — which
    /// surface the sample reads when the barrier rect falls inside a
    /// [`GroupComposite`] range was left to the first painter that
    /// implements the sampling, and `dashscene-skia` settled it (story
    /// #393): **a render-target group is a backdrop root.** Such a rect
    /// reads that group's offscreen layer, not the canvas beneath the
    /// group; outside a group range it reads the canvas. Sampling through
    /// the group would composite the backdrop twice, which is the defect
    /// [`GroupComposite`] exists to prevent, one level up. Glyph runs are outside
    /// it for the same reason they are outside `groups` below: the v0.5
    /// subset composites every run over all rects, so no run is ever
    /// beneath a barrier and no run can enter a sampled backdrop (a named
    /// limitation, not a silent drop).
    ///
    /// Each rect's paint alpha is modulated by [`RectEntry::opacity`], the
    /// resolved free-path group alpha (`1.0` when none applies). `groups`
    /// carries the render-target group opacities: for a
    /// [`GroupComposite`] the painter composites the rect range
    /// `[start, end)` offscreen and blends the layer at `alpha`, so an
    /// overlapping group at partial opacity flattens before its alpha
    /// applies. The groups nest by range; an empty slice is valid input for
    /// a scene with no render-target opacity.
    ///
    /// `glyphs` is the glyph-run table: positioned glyph runs and the
    /// atlases they sample, staged already shaped and placed. The v0.5
    /// Latin subset composites every run over all rects (text is
    /// foreground); a full z-interleave of runs with rects is later work.
    /// An empty table ([`GlyphRunTable::new`]) is valid input for a
    /// text-free scene. A run's free-path group alpha rides on
    /// [`GlyphRun::opacity`], but render-target `groups` and clip/mask
    /// regions are **not** applied to runs (a run draws as foreground, not
    /// composited into a group's layer nor clipped to a region — a named
    /// limitation, story #44, a debt candidate).
    ///
    /// `dirty` is the rect indices whose entry changed since the commit
    /// that produced the previous `rects` — **advisory**. `None` means
    /// the caller has no dirty information (hand-built tables, or a first
    /// frame). Ignoring it and redrawing everything is always correct,
    /// and a painter that honors it MUST produce output identical to one
    /// that does not. It exists so a painter can meet R-T4
    /// (docs/specification/03-target-hardware-rules.md): per-frame CPU
    /// cost is the dirty-range instance-buffer upload from the rect table
    /// plus submission, and nothing else. It is not a license for
    /// damage-region partial redraw, which R-T1 forbids on a tiling GPU.
    ///
    /// Infallible by design: vocabulary and indices are validated upstream
    /// (P4), so there is no legitimate runtime failure. An out-of-range
    /// `paint` or `clip` index is a broken contract between crates;
    /// [`PaintTable::resolve`] and [`ClipTable::resolve`] centralize the
    /// panic for that case.
    /// Whether this painter can use a payload in `format` as it stands.
    ///
    /// # Why this is a declaration and not a result
    ///
    /// [`Painter::paint`] returns nothing, by decision
    /// (`docs/decisions/painter-trait-infallible-slice-input.md`), so "this
    /// painter cannot sample ASTC 6x6" cannot be reported from inside a frame.
    /// And P4 forbids discovering it there in any case: an unsupported
    /// construct is a named diagnostic, never a silent drop. So the question is
    /// asked **before** a payload is bound, by whoever selects which derivation
    /// to load, and this is the answer they ask for.
    ///
    /// # The default is the conservative half
    ///
    /// A painter that says nothing claims only the source-encoded containers,
    /// which is what every painter written before story #640 does. The default
    /// is safe in the direction that matters: a painter that *could* upload a
    /// baked payload but forgot to say so is handed an encoded one and decodes
    /// it — slower, and correct. The reverse could not be made safe, which is
    /// why the default is not "everything".
    fn samples(&self, format: ImageFormat) -> bool {
        format.is_encoded()
    }

    /// Whether this painter draws [`RectEntry::rotation`].
    ///
    /// # Why this is a declaration and not a result
    ///
    /// The same argument as [`Painter::samples`] above. [`Painter::paint`]
    /// returns nothing, so "this painter cannot rotate" cannot be reported
    /// from inside a frame, and P4 forbids discovering it there: an
    /// unsupported construct is a named diagnostic, never a silent drop. So
    /// the question is asked **before** a document is bound, by whoever
    /// chooses a painter for it.
    ///
    /// # The default is the conservative half, and here that matters more
    ///
    /// A painter that says nothing does not rotate, which is what every
    /// painter written before story #770 does. The direction is the safe
    /// one for the reason
    /// `docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`
    /// gives: a painter that accepted a rotation and drew the node unrotated
    /// would produce a plausible picture that is silently wrong, and no
    /// golden could tell it from a correct one at an angle of zero. A painter
    /// that declares the gap can be asserted against.
    fn rotates(&self) -> bool {
        false
    }

    // Boundary B is a fixed set of parallel tables (§7.3), so `paint`
    // takes one per table plus the advisory dirty set — the arity is the
    // contract, not a call-site smell.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        groups: &[GroupComposite],
        glyphs: &GlyphRunTable,
        dirty: Option<&[u32]>,
    );
}

/// The two flat-array range assignments, tested here rather than through the
/// public pushes because the case they refuse needs an array four billion rows
/// long — about 48 GB of `GlyphQuad` — which no test can build (issue #1014).
///
/// The arithmetic is the whole of the fix, so testing it directly is testing
/// the fix: both helpers are the only producer of a range in their table, and
/// both pushes route every range they assign through one of them.
#[cfg(test)]
mod flat_ranges {
    use super::{GlyphRunTable, PaintTable};

    /// The tipping case from the issue: an array just under the ceiling taking
    /// a part that carries it over. Each length converts to a `u32` on its own,
    /// which is exactly why checking them separately let this through.
    #[test]
    fn a_range_whose_end_runs_past_u32_max_is_refused() {
        let array_len = u32::MAX as usize - 999;
        let count = 1000;
        assert!(u32::try_from(array_len).is_ok() && u32::try_from(count).is_ok());

        let paint = std::panic::catch_unwind(|| PaintTable::flat_range(array_len, count))
            .expect_err("a paint range ending past u32::MAX must be refused");
        let quads = std::panic::catch_unwind(|| GlyphRunTable::quad_range(array_len, count))
            .expect_err("a quad range ending past u32::MAX must be refused");
        for payload in [paint, quads] {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .unwrap_or("<panic payload was not a string>")
                .to_owned();
            assert!(
                message.contains("past u32::MAX"),
                "the refusal must be the end-of-range check, not one of the two length \
                 conversions above it; got: {message}"
            );
        }
    }

    /// The largest range that fits is assigned rather than refused, so the
    /// guard bounds the range and does not merely bound the offset.
    #[test]
    fn a_range_ending_exactly_at_u32_max_is_assigned() {
        let array_len = u32::MAX as usize - 1000;
        let count = 1000;
        assert_eq!(
            PaintTable::flat_range(array_len, count),
            (u32::MAX - 1000, 1000)
        );
        assert_eq!(
            GlyphRunTable::quad_range(array_len, count),
            (u32::MAX - 1000, 1000)
        );
    }

    /// An empty part at a non-empty array keeps the offset it is given.
    ///
    /// `PaintTable::span` canonicalizes that case to `(0, 0)` before it reaches
    /// `flat_range`, and `intern_fill`'s stop range deliberately does not —
    /// which is the whole reason the two are separate functions, and what this
    /// pins.
    #[test]
    fn an_empty_part_keeps_its_offset() {
        assert_eq!(PaintTable::flat_range(7, 0), (7, 0));
        assert_eq!(PaintTable::span(7, 0), (0, 0));
        assert_eq!(GlyphRunTable::quad_range(7, 0), (7, 0));
    }
}

#[cfg(test)]
mod field_draws {
    use super::VectorField;

    /// A field over an 8x8 atlas rectangle with the given plane quad.
    fn field(plane_bounds: [f32; 4]) -> VectorField {
        VectorField {
            image: 0,
            atlas_rect: [0, 0, 8, 8],
            plane_bounds,
            distance_range: 4.0,
        }
    }

    /// **The three ways a quad's extent leaves its domain**, which is what
    /// [`VectorField::draws`] tests rather than testing the four bounds
    /// (issue #1034).
    ///
    /// The third row is the one that makes the distinction load-bearing: every
    /// bound is finite and they are in the right order, so a predicate checking
    /// `is_finite` on each of them admits it — and `3.0e38 - -3.0e38` is
    /// `6.0e38`, which overflows `f32`. That was the first fix for #1034 and it
    /// was still wrong.
    ///
    /// An infinity is given in the one position that *passes* the ordering:
    /// negative for `left` and `top`, positive for `right` and `bottom`. In the
    /// other position it fails the ordering and would prove nothing.
    #[test]
    fn a_quad_whose_extent_is_not_finite_draws_nothing() {
        for (what, plane_bounds) in [
            ("a NaN left bound", [f32::NAN, 0.0, 8.0, 8.0]),
            ("a NaN bottom bound", [0.0, 0.0, 8.0, f32::NAN]),
            ("an infinite left bound", [f32::NEG_INFINITY, 0.0, 8.0, 8.0]),
            ("an infinite top bound", [0.0, f32::NEG_INFINITY, 8.0, 8.0]),
            ("an infinite right bound", [0.0, 0.0, f32::INFINITY, 8.0]),
            ("an infinite bottom bound", [0.0, 0.0, 8.0, f32::INFINITY]),
            ("a width that overflows", [-3.0e38, 0.0, 3.0e38, 8.0]),
            ("a height that overflows", [0.0, -3.0e38, 8.0, 3.0e38]),
        ] {
            assert!(
                !field(plane_bounds).draws(),
                "a field with {what} must draw nothing: both painters divide by the quad's \
                 extent, and each of these makes that divisor non-finite",
            );
        }
    }

    /// An empty or inverted quad draws nothing, which is the ordinary
    /// degenerate case rather than an out-of-domain one.
    #[test]
    fn a_quad_of_no_positive_extent_draws_nothing() {
        for (what, plane_bounds) in [
            ("no extent at all", [0.0, 0.0, 0.0, 0.0]),
            ("no width", [4.0, 0.0, 4.0, 8.0]),
            ("no height", [0.0, 4.0, 8.0, 4.0]),
            ("a quad inverted in x", [8.0, 0.0, 2.0, 8.0]),
            ("a quad inverted in y", [0.0, 8.0, 8.0, 2.0]),
        ] {
            assert!(
                !field(plane_bounds).draws(),
                "a field with {what} must draw nothing",
            );
        }
    }

    /// An atlas rectangle with no texels draws nothing, on either axis.
    ///
    /// Both axes, because the two are separate divisors in both painters: a
    /// fixture zeroing only one could not tell a test of the pair from a test
    /// of that one.
    #[test]
    fn an_atlas_rectangle_with_no_texels_draws_nothing() {
        for (what, atlas_rect) in [
            ("no extent at all", [0, 0, 0, 0]),
            ("no width", [0, 0, 0, 8]),
            ("no height", [0, 0, 8, 0]),
        ] {
            let mut sound = field([0.0, 0.0, 8.0, 8.0]);
            sound.atlas_rect = atlas_rect;
            assert!(
                !sound.draws(),
                "a field whose atlas rectangle has {what} must draw nothing",
            );
        }
    }

    /// **A sound field draws**, without which every assertion above passes
    /// against a predicate that answers `false` unconditionally.
    ///
    /// The bounds are deliberately not symmetric and not at the origin, so a
    /// predicate that had transposed a pair or dropped an origin term would
    /// still have to answer `true` here for the right reason.
    #[test]
    fn a_sound_field_draws() {
        assert!(field([1.0, 2.0, 9.0, 14.0]).draws());
    }
}
