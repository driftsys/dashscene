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
use std::path::Path;

use dashc_wasm::EmitPolicy;
use dashc_wasm::compile_figma_with_bindings_and_policy;
use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::Profile;
use goldens::render;
use serde_json::Value;

mod common;
use common::manifest;
use common::manifest::repo_root;

/// This oracle's manifest, walked through the shared harness (debt #338).
fn manifest() -> manifest::OracleManifest {
    manifest::OracleManifest::load(
        "oracle/import-manifest.json",
        "pending-332",
        "IMPORT ORACLE (#332)",
    )
}

/// The `.dsb` image-table format tag for a corpus image file's extension —
/// the capture tool's own naming (`EXTENSION_OF`, `importers/figma/src/
/// capture.ts`), inverted: `.png` -> Png, `.jpg` -> Jpeg, `.gif` -> Gif
/// (story #342). Any other extension is not a capture-tool asset (a file
/// browser's `.DS_Store`, an editor backup) and is skipped, never guessed
/// at (P4).
fn image_format_of_extension(ext: &std::ffi::OsStr) -> Option<ImageFormat> {
    match ext.to_str()? {
        "png" => Some(ImageFormat::Png),
        "jpg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        _ => None,
    }
}

/// The committed image bytes beside a fixture:
/// `<name>.images/<imageRef><ext>`, keyed by the `imageRef` (the file stem),
/// as `compile_figma` takes them. This is the map the Deno importer resolves
/// live from `GET /images`; here it is the committed corpus directory the
/// capture tool wrote (`capture.ts`), so the compile is hermetic. A fixture
/// with no image fills has no `.images/` directory and gets the empty map.
fn images_for(fixture: &Path) -> BTreeMap<String, ImageAsset> {
    let dir = fixture.with_extension("images");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return BTreeMap::new();
    };
    entries
        .filter_map(|entry| {
            let path = entry.expect("readable corpus directory entry").path();
            let format = path.extension().and_then(image_format_of_extension)?;
            let image_ref = path
                .file_stem()
                .expect("an image asset file has a stem")
                .to_string_lossy()
                .into_owned();
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
            Some((image_ref, ImageAsset { format, bytes }))
        })
        .collect()
}

/// The Figma node id the frame's design source is an export of — the
/// `figmaNodeId` the capture tool wrote beside `figmaFileKey`.
///
/// Required on every frame that is **rendered**, because it is what
/// [`scope_to_exported_node`] narrows the fixture to. It is not required of the
/// manifest at large: this format shares the E7 manifest's shape, where a frame
/// stays `pending-332` with a null `figmaNodeId` until it is authored and
/// captured (`importers/figma/src/render_oracle.ts`), and such a frame is
/// counted as pending rather than measured, so this is never reached for it.
fn figma_node_id<'a>(frame: &'a Value, name: &str) -> &'a str {
    frame["figmaNodeId"].as_str().unwrap_or_else(|| {
        panic!("frame {name} must name the figmaNodeId its design source exports")
    })
}

/// Narrows a captured file's JSON to the one canvas node the frame's design
/// source is an export of, returning the re-serialized document.
///
/// A design source is Figma's `GET /images` render of a **single node**, but a
/// committed fixture is the whole captured file. `dashc` lowers every top-level
/// canvas node as an independent root re-based to the origin
/// (`crates/dashc/src/figma/mod.rs`), and the render walk stages text for every
/// root (`goldens/tooling/src/render.rs`), so a canvas *sibling* of the
/// exported frame is painted over it while the export cannot contain it.
///
/// That is debt #382: `liga-text`'s `_manual-checklist` — the fixture-author
/// plugin's authoring instruction, sitting beside the frame at y=224, outside
/// the exported 0..200 region — contributed 1901 of the frame's 1907 differing
/// pixels, so its recorded 2.270 % was almost entirely annotation ink rather
/// than the ligature residual its note described. Scoped, the frame measures
/// 6/84000 px (0.007 %). `effects-2025` carries the same sibling and would have
/// inherited the artifact the moment it gained a frame.
///
/// Scoping here rather than at capture keeps the committed fixture a faithful
/// copy of the Figma file — the checklist is real content of that file — while
/// making the oracle compile exactly what Figma rendered.
///
/// # Panics
///
/// Panics unless the document holds exactly one canvas child with `node_id`.
/// Falling back to the whole file would silently re-introduce the artifact this
/// removes (P4: a gap is a named diagnostic, never a silent drop).
fn scope_to_exported_node(fixture_json: &str, name: &str, node_id: &str) -> String {
    let mut file: Value = serde_json::from_str(fixture_json)
        .unwrap_or_else(|e| panic!("frame {name} fixture parses as Figma file JSON: {e}"));
    let canvases = file["document"]["children"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("frame {name} fixture has a document with canvas children"));
    let mut kept = 0usize;
    for canvas in canvases.iter_mut() {
        if let Some(children) = canvas["children"].as_array_mut() {
            children.retain(|child| child["id"].as_str() == Some(node_id));
            kept += children.len();
        }
    }
    assert_eq!(
        kept, 1,
        "frame {name} names exported node {node_id}, which must appear exactly \
         once among the fixture's canvas children — the design source is Figma's \
         render of that one node, so any other canvas root would lower re-based \
         to the origin and paint over it (#382)"
    );
    // Drop the pages the node is not on, so the compile sees one canvas holding
    // one root — the shape the export is a render of.
    canvases.retain(|canvas| {
        canvas["children"]
            .as_array()
            .is_some_and(|children| !children.is_empty())
    });
    file.to_string()
}

/// Imports a committed fixture the way a real producer does — narrow it to the
/// exported node ([`scope_to_exported_node`]), compile through `dashc`'s
/// `compile_figma_with_bindings_and_policy` (`Profile::Core`, no bindings,
/// `EmitPolicy::Partial`, the committed image bytes supplied) — and renders the
/// emitted `.dsb` through the Sf-1 production render path
/// (`goldens::render::render_dsb`: measure seam, text axes, embedded images),
/// returning the PNG the design source is diffed against.
///
/// Partial, not strict: most committed fixtures still lower fully clean (an
/// empty `expected_warnings`, the historical invariant this asserts exactly as
/// before), but a frame may disclose one or more constructs it deliberately
/// does not lower — story C2/#143's `node-fx` skips a rotated rect (rotation
/// stays refused by design) — declared as `expectedWarnings` in the manifest
/// so the assertion still catches anything *un*expected. A diagnostic outside
/// that declared set would mean part of the frame was skipped or refused for
/// an undisclosed reason, so the diff would measure an omission, not fidelity.
///
/// The declared set is read against the **scoped** document, so a diagnostic
/// raised only by a canvas sibling is no longer expected of the frame either.
fn render_import_fixture(
    name: &str,
    fixture: &Path,
    node_id: &str,
    expected_warnings: &[String],
) -> Vec<u8> {
    let fixture_json = std::fs::read_to_string(fixture)
        .unwrap_or_else(|e| panic!("frame {name} fixture {}: {e}", fixture.display()));
    let fixture_json = scope_to_exported_node(&fixture_json, name, node_id);
    let images = images_for(fixture);
    let (bytes, report) = compile_figma_with_bindings_and_policy(
        &fixture_json,
        Profile::Core,
        &images,
        &[],
        EmitPolicy::Partial,
    )
    .unwrap_or_else(|e| panic!("frame {name} fixture compiles: {e:?}"));
    let mut messages: Vec<&str> = report
        .diagnostics()
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    messages.sort_unstable();
    let mut expected: Vec<&str> = expected_warnings.iter().map(String::as_str).collect();
    expected.sort_unstable();
    assert_eq!(
        messages, expected,
        "frame {name} fixture lowers with exactly its declared expectedWarnings \
         (empty by default — an undeclared diagnostic means part of the frame \
         was skipped or refused for an undisclosed reason, so the diff would \
         measure an omission, not fidelity)"
    );
    render::render_dsb(&bytes)
}

/// A frame's optional `expectedWarnings`: the exact diagnostic messages a
/// fixture is declared to lower with under `EmitPolicy::Partial` (a
/// deliberately un-lowered construct, disclosed rather than silently
/// tolerated). Absent or empty means the fixture must lower fully clean — the
/// historical invariant every frame before story C2/#143 satisfies.
fn expected_warnings(frame: &Value) -> Vec<String> {
    let name = frame["frame"].as_str().unwrap_or("<unnamed>");
    let Some(warnings) = frame.get("expectedWarnings") else {
        return Vec::new();
    };
    warnings
        .as_array()
        .unwrap_or_else(|| panic!("frame {name}'s expectedWarnings must be an array"))
        .iter()
        .map(|w| {
            w.as_str()
                .unwrap_or_else(|| {
                    panic!("frame {name}'s expectedWarnings entries must be strings")
                })
                .to_string()
        })
        .collect()
}

#[test]
fn every_import_frame_names_a_known_band_and_any_declared_fixture_exists() {
    manifest().assert_bands_and_fixtures();
}

/// The #382 invariant: what the oracle compiles is the one node the design
/// source is an export of, never the rest of the captured file.
///
/// Asserted structurally rather than by re-measuring, because the artifact is
/// structural: a canvas sibling of the exported frame lowers as its own root
/// re-based to the origin and paints over the frame, so the diff counts ink the
/// export cannot contain. The count of siblings removed is reported so a
/// fixture that gains or loses one is visible in the test log.
#[test]
fn scoping_leaves_exactly_the_exported_node_of_every_frame() {
    let m = manifest();
    let repo = repo_root();

    for frame in m.frames() {
        let name = frame["frame"].as_str().expect("frame name");
        // Only the frames that actually render: a `pending-332` frame has no
        // design source to be an export of, and no `figmaNodeId` yet.
        let (Some(fixture), true) = (
            frame["fixture"].as_str(),
            frame["designSource"].as_str().is_some(),
        ) else {
            continue;
        };
        let node_id = figma_node_id(frame, name);
        let path = repo.join(fixture);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("frame {name} fixture {}: {e}", path.display()));

        let before: Value = serde_json::from_str(&raw).expect("the fixture parses");
        let roots_before: usize = before["document"]["children"]
            .as_array()
            .expect("the document has canvas children")
            .iter()
            .filter_map(|canvas| canvas["children"].as_array())
            .map(Vec::len)
            .sum();

        let scoped: Value = serde_json::from_str(&scope_to_exported_node(&raw, name, node_id))
            .expect("the scoped document parses");
        let canvases = scoped["document"]["children"]
            .as_array()
            .expect("the scoped document has canvas children");
        assert_eq!(
            canvases.len(),
            1,
            "frame {name} must scope to the one page its exported node is on"
        );
        let roots: Vec<&str> = canvases[0]["children"]
            .as_array()
            .expect("the scoped canvas has children")
            .iter()
            .map(|child| child["id"].as_str().expect("a node carries an id"))
            .collect();
        assert_eq!(
            roots,
            vec![node_id],
            "frame {name} must compile exactly its exported node {node_id} — any \
             other canvas root would lower re-based to the origin and paint over \
             it (#382)"
        );
        eprintln!(
            "  {name}: {} canvas root(s) captured, {} dropped as not exported",
            roots_before,
            roots_before - 1
        );
    }
}

#[test]
#[should_panic(expected = "must appear exactly once among the fixture's canvas children")]
fn scoping_refuses_a_node_the_fixture_does_not_carry() {
    // Never fall back to the whole file: that is exactly the silent behavior
    // #382 removed, so an unknown id has to be loud (P4).
    let repo = repo_root();
    let raw = std::fs::read_to_string(repo.join("corpus/figma-fixtures/liga-text.json"))
        .expect("the liga-text fixture is committed");
    scope_to_exported_node(&raw, "liga-text", "9:99");
}

#[test]
fn every_import_frame_declares_a_captured_source_or_is_pending_332() {
    // The same accounting discipline as the E7 gate (G-11: nothing fabricated,
    // nothing silently dropped). This manifest spells its gate as a top-level
    // `issue`, where the E7 one uses `gate.issue`, so the field check stays
    // here while the per-frame accounting is shared.
    let m = manifest();
    assert_eq!(
        m.value()["issue"].as_u64(),
        Some(332),
        "the manifest names issue #332"
    );
    m.assert_captured_or_pending();
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
    let repo = repo_root();
    manifest().measure(|frame| {
        let name = frame["frame"].as_str().expect("frame name");
        let fixture = frame["fixture"].as_str().unwrap_or_else(|| {
            panic!("frame {name} has a design source but names no fixture to render")
        });
        let warnings = expected_warnings(frame);
        let node_id = figma_node_id(frame, name);
        render_import_fixture(name, &repo.join(fixture), node_id, &warnings)
    });
}
