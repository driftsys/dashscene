//! `surfaces` — the paint half of the v0 vocabulary on one screen.
//!
//! A titled header over a gallery of sixteen tiles, each tile showing one
//! construct, with a frosted panel that slides across them so its backdrop
//! blur is seen against several different backdrops rather than against one.
//! What each tile is for is named at its own child below.
//!
//! # Every animated channel here is layout-affecting, deliberately
//!
//! A tick that writes only paint props commits through `dashlang`'s rect
//! replay, and that replay stages no glyph runs at all, so the header's title
//! would blink out for as long as such a frame lasted (the defect recorded in
//! `README.md`). Both springs in this scene therefore drive a channel that
//! re-solves: the header's width, on a node with children, and the frosted
//! panel's x, likewise. Group opacity and the frosted panel's own alpha are
//! set once rather than animated, for the same reason.

use dashlang::{Channel, LiveScene, Scene, Spring, node};
use dashpaint::{GradientKind, Mat23, ScaleMode, StrokeAlign};
use dashscene_core::{Arena, CrossAxisAlign, LayoutMode, Prop, TextAlignV};

use crate::resources::{self, LATIN_FAMILY};
use crate::solver::ShowcaseSolver;
use crate::vocabulary::{
    Painting, backdrop_blur, corners, diagonal_gradient, drop_shadow, gradient, image_crop,
    image_fill, inner_shadow, palette, rgba, shape_field, stroke, text_style,
};

/// The one signal, named so the pulse can find it in a scene it did not build.
pub const SWEEP: &str = "surfaces.sweep";

/// The design the proportions are written against. The scene is laid out at
/// this size and scaled to the drawable, so a tile keeps its proportions on a
/// high-density display instead of shrinking to a corner of it.
const DESIGN: (f32, f32) = (960.0, 600.0);

/// The gallery's packing. Sixteen tiles in six columns leaves the last row
/// four wide, and the tile size is derived from these two numbers so the wrap
/// never packs a seventh tile into a line or spills a fourth line past the
/// bottom of the window.
const COLUMNS: f32 = 6.0;
const ROWS: f32 = 3.0;

const TITLE: &str = "dashscene";
const SUBTITLE: &str = "the v0 paint vocabulary, drawn by the Skia reference painter";

pub fn build(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
    let (width, height) = (width as f32, height as f32);
    // One scale for both axes, so nothing is stretched; the gallery's flex
    // takes whatever aspect ratio is left over.
    let unit = (width / DESIGN.0).min(height / DESIGN.1);

    let margin = 28.0 * unit;
    let header_height = 74.0 * unit;
    let gap = 16.0 * unit;
    let gallery_top = margin + header_height + gap;
    let gallery_width = width - 2.0 * margin;
    let gallery_height = height - gallery_top - margin;
    // Both axes bound the tile: the width so exactly `COLUMNS` fit in a line,
    // the height so `ROWS` lines fit in the gallery. A narrower window makes
    // the tile height-bound, which packs *more* per line and therefore fewer
    // lines — the direction that cannot overflow.
    let tile = (((gallery_width - gap * (COLUMNS - 1.0)) / COLUMNS)
        .min((gallery_height - gap * (ROWS - 1.0)) / ROWS))
    .floor()
    .max(1.0);
    let radius = 10.0 * unit;

    // A baked field paints at the size it was baked, so the node carrying it
    // is sized to the bake and inset to sit in the middle of its tile.
    let star = (tile * 0.66).round();
    let star_inset = ((tile - star) * 0.5).round();

    let frost_width = gallery_width * 0.24;
    let frost_height = gallery_height * 0.46;
    let frost_left = margin + gallery_width * 0.05;
    let frost_right = margin + gallery_width - frost_width - gallery_width * 0.05;
    let frost_top = gallery_top + gallery_height * 0.27;

    let mut scene = Scene::new();
    let sweep = scene.signal_named(SWEEP, 0.0);

    let plain = |name: &str| node(name).size(tile, tile);

    let root = node("surfaces")
        .size(width, height)
        .child(node("backdrop").size(width, height))
        .child(
            node("header")
                .at(margin, margin)
                .size(gallery_width, header_height)
                .mode(LayoutMode::Horizontal)
                .cross_align(CrossAxisAlign::Center)
                .padding(gap * 1.5, 0.0, gap * 1.5, 0.0)
                .gap(gap)
                // The header has children, so a width write redistributes them
                // and the frame re-solves. That is what keeps the title's
                // glyphs staged while the scene animates.
                .bind(
                    Channel::Width,
                    sweep.map_range(0.0, 1.0, gallery_width, gallery_width * 0.72),
                )
                .smooth(Channel::Width, Spring::critically_damped(0.45))
                .child(node("header-title").size(gallery_width * 0.24, header_height * 0.62))
                .child(node("header-subtitle").size(gallery_width * 0.62, header_height * 0.42)),
        )
        .child(
            node("gallery")
                .at(margin, gallery_top)
                .size(gallery_width, gallery_height)
                .mode(LayoutMode::Wrap)
                .gap(gap)
                .cross_gap(gap)
                .cross_align(CrossAxisAlign::Start)
                // Fills and strokes: one stroke alignment per tile, at one
                // width, so the three read against each other.
                .child(plain("tile-solid"))
                .child(plain("tile-linear"))
                .child(plain("tile-radial"))
                .child(plain("tile-angular"))
                .child(plain("tile-diamond"))
                // Images, one tile per Figma scale mode. The `Fit` tile puts
                // the image on a short child so the letterbox the mode
                // produces has the tile's own fill behind it — on a square box
                // a square payload fits and fills identically.
                .child(plain("tile-image-fill"))
                .child(
                    plain("tile-image-fit")
                        .child(node("fit-image").at(0.0, tile * 0.2).size(tile, tile * 0.6)),
                )
                .child(plain("tile-image-crop"))
                .child(plain("tile-image-tile"))
                // A baked vector field masking a gradient.
                .child(
                    plain("tile-vector").child(
                        node("vector-star")
                            .at(star_inset, star_inset)
                            .size(star, star),
                    ),
                )
                // Effects.
                .child(plain("tile-drop-shadow"))
                .child(plain("tile-inner-shadow"))
                // A clipping container whose child overflows it on every edge,
                // against a fully rounded box, so the clip is the circle.
                .child(
                    plain("tile-clip").child(
                        node("clip-overflow")
                            .at(-tile * 0.25, -tile * 0.25)
                            .size(tile * 1.5, tile * 1.5),
                    ),
                )
                // A mask sibling stencilling the sibling that follows it.
                .child(
                    plain("tile-mask")
                        .child(
                            node("mask-shape")
                                .at(tile * 0.12, tile * 0.12)
                                .size(tile * 0.76, tile * 0.76),
                        )
                        .child(node("mask-content").size(tile, tile)),
                )
                // A subtree at less than full alpha, with overlapping
                // children, which is what makes commit resolve a render-target
                // group rather than a per-rect alpha.
                .child(
                    plain("tile-group")
                        .child(
                            node("group-back")
                                .at(tile * 0.10, tile * 0.16)
                                .size(tile * 0.58, tile * 0.58),
                        )
                        .child(
                            node("group-front")
                                .at(tile * 0.34, tile * 0.30)
                                .size(tile * 0.58, tile * 0.58),
                        ),
                )
                // Corner radii on their own, two corners rounded and two
                // square, so the per-corner vocabulary is what the tile shows.
                .child(plain("tile-corners")),
        )
        // Last in document order, so it composites over the gallery — which is
        // what gives its backdrop blur something to read. It carries a child
        // for the same reason the header does: an x write on a node with
        // children re-solves.
        .child(
            node("frost")
                .at(frost_left, frost_top)
                .size(frost_width, frost_height)
                .bind(
                    Channel::X,
                    sweep.map_range(0.0, 1.0, frost_left, frost_right),
                )
                .smooth(Channel::X, Spring::critically_damped(0.55))
                .child(
                    node("frost-handle")
                        .at(frost_width * 0.3, frost_height * 0.86)
                        .size(frost_width * 0.4, 5.0 * unit),
                ),
        );

    scene.roots([root]);
    let live = scene.build_live(
        arena,
        Box::new(ShowcaseSolver::new(
            resources::new_typesetter(),
            resources::atlases(),
        )),
    );
    paint(arena, unit, radius, tile, star);
    live
}

/// Stages the paint vocabulary onto the tiles, by name.
fn paint(arena: &mut Arena, unit: f32, radius: f32, tile: f32, star_size: f32) {
    let star = resources::baked_star(star_size);
    let mut painting = Painting::open(arena);
    let photo = painting.add_image(resources::photo());
    let field = painting.add_image(star.atlas.clone());

    painting
        .set("backdrop", diagonal_gradient(palette::NAVY, palette::INK))
        .set(
            "header",
            gradient(GradientKind::Linear, palette::VIOLET, palette::SKY),
        )
        .set("header", corners(radius))
        .set(
            "header",
            drop_shadow(0.0, 6.0 * unit, 18.0 * unit, 0.0, rgba(0.0, 0.0, 0.0, 0.55)),
        )
        .set(
            "header",
            stroke(1.5 * unit, StrokeAlign::Inside, rgba(1.0, 1.0, 1.0, 0.35)),
        )
        .set("header-title", Prop::Text(TITLE.to_owned()))
        .set(
            "header-title",
            Prop::TextStyle({
                let mut style = text_style(LATIN_FAMILY, 34.0 * unit, 600, palette::NEAR_WHITE);
                style.letter_spacing = -0.6 * unit;
                style.text_align_v = TextAlignV::Center;
                style
            }),
        )
        .set("header-subtitle", Prop::Text(SUBTITLE.to_owned()))
        .set(
            "header-subtitle",
            Prop::TextStyle({
                let mut style =
                    text_style(LATIN_FAMILY, 15.0 * unit, 400, rgba(1.0, 1.0, 1.0, 0.85));
                style.text_align_v = TextAlignV::Center;
                style
            }),
        );

    painting
        .set("tile-solid", Prop::Fill(palette::PANEL))
        .set("tile-solid", corners(radius))
        .set(
            "tile-solid",
            stroke(4.0 * unit, StrokeAlign::Inside, palette::AMBER),
        )
        .set(
            "tile-linear",
            gradient(GradientKind::Linear, palette::CRIMSON, palette::VIOLET),
        )
        .set(
            "tile-linear",
            stroke(4.0 * unit, StrokeAlign::Center, palette::NEAR_WHITE),
        )
        .set(
            "tile-radial",
            gradient(GradientKind::Radial, palette::AMBER, palette::CRIMSON),
        )
        .set(
            "tile-radial",
            stroke(4.0 * unit, StrokeAlign::Outside, palette::TEAL),
        )
        .set(
            "tile-angular",
            gradient(GradientKind::Angular, palette::SKY, palette::AMBER),
        )
        .set(
            "tile-diamond",
            gradient(GradientKind::Diamond, palette::TEAL, palette::VIOLET),
        );

    // The crop transform halves the sampled region and moves it to the
    // payload's centre, so the crop tile shows a visibly different part of the
    // same photograph than the tile beside it.
    painting
        .set("tile-image-fill", image_fill(photo, ScaleMode::Fill, 1.0))
        .set("tile-image-fill", corners(radius))
        .set("tile-image-fit", Prop::Fill(palette::PANEL))
        .set("tile-image-fit", corners(radius))
        .set("fit-image", image_fill(photo, ScaleMode::Fit, 1.0))
        .set(
            "tile-image-crop",
            image_crop(
                photo,
                Mat23 {
                    a: 0.5,
                    b: 0.0,
                    c: 0.0,
                    d: 0.5,
                    tx: 0.25,
                    ty: 0.25,
                },
            ),
        )
        .set("tile-image-crop", corners(radius))
        .set("tile-image-tile", image_fill(photo, ScaleMode::Tile, 0.25))
        .set("tile-image-tile", corners(radius));

    // A baked MSDF field masking a gradient: the painter composes the field's
    // coverage with the node's own fill and rasterises no path.
    painting
        .set("tile-vector", Prop::Fill(palette::PANEL))
        .set("tile-vector", corners(radius))
        .set(
            "vector-star",
            gradient(GradientKind::Linear, palette::AMBER, palette::CRIMSON),
        )
        .set("vector-star", shape_field(star.field(field)));

    painting
        .set("tile-drop-shadow", Prop::Fill(palette::NEAR_WHITE))
        .set("tile-drop-shadow", corners(radius))
        .set(
            "tile-drop-shadow",
            drop_shadow(
                6.0 * unit,
                10.0 * unit,
                16.0 * unit,
                2.0 * unit,
                rgba(0.0, 0.0, 0.0, 0.8),
            ),
        )
        .set("tile-inner-shadow", Prop::Fill(palette::MOSS))
        .set("tile-inner-shadow", corners(radius))
        .set(
            "tile-inner-shadow",
            inner_shadow(
                0.0,
                4.0 * unit,
                18.0 * unit,
                8.0 * unit,
                rgba(0.0, 0.0, 0.0, 0.95),
            ),
        );

    // The clip tile's child is half again as wide as the tile and offset up
    // and left, so the clip is what keeps it inside — and the clip follows the
    // tile's corner radii, which a plain rectangular clip would not.
    painting
        .set("tile-clip", Prop::Fill(palette::INK))
        .set("tile-clip", corners(tile * 0.5))
        .set("tile-clip", Prop::Clip(true))
        .set(
            "clip-overflow",
            gradient(GradientKind::Radial, palette::SKY, palette::MOSS),
        );

    // The mask stencils the sibling that follows it and draws nothing itself.
    painting
        .set("tile-mask", Prop::Fill(palette::INK))
        .set("tile-mask", corners(radius))
        .set("mask-shape", corners(tile * 0.38))
        .set("mask-shape", Prop::Mask(true))
        .set(
            "mask-content",
            gradient(GradientKind::Angular, palette::AMBER, palette::TEAL),
        );

    painting
        .set("tile-group", Prop::Fill(palette::INK))
        .set("tile-group", corners(radius))
        .set("tile-group", Prop::Opacity(0.55))
        .set("group-back", Prop::Fill(palette::CRIMSON))
        .set("group-back", corners(radius))
        .set("group-front", Prop::Fill(palette::SKY))
        .set("group-front", corners(radius));

    painting
        .set("tile-corners", Prop::Fill(palette::AMBER))
        .set(
            "tile-corners",
            Prop::Corners {
                top_left: tile * 0.45,
                top_right: 0.0,
                bottom_right: tile * 0.45,
                bottom_left: 0.0,
            },
        );

    // The frosted panel: a translucent white over a backdrop blur, which is
    // the one effect whose result depends on what is already composited
    // beneath it.
    painting
        .set("frost", Prop::Fill(palette::FROST))
        .set("frost", corners(radius * 1.6))
        .set("frost", backdrop_blur(24.0 * unit))
        .set(
            "frost",
            stroke(1.5 * unit, StrokeAlign::Inside, rgba(1.0, 1.0, 1.0, 0.5)),
        )
        .set("frost-handle", Prop::Fill(rgba(1.0, 1.0, 1.0, 0.6)))
        .set("frost-handle", corners(3.0 * unit));

    painting.commit(&mut ShowcaseSolver::new(
        resources::new_typesetter(),
        resources::atlases(),
    ));
}

/// The scripted phase: drive `sweep` to one end of its range, then the other.
pub fn pulse(live: &mut LiveScene, index: u64) {
    let Some(sweep) = live.signal_named(SWEEP) else {
        return;
    };
    live.set(sweep, if index.is_multiple_of(2) { 0.0 } else { 1.0 });
}
