//! The painter badge: the value-to-text mapping, and the badge's place in
//! a scene's root list.

use dashlang::{Arena, Scene};
use dashscene_engine::TaffySolver;
use showcase::badge;

/// Builds an arena and a scene with a plain content root and the badge as
/// its second root, the setup every scene-building test below starts from.
fn arena_and_scene_with_badge() -> (Arena, Scene) {
    let arena = Arena::new();
    let mut scene = Scene::new();
    let content = dashlang::node("content")
        .mode(dashlang::LayoutMode::None)
        .size(400.0, 300.0);
    let label = badge::badge(&mut scene, 400.0, 300.0);
    scene.roots([content, label]);
    (arena, scene)
}

/// The mapping the host drives. `0.0` is the unannounced state and must
/// render nothing, which is what keeps the badge out of the still that
/// produces the repository README's picture.
#[test]
fn each_value_names_its_painter_and_zero_names_nothing() {
    assert_eq!(badge::label(0.0), "");
    assert_eq!(badge::label(badge::SKIA), "dashscene-skia");
    assert_eq!(badge::label(badge::GPU), "dashscene-gpu");
}

/// The two announced values must differ, or the host cannot distinguish
/// the painters through the one signal it writes.
#[test]
fn the_two_painters_have_distinct_values() {
    assert_ne!(badge::SKIA, badge::GPU);
    assert_ne!(badge::label(badge::SKIA), badge::label(badge::GPU));
}

/// Built into a scene, the badge is committed empty and transparent, and
/// it is the last root — which is what paints it above the content.
#[test]
fn the_badge_builds_invisible_and_last() {
    let (mut arena, scene) = arena_and_scene_with_badge();
    let live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));
    drop(live);

    let roots = arena.roots();
    assert_eq!(roots.len(), 2, "the badge is a second root");
    let badge_id = roots[1];
    assert_eq!(arena.name(badge_id), Some("backend-badge"));
    assert_eq!(arena.opacity(badge_id), 0.0, "invisible until announced");

    // The label lives on the badge's one child, not on the root itself
    // — the root's own width is bound instead, so the tick re-solves.
    let label_id = arena.children(badge_id)[0];
    assert_eq!(arena.name(label_id), Some("backend-badge-label"));
    assert_eq!(arena.text(label_id), Some(""), "no painter announced yet");
}

/// Writing the signal changes the text and raises the badge, with no
/// rebuild — this is the path the `P` swap key takes.
#[test]
fn announcing_a_painter_shows_it_without_a_rebuild() {
    let (mut arena, scene) = arena_and_scene_with_badge();
    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let signal = live
        .signal_named(badge::BACKEND)
        .expect("the badge declares its signal under this name");
    live.set(signal, badge::GPU);
    live.tick(0.016, &mut arena);

    let badge_id = arena.roots()[1];
    let label_id = arena.children(badge_id)[0];
    assert_eq!(arena.text(label_id), Some("dashscene-gpu"));
    assert_eq!(arena.opacity(badge_id), 1.0);

    live.set(signal, badge::SKIA);
    live.tick(0.016, &mut arena);
    assert_eq!(arena.text(label_id), Some("dashscene-skia"));
    assert_eq!(arena.opacity(badge_id), 1.0);
}

/// The badge is placed by its own offset rather than laid out under the
/// content root, so it overlaps the scene instead of stacking below it,
/// and its rects come last in the committed table, which is what draws it
/// above the content.
///
/// This asserts on `arena.committed().rects()`, never on `arena.layout()`.
/// `Arena::layout` returns authored layout intent and not solved geometry
/// (`crates/dashscene-core/src/arena.rs`, `Arena::base_layout`'s doc
/// comment says so directly), so an assertion over it reads back exactly
/// what `badge()` passed to `.at()` and `.size()` and pins no property of
/// the solved scene at all. Measured: a badge built as the second child of
/// a `LayoutMode::Vertical` content root, which the solver genuinely
/// stacks to committed rect `(0.00, 280.00)`, satisfies every such
/// assertion unchanged.
///
/// This is also the only test in the suite that holds the badge to a
/// drawable extent — see the first assertion below for why nothing else
/// catches a badge that renders nothing.
#[test]
fn the_badge_overlaps_the_content_rather_than_stacking_below_it() {
    let (mut arena, scene) = arena_and_scene_with_badge();
    let live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));
    drop(live);

    let committed = arena.committed();
    let rects = committed.rects();
    // Found by name rather than through the root list: which root the
    // badge is has its own test above, and reading it from there would
    // assume the arrangement this test exists to measure.
    let index_named = |wanted: &str| {
        (0..rects.len() as u32)
            .find(|&i| arena.name(committed.node_of(i)) == Some(wanted))
            .unwrap_or_else(|| panic!("{wanted} must be committed"))
    };
    let badge_index = index_named("backend-badge");
    let content_index = index_named("content");
    let badge_id = committed.node_of(badge_index);
    let badge_rect = rects[badge_index as usize];
    let content_rect = rects[content_index as usize];

    // The badge draws something at all — the branch's whole deliverable,
    // and the one property nothing else in the suite holds. Every other
    // assertion here survives a zero-extent badge: the overlap test below
    // is satisfied at `w == 0`, because `content.x < badge.x + badge.w`
    // still holds. Mutation-proved by forcing `pill_width` to return
    // `0.0`, which passes all seven tests in this file without this
    // assertion. Read against the committed rect, so it also covers a
    // solver that collapsed an authored extent.
    assert!(
        badge_rect.w > 0.0 && badge_rect.h > 0.0,
        "the badge must commit a drawable extent, but solved to {}x{} — a badge with no \
         extent announces nothing on screen",
        badge_rect.w,
        badge_rect.h
    );

    // The solver placed the badge at the offset the badge authored: a
    // second root takes no part in the content's layout. This compares
    // two different tables — the solved rect against the authored intent
    // — so a badge the solver flowed fails it. The tolerance absorbs
    // solver rounding only; the stacked arrangement above misses by
    // 273 px.
    const TOLERANCE_PX: f32 = 0.001;
    let authored = arena.layout(badge_id);
    assert!(
        (badge_rect.x - authored.x).abs() < TOLERANCE_PX
            && (badge_rect.y - authored.y).abs() < TOLERANCE_PX,
        "the badge must solve to the offset it authored, but authored ({}, {}) solved to ({}, {})",
        authored.x,
        authored.y,
        badge_rect.x,
        badge_rect.y
    );

    // Overlap, on both axes: the badge is drawn over the scene, not
    // beside it and not past its bottom edge.
    assert!(
        badge_rect.x < content_rect.x + content_rect.w
            && content_rect.x < badge_rect.x + badge_rect.w
            && badge_rect.y < content_rect.y + content_rect.h
            && content_rect.y < badge_rect.y + badge_rect.h,
        "the badge's solved rect must overlap the content's on both axes: badge ({}, {}) \
         {}x{}, content ({}, {}) {}x{}",
        badge_rect.x,
        badge_rect.y,
        badge_rect.w,
        badge_rect.h,
        content_rect.x,
        content_rect.y,
        content_rect.w,
        content_rect.h
    );

    // Last in the committed table, which is what paints the badge on
    // top: a painter draws the rect table in order, so every rect from
    // the badge's own index to the end must belong to the badge's
    // subtree and nothing the content staged may follow it.
    assert!(
        badge_index > content_index,
        "the badge's rect must come after the content's ({badge_index} vs {content_index})"
    );
    let in_badge_subtree = |node| {
        let mut at = Some(node);
        while let Some(current) = at {
            if current == badge_id {
                return true;
            }
            at = arena.parent(current);
        }
        false
    };
    for index in badge_index..rects.len() as u32 {
        let node = committed.node_of(index);
        assert!(
            in_badge_subtree(node),
            "rect {index} ({:?}) is committed after the badge, so it would paint over it",
            arena.name(node)
        );
    }
}

/// Every showcase scene carries the badge, as its last root. A scene
/// added later without one is a scene whose frames cannot be attributed
/// to a painter, so this asserts over the registry rather than over a
/// list repeated here.
#[test]
fn every_showcase_scene_carries_the_badge_as_its_last_root() {
    for scene in showcase::SCENES {
        let mut arena = Arena::new();
        let live = (scene.build)(&mut arena, 960, 600);
        drop(live);

        let roots = arena.roots();
        assert_eq!(
            roots.len(),
            2,
            "scene {} must build with exactly two roots",
            scene.name
        );
        let last = *roots.last().expect("a scene has at least one root");
        assert_eq!(
            arena.name(last),
            Some("backend-badge"),
            "scene {} must carry the badge as its last root",
            scene.name
        );
        let label = arena.children(last)[0];
        assert_eq!(
            arena.text(label),
            Some(""),
            "scene {} must build with no painter announced",
            scene.name
        );
    }
}

/// Announcing a painter must add exactly one glyph run — the badge's
/// own — leaving every run the scene already staged in place.
///
/// This asserts on the committed glyph-run table rather than on the text
/// prop. The prop updates whether or not the tick actually re-solves, so
/// a version of the badge whose signal drove only paint-only channels
/// (opacity and the text itself) could pass a prop-only assertion while
/// staging no glyph runs at all — a paint-only tick commits through the
/// cached-rect replay, which staged none until issue #621 fixed it
/// (`badge.rs`'s "Why the pill's width is bound"). The plus-one is what a
/// wipe cannot fake: `layout`
/// has no text of its own, so a run count that only checked "greater
/// than zero" would not catch it losing every run it started with.
#[test]
fn announcing_a_painter_adds_exactly_one_glyph_run_to_every_scene() {
    for scene in showcase::SCENES {
        let mut arena = Arena::new();
        let mut live = (scene.build)(&mut arena, 960, 600);
        let before = arena.committed().glyphs().runs().len();

        let signal = live
            .signal_named(badge::BACKEND)
            .expect("every scene carries the badge's signal");
        live.set(signal, badge::GPU);
        live.tick(0.016, &mut arena);

        let after = arena.committed().glyphs().runs().len();
        assert_eq!(
            after,
            before + 1,
            "scene {} must gain exactly the badge's own glyph run when a painter is \
             announced, losing none of its own ({before} before, {after} after)",
            scene.name
        );
    }
}
