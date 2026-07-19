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

/// Letter spacing widens the measured line by one tracking step per glyph
/// *gap* — n - 1 steps for n glyphs, not n. The placement pen
/// (`layout::place`) still advances past every glyph including the last, but
/// the reported width — `line.width` and the layout width — drops that
/// final step: Figma excludes it from the box extent and alignment (debt
/// #336; the import oracle's `import-text-axes` frame measures this against
/// Figma's own `GET /images` render). This test previously pinned the #310
/// "n glyphs, n steps" contract; #336 corrects it.
#[test]
fn letter_spacing_widens_the_measured_line_by_n_minus_one_steps() {
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
    // "abc" is three glyphs, two internal gaps; the trailing step after 'c'
    // is excluded.
    assert!((wide - base - 8.0).abs() < 1e-3, "base {base} wide {wide}");
}

/// A wrapped line drops its *own* trailing tracking step, not only the
/// paragraph's last line (story #336): "aaa" (3 glyphs, not the last line)
/// still gets 3 - 1 = 2 steps, and "b" (1 glyph, the last line) gets
/// 1 - 1 = 0 steps — each `Line.width` is corrected independently, since
/// each line gets its own alignment shift and can be the widest line the
/// HUG box sizes to.
#[test]
fn each_wrapped_line_drops_its_own_trailing_step() {
    let mut ts = typesetter();
    let natural_aaa = ts.layout("aaa", 24.0, None).lines[0].width;
    let natural_b = ts.layout("b", 24.0, None).lines[0].width;

    // Wide enough for "aaa" plus its 2 tracking steps alone; narrow enough
    // that appending " b" overflows — forces a wrap right after "aaa".
    let max_width = natural_aaa + 2.0 * 4.0 + 1.0;
    let laid = ts.layout_with(
        "aaa b",
        24.0,
        Some(max_width),
        TextShape {
            letter_spacing: 4.0,
            ..Default::default()
        },
    );
    assert_eq!(laid.lines.len(), 2, "expected a wrap into two lines");
    assert!(
        (laid.lines[0].width - (natural_aaa + 2.0 * 4.0)).abs() < 1e-3,
        "line 0 (\"aaa\", 3 glyphs) width {} != natural {natural_aaa} + 2 steps",
        laid.lines[0].width
    );
    assert!(
        (laid.lines[1].width - natural_b).abs() < 1e-3,
        "line 1 (\"b\", 1 glyph) width {} != natural {natural_b} + 0 steps",
        laid.lines[1].width
    );
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
