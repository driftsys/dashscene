//! The compile-gate wiring for story #400: every image entering the
//! `images` map is verified against its producer-supplied tag before it
//! becomes a document asset (`crates/dashc/src/image_id.rs`).
//!
//! `crates/dashc/src/image_id.rs`'s own unit tests cover `identify` in
//! isolation — the header math, the truncation robustness. These tests cover
//! the four named diagnostics *at the compile gate*: that a bad image aborts
//! `compile_figma` with the right rule, always as an error, and that the
//! `imageRef` a designer could search for rides in the message.

use std::collections::BTreeMap;

use dashc_wasm::figma::rule;
use dashc_wasm::{CompileError, compile_figma};
use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::{Location, Profile, Severity};

const IMAGE_REF: &str = "test-image";

// The same real, independently-dimensioned fixtures `image_id`'s own unit
// tests use (7x5 PNG, 9x6 JPEG, 11x8 GIF — see that module for how their
// sizes were confirmed).
const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");
const SAMPLE_GIF: &[u8] = include_bytes!("fixtures/image_id/sample.gif");

/// A one-page document whose root `FRAME` carries a single `IMAGE` fill
/// referencing [`IMAGE_REF`] — the shape every test in this file compiles.
fn document_with_image_fill() -> String {
    serde_json::json!({
        "document": {
            "name": "Document",
            "type": "DOCUMENT",
            "children": [{
                "name": "Page 1",
                "type": "CANVAS",
                "children": [{
                    "name": "image-frame",
                    "type": "FRAME",
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 },
                    "fills": [{ "type": "IMAGE", "scaleMode": "FILL", "imageRef": IMAGE_REF }],
                }],
            }],
        },
    })
    .to_string()
}

fn images_with(asset: ImageAsset) -> BTreeMap<String, ImageAsset> {
    BTreeMap::from([(IMAGE_REF.to_string(), asset)])
}

/// Asserts `result` is a `CompileError::Diagnostics` carrying exactly one
/// diagnostic under `rule`, that it is always `Severity::Error` (never
/// downgraded by the emit policy — the whole point of these four bypassing
/// `figma.unsupported`), located at a node, and that its message names
/// [`IMAGE_REF`] — the locator a designer could actually search a Figma file
/// for.
fn assert_sole_image_diagnostic(
    result: Result<(Vec<u8>, dashscene_validator::Report), CompileError>,
    rule: &str,
) {
    let err = result.expect_err("a bad image must refuse the compile");
    let CompileError::Diagnostics(report) = err else {
        panic!("expected diagnostics, got {err:?}");
    };

    let found: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule)
        .collect();
    let [diagnostic] = found[..] else {
        panic!("expected exactly one {rule} diagnostic, got {found:?}");
    };
    assert_eq!(
        diagnostic.severity,
        Severity::Error,
        "an image-identification finding is always an error"
    );
    assert!(
        matches!(diagnostic.at, Location::Node(_)),
        "an image-identification finding is located at a node"
    );
    assert!(
        diagnostic.message.contains(IMAGE_REF),
        "the message names the imageRef a designer can find: {}",
        diagnostic.message
    );
}

#[test]
fn bytes_matching_no_signature_are_refused() {
    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: b"not an image, just some plain bytes of a similar length".to_vec(),
    };
    let result = compile_figma(
        &document_with_image_fill(),
        Profile::Core,
        &images_with(asset),
    );
    assert_sole_image_diagnostic(result, rule::IMAGE_UNKNOWN_SIGNATURE);
}

#[test]
fn a_signature_that_contradicts_the_tag_is_refused() {
    // A real, well-formed GIF, tagged as PNG. identify() reports the bytes'
    // own signature (Gif); the caller's tag disagrees.
    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_GIF.to_vec(),
    };
    let result = compile_figma(
        &document_with_image_fill(),
        Profile::Core,
        &images_with(asset),
    );
    assert_sole_image_diagnostic(result, rule::IMAGE_FORMAT_MISMATCH);
}

#[test]
fn a_header_truncated_mid_parse_is_refused() {
    // The 8-byte PNG signature is intact, but the IHDR chunk length (the
    // next 4 bytes) is cut off after 2 — truncated *inside* the header, not
    // merely a short/empty payload.
    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG[..10].to_vec(),
    };
    let result = compile_figma(
        &document_with_image_fill(),
        Profile::Core,
        &images_with(asset),
    );
    assert_sole_image_diagnostic(result, rule::IMAGE_HEADER_MALFORMED);
}

#[test]
fn a_zero_reported_dimension_is_refused() {
    // A structurally valid PNG header whose IHDR width field is zeroed —
    // identify() parses it cleanly; only the caller's zero-dimension check
    // catches it.
    let mut bytes = SAMPLE_PNG.to_vec();
    assert_ne!(
        &bytes[16..20],
        &[0, 0, 0, 0],
        "the fixture's real width is not already zero"
    );
    bytes[16..20].copy_from_slice(&0u32.to_be_bytes());
    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes,
    };
    let result = compile_figma(
        &document_with_image_fill(),
        Profile::Core,
        &images_with(asset),
    );
    assert_sole_image_diagnostic(result, rule::IMAGE_ZERO_DIMENSION);
}

#[test]
fn a_correctly_tagged_valid_image_still_compiles() {
    // The gate this story adds must not refuse a real, correctly-tagged
    // image — the fixture-driven suite in tests/figma_lowering.rs already
    // pins this for the captured v03-paint.json corpus fixture end to end
    // (byte-reproducible emission, unchanged golden .dsb); this is the same
    // claim against image_id's own local fixture.
    let asset = ImageAsset {
        format: ImageFormat::Png,
        bytes: SAMPLE_PNG.to_vec(),
    };
    let (bytes, report) = compile_figma(
        &document_with_image_fill(),
        Profile::Core,
        &images_with(asset),
    )
    .expect("a correctly-tagged, well-formed image must compile");
    assert!(!bytes.is_empty());
    assert!(
        !report.has_errors(),
        "a valid image must not carry any image-identification diagnostic"
    );
}
