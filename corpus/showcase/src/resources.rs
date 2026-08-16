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
use dashpaint::{Atlas, ImageAsset, ImageFormat, VectorField};
use dashscene_engine::{AtlasBytes, FaceBytes, TaffySolver, TextResources};
use dashscene_typeset::text::Typesetter;

/// A path under the repository root, resolved from this crate's manifest
/// directory (`corpus/showcase`).
macro_rules! corpus {
    ($tail:literal) => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../", $tail)
    };
}

/// The bytes of a corpus file, however this target can reach them.
///
/// On a **development machine** that is a read from disk: the binary stays
/// small, and an edited corpus file is picked up without rebuilding. On a target
/// that cannot see this repository's files the same bytes are embedded at
/// compile time instead — roughly 1.1 MB across the three fonts, three atlases
/// and one photograph the scenes use.
///
/// The two arms can be one list because [`corpus`] already resolves to a
/// compile-time literal, which is exactly what `include_bytes!` requires.
///
/// # What the condition actually is
///
/// **Not "is this wasm".** It is "can this target open a path under
/// `CARGO_MANIFEST_DIR` at run time", and the answer is no for every target that
/// runs somewhere other than the machine that compiled it.
///
/// The gate said `wasm32` for one slice, because the browser was the only such
/// target when it was written — and the comment recorded the symptom: the
/// browser host compiled and then panicked on its first scene,
/// `operation not supported on this platform`, from `std::fs::read`. **Android
/// is the second, and the gate did not cover it**: `target_arch` there is
/// `aarch64`, so a device took the filesystem arm and tried to read an absolute
/// path from a developer's laptop. The scene never built and the frame loop
/// never ran (story #842).
///
/// A build succeeding still says nothing about a build running. The condition is
/// named for what it is now, so the next such target is a question about this
/// list rather than a silent third occurrence.
macro_rules! corpus_bytes {
    ($tail:literal) => {{
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            include_bytes!(corpus!($tail)).to_vec()
        }
        #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
        {
            std::fs::read(corpus!($tail))
                .unwrap_or_else(|error| panic!("corpus file {}: {error}", corpus!($tail)))
        }
    }};
}

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
/// Noto Sans Arabic at 400, each with the committed sheet its glyphs sample.
///
/// Three faces rather than the goldens' eight. A face no scene uses is not
/// free: its font is parsed again on every scene build, and its sheet is
/// converted once and then held for the run of the program. It is no longer a
/// per-frame copy of the payload, which is what the sentence here used to say —
/// the crate's own solver wrapper stopped deep-copying the set at issue #621,
/// and since issue #950 there is no wrapper: [`solver`] shares the [`Arc`] the
/// engine holds.
///
/// **The pairing is the contract.** Families flatten family-major, so a shaped
/// glyph's font slot is `0` for Inter Regular, `1` for Inter SemiBold and `2`
/// for Noto Sans Arabic, and that slot indexes [`atlases`] directly.
/// Reordering this list is safe on its own: [`TextResources::from_faces`]
/// pushes a face's sheet in the same step it pushes the face, so both move
/// together and only the cascade's coverage order changes. Handing a face the
/// **wrong** sheet is what samples one face through another's atlas, and it
/// fails no assertion — a valid sheet for the wrong face is still a valid
/// sheet. That is why the two are written side by side on one entry here.
///
/// `with_atlases` says which of the two is being fed, because they cannot be
/// one call. [`atlases`] converts its sheets once and shares the result for
/// the whole run of the program, while a scene's typesetter is built fresh per
/// scene — the solver that shapes with it needs it exclusively, and
/// [`Typesetter`] is not [`Clone`]. So what is declared once is this list, and
/// not the single [`TextResources::from_faces`] walk that binds a cascade to
/// its atlases inside the engine.
fn faces(with_atlases: bool) -> Vec<FaceBytes> {
    vec![
        FaceBytes {
            family: LATIN_FAMILY.to_string(),
            weight: 400,
            font: corpus_bytes!("corpus/fonts/inter/Inter-Regular.otf"),
            face_index: 0,
            atlas: with_atlases.then(|| AtlasBytes {
                png: corpus_bytes!("corpus/atlas/inter-ascii/atlas.png"),
                metrics: corpus_bytes!("corpus/atlas/inter-ascii/atlas.metrics"),
            }),
        },
        FaceBytes {
            family: LATIN_FAMILY.to_string(),
            weight: 600,
            font: corpus_bytes!("corpus/fonts/inter/Inter-SemiBold.otf"),
            face_index: 0,
            atlas: with_atlases.then(|| AtlasBytes {
                png: corpus_bytes!("corpus/atlas/inter-ascii-semibold/atlas.png"),
                metrics: corpus_bytes!("corpus/atlas/inter-ascii-semibold/atlas.metrics"),
            }),
        },
        FaceBytes {
            family: ARABIC_FAMILY.to_string(),
            weight: 400,
            font: corpus_bytes!("corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"),
            face_index: 0,
            atlas: with_atlases.then(|| AtlasBytes {
                png: corpus_bytes!("corpus/atlas/arabic/atlas.png"),
                metrics: corpus_bytes!("corpus/atlas/arabic/atlas.metrics"),
            }),
        },
    ]
}

/// A fresh typesetter for one scene. Each live scene owns its own, because
/// the solver that shapes with it needs it exclusively.
///
/// Assembled without the sheets: a solve only measures, and the set a staged
/// run samples comes from [`atlases`] instead — once, rather than once per
/// scene. A cascade carrying no atlas at all is the measure-only case
/// [`TextResources::from_faces`] admits, not the mixed one it refuses.
pub fn new_typesetter() -> Typesetter {
    TextResources::from_faces(faces(false))
        .unwrap_or_else(|error| panic!("the showcase cascade assembles: {error}"))
        .typesetter
}

/// The atlases the staged runs sample, in the cascade's font-slot order.
///
/// Shared behind an `Arc` because commit rebuilds the glyph-run table every
/// frame while the atlas set behind it never changes.
///
/// The conversion from a committed sheet's two files to a boundary-B [`Atlas`]
/// is `dashscene-engine`'s and no longer restated here (issue #962), which
/// also buys the checks a second copy never had: the PNG header against the
/// extent the metrics declare, and a glyph described by one quad and not the
/// other. Only glyphs that paint carry a quad, so an empty outline such as the
/// space is still dropped.
///
/// The typesetter assembled beside them is dropped. That is one parse of the
/// three faces, once per process, against carrying a second copy of the
/// conversion — and it is the same list of faces, so the atlas at each slot is
/// the one that slot's face shapes with.
pub fn atlases() -> Arc<Vec<Atlas>> {
    static ATLASES: LazyLock<Arc<Vec<Atlas>>> = LazyLock::new(|| {
        TextResources::from_faces(faces(true))
            // The face index the error names is an index into `faces` above,
            // which is where the fixture path for it is written.
            .unwrap_or_else(|error| panic!("the committed showcase atlases load: {error}"))
            .atlases
    });
    Arc::clone(&ATLASES)
}

/// A solver for one scene: a fresh typesetter, held, paired with the shared
/// atlas set.
///
/// **It retains Taffy's tree.** [`TaffySolver::owning`] is a `'static` solver,
/// so it goes into the `Box<dyn LayoutSolver>` a `LiveScene` keeps for its
/// whole life and patches that tree from each commit's dirty set rather than
/// rebuilding it. The showcase used to hold the typesetter in a wrapper of its
/// own and construct a `TaffySolver` inside every call, which threw the
/// retained tree away on every solve — `owning` did not exist when that was
/// written, and the alternative then was leaking a typesetter per scene
/// rebuild (issue #950, story #863).
///
/// A retained tree is only correct while **one** solver sees every commit into
/// the arena, in order: a commit consumes the arena's layout-dirty set, so a
/// second solver committing geometry takes a dirty set this one never sees and
/// this one then patches a tree that no longer describes the scene.
///
/// Each scene therefore builds one of these for `build_live` and commits **no
/// geometry** through any other. It is not the stronger rule that nothing else
/// commits at all: `layout::paint` and `surfaces::paint` each call this for a
/// throwaway solver of their own, because the nodes they address do not exist
/// until `build_live` has written them, so their staging cannot join that
/// commit. What keeps those two safe is that they stage paint intent and arena
/// metadata only — a dirty set they consume is empty of layout, so this solver
/// misses nothing. A third such pass has to satisfy the same condition, and
/// `crate::vocabulary` states it where such a pass would be written.
///
/// See `docs/decisions/one-solver-per-live-scene.md`.
pub fn solver() -> TaffySolver<'static> {
    TaffySolver::owning(TextResources::new(new_typesetter(), atlases()))
}

/// The corpus photograph, as the payload an image fill references.
///
/// A 512x512 CC0 payload committed for the asset pipeline's band measurements
/// (`corpus/photo/`), reused here rather than a new picture being added to the
/// tree.
pub fn photo() -> ImageAsset {
    static PAYLOAD: LazyLock<Arc<Vec<u8>>> =
        LazyLock::new(|| Arc::new(corpus_bytes!("corpus/photo/dawn-mountains.png")));
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
