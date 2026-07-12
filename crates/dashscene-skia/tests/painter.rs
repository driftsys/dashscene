//! Story #4 acceptance path: a scene committed by dashscene-core paints
//! through the Skia CPU raster painter with exact, deterministic pixels
//! (issue #4; DESIGN_1.md §8.1) — the first end-to-end crossing of
//! boundary B.

use dashpaint::{
    Color, Gradient, GradientKind, ImageTable, PaintEntry, PaintKind, Painter, RectEntry, Vec2,
};
use dashscene_core::{Arena, Prop};
use dashscene_skia::SkiaPainter;

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

const RED_RGBA: [u8; 4] = [255, 0, 0, 255];
const BLUE_RGBA: [u8; 4] = [0, 0, 255, 255];
const TRANSPARENT_RGBA: [u8; 4] = [0, 0, 0, 0];

fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

#[test]
fn paints_a_core_committed_scene_with_exact_pixels() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("bg"));
    txn.set_prop(root, Prop::Width(4.0));
    txn.set_prop(root, Prop::Height(4.0));
    txn.set_prop(root, Prop::Fill(RED));
    let child = txn.add_node(Some(root), Some("badge"));
    txn.set_prop(child, Prop::X(1.0));
    txn.set_prop(child, Prop::Y(1.0));
    txn.set_prop(child, Prop::Width(2.0));
    txn.set_prop(child, Prop::Height(2.0));
    txn.set_prop(child, Prop::Fill(BLUE));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

    let rgba = painter.rgba_bytes();
    assert_eq!(rgba.len(), 4 * 4 * 4);
    for y in 0..4 {
        for x in 0..4 {
            let expected = if (1..3).contains(&x) && (1..3).contains(&y) {
                BLUE_RGBA
            } else {
                RED_RGBA
            };
            assert_eq!(pixel(&rgba, 4, x, y), expected, "pixel ({x}, {y})");
        }
    }
}

#[test]
fn an_unfilled_node_draws_nothing() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    // A layout-only container: occupies the full surface, draws nothing.
    let container = txn.add_node(None, Some("container"));
    txn.set_prop(container, Prop::Width(4.0));
    txn.set_prop(container, Prop::Height(4.0));
    let child = txn.add_node(Some(container), Some("dot"));
    txn.set_prop(child, Prop::Width(1.0));
    txn.set_prop(child, Prop::Height(1.0));
    txn.set_prop(child, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(4, 4);
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

    let rgba = painter.rgba_bytes();
    assert_eq!(pixel(&rgba, 4, 0, 0), RED_RGBA, "the filled child paints");
    assert_eq!(
        pixel(&rgba, 4, 3, 3),
        TRANSPARENT_RGBA,
        "the unfilled container leaves the surface untouched"
    );
}

#[test]
fn encodes_png() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, None);
    txn.set_prop(root, Prop::Width(2.0));
    txn.set_prop(root, Prop::Height(2.0));
    txn.set_prop(root, Prop::Fill(RED));
    txn.commit();

    let scene = arena.committed();
    let mut painter = SkiaPainter::new(2, 2);
    painter.paint(scene.rects(), scene.paints(), &ImageTable::new());

    let png = painter.png_bytes();
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        "PNG signature"
    );
    assert!(png.len() > 8);
}

#[test]
#[should_panic(expected = "story #14")]
fn unimplemented_vocabulary_panics_by_name() {
    // Hand-built boundary-B input: v0.1 producers cannot emit a
    // gradient, so this pins the honest-failure contract until #14.
    let mut paints = dashpaint::PaintTable::new();
    let gradient = paints.push(PaintEntry {
        fill: Some(PaintKind::Gradient(Gradient {
            kind: GradientKind::Linear,
            handle_origin: Vec2 { x: 0.0, y: 0.0 },
            handle_primary: Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: Vec2 { x: 0.0, y: 1.0 },
            stops: vec![],
        })),
        ..PaintEntry::default()
    });
    let rects = [RectEntry {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
        paint: gradient,
    }];

    let mut painter = SkiaPainter::new(1, 1);
    painter.paint(&rects, &paints, &ImageTable::new());
}
