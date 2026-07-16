//! Shared helpers for this crate's integration tests. Each test binary
//! compiles its own copy of this module, so a helper unused by one binary
//! is still used by another — hence the `dead_code` allowances (the same
//! pattern as `dashscene-typeset`'s `tests/common`).

/// Parses a captured (or derived) Figma REST fixture.
#[allow(dead_code)]
pub fn parse(json: &str) -> dashc_wasm::figma::rest::FigmaFile {
    serde_json::from_str(json).expect("the captured fixture parses")
}

/// The lowered node named `name`, and its index in the rect table.
#[allow(dead_code)]
pub fn node<'a>(doc: &'a dashc_wasm::Document, name: &str) -> (u32, &'a dashc_wasm::Node) {
    doc.nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.name.as_deref() == Some(name))
        .map(|(i, n)| (i as u32, n))
        .unwrap_or_else(|| panic!("no lowered node named {name}"))
}
