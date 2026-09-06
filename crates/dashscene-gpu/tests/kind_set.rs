//! The document's kind set selects the pipeline, and the pipeline it selects
//! really is missing the arm the set compiled out (story #1449).
//!
//! # Why this is a file of its own, and why it holds ONE renderer
//!
//! Every other layer-3 fixture builds a renderer per scene, because what it
//! asks about is one picture. The claim here is about **two paints on the same
//! painter**: that the set is a census of the tables this frame was handed
//! rather than of the document that was loaded, so a paint that interns the
//! first stroke moves the pipeline. A helper that builds a renderer per call
//! cannot ask that question at all — each paint would be a first paint.
//!
//! # The cache key is not the observable
//!
//! `Renderer::pipeline_kind_set` reports which pipeline was bound, and a test
//! that stops there measures the bookkeeping: `KindSet::of` returning a
//! constant, or a pipeline built with no constants at all, passes every
//! assertion about the key. So the middle test paints a **clipped** scene
//! through the pipeline built for a clip-free document, using
//! `Renderer::force_kind_set`, and reads the pixel the clip should have
//! rejected. That pixel is the compiled-out branch, seen from outside.

use dashpaint::kind_set::KindSet;
use dashpaint::{
    ClipBox, ClipIndex, ClipTable, Color, CornerRadii, EntryParts, GlyphRunTable, ImageTable,
    PaintEntry, PaintTable, Painter, RectEntry, Stroke, StrokeAlign, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};

mod common;
use common::{H, W, renderer, texel};

fn rect(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    paint: dashpaint::PaintIndex,
    clip: ClipIndex,
) -> RectEntry {
    RectEntry {
        x,
        y,
        w,
        h,
        paint,
        clip,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }
}

/// Packs one scene and paints it through `renderer`, which is **kept across
/// calls** — the whole point of this file.
fn paint(
    renderer: &mut Renderer,
    rects: &[RectEntry],
    paints: &PaintTable,
    clips: &ClipTable,
) -> Vec<u8> {
    let mut painter = GpuPainter::new();
    painter.paint(
        rects,
        paints,
        &ImageTable::new(),
        clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    renderer
        .render(
            painter.instances(),
            paints,
            &ImageTable::new(),
            clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum")
}

/// A green fill, and the rect that carries it: 48 units wide from x = 8, so it
/// reaches well past the clip box below.
fn green_rect(paints: &mut PaintTable, clip: ClipIndex) -> RectEntry {
    let green = paints.push_solid(Color {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    });
    rect(8.0, 12.0, 48.0, 24.0, green, clip)
}

/// A clip table holding the canvas's left half, and the index of that region.
fn left_half() -> (ClipTable, ClipIndex) {
    let mut clips = ClipTable::new();
    let region = clips.push(&[ClipBox {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 48.0,
        corners: CornerRadii::default(),
    }]);
    (clips, region)
}

/// The pixel the clip box excludes: inside the rect, outside the region.
const OUTSIDE_THE_CLIP: (u32, u32) = (48, 24);

#[test]
fn a_document_with_no_clip_boxes_selects_the_clip_free_pipeline_and_a_clipped_one_does_not() {
    let mut renderer = renderer();

    let mut unclipped_paints = PaintTable::new();
    let unclipped = green_rect(&mut unclipped_paints, ClipIndex::UNCLIPPED);
    paint(
        &mut renderer,
        &[unclipped],
        &unclipped_paints,
        &ClipTable::new(),
    );
    assert_eq!(
        renderer.pipeline_kind_set(),
        KindSet::default(),
        "a document with no clip box and no stroke reaches neither arm"
    );

    // The same painter, a second paint, a document that clips. Nothing was
    // reloaded and no renderer was rebuilt.
    let (clips, region) = left_half();
    let mut clipped_paints = PaintTable::new();
    let clipped = green_rect(&mut clipped_paints, region);
    paint(&mut renderer, &[clipped], &clipped_paints, &clips);
    assert_eq!(
        renderer.pipeline_kind_set(),
        KindSet {
            clips: true,
            strokes: false,
        },
        "a clip box in the table selects the pipeline that has the clip loop"
    );
}

/// The clip-free pipeline really has no clip loop, seen from the pixels.
///
/// The same clipped scene twice on one renderer: once through the pipeline its
/// own census selects, and once through the clip-free one, forced. The first
/// leaves [`OUTSIDE_THE_CLIP`] clear. The second inks it — because the loop
/// that would have rejected it is not in that pipeline's fragment stage.
///
/// **This is the assertion that fails when nothing is actually specialised.**
/// Delete the `if !HAS_CLIPS { return 1.0; }` from `clip_coverage`, or build
/// every pipeline with `PipelineCompilationOptions::default()`, and the forced
/// paint clips exactly like the selected one — so both reads are zero and this
/// fails, while every assertion about the cache key still passes.
#[test]
fn the_clip_free_pipeline_really_has_no_clip_loop() {
    let mut renderer = renderer();
    let (clips, region) = left_half();
    let mut paints = PaintTable::new();
    let clipped = green_rect(&mut paints, region);

    let selected = paint(&mut renderer, &[clipped], &paints, &clips);
    assert!(
        renderer.pipeline_kind_set().clips,
        "the census selects the pipeline with the clip loop"
    );
    let (x, y) = OUTSIDE_THE_CLIP;
    assert_eq!(
        texel(&selected, x, y)[3],
        0,
        "the selected pipeline applies the clip, so ({x}, {y}) is clear"
    );

    renderer.force_kind_set(Some(KindSet::default()));
    let forced = paint(&mut renderer, &[clipped], &paints, &clips);
    assert_eq!(
        renderer.pipeline_kind_set(),
        KindSet::default(),
        "the forced set is the one that was bound"
    );
    assert!(
        texel(&forced, x, y)[3] > 250,
        "the clip-free pipeline has no clip loop, so ({x}, {y}) is inked; got {:?}",
        texel(&forced, x, y)
    );

    // And the force is undone, so nothing after this test's scope inherits it.
    renderer.force_kind_set(None);
    let restored = paint(&mut renderer, &[clipped], &paints, &clips);
    assert_eq!(
        texel(&restored, x, y)[3],
        0,
        "clearing the force returns the painter to its own census"
    );
}

/// A later paint that interns a stroke re-selects the pipeline.
///
/// The property the whole cache rests on, and the one an atlas set does not
/// share: a paint table **grows** while a document runs, so a set read once at
/// load would be wrong from the first stroke onward. Selecting at load only
/// fails this.
#[test]
fn a_paint_that_interns_a_stroke_reselects_the_pipeline() {
    let mut renderer = renderer();

    let mut plain = PaintTable::new();
    let unstroked = green_rect(&mut plain, ClipIndex::UNCLIPPED);
    paint(&mut renderer, &[unstroked], &plain, &ClipTable::new());
    assert!(
        !renderer.pipeline_kind_set().strokes,
        "nothing is stroked yet"
    );

    let mut stroked_paints = PaintTable::new();
    let stroked = stroked_paints.push_with(
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: 3.0,
                align: StrokeAlign::Center,
                color: Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            }),
            ..Default::default()
        },
    );
    paint(
        &mut renderer,
        &[rect(8.0, 12.0, 48.0, 24.0, stroked, ClipIndex::UNCLIPPED)],
        &stroked_paints,
        &ClipTable::new(),
    );
    assert_eq!(
        renderer.pipeline_kind_set(),
        KindSet {
            clips: false,
            strokes: true,
        },
        "the stroke the second paint interned moves the set, on the same renderer"
    );
}

/// A stroke instance reaching a pipeline built without the stroke arm draws
/// **nothing**, rather than something wrong.
///
/// A frame cannot hold one in normal operation — the set is a census of the very
/// table the packer read — so this is a claim about what the shading does at a
/// state the census forbids, and `force_kind_set` is the only way to build it.
/// It is worth pinning because the two arms are gated separately: gating the
/// coverage chain alone would leave the colour chain reading the stroke table in
/// a pipeline compiled without it, and the picture would be a filled box in the
/// stroke's colour rather than an absence.
#[test]
fn a_stroke_under_a_stroke_free_pipeline_draws_nothing() {
    let mut renderer = renderer();
    let mut paints = PaintTable::new();
    let stroked = paints.push_with(
        PaintEntry::default(),
        EntryParts {
            stroke: Some(Stroke {
                width: 4.0,
                align: StrokeAlign::Center,
                color: Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            }),
            ..Default::default()
        },
    );
    let scene = [rect(8.0, 12.0, 48.0, 24.0, stroked, ClipIndex::UNCLIPPED)];
    // On the box's left edge, inside a centred band four units wide.
    let (x, y) = (8, 24);

    let drawn = paint(&mut renderer, &scene, &paints, &ClipTable::new());
    assert!(
        renderer.pipeline_kind_set().strokes,
        "the census selects the pipeline that has the stroke arm"
    );
    assert!(
        texel(&drawn, x, y)[3] > 250,
        "the stroke draws through its own pipeline; got {:?}",
        texel(&drawn, x, y)
    );

    renderer.force_kind_set(Some(KindSet::default()));
    let forced = paint(&mut renderer, &scene, &paints, &ClipTable::new());
    assert_eq!(
        texel(&forced, x, y)[3],
        0,
        "a stroke-free pipeline draws no stroke at all — not a filled box in the \
         stroke's colour, which is what gating only the coverage arm would give"
    );
    renderer.force_kind_set(None);
}
