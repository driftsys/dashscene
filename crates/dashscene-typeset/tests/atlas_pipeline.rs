//! Tool-dependent pipeline tests. They self-skip when msdf-atlas-gen
//! is absent, but fail when `DASHSCENE_REQUIRE_ATLAS_TOOL` is set —
//! CI sets it, so a skip can never masquerade as green there.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dashscene_typeset::atlas::{AtlasBundle, AtlasSpec, generate};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// Returns false (and prints why) when the pinned tool is unavailable
/// and the environment tolerates that; panics when CI demands it.
fn tool_available() -> bool {
    match dashscene_typeset::atlas::find_tool_checked() {
        Ok(_) => true,
        Err(e) if std::env::var_os("DASHSCENE_REQUIRE_ATLAS_TOOL").is_some() => {
            panic!("DASHSCENE_REQUIRE_ATLAS_TOOL is set but: {e}")
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

fn ascii_spec() -> AtlasSpec {
    AtlasSpec::new(PathBuf::from(FONT), ascii_charset())
}

#[test]
fn generates_ascii_atlas_with_full_coverage() {
    if !tool_available() {
        return;
    }
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
    let m = &bundle.metrics;
    assert!(m.missing_codepoints.is_empty());
    // 95 ASCII chars resolve to 95 distinct gids, plus .notdef.
    assert_eq!(m.glyphs.len(), 96);
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
    let a = generate(&ascii_spec()).expect("first run");
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
    let bundle = generate(&ascii_spec()).expect("pipeline runs");
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
    let mut spec = ascii_spec();
    spec.charset.insert('\u{0710}'); // Syriac alaph — not in Noto Sans LGC
    let bundle = generate(&spec).expect("pipeline runs");
    assert_eq!(bundle.metrics.missing_codepoints, vec![0x0710]);
}
