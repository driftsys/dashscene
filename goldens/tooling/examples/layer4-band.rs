//! Layer 4 (story #586): the perceptual band, measured on a real GPU.
//!
//! `cargo run -p goldens --example layer4-band`
//!
//! For every frame the render oracle has a committed Figma design source for,
//! this builds the scene once and paints it **twice** — through
//! `dashscene-skia` and through `dashscene-gpu` — then diffs each render
//! against the same design source within the frame's own tolerance band.
//!
//! # An example rather than a test, and it asserts nothing
//!
//! Story #586's own reason: fidelity needs real hardware, CI is entirely
//! `ubuntu-latest` with no GPU, and a band tuned on lavapipe would drift with
//! the Mesa version in the runner image while saying nothing about a real
//! driver. So this prints numbers with the adapter and driver beside them, and
//! the numbers are read by a person. The same shape as
//! `examples/painter-diff.rs` and `corpus/showcase/examples/still.rs`.
//!
//! # Why the reference painter is measured here too
//!
//! It would be shorter to compare the lean painter's number against the ones
//! `tests/render_oracle.rs` already publishes. That would be wrong, and
//! quietly: `src/render.rs` records a deliberate decision **not** to share the
//! oracle's font and atlas loaders — "the E7 oracle and its helpers are left
//! byte-identical" — and its own cascade is eight atlases where the oracle's is
//! three. Any harness outside that test file is therefore building a scene the
//! oracle did not build, and a text frame rendered against a different cascade
//! is not comparable to one rendered against the oracle's.
//!
//! Painting **both** painters from the scene this file builds removes that
//! dependency entirely. The question layer 4 asks is whether the two painters
//! land in the same place against one design source, and that is answerable
//! without either number matching the oracle test's, because both arms here see
//! the same scene. The reference column doubles as the check that this file's
//! pipeline is faithful: it should land near the oracle test's published
//! figures, and a large disagreement means this harness is wrong rather than
//! the painter.
//!
//! The cascade below is the oracle test's three, in its order, for that reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dashc_wasm::compile_figma;
use dashpaint::{Atlas, Painter};
use dashscene_core::{Arena, load_document};
use dashscene_engine::TaffySolver;
use dashscene_gpu::{GpuPainter, Renderer};
use dashscene_skia::SkiaPainter;
use dashscene_typeset::text::{Font, FontFamily, Typesetter, WeightedFont};
use dashscene_validator::Profile;
use goldens::oracle::{self, ToleranceBand};
use goldens::render::{load_atlas, png_wrap};

const FONT_LATIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans/NotoSans-Regular.ttf"
);
const FONT_ARABIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
);
const FONT_INTER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/fonts/inter/Inter-Regular.otf"
);
const ATLAS_ASCII_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/ascii");
const ATLAS_INTER_ASCII_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../corpus/atlas/inter-ascii"
);
const ATLAS_ARABIC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

/// The oracle's cascade — Noto Sans, Inter, Noto Sans Arabic, in that order.
fn oracle_typesetter() -> Typesetter {
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
            vec![WeightedFont::regular(load(FONT_LATIN, "Noto Sans"))],
        ),
        FontFamily::new(
            "Inter",
            vec![WeightedFont::regular(load(FONT_INTER, "Inter"))],
        ),
        FontFamily::new(
            "Noto Sans Arabic",
            vec![WeightedFont::regular(load(FONT_ARABIC, "Noto Sans Arabic"))],
        ),
    ])
}

/// The atlases those faces sample, in the same slot order.
fn cascade_atlases() -> Vec<Atlas> {
    vec![
        load_atlas(ATLAS_ASCII_DIR),
        load_atlas(ATLAS_INTER_ASCII_DIR),
        load_atlas(ATLAS_ARABIC_DIR),
    ]
}

/// One frame's committed fixture, compiled and solved into an arena — the
/// oracle's own path, up to but not including the paint.
fn scene_for(name: &str, fixture_json: &str) -> Arena {
    let (bytes, _report) = compile_figma(fixture_json, Profile::Core, &BTreeMap::new())
        .unwrap_or_else(|e| panic!("frame {name} fixture compiles: {e:?}"));
    let (document, payloads) = dashbuf::open_verified(&bytes).expect("a valid .dsb file");
    let mut arena = Arena::new();
    load_document(&document, &payloads, &mut arena);
    // `load_document` commits with the fixed solver, which measures a text node
    // to zero; re-commit through a typesetter-backed solver so the measure seam
    // runs and TEXT nodes size to their shaped extent.
    let mut ts = oracle_typesetter();
    arena
        .open()
        .commit_with(&mut TaffySolver::with_text(&mut ts, cascade_atlases()));
    arena
}

/// One diff, formatted the way `OracleManifest::measure` formats its own, so a
/// reader can put the two side by side without converting units.
fn line(label: &str, d: &oracle::OracleDiff, band: &ToleranceBand) -> String {
    let gate = match &band.gate {
        Some(g) => format!(
            "  gate {:>7.3}% / {:.1}%{}",
            d.gate_fraction() * 100.0,
            g.differing_fraction * 100.0,
            if d.within_gate() { "" } else { "  GATE FAIL" },
        ),
        None => String::new(),
    };
    format!(
        "  {label:<9} {:>7.3}% / {:.1}%  maxΔ {:>3}{gate}{}",
        d.fraction() * 100.0,
        band.differing_fraction * 100.0,
        d.max_channel_delta,
        if d.passes() { "" } else { "   FAIL" },
    )
}

fn main() {
    let root = repo_root();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("goldens/oracle/manifest.json")).expect("the oracle manifest"),
    )
    .expect("the oracle manifest parses");

    let mut renderer = Renderer::new().expect("layer 4 needs a real GPU; this machine has none");
    let info = renderer.adapter_info();
    println!("layer 4 — the perceptual band against the Figma design source\n");
    println!("  adapter   {}", info.name);
    println!("  backend   {:?}", info.backend);
    println!("  device    {:?}", info.device_type);
    // Story #586 asks for the driver and its version beside every number.
    // **wgpu's Metal backend does not report either**: `wgpu-hal`'s Metal
    // adapter builds its `AdapterInfo` through `wgt::AdapterInfo::new`, which
    // defaults `driver` and `driver_info` to empty strings, and nothing on that
    // path fills them in. Printing two blanks would read as a harness bug, so
    // the absence is named — it is a property of the backend, and on a backend
    // that does report them this prints them.
    let driver = format!("{} {}", info.driver, info.driver_info);
    println!(
        "  driver    {}",
        match driver.trim() {
            "" => "not reported by this backend (wgpu leaves both fields empty on Metal)",
            reported => reported,
        }
    );
    println!();
    println!("  each frame: differing fraction / the band's budget, then the gate where one");
    println!("  exists. `skia` is the reference painter, `gpu` is dashscene-gpu.\n");

    let mut worse = Vec::new();

    for frame in manifest["frames"].as_array().expect("frames") {
        let name = frame["frame"].as_str().expect("frame name");
        let band_name = frame["band"].as_str().expect("band name");
        let band = oracle::band_for(band_name).expect("a known band");
        let Some(source) = frame["designSource"].as_str() else {
            println!("{name} [{band_name}]: pending, no design source");
            continue;
        };
        // The manifest states a design source relative to `goldens/` and a
        // fixture relative to the repository root — two bases, as
        // `common::manifest`'s own `goldens_root`/`repo_root` split records.
        let source_bytes =
            std::fs::read(root.join("goldens").join(source)).expect("the design source");
        let fixture = frame["fixture"].as_str().expect("a fixture");
        let fixture_json = std::fs::read_to_string(root.join(fixture)).expect("the fixture");

        let arena = scene_for(name, &fixture_json);
        let scene = arena.committed();
        let root_rect = scene.rects()[0];
        let (w, h) = (root_rect.w as u32, root_rect.h as u32);

        let mut skia = SkiaPainter::new(w as i32, h as i32);
        skia.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            None,
        );
        let skia_png = skia.png_bytes();

        // One renderer draws all seven frames and each frame is a **separate
        // arena**, which is precisely what `forget_uploaded` exists for:
        // residency is keyed by the image table's own row, and a fresh arena
        // starts that table again from zero, so one key can name a different
        // picture across two frames. The digest that would catch a collision is
        // `#[cfg(debug_assertions)]`, so a release run — which is how this is
        // run — would be silent, and the wrong texels would be reported as a
        // fidelity number rather than as a bug in this file.
        //
        // No frame in this corpus carries an image today, so nothing collides
        // yet. Issue #753 asks for exactly the gradient and image-fill frames
        // that would make it possible, and `examples/painter-diff.rs` already
        // carries this call for the same reason.
        renderer.forget_uploaded();

        let mut gpu = GpuPainter::new();
        gpu.paint(
            scene.rects(),
            scene.paints(),
            scene.images(),
            scene.clips(),
            scene.groups(),
            scene.glyphs(),
            None,
        );
        let gpu_rgba = renderer
            .render(
                gpu.instances(),
                scene.paints(),
                scene.images(),
                scene.clips(),
                scene.glyphs(),
                w,
                h,
            )
            .expect("the frame is within this device's maximum");
        let gpu_png = png_wrap(w, h, &gpu_rgba);

        let ds = oracle::diff(&skia_png, &source_bytes, band).expect("same size");
        let dg = oracle::diff(&gpu_png, &source_bytes, band).expect("same size");

        println!("{name} [{band_name}] {w}x{h}");
        println!("{}", line("skia", &ds, band));
        println!("{}", line("gpu", &dg, band));
        if !dg.passes() {
            worse.push(format!(
                "{name}: gpu {:.3}% vs skia {:.3}% against a {:.1}% budget",
                dg.fraction() * 100.0,
                ds.fraction() * 100.0,
                band.differing_fraction * 100.0,
            ));
        }
        println!();

        // The pictures, so a number that moves can be looked at rather than
        // argued about. Written beside the design source they were measured
        // against, and ignored by git.
        let out = root.join("target/layer4");
        std::fs::create_dir_all(&out).expect("an output directory");
        std::fs::write(out.join(format!("{name}-skia.png")), &skia_png).expect("write");
        std::fs::write(out.join(format!("{name}-gpu.png")), &gpu_png).expect("write");
    }

    println!("pictures written to target/layer4/");
    if worse.is_empty() {
        println!("\nevery frame is inside its band through BOTH painters.");
    } else {
        println!("\nframes outside their band through the lean painter:");
        for line in &worse {
            println!("  {line}");
        }
    }
}
