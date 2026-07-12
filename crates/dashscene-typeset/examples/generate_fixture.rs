//! Regenerates the committed ASCII test fixture:
//! `cargo run -p dashscene-typeset --example generate_fixture`
//!
//! Only rerun this when the pipeline parameters or the tool version
//! change deliberately — the fixture is the R7 cross-machine evidence.

use std::collections::BTreeSet;
use std::path::PathBuf;

use dashscene_typeset::atlas::{AtlasSpec, generate};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let font = root.join("../../corpus/fonts/noto-sans/NotoSans-Regular.ttf");
    let out = root.join("tests/fixtures/ascii");
    let charset: BTreeSet<char> = (0x20u8..=0x7e).map(char::from).collect();
    let bundle = generate(&AtlasSpec::new(font, charset)).expect("pipeline");
    bundle.write_to_dir(&out).expect("write fixture");
    println!(
        "wrote {} ({} glyphs, {}x{})",
        out.display(),
        bundle.metrics.glyphs.len(),
        bundle.metrics.atlas.width,
        bundle.metrics.atlas.height
    );
}
