//! The import-fidelity oracle (issue #332, story Sf-2 of the full
//! real-file-import epic): a perceptual diff of the reference painter's render
//! of two **self-authored** imported fixtures against their design source —
//! Figma's REST `GET /images` export — within the E7 oracle's pinned tolerance
//! bands.
//!
//! Why this exists beside the E7 oracle (`render_oracle.rs`): the epic's exit
//! criterion asks that a real imported file render inside a measured band of
//! Figma's own render, but the epic's two real targets are third-party
//! Community files, and `docs/decisions/figma-corpus-self-authored-only.md`
//! forbids committing their JSON or their render — their fidelity is checked
//! live only (`just render`). This test carries the committed, license-clean
//! half: two self-authored fixtures exercising the two vocabulary paths the
//! real import proved live but no E7 frame measures — an embedded image fill
//! (the hero's phone mockups), and the #310 text axes end-to-end through the
//! #327/#334 render wiring.
//!
//! Deliberately separate from the E7 exit gate: this test reads
//! `goldens/oracle/import-manifest.json` and `import-design-source/`, never
//! `manifest.json` or `design-source/` (the live v0.9 gate), and reuses the
//! diff harness and the three pinned bands (`goldens::oracle`) read-only — a
//! band is never retuned here. The reference render is `goldens::render::
//! render_dsb`, the Sf-1 production path: unlike the E7 test's own
//! `render_fixture` (empty images map, default text axes), it paints embedded
//! image-fill bytes and honors the lowered text axes — the two capabilities
//! these frames measure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dashc_wasm::compile_figma;
use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::Profile;
use goldens::{oracle, render};
use serde_json::Value;

/// The `goldens/` root — one level up from this crate (`goldens/tooling`).
fn goldens_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The repository root — two levels up from this crate. A frame's `fixture`
/// path is repo-relative (`corpus/figma-fixtures/<name>.json`), unlike the
/// goldens-relative `designSource`.
fn repo_root() -> PathBuf {
    goldens_root().join("..")
}

fn load_manifest() -> Value {
    let path = goldens_root().join("oracle/import-manifest.json");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("import-oracle manifest {} present: {e}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("import-oracle manifest {} parses: {e}", path.display()))
}

fn frames(manifest: &Value) -> &Vec<Value> {
    manifest["frames"]
        .as_array()
        .expect("the manifest has a frames array")
}

/// The committed image bytes beside a fixture: `<name>.images/<imageRef>.png`,
/// keyed by the `imageRef` (the file stem), as `compile_figma` takes them.
/// This is the map the Deno importer resolves live from `GET /images`; here it
/// is the committed corpus directory the capture tool wrote (`capture.ts`), so
/// the compile is hermetic. A fixture with no image fills has no `.images/`
/// directory and gets the empty map.
fn images_for(fixture: &Path) -> BTreeMap<String, ImageAsset> {
    let dir = fixture.with_extension("images");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return BTreeMap::new();
    };
    entries
        .filter_map(|entry| {
            let path = entry.expect("readable corpus directory entry").path();
            // The capture tool writes exactly `<imageRef>.png` files and prunes
            // the rest; anything else here — a file browser's .DS_Store, an
            // editor backup — is not an image asset and must not enter the map
            // with a bogus key and a false Png label.
            if path.extension().is_none_or(|ext| ext != "png") {
                return None;
            }
            let image_ref = path
                .file_stem()
                .expect("an image asset file has a stem")
                .to_string_lossy()
                .into_owned();
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
            Some((
                image_ref,
                ImageAsset {
                    format: ImageFormat::Png,
                    bytes,
                },
            ))
        })
        .collect()
}

/// Imports a committed fixture the way a real producer does — compile through
/// `dashc`'s `compile_figma` (`Profile::Core`, strict policy, the committed
/// image bytes supplied) — and renders the emitted `.dsb` through the Sf-1
/// production render path (`goldens::render::render_dsb`: measure seam, text
/// axes, embedded images), returning the PNG the design source is diffed
/// against.
fn render_import_fixture(name: &str, fixture: &Path) -> Vec<u8> {
    let fixture_json = std::fs::read_to_string(fixture)
        .unwrap_or_else(|e| panic!("frame {name} fixture {}: {e}", fixture.display()));
    let images = images_for(fixture);
    let (bytes, report) = compile_figma(&fixture_json, Profile::Core, &images)
        .unwrap_or_else(|e| panic!("frame {name} fixture compiles: {e:?}"));
    // A committed import-oracle fixture must lower fully clean under the
    // strict policy: a diagnostic would mean part of the frame was skipped or
    // refused, so the diff would measure an omission, not fidelity. This
    // guards only lowering; render-time fidelity is what the diff measures.
    assert!(
        report.is_empty(),
        "frame {name} fixture lowers clean: {report}"
    );
    render::render_dsb(&bytes)
}

#[test]
fn every_import_frame_names_a_known_band_and_any_declared_fixture_exists() {
    let manifest = load_manifest();
    let repo = repo_root();
    assert!(!frames(&manifest).is_empty(), "the manifest lists frames");

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name");
        let band = frame["band"].as_str().expect("band name");
        let resolved = oracle::band_for(band).unwrap_or_else(|| {
            panic!("frame {name} names band {band}, which is not one of the pinned rules")
        });
        assert_eq!(
            resolved.rule, band,
            "band_for({band}) must return the band whose rule matches the name"
        );
        if let Some(fixture) = frame["fixture"].as_str() {
            let path = repo.join(fixture);
            assert!(
                path.exists(),
                "frame {name}'s fixture {} is not committed",
                path.display()
            );
        }
    }
}

#[test]
fn every_import_frame_declares_a_captured_source_or_is_pending_332() {
    // A frame with no committed design source must say so (status
    // pending-332), and one that has a source must actually ship the file —
    // the same accounting discipline as the E7 gate (G-11: nothing
    // fabricated, nothing silently dropped).
    let manifest = load_manifest();
    let root = goldens_root();
    assert_eq!(
        manifest["issue"].as_u64(),
        Some(332),
        "the manifest names issue #332"
    );

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name");
        match frame["designSource"].as_str() {
            None => assert_eq!(
                frame["status"].as_str(),
                Some("pending-332"),
                "frame {name} has no design source, so it must be marked pending-332"
            ),
            Some(source) => {
                let path = root.join(source);
                assert!(
                    path.exists(),
                    "frame {name} declares design source {} but the file is not committed",
                    path.display()
                );
                assert_eq!(
                    frame["status"].as_str(),
                    Some("captured"),
                    "frame {name} has a design source, so its status must be captured"
                );
            }
        }
    }
}

/// The import-fidelity assertion itself: for every frame that has a committed
/// design source, the production render of the frame's committed fixture must
/// fall within the frame's band of Figma's REST `GET /images` export. Each
/// measured frame compiles its fixture in-process with its committed image
/// bytes ([`render_import_fixture`]) and diffs the render against the export —
/// hermetic (committed fixture + committed export, no network) and fast, so it
/// runs in the ordinary `test` job.
#[test]
fn the_import_renders_match_their_design_source() {
    let manifest = load_manifest();
    let root = goldens_root();
    let repo = repo_root();

    let mut measured_lines: Vec<String> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    for frame in frames(&manifest) {
        let name = frame["frame"].as_str().expect("frame name").to_string();
        let band_name = frame["band"].as_str().expect("band name");
        let band = oracle::band_for(band_name)
            .unwrap_or_else(|| panic!("frame {name} names unknown band {band_name}"));

        match frame["designSource"].as_str() {
            None => pending.push(name),
            Some(source) => {
                let source_bytes = std::fs::read(root.join(source))
                    .unwrap_or_else(|e| panic!("frame {name} design source {source}: {e}"));
                let fixture = frame["fixture"].as_str().unwrap_or_else(|| {
                    panic!("frame {name} has a design source but names no fixture to render")
                });
                let reference_bytes = render_import_fixture(&name, &repo.join(fixture));

                let d = oracle::diff(&reference_bytes, &source_bytes, band)
                    .unwrap_or_else(|e| panic!("frame {name}: {e}"));
                let line = format!(
                    "{name}: {}/{} px differ ({:.3}%, max Δ {}) vs the {} band's {:.1}% budget",
                    d.differing,
                    d.total,
                    d.fraction() * 100.0,
                    d.max_channel_delta,
                    band.rule,
                    band.differing_fraction * 100.0,
                );
                if !d.passes() {
                    failures.push(line.clone());
                }
                measured_lines.push(line);
            }
        }
    }

    eprintln!(
        "IMPORT ORACLE (#332): {} frame(s) measured against a Figma design source, {} pending{}",
        measured_lines.len(),
        pending.len(),
        if pending.is_empty() {
            String::new()
        } else {
            format!(" ({})", pending.join(", "))
        }
    );
    for line in &measured_lines {
        eprintln!("  {line}");
    }

    // The accounting, asserted: every frame is either measured against a real
    // design source or explicitly pending — nothing silently dropped — and
    // `pending` names exactly the frames whose `designSource` is null.
    let expected_pending: Vec<String> = frames(&manifest)
        .iter()
        .filter(|frame| frame["designSource"].as_str().is_none())
        .map(|frame| frame["frame"].as_str().expect("frame name").to_string())
        .collect();
    assert_eq!(
        measured_lines.len() + pending.len(),
        frames(&manifest).len(),
        "every manifest frame must be measured or pending — none silently dropped"
    );
    assert_eq!(
        pending, expected_pending,
        "pending must be exactly the frames whose designSource is null"
    );

    assert!(
        failures.is_empty(),
        "import-fidelity failures:\n{}",
        failures.join("\n")
    );
}
