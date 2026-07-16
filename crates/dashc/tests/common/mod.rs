//! Shared helpers for this crate's integration tests. Each test binary
//! compiles its own copy of this module, so a helper unused by one binary
//! is still used by another — hence the `dead_code` allowances (the same
//! pattern as `dashscene-typeset`'s `tests/common`).

#![allow(dead_code)]

use dashc_wasm::figma::rule;
use dashscene_validator::{Diagnostic, Location, Severity};

/// Parses a captured (or derived) Figma REST fixture.
pub fn parse(json: &str) -> dashc_wasm::figma::rest::FigmaFile {
    serde_json::from_str(json).expect("the captured fixture parses")
}

/// The lowered node named `name`, and its index in the rect table.
pub fn node<'a>(doc: &'a dashc_wasm::Document, name: &str) -> (u32, &'a dashc_wasm::Node) {
    doc.nodes
        .iter()
        .enumerate()
        .find(|(_, n)| n.name.as_deref() == Some(name))
        .map(|(i, n)| (i as u32, n))
        .unwrap_or_else(|| panic!("no lowered node named {name}"))
}

/// Every `figma.unsupported` diagnostic, as `(path, construct)` pairs with
/// the message's fixed suffix stripped — what the tests actually pin.
pub fn unsupported(diagnostics: &[Diagnostic]) -> Vec<(String, String)> {
    diagnostics
        .iter()
        .filter(|d| d.rule == rule::UNSUPPORTED)
        .map(|d| {
            assert_eq!(
                d.severity,
                Severity::Error,
                "unsupported is always an error"
            );
            let Location::Node(at) = &d.at else {
                panic!("an unsupported construct is located at a node");
            };
            let what = d
                .message
                .strip_suffix(" is not in the document vocabulary yet")
                .unwrap_or_else(|| panic!("unexpected message shape: {}", d.message));
            (at.path.clone(), what.to_string())
        })
        .collect()
}

/// The captured JSON with every node `predicate` matches rewritten by
/// `patch`, and nothing else changed. The declared-derivation mechanism the
/// lowering tests use to isolate a construct another story owns (or to lift a
/// refusal a golden needs past — see `goldens/dsb/README.md`).
pub fn derive(
    json: &str,
    predicate: impl Fn(&serde_json::Map<String, serde_json::Value>) -> bool + Copy,
    patch: impl Fn(&mut serde_json::Map<String, serde_json::Value>) + Copy,
) -> String {
    fn walk(
        value: &mut serde_json::Value,
        predicate: impl Fn(&serde_json::Map<String, serde_json::Value>) -> bool + Copy,
        patch: impl Fn(&mut serde_json::Map<String, serde_json::Value>) + Copy,
    ) {
        if let Some(object) = value.as_object_mut() {
            if predicate(object) {
                patch(object);
            }
            if let Some(children) = object.get_mut("children").and_then(|c| c.as_array_mut()) {
                for child in children {
                    walk(child, predicate, patch);
                }
            }
        }
    }

    let mut file: serde_json::Value = serde_json::from_str(json).expect("the capture parses");
    walk(&mut file["document"], predicate, patch);
    file.to_string()
}

/// Whether a JSON node object is of Figma node `kind`.
pub fn kind_is(object: &serde_json::Map<String, serde_json::Value>, kind: &str) -> bool {
    object.get("type").and_then(|t| t.as_str()) == Some(kind)
}
