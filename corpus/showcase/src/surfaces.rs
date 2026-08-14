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
//! replay, which **used to** stage no glyph runs at all — so the header's
//! title blinked out for as long as such a frame lasted. Both springs in this
//! scene therefore drive a channel that re-solves: the header's width, on a
//! node with children, and the frosted panel's x, likewise. Group opacity and
//! the frosted panel's own alpha are set once rather than animated, for the
//! same reason.
//!
//! Issue #621 fixed the replay and it stages text now, so this shape is no
//! longer required. It is kept because it is what the scene was tuned to
//! animate; changing that is a scene decision rather than a fix.

use dashlang::{Channel, LiveScene, Scene, Spring, node};
use dashpaint::{GradientKind, Mat23, ScaleMode, Stroke, StrokeAlign};
use dashscene_core::{Arena, CrossAxisAlign, LayoutMode, TextAlignV};

use crate::badge;
use crate::resources::{self, LATIN_FAMILY};
use crate::solver::ShowcaseSolver;
use crate::vocabulary::{
    Painting, diagonal_gradient, gradient, image_crop, image_fill, palette, rgba, shape_field,
    text_style,
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
const SUBTITLE: &str = "the v0 paint vocabulary";

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
        .child(
            node("backdrop")
                .size(width, height)
                .fill_with(diagonal_gradient(palette::NAVY, palette::INK)),
        )
        .child(
            node("header")
                .at(margin, margin)
                .size(gallery_width, header_height)
                .mode(LayoutMode::Horizontal)
                .cross_align(CrossAxisAlign::Center)
                .padding(gap * 1.5, 0.0, gap * 1.5, 0.0)
                .gap(gap)
                .fill_with(gradient(
                    GradientKind::Linear,
                    palette::VIOLET,
                    palette::SKY,
                ))
                .corners(radius)
                .drop_shadow(0.0, 6.0 * unit, 18.0 * unit, 0.0, rgba(0.0, 0.0, 0.0, 0.55))
                .stroke(Stroke {
                    width: 1.5 * unit,
                    align: StrokeAlign::Inside,
                    color: rgba(1.0, 1.0, 1.0, 0.35),
                })
                // The header has children, so a width write redistributes them
                // and the frame re-solves. That is what keeps the title's
                // glyphs staged while the scene animates.
                .bind(
                    Channel::Width,
                    sweep.map_range(0.0, 1.0, gallery_width, gallery_width * 0.72),
                )
                .smooth(Channel::Width, Spring::critically_damped(0.45))
                .child(
                    node("header-title")
                        .size(gallery_width * 0.24, header_height * 0.62)
                        .text(TITLE)
                        .text_style({
                            let mut style =
                                text_style(LATIN_FAMILY, 34.0 * unit, 600, palette::NEAR_WHITE);
                            style.letter_spacing = -0.6 * unit;
                            style.text_align_v = TextAlignV::Center;
                            style
                        }),
                )
                .child(
                    node("header-subtitle")
                        .size(gallery_width * 0.62, header_height * 0.42)
                        .text(SUBTITLE)
                        .text_style({
                            let mut style = text_style(
                                LATIN_FAMILY,
                                15.0 * unit,
                                400,
                                rgba(1.0, 1.0, 1.0, 0.85),
                            );
                            style.text_align_v = TextAlignV::Center;
                            style
                        }),
                ),
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
                .child(
                    plain("tile-solid")
                        .fill(palette::PANEL)
                        .corners(radius)
                        .stroke(Stroke {
                            width: 4.0 * unit,
                            align: StrokeAlign::Inside,
                            color: palette::AMBER,
                        }),
                )
                .child(
                    plain("tile-linear")
                        .fill_with(gradient(
                            GradientKind::Linear,
                            palette::CRIMSON,
                            palette::VIOLET,
                        ))
                        .stroke(Stroke {
                            width: 4.0 * unit,
                            align: StrokeAlign::Center,
                            color: palette::NEAR_WHITE,
                        }),
                )
                .child(
                    plain("tile-radial")
                        .fill_with(gradient(
                            GradientKind::Radial,
                            palette::AMBER,
                            palette::CRIMSON,
                        ))
                        .stroke(Stroke {
                            width: 4.0 * unit,
                            align: StrokeAlign::Outside,
                            color: palette::TEAL,
                        }),
                )
                .child(plain("tile-angular").fill_with(gradient(
                    GradientKind::Angular,
                    palette::SKY,
                    palette::AMBER,
                )))
                .child(plain("tile-diamond").fill_with(gradient(
                    GradientKind::Diamond,
                    palette::TEAL,
                    palette::VIOLET,
                )))
                // Images, one tile per Figma scale mode. The `Fit` tile puts
                // the image on a short child so the letterbox the mode
                // produces has the tile's own fill behind it — on a square box
                // a square payload fits and fills identically.
                //
                // The fills themselves are the one part of this scene's paint
                // that is still staged in a second pass: an image's index is
                // issued by the arena, which this inert tree does not have.
                .child(plain("tile-image-fill").corners(radius))
                .child(
                    plain("tile-image-fit")
                        .fill(palette::PANEL)
                        .corners(radius)
                        .child(node("fit-image").at(0.0, tile * 0.2).size(tile, tile * 0.6)),
                )
                .child(plain("tile-image-crop").corners(radius))
                .child(plain("tile-image-tile").corners(radius))
                // A baked vector field masking a gradient. The field is staged
                // in the second pass for the reason the image fills are.
                .child(
                    plain("tile-vector")
                        .fill(palette::PANEL)
                        .corners(radius)
                        .child(
                            node("vector-star")
                                .at(star_inset, star_inset)
                                .size(star, star)
                                .fill_with(gradient(
                                    GradientKind::Linear,
                                    palette::AMBER,
                                    palette::CRIMSON,
                                )),
                        ),
                )
                // Effects.
                .child(
                    plain("tile-drop-shadow")
                        .fill(palette::NEAR_WHITE)
                        .corners(radius)
                        .drop_shadow(
                            6.0 * unit,
                            10.0 * unit,
                            16.0 * unit,
                            2.0 * unit,
                            rgba(0.0, 0.0, 0.0, 0.8),
                        ),
                )
                .child(
                    plain("tile-inner-shadow")
                        .fill(palette::MOSS)
                        .corners(radius)
                        .inner_shadow(
                            0.0,
                            4.0 * unit,
                            18.0 * unit,
                            8.0 * unit,
                            rgba(0.0, 0.0, 0.0, 0.95),
                        ),
                )
                // A clipping container whose child overflows it on every edge,
                // against a fully rounded box, so the clip is the circle. The
                // clip follows the tile's corner radii, which a plain
                // rectangular clip would not.
                .child(
                    plain("tile-clip")
                        .fill(palette::INK)
                        .corners(tile * 0.5)
                        .clip(true)
                        .child(
                            node("clip-overflow")
                                .at(-tile * 0.25, -tile * 0.25)
                                .size(tile * 1.5, tile * 1.5)
                                .fill_with(gradient(
                                    GradientKind::Radial,
                                    palette::SKY,
                                    palette::MOSS,
                                )),
                        ),
                )
                // A mask sibling stencilling the sibling that follows it. The
                // mask node draws nothing itself.
                .child(
                    plain("tile-mask")
                        .fill(palette::INK)
                        .corners(radius)
                        .child(
                            node("mask-shape")
                                .at(tile * 0.12, tile * 0.12)
                                .size(tile * 0.76, tile * 0.76)
                                .corners(tile * 0.38)
                                .mask(true),
                        )
                        .child(node("mask-content").size(tile, tile).fill_with(gradient(
                            GradientKind::Angular,
                            palette::AMBER,
                            palette::TEAL,
                        ))),
                )
                // A subtree at less than full alpha, with overlapping
                // children, which is what makes commit resolve a render-target
                // group rather than a per-rect alpha.
                .child(
                    plain("tile-group")
                        .fill(palette::INK)
                        .corners(radius)
                        .opacity(0.55)
                        .child(
                            node("group-back")
                                .at(tile * 0.10, tile * 0.16)
                                .size(tile * 0.58, tile * 0.58)
                                .fill(palette::CRIMSON)
                                .corners(radius),
                        )
                        .child(
                            node("group-front")
                                .at(tile * 0.34, tile * 0.30)
                                .size(tile * 0.58, tile * 0.58)
                                .fill(palette::SKY)
                                .corners(radius),
                        ),
                )
                // Corner radii on their own, two corners rounded and two
                // square, so the per-corner vocabulary is what the tile shows.
                .child(plain("tile-corners").fill(palette::AMBER).corners_each(
                    tile * 0.45,
                    0.0,
                    tile * 0.45,
                    0.0,
                )),
        )
        // Last in document order, so it composites over the gallery — which is
        // what gives its backdrop blur something to read. It carries a child
        // for the same reason the header does: an x write on a node with
        // children re-solves.
        .child(
            node("frost")
                .at(frost_left, frost_top)
                .size(frost_width, frost_height)
                // A translucent white over a backdrop blur, which is the one
                // effect whose result depends on what is already composited
                // beneath it.
                .fill(palette::FROST)
                .corners(radius * 1.6)
                .backdrop_blur(24.0 * unit)
                .stroke(Stroke {
                    width: 1.5 * unit,
                    align: StrokeAlign::Inside,
                    color: rgba(1.0, 1.0, 1.0, 0.5),
                })
                .bind(
                    Channel::X,
                    sweep.map_range(0.0, 1.0, frost_left, frost_right),
                )
                .smooth(Channel::X, Spring::critically_damped(0.55))
                .child(
                    node("frost-handle")
                        .at(frost_width * 0.3, frost_height * 0.86)
                        .size(frost_width * 0.4, 5.0 * unit)
                        .fill(rgba(1.0, 1.0, 1.0, 0.6))
                        .corners(3.0 * unit),
                ),
        );

    let label = badge::badge(&mut scene, width, height);
    scene.roots([root, label]);
    let live = scene.build_live(
        arena,
        Box::new(ShowcaseSolver::new(
            resources::new_typesetter(),
            resources::atlases(),
        )),
    );
    paint(arena, star);
    live
}

/// Stages the paint intent that cannot be authored on the value tree: the four
/// image fills and the baked vector field.
///
/// Everything else this scene paints is authored on the builder above. What is
/// left here is what needs an image index, and an index is issued by
/// `Txn::add_image` against the arena — which an inert value tree does not
/// have.
fn paint(arena: &mut Arena, star_size: f32) {
    let star = resources::baked_star(star_size);
    let mut painting = Painting::open(arena);
    let photo = painting.add_image(resources::photo());
    let field = painting.add_image(star.atlas.clone());

    // The crop transform halves the sampled region and moves it to the
    // payload's centre, so the crop tile shows a visibly different part of the
    // same photograph than the tile beside it.
    painting
        .set("tile-image-fill", image_fill(photo, ScaleMode::Fill, 1.0))
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
        .set("tile-image-tile", image_fill(photo, ScaleMode::Tile, 0.25))
        // A baked MSDF field masking the gradient the builder put on this
        // node: the painter composes the field's coverage with the node's own
        // fill and rasterises no path.
        .set("vector-star", shape_field(star.field(field)));

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
