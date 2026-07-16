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

use dashbuf::root_as_document;
use dashc_wasm::compile_figma;
use dashpaint::{AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, Typesetter};
use dashscene_validator::Profile;

mod common;
use common::{decode_golden, decode_rgba, diff_vs, load_atlas, origin_of};

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
/// erases the content passes). Measured on this scene: the healthy render
/// matches the committed golden exactly (0 px, same-machine determinism); the
/// resolved instance's label inks 480 px. 200 px (≈ 0.42× the footprint) leaves
/// headroom for cross-machine MSDF edge jitter along the label's glyph edges —
/// the same per-pixel jitter budget the 14px v0.7 text golden uses — while
/// staying well under that footprint, so dropping the label fails by a 280 px
/// margin, proven by `dropping_the_instance_label_exceeds_the_budget`.
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
    let document = root_as_document(&bytes).expect("a valid buffer");

    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; an empty transaction re-committed through a typesetter-backed
    // solver performs a full solve with the measure seam.
    {
        let mut solver = TaffySolver::with_typesetter(ts);
        arena.open().commit_with(&mut solver);
    }
    let text = find_text(&arena, TEXT);
    (arena, text)
}

/// Shapes `text` and places every glyph in absolute document space by adding the
/// node's resolved box origin (the painter moves nothing, P2).
fn text_run(ts: &mut Typesetter, atlas: AtlasIndex, origin: (f32, f32)) -> GlyphRun {
    let laid = ts.layout(TEXT, TEXT_SIZE, None);
    let glyphs = laid
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .map(|g| GlyphQuad {
            glyph_id: g.glyph_id,
            x: origin.0 + g.x,
            y: origin.1 + g.y,
        })
        .collect();
    GlyphRun {
        atlas,
        size: TEXT_SIZE,
        color: INK,
        glyphs,
    }
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

    let mut glyphs = GlyphRunTable::new();
    let atlas = glyphs.push_atlas(load_atlas(ATLAS_DIR));
    glyphs.push_run(text_run(&mut ts, atlas, origin_of(&arena, text)));

    let png = render(&arena, &glyphs);
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
