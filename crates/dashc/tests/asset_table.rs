//! The content-addressed asset table and its blob sections (story #107).
//!
//! # Why this file exists rather than leaning on the committed fixture
//!
//! `corpus/figma-fixtures/v03-paint.json` is the only fixture with an image
//! fill, and it has exactly one image. A one-image corpus cannot fail a dedup
//! bug, cannot fail a blob-ordering bug, and cannot fail a wrong-index bug —
//! every index in it is 0, so every one of those bugs produces the same bytes
//! as the correct code.
//!
//! That is the shape debt #395 had: a silent paint-entry collapse lost a fill
//! layer on load and survived because the fixture exercising stacked fills had
//! only one stacked node, so the collision never formed. Fifteen oracle frames
//! stayed green throughout, and fixing it moved the live hero by 0.59 pp.
//!
//! So every test here uses **three** references over **two** distinct payloads,
//! and asserts which payload each reference resolves to by content rather than
//! by count. A test that only counted would pass with the mapping inverted.

use std::collections::BTreeMap;

use dashbuf::container::{Container, FLAVOR_ASSET, FLAVOR_UI, SectionKind};
use dashc_wasm::{Asset, AssetKind, Box2D, Document, Node, Paint, compile};
use dashpaint::{ImageFormat, PaintEntry, PaintKind, ScaleMode};
use dashscene_core::{Arena, load_document};

/// The two distinct payloads every document here is built from.
///
/// Real images rather than synthetic bytes, because the load gate now checks
/// that an entry's recorded format and extent agree with the header of the
/// payload it names (story #437, closing debt #416) — and a run of counting
/// bytes has no header to agree with. These are the same
/// independently-dimensioned fixtures `dashpaint::image_id`'s own unit tests
/// use: a 7x5 PNG and a 9x6 JPEG. They differ in container *and* in extent, so
/// a mapping that resolves a fill to the wrong payload is visible in two
/// independent ways rather than merely counted.
const PAYLOAD_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");
const PAYLOAD_JPEG: &[u8] = include_bytes!("fixtures/image_id/sample.jpg");

/// A node whose paint is an image fill naming `asset`.
fn image_node(name: &str, asset: u32) -> Node {
    Node {
        name: Some(name.to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
        },
        paint: Some(Paint {
            entry: PaintEntry {
                fill: Some(PaintKind::Image {
                    image: asset,
                    scale_mode: ScaleMode::Fill,
                    transform: None,
                    tile_scale: 1.0,
                }),
                stroke: None,
                corners: dashpaint::CornerRadii::default(),
                shadows: Vec::new(),
                blurs: Vec::new(),
                shape: None,
                extra_fills: Vec::new(),
            },
            clip: false,
            shape_field: None,
        }),
        ..Node::default()
    }
}

/// Three image fills over two distinct payloads: the first and third share
/// bytes, the second does not.
fn document() -> Document {
    let mut doc = Document::new();
    let first = doc.push_asset(Asset {
        format: ImageFormat::Png,
        kind: AssetKind::Image,
        bytes: PAYLOAD_PNG.to_vec(),
        width: 7,
        height: 5,
    });
    let second = doc.push_asset(Asset {
        format: ImageFormat::Jpeg,
        kind: AssetKind::Image,
        bytes: PAYLOAD_JPEG.to_vec(),
        width: 9,
        height: 6,
    });
    // The same bytes as `first`, arriving as a separate reference — two
    // `imageRef`s resolving to one payload is the ordinary case for a real file
    // that reuses an image.
    let third = doc.push_asset(Asset {
        format: ImageFormat::Png,
        kind: AssetKind::Image,
        bytes: PAYLOAD_PNG.to_vec(),
        width: 7,
        height: 5,
    });

    assert_eq!(
        (first, second, third),
        (0, 1, 0),
        "the third reference must collapse onto the first: content addressing \
         means identical bytes are one asset"
    );

    doc.push(image_node("first", first));
    doc.push(image_node("second", second));
    doc.push(image_node("third", third));
    doc
}

#[test]
fn identical_payloads_collapse_to_one_entry_and_one_blob() {
    let file = compile(&document()).expect("the document validates");
    let container = Container::parse(&file).expect("the file parses");

    // One ui section, then one blob per entry — two, not three.
    assert_eq!(container.len(), 3, "one ui section plus two blobs");
    assert_eq!(container.section(0).kind, SectionKind::Structured as u16);
    assert_eq!(container.section(0).flavor, FLAVOR_UI);
    for index in 1..3 {
        assert_eq!(container.section(index).kind, SectionKind::Blob as u16);
        assert_eq!(container.section(index).flavor, FLAVOR_ASSET);
    }

    let (document, payloads) = dashbuf::open(&file).expect("the file opens");
    let entries = document.assets().expect("the document carries assets");
    assert_eq!(entries.len(), 2, "three references over two payloads");
    assert_eq!(payloads.len(), 2, "one payload bound per entry");

    // Entry order is blob order, and each blob is the payload its entry names.
    assert_eq!(payloads[0], PAYLOAD_PNG);
    assert_eq!(payloads[1], PAYLOAD_JPEG);
}

/// The entry's hash is the section's hash — that identity is what makes the
/// null binding an identity map, and it is the whole reason an entry can name a
/// payload without naming a section index.
///
/// Note what this pins and what it does not. Entry order equalling blob order is
/// **writer policy**, not correctness: `dashbuf::open` resolves by hash, so a
/// file with its blobs in any order loads identically. This test is the only
/// thing forbidding a reordering, which is worth knowing before someone changes
/// blob placement for a packing reason and finds it red. What would be a real
/// defect is an entry whose hash is not the digest of the bytes it names, and the
/// second assertion below is the one that catches that.
#[test]
fn each_entry_hash_is_its_blob_sections_content_hash() {
    let file = compile(&document()).expect("the document validates");
    let container = Container::parse(&file).expect("the file parses");
    let (document, _) = dashbuf::open(&file).expect("the file opens");
    let entries = document.assets().expect("assets present");

    for (index, entry) in entries.iter().enumerate() {
        let section = container.section(index + 1);
        assert_eq!(
            entry.hash().bytes(),
            &section.hash[..],
            "entry {index}'s hash is not its blob section's content hash"
        );
        // And it is genuinely BLAKE3 of the bytes, not merely a value copied
        // into both places.
        assert_eq!(
            entry.hash().bytes(),
            blake3::hash(container.section_bytes(index + 1)).as_bytes(),
            "entry {index}'s hash is not the digest of the payload it names"
        );
    }
}

/// The metadata rides in the hot entry, so it is readable with no payload
/// resident. Each entry carries the extent and format of *its own* payload —
/// asserted per entry, because two entries carrying the first one's metadata
/// would still pass a count.
#[test]
fn each_entry_carries_its_own_intrinsic_metadata() {
    let file = compile(&document()).expect("the document validates");
    let (document, _) = dashbuf::open(&file).expect("the file opens");
    let entries = document.assets().expect("assets present");

    assert_eq!(entries.get(0).format(), dashbuf::ImageFormat::Png);
    assert_eq!((entries.get(0).width(), entries.get(0).height()), (7, 5));
    assert_eq!(entries.get(1).format(), dashbuf::ImageFormat::Jpeg);
    assert_eq!((entries.get(1).width(), entries.get(1).height()), (9, 6));
}

/// The end of the path: each node's fill reaches the arena pointing at the
/// bytes it named. This is the assertion a wrong-index bug fails — the other
/// tests above would all still pass if the loader swapped two payloads.
#[test]
fn every_node_resolves_to_the_payload_it_named() {
    let file = compile(&document()).expect("the document validates");
    let (doc, payloads) = dashbuf::open(&file).expect("the file opens");

    let report = dashscene_validator::validate_document(&doc);
    assert!(!report.has_errors(), "the load gate accepts it: {report}");

    // The load gate's asset half, over the payloads the file bound. A real
    // compile output has to pass it, not only the hand-built documents in the
    // validator's own tests (story #437).
    let payload_report = dashscene_validator::validate_asset_payloads(&doc, &payloads);
    assert!(
        !payload_report.has_errors(),
        "the asset payloads agree with their entries: {payload_report}"
    );

    let mut arena = Arena::new();
    load_document(&doc, &payloads, &mut arena);
    let scene = arena.committed();

    // The three nodes are in DFS order, so rect 0/1/2 are first/second/third.
    let expected = [PAYLOAD_PNG, PAYLOAD_JPEG, PAYLOAD_PNG];
    for (index, want) in expected.iter().enumerate() {
        let paint = scene.paints().resolve(scene.rects()[index].paint);
        let PaintKind::Image { image, .. } = paint
            .fill
            .as_ref()
            .expect("every node in this document has an image fill")
        else {
            panic!("node {index} did not resolve to an image fill");
        };
        let asset = scene.images().resolve(*image);
        assert_eq!(
            asset.bytes.as_slice(),
            *want,
            "node {index} resolved to the wrong payload"
        );
    }
}

/// R7 over the whole file, blob order included. A dedup keyed on an unordered
/// map would emit the same document with the blobs in either order, and only
/// this notices.
#[test]
fn the_same_document_compiles_to_the_same_file_twice() {
    let first = compile(&document()).expect("validates");
    let second = compile(&document()).expect("validates");
    assert_eq!(first, second);
}

/// A document with no assets writes no blob section and gets no page padding —
/// the boundary it does not have costs it nothing. Six of the seven committed
/// goldens are in exactly this shape, which is why only one of them moved when
/// the asset table landed.
#[test]
fn a_document_with_no_assets_carries_one_section() {
    let mut doc = Document::new();
    doc.push(Node {
        name: Some("plain".to_owned()),
        parent: None,
        box2d: Box2D {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
        },
        ..Node::default()
    });
    let file = compile(&doc).expect("validates");

    let container = Container::parse(&file).expect("parses");
    assert_eq!(container.len(), 1, "the ui section and nothing else");
    let (document, payloads) = dashbuf::open(&file).expect("opens");
    assert!(document.assets().is_none_or(|a| a.is_empty()));
    assert!(payloads.is_empty());
}

/// A Figma compile takes the same path: the fixture's one image becomes one
/// entry and one blob, through the gate that parsed its header.
#[test]
fn a_figma_compile_emits_its_image_as_a_blob_section() {
    const FIXTURE: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");
    let images: BTreeMap<String, dashpaint::ImageAsset> = image_map();

    let (file, report) =
        dashc_wasm::compile_figma(FIXTURE, dashscene_validator::Profile::Core, &images)
            .expect("the paint fixture compiles");
    assert!(report.is_empty(), "v03-paint emits clean: {report}");

    let container = Container::parse(&file).expect("parses");
    assert_eq!(container.len(), 2, "the ui section plus one asset blob");
    assert_eq!(container.section(1).kind, SectionKind::Blob as u16);

    let (document, payloads) = dashbuf::open(&file).expect("opens");
    let entries = document.assets().expect("assets present");
    assert_eq!(entries.len(), 1);
    // The extent came from the payload's own PNG header, through story #400's
    // gate — not from anything the caller asserted. Pinned to the fixture's real
    // dimensions, confirmed independently with `magick identify`: a non-zero
    // check would pass with the gate wired to the wrong field, a stale value, or
    // a constant, and nothing else in the repo pins that a recorded extent
    // equals its payload's actual extent (debt #416 is the load-time version of
    // the same check).
    let entry = entries.get(0);
    assert_eq!(entry.format(), dashbuf::ImageFormat::Png);
    assert_eq!(
        (entry.width(), entry.height()),
        (16, 16),
        "the recorded extent is not the fixture PNG's real 16x16"
    );
    assert_eq!(payloads.len(), 1);
    assert_eq!(
        payloads[0],
        &images.values().next().expect("one image").bytes[..]
    );
}

/// The fixture's image bytes, as the importer would supply them.
fn image_map() -> BTreeMap<String, dashpaint::ImageAsset> {
    const IMAGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
    const BYTES: &[u8] = include_bytes!(
        "../../../corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png"
    );
    let mut images = BTreeMap::new();
    images.insert(
        IMAGE_REF.to_owned(),
        dashpaint::ImageAsset {
            format: ImageFormat::Png,
            bytes: BYTES.to_vec(),
        },
    );
    images
}

/// The native `compile` API refuses an asset whose recorded metadata
/// contradicts its bytes.
///
/// Story #400 gated the *Figma* path — `compile_figma` runs `identify` on every
/// image in the `images` map. It did not gate this one: `compile` takes an
/// `Asset`'s `format`, `width`, `height`, and `bytes` straight from the producer
/// and verified none of them, so `dashlang` or any native producer could record
/// anything at all and the file would emit clean. Story #437 closed that by
/// running the load gate's asset half over what was just emitted.
///
/// Without the wiring in `emit_and_validate`, every assertion here fails —
/// which is the point, because deleting it otherwise leaves the suite green.
#[test]
fn compile_refuses_an_asset_whose_metadata_contradicts_its_bytes() {
    // The container is wrong: PNG bytes recorded as a JPEG. A painter
    // dispatches its decoder on the recorded format, so this is the one that
    // hands PNG bytes to a JPEG decoder.
    let mut doc = Document::new();
    let asset = doc.push_asset(Asset {
        format: ImageFormat::Jpeg,
        kind: AssetKind::Image,
        bytes: PAYLOAD_PNG.to_vec(),
        width: 7,
        height: 5,
    });
    doc.push(image_node("mistagged", asset));

    let report = compile(&doc).expect_err("a mistagged asset must not compile");
    assert!(
        report.has(dashscene_validator::rule::ASSET_FORMAT_MISMATCH),
        "expected asset.format-mismatch, got: {report}"
    );
    assert!(report.has_errors(), "it has to block, not merely warn");

    // The extent is wrong: the right container, a lie about its size. Layout
    // runs on the recorded extent before the payload is resident.
    let mut doc = Document::new();
    let asset = doc.push_asset(Asset {
        format: ImageFormat::Png,
        kind: AssetKind::Image,
        bytes: PAYLOAD_PNG.to_vec(),
        width: 7,
        height: 5000,
    });
    doc.push(image_node("wrong-extent", asset));

    let report = compile(&doc).expect_err("a lying extent must not compile");
    assert!(
        report.has(dashscene_validator::rule::ASSET_EXTENT_MISMATCH),
        "expected asset.extent-mismatch, got: {report}"
    );
    assert!(report.has_errors(), "it has to block, not merely warn");
}
