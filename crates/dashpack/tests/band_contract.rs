//! The measured band contracts — every number in `dashpack::profile`, the
//! escalation each one produces, and the mutation that fails each band.
//!
//! # What this file is for
//!
//! A tolerance band is only a gate if something can fail it. Issue #422
//! measured that the render oracle's `blur-falloff` band catches **none** of
//! the six defects the frames it governs exist to catch, because a 12 % area
//! budget cannot be exceeded by a bounded-area defect. A budget chosen in
//! advance and never exercised is not a gate.
//!
//! So every band `dashpack::profile` pins ships here with the mutation that
//! fails it, measured (`MUTATIONS`), and the whole contract table is recorded
//! rather than described (`TABLE`). Nothing below is predicted: the numbers
//! were produced by running this code and reading them off, which is also why
//! `docs/technotes/2026-07-26-tolerance-band-coverage.md`'s rule applies —
//! classify from the measured residual, never from expectation.
//!
//! # Why more than one asset per class
//!
//! Debt #395 was a silent paint-entry collapse that survived because the
//! fixture that should have caught it had exactly one stacked node, so every
//! index in it was 0. `goldens/dsb/v03-paint.dsb` has the same shape for
//! assets: one image, so no dedup, ordering or wrong-index bug could show. This
//! file carries three image fills and two distance fields, and
//! `more_than_one_asset_per_class_and_one_that_escalates` holds it to that.
//!
//! # Why a digest and not only a length (issue #458)
//!
//! Story #434 recorded each chosen rung's KTX2 file *length* and measured what
//! that misses: a change that rewrites the file without resizing it survives.
//! It also measured that the byte-exact golden `goldens/dsb/v03-paint-hifi.dsb`
//! survives an encoder-effort regression, because the one asset that golden
//! carries encodes identically at every effort. Issue #458 proposed closing
//! that with a second byte-exact golden over `import-image-fill`, whose HiFi
//! payload is about 21 KB.
//!
//! `TABLE` records a BLAKE3 digest per derived row instead. A digest is
//! byte-exactness — the same property a committed payload pins — over ten
//! derived files spanning 249 to 196782 bytes, at 64 hex characters a row
//! rather than one 21 KB binary that has to be regenerated whenever the
//! encoder, the compressor or the writer string moves. What a committed
//! payload would add over a digest is the ability to inspect the difference,
//! and a Zstd-compressed block payload is not inspectable by diff.
//!
//! Measured on this branch, the digest is the assertion in this file that fails
//! when the `KTXorientation` value changes to another two-byte string: the file
//! keeps its length, every fraction holds, every rung holds, and the digests
//! move. Before #458 the whole file stayed green on that change.
//!
//! **What it still does not cover.** A digest says the bytes moved, never which
//! ones; the `bytes` column beside it separates a resize from a rewrite, and
//! nothing here narrows it further. It covers the packer's own KTX2 files and
//! not the assembled `.dsb` container around them — the section table, the
//! derivation manifest, the page alignment — which is
//! `goldens/tooling/tests/derived_bank.rs`'s golden, and that golden carries
//! one 249-byte asset. No check in the repository pins assembled-container
//! bytes over a multi-block asset.
//!
//! **No mutation is caught by the digest alone**, and that is stated rather
//! than left to be assumed. Six were run for #458, and each one that reaches
//! this file also moves a rung, a fraction or a length. The one class that
//! would need the digest on its own — a multi-block ASTC payload whose blocks
//! change while the Zstd-compressed length and the 4-decimal differing fraction
//! both hold — is reachable by revendoring astcenc, not by editing our own
//! source, so it could not be synthesised. Two of the six are recorded: a
//! `tune_block_mode_limit` change in the vendored medium-bandwidth `THOROUGH`
//! preset moved the `import-image-fill` accepted fraction from 0.2133 % to
//! 0.2181 %, and the same change left the assembled golden green because
//! `v03-paint` ships at 8x8, out of that preset table's range. The digest is
//! defence in depth over the columns beside it, not a new class of coverage.
//!
//! **It is also the first check that can fail the invariance claim.**
//! `crates/dashpack-astcenc-sys/build.rs` argues that an invariant astcenc
//! build emits bit-identical output on any CPU and any compiler, and nothing
//! measured it. A length can agree across two architectures while the bytes
//! differ; a digest cannot. If a run on another architecture disagrees with a
//! digest here, that is the invariance claim failing and it must be
//! investigated, not re-recorded.
//!
//! # Where these numbers sit on a scale outside this repository (issue #544)
//!
//! Every band below is a per-texel threshold and an area budget, and both are
//! this project's own units. Nothing here says whether the rung a band chooses
//! is *good*. `goldens/tooling/tests/perceptual_calibration.rs` records that:
//! it walks the same ladder over the same nine fixtures and scores every rung
//! on SSIMULACRA2 and FLIP, so the rung a band accepted can be read beside the
//! rung it rejected.
//!
//! It walks the whole ladder rather than only the selected rung, because the
//! selected rung alone cannot say whether the cut is in the right place. It
//! lives in `goldens` rather than here so that one metric implementation serves
//! both the per-asset ladder and the scene arms of the profile-preview oracle;
//! nothing in that file gates this one, and no number below depends on it.

use dashbuf::AssetKind;
use dashpack::astc::{BlockSize, ColorSpace, Quality, Rgba8, decode, encode};
use dashpack::band::{ToleranceBand, diff};
use dashpack::profile::{
    self, AssetClass, BANDS, Binding, Contract, Derivation, HIFI_IMAGE_FILL, LOFI_IMAGE_FILL,
    PACK_QUALITY, Profile, Rung, contract, pack,
};

// ---------------------------------------------------------------- fixtures

/// One canonical payload the contracts are measured on.
struct Fixture {
    /// The name used in `TABLE` and `MUTATIONS`.
    name: &'static str,
    /// What the asset is, which is what the hard rule reads.
    kind: AssetKind,
    /// Where the canonical payload lives, relative to the repository root, or
    /// `None` for the one generated fixture.
    path: Option<&'static str>,
}

/// Three image fills and two distance fields.
///
/// Four are real committed payloads — the bytes Figma served for two imported
/// image fills, and two MSDF atlases the glyph pipeline baked. The fifth,
/// `detail-noise`, is generated by [`detail_noise`] rather than committed,
/// because when it was written no asset in the tree separated the two profiles'
/// *area budgets*: the two Figma image fills are a gradient and flat
/// rectangles, which ASTC reproduces almost exactly at every footprint. It
/// exists to make the `lofi-image-fill` budget the binding term, which is the
/// property #422 found the render bands lacked. It is deterministic — an
/// integer hash, no floating point, no randomness — so it is as reproducible as
/// a committed file.
///
/// The four `photo-*` payloads close that gap with real content (issue #455,
/// `corpus/photo/README.md`): a photorealistic 3D interior render and three
/// landscape photographs, the content class
/// `docs/wip/2026-07-28-photorealistic-3d-content.md` records as target. They
/// are the first fixtures on which `lofi-image-fill`'s budget binds without a
/// generator, and the first to reach the ladder's 4x4 and 5x5 rungs.
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
        name: "detail-noise",
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

/// The extent of the generated fixture, and the amplitude of its detail.
///
/// 8 rather than a larger number on purpose: it puts the encoder's error just
/// either side of LoFi's 5 % budget across the ladder, which is what makes the
/// budget rather than the threshold the term that chooses the rung. A larger
/// amplitude fails every rung and would prove nothing about the number.
const NOISE_EXTENT: u32 = 256;
const NOISE_AMPLITUDE: i32 = 8;

/// A deterministic integer hash. No floating point and no randomness, so the
/// generated fixture is identical on every machine and every build profile.
fn splitmix(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

/// A smooth three-channel gradient with a bounded per-texel perturbation — a
/// low-amplitude, high-spatial-frequency image, which is the content class
/// block compression is worst at and the one a real photograph would supply if
/// the corpus held one.
fn detail_noise(width: u32, height: u32, amplitude: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let base = [
                (x * 255 / width) as u8,
                (y * 255 / height) as u8,
                ((x + y) * 255 / (width + height)) as u8,
            ];
            for (channel, value) in base.iter().enumerate() {
                let span = 2 * amplitude as u32 + 1;
                let offset = (splitmix(x, y, channel as u32) % span) as i32 - amplitude;
                out.push((i32::from(*value) + offset).clamp(0, 255) as u8);
            }
            out.push(255);
        }
    }
    out
}

/// Decodes a committed PNG to 8-bit RGBA, expanding RGB sources to opaque RGBA.
fn load_png(path: &str) -> (u32, u32, Vec<u8>) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let bytes = std::fs::read(format!("{root}/{path}"))
        .unwrap_or_else(|error| panic!("the committed fixture {path} opens: {error}"));
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .expect("the fixture has a readable PNG header");
    let mut buffer = vec![0u8; reader.output_buffer_size().expect("a bounded buffer size")];
    let info = reader.next_frame(&mut buffer).expect("the fixture decodes");
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|texel| [texel[0], texel[1], texel[2], 255])
            .collect(),
        other => panic!("fixture {path} is {other:?}; the fixtures are RGB or RGBA"),
    };
    (info.width, info.height, rgba)
}

impl Fixture {
    /// `(width, height, canonical RGBA texels)`.
    fn texels(&self) -> (u32, u32, Vec<u8>) {
        match self.path {
            Some(path) => load_png(path),
            None => (
                NOISE_EXTENT,
                NOISE_EXTENT,
                detail_noise(NOISE_EXTENT, NOISE_EXTENT, NOISE_AMPLITUDE),
            ),
        }
    }

    fn class(&self) -> AssetClass {
        AssetClass::of(self.kind).expect("every fixture names a known kind")
    }
}

fn fixture(name: &str) -> &'static Fixture {
    FIXTURES
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fixture named {name}"))
}

/// A measured fraction, formatted the way `TABLE` and `MUTATIONS` record it.
///
/// Compared as a string rather than as a float: four decimal places of a
/// percentage is finer than any real difference between two rungs, and string
/// equality is exact where float equality is a judgement call.
fn percent(fraction: f64) -> String {
    format!("{:.4}", fraction * 100.0)
}

// ------------------------------------------------------------ the contract

/// One row of the recorded contract table.
struct Row {
    fixture: &'static str,
    profile: Profile,
    /// The rung the escalation stopped at, or `CANONICAL` under RAW, whose
    /// binding derives nothing at all.
    rung: &'static str,
    /// The chosen rung's measured differing fraction as a percentage, or
    /// `LOSSLESS` where the class has no band because it has no choice.
    accepted: &'static str,
    /// Every rung rejected before it, cheapest first, with what it measured.
    rejected: &'static [(&'static str, &'static str)],
    /// The length of the chosen rung's KTX2 file, in bytes. Zero under RAW.
    bytes: usize,
    /// The BLAKE3 of the chosen rung's KTX2 file, hex, or `CANONICAL` under
    /// RAW, whose binding produces no file to hash. See "Why a digest and not
    /// only a length" above.
    digest: &'static str,
}

const CANONICAL: &str = "canonical";
const LOSSLESS: &str = "lossless";

/// **Measured, not predicted.** Every number below was read off a run of
/// [`the_recorded_contract_table`], not chosen in advance.
///
/// Read it as the answer to "what do these three profiles actually do to these
/// five assets":
///
/// - RAW derives nothing for any of them. It is the null binding.
/// - The two profiles genuinely differ: `import-image-fill` ships at 6x6 under
///   HiFi and at 12x12 under LoFi, and `detail-noise` ships uncompressed under
///   HiFi and at 6x6 under LoFi. A profile pair that chose the same rung
///   everywhere would be one contract with two names.
/// - `detail-noise` under HiFi is the terminal rung reached in anger: every one
///   of the six lossy rungs is refused and the lossless one wins. That is what
///   "over-compression is structurally impossible" means in practice.
/// - Both distance fields go straight to the lossless rung with nothing
///   rejected, because the hard rule left no lossy rung to try.
/// - The design capture expected HiFi to be "typically ASTC 4x4". On these
///   assets it measures 6x6, 8x8 and uncompressed, and never 4x4. The
///   measurement is what is recorded; the expectation is not.
///
/// # The one time `bytes` moved, and why
///
/// Story #434 changed `ktx2::WRITER` from the crate version to a pinned string
/// naming the writer generation and the encoder pin, so that a release bump
/// stops rewriting every texture in every shipped bank. The new string is 12
/// bytes longer once the key/value entry is padded, and that section is not
/// compressed, so **every** row's `bytes` grew by exactly 12 and nothing else
/// changed: no rung, no accepted fraction, no rejected fraction. A uniform
/// twelve is the signature of a writer-string change; a single row moving, or a
/// row moving by anything else, is not and must be explained on its own terms.
/// Since #458 every `digest` moves with it too, so that signature is now "a
/// uniform twelve and ten new digests" rather than a length change alone.
///
/// # Reading the digests
///
/// Three pairs of rows share a digest — `v03-paint`, `inter-ascii-atlas` and
/// `arabic-atlas` each under HiFi and LoFi. That is the two profiles reaching
/// the same rung for the same asset, which must produce the same file: the
/// binding differs only in which rung it stops at, and nothing downstream of
/// the rung depends on the profile. A shared rung whose digests diverged would
/// mean the profile leaked into the encoding.
const TABLE: [Row; 27] = [
    Row {
        fixture: "import-image-fill",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "import-image-fill",
        profile: Profile::HiFi,
        rung: "astc-6x6",
        accepted: "0.2133",
        rejected: &[
            ("astc-12x12", "19.1129"),
            ("astc-10x10", "10.6115"),
            ("astc-8x8", "2.8012"),
        ],
        bytes: 21026,
        digest: "a73ce67df7aa4382d085c3ceea24256ca11c878d3e7c60f04b866154faab319b",
    },
    Row {
        fixture: "import-image-fill",
        profile: Profile::LoFi,
        rung: "astc-12x12",
        accepted: "0.0000",
        rejected: &[],
        bytes: 7961,
        digest: "2f13049640c364b75a6039b5147867d39e1ac5fbffecd39d0db5028fc678d78c",
    },
    Row {
        fixture: "v03-paint",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "v03-paint",
        profile: Profile::HiFi,
        rung: "astc-8x8",
        accepted: "0.0000",
        rejected: &[("astc-12x12", "51.5625"), ("astc-10x10", "100.0000")],
        bytes: 249,
        digest: "8ff1116963b8d320b8ab061d03aceb256723a2effc88ee2d0b3ccd1b5db43377",
    },
    Row {
        fixture: "v03-paint",
        profile: Profile::LoFi,
        rung: "astc-8x8",
        accepted: "0.0000",
        rejected: &[("astc-12x12", "50.0000"), ("astc-10x10", "68.7500")],
        bytes: 249,
        digest: "8ff1116963b8d320b8ab061d03aceb256723a2effc88ee2d0b3ccd1b5db43377",
    },
    Row {
        fixture: "detail-noise",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "detail-noise",
        profile: Profile::HiFi,
        rung: "astc-4x4",
        accepted: "54.4891",
        rejected: &[
            ("astc-12x12", "96.2921"),
            ("astc-10x10", "95.5338"),
            ("astc-8x8", "93.5471"),
            ("astc-6x6", "87.6099"),
            ("astc-5x5", "76.1032"),
        ],
        bytes: 63220,
        digest: "3d141b550b03aee6bc5ede7daa41c6b2e686874af73c9c45d51aadeb2a59131e",
    },
    Row {
        fixture: "detail-noise",
        profile: Profile::LoFi,
        rung: "astc-6x6",
        accepted: "4.8187",
        rejected: &[
            ("astc-12x12", "15.5731"),
            ("astc-10x10", "13.3896"),
            ("astc-8x8", "10.4401"),
        ],
        bytes: 29181,
        digest: "bd8568cbca85a8288beaf04db6b3ae3da317b9c11bf1b9b48110411709145d1b",
    },
    Row {
        fixture: "photo-interior-render",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "photo-interior-render",
        profile: Profile::HiFi,
        rung: "astc-4x4",
        accepted: "7.6714",
        rejected: &[
            ("astc-12x12", "37.6316"),
            ("astc-10x10", "32.8083"),
            ("astc-8x8", "26.6972"),
            ("astc-6x6", "18.5814"),
            ("astc-5x5", "13.3068"),
        ],
        bytes: 210102,
        digest: "aa32016024ab2abb71ac40a5aa127cf811cac66f15b00fee2603a1ace4172684",
    },
    Row {
        fixture: "photo-interior-render",
        profile: Profile::LoFi,
        rung: "astc-6x6",
        accepted: "4.2152",
        rejected: &[
            ("astc-12x12", "12.9505"),
            ("astc-10x10", "10.3706"),
            ("astc-8x8", "7.4951"),
        ],
        bytes: 104436,
        digest: "e372cedbfa2eb9b2e68eb0692df67dfbd0219d2f93143baf34e419731a2252aa",
    },
    Row {
        fixture: "photo-coast-forest",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "photo-coast-forest",
        profile: Profile::HiFi,
        rung: "astc-4x4",
        accepted: "46.2559",
        rejected: &[
            ("astc-12x12", "71.5351"),
            ("astc-10x10", "68.7553"),
            ("astc-8x8", "66.7664"),
            ("astc-6x6", "62.6858"),
            ("astc-5x5", "58.0051"),
        ],
        bytes: 249238,
        digest: "acf93ae0a98de0276c6768a4fcc0bd9905123dfe1847221a9e864a567ba82efa",
    },
    Row {
        fixture: "photo-coast-forest",
        profile: Profile::LoFi,
        rung: "astc-4x4",
        accepted: "4.3911",
        rejected: &[
            ("astc-12x12", "53.5313"),
            ("astc-10x10", "49.8661"),
            ("astc-8x8", "43.4937"),
            ("astc-6x6", "28.9410"),
            ("astc-5x5", "16.0816"),
        ],
        bytes: 249238,
        digest: "acf93ae0a98de0276c6768a4fcc0bd9905123dfe1847221a9e864a567ba82efa",
    },
    Row {
        fixture: "photo-snowy-forest",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "photo-snowy-forest",
        profile: Profile::HiFi,
        rung: "astc-4x4",
        accepted: "21.5595",
        rejected: &[
            ("astc-12x12", "90.4366"),
            ("astc-10x10", "88.4422"),
            ("astc-8x8", "85.1051"),
            ("astc-6x6", "75.0023"),
            ("astc-5x5", "55.4966"),
        ],
        bytes: 248781,
        digest: "82e390d1a76f204607245336af7500f13fb674046adcf02a13c053b0637d03cc",
    },
    Row {
        fixture: "photo-snowy-forest",
        profile: Profile::LoFi,
        rung: "astc-5x5",
        accepted: "2.2385",
        rejected: &[
            ("astc-12x12", "51.9840"),
            ("astc-10x10", "45.6093"),
            ("astc-8x8", "34.8446"),
            ("astc-6x6", "12.9860"),
        ],
        bytes: 159969,
        digest: "65ecbc2e224455abdfbc46c076791fa9fc7a9a051a6baefe40374518033879e4",
    },
    Row {
        fixture: "photo-dawn-mountains",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "photo-dawn-mountains",
        profile: Profile::HiFi,
        rung: "astc-4x4",
        accepted: "0.2811",
        rejected: &[
            ("astc-12x12", "8.7715"),
            ("astc-10x10", "6.4636"),
            ("astc-8x8", "4.4891"),
            ("astc-6x6", "2.4605"),
            ("astc-5x5", "1.2505"),
        ],
        bytes: 186008,
        digest: "dd62cafbda019edef88b015320512df2813d885537e4251684bb6495e78fe45c",
    },
    Row {
        fixture: "photo-dawn-mountains",
        profile: Profile::LoFi,
        rung: "astc-12x12",
        accepted: "1.0078",
        rejected: &[],
        bytes: 20228,
        digest: "7d485b2c6c2e0142ff44f817c9e1ce67ed20c3dce25df959422a954c58bbde90",
    },
    Row {
        fixture: "inter-ascii-atlas",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "inter-ascii-atlas",
        profile: Profile::HiFi,
        rung: "uncompressed",
        accepted: LOSSLESS,
        rejected: &[],
        bytes: 73703,
        digest: "8faf218894227208fe82b49989fd884ab0e6fbf461e387f1df63a1d2edf4476d",
    },
    Row {
        fixture: "inter-ascii-atlas",
        profile: Profile::LoFi,
        rung: "uncompressed",
        accepted: LOSSLESS,
        rejected: &[],
        bytes: 73703,
        digest: "8faf218894227208fe82b49989fd884ab0e6fbf461e387f1df63a1d2edf4476d",
    },
    Row {
        fixture: "arabic-atlas",
        profile: Profile::Raw,
        rung: CANONICAL,
        accepted: CANONICAL,
        rejected: &[],
        bytes: 0,
        digest: CANONICAL,
    },
    Row {
        fixture: "arabic-atlas",
        profile: Profile::HiFi,
        rung: "uncompressed",
        accepted: LOSSLESS,
        rejected: &[],
        bytes: 98538,
        digest: "ea0f7d6489186edf7feb6edd2d4af1bc3576759a7c4a91b8523679942f61ff9b",
    },
    Row {
        fixture: "arabic-atlas",
        profile: Profile::LoFi,
        rung: "uncompressed",
        accepted: LOSSLESS,
        rejected: &[],
        bytes: 98538,
        digest: "ea0f7d6489186edf7feb6edd2d4af1bc3576759a7c4a91b8523679942f61ff9b",
    },
];

/// The whole contract, re-measured and compared against what is recorded.
#[test]
fn the_recorded_contract_table() {
    for row in &TABLE {
        let fixture = fixture(row.fixture);
        let (width, height, texels) = fixture.texels();
        let image = Rgba8::new(width, height, &texels).expect("a fixture is a valid RGBA image");
        let binding = pack(row.profile, fixture.kind, image).expect("a fixture packs");
        let context = format!("{}/{:?}", row.fixture, row.profile);

        let derivation = match binding {
            Binding::Canonical => {
                assert_eq!(row.rung, CANONICAL, "{context}: RAW derives nothing");
                assert_eq!(
                    row.digest, CANONICAL,
                    "{context}: a row that derives nothing has no file to hash, so recording a \
                     digest for it would record a number nothing produced"
                );
                continue;
            }
            Binding::Derived(derivation) => derivation,
        };
        assert_ne!(
            row.rung, CANONICAL,
            "{context}: only RAW may derive nothing"
        );

        assert_eq!(
            derivation.rung.to_string(),
            row.rung,
            "{context}: the escalation stopped at a different rung"
        );
        let accepted = derivation
            .accepted
            .map_or_else(|| LOSSLESS.to_string(), |diff| percent(diff.fraction()));
        assert_eq!(
            accepted, row.accepted,
            "{context}: the chosen rung measured differently"
        );
        let rejected: Vec<(String, String)> = derivation
            .rejected
            .iter()
            .map(|attempt| (attempt.rung.to_string(), percent(attempt.diff.fraction())))
            .collect();
        let recorded: Vec<(String, String)> = row
            .rejected
            .iter()
            .map(|(rung, measured)| ((*rung).to_string(), (*measured).to_string()))
            .collect();
        assert_eq!(
            rejected, recorded,
            "{context}: a different set of rungs was rejected"
        );
        assert_eq!(
            derivation.file.len(),
            row.bytes,
            "{context}: the chosen rung's container is a different size"
        );
        assert_eq!(
            blake3::hash(&derivation.file).to_hex().as_str(),
            row.digest,
            "{context}: the chosen rung's container is {} bytes as recorded but not the \
             recorded bytes, so this is a rewrite rather than a resize — an encoder, \
             compressor or writer change, not a band change (issue #458)",
            row.bytes
        );
    }
}

// ----------------------------------------------- the mutation that fails it

/// The measured mutation that fails one band.
///
/// The mutation is **pin the ladder one rung coarser than the packer chose**,
/// which is the defect this whole mechanism exists to prevent: shipping an
/// asset at an encoding that is too cheap for its profile. It is measured on a
/// fixture whose failure is *narrow*, so what fails is the recorded budget
/// rather than arithmetic that any budget would have failed.
struct Mutation {
    /// The band under test, by its `rule` name.
    band: &'static str,
    /// The fixture the mutation is measured on.
    fixture: &'static str,
    /// The rung the packer chose for it.
    chosen: &'static str,
    /// One rung coarser — the mutation.
    mutated: &'static str,
    /// What the mutated rung measures, as a percentage.
    measured: &'static str,
    /// The band's budget, as a percentage.
    budget: &'static str,
}

/// One row per band in `dashpack::profile::BANDS`.
/// `every_band_ships_the_measured_mutation_that_fails_it` enforces that.
///
/// Both are near misses on purpose — 2.8 times and 2.1 times the budget, not
/// fifty times it. A mutation that fails a band by two orders of magnitude
/// shows the band is not vacuous but says nothing about whether the *number* is
/// the binding term. These two do: move either budget up by a factor of three
/// and the packer ships the coarser rung.
const MUTATIONS: [Mutation; 2] = [
    Mutation {
        band: "hifi-image-fill",
        fixture: "import-image-fill",
        chosen: "astc-6x6",
        mutated: "astc-8x8",
        measured: "2.8012",
        budget: "1.0000",
    },
    Mutation {
        band: "lofi-image-fill",
        fixture: "detail-noise",
        chosen: "astc-6x6",
        mutated: "astc-8x8",
        measured: "10.4401",
        budget: "5.0000",
    },
];

/// Parses `astc-NxM` back to a footprint.
fn rung_named(name: &str) -> BlockSize {
    let (x, y) = name
        .strip_prefix("astc-")
        .and_then(|rest| rest.split_once('x'))
        .unwrap_or_else(|| panic!("{name} is not an ASTC rung"));
    BlockSize {
        x: x.parse().expect("a footprint width"),
        y: y.parse().expect("a footprint height"),
    }
}

/// Encodes `fixture` at one footprint and measures it against `band`.
fn measure_at(fixture: &Fixture, block: BlockSize, band: &'static ToleranceBand) -> String {
    let (width, height, texels) = fixture.texels();
    let class = fixture.class();
    let image = Rgba8::new(width, height, &texels).expect("a fixture is a valid RGBA image");
    let payload =
        encode(image, block, class.color_space(), PACK_QUALITY).expect("the rung encodes");
    let decoded =
        decode(&payload, width, height, block, class.color_space()).expect("the rung decodes");
    let measured = diff(&texels, &decoded, band).expect("the candidate matches the canonical size");
    assert!(
        !measured.passes(),
        "the mutation must fail {}: it measured {} % against {} %",
        band.rule,
        percent(measured.fraction()),
        percent(band.differing_fraction)
    );
    percent(measured.fraction())
}

/// #422's recommendation, as a gate: every pinned band carries a mutation that
/// is measured to fail it.
#[test]
fn every_band_ships_the_measured_mutation_that_fails_it() {
    for band in BANDS {
        let mutation = MUTATIONS
            .iter()
            .find(|m| m.band == band.rule)
            .unwrap_or_else(|| {
                panic!(
                    "band {} pins a budget with no recorded mutation that fails it; a budget \
                     never exercised is not a gate (#422)",
                    band.rule
                )
            });

        assert_eq!(
            mutation.budget,
            percent(band.differing_fraction),
            "{}: the recorded budget is not the band's budget",
            band.rule
        );

        // The mutation is the rung immediately coarser than the chosen one, so
        // it is one step of the ladder rather than an arbitrary bad encoding.
        let rungs = AssetClass::ImageFill.lossy_rungs();
        let chosen = rungs
            .iter()
            .position(|b| Rung::Astc(*b).to_string() == mutation.chosen)
            .unwrap_or_else(|| panic!("{} is not a rung of the ladder", mutation.chosen));
        assert!(
            chosen > 0,
            "{}: the chosen rung has nothing coarser",
            band.rule
        );
        assert_eq!(
            Rung::Astc(rungs[chosen - 1]).to_string(),
            mutation.mutated,
            "{}: the mutation is not one rung coarser than the chosen rung",
            band.rule
        );

        let measured = measure_at(
            fixture(mutation.fixture),
            rung_named(mutation.mutated),
            band,
        );
        assert_eq!(
            measured, mutation.measured,
            "{}: the mutation measured differently than recorded",
            band.rule
        );
    }
}

/// The other half of a gate: the chosen rung must actually pass. A band that
/// nothing passes would refuse every lossy rung and reduce the packer to
/// shipping uncompressed bytes.
#[test]
fn the_rung_each_mutation_names_as_chosen_is_the_one_that_passes() {
    for mutation in &MUTATIONS {
        let band = profile::band_for(mutation.band).expect("the mutation names a pinned band");
        let fixture = fixture(mutation.fixture);
        let (width, height, texels) = fixture.texels();
        let block = rung_named(mutation.chosen);
        let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
        let payload =
            encode(image, block, fixture.class().color_space(), PACK_QUALITY).expect("encodes");
        let decoded = decode(
            &payload,
            width,
            height,
            block,
            fixture.class().color_space(),
        )
        .expect("decodes");
        let measured = diff(&texels, &decoded, band).expect("same size");
        assert!(
            measured.passes(),
            "{}: the chosen rung {} must pass, but it measured {} % against {} %",
            mutation.band,
            mutation.chosen,
            percent(measured.fraction()),
            percent(band.differing_fraction)
        );
    }
}

// ------------------------------------------------------------- mutation tests

/// Widening the budget accepts the rung the pinned budget rejects.
///
/// This is the direct test that the *number* binds, and the answer to #422's
/// finding: for `blur-falloff`, no reachable measurement exceeded the budget,
/// so the number could have been anything. Here, tripling either budget changes
/// what ships.
#[test]
fn widening_a_budget_changes_which_rung_ships() {
    for mutation in &MUTATIONS {
        let pinned = profile::band_for(mutation.band).expect("a pinned band");
        let widened: &'static ToleranceBand = Box::leak(Box::new(ToleranceBand {
            rule: "widened",
            channel_delta: pinned.channel_delta,
            differing_fraction: pinned.differing_fraction * 3.0,
        }));
        let fixture = fixture(mutation.fixture);
        let (width, height, texels) = fixture.texels();
        let block = rung_named(mutation.mutated);
        let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
        let payload =
            encode(image, block, fixture.class().color_space(), PACK_QUALITY).expect("encodes");
        let decoded = decode(
            &payload,
            width,
            height,
            block,
            fixture.class().color_space(),
        )
        .expect("decodes");

        assert!(
            !diff(&texels, &decoded, pinned).expect("same size").passes(),
            "{}: the pinned budget must reject {}",
            mutation.band,
            mutation.mutated
        );
        assert!(
            diff(&texels, &decoded, widened)
                .expect("same size")
                .passes(),
            "{}: a budget three times as wide must accept {}, or the budget is not what \
             chooses the rung",
            mutation.band,
            mutation.mutated
        );
    }
}

/// A band that cannot refuse anything stops the escalation at the first rung.
///
/// The "make the diff always return zero" mutation, expressed through the band
/// rather than by breaking the diff: with a threshold no texel can exceed,
/// every rung passes, and the packer ships the cheapest one. It is what the
/// pinned bands would do if their measurement were inert — and the recorded
/// table shows they do not.
#[test]
fn an_unfailable_band_would_ship_the_cheapest_rung() {
    let inert: &'static ToleranceBand = Box::leak(Box::new(ToleranceBand {
        rule: "inert",
        channel_delta: u8::MAX,
        differing_fraction: 1.0,
    }));
    let fixture = fixture("detail-noise");
    let (width, height, texels) = fixture.texels();
    let cheapest = AssetClass::ImageFill.lossy_rungs()[0];
    let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
    let payload = encode(image, cheapest, ColorSpace::Srgb, PACK_QUALITY).expect("encodes");
    let decoded = decode(&payload, width, height, cheapest, ColorSpace::Srgb).expect("decodes");
    assert!(
        diff(&texels, &decoded, inert).expect("same size").passes(),
        "an unfailable band accepts the cheapest rung"
    );

    // The pinned band, on the same rung, does not — which is the whole point.
    assert!(
        !diff(&texels, &decoded, &HIFI_IMAGE_FILL)
            .expect("same size")
            .passes(),
        "the pinned HiFi band must refuse what an unfailable band accepts"
    );
}

// ------------------------------------------------------- the hard kind rule

/// The fields-never-lossy rule is measured, not assumed.
///
/// The design capture states it as a rule and the reading was left ambiguous
/// (a distance field is never lossy, yet single-channel fields were to ride the
/// lossy EAC-R11). This measures the rule instead: at the **finest** ASTC
/// footprint the ladder offers — 4x4, 8 bits per texel, the most expensive
/// lossy rung there is — an MSDF atlas still fails both profiles' bands. There
/// is no lossy rung that could have been chosen, so the strict reading costs
/// nothing a measurement would have bought back.
#[test]
fn no_lossy_rung_could_hold_a_distance_field() {
    for name in ["inter-ascii-atlas", "arabic-atlas"] {
        let fixture = fixture(name);
        assert_eq!(fixture.class(), AssetClass::DistanceField);
        let (width, height, texels) = fixture.texels();
        let finest = BlockSize::ASTC_4X4;
        let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
        let payload = encode(image, finest, ColorSpace::Linear, PACK_QUALITY).expect("encodes");
        let decoded = decode(&payload, width, height, finest, ColorSpace::Linear).expect("decodes");

        for band in BANDS {
            let measured = diff(&texels, &decoded, band).expect("same size");
            assert!(
                !measured.passes(),
                "{name} at the finest lossy rung must fail {}: it measured {} % against {} %",
                band.rule,
                percent(measured.fraction()),
                percent(band.differing_fraction)
            );
        }
    }
}

/// The rule is structural: there is no lossy rung to reach, under any profile.
#[test]
fn a_distance_field_is_packed_losslessly_whatever_the_profile() {
    for name in ["inter-ascii-atlas", "arabic-atlas"] {
        let fixture = fixture(name);
        let (width, height, texels) = fixture.texels();
        for profile in [Profile::HiFi, Profile::LoFi] {
            assert_eq!(contract(profile, fixture.class()), Contract::LosslessOnly);
            let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
            let Binding::Derived(derivation) = pack(profile, fixture.kind, image).expect("packs")
            else {
                panic!("{name}/{profile:?}: a production profile derives a payload");
            };
            assert_eq!(derivation.rung, Rung::Uncompressed);
            assert!(
                derivation.rejected.is_empty(),
                "{name}/{profile:?}: nothing is tried, because nothing lossy is offered"
            );
            assert!(
                derivation.accepted.is_none(),
                "{name}/{profile:?}: no band, because there is no choice to make"
            );
        }
    }
}

// ------------------------------------------------------- fixture discipline

/// The v0.11 acceptance condition, held to by a test rather than by review.
#[test]
fn more_than_one_asset_per_class_and_one_that_escalates() {
    for class in [AssetClass::ImageFill, AssetClass::DistanceField] {
        let count = FIXTURES.iter().filter(|f| f.class() == class).count();
        assert!(
            count > 1,
            "{class:?} has {count} fixture(s); one instance cannot fail a dedup, ordering or \
             wrong-index bug (debt #395)"
        );
    }

    // At least one asset must actually walk the ladder, or the escalation
    // mechanism is untested by everything above.
    let escalating: Vec<&str> = TABLE
        .iter()
        .filter(|row| !row.rejected.is_empty())
        .map(|row| row.fixture)
        .collect();
    assert!(
        !escalating.is_empty(),
        "no recorded row escalates; a band oracle where nothing escalates tests none of the \
         escalation mechanism"
    );
    // Reaching the lossless rung *by escalation* is no longer something any
    // committed fixture does, and that is a deliberate consequence rather than
    // an oversight: HiFi's image-fill contract now ends at
    // `Terminal::FinestLossy` (section 7 of the band decision record), and
    // LoFi's band accepts a lossy rung on every committed asset. The rows that
    // sit at `uncompressed` are the two distance fields, which reach it under
    // `Contract::LosslessOnly` without walking a ladder at all.
    //
    // So the claim "over-compression is impossible rather than merely unlikely"
    // is proven by `lofi_escalates_to_the_lossless_terminal_when_the_band_never
    // _holds` below, on content generated to be hard enough to force it, rather
    // than by a committed fixture that happens to be that hard.
    assert!(
        TABLE
            .iter()
            .any(|row| row.rung == "uncompressed" && row.rejected.is_empty()),
        "no recorded row sits at the lossless rung at all"
    );
}

/// The lossless terminal, still reachable and still lossless.
///
/// `Terminal::Lossless` is what makes over-compression impossible for every
/// contract that keeps it — RAW, LoFi, and every distance field. Capping HiFi
/// at the finest lossy rung left that path with no committed fixture exercising
/// it, so it is exercised here directly: content generated to defeat every rung
/// of the ladder, packed under LoFi, must arrive at `Rung::Uncompressed` having
/// rejected all six lossy rungs, and must be bit-exact when it does.
///
/// The amplitude is 16 rather than the fixture's 8 for the reason `FIXTURES`
/// records: at 8 both budgets bind partway up the ladder, which is what makes
/// it a good band fixture and a useless terminal one.
#[test]
fn lofi_escalates_to_the_lossless_terminal_when_the_band_never_holds() {
    let texels = detail_noise(NOISE_EXTENT, NOISE_EXTENT, 16);
    let image = Rgba8::new(NOISE_EXTENT, NOISE_EXTENT, &texels).expect("the extent matches");
    let Binding::Derived(derived) = pack(Profile::LoFi, AssetKind::Image, image).expect("it packs")
    else {
        panic!("only RAW binds canonically");
    };

    assert_eq!(
        derived.rung,
        Rung::Uncompressed,
        "content this hard must escalate past every lossy rung, or the terminal is unreachable          and over-compression is only unlikely rather than impossible",
    );
    assert_eq!(
        derived.rejected.len(),
        AssetClass::ImageFill.lossy_rungs().len(),
        "every lossy rung must have been tried and named before the terminal was taken",
    );
    let accepted = derived
        .accepted
        .expect("a banded contract measures what it accepted");
    assert!(
        accepted.is_lossless(),
        "the terminal rung's payload is the canonical texels, so it cannot differ from them",
    );
    assert!(
        accepted.passes(),
        "and being lossless, it satisfies any band — which is the whole point of it",
    );
}

/// The two profiles are two contracts, not one contract with two names.
#[test]
fn hifi_and_lite_choose_different_rungs_for_the_same_asset() {
    let differing: Vec<&str> = FIXTURES
        .iter()
        .filter(|fixture| {
            let rung = |profile| {
                TABLE
                    .iter()
                    .find(|row| row.fixture == fixture.name && row.profile == profile)
                    .map(|row| row.rung)
            };
            rung(Profile::HiFi) != rung(Profile::LoFi)
        })
        .map(|fixture| fixture.name)
        .collect();
    assert!(
        differing.len() >= 2,
        "only {differing:?} separate HiFi from LoFi; a profile pair that agrees everywhere is \
         one contract with two names"
    );
}

/// RAW derives nothing for anything, which is what "null binding" means.
#[test]
fn raw_derives_nothing_for_any_asset() {
    for fixture in &FIXTURES {
        let (width, height, texels) = fixture.texels();
        let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
        assert_eq!(
            pack(Profile::Raw, fixture.kind, image).expect("packs"),
            Binding::Canonical,
            "{}: RAW is the identity map, not an encoding",
            fixture.name
        );
    }
}

/// The chosen container is the one the rung names, and it is a real KTX2 file.
#[test]
fn the_derived_file_is_the_container_the_rung_names() {
    let fixture = fixture("v03-paint");
    let (width, height, texels) = fixture.texels();
    let image = Rgba8::new(width, height, &texels).expect("a valid RGBA image");
    let Binding::Derived(Derivation {
        rung, format, file, ..
    }) = pack(Profile::HiFi, fixture.kind, image).expect("packs")
    else {
        panic!("HiFi derives a payload");
    };
    assert_eq!(format, rung.format(AssetClass::ImageFill));
    // The KTX2 identifier, so the bytes really are a container rather than a
    // bare payload.
    assert_eq!(
        &file[..12],
        &[
            0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A
        ]
    );
}

/// The pack quality is pinned, and it is the offline one.
#[test]
fn the_pack_quality_is_pinned_to_the_offline_preset() {
    assert_eq!(PACK_QUALITY, Quality::Thorough);
}

/// The two bands differ in both knobs, in the same direction.
///
/// Checked at compile time rather than at run time: both sides are constants,
/// so an edit that made the entry target stricter than the premium one should
/// fail the build rather than wait for a test run.
const _: () = assert!(
    LOFI_IMAGE_FILL.channel_delta > HIFI_IMAGE_FILL.channel_delta,
    "LoFi's per-texel threshold must be looser than HiFi's"
);
const _: () = assert!(
    LOFI_IMAGE_FILL.differing_fraction > HIFI_IMAGE_FILL.differing_fraction,
    "LoFi's area budget must be looser than HiFi's"
);

/// Every fixture is recorded under every profile.
///
/// `the_recorded_contract_table` walks [`TABLE`], so it grades the rows that
/// exist and cannot notice a row that does not. A fixture added to
/// [`FIXTURES`] without its rows is therefore packed by
/// `raw_derives_nothing_for_any_asset` — which only checks that RAW is the
/// identity map — and measured by nothing else, while the suite reports green
/// (debt #535).
///
/// That failure mode is forward-looking rather than a defect today: all five
/// fixtures are fully recorded. It matters because the next person to add one
/// can believe it is covered when it is not, and the recorded table is the
/// whole instrument this file exists to keep honest.
///
/// The relation is asserted in **both** directions. A row naming a fixture
/// that no longer exists is the same defect seen from the other side: it
/// records numbers nothing produces any more, and `fixture()` would panic on
/// it — a panic in a helper rather than a named failure saying what is wrong.
#[test]
fn every_fixture_is_recorded_under_every_profile() {
    const PROFILES: [Profile; 3] = [Profile::Raw, Profile::HiFi, Profile::LoFi];

    for fixture in &FIXTURES {
        for profile in PROFILES {
            assert!(
                TABLE
                    .iter()
                    .any(|row| row.fixture == fixture.name && row.profile == profile),
                "fixture {} has no recorded row under {profile:?}, so its behaviour under that \
                 profile is measured by nothing while this file reports green",
                fixture.name,
            );
        }
    }

    for row in &TABLE {
        assert!(
            FIXTURES.iter().any(|fixture| fixture.name == row.fixture),
            "the table records {} under {:?}, but no such fixture exists — the row's numbers \
             describe nothing that is still produced",
            row.fixture,
            row.profile,
        );
    }

    // The counts agree, so neither loop above can be satisfied by duplicates
    // standing in for a missing pair.
    assert_eq!(
        TABLE.len(),
        FIXTURES.len() * PROFILES.len(),
        "the table holds one row per fixture per profile, so a duplicate row cannot cover for \
         an absent one",
    );
}
