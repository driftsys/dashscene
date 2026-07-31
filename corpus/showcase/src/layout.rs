//! `layout` — the flex vocabulary, and two topology changes that reflow.
//!
//! Four panels: a vertical column that hugs its content, a row that splits its
//! free space between `Fill` children, a wrapping row, and a grid with track
//! sizing and spans. Below them a row whose gap animates, whose middle chip
//! comes and goes, and whose rightmost chip is driven by a variant set — so the
//! solver is seen redistributing rather than a rect being moved.
//!
//! # Two topology changes, one picture, two mechanisms
//!
//! The middle chip (`reflow-b`) leaves and rejoins the laid-out set through
//! `Prop::Visible`, written by `dashlang`'s `visible_when` from the scripted
//! phase. The rightmost chip (`reflow-d`) narrows, changes colour and then
//! leaves entirely through [`switch_variant`], which is a real
//! `Txn::set_variant` over a real `VariantSet` declared at build time. The two
//! look alike on screen deliberately: that is the point the corpus already
//! proves in `corpus/dsl-generated/variant-topology.md`, that a variant switch
//! and a `Visible` write reach the same committed layout by different routes.
//!
//! Before the scene seam carried an action (stories #573, #625) only the first
//! was reachable, because `Txn::set_variant` needs the arena and the scripted
//! phase is handed only a `LiveScene`.
//!
//! # What this scene still does not do
//!
//! **`VariantFlip` does not animate the switch.** FLIP needs the before and
//! after rect slices around the switch — which [`switch_variant`] has — plus an
//! `advance(dt)` and a commit composing its samples over the after layout
//! **once per frame**. The seam has no per-frame hook for a scene:
//! `LiveScene::tick` is the only thing the host calls per frame and it owns the
//! single commit, and an action is called once, on the key press. So the switch
//! lands in one frame rather than easing, and the rect deltas it produces are
//! not animated. Widening the seam to a per-frame scene driver is the change
//! issue #625 sketched and this story did not make.

use std::sync::OnceLock;

use dashlang::{
    AxisSizing, Channel, CrossAxisAlign, GridTrack, LayoutMode, LiveScene, MainAxisAlign, Scene,
    Signal, Spring, node,
};
use dashscene_core::{Arena, Prop, VariantMember, VariantSetId, VariantValue};

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

/// The variant set [`switch_variant`] cycles, kept here for the same reason
/// [`SHOW_MIDDLE`] is: a `VariantSetId` is an index into the arena's variant
/// table, assigned in declaration order, and this scene declares exactly one.
/// Every rebuild the host performs runs the same builder in the same order and
/// produces the same index, so the handle stored by the first build addresses
/// the set of every later arena as well.
static CHIP_SET: OnceLock<VariantSetId> = OnceLock::new();

/// How many members [`CHIP_SET`] has, so [`switch_variant`] can wrap without
/// asking the arena for a member list it already knows.
const CHIP_MEMBERS: usize = 3;

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

    declare_chip_variants(&mut painting, unit);

    painting.commit(&mut ShowcaseSolver::new(
        resources::new_typesetter(),
        resources::atlases(),
    ));
}

/// Declares the three-member variant set on the reflow row's rightmost chip.
///
/// Member 0 has no overrides, so the set is inert until [`switch_variant`]
/// moves it: the frame this build publishes is the authored one. The other two
/// members between them cover three of the six `VariantValue` cases —
/// `Width`, `Fill` and `Visible` — which is what makes this a variant switch
/// and not a recoloured rect: `Width` reflows the row, `Visible` takes the chip
/// out of the laid-out set entirely, and both are resolved through the arena's
/// variant overlay rather than through a `set_prop`.
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

/// The variant set this scene declares, once a build has declared one.
///
/// The host never needs it — that is the whole point of [`switch_variant`] —
/// but it is the handle `Arena::active_variant` takes, so it is what lets
/// anything outside this module ask which member is live.
pub fn variant_set() -> Option<VariantSetId> {
    CHIP_SET.get().copied()
}

/// The scene's own variant switch: advance the reflow row's rightmost chip to
/// the next member of its variant set, wrapping.
///
/// This is the whole of what a key does. The host calls it and constructs
/// nothing — it holds no `VariantSetId`, no member list, and no node name
/// (stories #573, #625).
///
/// # Why it commits, and why that is safe against the retained rect cache
///
/// `LiveScene` assumes it solely owns its arena's committed geometry between
/// ticks: a tick that solves nothing replays a retained rect cache, so a second
/// producer that moved a node would have the move reverted at the next
/// paint-only tick. This switch does move nodes, so the guarantee it relies on
/// is the one this scene is already built around — **every signal here drives a
/// layout-affecting channel**, so every tick that commits at all is a solving
/// tick, and a solve reads the arena's live variant overlay. `spread` binds
/// `Channel::Gap`, which `dashlang` always classifies as a solve, and
/// `show_middle` is a visibility binding, which always forces one. There is
/// therefore no tick that could replay a stale cache over this commit.
///
/// The active member is **not** carried across a scene rebuild. A resize
/// rebuilds the arena from scratch and the set comes back with member 0 active,
/// the same cost `demo`'s `Host::rebuild` already records for the scene's
/// springs.
///
/// The parameters are the seam's, not this function's needs: nothing here
/// touches `live`. A signal write and a variant switch go through one key
/// handler in the host, so they take one shape.
pub fn switch_variant(_live: &mut LiveScene, arena: &mut Arena) {
    let Some(&set) = CHIP_SET.get() else {
        // No build has run against any arena yet, so there is no set to
        // switch. Unreachable through the host, which builds before it takes
        // an event.
        return;
    };
    let next = (arena.active_variant(set) + 1) % CHIP_MEMBERS;
    let mut txn = arena.open();
    txn.set_variant(set, next);
    txn.commit_with(&mut ShowcaseSolver::new(
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
