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

use dashpaint::{Atlas, AtlasGlyph, ImageAsset, ImageFormat, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_engine::TaffySolver;
use dashscene_skia::SkiaPainter;
use dashscene_typeset::atlas::AtlasBundle;
use dashscene_typeset::text::{Font, FontFamily, Typesetter, WeightedFont};

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

/// The atlases the staged runs sample, in the cascade's font-slot order —
/// family-major over Noto Sans, Inter and Noto Sans Arabic, exactly as
/// [`oracle_typesetter`] declares them, so the slot a shaped glyph carries
/// selects the atlas of the face that actually shaped it.
///
/// The order is the contract between this list and the typesetter beside it
/// (`TaffySolver::with_text`): getting it wrong samples the wrong face rather
/// than failing, which is why both are built here, together.
fn cascade_atlases() -> Vec<Atlas> {
    vec![
        load_atlas(ATLAS_ASCII_DIR),
        load_atlas(ATLAS_ASCII_SEMIBOLD_DIR),
        load_atlas(ATLAS_ASCII_BOLD_DIR),
        load_atlas(ATLAS_INTER_ASCII_DIR),
        load_atlas(ATLAS_INTER_ASCII_MEDIUM_DIR),
        load_atlas(ATLAS_INTER_ASCII_SEMIBOLD_DIR),
        load_atlas(ATLAS_INTER_ASCII_BOLD_DIR),
        load_atlas(ATLAS_ARABIC_DIR),
    ]
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
         to render a HiFi or LoFi bank"
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
    // Under RAW every payload passes through untouched. Under HiFi or LoFi the
    // resident payload is a block-compressed KTX2 file, which is decoded here —
    // in the loader, before any byte reaches the painter.
    let paintable: Vec<Cow<'_, [u8]>> = payloads.iter().map(|p| paintable_payload(p)).collect();
    let paintable: Vec<&[u8]> = paintable.iter().map(Cow::as_ref).collect();
    let mut arena = Arena::new();
    load_document(&document, &paintable, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit an empty transaction through a typesetter-backed solver
    // so a full solve runs the measure seam — and, since the solver carries the
    // atlas set, so the same commit stages every TEXT node's glyph runs. Measure
    // and paint agree by construction: one typesetter, asked once, at commit.
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_text(&mut ts, cascade_atlases()));

    // P4: a weight the corpus cannot supply exactly is a named diagnostic,
    // never a silent drop. The commit above both measured and staged, so every
    // substitution that actually rendered glyphs is now recorded (story #368).
    // Reported to stderr because this is the render path's only caller-visible
    // surface — the return value is the PNG.
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
        scene.glyphs(),
        None,
    );
    painter.png_bytes()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dashc_wasm::compile_figma;
    use dashpaint::{AtlasIndex, Color, GlyphRun};
    use dashscene_validator::Profile;

    use super::{Arena, TaffySolver, Typesetter, cascade_atlases, oracle_typesetter, render_dsb};

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

    /// A one-text-node scene in a fixed `box_size` box, committed through the
    /// one stager, returning the runs it staged. Everything these two tests
    /// vary is a text-style field, so the scene itself is shared.
    fn staged_runs(
        ts: &mut Typesetter,
        box_size: (f32, f32),
        text: &str,
        style: dashscene_core::TextStyle,
    ) -> Vec<GlyphRun> {
        use dashscene_core::Prop;

        let mut arena = Arena::new();
        {
            let mut txn = arena.open();
            let node = txn.add_node(None, Some("label"));
            txn.set_prop(node, Prop::Width(box_size.0));
            txn.set_prop(node, Prop::Height(box_size.1));
            txn.set_prop(node, Prop::Text(text.to_string()));
            txn.set_prop(node, Prop::TextStyle(style));
            txn.commit_with(&mut TaffySolver::with_text(ts, cascade_atlases()));
        }
        arena.committed().glyphs().runs().to_vec()
    }

    /// The text style the tests vary from: the lowered defaults every
    /// committed fixture authors — auto line height, no tracking, left and
    /// top aligned, ligatures on.
    fn plain_style(size: f32, weight: u16) -> dashscene_core::TextStyle {
        dashscene_core::TextStyle {
            family: String::new(),
            size,
            weight,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            line_height_px: None,
            letter_spacing: 0.0,
            text_align: dashscene_core::TextAlign::Left,
            text_align_v: dashscene_core::TextAlignV::Top,
            ligatures_off: false,
        }
    }

    #[test]
    fn the_stager_shifts_glyphs_for_center_and_vertical_center_alignment() {
        let mut ts = oracle_typesetter();
        // A box much wider and taller than "Hi", so centering has slack.
        let box_size = (400.0, 200.0);

        let left = staged_runs(&mut ts, box_size, "Hi", plain_style(32.0, 400));
        let center = staged_runs(
            &mut ts,
            box_size,
            "Hi",
            dashscene_core::TextStyle {
                text_align: dashscene_core::TextAlign::Center,
                text_align_v: dashscene_core::TextAlignV::Center,
                ..plain_style(32.0, 400)
            },
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

    /// Story #368: the stager stages a weighted run against the atlas of the
    /// face that actually shaped it. The cascade flattens family-major, so
    /// Latin 400/600/700 are slots 0/1/2 and the atlas list `cascade_atlases`
    /// builds is indexed by the slot a shaped glyph carries. Before that story
    /// the walk never read `style.weight`, so all three rows staged from slot 0
    /// and rendered Regular.
    #[test]
    fn the_stager_selects_an_atlas_by_the_nodes_weight() {
        let mut ts = oracle_typesetter();
        let staged = |ts: &mut Typesetter, weight| {
            staged_runs(
                ts,
                (400.0, 100.0),
                "Sphinx of quartz 123",
                plain_style(28.0, weight),
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
        // Weight 500 has no committed Noto Sans face: the CSS Fonts 4 rule
        // resolves it to Regular, and the substitution is reported rather than
        // silent.
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
        let end_x = |ts: &mut Typesetter, weight| {
            staged(ts, weight).last().unwrap().glyphs.last().unwrap().x
        };
        let regular_end = end_x(&mut ts, 400);
        let bold_end = end_x(&mut ts, 700);
        assert!(
            bold_end > regular_end,
            "the bold row must run wider (regular {regular_end}, bold {bold_end})"
        );
    }

    /// Commit stamps every run with the rect index of the text node it was
    /// shaped from, so a painter can resolve the run's clip, its group and its
    /// z position without being told them separately
    /// (`docs/decisions/glyph-runs-cross-boundary-b.md`).
    #[test]
    fn commit_stamps_each_run_with_its_text_nodes_rect_index() {
        use dashscene_core::Prop;

        let mut ts = oracle_typesetter();
        let mut arena = Arena::new();
        let (first, second) = {
            let mut txn = arena.open();
            let root = txn.add_node(None, Some("root"));
            txn.set_prop(root, Prop::Width(400.0));
            txn.set_prop(root, Prop::Height(200.0));

            let spacer = txn.add_node(Some(root), Some("spacer"));
            txn.set_prop(spacer, Prop::Width(10.0));
            txn.set_prop(spacer, Prop::Height(10.0));

            let first = txn.add_node(Some(root), Some("first"));
            txn.set_prop(first, Prop::Width(200.0));
            txn.set_prop(first, Prop::Height(40.0));
            txn.set_prop(first, Prop::Text("one".to_string()));
            txn.set_prop(first, Prop::TextStyle(plain_style(20.0, 400)));

            let second = txn.add_node(Some(root), Some("second"));
            txn.set_prop(second, Prop::Width(200.0));
            txn.set_prop(second, Prop::Height(40.0));
            txn.set_prop(second, Prop::Text("two".to_string()));
            txn.set_prop(second, Prop::TextStyle(plain_style(20.0, 400)));

            txn.commit_with(&mut TaffySolver::with_text(&mut ts, cascade_atlases()));
            (first, second)
        };

        let scene = arena.committed();
        let anchors: Vec<u32> = scene.glyphs().runs().iter().map(|r| r.rect).collect();
        // Rect indices are document DFS order, so the anchors are the two text
        // nodes' own indices — 2 and 3 here, past the root and the spacer.
        let expected = [
            scene.rect_index_of(first).expect("first is committed"),
            scene.rect_index_of(second).expect("second is committed"),
        ];
        assert_eq!(
            anchors, expected,
            "each run is anchored to its own text node"
        );
        assert!(
            anchors.windows(2).all(|w| w[0] <= w[1]),
            "the run table is ordered by anchor: {anchors:?}"
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
