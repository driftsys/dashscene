//! Waivers: the strict-mode exception workflow (issue #41).
//!
//! docs/design/architecture.md: a strict (release) build refuses any
//! warning unless a declared waiver records that its degrade is acceptable
//! for one specific target. A waiver is never a global mute, an error is
//! never waivable, and an out-of-scope waiver is itself a named diagnostic
//! (P4 applies to the waiver vocabulary too).

use dashscene_validator::{Construct, Location, NodePath, Profile, Report, Waiver, rule, triage};

/// A LATER-band construct is a warning under either profile — deferred
/// vocabulary with a declared degrade, the only thing a waiver may convert.
fn warning_at(index: u32, path: &'static str) -> Report {
    vec![triage(
        Construct::LayerBlur,
        Profile::Core,
        NodePath::new(index, path),
    )]
    .into_iter()
    .collect()
}

fn node(index: u32, path: &'static str) -> Location {
    Location::Node(NodePath::new(index, path))
}

#[test]
fn a_clean_report_passes_strict_with_no_waivers() {
    let report = Report::default();
    assert!(report.strict(&[]).passes());
}

#[test]
fn a_warning_without_a_waiver_fails_strict() {
    // The strict rule: zero warnings, or an explicit waiver. An unwaived
    // warning blocks a release build even though it does not block a normal
    // one (a normal build gates on `has_errors`).
    let report = warning_at(1, "/card");
    assert!(
        !report.has_errors(),
        "a normal build lets the warning through"
    );

    let outcome = report.strict(&[]);
    assert!(!outcome.passes(), "a strict build refuses the warning");
    assert!(
        outcome
            .blocking()
            .iter()
            .any(|d| d.rule == rule::LAYER_BLUR)
    );
}

#[test]
fn a_waiver_converts_a_specific_warning_in_strict_mode() {
    // The story's acceptance criterion: a waiver entry converts a specific
    // warning in strict mode. With the waiver present the build passes, and
    // the waiver shows up in the audit trail.
    let report = warning_at(1, "/card");
    let waivers = [Waiver::new(
        rule::LAYER_BLUR,
        node(1, "/card"),
        "design review 2026-07: the blur is baked into the exported raster",
    )];

    let outcome = report.strict(&waivers);
    assert!(outcome.passes(), "{outcome}");
    assert!(outcome.blocking().is_empty(), "{outcome}");
    assert_eq!(outcome.applied().len(), 1, "the waiver is on record");
}

#[test]
fn a_waiver_is_not_a_global_mute() {
    // Two identical warnings on two nodes. A waiver names one target, so it
    // suppresses that one and leaves the other blocking — a rule-only waiver
    // would silence both, which is the global mute the design forbids.
    let report: Report = vec![
        triage(
            Construct::LayerBlur,
            Profile::Core,
            NodePath::new(1, "/card"),
        ),
        triage(
            Construct::LayerBlur,
            Profile::Core,
            NodePath::new(2, "/hero"),
        ),
    ]
    .into_iter()
    .collect();

    let waivers = [Waiver::new(rule::LAYER_BLUR, node(1, "/card"), "baked")];

    let outcome = report.strict(&waivers);
    assert!(!outcome.passes(), "the /hero warning is still unwaived");
    assert_eq!(outcome.applied().len(), 1);
    let blocking: Vec<&Location> = outcome.blocking().iter().map(|d| &d.at).collect();
    assert_eq!(blocking, vec![&node(2, "/hero")], "only /hero blocks");
}

#[test]
fn a_waiver_at_the_wrong_target_does_not_apply() {
    // The target must match. A waiver for the same rule at a different node
    // neither suppresses the warning nor counts as applied — it is dead.
    let report = warning_at(1, "/card");
    let waivers = [Waiver::new(
        rule::LAYER_BLUR,
        node(2, "/hero"),
        "wrong node",
    )];

    let outcome = report.strict(&waivers);
    assert!(!outcome.passes());
    assert!(outcome.applied().is_empty());
    assert!(
        outcome
            .waiver_diagnostics()
            .iter()
            .any(|d| d.rule == rule::WAIVER_UNUSED)
    );
}

#[test]
fn an_error_is_never_waivable() {
    // An error blocks the document unconditionally. A waiver that names an
    // error is out of scope: the error stays blocking, and the attempt is
    // itself diagnosed rather than silently ignored (P4).
    let report: Report = vec![triage(
        Construct::NoiseOrTextureEffect,
        Profile::Core,
        NodePath::new(3, "/bg"),
    )]
    .into_iter()
    .collect();

    let waivers = [Waiver::new(
        rule::NOISE_OR_TEXTURE_EFFECT,
        node(3, "/bg"),
        "trying to waive an error",
    )];

    let outcome = report.strict(&waivers);
    assert!(!outcome.passes(), "an error is never waivable");
    assert!(
        outcome
            .blocking()
            .iter()
            .any(|d| d.rule == rule::NOISE_OR_TEXTURE_EFFECT),
        "the error still blocks",
    );
    assert!(
        outcome
            .waiver_diagnostics()
            .iter()
            .any(|d| d.rule == rule::WAIVER_COVERS_AN_ERROR),
    );
    assert!(outcome.applied().is_empty(), "nothing was actually waived");
}

#[test]
fn a_waiver_naming_an_unknown_rule_is_diagnosed() {
    // A waiver's rule id is vocabulary too: naming a rule that does not exist
    // is a typo that would otherwise silently protect nothing. P4 names it.
    let report = warning_at(1, "/card");
    let waivers = [
        Waiver::new(rule::LAYER_BLUR, node(1, "/card"), "valid"),
        Waiver::new("profile.no-such-rule", node(1, "/card"), "typo"),
    ];

    let outcome = report.strict(&waivers);
    assert!(!outcome.passes(), "the unknown-rule waiver is an error");
    let unknown = outcome
        .waiver_diagnostics()
        .iter()
        .find(|d| d.rule == rule::WAIVER_UNKNOWN_RULE)
        .expect("the unknown rule is named");
    assert!(unknown.message.contains("profile.no-such-rule"));
}

#[test]
fn one_waiver_covers_every_identical_finding_at_a_target() {
    // Two identical findings — same rule, same node — are genuinely
    // reachable (e.g. a node with two advanced-blend-mode paints). They carry
    // no discriminating information, so one waiver covers both, and it counts
    // as a single application (C3: cover-all-at-target).
    let report: Report = vec![
        triage(
            Construct::LayerBlur,
            Profile::Core,
            NodePath::new(1, "/card"),
        ),
        triage(
            Construct::LayerBlur,
            Profile::Core,
            NodePath::new(1, "/card"),
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(report.diagnostics().len(), 2, "two identical findings");

    let waivers = [Waiver::new(
        rule::LAYER_BLUR,
        node(1, "/card"),
        "both baked",
    )];

    let outcome = report.strict(&waivers);
    assert!(outcome.passes(), "one waiver covers both:\n{outcome}");
    assert!(outcome.blocking().is_empty());
    assert_eq!(
        outcome.applied().len(),
        1,
        "counted once, not once per finding"
    );
}

#[test]
fn a_duplicate_waiver_is_flagged_redundant_not_double_counted() {
    // Two identical waivers for one finding: the first applies, the second
    // covers nothing new. The second is surfaced as redundant rather than
    // silently counted as a second application (C4).
    let report = warning_at(1, "/card");
    let waivers = [
        Waiver::new(rule::LAYER_BLUR, node(1, "/card"), "baked"),
        Waiver::new(rule::LAYER_BLUR, node(1, "/card"), "baked again"),
    ];

    let outcome = report.strict(&waivers);
    assert!(outcome.passes(), "{outcome}");
    assert_eq!(
        outcome.applied().len(),
        1,
        "the duplicate does not double-count"
    );
    assert!(
        outcome
            .waiver_diagnostics()
            .iter()
            .any(|d| d.rule == rule::WAIVER_REDUNDANT),
        "the duplicate is surfaced:\n{outcome}",
    );
}

#[test]
fn an_unused_waiver_is_surfaced_but_does_not_block() {
    // A dead waiver against a clean report is worth flagging for hygiene, but
    // it is not itself a build failure — it protects nothing and breaks
    // nothing.
    let report = Report::default();
    let waivers = [Waiver::new(rule::LAYER_BLUR, node(1, "/card"), "stale")];

    let outcome = report.strict(&waivers);
    assert!(
        outcome.passes(),
        "an unused waiver is non-blocking:\n{outcome}"
    );
    assert!(
        outcome
            .waiver_diagnostics()
            .iter()
            .any(|d| d.rule == rule::WAIVER_UNUSED),
        "but it is surfaced for the audit",
    );
}
