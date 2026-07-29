//! The v0.7 component-lowering golden (story #242): a captured Figma screen
//! carrying a component set, its variant members, and one instance, lowered by
//! `dashc` and driven the whole way to pixels —
//!
//!     Figma REST JSON → dashc lower + emit → .dsb
//!         → dashscene-core load → engine measure callback (#29)
//!         → boundary-B glyph runs → Skia painter → PNG
//!
//! It is the visual proof that a local `INSTANCE` resolves to its component's
//! content while the `COMPONENT_SET`/`COMPONENT` definitions resolve but do not
//! paint (`docs/decisions/figma-component-lowering.md`): only the instance's
//! authored (collapsed) subtree reaches the picture — the gray container, the
//! dark "state: collapsed" label, and the one blue row — never the set or its
//! expanded variant with four rows.
//!
//! The document carries the authored family "Inter", which no committed corpus
//! font provides; the runtime resolves a family to fonts caller-side (there is
//! no registry — `docs/design/typeset-latin.md`), so this golden renders with
//! the committed Noto Sans, the same substitution the v0.7 text golden makes.
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): the font and the
//! ASCII atlas are the committed, R7-reproducible fixtures; lowering, loading,
//! shaping, and solving are deterministic. MSDF resolve is anti-aliased at every
//! glyph edge, so — like the other text goldens — the comparison is an
//! absolute-pixel budget, not bit-exact.
//!
//! Regeneration and diff workflow: `goldens/README.md`.

use std::collections::BTreeMap;

use dashc_wasm::compile_figma;
use dashpaint::{Color, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};
use dashscene_validator::Profile;

mod common;
use common::{decode_golden, decode_rgba, diff_vs, load_atlas, rect_index_of};

const VARIANT_TOPOLOGY: &str =
    include_str!("../../../corpus/figma-fixtures/lowering-variant-topology.json");
const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const ATLAS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");

/// The instance's label: the collapsed variant's text, its size, and ink color.
const TEXT: &str = "state: collapsed";
const TEXT_SIZE: f32 = 14.0;
const INK: Color = Color {
    r: 0.1,
    g: 0.1,
    b: 0.1,
    a: 1.0,
};

/// The golden's absolute-pixel budget, calibrated for this scene (the #232/#235
/// lesson: a budget must sit below the inked footprint, or a regression that
/// erases the content passes). Re-measured on this scene after story #542: the
/// healthy render matches the committed golden exactly (0 px, same-machine
/// determinism); the resolved instance's label inks **593 px**. 200 px
/// (≈ 0.34× the footprint) leaves headroom for cross-machine MSDF edge jitter
/// along the label's glyph edges — the same per-pixel jitter budget the 14px
/// v0.7 text golden uses — while staying well under that footprint, so dropping
/// the label fails by a 393 px margin, proven by
/// `dropping_the_instance_label_exceeds_the_budget`.
///
/// The footprint grew from 480 px because the golden itself was re-recorded in
/// story #542, and the budget is deliberately left where it was: the change
/// widened the margin rather than narrowing it, so no recalibration is owed.
///
/// **Why the golden moved.** Until #542 this file staged the label itself, with
/// `ts.layout(TEXT, TEXT_SIZE, None)` — no wrap width. The instance container
/// solves to a fixed 100 px, so the label's own box measures 62.09 x 38.136:
/// two lines. The old staging drew one 102 px line from x = 16, which ran off
/// the 100 px canvas and was clipped at its right edge. The committed image
/// pinned that, even though the production `.dsb` render path
/// (`goldens::render`) already wrapped at the solved width and would never have
/// produced it. Commit is now the one producer, so the picture is the two-line
/// label the solve measured, ending inside the box.
const BUDGET: usize = 200;

fn typesetter() -> Typesetter {
    Typesetter::new(
        Font::from_bytes(std::fs::read(FONT).expect("corpus font present"), 0)
            .expect("Noto Sans parses"),
    )
}

/// Lowers the raw component fixture, loads it into the arena, and re-commits
/// through the measure-callback solver so the HUG text label is sized by the
/// typesetter (#29). Returns the committed arena and the label node.
fn lower_and_solve(ts: &mut Typesetter) -> (Arena, NodeId) {
    let (bytes, report) = compile_figma(VARIANT_TOPOLOGY, Profile::Core, &BTreeMap::new())
        .expect("the component fixture compiles");
    assert!(report.is_empty(), "the raw fixture lowers clean: {report}");
    let (document, payloads) = dashbuf::open(&bytes).expect("a valid .dsb file");

    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; an empty transaction re-committed through a typesetter-backed
    // solver performs a full solve with the measure seam, and — since the solver
    // carries the atlas — stages the label's glyph runs in the same commit.
    {
        let mut solver = TaffySolver::with_text(ts, vec![load_atlas(ATLAS_DIR)]);
        arena.open().commit_with(&mut solver);
    }
    let text = find_text(&arena, TEXT);
    (arena, text)
}

#[test]
fn the_resolved_instance_solves_and_paints_to_its_golden() {
    let mut ts = typesetter();
    let (arena, text) = lower_and_solve(&mut ts);

    // Only the instance's subtree is in the scene: its root, the label, and the
    // one row — the set and its members never entered the document.
    assert_eq!(
        arena.roots().len(),
        1,
        "the instance is the one document root; the definitions do not paint",
    );

    // The label's run comes from the commit, so its anchor, size and ink are
    // what the resolved instance actually lowered to.
    let runs = arena.committed().glyphs().runs();
    assert_eq!(runs.len(), 1, "the resolved instance has one text leaf");
    assert_eq!(runs[0].rect, rect_index_of(&arena, text));
    assert_eq!(runs[0].size, TEXT_SIZE);
    assert_eq!(runs[0].color, INK);

    let png = render(&arena, arena.committed().glyphs());
    goldens::assert_matches_golden_max_pixels("v07-variant-topology", &png, BUDGET);
}

/// Sensitivity guard (the #232/#235 lesson): the budget must be tight enough
/// that erasing the resolved instance's label fails the compare. Renders the
/// lowered scene with the glyph run dropped and asserts it differs from the
/// committed golden by more than the budget.
#[test]
fn dropping_the_instance_label_exceeds_the_budget() {
    let mut ts = typesetter();
    let (arena, _) = lower_and_solve(&mut ts);

    let empty = render(&arena, &GlyphRunTable::new());
    let differed = diff_vs(&decode_golden("v07-variant-topology"), &decode_rgba(&empty));
    assert!(
        differed > BUDGET,
        "dropping the resolved label must exceed the {BUDGET}px budget, differed by {differed}",
    );
}

/// Renders the committed scene with a glyph-run table into a PNG at the root's
/// solved size.
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
