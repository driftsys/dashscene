//! The v0.6 Arabic screen golden (issue #35, exit criterion E2): a pure
//! Arabic-plus-numerals screen — right-to-left, shaped contextual forms,
//! lam-alef, harakat, and mixed numerals — rendered as textured MSDF
//! atlas quads through the Skia reference painter. It is the visual proof
//! that the bidi/RTL seam (#32), Arabic shaping and digit-shape selection
//! (#33), the GSUB-closure atlas (#34), and glyph painting (#30) compose
//! end to end.
//!
//! What the screen exercises, one node per named E2 feature:
//! - a fixed-width banner whose right-to-left paragraph sits flush-right
//!   in a box wider than the text, with an as-salaamu-alaikum greeting
//!   that carries a lam-alef ligature (RTL placement + contextual forms);
//! - a hug-sized harakat word (marhaban) whose fatha/sukun marks are
//!   GPOS-stacked above the letters (harakat + hug via the #29 measure
//!   callback);
//! - a hug-sized speed chip whose authored European digits ("120") render
//!   as Arabic-Indic shapes because their context is Arabic (mixed
//!   numerals + hug).
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): the font is
//! the committed corpus Noto Sans Arabic; the atlas is the committed,
//! R7-reproducible Arabic fixture (`corpus/atlas/arabic`) — no
//! `msdf-atlas-gen` at render time; bidi, shaping, and line breaking are
//! deterministic. MSDF resolve is anti-aliased at every glyph edge, so —
//! like the v0.5 Latin text golden — the comparison is tolerance-based,
//! not bit-exact.
//!
//! Regeneration and diff workflow: goldens/README.md.

use std::collections::BTreeSet;

use dashpaint::{AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, AxisSizing, LayoutMode, NodeId, Prop, TextStyle};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, Typesetter};

mod common;

use common::{AMBER, INK, NAVY, NEAR_WHITE, PANEL, load_atlas, origin_of, size_of};

const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);

/// The committed, R7-reproducible Arabic atlas — the same font as `FONT`,
/// under the shared `corpus/atlas/` home (not a crate's private test
/// tree — debt #217). Reused rather than regenerated so the golden needs
/// no build tool.
const ATLAS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// The greeting "as-salaamu alaikum". The second lam is directly
/// followed by an alef, so it shapes to a lam-alef ligature; every letter
/// takes its joining-context form.
const BANNER: &str = "السلام عليكم";
/// "marhaban" (welcome) with harakat: a fatha over the meem, a sukun over
/// the reh, a fatha over the hah, and a fathatan over the beh — marks the
/// typesetter stacks above the baseline through GPOS.
const HARAKAT_WORD: &str = "مَرْحَبًا";
/// A speed readout. The Arabic word "sur'a" (speed) makes the run's
/// context Arabic, so the authored European "120" renders with
/// Arabic-Indic digit shapes.
const SPEED: &str = "سرعة 120";

const BANNER_WIDTH: f32 = 300.0;
const BANNER_SIZE: f32 = 26.0;
const WORD_SIZE: f32 = 34.0;
const SPEED_SIZE: f32 = 34.0;

/// Shapes `text` at `size` within `max_width` and places every glyph in
/// absolute document space by adding the node's resolved box origin. For
/// a fixed-width RTL box `max_width` is the box width, so the paragraph's
/// flush-right shift is already in each glyph's `x` when it crosses
/// boundary B; the painter never moves anything (P2).
fn text_run(
    ts: &mut Typesetter,
    atlas: AtlasIndex,
    origin: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
    max_width: Option<f32>,
) -> GlyphRun {
    let laid = ts.layout(text, size, max_width);
    let mut glyphs = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            glyphs.push(GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + g.y,
            });
        }
    }
    GlyphRun {
        atlas,
        size,
        color,
        glyphs,
        opacity: 1.0,
    }
}

/// Authors the Arabic screen and commits it through the one typesetter,
/// returning the arena and the three text nodes. Shared by the golden and
/// the layout guard so both pin the same scene.
fn author_scene(ts: &mut Typesetter) -> (Arena, NodeId, NodeId, NodeId) {
    let mut arena = Arena::new();
    let nodes = {
        let mut solver = TaffySolver::with_typesetter(ts);
        let mut txn = arena.open();

        // A 340x210 navy backdrop; children place by their authored
        // offset (mode None).
        let root = txn.add_node(None, Some("backdrop"));
        txn.set_prop(root, Prop::Width(340.0));
        txn.set_prop(root, Prop::Height(210.0));
        txn.set_prop(root, Prop::Mode(LayoutMode::None));
        txn.set_prop(root, Prop::Fill(NAVY));

        // The banner: a fixed-width panel wider than its text, so the
        // right-to-left greeting sits flush-right against the panel's
        // right edge. The panel fill makes the box extent — and the
        // flush-right placement — visible.
        let banner = txn.add_node(Some(root), Some("banner"));
        txn.set_prop(banner, Prop::X(20.0));
        txn.set_prop(banner, Prop::Y(20.0));
        txn.set_prop(banner, Prop::SizingH(AxisSizing::Fixed));
        txn.set_prop(banner, Prop::Width(BANNER_WIDTH));
        txn.set_prop(banner, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(banner, Prop::Fill(PANEL));
        txn.set_prop(
            banner,
            Prop::Corners {
                top_left: 6.0,
                top_right: 6.0,
                bottom_right: 6.0,
                bottom_left: 6.0,
            },
        );
        txn.set_prop(banner, Prop::Text(BANNER.to_string()));
        txn.set_prop(
            banner,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans Arabic".to_string(),
                size: BANNER_SIZE,
                weight: 400,
                color: NEAR_WHITE,
            }),
        );

        // The harakat word: hug-sized, so the measure callback (#29)
        // sizes its box to the shaped marks-and-letters extent.
        let word = txn.add_node(Some(root), Some("word"));
        txn.set_prop(word, Prop::X(20.0));
        txn.set_prop(word, Prop::Y(86.0));
        txn.set_prop(word, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(word, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(word, Prop::Text(HARAKAT_WORD.to_string()));
        txn.set_prop(
            word,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans Arabic".to_string(),
                size: WORD_SIZE,
                weight: 400,
                color: NEAR_WHITE,
            }),
        );

        // The speed chip: hug-sized, an amber rounded box behind the
        // readout whose authored "120" renders Arabic-Indic in context.
        let chip = txn.add_node(Some(root), Some("chip"));
        txn.set_prop(chip, Prop::X(20.0));
        txn.set_prop(chip, Prop::Y(146.0));
        txn.set_prop(chip, Prop::SizingH(AxisSizing::Hug));
        txn.set_prop(chip, Prop::SizingV(AxisSizing::Hug));
        txn.set_prop(chip, Prop::Fill(AMBER));
        txn.set_prop(
            chip,
            Prop::Corners {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        txn.set_prop(chip, Prop::Text(SPEED.to_string()));
        txn.set_prop(
            chip,
            Prop::TextStyle(TextStyle {
                family: "Noto Sans Arabic".to_string(),
                size: SPEED_SIZE,
                weight: 400,
                color: INK,
            }),
        );

        txn.commit_with(&mut solver);
        (banner, word, chip)
    };
    (arena, nodes.0, nodes.1, nodes.2)
}

#[test]
fn arabic_screen_matches_its_golden() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans Arabic parses");
    let mut ts = Typesetter::new(font);
    let (arena, banner, word, chip) = author_scene(&mut ts);

    // Stage the positioned glyph runs at boundary B, sampling the atlas.
    // The banner passes its fixed box width as the wrap width, so its
    // paragraph is flush-right; the hug nodes pass None (one line at the
    // shaped natural width).
    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(load_atlas(ATLAS_DIR));
    glyphs.push_run(text_run(
        &mut ts,
        atlas,
        origin_of(&arena, banner),
        BANNER,
        BANNER_SIZE,
        NEAR_WHITE,
        Some(BANNER_WIDTH),
    ));
    glyphs.push_run(text_run(
        &mut ts,
        atlas,
        origin_of(&arena, word),
        HARAKAT_WORD,
        WORD_SIZE,
        NEAR_WHITE,
        None,
    ));
    glyphs.push_run(text_run(
        &mut ts,
        atlas,
        origin_of(&arena, chip),
        SPEED,
        SPEED_SIZE,
        INK,
        None,
    ));

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

    // The inked text is only ~2,820 px of this 71,400-px canvas (3.95%),
    // so a canvas-fraction budget wide enough to clear anti-aliasing
    // jitter would exceed the whole footprint and pass a render that drew
    // no text at all. An absolute pixel budget avoids that (see
    // `docs/decisions/golden-comparison-space.md`, "Text goldens").
    //
    // 1,000 px is ~2.5x the scene's anti-aliased edge population (the
    // ~400 px that shift by one code point on a premultiply round-trip),
    // so it clears cross-machine coverage jitter, while staying well under
    // the measured breaks it must catch: erasing the text differs by
    // 2,818 px, and a shaping regression that isolates the joined forms
    // differs by 4,633 px. The `..._is_laid_out_and_shaped...` guard pins
    // the shaping features exactly (glyph-id level, machine-independent);
    // this golden is the coarse full-frame check.
    goldens::assert_matches_golden_max_pixels("v06-text-arabic", &painter.png_bytes(), 1_000);
}

/// A layout-and-shaping guard independent of the golden image, pinning at
/// glyph-id level the E2 features the pixel golden is too coarse to catch
/// at this ink density. Each assertion fails with a specific message, not
/// just a pixel count, if the corresponding feature regresses:
///
/// - the three boxes land where the golden expects, the hug nodes take
///   the shaped size, and the banner's RTL paragraph is flush-right;
/// - the banner carries the seen-joined lam-alef ligature and no isolated
///   lam (a lam-alef splitting to isolated forms fails here);
/// - the speed word shapes to contextual forms, not the isolated forms of
///   its letters;
/// - the harakat word's four marks carry nonzero GPOS y-offsets (marks
///   dropping to the baseline fails here);
/// - the authored European speed digits render as Arabic-Indic shapes.
#[test]
fn the_arabic_screen_is_laid_out_and_shaped_as_the_golden_expects() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans Arabic parses");
    let mut ts = Typesetter::new(font);
    let (arena, banner, word, chip) = author_scene(&mut ts);

    // The banner keeps its authored fixed width; the hug nodes take the
    // shaped text's own size from the measure callback (#29).
    let (bx, by) = origin_of(&arena, banner);
    assert_eq!((bx, by), (20.0, 20.0), "banner placed at its offset");
    let (bw, _) = size_of(&arena, banner);
    assert_eq!(bw, BANNER_WIDTH, "banner keeps its fixed width");

    let word_shaped = ts.layout(HARAKAT_WORD, WORD_SIZE, None);
    let (ww, wh) = size_of(&arena, word);
    assert!(
        (ww - word_shaped.width).abs() < 0.01 && ww > 1.0,
        "the harakat word hugged the shaped width ({ww} vs {})",
        word_shaped.width
    );
    assert!((wh - word_shaped.height).abs() < 0.01 && wh > 1.0);

    let speed_shaped = ts.layout(SPEED, SPEED_SIZE, None);
    let (cw, _) = size_of(&arena, chip);
    assert!(
        (cw - speed_shaped.width).abs() < 0.01 && cw > 1.0,
        "the speed chip hugged the shaped width ({cw} vs {})",
        speed_shaped.width
    );

    // Flush-right: within the 300px banner box the greeting occupies the
    // right side, leaving a gap on the left. Positions are box-relative
    // (the paragraph shift, before the origin is added).
    let banner_line = &ts.layout(BANNER, BANNER_SIZE, Some(BANNER_WIDTH)).lines[0];
    let min_x = banner_line.glyphs.iter().fold(f32::MAX, |m, g| m.min(g.x));
    let max_x = banner_line.glyphs.iter().fold(f32::MIN, |m, g| m.max(g.x));
    assert!(
        max_x <= BANNER_WIDTH + 0.01,
        "the RTL paragraph stays inside the box (max glyph x {max_x} <= {BANNER_WIDTH})"
    );
    assert!(
        min_x > 0.5 * BANNER_WIDTH,
        "the RTL paragraph is flush-right, not flush-left (min glyph x {min_x} \
         is past the box mid-line)"
    );

    // Lam-alef ligature. Shaping the lam-alef pair does not just place an
    // isolated lam next to an isolated alef — it ligates (`rlig`).
    let lam = layout_gids(&mut ts, "ل");
    let alef = layout_gids(&mut ts, "ا");
    let lam_alef = layout_gids(&mut ts, "لا");
    assert_ne!(
        lam_alef,
        [lam.clone(), alef].concat(),
        "lam+alef must ligate, not stay two isolated glyphs"
    );
    // In the banner the lam-alef is seen-joined, a distinct contextual
    // form; derive it from "سلا" (seen, then the two-glyph lam-alef
    // ligature in visual order) and require the banner to contain it.
    let sla = layout_gids(&mut ts, "سلا");
    assert_eq!(
        sla.len(),
        3,
        "fixture: seen + a two-glyph seen-joined lam-alef ligature — \
         regenerate this pin if the font changed ({sla:?})"
    );
    let seen_joined_lam_alef = &sla[..2];
    let banner_gids = layout_gids(&mut ts, BANNER);
    assert!(
        contains_subslice(&banner_gids, seen_joined_lam_alef),
        "banner must carry the seen-joined lam-alef ligature {seen_joined_lam_alef:?}, \
         got {banner_gids:?}"
    );
    assert!(
        !banner_gids.contains(&lam[0]),
        "banner must carry no isolated lam (gid {}) — every lam is joined",
        lam[0]
    );

    // Contextual forms. The speed word shapes to joining forms, not the
    // concatenation of its letters shaped in isolation.
    let word_ctx = layout_gids(&mut ts, "سرعة");
    let word_iso: Vec<u16> = "سرعة"
        .chars()
        .flat_map(|c| layout_gids(&mut ts, &c.to_string()))
        .collect();
    assert_ne!(
        word_ctx, word_iso,
        "the word must shape to contextual forms, not isolated ones"
    );

    // Harakat. The four marks of the harakat word carry nonzero GPOS
    // y-offsets (they are positioned off the baseline, not dropped onto
    // it). Letters sit at the baseline; a mark's pen y differs from it.
    let word_line = &ts.layout(HARAKAT_WORD, WORD_SIZE, None).lines[0];
    let marks = word_line
        .glyphs
        .iter()
        .filter(|g| (g.y - word_line.baseline_y).abs() > 1.0)
        .count();
    assert!(
        marks >= 4,
        "the four harakat must carry nonzero GPOS y-offsets, found {marks} offset glyphs"
    );

    // Mixed numerals: the authored European "120" in Arabic context
    // shapes to the same glyph ids as authored Arabic-Indic "١٢٠", and
    // not to the European glyphs an unanchored "120" keeps.
    let ascii_ctx = layout_gids(&mut ts, "سرعة 120");
    let arabic_ctx = layout_gids(&mut ts, "سرعة ١٢٠");
    assert_eq!(
        ascii_ctx, arabic_ctx,
        "European digits must render as Arabic-Indic shapes in Arabic context"
    );
    let unanchored = layout_gids(&mut ts, "120");
    assert!(
        unanchored.iter().all(|g| !arabic_ctx.contains(g)),
        "the Arabic-Indic digit glyphs must differ from the European ones"
    );
}

/// Coverage oracle: every glyph id the scene's exact strings lay out to
/// is present in the committed Arabic atlas. A post-GSUB form the closure
/// sweep missed would be shaped by the runtime but absent from the atlas —
/// a silent missing glyph the golden's coarse tolerance could hide. This
/// catches it before it reaches the golden, and runs everywhere (no atlas
/// tool needed — the fixture is committed).
#[test]
fn every_scene_glyph_is_covered_by_the_committed_atlas() {
    let font = Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
        .expect("Noto Sans Arabic parses");
    let mut ts = Typesetter::new(font);

    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(ATLAS_DIR))
        .expect("committed Arabic atlas fixture loads");
    // The full metrics glyph set (a no-outline glyph such as space is in
    // the atlas and paints nothing — the run is still covered).
    let covered: BTreeSet<u16> = bundle.metrics.glyphs.iter().map(|g| g.glyph_id).collect();

    for text in [BANNER, HARAKAT_WORD, SPEED] {
        for gid in layout_gids(&mut ts, text) {
            assert!(
                covered.contains(&gid),
                "scene text {text:?} lays out glyph id {gid}, absent from the committed atlas"
            );
        }
    }
}

/// The flat glyph-id sequence a layout produces. Glyph ids are
/// size-independent, so one size serves every call.
fn layout_gids(ts: &mut Typesetter, text: &str) -> Vec<u16> {
    ts.layout(text, SPEED_SIZE, None)
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter().map(|g| g.glyph_id))
        .collect()
}

/// Whether `haystack` contains `needle` as a contiguous run.
fn contains_subslice(haystack: &[u16], needle: &[u16]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}
