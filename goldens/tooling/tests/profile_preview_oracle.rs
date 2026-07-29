//! The profile-preview oracle (story #435, epic #345): every corpus scene
//! rendered under RAW, HiFi and LoFi, with the two production arms diffed
//! against RAW inside a pinned scene band.
//!
//! # Why this measures a scene and not only an asset
//!
//! `crates/dashpack/tests/band_contract.rs` measures each asset's texels in
//! isolation, and that is the right gate for choosing a rung. It is blind to
//! the asset **in context**: banding read behind a caption, a block boundary
//! read against a stroke. Both are scene properties, and both are what a
//! designer actually looks at, so they are measured here.
//!
//! Skia+profile against Skia+RAW is also the purest possible asset-axis
//! measurement. The other two oracles compare against Figma's server-side
//! export and must absorb rasterizer, resampling and gamma disagreement; here
//! both arms are the same painter on the same canvas and the only variable is
//! which bytes the asset entries resolve to.
//!
//! # Why the shared harness is not used
//!
//! `tests/common/manifest.rs` walks a design-source manifest: it resolves a
//! `designSource` PNG, tracks a capture `status`, and gates on a pending-issue
//! number. This oracle has no external design source at all — the RAW render is
//! the reference and is produced in the same run — so every one of those fields
//! would be a field that does not apply. It reuses the diff itself
//! (`goldens::oracle::diff`), the band type, and `common::repo_root`.
//!
//! # What this cannot show
//!
//! GPU filtering behaviour, driver-level effects (vendor bandwidth compression
//! such as UBWC, and the NVIDIA case where ASTC is emulated rather than sampled
//! natively — the pack-time probe's job), and where in a target pipeline the
//! sRGB transfer function is applied. A target bench confirms that short list.
//! It does not discover quality, because quality is settled here.
//!
//! # Artifacts
//!
//! Every run writes the triptych and its diff heatmaps to
//! `target/profile-preview/<scene>/`, which is the Gfx QA review surface
//! (`just triptych`). They are written rather than committed on purpose: a
//! committed render of a scene that exists to show codec loss would have to be
//! re-baselined for every unrelated painter change, and the numbers below are
//! the durable record.
//!
//! # Viewing conditions, if a person is shown these images
//!
//! The viewing conditions decide the answer as much as the codec does, so they
//! are stated where the artifacts are produced rather than left to whoever
//! opens the files. `just triptych` prints the short form beside the paths.
//!
//! - **Native pixels.** No browser zoom, no display scaling, no window that
//!   resizes the image. Smooth scaling averages block artifacts away, so a
//!   viewer who rescales is reporting on the resampler and not on the codec.
//! - **Integer nearest-neighbour** if any zoom is needed at all.
//! - **Blind and randomised order** if the opinion is to mean anything. A
//!   reviewer who knows which arm is LoFi is not answering the question.
//! - **The full ladder rather than three points.** The useful question is where
//!   loss becomes visible; the per-rung figures are in
//!   `goldens/tooling/tests/perceptual_calibration.rs`.
//! - **ITU-R BT.500 and ITU-T P.910** are the standard protocols for running
//!   this properly. Nothing here implements one, and the scores this file
//!   records are models of perception rather than observers.

use std::collections::BTreeMap;
use std::path::PathBuf;

use dashbuf::bank::ColdBank;
use dashbuf::container::HASH_LEN;
use dashpack::astc::{self, BlockSize, ColorSpace, Rgba8};
use dashpack::ktx2;
use dashpack::profile::{Binding, PACK_QUALITY, Profile};
use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::Profile as CompileProfile;
use goldens::metric::{self, Scores};
use goldens::oracle::{self, OracleDiff, ToleranceBand};
use goldens::render::{png_texels, png_wrap, render_dsb};
use serde_json::Value;

mod common;
use common::manifest::repo_root;
use common::stress::{STRESS_AMPLITUDE, STRESS_EXTENT, STRESS_REF, block_stress};

/// The manifest this oracle is wired by.
const MANIFEST: &str = "goldens/oracle/profile-manifest.json";

// ------------------------------------------------------------ the scenes

/// The committed 380x380 image-fill payload — gradients, two hard-edged
/// rectangles and a semi-transparent square. Story #432 measures its per-asset
/// bands on the same bytes.
const PHOTO_REF: &str = "f856e637d6f6c2eb858e17a31d810f00542d2035";
const PHOTO_PATH: &str = "corpus/figma-fixtures/import-image-fill.images/\
                          f856e637d6f6c2eb858e17a31d810f00542d2035.png";

/// The committed 16x16 payload from `v03-paint`. It is here as a second asset,
/// not as a second measurement: with one asset every index in a document is 0
/// and no ordering, deduplication or wrong-index bug can show (debt #395,
/// story #432).
const BADGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const BADGE_PATH: &str = "corpus/figma-fixtures/v03-paint.images/\
                          390616a0e7321eddb464388366d9a2a1bcb7f4c3.png";

/// A caption and a stroked rectangle over an image fill — the two in-context
/// constructs this oracle exists to measure against, sized to `canvas`.
fn overlays(canvas: f64, id: &str, text: &str, size: f64) -> Vec<Value> {
    let inset = canvas * 0.0625;
    vec![
        serde_json::json!({
            "name": "caption", "type": "TEXT", "id": format!("{id}:2"),
            "absoluteBoundingBox": {
                "x": inset, "y": canvas * 0.09375,
                "width": canvas - 2.0 * inset, "height": size * 1.5,
            },
            "characters": text,
            "style": { "fontFamily": "Noto Sans", "fontWeight": 400, "fontSize": size },
            "fills": [{ "blendMode": "NORMAL", "type": "SOLID",
                        "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
        }),
        serde_json::json!({
            "name": "frame-stroke", "type": "RECTANGLE", "id": format!("{id}:3"),
            "absoluteBoundingBox": {
                "x": inset * 1.5, "y": canvas * 0.43,
                "width": canvas - 3.0 * inset, "height": canvas * 0.47,
            },
            "fills": [],
            "strokes": [{ "blendMode": "NORMAL", "type": "SOLID",
                          "color": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } }],
            "strokeWeight": 3.0,
            "strokeAlign": "CENTER",
        }),
    ]
}

/// Wraps a root FRAME in the one-page Figma REST document `compile_figma`
/// takes.
fn document(root: Value) -> Value {
    serde_json::json!({
        "document": {
            "name": "Document", "type": "DOCUMENT",
            "children": [{ "name": "Page 1", "type": "CANVAS", "children": [root] }],
        },
    })
}

/// One scene: its name, its compiled `.dsb`, and its canvas extent.
struct Scene {
    name: &'static str,
    dsb: Vec<u8>,
    canvas: (u32, u32),
}

/// Reads a committed corpus payload as the image map entry `compile_figma`
/// takes.
fn corpus_png(path: &str) -> ImageAsset {
    ImageAsset {
        format: ImageFormat::Png,
        bytes: std::fs::read(repo_root().join(path))
            .unwrap_or_else(|e| panic!("the committed corpus payload {path} reads: {e}")),
    }
}

/// Compiles a document through `dashc`, refusing anything the Core profile
/// reports on: a scene that lowers with diagnostics would measure something
/// other than what it describes.
fn compile(scene: Value, images: &BTreeMap<String, ImageAsset>) -> Vec<u8> {
    let (dsb, report) = dashc_wasm::compile_figma(&scene.to_string(), CompileProfile::Core, images)
        .expect("the scene compiles");
    assert!(
        report.is_empty(),
        "the scene must lower clean, got: {report}"
    );
    dsb
}

/// Every scene this oracle measures, built in process.
///
/// Built rather than committed: the only committed compiled document with an
/// image is `goldens/dsb/v03-paint.dsb`, whose single image is 16x16 — one ASTC
/// block at every footprint on the ladder — so all three arms of its triptych
/// render byte-identically and it could not fail anything. See the manifest's
/// `scenesAreBuiltInProcess` field.
fn scenes() -> Vec<Scene> {
    let photo_canvas = 380.0;
    let mut photo_children = overlays(photo_canvas, "1", "Profile preview", 34.0);
    photo_children.push(serde_json::json!({
        "name": "badge", "type": "FRAME", "id": "1:4",
        "absoluteBoundingBox": { "x": 300.0, "y": 300.0, "width": 64.0, "height": 64.0 },
        "fills": [{ "blendMode": "NORMAL", "type": "IMAGE",
                    "scaleMode": "FILL", "imageRef": BADGE_REF }],
    }));
    let photo = document(serde_json::json!({
        "name": "profile-photo", "type": "FRAME", "id": "1:1",
        "absoluteBoundingBox": {
            "x": 0.0, "y": 0.0, "width": photo_canvas, "height": photo_canvas,
        },
        "fills": [{ "blendMode": "NORMAL", "type": "IMAGE",
                    "scaleMode": "FILL", "imageRef": PHOTO_REF }],
        "children": photo_children,
    }));
    let photo_images = BTreeMap::from([
        (PHOTO_REF.to_string(), corpus_png(PHOTO_PATH)),
        (BADGE_REF.to_string(), corpus_png(BADGE_PATH)),
    ]);

    let stress_canvas = STRESS_EXTENT as f64;
    let stress = document(serde_json::json!({
        "name": "profile-stress", "type": "FRAME", "id": "2:1",
        "absoluteBoundingBox": {
            "x": 0.0, "y": 0.0, "width": stress_canvas, "height": stress_canvas,
        },
        "fills": [{ "blendMode": "NORMAL", "type": "IMAGE",
                    "scaleMode": "FILL", "imageRef": STRESS_REF }],
        "children": overlays(stress_canvas, "2", "LoFi band", 30.0),
    }));
    let stress_images = BTreeMap::from([(
        STRESS_REF.to_string(),
        ImageAsset {
            format: ImageFormat::Png,
            // PNG is lossless, so the canonical payload carries exactly the
            // generated texels and the packer measures the bytes the generator
            // produced.
            bytes: png_wrap(
                STRESS_EXTENT,
                STRESS_EXTENT,
                &block_stress(STRESS_EXTENT, STRESS_EXTENT, STRESS_AMPLITUDE),
            ),
        },
    )]);

    vec![
        Scene {
            name: "profile-photo",
            dsb: compile(photo, &photo_images),
            canvas: (380, 380),
        },
        Scene {
            name: "profile-stress",
            dsb: compile(stress, &stress_images),
            canvas: (STRESS_EXTENT, STRESS_EXTENT),
        },
    ]
}

// ------------------------------------------------------- the mutation path

/// Renders `dsb` with every image asset forced to one ASTC footprint, bypassing
/// the escalation entirely.
///
/// This is the measured mutation each band ships with (issue #422): a packer
/// whose ladder walk accepted a rung it should have refused. It is built out of
/// the same public API the packer uses — encode, write the container, bind the
/// canonical hashes, assemble — so it exercises the whole load path exactly as
/// a real derived bank does, and differs from one only in which rung was
/// chosen.
///
/// The canonical hashes come from the document's own asset entries rather than
/// from re-hashing the payloads, because an entry's hash is the asset's
/// identity by definition (`docs/decisions/asset-model-content-addressed-blobs.md`).
fn render_forced(dsb: &[u8], block: BlockSize) -> Vec<u8> {
    let (document, payloads) = dashbuf::open(dsb).expect("the scene opens");
    let ui = dashbuf::container::ui_document(dsb)
        .expect("a ui section")
        .to_vec();
    let entries = document.assets().expect("the scene carries assets");
    let hashes: Vec<[u8; HASH_LEN]> = entries
        .iter()
        .map(|entry| {
            entry
                .hash()
                .bytes()
                .try_into()
                .expect("an asset entry's hash is HASH_LEN bytes")
        })
        .collect();

    let files: Vec<Vec<u8>> = payloads
        .iter()
        .map(|payload| {
            let ((width, height), texels) = png_texels(payload);
            let image = Rgba8::new(width, height, &texels).expect("decoded canonical texels");
            let encoded = astc::encode(image, block, ColorSpace::Srgb, PACK_QUALITY)
                .expect("the forced rung encodes");
            ktx2::write(
                &encoded,
                width,
                height,
                ktx2::Format::Astc {
                    block,
                    color: ColorSpace::Srgb,
                },
            )
            .expect("the forced rung writes")
        })
        .collect();

    let bank = ColdBank::derived(hashes.iter().copied().zip(files.iter().map(Vec::as_slice)));
    let forced = dashbuf::bank::assemble(&ui, &bank).expect("the forced bank assembles");
    render_dsb(&forced)
}

// ------------------------------------------------------------- the harness

/// The manifest, parsed.
fn manifest() -> Value {
    let path = repo_root().join(MANIFEST);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{MANIFEST} parses: {e}"))
}

/// A measured fraction as the manifest records it: a percentage to four decimal
/// places, compared as a string.
///
/// The same convention `crates/dashpack/tests/band_contract.rs` uses, and for
/// the same reason: four decimal places of a percentage is finer than any real
/// difference between two rungs, and string equality is exact where float
/// equality is a judgement call.
fn percent(fraction: f64) -> String {
    format!("{:.4}", fraction * 100.0)
}

/// Whether this run is a deliberate re-baseline of the recorded numbers.
///
/// The same `UPDATE_GOLDENS` switch the rest of the harness uses
/// (`goldens/tooling/src/lib.rs`, `tests/derived_bank.rs`), because one knob for
/// "I am regenerating recorded artifacts" is easier to reason about than three.
/// The manifest carries prose notes as well as numbers, so it is not rewritten
/// automatically: under this switch the oracle prints each row's measured
/// values in the manifest's own field names and skips only the equality against
/// the recorded ones. The band, the mutation, and the rung check still run, so
/// a re-baseline cannot record a scene that fails its contract.
fn updating() -> bool {
    std::env::var_os("UPDATE_GOLDENS").is_some()
}

/// The band a manifest row names, refusing a name that is not one of the two
/// pinned scene contracts.
fn band_of(row: &Value, scene: &str) -> &'static ToleranceBand {
    let name = row["band"]
        .as_str()
        .unwrap_or_else(|| panic!("{scene}: every profile row names a band"));
    oracle::profile_band_for(name).unwrap_or_else(|| {
        panic!(
            "{scene}: {name} is not a pinned profile-preview band — expected one of {:?}",
            oracle::PROFILE_BANDS
                .iter()
                .map(|b| b.rule)
                .collect::<Vec<_>>()
        )
    })
}

/// The output directory for a scene's triptych and heatmaps.
fn output_dir(scene: &str) -> PathBuf {
    let dir = repo_root().join("target/profile-preview").join(scene);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{} is writable: {e}", dir.display()));
    dir
}

/// A diff heatmap: one grayscale-on-black pixel per scene pixel, bright where
/// the profile and RAW disagree.
///
/// The scale is per-image — the largest delta present maps to full white — so a
/// heatmap always shows the shape of the disagreement rather than a nearly
/// black image whenever the residual is small, which for these scenes is
/// always. The scale factor is printed beside the file so a reader is never
/// misled into comparing brightness across two heatmaps.
fn heatmap(profile_png: &[u8], raw_png: &[u8], width: u32, height: u32) -> (Vec<u8>, u8) {
    let (_, a) = png_texels(profile_png);
    let (_, b) = png_texels(raw_png);
    let deltas: Vec<u8> = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .map(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let peak = deltas.iter().copied().max().unwrap_or(0);
    let scale = if peak == 0 { 1u32 } else { 255 / peak as u32 };
    let mut rgba = Vec::with_capacity(deltas.len() * 4);
    for delta in &deltas {
        let v = (*delta as u32 * scale).min(255) as u8;
        rgba.extend_from_slice(&[v, v, v, 255]);
    }
    (png_wrap(width, height, &rgba), peak)
}

// ------------------------------------------------------------ the assertions

/// The oracle proper: every scene, every profile, measured against its band and
/// against its recorded numbers, with the triptych and heatmaps written for
/// review.
#[test]
fn every_scene_renders_within_its_profile_band() {
    let manifest = manifest();
    let rows = manifest["scenes"]
        .as_array()
        .expect("the manifest carries a scenes array");
    let scenes = scenes();
    assert_eq!(
        rows.len(),
        scenes.len(),
        "the manifest describes {} scenes but the oracle builds {} — a scene that is \
         built and not described is unmeasured, and one that is described and not built \
         is a number nothing produces",
        rows.len(),
        scenes.len(),
    );

    for (row, scene) in rows.iter().zip(&scenes) {
        assert_eq!(
            row["scene"].as_str(),
            Some(scene.name),
            "the manifest's scene order must match the oracle's",
        );
        let dir = output_dir(scene.name);
        let raw = render_dsb(&scene.dsb);
        std::fs::write(dir.join("raw.png"), &raw).expect("the RAW arm is writable");

        for profile_row in row["profiles"]
            .as_array()
            .expect("every scene carries a profiles array")
        {
            let name = profile_row["profile"].as_str().expect("a profile name");
            let profile = goldens::profile::profile_named(name)
                .unwrap_or_else(|| panic!("{name} is not a profile"));
            assert_ne!(
                profile,
                Profile::Raw,
                "{}: RAW is the reference arm of the comparison, not a measured one",
                scene.name,
            );
            let band = band_of(profile_row, scene.name);

            // The rungs the escalation actually chose, so a manifest row cannot
            // describe a ladder position the packer no longer takes.
            assert_chosen_rungs(&scene.dsb, profile, profile_row, scene.name, name);

            let derived = goldens::profile::derive(&scene.dsb, profile).expect("the scene derives");
            let png = render_dsb(&derived);
            std::fs::write(dir.join(format!("{name}.png")), &png)
                .expect("the profile arm is writable");

            let diff = oracle::diff(&png, &raw, band).expect("the scene and RAW are the same size");
            let (map, peak) = heatmap(&png, &raw, scene.canvas.0, scene.canvas.1);
            std::fs::write(dir.join(format!("{name}-heat.png")), &map)
                .expect("the heatmap is writable");

            // The same two arms the diff above measured, scored on the two
            // published perceptual scales (issue #544). Skia's decode here and
            // not the `png` crate's: both arms are Skia renders, so the
            // comparison has to start from the decode that produced them.
            let ((width, height), profile_texels) = png_texels(&png);
            let (_, raw_texels) = png_texels(&raw);
            let scores = goldens::metric::score(width, height, &raw_texels, &profile_texels)
                .expect("both arms render at the scene's canvas extent");

            report(scene.name, name, &diff, peak, &scores);
            assert_measurement(scene.name, name, &diff, profile_row, band);
            assert_scores(scene.name, name, &scores, profile_row);
            assert_mutation(scene, name, profile_row, band);
        }
    }
}

/// Prints one measured row, so a run reports the numbers rather than only
/// whether they passed. Fidelity is a measured value, not a bare pass or fail
/// (guardrail G-11).
fn report(scene: &str, profile: &str, diff: &OracleDiff, peak: u8, scores: &Scores) {
    eprintln!(
        "PROFILE PREVIEW  {scene:<15} {profile:<5} {:>7}/{} = {:>8}%  max delta {:>3}  \
         (band {} <= {}%, heatmap scaled x{})",
        diff.differing,
        diff.total,
        percent(diff.fraction()),
        diff.max_channel_delta,
        diff.band.channel_delta,
        percent(diff.band.differing_fraction),
        if peak == 0 { 1 } else { 255 / peak as u32 },
    );
    eprintln!(
        "PROFILE PREVIEW  {:<15} {profile:<5} ssimulacra2 {:>8}  flip {:>6} desk / {:>6} panel  \
         psnr {:>8} rgb / {:>8} alpha",
        "",
        scores
            .ssimulacra2
            .map(|v| metric::fixed(v, 2))
            .unwrap_or_else(|| "withheld".to_string()),
        metric::fixed(scores.flip_desk, 4),
        metric::fixed(scores.flip_panel, 4),
        metric::fixed(scores.psnr_rgb, 2),
        metric::fixed(scores.psnr_alpha, 2),
    );
}

/// The two published perceptual scales for one scene arm, against the values
/// the manifest records (issue #544).
///
/// These calibrate rather than gate. The band above is what decides whether the
/// arm passes; these say where the arm's residual sits on a scale a reader
/// outside this repository recognises, and they are pinned so that a change to
/// the codec, the painter or the scene cannot move them silently.
fn assert_scores(scene: &str, name: &str, scores: &Scores, row: &Value) {
    let recorded = |key: &str| -> String {
        row[key]
            .as_str()
            .unwrap_or_else(|| panic!("{scene}/{name}: the row records {key}"))
            .to_string()
    };
    let measured = [
        (
            "ssimulacra2",
            scores
                .ssimulacra2
                .map(|v| metric::fixed(v, 2))
                .unwrap_or_else(|| "withheld".to_string()),
        ),
        ("flipDesk", metric::fixed(scores.flip_desk, 4)),
        ("flipPanel", metric::fixed(scores.flip_panel, 4)),
        ("psnrRgb", metric::fixed(scores.psnr_rgb, 2)),
        ("psnrAlpha", metric::fixed(scores.psnr_alpha, 2)),
    ];

    if updating() {
        let fields: Vec<String> = measured
            .iter()
            .map(|(key, value)| format!("\"{key}\": \"{value}\""))
            .collect();
        eprintln!("REBASELINE {scene}/{name}: {}", fields.join(", "));
        return;
    }

    for (key, value) in measured {
        assert_eq!(
            value,
            recorded(key),
            "{scene}/{name}: the recorded {key} moved",
        );
    }
}

/// The rungs the escalation chose for this scene under this profile, checked
/// against the manifest.
///
/// Separate from the pixel measurement on purpose: a band tells you the scene
/// still looks right, and this tells you it looks right *for the reason
/// recorded*. A packer that changed rung and happened to stay inside the band
/// would otherwise pass with a manifest that had quietly become fiction.
fn assert_chosen_rungs(dsb: &[u8], profile: Profile, row: &Value, scene: &str, name: &str) {
    let (document, payloads) = dashbuf::open(dsb).expect("the scene opens");
    let entries = document.assets().expect("the scene carries assets");
    let chosen: Vec<String> = entries
        .iter()
        .zip(&payloads)
        .map(|(entry, payload)| {
            let ((width, height), texels) = png_texels(payload);
            let image = Rgba8::new(width, height, &texels).expect("decoded canonical texels");
            match dashpack::profile::pack(profile, entry.kind(), image).expect("the asset packs") {
                Binding::Derived(derivation) => derivation.rung.to_string(),
                Binding::Canonical => "canonical".to_string(),
            }
        })
        .collect();
    let recorded: Vec<String> = row["rungs"]
        .as_array()
        .unwrap_or_else(|| panic!("{scene}/{name}: the row records the rungs it chose"))
        .iter()
        .map(|value| value.as_str().expect("a rung name").to_string())
        .collect();
    assert_eq!(
        chosen, recorded,
        "{scene}/{name}: the escalation now chooses {chosen:?} but the manifest records \
         {recorded:?} — the recorded rungs are what the measured numbers below belong to, \
         so they are re-baselined together or not at all",
    );
}

/// The live measurement against the band and against the recorded numbers.
fn assert_measurement(
    scene: &str,
    name: &str,
    diff: &OracleDiff,
    row: &Value,
    band: &'static ToleranceBand,
) {
    // A row may declare that its arm ships *outside* the scene band, because
    // its profile's contract permits it: `dashpack::profile::Terminal::
    // FinestLossy` accepts the finest lossy rung with the exceedance disclosed
    // rather than escalating past it (section 7 of the band decision record,
    // issue #553). Where that is declared, the assertion is inverted rather
    // than dropped — the exceedance must still be *there*, so a change that
    // silently brought the arm back inside its band fails here and has to be
    // re-recorded deliberately.
    let declared = row["bandExceeded"].as_bool().unwrap_or(false);
    if declared {
        assert!(
            !diff.passes(),
            "{scene}/{name}: the row declares bandExceeded, but the arm is inside the {} band \
             at {}% against a {}% budget. The declaration is now false and must be removed \
             rather than left standing.",
            band.rule,
            percent(diff.fraction()),
            percent(band.differing_fraction),
        );
    } else {
        assert!(
            diff.passes(),
            "{scene}/{name}: {}/{} = {}% of pixels exceed a per-channel delta of {}, over the \
             {} band's {}% budget (max delta seen {})",
            diff.differing,
            diff.total,
            percent(diff.fraction()),
            band.channel_delta,
            band.rule,
            percent(band.differing_fraction),
            diff.max_channel_delta,
        );
    }
    if updating() {
        eprintln!(
            "REBASELINE {scene}/{name}: \"measured\": \"{}\", \"measuredDiffering\": {}, \
             \"measuredTotal\": {}, \"maxChannelDelta\": {}",
            percent(diff.fraction()),
            diff.differing,
            diff.total,
            diff.max_channel_delta,
        );
        return;
    }
    assert_eq!(
        diff.differing,
        row["measuredDiffering"].as_u64().expect("a recorded count") as usize,
        "{scene}/{name}: the differing count moved",
    );
    assert_eq!(
        diff.total,
        row["measuredTotal"].as_u64().expect("a recorded total") as usize,
        "{scene}/{name}: the compared pixel count moved",
    );
    assert_eq!(
        percent(diff.fraction()),
        row["measured"].as_str().expect("a recorded fraction"),
        "{scene}/{name}: the measured fraction moved",
    );
    // The area budget cannot see a small number of pixels going badly wrong,
    // which is issue #422's finding in its general form. The recorded maximum
    // is the knob that can: it is the one number in this row that moves when a
    // bounded-area defect appears.
    assert_eq!(
        diff.max_channel_delta,
        u8::try_from(row["maxChannelDelta"].as_u64().expect("a recorded maximum"))
            .expect("a channel delta is a u8"),
        "{scene}/{name}: the maximum per-channel delta moved",
    );
}

/// The measured mutation that fails this band in this scene, or the stated
/// reason there is none.
fn assert_mutation(scene: &Scene, name: &str, row: &Value, band: &'static ToleranceBand) {
    let Some(mutation) = row["mutation"].as_object() else {
        assert!(
            row.get("mutationNote").and_then(Value::as_str).is_some(),
            "{}/{name}: a row with no mutation must say why in mutationNote — a band with \
             nothing that fails it is not a gate, and the manifest has to admit which rows \
             are which (issue #422)",
            scene.name,
        );
        return;
    };
    let block = mutation["forceBlock"]
        .as_array()
        .expect("forceBlock is a two-element array");
    let block = BlockSize {
        x: block[0].as_u64().expect("a block width") as u32,
        y: block[1].as_u64().expect("a block height") as u32,
    };

    let raw = render_dsb(&scene.dsb);
    let mutated = render_forced(&scene.dsb, block);
    let diff = oracle::diff(&mutated, &raw, band).expect("the mutated scene is the same size");
    assert!(
        !diff.passes(),
        "{}/{name}: forcing every asset to {}x{} measured {}% against the {} band's {}% \
         budget and PASSED. The band is then a number nothing can fail, which is exactly \
         what issue #422 measured about blur-falloff. Either the mutation is no longer a \
         defect or the budget is too wide.",
        scene.name,
        block.x,
        block.y,
        percent(diff.fraction()),
        band.rule,
        percent(band.differing_fraction),
    );
    if updating() {
        eprintln!(
            "REBASELINE {}/{name} mutation {}x{}: \"measured\": \"{}\"",
            scene.name,
            block.x,
            block.y,
            percent(diff.fraction()),
        );
        return;
    }
    assert_eq!(
        percent(diff.fraction()),
        mutation["measured"].as_str().expect("a recorded fraction"),
        "{}/{name}: the mutation's measured fraction moved",
        scene.name,
    );
}

/// Issue #422's requirement, expressed over the manifest: no band may be
/// declared that nothing in the corpus can fail.
///
/// The per-row check above allows a scene to record a number without a
/// mutation, because a scene may genuinely sit where its ladder bottoms out.
/// This one closes the loophole that would let *every* scene do that.
#[test]
fn every_band_is_exercised_by_at_least_one_scene() {
    let manifest = manifest();
    for band in oracle::PROFILE_BANDS {
        let exercised = manifest["scenes"]
            .as_array()
            .expect("a scenes array")
            .iter()
            .flat_map(|scene| scene["profiles"].as_array().expect("a profiles array"))
            .any(|row| row["band"].as_str() == Some(band.rule) && row["mutation"].is_object());
        assert!(
            exercised,
            "no scene ships a mutation that fails the {} band. A budget chosen in advance \
             and never exercised is not a gate (issue #422), so either a scene that can \
             fail it is added or the band is not declared.",
            band.rule,
        );
    }
}

/// The scene bands carry the packer's own numbers.
///
/// The profile's promise is a per-asset band; this oracle asks whether the
/// profile keeps that promise once the asset is composited, so the number to
/// hold it to is the promise itself. Asserted rather than left as a comment,
/// so that retuning a pack band cannot silently leave the scene band behind —
/// if the two ever need to differ, that is a decision to record.
#[test]
fn the_scene_bands_are_the_packers_bands() {
    for (scene_band, pack_band) in [
        (
            &oracle::PROFILE_HIFI_SCENE,
            &dashpack::profile::HIFI_IMAGE_FILL,
        ),
        (
            &oracle::PROFILE_LOFI_SCENE,
            &dashpack::profile::LOFI_IMAGE_FILL,
        ),
    ] {
        assert_eq!(
            (scene_band.channel_delta, scene_band.differing_fraction),
            (pack_band.channel_delta, pack_band.differing_fraction),
            "the {} scene band and the {} pack band must carry the same numbers",
            scene_band.rule,
            pack_band.rule,
        );
    }
}

/// The profile-preview bands are not reachable from the design-source lookup,
/// and the design-source bands are not reachable from this one.
///
/// Two families of bands answer different questions against different
/// references. One name space would let a design-source frame be graded against
/// a codec band, which at a threshold of 2 would fail every frame, or a scene be
/// graded against `blur-falloff`, which at 24 would pass anything.
#[test]
fn the_two_band_families_do_not_share_a_name_space() {
    for band in oracle::PROFILE_BANDS {
        assert!(
            oracle::band_for(band.rule).is_none(),
            "{} must not resolve through the design-source lookup",
            band.rule,
        );
    }
    for band in oracle::BANDS {
        assert!(
            oracle::profile_band_for(band.rule).is_none(),
            "{} must not resolve through the profile-preview lookup",
            band.rule,
        );
    }
}
