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
    use dashscene_core::PaintKind;

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
        Some(PaintKind::Solid { color }) => {
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
    use dashscene_core::{Channel as CoreChannel, Color, PaintKind, Prop, ScalarTransform};

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
        Some(PaintKind::Solid { color }) => {
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
