//! The v0.8 masks + group-opacity goldens (story #44). Like the clip
//! golden, every scene is authored through `dashscene-core`'s producer
//! API — a mask and a group opacity are constructs a painter cannot be
//! handed directly (the sibling and ancestor relations only exist on the
//! producer side, P2). Each scene exercises the whole path: `Prop::Mask` /
//! `Prop::Opacity` intent → commit-time resolution (a mask into clip
//! regions, an opacity into per-rect free alpha or a render-target group)
//! → the reference painter.
//!
//! `docs/decisions/masks-and-group-opacity.md`. Rounded masks and blended
//! interiors compare with the same 2% differing-pixel tolerance as the
//! other 64×64 goldens (`docs/decisions/golden-comparison-space.md`); the
//! probes sit in flat interiors and assert relationships that stay exact.

use dashpaint::{Color, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, NodeId, Prop, Txn};
use dashscene_skia::SkiaPainter;

const SIZE: usize = 64;
const TOLERANCE: f64 = 0.02;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

const NAVY: Color = Color {
    r: 0.06,
    g: 0.08,
    b: 0.16,
    a: 1.0,
};
const AMBER: Color = Color {
    r: 0.98,
    g: 0.78,
    b: 0.20,
    a: 1.0,
};
const TEAL: Color = Color {
    r: 0.20,
    g: 0.60,
    b: 0.70,
    a: 1.0,
};

fn boxed(txn: &mut Txn<'_>, parent: Option<NodeId>, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    let node = txn.add_node(parent, None);
    txn.set_prop(node, Prop::X(x));
    txn.set_prop(node, Prop::Y(y));
    txn.set_prop(node, Prop::Width(w));
    txn.set_prop(node, Prop::Height(h));
    node
}

fn render(arena: &Arena, painter: &mut SkiaPainter) -> Vec<u8> {
    let scene = arena.committed();
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        Some(scene.dirty()),
    );
    painter.rgba_bytes()
}

/// A rounded-rectangle mask over an oversized amber fill: the fill shows
/// only inside the mask's shape, the navy background everywhere else.
///
///   bg (navy 64×64)
///     └── mask (16,16) 32×32 rounded r=8, isMask — draws nothing
///     └── content amber (8,8) 48×48 — overflows the mask on every side
fn mask_scene(arena: &mut Arena) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let mask = boxed(&mut txn, Some(bg), 16.0, 16.0, 32.0, 32.0);
    txn.set_prop(
        mask,
        Prop::Corners {
            top_left: 8.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 8.0,
        },
    );
    // A red fill that must NOT show: a mask is a stencil, not paint.
    txn.set_prop(mask, Prop::Fill(rgba(0.85, 0.2, 0.2)));
    txn.set_prop(mask, Prop::Mask(true));

    let content = boxed(&mut txn, Some(bg), 8.0, 8.0, 48.0, 48.0);
    txn.set_prop(content, Prop::Fill(AMBER));

    txn.commit();
}

#[test]
fn the_mask_scene_matches_its_golden() {
    let mut arena = Arena::new();
    mask_scene(&mut arena);
    let scene = arena.committed();

    // The mask contributes one clip region (unclipped + the mask's box).
    assert_eq!(scene.clips().len(), 2);
    assert!(scene.groups().is_empty(), "a mask needs no render target");

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    let quantized = |c: Color| {
        let q = |v: f32| (v * 255.0).round() as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    };
    // Inside the mask shape: the amber content shows.
    assert_eq!(probe(32, 32), quantized(AMBER), "content inside the mask");
    // Left of the mask (x < 16): the content is clipped away, navy shows —
    // and the mask's own red fill does not paint there either.
    assert_eq!(
        probe(10, 32),
        quantized(NAVY),
        "content outside the mask is stenciled away"
    );
    // Above the mask (y < 16): likewise clipped to navy.
    assert_eq!(probe(32, 10), quantized(NAVY), "content above the mask");

    goldens::assert_matches_golden_within("v08-mask", &painter.png_bytes(), TOLERANCE);
}

/// A group at opacity 0.5 over two non-overlapping children: the free
/// path. Each child renders at half alpha; no render-target group.
///
///   bg (navy 64×64)
///     └── group (opacity 0.5, passthrough)
///           ├── amber (6,6) 20×52   — left column
///           └── amber (38,6) 20×52  — right column, disjoint
fn free_scene(arena: &mut Arena) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let group = boxed(&mut txn, Some(bg), 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(group, Prop::Opacity(0.5));

    let left = boxed(&mut txn, Some(group), 6.0, 6.0, 20.0, 52.0);
    txn.set_prop(left, Prop::Fill(AMBER));
    let right = boxed(&mut txn, Some(group), 38.0, 6.0, 20.0, 52.0);
    txn.set_prop(right, Prop::Fill(AMBER));

    txn.commit();
}

#[test]
fn the_free_group_opacity_scene_matches_its_golden() {
    let mut arena = Arena::new();
    free_scene(&mut arena);
    let scene = arena.committed();

    assert!(
        scene.groups().is_empty(),
        "non-overlapping children take the free path — no render target"
    );

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    let left = probe(14, 32);
    let right = probe(46, 32);
    let gap = probe(32, 32);
    let quantized = |c: Color| {
        let q = |v: f32| (v * 255.0).round() as u8;
        [q(c.r), q(c.g), q(c.b), q(c.a)]
    };
    // The two children render identically (same fill, same free alpha),
    // are blended (not full amber), and the gap between them shows the
    // untouched navy background (the group has no fill of its own).
    assert_eq!(left, right, "both children carry the same free alpha");
    assert_ne!(left, quantized(AMBER), "the child is dimmed by the group");
    assert_eq!(gap, quantized(NAVY), "the gap shows the background");

    goldens::assert_matches_golden_within(
        "v08-group-opacity-free",
        &painter.png_bytes(),
        TOLERANCE,
    );
}

/// A group at opacity 0.5 over two overlapping children: the render-target
/// path. The subtree flattens before the alpha applies, so the overlap is
/// no darker than a single child.
///
///   bg (navy 64×64)
///     └── group (opacity 0.5, passthrough)
///           ├── amber (12,12) 28×40
///           └── teal (24,12) 28×40   — overlaps the amber in x = [24, 40)
fn render_target_scene(arena: &mut Arena) {
    let mut txn = arena.open();
    let bg = boxed(&mut txn, None, 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(bg, Prop::Fill(NAVY));

    let group = boxed(&mut txn, Some(bg), 0.0, 0.0, 64.0, 64.0);
    txn.set_prop(group, Prop::Opacity(0.5));

    let amber = boxed(&mut txn, Some(group), 12.0, 12.0, 28.0, 40.0);
    txn.set_prop(amber, Prop::Fill(AMBER));
    let teal = boxed(&mut txn, Some(group), 24.0, 12.0, 28.0, 40.0);
    txn.set_prop(teal, Prop::Fill(TEAL));

    txn.commit();
}

#[test]
fn the_render_target_group_opacity_scene_matches_its_golden() {
    let mut arena = Arena::new();
    render_target_scene(&mut arena);
    let scene = arena.committed();

    // One render-target group over the whole subtree, at the group's alpha.
    assert_eq!(scene.groups().len(), 1, "overlapping children need a layer");
    assert_eq!(scene.groups()[0].alpha, 0.5);

    let mut painter = SkiaPainter::new(SIZE as i32, SIZE as i32);
    let bytes = render(&arena, &mut painter);
    let probe = |x: usize, y: usize| goldens::pixel(&bytes, SIZE, x, y);

    let amber_only = probe(16, 32); // x in [12, 24): amber alone
    let overlap = probe(32, 32); // x in [24, 40): teal over amber
    let teal_only = probe(46, 32); // x in [40, 52): teal alone

    // The group flattened before its alpha applied: teal covers amber in
    // the layer, so the overlap equals the teal-only region — not the
    // darker double-blend the free path would produce.
    assert_eq!(
        overlap, teal_only,
        "the overlap is teal at the group alpha, not teal-over-amber twice"
    );
    assert_ne!(
        overlap, amber_only,
        "and it differs from the amber-only region"
    );

    goldens::assert_matches_golden_within("v08-group-opacity-rt", &painter.png_bytes(), TOLERANCE);
}
