//! Latin pipeline acceptance (issue #28): known-font run geometry,
//! greedy wrapping, baselines, and the shaped-run cache. Needs only
//! the committed corpus font — no external tool.

use dashscene_typeset::text::{Font, Typesetter};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

fn font_data() -> Vec<u8> {
    std::fs::read(FONT).expect("fixture font present")
}

fn typesetter() -> Typesetter {
    Typesetter::new(Font::from_bytes(font_data(), 0).expect("loads"))
}

fn cmap(c: char) -> u16 {
    let data = font_data();
    ttf_parser::Face::parse(&data, 0)
        .unwrap()
        .glyph_index(c)
        .unwrap()
        .0
}

fn hmtx(c: char) -> u16 {
    let data = font_data();
    let face = ttf_parser::Face::parse(&data, 0).unwrap();
    let gid = face.glyph_index(c).unwrap();
    face.glyph_hor_advance(gid).unwrap()
}

#[test]
fn newline_forces_a_break() {
    let mut ts = typesetter();
    let l = ts.layout("a\nb", 16.0, None);
    assert_eq!(l.lines.len(), 2);
    assert_eq!(l.lines[0].glyphs.len(), 1);
    assert_eq!(l.lines[1].glyphs.len(), 1);
    assert_eq!(l.lines[1].glyphs[0].glyph_id, cmap('b'));
}

#[test]
fn greedy_wrap_breaks_at_the_space() {
    let mut ts = typesetter();
    // A width that fits "Hello" but not "Hello world".
    let hello_width = ts.layout("Hello", 16.0, None).width;
    let full_width = ts.layout("Hello world", 16.0, None).width;
    let max = (hello_width + full_width) / 2.0;
    let l = ts.layout("Hello world", 16.0, Some(max));
    assert_eq!(l.lines.len(), 2, "must wrap at the space");
    assert_eq!(
        l.lines[0].glyphs.len(),
        5,
        "the broken-at space is on neither line"
    );
    assert_eq!(l.lines[1].glyphs.len(), 5);
    assert_eq!(l.lines[1].glyphs[0].glyph_id, cmap('w'));
    assert!(l.lines[0].width <= max);
    assert!((l.lines[0].width - hello_width).abs() < 1e-4);
}

#[test]
fn spaces_inside_a_line_still_advance() {
    let mut ts = typesetter();
    let spaced = ts.layout("a b", 16.0, Some(1000.0));
    let plain = ts.layout("ab", 16.0, Some(1000.0));
    assert_eq!(spaced.lines.len(), 1);
    assert_eq!(spaced.lines[0].glyphs.len(), 3, "the space glyph stays");
    assert!(spaced.width > plain.width);
}

#[test]
fn single_line_width_is_the_scaled_advance_sum() {
    let mut ts = typesetter();
    let size = 20.0f32;
    let l = ts.layout("ll", size, None);
    let upem = f32::from(ts.font().units_per_em());
    let expect = 2.0 * f32::from(hmtx('l')) * size / upem;
    assert!(
        (l.width - expect).abs() < 1e-4,
        "width {} vs hmtx-derived {expect}",
        l.width
    );
}

#[test]
fn baselines_advance_by_the_line_metric() {
    let mut ts = typesetter();
    let size = 16.0f32;
    let l = ts.layout("a\nb", size, None);
    let upem = f32::from(ts.font().units_per_em());
    let scale = size / upem;
    let ascent = f32::from(ts.font().ascender()) * scale;
    let advance = (f32::from(ts.font().ascender()) - f32::from(ts.font().descender())
        + f32::from(ts.font().line_gap()))
        * scale;
    assert!((l.lines[0].baseline_y - ascent).abs() < 1e-4);
    assert!((l.lines[1].baseline_y - (ascent + advance)).abs() < 1e-4);
    assert!((l.height - 2.0 * advance).abs() < 1e-4);
}

#[test]
fn a_word_wider_than_max_width_overflows() {
    let mut ts = typesetter();
    let l = ts.layout("incomprehensibilities", 16.0, Some(10.0));
    assert_eq!(l.lines.len(), 1, "no mid-word breaking in v0.5");
    assert!(l.width > 10.0);
}

#[test]
fn empty_text_lays_out_empty() {
    let mut ts = typesetter();
    let l = ts.layout("", 16.0, Some(100.0));
    assert!(l.lines.is_empty());
    assert_eq!(l.width, 0.0);
    assert_eq!(l.height, 0.0);
}

#[test]
fn cache_hits_across_sizes_and_counts() {
    let mut ts = typesetter();
    ts.layout("Speed", 16.0, None);
    ts.layout("Speed", 16.0, None);
    ts.layout("Speed", 32.0, Some(50.0)); // other size/width: same entry
    let s = ts.cache_stats();
    assert_eq!(s.misses, 1);
    assert_eq!(s.hits, 2);
    ts.layout("RPM", 16.0, None);
    assert_eq!(ts.cache_stats().misses, 2);
}

#[test]
fn offsets_reach_positioned_glyphs() {
    // Offset-less Latin glyphs sit exactly on the baseline — the
    // y-offset negation convention is exercised (and is a no-op here;
    // GPOS mark offsets get real coverage with the v0.6 Arabic story).
    let mut ts = typesetter();
    let l = ts.layout("ab", 16.0, None);
    for g in &l.lines[0].glyphs {
        assert_eq!(g.y, l.lines[0].baseline_y);
    }
}
