//! The CSS Fonts Level 4 font-matching algorithm, weight step (§5.2) —
//! the rule that picks one face out of a family when the requested CSS
//! weight has no exact face (story #368).
//!
//! The rule is adopted verbatim rather than a nearest-weight rule because
//! it is fully specified at every requested weight: with faces {400, 600,
//! 700} a nearest rule is ambiguous at 500 (equidistant from 400 and 600)
//! and resolves by an arbitrary tie-break, while this rule resolves 500 to
//! 400 by specification — the same answer the designer's own browser and
//! design tool give, so a substitution matches what they saw.
//!
//! Matching is **non-fatal**: a family that offers no face at or near the
//! requested weight still returns its best candidate rather than failing.
//! Two committed fixtures (`lowering-baseline.json`,
//! `lowering-variant-topology.json`) request weight 700 against
//! single-face cascades, and their goldens depend on that request
//! resolving to the one available face instead of erroring.

/// Picks the face index within one family for `requested`, per CSS Fonts 4
/// §5.2. `weights` are the family's face weights in declared order and must
/// be non-empty; ties (two faces at one weight) resolve to the first
/// declared. The returned index is always valid — the rule's phases cover
/// every weight, and index 0 is the final fallback.
///
/// The three phases, exactly as specified:
///
/// - requested inclusively within 400..=500: weights at or above the
///   request ascending, but no higher than 500; then weights below the
///   request descending; then weights above 500 ascending.
/// - requested below 400: weights at or below the request descending;
///   then weights above it ascending.
/// - requested above 500: weights at or above the request ascending;
///   then weights below it descending.
///
/// An exact match always wins: it is the first candidate of the first
/// phase in each of the three branches.
pub(crate) fn match_weight(weights: &[u16], requested: u16) -> usize {
    debug_assert!(!weights.is_empty(), "a family has at least one face");
    // The nearest weight satisfying `keep`, scanning toward the request:
    // ascending takes the smallest candidate, descending the largest. A
    // strict comparison keeps the first-declared face on a tie.
    let nearest = |keep: &dyn Fn(u16) -> bool, ascending: bool| -> Option<usize> {
        weights
            .iter()
            .enumerate()
            .filter(|&(_, &w)| keep(w))
            .reduce(|best, next| {
                let closer = if ascending {
                    *next.1 < *best.1
                } else {
                    *next.1 > *best.1
                };
                if closer { next } else { best }
            })
            .map(|(i, _)| i)
    };
    let phases: [(&dyn Fn(u16) -> bool, bool); 3] = if (400..=500).contains(&requested) {
        [
            (&|w| w >= requested && w <= 500, true),
            (&|w| w < requested, false),
            (&|w| w > 500, true),
        ]
    } else if requested < 400 {
        [
            (&|w| w <= requested, false),
            (&|w| w > requested, true),
            (&|_| false, true),
        ]
    } else {
        [
            (&|w| w >= requested, true),
            (&|w| w < requested, false),
            (&|_| false, true),
        ]
    };
    for (keep, ascending) in phases {
        if let Some(i) = nearest(keep, ascending) {
            return i;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolved *weight* (not index) for a family, for readable cases.
    fn resolve(weights: &[u16], requested: u16) -> u16 {
        weights[match_weight(weights, requested)]
    }

    #[test]
    fn an_exact_face_always_wins() {
        let corpus = [400u16, 600, 700];
        for &w in &corpus {
            assert_eq!(resolve(&corpus, w), w, "exact match for {w}");
        }
    }

    #[test]
    fn the_corpus_resolves_every_css_weight() {
        // The committed corpus after story #368: Regular, SemiBold, Bold.
        let corpus = [400u16, 600, 700];
        // Below 400 descends first, then ascends — nothing is below 400,
        // so every light weight lands on Regular.
        for w in [100u16, 200, 300] {
            assert_eq!(resolve(&corpus, w), 400, "weight {w}");
        }
        // 400..=500 tries at-or-above the request up to 500 first. 400 is
        // exact; 500 finds nothing in 500..=500 and descends to 400 — the
        // specified answer, and why story #368 adds no Medium face.
        assert_eq!(resolve(&corpus, 400), 400);
        assert_eq!(resolve(&corpus, 500), 400);
        // Above 500 ascends from the request.
        assert_eq!(resolve(&corpus, 600), 600);
        assert_eq!(resolve(&corpus, 700), 700);
        assert_eq!(resolve(&corpus, 800), 700);
        assert_eq!(resolve(&corpus, 900), 700);
    }

    #[test]
    fn a_single_face_family_absorbs_every_request() {
        // The non-fatal constraint: `lowering-baseline.json` and
        // `lowering-variant-topology.json` request weight 700 against
        // single-font cascades, and their committed goldens depend on that
        // resolving to the one face rather than failing.
        for w in [100u16, 400, 500, 700, 900] {
            assert_eq!(resolve(&[400], w), 400, "weight {w} against Regular only");
        }
        assert_eq!(resolve(&[700], 400), 700);
    }

    #[test]
    fn the_400_to_500_band_prefers_500_over_descending() {
        // The band's distinguishing rule: from 400, 500 is reached before
        // anything below 400 — a nearest rule would also pick 500 here,
        // but the two diverge in the next case.
        assert_eq!(resolve(&[300, 500], 400), 500);
        // From 450, 500 is 50 away and 400 is 50 away: a nearest rule ties,
        // this rule takes the at-or-above candidate within the band.
        assert_eq!(resolve(&[400, 500], 450), 500);
    }

    #[test]
    fn above_500_ascends_before_descending() {
        // 600 requested against {400, 900}: ascending first reaches 900,
        // even though 400 is nearer in absolute distance.
        assert_eq!(resolve(&[400, 900], 600), 900);
    }

    #[test]
    fn below_400_descends_before_ascending() {
        // 300 requested against {100, 700}: descending first reaches 100,
        // even though 700 is not considered until the descent is exhausted.
        assert_eq!(resolve(&[100, 700], 300), 100);
        // Nothing at or below 300, so it ascends to the lightest above.
        assert_eq!(resolve(&[600, 400], 300), 400);
    }

    #[test]
    fn a_tie_takes_the_first_declared_face() {
        // Two faces at one weight: the earlier declaration wins, so the
        // flattened slot order is a caller's choice rather than arbitrary.
        assert_eq!(match_weight(&[700, 700], 700), 0);
        assert_eq!(match_weight(&[400, 400, 400], 900), 0);
    }
}
