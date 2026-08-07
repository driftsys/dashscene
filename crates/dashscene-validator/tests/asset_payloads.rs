//! The load gate's asset half (story #437, closing debt #416): does the
//! payload an `AssetEntry` names agree with what the entry records about it?
//!
//! An entry says "PNG, 7x5" and names its bytes by content hash; the bytes sit
//! in their own blob section. Two places describe one asset, and until this
//! gate nothing checked that they agree. It stayed latent because `dashc`
//! derived both halves from one header parse — the packer is the second
//! writer, and it re-derives payloads.
//!
//! Every disagreement here is a **named diagnostic**, never a silent pass and
//! never a panic (P4). Each test below names the rule it expects rather than
//! asserting on message text, and the malformed-input tests exist to pin that
//! bad bytes produce a `Report` rather than an unwind.

use dashbuf::{AssetEntry, AssetEntryArgs, Document, DocumentArgs, ImageFormat, root_as_document};
use dashscene_validator::{Location, Severity, rule, validate_asset_payloads, validate_document};
use flatbuffers::FlatBufferBuilder;

// The same real, independently-dimensioned fixtures `dashpaint::image_id`'s
// own unit tests use — 7x5 PNG, 9x6 JPEG, 11x8 GIF. Non-square on both axes
// and different from each other in both container and extent, so a test that
// swapped two of them fails rather than coincidentally passing.
const SAMPLE_PNG: &[u8] = include_bytes!("fixtures/image_id/sample.png");
const SAMPLE_JPEG: &[u8] = include_bytes!("fixtures/image_id/sample.jpg");
const SAMPLE_GIF: &[u8] = include_bytes!("fixtures/image_id/sample.gif");

/// What one `AssetEntry` records. The hash is irrelevant to every rule in this
/// file — a payload is verified against its entry's hash before this gate ever
/// runs, by `dashbuf::open_verified` for a caller that reads a whole file and by
/// `dashbuf::residency::Residency::touch` for one that makes payloads resident,
/// so a payload that does not match its hash never arrives here — which is why
/// these tests hand the payloads in directly.
struct Entry {
    format: ImageFormat,
    width: u32,
    height: u32,
}

/// A document carrying nothing but asset entries. Everything else the load
/// gate reads is absent, which keeps each test to the one rule it is about.
fn document_with(entries: &[Entry]) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();

    let mut offsets = Vec::new();
    for entry in entries {
        let hash = b.create_vector(&[0u8; 32]);
        offsets.push(AssetEntry::create(
            &mut b,
            &AssetEntryArgs {
                hash: Some(hash),
                format: entry.format,
                width: entry.width,
                height: entry.height,
                kind: dashbuf::AssetKind::Image,
            },
        ));
    }
    let assets = b.create_vector(&offsets);

    let doc = Document::create(
        &mut b,
        &DocumentArgs {
            assets: Some(assets),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    b.finished_data().to_vec()
}

fn png_entry() -> Entry {
    Entry {
        format: ImageFormat::Png,
        width: 7,
        height: 5,
    }
}

#[test]
fn an_entry_agreeing_with_its_payload_reports_nothing() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("the builder produces a valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
    assert!(report.is_empty(), "expected no findings, got: {report}");
}

/// All three containers, each entry recording its own real format and extent.
/// One test per format would let a rule that only understands PNG pass.
#[test]
fn every_container_in_the_closure_agrees_with_its_own_payload() {
    let bytes = document_with(&[
        png_entry(),
        Entry {
            format: ImageFormat::Jpeg,
            width: 9,
            height: 6,
        },
        Entry {
            format: ImageFormat::Gif,
            width: 11,
            height: 8,
        },
    ]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG, SAMPLE_JPEG, SAMPLE_GIF]);
    assert!(report.is_empty(), "expected no findings, got: {report}");
}

/// The entry says JPEG, the bytes are a PNG. A painter dispatches its decoder
/// on the recorded format, so this is the disagreement that hands PNG bytes to
/// a JPEG decoder.
#[test]
fn a_recorded_format_contradicting_the_payloads_signature_is_named() {
    let bytes = document_with(&[Entry {
        format: ImageFormat::Jpeg,
        width: 7,
        height: 5,
    }]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
    assert!(
        report.has(rule::ASSET_FORMAT_MISMATCH),
        "expected {}, got: {report}",
        rule::ASSET_FORMAT_MISMATCH
    );
    let diagnostic = report.find(rule::ASSET_FORMAT_MISMATCH).unwrap();
    assert_eq!(
        diagnostic.at,
        Location::ImageAsset(0),
        "the diagnostic points at the entry that carries the wrong format"
    );
    // Severity, not only the rule id. `Report::has` is severity-agnostic, so a
    // gate quietly downgraded to `warning` would still satisfy every
    // rule-id assertion in this file while no longer blocking anything —
    // `compile` would emit the bad document and `dashc check` would exit 0.
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(report.has_errors(), "a lying entry must block the document");
}

/// The container is right and the extent is not. Layout runs on the recorded
/// extent before the payload is resident, so a wrong one reflows the frame
/// when the real size arrives.
#[test]
fn a_recorded_extent_contradicting_the_payloads_header_is_named() {
    let bytes = document_with(&[Entry {
        format: ImageFormat::Png,
        width: 512,
        height: 512,
    }]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
    assert!(
        report.has(rule::ASSET_EXTENT_MISMATCH),
        "expected {}, got: {report}",
        rule::ASSET_EXTENT_MISMATCH
    );
    assert!(
        !report.has(rule::ASSET_FORMAT_MISMATCH),
        "the format agrees; only the extent should be named: {report}"
    );
    assert_eq!(
        report.find(rule::ASSET_EXTENT_MISMATCH).unwrap().severity,
        Severity::Error,
        "a lying extent must block the document, not merely warn"
    );
}

/// Each axis is compared, not just one. The mismatch tests either side of this
/// one differ on *both* axes, so a comparison that had dropped the height term
/// would still pass them; these two isolate each term.
#[test]
fn a_mismatch_on_one_axis_alone_is_named() {
    for (width, height, axis) in [(7, 9, "height"), (3, 5, "width")] {
        let bytes = document_with(&[Entry {
            format: ImageFormat::Png,
            width,
            height,
        }]);
        let doc = root_as_document(&bytes).expect("valid buffer");

        // The fixture is 7x5, so exactly one axis disagrees in each case.
        let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
        assert!(
            report.has(rule::ASSET_EXTENT_MISMATCH),
            "a 7x5 payload recorded as {width}x{height} disagrees on {axis} alone \
             and must be named: {report}"
        );
    }
}

/// A width/height transposition is the mistake a square fixture cannot catch,
/// which is why every fixture here is non-square on both axes.
#[test]
fn a_transposed_extent_is_named() {
    let bytes = document_with(&[Entry {
        format: ImageFormat::Png,
        width: 5,
        height: 7,
    }]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
    assert!(
        report.has(rule::ASSET_EXTENT_MISMATCH),
        "a 7x5 payload recorded as 5x7 must be named: {report}"
    );
}

/// Format and extent are independent facts, and a producer that swapped two
/// assets outright gets both wrong. Reporting only the first would hide half
/// of what went wrong.
#[test]
fn a_wholly_wrong_payload_names_both_the_format_and_the_extent() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    // A 7x5 PNG entry handed the 11x8 GIF payload.
    let report = validate_asset_payloads(&doc, &[SAMPLE_GIF]);
    assert!(report.has(rule::ASSET_FORMAT_MISMATCH), "{report}");
    assert!(report.has(rule::ASSET_EXTENT_MISMATCH), "{report}");
}

/// Bytes matching no container signature at all. The point of naming this at
/// the load gate is that the alternative is discovering it inside a painter's
/// decoder — the component the target-hardware rules keep out of the trusted
/// path.
#[test]
fn a_payload_that_is_not_an_image_is_named_rather_than_passed() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[b"this is plain text, not an image".as_slice()]);
    assert!(
        report.has(rule::ASSET_PAYLOAD_UNREADABLE),
        "expected {}, got: {report}",
        rule::ASSET_PAYLOAD_UNREADABLE
    );
    assert_eq!(
        report
            .find(rule::ASSET_PAYLOAD_UNREADABLE)
            .unwrap()
            .severity,
        Severity::Error,
        "a payload that is not an image must block the document"
    );
}

/// A truncated payload is a malformed header, not a panic. `identify` returns
/// an `Err` for every prefix shorter than a header; this pins that the gate
/// turns each of them into a diagnostic.
#[test]
fn every_truncation_of_a_payload_is_a_diagnostic_never_a_panic() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    // 24 is this fixture's header end (8-byte signature + 4 length + 4 type +
    // 4 width + 4 height), the same constant `image_id`'s own truncation test
    // uses. Below it the header cannot be read at all.
    for cut in 0..24 {
        let report = validate_asset_payloads(&doc, &[&SAMPLE_PNG[..cut]]);
        assert!(
            report.has(rule::ASSET_PAYLOAD_UNREADABLE),
            "a {cut}-byte payload must be named unreadable, not accepted: {report}"
        );
    }
}

/// The decode boundary, stated as a test so it is not mistaken for a gap.
///
/// A payload whose header is intact but whose compressed data is truncated
/// passes this gate: the recorded format and extent are both true, and only a
/// decoder could discover that the pixels are missing. Keeping that discovery
/// in the painter is deliberate — decode is the CVE-bearing part, and
/// `docs/decisions/dashc-identifies-images-never-decodes.md` keeps it out of
/// every crate a producer links.
#[test]
fn a_payload_truncated_after_its_header_passes_because_this_gate_never_decodes() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    // The full 24-byte header, and nothing after it: no image data at all.
    let report = validate_asset_payloads(&doc, &[&SAMPLE_PNG[..24]]);
    assert!(
        report.is_empty(),
        "the header agrees, so this gate accepts it and the painter's decoder \
         is what finds the missing pixels: {report}"
    );
}

/// Fewer payloads than entries: named once, at the first entry that has none,
/// and no index runs past the end of the slice.
#[test]
fn fewer_payloads_than_entries_is_named_once_and_never_panics() {
    let bytes = document_with(&[png_entry(), png_entry(), png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG]);
    assert!(
        report.has(rule::ASSET_PAYLOAD_MISSING),
        "expected {}, got: {report}",
        rule::ASSET_PAYLOAD_MISSING
    );
    assert_eq!(
        report
            .diagnostics()
            .iter()
            .filter(|d| d.rule == rule::ASSET_PAYLOAD_MISSING)
            .count(),
        1,
        "one caller mistake is one diagnostic, not one per unchecked entry: {report}"
    );
    let diagnostic = report.find(rule::ASSET_PAYLOAD_MISSING).unwrap();
    assert_eq!(
        diagnostic.at,
        Location::ImageAsset(1),
        "it points at the first entry that has no payload"
    );
    assert_eq!(diagnostic.severity, Severity::Error);
}

/// No payloads at all — the shape a caller reaches by passing an empty slice.
/// It must diagnose, not panic and not quietly accept.
#[test]
fn no_payloads_at_all_is_named() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[]);
    assert!(report.has(rule::ASSET_PAYLOAD_MISSING), "{report}");
}

/// Surplus payloads describe nothing in the document, so there is no document
/// defect to name. The entries that exist are still checked.
#[test]
fn surplus_payloads_are_ignored_and_the_real_entries_still_check() {
    let bytes = document_with(&[png_entry()]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[SAMPLE_PNG, SAMPLE_GIF, SAMPLE_JPEG]);
    assert!(report.is_empty(), "expected no findings, got: {report}");

    // And the one entry that does exist is genuinely still being checked,
    // rather than the surplus causing the whole loop to be skipped.
    let wrong = document_with(&[Entry {
        format: ImageFormat::Png,
        width: 1,
        height: 1,
    }]);
    let wrong_doc = root_as_document(&wrong).expect("valid buffer");
    let report = validate_asset_payloads(&wrong_doc, &[SAMPLE_PNG, SAMPLE_GIF]);
    assert!(report.has(rule::ASSET_EXTENT_MISMATCH), "{report}");
}

/// A document with no assets has nothing to cross-check.
#[test]
fn a_document_with_no_assets_reports_nothing() {
    let bytes = document_with(&[]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    let report = validate_asset_payloads(&doc, &[]);
    assert!(report.is_empty(), "{report}");
}

/// A format value this build does not know is skipped here rather than judged.
///
/// `dashbuf`'s enums are append-only, so such a document was written by a
/// newer producer, and its payload may be a container this build cannot
/// identify either — calling that "unreadable" would report a stale reader as
/// a broken file. `validate_document` names it as `UNKNOWN_ENUM`, which is the
/// honest diagnosis, and this test pins that the two gates divide the case
/// that way rather than both guessing at it.
#[test]
fn an_unrecognized_recorded_format_is_left_to_the_unknown_enum_rule() {
    let bytes = document_with(&[Entry {
        // Past `Gif = 2` — the next variant an append would add.
        format: ImageFormat(7),
        width: 7,
        height: 5,
    }]);
    let doc = root_as_document(&bytes).expect("valid buffer");

    // The payload deliberately *disagrees* with the recorded extent — an 11x8
    // GIF against a recorded 7x5. A build that judged the unknown format would
    // raise both mismatches here, so `is_empty` genuinely pins the skip. Handing
    // it an agreeing payload would pass whether or not the skip existed.
    let payload_report = validate_asset_payloads(&doc, &[SAMPLE_GIF]);
    assert!(
        payload_report.is_empty(),
        "this gate cannot judge a format it does not know: {payload_report}"
    );

    let document_report = validate_document(&doc);
    assert!(
        document_report.has(rule::UNKNOWN_ENUM),
        "the document gate is what names it: {document_report}"
    );
}

/// Every rule this gate can raise is registered in `rule::ALL`, which is what
/// `rule::is_known` answers on — a waiver naming an unregistered id is
/// silently out of scope, so an unregistered rule is unwaivable by accident.
#[test]
fn every_rule_this_gate_raises_is_a_known_rule_id() {
    for id in [
        rule::ASSET_PAYLOAD_UNREADABLE,
        rule::ASSET_FORMAT_MISMATCH,
        rule::ASSET_EXTENT_MISMATCH,
        rule::ASSET_PAYLOAD_MISSING,
    ] {
        assert!(rule::is_known(id), "{id} is not registered in rule::ALL");
    }
}
