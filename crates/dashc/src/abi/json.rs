//! The JSON half of the ABI: the report and the error.
//!
//! `dashscene-validator` and `dashpaint` carry no `serde`, deliberately — they
//! are dependency-lean, and one ABI is not a good enough reason to change that.
//! So the serializable shapes live here, in the one crate that already depends
//! on `serde`, and they are mirrors: a field added to `Diagnostic` that is not
//! added here simply does not cross, which a reviewer can see.

use serde::Serialize;

use dashscene_validator::{Diagnostic, Location, Report, Severity};

use crate::figma::CompileError;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireLocation<'a> {
    Node { index: u32, path: &'a str },
    PaintEntry { index: u32 },
    ImageAsset { index: u32 },
    VariantSet { index: u32 },
    TextStyle { index: u32 },
    Signal { index: u32 },
    Binding { index: u32 },
}

impl<'a> From<&'a Location> for WireLocation<'a> {
    fn from(at: &'a Location) -> Self {
        match at {
            Location::Node(node) => Self::Node {
                index: node.index,
                path: &node.path,
            },
            Location::PaintEntry(index) => Self::PaintEntry { index: *index },
            Location::ImageAsset(index) => Self::ImageAsset { index: *index },
            Location::VariantSet(index) => Self::VariantSet { index: *index },
            Location::TextStyle(index) => Self::TextStyle { index: *index },
            Location::Signal(index) => Self::Signal { index: *index },
            Location::Binding(index) => Self::Binding { index: *index },
        }
    }
}

#[derive(Serialize)]
struct WireDiagnostic<'a> {
    rule: &'a str,
    severity: &'static str,
    at: WireLocation<'a>,
    message: &'a str,
}

impl<'a> From<&'a Diagnostic> for WireDiagnostic<'a> {
    fn from(diagnostic: &'a Diagnostic) -> Self {
        Self {
            rule: diagnostic.rule,
            severity: match diagnostic.severity {
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            at: (&diagnostic.at).into(),
            message: &diagnostic.message,
        }
    }
}

#[derive(Serialize)]
struct WireReport<'a> {
    diagnostics: Vec<WireDiagnostic<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireError<'a> {
    Parse {
        message: String,
    },
    Unsupported {
        path: &'a str,
        what: &'a str,
    },
    UnresolvedImage {
        path: &'a str,
        #[serde(rename = "imageRef")]
        image_ref: &'a str,
    },
    Diagnostics {
        diagnostics: Vec<WireDiagnostic<'a>>,
    },
}

fn wire_diagnostics(report: &Report) -> Vec<WireDiagnostic<'_>> {
    report
        .diagnostics()
        .iter()
        .map(WireDiagnostic::from)
        .collect()
}

/// Serializing cannot fail: every mirror is a plain struct of strings and
/// integers, and `serde_json` only errors on a map with a non-string key or a
/// non-finite float. Neither exists here, so the `expect` is unreachable rather
/// than optimistic — and returning a `Result` would push an unrepresentable
/// failure onto every caller.
fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a mirror type always serializes")
}

/// The diagnostics a compile produced, as the ABI's `json` field.
pub fn report_json(report: &Report) -> String {
    to_json(&WireReport {
        diagnostics: wire_diagnostics(report),
    })
}

/// Why a compile emitted nothing, as the ABI's `json` field.
pub fn compile_error_json(error: &CompileError) -> String {
    let wire = match error {
        CompileError::Parse(e) => WireError::Parse {
            message: e.to_string(),
        },
        CompileError::Unsupported { path, what } => WireError::Unsupported { path, what },
        CompileError::UnresolvedImage { path, image_ref } => {
            WireError::UnresolvedImage { path, image_ref }
        }
        CompileError::Diagnostics(report) => WireError::Diagnostics {
            diagnostics: wire_diagnostics(report),
        },
    };
    to_json(&wire)
}

/// The `imageRef`s a lowering will demand, as the ABI's `json` field.
pub fn image_refs_json(refs: &[String]) -> String {
    to_json(&refs)
}

#[cfg(test)]
mod tests {
    use dashscene_validator::NodePath;

    use super::*;

    fn diagnostic(at: Location) -> Diagnostic {
        Diagnostic {
            rule: "paint.effect.noise",
            severity: Severity::Error,
            at,
            message: "noise is not in the v0.3 vocabulary".to_string(),
        }
    }

    #[test]
    fn a_node_location_is_tagged_and_carries_its_path() {
        let report: Report = vec![diagnostic(Location::Node(NodePath::new(3, "/card/badge")))]
            .into_iter()
            .collect();

        assert_eq!(
            report_json(&report),
            r#"{"diagnostics":[{"rule":"paint.effect.noise","severity":"error","at":{"kind":"node","index":3,"path":"/card/badge"},"message":"noise is not in the v0.3 vocabulary"}]}"#
        );
    }

    #[test]
    fn a_pool_location_is_tagged_apart_from_a_node() {
        // A paint-pool index and a node index are both integers. The tag is what
        // stops a consumer from resolving one as the other.
        let report: Report = vec![diagnostic(Location::PaintEntry(2))]
            .into_iter()
            .collect();
        assert!(report_json(&report).contains(r#""at":{"kind":"paintEntry","index":2}"#));

        let report: Report = vec![diagnostic(Location::ImageAsset(0))]
            .into_iter()
            .collect();
        assert!(report_json(&report).contains(r#""at":{"kind":"imageAsset","index":0}"#));

        // A text style is a pooled surface too (issue #41): its index must
        // tag as `textStyle`, not resolve as a node index.
        let report: Report = vec![diagnostic(Location::TextStyle(4))]
            .into_iter()
            .collect();
        assert!(report_json(&report).contains(r#""at":{"kind":"textStyle","index":4}"#));
    }

    #[test]
    fn every_compile_error_variant_is_tagged() {
        let unsupported = CompileError::Unsupported {
            path: "/card".to_string(),
            what: "an auto-layout frame".to_string(),
        };
        assert_eq!(
            compile_error_json(&unsupported),
            r#"{"kind":"unsupported","path":"/card","what":"an auto-layout frame"}"#
        );

        let unresolved = CompileError::UnresolvedImage {
            path: "/hero".to_string(),
            image_ref: "abc".to_string(),
        };
        assert_eq!(
            compile_error_json(&unresolved),
            r#"{"kind":"unresolvedImage","path":"/hero","imageRef":"abc"}"#
        );

        let parse = CompileError::Parse(serde_json::from_str::<u8>("nope").unwrap_err());
        assert!(compile_error_json(&parse).starts_with(r#"{"kind":"parse","message":"#));

        let report: Report = vec![diagnostic(Location::PaintEntry(0))]
            .into_iter()
            .collect();
        let diagnostics = CompileError::Diagnostics(report);
        assert!(
            compile_error_json(&diagnostics)
                .starts_with(r#"{"kind":"diagnostics","diagnostics":[{"#)
        );
    }

    #[test]
    fn image_refs_serialize_as_an_array() {
        assert_eq!(
            image_refs_json(&["a".to_string(), "b".to_string()]),
            r#"["a","b"]"#
        );
    }
}
