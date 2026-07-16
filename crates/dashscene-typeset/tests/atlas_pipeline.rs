//! Tool-dependent pipeline tests. They self-skip when msdf-atlas-gen
//! is absent, but fail when `DASHSCENE_REQUIRE_ATLAS_TOOL` is set —
//! CI sets it, so a skipped test cannot be reported as passing there.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use dashscene_typeset::atlas::{AtlasBundle, AtlasSpec, REQUIRE_TOOL_ENV, generate};

mod common;

use common::{FONT, FONT_ARABIC};

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ascii");

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
    let spec = AtlasSpec::new(PathBuf::from(FONT_ARABIC), arabic_charset());
    let bundle = generate(&spec).expect("pipeline runs over the Arabic fixture");
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

/// The Arabic atlas is byte-reproducible from the same inputs on one
/// machine (R7), the same guarantee the ASCII fixture proves — over a
/// charset whose closure runs the full GSUB sweep.
#[test]
fn arabic_atlas_double_run_is_byte_identical() {
    if !tool_available() {
        return;
    }
    let spec = AtlasSpec::new(PathBuf::from(FONT_ARABIC), arabic_charset());
    let a = generate(&spec).expect("first run");
    let b = generate(&spec).expect("second run");
    assert_eq!(a.image_png, b.image_png, "atlas.png must be byte-identical");
    assert_eq!(
        a.metrics.to_bytes(),
        b.metrics.to_bytes(),
        "atlas.metrics must be byte-identical"
    );
}

#[test]
fn committed_fixture_is_reproducible() {
    if !tool_available() {
        return;
    }
    let committed = AtlasBundle::load_from_dir(&PathBuf::from(FIXTURE_DIR)).expect(
        "committed fixture loads — regenerate with `cargo test -p dashscene-typeset \
         --test atlas_pipeline -- --ignored regenerate_committed_fixture`",
    );
    let fresh = shared_ascii_bundle();
    assert_eq!(
        committed.image_png, fresh.image_png,
        "committed atlas.png no longer reproducible (R7) — if the \
         toolchain legitimately changed, regenerate the fixture and \
         record why"
    );
    assert_eq!(committed.metrics.to_bytes(), fresh.metrics.to_bytes());
}

/// Rewrites the committed fixture from the current pipeline. Ignored:
/// run it only after a deliberate parameter or toolchain change, then
/// commit the result with a note recording why.
#[test]
#[ignore = "regenerates the committed fixture; run explicitly"]
fn regenerate_committed_fixture() {
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
    bundle
        .write_to_dir(&PathBuf::from(FIXTURE_DIR))
        .expect("write fixture");
    println!(
        "wrote {FIXTURE_DIR} ({} glyphs, {}x{})",
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}
