//! The whole text chain, end to end, through the lean painter: the one
//! typesetter shapes Latin and Arabic, commit stages the runs, and
//! `dashscene-gpu` puts their glyphs on a device (story #582).
//!
//! # Why this exists beside `dashscene-gpu`'s own layer-3 suite
//!
//! That suite proves the painter reads the rectangle it was given, out of a
//! hand-built atlas whose texels the test wrote. It cannot prove that a **real**
//! atlas reaches it: the committed corpus fixtures are PNGs of MSDF channels
//! produced by a pinned generator, their glyph ids come from shaping, and their
//! `plane_em`/`atlas_px` come from a metrics blob. Every one of those is a join
//! this story crosses and none of them is exercised by a fixture the test
//! authored itself.
//!
//! Story #582's body asks for both scripts by name — "Both Latin and Arabic must
//! render, because the showcase requires both and the render oracle already
//! measures Arabic against Figma" — so both are here, in one scene, sampling two
//! different atlases.
//!
//! # What it does not establish
//!
//! Nothing about fidelity. It asserts that ink appears where a run was placed
//! and nowhere a run was not, which is layer 3's kind of claim. How this painter
//! compares against the Skia oracle — the `arabic` band the render oracle
//! measures at 1.405 % for Skia — is layer 4's, needs recorded hardware, and is
//! story #586's.
//!
//! It lives in `goldens/` for the reason `lean_painter_baked_assets` gives: it
//! is the only workspace member that depends on both the corpus fixtures and the
//! lean painter.

use dashpaint::{AtlasIndex, GlyphRange, GlyphRunTable, Painter};
use dashscene_core::{Arena, AxisSizing, LayoutMode, NodeId, Prop};
use dashscene_engine::TaffySolver;
use dashscene_gpu::{GpuPainter, InstanceKind, Renderer};
use dashscene_typeset::text::{Font, Typesetter};
use dashscene_typeset::text::{FontFamily, WeightedFont};

mod common;

use common::{NAVY, NEAR_WHITE, load_atlas, origin_of, rect_index_of, size_of, text_style};

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
const ATLAS_LATIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
const ATLAS_ARABIC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// "as-salaamu alaikum" — the same greeting the v0.6 Arabic golden uses, so the
/// shaping this test depends on is the shaping that golden already pins.
const ARABIC: &str = "السلام عليكم";
const LATIN: &str = "Range";

const CANVAS_W: f32 = 320.0;
const CANVAS_H: f32 = 140.0;
const SIZE: f32 = 28.0;

/// The two families the scene names, in the atlas order [`atlases`] declares.
///
/// Two faces and two atlases, which is the point: a painter that took one run's
/// atlas for both would draw one script from the other's texels, and the glyph
/// ids of one are meaningless in the other.
fn typesetter() -> Typesetter {
    let load = |path: &str, what: &str| {
        Font::from_bytes(
            std::fs::read(path).unwrap_or_else(|e| panic!("corpus {what} font present: {e}")),
            0,
        )
        .unwrap_or_else(|e| panic!("{what} parses: {e}"))
    };
    Typesetter::with_named_font_families(vec![
        FontFamily::new(
            "Noto Sans",
            vec![WeightedFont::new(
                load(FONT_LATIN, "Noto Sans Regular"),
                400,
            )],
        ),
        FontFamily::new(
            "Noto Sans Arabic",
            vec![WeightedFont::new(
                load(FONT_ARABIC, "Noto Sans Arabic Regular"),
                400,
            )],
        ),
    ])
}

/// The atlases in the cascade's font-slot order, matching [`typesetter`].
///
/// The contract between the two lists, and getting it wrong samples the wrong
/// face rather than failing — the same hazard `goldens::render`'s own cascade
/// records.
fn atlases() -> Vec<dashpaint::Atlas> {
    vec![load_atlas(ATLAS_LATIN), load_atlas(ATLAS_ARABIC)]
}

/// A backdrop with one Latin text node and one Arabic one, committed.
fn author_scene(ts: &mut Typesetter) -> (Arena, NodeId, NodeId) {
    let mut arena = Arena::new();
    let nodes = {
        let mut solver = TaffySolver::with_text(ts, atlases());
        let mut txn = arena.open();

        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(CANVAS_W));
        txn.set_prop(root, Prop::Height(CANVAS_H));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));

        let latin = txn.add_node(Some(root), Some("latin"));
        txn.set_prop(latin, Prop::X(20.0));
        txn.set_prop(latin, Prop::Y(20.0));
        txn.set_prop(latin, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(latin, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(latin, Prop::Text(LATIN.to_string()));
        txn.set_prop(
            latin,
            Prop::TextStyle(text_style("Noto Sans", SIZE, NEAR_WHITE)),
        );

        let arabic = txn.add_node(Some(root), Some("arabic"));
        txn.set_prop(arabic, Prop::X(20.0));
        txn.set_prop(arabic, Prop::Y(80.0));
        txn.set_prop(arabic, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(arabic, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(arabic, Prop::Text(ARABIC.to_string()));
        txn.set_prop(
            arabic,
            Prop::TextStyle(text_style("Noto Sans Arabic", SIZE, NEAR_WHITE)),
        );

        txn.commit_with(&mut solver);
        (latin, arabic)
    };
    (arena, nodes.0, nodes.1)
}

/// How many pixels of `pixels` inside `(x, y, w, h)` carry ink brighter than the
/// backdrop.
///
/// The backdrop is `NAVY` and the text is `NEAR_WHITE`, so "brighter than the
/// midpoint" separates them without depending on the resolve's exact ramp — the
/// ramp is layer 2's and its value at a given texel is not what this test is
/// about.
fn ink(pixels: &[u8], width: u32, x: f32, y: f32, w: f32, h: f32) -> usize {
    let (x0, y0) = (x.max(0.0) as u32, y.max(0.0) as u32);
    let (x1, y1) = ((x + w) as u32, (y + h) as u32);
    let mut count = 0;
    for row in y0..y1 {
        for col in x0..x1 {
            let at = ((row * width + col) * 4) as usize;
            if pixels[at] > 128 && pixels[at + 1] > 128 && pixels[at + 2] > 128 {
                count += 1;
            }
        }
    }
    count
}

/// Both scripts reach the device, each out of its own atlas, and each inks the
/// node it was anchored to.
#[test]
fn latin_and_arabic_both_draw_through_the_lean_painter() {
    let mut ts = typesetter();
    let (arena, latin, arabic) = author_scene(&mut ts);
    let scene = arena.committed();

    // Both runs staged, and they name two different atlases — which is what says
    // the cascade resolved each script to its own face.
    let runs = scene.glyphs().runs();
    assert_eq!(runs.len(), 2, "one run per text node: {runs:?}");
    let latin_rect = rect_index_of(&arena, latin);
    let arabic_rect = rect_index_of(&arena, arabic);
    let atlas_of = |rect: u32| {
        runs.iter()
            .find(|run| run.rect == rect)
            .unwrap_or_else(|| panic!("a run is anchored to rect {rect}"))
            .atlas
    };
    assert_ne!(
        atlas_of(latin_rect),
        atlas_of(arabic_rect),
        "the two scripts must sample two different atlases"
    );

    let mut painter = GpuPainter::new();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        scene.glyphs(),
        None,
    );

    // Every quad that places becomes an instance, and each sits in the span of
    // the rect its run is anchored to.
    let quads: usize = runs
        .iter()
        .map(|run| {
            scene
                .glyphs()
                .quads(run)
                .iter()
                .filter(|q| scene.glyphs().atlas(run.atlas).glyph(q.glyph_id).is_some())
                .count()
        })
        .sum();
    let packed = painter
        .instances()
        .instances()
        .iter()
        .filter(|i| i.kind == InstanceKind::Text.as_u32())
        .count();
    assert_eq!(
        packed, quads,
        "every placed glyph of both runs packs one instance"
    );
    assert!(packed >= 10, "the scene shapes more than a token glyph");
    for rect in [latin_rect, arabic_rect] {
        assert!(
            painter
                .instances()
                .rect_instances(rect)
                .iter()
                .any(|i| i.kind == InstanceKind::Text.as_u32()),
            "rect {rect} carries its run's glyphs"
        );
    }

    let mut renderer = Renderer::new().expect("this suite needs a device");
    let (w, h) = (CANVAS_W as u32, CANVAS_H as u32);
    let pixels = renderer
        .render(
            painter.instances(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.glyphs(),
            w,
            h,
        )
        .expect("the canvas is within any device's maximum");

    // Ink inside each text node's own box, and none in the gap between them —
    // so neither run is drawing the other's glyphs at the other's position.
    for (node, name) in [(latin, "latin"), (arabic, "arabic")] {
        let (x, y) = origin_of(&arena, node);
        let (nw, nh) = size_of(&arena, node);
        let inked = ink(&pixels, w, x, y, nw, nh);
        assert!(
            inked > 30,
            "the {name} node's box is inked ({inked} px of {}x{})",
            nw as u32,
            nh as u32
        );
    }
    let (_, latin_y) = origin_of(&arena, latin);
    let (_, latin_h) = size_of(&arena, latin);
    let (_, arabic_y) = origin_of(&arena, arabic);
    let gap_top = latin_y + latin_h;
    assert!(
        arabic_y > gap_top,
        "the fixture leaves a gap between the two nodes"
    );
    assert_eq!(
        ink(&pixels, w, 0.0, gap_top, CANVAS_W, arabic_y - gap_top),
        0,
        "nothing is drawn between the two text nodes"
    );

    // Two atlases of one texel format share one residency texture, so the frame
    // is still one draw call — the property that makes text cost no more calls
    // than a solid fill.
    assert_eq!(renderer.last_draw_runs(), 1, "one atlas texture, one run");
    // Both atlases are PNGs, so both were decoded — once each.
    assert_eq!(renderer.decodes(), 2, "one decode per atlas");
}

/// A second frame of the same scene decodes nothing and allocates nothing.
///
/// The counter, not the picture. Story #581's review found a resident PNG being
/// re-decoded on every frame — 20.4 % of every frame, and invisible in the
/// picture, because the picture is identical either way. A glyph atlas is the
/// larger payload of the two, so the same defect here would cost more.
#[test]
fn a_second_frame_re_decodes_no_atlas_and_allocates_nothing() {
    let mut ts = typesetter();
    let (arena, _, _) = author_scene(&mut ts);
    let scene = arena.committed();
    let mut painter = GpuPainter::new();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        scene.glyphs(),
        None,
    );

    let mut renderer = Renderer::new().expect("this suite needs a device");
    let (w, h) = (CANVAS_W as u32, CANVAS_H as u32);
    let frame = |renderer: &mut Renderer| {
        renderer
            .render(
                painter.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                scene.glyphs(),
                w,
                h,
            )
            .expect("the canvas is within any device's maximum")
    };

    let first = frame(&mut renderer);
    let decodes = renderer.decodes();
    let allocations = renderer.allocations();
    let second = frame(&mut renderer);

    assert_eq!(first, second, "the same frame draws the same pixels");
    assert_eq!(
        renderer.decodes(),
        decodes,
        "a resident glyph atlas must never be decoded again"
    );
    assert_eq!(
        renderer.allocations(),
        allocations,
        "a steady-state frame allocates nothing, residency included"
    );
}

/// A copy of `source` in which run `run_index` names `atlas` instead of its own.
///
/// Rebuilt rather than mutated because `GlyphRunTable::push_run` assigns a run's
/// quad range and refuses one that arrives with offsets into some other array.
fn with_run_atlas(source: &GlyphRunTable, run_index: usize, atlas: AtlasIndex) -> GlyphRunTable {
    let mut out = GlyphRunTable::new();
    for existing in source.atlases() {
        out.push_atlas(existing.clone());
    }
    for (index, run) in source.runs().iter().enumerate() {
        let quads = source.quads(run).to_vec();
        let mut run = *run;
        run.glyphs = GlyphRange::UNASSIGNED;
        if index == run_index {
            run.atlas = atlas;
        }
        out.push_run(run, &quads);
    }
    out
}

/// The renderer resolves each run's parameters from the atlas **that run names**.
///
/// # Why this is a probe and not an ordinary assertion
///
/// The two committed atlases agree on extent, on `px_per_em` and on
/// `distance_range_px` — they are produced by one pinned generator at one
/// setting — so a run's row is bit-identical whichever of them it is built from.
/// The only thing that differs is which sheet of texels the slot holds, and that
/// shows up in the *shapes* drawn rather than in how many pixels are inked:
/// substituting one atlas for the other moved the Arabic node's ink by five
/// pixels of 736, which no tolerance can separate from noise.
///
/// So the probe varies exactly one input. It packs **once**, from the scene's
/// own table, and renders twice — the second time with a table in which the
/// second run names the first run's atlas. The instance buffer is byte-identical
/// across the two, so the pictures can differ at all only if the renderer reads
/// `GlyphRun::atlas`. A renderer that resolved a fixed atlas, or the wrong one,
/// draws the same picture twice and fails here.
///
/// Handing the renderer a table the buffer was not packed from is not a
/// supported call — the two are one frame — and that is the point: it is the
/// only way to hold the packer still while the renderer's view moves.
#[test]
fn the_renderer_resolves_each_run_against_the_atlas_that_run_names() {
    let mut ts = typesetter();
    let (arena, _, _) = author_scene(&mut ts);
    let scene = arena.committed();
    let table = scene.glyphs();
    assert_eq!(table.runs().len(), 2);
    assert_ne!(
        table.runs()[0].atlas,
        table.runs()[1].atlas,
        "the probe needs two runs naming two atlases"
    );

    let mut painter = GpuPainter::new();
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        table,
        None,
    );

    let (w, h) = (CANVAS_W as u32, CANVAS_H as u32);
    let draw = |glyphs: &GlyphRunTable| {
        Renderer::new()
            .expect("this suite needs a device")
            .render(
                painter.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                glyphs,
                w,
                h,
            )
            .expect("the canvas is within any device's maximum")
    };

    let authored = draw(table);
    let swapped = draw(&with_run_atlas(table, 1, table.runs()[0].atlas));
    assert_ne!(
        authored, swapped,
        "the second run drew the same pixels from a different atlas, so the renderer is not \
         reading GlyphRun::atlas"
    );
}
