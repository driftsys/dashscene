//! Waivers: the strict-mode exception workflow (issue #41).
//!
//! docs/design/architecture.md: an `Error` blocks the document; a `Warning`
//! is deferred vocabulary with a declared degrade, which a normal build
//! lets through. A **release build runs strict** — it refuses even a
//! warning, unless a declared waiver records that the degrade is acceptable
//! for one specific target.
//!
//! Three properties the design fixes:
//!
//! - **Never a global mute, but target-complete.** A waiver names a rule id
//!   *and* a [`Location`]; it suppresses that rule at that one target, not
//!   the rule everywhere. A waiver keyed on the rule alone would silence a
//!   whole class of warnings across the document — the opposite of an
//!   auditable exception. When a target carries several *identical* findings
//!   — the same rule at the same location, which is genuinely reachable (two
//!   advanced-blend-mode paints on one node both triage to
//!   `profile.advanced-blend-mode` at that node) — one waiver covers them
//!   all: the findings are indistinguishable, so requiring one waiver each
//!   would be ceremony with no discriminating information to key on.
//! - **Auditable.** Each waiver carries a `reason`, and the check reports
//!   which waivers it applied — so a reviewer can see every exception and
//!   why it was granted. A waiver that covers nothing new because an earlier
//!   waiver already did (a duplicate) is surfaced as `waiver.redundant`
//!   rather than silently counted as a second application.
//! - **The waiver vocabulary is itself validated (P4).** An out-of-scope
//!   waiver — one naming a rule that does not exist, one that tries to waive
//!   an error, one that matches nothing, or one that duplicates another — is
//!   a named diagnostic, never a silent no-op. P4 ("vocabulary is validated,
//!   never discovered") applies to the waiver declarations as much as to the
//!   design vocabulary.

use std::fmt;

use crate::paint::{error, warning};
use crate::{Diagnostic, Location, Report, Severity, rule};

/// A declared exception: a strict build may proceed past the warnings that
/// carry `rule` at `at`, with `reason` on record.
///
/// The (rule, location) pair is the whole point — see the module docs for
/// why a rule-only waiver would be a global mute rather than an exception,
/// and why one waiver covers every identical finding at its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiver {
    /// The rule id this waiver suppresses. A value outside [`rule::ALL`] is
    /// out of scope and is diagnosed.
    pub rule: String,
    /// The target the waiver covers. Matched by equality against a
    /// diagnostic's [`Diagnostic::at`], so a node waiver names the node and
    /// a pooled-entry waiver names the pool index — never "everywhere". Every
    /// warning carrying `rule` at this target is covered.
    pub at: Location,
    /// Why the exception was granted. Recorded, not interpreted — this is
    /// what makes the waiver auditable.
    pub reason: String,
}

impl Waiver {
    pub fn new(rule: impl Into<String>, at: Location, reason: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            at,
            reason: reason.into(),
        }
    }

    /// Whether this waiver covers `diagnostic`: same rule id, same target.
    fn matches(&self, diagnostic: &Diagnostic) -> bool {
        diagnostic.rule == self.rule && diagnostic.at == self.at
    }
}

/// The verdict of a strict-mode check: whether a release build may proceed,
/// what still blocks it, which waivers it used, and the P4 diagnostics about
/// the waiver declarations themselves.
#[derive(Debug, Clone)]
pub struct StrictReport<'a> {
    passes: bool,
    blocking: Vec<&'a Diagnostic>,
    applied: Vec<&'a Waiver>,
    waiver_diagnostics: Vec<Diagnostic>,
}

impl<'a> StrictReport<'a> {
    /// Whether a strict (release) build may proceed: no error remains, every
    /// warning is covered by a valid waiver, and no waiver declaration is
    /// itself an error (an unknown rule, or an attempt to waive an error).
    pub fn passes(&self) -> bool {
        self.passes
    }

    /// The diagnostics that still block a strict build: every error (never
    /// waivable) and every warning no valid waiver covers.
    pub fn blocking(&self) -> &[&'a Diagnostic] {
        &self.blocking
    }

    /// The waivers that suppressed at least one warning — the audit trail of
    /// exceptions actually granted.
    pub fn applied(&self) -> &[&'a Waiver] {
        &self.applied
    }

    /// The P4 diagnostics about the waiver declarations: `waiver.unknown-rule`
    /// and `waiver.covers-an-error` (both errors, both fail the build), and
    /// `waiver.unused` / `waiver.redundant` (warnings — a dead or duplicate
    /// waiver, surfaced for hygiene but not themselves blocking).
    pub fn waiver_diagnostics(&self) -> &[Diagnostic] {
        &self.waiver_diagnostics
    }
}

impl fmt::Display for StrictReport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "strict build {}",
            if self.passes { "passes" } else { "fails" }
        )?;
        for diagnostic in &self.blocking {
            writeln!(f, "  blocking: {diagnostic}")?;
        }
        for waiver in &self.applied {
            writeln!(
                f,
                "  waived: {} at {} ({})",
                waiver.rule, waiver.at, waiver.reason
            )?;
        }
        for diagnostic in &self.waiver_diagnostics {
            writeln!(f, "  waiver issue: {diagnostic}")?;
        }
        Ok(())
    }
}

/// Runs the strict-mode check of `report` against `waivers`. See
/// [`Report::strict`](crate::Report::strict).
pub(crate) fn strict<'a>(report: &'a Report, waivers: &'a [Waiver]) -> StrictReport<'a> {
    let diagnostics = report.diagnostics();
    let mut waived = vec![false; diagnostics.len()];
    let mut applied = Vec::new();
    let mut waiver_diagnostics = Vec::new();

    for waiver in waivers {
        if !rule::is_known(&waiver.rule) {
            waiver_diagnostics.push(error(
                rule::WAIVER_UNKNOWN_RULE,
                &waiver.at,
                format!(
                    "waiver names rule `{}`, which is not a diagnostic rule; a waiver must name a \
                     real rule (dashscene_validator::rule)",
                    waiver.rule
                ),
            ));
            continue;
        }

        let matched: Vec<usize> = diagnostics
            .iter()
            .enumerate()
            .filter(|(_, diagnostic)| waiver.matches(diagnostic))
            .map(|(i, _)| i)
            .collect();

        if matched.is_empty() {
            waiver_diagnostics.push(warning(
                rule::WAIVER_UNUSED,
                &waiver.at,
                format!(
                    "waiver for `{}` at {} matches no diagnostic; it is dead and should be removed",
                    waiver.rule, waiver.at
                ),
            ));
            continue;
        }

        if matched
            .iter()
            .any(|&i| diagnostics[i].severity == Severity::Error)
        {
            // An error blocks the document unconditionally — only a warning
            // is a "declared degrade" a waiver can accept. Waiving an error
            // would let a blocked document ship, so it is refused by name and
            // the error stays in `blocking` below.
            waiver_diagnostics.push(error(
                rule::WAIVER_COVERS_AN_ERROR,
                &waiver.at,
                format!(
                    "waiver for `{}` at {} matches an error; an error blocks the document and is \
                     never waivable — only a warning is",
                    waiver.rule, waiver.at
                ),
            ));
            continue;
        }

        // Every matched warning at this target is covered — one waiver for
        // identical findings (module docs). A waiver whose matches an earlier
        // waiver already covered adds nothing new: it is a duplicate, and is
        // surfaced rather than counted as a second application.
        let newly: Vec<usize> = matched.iter().copied().filter(|&i| !waived[i]).collect();
        if newly.is_empty() {
            waiver_diagnostics.push(warning(
                rule::WAIVER_REDUNDANT,
                &waiver.at,
                format!(
                    "waiver for `{}` at {} duplicates an earlier waiver; it covers nothing new",
                    waiver.rule, waiver.at
                ),
            ));
            continue;
        }

        for i in newly {
            waived[i] = true;
        }
        applied.push(waiver);
    }

    let blocking: Vec<&Diagnostic> = diagnostics
        .iter()
        .enumerate()
        .filter(|(i, diagnostic)| diagnostic.severity == Severity::Error || !waived[*i])
        .map(|(_, diagnostic)| diagnostic)
        .collect();

    let passes = blocking.is_empty()
        && !waiver_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error);

    StrictReport {
        passes,
        blocking,
        applied,
        waiver_diagnostics,
    }
}
