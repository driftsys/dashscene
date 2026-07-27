//! The weld test (story #435): the reference painter draws exactly the texels
//! the encoder's own reference decode produces from the blocks the shipped file
//! carries.
//!
//! # What a weld is for
//!
//! The profile preview rests on one claim: **ASTC decode is bit-exact by
//! specification, so a software decode of a derived bank reconstructs the
//! texels the target GPU samples.** Everything the Gfx QA path is worth follows
//! from that claim, and a claim carried by a comment is a claim nobody checks.
//! This file turns it into three assertions, each separately falsifiable.
//!
//! # What this weld does *not* prove, stated first
//!
//! The block decode on the preview path is `dashpack::astc::decode`, which is
//! the same vendored, version-pinned astcenc that `dashpack::astc::encode`
//! produced the payload with — one pinned tool, both directions, which is the
//! mechanism the story specifies. Both sides of leg 2 therefore run the same
//! codec, and **no assertion here can catch a defect in that codec.** A second
//! implementation would be needed for that, and the design deliberately does
//! not have one: two codec implementations would leave a difference no test
//! could attribute (`dashpack::astc`).
//!
//! # What it does prove
//!
//! Everything between the encoder and the painter, which is where the defects a
//! shared codec cannot hide actually live:
//!
//! - **Leg 1, the blocks that ship.** The ASTC blocks recovered from the
//!   assembled `.dsb` are byte-identical to what `astc::encode` returned. This
//!   is the premise itself: what the target samples is what the encoder made.
//!   It crosses the KTX2 writer, the Zstd level, the cold-bank assembly, the
//!   section table and page alignment, `dashbuf::open`'s resolution of a
//!   canonical hash through the derivation manifest, and an **independently
//!   written** KTX2 parser (the `ktx2` crate, which `dashpack::ktx2` keeps off
//!   its emit path for exactly this reason).
//!
//! - **Leg 2, the reconstruction.** `preview::decode` of the shipped file
//!   equals `astc::decode` of the encoder's own payload. Since the codec is
//!   shared, what this actually holds is **parameter recovery**: the preview
//!   reads the block footprint and the colour space out of the file's
//!   `VkFormat` rather than being told them. A preview that took them from its
//!   caller would agree with the encoder even when the file said something
//!   else, and would then render a bank correctly on a desk that renders wrongly
//!   on the target. The two mutations below decode at the wrong footprint and
//!   the wrong colour space and assert the result differs, so recovery is held
//!   by a measurement and not by a comment.
//!
//! - **Leg 3, the handoff.** Skia's own decode of the PNG re-wrap equals the
//!   texels the block decode produced. The painter is unchanged — it draws
//!   RGBA, it never measures, wraps, kerns or moves anything, so P2 holds — and
//!   the only reason a re-wrap exists at all is to hand it a container it
//!   already accepts. A wrap that premultiplied, resampled or dropped alpha
//!   would put loss into the preview that the codec never introduced.
//!
//! Composed: the texels the painter draws are exactly the texels the encoder's
//! reference decode produces from the blocks the file ships.

use dashbuf::AssetKind;
use dashbuf::bank::ColdBank;
use dashbuf::container::HASH_LEN;
use dashpack::astc::{self, BlockSize, ColorSpace, Quality, Rgba8};
use dashpack::ktx2::Format;
use dashpack::preview;
use dashpack::profile::{self, Binding, PACK_QUALITY, Profile};
use goldens::render::{png_texels, png_wrap};

mod common;
use common::manifest::repo_root;

/// The committed 380x380 image-fill payload — a real gradient with hard-edged
/// rectangles and a semi-transparent square, so the weld runs over content with
/// smooth regions, sharp edges and a non-trivial alpha channel rather than over
/// a flat colour that every rung reproduces exactly.
const PHOTO_PATH: &str = "corpus/figma-fixtures/import-image-fill.images/\
                          f856e637d6f6c2eb858e17a31d810f00542d2035.png";

/// One asset's texels, decoded from the committed corpus PNG.
fn corpus_texels() -> (u32, u32, Vec<u8>) {
    let bytes = std::fs::read(repo_root().join(PHOTO_PATH)).expect("the corpus payload reads");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("a readable PNG header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded frame")];
    let info = reader.next_frame(&mut buffer).expect("it decodes");
    buffer.truncate(info.buffer_size());
    let texels = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("the corpus payload is {other:?}; the corpus images are RGB or RGBA"),
    };
    (info.width, info.height, texels)
}

/// A minimal one-asset document, and the KTX2 file a profile derived for it,
/// assembled into a real `.dsb` and read back out of it.
///
/// The point of going through a whole file rather than handing the KTX2 bytes
/// straight to the preview is that the file is where the defects are: a section
/// table offset, a page-alignment gap, a derivation-manifest row binding a
/// canonical hash to the wrong payload. Reading the payload back out of an
/// assembled file exercises all three.
struct Shipped {
    /// The KTX2 file as `dashbuf::open` resolved it out of the assembled `.dsb`.
    resident: Vec<u8>,
    /// The rung the escalation chose, so a test can encode the same one.
    rung: profile::Rung,
}

/// Packs one asset under `profile`, assembles it into a `.dsb`, and resolves
/// the derived payload back out of the file.
fn ship(profile: Profile, width: u32, height: u32, texels: &[u8]) -> Shipped {
    let image = Rgba8::new(width, height, texels).expect("the canonical texels");
    let binding = profile::pack(profile, AssetKind::Image, image).expect("the asset packs");
    let Binding::Derived(derivation) = binding else {
        panic!(
            "{profile:?} must derive rather than bind canonically for this test to mean anything"
        )
    };

    // A hand-built document naming exactly this asset by its canonical hash.
    // `dashbuf`'s own bank tests build documents this way; here it keeps the
    // weld independent of any compiler.
    let canonical = png_canonical(width, height, texels);
    let hash: [u8; HASH_LEN] = blake3_of(&canonical);
    let ui = ui_section_naming(&hash, width, height);
    let bank = ColdBank::derived([(hash, derivation.file.as_slice())]);
    let file = dashbuf::bank::assemble(&ui, &bank).expect("the derived bank assembles");

    let (_, payloads) = dashbuf::open(&file).expect("the derived file opens");
    assert_eq!(payloads.len(), 1, "the document names exactly one asset");
    Shipped {
        resident: payloads[0].to_vec(),
        rung: derivation.rung,
    }
}

/// The block footprint and colour space a rung and the image-fill class imply.
fn format_of(rung: profile::Rung) -> Format {
    rung.format(profile::AssetClass::ImageFill)
}

#[test]
fn leg_1_the_blocks_that_ship_are_the_blocks_the_encoder_made() {
    let (width, height, texels) = corpus_texels();
    for profile in [Profile::HiFi, Profile::LoFi] {
        let shipped = ship(profile, width, height, &texels);
        let Format::Astc { block, color } = format_of(shipped.rung) else {
            panic!(
                "{profile:?} chose {} on this payload, so there are no blocks to weld — \
                    pick a payload whose escalation lands on a lossy rung",
                shipped.rung
            )
        };

        // What the encoder produced, directly.
        let image = Rgba8::new(width, height, &texels).expect("the canonical texels");
        let encoded = astc::encode(image, block, color, PACK_QUALITY).expect("the payload encodes");

        // What the file ships, read back with the independent parser.
        let inflated = level_payload(&shipped.resident);

        assert_eq!(
            inflated, encoded,
            "{profile:?}: the block payload the assembled .dsb ships is not the payload \
             astcenc produced. The whole preview rests on the target sampling the bytes \
             the encoder made, so this is the premise itself and not a detail of it.",
        );
    }
}

#[test]
fn leg_2_the_preview_reconstructs_the_encoders_own_reference_decode() {
    let (width, height, texels) = corpus_texels();
    for profile in [Profile::HiFi, Profile::LoFi] {
        let shipped = ship(profile, width, height, &texels);
        let Format::Astc { block, color } = format_of(shipped.rung) else {
            panic!("{profile:?} chose {}, no blocks to decode", shipped.rung)
        };

        let image = Rgba8::new(width, height, &texels).expect("the canonical texels");
        let encoded = astc::encode(image, block, color, PACK_QUALITY).expect("encodes");
        let reference = astc::decode(&encoded, width, height, block, color).expect("decodes");

        let previewed = preview::decode(&shipped.resident).expect("the shipped file previews");

        assert_eq!(
            (previewed.width, previewed.height),
            (width, height),
            "{profile:?}: the preview recovered the wrong extent from the file",
        );
        assert_eq!(
            previewed.format,
            Format::Astc { block, color },
            "{profile:?}: the preview recovered the wrong format from the file",
        );
        assert_eq!(
            previewed.rgba, reference,
            "{profile:?}: the preview's texels differ from the encoder's own reference \
             decode of the same blocks",
        );
    }
}

#[test]
fn leg_3_the_png_wrap_hands_the_painter_the_texels_unchanged() {
    let (width, height, texels) = corpus_texels();
    // HiFi over this payload lands on a lossy rung, so the texels carry real
    // codec residual and a partially transparent region — the two things a
    // careless wrap damages.
    let shipped = ship(Profile::HiFi, width, height, &texels);
    let previewed = preview::decode(&shipped.resident).expect("the shipped file previews");

    let wrapped = png_wrap(previewed.width, previewed.height, &previewed.rgba);
    let (size, back) = png_texels(&wrapped);

    assert_eq!(
        size,
        (previewed.width, previewed.height),
        "the PNG re-wrap changed the image extent",
    );
    assert_eq!(
        back, previewed.rgba,
        "the PNG re-wrap is not lossless: the texels Skia reads back differ from the ones \
         the block decode produced. A wrap that premultiplied, resampled or dropped alpha \
         would put loss into the preview that the codec never introduced.",
    );
}

// ------------------------------------------------------ the weld's mutations
//
// A weld is only a weld if breaking either side breaks the test. Each mutation
// below is the specific defect the corresponding leg exists to catch, applied
// deliberately, with the assertion that the two paths then disagree.

#[test]
fn decoding_at_a_footprint_the_file_does_not_record_disagrees() {
    let (width, height, texels) = corpus_texels();
    let shipped = ship(Profile::HiFi, width, height, &texels);
    let Format::Astc { block, color } = format_of(shipped.rung) else {
        panic!("HiFi chose {}, no footprint to get wrong", shipped.rung)
    };
    let previewed = preview::decode(&shipped.resident).expect("previews");

    // The next rung up and the next rung down, so the mutation is not one
    // arbitrary footprint that might happen to agree.
    for wrong in [BlockSize { x: 8, y: 8 }, BlockSize { x: 5, y: 5 }] {
        assert_ne!(
            wrong, block,
            "the mutation must actually change the footprint the file records",
        );
        let image = Rgba8::new(width, height, &texels).expect("texels");
        let encoded = astc::encode(image, block, color, PACK_QUALITY).expect("encodes");
        // Decoding the *right* blocks at the *wrong* footprint: exactly what a
        // preview that took its parameters from its caller would do.
        let Ok(mangled) = astc::decode(&encoded, width, height, wrong, color) else {
            // A footprint whose block count does not divide the payload is
            // refused outright, which is a stronger failure than a wrong
            // picture and equally proves the point.
            continue;
        };
        assert_ne!(
            mangled, previewed.rgba,
            "decoding at {}x{} produced the same texels as decoding at the {}x{} the file \
             records. Parameter recovery would then be unfalsifiable, and a bank whose \
             header disagreed with its payload would preview correctly and ship wrongly.",
            wrong.x, wrong.y, block.x, block.y,
        );
    }
}

#[test]
fn decoding_in_a_colour_space_the_file_does_not_record_disagrees() {
    let (width, height, texels) = corpus_texels();
    let shipped = ship(Profile::HiFi, width, height, &texels);
    let Format::Astc { block, color } = format_of(shipped.rung) else {
        panic!("HiFi chose {}, no colour space to get wrong", shipped.rung)
    };
    let wrong = match color {
        ColorSpace::Srgb => ColorSpace::Linear,
        ColorSpace::Linear => ColorSpace::Srgb,
    };
    let previewed = preview::decode(&shipped.resident).expect("previews");

    let image = Rgba8::new(width, height, &texels).expect("texels");
    let encoded = astc::encode(image, block, color, PACK_QUALITY).expect("encodes");
    let mangled = astc::decode(&encoded, width, height, block, wrong).expect("decodes");

    assert_ne!(
        mangled, previewed.rgba,
        "decoding as {wrong:?} produced the same texels as decoding as the {color:?} the \
         file records, so the colour space the preview recovers is unfalsifiable. sRGB \
         conversion points are already on the short list a target bench has to confirm; \
         they must not also be a thing the desk preview can get silently wrong.",
    );
}

#[test]
fn a_file_whose_header_and_payload_disagree_is_refused_rather_than_decoded() {
    let (width, height, texels) = corpus_texels();
    let shipped = ship(Profile::HiFi, width, height, &texels);
    let Format::Astc { block, color } = format_of(shipped.rung) else {
        panic!("HiFi chose {}, nothing to corrupt", shipped.rung)
    };

    // Rewrite the header's vkFormat to a coarser footprint, leaving the level
    // data alone. The file now claims blocks the payload does not hold — the
    // shape a writer bug or a truncated re-pack produces — and the payload
    // length is the only thing that can tell.
    let coarser = Format::Astc {
        block: BlockSize {
            x: block.x + 2,
            y: block.y + 2,
        },
        color,
    };
    let claimed = coarser
        .vk_format()
        .expect("the coarser footprint has a VkFormat");
    let mut corrupt = shipped.resident.clone();
    // vkFormat is the first UInt32 after the 12-byte identifier.
    corrupt[12..16].copy_from_slice(&claimed.to_le_bytes());

    let error = preview::decode(&corrupt)
        .expect_err("a header that disagrees with its payload must be refused");
    assert!(
        matches!(error, preview::PreviewError::PayloadLen { .. }),
        "the refusal must name the length disagreement rather than any other cause, got \
         {error:?}",
    );
}

#[test]
fn a_canonical_payload_is_not_mistaken_for_a_block_payload() {
    // The loader's discriminator, from the other side: the three canonical
    // containers must never sniff as KTX2, or a RAW bank would be sent through
    // a block decoder.
    let bytes = std::fs::read(repo_root().join(PHOTO_PATH)).expect("the corpus payload reads");
    assert!(
        !preview::is_ktx2(&bytes),
        "a canonical PNG must not sniff as a KTX2 block payload",
    );
    for magic in [
        &[0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A][..],
        &[0xFF, 0xD8, 0xFF, 0xE0][..],
        b"GIF89a",
    ] {
        assert!(
            !preview::is_ktx2(magic),
            "the canonical container magic {magic:?} must not sniff as KTX2",
        );
    }
}

/// The quality preset the packer runs at is the one the weld encodes at.
///
/// A weld that encoded at a different effort than the packer would compare two
/// payloads that were never meant to be equal, and leg 1 would fail for a
/// reason that is not a defect. Asserted rather than assumed, because the two
/// are separate constants.
#[test]
fn the_weld_encodes_at_the_packers_own_quality() {
    assert_eq!(
        PACK_QUALITY,
        Quality::Thorough,
        "the packer's quality preset changed; leg 1 encodes at PACK_QUALITY and stays \
         correct, but the recorded numbers in goldens/oracle/profile-manifest.json were \
         measured at the old one",
    );
}

// ---------------------------------------------------------------- machinery

/// The level 0 payload of a KTX2 file, inflated, read with the independent
/// parser rather than with an inverse of our own writer.
fn level_payload(file: &[u8]) -> Vec<u8> {
    let reader = ktx2::Reader::new(file).expect("the independent reader accepts the shipped file");
    let level = reader.levels().next().expect("exactly one level");
    zstd::bulk::decompress(level.data, level.uncompressed_byte_length as usize)
        .expect("the level inflates")
}

/// A lossless PNG of the canonical texels — the asset's canonical payload, and
/// therefore the bytes its identity hash is taken over.
fn png_canonical(width: u32, height: u32, texels: &[u8]) -> Vec<u8> {
    png_wrap(width, height, texels)
}

/// BLAKE3 of a payload, through the packer rather than through a direct
/// dependency: `pack_bank` computes an asset's identity the same way, so using
/// its result keeps one definition of what a canonical hash is.
fn blake3_of(canonical: &[u8]) -> [u8; HASH_LEN] {
    let dummy = [0u8; 4];
    let asset = dashpack::bank::Asset {
        canonical,
        kind: AssetKind::Image,
        image: Rgba8::new(1, 1, &dummy).expect("a one-texel image"),
    };
    let packed = dashpack::bank::pack_bank(Profile::Raw, &[asset]).expect("RAW binds canonically");
    packed.assets[0].canonical_hash
}

/// A ui section carrying one `Document` whose single asset entry names `hash`.
///
/// Hand-built rather than compiled: the weld is about the container and the
/// codec, and routing it through `dashc` would make a compiler change able to
/// break it for reasons that have nothing to do with either.
fn ui_section_naming(hash: &[u8; HASH_LEN], width: u32, height: u32) -> Vec<u8> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let hash_bytes = builder.create_vector(hash);
    let entry = dashbuf::AssetEntry::create(
        &mut builder,
        &dashbuf::AssetEntryArgs {
            hash: Some(hash_bytes),
            kind: AssetKind::Image,
            format: dashbuf::ImageFormat::Png,
            width,
            height,
        },
    );
    let assets = builder.create_vector(&[entry]);
    let name = builder.create_string("root");
    let node = dashbuf::Node::create(
        &mut builder,
        &dashbuf::NodeArgs {
            name: Some(name),
            parent: dashbuf::NO_PARENT,
            ..Default::default()
        },
    );
    let nodes = builder.create_vector(&[node]);
    let document = dashbuf::Document::create(
        &mut builder,
        &dashbuf::DocumentArgs {
            nodes: Some(nodes),
            assets: Some(assets),
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}
