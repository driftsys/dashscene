//! The v0.7 multi-font fallback golden (issue #219): one mixed-script text
//! node — an Arabic label, Arabic-Indic numerals, and a Latin unit "km/h"
//! whose letters and solidus Noto Sans Arabic does not carry — rendered as
//! textured MSDF atlas quads through the Skia reference painter. It is the
//! visual proof that the typesetter's coverage cascade (story #219) routes
//! each codepoint to a font that can shape it, and that the boundary-B
//! run table samples the right atlas per run (#30's multi-atlas contract).
//!
//! What the node exercises in one paragraph:
//! - the Arabic word "sur'a" (speed) shapes in the primary Arabic font
//!   with its contextual forms (right-to-left);
//! - the authored European "60" renders as Arabic-Indic shapes because its
//!   context is Arabic, still in the primary Arabic font;
//! - "km/h" cascades to the Latin fallback font — glyphs the Arabic font
//!   cannot render — and is sampled from a second atlas.
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): both fonts
//! are the committed corpus fixtures; both atlases are the committed,
//! R7-reproducible fixtures (`corpus/atlas/arabic` and `corpus/atlas/ascii`)
//! — no `msdf-atlas-gen` at render time; bidi, the cascade, shaping, and
//! line breaking are deterministic. MSDF resolve is anti-aliased at every
//! glyph edge, so — like the v0.5 and v0.6 text goldens — the comparison is
//! an absolute-pixel budget, not bit-exact.
//!
//! Regeneration and diff workflow: goldens/README.md.

use std::collections::BTreeSet;

use dashpaint::{AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{
    Arena, AxisSizing, LayoutMode, NodeId, Prop, TextAlign, TextAlignV, TextStyle,
};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{NAVY, NEAR_WHITE, anchor_of, decode_golden, diff_vs, load_atlas, origin_of};

const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);

/// The committed, R7-reproducible atlases — one per font, under the shared
/// `corpus/atlas/` home (debt #217). The Arabic atlas covers the primary
/// font's glyphs; the ASCII atlas covers the Latin fallback's. Reused
/// rather than regenerated so the golden needs no build tool.
const ATLAS_ARABIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");
const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");

/// "sur'a 60 km/h": the Arabic word, a European "60" that renders
/// Arabic-Indic in context, and the Latin unit the fallback font carries.
const LABEL: &str = "سرعة 60 km/h";
const LABEL_SIZE: f32 = 34.0;

/// The golden's absolute-pixel budget, recalibrated for this scene (story
/// #219 review C3). Measured here: the healthy render matches the committed
/// golden exactly (0 px, same-machine determinism); the whole label inks
/// 1,491 px, of which the Latin unit is 781 px and the Arabic segment 714
/// px. The old 1,000-px budget (reused from the larger v0.6 scene) exceeded
/// the smaller 714-px segment, so either font's glyphs vanishing entirely
/// would have passed. 500 px is close to the same tolerance-per-inked-pixel
/// as the CI-proven v0.6 Arabic golden (its 1,000 px over 2,820 px of ink
/// scales to 528 px at this scene's 1,491 px), and it sits well below the
/// 714-px smaller segment — so a dropped font fails, proven by
/// `dropping_either_fonts_runs_exceeds_the_budget`.
///
/// That v0.6 comparison records the story #219 measurement; it is not a live
/// cross-reference. Issue #532 has since recalibrated the v0.6 golden to 440 px
/// against its own smallest break, and that scene's ink measures 2,421 px
/// today. This budget does not depend on either figure: it is gated on this
/// scene's own 714-px smaller segment, which is unchanged.
const BUDGET: usize = 500;

/// Stages `text` at `size` as one or more glyph runs, splitting where the
/// cascade switched fonts so each run samples the atlas of its own font.
/// The font index a glyph carries indexes `atlases` (built in font-list
/// order), so font 0 samples the primary atlas and font 1 the fallback.
/// Positions cross boundary B already absolute (P2): the node's box origin
/// is added here, the painter moves nothing.
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    anchor: u32,
    origin: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
) -> Vec<GlyphRun> {
    let laid = ts.layout(text, size, None);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + g.y,
            };
            match runs.last_mut() {
                // Consecutive glyphs of the same font extend the run;
                // a font switch starts a new run against the other atlas.
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
                    rect: anchor,
                    atlas,
                    size,
                    color,
                    glyphs: vec![quad],
                    opacity: 1.0,
                }),
            }
        }
    }
    runs
}

/// Authors the mixed-script scene and commits it through the multi-font
/// typesetter, returning the arena and the text node. Shared by the golden
/// and the staging guard so both pin the same scene.
fn author_scene(ts: &mut Typesetter) -> (Arena, NodeId) {
    let mut arena = Arena::new();
    let label = {
        let mut solver = TaffySolver::with_typesetter(ts);
        let mut txn = arena.open();

        // A 360x96 navy backdrop; children place by their authored offset.
        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(360.0));
        txn.set_prop(root, Prop::Height(96.0));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));

        // The mixed-script label: hug-sized, so the measure callback (#29)
        // sizes its box to the shaped extent across both fonts.
        let label = txn.add_node(Some(root), Some("label"));
        txn.set_prop(label, Prop::X(24.0));
        txn.set_prop(label, Prop::Y(28.0));
        txn.set_prop(label, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(label, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(label, Prop::Text(LABEL.to_string()));
        txn.set_prop(
            label,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans Arabic".to_string(),
                size: LABEL_SIZE,
                weight: 400,
                color: NEAR_WHITE,
                line_height_px: None,
                letter_spacing: 0.0,
                text_align: TextAlign::Left,
                text_align_v: TextAlignV::Top,
                ligatures_off: false,
            }),
        );

        txn.commit_with(&mut solver);
        label
    };
    (arena, label)
}

/// The multi-font typesetter the whole scene shares: primary Arabic,
/// Latin fallback.
fn typesetter() -> Typesetter {
    let arabic = Font::from_bytes(std::fs::read(FONT_ARABIC).expect("corpus font present"), 0)
        .expect("Noto Sans Arabic parses");
    let latin = Font::from_bytes(std::fs::read(FONT_LATIN).expect("corpus font present"), 0)
        .expect("Noto Sans parses");
    Typesetter::with_fonts(vec![arabic, latin])
}

#[test]
fn mixed_script_fallback_matches_its_golden() {
    let mut ts = typesetter();
    let (arena, label) = author_scene(&mut ts);

    // Stage the two atlases in font-list order: the Arabic primary at
    // index 0, the Latin fallback at index 1.
    let mut glyphs = GlyphRunTable::new();
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    let latin = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    for run in text_runs(
        &mut ts,
        &[arabic, latin],
        anchor_of(&arena, label),
        origin_of(&arena, label),
        LABEL,
        LABEL_SIZE,
        NEAR_WHITE,
    ) {
        glyphs.push_run(run);
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &glyphs,
        None,
    );

    // Sparse text on a large canvas, so an absolute-pixel budget (not a
    // canvas fraction) is the right tolerance — a fraction wide enough to
    // clear cross-machine anti-aliasing jitter would exceed the inked
    // footprint and pass a render that drew no text
    // (`docs/decisions/golden-comparison-space.md`, "Text goldens"). The
    // `BUDGET` is recalibrated for this scene so a vanished font fails (see
    // its doc and `dropping_either_fonts_runs_exceeds_the_budget`); the
    // staging guard below pins the cascade at glyph-and-font level,
    // machine-independent.
    goldens::assert_matches_golden_max_pixels("v07-text-fallback", &painter.png_bytes(), BUDGET);
}

/// A staging guard independent of the golden image: it pins, at
/// glyph-and-font level, that the scene actually cascades across both
/// fonts and samples both atlases — the features the coarse pixel budget
/// is too loose to catch. Each assertion fails with a specific message if
/// the cascade or the atlas routing regresses.
#[test]
fn the_label_cascades_across_both_atlases() {
    let mut ts = typesetter();
    let (arena, label) = author_scene(&mut ts);

    let mut glyphs = GlyphRunTable::new();
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    let latin = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    let runs = text_runs(
        &mut ts,
        &[arabic, latin],
        anchor_of(&arena, label),
        origin_of(&arena, label),
        LABEL,
        LABEL_SIZE,
        NEAR_WHITE,
    );

    // Both atlases are referenced — the label did not collapse to one font.
    assert!(
        runs.iter().any(|r| r.atlas == arabic),
        "the Arabic word and numerals must sample the primary atlas"
    );
    assert!(
        runs.iter().any(|r| r.atlas == latin),
        "the Latin unit must sample the fallback atlas"
    );

    // Every glyph a run carries is covered by that run's atlas: no run
    // points a glyph at an atlas that cannot place it (the coverage oracle
    // #35 established, per font). The full metrics glyph set includes
    // no-outline glyphs such as space, which paint nothing but are still
    // "covered".
    let arabic_covered = atlas_glyph_ids(ATLAS_ARABIC_DIR);
    let ascii_covered = atlas_glyph_ids(ATLAS_ASCII_DIR);
    for run in &runs {
        let covered = if run.atlas == arabic {
            &arabic_covered
        } else {
            &ascii_covered
        };
        for quad in &run.glyphs {
            // A .notdef would mean the cascade sent a codepoint to a font
            // that cannot shape it — the failure this guard exists to catch.
            assert_ne!(quad.glyph_id, 0, "no glyph shaped to .notdef");
            assert!(
                covered.contains(&quad.glyph_id),
                "run glyph {} is absent from its committed atlas",
                quad.glyph_id
            );
        }
    }
}

/// The glyph ids a committed atlas fixture places, from its metrics blob.
fn atlas_glyph_ids(dir: &str) -> BTreeSet<u16> {
    let bundle = dashscene_typeset::atlas::AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"));
    bundle.metrics.glyphs.iter().map(|g| g.glyph_id).collect()
}

/// C3 sensitivity guard: the pixel budget must be tight enough that either
/// font's glyphs vanishing fails the compare. This renders the scene with
/// one font's runs dropped and asserts each broken render differs from the
/// committed golden by MORE than the budget — so a painter-side per-atlas
/// regression that erases one font cannot slip through the golden's
/// tolerance. Measured on this scene (story #219 review C3): dropping the
/// Latin unit differs by 781 px, dropping the Arabic segment by 714 px,
/// both above the 500-px budget; the healthy render differs by 0 px (the
/// golden test itself proves that).
#[test]
fn dropping_either_fonts_runs_exceeds_the_budget() {
    let mut ts = typesetter();
    let (arena, label) = author_scene(&mut ts);
    let scene = arena.committed();
    let root = scene.rects()[0];
    let (w, h) = (root.w as i32, root.h as i32);
    let golden = decode_golden("v07-text-fallback");

    let staged = |ts: &mut Typesetter, keep_fallback: bool| {
        let mut g = GlyphRunTable::new();
        let arabic = g.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
        let latin = g.push_atlas(load_atlas(ATLAS_ASCII_DIR));
        for run in text_runs(
            ts,
            &[arabic, latin],
            anchor_of(&arena, label),
            origin_of(&arena, label),
            LABEL,
            LABEL_SIZE,
            NEAR_WHITE,
        ) {
            // Drop the fallback (Latin) runs, or drop the primary (Arabic).
            let is_fallback = run.atlas == latin;
            if is_fallback == keep_fallback {
                g.push_run(run);
            }
        }
        g
    };

    let no_arabic = diff_vs(&golden, &render_rgba(&staged(&mut ts, true), w, h, scene));
    let no_latin = diff_vs(&golden, &render_rgba(&staged(&mut ts, false), w, h, scene));
    assert!(
        no_arabic > BUDGET,
        "dropping the Arabic segment must exceed the {BUDGET}px budget, differed by {no_arabic}"
    );
    assert!(
        no_latin > BUDGET,
        "dropping the Latin unit must exceed the {BUDGET}px budget, differed by {no_latin}"
    );
}

/// Renders a glyph-run table over the committed scene into unpremultiplied
/// RGBA8888 — the golden comparison space.
fn render_rgba(
    table: &GlyphRunTable,
    w: i32,
    h: i32,
    scene: &dashscene_core::CommittedScene,
) -> Vec<u8> {
    let mut painter = SkiaPainter::new(w, h);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        table,
        None,
    );
    painter.rgba_bytes()
}
