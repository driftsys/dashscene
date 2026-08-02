//! The paint vocabulary reaches the arena, and an unauthored node
//! stages exactly what it staged before the vocabulary existed.
//!
//! Every case asserts the DSL form and the hand-built `Txn` form commit
//! identical painter input — the claim `builder.rs` already makes for
//! the geometry and flex setters.

use dashlang::{
    Arena, Blur, BlurKind, CornerRadii, FillSpec, Shadow, ShadowKind, Stroke, StrokeAlign,
    TextAlign, TextAlignV, TextStyle, Vec2, VectorField, node, rgba, scene,
};
use dashscene_core::Prop;

/// Both arenas must have committed identical painter input.
fn assert_same_output(dsl: &Arena, hand: &Arena) {
    assert_eq!(dsl.committed().rects(), hand.committed().rects());
    assert_eq!(dsl.committed().paints(), hand.committed().paints());
    assert_eq!(dsl.committed().clips(), hand.committed().clips());
}

#[test]
fn corners_reach_the_arena() {
    let mut dsl = Arena::new();
    scene([node("card")
        .size(40.0, 20.0)
        .corners_each(8.0, 8.0, 0.0, 0.0)])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("card"));
    txn.set_prop(id, Prop::Width(40.0));
    txn.set_prop(id, Prop::Height(20.0));
    txn.set_prop(
        id,
        Prop::Corners {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        },
    );
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn a_node_with_no_paint_vocabulary_stages_what_it_always_did() {
    let mut dsl = Arena::new();
    scene([node("plain").size(40.0, 20.0)]).build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("plain"));
    txn.set_prop(id, Prop::Width(40.0));
    txn.set_prop(id, Prop::Height(20.0));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

/// A caller can hold radii in a `CornerRadii` value and spread them into
/// the setter — the shape a scene uses when several nodes share a
/// radius. Asserts the spread reaches the arena, not merely that it
/// compiles: Task 1 already covers nameability.
#[test]
fn corner_radii_can_be_spread_into_the_setter() {
    let r = CornerRadii {
        top_left: 4.0,
        top_right: 4.0,
        bottom_right: 4.0,
        bottom_left: 4.0,
    };

    let mut spread = Arena::new();
    scene([node("x").size(10.0, 10.0).corners_each(
        r.top_left,
        r.top_right,
        r.bottom_right,
        r.bottom_left,
    )])
    .build(&mut spread);

    let mut literal = Arena::new();
    scene([node("x").size(10.0, 10.0).corners_each(4.0, 4.0, 4.0, 4.0)]).build(&mut literal);

    assert_same_output(&spread, &literal);
}

#[test]
fn stroke_opacity_clip_and_mask_reach_the_arena() {
    let ink = rgba(0.1, 0.1, 0.1, 1.0);
    let stroke = Stroke {
        width: 2.0,
        align: StrokeAlign::Inside,
        color: ink,
    };

    let mut dsl = Arena::new();
    scene([node("panel")
        .size(60.0, 40.0)
        .stroke(stroke)
        .opacity(0.5)
        .clip(true)
        .child(node("child").size(10.0, 10.0).mask(true))])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let panel = txn.add_node(None, Some("panel"));
    txn.set_prop(panel, Prop::Width(60.0));
    txn.set_prop(panel, Prop::Height(40.0));
    txn.set_prop(panel, Prop::Stroke(stroke));
    txn.set_prop(panel, Prop::Opacity(0.5));
    txn.set_prop(panel, Prop::Clip(true));
    let child = txn.add_node(Some(panel), Some("child"));
    txn.set_prop(child, Prop::Width(10.0));
    txn.set_prop(child, Prop::Height(10.0));
    txn.set_prop(child, Prop::Mask(true));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn fill_with_and_extra_fills_reach_the_arena() {
    let base = FillSpec::Solid {
        color: rgba(0.2, 0.4, 0.9, 1.0),
    };
    let over = FillSpec::Solid {
        color: rgba(0.9, 0.7, 0.1, 0.5),
    };

    let mut dsl = Arena::new();
    scene([node("swatch")
        .size(30.0, 30.0)
        .fill_with(base.clone())
        .extra_fills([over.clone()])])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("swatch"));
    txn.set_prop(id, Prop::Width(30.0));
    txn.set_prop(id, Prop::Height(30.0));
    txn.set_prop(id, Prop::FillWith(base));
    txn.set_prop(id, Prop::ExtraFills(vec![over]));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

/// `clip(false)` and `mask(false)` must still stage: both props clear,
/// so `false` is a value an author can mean, not an absent one.
#[test]
fn clip_and_mask_stage_their_false_value() {
    let mut dsl = Arena::new();
    scene([node("n").size(10.0, 10.0).clip(false).mask(false)]).build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("n"));
    txn.set_prop(id, Prop::Width(10.0));
    txn.set_prop(id, Prop::Height(10.0));
    txn.set_prop(id, Prop::Clip(false));
    txn.set_prop(id, Prop::Mask(false));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn shadows_blurs_and_a_shape_field_reach_the_arena() {
    let ink = rgba(0.0, 0.0, 0.0, 0.4);
    let drop = Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 2.0 },
        blur: 6.0,
        spread: 0.0,
        color: ink,
    };
    let frost = Blur {
        kind: BlurKind::Backdrop,
        radius: 12.0,
    };
    let field = VectorField {
        image: 0,
        atlas_rect: [0, 0, 32, 32],
        plane_bounds: [0.0, 0.0, 32.0, 32.0],
        distance_range: 4.0,
    };

    let mut dsl = Arena::new();
    scene([node("card")
        .size(50.0, 50.0)
        .shadows([drop])
        .blurs([frost])
        .shape_field(field)])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("card"));
    txn.set_prop(id, Prop::Width(50.0));
    txn.set_prop(id, Prop::Height(50.0));
    txn.set_prop(id, Prop::Shadows(vec![drop]));
    txn.set_prop(id, Prop::Blurs(vec![frost]));
    txn.set_prop(id, Prop::ShapeField(field));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

#[test]
fn text_and_text_style_reach_the_arena() {
    let style = TextStyle {
        family: "Noto Sans".to_owned(),
        size: 18.0,
        weight: 400,
        color: rgba(0.1, 0.1, 0.1, 1.0),
        line_height_px: None,
        letter_spacing: 0.0,
        text_align: TextAlign::Left,
        text_align_v: TextAlignV::Top,
        ligatures_off: false,
    };

    let mut dsl = Arena::new();
    scene([node("label")
        .size(120.0, 24.0)
        .text("Hello dashscene")
        .text_style(style.clone())])
    .build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("label"));
    txn.set_prop(id, Prop::Width(120.0));
    txn.set_prop(id, Prop::Height(24.0));
    txn.set_prop(id, Prop::Text("Hello dashscene".to_owned()));
    txn.set_prop(id, Prop::TextStyle(style));
    txn.commit();

    assert_same_output(&dsl, &hand);
    assert_eq!(dsl.text(dsl.roots()[0]), Some("Hello dashscene"));
}

#[test]
fn visible_reaches_the_arena() {
    let mut dsl = Arena::new();
    scene([node("hidden").size(10.0, 10.0).visible(false)]).build(&mut dsl);

    let mut hand = Arena::new();
    let mut txn = hand.open();
    let id = txn.add_node(None, Some("hidden"));
    txn.set_prop(id, Prop::Width(10.0));
    txn.set_prop(id, Prop::Height(10.0));
    txn.set_prop(id, Prop::Visible(false));
    txn.commit();

    assert_same_output(&dsl, &hand);
}

/// Each sugar method must be exactly its mirror. Comparing the two DSL
/// forms is the whole assertion: the mirror is already proven against a
/// hand-built `Txn` above.
#[test]
fn corners_is_corners_each_four_times() {
    let mut sugar = Arena::new();
    scene([node("a").size(10.0, 10.0).corners(6.0)]).build(&mut sugar);

    let mut mirror = Arena::new();
    scene([node("a").size(10.0, 10.0).corners_each(6.0, 6.0, 6.0, 6.0)]).build(&mut mirror);

    assert_same_output(&sugar, &mirror);
}

#[test]
fn the_shadow_sugar_is_the_shadows_mirror() {
    let ink = rgba(0.0, 0.0, 0.0, 0.4);

    let mut sugar = Arena::new();
    scene([node("a")
        .size(10.0, 10.0)
        .drop_shadow(0.0, 2.0, 6.0, 1.0, ink)])
    .build(&mut sugar);

    let mut mirror = Arena::new();
    scene([node("a").size(10.0, 10.0).shadows([Shadow {
        kind: ShadowKind::Drop,
        offset: Vec2 { x: 0.0, y: 2.0 },
        blur: 6.0,
        spread: 1.0,
        color: ink,
    }])])
    .build(&mut mirror);

    assert_same_output(&sugar, &mirror);
}

/// `dx` and `dy` differ so an x/y transposition inside `inner_shadow`
/// fails this test. `drop_shadow`'s test does not cover that: the two
/// methods each build their own `Vec2` from two separate literals, so
/// neither carries the other's offset assembly.
#[test]
fn the_inner_shadow_sugar_is_the_shadows_mirror() {
    let ink = rgba(0.0, 0.0, 0.0, 0.4);

    let mut sugar = Arena::new();
    scene([node("a")
        .size(10.0, 10.0)
        .inner_shadow(1.0, 3.0, 4.0, 0.0, ink)])
    .build(&mut sugar);

    let mut mirror = Arena::new();
    scene([node("a").size(10.0, 10.0).shadows([Shadow {
        kind: ShadowKind::Inner,
        offset: Vec2 { x: 1.0, y: 3.0 },
        blur: 4.0,
        spread: 0.0,
        color: ink,
    }])])
    .build(&mut mirror);

    assert_same_output(&sugar, &mirror);
}

#[test]
fn the_backdrop_blur_sugar_is_the_blurs_mirror() {
    let mut sugar = Arena::new();
    scene([node("a").size(10.0, 10.0).backdrop_blur(12.0)]).build(&mut sugar);

    let mut mirror = Arena::new();
    scene([node("a").size(10.0, 10.0).blurs([Blur {
        kind: BlurKind::Backdrop,
        radius: 12.0,
    }])])
    .build(&mut mirror);

    assert_same_output(&sugar, &mirror);
}
