//! Story #166 acceptance: the reactive layer — signals, bindings,
//! transforms, and the per-frame flush (docs/wip/2026-07-13-reactive-bindings-spec.md;
//! docs/archive/2026-07-14-scope-decisions.md §23, cases A1–A4).
//!
//! The four cases span the axes that matter: scalar versus discrete
//! channels, contained versus propagating writes, and high versus low
//! frequency. A1/A2 assert *no solve*; A3/A4 assert a real reflow whose
//! moved rects reach the dirty set.

use std::cell::Cell;
use std::rc::Rc;

use dashlang::{Arena, AxisSizing, Channel, FormatSpec, LayoutMode, Scene, Spring, node};
use dashscene_core::{LayoutSolver, NodeId, SolvedRect};
use dashscene_engine::TaffySolver;

/// Wraps the real Taffy solve and counts how many times it runs. The
/// oracle for "no layout solve": a contained or paint-only tick must not
/// increment this. Core is unchanged — the "no solve" decision lives in
/// `dashlang`, so it is observable exactly here.
struct CountingSolver {
    inner: TaffySolver<'static>,
    count: Rc<Cell<u32>>,
}

impl CountingSolver {
    fn boxed(count: Rc<Cell<u32>>) -> Box<dyn LayoutSolver> {
        Box::new(CountingSolver {
            inner: TaffySolver::new(),
            count,
        })
    }
}

impl LayoutSolver for CountingSolver {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        self.count.set(self.count.get() + 1);
        self.inner.solve(arena)
    }
}

// ---------------------------------------------------------------------
// A1 — a contained, high-frequency scalar write performs no layout solve.
// ---------------------------------------------------------------------
#[test]
fn a1_contained_scalar_write_performs_no_layout_solve() {
    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let speed = scene.signal(0.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            // A gauge bar: its width fills every frame, but every
            // ancestor is a fixed passthrough box, so the write cannot
            // escape its own rect.
            node("bar")
                .mode(LayoutMode::None)
                .size(0.0, 12.0)
                .bind(Channel::Width, speed.map(|v| v * 2.0)),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));

    // The initial build solves once.
    assert_eq!(count.get(), 1, "build solves once");
    assert_eq!(arena.committed().rects()[1].w, 0.0, "seeded from signal 0");

    for frame in 0..10 {
        live.set(speed, frame as f32);
        live.tick(0.016, &mut arena);
    }

    assert_eq!(count.get(), 1, "no solve on any contained scalar tick");
    let bar = arena.committed().rects()[1];
    assert_eq!(bar.w, 18.0, "final width 9 * 2");
    assert_eq!((bar.x, bar.y, bar.h), (0.0, 0.0, 12.0), "only width moved");
    // The write is in the dirty set — a painter must re-upload it.
    assert!(arena.committed().dirty().contains(&1));
}

// ---------------------------------------------------------------------
// A2 — a contained, high-frequency discrete (text) write is paint-only.
// ---------------------------------------------------------------------
#[test]
fn a2_contained_text_write_is_paint_only() {
    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let value = scene.signal(0.0f32);
    scene.roots([node("box").mode(LayoutMode::None).size(120.0, 40.0).child(
        // A numeric readout inside a fixed box: the string changes
        // every frame, but the box does not resize, so no solve.
        node("readout")
            .size(120.0, 40.0)
            .bind_text(value.map(|v| format!("{v:.0}"))),
    )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1);

    for frame in 1..=5 {
        live.set(value, frame as f32 * 10.0);
        live.tick(0.016, &mut arena);
    }

    assert_eq!(count.get(), 1, "text in a fixed box never re-solves");
    let readout = arena.committed().node_of(1);
    assert_eq!(arena.text(readout), Some("50"), "text tracks the signal");
}

// ---------------------------------------------------------------------
// A3 — a Visible flip reflows siblings; the moved rects reach the dirty
//      set even though no binding wrote them.
// ---------------------------------------------------------------------
#[test]
fn a3_visible_flip_reflows_siblings_into_the_dirty_set() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let show_middle = scene.signal(true);
    scene.roots([node("col")
        .mode(LayoutMode::Vertical)
        .gap(10.0)
        .size(100.0, 300.0)
        .children([
            node("a").size(100.0, 50.0),
            node("b").size(100.0, 50.0).visible_when(show_middle),
            node("c").size(100.0, 50.0),
        ])]);

    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    // With b visible: a@0, b@60, c@120.
    assert_eq!(arena.committed().rects()[3].y, 120.0);

    // Hide b; c must slide up to fill the gap.
    live.set(show_middle, false);
    live.tick(0.016, &mut arena);

    let rects = arena.committed().rects();
    assert_eq!(rects[3].y, 60.0, "c reflowed up");
    assert_eq!((rects[2].w, rects[2].h), (0.0, 0.0), "b is degenerate");

    // c moved but no binding wrote it — its rect must come from the
    // solver's dirty report, not the binding flush.
    let dirty = arena.committed().dirty();
    assert!(dirty.contains(&3), "moved sibling c is dirty: {dirty:?}");
    assert!(dirty.contains(&2), "hidden b is dirty: {dirty:?}");
}

// ---------------------------------------------------------------------
// A4 — a bounded pool's members toggle Visible independently; the
//      hugging container collapses around them.
// ---------------------------------------------------------------------
#[test]
fn a4_bounded_pool_hugging_container_collapses() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let s0 = scene.signal(false);
    let s1 = scene.signal(false);
    let s2 = scene.signal(false);
    scene.roots([node("stack")
        .mode(LayoutMode::Vertical)
        .sizing_h(AxisSizing::Fixed)
        .sizing_v(AxisSizing::Hug)
        .size(100.0, 0.0)
        .children([
            node("i0").size(100.0, 20.0).visible_when(s0),
            node("i1").size(100.0, 20.0).visible_when(s1),
            node("i2").size(100.0, 20.0).visible_when(s2),
        ])]);

    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let height = |arena: &Arena| arena.committed().rects()[0].h;

    // All members hidden: the container hugs to nothing.
    assert_eq!(height(&arena), 0.0);

    live.set(s1, true);
    live.tick(0.016, &mut arena);
    assert_eq!(height(&arena), 20.0, "one member visible");

    live.set(s0, true);
    live.tick(0.016, &mut arena);
    assert_eq!(height(&arena), 40.0, "two members visible");

    live.set(s0, false);
    live.set(s1, false);
    live.tick(0.016, &mut arena);
    assert_eq!(height(&arena), 0.0, "collapses back to nothing");
}

// ---------------------------------------------------------------------
// A size write to a flex container is NOT single-rect: it redistributes
// the container's children, so it must re-solve rather than patch only
// the container's own cached rect.
// ---------------------------------------------------------------------
#[test]
fn a_size_write_to_a_flex_container_reflows_its_children() {
    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let width = scene.signal(100.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(400.0, 50.0)
        .child(
            // The row is ancestor-contained (its parent is a fixed
            // passthrough), but it is a flex container, so growing it must
            // reflow its two Fill children.
            node("row")
                .mode(LayoutMode::Horizontal)
                .size(100.0, 50.0)
                .bind(Channel::Width, width)
                .children([
                    node("l")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fixed)
                        .size(0.0, 50.0),
                    node("r")
                        .sizing_h(AxisSizing::Fill)
                        .sizing_v(AxisSizing::Fixed)
                        .size(0.0, 50.0),
                ]),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1);
    assert_eq!(arena.committed().rects()[2].w, 50.0, "children split 100");

    live.set(width, 200.0);
    live.tick(0.016, &mut arena);

    assert!(count.get() > 1, "a flex-container size write re-solves");
    let rects = arena.committed().rects();
    assert_eq!(rects[1].w, 200.0, "row grew");
    assert_eq!(rects[2].w, 100.0, "left child reflowed to half of 200");
    assert_eq!(rects[3].w, 100.0, "right child reflowed to half of 200");
}

// ---------------------------------------------------------------------
// Smoothing: a bound spring drives a contained channel over frames, and
// the drive stays a no-solve path.
// ---------------------------------------------------------------------
#[test]
fn smoothing_drives_a_contained_channel_without_solving() {
    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let target = scene.signal(0.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            node("bar")
                .mode(LayoutMode::None)
                .size(0.0, 12.0)
                .bind(Channel::Width, target)
                .smooth(Channel::Width, Spring::critically_damped(0.1)),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));

    live.set(target, 100.0);
    live.tick(0.016, &mut arena);
    let w1 = arena.committed().rects()[1].w;
    assert!(w1 > 0.0 && w1 < 100.0, "spring is mid-flight: {w1}");

    for _ in 0..300 {
        live.tick(0.016, &mut arena);
    }
    let wn = arena.committed().rects()[1].w;
    assert!(
        (wn - 100.0).abs() < 1.0,
        "spring settles near the target: {wn}"
    );
    assert_eq!(count.get(), 1, "the whole smoothed drive is contained");
}

// ---------------------------------------------------------------------
// The declarative transform vocabulary (D8), seeded at build.
// ---------------------------------------------------------------------
#[test]
fn declarative_transforms_apply() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let s = scene.signal(5.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(500.0, 100.0)
        .children([
            node("scale")
                .mode(LayoutMode::None)
                .size(0.0, 10.0)
                .bind(Channel::Width, s.scale(3.0)),
            node("range")
                .mode(LayoutMode::None)
                .size(0.0, 10.0)
                .bind(Channel::Width, s.map_range(0.0, 10.0, 0.0, 100.0)),
            node("clamp")
                .mode(LayoutMode::None)
                .size(0.0, 10.0)
                .bind(Channel::Width, s.clamp(0.0, 4.0)),
        ])]);

    let _live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let rects = arena.committed().rects();
    assert_eq!(rects[1].w, 15.0, "scale: 5 * 3");
    assert_eq!(rects[2].w, 50.0, "map_range: 5 of [0,10] to [0,100]");
    assert_eq!(rects[3].w, 4.0, "clamp: 5 into [0,4]");
}

#[test]
fn format_transform_renders_text() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let speed = scene.signal(87.0f32);
    scene.roots([node("box").mode(LayoutMode::None).size(100.0, 20.0).child(
        node("label")
            .size(100.0, 20.0)
            .bind_text(speed.format(FormatSpec::new("", 0, " km/h"))),
    )]);

    let _live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let label = arena.committed().node_of(1);
    assert_eq!(arena.text(label), Some("87 km/h"));
}

// ---------------------------------------------------------------------
// One signal, two targets — a binding graph is data-to-prop, and the
// same datum can feed two nodes (D2).
// ---------------------------------------------------------------------
#[test]
fn one_signal_binds_two_nodes() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let w = scene.signal(0.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(400.0, 40.0)
        .children([
            node("left")
                .mode(LayoutMode::None)
                .size(0.0, 10.0)
                .bind(Channel::Width, w),
            node("right")
                .mode(LayoutMode::None)
                .size(0.0, 10.0)
                .bind(Channel::Width, w.scale(2.0)),
        ])]);

    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    live.set(w, 30.0);
    live.tick(0.016, &mut arena);

    let rects = arena.committed().rects();
    assert_eq!(rects[1].w, 30.0, "identity");
    assert_eq!(rects[2].w, 60.0, "scaled");
}

// ---------------------------------------------------------------------
// Story #167 — the completed channel vocabulary (debt #201), the
// smooth-without-bind diagnostic (debt #194), named signals, the staged
// core binding table, and the loader-side attach.
// ---------------------------------------------------------------------

/// Mirrors A1 for a fill channel (debt #201): a bound fill component
/// changes every frame, and the frame is paint-only — no layout solve —
/// while the other components of the authored color hold their values.
#[test]
fn a_fill_channel_write_is_paint_only() {
    use dashlang::rgba;
    use dashscene_core::FillSpec;

    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let level = scene.signal(0.25f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            node("meter")
                .size(200.0, 20.0)
                .fill(rgba(0.0, 0.5, 0.25, 1.0))
                .bind(Channel::FillR, level),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1, "build solves once");

    for frame in 1..=4 {
        live.set(level, frame as f32 * 0.2);
        live.tick(0.016, &mut arena);
    }
    assert_eq!(count.get(), 1, "a fill write never re-solves");

    let meter = arena.committed().node_of(1);
    match arena.fill(meter) {
        Some(FillSpec::Solid { color }) => {
            assert_eq!(color.r, 0.8, "the bound component tracks the signal");
            assert_eq!(
                (color.g, color.b, color.a),
                (0.5, 0.25, 1.0),
                "unbound components keep the authored color"
            );
        }
        other => panic!("expected a solid fill, got {other:?}"),
    }
    // The paint change reaches the dirty set — a painter must re-upload.
    assert!(arena.committed().dirty().contains(&1));
}

/// Mirrors A1 for the opacity channel (debt #253): a bound group opacity
/// changes every frame, and the frame is paint-only — no layout solve
/// (`docs/decisions/masks-and-group-opacity.md`). Opacity is the paint
/// side of the §23 split; its pair `Visible` is layout-affecting.
#[test]
fn an_opacity_channel_write_is_paint_only() {
    use dashlang::rgba;

    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let level = scene.signal(1.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            node("meter")
                .size(200.0, 20.0)
                .fill(rgba(0.0, 0.5, 0.25, 1.0))
                .bind(Channel::Opacity, level),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1, "build solves once");

    for frame in 1..=4 {
        live.set(level, frame as f32 * 0.2);
        live.tick(0.016, &mut arena);
    }
    assert_eq!(count.get(), 1, "an opacity write never re-solves");

    let meter = arena.committed().node_of(1);
    assert_eq!(
        arena.opacity(meter),
        0.8,
        "the bound opacity tracks the signal"
    );
    // The paint change reaches the dirty set — a painter must re-upload.
    assert!(arena.committed().dirty().contains(&1));
}

/// Mirrors A1's counterpart for `Gap` (debt #201): a bound gap is
/// layout-affecting by definition — the children move on every write.
#[test]
fn a_gap_binding_reflows_the_containers_children() {
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let gap = scene.signal_named("size/gap", 10.0);
    scene.roots([node("column")
        .mode(LayoutMode::Vertical)
        .size(100.0, 300.0)
        .bind(Channel::Gap, gap)
        .children((0..2).map(|_| node("item").size(100.0, 20.0)))]);

    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));
    assert_eq!(
        arena.committed().rects()[2].y,
        30.0,
        "seeded gap 10: second item at 20 + 10"
    );

    live.set(gap, 40.0);
    live.tick(0.016, &mut arena);
    assert_eq!(
        arena.committed().rects()[2].y,
        60.0,
        "gap 40: second item at 20 + 40"
    );
}

/// Debt #194: a smoothing spec with no binding on the same channel is
/// named at build, never silently discarded.
#[test]
#[should_panic(expected = "has no matching bind")]
fn smooth_without_a_matching_bind_is_refused_by_name() {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let _speed = scene.signal(0.0f32);
    scene.roots([node("bar")
        .size(10.0, 10.0)
        .smooth(Channel::Width, Spring::critically_damped(0.1))]);
    scene.build_live(&mut arena, Box::new(TaffySolver::new()));
}

/// A fill-channel binding writes one component of a solid color and
/// stages the whole color as `Prop::Fill`, which replaces the node's
/// fill slot outright. On a node that also authored `fill_with(...)`,
/// the binding's build-time seed would therefore erase the gradient
/// before the first frame is ever drawn, and nothing would report it.
/// Refused by name, like an inert spring (P4).
#[test]
#[should_panic(expected = "cannot be combined")]
fn fill_with_plus_a_fill_channel_binding_is_refused_by_name() {
    use dashlang::{FillSpec, Gradient, GradientKind, GradientStop, StopRange, Vec2, rgba};

    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let alpha = scene.signal(0.5f32);
    scene.roots([node("panel")
        .size(100.0, 100.0)
        .fill_with(FillSpec::Gradient {
            gradient: Gradient {
                kind: GradientKind::Linear,
                handle_origin: Vec2 { x: 0.0, y: 0.0 },
                handle_primary: Vec2 { x: 1.0, y: 0.0 },
                handle_secondary: Vec2 { x: 0.0, y: 1.0 },
                stops: StopRange::NONE,
            },
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: rgba(1.0, 0.0, 0.0, 1.0),
                },
                GradientStop {
                    offset: 1.0,
                    color: rgba(0.0, 0.0, 1.0, 1.0),
                },
            ],
        })
        .bind(Channel::FillA, alpha)]);
    scene.build_live(&mut arena, Box::new(TaffySolver::new()));
}

/// Story #167: every declarative binding is staged into the arena as a
/// document-construct row; a `Custom` closure binding stays out (D8).
#[test]
fn declarative_bindings_are_staged_into_the_arena_tables() {
    use dashscene_core::{Channel as CoreChannel, ScalarTransform};

    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let gap = scene.signal_named("size/gap", 16.0);
    let anon = scene.signal(1.0f32);
    scene.roots([node("card")
        .mode(LayoutMode::Vertical)
        .size(100.0, 100.0)
        .bind(Channel::Gap, gap)
        .child(
            node("chip")
                .size(10.0, 10.0)
                .bind(Channel::Width, anon.scale(2.0))
                // The closure stays dashlang-only: no row for it.
                .bind(Channel::Height, anon.map(|v| v + 1.0)),
        )]);
    scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let signals = arena.signals();
    assert_eq!(signals.len(), 2);
    assert_eq!(signals[0].name.as_deref(), Some("size/gap"));
    assert_eq!(signals[0].initial, 16.0);
    assert_eq!(signals[1].name, None);

    let rows = arena.bindings();
    assert_eq!(rows.len(), 2, "the Custom binding stages no row");
    assert_eq!(rows[0].channel, CoreChannel::Gap);
    assert_eq!(rows[0].transform, ScalarTransform::Identity);
    assert_eq!(rows[1].channel, CoreChannel::Width);
    assert_eq!(rows[1].transform, ScalarTransform::Scale(2.0));
}

/// Story #167: an arena that already carries binding tables — the shape
/// `load_document` leaves after loading an imported `.dsb` — attaches
/// into a `LiveScene` whose signals are addressable by name, drive the
/// same flush, and re-seed the bound channels.
#[test]
fn attach_live_drives_a_loaded_arenas_bindings() {
    use dashlang::attach_live;
    use dashscene_core::{Channel as CoreChannel, Color, FillSpec, Prop, ScalarTransform};

    // Stage what a loaded document would have staged: nodes, literals,
    // and the binding tables.
    let mut arena = Arena::new();
    let (card, chip) = {
        let mut txn = arena.open();
        let card = txn.add_node(None, Some("card"));
        txn.set_prop(card, Prop::Width(100.0));
        txn.set_prop(card, Prop::Height(300.0));
        txn.set_prop(card, Prop::Mode(dashscene_core::LayoutMode::Vertical));
        txn.set_prop(card, Prop::Gap(16.0));
        let chip = txn.add_node(Some(card), Some("chip"));
        txn.set_prop(chip, Prop::Width(24.0));
        txn.set_prop(chip, Prop::Height(24.0));
        txn.set_prop(
            chip,
            Prop::Fill(Color {
                r: 0.13,
                g: 0.45,
                b: 0.9,
                a: 1.0,
            }),
        );
        let filler = txn.add_node(Some(card), Some("filler"));
        txn.set_prop(filler, Prop::Width(24.0));
        txn.set_prop(filler, Prop::Height(24.0));

        let gap = txn.declare_signal(Some("size/gap"), 16.0);
        let accent_r = txn.declare_signal(Some("color/accent.r"), 0.13);
        txn.bind(card, CoreChannel::Gap, gap, ScalarTransform::Identity);
        txn.bind(
            chip,
            CoreChannel::FillR,
            accent_r,
            ScalarTransform::Identity,
        );
        txn.commit();
        (card, chip)
    };
    let _ = card;

    let mut live = attach_live(&mut arena, Box::new(TaffySolver::new()));

    // The document's signals are addressable by their authored names.
    let gap = live.signal_named("size/gap").expect("size/gap is declared");
    assert!(live.signal_named("no/such/name").is_none());

    // The attach seeded from the initials: gap 16 places the second
    // child at 24 + 16.
    assert_eq!(arena.committed().rects()[2].y, 40.0);

    live.set(gap, 30.0);
    live.tick(0.016, &mut arena);
    assert_eq!(arena.committed().rects()[2].y, 54.0, "gap 30: 24 + 30");

    let accent_r = live
        .signal_named("color/accent.r")
        .expect("color/accent.r is declared");
    live.set(accent_r, 0.4);
    live.tick(0.016, &mut arena);
    match arena.fill(chip) {
        Some(FillSpec::Solid { color }) => {
            assert_eq!(color.r, 0.4, "the bound component tracks the signal");
            assert_eq!(color.g, 0.45, "unbound components keep the literal");
        }
        other => panic!("expected a solid fill, got {other:?}"),
    }
}

/// The authoring-side mirror of the load gate's `signal.name-duplicate`
/// (probed at review, C4): a second `signal_named` under one name would
/// make the by-name lookup silently shadow the first declaration, so it
/// is refused at the declaration, by name — the #194 pattern.
#[test]
#[should_panic(expected = "is already declared")]
fn a_duplicate_named_signal_is_refused_at_declaration() {
    let mut scene = Scene::new();
    let _first = scene.signal_named("size/gap", 16.0);
    let _second = scene.signal_named("size/gap", 24.0);
}

// ---------------------------------------------------------------------
// An idle tick commits nothing, and a set to the current value is idle.
// ---------------------------------------------------------------------
#[test]
fn an_idle_tick_holds_the_generation_and_an_unchanged_set_is_a_no_op() {
    let count = Rc::new(Cell::new(0));
    let mut arena = Arena::new();

    let mut scene = Scene::new();
    let speed = scene.signal(5.0f32);
    scene.roots([node("frame")
        .mode(LayoutMode::None)
        .size(200.0, 20.0)
        .child(
            node("bar")
                .mode(LayoutMode::None)
                .size(0.0, 12.0)
                .bind(Channel::Width, speed.map(|v| v * 2.0)),
        )]);

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    let g0 = live.generation();

    // An idle tick — no signal changed, no live track — commits nothing, so
    // the generation holds steady and no solve runs (#203).
    let g1 = live.tick(0.016, &mut arena);
    assert_eq!(g1, g0, "an idle tick does not bump the generation");
    assert_eq!(count.get(), 1, "an idle tick does not solve");

    // A set to the signal's current value is a no-op, so the next tick is
    // still idle (#193).
    live.set(speed, 5.0);
    let g2 = live.tick(0.016, &mut arena);
    assert_eq!(
        g2, g1,
        "a set to the current value does not bump the generation"
    );

    // A real change bumps the generation and moves the bound rect (still no
    // solve — the write is contained).
    live.set(speed, 6.0);
    let g3 = live.tick(0.016, &mut arena);
    assert_ne!(g3, g2, "a real change bumps the generation");
    assert_eq!(
        count.get(),
        1,
        "the contained change still performs no solve"
    );
    assert_eq!(arena.committed().rects()[1].w, 12.0, "width tracks 6 * 2");
}

/// The rotation channels bind through the **builder** path, not only through
/// a loaded document (story #770).
///
/// `attach_live` — the loaded-`.dsb` path — seeds a rotation shadow and routes
/// the three channels through it. `stage_live`, the path a `dashlang`-authored
/// scene takes, did not, so binding any rotation channel here panicked twice
/// over: `initial_channel_value`'s wildcard treated it as a fill channel and
/// hit `fill_component`'s `unreachable!`, and past that `seed_scalar` found no
/// seeded shadow. A spinner authored in the DSL is the canonical case this
/// vocabulary exists for, so it is the one that must not panic.
#[test]
fn a_rotation_binding_drives_the_nodes_angle_from_the_builder() {
    use dashlang::rgba;
    let mut arena = Arena::new();
    let count = Rc::new(Cell::new(0));
    let mut scene = Scene::new();
    let angle = scene.signal(0.0);
    scene.roots(
        [node("dial").mode(LayoutMode::None).size(100.0, 20.0).child(
            node("needle")
                .size(100.0, 20.0)
                .fill(rgba(0.9, 0.2, 0.1, 1.0))
                .bind(Channel::Rotation, angle),
        )],
    );

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1, "build solves once");

    live.set(angle, 0.5);
    live.tick(0.016, &mut arena);

    assert_eq!(
        count.get(),
        1,
        "rotation is paint-only, so a rotation write never re-solves",
    );

    let needle = arena.committed().node_of(1);
    assert_eq!(
        arena.rotation(needle).0,
        0.5,
        "the bound angle tracks the signal",
    );
    // Asserted on the committed rect, not only on the arena's staged intent:
    // a rotation that never reached the rect leaves the painter drawing the
    // node upright.
    assert_eq!(arena.committed().rects()[1].rotation, 0.5);
    assert!(
        arena.committed().dirty().contains(&1),
        "the rotation change reaches the dirty set, so a painter re-uploads",
    );
}

/// The anchor components bind on the same path, and a binding that drives only
/// the angle must keep the anchor the scene authored rather than inventing
/// one — the whole reason `stage_live` needs a rotation shadow at all.
#[test]
fn a_rotation_binding_keeps_the_anchor_it_did_not_drive() {
    use dashlang::rgba;
    let mut arena = Arena::new();
    let count = Rc::new(Cell::new(0));
    let mut scene = Scene::new();
    let angle = scene.signal(0.0);
    let anchor_x = scene.signal(7.0);
    scene.roots(
        [node("dial").mode(LayoutMode::None).size(100.0, 20.0).child(
            node("needle")
                .size(100.0, 20.0)
                .fill(rgba(0.9, 0.2, 0.1, 1.0))
                .bind(Channel::Rotation, angle)
                .bind(Channel::RotationAnchorX, anchor_x),
        )],
    );

    let mut live = scene.build_live(&mut arena, CountingSolver::boxed(count.clone()));

    live.set(angle, 1.25);
    live.tick(0.016, &mut arena);

    let needle = arena.committed().node_of(1);
    let (got_angle, got_anchor) = arena.rotation(needle);
    assert_eq!(got_angle, 1.25, "the angle followed its own signal");
    assert_eq!(
        got_anchor.0, 7.0,
        "driving the angle did not reset the anchor its own binding seeded",
    );
}

/// A variant switch must leave every node it did not move in the retained
/// cache (story #771, found by review on PR #865).
///
/// `LayoutSolver::solve` is allowed to report only the nodes whose rect
/// changed, and `TaffySolver` does exactly that. Adopting its result as the
/// cache therefore dropped every unmoved node — and a switch tick sets no
/// `layout_dirty`, so nothing rebuilt it. The next contained write to a
/// dropped node panicked on `cached_index`.
///
/// The shape matters and is why nothing caught it: the scene needs a variant
/// set **and** an unrelated bound node that the switch does not move. Every
/// test written with the feature had only one or the other.
#[test]
fn a_variant_switch_leaves_an_untouched_bound_node_writable() {
    use dashscene_core::{ScalarTransform, VariantMember, VariantValue};

    let mut arena = Arena::new();
    let (set, bar) = {
        let mut txn = arena.open();

        // A flex row whose variant switch widens its first chip. Everything
        // here moves when the switch lands.
        let shelf = txn.add_node(None, Some("shelf"));
        txn.set_prop(shelf, dashscene_core::Prop::Width(200.0));
        txn.set_prop(shelf, dashscene_core::Prop::Height(40.0));
        txn.set_prop(shelf, dashscene_core::Prop::Mode(LayoutMode::Horizontal));
        let chip = txn.add_node(Some(shelf), Some("chip"));
        txn.set_prop(chip, dashscene_core::Prop::Width(40.0));
        txn.set_prop(chip, dashscene_core::Prop::Height(40.0));

        // A separate, fixed passthrough frame the switch never touches, with
        // one contained width binding inside it — the node the solve will
        // not report, and the one that used to vanish from the cache.
        let frame = txn.add_node(None, Some("gauge"));
        txn.set_prop(frame, dashscene_core::Prop::Width(200.0));
        txn.set_prop(frame, dashscene_core::Prop::Height(20.0));
        txn.set_prop(frame, dashscene_core::Prop::Mode(LayoutMode::None));
        let bar = txn.add_node(Some(frame), Some("bar"));
        txn.set_prop(bar, dashscene_core::Prop::Width(0.0));
        txn.set_prop(bar, dashscene_core::Prop::Height(12.0));
        let signal = txn.declare_signal(Some("bar.width"), 0.0);
        txn.bind(bar, Channel::Width, signal, ScalarTransform::Identity);

        let set = txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("wide".to_owned()),
                overrides: vec![(chip, VariantValue::Width(120.0))],
            },
        ]);
        txn.commit();
        (set, bar)
    };

    let mut live = dashlang::attach_live(&mut arena, Box::new(TaffySolver::new()));

    // The switch, staged the way an embedder stages it.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
    }
    live.tick(0.1, &mut arena);

    // The write that used to panic: an ordinary contained write to a node the
    // switch never moved.
    let width = live
        .signal_named("bar.width")
        .expect("the binding's signal is named");
    live.set(width, 48.0);
    live.tick(0.1, &mut arena);

    let scene = arena.committed();
    let index = (0..scene.rects().len())
        .find(|i| scene.node_of(*i as u32) == bar)
        .expect("the bar is still in the committed table");
    assert_eq!(
        scene.rects()[index].w,
        48.0,
        "a node the switch did not move still takes a contained write",
    );
}

/// The node the builder gave `name`, through the committed table — the only
/// way back to a `NodeId` for a tree the `node!` builder authored.
fn node_named(arena: &Arena, name: &str) -> NodeId {
    let committed = arena.committed();
    (0..committed.rects().len() as u32)
        .map(|i| committed.node_of(i))
        .find(|&n| arena.name(n) == Some(name))
        .unwrap_or_else(|| panic!("no node is named {name}"))
}

/// One node's committed rect as `(x, y, w)`.
fn committed_rect(arena: &Arena, node: NodeId) -> (f32, f32, f32) {
    let committed = arena.committed();
    let i = (0..committed.rects().len() as u32)
        .find(|&i| committed.node_of(i) == node)
        .expect("the node is in the committed table");
    let r = &committed.rects()[i as usize];
    (r.x, r.y, r.w)
}

/// A variant switch and a layout-forcing write in the same tick must still
/// ease (story #771, finding 3 on PR #865).
///
/// `layout_dirty` takes priority over the switch branch in `LiveScene::tick`,
/// and that branch commits through the real solver — whose answer is the
/// *destination* layout. This frame's FLIP samples were discarded, so the
/// node snapped to its endpoint; the track stayed live, so the next frame
/// patched the cache back to an early sample and the node rewound and
/// re-animated from there.
///
/// The shape is why nothing caught it: the scene needs a variant transition
/// **and** a write that forces a solve in the same tick. Every other test
/// written with the feature has one or the other.
#[test]
fn a_switch_and_a_layout_dirty_write_in_one_tick_still_eases() {
    use dashscene_core::{
        Easing, PropTransition, TransitionSpec, VariantMember, VariantTransition, VariantValue,
    };

    let mut arena = Arena::new();

    // Two independent roots. The shelf is what the switch animates; the
    // column holds the `Visible` binding that forces the solve, in a subtree
    // the switch never touches — so the travelling node's endpoints stay
    // fixed and the only thing under test is whether this frame's sample
    // survives the commit.
    let mut scene = Scene::new();
    let show_b = scene.signal(true);
    scene.roots([
        node("shelf")
            .mode(LayoutMode::Horizontal)
            .size(200.0, 40.0)
            .children([
                node("left").size(40.0, 40.0),
                node("right").size(40.0, 40.0),
            ]),
        node("col")
            .mode(LayoutMode::Vertical)
            .gap(10.0)
            .size(100.0, 300.0)
            .children([
                node("a").size(100.0, 50.0),
                node("b").size(100.0, 50.0).visible_when(show_b),
                node("c").size(100.0, 50.0),
            ]),
    ]);

    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let left = node_named(&arena, "left");
    let right = node_named(&arena, "right");
    let right_x = |arena: &Arena| -> f32 { committed_rect(arena, right).0 };

    let (from, to) = (40.0_f32, 120.0_f32);
    assert_eq!(right_x(&arena), from, "the travelling node starts at 40");

    // Widening `left` pushes `right` from 40 to 120 over one second. The
    // set is staged after the build because the builder has no variant
    // vocabulary — this is the seam a producer stages one through.
    let set = {
        let mut txn = arena.open();
        let set = txn.add_variant_set(vec![
            VariantMember::default(),
            VariantMember {
                name: Some("wide".to_owned()),
                overrides: vec![(left, VariantValue::Width(120.0))],
            },
        ]);
        txn.set_variant_transition(
            set,
            1,
            VariantTransition {
                tracks: vec![PropTransition {
                    node: right,
                    channel: Channel::X,
                    spec: TransitionSpec::Tween {
                        duration: 1.0,
                        easing: Easing::Linear,
                    },
                }],
                stagger: 0.0,
            },
        );
        set
    };
    // One tick to register the set at member 0, so the switch below reads as
    // a switch rather than as a set adopted mid-flight.
    live.tick(0.1, &mut arena);
    assert_eq!(right_x(&arena), from, "registering the set moves nothing");

    // The switch and the layout-forcing write, in one tick.
    {
        let mut txn = arena.open();
        txn.set_variant(set, 1);
    }
    live.set(show_b, false);
    // One frame's worth: `MAX_FRAME_DELTA` clamps a tick to 0.1 s, so a
    // one-second tween takes ten of them however large the interval given.
    live.tick(0.1, &mut arena);

    let first = right_x(&arena);
    assert!(
        first > from && first < to,
        "the switch frame publishes a sample strictly between the endpoints, not the \
         destination: {first} is not in ({from}, {to})",
    );

    // The node the switch widened, which no track names, lands in the same
    // commit. Step 0's solve is the only one that reports it — the retained
    // solver reports a node once — so without the switch's rects written
    // back, `commit_with` carries its pre-switch width forward and the cache
    // is rebuilt from that, leaving it wrong for good.
    assert_eq!(
        committed_rect(&arena, left).2,
        120.0,
        "the switch's own reflow lands on the frame it happened",
    );

    // The write that forced the solve still landed: `c` reflowed up over the
    // hidden `b`. Without this the test could pass on a commit that ignored
    // the solve altogether.
    let c = node_named(&arena, "c");
    assert_eq!(
        committed_rect(&arena, c).1,
        60.0,
        "the same tick's Visible flip still reflowed the column",
    );

    // And it never moves backward: the track continues from where the frame
    // published it rather than rewinding to an early sample.
    let mut previous = first;
    for _ in 0..12 {
        live.tick(0.1, &mut arena);
        let x = right_x(&arena);
        assert!(
            x >= previous,
            "the travelling node never moves backward: {x} follows {previous}",
        );
        previous = x;
    }
    assert_eq!(previous, to, "the transition arrives at its destination");
}

/// A declared loop drives its channel with no signal and no switch behind it,
/// and keeps doing it (story #772) — the ambient class, which is the one class
/// nothing else in the vocabulary can express.
///
/// Asserted on the committed rect rather than on the arena's staged intent: a
/// sample that never reached the rect leaves the painter drawing the node
/// upright, however live the scheduler track looks.
#[test]
fn a_declared_loop_drives_its_channel_and_repeats() {
    use dashscene_core::{Easing, LoopTrack, Prop, TransitionSpec};

    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        // Two spinners on one document, offset half a cycle apart — the
        // skeleton-loader shape, and what says the phase offset survives the
        // whole path rather than being ignored.
        for (name, phase) in [("early", 0.0), ("late", 0.25)] {
            let n = txn.add_node(None, Some(name));
            txn.set_prop(n, Prop::Width(40.0));
            txn.set_prop(n, Prop::Height(40.0));
            txn.add_loop_track(LoopTrack {
                node: n,
                channel: Channel::Rotation,
                // A span of 8 over a half-second linear cycle, stepped
                // below by an eighth of that. Every number here is a
                // negative power of two, so the elapsed accumulation is
                // exact and the samples can be asserted by equality —
                // 0.1 is not representable, and stepping by it would drift
                // the wrap off the frame it lands on.
                from: 0.0,
                to: 8.0,
                spec: TransitionSpec::Tween {
                    duration: 0.5,
                    easing: Easing::Linear,
                },
                phase_offset: phase,
            });
        }
        txn.commit();
    }

    let count = Rc::new(Cell::new(0));
    let mut live = dashlang::attach_live(&mut arena, CountingSolver::boxed(count.clone()));
    assert_eq!(count.get(), 1, "attach solves once");

    let angle = |arena: &Arena, i: usize| arena.committed().rects()[i].rotation;

    // One full cycle and a little past it. The second track runs half a
    // cycle ahead of the first, so it wraps four frames earlier — which is
    // the whole point of the offset.
    let expected = [(1.0, 5.0), (2.0, 6.0), (3.0, 7.0), (4.0, 0.0), (5.0, 1.0)];
    let mut previous_generation = live.generation();
    for (frame, (early, late)) in expected.into_iter().enumerate() {
        live.tick(0.0625, &mut arena);
        assert_eq!(angle(&arena, 0), early, "frame {frame}: the early spinner");
        assert_eq!(angle(&arena, 1), late, "frame {frame}: the late spinner");

        // A loop never settles, so no frame takes the idle early return and
        // the generation moves every time. That is the cost recorded in the
        // ruling: a document carrying one loop draws continuously.
        assert!(
            live.generation() > previous_generation,
            "frame {frame}: a live loop commits every frame",
        );
        previous_generation = live.generation();
    }

    // And it never solves. A loop is held to paint channels precisely so a
    // track that never settles cannot put the solver in the frame loop.
    assert_eq!(
        count.get(),
        1,
        "a loop animates paint only, so no frame of it re-solves",
    );
}

/// The builder path drives a declared loop too (story #772).
///
/// `attach_live` and `build_live` are two separate ways into a `LiveScene`,
/// and wiring a channel into one of them only is a mistake this crate has
/// already made — the loaded path worked and the DSL panicked. Mutation
/// testing found this one: removing the `attach_loops` call from the builder
/// path left every other test in the crate green.
///
/// The loop is staged on the arena before the scene is built, because the
/// builder has no loop vocabulary of its own — `build_live` appends its nodes
/// to whatever the arena already holds, so the two coexist.
#[test]
fn the_builder_path_drives_a_loop_staged_on_its_arena() {
    use dashscene_core::{Easing, LoopTrack, Prop, TransitionSpec};

    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        let spinner = txn.add_node(None, Some("spinner"));
        txn.set_prop(spinner, Prop::Width(40.0));
        txn.set_prop(spinner, Prop::Height(40.0));
        txn.add_loop_track(LoopTrack {
            node: spinner,
            channel: Channel::Rotation,
            from: 0.0,
            to: 8.0,
            spec: TransitionSpec::Tween {
                duration: 0.5,
                easing: Easing::Linear,
            },
            phase_offset: 0.0,
        });
        txn.commit();
    }

    // An ordinary builder scene beside it, so the two paths are exercised at
    // once rather than the loop being the only thing present.
    let mut scene = Scene::new();
    let width = scene.signal(10.0);
    scene.roots([node("panel")
        .size(100.0, 20.0)
        .children([node("bar").size(10.0, 12.0).bind(Channel::Width, width)])]);
    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let angle = |arena: &Arena| arena.committed().rects()[0].rotation;
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 1.0, "the builder path started the loop");
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 2.0);

    // And the builder's own binding still works beside it.
    live.set(width, 42.0);
    live.tick(0.0625, &mut arena);
    assert_eq!(angle(&arena), 3.0, "the loop kept running");
    let bar = arena.committed().rects().len() - 1;
    assert_eq!(
        arena.committed().rects()[bar].w,
        42.0,
        "the builder's own binding still drives its node",
    );
}

// ---------------------------------------------------------------------
// Story #838 — the reactive layer's caches are keyed on the committed
// table, and since that story the committed table is the shown root's
// subtree rather than the whole document.
//
// All three use `attach_live` on an arena that already names its shown
// root, because that is the order the hosts run in: `load_document_mapped`
// stages every root's nodes and bindings, `Document::load` names the root,
// and `attach_live` builds its caches from what that commit left. Building
// the caches first and naming the root afterwards hides every one of these.
// ---------------------------------------------------------------------

/// Two roots, each a fixed passthrough box with a width-bound child, with
/// `shown` named before the scene is attached. Returns the live scene and the
/// two bars, first root's then second's.
fn two_bound_roots(
    arena: &mut Arena,
    shown: dashscene_core::ShownRoot,
) -> (dashlang::LiveScene, NodeId, NodeId) {
    use dashscene_core::{Channel as CoreChannel, Prop, ScalarTransform};

    let (first_bar, second_bar) = {
        let mut txn = arena.open();
        let width = txn.declare_signal(Some("bar/width"), 10.0);
        // **The two subtrees are deliberately different shapes**, so the bound
        // node sits at a different rect index under each root: row 2 under the
        // first, row 1 under the second. Two roots of the same shape put it at
        // the same index in both, and a cache left over from one then lands on
        // the right row of the other by coincidence — which is a fixture that
        // cannot tell a rebuilt index from a stale one.
        let mut root_with_bar = |name: &str, fillers: usize| {
            let root = txn.add_node(None, Some(name));
            txn.set_prop(root, Prop::Mode(LayoutMode::None));
            txn.set_prop(root, Prop::Width(200.0));
            txn.set_prop(root, Prop::Height(20.0));
            for _ in 0..fillers {
                let filler = txn.add_node(Some(root), None);
                txn.set_prop(filler, Prop::Mode(LayoutMode::None));
                txn.set_prop(filler, Prop::Width(4.0));
                txn.set_prop(filler, Prop::Height(4.0));
            }
            let bar = txn.add_node(Some(root), None);
            txn.set_prop(bar, Prop::Mode(LayoutMode::None));
            txn.set_prop(bar, Prop::Width(10.0));
            txn.set_prop(bar, Prop::Height(12.0));
            txn.bind(bar, CoreChannel::Width, width, ScalarTransform::Identity);
            bar
        };
        let first = root_with_bar("first", 1);
        let second = root_with_bar("second", 0);
        // Named in the same transaction that adds the roots, which is what a
        // loader does and what makes the ordinal judgeable only at the commit.
        txn.show_root(Some(shown));
        txn.commit();
        (first, second)
    };

    let live = dashlang::attach_live(arena, Box::new(TaffySolver::new()));
    (live, first_bar, second_bar)
}

/// **A bound node under an unshown root must not panic.**
///
/// `attach_live` binds every node the document declares, and `cached_index` is
/// built from the committed table — which since story #838 holds the shown
/// root's subtree alone. A contained rect write classifies as
/// `WriteClass::Patch`, and indexing `cached_index[&node]` for a node with no
/// row panics inside the frame loop. That is every multi-artboard Figma file
/// with a variable bound on an artboard the host is not showing.
///
/// The write is still staged — intent is intent, and it takes effect if that
/// root is ever shown. What is skipped is the patch, because a patch is an
/// overlay on a solved rect and this node has none.
#[test]
fn a_bound_node_under_an_unshown_root_is_staged_and_not_patched() {
    let mut arena = Arena::new();
    let (mut live, first_bar, second_bar) =
        two_bound_roots(&mut arena, dashscene_core::ShownRoot::FIRST);
    assert_eq!(
        arena.committed().rect_index_of(second_bar),
        None,
        "the fixture's whole point: the second root's bar has no row to patch"
    );

    let width = live.signal_named("bar/width").expect("declared above");
    live.set(width, 44.0);
    // Without the guard this panics on `cached_index[&second_bar]`.
    live.tick(0.016, &mut arena);

    let scene = arena.committed();
    assert_eq!(
        scene.rects().len(),
        3,
        "the shown root, its filler and its bar"
    );
    let row = scene
        .rect_index_of(first_bar)
        .expect("the shown bar has a row");
    assert_eq!(
        scene.rects()[row as usize].w,
        44.0,
        "the shown root's own bound child took the write"
    );
}

/// **A change of shown root rebuilds the patch cache.**
///
/// `cached_index` maps NodeId to a row of `cached_solve`, and both are built
/// from the committed table. A renumbering makes every row name a different
/// node, and no layout intent changed, so nothing else reports it —
/// `CommittedScene::renumbered` is what does, and this is the consumer it
/// exists for. Without reading it the cache keeps the old root's mapping and
/// every later contained write patches the wrong rect, silently in a release
/// build.
#[test]
fn a_change_of_shown_root_rebuilds_the_patch_cache() {
    let mut arena = Arena::new();
    let (mut live, _, second_bar) = two_bound_roots(&mut arena, dashscene_core::ShownRoot::FIRST);
    let width = live.signal_named("bar/width").expect("declared above");

    live.set(width, 30.0);
    live.tick(0.016, &mut arena);

    // Show the other root, and let the tick commit it — a bare `commit()`
    // could not, because the newly shown subtree has no previous rect for a
    // commit to carry forward and no solver runs to produce one.
    // A scope rather than `drop`: `Txn` implements no `Drop`, which is the
    // whole reason a staged shown root survives to the next tick.
    {
        let mut txn = arena.open();
        txn.show_root(Some(dashscene_core::ShownRoot::nth(1)));
    }
    live.tick(0.016, &mut arena);
    assert!(arena.committed().renumbered(), "that tick renumbered");

    // The tick that needs the rebuilt cache: a contained write on a node whose
    // row exists only in the table the renumbering produced.
    live.set(width, 77.0);
    live.tick(0.016, &mut arena);

    let scene = arena.committed();
    assert_eq!(
        scene.rects().len(),
        2,
        "the second root and its bar, a different shape"
    );
    let row = scene
        .rect_index_of(second_bar)
        .expect("the newly shown bar has a row");
    assert_eq!(
        scene.rects()[row as usize].w,
        77.0,
        "the newly shown root's bound child took the write, which needs the cache rebuilt \
         against its own rows"
    );
}

/// **A shown root staged between ticks is committed, and through the real
/// solver.**
///
/// `Txn` has no `Drop` that reverts, so `show_root` leaves the arena changed
/// and uncommitted the way a staged `set_variant` used to (issue #617). A
/// change of shown root moves no signal and starts no track, so `tick`'s idle
/// early return would swallow it and the host would keep painting the artboard
/// it was already showing.
///
/// It must also take the reflow arm: the cached arms replay `cached_solve`,
/// which holds the rects of the root that *was* shown, so the newly shown
/// subtree would reach `commit_with` with no rect for any of its nodes — which
/// it refuses by name.
#[test]
fn a_shown_root_staged_between_ticks_is_not_swallowed_by_the_idle_return() {
    let mut arena = Arena::new();
    let (mut live, _, second_bar) = two_bound_roots(&mut arena, dashscene_core::ShownRoot::FIRST);

    // Settle: nothing is dirty and no track is live, so the next tick would
    // take the idle return if the staged root did not stop it.
    let settled = live.tick(0.016, &mut arena);
    assert_eq!(
        live.tick(0.016, &mut arena),
        settled,
        "a genuinely idle tick holds the generation, which is what makes the assertion below \
         about the shown root rather than about ordinary churn"
    );

    // A scope rather than `drop`: `Txn` implements no `Drop`, which is the
    // whole reason a staged shown root survives to the next tick.
    {
        let mut txn = arena.open();
        txn.show_root(Some(dashscene_core::ShownRoot::nth(1)));
    }

    let after = live.tick(0.016, &mut arena);
    assert_ne!(after, settled, "the staged shown root committed");
    let scene = arena.committed();
    assert_eq!(
        scene.shown_root(),
        Some(dashscene_core::ShownRoot::nth(1)),
        "and the commit is the one that carried it"
    );
    assert!(
        scene.rect_index_of(second_bar).is_some(),
        "the newly shown subtree was solved rather than replayed from the old root's cache"
    );
}

// ---------------------------------------------------------------------
// Issue #621 — a tick that does not solve must still publish the text.
//
// `LiveScene::tick` commits through `CachedSolver`, the retained rect
// replay, whenever no binding forced a re-solve. That solver used to
// implement `solve` alone and take the trait's defaults for `atlases` and
// `stage_text`, which return an empty atlas set and no runs — and
// `Txn::commit_with` rebuilds the glyph-run table from whatever the solver
// stages, carrying nothing forward. So every glyph run disappeared on such
// a commit and came back on the next frame that solved.
//
// The two halves are asserted separately because they fail separately: a
// solver can publish the atlas set and stage no runs, or the reverse.
// Every assertion below reads `arena.committed()`, never the arena's
// staged intent — the painter reads the committed table, and a run that
// never reached it is a run nothing draws.
// ---------------------------------------------------------------------

/// A stager that publishes one atlas and stages one run against the first
/// root, on every commit it is asked to serve.
///
/// Deliberately not a wrapper around `TaffySolver`: this test is about
/// whether `CachedSolver` *asks* the scene's solver at all, so the stub
/// answers unconditionally and any empty result is `CachedSolver`'s doing
/// rather than a font, a cascade or a measure path failing to produce one.
/// `CountingSolver` above cannot serve here — it implements `solve` alone,
/// so it carries the very defaults this is testing for.
struct TextStubSolver {
    atlases: std::sync::Arc<Vec<dashscene_core::Atlas>>,
    /// How many times the real solve ran. A paint-only tick must not increment
    /// it — which is what says the commit went through `CachedSolver` and not
    /// through `FlipOverlay`, whose forward of these two methods predates this
    /// change and would make both tests pass with `CachedSolver`'s deleted.
    solves: Rc<Cell<u32>>,
}

impl TextStubSolver {
    fn boxed(solves: Rc<Cell<u32>>) -> Box<dyn LayoutSolver> {
        use dashscene_core::{Atlas, ImageAsset, ImageFormat};
        Box::new(TextStubSolver {
            solves,
            atlases: std::sync::Arc::new(vec![
                Atlas::new(
                    ImageAsset {
                        format: ImageFormat::Png,
                        bytes: vec![0],
                    },
                    1,
                    1,
                    16,
                    2.0,
                    vec![],
                )
                .expect("16 texels per em is a valid atlas scale"),
            ]),
        })
    }
}

impl LayoutSolver for TextStubSolver {
    /// Every node at a fixed box. `commit_with` requires every node to have
    /// a rect from this call or from the previous commit, and reporting all
    /// of them keeps the stub out of the incremental question entirely.
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        self.solves.set(self.solves.get() + 1);
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = arena.roots().to_vec();
        while let Some(id) = stack.pop() {
            let layout = arena.layout(id);
            out.push((
                id,
                SolvedRect {
                    x: layout.x,
                    y: layout.y,
                    w: layout.width,
                    h: layout.height,
                },
            ));
            stack.extend(arena.children(id).iter().copied());
        }
        out
    }

    fn atlases(&mut self) -> std::sync::Arc<Vec<dashscene_core::Atlas>> {
        std::sync::Arc::clone(&self.atlases)
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        _geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<dashscene_core::StagedRun> {
        use dashscene_core::{AtlasIndex, Color, GlyphQuad, GlyphRange, GlyphRun, StagedRun};
        vec![StagedRun {
            node: arena.roots()[0],
            run: GlyphRun {
                rect: u32::MAX,
                atlas: AtlasIndex(0),
                size: 12.0,
                color: Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                glyphs: GlyphRange::UNASSIGNED,
                opacity: 1.0,
            },
            quads: vec![GlyphQuad {
                glyph_id: 7,
                x: 0.0,
                y: 0.0,
            }],
        }]
    }
}

/// A scene whose only binding is a fill alpha, which is paint-only: `tick`
/// does not set `layout_dirty` for it, so the commit goes through
/// `CachedSolver`.
fn paint_only_text_scene() -> (
    Arena,
    dashlang::LiveScene,
    dashlang::Signal<f32>,
    Rc<Cell<u32>>,
) {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let alpha = scene.signal(1.0f32);
    scene.roots([node("label")
        .mode(LayoutMode::None)
        .size(100.0, 20.0)
        .bind(Channel::FillA, alpha)]);
    let solves = Rc::new(Cell::new(0));
    let live = scene.build_live(&mut arena, TextStubSolver::boxed(Rc::clone(&solves)));
    (arena, live, alpha, solves)
}

/// Runs the paint-only tick, having established that it actually commits and
/// actually takes the replay path.
///
/// Both are load-bearing. `tick` has an idle early return, so a test that only
/// compared two equal numbers would pass if no commit happened at all. And
/// `FlipOverlay` already forwards these two methods, so a test that did not pin
/// "no solve" would pass with `CachedSolver`'s forward deleted — the regression
/// would go unnoticed.
fn drive_paint_only_tick(
    arena: &mut Arena,
    live: &mut dashlang::LiveScene,
    alpha: dashlang::Signal<f32>,
    solves: &Rc<Cell<u32>>,
) {
    let solves_before = solves.get();
    let generation_before = live.generation();

    live.set(alpha, 0.25);
    live.tick(0.016, arena);

    assert!(
        live.generation() > generation_before,
        "the tick must commit, or the assertions compare the build commit with itself",
    );
    assert_eq!(
        solves.get(),
        solves_before,
        "a fill-alpha write must not solve — this is the tick that goes through CachedSolver",
    );
}

/// The first half: a paint-only tick must still stage the scene's glyph
/// runs. Without the forward this reads 1 before the tick and 0 after —
/// the scene blanks its own text on the frame the alpha changes.
#[test]
fn a_paint_only_tick_still_publishes_glyph_runs() {
    let (mut arena, mut live, alpha, solves) = paint_only_text_scene();

    let before = arena.committed().glyphs().runs().len();
    assert_eq!(before, 1, "the build commit stages the stub's one run");

    drive_paint_only_tick(&mut arena, &mut live, alpha, &solves);

    let after = arena.committed().glyphs().runs().len();
    assert_eq!(
        after, 1,
        "a tick that changes only the fill alpha must not drop the scene's glyph runs \
         ({before} before, {after} after)"
    );
}

/// The second half: the same tick must still publish the atlas set. A run
/// whose atlas index resolves against an empty table is a run no painter
/// can draw, so this fails independently of the run count above.
#[test]
fn a_paint_only_tick_still_publishes_the_atlas_set() {
    let (mut arena, mut live, alpha, solves) = paint_only_text_scene();

    let before = arena.committed().glyphs().atlases().len();
    assert_eq!(before, 1, "the build commit publishes the stub's one atlas");

    drive_paint_only_tick(&mut arena, &mut live, alpha, &solves);

    let after = arena.committed().glyphs().atlases().len();
    assert_eq!(
        after, 1,
        "a tick that changes only the fill alpha must not drop the atlas set \
         ({before} before, {after} after)"
    );
}
