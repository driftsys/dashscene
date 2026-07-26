//! Load a committed `.dsb` and render it through the v0 Skia reference painter —
//! the public render entry point behind `just render` and the `render-dsb`
//! binary (story Sf-1, docs/wip/2026-07-18-render-dsb-design.md).
//!
//! This mirrors the test-only `render_fixture` in `tests/render_oracle.rs` (the
//! E7 design-source oracle) with one deliberate difference: it takes emitted
//! `.dsb` *bytes* directly rather than recompiling a fixture with an empty
//! images map, so an embedded image fill (`dashbuf` `Image { bytes }`) is present
//! in `scene.images()` and paints. The E7 oracle and its helpers are left
//! byte-identical (docs/wip design, E7 safety), so this module carries its own
//! copy of the font/atlas resource loaders rather than moving them out of the
//! live test file.

use std::borrow::Cow;

use dashpaint::{
    Atlas, AtlasGlyph, AtlasIndex, Color, GlyphQuad, GlyphRun, GlyphRunTable, ImageAsset,
    ImageFormat, Painter,
};
use dashscene_core::{Arena, NodeId, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, FontFamily, TextShape, Typesetter, WeightedFont};

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_LATIN_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-SemiBold.ttf"
);
const FONT_LATIN_BOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Bold.ttf"
);
const FONT_INTER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Regular.otf"
);
const FONT_INTER_MEDIUM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Medium.otf"
);
const FONT_INTER_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-SemiBold.otf"
);
const FONT_INTER_BOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Bold.otf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);

/// The committed ASCII glyph-atlas fixture directory (Noto Sans Regular, slot 0).
pub const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
/// The committed SemiBold ASCII atlas (Noto Sans SemiBold, slot 1) — story #368.
pub const ATLAS_ASCII_SEMIBOLD_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/ascii-semibold"
);
/// The committed Bold ASCII atlas (Noto Sans Bold, slot 2) — story #368.
pub const ATLAS_ASCII_BOLD_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii-bold");
/// The committed Inter ASCII atlases, one per weight — story #385.
pub const ATLAS_INTER_ASCII_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii"
);
/// The committed Inter Medium (500) ASCII atlas — story #385.
pub const ATLAS_INTER_ASCII_MEDIUM_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-medium"
);
/// The committed Inter SemiBold (600) ASCII atlas — story #385.
pub const ATLAS_INTER_ASCII_SEMIBOLD_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-semibold"
);
/// The committed Inter Bold (700) ASCII atlas — story #385.
pub const ATLAS_INTER_ASCII_BOLD_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-bold"
);
/// The committed Arabic glyph-atlas fixture directory (Noto Sans Arabic,
/// the last slot).
pub const ATLAS_ARABIC_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

/// The one cascade every TEXT node is measured and staged through: three
/// **named** families — Noto Sans at the committed weights 400/600/700,
/// Inter at 400/500/600/700, and Noto Sans Arabic at 400. Story #368
/// widened this from a flat Regular-only list; story #385 named the
/// families and added Inter, so a document's `TextStyle::family` selects
/// rather than being carried and ignored.
///
/// Families flatten family-major, so the slot list — what a shaped glyph's
/// `font` index selects — is
///
/// ```text
/// [ascii, ascii-semibold, ascii-bold,
///  inter-ascii, inter-ascii-medium, inter-ascii-semibold, inter-ascii-bold,
///  arabic]
/// ```
///
/// and [`render_dsb`] pushes its atlases in exactly that order.
///
/// **Noto Sans stays first**, so a document naming a family this cascade
/// does not carry still resolves exactly where it did before Inter
/// existed. A document naming Inter reaches Inter by name, not by
/// position, so the order is a fallback rule rather than a preference.
///
/// Selection is family, then coverage, then weight. Coverage outranking
/// weight is why an Arabic run at any weight resolves within the Arabic
/// family — it has only a Regular face, so a bold Arabic run renders
/// Regular and reports `text.weight-substituted` rather than falling into
/// a Latin bold face that cannot render it at all. Coverage also outranks
/// the family request: an Arabic run in a document that names Inter shapes
/// in Noto Sans Arabic and reports `text.family-substituted`.
pub fn oracle_typesetter() -> Typesetter {
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
            vec![
                WeightedFont::new(load(FONT_LATIN, "Noto Sans Regular"), 400),
                WeightedFont::new(load(FONT_LATIN_SEMIBOLD, "Noto Sans SemiBold"), 600),
                WeightedFont::new(load(FONT_LATIN_BOLD, "Noto Sans Bold"), 700),
            ],
        ),
        FontFamily::new(
            "Inter",
            vec![
                WeightedFont::new(load(FONT_INTER, "Inter Regular"), 400),
                WeightedFont::new(load(FONT_INTER_MEDIUM, "Inter Medium"), 500),
                WeightedFont::new(load(FONT_INTER_SEMIBOLD, "Inter SemiBold"), 600),
                WeightedFont::new(load(FONT_INTER_BOLD, "Inter Bold"), 700),
            ],
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

/// Converts a committed build-time atlas fixture at `dir` into a boundary-B
/// [`Atlas`]: only glyphs that paint (bounded outlines) carry a quad, so an
/// empty-outline glyph (space) is dropped. A copy of the goldens `common`
/// helper, kept here so the E7 oracle test file stays byte-identical.
pub fn load_atlas(dir: &str) -> Atlas {
    let bundle = AtlasBundle::load_from_dir(std::path::Path::new(dir))
        .unwrap_or_else(|e| panic!("committed atlas fixture at {dir} loads: {e}"));
    let m = &bundle.metrics;
    let glyphs = m
        .glyphs
        .iter()
        .filter_map(|g| {
            Some(AtlasGlyph {
                glyph_id: g.glyph_id,
                plane_em: g.plane_em?,
                atlas_px: g.atlas_px?,
            })
        })
        .collect();
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: bundle.image_png.clone(),
        },
        m.atlas.width,
        m.atlas.height,
        m.atlas.px_per_em,
        m.atlas.distance_range_px,
        glyphs,
    )
}

/// The resolved box of a committed node: origin (x, y) and size (w, h).
fn box_of(arena: &Arena, node: NodeId) -> (f32, f32, f32, f32) {
    let scene = arena.committed();
    let rect = scene.rects()[scene.rect_index_of(node).expect("the node is committed") as usize];
    (rect.x, rect.y, rect.w, rect.h)
}

/// The `TextShape` for a node's text style (story #327, #341): the fixed line
/// height, letter spacing, horizontal alignment, and standard-ligatures-off
/// bit the stager lays the run out under.
fn text_shape(style: &dashscene_core::TextStyle) -> TextShape {
    TextShape {
        line_height_px: style.line_height_px,
        letter_spacing: style.letter_spacing,
        align: match style.text_align {
            dashscene_core::TextAlign::Left => dashscene_typeset::text::TextAlign::Left,
            dashscene_core::TextAlign::Center => dashscene_typeset::text::TextAlign::Center,
            dashscene_core::TextAlign::Right => dashscene_typeset::text::TextAlign::Right,
        },
        ligatures_off: style.ligatures_off,
    }
}

/// The stager's vertical alignment for a node's text style (story #327).
fn vertical_align(align: dashscene_core::TextAlignV) -> crate::VerticalAlign {
    match align {
        dashscene_core::TextAlignV::Top => crate::VerticalAlign::Top,
        dashscene_core::TextAlignV::Center => crate::VerticalAlign::Center,
        dashscene_core::TextAlignV::Bottom => crate::VerticalAlign::Bottom,
    }
}

/// Shapes `text` at `size` and places every glyph in absolute document space
/// (the painter moves nothing, P2: the node's box origin is added here),
/// splitting a new run wherever the cascade switched fonts so each run samples
/// the atlas of its own font. Unlike the E7 oracle's `text_runs` (default axes),
/// this honors the node's lowered text axes (story #327): it lays out under
/// `shape` (fixed line height, letter spacing, horizontal align) within the
/// resolved box width `box_size.0` — so horizontal alignment centers within the
/// box, and the line breaks match the box the engine measured — and offsets the
/// whole block down by the vertical alignment over `box_size.1`.
#[allow(clippy::too_many_arguments)]
fn text_runs(
    ts: &mut Typesetter,
    atlases: &[AtlasIndex],
    origin: (f32, f32),
    box_size: (f32, f32),
    text: &str,
    size: f32,
    color: Color,
    shape: TextShape,
    valign: crate::VerticalAlign,
    weight: u16,
    family: &str,
) -> Vec<GlyphRun> {
    let (box_w, box_h) = box_size;
    let laid = ts.layout_styled(text, size, Some(box_w), shape, weight, family);
    // Vertical alignment is block placement, not paint (P2) and not a measured
    // extent (P1): shift every glyph down by the box's free space above the block.
    let voff = crate::vertical_offset(box_h, laid.height, valign);
    let mut runs: Vec<GlyphRun> = Vec::new();
    for line in &laid.lines {
        for g in &line.glyphs {
            let atlas = atlases[g.font as usize];
            let quad = GlyphQuad {
                glyph_id: g.glyph_id,
                x: origin.0 + g.x,
                y: origin.1 + voff + g.y,
            };
            match runs.last_mut() {
                Some(run) if run.atlas == atlas => run.glyphs.push(quad),
                _ => runs.push(GlyphRun {
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

/// Walks the committed arena and stages glyph runs for every TEXT node — one or
/// more runs per node, placed within the node's resolved box. A node is a text
/// leaf exactly when it carries both authored characters and a text style; its
/// style's `size`, `color`, and the lowered text axes (line height, letter
/// spacing, horizontal and vertical align) drive the run (story #327). Based on
/// the E7 oracle's `stage_text`, which stays on default axes.
fn stage_text(arena: &Arena, ts: &mut Typesetter, atlases: &[AtlasIndex]) -> Vec<GlyphRun> {
    fn walk(
        arena: &Arena,
        node: NodeId,
        ts: &mut Typesetter,
        atlases: &[AtlasIndex],
        out: &mut Vec<GlyphRun>,
    ) {
        if let (Some(text), Some(style)) = (arena.text(node), arena.text_style(node)) {
            let (x, y, w, h) = box_of(arena, node);
            out.extend(text_runs(
                ts,
                atlases,
                (x, y),
                (w, h),
                text,
                style.size,
                style.color,
                text_shape(style),
                vertical_align(style.text_align_v),
                style.weight,
                &style.family,
            ));
        }
        for &child in arena.children(node) {
            walk(arena, child, ts, atlases, out);
        }
    }
    let mut out = Vec::new();
    for &root in arena.roots() {
        walk(arena, root, ts, atlases, &mut out);
    }
    out
}

/// One resident payload, in a container the reference painter's own codec
/// reads.
///
/// Under RAW that is the canonical payload itself and nothing happens. Under a
/// derived profile the payload is a block-compressed KTX2 file, which no image
/// codec decodes, so it is software-decoded to texels here and re-wrapped as a
/// PNG. The re-wrap is lossless and its only purpose is to hand the painter a
/// container it already accepts, so the painter stays exactly as it is: it
/// draws RGBA, it never measures, wraps, kerns or moves anything, and P2 holds
/// (story #435).
///
/// The block decode is the same version-pinned astcenc that produced the
/// payload — one pinned tool, both directions — and it recovers the block
/// footprint and colour space from the file rather than being told them. The
/// weld test (`goldens/tooling/tests/profile_preview_weld.rs`) holds this whole
/// path byte-equal against the encoder's own reference decode.
#[cfg(feature = "profile-preview")]
fn paintable_payload(payload: &[u8]) -> Cow<'_, [u8]> {
    if !dashpack::preview::is_ktx2(payload) {
        // Borrowed, so RAW does not pay a copy for a capability it never uses.
        return Cow::Borrowed(payload);
    }
    let preview = dashpack::preview::decode(payload)
        .unwrap_or_else(|error| panic!("a derived asset payload does not preview: {error}"));
    Cow::Owned(png_wrap(preview.width, preview.height, &preview.rgba))
}

/// The same seam in a build without the profile preview.
///
/// A block payload is refused by name rather than handed to a codec that would
/// reject it with a less useful message — or, worse, than being drawn as
/// whatever a lenient decoder made of it. P4: an out-of-profile construct is a
/// named diagnostic, never a silent drop.
#[cfg(not(feature = "profile-preview"))]
fn paintable_payload(payload: &[u8]) -> Cow<'_, [u8]> {
    // The first twelve bytes of every KTX2 file, from the specification's own
    // table. Spelled out here rather than imported, because this build does not
    // link `dashpack` at all — a refusal that needed the packer present would
    // not exist in the build that needs to make it.
    const KTX2_IDENTIFIER: [u8; 12] = [
        0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
    ];
    assert!(
        !payload.starts_with(&KTX2_IDENTIFIER),
        "this asset payload is a KTX2 block payload, and this build of the goldens harness \
         has no block decoder: rebuild with the `profile-preview` feature (on by default) \
         to render a HiFi or Lite bank"
    );
    Cow::Borrowed(payload)
}

/// Wraps decoded texels as a PNG, in the unpremultiplied RGBA8888 comparison
/// space the whole harness works in
/// (`docs/decisions/golden-comparison-space.md`).
///
/// Lossless, and welded: the weld test's
/// `leg_3_the_png_wrap_hands_the_painter_the_texels_unchanged` decodes the
/// result back with [`png_texels`] and asserts byte equality, so the wrap cannot
/// quietly premultiply, resample or drop alpha.
#[cfg(feature = "profile-preview")]
pub fn png_wrap(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use skia_safe::{AlphaType, ColorType, Data, EncodedImageFormat, ImageInfo, images};

    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let image = images::raster_from_data(&info, Data::new_copy(rgba), width as usize * 4)
        .expect("decoded texels form a raster image");
    image
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("a raster image PNG-encodes")
        .as_bytes()
        .to_vec()
}

/// The inverse of [`png_wrap`]: an encoded image decoded to
/// `((width, height), unpremultiplied RGBA8888 rows)` through the reference
/// painter's own codec.
///
/// Public because the profile-preview tests need exactly this decode and no
/// other. Both arms of a profile diff have to start from one decode of the
/// canonical bytes, and a second decoder anywhere in that path would put a
/// library disagreement into every measurement where it would be
/// indistinguishable from encoder loss.
#[cfg(feature = "profile-preview")]
pub fn png_texels(png_bytes: &[u8]) -> ((u32, u32), Vec<u8>) {
    use skia_safe::{AlphaType, ColorType, Data, ImageInfo, images};

    let image = images::deferred_from_encoded_data(Data::new_copy(png_bytes), None)
        .expect("the bytes decode as an image the reference painter's codec reads");
    let (width, height) = (image.width(), image.height());
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = width as usize * 4;
    let mut pixels = vec![0u8; row_bytes * height as usize];
    assert!(
        image.read_pixels(
            &info,
            &mut pixels,
            row_bytes,
            (0, 0),
            skia_safe::image::CachingHint::Disallow,
        ),
        "the image has a readable header but its pixel data does not decode"
    );
    ((width as u32, height as u32), pixels)
}

/// Loads a committed `.dsb`, re-solves it through the one typesetter-backed
/// `TaffySolver` (so TEXT nodes size to their shaped extent rather than
/// collapsing to 0x0), stages a glyph run for every TEXT node, and renders the
/// committed scene with the Skia reference painter — returning the PNG. The
/// canvas is sized to the root node's solved box (`scene.rects()[0]`).
///
/// Unlike the test-only `render_fixture`, this loads emitted `.dsb` bytes
/// directly, so embedded image-fill bytes the `.dsb` carries are present in
/// `scene.images()` and paint. Font resolution is the committed cascade, which
/// carries Noto Sans, Inter and Noto Sans Arabic by name (story #385); a family
/// it does not provide resolves by coverage and is reported to stderr as
/// `text.family-substituted`.
pub fn render_dsb(dsb: &[u8]) -> Vec<u8> {
    // One call runs the envelope check, the flatbuffer verifier, and the
    // binding that resolves each asset entry's hash to its blob section —
    // through the derivation manifest when the file carries one.
    let (document, payloads) = dashbuf::open(dsb).expect("a valid .dsb file");
    // Under RAW every payload passes through untouched. Under HiFi or Lite the
    // resident payload is a block-compressed KTX2 file, which is decoded here —
    // in the loader, before any byte reaches the painter.
    let paintable: Vec<Cow<'_, [u8]>> = payloads.iter().map(|p| paintable_payload(p)).collect();
    let paintable: Vec<&[u8]> = paintable.iter().map(Cow::as_ref).collect();
    let mut arena = Arena::new();
    load_document(&document, &paintable, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit an empty transaction through a typesetter-backed solver
    // so a full solve runs the measure seam (the pattern the text goldens use).
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_typesetter(&mut ts));

    // Stage glyph runs for every TEXT node. The atlases are pushed in the
    // cascade's slot order — family-major over Noto Sans, Inter and Noto Sans
    // Arabic, see `oracle_typesetter` — so the slot index a shaped glyph
    // carries selects the atlas of the face that actually shaped it.
    let mut glyphs = GlyphRunTable::new();
    let ascii = glyphs.push_atlas(load_atlas(ATLAS_ASCII_DIR));
    let ascii_semibold = glyphs.push_atlas(load_atlas(ATLAS_ASCII_SEMIBOLD_DIR));
    let ascii_bold = glyphs.push_atlas(load_atlas(ATLAS_ASCII_BOLD_DIR));
    let inter = glyphs.push_atlas(load_atlas(ATLAS_INTER_ASCII_DIR));
    let inter_medium = glyphs.push_atlas(load_atlas(ATLAS_INTER_ASCII_MEDIUM_DIR));
    let inter_semibold = glyphs.push_atlas(load_atlas(ATLAS_INTER_ASCII_SEMIBOLD_DIR));
    let inter_bold = glyphs.push_atlas(load_atlas(ATLAS_INTER_ASCII_BOLD_DIR));
    let arabic = glyphs.push_atlas(load_atlas(ATLAS_ARABIC_DIR));
    for run in stage_text(
        &arena,
        &mut ts,
        &[
            ascii,
            ascii_semibold,
            ascii_bold,
            inter,
            inter_medium,
            inter_semibold,
            inter_bold,
            arabic,
        ],
    ) {
        glyphs.push_run(run);
    }

    // P4: a weight the corpus cannot supply exactly is a named diagnostic,
    // never a silent drop. Staging is finished, so every substitution that
    // actually rendered glyphs is now recorded (story #368). Reported to
    // stderr because this is the render path's only caller-visible surface —
    // the return value is the PNG.
    for report in ts.weight_substitutions() {
        eprintln!("warning: {report}");
    }
    // The same P4 accounting for the family axis (story #385): a family the
    // cascade cannot supply is named, never silently swapped.
    for report in ts.family_substitutions() {
        eprintln!("warning: {report}");
    }

    let scene = arena.committed();
    let root = scene.rects()[0];
    let mut painter = SkiaPainter::new(root.w as i32, root.h as i32);
    painter.paint(
        scene.rects(),
        scene.paints(),
        scene.images(),
        scene.clips(),
        scene.groups(),
        &glyphs,
        None,
    );
    painter.png_bytes()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dashc_wasm::compile_figma;
    use dashscene_validator::Profile;

    use super::render_dsb;

    /// A one-page Figma REST document whose root FRAME is `root`.
    fn document_json(root: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "document": {
                "name": "Document",
                "type": "DOCUMENT",
                "children": [{
                    "name": "Page 1",
                    "type": "CANVAS",
                    "children": [root],
                }],
            },
        })
    }

    #[test]
    fn the_stager_shifts_glyphs_for_center_and_vertical_center_alignment() {
        use dashpaint::{AtlasIndex, Color};
        use dashscene_typeset::text::{TextAlign, TextShape};

        use crate::VerticalAlign;

        use super::{oracle_typesetter, text_runs};

        let mut ts = oracle_typesetter();
        // The atlas index only tags a run; it does not affect placement, so a
        // bare index pair is enough (font 0 = Latin, font 1 = Arabic).
        // One index per cascade slot, in the flattened family-major order
        // `oracle_typesetter` declares. This test's text is Latin at weight
        // 400 with no family requested, so it only ever reaches slot 0, but
        // the array has to be as long as the cascade for the slot lookup to
        // be in bounds at all.
        let atlases: Vec<AtlasIndex> = (0..8).map(AtlasIndex).collect();
        let text = "Hi";
        let size = 32.0;
        let black = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let origin = (0.0, 0.0);
        // A box much wider and taller than "Hi", so centering has slack.
        let box_size = (400.0, 200.0);

        let left = text_runs(
            &mut ts,
            &atlases,
            origin,
            box_size,
            text,
            size,
            black,
            TextShape::default(),
            VerticalAlign::Top,
            400,
            "",
        );
        let center = text_runs(
            &mut ts,
            &atlases,
            origin,
            box_size,
            text,
            size,
            black,
            TextShape {
                line_height_px: None,
                letter_spacing: 0.0,
                align: TextAlign::Center,
                ligatures_off: false,
            },
            VerticalAlign::Center,
            400,
            "",
        );

        let left_glyph = left[0].glyphs[0];
        let center_glyph = center[0].glyphs[0];
        assert!(
            center_glyph.x > left_glyph.x,
            "center alignment shifts the first glyph right within the box \
             (left {}, center {})",
            left_glyph.x,
            center_glyph.x
        );
        assert!(
            center_glyph.y > left_glyph.y,
            "vertical centering shifts the block down within the box \
             (left {}, center {})",
            left_glyph.y,
            center_glyph.y
        );
    }

    /// Story #368: the render walk stages a weighted run against the atlas
    /// of the face that actually shaped it. The cascade flattens
    /// family-major, so Latin 400/600/700 are slots 0/1/2 and the atlas list
    /// `[ascii, ascii-semibold, ascii-bold, arabic]` is indexed by the slot
    /// a shaped glyph carries. Before this story the walk never read
    /// `style.weight`, so all three rows staged from slot 0 and rendered
    /// Regular.
    #[test]
    fn the_stager_selects_an_atlas_by_the_nodes_weight() {
        use dashpaint::{AtlasIndex, Color};
        use dashscene_typeset::text::TextShape;

        use crate::VerticalAlign;

        use super::{oracle_typesetter, text_runs};

        let mut ts = oracle_typesetter();
        let atlases = [
            AtlasIndex(0), // ascii (Noto Regular)
            AtlasIndex(1), // ascii-semibold
            AtlasIndex(2), // ascii-bold
            AtlasIndex(3), // inter-ascii
            AtlasIndex(4), // inter-ascii-medium
            AtlasIndex(5), // inter-ascii-semibold
            AtlasIndex(6), // inter-ascii-bold
            AtlasIndex(7), // arabic
        ];
        let black = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let staged = |ts: &mut _, weight| {
            text_runs(
                ts,
                &atlases,
                (0.0, 0.0),
                (400.0, 100.0),
                "Sphinx of quartz 123",
                28.0,
                black,
                TextShape::default(),
                VerticalAlign::Top,
                weight,
                "",
            )
        };
        for (weight, expected) in [(400u16, 0u32), (600, 1), (700, 2)] {
            let runs = staged(&mut ts, weight);
            assert_eq!(runs.len(), 1, "one atlas per row");
            assert_eq!(
                runs[0].atlas,
                AtlasIndex(expected),
                "weight {weight} must stage against atlas {expected}"
            );
        }
        // Weight 500 has no committed face: the CSS Fonts 4 rule resolves it
        // to Regular, and the substitution is reported rather than silent.
        let at_500 = staged(&mut ts, 500);
        assert_eq!(at_500[0].atlas, AtlasIndex(0));
        assert!(
            ts.weight_substitutions()
                .iter()
                .any(|s| (s.requested, s.resolved) == (500, 400)),
            "the 500 -> 400 substitution is reported: {:?}",
            ts.weight_substitutions()
        );
        // The rows are not merely tagged differently — they are placed
        // differently, because the heavier faces advance wider.
        let regular_end = staged(&mut ts, 400)
            .last()
            .unwrap()
            .glyphs
            .last()
            .unwrap()
            .x;
        let bold_end = staged(&mut ts, 700)
            .last()
            .unwrap()
            .glyphs
            .last()
            .unwrap()
            .x;
        assert!(
            bold_end > regular_end,
            "the bold row must run wider (regular {regular_end}, bold {bold_end})"
        );
    }

    #[test]
    fn render_dsb_returns_a_png_of_the_root_box_size() {
        // A single 100x60 frame with one solid fill — no text, no image — is the
        // smallest scene that exercises load -> solve -> paint -> png. It has no
        // fixture on disk: it is compiled in-process into a `.dsb`, exactly the
        // bytes `render_dsb` consumes at runtime.
        let root = serde_json::json!({
            "name": "one-frame",
            "type": "FRAME",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 60.0 },
            "fills": [{ "type": "SOLID", "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        });
        let json = document_json(root).to_string();
        let (dsb, report) = compile_figma(&json, Profile::Core, &BTreeMap::new())
            .expect("the one-frame fixture compiles");
        assert!(
            report.is_empty(),
            "the one-frame fixture lowers clean: {report}"
        );

        let png = render_dsb(&dsb);

        assert!(!png.is_empty(), "render_dsb returns non-empty PNG bytes");
        let data = skia_safe::Data::new_copy(&png);
        let image = skia_safe::images::deferred_from_encoded_data(data, None)
            .expect("the rendered bytes decode as a PNG");
        assert_eq!(
            (image.width(), image.height()),
            (100, 60),
            "the PNG is sized to the root frame's solved box"
        );
    }
}
