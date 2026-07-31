//! `layout` — the flex vocabulary, and a topology change that reflows.
//!
//! Four panels: a vertical column that hugs its content, a row that splits its
//! free space between `Fill` children, a wrapping row, and a grid with track
//! sizing and spans. Below them a row whose gap animates and whose middle chip
//! comes and goes, so the solver is seen redistributing rather than a rect
//! being moved.
//!
//! # What this scene does not do
//!
//! The topology change here is driven by `Prop::Visible`, through
//! `dashlang`'s `visible_when`. A **variant switch** — `Txn::set_variant` over
//! a `VariantSet` — makes the same committed change, and `VariantFlip` is what
//! animates the rect deltas it produces. Neither is reachable from a scene
//! through the host's scene seam, which hands a scene builder an `&mut Arena`
//! once and then hands the scripted phase only an `&mut LiveScene`. The gap is
//! recorded in `README.md`; what is shown here is the committed effect a
//! variant switch has, not the variant machinery that would produce it.

use std::sync::OnceLock;

use dashlang::{
    AxisSizing, Channel, CrossAxisAlign, GridTrack, LayoutMode, LiveScene, MainAxisAlign, Scene,
    Signal, Spring, node,
};
use dashscene_core::{Arena, Prop};

use crate::resources;
use crate::solver::ShowcaseSolver;
use crate::vocabulary::{Painting, corners, palette, stroke};
use dashpaint::StrokeAlign;

/// The scalar signal, named so the pulse can find it.
pub const SPREAD: &str = "layout.spread";

/// The bool signal behind the topology change.
///
/// `Scene::signal_named` declares and looks up scalar signals only, so a bool
/// signal has no runtime name and the scripted phase — which is handed a
/// `LiveScene` and nothing else — cannot ask for one by name. The handle is
/// therefore kept here, set by the first build.
///
/// This is sound rather than convenient: a `Signal<bool>` is an index into the
/// scene's bool table, assigned in declaration order, and this scene declares
/// exactly one. Every rebuild the host performs runs the same builder in the
/// same order and produces the same index, so the handle stored by the first
/// build addresses the same signal in every later scene.
static SHOW_MIDDLE: OnceLock<Signal<bool>> = OnceLock::new();

const DESIGN: (f32, f32) = (960.0, 600.0);

pub fn build(arena: &mut Arena, width: u32, height: u32) -> LiveScene {
    let (width, height) = (width as f32, height as f32);
    let unit = (width / DESIGN.0).min(height / DESIGN.1);

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
    paint(arena, radius, unit);
    live
}

const WRAP_CHIPS: [&str; 7] = [
    "wrap-0", "wrap-1", "wrap-2", "wrap-3", "wrap-4", "wrap-5", "wrap-6",
];

fn paint(arena: &mut Arena, radius: f32, unit: f32) {
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

    painting.commit(&mut ShowcaseSolver::new(
        resources::new_typesetter(),
        resources::atlases(),
    ));
}

/// The scripted phase: widen the gap, then take the middle chip out of the
/// laid-out set, then both, then neither.
pub fn pulse(live: &mut LiveScene, index: u64) {
    if let Some(spread) = live.signal_named(SPREAD) {
        live.set(spread, if index % 4 < 2 { 0.0 } else { 1.0 });
    }
    if let Some(&show_middle) = SHOW_MIDDLE.get() {
        live.set(show_middle, index.is_multiple_of(2));
    }
}
