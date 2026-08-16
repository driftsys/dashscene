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
//! **`VariantFlip` does not animate the switch, and the seam is no longer why.**
//! FLIP needs the before and after rect slices around the switch plus an
//! `advance(dt)` and a commit composing its samples over the after layout
//! **once per frame**, and `LiveScene::tick` does all of it for a **staged**
//! switch: it re-solves for the after layout, binds the transition the member
//! declares and composes the samples on the frames that follow (story #771).
//! [`switch_variant`] stages rather than commits since issue #950, so it goes
//! through that path.
//!
//! What is missing is one declaration. No member of this set carries a
//! `Txn::set_variant_transition`, and a member with none starts no track and
//! lands whole — so the switch still arrives in one frame, one tick after the
//! key. Declaring a transition on a member is now the whole of what animating it
//! would take, where before it needed the per-frame scene driver issue #625
//! sketched.

use std::sync::OnceLock;

use dashlang::{
    AxisSizing, Channel, CrossAxisAlign, GridTrack, LayoutMode, LiveScene, MainAxisAlign, Scene,
    Signal, Spring, node,
};
use dashscene_core::{Arena, VariantMember, VariantSetId, VariantValue};

use crate::badge;
use crate::resources;
use crate::vocabulary::{Painting, palette};
use dashpaint::{Stroke, StrokeAlign};

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
        .fill(palette::NAVY)
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
                        .fill(palette::PANEL)
                        .corners(radius)
                        .children((0..3).map(|i| {
                            node(match i {
                                0 => "column-a",
                                1 => "column-b",
                                _ => "column-c",
                            })
                            .size(panel_width - 2.0 * panel_gap, chip)
                            .fill(palette::SKY)
                            .corners(radius * 0.6)
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
                        .fill(palette::PANEL)
                        .corners(radius)
                        .child(
                            node("fill-fixed")
                                .size(chip * 1.4, chip * 2.0)
                                .fill(palette::AMBER)
                                .corners(radius * 0.6),
                        )
                        .child(
                            node("fill-a")
                                .size(chip, chip * 3.0)
                                .sizing_h(AxisSizing::Fill)
                                .fill(palette::MOSS)
                                .corners(radius * 0.6),
                        )
                        .child(
                            node("fill-b")
                                .size(chip, chip * 4.0)
                                .sizing_h(AxisSizing::Fill)
                                .fill(palette::MOSS)
                                .corners(radius * 0.6),
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
                        .fill(palette::PANEL)
                        .corners(radius)
                        .children((0..7).map(|i| {
                            node(WRAP_CHIPS[i])
                                .size(chip * 1.7, chip)
                                .fill(palette::VIOLET)
                                .corners(radius * 0.6)
                        })),
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
                .fill(palette::PANEL)
                .corners(radius)
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
                        .grid_row_span(2)
                        .fill(palette::VIOLET)
                        .corners(radius * 0.6),
                )
                .child(
                    node("grid-wide")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fill)
                        .grid_row(0)
                        .grid_column(1)
                        .grid_column_span(2)
                        .fill(palette::CRIMSON)
                        .corners(radius * 0.6),
                )
                .child(
                    node("grid-one")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fill)
                        .grid_row(1)
                        .grid_column(1)
                        .fill(palette::TEAL)
                        .corners(radius * 0.6),
                )
                .child(
                    node("grid-two")
                        .size(120.0 * unit, 40.0 * unit)
                        .grid_row(1)
                        .grid_column(2)
                        .fill(palette::TEAL)
                        .corners(radius * 0.6),
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
                .fill(palette::PANEL)
                .corners(radius)
                .child(
                    node("reflow-a")
                        .size(120.0 * unit, reflow_height * 0.5)
                        .fill(palette::AMBER)
                        .corners(radius * 0.6),
                )
                .child(
                    node("reflow-b")
                        .size(120.0 * unit, reflow_height * 0.5)
                        .visible_when(show_middle)
                        .fill(palette::CRIMSON)
                        .corners(radius * 0.6)
                        // Outlined as well as filled, so the chip that comes
                        // and goes is still identifiable in a still frame
                        // taken while it is present.
                        .stroke(Stroke {
                            width: 2.0 * unit,
                            align: StrokeAlign::Inside,
                            color: palette::NEAR_WHITE,
                        }),
                )
                .child(
                    node("reflow-c")
                        .size(120.0 * unit, reflow_height * 0.5)
                        .fill(palette::AMBER)
                        .corners(radius * 0.6),
                )
                .child(
                    node("reflow-d")
                        .size(120.0 * unit, reflow_height * 0.5)
                        .fill(palette::AMBER)
                        .corners(radius * 0.6),
                ),
        );

    let label = badge::badge(&mut scene, width, height);
    scene.roots([root, label]);
    let live = scene.build_live(arena, Box::new(resources::solver()));
    paint(arena, unit);
    live
}

const WRAP_CHIPS: [&str; 7] = [
    "wrap-0", "wrap-1", "wrap-2", "wrap-3", "wrap-4", "wrap-5", "wrap-6",
];

/// Stages the one construct the builder above cannot carry: the variant set,
/// which needs the arena `add_variant_set` issues its handle against.
/// Everything else this scene paints is authored on the builder.
fn paint(arena: &mut Arena, unit: f32) {
    let mut painting = Painting::open(arena);

    declare_chip_variants(&mut painting, unit);

    painting.commit(&mut resources::solver());
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
/// # Why it stages and does not commit
///
/// The switch is published by the **next tick**, through the staged-variant
/// seam `LiveScene::tick` already carries: it compares each set's active member
/// against its own snapshot, re-solves through its own solver for the layout the
/// new members produce, and binds whatever transition the member declares
/// (story #771). Staging is all a producer owes it — `Txn::set_variant` is
/// visible to the solver the moment it is staged (P3), and `Txn` has no `Drop`
/// that reverts, so leaving the transaction uncommitted leaves the switch on the
/// arena for that tick to find.
///
/// It used to commit here, through a solver of its own. That was correct only
/// while the scene's solver rebuilt Taffy's tree on every solve. A retained
/// solver patches its tree from the arena's layout-dirty set, and a commit
/// consumes that set — so this commit took the dirty set naming the chip, and
/// the scene's own solver then patched nothing and replayed a tree still holding
/// the pre-switch width. `demo`'s `the_switch_survives_the_ticks_and_pulses_that_follow_it`
/// is what caught it (issue #950, `docs/decisions/one-solver-per-live-scene.md`).
///
/// **What moved and what did not.** The switch is committed one tick later than
/// it used to be, not one frame slower to appear: this scene declares no
/// `set_variant_transition`, so the tick that finds it starts no track and lands
/// it whole, exactly as the commit here did. `demo` is the only host that
/// reaches this at all — `demo-web` and `demo-android` run the scripted pulse
/// and take no key — and its handler returns `Reaction::Redraw`, which forces a
/// frame, which ticks before it presents. So nothing on screen waits. Declaring
/// a transition on a member is now all it would take for this switch to animate,
/// which is the point of routing it through the seam rather than around it.
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
    // The staged member, not the committed one, so two presses inside one frame
    // advance two members rather than landing on the same one twice.
    let next = (arena.active_variant(set) + 1) % CHIP_MEMBERS;
    let mut txn = arena.open();
    txn.set_variant(set, next);
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
