//! The additive text-shape knobs (story #310): `layout_with` honors a fixed
//! line height, letter spacing, and horizontal alignment while `layout` keeps
//! its signature and reproduces the previous behavior exactly (the E7 guard).

use dashscene_typeset::text::{TextAlign, TextShape, Typesetter};

mod common;

use common::FONT;

fn typesetter() -> Typesetter {
    common::typesetter(FONT)
}

/// E7 guard: `layout(t, s, w)` must be identical to `layout_with(t, s, w,
/// TextShape::default())` — every oracle/golden call site uses `layout`, so the
/// default shape must reproduce the pre-#310 output byte for byte.
#[test]
fn layout_equals_layout_with_default() {
    let mut a = typesetter();
    let mut b = typesetter();
    let plain = a.layout("Hello world of text", 24.0, Some(120.0));
    let with_default = b.layout_with(
        "Hello world of text",
        24.0,
        Some(120.0),
        TextShape::default(),
    );
    assert_eq!(plain, with_default);
}

/// A fixed line height overrides the intrinsic line advance: a two-line block
/// stacks by exactly the fixed height, and the total height is two of them.
#[test]
fn line_height_px_overrides_the_line_advance() {
    let mut ts = typesetter();
    let laid = ts.layout_with(
        "a\nb",
        24.0,
        None,
        TextShape {
            line_height_px: Some(50.0),
            ..Default::default()
        },
    );
    assert_eq!(laid.lines.len(), 2);
    assert!((laid.height - 100.0).abs() < 1e-3, "height {}", laid.height);
    let advance = laid.lines[1].baseline_y - laid.lines[0].baseline_y;
    assert!((advance - 50.0).abs() < 1e-3, "line advance {advance}");
}

/// A fixed line height places each baseline by Figma's model — the intrinsic
/// em box is centered within the fixed line box (half-leading), so the first
/// baseline sits at `ascent + (line_height - intrinsic) / 2`, not at the full
/// intrinsic ascent. Measured against Figma's own `GET /images` render by the
/// #332 import oracle: its fixed-18px-line-height frame put our whole run
/// 7 px below Figma's until the half-leading was applied.
#[test]
fn a_fixed_line_height_centers_the_em_box_half_leading() {
    let mut ts = typesetter();
    // The intrinsic single-line metrics at this size, read from the natural
    // layout: its height is the intrinsic advance and its baseline the ascent.
    let natural = ts.layout("ag", 24.0, None);
    let intrinsic = natural.height;
    let ascent = natural.lines[0].baseline_y;

    // A line height below the intrinsic advance (negative leading) lifts the
    // baseline; one above it (positive leading) lowers it — half each way.
    for line_height in [18.0f32, 50.0] {
        let fixed = ts.layout_with(
            "ag",
            24.0,
            None,
            TextShape {
                line_height_px: Some(line_height),
                ..Default::default()
            },
        );
        let expected = ascent + (line_height - intrinsic) / 2.0;
        assert!(
            (fixed.lines[0].baseline_y - expected).abs() < 1e-3,
            "line height {line_height}: baseline {} != ascent {ascent} + ({line_height} - {intrinsic})/2 = {expected}",
            fixed.lines[0].baseline_y,
        );
    }
}

/// Letter spacing widens the measured line by one tracking step per glyph, in
/// both the placement pen advance (`line.width`) and the layout width.
#[test]
fn letter_spacing_widens_the_measured_line() {
    let mut ts = typesetter();
    let base = ts.layout("abc", 24.0, None).lines[0].width;
    let wide = ts
        .layout_with(
            "abc",
            24.0,
            None,
            TextShape {
                letter_spacing: 4.0,
                ..Default::default()
            },
        )
        .lines[0]
        .width;
    // "abc" is three glyphs; each advances by an extra 4.0.
    assert!((wide - base - 12.0).abs() < 1e-3, "base {base} wide {wide}");
}

/// `ligatures_off` plumbs through `layout_with` to the shaping seam (story
/// #341). A Latin run already shapes with `liga`/`clig` off by default
/// (`docs/decisions/liga-clig-off-until-gsub-closure.md`), so setting the
/// knob here is a no-op for this (non-Arabic) typesetter's output — the
/// override changing an Arabic-context run's posture is shape.rs's own unit
/// tests. This pins the wiring: the knob must not panic or otherwise disturb
/// output for the common case.
#[test]
fn ligatures_off_plumbs_through_without_disturbing_latin_output() {
    let mut a = typesetter();
    let mut b = typesetter();
    let base = a.layout("office", 24.0, None);
    let with_flag = b.layout_with(
        "office",
        24.0,
        None,
        TextShape {
            ligatures_off: true,
            ..Default::default()
        },
    );
    assert_eq!(base, with_flag);
}

/// CENTER and RIGHT shift the line within the container width; the default
/// (LEFT) leaves an LTR line flush at x = 0.
#[test]
fn center_and_right_shift_the_line_within_the_container() {
    let mut ts = typesetter();
    let natural = ts.layout("abc", 24.0, None).lines[0].width;
    let container = natural + 100.0;

    let left = ts.layout_with("abc", 24.0, Some(container), TextShape::default());
    assert!(
        left.lines[0].glyphs[0].x.abs() < 1e-3,
        "LTR default stays flush-left at x=0, got {}",
        left.lines[0].glyphs[0].x
    );

    let center = ts.layout_with(
        "abc",
        24.0,
        Some(container),
        TextShape {
            align: TextAlign::Center,
            ..Default::default()
        },
    );
    let expected_center = (container - center.lines[0].width) / 2.0;
    assert!(
        (center.lines[0].glyphs[0].x - expected_center).abs() < 1e-3,
        "center first-glyph x {} vs {expected_center}",
        center.lines[0].glyphs[0].x
    );

    let right = ts.layout_with(
        "abc",
        24.0,
        Some(container),
        TextShape {
            align: TextAlign::Right,
            ..Default::default()
        },
    );
    let expected_right = container - right.lines[0].width;
    assert!(
        (right.lines[0].glyphs[0].x - expected_right).abs() < 1e-3,
        "right first-glyph x {} vs {expected_right}",
        right.lines[0].glyphs[0].x
    );
}
