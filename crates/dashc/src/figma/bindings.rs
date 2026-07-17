//! Applying the importer's joined variable-binding rows to a lowered
//! document (story #167; `docs/decisions/token-resolution-phase-split.md`,
//! `docs/decisions/binding-table-in-the-document.md`).
//!
//! The importer joins the phase-1 sidecar (`{nodeId, property,
//! variableId}`) against the plugin-exported vartable and resolves each
//! variable's value for the node's mode; what crosses the ABI is one
//! [`BoundVariable`] per site: the Figma node id, the Figma property
//! path, the mode-qualified signal name, and the resolved value. This
//! module is the Figma-aware half of the split (P5): it knows which
//! property paths map onto which binding channels; the importer knows
//! nothing of channels, and this module knows nothing of modes.
//!
//! Every unsupported site is a named diagnostic, never a silent drop
//! (P4) — and never an error: the lowering already emitted the site's
//! resolved literal, so the picture is right; only the *live* binding is
//! not carried yet.

use std::collections::{BTreeMap, HashMap};

use dashpaint::PaintKind;
use dashscene_validator::{Diagnostic, Location, NodePath, Severity};

use crate::document::{Binding, BindingChannel, BindingTransform, Document, SignalDecl};

/// The diagnostic rules this producer assembles for binding rows.
pub mod rule {
    /// The row names a Figma node the lowering emitted no document node
    /// for — a component definition (resolves but does not paint), or a
    /// node id the capture does not carry. The binding has no target, so
    /// it is named and not emitted; nothing rendered is affected.
    pub const UNLOWERED_NODE: &str = "figma.bindings.unlowered-node";
    /// The row's property path has no binding channel in the vocabulary
    /// yet (a corner radius, a stroke color, a gradient stop, an effect,
    /// an opacity), or its value type does not fit the path. The resolved
    /// literal ships; the live binding does not — named, never silent.
    pub const UNSUPPORTED_PROPERTY: &str = "figma.bindings.unsupported-property";
    /// Two rows declare one signal name with two different initial
    /// values. The join qualifies a signal's name by its resolved mode,
    /// so one name always resolves to one value — a conflict means the
    /// rows are inconsistent, and emitting either value would silently
    /// pick one (P4).
    pub const SIGNAL_CONFLICT: &str = "figma.bindings.signal-conflict";
}

/// One joined variable-binding row, as decoded from the compile request.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundVariable {
    /// The Figma node id the sidecar recorded (`"1:8"`).
    pub node_id: String,
    /// The sidecar property path (`"itemSpacing"`, `"fills[0].color"`).
    pub property: String,
    /// The mode-qualified signal name the join produced
    /// (`"size/gap"`, `"size/gap@dark"`). A color site appends the
    /// channel suffix here (`.r`/`.g`/`.b`/`.a`).
    pub signal: String,
    /// The variable's value, resolved for the node's mode.
    pub value: BoundValue,
}

/// A joined row's resolved value. `FLOAT` and `COLOR` are the two
/// variable types with a channel vocabulary this slice; the importer
/// names `STRING`/`BOOLEAN` rows itself and does not send them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoundValue {
    Float(f32),
    Color { r: f32, g: f32, b: f32, a: f32 },
}

/// Where one lowered node landed, and what the binding rows need to know
/// about how it lowered.
pub(super) struct LoweredNode {
    /// The node's document index.
    pub(super) index: u32,
    /// The node's diagnostic name path.
    pub(super) path: String,
    /// The visible fill's paint `opacity` (1.0 when absent). The lowering
    /// folds it into the shipped literal's alpha (`color_of`:
    /// `color.a * opacity`), so a FillA binding must capture the same
    /// multiply as its transform — otherwise the seeded scene would jump
    /// to the raw variable alpha.
    pub(super) fill_opacity: f32,
}

/// Figma node id → how it lowered, for every node the walk emitted.
pub(super) type IndexOfId = BTreeMap<String, LoweredNode>;

/// Applies the joined rows to the lowered document, appending
/// `Document.signals` and `Document.bindings`, and returns the
/// diagnostics the rows earned. Rows apply in the order given (the
/// sidecar's document order), and signals intern in first-use order, so
/// the same input yields the same tables (R7).
pub(super) fn apply(
    doc: &mut Document,
    rows: &[BoundVariable],
    index_of_id: &IndexOfId,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut signal_of: HashMap<String, u32> = HashMap::new();

    for row in rows {
        let Some(lowered) = index_of_id.get(&row.node_id) else {
            diagnostics.push(Diagnostic {
                rule: rule::UNLOWERED_NODE,
                severity: Severity::Warning,
                at: Location::Node(NodePath::new(0, format!("<figma:{}>", row.node_id))),
                message: format!(
                    "binding of \"{}\" targets Figma node {}, which lowered to no document node \
                     (a definition, or a node outside the export); the binding is not carried",
                    row.signal, row.node_id
                ),
            });
            continue;
        };
        let (node, path) = (&lowered.index, &lowered.path);
        let at = || Location::Node(NodePath::new(*node, path.clone()));
        let unsupported = |what: &str| Diagnostic {
            rule: rule::UNSUPPORTED_PROPERTY,
            severity: Severity::Warning,
            at: at(),
            message: format!(
                "{what}; the resolved literal ships in the document, but the live binding of \
                 \"{}\" is not carried",
                row.signal
            ),
        };

        match (&row.value, classify_property(&row.property)) {
            (BoundValue::Float(v), PropertySite::ItemSpacing) => {
                if doc.nodes[*node as usize].container.is_none() {
                    diagnostics.push(unsupported(&format!(
                        "itemSpacing is bound on {path}, but the node lowered without an \
                         auto-layout container"
                    )));
                    continue;
                }
                match intern_signal(doc, &mut signal_of, &row.signal, *v) {
                    Ok(signal) => doc.bindings.push(Binding {
                        signal,
                        node: *node,
                        channel: BindingChannel::Gap,
                        transform: BindingTransform::Identity,
                    }),
                    Err(diagnostic) => diagnostics.push(diagnostic_at(diagnostic, at())),
                }
            }
            (BoundValue::Color { r, g, b, a }, PropertySite::FillColor) => {
                let solid = matches!(
                    doc.nodes[*node as usize].paint,
                    Some(ref paint) if matches!(paint.entry.fill, Some(PaintKind::Solid { .. }))
                );
                if !solid {
                    diagnostics.push(unsupported(&format!(
                        "a fill color is bound on {path}, but the node lowered without a solid \
                         fill (a text node's fill lives in its text style)"
                    )));
                    continue;
                }
                // The lowering folds the paint's opacity into the shipped
                // literal's alpha (`color_of`), so the alpha channel's
                // transform captures the same multiply over the raw
                // variable alpha — the transform-applied initial equals
                // the literal, and a producer pushing a new variable
                // value stays folded. The other components are unfolded.
                let alpha_transform = if lowered.fill_opacity == 1.0 {
                    BindingTransform::Identity
                } else {
                    BindingTransform::Scale(lowered.fill_opacity)
                };
                let components: [(&str, BindingChannel, f32, BindingTransform); 4] = [
                    (".r", BindingChannel::FillR, *r, BindingTransform::Identity),
                    (".g", BindingChannel::FillG, *g, BindingTransform::Identity),
                    (".b", BindingChannel::FillB, *b, BindingTransform::Identity),
                    (".a", BindingChannel::FillA, *a, alpha_transform),
                ];
                for (suffix, channel, component, transform) in components {
                    let name = format!("{}{suffix}", row.signal);
                    match intern_signal(doc, &mut signal_of, &name, component) {
                        Ok(signal) => doc.bindings.push(Binding {
                            signal,
                            node: *node,
                            channel,
                            transform,
                        }),
                        Err(diagnostic) => diagnostics.push(diagnostic_at(diagnostic, at())),
                    }
                }
            }
            (BoundValue::Float(v), PropertySite::Opacity) => {
                // Node/group opacity binds on any node — no container or
                // fill precondition, unlike gap and fill color (story #44,
                // debt #253). The natural landing for a Figma opacity
                // variable now that the document carries `Node.opacity`.
                match intern_signal(doc, &mut signal_of, &row.signal, *v) {
                    Ok(signal) => doc.bindings.push(Binding {
                        signal,
                        node: *node,
                        channel: BindingChannel::Opacity,
                        transform: BindingTransform::Identity,
                    }),
                    Err(diagnostic) => diagnostics.push(diagnostic_at(diagnostic, at())),
                }
            }
            (value, site) => {
                let kind = match value {
                    BoundValue::Float(_) => "FLOAT",
                    BoundValue::Color { .. } => "COLOR",
                };
                let what = match site {
                    PropertySite::ItemSpacing | PropertySite::FillColor | PropertySite::Opacity => {
                        format!(
                            "a {kind} variable is bound to {}, which takes the other value type",
                            row.property
                        )
                    }
                    PropertySite::Other => format!(
                        "{} has no binding channel in the vocabulary yet",
                        row.property
                    ),
                };
                diagnostics.push(unsupported(&what));
            }
        }
    }

    diagnostics
}

/// A partially-built signal-conflict diagnostic, completed with the
/// node location by the caller.
struct SignalConflict {
    message: String,
}

fn diagnostic_at(conflict: SignalConflict, at: Location) -> Diagnostic {
    Diagnostic {
        rule: rule::SIGNAL_CONFLICT,
        severity: Severity::Error,
        at,
        message: conflict.message,
    }
}

/// Interns one signal declaration by name, first-use order. A repeated
/// name must repeat its initial value bit-for-bit: the join qualifies
/// names by mode, so one name is one value, and a mismatch is an
/// inconsistent row set.
fn intern_signal(
    doc: &mut Document,
    signal_of: &mut HashMap<String, u32>,
    name: &str,
    initial: f32,
) -> Result<u32, SignalConflict> {
    if let Some(&index) = signal_of.get(name) {
        let declared = doc.signals[index as usize].initial;
        if declared.to_bits() != initial.to_bits() {
            return Err(SignalConflict {
                message: format!(
                    "signal \"{name}\" is declared with initial {declared} and bound again with \
                     {initial}; one mode-qualified name resolves to one value, so the joined \
                     rows are inconsistent"
                ),
            });
        }
        return Ok(index);
    }
    let index = u32::try_from(doc.signals.len()).expect("document exceeds u32::MAX signals");
    doc.signals.push(SignalDecl {
        name: name.to_owned(),
        initial,
    });
    signal_of.insert(name.to_owned(), index);
    Ok(index)
}

/// The property paths with a channel mapping this slice.
enum PropertySite {
    /// `itemSpacing` — the auto-layout gap.
    ItemSpacing,
    /// `fills[i].color` — a paint's own solid color. Hidden paints are
    /// not in the sidecar and stacked visible fills refuse the compile,
    /// so any index here is the single lowered fill.
    FillColor,
    /// `opacity` — the node/group alpha (story #44, debt #253).
    Opacity,
    Other,
}

fn classify_property(property: &str) -> PropertySite {
    if property == "itemSpacing" {
        return PropertySite::ItemSpacing;
    }
    if property == "opacity" {
        return PropertySite::Opacity;
    }
    if let Some(rest) = property.strip_prefix("fills[")
        && let Some(index) = rest.strip_suffix("].color")
        && index.chars().all(|c| c.is_ascii_digit())
        && !index.is_empty()
    {
        return PropertySite::FillColor;
    }
    PropertySite::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_paths_classify_by_shape() {
        assert!(matches!(
            classify_property("itemSpacing"),
            PropertySite::ItemSpacing
        ));
        assert!(matches!(
            classify_property("fills[0].color"),
            PropertySite::FillColor
        ));
        assert!(matches!(
            classify_property("fills[12].color"),
            PropertySite::FillColor
        ));
        assert!(matches!(
            classify_property("opacity"),
            PropertySite::Opacity
        ));
        for other in [
            "strokes[0].color",
            "fills[0].gradientStops[2].color",
            "effects[0].color",
            "rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS",
            "fills[].color",
            "fills[x].color",
        ] {
            assert!(
                matches!(classify_property(other), PropertySite::Other),
                "{other} must not classify as a supported site"
            );
        }
    }
}
