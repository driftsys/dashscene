//! Tool-dependent pipeline tests. They self-skip when msdf-atlas-gen
//! is absent, but fail when `DASHSCENE_REQUIRE_ATLAS_TOOL` is set —
//! CI sets it, so a skipped test cannot be reported as passing there.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use dashscene_typeset::atlas::{
    AtlasBundle, AtlasSpec, REQUIRE_TOOL_ENV, charset_closure, generate,
};
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{
    FONT, FONT_ARABIC, FONT_BOLD, FONT_INTER, FONT_INTER_BOLD, FONT_INTER_MEDIUM,
    FONT_INTER_SEMIBOLD, FONT_SEMIBOLD,
};

// The committed atlas fixtures live under the shared corpus/atlas/ home,
// beside the fonts they are generated from — not under this crate's
// tests/, so a golden in another crate can load them without reaching
// into a crate's private test tree (debt #217).
const ASCII_FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
const ARABIC_FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");
// One atlas directory per (script, weight) — story #368's decision. The
// Regular fixtures above are never rewritten, so their bytes and the E7
// frames that render through them are untouched by adding a weight.
const ASCII_SEMIBOLD_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/ascii-semibold"
);
const ASCII_BOLD_FIXTURE_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii-bold");
// The Inter fixtures (story #385). The same one-directory-per-(script,
// weight) rule, now with the family in the name: the four `ascii*`
// directories above stay exactly as they are — renaming them would reach
// the shared `tests/common/` loader the E7 oracle uses, and the atlases
// they hold are unaffected by a second family joining the cascade.
const INTER_ASCII_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii"
);
const INTER_ASCII_MEDIUM_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-medium"
);
const INTER_ASCII_SEMIBOLD_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-semibold"
);
const INTER_ASCII_BOLD_FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-bold"
);

/// Returns false (and prints why) when the pinned tool is unavailable
/// and the environment tolerates that; panics when CI demands it.
fn tool_available() -> bool {
    match dashscene_typeset::atlas::find_tool_checked() {
        Ok(_) => true,
        Err(e) if std::env::var_os(REQUIRE_TOOL_ENV).is_some() => {
            panic!("{REQUIRE_TOOL_ENV} is set but: {e}")
        }
        Err(e) => {
            eprintln!("skipping tool-dependent test: {e}");
            false
        }
    }
}

fn ascii_charset() -> BTreeSet<char> {
    (0x20u8..=0x7e).map(char::from).collect()
}

/// The one fixture contract: the committed fixture, the regeneration
/// test, and every ASCII-atlas assertion all build from this spec.
fn ascii_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT), ascii_charset())
}

/// The SemiBold (600) and Bold (700) ASCII fixture contracts, mirroring
/// [`ascii_spec`]: same charset, same atlas parameters, a different face
/// (story #368). Weight is carried by the face, not by the spec — the
/// atlas format is unchanged and `AtlasMetrics::FORMAT_VERSION` stays 1,
/// which is why the Regular fixtures need no regeneration.
fn ascii_semibold_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_SEMIBOLD), ascii_charset())
}

fn ascii_bold_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_BOLD), ascii_charset())
}

/// The four Inter ASCII fixture contracts (story #385), mirroring
/// [`ascii_spec`] exactly: the same charset and the same atlas
/// parameters, a different family. Nothing about the atlas format
/// changes for a second family, so `AtlasMetrics::FORMAT_VERSION`
/// stays 1 and the Noto fixtures are not regenerated.
fn inter_ascii_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_INTER), ascii_charset())
}

fn inter_ascii_medium_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_INTER_MEDIUM), ascii_charset())
}

fn inter_ascii_semibold_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_INTER_SEMIBOLD), ascii_charset())
}

fn inter_ascii_bold_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_INTER_BOLD), ascii_charset())
}

/// One shared generation for the read-only tests; `double_run` adds
/// the second, independent run that proves byte-identity.
fn shared_ascii_bundle() -> &'static AtlasBundle {
    static BUNDLE: OnceLock<AtlasBundle> = OnceLock::new();
    BUNDLE.get_or_init(|| generate(&ascii_spec()).expect("pipeline runs"))
}

#[test]
fn generates_ascii_atlas_with_full_coverage() {
    if !tool_available() {
        return;
    }
    let bundle = shared_ascii_bundle();
    let m = &bundle.metrics;
    assert!(m.missing_codepoints.is_empty());
    // 95 ASCII chars resolve to 95 distinct gids, plus .notdef, plus the
    // three Latin ligatures the GSUB closure now covers (ff, fi, fl —
    // the two-character `liga` outputs; ffi/ffl are three-character and
    // out of the pairwise sweep).
    assert_eq!(m.glyphs.len(), 99);
    assert!(m.glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id));
    // space advances but paints nothing; every other glyph has bounds.
    let space_gid = {
        let data = std::fs::read(FONT).unwrap();
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        face.glyph_index(' ').unwrap().0
    };
    for g in &m.glyphs {
        assert!(g.advance_units > 0 || g.glyph_id == 0);
        if g.glyph_id == space_gid {
            assert!(g.plane_em.is_none() && g.atlas_px.is_none());
        } else if g.glyph_id != 0 {
            assert!(
                g.plane_em.is_some() && g.atlas_px.is_some(),
                "gid {}",
                g.glyph_id
            );
        }
    }
    assert_eq!(m.atlas.px_per_em, 32);
    assert_eq!(m.atlas.distance_range_px, 4.0);
    assert!(m.atlas.width > 0 && m.atlas.height > 0);
    assert!(!bundle.image_png.is_empty());
    assert_eq!(&bundle.image_png[1..4], b"PNG");
}

#[test]
fn double_run_is_byte_identical() {
    if !tool_available() {
        return;
    }
    let a = shared_ascii_bundle();
    let b = generate(&ascii_spec()).expect("second run");
    assert_eq!(
        a.image_png, b.image_png,
        "atlas.png must be byte-identical (R7)"
    );
    assert_eq!(
        a.metrics.to_bytes(),
        b.metrics.to_bytes(),
        "atlas.metrics must be byte-identical (R7)"
    );
}

#[test]
fn bundle_write_load_round_trips() {
    if !tool_available() {
        return;
    }
    let bundle = shared_ascii_bundle();
    let dir = tempfile::tempdir().expect("tempdir");
    bundle.write_to_dir(dir.path()).expect("writes");
    let back = AtlasBundle::load_from_dir(dir.path()).expect("loads");
    assert_eq!(bundle.image_png, back.image_png);
    assert_eq!(bundle.metrics, back.metrics);
}

#[test]
fn missing_codepoints_are_reported_not_dropped() {
    if !tool_available() {
        return;
    }
    // A tiny charset is enough: this test only proves the closure's
    // missing list reaches the blob.
    let charset: BTreeSet<char> = ['A', '\u{0710}'].into_iter().collect(); // Syriac alaph
    let spec = AtlasSpec::new(PathBuf::from(FONT), charset);
    let bundle = generate(&spec).expect("pipeline runs");
    assert_eq!(bundle.metrics.missing_codepoints, vec![0x0710]);
}

/// A representative Arabic UI charset: the standard Arabic letters,
/// Arabic-Indic digits, the common harakat, and space. The letter and
/// haraka ranges are the closure's own sweep ranges, reused here so the
/// two definitions cannot drift.
fn arabic_charset() -> BTreeSet<char> {
    let letters = dashscene_typeset::atlas::ARABIC_LETTERS.filter_map(char::from_u32);
    let harakat = dashscene_typeset::atlas::ARABIC_HARAKAT.filter_map(char::from_u32);
    let digits = (0x0660u32..=0x0669).filter_map(char::from_u32);
    letters.chain(harakat).chain(digits).chain([' ']).collect()
}

/// The Arabic fixture contract, mirroring `ascii_spec`: the committed
/// fixture, its regeneration, and every Arabic-atlas assertion build
/// from this one spec, so the writer and the checker cannot drift.
fn arabic_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT_ARABIC), arabic_charset())
}

/// Shapes `text` with the default OpenType feature set (ligatures on) —
/// the independent oracle for what the atlas must cover.
fn shaped_gids(face: &rustybuzz::Face<'_>, text: &str) -> Vec<u16> {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    rustybuzz::shape(face, &[], buffer)
        .glyph_infos()
        .iter()
        .map(|i| i.glyph_id as u16)
        .collect()
}

/// The whole point of the story: an atlas built from a declared Arabic
/// charset covers the GSUB contextual forms and ligatures that real
/// words composed from that charset shape to — glyphs that carry no
/// cmap entry and that a cmap-only closure would drop.
#[test]
fn arabic_atlas_covers_shaped_contextual_forms() {
    if !tool_available() {
        return;
    }
    let bundle = generate(&arabic_spec()).expect("pipeline runs over the Arabic fixture");
    let m = &bundle.metrics;
    assert!(
        m.missing_codepoints.is_empty(),
        "the fixture covers every declared codepoint: {:?}",
        m.missing_codepoints
    );
    let covered: BTreeSet<u16> = m.glyphs.iter().map(|g| g.glyph_id).collect();
    assert!(m.glyphs.windows(2).all(|w| w[0].glyph_id < w[1].glyph_id));

    let data = std::fs::read(FONT_ARABIC).unwrap();
    let face = rustybuzz::Face::from_slice(&data, 0).unwrap();

    // The atlas must carry glyphs cmap alone cannot reach — the
    // contextual forms and dots Noto Sans Arabic composes through GSUB.
    let cmap_only: BTreeSet<u16> = arabic_charset()
        .iter()
        .filter_map(|&c| face.glyph_index(c).map(|g| g.0))
        .chain(std::iter::once(0))
        .collect();
    assert!(
        covered.iter().any(|g| !cmap_only.contains(g)),
        "atlas added no GSUB-only glyph"
    );

    // Every glyph real Arabic words shape to (contextual forms, lam-alef
    // ligature, harakat) must be in the atlas.
    for word in [
        "مرحبا", // hello
        "السلام", // the peace — seen-joined lam-alef
        "بيت",   // house
        "كتاب",  // book
        "مدرسة", // school
        "درجة",  // degree
        "لا",     // isolated lam-alef
        "بَ",     // beh + fatha
        "١٢٣٤٥", // Arabic-Indic digits
    ] {
        for gid in shaped_gids(&face, word) {
            assert!(
                covered.contains(&gid),
                "word {word:?} shapes to gid {gid}, absent from the atlas"
            );
        }
    }
}

/// The full-charset production↔coverage pin for the E2 screen: every
/// glyph id the runtime pipeline lays out for text composed from the
/// declared Arabic charset — every letter through the four joining
/// contexts, every haraka on an isolated and a joined base, and
/// realistic strings with both digit systems — is inside
/// `charset_closure`'s coverage. A failure means the text and atlas
/// modules drifted on direction, feature set, or digit selection.
///
/// The full pairwise sweep costs seconds, so this pin runs where CI
/// already demands thoroughness — the atlas-repro job's env gate —
/// and self-skips on a plain `cargo test`. No tool binary is
/// involved; the env var is reused purely as that job's marker. The
/// corpus-charset variant in `tests/typeset_arabic.rs` runs
/// everywhere.
#[test]
fn production_layout_stays_within_full_charset_coverage() {
    if std::env::var_os(REQUIRE_TOOL_ENV).is_none() {
        eprintln!("skipping full-charset coverage pin: {REQUIRE_TOOL_ENV} unset");
        return;
    }
    let mut charset = arabic_charset();
    charset.extend('0'..='9');
    let data = std::fs::read(FONT_ARABIC).unwrap();
    let face = rustybuzz::Face::from_slice(&data, 0).unwrap();
    let closure = charset_closure(&face, &charset, &BTreeSet::new());
    assert!(closure.missing_codepoints.is_empty());
    let covered: BTreeSet<u16> = closure.glyph_ids.iter().copied().collect();

    // Every letter through the four joining contexts (beh as the
    // dual-joining connector, mirroring the closure's own sweep),
    // every haraka on an isolated and a joined base, and realistic
    // strings.
    let beh = '\u{0628}';
    let mut corpus: Vec<String> = Vec::new();
    for c in dashscene_typeset::atlas::ARABIC_LETTERS.filter_map(char::from_u32) {
        corpus.push(c.to_string());
        corpus.push(format!("{beh}{c}"));
        corpus.push(format!("{c}{beh}"));
        corpus.push(format!("{beh}{c}{beh}"));
    }
    for c in dashscene_typeset::atlas::ARABIC_HARAKAT.filter_map(char::from_u32) {
        corpus.push(format!("{beh}{c}"));
        corpus.push(format!("{beh}{beh}{c}"));
    }
    for s in [
        "كتاب",
        "مدرسة",
        "مرحبا بالعالم",
        "الله",
        "سَلَامٌ",
        "سرعة ١٢٣",
        "سرعة 123",
        "123 سرعة",
        "١٢ ٣٤",
    ] {
        corpus.push(s.to_string());
    }

    let mut ts = Typesetter::new(Font::from_bytes(data.clone(), 0).expect("loads"));
    for text in &corpus {
        let l = ts.layout(text, 16.0, None);
        for line in &l.lines {
            for g in &line.glyphs {
                assert!(
                    covered.contains(&g.glyph_id),
                    "{text:?} lays out glyph id {} outside the declared \
                     charset's coverage",
                    g.glyph_id
                );
            }
        }
    }
}

/// The Arabic atlas is byte-reproducible from the same inputs on one
/// machine (R7), the same guarantee the ASCII fixture proves — over a
/// charset whose closure runs the full GSUB sweep.
#[test]
fn arabic_atlas_double_run_is_byte_identical() {
    if !tool_available() {
        return;
    }
    let a = generate(&arabic_spec()).expect("first run");
    let b = generate(&arabic_spec()).expect("second run");
    assert_eq!(a.image_png, b.image_png, "atlas.png must be byte-identical");
    assert_eq!(
        a.metrics.to_bytes(),
        b.metrics.to_bytes(),
        "atlas.metrics must be byte-identical"
    );
}

/// The largest channel movement a reproduced atlas may carry: one step,
/// the smallest difference an 8-bit channel can express.
const MAX_CHANNEL_STEP: u8 = 1;

/// The largest share of an atlas's pixels that may differ at all, as a
/// fraction. Measured worst case across all eight committed fixtures on
/// both architectures: 4 pixels of 65536, which is 0.006 %.
const MAX_DIFFERING_PIXEL_RATIO: f64 = 0.001;

/// Decodes an atlas PNG to raw samples, with its dimensions and its
/// channel count. Comparing decoded samples rather than PNG bytes keeps
/// the assertion about the atlas rather than about the encoder.
fn decode_atlas(png_bytes: &[u8], which: &str) -> (Vec<u8>, (u32, u32), usize) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap_or_else(|e| panic!("{which} atlas.png has a readable PNG header: {e}"));
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .expect("atlas.png fits in memory")
    ];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|e| panic!("{which} atlas.png decodes: {e}"));
    buf.truncate(info.buffer_size());
    let pixels = (info.width as usize) * (info.height as usize);
    assert!(pixels > 0, "{which} atlas.png is not empty");
    let channels = buf.len() / pixels;
    (buf, (info.width, info.height), channels)
}

/// Asserts a freshly generated atlas reproduces the committed one (R7).
///
/// The metrics blob — packing, per-glyph boxes, atlas parameters and
/// generator provenance — is compared byte for byte, because it is
/// byte-identical on every machine measured.
///
/// The image is not compared byte for byte, because it cannot be:
/// `msdf-atlas-gen`'s floating-point arithmetic differs between CPU
/// architectures, so the committed Bold fixture decodes 4 pixels apart
/// between arm64 and x86_64, each by a single channel step (story #654).
/// Byte-identity there asserts a property no committed fixture can hold
/// on two architectures at once, and fails on whichever fixture the
/// drift happens to land on.
///
/// The two bounds below admit that noise and nothing else. A generator
/// that re-rasterises a glyph, moves the packing, or changes the
/// distance range moves some channel by far more than one step; a
/// systematic shift of the whole field moves far more than 0.1 % of the
/// pixels. Same-machine determinism is still asserted byte for byte, by
/// `double_run_is_byte_identical` and its Arabic twin.
fn assert_atlas_reproduces(committed: &AtlasBundle, fresh: &AtlasBundle, face: &str) {
    assert_eq!(
        committed.metrics.to_bytes(),
        fresh.metrics.to_bytes(),
        "committed {face} atlas.metrics no longer reproducible (R7) — if the \
         toolchain legitimately changed, regenerate the fixture and record why"
    );

    let (a, a_dim, channels) = decode_atlas(&committed.image_png, "committed");
    let (b, b_dim, _) = decode_atlas(&fresh.image_png, "fresh");
    assert_eq!(
        a_dim, b_dim,
        "committed {face} atlas.png changed dimensions (R7) — a relayout, not \
         cross-architecture noise; regenerate the fixture and record why"
    );
    assert_eq!(
        a.len(),
        b.len(),
        "{face}: equal dimensions, equal sample count"
    );

    let mut differing = 0usize;
    let mut peak = 0u8;
    for (p, q) in a.chunks(channels).zip(b.chunks(channels)) {
        let mut moved = false;
        for (x, y) in p.iter().zip(q) {
            let delta = x.abs_diff(*y);
            peak = peak.max(delta);
            moved |= delta > 0;
        }
        differing += usize::from(moved);
    }

    let pixels = (a_dim.0 as usize) * (a_dim.1 as usize);
    let budget = (pixels as f64 * MAX_DIFFERING_PIXEL_RATIO) as usize;
    assert!(
        peak <= MAX_CHANNEL_STEP,
        "committed {face} atlas.png no longer reproducible (R7): a channel moved \
         by {peak}, over the {MAX_CHANNEL_STEP}-step bound that admits only \
         cross-architecture noise. If the toolchain legitimately changed, \
         regenerate the fixture and record why"
    );
    assert!(
        differing <= budget,
        "committed {face} atlas.png no longer reproducible (R7): {differing} of \
         {pixels} pixels differ, over the {budget}-pixel budget. Each moved by at \
         most one step, so this is a systematic shift rather than a re-rasterised \
         glyph. If the toolchain legitimately changed, regenerate the fixture and \
         record why"
    );
}

#[test]
fn committed_ascii_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(ASCII_FIXTURE_DIR)).expect(
        "committed fixture loads — regenerate with `cargo test -p dashscene-typeset \
         --test atlas_pipeline -- --ignored regenerate_committed_ascii_fixture`",
    );
    let fresh = shared_ascii_bundle();
    assert_atlas_reproduces(&committed, fresh, "Regular");
}

/// The Arabic committed fixture is reproducible across machines, the
/// same guarantee `committed_ascii_fixture_is_reproducible` proves —
/// over a charset whose closure runs the full GSUB sweep. Generated on
/// macOS by `regenerate_committed_arabic_fixture`, byte-compared on the
/// CI atlas-repro runner (Linux).
#[test]
fn committed_arabic_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(ARABIC_FIXTURE_DIR)).expect(
        "committed Arabic fixture loads — regenerate with `cargo test -p dashscene-typeset \
         --test atlas_pipeline -- --ignored regenerate_committed_arabic_fixture`",
    );
    let fresh = generate(&arabic_spec()).expect("pipeline runs over the Arabic fixture");
    assert_atlas_reproduces(&committed, &fresh, "Arabic");
}

/// The committed SemiBold ASCII fixture is reproducible, exactly as
/// `committed_ascii_fixture_is_reproducible` proves for Regular — same
/// charset and parameters, the SemiBold face (story #368).
#[test]
fn committed_ascii_semibold_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(ASCII_SEMIBOLD_FIXTURE_DIR)).expect(
        "committed SemiBold fixture loads — regenerate with `cargo test -p dashscene-typeset \
         --test atlas_pipeline -- --ignored regenerate_committed_ascii_semibold_fixture`",
    );
    let fresh = generate(&ascii_semibold_spec()).expect("pipeline runs over the SemiBold face");
    assert_atlas_reproduces(&committed, &fresh, "SemiBold");
}

/// The committed Bold ASCII fixture is reproducible (story #368).
#[test]
fn committed_ascii_bold_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(ASCII_BOLD_FIXTURE_DIR)).expect(
        "committed Bold fixture loads — regenerate with `cargo test -p dashscene-typeset \
         --test atlas_pipeline -- --ignored regenerate_committed_ascii_bold_fixture`",
    );
    let fresh = generate(&ascii_bold_spec()).expect("pipeline runs over the Bold face");
    assert_atlas_reproduces(&committed, &fresh, "Bold");
}

/// The three ASCII weights are distinct rasterizations of one charset:
/// each covers the same 95 printable ASCII characters, and a stem-heavy
/// glyph is measurably wider at each heavier weight. A fixture accidentally
/// baked from the wrong face would pass both reproducibility tests above
/// and fail here (story #368).
#[test]
fn the_three_ascii_weights_are_distinct_faces() {
    let weights = [
        (ASCII_FIXTURE_DIR, FONT, "Regular"),
        (ASCII_SEMIBOLD_FIXTURE_DIR, FONT_SEMIBOLD, "SemiBold"),
        (ASCII_BOLD_FIXTURE_DIR, FONT_BOLD, "Bold"),
    ];
    let mut advances = Vec::new();
    for (dir, font_path, name) in weights {
        let bundle = AtlasBundle::load_from_dir(&PathBuf::from(dir))
            .unwrap_or_else(|e| panic!("committed {name} fixture loads: {e}"));
        // Same charset, so the same glyph count as the Regular fixture:
        // 95 ASCII + .notdef + the three two-character Latin ligatures.
        assert_eq!(
            bundle.metrics.glyphs.len(),
            99,
            "{name} atlas covers the same charset as Regular"
        );
        assert!(bundle.metrics.missing_codepoints.is_empty());
        // 'H' — a two-stem glyph, so its advance grows with stem weight.
        let data = std::fs::read(font_path).expect("fixture font present");
        let gid = ttf_parser::Face::parse(&data, 0)
            .unwrap()
            .glyph_index('H')
            .unwrap()
            .0;
        let g = bundle
            .metrics
            .glyphs
            .iter()
            .find(|g| g.glyph_id == gid)
            .unwrap_or_else(|| panic!("{name} atlas covers 'H'"));
        advances.push((name, g.advance_units));
    }
    assert!(
        advances[0].1 < advances[1].1 && advances[1].1 < advances[2].1,
        "'H' must advance wider at each heavier weight, got {advances:?}"
    );
}

/// Rewrites the committed ASCII fixture from the current pipeline.
/// Ignored: run it only after a deliberate parameter or toolchain
/// change, then commit the result with a note recording why.
#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_ascii_fixture() {
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(ASCII_FIXTURE_DIR))
        .expect("write fixture");
    println!(
        "wrote {ASCII_FIXTURE_DIR} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}

/// Rewrites the committed Arabic fixture (the E2 golden's atlas) from
/// the current pipeline. Ignored, like the ASCII regenerator.
#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_arabic_fixture() {
    let bundle = generate(&arabic_spec()).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(ARABIC_FIXTURE_DIR))
        .expect("write fixture");
    println!(
        "wrote {ARABIC_FIXTURE_DIR} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}

/// Rewrites the committed SemiBold ASCII fixture (story #368). Ignored,
/// like the Regular regenerator.
#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_ascii_semibold_fixture() {
    let bundle = generate(&ascii_semibold_spec()).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(ASCII_SEMIBOLD_FIXTURE_DIR))
        .expect("write fixture");
    println!(
        "wrote {ASCII_SEMIBOLD_FIXTURE_DIR} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}

/// Rewrites the committed Bold ASCII fixture (story #368). Ignored, like
/// the Regular regenerator.
#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_ascii_bold_fixture() {
    let bundle = generate(&ascii_bold_spec()).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(ASCII_BOLD_FIXTURE_DIR))
        .expect("write fixture");
    println!(
        "wrote {ASCII_BOLD_FIXTURE_DIR} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}

// ------------------------------------------------------------- Inter (#385)
//
// Four more fixtures, one per (script, weight), for the family real Figma
// files are authored in. They are written against shared helpers rather than
// spelled out one function per fixture like the Noto four above: the four
// differ only in face and directory, and eight more copies of the same body
// would bury that. The Noto tests are deliberately left as they are — they
// are what the E7 gate's atlases are checked by, and this story does not
// rewrite them.

/// Loads a committed fixture, regenerates it from `spec`, and compares
/// both files — the R7 guarantee `committed_ascii_fixture_is_reproducible`
/// states for the Regular Noto fixture, under the bounds
/// [`assert_atlas_reproduces`] documents. `regenerator` names the ignored
/// test that rewrites this fixture, so a failure says how to fix itself.
fn assert_fixture_reproducible(dir: &str, spec: &AtlasSpec, regenerator: &str) {
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(dir)).unwrap_or_else(|e| {
        panic!(
            "committed fixture {dir} loads ({e}) — regenerate with `cargo test \
             -p dashscene-typeset --test atlas_pipeline -- --ignored {regenerator}`"
        )
    });
    let fresh = generate(spec).expect("pipeline runs over the committed face");
    assert_atlas_reproduces(&committed, &fresh, dir);
}

/// Rewrites a committed fixture from the current pipeline. Called only by the
/// `#[ignore]`d regenerators, for the reason the Noto regenerators give: run
/// one after a deliberate parameter or toolchain change, then commit the
/// result with a note recording why.
fn regenerate_fixture(dir: &str, spec: &AtlasSpec) {
    let bundle = generate(spec).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(dir))
        .expect("write fixture");
    println!(
        "wrote {dir} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}

#[test]
fn committed_inter_ascii_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    assert_fixture_reproducible(
        INTER_ASCII_FIXTURE_DIR,
        &inter_ascii_spec(),
        "regenerate_committed_inter_ascii_fixture",
    );
}

#[test]
fn committed_inter_ascii_medium_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    assert_fixture_reproducible(
        INTER_ASCII_MEDIUM_FIXTURE_DIR,
        &inter_ascii_medium_spec(),
        "regenerate_committed_inter_ascii_medium_fixture",
    );
}

#[test]
fn committed_inter_ascii_semibold_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    assert_fixture_reproducible(
        INTER_ASCII_SEMIBOLD_FIXTURE_DIR,
        &inter_ascii_semibold_spec(),
        "regenerate_committed_inter_ascii_semibold_fixture",
    );
}

#[test]
fn committed_inter_ascii_bold_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    assert_fixture_reproducible(
        INTER_ASCII_BOLD_FIXTURE_DIR,
        &inter_ascii_bold_spec(),
        "regenerate_committed_inter_ascii_bold_fixture",
    );
}

/// The four Inter faces are genuinely different faces, not one face committed
/// four times — the same guard `the_three_ascii_weights_are_distinct_faces`
/// puts on the Noto weights. A heavier weight advances 'H' wider.
#[test]
fn the_four_inter_weights_are_distinct_faces() {
    let advances: Vec<(&str, u16)> = [
        ("Regular", FONT_INTER),
        ("Medium", FONT_INTER_MEDIUM),
        ("SemiBold", FONT_INTER_SEMIBOLD),
        ("Bold", FONT_INTER_BOLD),
    ]
    .iter()
    .map(|(name, path)| {
        let data = common::font_data(path);
        let face = ttf_parser::Face::parse(&data, 0).expect("an Inter face parses");
        let gid = face.glyph_index('H').expect("the face covers 'H'");
        (
            *name,
            face.glyph_hor_advance(gid).expect("'H' has an advance"),
        )
    })
    .collect();
    assert!(
        advances.windows(2).all(|w| w[0].1 < w[1].1),
        "'H' must advance wider at each heavier Inter weight, got {advances:?}"
    );
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_inter_ascii_fixture() {
    regenerate_fixture(INTER_ASCII_FIXTURE_DIR, &inter_ascii_spec());
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_inter_ascii_medium_fixture() {
    regenerate_fixture(INTER_ASCII_MEDIUM_FIXTURE_DIR, &inter_ascii_medium_spec());
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_inter_ascii_semibold_fixture() {
    regenerate_fixture(
        INTER_ASCII_SEMIBOLD_FIXTURE_DIR,
        &inter_ascii_semibold_spec(),
    );
}

#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_inter_ascii_bold_fixture() {
    regenerate_fixture(INTER_ASCII_BOLD_FIXTURE_DIR, &inter_ascii_bold_spec());
}
