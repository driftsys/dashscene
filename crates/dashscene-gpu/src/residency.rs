//! Atlas residency: which payloads are on the device, where, and which one
//! leaves when there is no room (story #581).
//!
//! # One mechanism, three consumers
//!
//! Image fills reach it here. MSDF glyph atlases and baked vector fields reach
//! it at story #582, through the same [`Residency::resident`] call: all three
//! are "a payload of some texel format that has to be somewhere a shader can
//! sample". The alternative — a texture per payload — needs either a bind group
//! per draw or a binding array, and a binding array is not in
//! `wgpu::Limits::downlevel_defaults`, which
//! `docs/decisions/pipelines-and-layer-3.md` D2 holds this painter to.
//!
//! # An atlas per texel format, and why the colour space is not one of them
//!
//! Payloads of different texel formats cannot share a texture, so there is one
//! atlas per [`AtlasFormat`]. The two colour spaces of one block footprint do
//! share one, because this painter samples every payload through wgpu's
//! **Unorm** channel whatever the payload's declared colour space is.
//!
//! That is not a shortcut. `docs/decisions/blur-blends-in-srgb-encoded-space.md`
//! makes sRGB-encoded blending a term of the boundary-B contract rather than a
//! per-painter choice, and `pipelines-and-layer-3.md` D3 is why the render
//! target is `Rgba8Unorm` and not `Rgba8UnormSrgb`. A `*Srgb` texture view
//! would have the sampler linearise on read, putting image texels in a
//! different space from every other colour in the shader. So an sRGB-encoded
//! payload is sampled as the encoded value it is, and a linear one — which is
//! what `dashpack` writes for a distance field — is sampled as the raw value it
//! is. Both are "give me the stored number", which is wgpu's Unorm channel.
//!
//! # Two samplers, and the gutter that is absent for both
//!
//! An **image fill** is read with `Nearest`, matching `dashscene-skia`'s
//! `SamplingOptions::default()` — the reference painter's own deliberate choice
//! ("deterministic and exact for the v0.3 corpus; filtering quality is a later,
//! deliberate decision").
//!
//! An **MSDF payload** — a glyph atlas or a baked-vector coverage mask — is read
//! with `Linear`, through a second sampler story #582 added. The reference
//! painter draws the same distinction and for the same reason: a distance field
//! is not a colour, and quantising it to the atlas's texel grid turns a smooth
//! edge into a staircase.
//!
//! **Neither needs a gutter between allocations**, and that is a property of the
//! read rather than of this allocator. Every sampler here is clamped, and both
//! read paths clamp their coordinate half a source texel inside the payload's
//! own sub-rect before sampling — `image_colour` and `msdf_sample` in
//! `shaders/paint.wgsl`. A nearest sample taken from there lands on a texel of
//! this payload, and a bilinear footprint taken from a texel's own centre
//! weights that texel alone at the payload's edge. So allocations are packed
//! adjacent with no padding between them.
//!
//! Earlier revisions of this paragraph named a one-texel gutter as the first
//! thing to add if filtering ever became linear. Filtering did become linear,
//! at story #582, and the gutter was still not needed — the clamp that was
//! there for the nearest case turned out to be exactly the condition filtering
//! wants. `docs/decisions/tables-the-vertex-stage-reads.md` D5 records it.
//! **What would bring the hazard back is a sampler that is not clamped, or a
//! read path that samples without that inset** — not the filter mode on its own.

use std::collections::HashMap;

use dashpaint::{ImageFormat, ImageRef};

/// The texel format one atlas texture holds.
///
/// Derived from an [`ImageFormat`] by an exhaustive match, never by a cast —
/// the rule `crate::instance::InstanceKind` records, for the same reason: a
/// reordered variant in `dashpaint` would silently change which atlas a payload
/// landed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AtlasFormat {
    /// Eight-bit RGBA texels: a decoded container, or a `Rgba8` baked payload
    /// uploaded as it stands.
    Rgba8,
    /// ASTC LDR at one square footprint, uploaded block for block.
    Astc { block: (u32, u32) },
}

impl AtlasFormat {
    /// The atlas an [`ImageFormat`] payload belongs in, once it is on the
    /// device.
    ///
    /// Every encoded container answers [`AtlasFormat::Rgba8`], because a
    /// painter that decodes one produces RGBA texels — the format names what
    /// the *bytes* are, and this names what the *texture* is.
    pub fn of(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif => Self::Rgba8,
            ImageFormat::Rgba8Srgb | ImageFormat::Rgba8Unorm => Self::Rgba8,
            ImageFormat::Astc4x4Srgb | ImageFormat::Astc4x4Unorm => Self::Astc { block: (4, 4) },
            ImageFormat::Astc5x5Srgb | ImageFormat::Astc5x5Unorm => Self::Astc { block: (5, 5) },
            ImageFormat::Astc6x6Srgb | ImageFormat::Astc6x6Unorm => Self::Astc { block: (6, 6) },
            ImageFormat::Astc8x8Srgb | ImageFormat::Astc8x8Unorm => Self::Astc { block: (8, 8) },
            ImageFormat::Astc10x10Srgb | ImageFormat::Astc10x10Unorm => {
                Self::Astc { block: (10, 10) }
            }
            ImageFormat::Astc12x12Srgb | ImageFormat::Astc12x12Unorm => {
                Self::Astc { block: (12, 12) }
            }
        }
    }

    /// The wgpu format a texture of this atlas is created with.
    ///
    /// Always the Unorm channel — see this module's documentation for why the
    /// payload's own colour space does not choose it.
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Self::Astc { block } => wgpu::TextureFormat::Astc {
                block: astc_block(block),
                channel: wgpu::AstcChannel::Unorm,
            },
        }
    }

    /// The texel footprint one stored block covers: `(1, 1)` for an
    /// uncompressed atlas, the ASTC footprint otherwise.
    ///
    /// What every allocation is quantised to. A compressed texture can only be
    /// written on block boundaries, so an allocation that did not land on one
    /// could not be uploaded into.
    pub fn block(self) -> (u32, u32) {
        match self {
            Self::Rgba8 => (1, 1),
            Self::Astc { block } => block,
        }
    }

    /// The largest atlas of this format that fits inside `extent` texels on a
    /// side: `extent` itself for an uncompressed atlas, and `extent` rounded
    /// **down** to a whole number of blocks for a compressed one.
    ///
    /// A block-compressed texture's dimensions must be a multiple of its
    /// footprint — wgpu refuses `2048` at a 6x6 footprint by name — so an atlas
    /// at the device's largest texture size is not a legal size for four of the
    /// six ladders. Rounding down rather than up keeps every atlas inside the
    /// limit the device actually stated.
    pub fn usable_extent(self, extent: u32) -> (u32, u32) {
        let (bx, by) = self.block();
        (extent / bx * bx, extent / by * by)
    }

    /// The device feature a texture in this format needs, or `None` when every
    /// adapter can hold it.
    pub fn required_feature(self) -> Option<wgpu::Features> {
        match self {
            Self::Rgba8 => None,
            Self::Astc { .. } => Some(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
        }
    }
}

/// The `wgpu` footprint enum for a square ASTC block.
///
/// # Panics
///
/// Panics on a footprint no [`AtlasFormat::Astc`] carries. The only constructor
/// is [`AtlasFormat::of`], whose match produces exactly these six, so a miss is
/// this file disagreeing with itself.
fn astc_block(block: (u32, u32)) -> wgpu::AstcBlock {
    match block {
        (4, 4) => wgpu::AstcBlock::B4x4,
        (5, 5) => wgpu::AstcBlock::B5x5,
        (6, 6) => wgpu::AstcBlock::B6x6,
        (8, 8) => wgpu::AstcBlock::B8x8,
        (10, 10) => wgpu::AstcBlock::B10x10,
        (12, 12) => wgpu::AstcBlock::B12x12,
        other => panic!("no atlas format carries the ASTC footprint {other:?}"),
    }
}

/// What identifies a payload in the residency set.
///
/// The table's own index, plus enough of the stored row that two different
/// payloads cannot compare equal.
///
/// # Two tables reach this set, and an index alone does not say which
///
/// An image fill and a baked vector field are both rows of the
/// [`dashpaint::ImageTable`]; a glyph atlas is not — `dashpaint::Atlas` owns its
/// payload directly, because a run's glyph ids are meaningless without the
/// atlas that places them and the two travel together. So there are two index
/// spaces, and index 0 of each is an ordinary value in both.
///
/// `source` separates them. Without it, a document whose first
/// image asset and whose first glyph atlas happen to agree on format and length
/// — two PNGs of the same byte count is not a contrived case — would have the
/// second draw the first, and the only thing that would notice is the debug
/// digest. That is a cache collision, which is precisely what the rest of this
/// key is shaped to make impossible.
///
/// # Why the row travels with the index
///
/// An index is only meaningful in the table that assigned it, and a painter is
/// handed a fresh `&ImageTable` every frame with nothing saying whether it is
/// the same table as last frame's. Assets are append-only within one arena
/// (`dashscene_core::Transaction::add_image`), so within one arena an index
/// keeps its meaning forever — but a rebuilt arena starts again from zero, and
/// index 0 of the new table is a different picture with no symptom to notice.
///
/// This is the same hazard `crate::render::Changes` carries a generation for,
/// and it is closed the same way: by carrying enough of the thing itself that
/// two different payloads cannot compare equal. `ImageEntry` is the format, the
/// pool offset and the length, so a table rebuilt with different content
/// disagrees on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PayloadKey {
    /// Which of the two index spaces [`index`](Self::index) belongs to — see
    /// this type's documentation.
    source: u32,
    index: u32,
    format: u32,
    offset: u32,
    len: u32,
}

impl PayloadKey {
    /// [`PayloadKey::source`] for a row of the image table.
    const IMAGE_TABLE: u32 = 0;
    /// [`PayloadKey::source`] for a glyph run table's atlas.
    const GLYPH_ATLAS: u32 = 1;

    /// The key for image-table row `index`, whose entry `asset` resolved to.
    pub fn image(index: u32, entry: &dashpaint::ImageEntry) -> Self {
        Self {
            source: Self::IMAGE_TABLE,
            index,
            format: entry.format,
            offset: entry.offset,
            len: entry.len,
        }
    }

    /// The key for glyph atlas `index` of the frame's glyph-run table.
    ///
    /// There is no pool offset to carry — an [`dashpaint::Atlas`] owns its
    /// bytes rather than borrowing a range of a blob — so `offset` is zero for
    /// every atlas. The rest of the key is what it is for an image: the format,
    /// the length, and the index, so an atlas set rebuilt with different
    /// content disagrees on it. The same-shape-different-bytes case that
    /// remains is the one the debug digest in [`Residency::resident`] exists
    /// for, and a host that replaces a document says so through
    /// [`Residency::forget_resident`].
    pub fn atlas(index: u32, atlas: &dashpaint::Atlas) -> Self {
        Self {
            source: Self::GLYPH_ATLAS,
            index,
            format: atlas.image.format as u32,
            offset: 0,
            len: u32::try_from(atlas.image.bytes.len())
                .expect("a glyph atlas payload exceeds u32::MAX bytes"),
        }
    }
}

/// Where a resident payload sits: which atlas, and which texels of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    /// Index into `Residency::atlases` — the texture to bind to draw this.
    pub atlas: u32,
    /// The payload's rectangle in that texture, in texels: `[x, y, w, h]`.
    pub rect: [u32; 4],
}

impl Slot {
    /// The rectangle as normalised texture coordinates over an atlas of
    /// `extent` texels: `[u0, v0, du, dv]`.
    ///
    /// The extent is the atlas's own rather than the residency set's, because
    /// they differ: a compressed atlas is rounded down to a whole number of
    /// blocks (see [`AtlasFormat::usable_extent`]), so normalising by the set's
    /// nominal extent would put every coordinate slightly short and sample a
    /// texel or two inside the payload's left edge.
    pub fn uv(&self, extent: (u32, u32)) -> [f32; 4] {
        [
            self.rect[0] as f32 / extent.0 as f32,
            self.rect[1] as f32 / extent.1 as f32,
            self.rect[2] as f32 / extent.0 as f32,
            self.rect[3] as f32 / extent.1 as f32,
        ]
    }
}

/// One atlas texture and the allocator over it.
struct Atlas {
    format: AtlasFormat,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    allocator: etagere::AtlasAllocator,
    /// This texture's own extent in texels, which is the set's nominal extent
    /// rounded down to whole blocks.
    extent: (u32, u32),
}

/// What a resident payload's bookkeeping holds.
struct Residence {
    slot: Slot,
    /// The allocation to hand back on eviction.
    alloc: etagere::AllocId,
    /// The frame this payload was last asked for, so a payload the current
    /// frame needs is never the victim of the current frame's own allocation.
    touched: u64,
    /// A hash of the payload's own bytes as boundary B carried them, checked on
    /// every touch in a debug build — see [`Residency::resident`].
    #[cfg(debug_assertions)]
    digest: u64,
}

/// The residency set: the atlases, what is in them, and the recency order that
/// decides what leaves.
pub struct Residency {
    atlases: Vec<Atlas>,
    resident: HashMap<PayloadKey, Residence>,
    /// Recency, most recently used first. The value is unused: `lru` is here
    /// for the order, which is the part that is tedious and easy to get wrong.
    ///
    /// **Unbounded**, so that eviction rests on no capacity of its own. A bound
    /// here would let the order silently drop its oldest key while the payload
    /// stayed in `resident` and kept its atlas rectangle — after which
    /// [`Residency::evict_one`], which searches this order, could never choose
    /// it, and its space would be unreclaimable. What bounds memory is the
    /// atlas running out of room, which is a different question and the only
    /// one eviction should answer.
    ///
    /// It is emptied by [`Residency::forget_resident`] alongside everything
    /// else keyed on the image table.
    recency: lru::LruCache<PayloadKey, ()>,
    /// Texels on a side of every atlas this set creates.
    extent: u32,
    frame: u64,
    /// Textures and views allocated, counted for the same reason
    /// `crate::render::Renderer::allocations` counts its buffers: a
    /// steady-state frame must allocate nothing, and a comment cannot say so.
    allocations: u64,
    /// Payloads evicted since this set was built, so a test can tell an atlas
    /// that had room from one that made room.
    evictions: u64,
    /// How many encoded payloads have been decoded since this set was built.
    ///
    /// An instrument, for the same reason `dashscene-skia` counts its own
    /// decodes (issue #101): the cost this whole mechanism exists to remove is
    /// invisible in the picture, so a test that only compared pixels would pass
    /// just as happily if every frame decoded its payloads again. It did, until
    /// review caught it.
    decodes: u64,
}

/// Why a payload could not be made resident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidencyError {
    /// The payload is larger than an atlas, so no amount of eviction helps.
    ///
    /// Named rather than scaled down: a painter that quietly halved an image
    /// would draw a plausible wrong picture, and P4 forbids discovering a
    /// limit at draw time. The atlas is `wgpu::Limits::downlevel_defaults`'
    /// `max_texture_dimension_2d` on a side, which is the entry-tier floor this
    /// painter targets.
    TooLarge {
        width: u32,
        height: u32,
        /// The atlas this payload's format is held in, in texels.
        extent: (u32, u32),
    },
    /// The atlas is full of payloads this same frame needs.
    ///
    /// Distinct from [`Self::TooLarge`] because the remedy is different: this
    /// one says a single frame's working set does not fit, where that one says
    /// one payload does not.
    FrameExceedsAtlas {
        format: AtlasFormat,
        resident: usize,
    },
    /// The adapter cannot sample this format at all.
    ///
    /// Unreachable through a host that honours `Painter::samples`, which is
    /// exactly what that declaration is for; kept as a named arm because the
    /// declaration is a promise a host can break.
    UnsupportedFormat { format: AtlasFormat },
}

impl std::fmt::Display for ResidencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge {
                width,
                height,
                extent,
            } => write!(
                f,
                "a {width}x{height} payload does not fit an atlas of {}x{}; this painter does \
                 not tile or downscale one",
                extent.0, extent.1
            ),
            Self::FrameExceedsAtlas { format, resident } => write!(
                f,
                "one frame's payloads do not fit the {format:?} atlas together ({resident} of \
                 them are already resident and needed by this same frame)"
            ),
            Self::UnsupportedFormat { format } => write!(
                f,
                "this adapter cannot sample {format:?}; Painter::samples said so before the \
                 payload was bound"
            ),
        }
    }
}

impl std::error::Error for ResidencyError {}

impl Residency {
    /// An empty residency set whose atlases will be `extent` texels square.
    pub fn new(extent: u32) -> Self {
        Self {
            atlases: Vec::new(),
            resident: HashMap::new(),
            recency: lru::LruCache::unbounded(),
            extent,
            frame: 0,
            allocations: 0,
            evictions: 0,
            decodes: 0,
        }
    }

    /// The extent every atlas in this set is created at, in texels.
    pub fn extent(&self) -> u32 {
        self.extent
    }

    /// Opens a frame. Payloads made resident after this call are safe from its
    /// own evictions.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Forgets every resident payload, keeping the atlas textures.
    ///
    /// # Why this is a call and not a check
    ///
    /// [`PayloadKey`] carries the stored row so that two payloads of *different*
    /// shape cannot collide. Two of the same shape still can — the table's own
    /// test says so — and a rebuilt arena produces exactly that: index 0 of the
    /// new table at the same offset and length as index 0 of the old one, with
    /// different bytes. Nothing in the key can see the difference, and the
    /// digest that can is a debug assertion.
    ///
    /// So the host has to say, and it already does:
    /// [`crate::Renderer::forget_uploaded`] is the "these commits come from a
    /// different chain" signal, wired to `Present::document_replaced`. This is
    /// residency's half of it. The instance rows had this from story #585 and
    /// residency did not, which is the same defect one table over — found in
    /// review, and invisible in a release build.
    ///
    /// The textures stay: they are the expensive objects, their format set is
    /// unchanged by a new document, and the allocators are reset with the
    /// bookkeeping so every texel of them is available again.
    pub fn forget_resident(&mut self) {
        self.resident.clear();
        self.recency.clear();
        for atlas in &mut self.atlases {
            atlas.allocator.clear();
        }
    }

    /// Device objects this set has allocated — one texture and one view per
    /// atlas.
    pub fn allocations(&self) -> u64 {
        self.allocations
    }

    /// Payloads evicted to make room since this set was built.
    pub fn evictions(&self) -> u64 {
        self.evictions
    }

    /// How many encoded payloads this set has decoded since it was built.
    ///
    /// Steady state is zero growth: a resident payload is never decoded again.
    pub fn decodes(&self) -> u64 {
        self.decodes
    }

    /// How many atlases exist, which is how many draw runs a frame can need at
    /// most.
    pub fn atlas_count(&self) -> usize {
        self.atlases.len()
    }

    /// The view for atlas `index`, to bind before drawing what sits in it.
    pub fn view(&self, index: u32) -> &wgpu::TextureView {
        &self.atlases[index as usize].view
    }

    /// Atlas `index`'s own extent in texels — what a [`Slot`] normalises
    /// against.
    pub fn atlas_extent(&self, index: u32) -> (u32, u32) {
        self.atlases[index as usize].extent
    }

    /// Makes `asset` resident and returns where it sits, uploading it if it is
    /// not there already.
    ///
    /// # The debug check, and what it is for
    ///
    /// A cached slot is returned without looking at the bytes, which is the
    /// whole point of a cache and also the whole risk: if the key ever named
    /// two different payloads, this would draw one picture as another with no
    /// symptom. [`PayloadKey`] is built so that cannot happen, and in a debug
    /// build the bytes are hashed on every touch and compared against what was
    /// uploaded, so the claim is checked rather than reasoned about. The dirty
    /// set's own equivalent defect was found exactly this way and no other.
    pub fn resident(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: PayloadKey,
        asset: ImageRef<'_>,
    ) -> Result<Slot, ResidencyError> {
        // The cache check comes first, and nothing above it touches the payload.
        //
        // `TexelPayload::of` used to run before this, which meant a resident PNG
        // was fully decoded on every frame that drew it and the result thrown
        // away — the whole cost residency exists to remove, and the exact number
        // story #581 was opened for (PNG decode was 20.4 % of every frame).
        // Found in review; nothing about the picture would have shown it.
        if let Some(existing) = self.resident.get_mut(&key) {
            // Over the payload's own bytes, not the decoded texels, so this
            // check costs no decode either. It is the same question — are these
            // the bytes that were uploaded under this key — asked of the thing
            // boundary B actually carries.
            #[cfg(debug_assertions)]
            debug_assert_eq!(
                existing.digest,
                digest(asset.bytes),
                "a resident payload's key named different bytes than the ones uploaded under it: \
                 the residency cache would draw one image as another"
            );
            existing.touched = self.frame;
            self.recency.put(key, ());
            return Ok(existing.slot);
        }

        let texels = TexelPayload::of(asset);
        if asset.format.is_encoded() {
            self.decodes += 1;
        }
        let format = texels.atlas_format();
        if let Some(feature) = format.required_feature()
            && !device.features().contains(feature)
        {
            return Err(ResidencyError::UnsupportedFormat { format });
        }
        let usable = format.usable_extent(self.extent);
        if texels.width > usable.0 || texels.height > usable.1 {
            return Err(ResidencyError::TooLarge {
                width: texels.width,
                height: texels.height,
                extent: usable,
            });
        }

        let atlas = self.atlas_for(device, format);
        let (alloc, rect) = self.allocate(atlas, texels.width, texels.height)?;
        let slot = Slot { atlas, rect };
        texels.upload(queue, &self.atlases[atlas as usize].texture, rect);

        self.resident.insert(
            key,
            Residence {
                slot,
                alloc,
                touched: self.frame,
                #[cfg(debug_assertions)]
                digest: digest(asset.bytes),
            },
        );
        self.recency.put(key, ());
        Ok(slot)
    }

    /// The atlas holding `format`, creating it if this is the first payload of
    /// that format.
    fn atlas_for(&mut self, device: &wgpu::Device, format: AtlasFormat) -> u32 {
        if let Some(index) = self.atlases.iter().position(|a| a.format == format) {
            return index as u32;
        }
        let extent = format.usable_extent(self.extent);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dashscene-gpu atlas"),
            size: wgpu::Extent3d {
                width: extent.0,
                height: extent.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format.texture_format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (bx, by) = format.block();
        // The allocator counts in blocks, so every rectangle it returns lands
        // on a block boundary by construction rather than by rounding after the
        // fact. For an uncompressed atlas the block is one texel and this is
        // the identity.
        self.atlases.push(Atlas {
            format,
            texture,
            view,
            allocator: etagere::AtlasAllocator::new(etagere::size2(
                (extent.0 / bx) as i32,
                (extent.1 / by) as i32,
            )),
            extent,
        });
        self.allocations += 2;
        (self.atlases.len() - 1) as u32
    }

    /// Allocates a `width` x `height` texel rectangle in atlas `index`,
    /// evicting by recency until it fits.
    fn allocate(
        &mut self,
        index: u32,
        width: u32,
        height: u32,
    ) -> Result<(etagere::AllocId, [u32; 4]), ResidencyError> {
        let (bx, by) = self.atlases[index as usize].format.block();
        let blocks = etagere::size2(
            width.div_ceil(bx).max(1) as i32,
            height.div_ceil(by).max(1) as i32,
        );

        loop {
            if let Some(allocation) = self.atlases[index as usize].allocator.allocate(blocks) {
                let min = allocation.rectangle.min;
                return Ok((
                    allocation.id,
                    [min.x as u32 * bx, min.y as u32 * by, width, height],
                ));
            }
            if !self.evict_one(index)? {
                let resident = self
                    .resident
                    .values()
                    .filter(|r| r.slot.atlas == index)
                    .count();
                return Err(ResidencyError::FrameExceedsAtlas {
                    format: self.atlases[index as usize].format,
                    resident,
                });
            }
        }
    }

    /// Frees the least recently used payload in atlas `index`, reporting
    /// whether one was found.
    ///
    /// A payload this frame has already asked for is never a victim: evicting
    /// it would free space the same frame is about to need again, and the
    /// caller would loop. `false` means every resident payload of this atlas is
    /// in the current frame's own working set.
    fn evict_one(&mut self, index: u32) -> Result<bool, ResidencyError> {
        let victim = self.recency.iter().rev().map(|(key, ())| *key).find(|key| {
            self.resident
                .get(key)
                .is_some_and(|r| r.slot.atlas == index && r.touched < self.frame)
        });
        let Some(victim) = victim else {
            return Ok(false);
        };
        let residence = self
            .resident
            .remove(&victim)
            .expect("the recency order names a resident payload");
        self.recency.pop(&victim);
        self.atlases[index as usize]
            .allocator
            .deallocate(residence.alloc);
        self.evictions += 1;
        Ok(true)
    }
}

/// A payload's texels in the layout its atlas stores, and the extent they cover.
///
/// The one place a decode happens, and the one place that decides it does not
/// need to: a baked payload's bytes are already what the texture holds, so they
/// are borrowed rather than copied. That borrow is what
/// `docs/specification/03-target-hardware-rules.md` means by "no transcode step
/// of any kind" — the bytes that were packed offline are the bytes the queue
/// writes.
enum Texels<'a> {
    /// Already in the texture's own layout: uploaded as they arrived.
    Baked {
        format: AtlasFormat,
        bytes: &'a [u8],
    },
    /// Decoded from a container into eight-bit RGBA.
    Decoded { rgba: Vec<u8> },
}

/// A payload ready to upload: its texels and the extent they cover.
struct TexelPayload<'a> {
    texels: Texels<'a>,
    width: u32,
    height: u32,
}

impl<'a> TexelPayload<'a> {
    fn atlas_format(&self) -> AtlasFormat {
        match &self.texels {
            Texels::Baked { format, .. } => *format,
            Texels::Decoded { .. } => AtlasFormat::Rgba8,
        }
    }

    fn bytes(&self) -> &[u8] {
        match &self.texels {
            Texels::Baked { bytes, .. } => bytes,
            Texels::Decoded { rgba } => rgba,
        }
    }

    /// Writes these texels into `texture` at `rect`.
    fn upload(&self, queue: &wgpu::Queue, texture: &wgpu::Texture, rect: [u32; 4]) {
        let (bx, by) = self.atlas_format().block();
        let bytes_per_block = match self.atlas_format() {
            AtlasFormat::Rgba8 => 4,
            AtlasFormat::Astc { .. } => 16,
        };
        let blocks_across = rect[2].div_ceil(bx);
        let rows = rect[3].div_ceil(by);
        // The copy covers whole blocks, not the payload's logical extent. A
        // block-compressed copy must be a multiple of the footprint unless it
        // reaches the texture's own edge, and an allocation in the middle of an
        // atlas never does — a 380x380 payload at a 6x6 footprint is refused by
        // name as "copy width is not a multiple of block width".
        //
        // The extra texels are the ones the encoder already padded the last
        // block row and column with, and the allocator reserved room for them:
        // it allocates in blocks, so the rectangle it returned is
        // `blocks_across` wide however few texels of the last block are the
        // payload's own. The identity for an uncompressed atlas, whose block is
        // one texel.
        let copy = (blocks_across * bx, rows * by);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect[0],
                    y: rect[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            self.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(blocks_across * bytes_per_block),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: copy.0,
                height: copy.1,
                depth_or_array_layers: 1,
            },
        );
    }
}

impl<'a> TexelPayload<'a> {
    /// The texels `asset` becomes.
    ///
    /// # Panics
    ///
    /// Panics on JPEG and GIF, the two containers this painter links no decoder
    /// for. Reaching here means the binding ignored `Painter::samples`, which is
    /// the same contract breach `dashscene-skia` asserts on for the opposite
    /// half, and it cannot be reported from inside a frame because
    /// `Painter::paint` returns nothing by decision.
    ///
    /// The other half of that declaration — a block format on a device without
    /// `TEXTURE_COMPRESSION_ASTC` — does **not** panic here. It cannot be
    /// decided from the payload alone, so it is refused where the device is in
    /// scope, as [`ResidencyError::UnsupportedFormat`].
    fn of(asset: ImageRef<'a>) -> Self {
        match asset.format {
            ImageFormat::Png => {
                let (width, height, rgba) = decode_png(asset.bytes);
                Self {
                    texels: Texels::Decoded { rgba },
                    width,
                    height,
                }
            }
            ImageFormat::Jpeg | ImageFormat::Gif => panic!(
                "this painter was handed a {:?} payload, which it declared it cannot sample \
                 (Painter::samples, issue #718); it links one decoder and that is PNG",
                asset.format
            ),
            baked => Self {
                texels: Texels::Baked {
                    format: AtlasFormat::of(baked),
                    bytes: asset.bytes,
                },
                width: asset.width,
                height: asset.height,
            },
        }
    }
}

/// Decodes a PNG into eight-bit RGBA.
///
/// # Every colour type, because a texture has only one
///
/// `goldens/tooling` decodes the corpus photographs and accepts RGB and RGBA
/// alone, which is true of those files. It is not true of PNG in general and it
/// was not true of the first fixture this was pointed at: an **indexed** image,
/// which is what any tool writing a small flat-coloured picture produces, and
/// which a two-arm match rejects by name at the moment it is drawn.
///
/// `normalize_to_color8` is the decoder's own answer — it expands a palette,
/// widens sub-byte greyscale, strips sixteen-bit channels down to eight, and
/// turns a `tRNS` chunk into a real alpha channel — after which the four arms
/// below are the whole of what can arrive. There is no fifth to forget.
///
/// # Panics
///
/// Panics on a payload that does not decode. Asset payloads are validated
/// upstream — `dashc`'s image-identity gate reads the same header — so a
/// failure here is a broken contract between crates (P4), the same as an
/// out-of-range table index.
fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .expect("an image payload has a readable PNG header (validated upstream, P4)");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded frame")];
    let info = reader
        .next_frame(&mut buffer)
        .expect("an image payload decodes (validated upstream, P4)");
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        // `normalize_to_color8` expands a palette into one of the four above, so
        // this is unreachable rather than unhandled — named so that a decoder
        // that stopped expanding says which type arrived.
        other => {
            panic!("an image payload is {other:?} after normalisation, which expands every palette")
        }
    };
    (info.width, info.height, rgba)
}

/// A cheap content hash, for the debug residency check only.
///
/// Not a content address and not stable across builds: it exists to make a
/// stale cache entry fail an assertion in a debug build, and `DefaultHasher` is
/// exactly strong enough for that.
#[cfg(debug_assertions)]
fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashpaint::{ImageAsset, ImageTable};

    /// A device, without a pipeline. Panics rather than skipping, for the reason
    /// the layer-3 suite gives: a residency test that quietly passes with no
    /// device establishes nothing about residency.
    fn device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .expect("residency needs a device");
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("residency test"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .expect("residency needs a device")
    }

    /// A baked RGBA payload of the given extent, filled with `seed`.
    fn baked(width: u32, height: u32, seed: u8) -> ImageAsset {
        ImageAsset {
            format: ImageFormat::Rgba8Unorm,
            bytes: vec![seed; (width * height * 4) as usize],
        }
    }

    /// A table of the given payloads, and the keys that name them.
    fn table(assets: Vec<(u32, u32, u8)>) -> ImageTable {
        let mut images = ImageTable::new();
        for (w, h, seed) in assets {
            images.push_baked(baked(w, h, seed), w, h);
        }
        images
    }

    fn key(images: &ImageTable, index: u32) -> PayloadKey {
        PayloadKey::image(index, &images.all_entries()[index as usize])
    }

    /// Every payload of one format shares one atlas, and each gets its own
    /// rectangle.
    ///
    /// The rectangles are asserted to be *disjoint* rather than merely
    /// different: an allocator that handed out overlapping rectangles would give
    /// each payload a distinct origin and still let one overwrite another.
    #[test]
    fn payloads_of_one_format_share_an_atlas_at_disjoint_rectangles() {
        let (device, queue) = device();
        let mut residency = Residency::new(256);
        residency.begin_frame();
        // Three different extents, so no two allocations can coincide by
        // symmetry.
        let images = table(vec![(16, 8, 1), (8, 32, 2), (24, 24, 3)]);

        let slots: Vec<Slot> = (0..3)
            .map(|index| {
                residency
                    .resident(&device, &queue, key(&images, index), images.resolve(index))
                    .expect("a small payload is resident")
            })
            .collect();

        assert_eq!(residency.atlas_count(), 1, "one format, one atlas");
        assert!(
            slots.iter().all(|s| s.atlas == 0),
            "every payload of one format lands in that format's atlas"
        );
        for (i, a) in slots.iter().enumerate() {
            for b in slots.iter().skip(i + 1) {
                let disjoint = a.rect[0] + a.rect[2] <= b.rect[0]
                    || b.rect[0] + b.rect[2] <= a.rect[0]
                    || a.rect[1] + a.rect[3] <= b.rect[1]
                    || b.rect[1] + b.rect[3] <= a.rect[1];
                assert!(disjoint, "allocations overlap: {a:?} and {b:?}");
            }
        }
        // The rectangle is the payload's own extent, not the allocator's
        // rounding of it.
        assert_eq!(slots[0].rect[2..], [16, 8]);
        assert_eq!(slots[1].rect[2..], [8, 32]);
        assert_eq!(slots[2].rect[2..], [24, 24]);
    }

    /// Asking again for a resident payload returns the slot it already has and
    /// uploads nothing.
    #[test]
    fn a_resident_payload_keeps_its_slot_across_frames() {
        let (device, queue) = device();
        let mut residency = Residency::new(256);
        let images = table(vec![(16, 8, 1)]);

        residency.begin_frame();
        let first = residency
            .resident(&device, &queue, key(&images, 0), images.resolve(0))
            .expect("resident");
        residency.begin_frame();
        let again = residency
            .resident(&device, &queue, key(&images, 0), images.resolve(0))
            .expect("still resident");

        assert_eq!(first, again);
        assert_eq!(
            residency.atlas_count(),
            1,
            "a second frame must not build a second atlas for the same format"
        );
        assert_eq!(residency.evictions(), 0);
    }

    /// A full atlas makes room by evicting, and the payload the current frame
    /// has already touched survives.
    ///
    /// # What this does *not* establish, and where that is established
    ///
    /// It does not test the frame-aware half of the eviction rule. Removing
    /// `touched < self.frame` from `evict_one` leaves this test green, because
    /// the payload it re-checks is also the most recently used one and the
    /// recency order alone spares it. The guard only changes an outcome when
    /// **every** resident payload of an atlas belongs to the current frame, and
    /// that case is `a_frame_that_cannot_fit_its_own_working_set_is_named`
    /// below — which is where the guard's real value is: without it, a frame
    /// evicts a payload it has already resolved a slot for, hands that slot to
    /// another payload, and draws one image as another with no error at all.
    ///
    /// Named for what it checks rather than for what the code does, after a
    /// mutation showed the two were not the same thing.
    #[test]
    fn a_full_atlas_evicts_to_make_room_and_keeps_what_was_just_used() {
        let (device, queue) = device();
        // Four 32x32 payloads exactly fill a 64x64 atlas.
        let mut residency = Residency::new(64);
        let images = table(vec![
            (32, 32, 1),
            (32, 32, 2),
            (32, 32, 3),
            (32, 32, 4),
            (32, 32, 5),
        ]);

        residency.begin_frame();
        for index in 0..4 {
            residency
                .resident(&device, &queue, key(&images, index), images.resolve(index))
                .expect("the first four fit");
        }
        assert_eq!(
            residency.evictions(),
            0,
            "four payloads fill the atlas exactly"
        );

        // A new frame that touches payload 3 first, then asks for a fifth. The
        // fifth needs room, and the victim must be one of the untouched ones.
        residency.begin_frame();
        residency
            .resident(&device, &queue, key(&images, 3), images.resolve(3))
            .expect("still resident");
        residency
            .resident(&device, &queue, key(&images, 4), images.resolve(4))
            .expect("room is made for the fifth");
        assert_eq!(residency.evictions(), 1, "exactly one payload made way");

        // Payload 3 was touched this frame, so it is still where it was.
        residency
            .resident(&device, &queue, key(&images, 3), images.resolve(3))
            .expect("this frame's own payload was not the victim");
        assert_eq!(
            residency.evictions(),
            1,
            "re-asking for a payload this frame touched must not evict anything"
        );
    }

    /// A frame whose own working set does not fit is named rather than
    /// corrupted.
    ///
    /// This is the test the frame-aware eviction guard lives or dies by.
    /// Without the guard the fifth payload succeeds — by evicting one of the
    /// four this same frame has already been given a slot for, whose row still
    /// names the rectangle that now holds someone else's texels. The picture is
    /// then one image drawn as another, with no error anywhere. With the guard
    /// there is nothing evictable and the refusal says so.
    #[test]
    fn a_frame_that_cannot_fit_its_own_working_set_is_named() {
        let (device, queue) = device();
        let mut residency = Residency::new(64);
        let images = table(vec![
            (32, 32, 1),
            (32, 32, 2),
            (32, 32, 3),
            (32, 32, 4),
            (32, 32, 5),
        ]);
        residency.begin_frame();
        for index in 0..4 {
            residency
                .resident(&device, &queue, key(&images, index), images.resolve(index))
                .expect("the first four fit");
        }
        let refused = residency.resident(&device, &queue, key(&images, 4), images.resolve(4));
        assert!(
            matches!(refused, Err(ResidencyError::FrameExceedsAtlas { .. })),
            "a fifth payload in the same frame has nothing evictable: {refused:?}"
        );
    }

    /// A payload larger than an atlas is named rather than scaled or tiled.
    #[test]
    fn a_payload_larger_than_the_atlas_is_named() {
        let (device, queue) = device();
        let mut residency = Residency::new(64);
        let images = table(vec![(128, 8, 1)]);
        residency.begin_frame();
        let refused = residency.resident(&device, &queue, key(&images, 0), images.resolve(0));
        assert!(
            matches!(
                refused,
                Err(ResidencyError::TooLarge {
                    width: 128,
                    height: 8,
                    extent: (64, 64)
                })
            ),
            "an oversized payload must name its own extent: {refused:?}"
        );
    }

    /// Two payloads at the same table index but from different tables are
    /// different keys.
    ///
    /// The rebuilt-arena hazard. An index alone would make the second payload a
    /// cache hit on the first, and the picture would be one image drawn as
    /// another with nothing to notice.
    #[test]
    fn a_rebuilt_table_does_not_collide_with_the_one_before_it() {
        let first = table(vec![(16, 8, 1)]);
        // A different payload at the same index, of a different length, which is
        // what a rebuilt arena produces.
        let second = table(vec![(8, 8, 2)]);
        assert_ne!(
            key(&first, 0),
            key(&second, 0),
            "index 0 of two different tables must not be one residency key"
        );

        // And the harder case: same length, same offset, different bytes. The
        // key cannot separate these, which is why `Residency::resident` checks
        // the bytes in a debug build rather than trusting the key alone.
        let same_shape = table(vec![(16, 8, 9)]);
        assert_eq!(
            key(&first, 0),
            key(&same_shape, 0),
            "this is the case the debug digest exists for, and it is stated here so that a \
             change making the key finer is a deliberate one"
        );
    }

    /// Every image format maps to an atlas format, and the mapping is by block
    /// footprint rather than by colour space.
    #[test]
    fn the_atlas_format_folds_the_colour_space_and_nothing_else() {
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc6x6Srgb),
            AtlasFormat::of(ImageFormat::Astc6x6Unorm),
            "the two colour spaces of one footprint share an atlas"
        );
        assert_ne!(
            AtlasFormat::of(ImageFormat::Astc6x6Srgb),
            AtlasFormat::of(ImageFormat::Astc8x8Srgb),
            "two footprints cannot share a texture"
        );
        assert_eq!(AtlasFormat::of(ImageFormat::Png), AtlasFormat::Rgba8);
        assert_eq!(AtlasFormat::of(ImageFormat::Rgba8Srgb), AtlasFormat::Rgba8);

        // The six footprints are six distinct atlases, and each names its own
        // block. A mapping that returned one footprint for two formats would put
        // one payload's blocks in a texture that reads them at another size.
        let footprints: Vec<(u32, u32)> = [
            ImageFormat::Astc4x4Srgb,
            ImageFormat::Astc5x5Srgb,
            ImageFormat::Astc6x6Srgb,
            ImageFormat::Astc8x8Srgb,
            ImageFormat::Astc10x10Srgb,
            ImageFormat::Astc12x12Srgb,
        ]
        .iter()
        .map(|f| AtlasFormat::of(*f).block())
        .collect();
        assert_eq!(
            footprints,
            [(4, 4), (5, 5), (6, 6), (8, 8), (10, 10), (12, 12)]
        );
        // Every ASTC atlas asks for the feature, and no other does.
        assert_eq!(AtlasFormat::Rgba8.required_feature(), None);
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc4x4Unorm).required_feature(),
            Some(wgpu::Features::TEXTURE_COMPRESSION_ASTC)
        );
    }

    /// A slot's normalised rectangle is its texels over the atlas extent, on
    /// each axis independently.
    #[test]
    fn a_slots_uv_is_its_texels_over_the_extent() {
        let slot = Slot {
            atlas: 0,
            // Asymmetric in all four numbers, so a swapped axis or a reused
            // component fails.
            rect: [16, 32, 64, 8],
        };
        assert_eq!(slot.uv((128, 128)), [0.125, 0.25, 0.5, 0.0625]);
        // A non-square atlas normalises each axis against its own extent, which
        // is what a compressed atlas rounded down per axis actually is.
        assert_eq!(slot.uv((128, 64)), [0.125, 0.5, 0.5, 0.125]);

        // Four of the six footprints do not divide a power-of-two extent, so
        // rounding down is not a no-op for them — and rounding *up* would ask
        // the device for a texture past the limit it stated.
        assert_eq!(AtlasFormat::Rgba8.usable_extent(2048), (2048, 2048));
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc4x4Srgb).usable_extent(2048),
            (2048, 2048)
        );
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc6x6Srgb).usable_extent(2048),
            (2046, 2046)
        );
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc5x5Srgb).usable_extent(2048),
            (2045, 2045)
        );
        assert_eq!(
            AtlasFormat::of(ImageFormat::Astc12x12Srgb).usable_extent(2048),
            (2040, 2040)
        );
    }
}
