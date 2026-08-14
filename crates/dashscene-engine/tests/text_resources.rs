//! `TextResources::from_faces` — the assembly the C ABI marshals into
//! (story #947).
//!
//! The property under test is the one that fails **silently** everywhere
//! else: `TextResources::atlases` is indexed by the slot of the face that
//! shaped a glyph, so a list in the wrong order samples the wrong face
//! rather than failing.

use dashscene_engine::{AtlasBytes, FaceBytes, TextResources, TextResourcesError};
use dashscene_typeset::atlas::AtlasMetrics;
use dashscene_typeset::text::TextShape;

const INTER_REGULAR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Regular.otf"
);
const INTER_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-SemiBold.otf"
);
const ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
const ATLAS_REGULAR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii"
);
const ATLAS_SEMIBOLD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii-semibold"
);
const ATLAS_ARABIC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("corpus file {path}: {error}"))
}

fn sheet(dir: &str) -> AtlasBytes {
    AtlasBytes {
        png: read(&format!("{dir}/atlas.png")),
        metrics: read(&format!("{dir}/atlas.metrics")),
    }
}

fn face(family: &str, weight: u16, font: &str, atlas: Option<&str>) -> FaceBytes {
    FaceBytes {
        family: family.to_string(),
        weight,
        font: read(font),
        face_index: 0,
        atlas: atlas.map(sheet),
    }
}

/// The pairing survives a caller that lists one family's faces
/// **non-contiguously**, which is the case a per-face atlas is for.
///
/// `Typesetter::with_named_font_families` flattens family-major over the
/// order it is given, so the slot order here is Inter 400, Inter 600, Noto
/// — not the argument order. An implementation that collected atlases in
/// argument order would put the Arabic sheet at slot 1, and every SemiBold
/// glyph would sample it: Arabic letterforms for Latin text, with nothing
/// failing.
#[test]
fn a_faces_atlas_follows_it_through_the_family_major_flatten() {
    let resources = TextResources::from_faces(vec![
        face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
        face("Noto Sans Arabic", 400, ARABIC, Some(ATLAS_ARABIC)),
        face("Inter", 600, INTER_SEMIBOLD, Some(ATLAS_SEMIBOLD)),
    ])
    .expect("the corpus faces and their committed sheets assemble");

    assert_eq!(
        resources.typesetter.fonts().len(),
        3,
        "three faces flatten to three slots"
    );
    assert_eq!(
        resources.typesetter.weights(),
        [400, 600, 400],
        "family-major: Inter's two faces take slots 0 and 1, so the argument order \
         does not survive the flatten"
    );
    assert_eq!(resources.atlases.len(), 3, "one sheet per slot");

    // Compared by the sheet's own bytes. `Atlas::image` is a public field
    // holding the PNG it was built from, so each slot is matched against the
    // file that belongs at it — a real pairing assertion, not a length check.
    let carried: Vec<&[u8]> = resources
        .atlases
        .iter()
        .map(|atlas| atlas.image.bytes.as_slice())
        .collect();
    let expected: Vec<Vec<u8>> = [ATLAS_REGULAR, ATLAS_SEMIBOLD, ATLAS_ARABIC]
        .iter()
        .map(|dir| read(&format!("{dir}/atlas.png")))
        .collect();
    assert_eq!(
        carried,
        expected.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        "each slot carries the sheet of the face that occupies it, in flatten order — \
         the Arabic sheet is at slot 2 though it was argument 1"
    );
}

/// Empty is the measure-only cascade and stays legal — it is what
/// `TextResources::new` already allows, and it is not the same mistake as a
/// short list.
#[test]
fn no_atlases_at_all_is_the_measure_only_cascade() {
    let resources = TextResources::from_faces(vec![face("Inter", 400, INTER_REGULAR, None)])
        .expect("a cascade with no sheets assembles");
    assert!(resources.atlases.is_empty());
}

/// A short list resolves an index past its end and a reordered one samples
/// the wrong face. Neither fails on its own, so the set is rejected here.
#[test]
fn a_mixed_set_is_rejected_rather_than_truncated() {
    let error = TextResources::from_faces(vec![
        face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
        face("Inter", 600, INTER_SEMIBOLD, None),
    ])
    .expect_err("some faces carrying a sheet and some not is not representable");
    assert!(matches!(error, TextResourcesError::MixedAtlases));
}

#[test]
fn an_empty_face_list_is_named_rather_than_asserted_on() {
    assert!(matches!(
        TextResources::from_faces(Vec::new()),
        Err(TextResourcesError::NoFaces)
    ));
}

/// An empty family name is rejected because no document could ever ask for
/// it.
///
/// `FontFamily::name_matches` trims both sides and returns false when either
/// is empty, so no `TextStyle::family` selects such a family. Whitespace is
/// the same case, because that function trims first.
///
/// **Unrequestable is not unreachable.** `Typesetter::probe_order` builds
/// `(0..families.len())` and only *promotes* a matched family to the head, so
/// every family stays in the cascade and shapes whatever the ones ahead of it
/// do not cover — which is what `FontFamily::unnamed` families are, the whole
/// pre-#385 cascade shape. Such a face would draw, as an unlabelled coverage
/// fallback at whatever position the caller listed it. A host declaring a
/// cascade is naming its families, so a face it can never name back is a
/// mistake in the descriptor rather than a fallback it asked for.
///
/// It is **not** rejected to avoid a panic.
/// `Typesetter::with_named_font_families` asserts on a family whose *faces*
/// are empty and never inspects the name.
#[test]
fn an_unselectable_family_name_is_named_rather_than_silently_kept() {
    for name in ["", "   "] {
        assert!(
            matches!(
                TextResources::from_faces(vec![face(name, 400, INTER_REGULAR, None)]),
                Err(TextResourcesError::EmptyFamily { index: 0 })
            ),
            "a family named {name:?} can never be matched, so it is refused"
        );
    }
}

#[test]
fn bytes_that_are_not_a_face_are_named_with_their_index() {
    let error = TextResources::from_faces(vec![
        face("Inter", 400, INTER_REGULAR, None),
        FaceBytes {
            family: "Junk".to_string(),
            weight: 400,
            font: vec![0; 64],
            face_index: 0,
            atlas: None,
        },
    ])
    .expect_err("junk is not a parseable face");
    assert!(matches!(error, TextResourcesError::Font { index: 1, .. }));
}

#[test]
fn metrics_that_do_not_decode_are_named_with_their_index() {
    let error = TextResources::from_faces(vec![FaceBytes {
        family: "Inter".to_string(),
        weight: 400,
        font: read(INTER_REGULAR),
        face_index: 0,
        atlas: Some(AtlasBytes {
            png: read(&format!("{ATLAS_REGULAR}/atlas.png")),
            metrics: vec![0xff; 32],
        }),
    }])
    .expect_err("junk is not a postcard AtlasMetrics");
    assert!(matches!(error, TextResourcesError::Atlas { index: 0, .. }));
}

/// Two spellings of one family become **one** family, because grouping uses
/// the predicate the typesetter selects with.
///
/// `Typesetter::probe_order` promotes only the *first* family whose name
/// matches and leaves every other one behind it in cascade order. Grouped by
/// string equality, "Inter" and " inter " are two families: a request for
/// Inter at 600 resolves inside the first, which holds only the 400 face, and
/// bold renders regular — recorded as a `WeightSubstitution`, which no ABI
/// entry point exposes, so it is silent from C.
///
/// The three faces are listed non-contiguously so the atlas pairing is a real
/// assertion here too: grouped by string equality the slots would be Inter
/// 400, Noto, inter 600, and the sheets would follow that order instead.
#[test]
fn one_family_spelled_two_ways_is_one_family_with_both_weights() {
    let resources = TextResources::from_faces(vec![
        face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
        face("Noto Sans Arabic", 400, ARABIC, Some(ATLAS_ARABIC)),
        face(" inter ", 600, INTER_SEMIBOLD, Some(ATLAS_SEMIBOLD)),
    ])
    .expect("the corpus faces and their committed sheets assemble");

    assert_eq!(
        resources.typesetter.family_names(),
        ["Inter".to_string(), "Noto Sans Arabic".to_string()],
        "two families, and the first spelling to appear names the merged one"
    );
    assert_eq!(
        resources.typesetter.weights(),
        [400, 600, 400],
        "family-major over two families: Inter's two faces take slots 0 and 1"
    );

    let carried: Vec<&[u8]> = resources
        .atlases
        .iter()
        .map(|atlas| atlas.image.bytes.as_slice())
        .collect();
    let expected: Vec<Vec<u8>> = [ATLAS_REGULAR, ATLAS_SEMIBOLD, ATLAS_ARABIC]
        .iter()
        .map(|dir| read(&format!("{dir}/atlas.png")))
        .collect();
    assert_eq!(
        carried,
        expected.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        "the pairing survives the merge: the SemiBold sheet is at slot 1, where the \
         merged family put its 600 face"
    );

    // The reason the merge matters, asserted on the shaped result rather than
    // on the cascade's shape.
    let mut typesetter = resources.typesetter;
    let layout = typesetter.layout_styled("Ab", 16.0, None, TextShape::default(), 600, "Inter");
    let slots: Vec<u16> = layout
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter().map(|glyph| glyph.font))
        .collect();
    assert!(!slots.is_empty(), "the request shaped glyphs");
    assert!(
        slots.iter().all(|&slot| slot == 1),
        "Inter at 600 shapes with the SemiBold face at slot 1, not the Regular at \
         slot 0: {slots:?}"
    );
    assert!(
        typesetter.weight_substitutions().is_empty(),
        "nothing was substituted, because the requested weight is in the family \
         that was probed: {:?}",
        typesetter.weight_substitutions()
    );
}

/// A host's sheet reaches the painter without passing `dashc`'s
/// image-identity gate, so its **header** is read at load through
/// `dashpaint::image_id::identify` — the same reader every other writer in
/// this workspace uses, which checks the chunk type is `IHDR` and its length
/// is 13 rather than trusting two fixed offsets.
///
/// **What that buys is the header and the extent, and no more.**
/// `dashscene_gpu`'s `decode_png` calls `next_frame(..).expect(..)`, so a
/// correctly-headed PNG with a truncated or CRC-corrupt `IDAT` still passes
/// everything here and still panics at the first draw — caught at the C
/// boundary and reported as `DsStatus::Panic`. Closing that would mean
/// decoding the whole sheet at load, which is a separate decision.
///
/// The good path is covered by every other test in this file, which all build
/// from the committed corpus sheets.
#[test]
fn a_sheet_that_is_not_the_png_its_metrics_describe_is_rejected() {
    let metrics = read(&format!("{ATLAS_REGULAR}/atlas.metrics"));
    let png = read(&format!("{ATLAS_REGULAR}/atlas.png"));

    let with_png = |png: Vec<u8>| {
        TextResources::from_faces(vec![FaceBytes {
            family: "Inter".to_string(),
            weight: 400,
            font: read(INTER_REGULAR),
            face_index: 0,
            atlas: Some(AtlasBytes {
                png,
                metrics: metrics.clone(),
            }),
        }])
    };
    let refused = |result: Result<TextResources, TextResourcesError>, why: &str| -> String {
        let error = result.expect_err(why);
        let TextResourcesError::Atlas { index: 0, message } = error else {
            panic!("expected Atlas at index 0, got {error:?}");
        };
        message
    };

    // Truncated inside the IHDR, before the extent.
    let message = refused(with_png(png[..16].to_vec()), "a truncated sheet is refused");
    assert!(message.contains("truncated"), "{message}");

    // Right length, wrong file.
    let message = refused(
        with_png(vec![0_u8; 64]),
        "bytes that are not an image are refused",
    );
    assert!(message.contains("signature"), "{message}");

    // A re-muxed PNG whose first chunk is not IHDR. This is what a signature
    // test plus two fixed offsets misses: the bytes at 16..24 are then some
    // other chunk's payload, read as an extent.
    let mut not_ihdr_first = png.clone();
    not_ihdr_first[12..16].copy_from_slice(b"tEXt");
    let message = refused(
        with_png(not_ihdr_first),
        "a first chunk that is not IHDR is refused",
    );
    assert!(message.contains("IHDR"), "{message}");

    // A real PNG whose extent disagrees with the metrics — the silent case:
    // the decode's extent builds the texel payload while the metrics extent
    // normalises the glyph quads, so this samples the wrong texels.
    let mut wrong_extent = png.clone();
    let declared = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    wrong_extent[16..20].copy_from_slice(&(declared + 1).to_be_bytes());
    let message = refused(
        with_png(wrong_extent),
        "an extent the metrics do not declare is refused",
    );
    assert!(message.contains("metrics declare"), "{message}");
}

/// A glyph described by exactly one of its two quads is **refused**, not
/// dropped.
///
/// `AtlasMetrics::from_bytes` does not check the pair agrees, and these are
/// host bytes that passed no `dashc` gate. Dropping such a glyph would leave
/// `Atlas::glyph`'s binary search missing it, so the character does not paint
/// — with the load reporting success and nothing naming the glyph. Both
/// absent stays a drop: that is an empty outline, which the space is.
#[test]
fn a_glyph_described_by_one_quad_and_not_the_other_is_refused() {
    let png = read(&format!("{ATLAS_REGULAR}/atlas.png"));
    let raw = read(&format!("{ATLAS_REGULAR}/atlas.metrics"));

    let with_metrics = |metrics: Vec<u8>| {
        TextResources::from_faces(vec![FaceBytes {
            family: "Inter".to_string(),
            weight: 400,
            font: read(INTER_REGULAR),
            face_index: 0,
            atlas: Some(AtlasBytes {
                png: png.clone(),
                metrics,
            }),
        }])
    };

    // The committed sheet already carries a both-absent glyph — the space —
    // and every other test here builds from it, so the drop is covered.
    let decoded = AtlasMetrics::from_bytes(&raw).expect("the committed metrics decode");
    assert!(
        decoded
            .glyphs
            .iter()
            .any(|glyph| glyph.plane_em.is_none() && glyph.atlas_px.is_none()),
        "the committed sheet carries at least one empty-outline glyph, which is what \
         makes the both-absent drop a real path"
    );

    for drop_plane in [true, false] {
        let mut metrics = decoded.clone();
        let victim = metrics
            .glyphs
            .iter_mut()
            .find(|glyph| glyph.plane_em.is_some() && glyph.atlas_px.is_some())
            .expect("the committed sheet carries a drawn glyph");
        let id = victim.glyph_id;
        if drop_plane {
            victim.plane_em = None;
        } else {
            victim.atlas_px = None;
        }

        let error =
            with_metrics(metrics.to_bytes()).expect_err("a half-described glyph is refused");
        let TextResourcesError::Atlas { index: 0, message } = error else {
            panic!("expected Atlas at index 0, got {error:?}");
        };
        assert!(
            message.contains(&format!("glyph {id}")),
            "the message names the glyph it refused: {message}"
        );
    }
}

/// Every variant reads as a sentence, because `dashscene-ffi` puts this
/// string straight into `ds_last_error_message` where every other message is
/// prose.
#[test]
fn every_error_displays_as_prose_rather_than_debug() {
    let cases = [
        TextResources::from_faces(Vec::new()),
        TextResources::from_faces(vec![face("", 400, INTER_REGULAR, None)]),
        TextResources::from_faces(vec![FaceBytes {
            family: "Junk".to_string(),
            weight: 400,
            font: vec![0; 64],
            face_index: 0,
            atlas: None,
        }]),
        TextResources::from_faces(vec![FaceBytes {
            family: "Inter".to_string(),
            weight: 400,
            font: read(INTER_REGULAR),
            face_index: 0,
            atlas: Some(AtlasBytes {
                png: read(&format!("{ATLAS_REGULAR}/atlas.png")),
                metrics: vec![0xff; 32],
            }),
        }]),
        TextResources::from_faces(vec![
            face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
            face("Inter", 600, INTER_SEMIBOLD, None),
        ]),
    ];
    for case in cases {
        let error = case.expect_err("each case is a refusal");
        let shown = error.to_string();
        assert!(
            !shown.contains('{') && !shown.contains('\\'),
            "a struct literal and an escaped quote are what `{{:?}}` produces: {shown}"
        );
        assert!(
            shown
                .chars()
                .next()
                .is_some_and(|first| first.is_lowercase()),
            "the message opens as a sentence fragment, not as a variant name: {shown}"
        );
    }
}
