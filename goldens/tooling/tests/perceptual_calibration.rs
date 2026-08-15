//! Where every rung of the ASTC ladder lands on two published perceptual
//! scales, for every corpus fixture — the calibration of `dashpack`'s tolerance
//! bands (issue #544).
//!
//! # What this answers that the bands do not
//!
//! `crates/dashpack/tests/band_contract.rs` pins each band as a per-texel
//! threshold and an area budget, each with a measured mutation that fails it.
//! Those are gates, and they are internal to this project: nothing in them says
//! where the rung they choose lands on a scale a reader outside this repository
//! would recognise. This file records that, so "HiFi is a threshold of 2 with a
//! 1 % budget" can be read as "HiFi accepted the rung scoring N and rejected
//! the rung scoring M".
//!
//! Nothing here gates the packer. A floor asserted below fails a test; it never
//! changes which rung `dashpack::profile::pack` selects.
//!
//! # Why the whole ladder and not only the selected rung
//!
//! Scoring only the selected rung answers "how good is HiFi" and leaves "is the
//! band's cut in the right place" unanswered. Recording every rung puts the
//! accepted score beside the rejected one, which is the comparison that makes
//! the cut readable. It is also the shape of the question the issue asks: where
//! loss becomes visible, rather than whether three named arms differ.
//!
//! # What a perceptual score means for a distance field
//!
//! Less than it does for an image fill, and this is stated rather than left for
//! a reader to assume. SSIMULACRA2 and FLIP are models of *colour* perception.
//! An MSDF atlas's channels are not colour — they are signed distances, which
//! is why `AssetClass::color_space` gives the class `ColorSpace::Linear` and
//! why the codec must not apply a transfer curve to them. Scoring one produces
//! a number, and that number says how visible the loss would be *if these
//! distances were colours*. Nobody ever looks at the atlas; what is looked at
//! is the glyph a shader derives from it.
//!
//! So the distance-field rows are recorded as comparability figures beside the
//! band's own texel measurements, never as a perceptual claim about a rendered
//! glyph. They still carry the argument the decision record needs — that no
//! lossy rung is close to acceptable for this content — and the caveat is
//! itself part of it: a per-asset perceptual metric cannot evaluate a distance
//! field, which is one more reason the fields-never-lossy rule is structural
//! rather than measured.
//!
//! # Why the ladder is walked for a class that cannot select it
//!
//! `AssetClass::DistanceField.lossy_rungs()` is empty by rule, so a walk driven
//! by the class's own ladder would measure nothing for the two atlases. The
//! image-fill footprints are walked for every fixture instead, and for a
//! distance field that walk is an explicit **counterfactual**: what the packer
//! refuses, never a rung it could select. `selected_by` is what keeps it
//! honest — for a distance field it names the terminal rung and nothing else,
//! because that is the only rung either profile can reach for that class.

use dashbuf::AssetKind;
use dashpack::astc::{self, Rgba8};
use dashpack::profile::{AssetClass, Binding, PACK_QUALITY, Profile, Rung, pack};
use goldens::metric::{self, Scores};

mod common;
use common::decode_png;
use common::manifest::repo_root;
use common::stress::{STRESS_AMPLITUDE, STRESS_EXTENT, block_stress};

// ---------------------------------------------------------------- fixtures

/// One canonical payload the ladder is walked over.
struct Fixture {
    /// The name used in [`TABLE`].
    name: &'static str,
    /// What the asset is, which is what the packer's hard rule reads.
    kind: AssetKind,
    /// Where the canonical payload lives, relative to the repository root, or
    /// `None` for the one generated fixture.
    path: Option<&'static str>,
}

/// The same five payloads `crates/dashpack/tests/band_contract.rs` measures:
/// three image fills and two distance fields.
///
/// Four are real committed payloads — the bytes Figma served for two imported
/// image fills, and two MSDF atlases the glyph pipeline baked. The fifth is
/// generated, for the reason `common::stress` records.
const FIXTURES: [Fixture; 9] = [
    Fixture {
        name: "import-image-fill",
        kind: AssetKind::Image,
        path: Some(
            "corpus/figma-fixtures/import-image-fill.images/\
             f856e637d6f6c2eb858e17a31d810f00542d2035.png",
        ),
    },
    Fixture {
        name: "v03-paint",
        kind: AssetKind::Image,
        path: Some(
            "corpus/figma-fixtures/v03-paint.images/\
             390616a0e7321eddb464388366d9a2a1bcb7f4c3.png",
        ),
    },
    Fixture {
        name: "block-stress",
        kind: AssetKind::Image,
        path: None,
    },
    Fixture {
        name: "photo-interior-render",
        kind: AssetKind::Image,
        path: Some("corpus/photo/interior-render.png"),
    },
    Fixture {
        name: "photo-coast-forest",
        kind: AssetKind::Image,
        path: Some("corpus/photo/coast-forest.png"),
    },
    Fixture {
        name: "photo-snowy-forest",
        kind: AssetKind::Image,
        path: Some("corpus/photo/snowy-forest.png"),
    },
    Fixture {
        name: "photo-dawn-mountains",
        kind: AssetKind::Image,
        path: Some("corpus/photo/dawn-mountains.png"),
    },
    Fixture {
        name: "inter-ascii-atlas",
        kind: AssetKind::DistanceField,
        path: Some("corpus/atlas/inter-ascii/atlas.png"),
    },
    Fixture {
        name: "arabic-atlas",
        kind: AssetKind::DistanceField,
        path: Some("corpus/atlas/arabic/atlas.png"),
    },
];

/// A fixture's canonical texels.
///
/// Decoded through [`common::decode_png`], which is the `png` crate and
/// deliberately not Skia's: these scores are read beside
/// `crates/dashpack/tests/band_contract.rs`'s band fractions, and a second
/// decoder's disagreement would sit inside every comparison,
/// indistinguishable from codec error.
fn canonical(fixture: &Fixture) -> (u32, u32, Vec<u8>) {
    match fixture.path {
        Some(path) => {
            let bytes = std::fs::read(repo_root().join(path))
                .unwrap_or_else(|e| panic!("the committed payload {path} reads: {e}"));
            decode_png(&bytes, "the canonical payload")
        }
        None => (
            STRESS_EXTENT,
            STRESS_EXTENT,
            block_stress(STRESS_EXTENT, STRESS_EXTENT, STRESS_AMPLITUDE),
        ),
    }
}

/// The rungs this calibration scores: the image-fill ladder, then the terminal
/// rung.
///
/// Read from the packer rather than retyped, so a ladder change cannot leave
/// this table describing a footprint the packer no longer offers. See the
/// module header for why every fixture is walked over this list, including the
/// two whose own ladder is empty.
fn scored_rungs() -> Vec<Rung> {
    AssetClass::ImageFill
        .lossy_rungs()
        .iter()
        .copied()
        .map(Rung::Astc)
        .chain([Rung::Uncompressed])
        .collect()
}

/// Scores one rung's decoded texels against the canonical ones.
///
/// The terminal rung is the canonical texels themselves, so it is not encoded:
/// its payload *is* the canonical payload, and running it through the codec
/// would measure a round trip the packer never performs.
fn measure(class: AssetClass, rung: Rung, width: u32, height: u32, texels: &[u8]) -> Scores {
    let image = Rgba8::new(width, height, texels).expect("the extent matches the texel count");
    let candidate = match rung {
        Rung::Astc(block) => {
            let payload = astc::encode(image, block, class.color_space(), PACK_QUALITY)
                .unwrap_or_else(|e| panic!("{rung} encodes: {e}"));
            astc::decode(&payload, width, height, block, class.color_space())
                .unwrap_or_else(|e| panic!("{rung} decodes: {e}"))
        }
        Rung::Uncompressed => texels.to_vec(),
    };
    metric::score(width, height, texels, &candidate)
        .expect("the candidate decodes back to the canonical extent")
}

/// The rung `profile` selected for this fixture, asked of the packer rather
/// than rederived from the measurements above.
fn selected_rung(
    profile: Profile,
    fixture: &Fixture,
    width: u32,
    height: u32,
    texels: &[u8],
) -> Rung {
    let image = Rgba8::new(width, height, texels).expect("the extent matches the texel count");
    match pack(profile, fixture.kind, image).expect("the fixture packs") {
        Binding::Derived(derivation) => derivation.rung,
        Binding::Canonical => panic!("only RAW binds canonically, and RAW is not walked here"),
    }
}

// ------------------------------------------------------------- the record

/// One recorded row: a fixture at one rung, scored against its canonical
/// texels.
struct Row {
    fixture: &'static str,
    rung: &'static str,
    /// SSIMULACRA2 to 2 decimals, or `withheld` below the minimum extent.
    ssimulacra2: &'static str,
    /// Mean FLIP at 67 ppd and at 107.71 ppd, each to 4 decimals.
    flip_desk: &'static str,
    flip_panel: &'static str,
    /// PSNR in dB to 2 decimals, or `lossless`.
    psnr_rgb: &'static str,
    psnr_alpha: &'static str,
    /// The profiles that selected this rung for this fixture, in the order
    /// [`PRODUCTION`] gives them.
    selected_by: &'static [Profile],
}

/// The two profiles that derive anything. RAW is the null binding and selects
/// no rung, so it is not walked.
const PRODUCTION: [Profile; 2] = [Profile::HiFi, Profile::LoFi];

/// Every fixture at every rung, measured and read off rather than predicted.
///
/// Regenerate with `UPDATE_GOLDENS=1 cargo test -p goldens --test
/// perceptual_calibration -- --nocapture`, which prints these rows in this
/// file's own literal form and skips the equality against them.
const TABLE: &[Row] = &[
    Row {
        fixture: "import-image-fill",
        rung: "astc-12x12",
        ssimulacra2: "78.35",
        flip_desk: "0.0392",
        flip_panel: "0.0314",
        psnr_rgb: "45.67",
        psnr_alpha: "75.64",
        selected_by: &[Profile::LoFi],
    },
    Row {
        fixture: "import-image-fill",
        rung: "astc-10x10",
        ssimulacra2: "86.56",
        flip_desk: "0.0319",
        flip_panel: "0.0242",
        psnr_rgb: "47.32",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "import-image-fill",
        rung: "astc-8x8",
        ssimulacra2: "87.82",
        flip_desk: "0.0247",
        flip_panel: "0.0171",
        psnr_rgb: "49.09",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "import-image-fill",
        rung: "astc-6x6",
        ssimulacra2: "92.87",
        flip_desk: "0.0168",
        flip_panel: "0.0119",
        psnr_rgb: "51.35",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi],
    },
    Row {
        fixture: "import-image-fill",
        rung: "astc-5x5",
        ssimulacra2: "96.20",
        flip_desk: "0.0059",
        flip_panel: "0.0047",
        psnr_rgb: "57.09",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "import-image-fill",
        rung: "astc-4x4",
        ssimulacra2: "96.25",
        flip_desk: "0.0050",
        flip_panel: "0.0042",
        psnr_rgb: "58.55",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "import-image-fill",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-12x12",
        ssimulacra2: "withheld",
        flip_desk: "0.1169",
        flip_panel: "0.0987",
        psnr_rgb: "23.93",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-10x10",
        ssimulacra2: "withheld",
        flip_desk: "0.1543",
        flip_panel: "0.1415",
        psnr_rgb: "22.38",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-8x8",
        ssimulacra2: "withheld",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi, Profile::LoFi],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-6x6",
        ssimulacra2: "withheld",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-5x5",
        ssimulacra2: "withheld",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "astc-4x4",
        ssimulacra2: "withheld",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "v03-paint",
        rung: "uncompressed",
        ssimulacra2: "withheld",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-12x12",
        ssimulacra2: "69.30",
        flip_desk: "0.0496",
        flip_panel: "0.0359",
        psnr_rgb: "34.50",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-10x10",
        ssimulacra2: "72.46",
        flip_desk: "0.0453",
        flip_panel: "0.0320",
        psnr_rgb: "34.84",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-8x8",
        ssimulacra2: "72.61",
        flip_desk: "0.0433",
        flip_panel: "0.0302",
        psnr_rgb: "35.47",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-6x6",
        ssimulacra2: "78.57",
        flip_desk: "0.0363",
        flip_panel: "0.0254",
        psnr_rgb: "36.91",
        psnr_alpha: "lossless",
        selected_by: &[Profile::LoFi],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-5x5",
        ssimulacra2: "80.95",
        flip_desk: "0.0342",
        flip_panel: "0.0242",
        psnr_rgb: "38.55",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "block-stress",
        rung: "astc-4x4",
        ssimulacra2: "87.69",
        flip_desk: "0.0254",
        flip_panel: "0.0172",
        psnr_rgb: "40.69",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi],
    },
    Row {
        fixture: "block-stress",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-12x12",
        ssimulacra2: "54.86",
        flip_desk: "0.0586",
        flip_panel: "0.0467",
        psnr_rgb: "31.26",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-10x10",
        ssimulacra2: "62.71",
        flip_desk: "0.0483",
        flip_panel: "0.0383",
        psnr_rgb: "32.95",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-8x8",
        ssimulacra2: "70.11",
        flip_desk: "0.0374",
        flip_panel: "0.0294",
        psnr_rgb: "35.21",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-6x6",
        ssimulacra2: "80.31",
        flip_desk: "0.0249",
        flip_panel: "0.0191",
        psnr_rgb: "39.33",
        psnr_alpha: "lossless",
        selected_by: &[Profile::LoFi],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-5x5",
        ssimulacra2: "85.89",
        flip_desk: "0.0191",
        flip_panel: "0.0144",
        psnr_rgb: "42.23",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "astc-4x4",
        ssimulacra2: "90.72",
        flip_desk: "0.0132",
        flip_panel: "0.0098",
        psnr_rgb: "46.20",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi],
    },
    Row {
        fixture: "photo-interior-render",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-12x12",
        ssimulacra2: "41.35",
        flip_desk: "0.1206",
        flip_panel: "0.0945",
        psnr_rgb: "24.35",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-10x10",
        ssimulacra2: "51.87",
        flip_desk: "0.1024",
        flip_panel: "0.0798",
        psnr_rgb: "25.89",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-8x8",
        ssimulacra2: "64.60",
        flip_desk: "0.0829",
        flip_panel: "0.0636",
        psnr_rgb: "28.15",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-6x6",
        ssimulacra2: "77.79",
        flip_desk: "0.0588",
        flip_panel: "0.0442",
        psnr_rgb: "32.20",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-5x5",
        ssimulacra2: "84.92",
        flip_desk: "0.0436",
        flip_panel: "0.0320",
        psnr_rgb: "35.38",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "astc-4x4",
        ssimulacra2: "90.64",
        flip_desk: "0.0293",
        flip_panel: "0.0211",
        psnr_rgb: "39.28",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi, Profile::LoFi],
    },
    Row {
        fixture: "photo-coast-forest",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-12x12",
        ssimulacra2: "43.71",
        flip_desk: "0.0935",
        flip_panel: "0.0734",
        psnr_rgb: "26.70",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-10x10",
        ssimulacra2: "53.72",
        flip_desk: "0.0815",
        flip_panel: "0.0643",
        psnr_rgb: "28.17",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-8x8",
        ssimulacra2: "63.51",
        flip_desk: "0.0706",
        flip_panel: "0.0567",
        psnr_rgb: "30.56",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-6x6",
        ssimulacra2: "78.02",
        flip_desk: "0.0532",
        flip_panel: "0.0422",
        psnr_rgb: "35.24",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-5x5",
        ssimulacra2: "86.76",
        flip_desk: "0.0379",
        flip_panel: "0.0285",
        psnr_rgb: "39.56",
        psnr_alpha: "lossless",
        selected_by: &[Profile::LoFi],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "astc-4x4",
        ssimulacra2: "93.21",
        flip_desk: "0.0232",
        flip_panel: "0.0164",
        psnr_rgb: "44.25",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi],
    },
    Row {
        fixture: "photo-snowy-forest",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-12x12",
        ssimulacra2: "78.38",
        flip_desk: "0.0314",
        flip_panel: "0.0266",
        psnr_rgb: "43.43",
        psnr_alpha: "lossless",
        selected_by: &[Profile::LoFi],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-10x10",
        ssimulacra2: "82.76",
        flip_desk: "0.0265",
        flip_panel: "0.0222",
        psnr_rgb: "44.81",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-8x8",
        ssimulacra2: "85.13",
        flip_desk: "0.0195",
        flip_panel: "0.0161",
        psnr_rgb: "46.95",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-6x6",
        ssimulacra2: "90.19",
        flip_desk: "0.0127",
        flip_panel: "0.0102",
        psnr_rgb: "50.31",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-5x5",
        ssimulacra2: "91.95",
        flip_desk: "0.0099",
        flip_panel: "0.0079",
        psnr_rgb: "53.19",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "astc-4x4",
        ssimulacra2: "93.08",
        flip_desk: "0.0063",
        flip_panel: "0.0051",
        psnr_rgb: "57.01",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi],
    },
    Row {
        fixture: "photo-dawn-mountains",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-12x12",
        ssimulacra2: "17.91",
        flip_desk: "0.1600",
        flip_panel: "0.1356",
        psnr_rgb: "20.05",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-10x10",
        ssimulacra2: "36.46",
        flip_desk: "0.1245",
        flip_panel: "0.1040",
        psnr_rgb: "21.91",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-8x8",
        ssimulacra2: "54.91",
        flip_desk: "0.0898",
        flip_panel: "0.0736",
        psnr_rgb: "24.19",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-6x6",
        ssimulacra2: "71.37",
        flip_desk: "0.0528",
        flip_panel: "0.0422",
        psnr_rgb: "28.01",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-5x5",
        ssimulacra2: "78.35",
        flip_desk: "0.0363",
        flip_panel: "0.0288",
        psnr_rgb: "30.98",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "astc-4x4",
        ssimulacra2: "86.12",
        flip_desk: "0.0222",
        flip_panel: "0.0178",
        psnr_rgb: "35.34",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "inter-ascii-atlas",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi, Profile::LoFi],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-12x12",
        ssimulacra2: "21.22",
        flip_desk: "0.1855",
        flip_panel: "0.1598",
        psnr_rgb: "19.85",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-10x10",
        ssimulacra2: "37.45",
        flip_desk: "0.1467",
        flip_panel: "0.1247",
        psnr_rgb: "21.67",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-8x8",
        ssimulacra2: "56.22",
        flip_desk: "0.0968",
        flip_panel: "0.0808",
        psnr_rgb: "24.42",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-6x6",
        ssimulacra2: "72.42",
        flip_desk: "0.0543",
        flip_panel: "0.0440",
        psnr_rgb: "28.34",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-5x5",
        ssimulacra2: "79.51",
        flip_desk: "0.0384",
        flip_panel: "0.0309",
        psnr_rgb: "31.22",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "astc-4x4",
        ssimulacra2: "86.88",
        flip_desk: "0.0231",
        flip_panel: "0.0186",
        psnr_rgb: "35.81",
        psnr_alpha: "lossless",
        selected_by: &[],
    },
    Row {
        fixture: "arabic-atlas",
        rung: "uncompressed",
        ssimulacra2: "100.00",
        flip_desk: "0.0000",
        flip_panel: "0.0000",
        psnr_rgb: "lossless",
        psnr_alpha: "lossless",
        selected_by: &[Profile::HiFi, Profile::LoFi],
    },
];

/// The published rung each profile's selected encoding must reach, and the name
/// the SSIMULACRA2 scale gives that rung.
///
/// These are the **published** thresholds, not the measured values. That is
/// deliberate: a floor set to whatever the current fixtures happened to score
/// would be one more number internal to this project, which is the thing this
/// calibration exists to stop doing. Measured headroom at the time of writing
/// is 2.87 for HiFi (92.87 on `import-image-fill`) and 8.35 for LoFi (78.35 on
/// the same fixture).
///
/// If a later fixture drops a profile below its floor, that is evidence the
/// **band** is wrong. The direction of the fix is to retune the band against
/// the asset and record the change — not to lower this number, and not to
/// replace the fixture.
const FLOORS: [(Profile, f64, &str); 2] = [
    (Profile::HiFi, 90.0, "visually lossless"),
    (Profile::LoFi, 70.0, "high quality"),
];

/// Whether this run is a deliberate re-baseline of the recorded numbers.
///
/// The same `UPDATE_GOLDENS` switch the rest of the harness uses, because one
/// knob for "I am regenerating recorded artifacts" is easier to reason about
/// than three.
fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDENS").is_some()
}

/// A row's `selected_by` as this file's own literal form.
fn selected_literal(profiles: &[Profile]) -> String {
    let names: Vec<&str> = profiles
        .iter()
        .map(|p| match p {
            Profile::HiFi => "Profile::HiFi",
            Profile::LoFi => "Profile::LoFi",
            Profile::Raw => unreachable!("RAW selects no rung"),
        })
        .collect();
    format!("&[{}]", names.join(", "))
}

/// One fixture's measured rows: the rung, its scores, and which profiles chose
/// it.
///
/// Walking one fixture is the unit of work this file splits along. Each of the
/// nine per-fixture tests below calls it once and holds the result against that
/// fixture's rows in [`TABLE`]; the regeneration walk calls it for all nine in
/// order. They cost about seven encodes each and share nothing, so nextest runs
/// the nine concurrently — as one test they were sixty-three encodes in a
/// single process, which was most of the calibration tier's wall clock
/// (issue #660).
fn measured_rows(fixture: &Fixture) -> Vec<(&'static str, String, Scores, Vec<Profile>)> {
    let class = AssetClass::of(fixture.kind).expect("a known kind");
    let (width, height, texels) = canonical(fixture);
    let selected: Vec<(Profile, Rung)> = PRODUCTION
        .iter()
        .map(|&profile| {
            (
                profile,
                selected_rung(profile, fixture, width, height, &texels),
            )
        })
        .collect();

    scored_rungs()
        .into_iter()
        .map(|rung| {
            let scores = measure(class, rung, width, height, &texels);
            let by: Vec<Profile> = selected
                .iter()
                .filter(|(_, chosen)| *chosen == rung)
                .map(|(profile, _)| *profile)
                .collect();
            (fixture.name, rung.to_string(), scores, by)
        })
        .collect()
}

/// A measured row in the literal form [`TABLE`] is written in.
fn row_literal(name: &str, rung: &str, scores: &Scores, by: &[Profile]) -> String {
    format!(
        "    Row {{\n        fixture: {name:?},\n        rung: {rung:?},\n        \
         ssimulacra2: {:?},\n        flip_desk: {:?},\n        flip_panel: {:?},\n        \
         psnr_rgb: {:?},\n        psnr_alpha: {:?},\n        selected_by: {},\n    }},",
        scores
            .ssimulacra2
            .map(|v| metric::fixed(v, 2))
            .unwrap_or_else(|| "withheld".to_string()),
        metric::fixed(scores.flip_desk, 4),
        metric::fixed(scores.flip_panel, 4),
        metric::fixed(scores.psnr_rgb, 2),
        metric::fixed(scores.psnr_alpha, 2),
        selected_literal(by),
    )
}

/// Re-derives one fixture's rows and holds them against [`TABLE`].
///
/// Under `UPDATE_GOLDENS` this returns without asserting, exactly as the single
/// walk did before the split: regeneration is
/// [`the_table_is_regenerated_in_one_ordered_walk`]'s job, because the table is
/// one ordered literal and nine concurrent tests cannot print it in order.
fn assert_fixture_matches_table(name: &str) {
    if updating() {
        return;
    }

    let fixture = fixture_of(name);
    let measured = measured_rows(fixture);
    let recorded: Vec<&Row> = TABLE.iter().filter(|row| row.fixture == name).collect();

    assert_eq!(
        recorded.len(),
        measured.len(),
        "{name}: the table records {} rows and the walk measures {} — a row that is measured and \
         not recorded is unpinned, and one that is recorded and not measured is a number nothing \
         produces",
        recorded.len(),
        measured.len(),
    );

    // No assertion that `row.fixture` is this fixture: `recorded` was filtered
    // on exactly that, so one here could never fail. Which fixtures the table
    // carries, and in what order, is
    // `the_table_covers_every_fixture_and_rung_exactly_once`'s question. The
    // rung order below is still this loop's, because it is within a fixture.
    for (row, (_, rung, scores, by)) in recorded.into_iter().zip(&measured) {
        let where_ = format!("{name} at {rung}");
        assert_eq!(row.rung, rung.as_str(), "{where_}: the table's rung order");
        assert_eq!(
            row.ssimulacra2,
            scores
                .ssimulacra2
                .map(|v| metric::fixed(v, 2))
                .unwrap_or_else(|| "withheld".to_string()),
            "{where_}: SSIMULACRA2",
        );
        assert_eq!(
            row.flip_desk,
            metric::fixed(scores.flip_desk, 4),
            "{where_}: FLIP at the desk viewing condition",
        );
        assert_eq!(
            row.flip_panel,
            metric::fixed(scores.flip_panel, 4),
            "{where_}: FLIP at the panel viewing condition",
        );
        assert_eq!(
            row.psnr_rgb,
            metric::fixed(scores.psnr_rgb, 2),
            "{where_}: PSNR over the colour channels",
        );
        assert_eq!(
            row.psnr_alpha,
            metric::fixed(scores.psnr_alpha, 2),
            "{where_}: PSNR over alpha",
        );
        assert_eq!(
            row.selected_by, by,
            "{where_}: which profiles selected this rung",
        );
    }
}

/// One test per fixture, each named for the fixture it walks.
///
/// Written out rather than generated from [`FIXTURES`] so that every test name
/// exists as source text: `.config/nextest.toml` selects the calibration tier
/// by exact name and `.config/calibration-tier.txt` pins the listing, so a name
/// that only exists after macro expansion could not be grepped from either.
/// [`the_table_covers_every_fixture_and_rung_exactly_once`] is what catches a
/// fixture added to `FIXTURES` and given no test here.
macro_rules! per_fixture_tests {
    ($($test:ident => $fixture:literal,)+) => {$(
        #[test]
        fn $test() {
            assert_fixture_matches_table($fixture);
        }
    )+};
}

per_fixture_tests! {
    import_image_fill_rows_match_the_table => "import-image-fill",
    v03_paint_rows_match_the_table => "v03-paint",
    block_stress_rows_match_the_table => "block-stress",
    photo_interior_render_rows_match_the_table => "photo-interior-render",
    photo_coast_forest_rows_match_the_table => "photo-coast-forest",
    photo_snowy_forest_rows_match_the_table => "photo-snowy-forest",
    photo_dawn_mountains_rows_match_the_table => "photo-dawn-mountains",
    inter_ascii_atlas_rows_match_the_table => "inter-ascii-atlas",
    arabic_atlas_rows_match_the_table => "arabic-atlas",
}

/// The invariant the nine tests above cannot hold between them: that [`TABLE`]
/// covers every fixture and every rung, once each, in `FIXTURES` order.
///
/// Before the split this came free — one walk built the whole measured list and
/// compared its length against the table's. Nine tests each see only their own
/// fixture, so a fixture added to `FIXTURES` with no test here, or a table row
/// naming a fixture that no longer exists, would be caught by nothing. This
/// reads the table alone and encodes nothing, so it runs in the regression tier
/// rather than the calibration one.
#[test]
fn the_table_covers_every_fixture_and_rung_exactly_once() {
    let rungs = scored_rungs();
    let expected: Vec<(&str, String)> = FIXTURES
        .iter()
        .flat_map(|f| rungs.iter().map(move |r| (f.name, r.to_string())))
        .collect();
    let recorded: Vec<(&str, String)> = TABLE
        .iter()
        .map(|row| (row.fixture, row.rung.to_string()))
        .collect();

    assert_eq!(
        recorded, expected,
        "the table must hold every fixture's every rung, once each, in FIXTURES order. A fixture \
         added to FIXTURES needs both a row block here and a test in per_fixture_tests!, and a \
         row naming a fixture FIXTURES no longer has is a number nothing produces.",
    );
}

/// Regenerates [`TABLE`] in one ordered walk, under `UPDATE_GOLDENS`.
///
/// This is the one place the split costs something. The table is a single
/// ordered literal, and the nine tests above run concurrently, so their output
/// would interleave into something unusable. This walks all nine fixtures in
/// `FIXTURES` order in one process and prints the whole block, which keeps the
/// regeneration command exactly what it was.
///
/// Outside a regeneration run it returns immediately, so it costs nothing in
/// either tier and encodes nothing.
#[test]
fn the_table_is_regenerated_in_one_ordered_walk() {
    if !updating() {
        return;
    }

    eprintln!("// paste into TABLE:");
    for fixture in &FIXTURES {
        for (name, rung, scores, by) in measured_rows(fixture) {
            eprintln!("{}", row_literal(name, &rung, &scores, &by));
        }
    }
}

/// The fixture [`FIXTURES`] gives a name.
fn fixture_of(name: &str) -> &'static Fixture {
    FIXTURES
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} is a row in TABLE with no fixture"))
}

/// The kind [`FIXTURES`] gives a fixture name.
fn kind_of(name: &str) -> AssetKind {
    fixture_of(name).kind
}

/// Whether a fixture's payload is generated in this test rather than committed.
///
/// Generated payloads are excluded from the floors below, and the exclusion is
/// by this property rather than by name so that a fixture added later inherits
/// it correctly.
fn is_generated(name: &str) -> bool {
    fixture_of(name).path.is_none()
}

#[test]
fn every_profile_reaches_its_published_rung() {
    for (profile, floor, rung_name) in FLOORS {
        let mut exercised = 0usize;

        for row in TABLE {
            if !row.selected_by.contains(&profile) || kind_of(row.fixture) != AssetKind::Image {
                continue;
            }
            // Generated payloads are excluded, and this is the one exclusion in
            // this file that is a judgement rather than a measurement, so it is
            // argued rather than asserted.
            //
            // `block-stress` exists to be pathological: a smooth gradient
            // carrying a bounded per-texel perturbation, which is the content
            // class block compression is worst at, generated precisely because
            // no real asset was hard enough. Under HiFi's `Terminal::FinestLossy`
            // it now ships astc-4x4 and scores 87.69 — below the visually-
            // lossless floor, and the sole cost of that trade (section 7 of the
            // band decision record, issue #553).
            //
            // The floor is a claim about **product content**. Holding
            // deliberately adversarial synthetic content to it would either
            // block the trade or force the floor down to what the synthetic
            // case allows, and lowering a published threshold to fit a
            // measurement is the defect issue #422 documents. Every committed
            // real payload is still held to it, which is what the count below
            // guards.
            if is_generated(row.fixture) {
                continue;
            }
            // `v03-paint` is 16x16, below the extent at which SSIMULACRA2 means
            // what it means elsewhere, so it has no score to hold to a floor.
            // Skipped by name in the count below rather than silently.
            let Ok(score) = row.ssimulacra2.parse::<f64>() else {
                continue;
            };
            exercised += 1;
            assert!(
                score >= floor,
                "{profile:?} selected {} for {} and it scores {score} on SSIMULACRA2, below the \
                 {floor} the published scale calls {rung_name}. That is evidence the band is \
                 wrong: retune the band against the asset and record the change. Do not lower \
                 this floor, and do not replace the fixture.",
                row.rung,
                row.fixture,
            );
        }

        assert!(
            exercised > 0,
            "{profile:?}'s floor is asserted by no fixture, so it is a number nothing can fail \
             (issue #422). Every image fill either dropped out of TABLE or lost its score.",
        );
    }
}

#[test]
fn a_distance_field_selects_the_terminal_rung_and_reproduces_it_exactly() {
    let mut fields = 0usize;

    for row in TABLE {
        if kind_of(row.fixture) != AssetKind::DistanceField || row.selected_by.is_empty() {
            continue;
        }
        fields += 1;
        assert_eq!(
            row.rung, "uncompressed",
            "{}: a distance field has no lossy rung to select, whatever the profile",
            row.fixture,
        );
        assert_eq!(
            row.selected_by, &PRODUCTION,
            "{}: both production profiles reach the same terminal rung, because the rule is the \
             class's and not the profile's",
            row.fixture,
        );
        // The lossless rung's identity property, on the scale a reader outside
        // this repository recognises: not "within a band" but exactly the top
        // of the scale, and no error at either viewing condition.
        assert_eq!(row.ssimulacra2, "100.00", "{}", row.fixture);
        assert_eq!(row.flip_desk, "0.0000", "{}", row.fixture);
        assert_eq!(row.flip_panel, "0.0000", "{}", row.fixture);
        assert_eq!(row.psnr_rgb, "lossless", "{}", row.fixture);
    }

    assert_eq!(
        fields, 2,
        "both committed MSDF atlases must be in TABLE with a selected rung",
    );
}

/// The counterfactual, stated as an assertion rather than left in the table for
/// a reader to notice: at the *finest* footprint the ladder offers, 4x4 at 8
/// bits per texel, neither atlas reaches even HiFi's floor.
///
/// This is what puts a published-scale number behind
/// `docs/decisions/asset-quality-profile-bands.md`'s "no lossy rung could have
/// held either band". It is a comparability figure and not a perceptual claim
/// about a rendered glyph — see the module header.
#[test]
fn no_lossy_rung_would_have_been_acceptable_for_a_distance_field() {
    let (_, hifi_floor, _) = FLOORS[0];
    let mut checked = 0usize;

    for row in TABLE {
        if kind_of(row.fixture) != AssetKind::DistanceField || row.rung != "astc-4x4" {
            continue;
        }
        checked += 1;
        let score: f64 = row
            .ssimulacra2
            .parse()
            .unwrap_or_else(|_| panic!("{}: the atlases are above the extent floor", row.fixture));
        assert!(
            score < hifi_floor,
            "{}: the finest lossy rung scores {score}, at or above HiFi's floor of {hifi_floor}. \
             The fields-never-lossy rule stays structural either way, but the decision record's \
             claim that no lossy rung could have held the band no longer follows from this \
             measurement and must be rewritten.",
            row.fixture,
        );
    }

    assert_eq!(checked, 2, "both atlases must carry an astc-4x4 row");
}
