//! The v0.7 text-lowering golden (story #160): a captured Figma screen,
//! lowered by `dashc`, driven the whole way to pixels —
//!
//!     Figma REST JSON → dashc lower + emit → .dsb
//!         → dashscene-core load → engine measure callback (#29)
//!         → boundary-B glyph runs → Skia painter → PNG
//!
//! It is the visual proof that a `TEXT` node's authored characters and style
//! survive the lowering and render: the `lowering-hug-in-fill` fixture's HUG
//! text leaf flows through the measure seam (its box is the shaped text's own
//! size, not a baked one — P1), and the painter draws each glyph as one MSDF
//! atlas quad sampled from the committed ASCII atlas.
//!
//! The document carries the authored family "Inter", which no committed corpus
//! font provides; the runtime resolves a family to fonts caller-side (there is
//! no registry — `docs/design/typeset-latin.md`), so this golden renders with
//! the committed Noto Sans. The measure callback reads only the style's size,
//! so the hug box is font-driven either way.
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): the font and the
//! ASCII atlas are the committed, R7-reproducible fixtures; lowering, loading,
//! shaping, and solving are deterministic. MSDF resolve is anti-aliased at
//! every glyph edge, so — like the v0.5/v0.6/v0.7 text goldens — the
//! comparison is an absolute-pixel budget, not bit-exact.
//!
//! Regeneration and diff workflow: goldens/README.md.

use std::collections::BTreeMap;

use dashc_wasm::compile_figma;
use dashpaint::{Color, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};
use dashscene_validator::Profile;

mod common;
use common::{decode_golden, decode_rgba, diff_vs, load_atlas, size_of};

const HUG_IN_FILL: &str = include_str!("../../../corpus/figma-fixtures/lowering-hug-in-fill.json");
const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const ATLAS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");

/// The fixture's text leaf: its characters, size, and ink color.
const TEXT: &str = "hug inside fill";
const TEXT_SIZE: f32 = 14.0;
const INK: Color = Color {
    r: 0.1,
    g: 0.1,
    b: 0.1,
    a: 1.0,
};

/// The golden's absolute-pixel budget, calibrated for this scene (the
/// #232/#235 lesson: a text budget must sit below the inked footprint, or a
/// regression that erases the text passes). Measured on this scene: the
/// healthy render matches the committed golden exactly (0 px, same-machine
/// determinism); the lowered text inks 484 px. 200 px (≈ 0.41× the footprint)
/// leaves headroom for cross-machine MSDF edge jitter — higher per inked pixel
/// for 14px text than for the larger v0.6 scene — while staying well under
/// that footprint, so dropping the text run fails by a 284 px margin, proven
/// by `dropping_the_text_run_exceeds_the_budget`.
const BUDGET: usize = goldens::CROSS_ARCH_BUDGET_PX;

fn typesetter() -> Typesetter {
    Typesetter::new(
        Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
            .expect("Noto Sans parses"),
    )
}

/// Lowers the fixture, loads it into the arena, and re-commits through the
/// measure-callback solver so the HUG text leaf is sized by the typesetter
/// (#29). Returns the committed arena and the text node.
fn lower_and_solve(ts: &mut Typesetter) -> (Arena, NodeId) {
    let (bytes, report) =
        compile_figma(HUG_IN_FILL, Profile::Core, &BTreeMap::new()).expect("the fixture compiles");
    assert!(report.is_empty(), "the raw fixture lowers clean: {report}");
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");

    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text
    // node to zero; an empty transaction re-committed through a
    // typesetter-backed solver performs a full solve with the measure seam,
    // and — since the solver carries the atlas — stages the text leaf's glyph
    // runs in the same commit.
    {
        let mut solver = TaffySolver::with_text(ts, vec![load_atlas(ATLAS_DIR)]);
        arena.open().commit_with(&mut solver);
    }
    let text = find_text(&arena, TEXT);
    (arena, text)
}

#[test]
fn lowered_text_solves_and_paints_to_its_golden() {
    let mut ts = typesetter();
    let (arena, text) = lower_and_solve(&mut ts);

    // The lowered HUG text flowed through the measure callback (#29): its box
    // is the shaped text's own size, not the authored (zero) default.
    let expected = ts.layout(TEXT, TEXT_SIZE, None);
    let (tw, th) = size_of(&arena, text);
    assert!(
        (tw - expected.width).abs() < 0.01 && tw > 1.0,
        "the text leaf hugged the shaped width ({tw} vs {})",
        expected.width,
    );
    assert!((th - expected.height).abs() < 0.01 && th > 1.0);

    // The run's size and fill now come from the lowered text style rather than
    // from constants this test repeats, so asserting them here pins what
    // `dashc` lowered instead of restating it.
    let runs = arena.committed().glyphs().runs();
    assert_eq!(runs.len(), 1, "the fixture has exactly one text leaf");
    assert_eq!(runs[0].size, TEXT_SIZE, "the run is at the lowered size");
    assert_eq!(runs[0].color, INK, "the run inks the lowered colour");

    let png = render(&arena, arena.committed().glyphs());
    goldens::assert_matches_golden_max_pixels("v07-text-lowering", &png, BUDGET);
}

/// Sensitivity guard (the #232/#235 lesson): the budget must be tight enough
/// that erasing the text fails the compare. Renders the lowered scene with the
/// glyph run dropped and asserts it differs from the committed golden by more
/// than the budget, so a regression that drops the lowered text cannot slip
/// through the tolerance.
#[test]
fn dropping_the_text_run_exceeds_the_budget() {
    let mut ts = typesetter();
    let (arena, _) = lower_and_solve(&mut ts);

    let empty = render(&arena, &GlyphRunTable::new());
    let differed = diff_vs(&decode_golden("v07-text-lowering"), &decode_rgba(&empty));
    assert!(
        differed > BUDGET,
        "dropping the lowered text must exceed the {BUDGET}px budget, differed by {differed}",
    );
}

/// Renders the committed scene with a glyph-run table into a PNG.
fn render(arena: &Arena, glyphs: &GlyphRunTable) -> Vec<u8> {
    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        glyphs,
        None,
    );
    painter.png_bytes()
}

/// The lowered node in `arena` whose text equals `want`.
fn find_text(arena: &Arena, want: &str) -> NodeId {
    fn search(arena: &Arena, node: NodeId, want: &str) -> Option<NodeId> {
        if arena.text(node) == Some(want) {
            return Some(node);
        }
        arena
            .children(node)
            .iter()
            .find_map(|&child| search(arena, child, want))
    }
    arena
        .roots()
        .iter()
        .find_map(|&root| search(arena, root, want))
        .unwrap_or_else(|| panic!("no arena node carries the text {want:?}"))
}
