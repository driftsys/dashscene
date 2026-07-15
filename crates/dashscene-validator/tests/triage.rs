//! The import gate (story #15): docs/specification/04-figma-vocabulary-profile.md's vocabulary triage.
//!
//! P4 — every out-of-profile construct is a named diagnostic, never a
//! silent drop.

use dashscene_validator::{Construct, Location, NodePath, Profile, Report, Severity, rule, triage};

/// Every construct docs/specification/04-figma-vocabulary-profile.md lists, so the exhaustiveness assertions
/// below cannot rot when a variant is added.
const ALL: &[Construct] = &[
    Construct::LayerBlur,
    Construct::BackdropBlur,
    Construct::AdvancedBlendMode,
    Construct::CornerSmoothing,
    Construct::LuminanceMask,
    Construct::ClipOnRotated,
    Construct::KashidaJustification,
    Construct::NoiseOrTextureEffect,
    Construct::ProgressiveBlur,
    Construct::AnimatedBooleanOp,
    Construct::AnimatedVariableFontAxis,
];

fn at(index: u32) -> NodePath {
    NodePath::new(index, "/screen/card")
}

#[test]
fn every_construct_produces_a_named_diagnostic_in_every_profile() {
    // The acceptance criterion: nothing is silently dropped. A construct
    // outside the NOW band always names a rule and always carries a
    // message, whichever profile is targeted.
    for &profile in &[Profile::Core, Profile::Full] {
        for &construct in ALL {
            let diagnostic = triage(construct, profile, at(3));
            assert!(
                !diagnostic.rule.is_empty(),
                "{construct:?} produced an unnamed diagnostic"
            );
            assert!(
                !diagnostic.message.is_empty(),
                "{construct:?} produced an empty message"
            );
            assert_eq!(
                diagnostic.at,
                Location::Node(at(3)),
                "{construct:?} lost its node path"
            );
        }
    }
}

#[test]
fn reject_band_is_an_error_in_both_profiles() {
    // docs/specification/04-figma-vocabulary-profile.md REJECT: "each with a documented workaround (bake it,
    // slot it, design without it)". No profile buys these back.
    let rejected = [
        (
            Construct::NoiseOrTextureEffect,
            rule::NOISE_OR_TEXTURE_EFFECT,
        ),
        (Construct::ProgressiveBlur, rule::PROGRESSIVE_BLUR),
        (Construct::AnimatedBooleanOp, rule::ANIMATED_BOOLEAN_OP),
        (
            Construct::AnimatedVariableFontAxis,
            rule::ANIMATED_VARIABLE_FONT_AXIS,
        ),
    ];
    for (construct, expected_rule) in rejected {
        for profile in [Profile::Core, Profile::Full] {
            let diagnostic = triage(construct, profile, at(0));
            assert_eq!(diagnostic.rule, expected_rule);
            assert_eq!(
                diagnostic.severity,
                Severity::Error,
                "{construct:?} must block the document under profile:{profile:?}"
            );
        }
    }
}

#[test]
fn later_band_is_a_warning_in_both_profiles() {
    // docs/design/architecture.md: a warning is deferred vocabulary with a declared degrade.
    // These four are not profile-annotated, so both profiles degrade them.
    let deferred = [
        (Construct::LayerBlur, rule::LAYER_BLUR),
        (Construct::CornerSmoothing, rule::CORNER_SMOOTHING),
        (Construct::LuminanceMask, rule::LUMINANCE_MASK),
        (Construct::ClipOnRotated, rule::CLIP_ON_ROTATED),
        (Construct::KashidaJustification, rule::KASHIDA_JUSTIFICATION),
    ];
    for (construct, expected_rule) in deferred {
        for profile in [Profile::Core, Profile::Full] {
            let diagnostic = triage(construct, profile, at(0));
            assert_eq!(diagnostic.rule, expected_rule);
            assert_eq!(
                diagnostic.severity,
                Severity::Warning,
                "{construct:?} degrades rather than blocking under profile:{profile:?}"
            );
        }
    }
}

#[test]
fn profile_full_only_constructs_block_a_core_target() {
    // This is the one place the two profiles disagree at v0.3. DESIGN
    // §10.1 annotates backdrop blur and advanced blend modes
    // "(profile:full)": a lean painter never gets them, so under
    // profile:core there is nothing to degrade to — it is an error.
    for construct in [Construct::BackdropBlur, Construct::AdvancedBlendMode] {
        assert_eq!(
            triage(construct, Profile::Core, at(1)).severity,
            Severity::Error,
            "{construct:?} is profile:full-only, so profile:core must reject it"
        );
        assert_eq!(
            triage(construct, Profile::Full, at(1)).severity,
            Severity::Warning,
            "{construct:?} is deferred vocabulary, not rejected, under profile:full"
        );
    }
}

#[test]
fn rule_ids_are_stable_and_unique() {
    // Rule ids are the contract a designer greps for and a waiver file
    // (v0.7, issue #41) will key on. Two constructs sharing one id would
    // make a waiver silently cover both.
    let ids: Vec<&str> = ALL.iter().map(|c| c.rule()).collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "duplicate rule id among {ids:?}");
}

#[test]
fn diagnostic_display_names_severity_rule_and_path() {
    let rendered = triage(Construct::BackdropBlur, Profile::Core, at(3)).to_string();
    assert!(
        rendered.starts_with("error[profile.backdrop-blur] at /screen/card (#3): "),
        "{rendered}"
    );
}

#[test]
fn a_producer_assembles_a_report_from_its_own_diagnostics() {
    // The import gate hands back bare Diagnostics. A producer (dashc) must be
    // able to turn its own findings into the one Report type both gates use,
    // or P4's "never a silent drop" has no channel to speak on.
    let found = vec![
        triage(Construct::LayerBlur, Profile::Core, NodePath::new(1, "/a")),
        triage(
            Construct::NoiseOrTextureEffect,
            Profile::Core,
            NodePath::new(2, "/b"),
        ),
    ];

    let report: Report = found.into_iter().collect();

    assert_eq!(report.diagnostics().len(), 2);
    assert!(report.has_errors(), "the noise effect is an error");
    assert!(report.has(rule::LAYER_BLUR));
    assert!(report.has(rule::NOISE_OR_TEXTURE_EFFECT));
}

#[test]
fn a_report_merges_a_second_gates_diagnostics() {
    // compile_figma merges the import gate's findings with the load gate's
    // Report before deciding whether to emit.
    let mut report: Report = vec![triage(
        Construct::CornerSmoothing,
        Profile::Core,
        NodePath::new(0, "/a"),
    )]
    .into_iter()
    .collect();

    assert!(!report.has_errors(), "corner smoothing only warns");

    report.extend([triage(
        Construct::ProgressiveBlur,
        Profile::Core,
        NodePath::new(1, "/b"),
    )]);

    assert_eq!(report.diagnostics().len(), 2);
    assert!(report.has_errors(), "progressive blur is an error");
}
