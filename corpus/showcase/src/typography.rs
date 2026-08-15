//! `typography` — MSDF text, in Latin and in Arabic, with one string driven by
//! a signal.
//!
//! # What this scene is the proof of
//!
//! Text is the part of the vocabulary that reaches furthest down the stack: a
//! run is shaped by `dashscene-typeset`, measured through the engine's measure
//! seam, staged as glyph runs by the solver at commit, and drawn as MSDF atlas
//! quads by the painter. Arabic adds bidi resolution, joining forms and mark
//! positioning to that path, and a signal-driven string re-runs the whole of
//! it every time the value changes — which is why issue #225 (full UAX #9 bidi
//! resolution repeated per `layout()` call) was made a prerequisite for the
//! slice. This scene is what that fix exists for.
//!
//! # Every signal here drives layout, deliberately
//!
//! A tick that changes only paint props commits through `dashlang`'s rect
//! replay, which **used to** stage no glyph runs at all, so the text vanished
//! for exactly as long as the paint-only frames lasted. Every binding in this
//! scene therefore writes a layout-affecting channel, and the one text binding
//! shares its signal with a width that reflows.
//!
//! **Issue #621 fixed the replay**, so removing the width binding no longer
//! blanks the readout — the sentence that used to stand here said it did, and
//! it was true when written. The binding stays because it is part of what this
//! scene demonstrates, not because text depends on it.
//!
//! # The strings
//!
//! The Arabic strings are the v0.6 golden's, verbatim
//! (`goldens/tooling/tests/v06_arabic.rs`). They are reused rather than
//! re-authored because the committed Arabic atlas
//! (`corpus/atlas/arabic`) was baked for exactly their glyph closure — a new
//! sentence would need a new atlas bake to render at all. Every Latin string
//! is printable ASCII for the same reason: the committed Inter atlases carry
//! the ASCII set.

use dashlang::{Channel, FormatSpec, LiveScene, Scene, Spring, node};
use dashscene_core::{Arena, CrossAxisAlign, LayoutMode, TextAlign, TextAlignV};

use crate::badge;
use crate::resources::{self, ARABIC_FAMILY, LATIN_FAMILY};
use crate::vocabulary::{palette, text_style};
use dashscene_engine::TaffySolver;

/// The one signal, named so the pulse can find it in a scene it did not build.
///
/// It carries a road speed in km/h rather than a normalized 0..1 fraction, so
/// the readout's text binding is a plain [`FormatSpec`] over the signal's own
/// value. `FormatSpec` is the only text transform that is not a Rust closure,
/// which makes it the one a compiled `.dsb` could carry, and choosing the
/// signal's units is what keeps the binding in that subset.
pub const LEVEL: &str = "typography.level";

/// The readout's full-scale value.
const TOP_SPEED: f32 = 240.0;

const DESIGN: (f32, f32) = (960.0, 600.0);

/// "as-salaamu alaikum" — the second lam is directly followed by an alef, so
/// it shapes to a lam-alef ligature and every letter takes its joining form.
const ARABIC_BANNER: &str = "السلام عليكم";
/// "marhaban" (welcome) with harakat, which the typesetter stacks above the
/// letters through GPOS.
const ARABIC_WORD: &str = "مَرْحَبًا";
/// "sur'a 120" — the Arabic word makes the run's context Arabic, so the
/// authored European digits render with Arabic-Indic shapes.
const ARABIC_SPEED: &str = "سرعة 120";

const HEADING: &str = "dashscene";
const SUBHEADING: &str = "one document, one solver, one typesetter, interchangeable painters";
const BODY: &str = concat!(
    "The document carries intent and never results: no resolved x, y, width ",
    "or height, no rasterized pixels, and no glyph positions. This paragraph ",
    "wraps inside its own box, at the line height and letter spacing the ",
    "style asks for, and the painter moves none of it."
);
const CLIPPED: &str = "this line runs past the box that holds it, and the clip is what stops it";

pub fn build(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
    let (width, height) = (width as f32, height as f32);
    let unit = (width / DESIGN.0).min(height / DESIGN.1);

    let margin = 34.0 * unit;
    let gap = 18.0 * unit;
    let column = width - 2.0 * margin;
    let radius = 10.0 * unit;
    let gauge_height = 26.0 * unit;
    let gauge_track = column * 0.46;

    let mut scene = Scene::new();
    let level = scene.signal_named(LEVEL, 0.0);

    let root = node("typography")
        .size(width, height)
        .mode(LayoutMode::Vertical)
        .padding(margin, margin, margin, margin)
        .gap(gap)
        .fill(palette::NAVY)
        .child(
            node("type-heading")
                .size(column, 46.0 * unit)
                .text(HEADING)
                .text_style({
                    let mut style = text_style(LATIN_FAMILY, 40.0 * unit, 600, palette::NEAR_WHITE);
                    style.letter_spacing = -0.8 * unit;
                    style
                }),
        )
        .child(
            node("type-sub")
                .size(column, 26.0 * unit)
                .text(SUBHEADING)
                .text_style(text_style(LATIN_FAMILY, 17.0 * unit, 400, palette::SKY)),
        )
        .child(
            // The Arabic panel. Each run asks for the Arabic family by name;
            // coverage outranks the request in the cascade, so an Arabic run
            // would reach that family even if it asked for Inter — naming it
            // keeps the intent in the scene rather than in the fallback.
            node("arabic-panel")
                .size(column, 78.0 * unit)
                .mode(LayoutMode::Horizontal)
                .gap(gap)
                .padding(gap, gap * 0.6, gap, gap * 0.6)
                .cross_align(CrossAxisAlign::Center)
                .fill(palette::PANEL)
                .corners(radius)
                .child(
                    node("arabic-banner")
                        .size(column * 0.42, 46.0 * unit)
                        .text(ARABIC_BANNER)
                        .text_style(text_style(
                            ARABIC_FAMILY,
                            26.0 * unit,
                            400,
                            palette::NEAR_WHITE,
                        )),
                )
                .child(
                    node("arabic-word")
                        .size(column * 0.22, 52.0 * unit)
                        .text(ARABIC_WORD)
                        .text_style(text_style(ARABIC_FAMILY, 34.0 * unit, 400, palette::AMBER)),
                )
                .child(
                    node("arabic-speed")
                        .size(column * 0.24, 52.0 * unit)
                        .text(ARABIC_SPEED)
                        .text_style(text_style(ARABIC_FAMILY, 34.0 * unit, 400, palette::TEAL)),
                ),
        )
        .child(
            node("readout")
                .size(column, 44.0 * unit)
                .mode(LayoutMode::Horizontal)
                .gap(gap)
                .cross_align(CrossAxisAlign::Center)
                .child(
                    node("gauge-track")
                        .size(gauge_track, gauge_height)
                        .fill(palette::PANEL)
                        .corners(radius)
                        // The bar's own width is what the signal drives. It
                        // sits under a flex parent, so the write is
                        // layout-affecting and the frame re-solves — which is
                        // what keeps the readout's glyphs staged.
                        .child(
                            node("gauge-fill")
                                .size(gauge_track * 0.08, gauge_height)
                                .fill(palette::AMBER)
                                .corners(radius)
                                .bind(
                                    Channel::Width,
                                    level.map_range(
                                        0.0,
                                        TOP_SPEED,
                                        gauge_track * 0.08,
                                        gauge_track,
                                    ),
                                )
                                .smooth(Channel::Width, Spring::critically_damped(0.5)),
                        ),
                )
                // The same signal, rendered as a string. `FormatSpec` is the
                // one text transform that is not a closure, so this binding is
                // the shape a compiled document could carry.
                .child(
                    node("readout-value")
                        .size(column * 0.4, 36.0 * unit)
                        .text_style({
                            let mut style =
                                text_style(LATIN_FAMILY, 28.0 * unit, 600, palette::NEAR_WHITE);
                            style.text_align_v = TextAlignV::Center;
                            style
                        })
                        .bind_text(level.format(FormatSpec::new("", 0, " km/h"))),
                ),
        )
        .child(
            // A wrapping paragraph, with the four v0.9 text axes set away from
            // their defaults so each is visible: a fixed line height, letter
            // spacing, and both alignments.
            node("type-body")
                .size(column * 0.72, 92.0 * unit)
                .text(BODY)
                .text_style({
                    let mut style = text_style(LATIN_FAMILY, 15.0 * unit, 400, palette::NEAR_WHITE);
                    style.line_height_px = Some(24.0 * unit);
                    style.letter_spacing = 0.3 * unit;
                    style.text_align = TextAlign::Left;
                    style.text_align_v = TextAlignV::Top;
                    style
                }),
        )
        .child(
            // Glyphs are clipped by the same resolved clip regions the rects
            // are.
            node("clip-box")
                .size(column * 0.62, 40.0 * unit)
                .fill(palette::PANEL)
                .corners(radius)
                .clip(true)
                .child(
                    node("clip-text")
                        .at(0.0, 0.0)
                        .size(column * 1.4, 40.0 * unit)
                        .text(CLIPPED)
                        .text_style({
                            let mut style =
                                text_style(LATIN_FAMILY, 20.0 * unit, 400, palette::AMBER);
                            style.text_align_v = TextAlignV::Center;
                            style
                        }),
                ),
        );

    let label = badge::badge(&mut scene, width, height);
    scene.roots([root, label]);
    scene.build_live(arena, Box::new(TaffySolver::owning(resources::text())))
}

/// The scripted phase: drive `level` between its two ends, so the bar reflows
/// and the readout's string changes with it.
pub fn pulse(live: &mut LiveScene, index: u64) {
    let Some(level) = live.signal_named(LEVEL) else {
        return;
    };
    // Four phases rather than two, so the readout shows more than one pair of
    // numbers and the spring is seen both accelerating and settling.
    let value = match index % 4 {
        0 => 0.0,
        1 => 0.45 * TOP_SPEED,
        2 => TOP_SPEED,
        _ => 0.7 * TOP_SPEED,
    };
    live.set(level, value);
}
