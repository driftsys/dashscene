//! The whole baked-asset chain, end to end: `dashpack` packs a corpus image to
//! ASTC, a host binds that derivation, and the lean painter uploads the blocks
//! and draws them (story #581).
//!
//! # Why this test exists, and why it lives here
//!
//! Every leg of this chain was already tested in isolation and the chain itself
//! was tested nowhere. `dashpack` proved it can encode ASTC and read it back;
//! issue #640 proved boundary B can carry a baked format; issue #716 proved the
//! row can carry an extent; `dashscene-gpu`'s own suite proves a payload reaches
//! a texture. None of that establishes the claim the work was done for —
//! `docs/specification/03-target-hardware-rules.md`'s "native ASTC directly,
//! with no Basis and no transcode step of any kind" — because that claim is
//! about the joins.
//!
//! It lives in `goldens/` because it is the only workspace member that depends
//! on both ends: `dashpack` under the default `profile-preview` feature, and now
//! the lean painter. `dashscene-gpu` must not depend on the packer, and the
//! packer must not depend on a painter.
//!
//! # What it does not establish
//!
//! Nothing about fidelity. The ASTC encoder is lossy and this compares the drawn
//! pixels against the *decoded blocks*, not against the source image — so the
//! encoder's own error is on both sides and cancels. What ASTC costs in quality
//! is `dashpack`'s band contract, measured per asset and per profile; what it
//! looks like on a real driver is layer 4's, and story #586's.

#![cfg(feature = "profile-preview")]

use dashpaint::{
    ClipIndex, ClipTable, FillSpec, GlyphRunTable, ImageAsset, ImageFill, ImageFormat, ImageTable,
    Mat23, PaintEntry, PaintTable, Painter, RectEntry, ScaleMode, Vec2,
};
use dashscene_gpu::{GpuPainter, Renderer};

/// The committed 380x380 corpus payload the profile oracle measures. Reused
/// rather than a new picture being added to the tree, and large enough that the
/// ASTC ladder has something to work on.
const PHOTO: &str = "corpus/figma-fixtures/import-image-fill.images/\
                     f856e637d6f6c2eb858e17a31d810f00542d2035.png";

const W: u32 = 96;
const H: u32 = 96;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

/// The corpus payload, decoded to the RGBA the packer takes.
fn source_texels() -> (u32, u32, Vec<u8>) {
    let bytes = std::fs::read(repo_root().join(PHOTO)).expect("the committed corpus payload reads");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("the payload has a PNG header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("a bounded frame")];
    let info = reader.next_frame(&mut buffer).expect("it decodes");
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("the corpus payload is {other:?}"),
    };
    (info.width, info.height, rgba)
}

/// The unpremultiplied RGBA texel at (x, y) of a rendered frame.
fn texel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// Draws one image-filled rect covering the canvas, and returns the pixels.
fn draw(images: ImageTable, index: u32) -> Vec<u8> {
    let mut paints = PaintTable::new();
    let fill = paints.intern_fill(&FillSpec::Image(ImageFill {
        image: index,
        scale_mode: ScaleMode::Fill,
        transform: Mat23::IDENTITY,
        tile_scale: 1.0,
    }));
    let paint = paints.push(PaintEntry {
        fill,
        ..PaintEntry::default()
    });
    let clips = ClipTable::new();
    let rects = [RectEntry {
        x: 0.0,
        y: 0.0,
        w: W as f32,
        h: H as f32,
        paint,
        clip: ClipIndex::UNCLIPPED,
        opacity: 1.0,
        rotation: 0.0,
        rotation_anchor: Vec2 { x: 0.0, y: 0.0 },
    }];
    let mut renderer = Renderer::new().expect("this test needs a device");
    let mut painter = GpuPainter::on(&renderer);
    painter.paint(
        &rects,
        &paints,
        &images,
        &clips,
        &[],
        &GlyphRunTable::new(),
        None,
    );
    renderer
        .render(
            painter.instances(),
            &paints,
            &images,
            &clips,
            &GlyphRunTable::new(),
            W,
            H,
        )
        .expect("the fixture extent is within any device's maximum")
}

/// A `dashpack` ASTC derivation is uploaded as blocks and drawn, and it draws
/// what those blocks decode to.
///
/// The reference arm is `dashpack`'s own software decoder — the one the profile
/// preview uses — so what is compared is "the GPU's ASTC unit and this
/// project's ASTC decoder agree about these blocks". That is the strongest
/// statement available without hardware, and it fails loudly if the blocks
/// arrive at the wrong footprint, in the wrong colour space, at the wrong
/// extent, or byte-shifted.
#[test]
fn a_dashpack_derivation_uploads_as_blocks_and_draws_what_it_decodes_to() {
    let renderer = Renderer::new().expect("this test needs a device");
    if !renderer.samples_astc() {
        // Loud, not silent: a skipped baked path is exactly the case that would
        // otherwise read as covered. The rest of the chain is still checked by
        // the uncompressed arm below, which runs everywhere.
        println!(
            "SKIPPED: this adapter ({}) cannot sample ASTC, so the block arm of the baked path \
             did not run. The uncompressed arm did.",
            renderer.adapter_info().name
        );
        return;
    }
    drop(renderer);

    let (width, height, source) = source_texels();
    let image =
        dashpack::astc::Rgba8::new(width, height, &source).expect("the texels are complete");
    let block = dashpack::astc::BlockSize { x: 6, y: 6 };
    let color = dashpack::astc::ColorSpace::Srgb;
    // Fastest, because what is being checked is that the blocks survive the
    // journey rather than how good they are: the encoder's own quality is
    // `dashpack`'s band contract, measured per asset and per profile.
    let encoded = dashpack::astc::encode(image, block, color, dashpack::astc::Quality::Fastest)
        .expect("the corpus image encodes");
    let file = dashpack::ktx2::write(
        &encoded,
        width,
        height,
        dashpack::ktx2::Format::Astc { block, color },
    )
    .expect("the derivation writes");

    // What a host does when it binds a derivation: unwrap the container once, at
    // load, and hand the blocks over. `dashpack::preview::blocks` is that step,
    // and it does not decode.
    let baked = dashpack::preview::blocks(&file).expect("the derivation reads back");
    assert_eq!((baked.width, baked.height), (width, height));
    assert_eq!(
        baked.texels, encoded,
        "the blocks that come out are the blocks the encoder wrote — this is the whole of the \
         no-transcode claim"
    );

    let mut images = ImageTable::new();
    let index = images.push_baked(
        ImageAsset {
            format: ImageFormat::Astc6x6Srgb,
            bytes: baked.texels.clone(),
        },
        baked.width,
        baked.height,
    );
    let drawn = draw(images, index);

    // The reference: the same blocks through this project's own ASTC decoder.
    let decoded = dashpack::preview::decode(&file).expect("the derivation previews");
    let sample = |x: u32, y: u32| {
        // The canvas is square and the image is square, so Fill maps the canvas
        // onto the image by a single ratio.
        let sx = (x * width / W).min(width - 1);
        let sy = (y * height / H).min(height - 1);
        let i = ((sy * width + sx) * 4) as usize;
        [
            decoded.rgba[i],
            decoded.rgba[i + 1],
            decoded.rgba[i + 2],
            decoded.rgba[i + 3],
        ]
    };

    // Sampled across the picture rather than at one point: a byte-shifted upload
    // or a transposed extent is correct at the origin and wrong everywhere else.
    let mut checked = 0;
    for (x, y) in [(8, 8), (48, 12), (12, 48), (80, 80), (48, 48), (88, 20)] {
        let actual = texel(&drawn, x, y);
        let expected = sample(x, y);
        for channel in 0..4 {
            let delta = actual[channel].abs_diff(expected[channel]);
            assert!(
                // A GPU's ASTC unit and a software decoder are both exact for
                // LDR, but the canvas-to-image mapping above rounds to a texel
                // where the shader rounds to a texel centre, so a neighbouring
                // source texel is admissible and a distant one is not.
                delta <= 24,
                "at ({x}, {y}) channel {channel}: the GPU drew {actual:?} where the decoder says \
                 {expected:?}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 6, "every sample point was compared");
}

/// The uncompressed rung of the same ladder, which needs no device feature and
/// therefore runs on every runner.
///
/// `Rung::Uncompressed` is a baked format too — it is uploaded as texels rather
/// than decoded — so this exercises the same `push_baked`, the same residency
/// upload and the same declaration, with the block arithmetic removed. It is the
/// arm that keeps this file meaningful on a runner without ASTC.
#[test]
fn the_uncompressed_rung_uploads_as_texels_and_draws_them() {
    let (width, height, source) = source_texels();
    let image =
        dashpack::astc::Rgba8::new(width, height, &source).expect("the texels are complete");
    let file = dashpack::ktx2::write(
        image.texels(),
        width,
        height,
        dashpack::ktx2::Format::Rgba8 {
            color: dashpack::astc::ColorSpace::Srgb,
        },
    )
    .expect("the derivation writes");

    let baked = dashpack::preview::blocks(&file).expect("the derivation reads back");
    assert_eq!(
        baked.texels, source,
        "the lossless rung round-trips its texels exactly"
    );

    let mut images = ImageTable::new();
    let index = images.push_baked(
        ImageAsset {
            format: ImageFormat::Rgba8Srgb,
            bytes: baked.texels,
        },
        baked.width,
        baked.height,
    );
    let drawn = draw(images, index);

    // Exact, because nothing between the source texels and the drawn pixels is
    // lossy: no encode, no decode, and a nearest sampler.
    for (x, y) in [(8, 8), (48, 12), (12, 48), (80, 80)] {
        let sx = (x * width / W).min(width - 1);
        let sy = (y * height / H).min(height - 1);
        let i = ((sy * width + sx) * 4) as usize;
        let expected = [source[i], source[i + 1], source[i + 2], source[i + 3]];
        let actual = texel(&drawn, x, y);
        for channel in 0..4 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 2,
                "at ({x}, {y}) channel {channel}: drew {actual:?}, source says {expected:?}"
            );
        }
    }
}
