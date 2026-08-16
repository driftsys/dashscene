//! The text-carrying document the per-frame band's glyph term is stated over
//! (issue #1015).
//!
//! [`super::many_root`] beside this module is the fixture the band's other three
//! terms use, and it carries **no text at all** — its roots are leaves with an
//! image fill. That is deliberate there: the solve count and the rect-row count
//! are about root count, and a typesetter's per-frame work in either of them
//! would be a different quantity. The consequence is that zero glyph quads pass
//! through `GlyphRunTable::push_run` in the only per-frame band there was, so a
//! change that made the glyph-run path materially more expensive per frame left
//! every tier green. PR #1005 added a per-quad pass to that method, on the
//! commit path, with no instrument to weigh it against.
//!
//! This is that instrument's fixture, and it is a second document rather than
//! text added to the first one: `startup_scaling.rs` is stated over the same
//! bytes `many_root` produces, so text there would move its recorded figures
//! too.
//!
//! # Why it is one root and a variable line count
//!
//! The quantity being weighed is per-**quad** work on the commit path, so the
//! term has to move with the number of quads and with nothing else. One root
//! holds the solve count and the rect-row count still while the glyph count
//! varies, which is what makes
//! `per_frame_scaling.rs`'s sensitivity guard a statement about the glyph term
//! alone.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use dashc_wasm::{Box2D, Document, Node, TextAlign, TextAlignV, TextStyle, compile};
use dashpaint::Color;
use dashscene_engine::TextResources;
use dashscene_typeset::text::{Font, FontFamily, Typesetter, WeightedFont};

/// The Noto Sans Regular face, and the committed atlas generated from it over
/// printable ASCII (`corpus/atlas/README.md`).
///
/// Both come from `goldens::render` rather than being re-declared here. The
/// pairing is what makes a shaped glyph id resolve to a real row — a mismatched
/// pair stages runs whose quads are dropped, which reads as a smaller number
/// rather than as a failure — and that crate is where the two lists are already
/// kept in step.
use goldens::render::{ATLAS_ASCII_DIR, FONT_LATIN};

const FAMILY: &str = "Noto Sans";
const SIZE: f32 = 24.0;

/// The line every text node carries.
///
/// Printable ASCII, so every character has a row in the committed atlas, and
/// **no space**: a space has an empty outline and therefore no atlas row, so a
/// line with spaces would stage fewer quads than it has characters and the
/// quad-per-line arithmetic below would stop being obvious.
pub const LINE: &str = "Frame";

/// Glyph quads one [`LINE`] stages.
///
/// A literal rather than `LINE.len()`, so that it and the line are two
/// statements that can disagree. Derived from the line, every assertion over it
/// would be a tautology: changing [`LINE`] to one carrying a space would move
/// this constant with it and the band would keep passing while measuring
/// something else. `per_frame_scaling.rs`'s
/// `the_fixtures_line_stages_one_quad_per_character` is what ties the two
/// together, and it is the assertion that fails on such an edit — it lives
/// there rather than here because a `#[test]` in this directory is compiled and
/// run by all eighteen binaries that declare `mod common;`.
pub const QUADS_PER_LINE: usize = 5;

/// The typesetter and atlas a solver needs to stage drawable runs from this
/// document.
///
/// A `TaffySolver::new()` measures a text node as an empty leaf and stages
/// nothing, so a band built on one would read zero quads over this fixture and
/// pin a term that cannot move.
///
/// The two expensive inputs are read and built once per process, for the reason
/// [`document`] is memoised (issue #930): under `cargo test` — the shape CI's
/// per-frame scaling step runs — every test in a binary shares one process, and
/// both callers of this would otherwise re-read and re-parse the font file and
/// rebuild the atlas's glyph vector. The `Typesetter` itself is rebuilt per
/// call, because it owns a shaped-run cache a caller lends to a solver and two
/// solvers must not share one.
pub fn text_resources() -> TextResources {
    static FONT_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    static ATLASES: OnceLock<Arc<Vec<dashpaint::Atlas>>> = OnceLock::new();

    let bytes = FONT_BYTES
        .get_or_init(|| std::fs::read(FONT_LATIN).expect("the corpus Latin font is present"));
    let font = Font::from_bytes(bytes.clone(), 0).expect("the corpus Latin font parses");
    let typesetter = Typesetter::with_named_font_families(vec![FontFamily::new(
        FAMILY,
        vec![WeightedFont::new(font, 400)],
    )]);
    let atlases = ATLASES.get_or_init(|| Arc::new(vec![super::load_atlas(ATLAS_ASCII_DIR)]));
    TextResources::new(typesetter, Arc::clone(atlases))
}

/// A one-root document carrying `lines` text nodes, each holding [`LINE`].
///
/// Memoised per line count through [`super::memoised`], for the reason
/// [`super::many_root::document`] records: the compile is the expensive part and
/// several tests ask for the same document.
pub fn document(lines: usize) -> Vec<u8> {
    static BUILT: Mutex<BTreeMap<usize, Arc<OnceLock<Vec<u8>>>>> = Mutex::new(BTreeMap::new());
    super::memoised(&BUILT, lines, || build(lines))
}

fn build(lines: usize) -> Vec<u8> {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("shown-root".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 40.0 * lines as f32 + 40.0,
        },
        ..Node::default()
    });
    for index in 0..lines {
        doc.push(Node {
            name: Some(format!("line-{index}")),
            parent: Some(0),
            box2d: Box2D {
                x: 20.0,
                y: 20.0 + 40.0 * index as f32,
                width: 360.0,
                height: 32.0,
            },
            text: Some(LINE.to_owned()),
            text_style: Some(TextStyle {
                family: FAMILY.to_owned(),
                size: SIZE,
                weight: 400,
                color: Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                line_height_px: None,
                letter_spacing: 0.0,
                text_align: TextAlign::Left,
                text_align_v: TextAlignV::Top,
                ligatures_off: false,
            }),
            ..Node::default()
        });
    }
    compile(&doc).expect("the generated text document compiles")
}
