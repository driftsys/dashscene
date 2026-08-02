//! Proof that collapsing each showcase scene from two authoring passes
//! to one moved nothing. Each case builds the frozen pre-migration
//! builder and the migrated one into separate arenas and compares the
//! committed painter input.
//!
//! # The frozen builders are frozen
//!
//! Each `*_two_pass` function below is a copy of that scene's builder as
//! it stood before the migration, verbatim except where a signature it
//! calls no longer exists — each such adaptation is named at the section
//! it appears in, and each is provably behaviour-preserving. It is never
//! edited to track a later scene *change*: its whole value is that it is
//! the pre-migration authoring, unchanged.
//!
//! So a deliberate change to a scene will fail its equivalence test,
//! and that is the test working. It asserts "this scene still paints
//! what it painted at the migration". Whoever makes that change deletes
//! the scene's frozen builder and its test in the same commit, and says
//! so in the message. This is a one-way ratchet, not a specification of
//! what the scene should look like.
//!
//! All three scenes have migrated. Nothing in `corpus/showcase/src/`
//! authors paint in two passes any more, so a future two-pass call is
//! a regression, not a leftover.

use std::sync::OnceLock;

use dashlang::{Arena, Channel, FormatSpec, LiveScene, Scene, Signal, Spring, node};
use dashpaint::{
    Blur, BlurKind, Color, GradientKind, Mat23, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign,
    Vec2,
};
use dashscene_core::{
    AxisSizing, CrossAxisAlign, GridTrack, LayoutMode, MainAxisAlign, Prop, TextAlign, TextAlignV,
    VariantMember, VariantSetId, VariantValue,
};

use showcase::layout::SPREAD;
use showcase::resources::{self, ARABIC_FAMILY, LATIN_FAMILY};
use showcase::solver::ShowcaseSolver;
use showcase::surfaces::SWEEP;
use showcase::typography::LEVEL;
use showcase::vocabulary::{
    Painting, diagonal_gradient, gradient, image_crop, image_fill, palette, rgba, shape_field,
    text_style,
};

// `vocabulary.rs` no longer exports `corners`, `stroke`, `drop_shadow`,
// `inner_shadow` or `backdrop_blur` (Task 11): `dashlang::Node` grew a setter
// for each, and every scene now calls that setter instead. The frozen
// builders below predate that setter and still call these five under their
// original names, so — for the same reason `DESIGN` and the other
// module-private constants are copied into this file rather than reached for
// on the live module — a frozen copy of each lives here too. This is not the
// scene's own vocabulary; it is what the pre-migration `Painting` pass used
// to build a `Prop` by hand, kept only so the frozen builders keep compiling.

/// All four corners at one radius.
fn corners(radius: f32) -> Prop {
    Prop::Corners {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    }
}

/// A stroke of `width` document units, placed by `align`.
fn stroke(width: f32, align: StrokeAlign, color: Color) -> Prop {
    Prop::Stroke(Stroke {
        width,
        align,
        color,
    })
}

/// One drop shadow.
fn drop_shadow(dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Prop {
    Prop::Shadows(vec![Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: dx, y: dy },
        blur,
        spread,
        color,
    }])
}

/// One inner shadow.
fn inner_shadow(dx: f32, dy: f32, blur: f32, spread: f32, color: Color) -> Prop {
    Prop::Shadows(vec![Shadow {
        kind: ShadowKind::Inner,
        offset: Vec2 { x: dx, y: dy },
        blur,
        spread,
        color,
    }])
}

/// One backdrop blur of `radius` document units.
fn backdrop_blur(radius: f32) -> Prop {
    Prop::Blurs(vec![Blur {
        kind: BlurKind::Backdrop,
        radius,
    }])
}

/// The whole painter input, compared exactly — through the same indirection a
/// painter reads it through.
///
/// Every rect is compared field for field: its box and its opacity directly,
/// its paint and its clip through the entry each index resolves to. The group,
/// glyph-run and image tables are compared whole.
///
/// The image table is compared whole rather than per rect on purpose. A rect
/// reaches an image only through the paint entry it resolves to, and that entry
/// carries the image's *index*, so comparing paints alone proves the two sides
/// name the same index — never that the index names the same payload. A swapped
/// payload behind a matching index, and an extra asset no rect references, both
/// pass a per-rect comparison and both fail this one.
///
/// # Why the paint and clip tables are resolved rather than compared whole
///
/// A table index is handed out by the arena's **retained** interner, so it is a
/// function of the commit history and not of the picture: an entry keeps the
/// index it was first assigned, a changed entry earns a new index, and the one
/// it replaced stays in the table (`Arena::paint_map`, issue #164).
///
/// The two sides of this comparison reach the same picture through different
/// commit sequences, by construction. The frozen two-pass builder stages all of
/// its paint in the second commit; the migrated one-pass builder stages all of
/// it in the first except what needs an arena-issued image index. So the two
/// tables hold the same entries under different indices, and the migrated side
/// also keeps the entries its image-carrying nodes held between the two
/// commits — entries referenced by no rect on the migrated side, and by none on
/// the frozen side. Each scene's own test comment records the counts it
/// measured.
///
/// Comparing the tables whole would therefore assert the order the interner
/// handed out its indices, which no migration that moves paint into the first
/// commit can preserve, and which no painter can observe. This compares what a
/// painter draws instead, and it is exact.
///
/// The clip side resolves to a [`dashpaint::ClipView`], which derives
/// `PartialEq` over both its stored range (`offset`/`count` into
/// `ClipTable::all_boxes`) and its boxes. The range is exactly the kind of
/// commit-history-dependent position this function exists to avoid comparing,
/// so this reads `.boxes()` off each view rather than comparing the views
/// themselves.
///
/// # The general rule this helper follows
///
/// This helper compares two independent arenas, so no value it compares may
/// carry an index or offset into a per-arena table — such a position is a
/// function of that arena's own commit history, not of the picture, and two
/// arenas that reach the same picture by different commit sequences earn
/// different positions for the same content. Every such value must be
/// resolved to its contents first, and only the contents compared. This has
/// bitten the helper four times: the paint-table index on a rect (resolved
/// through `PaintTable::resolve`, above), `ClipRegion`'s flattening to a
/// range read through `ClipView::boxes()` (also above), `PaintEntry`'s
/// `shadows`/`blurs` fields, which story #578 turned into arena-relative
/// positions in their own right — resolved below through
/// `PaintTable::shadows`/`PaintTable::blurs` — and now `fill` and
/// `extra_fills`, which the last step of that story turned into row indices
/// into each arena's own per-kind fill tables. Those are resolved through
/// [`fills_of`]; comparing them directly was the exact mistake
/// `docs/decisions/cross-arena-comparison-resolves-indices.md` exists to
/// stop, and it passed only because both builders happened to intern in the
/// same order.
fn assert_same_committed(a: &Arena, b: &Arena) {
    let (a, b) = (a.committed(), b.committed());

    assert_eq!(a.rects().len(), b.rects().len(), "rects (count)");
    for (index, (left, right)) in a.rects().iter().zip(b.rects()).enumerate() {
        assert_eq!(
            (left.x, left.y, left.w, left.h, left.opacity),
            (right.x, right.y, right.w, right.h, right.opacity),
            "rects (entry {index})"
        );
        let (left_paint, right_paint) = (
            a.paints().resolve(left.paint),
            b.paints().resolve(right.paint),
        );
        assert_eq!(
            (&left_paint.stroke, &left_paint.corners, &left_paint.shape),
            (
                &right_paint.stroke,
                &right_paint.corners,
                &right_paint.shape
            ),
            "paints (rect {index})"
        );
        assert_eq!(
            fills_of(a, left_paint),
            fills_of(b, right_paint),
            "paint fills (rect {index})"
        );
        assert_eq!(
            a.paints().shadows(left_paint),
            b.paints().shadows(right_paint),
            "paint shadows (rect {index})"
        );
        assert_eq!(
            a.paints().blurs(left_paint),
            b.paints().blurs(right_paint),
            "paint blurs (rect {index})"
        );
        assert_eq!(
            a.clips().resolve(left.clip).boxes(),
            b.clips().resolve(right.clip).boxes(),
            "clips (rect {index})"
        );
    }

    assert_eq!(a.groups(), b.groups(), "groups");
    assert_eq!(a.glyphs(), b.glyphs(), "glyphs");
    assert_eq!(a.images(), b.images(), "images");
}

/// One entry's fills — the base fill and every stacked layer — resolved to
/// their contents.
///
/// A `PaintKind` is a row index into the arena's own per-kind fill tables
/// since story #578, so two arenas that reached the same picture by
/// different commit sequences can hold the same fill at different rows. Only
/// the resolved contents mean anything across the two.
fn fills_of<'a>(
    scene: &'a dashscene_core::CommittedScene,
    entry: &dashscene_core::PaintEntry,
) -> Vec<dashscene_core::Fill<'a>> {
    entry
        .fill
        .iter()
        .chain(entry.extra_fills.iter())
        .map(|&kind| scene.paints().fill(kind))
        .collect()
}

// --- `surfaces`, frozen at its two-pass authoring -------------------------
//
// `surfaces::build` and the private `surfaces::paint` it calls, copied
// verbatim except for two adaptations. `paint` is renamed to
// `surfaces_two_pass_paint` because this file holds more than one frozen
// scene. And its nine `gradient`/`diagonal_gradient` calls are each
// wrapped in `Prop::FillWith(...)`, because those two helpers used to
// return a `Prop` — a `Prop::FillWith` around the `FillSpec` they built —
// and on this branch they return the bare `FillSpec`, so that the scenes
// can hand it to `Node::fill_with`. The wrap restores exactly the value the
// helper used to return, at the same call site, so the `Prop` this body
// stages is byte-for-byte the one it staged before the change.
// The module-private constants the body reads are copied with it.

const DESIGN: (f32, f32) = (960.0, 600.0);

const COLUMNS: f32 = 6.0;
const ROWS: f32 = 3.0;

const TITLE: &str = "dashscene";
const SUBTITLE: &str = "the v0 paint vocabulary, drawn by the Skia reference painter";

fn surfaces_two_pass(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
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
    surfaces_two_pass_paint(arena, unit, radius, tile, star);
    live
}

/// Stages the paint vocabulary onto the tiles, by name.
fn surfaces_two_pass_paint(arena: &mut Arena, unit: f32, radius: f32, tile: f32, star_size: f32) {
    let star = resources::baked_star(star_size);
    let mut painting = Painting::open(arena);
    let photo = painting.add_image(resources::photo());
    let field = painting.add_image(star.atlas.clone());

    painting
        .set(
            "backdrop",
            Prop::FillWith(diagonal_gradient(palette::NAVY, palette::INK)),
        )
        .set(
            "header",
            Prop::FillWith(gradient(
                GradientKind::Linear,
                palette::VIOLET,
                palette::SKY,
            )),
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
            Prop::FillWith(gradient(
                GradientKind::Linear,
                palette::CRIMSON,
                palette::VIOLET,
            )),
        )
        .set(
            "tile-linear",
            stroke(4.0 * unit, StrokeAlign::Center, palette::NEAR_WHITE),
        )
        .set(
            "tile-radial",
            Prop::FillWith(gradient(
                GradientKind::Radial,
                palette::AMBER,
                palette::CRIMSON,
            )),
        )
        .set(
            "tile-radial",
            stroke(4.0 * unit, StrokeAlign::Outside, palette::TEAL),
        )
        .set(
            "tile-angular",
            Prop::FillWith(gradient(
                GradientKind::Angular,
                palette::SKY,
                palette::AMBER,
            )),
        )
        .set(
            "tile-diamond",
            Prop::FillWith(gradient(
                GradientKind::Diamond,
                palette::TEAL,
                palette::VIOLET,
            )),
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
            Prop::FillWith(gradient(
                GradientKind::Linear,
                palette::AMBER,
                palette::CRIMSON,
            )),
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
            Prop::FillWith(gradient(GradientKind::Radial, palette::SKY, palette::MOSS)),
        );

    // The mask stencils the sibling that follows it and draws nothing itself.
    painting
        .set("tile-mask", Prop::Fill(palette::INK))
        .set("tile-mask", corners(radius))
        .set("mask-shape", corners(tile * 0.38))
        .set("mask-shape", Prop::Mask(true))
        .set(
            "mask-content",
            Prop::FillWith(gradient(
                GradientKind::Angular,
                palette::AMBER,
                palette::TEAL,
            )),
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

/// Measured at the migration: 31 rects on both sides with every resolved
/// `PaintEntry` and `ClipRegion` equal, against 25 paint entries on the frozen
/// side and 27 on the migrated one, of which exactly 2 are referenced by no
/// rect.
#[test]
fn surfaces_migrates_without_changing_committed_output() {
    let (w, h) = (1280, 720);

    let mut old = Arena::new();
    let _ = surfaces_two_pass(&mut old, w, h);

    let mut new = Arena::new();
    let _ = showcase::surfaces::build(&mut new, w, h);

    assert_same_committed(&old, &new);
}

// --- `layout`, frozen at its two-pass authoring ---------------------------
//
// `layout::build` and the private `layout::paint` it calls, copied verbatim.
// `paint` is renamed to `layout_two_pass_paint` for the same reason
// `surfaces`'s was. The module-private constants and statics the bodies read
// are copied with them; `DESIGN` is renamed to `LAYOUT_DESIGN` because
// `surfaces` already defines a `DESIGN` at the same value, and the two cannot
// share a module. One exception: `declare_chip_variants`'s doc comment is
// shortened to a pointer at the live version rather than copied verbatim,
// since what this test freezes is the code that runs, not the prose beside
// it.

const LAYOUT_DESIGN: (f32, f32) = (960.0, 600.0);

/// The bool signal behind the topology change — see `layout::SHOW_MIDDLE`'s
/// own doc comment for why this is a `OnceLock` rather than a return value.
static SHOW_MIDDLE: OnceLock<Signal<bool>> = OnceLock::new();

/// The variant set the frozen `declare_chip_variants` below declares.
static CHIP_SET: OnceLock<VariantSetId> = OnceLock::new();

const CHIP_MEMBERS: usize = 3;

const WRAP_CHIPS: [&str; 7] = [
    "wrap-0", "wrap-1", "wrap-2", "wrap-3", "wrap-4", "wrap-5", "wrap-6",
];

fn layout_two_pass(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
    let (width, height) = (width as f32, height as f32);
    let unit = (width / LAYOUT_DESIGN.0).min(height / LAYOUT_DESIGN.1);

    let margin = 30.0 * unit;
    let gap = 16.0 * unit;
    let column = width - 2.0 * margin;
    let radius = 8.0 * unit;
    let panel_gap = 10.0 * unit;
    let chip = 26.0 * unit;
    // The three sections share the column's height between them, so the scene
    // fills the window at any aspect ratio rather than leaving a band of
    // background under it.
    let stack = height - 2.0 * margin - 2.0 * gap;
    let panels_height = stack * 0.34;
    let grid_height = stack * 0.50;
    let reflow_height = stack * 0.16;

    let mut scene = Scene::new();
    let spread = scene.signal_named(SPREAD, 0.0);
    let show_middle = scene.signal(true);
    let _ = SHOW_MIDDLE.set(show_middle);

    let panel_width = (column - 2.0 * gap) / 3.0;

    let root = node("layout")
        .size(width, height)
        .mode(LayoutMode::Vertical)
        .padding(margin, margin, margin, margin)
        .gap(gap)
        .child(
            node("panels")
                .size(column, panels_height)
                .mode(LayoutMode::Horizontal)
                .gap(gap)
                .cross_align(CrossAxisAlign::Start)
                // A vertical column whose height hugs its children, so the
                // panel is exactly as tall as the chips inside it.
                .child(
                    node("panel-column")
                        .size(panel_width, panels_height)
                        .mode(LayoutMode::Vertical)
                        .sizing_v(AxisSizing::Hug)
                        .gap(panel_gap)
                        .padding(panel_gap, panel_gap, panel_gap, panel_gap)
                        .children((0..3).map(|i| {
                            node(match i {
                                0 => "column-a",
                                1 => "column-b",
                                _ => "column-c",
                            })
                            .size(panel_width - 2.0 * panel_gap, chip)
                        })),
                )
                // A row splitting its free space: one fixed chip, two `Fill`
                // chips that take equal shares of what is left.
                .child(
                    node("panel-fill")
                        .size(panel_width, panels_height)
                        .mode(LayoutMode::Horizontal)
                        .gap(panel_gap)
                        .padding(panel_gap, panel_gap, panel_gap, panel_gap)
                        .cross_align(CrossAxisAlign::Center)
                        .child(node("fill-fixed").size(chip * 1.4, chip * 2.0))
                        .child(
                            node("fill-a")
                                .size(chip, chip * 3.0)
                                .sizing_h(AxisSizing::Fill),
                        )
                        .child(
                            node("fill-b")
                                .size(chip, chip * 4.0)
                                .sizing_h(AxisSizing::Fill),
                        ),
                )
                // A wrapping row with its own cross-axis gap, so the line
                // spacing is distinct from the spacing inside a line.
                .child(
                    node("panel-wrap")
                        .size(panel_width, panels_height)
                        .mode(LayoutMode::Wrap)
                        .gap(panel_gap)
                        .cross_gap(panel_gap * 1.8)
                        .padding(panel_gap, panel_gap, panel_gap, panel_gap)
                        .children((0..7).map(|i| node(WRAP_CHIPS[i]).size(chip * 1.7, chip))),
                ),
        )
        // A grid with a fixed first column, two fractional columns, and two
        // children that span more than one track.
        .child(
            node("panel-grid")
                .size(column, grid_height)
                .mode(LayoutMode::Grid)
                .gap(panel_gap)
                .cross_gap(panel_gap)
                .padding(panel_gap, panel_gap, panel_gap, panel_gap)
                .grid_columns([
                    GridTrack::Fixed(140.0 * unit),
                    GridTrack::Fraction(1.0),
                    GridTrack::Fraction(2.0),
                ])
                .grid_rows([GridTrack::Fraction(1.0), GridTrack::Fraction(1.0)])
                // `Fill` on both axes is what stretches a grid child to its
                // cell. The last child is left at a fixed size instead, so the
                // difference between stretching and anchoring is visible in
                // the same grid.
                .child(
                    node("grid-tall")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fill)
                        .grid_row(0)
                        .grid_column(0)
                        .grid_row_span(2),
                )
                .child(
                    node("grid-wide")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fill)
                        .grid_row(0)
                        .grid_column(1)
                        .grid_column_span(2),
                )
                .child(
                    node("grid-one")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fill)
                        .grid_row(1)
                        .grid_column(1),
                )
                .child(
                    node("grid-two")
                        .size(120.0 * unit, 40.0 * unit)
                        .grid_row(1)
                        .grid_column(2),
                ),
        )
        // The reflow row: the gap animates, and the middle chip leaves and
        // rejoins the laid-out set.
        .child(
            node("reflow")
                .size(column, reflow_height)
                .mode(LayoutMode::Horizontal)
                .main_align(MainAxisAlign::Center)
                .cross_align(CrossAxisAlign::Center)
                .padding(panel_gap, panel_gap, panel_gap, panel_gap)
                .gap(gap)
                .bind(Channel::Gap, spread.map_range(0.0, 1.0, gap, gap * 6.0))
                .smooth(Channel::Gap, Spring::critically_damped(0.55))
                .child(node("reflow-a").size(120.0 * unit, reflow_height * 0.5))
                .child(
                    node("reflow-b")
                        .size(120.0 * unit, reflow_height * 0.5)
                        .visible_when(show_middle),
                )
                .child(node("reflow-c").size(120.0 * unit, reflow_height * 0.5))
                .child(node("reflow-d").size(120.0 * unit, reflow_height * 0.5)),
        );

    scene.roots([root]);
    let live = scene.build_live(
        arena,
        Box::new(ShowcaseSolver::new(
            resources::new_typesetter(),
            resources::atlases(),
        )),
    );
    layout_two_pass_paint(arena, radius, unit);
    live
}

fn layout_two_pass_paint(arena: &mut Arena, radius: f32, unit: f32) {
    let mut painting = Painting::open(arena);
    painting.set("layout", Prop::Fill(palette::NAVY));

    for panel in [
        "panel-column",
        "panel-fill",
        "panel-wrap",
        "panel-grid",
        "reflow",
    ] {
        painting.set(panel, Prop::Fill(palette::PANEL));
        painting.set(panel, corners(radius));
    }

    for (name, colour) in [
        ("column-a", palette::SKY),
        ("column-b", palette::SKY),
        ("column-c", palette::SKY),
        ("fill-fixed", palette::AMBER),
        ("fill-a", palette::MOSS),
        ("fill-b", palette::MOSS),
        ("grid-tall", palette::VIOLET),
        ("grid-wide", palette::CRIMSON),
        ("grid-one", palette::TEAL),
        ("grid-two", palette::TEAL),
        ("reflow-a", palette::AMBER),
        ("reflow-b", palette::CRIMSON),
        ("reflow-c", palette::AMBER),
        ("reflow-d", palette::AMBER),
    ] {
        painting.set(name, Prop::Fill(colour));
        painting.set(name, corners(radius * 0.6));
    }

    for chip in WRAP_CHIPS {
        painting.set(chip, Prop::Fill(palette::VIOLET));
        painting.set(chip, corners(radius * 0.6));
    }

    // The chip that comes and goes is outlined as well as filled, so it is
    // still identifiable in a still frame taken while it is present.
    painting.set(
        "reflow-b",
        stroke(2.0 * unit, StrokeAlign::Inside, palette::NEAR_WHITE),
    );

    declare_chip_variants(&mut painting, unit);

    painting.commit(&mut ShowcaseSolver::new(
        resources::new_typesetter(),
        resources::atlases(),
    ));
}

/// Declares the three-member variant set on the reflow row's rightmost chip,
/// copied verbatim from `layout::declare_chip_variants`.
fn declare_chip_variants(painting: &mut Painting<'_>, unit: f32) {
    let chip = painting.node("reflow-d");
    let members = vec![
        VariantMember {
            name: Some("wide".to_owned()),
            overrides: Vec::new(),
        },
        VariantMember {
            name: Some("narrow".to_owned()),
            overrides: vec![
                (chip, VariantValue::Width(48.0 * unit)),
                (chip, VariantValue::Fill(palette::TEAL)),
            ],
        },
        VariantMember {
            name: Some("gone".to_owned()),
            overrides: vec![(chip, VariantValue::Visible(false))],
        },
    ];
    assert_eq!(
        members.len(),
        CHIP_MEMBERS,
        "CHIP_MEMBERS is what switch_variant wraps on, so it has to count this list"
    );
    let _ = CHIP_SET.set(painting.add_variant_set(members));
}

/// Measured at the migration: 28 rects on both sides with every resolved
/// `PaintEntry` and `ClipRegion` equal, against 10 paint entries on each
/// side, all 10 referenced by a rect. This scene stages no image and no
/// vector field, so no entry is issued between the two commits and neither
/// side carries an orphan.
#[test]
fn layout_migrates_without_changing_committed_output() {
    let (w, h) = (1280, 720);

    let mut old = Arena::new();
    let _ = layout_two_pass(&mut old, w, h);

    let mut new = Arena::new();
    let _ = showcase::layout::build(&mut new, w, h);

    assert_same_committed(&old, &new);
}

// --- `typography`, frozen at its two-pass authoring -----------------------
//
// `typography::build` and the private `typography::paint` it calls, copied
// verbatim. `paint` is renamed to `typography_two_pass_paint` for the same
// reason `surfaces`'s was. The module-private constants it reads are copied
// with it; `DESIGN` is renamed to `TYPOGRAPHY_DESIGN` because `surfaces`
// already defines a `DESIGN` at the same value, and the two cannot share a
// module.

/// The readout's full-scale value.
const TOP_SPEED: f32 = 240.0;

const TYPOGRAPHY_DESIGN: (f32, f32) = (960.0, 600.0);

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

fn typography_two_pass(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
    let (width, height) = (width as f32, height as f32);
    let unit = (width / TYPOGRAPHY_DESIGN.0).min(height / TYPOGRAPHY_DESIGN.1);

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
        .child(node("type-heading").size(column, 46.0 * unit))
        .child(node("type-sub").size(column, 26.0 * unit))
        .child(
            node("arabic-panel")
                .size(column, 78.0 * unit)
                .mode(LayoutMode::Horizontal)
                .gap(gap)
                .padding(gap, gap * 0.6, gap, gap * 0.6)
                .cross_align(CrossAxisAlign::Center)
                .child(node("arabic-banner").size(column * 0.42, 46.0 * unit))
                .child(node("arabic-word").size(column * 0.22, 52.0 * unit))
                .child(node("arabic-speed").size(column * 0.24, 52.0 * unit)),
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
                        // The bar's own width is what the signal drives. It
                        // sits under a flex parent, so the write is
                        // layout-affecting and the frame re-solves — which is
                        // what keeps the readout's glyphs staged.
                        .child(
                            node("gauge-fill")
                                .size(gauge_track * 0.08, gauge_height)
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
                        .bind_text(level.format(FormatSpec::new("", 0, " km/h"))),
                ),
        )
        .child(node("type-body").size(column * 0.72, 92.0 * unit))
        .child(
            node("clip-box").size(column * 0.62, 40.0 * unit).child(
                node("clip-text")
                    .at(0.0, 0.0)
                    .size(column * 1.4, 40.0 * unit),
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
    typography_two_pass_paint(arena, unit, radius);
    live
}

fn typography_two_pass_paint(arena: &mut Arena, unit: f32, radius: f32) {
    let mut painting = Painting::open(arena);

    painting
        .set("typography", Prop::Fill(palette::NAVY))
        .set("type-heading", Prop::Text(HEADING.to_owned()))
        .set(
            "type-heading",
            Prop::TextStyle({
                let mut style = text_style(LATIN_FAMILY, 40.0 * unit, 600, palette::NEAR_WHITE);
                style.letter_spacing = -0.8 * unit;
                style
            }),
        )
        .set("type-sub", Prop::Text(SUBHEADING.to_owned()))
        .set(
            "type-sub",
            Prop::TextStyle(text_style(LATIN_FAMILY, 17.0 * unit, 400, palette::SKY)),
        );

    // The Arabic panel. Each run asks for the Arabic family by name; coverage
    // outranks the request in the cascade, so an Arabic run would reach that
    // family even if it asked for Inter — naming it keeps the intent in the
    // scene rather than in the fallback.
    painting
        .set("arabic-panel", Prop::Fill(palette::PANEL))
        .set("arabic-panel", corners(radius))
        .set("arabic-banner", Prop::Text(ARABIC_BANNER.to_owned()))
        .set(
            "arabic-banner",
            Prop::TextStyle(text_style(
                ARABIC_FAMILY,
                26.0 * unit,
                400,
                palette::NEAR_WHITE,
            )),
        )
        .set("arabic-word", Prop::Text(ARABIC_WORD.to_owned()))
        .set(
            "arabic-word",
            Prop::TextStyle(text_style(ARABIC_FAMILY, 34.0 * unit, 400, palette::AMBER)),
        )
        .set("arabic-speed", Prop::Text(ARABIC_SPEED.to_owned()))
        .set(
            "arabic-speed",
            Prop::TextStyle(text_style(ARABIC_FAMILY, 34.0 * unit, 400, palette::TEAL)),
        );

    painting
        .set("gauge-track", Prop::Fill(palette::PANEL))
        .set("gauge-track", corners(radius))
        .set("gauge-fill", Prop::Fill(palette::AMBER))
        .set("gauge-fill", corners(radius))
        .set(
            "readout-value",
            Prop::TextStyle({
                let mut style = text_style(LATIN_FAMILY, 28.0 * unit, 600, palette::NEAR_WHITE);
                style.text_align_v = TextAlignV::Center;
                style
            }),
        );

    // A wrapping paragraph, with the four v0.9 text axes set away from their
    // defaults so each is visible: a fixed line height, letter spacing, and
    // both alignments.
    painting.set("type-body", Prop::Text(BODY.to_owned())).set(
        "type-body",
        Prop::TextStyle({
            let mut style = text_style(LATIN_FAMILY, 15.0 * unit, 400, palette::NEAR_WHITE);
            style.line_height_px = Some(24.0 * unit);
            style.letter_spacing = 0.3 * unit;
            style.text_align = TextAlign::Left;
            style.text_align_v = TextAlignV::Top;
            style
        }),
    );

    // Glyphs are clipped by the same resolved clip regions the rects are.
    painting
        .set("clip-box", Prop::Fill(palette::PANEL))
        .set("clip-box", corners(radius))
        .set("clip-box", Prop::Clip(true))
        .set("clip-text", Prop::Text(CLIPPED.to_owned()))
        .set(
            "clip-text",
            Prop::TextStyle({
                let mut style = text_style(LATIN_FAMILY, 20.0 * unit, 400, palette::AMBER);
                style.text_align_v = TextAlignV::Center;
                style
            }),
        );

    painting.commit(&mut ShowcaseSolver::new(
        resources::new_typesetter(),
        resources::atlases(),
    ));
}

/// Measured at the migration: 14 rects on both sides with every resolved
/// `PaintEntry` and `ClipRegion` equal, against 4 paint entries on each side,
/// all 4 referenced by a rect, for the same reason `layout`'s are.
#[test]
fn typography_migrates_without_changing_committed_output() {
    let (w, h) = (1280, 720);

    let mut old = Arena::new();
    let _ = typography_two_pass(&mut old, w, h);

    let mut new = Arena::new();
    let _ = showcase::typography::build(&mut new, w, h);

    assert_same_committed(&old, &new);
}
