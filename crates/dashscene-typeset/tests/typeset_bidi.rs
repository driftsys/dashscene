//! Bidi pipeline acceptance (issue #32): UAX #9 level runs drive
//! direction-aware shaping, lines reorder for display, and RTL
//! paragraphs sit flush-right. Supersedes the v0.5 forced-LTR pin in
//! `typeset_latin.rs`. The committed fixture font has no Hebrew
//! glyphs, so RTL content shapes to `.notdef` (glyph id 0) — the
//! assertions read run structure and positions, never non-Latin
//! glyph ids.

use dashscene_typeset::text::{Font, Line, Typesetter};

mod common;

use common::FONT;

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

/// The x position of the line's one glyph with the given id.
fn x_of(line: &Line, glyph_id: u16) -> f32 {
    let mut hits = line.glyphs.iter().filter(|g| g.glyph_id == glyph_id);
    let x = hits.next().expect("glyph present").x;
    assert!(hits.next().is_none(), "glyph id not unique on the line");
    x
}

fn min_x(line: &Line) -> f32 {
    line.glyphs
        .iter()
        .map(|g| g.x)
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn embedded_digits_keep_ltr_order_in_rtl_text() {
    // Logical "א 12 גדה" displays as [ה ד ג ␣][1 2][␣ א]: the digits
    // stay in ascending order (the spike's mis-ordered-embedded-digits
    // finding — the bidi split runs before shaping), three of the four
    // Hebrew `.notdef`s land left of the digits and one lands right.
    let mut ts = typesetter();
    let l = ts.layout("א 12 גדה", 16.0, None);
    assert_eq!(l.lines.len(), 1);
    let line = &l.lines[0];
    let x1 = x_of(line, cmap('1'));
    let x2 = x_of(line, cmap('2'));
    assert!(x1 < x2, "digits must keep logical order: {x1} vs {x2}");
    let left = line
        .glyphs
        .iter()
        .filter(|g| g.glyph_id == 0 && g.x < x1)
        .count();
    let right = line
        .glyphs
        .iter()
        .filter(|g| g.glyph_id == 0 && g.x > x2)
        .count();
    assert_eq!(left, 3, "the trailing RTL word displays left of the digits");
    assert_eq!(
        right, 1,
        "the leading RTL word displays right of the digits"
    );
}

#[test]
fn rtl_base_places_an_ltr_segment_leftmost() {
    // Logical "אבג abc" (RTL base) displays as [a b c][␣ ג ב א]:
    // every Latin glyph sits left of every Hebrew `.notdef`.
    let mut ts = typesetter();
    let l = ts.layout("אבג abc", 16.0, None);
    let line = &l.lines[0];
    let xa = x_of(line, cmap('a'));
    let xb = x_of(line, cmap('b'));
    let xc = x_of(line, cmap('c'));
    assert!(xa < xb && xb < xc, "the LTR run keeps its internal order");
    let notdef_min = line
        .glyphs
        .iter()
        .filter(|g| g.glyph_id == 0)
        .map(|g| g.x)
        .fold(f32::INFINITY, f32::min);
    assert!(
        xc < notdef_min,
        "the LTR segment displays before (left of) the RTL text"
    );
}

#[test]
fn rtl_paragraph_sits_flush_right_within_the_wrap_width() {
    let mut ts = typesetter();
    let l = ts.layout("אבג", 16.0, Some(200.0));
    let line = &l.lines[0];
    assert!(line.width < 200.0, "sanity: the line fits the wrap width");
    assert!(
        (min_x(line) - (200.0 - line.width)).abs() < 1e-4,
        "RTL lines start at max_width − line width, not at x = 0"
    );
    // LTR stays flush-left.
    let ltr = ts.layout("abc", 16.0, Some(200.0));
    assert!(min_x(&ltr.lines[0]).abs() < 1e-4);
}

#[test]
fn rtl_wrap_keeps_each_line_flush_right() {
    let mut ts = typesetter();
    // A width that fits one word but not both.
    let one = ts.layout("אבג", 16.0, None).width;
    let full = ts.layout("אבג אבג", 16.0, None).width;
    let max = (one + full) / 2.0;
    let l = ts.layout("אבג אבג", 16.0, Some(max));
    assert_eq!(l.lines.len(), 2, "must wrap at the space");
    for line in &l.lines {
        assert!(
            (min_x(line) - (max - line.width)).abs() < 1e-4,
            "every line of an RTL paragraph shares the right edge"
        );
    }
}

#[test]
fn unconstrained_rtl_right_aligns_to_the_widest_line() {
    // With no wrap width the container is the layout's own width, so
    // an RTL paragraph still shares the wider LTR line's right edge.
    let mut ts = typesetter();
    let l = ts.layout("אב\nabcdef", 16.0, None);
    assert_eq!(l.lines.len(), 2);
    let (heb, lat) = (&l.lines[0], &l.lines[1]);
    assert!(lat.width > heb.width, "sanity: the LTR line sets the width");
    assert!((min_x(heb) - (l.width - heb.width)).abs() < 1e-4);
    assert!(min_x(lat).abs() < 1e-4);
}

#[test]
fn class_b_separators_split_paragraphs_within_a_chunk() {
    // A lone CR (like NEL and U+2029) is a UAX #9 block separator: it
    // ends a bidi paragraph exactly as '\n' ends a chunk, so no line
    // spans two paragraphs and each paragraph reorders under its own
    // base direction. The separator itself renders on no line.
    let mut ts = typesetter();
    let l = ts.layout("אבג\rabc", 16.0, None);
    assert_eq!(l.lines.len(), 2, "CR ends the RTL paragraph");
    assert_eq!(l.lines[0].glyphs.len(), 3, "Hebrew only, no separator");
    let latin = &l.lines[1];
    assert_eq!(latin.glyphs.len(), 3);
    let xa = x_of(latin, cmap('a'));
    let xb = x_of(latin, cmap('b'));
    let xc = x_of(latin, cmap('c'));
    assert!(xa < xb && xb < xc, "the LTR paragraph keeps logical order");
    assert!(l.lines[1].baseline_y > l.lines[0].baseline_y);
    // U+2029 (a multi-byte separator) splits identically.
    let ps = ts.layout("אבג\u{2029}abc", 16.0, None);
    assert_eq!(ps.lines.len(), 2);
    assert_eq!(ps.lines[0].glyphs.len(), 3);
    assert_eq!(ps.lines[1].glyphs.len(), 3);
}

#[test]
fn each_bidi_paragraph_aligns_under_its_own_direction() {
    // The flush-right shift is per bidi paragraph, not per '\n'
    // chunk: after a lone CR, the Hebrew paragraph sits flush-right
    // and the Latin paragraph stays flush-left at x = 0.
    let mut ts = typesetter();
    let natural = ts.layout("אבג\rabc", 16.0, None).width;
    let max = natural + 40.0;
    let l = ts.layout("אבג\rabc", 16.0, Some(max));
    assert_eq!(l.lines.len(), 2);
    let (heb, lat) = (&l.lines[0], &l.lines[1]);
    assert!(
        (min_x(heb) - (max - heb.width)).abs() < 1e-4,
        "the RTL paragraph sits flush-right"
    );
    assert!(min_x(lat).abs() < 1e-4, "the LTR paragraph stays at x = 0");
}

#[test]
fn empty_and_space_only_paragraphs_still_lay_out() {
    // Guards the empty-paragraph path (no bidi paragraph to resolve)
    // and the all-space path (a line with an empty glyph range), both
    // running through the alignment pass: the RTL lines around them
    // still sit flush-right.
    let mut ts = typesetter();
    let l = ts.layout("א\n\n \nב", 16.0, Some(100.0));
    assert_eq!(l.lines.len(), 4);
    assert!(l.lines[1].glyphs.is_empty());
    assert!(l.lines[2].glyphs.is_empty());
    assert_eq!(l.lines[3].glyphs.len(), 1);
    for line in [&l.lines[0], &l.lines[3]] {
        assert!(
            (min_x(line) - (100.0 - line.width)).abs() < 1e-4,
            "RTL lines around empty paragraphs still sit flush-right"
        );
    }
}
