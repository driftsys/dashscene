//! What the scenes draw with: the corpus fonts, the committed glyph
//! atlases, a corpus image payload, and one baked vector field.
//!
//! Everything here is loaded from `corpus/`, not generated, and everything
//! here is loaded **once**. The host rebuilds a scene on every resize step,
//! so a font parse or an MSDF bake in the build path would run hundreds of
//! times during one window drag.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use dashc_wasm::figma::vector_field::{VectorAtlasBaker, VectorPath, WindingRule};
use dashpaint::{Atlas, AtlasGlyph, ImageAsset, ImageFormat, VectorField};
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, FontFamily, Typesetter, WeightedFont};

/// A path under the repository root, resolved from this crate's manifest
/// directory (`corpus/showcase`).
macro_rules! corpus {
    ($tail:literal) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $tail)
    };
}

const FONT_INTER_REGULAR: &str = corpus!("corpus/fonts/inter/Inter-Regular.otf");
const FONT_INTER_SEMIBOLD: &str = corpus!("corpus/fonts/inter/Inter-SemiBold.otf");
const FONT_ARABIC: &str = corpus!("corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf");

const ATLAS_INTER_REGULAR: &str = corpus!("corpus/atlas/inter-ascii");
const ATLAS_INTER_SEMIBOLD: &str = corpus!("corpus/atlas/inter-ascii-semibold");
const ATLAS_ARABIC: &str = corpus!("corpus/atlas/arabic");

/// The corpus photograph the image fills draw. A 512x512 CC0 payload
/// committed for the asset pipeline's band measurements (`corpus/photo/`),
/// reused here rather than a new picture being added to the tree.
const PHOTO: &str = corpus!("corpus/photo/dawn-mountains.png");

/// The family name every Latin run in the showcase asks for. It must be one
/// the cascade below declares, or the run falls back and the atlas slot it
/// samples is no longer the face it was authored against.
pub const LATIN_FAMILY: &str = "Inter";

/// The family name every Arabic run asks for. Coverage outranks the family
/// request in the cascade, so an Arabic run reaches this family even when it
/// asks for Inter — naming it is what keeps the intent visible in the scene
/// source.
pub const ARABIC_FAMILY: &str = "Noto Sans Arabic";

/// The cascade the showcase is authored against: Inter at 400 and 600, then
/// Noto Sans Arabic at 400.
///
/// Three faces rather than the goldens' eight. The atlas list beside it is
/// cloned once per commit that stages text, so every face carried here that
/// no scene uses is a per-frame copy of an atlas nothing samples.
///
/// **The order is the contract.** Families flatten family-major, so a shaped
/// glyph's font slot is `0` for Inter Regular, `1` for Inter SemiBold and `2`
/// for Noto Sans Arabic, and that slot indexes [`atlases`] directly. A list in
/// any other order samples the wrong face rather than failing, which is why
/// both are built in this one module.
fn typesetter() -> Typesetter {
    let load = |path: &str, what: &str| {
        Font::from_bytes(
            std::fs::read(path).unwrap_or_else(|error| panic!("corpus font {what}: {error}")),
            0,
        )
        .unwrap_or_else(|error| panic!("corpus font {what} parses: {error}"))
    };
    Typesetter::with_named_font_families(vec![
        FontFamily::new(
            LATIN_FAMILY,
            vec![
                WeightedFont::new(load(FONT_INTER_REGULAR, "Inter Regular"), 400),
                WeightedFont::new(load(FONT_INTER_SEMIBOLD, "Inter SemiBold"), 600),
            ],
        ),
        FontFamily::new(
            ARABIC_FAMILY,
            vec![WeightedFont::new(
                load(FONT_ARABIC, "Noto Sans Arabic Regular"),
                400,
            )],
        ),
    ])
}

/// A fresh typesetter for one scene. Each live scene owns its own, because
/// the solver that shapes with it needs it exclusively.
pub fn new_typesetter() -> Typesetter {
    typesetter()
}

/// The atlases the staged runs sample, in the cascade's font-slot order.
///
/// Shared behind an `Arc` because commit rebuilds the glyph-run table every
/// frame while the atlas set behind it never changes.
pub fn atlases() -> Arc<Vec<Atlas>> {
    static ATLASES: LazyLock<Arc<Vec<Atlas>>> = LazyLock::new(|| {
        Arc::new(vec![
            load_atlas(ATLAS_INTER_REGULAR),
            load_atlas(ATLAS_INTER_SEMIBOLD),
            load_atlas(ATLAS_ARABIC),
        ])
    });
    Arc::clone(&ATLASES)
}

/// Converts a committed build-time atlas fixture at `dir` into a boundary-B
/// [`Atlas`]. Only glyphs that paint (bounded outlines) carry a quad, so an
/// empty-outline glyph such as the space is dropped.
fn load_atlas(dir: &str) -> Atlas {
    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|error| panic!("committed atlas fixture at {dir} loads: {error}"));
    let metrics = &bundle.metrics;
    let glyphs = metrics
        .glyphs
        .iter()
        .filter_map(|glyph| {
            Some(AtlasGlyph {
                glyph_id: glyph.glyph_id,
                plane_em: glyph.plane_em?,
                atlas_px: glyph.atlas_px?,
            })
        })
        .collect();
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: bundle.image_png.clone(),
        },
        metrics.atlas.width,
        metrics.atlas.height,
        metrics.atlas.px_per_em,
        metrics.atlas.distance_range_px,
        glyphs,
    )
}

/// The corpus photograph, as the payload an image fill references.
pub fn photo() -> ImageAsset {
    static PAYLOAD: LazyLock<Arc<Vec<u8>>> = LazyLock::new(|| {
        Arc::new(std::fs::read(PHOTO).unwrap_or_else(|error| panic!("corpus photograph: {error}")))
    });
    ImageAsset {
        format: ImageFormat::Png,
        bytes: PAYLOAD.as_ref().clone(),
    }
}

/// A baked MSDF field and the atlas PNG it samples.
pub struct BakedVector {
    /// The packed distance-field atlas, to be staged with `Txn::add_image`.
    pub atlas: ImageAsset,
    /// The field's placement, less the image index — a scene fills that in
    /// once it knows what `add_image` returned.
    pub atlas_rect: [u32; 4],
    pub plane_bounds: [f32; 4],
    pub distance_range: f32,
}

impl BakedVector {
    /// The field, bound to the image index the arena gave its atlas.
    pub fn field(&self, image: u32) -> VectorField {
        VectorField {
            image,
            atlas_rect: self.atlas_rect,
            plane_bounds: self.plane_bounds,
            distance_range: self.distance_range,
        }
    }
}

/// A five-pointed star with a pentagonal hole, wound so the hole reads
/// through under the even-odd rule.
///
/// The outline is written in a unit square and scaled at bake time, because a
/// baked field paints at the size it was baked: `plane_bounds` are in the
/// shape's own space and the painter maps them to device space unscaled, so a
/// field baked once for a small window is still small in a large one.
///
/// It is authored as a path here rather than captured from Figma on purpose.
/// This is the one construct in the vocabulary whose point is that no painter
/// rasterises a path (P2), and it is baked by the same `dashc` generator a
/// Figma VECTOR node lowers through, so what the scene carries is the field
/// and never the outline.
const STAR_POINTS: [(f32, f32); 10] = [
    (0.500, 0.000),
    (0.618, 0.363),
    (1.000, 0.363),
    (0.691, 0.588),
    (0.809, 0.951),
    (0.500, 0.726),
    (0.191, 0.951),
    (0.309, 0.588),
    (0.000, 0.363),
    (0.382, 0.363),
];

/// The pentagonal hole, in the same unit square.
const STAR_HOLE: [(f32, f32); 5] = [
    (0.500, 0.276),
    (0.633, 0.373),
    (0.582, 0.529),
    (0.418, 0.529),
    (0.367, 0.373),
];

/// The star's outline at `size` units on a side.
fn star_path(size: f32) -> String {
    let mut path = String::new();
    for (index, (x, y)) in STAR_POINTS.iter().enumerate() {
        let verb = if index == 0 { 'M' } else { 'L' };
        path.push_str(&format!("{verb} {:.3} {:.3} ", x * size, y * size));
    }
    path.push_str("Z ");
    for (index, (x, y)) in STAR_HOLE.iter().enumerate() {
        let verb = if index == 0 { 'M' } else { 'L' };
        path.push_str(&format!("{verb} {:.3} {:.3} ", x * size, y * size));
    }
    path.push('Z');
    path
}

/// The bake sizes are rounded to this many units, so a window drag asks for a
/// handful of distinct bakes rather than one per pixel of travel.
const BAKE_QUANTUM: u32 = 16;

/// The star baked to fill a box of about `size` units on a side.
///
/// Memoized on the rounded size: the host rebuilds a scene on every resize
/// step, and an MSDF bake per step would be paid on every one of them.
pub fn baked_star(size: f32) -> Arc<BakedVector> {
    static BAKES: LazyLock<Mutex<HashMap<u32, Arc<BakedVector>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let quantized = ((size.max(BAKE_QUANTUM as f32) / BAKE_QUANTUM as f32).round() as u32).max(1)
        * BAKE_QUANTUM;
    let mut bakes = BAKES.lock().expect("the bake cache is never poisoned");
    Arc::clone(bakes.entry(quantized).or_insert_with(|| {
        let mut baker = VectorAtlasBaker::new();
        let shape = baker
            .add(&VectorPath {
                path: &star_path(quantized as f32),
                winding: WindingRule::EvenOdd,
            })
            .expect("the showcase star path bakes");
        let baked = baker.finish().expect("the showcase star atlas packs");
        let placement = &baked.shapes[shape as usize];
        let rect = placement.atlas_rect;
        let plane = placement.plane_bounds;
        Arc::new(BakedVector {
            atlas: ImageAsset {
                format: ImageFormat::Png,
                bytes: baked.image_png,
            },
            atlas_rect: [rect.x, rect.y, rect.width, rect.height],
            plane_bounds: [
                plane.left as f32,
                plane.top as f32,
                plane.right as f32,
                plane.bottom as f32,
            ],
            distance_range: baked.distance_range as f32,
        })
    }))
}
