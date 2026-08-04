//! The painter badge: a label naming the painter that drew the frame.
//!
//! # Why this is content and not host code
//!
//! The label is text, and text is staged as glyph runs by the solver the
//! scene injects. Only `crate::solver::ShowcaseSolver` carries a
//! typesetter and the atlas list, so a label authored by the host — which
//! holds no solver handle — would commit through a text-incapable path
//! and stage no glyph runs at all. It lives here for the same reason the
//! variant switch does (`crate::lib`, "What a scene tells the host about
//! input"): the crate that owns the arena owns the content.
//!
//! # Why a second root
//!
//! The badge is appended to a scene's root list rather than added to its
//! tree, so it takes no part in the scene's layout and cannot move
//! anything the scene is demonstrating. Roots stage in list order and the
//! last one's rects come last in the committed table, which is what
//! draws the badge above the content.
//!
//! # Why the pill's width is bound
//!
//! A tick that changes only a paint-only channel — opacity, a fill, or a
//! text prop by itself — commits through the cached-rect replay, and that
//! replay stages no glyph runs at all (`typography.rs`'s "Every signal
//! here drives layout, deliberately"). Announcing a painter through a
//! paint-only write would wipe every glyph run already staged in the
//! scene, this badge's own label included, rather than adding the
//! badge's run to what was already there.
//!
//! The root is therefore a flex container (`LayoutMode::Horizontal`) with
//! the label as its one child, and the same `backend` signal also drives
//! the root's `Channel::Width`. A width write on a container that has
//! children cannot patch one cached rect (`write_is_single_rect` in
//! `crates/dashlang/src/reactive.rs`), so it forces the tick to solve —
//! which is what re-stages every glyph run, the scene's own as well as
//! the badge's. The width this forces a solve toward is not a lever
//! pulled only for that effect: it is the pill's real content, sized to
//! the announced label's own length (see `pill_width` below).

use dashlang::{Channel, Node, Scene, node};
use dashscene_core::{AxisSizing, LayoutMode, TextAlign, TextAlignV};

use crate::resources::LATIN_FAMILY;
use crate::vocabulary::{palette, text_style};

/// The name the badge declares its signal under, so the host can find it
/// in a scene it did not build.
pub const BACKEND: &str = "backend";

/// The value naming `dashscene-skia`.
pub const SKIA: f32 = 1.0;

/// The value naming `dashscene-gpu`.
pub const GPU: f32 = 2.0;

/// The design extent every measurement below is expressed against, the
/// same convention the three scenes use.
const DESIGN: (f32, f32) = (960.0, 600.0);

/// How many design pixels one label character adds to the pill's width,
/// at `unit == 1.0`. Chosen against the label style's own 14 px, weight
/// 600 metrics, close enough that neither announced name clips.
const CHAR_ADVANCE: f32 = 10.0;

/// The constant term of `pill_width`'s linear fit, at `unit == 1.0`.
/// Picked so the longer name, `"dashscene-skia"` at 14 characters, lands
/// on the 190 px the pill was fixed at before it tracked its label's
/// length: `PILL_PADDING + 14.0 * CHAR_ADVANCE == 190.0`.
///
/// This is the fit's intercept and not the margin around the label. The
/// margin comes from `CHAR_ADVANCE` being wider than the label style's
/// real average advance, so the fit overshoots and the surplus appears
/// as space around the glyphs: measured at `unit == 1.0`,
/// `"dashscene-skia"` advances about 108 px inside its 190 px pill and
/// `"dashscene-gpu"` about 107 px inside its 180 px pill, which leaves
/// about 82 px and about 74 px of space respectively, split evenly
/// because the label is centre-aligned.
const PILL_PADDING: f32 = 50.0;

/// The pill's width for `value`, so it tracks its label's length rather
/// than a span sized for whichever name happens to be longest. Also the
/// write this module binds `Channel::Width` to — see "Why the pill's
/// width is bound" above for why the bound channel has to be one that
/// means something.
fn pill_width(value: f32, unit: f32) -> f32 {
    (PILL_PADDING + label(value).chars().count() as f32 * CHAR_ADVANCE) * unit
}

/// The text a signal value names. `0.0` is the state before any painter
/// has been announced, and renders nothing: the still-image example never
/// writes the signal, so this is what keeps the badge out of
/// `docs/images/showcase-surfaces.png`.
///
/// Public so the mapping can be asserted without building a scene, and so
/// the host's own values can be checked against it.
pub fn label(value: f32) -> String {
    if value == SKIA {
        "dashscene-skia".to_owned()
    } else if value == GPU {
        "dashscene-gpu".to_owned()
    } else {
        String::new()
    }
}

/// Declares the badge's signal on `scene` and returns the root that draws
/// it, sized against the drawable `width` and `height`.
///
/// The caller appends the returned node to its root list:
///
/// ```ignore
/// let label = badge::badge(&mut scene, width, height);
/// scene.roots([root, label]);
/// ```
pub fn badge(scene: &mut Scene, width: f32, height: f32) -> Node {
    let unit = (width / DESIGN.0).min(height / DESIGN.1);
    let backend = scene.signal_named(BACKEND, 0.0);

    node("backend-badge")
        .at(20.0 * unit, 16.0 * unit)
        // The authored width matches what the binding below seeds it to
        // at the signal's own initial value (`0.0`, unannounced), so this
        // reads correctly before any tick has run.
        .size(pill_width(0.0, unit), 30.0 * unit)
        .mode(LayoutMode::Horizontal)
        .fill(palette::PANEL)
        .corners(8.0 * unit)
        // Layout-affecting, and deliberately so — see "Why the pill's
        // width is bound" above. The root has a child, so this write
        // cannot patch a single cached rect and forces the tick to
        // solve, which is what re-stages every glyph run in the scene
        // rather than leaving the paint-only writes below to wipe them.
        .bind(
            Channel::Width,
            backend.map(move |value| pill_width(value, unit)),
        )
        // Group opacity is paint-only, so raising and lowering the badge
        // reflows nothing.
        .bind(
            Channel::Opacity,
            backend.map(|value| if value > 0.0 { 1.0 } else { 0.0 }),
        )
        .child(
            node("backend-badge-label")
                .sizing_h(AxisSizing::Fill)
                .sizing_v(AxisSizing::Fill)
                .text_style({
                    let mut style = text_style(LATIN_FAMILY, 14.0 * unit, 600, palette::NEAR_WHITE);
                    style.text_align = TextAlign::Center;
                    style.text_align_v = TextAlignV::Center;
                    style
                })
                // A closure rather than the declarative transforms:
                // `Signal::map_range` and `Signal::clamp` are each
                // methods on `Signal<f32>` returning a `ScalarExpr` that
                // carries neither, so they do not compose into a
                // clamped remap.
                .bind_text(backend.map(label)),
        )
}
