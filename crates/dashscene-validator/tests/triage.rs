//! The import gate (story #15): docs/specification/04-figma-vocabulary-profile.md's vocabulary triage.
//!
//! P4 — every out-of-profile construct is a named diagnostic, never a
//! silent drop.

use dashscene_validator::{Construct, Location, NodePath, Profile, Report, Severity, rule, triage};

/// Every construct docs/specification/04-figma-vocabulary-profile.md lists, so the exhaustiveness assertions
/// below cannot rot when a variant is added.
const ALL: &[Construct] = &[
    Construct::LayerBlur,
    Construct::AdvancedBlendMode,
    Construct::CornerSmoothing,
    Construct::LuminanceMask,
    Construct::ClipOnRotated,
    Construct::KashidaJustification,
    Construct::NoiseOrTextureEffect,
    Construct::ProgressiveBlur,
    Construct::AnimatedBooleanOp,
    Construct::AnimatedVariableFontAxis,
    Construct::VariableWidthStroke,
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
        // Issue #145: variable-width stroke sits on the REJECT list
        // (`docs/archive/2026-07-14-scope-decisions.md` §8) — no paint entry
        // can express a per-length width, so no profile buys it back.
        (Construct::VariableWidthStroke, rule::VARIABLE_WIDTH_STROKE),
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
    // This is the one place the two profiles disagree. DESIGN §10.1
    // annotated backdrop blur and advanced blend modes "(profile:full)": a
    // lean painter never gets them, so under profile:core there is nothing to
    // degrade to — it is an error. Backdrop blur left that set at story #393,
    // which made it core vocabulary every painter honours
    // (docs/decisions/backdrop-blur-is-core-vocabulary.md), so the advanced
    // blend mode is the last member.
    let construct = Construct::AdvancedBlendMode;
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
fn the_rule_registry_is_unique_and_covers_every_construct() {
    // `rule::ALL` is the vocabulary a waiver may name; a duplicate would let
    // one waiver silently cover two rules, and a gap would make a real rule
    // look unknown to the waiver check.
    let mut ids = rule::ALL.to_vec();
    let len = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), len, "duplicate id in rule::ALL");

    for id in rule::ALL {
        assert!(rule::is_known(id), "{id} is in ALL but is_known says no");
    }
    for &construct in ALL {
        assert!(
            rule::is_known(construct.rule()),
            "{construct:?}'s rule is missing from rule::ALL",
        );
    }

    // The waiver meta-rules are not document/scene diagnostics, so they are
    // deliberately absent from ALL and are not themselves waivable.
    assert!(!rule::is_known(rule::WAIVER_UNKNOWN_RULE));
    assert!(!rule::is_known(rule::WAIVER_COVERS_AN_ERROR));
    assert!(!rule::is_known(rule::WAIVER_UNUSED));
}

/// The registry pin the test above cannot be (issue #1042).
///
/// `the_rule_registry_is_unique_and_covers_every_construct` walks
/// `Construct::ALL`, which is the import gate's vocabulary alone. Every
/// load-gate, index-chain and document-gate rule sits outside it, so an
/// omission from `rule::ALL` was invisible: three baked-vector rules had been
/// absent since story B1 landed and were found by a reviewer rather than by a
/// test, and **nine more were absent when this test was first run** — the five
/// grid rules and the four keyframe rules, all of them raised by
/// `validate_document`.
///
/// An unregistered rule is not cosmetic. `waiver::strict` takes `continue` on
/// `!rule::is_known` before it looks for matches, so a waiver naming one
/// collects `waiver.unknown-rule` — an error that blocks a strict build — and
/// reports the reason wrongly, saying the rule is not real when it is.
///
/// # Why it reads the source
///
/// Rust has no way to enumerate a module's constants, so the alternative to
/// reading the text is a hand-kept list in the test — which is the same list
/// `ALL` already is, and rots the same way. Two things keep the parse from
/// passing quietly over what it cannot read:
///
/// - the member names it reads out of the `ALL` literal are counted against
///   `rule::ALL.len()`, which the compiler produced, so a member the parse
///   dropped is caught rather than silently shrinking the comparison;
/// - **every `pub const` in the module must parse.** [`declared`] refuses an
///   unrecognised declaration rather than skipping it. Skipping was the first
///   shape of this test and it had a hole its own documentation denied: a
///   declaration written in a form the parse did not recognise — `&'static str`
///   for the elided lifetime, say — was dropped from the declared set, and if it
///   was *also* missing from `ALL` it was absent from both sides of the equality
///   and the test passed green over exactly the omission it exists to catch.
///   Set equality is symmetric; it cannot see a row missing from both sets.
///
/// A structural alternative — declaring every rule through one macro that emits
/// the constants and the slice together — would make an omission unrepresentable
/// rather than merely caught. It is recorded in the pull request as the shape to
/// take if this list grows a second failure mode; it was not taken here because
/// it rewrites every declaration and its documentation to close a gap a test
/// closes. **Re-derive the count rather than trusting a figure here** — this
/// comment has carried a stale one twice, at 81 and then at 84, each left behind
/// by a branch that added rules without touching it:
///
///     awk '/^pub mod rule \{/,/^\}$/' src/lib.rs | grep -c '^    pub const .*: &str'
mod registry {
    use dashscene_validator::rule;

    const SOURCE: &str = include_str!("../src/lib.rs");

    /// The four waiver meta-rules. They never appear on a document or scene
    /// diagnostic, so they are deliberately outside `rule::ALL` and are not
    /// themselves waivable — `rule::ALL`'s own documentation says so.
    ///
    /// Every name here is asserted to be a real declaration below, so this list
    /// cannot rot by naming a constant that no longer exists.
    const NOT_IN_ALL: &[&str] = &[
        "WAIVER_UNKNOWN_RULE",
        "WAIVER_COVERS_AN_ERROR",
        "WAIVER_UNUSED",
        "WAIVER_REDUNDANT",
    ];

    /// `pub mod rule`'s body, bounded by its closing brace at column zero.
    ///
    /// The bound matters: `rule` is not the last item in the file. Getting it
    /// wrong widens the declaration set, which the equality below then fails on
    /// rather than absorbing.
    fn module() -> &'static str {
        let start = SOURCE
            .find("pub mod rule {")
            .expect("dashscene-validator declares `pub mod rule`");
        let body = &SOURCE[start..];
        let end = body
            .find("\n}\n")
            .expect("the rule module's closing brace sits at column zero");
        &body[..end]
    }

    /// Every `pub const NAME: &str` the module declares, in source order.
    ///
    /// The name is on the `pub const` line even when the value wraps to the
    /// next one, which two of these declarations do — so this reads names and
    /// never values, and has no line-continuation case to get wrong.
    ///
    /// **It panics on a declaration it cannot read**, rather than skipping one.
    /// That is what makes a missed declaration a failure: a skipped one that is
    /// also missing from `ALL` is missing from both sides of the comparison
    /// below, which set equality cannot see. Exactly two forms are recognised —
    /// a `&str` rule and the `&[&str]` registry itself — so any third form is a
    /// change to this module's shape that this test must be updated for
    /// deliberately.
    fn declared() -> Vec<&'static str> {
        module()
            .lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix("pub const ")?;
                let (name, tail) = rest.split_once(':').unwrap_or_else(|| {
                    panic!("`pub const` declaration this test cannot parse: {line:?}")
                });
                let tail = tail.trim_start();
                assert!(
                    tail.starts_with("&str") || tail.starts_with("&[&str]"),
                    "`pub const {name}` is declared as neither `&str` nor `&[&str]`: {line:?}. \
                     Skipping it would drop it from the declared set, and a rule missing from \
                     both that set and `rule::ALL` passes this test green — which is the hole \
                     this refusal closes"
                );
                // `ALL` itself is the `&[&str]`, so it is recognised above and
                // dropped here: it is the registry, not a rule in it.
                tail.starts_with("&str").then_some(name)
            })
            .collect()
    }

    /// The identifiers listed inside the `rule::ALL` literal, in source order.
    fn members() -> Vec<&'static str> {
        let module = module();
        let start = module
            .find("pub const ALL: &[&str] = &[")
            .expect("the rule registry is a slice literal");
        let body = &module[start..];
        let end = body
            .find("\n    ];")
            .expect("the registry literal closes at the module's indentation");
        body[..end]
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//"))
            .filter_map(|line| line.strip_suffix(','))
            .filter(|name| {
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            })
            .collect()
    }

    #[test]
    fn every_declared_rule_is_registered_in_all() {
        let members = members();
        // The anchor. `rule::ALL.len()` is what the compiler built from this
        // literal, so a member the parse dropped is caught here rather than
        // silently shrinking the set the comparison below runs over.
        assert_eq!(
            members.len(),
            rule::ALL.len(),
            "the parse of the ALL literal disagrees with the compiled slice; \
             read {members:?}"
        );

        let declared = declared();
        let mut sorted = declared.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            declared.len(),
            "a rule constant is declared twice"
        );

        for name in NOT_IN_ALL {
            assert!(
                declared.contains(name),
                "{name} is listed as deliberately outside ALL but is no longer declared"
            );
        }

        let mut expected: Vec<&str> = declared
            .iter()
            .copied()
            .filter(|name| !NOT_IN_ALL.contains(name))
            .collect();
        let mut found = members;
        expected.sort_unstable();
        found.sort_unstable();

        let missing: Vec<&&str> = expected.iter().filter(|n| !found.contains(n)).collect();
        assert!(
            missing.is_empty(),
            "these rules are declared but absent from rule::ALL, so `is_known` calls them \
             unknown and a waiver naming one blocks a strict build: {missing:?}"
        );
        assert_eq!(
            expected, found,
            "rule::ALL lists an identifier the module does not declare as a rule"
        );
    }
}

#[test]
fn diagnostic_display_names_severity_rule_and_path() {
    let rendered = triage(Construct::AdvancedBlendMode, Profile::Core, at(3)).to_string();
    assert!(
        rendered.starts_with("error[profile.advanced-blend-mode] at /screen/card (#3): "),
        "{rendered}"
    );
}

#[test]
fn every_out_of_profile_construct_carries_a_workaround_hint() {
    // The fourth element of docs/archive/2026-07-14-design-1-seed.md §6.1's
    // diagnostic tuple: every out-of-profile construct names a
    // designer-visible workaround (§04 "bake it, slot it, design without
    // it"). It is a rule-keyed derivation, not a struct field — so the ABI
    // mirror dashc owns is untouched (docs/decisions/dashc-wasm-abi.md).
    for &construct in ALL {
        let diagnostic = triage(construct, Profile::Core, at(0));
        let workaround = diagnostic
            .workaround()
            .unwrap_or_else(|| panic!("{construct:?} carries no workaround hint"));
        assert!(!workaround.is_empty(), "{construct:?} workaround is empty");
        // The hint reaches a reader through Display, appended after the message.
        assert!(
            diagnostic.to_string().contains("workaround: "),
            "{construct:?} Display drops its workaround",
        );
    }
}

#[test]
fn a_referential_integrity_rule_carries_no_workaround() {
    // The workaround hint is for design vocabulary the designer can rework.
    // A dangling paint index is a producer bug, not a design choice — there
    // is nothing to bake or slot — so it deliberately answers None.
    assert_eq!(rule::workaround(rule::PAINT_ENTRY_OUT_OF_RANGE), None);
    assert_eq!(rule::workaround(rule::UNKNOWN_ENUM), None);
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
