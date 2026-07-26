//! The v0.7 ellipse-lowering golden (story #239): a captured Figma screen
//! carrying five full `ELLIPSE` nodes, lowered by `dashc` and driven to
//! pixels —
//!
//!     Figma REST JSON → dashc lower + emit → .dsb
//!         → dashscene-core load → engine solve → Skia painter → PNG
//!
//! It is the visual proof that a full ellipse lowers to a circle: each becomes
//! a rounded rect with corner radius = half its extent
//! (`docs/decisions/figma-ellipse-as-circle.md`), which the reference painter
//! draws as a true circle (a `56 × 56` box at radius `28` has no straight edge
//! left).
//!
//! The captured `lowering-negative-gap` root hugs its width, which Taffy 0.12
//! collapses over the lowered negative margins (engine debt #236); left raw,
//! that collapse would clip four of the five circles. So the golden lifts the
//! root's `HUG` width to `FIXED` — a declared derivation that defeats only the
//! engine bug and reproduces the picture Figma rendered
//! (`goldens/dsb/README.md`); the ellipses lower unchanged.
//!
//! Determinism (`docs/decisions/golden-comparison-space.md`): lowering,
//! loading, and solving are deterministic, and the render is solid-fill
//! circles. Only each curved edge is anti-aliased, and skia's coverage
//! rounding at a fractional edge is not bit-identical across architectures, so
//! — like the gradient and text goldens — the comparison is a calibrated
//! absolute-pixel budget, not bit-exact.
//!
//! Regeneration and diff workflow: `goldens/README.md`.

use std::collections::BTreeMap;

use dashbuf::root_as_document;
use dashc_wasm::compile_figma;
use dashpaint::{GlyphRunTable, ImageTable, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_validator::Profile;

mod common;
use common::{decode_golden, decode_rgba, diff_vs};

const NEGATIVE_GAP: &str =
    include_str!("../../../corpus/figma-fixtures/lowering-negative-gap.json");

/// The golden's absolute-pixel budget, calibrated for this scene. The healthy
/// render matches the committed golden exactly (0 px, same-machine
/// determinism); the budget's headroom is for cross-machine skia coverage
/// jitter along the five anti-aliased circle edges — comparable to the ~2%
/// canvas fraction the v0.3 gradient/curve goldens use (~633 px on this
/// 264×120 canvas, `goldens/README.md`). Retyping the ellipses to
/// sharp-cornered frames — the regression this golden exists to catch —
/// differs from the golden by 3,193 px
/// (`squaring_the_circles_exceeds_the_budget`), so 500 px (< 0.16× that
/// signal) sits well below the corner difference while clearing the edge
/// jitter: squaring the circles fails by a 2,693 px margin.
const BUDGET: usize = 500;

/// `lowering-negative-gap.json` with the root's `HUG` width lifted to `FIXED`,
/// so the engine-debt-#236 collapse does not clip the circles. The five
/// `ELLIPSE`s are untouched — they lower as circles.
fn circles() -> String {
    derive(&mut |object| {
        if is_root(object) {
            object.insert("layoutSizingHorizontal".to_string(), "FIXED".into());
        }
    })
}

/// The same scene with the five `ELLIPSE`s retyped to `FRAME`s: sharp-cornered
/// squares of the same size and fill. The sensitivity guard's negative image —
/// everything but the corner rounding is identical.
fn squares() -> String {
    derive(&mut |object| {
        if is_root(object) {
            object.insert("layoutSizingHorizontal".to_string(), "FIXED".into());
        }
        if object.get("type").and_then(|t| t.as_str()) == Some("ELLIPSE") {
            object.insert("type".to_string(), "FRAME".into());
        }
    })
}

/// The captured fixture with `patch` applied to every node object.
fn derive(patch: &mut impl FnMut(&mut serde_json::Map<String, serde_json::Value>)) -> String {
    fn walk(
        value: &mut serde_json::Value,
        patch: &mut impl FnMut(&mut serde_json::Map<String, serde_json::Value>),
    ) {
        if let Some(object) = value.as_object_mut() {
            patch(object);
            if let Some(children) = object.get_mut("children").and_then(|c| c.as_array_mut()) {
                for child in children {
                    walk(child, patch);
                }
            }
        }
    }

    let mut file: serde_json::Value =
        serde_json::from_str(NEGATIVE_GAP).expect("the capture parses");
    walk(&mut file["document"], patch);
    file.to_string()
}

fn is_root(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    object.get("type").and_then(|t| t.as_str()) == Some("FRAME")
        && object.get("name").and_then(|n| n.as_str()) == Some("lowering-negative-gap")
}

/// Compiles `json`, loads and solves it through the engine, and renders the
/// committed scene into a PNG at the root's solved size.
fn render(json: &str) -> Vec<u8> {
    let (bytes, report) =
        compile_figma(json, Profile::Core, &BTreeMap::new()).expect("the derived fixture compiles");
    assert!(
        report.is_empty(),
        "the derived fixture lowers clean: {report}"
    );
    let document = root_as_document(dashbuf::container::ui_document(&bytes).expect("a .dsb file"))
        .expect("a valid buffer");

    let mut arena = Arena::new();
    load_document(&document, &mut arena);
    // `load_document` commits with the fixed solver; an empty transaction
    // re-committed through a fresh `TaffySolver` performs a full flex solve so
    // the negative margins place the circles.
    arena.open().commit_with(&mut TaffySolver::new());

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        scene.groups(),
        &GlyphRunTable::new(),
        None,
    );
    painter.png_bytes()
}

#[test]
fn lowered_ellipses_solve_and_paint_to_their_golden() {
    let png = render(&circles());
    goldens::assert_matches_golden_max_pixels("v07-ellipse", &png, BUDGET);
}

/// Sensitivity guard (the #232/#235 lesson): the budget must be tight enough
/// that losing the ellipse-ness fails the compare. The same scene with the
/// five ellipses retyped to frames renders sharp-cornered squares; its diff
/// from the committed golden must exceed the budget, so a regression that
/// dropped the corner rounding cannot slip through the tolerance.
#[test]
fn squaring_the_circles_exceeds_the_budget() {
    let squares = render(&squares());
    let differed = diff_vs(&decode_golden("v07-ellipse"), &decode_rgba(&squares));
    assert!(
        differed > BUDGET,
        "squaring the circles must exceed the {BUDGET}px budget, differed by {differed}",
    );
}
