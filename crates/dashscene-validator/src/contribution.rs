//! The contribution gate: does the host's set of bound contributions agree
//! with the placeholders the document declares?
//!
//! Neither half can answer alone: the document states which nodes are
//! placeholders and which contribution ids they name, and only the host knows
//! which ids it binds. That is why the check cannot live in `dashc` — the
//! compiler holds the first half and never the second (issue #851). The caller
//! with both is whoever loads a document into a host, which is where
//! `validate_document` is already called from.
//! [`crate::validate_asset_payloads`] takes a second input for the same kind
//! of reason.
//!
//! Which of the four states warn, and why the two shapes that look unfilled do
//! not, is `docs/decisions/a-host-binds-a-contribution-by-id.md`.

use std::collections::HashSet;

use dashbuf::{Document, NO_CONTRIBUTION, NO_FRAGMENT};

use crate::document::node_path;
use crate::paint::warning;
use crate::{Location, Profile, Report, rule};

/// The contribution gate (story #1127). `bound` is the contribution ids the
/// host declares it fills. It is a list rather than a set so that an unmatched
/// id is reported once per entry — collapsing it would lose one diagnostic per
/// duplicated *undeclared* id. It is not duplicate detection: two entries
/// naming an id the document does declare are both silent.
///
/// Two of the four states warn:
///
/// | node is a placeholder | a host contribution binds it | verdict |
/// | --- | --- | --- |
/// | yes | yes | filled — silent |
/// | yes | no | [`rule::PLACEHOLDER_UNFILLED`] |
/// | no | yes | [`rule::PLACEHOLDER_UNDECLARED_OVERLOAD`], on both profiles |
/// | no | no | ordinary — silent |
///
/// [`rule::PLACEHOLDER_UNFILLED`] is suppressed on a [`Profile::Core`] target
/// that binds nothing a host contribution can fill, which is every ordinary
/// `Core` build: a lean painter has no host-content mechanism, so an unfilled
/// placeholder is the correct state and one warning per placeholder in every
/// build is how a diagnostic channel stops being read (issue #851). A `Core`
/// caller that binds an id **a host contribution can fill** has contradicted
/// that premise, so it is told what it left unfilled rather than told nothing.
/// Two kinds of binding do not lift it: one that matches nothing, since a
/// misspelled id is the likeliest cause and it would otherwise raise one
/// warning per placeholder; and one whose only match is a placeholder a
/// fragment fills, since no host contribution is owed for that id at all.
///
/// Both are warnings rather than errors, because neither names a malformed
/// document: an unfilled placeholder is a migration burn-down item, and an
/// unmatched binding costs a designer's time rather than a frame. A release
/// build refuses either through [`Report::strict`], and a target that accepts
/// one declares a waiver.
///
/// **A placeholder whose `contribution_id` this gate cannot read gets no
/// verdict, and silences [`rule::PLACEHOLDER_UNDECLARED_OVERLOAD`] for the
/// whole document.** On a [`Profile::Core`] target it can also suppress
/// [`rule::PLACEHOLDER_UNFILLED`]: the unreadable id is absent from the set
/// the arming test consults, so a binding whose only match was that id does
/// not arm it. Both directions are debt #1275. In each case the name this gate
/// could not read could have been any binding's, so neither can a binding be
/// called undeclared nor counted as arming evidence. Findings already made from
/// readable placeholders are kept, so the report is not necessarily empty and
/// its shape is not the signal.
///
/// The signal is [`crate::validate_document`]'s
/// `placeholder.string-out-of-range` **naming `contribution_id`** — that rule
/// also fires for a `fragment_ref`, which does not make this report partial,
/// so the field in its message is the part that matters. **Nothing this
/// function returns says so** — a report with no overload findings looks the
/// same either way — which is debt #1275.
///
/// **It walks every root, not the shown one**, in both directions. A host that
/// shows one root of a multi-root document is still told about placeholders in
/// the roots it never loads, and cannot clear them — it has no such
/// contributions to bind. No caller passes a shown root today because none
/// passes a binding list at all; closing it needs a seam that carries one into
/// the runtime. **Story #859 is not that seam** — it was named here as the
/// candidate before it was built, and what it added runs the other way: it
/// hands the committed tables *out* to a host that draws them. No entry point
/// takes a binding list. A placeholder in a root the host never loads can also
/// *satisfy* a binding meant for the root it does load, so
/// [`rule::PLACEHOLDER_UNDECLARED_OVERLOAD`] fails open there with nothing to
/// notice. Debt: #1272.
pub fn validate_contributions(doc: &Document<'_>, bound: &[&str], profile: Profile) -> Report {
    let mut report = Report::default();
    let nodes = doc.nodes().unwrap_or_default();
    let strings = doc.strings().unwrap_or_default();
    // Every contribution id the document declares, whatever its fill route —
    // this is what the overload check below asks against.
    let mut declared: HashSet<&str> = HashSet::new();
    // Whether that set is the whole of what the document declares. An id past
    // the string pool names something this gate cannot read, so `declared` is
    // missing an entry and any binding could be the missing one.
    let mut declared_is_complete = true;
    // The placeholders a host contribution is owed for: they declare a readable
    // id and no streamed route. Collected in the same walk that builds
    // `declared`, so the two rules never resolve a placeholder twice and cannot
    // drift apart on how one is read.
    let mut owed: Vec<(u32, &str)> = Vec::new();

    // One walk: what does the document declare, and for what is a contribution
    // owed?
    //
    // **This gate asks what each side declares, never whether what it declares
    // is well-formed.** A pool entry that is empty, or an index that names one,
    // is a document defect for the load gate to name (debt #1273) — reporting
    // it through a rule about host bindings would be the wrong rule saying the
    // wrong thing, and the message would name a contribution that does not
    // exist.
    for (index, node) in nodes.iter().enumerate() {
        let Some(placeholder) = node.placeholder() else {
            continue;
        };
        let id = placeholder.contribution_id();
        // "A box that reserves space without naming a binding" (`dashbuf.fbs`)
        // — a shape the schema permits, which declares no id.
        if id == NO_CONTRIBUTION {
            continue;
        }
        // Past the pool: this gate cannot read the name, so what the document
        // declares is not fully known.
        if id as usize >= strings.len() {
            declared_is_complete = false;
            continue;
        }
        let name = strings.get(id as usize);
        // In range and empty: read perfectly well, and it names nothing a host
        // could bind, so it declares nothing — exactly as `NO_CONTRIBUTION`
        // does, and the declaration set stays complete. Marking it incomplete
        // would switch off `placeholder.undeclared-overload` for the whole
        // document on the strength of a name that is known, not unknown.
        if name.is_empty() {
            continue;
        }
        declared.insert(name);

        // A fragment declares a streamed subtree as the fill route, so no host
        // contribution is owed. Set at all is the predicate: the schema's
        // reading of the sentinel is that `NO_FRAGMENT` means "the
        // contribution is drawn by the host rather than streamed", so the
        // field's presence is the declaration. Whether the index resolves is
        // the well-formedness question above, not this one. (A placeholder
        // naming both routes contradicts itself, and no gate reports that:
        // debt #1274.)
        if placeholder.fragment_ref() != NO_FRAGMENT {
            continue;
        }
        owed.push((index as u32, name));
    }

    // A `Core` target has no host-content mechanism, so an unfilled
    // placeholder is its correct state. A caller that binds an id **a host
    // contribution can fill** has contradicted that, and is told what it left
    // unfilled.
    //
    // The test is against `owed`, not against everything the document
    // declares. A binding that matches nothing is the weakest possible
    // evidence of a host-content mechanism — a misspelled id is likelier — and
    // one such binding would otherwise raise one warning per placeholder in
    // the document, the noise this suppression exists to prevent (issue #851).
    // A binding whose only match is a placeholder a fragment fills is evidence
    // of the opposite: no host contribution is owed for that id at all.
    //
    // One set, sized by the host's binding list rather than by the document:
    // the arming test and the fill test ask the same question of it. Built only
    // when there is something to ask about — with nothing owed, neither reader
    // touches it, and `dashc` lowers no placeholder today, so that is every
    // document this repository produces.
    let bound_set: HashSet<&str> = if owed.is_empty() {
        HashSet::new()
    } else {
        bound.iter().copied().collect()
    };
    // The capability being inferred here — does this target have a host-content
    // mechanism — is one the caller already knows, and `Profile` is defined as a
    // paint-vocabulary subset rather than a capability. Taking it from the
    // caller instead would remove this test and both its carve-outs: debt #1277.
    // A `match` rather than `profile == Profile::Full`, so a third `Profile`
    // variant has to state its answer here instead of inheriting the
    // suppression silently (P4).
    let arms_unfilled = match profile {
        Profile::Full => true,
        Profile::Core => owed.iter().any(|(_, name)| bound_set.contains(name)),
    };

    if arms_unfilled {
        for (index, name) in owed {
            if bound_set.contains(name) {
                continue;
            }
            // Once per declaring node, not once per id: two placeholders naming
            // one id are two boxes a designer must each decide about, and the
            // location is the node.
            report.push(warning(
                rule::PLACEHOLDER_UNFILLED,
                &Location::Node(node_path(&nodes, index)),
                format!("placeholder declares contribution `{name}`, which no host binding fills"),
            ));
        }
    }

    // With an unreadable id anywhere in the document, `declared` is not the
    // whole of what it declares, so "no placeholder declares this" cannot be
    // said of any binding. The document is blocked by the load gate's
    // `placeholder.string-out-of-range` either way; blaming the host for it
    // would be a second diagnostic pointing at the wrong half.
    //
    // **One unreadable id silences the rule for the whole document**, which is
    // as narrow as this can be: the missing name could have been any binding's,
    // so there is no subset it is safe to keep reporting. The findings already
    // pushed above are kept — they were reached from readable placeholders, and
    // dropping them would lose real findings to an unrelated corrupt index.
    //
    // Nothing this function *returns* distinguishes that state, which is why
    // the rustdoc tells a caller to read `validate_document`'s
    // `placeholder.string-out-of-range` rather than the shape of this report.
    if !declared_is_complete {
        return report;
    }

    for id in bound {
        // The other half of the empty-name skip above. An empty id names
        // nothing on either side, so it is not a binding — and reporting one
        // would claim no placeholder declares it, which is false whenever the
        // document carries the same empty string. Whether a malformed entry
        // deserves a diagnostic of its own rather than this silent skip is
        // debt #1273.
        if id.is_empty() || declared.contains(id) {
            continue;
        }
        report.push(warning(
            rule::PLACEHOLDER_UNDECLARED_OVERLOAD,
            &Location::Contribution((*id).to_owned()),
            format!(
                "the host binds contribution `{id}`, which no placeholder in this document \
                 declares"
            ),
        ));
    }

    report
}
