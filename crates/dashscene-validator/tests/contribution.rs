//! The contribution gate (story #1127): a placeholder no host filled, and a
//! host contribution no placeholder declares.
//!
//! The gate takes both halves because neither alone can answer. A document
//! states which nodes are placeholders and which contribution ids they name;
//! only the host knows which ids it binds. `dashc` holds the first half and
//! never the second, which is why the check cannot live there (issue #851).
//!
//! Documents are built with `common`'s `Doc`, shared with the load gate's
//! suite. Wherever one carries a string pool the entries are distinct and the
//! index a placeholder names differs from that node's own index, so a gate that
//! confused the two would fail rather than pass by coincidence. Some documents
//! here carry no string pool at all, and the rule does not apply to them.

mod common;

use common::{Doc, NodeSpec, PlaceholderSpec};
use dashbuf::root_as_document;
use dashscene_validator::{Location, NodePath, Profile, Report, Severity, Waiver, rule};

fn check(doc: Doc, bound: &[&str], profile: Profile) -> Report {
    let bytes = doc.build();
    let document = root_as_document(&bytes).expect("the flatbuffer verifier accepts this buffer");
    dashscene_validator::validate_contributions(&document, bound, profile)
}

/// A document whose node 1 is a placeholder naming `speedo`, under a root.
/// `speedo` sits at string index 2 so that no test passes by reading the node
/// index as a string index.
fn one_placeholder() -> Doc {
    Doc::default()
        .named_strings(&["unused-0", "unused-1", "speedo", "tacho"])
        .node(NodeSpec {
            name: "dash",
            ..Default::default()
        })
        .node(NodeSpec {
            name: "gauge-slot",
            parent: Some(0),
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        })
}

// -- row 2: unfilled ------------------------------------------------------

/// On a `Full` target a host is expected to fill what the document declares,
/// so a placeholder nothing binds is a migration burn-down item.
#[test]
fn an_unfilled_placeholder_warns_on_full() {
    let report = check(one_placeholder(), &[], Profile::Full);
    let d = report
        .find(rule::PLACEHOLDER_UNFILLED)
        .unwrap_or_else(|| panic!("expected {}: {report}", rule::PLACEHOLDER_UNFILLED));
    assert_eq!(d.severity, Severity::Warning, "{report}");
    assert_eq!(
        d.at,
        Location::Node(NodePath::new(1, "/dash/gauge-slot")),
        "{report}"
    );
    assert!(d.message.contains("speedo"), "{report}");
}

/// On a `Core` target nothing is ever filled, so row 2 is the correct state
/// and warning on it would put one warning per placeholder into every build
/// — which is how a diagnostic channel stops being read (issue #851).
#[test]
fn an_unfilled_placeholder_is_silent_on_core() {
    let report = check(one_placeholder(), &[], Profile::Core);
    assert!(report.is_empty(), "{report}");
}

/// The same document with the id bound. Row 1 is silent on both profiles.
#[test]
fn a_filled_placeholder_is_silent_on_both_profiles() {
    for profile in [Profile::Core, Profile::Full] {
        let report = check(one_placeholder(), &["speedo"], profile);
        assert!(report.is_empty(), "{profile:?}: {report}");
    }
}

// -- row 3: the undeclared overload ---------------------------------------

/// The expensive one, and the one nothing else catches: host code covering a
/// node the designer believes ships. It warns on both profiles — a `Core`
/// target binds nothing, so a binding there is wrong for two independent
/// reasons.
#[test]
fn a_binding_no_placeholder_declares_warns_on_both_profiles() {
    for profile in [Profile::Core, Profile::Full] {
        // `tacho` is in the string pool but no placeholder names it, so a
        // gate that scanned the pool rather than the placeholders would miss
        // this. The bound order is also not the pool order.
        let report = check(one_placeholder(), &["tacho", "speedo"], profile);
        let d = report
            .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
            .unwrap_or_else(|| {
                panic!(
                    "{profile:?}: expected {}: {report}",
                    rule::PLACEHOLDER_UNDECLARED_OVERLOAD
                )
            });
        assert_eq!(d.severity, Severity::Warning, "{profile:?}: {report}");
        assert_eq!(
            d.at,
            Location::Contribution("tacho".to_owned()),
            "{profile:?}: {report}"
        );
        assert!(d.message.contains("tacho"), "{profile:?}: {report}");
        assert!(
            !report.has(rule::PLACEHOLDER_UNFILLED),
            "{profile:?}: speedo is bound: {report}"
        );
    }
}

/// The location names the id, not a position. A `Waiver` matches on `Location`
/// equality, so a positional location would make a waiver follow the host's
/// argument order — this pins that reordering `bound` does not move it.
#[test]
fn the_overload_location_is_the_id_and_survives_reordering() {
    for bound in [
        ["speedo", "tacho"].as_slice(),
        ["tacho", "speedo"].as_slice(),
    ] {
        let report = check(one_placeholder(), bound, Profile::Full);
        let d = report
            .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
            .unwrap_or_else(|| panic!("{bound:?}: {report}"));
        assert_eq!(
            d.at,
            Location::Contribution("tacho".to_owned()),
            "{bound:?}: {report}"
        );
    }
}

// -- row 4, and the two shapes that are not row 2 -------------------------

/// Row 4. A document of ordinary nodes with a host that binds nothing is the
/// state every document before this vocabulary is in.
#[test]
fn an_ordinary_node_is_silent() {
    let report = check(
        Doc::default().node(NodeSpec {
            name: "dash",
            ..Default::default()
        }),
        &[],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

/// A placeholder at `NO_CONTRIBUTION` is "a box that reserves space without
/// naming a binding" (`dashbuf.fbs`). No host binding could ever match it, so
/// a warning here is permanent and nothing clears it.
#[test]
fn a_placeholder_naming_no_contribution_is_silent() {
    let report = check(
        Doc::default().node(NodeSpec {
            name: "spacer",
            placeholder: Some(PlaceholderSpec::default()),
            ..Default::default()
        }),
        &[],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

/// A placeholder carrying a `fragment_ref` is filled by a streamed subtree
/// rather than by a host contribution (`dashbuf.fbs`: `NO_FRAGMENT` means
/// "drawn by the host rather than streamed"), so no host binding is owed.
#[test]
fn a_placeholder_with_a_fragment_ref_is_silent() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", "unused-1", "speedo", "fragment.dsb"])
            .node(NodeSpec {
                name: "streamed-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(2),
                    fragment_ref: Some(3),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &[],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

// -- both warnings at once ------------------------------------------------

/// The two rules are independent, and a document can be in both states. Each
/// diagnostic must point at its own subject rather than at the first
/// candidate found.
#[test]
fn the_two_warnings_name_their_own_subjects() {
    let doc = Doc::default()
        .named_strings(&["filled", "unused-1", "unused-2", "speedo"])
        .node(NodeSpec {
            name: "dash",
            ..Default::default()
        })
        .node(NodeSpec {
            name: "bound-slot",
            parent: Some(0),
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "gauge-slot",
            parent: Some(0),
            // Pool index 3, at node index 2 — a gate feeding the contribution
            // id to `node_path` would report node 3, which does not exist.
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(3),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["filled", "tacho"], Profile::Full);

    let unfilled = report.find(rule::PLACEHOLDER_UNFILLED).expect("unfilled");
    assert_eq!(
        unfilled.at,
        Location::Node(NodePath::new(2, "/dash/gauge-slot")),
        "{report}"
    );
    let overload = report
        .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .expect("overload");
    assert_eq!(
        overload.at,
        Location::Contribution("tacho".to_owned()),
        "{report}"
    );
    assert_eq!(report.diagnostics().len(), 2, "{report}");
}

// -- a document the load gate has already failed --------------------------

/// A contribution id past the end of the string pool is
/// `placeholder.string-out-of-range`, an error the load gate raises. This
/// gate must not raise it again, and — since `flatbuffers::Vector::get`
/// asserts — must not panic reaching for the name either. Nothing orders the
/// gates, so this one runs on documents the load gate has failed.
#[test]
fn a_contribution_id_past_the_string_pool_does_not_panic_here() {
    let report = check(
        Doc::default().named_strings(&["speedo"]).node(NodeSpec {
            name: "gauge-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(7),
                ..Default::default()
            }),
            ..Default::default()
        }),
        &[],
        Profile::Full,
    );
    assert!(
        !report.has(rule::PLACEHOLDER_UNFILLED),
        "the load gate owns an out-of-range index: {report}"
    );
    assert!(
        !report.has(rule::PLACEHOLDER_STRING_OUT_OF_RANGE),
        "{report}"
    );
}

/// A document with no string pool at all — every field a `.dsb` may omit is a
/// reachable state, and the pool is omitted whenever nothing names a string.
/// The placeholder's id is unreadable here, so this gate says nothing at all:
/// see `an_unreadable_id_does_not_discard_the_findings_already_made`.
#[test]
fn a_document_with_no_string_pool_does_not_panic() {
    let report = check(
        Doc::default().node(NodeSpec {
            name: "gauge-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        }),
        &["speedo"],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

// -- what the exempt shapes still declare ---------------------------------

/// A `fragment_ref` placeholder is owed no host contribution, but it still
/// **declares** its id — so a host that binds that id is not an overload.
/// Without this, moving `declared.insert` below the `fragment_ref` guard passes
/// every other test here and silently turns a legitimate binding into a
/// warning.
#[test]
fn a_fragment_ref_placeholder_still_declares_its_id() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", "unused-1", "speedo", "fragment.dsb"])
            .node(NodeSpec {
                name: "streamed-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(2),
                    fragment_ref: Some(3),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &["speedo"],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

/// A `NO_CONTRIBUTION` placeholder declares nothing, so it shields no binding.
/// The reverse of the test above, and the two are easy to conflate.
#[test]
fn a_no_contribution_placeholder_shields_no_binding() {
    let report = check(
        Doc::default().node(NodeSpec {
            name: "spacer",
            placeholder: Some(PlaceholderSpec::default()),
            ..Default::default()
        }),
        &["speedo"],
        Profile::Full,
    );
    assert!(
        report.has(rule::PLACEHOLDER_UNDECLARED_OVERLOAD),
        "a box that names no binding declares no id: {report}"
    );
}

/// A contribution id resolving to an empty pool string names nothing a host
/// could bind. Reporting it would print a diagnostic whose subject is blank,
/// and this crate treats an empty name as absent elsewhere.
#[test]
fn an_empty_contribution_id_string_is_not_an_id() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", ""])
            .node(NodeSpec {
                name: "gauge-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &[],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

// -- the Core suppression is about binding nothing, not about the profile --

/// A `Core` caller that binds an id a host contribution can fill has
/// contradicted the premise the
/// suppression rests on — a lean painter has no host-content mechanism — so it
/// is told what it left unfilled rather than told nothing. Without this, a
/// `Core` host that binds one of two placeholders gets an empty report: the
/// bound one is declared so raises no overload, and the unbound one is
/// suppressed.
#[test]
fn a_core_target_that_binds_something_still_hears_about_the_rest() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "speedo", "unused-2", "tacho"])
        .node(NodeSpec {
            name: "bound-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "gauge-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(3),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["speedo"], Profile::Core);
    let d = report
        .find(rule::PLACEHOLDER_UNFILLED)
        .unwrap_or_else(|| panic!("{report}"));
    assert!(d.message.contains("tacho"), "{report}");
    assert_eq!(report.diagnostics().len(), 1, "{report}");
}

// -- multiplicity, both sides --------------------------------------------

/// Two placeholders naming one id are two boxes a designer must each decide
/// about, so each is named. The location is the node, which is what makes them
/// separately waivable.
#[test]
fn two_placeholders_naming_one_id_are_named_once_each() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "unused-1", "speedo"])
        .node(NodeSpec {
            name: "left",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "right",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &[], Profile::Full);
    let at: Vec<&Location> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule::PLACEHOLDER_UNFILLED)
        .map(|d| &d.at)
        .collect();
    assert_eq!(at.len(), 2, "{report}");
    assert_eq!(
        at[0],
        &Location::Node(NodePath::new(0, "/left")),
        "{report}"
    );
    assert_eq!(
        at[1],
        &Location::Node(NodePath::new(1, "/right")),
        "{report}"
    );
}

/// A duplicated `bound` entry is itself a host defect, so the unmatched id is
/// reported once per entry rather than collapsed. Both carry the same location,
/// since the location is the id.
#[test]
fn a_duplicated_binding_is_reported_once_per_entry() {
    let report = check(one_placeholder(), &["tacho", "tacho"], Profile::Full);
    let at: Vec<&Location> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .map(|d| &d.at)
        .collect();
    assert_eq!(at.len(), 2, "{report}");
    // Both carry the same location, because the location is the id — which is
    // what lets one waiver clear the pair. A positional variant would give them
    // different locations and keep the count green.
    assert_eq!(
        at[0],
        &Location::Contribution("tacho".to_owned()),
        "{report}"
    );
    assert_eq!(at[1], at[0], "{report}");
}

/// An empty id names nothing on either side of the comparison. Without the
/// skip on the `bound` side, this reports that no placeholder declares `` —
/// which is false here, and prints a diagnostic whose subject renders blank.
#[test]
fn an_empty_id_is_not_a_binding_on_either_side() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", ""])
            .node(NodeSpec {
                name: "gauge-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &[""],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

/// A fragment that is *set* declares a streamed fill route, so no host
/// contribution is owed — whatever the index resolves to. This gate asks what
/// each side declares, never whether what it declares is well-formed: an empty
/// or out-of-range pool entry is a document defect for the load gate to name
/// (debt #1273), and reporting it through a rule about host bindings would
/// print a message naming a contribution that does not exist.
///
/// Four rounds of this branch's review moved this line, because each argued
/// from the case rather than from the question the gate asks. Two facts settled
/// it: the rule fired only on placeholders that *also* named a contribution id,
/// so a fragment that was the sole declared route stayed silent anyway; and
/// `placeholder.unfilled`'s own message and workaround hint are about an id,
/// not a subtree.
#[test]
fn a_fragment_that_is_set_is_a_fill_route_however_it_resolves() {
    for fragment in [2, 9] {
        let report = check(
            Doc::default()
                .named_strings(&["unused-0", "speedo", ""])
                .node(NodeSpec {
                    name: "gauge-slot",
                    placeholder: Some(PlaceholderSpec {
                        contribution_id: Some(1),
                        fragment_ref: Some(fragment),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            &[],
            Profile::Full,
        );
        assert!(report.is_empty(), "fragment {fragment}: {report}");
    }
}

/// A placeholder naming a fragment **and** a contribution id is owed no host
/// contribution: the fragment fills it.
///
/// The fixture must carry a readable id or it never reaches the guard — a
/// placeholder at `NO_CONTRIBUTION` is skipped several lines earlier, so a test
/// built that way passes with the fragment guard deleted or inverted. An earlier
/// version of this test was built exactly that way, and its doc comment claimed
/// to pin the case it could not reach.
#[test]
fn a_fragment_fills_a_placeholder_that_also_names_an_id() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", "speedo", "fragment.dsb"])
            .node(NodeSpec {
                name: "streamed-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(1),
                    fragment_ref: Some(2),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &[],
        Profile::Full,
    );
    assert!(report.is_empty(), "{report}");
}

/// A `Core` binding whose only match is a fragment-route placeholder is not
/// evidence of a host-content mechanism — no host contribution is owed for that
/// id — so it must not arm reporting for every other placeholder.
#[test]
fn a_core_binding_matching_only_a_fragment_route_does_not_arm_reporting() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "streamed", "a", "b", "fragment.dsb"])
        .node(NodeSpec {
            name: "streamed-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(1),
                fragment_ref: Some(4),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "left",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "right",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(3),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["streamed"], Profile::Core);
    assert!(report.is_empty(), "{report}");
}

/// A `Core` caller passing only empty ids has bound nothing by this gate's own
/// definition, so it must not arm the reporting the way a real binding does.
#[test]
fn an_empty_id_does_not_arm_core_reporting() {
    let report = check(one_placeholder(), &[""], Profile::Core);
    assert!(report.is_empty(), "{report}");
}

/// The rendering a designer reads. Every other test here compares `Location`
/// values, so nothing else would notice this changing.
#[test]
fn the_overload_location_renders_the_id() {
    let report = check(one_placeholder(), &["tacho"], Profile::Full);
    let d = report
        .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .unwrap_or_else(|| panic!("{report}"));
    assert_eq!(d.at.to_string(), "<contribution tacho>", "{report}");
}

// -- the waiver path, which is what the id-carrying location is for ---------

/// `Location::Contribution` carries the id rather than a position **because**
/// `Waiver` matches on `Location` equality. Every other test here compares two
/// `Location` values, which is an assertion in the gate's own vocabulary and
/// invariant under substituting it — this one drives the mechanism the design
/// rests on, end to end, and pins that both new rule ids are waivable at all.
#[test]
fn a_waiver_at_the_contribution_clears_the_strict_build() {
    // `speedo` is bound so the only finding is the overload, and the waiver
    // below has one thing to clear.
    let report = check(one_placeholder(), &["speedo", "tacho"], Profile::Full);
    assert_eq!(report.diagnostics().len(), 1, "{report}");
    assert!(
        !report.strict(&[]).passes(),
        "a warning blocks a release build without a waiver: {report}"
    );

    let waivers = [Waiver::new(
        rule::PLACEHOLDER_UNDECLARED_OVERLOAD,
        Location::Contribution("tacho".to_owned()),
        "the 3D layer draws the tachometer; the document deliberately omits it",
    )];
    let strict = report.strict(&waivers);
    assert!(strict.passes(), "{report}");
    assert_eq!(strict.applied().len(), 1, "{report}");

    // The waiver is keyed by id, so it does not follow the host's ordering —
    // and it does not cover a different binding either.
    let other = check(one_placeholder(), &["speedo", "hvac.fan"], Profile::Full);
    assert!(
        !other.strict(&waivers).passes(),
        "a waiver for `tacho` must not cover `hvac.fan`: {other}"
    );
}

/// The unfilled rule is waivable too, at the node it names.
#[test]
fn a_waiver_at_the_node_clears_an_unfilled_placeholder() {
    let report = check(one_placeholder(), &[], Profile::Full);
    let waivers = [Waiver::new(
        rule::PLACEHOLDER_UNFILLED,
        Location::Node(NodePath::new(1, "/dash/gauge-slot")),
        "the speedometer migrates to a host contribution next slice",
    )];
    assert!(report.strict(&waivers).passes(), "{report}");
}

// -- the hint, and the order the report carries -----------------------------

/// Both rules name a design choice, so both owe a workaround hint. Deleting
/// their arms from `rule::workaround` left the whole suite green until this
/// test existed, and a rule-id rename would fall to the `_ => None` arm just
/// as silently.
#[test]
fn both_rules_carry_a_workaround_hint() {
    let report = check(one_placeholder(), &["tacho"], Profile::Full);
    // Each hint is checked against its OWN rule, not a disjunction both satisfy:
    // the overload hint contains "binding", so a `bind || declare` test passes
    // with the two arms swapped — and every hint would then tell the designer to
    // do the opposite of what is needed.
    let unfilled = report
        .find(rule::PLACEHOLDER_UNFILLED)
        .unwrap_or_else(|| panic!("{report}"))
        .workaround()
        .unwrap_or_else(|| panic!("unfilled carries no workaround: {report}"));
    assert!(
        unfilled.starts_with("bind a contribution"),
        "an unfilled placeholder is cleared by binding it: {unfilled}"
    );

    let overload = report
        .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .unwrap_or_else(|| panic!("{report}"))
        .workaround()
        .unwrap_or_else(|| panic!("overload carries no workaround: {report}"));
    assert!(
        overload.starts_with("declare a placeholder"),
        "an undeclared overload is cleared by declaring one: {overload}"
    );
}

/// `Report`'s own documentation promises the binding diagnostics come after the
/// document-order ones, because their subjects have no position in the
/// document. Every other test here asks per rule, which is invariant under
/// swapping the two loops.
#[test]
fn binding_diagnostics_come_after_the_document_ordered_ones() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "unused-1", "speedo"])
        .node(NodeSpec {
            name: "dash",
            ..Default::default()
        })
        .node(NodeSpec {
            name: "gauge-slot",
            parent: Some(0),
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["tacho"], Profile::Full);
    let rules: Vec<&str> = report.diagnostics().iter().map(|d| d.rule).collect();
    assert_eq!(
        rules,
        vec![
            rule::PLACEHOLDER_UNFILLED,
            rule::PLACEHOLDER_UNDECLARED_OVERLOAD
        ],
        "{report}"
    );
}

/// The early return that suppresses the overload rule keeps what the node loop
/// already found. Every other test reaching that branch builds a document whose
/// report is empty there anyway, so `return Report::default()` would pass them
/// all while silently discarding real findings.
#[test]
fn an_unreadable_id_does_not_discard_the_findings_already_made() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "unused-1", "speedo"])
        .node(NodeSpec {
            name: "corrupt-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(99),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "gauge-slot",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["tacho"], Profile::Full);

    let d = report
        .find(rule::PLACEHOLDER_UNFILLED)
        .unwrap_or_else(|| panic!("the readable placeholder is still judged: {report}"));
    assert!(d.message.contains("speedo"), "{report}");
    assert!(
        !report.has(rule::PLACEHOLDER_UNDECLARED_OVERLOAD),
        "`tacho` is unchecked, not absent: {report}"
    );
    assert_eq!(report.diagnostics().len(), 1, "{report}");
}

/// A `contribution_id` resolving to an empty pool entry declares nothing, and
/// this gate **knows** that — it read the name. So the declaration set stays
/// complete and the overload rule keeps working: a host binding that no
/// placeholder declares is still named.
///
/// The contrast is `an_unreadable_id_does_not_discard_the_findings_already_made`,
/// where the id is past the pool and the name is genuinely unknown. An earlier
/// round of this branch treated the two alike and silenced the overload rule
/// here, which failed open on the gate's highest-value rule with no signal to
/// the caller — the load gate raises nothing for an in-range index.
#[test]
fn an_empty_id_still_lets_the_overload_rule_work() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", ""])
            .node(NodeSpec {
                name: "gauge-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &["speedo"],
        Profile::Full,
    );
    assert!(
        report.has(rule::PLACEHOLDER_UNDECLARED_OVERLOAD),
        "an empty id declares nothing, so `speedo` is undeclared: {report}"
    );
    assert!(!report.has(rule::PLACEHOLDER_UNFILLED), "{report}");
}

/// A `Core` caller whose only binding matches nothing has not shown it has a
/// host-content mechanism — a misspelled id is the likeliest cause — so it must
/// not arm the reporting. Without this, one typo against a document of fifty
/// placeholders yields fifty-one warnings, which is the noise the suppression
/// exists to prevent.
#[test]
fn a_core_binding_that_matches_nothing_does_not_arm_reporting() {
    let doc = Doc::default()
        .named_strings(&["unused-0", "speedo", "tacho"])
        .node(NodeSpec {
            name: "left",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        })
        .node(NodeSpec {
            name: "right",
            placeholder: Some(PlaceholderSpec {
                contribution_id: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        });
    let report = check(doc, &["typo.id"], Profile::Core);
    assert!(
        !report.has(rule::PLACEHOLDER_UNFILLED),
        "a binding matching nothing is no evidence of a host mechanism: {report}"
    );
    assert!(
        report.has(rule::PLACEHOLDER_UNDECLARED_OVERLOAD),
        "the typo itself is still named: {report}"
    );
    assert_eq!(report.diagnostics().len(), 1, "{report}");
}

/// `Report`'s documentation promises the binding diagnostics come in the order
/// the host listed them. Every other test here yields at most one overload, or
/// two carrying the same id, so reversing the loop satisfied all of them.
#[test]
fn binding_diagnostics_keep_the_hosts_order() {
    let report = check(one_placeholder(), &["zulu", "alpha"], Profile::Full);
    let ids: Vec<String> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .map(|d| d.at.to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["<contribution zulu>", "<contribution alpha>"],
        "the host's order, not sorted and not reversed: {report}"
    );
}

/// The `Core` half of the documented partiality behaviour, which the rustdoc
/// asserts and nothing pinned until now: an unreadable `contribution_id` is
/// absent from the set the arming test consults, so a `Core` host whose binding
/// matched only that placeholder does not arm `placeholder.unfilled` — and the
/// unreadable id switches the overload rule off as well, so the report is empty.
///
/// The same document on `Full` reports the readable placeholder, which is what
/// distinguishes "suppressed by the arming test" from "suppressed by anything
/// else".
#[test]
fn on_core_an_unreadable_id_can_suppress_the_unfilled_rule_too() {
    let doc = || {
        Doc::default()
            .named_strings(&["unused-0", "unused-1", "tacho"])
            .node(NodeSpec {
                name: "corrupt-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(99),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .node(NodeSpec {
                name: "gauge-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(2),
                    ..Default::default()
                }),
                ..Default::default()
            })
    };
    // `hud` is in fact the corrupt placeholder's id, which this gate cannot read.
    let core = check(doc(), &["hud"], Profile::Core);
    assert!(
        core.is_empty(),
        "the arming set cannot contain an id this gate could not read: {core}"
    );

    let full = check(doc(), &["hud"], Profile::Full);
    assert!(
        full.has(rule::PLACEHOLDER_UNFILLED),
        "on Full the readable placeholder is still judged: {full}"
    );
}

/// An unreadable `fragment_ref` must NOT mark the declaration set incomplete:
/// the contribution id beside it was read fine, so the overload rule still
/// knows what the document declares.
///
/// The symmetric hole to the one rounds 5-9 fought over on the id side, and it
/// was open until now — the only fixture with an unreadable fragment passed no
/// bindings, so the overload loop emitted nothing whatever the flag held, and
/// marking it incomplete left the whole suite green while silencing the rule
/// the record calls the expensive one.
#[test]
fn an_unreadable_fragment_does_not_silence_the_overload_rule() {
    let report = check(
        Doc::default()
            .named_strings(&["unused-0", "speedo", "unused-2"])
            .node(NodeSpec {
                name: "gauge-slot",
                placeholder: Some(PlaceholderSpec {
                    contribution_id: Some(1),
                    fragment_ref: Some(9),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        &["tacho"],
        Profile::Full,
    );
    let d = report
        .find(rule::PLACEHOLDER_UNDECLARED_OVERLOAD)
        .unwrap_or_else(|| panic!("the id was readable, so the rule still runs: {report}"));
    assert_eq!(d.at, Location::Contribution("tacho".to_owned()), "{report}");
}
