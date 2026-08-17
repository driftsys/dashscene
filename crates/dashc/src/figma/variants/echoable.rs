//! The only place an echoed finding is written or placed.
//!
//! Its own file rather than a `mod` block inside `variants.rs`, so the property
//! it exists for — this is the whole surface on which a finding entering the
//! echo collapse is written — is read off a file boundary rather than off brace
//! depth in the 2,100 lines it guards. `Echoable`'s fields and
//! [`Echoable::at`] are private to it, and the two writers are its only
//! exports that build one.
//!
//! `use super::*` gives it the parent's types, [`Location`] among them, so the
//! boundary is not that this code cannot *see* a location — it is that the two
//! functions writing prose are not handed one. That is a small guarantee, and
//! [`Echoable`] says exactly how small — no guarantee covers the whole class,
//! and debt #1212 records what was tried and why it was not kept.
use std::fmt::Write as _;

use super::*;

/// Everything about a finding except where it sits, and the only shape the echo
/// collapse accepts.
///
/// [`Collapse::report`] decides the collapse by comparing the rule and the
/// message, see [`Self::renders_as`] — so a message naming the node it was
/// found on differs on every copy, the collapse silently stops, and the
/// fifty-one-findings shape debt #1056 removed comes back. Nothing detects
/// that: the surviving finding is still correct, there are simply N of it
/// again.
///
/// **The fields are private to [`echoable`].** The two functions that fill
/// them are exported so a caller can obtain a finding; what a caller cannot
/// do is make one or place one. Those two are the whole compile-checked
/// guarantee (debt #1137):
///
/// > Every finding that enters the collapse was written by
/// > [`interaction_diagnostics`] or [`contended_transition`], and neither is
/// > handed the node's [`Location`].
///
/// **It says nothing about where those functions' other inputs came from,
/// and deliberately so.** Five review rounds each found a wider reading
/// short by one term — the fields were visible, then [`Self::at`] was, then
/// `contended_transition` took caller-chosen text, then `Interactions`'
/// own fields turned out to be reachable from `crate::figma`. That sequence
/// does not terminate, because message inputs are ordinary data and no
/// visibility rule makes data copy-invariant. `contended_transition` taking
/// a member index is still worth having; it is one input made safe, not a
/// property of the module.
///
/// What the collapse actually rests on is listed in `docs/design/dashc.md`
/// and checked by reading, not by the compiler: `read` is echoed verbatim by
/// REST, `source` is chosen by `apply`, `Set::at` locates the set, and
/// `Reach` differs per copy on purpose.
///
/// Measured before it existed: of the nine message shapes
/// [`interaction_diagnostics`] writes, all nine are produced by the test suite
/// and only two — the `NotAMember` omission and the one-frame degrade — had a
/// test that failed when a per-copy token was added. The other seven left all
/// 335 `dashc` tests green, which is what a guard per message shape costs.
pub(super) struct Echoable {
    rule: &'static str,
    severity: Severity,
    message: String,
}

impl Echoable {
    /// Places this finding at `at`, which is the only way to turn one into a
    /// [`Diagnostic`] and is deliberately outside every function that writes
    /// a message.
    fn at(self, at: Location) -> Diagnostic {
        Diagnostic {
            rule: self.rule,
            severity: self.severity,
            at,
            message: self.message,
        }
    }

    /// Whether this finding is the same finding as `other`, which is what the
    /// collapse folds on.
    ///
    /// **The key is the specification's, not this module's.**
    /// `06-dashc-figma-lowering.md` says findings agreeing in "rule, message,
    /// and the layer the reaction was **authored** on … shall be reported
    /// once". The layer is the bucket's `source`; the rule and the message are
    /// here.
    ///
    /// **Severity is deliberately not compared.** Adding it was tried and
    /// reverted. Two copies of one authored reaction can earn
    /// different severities — the follow-up refusal names no landing, so a copy
    /// landing on an absent library earns `Warning` while one landing on no
    /// member earns the policy's, which under `Strict` is `Error` — and
    /// comparing severity reports those twice, which is what that `shall`
    /// forbids. It also changes what `Strict` does with the bytes: a file that
    /// compiled would stop compiling. Which severity the survivor should take
    /// is a real question the specification does not answer, and settling it
    /// means amending that sentence rather than quietly disagreeing with it
    /// (debt #1219).
    fn renders_as(&self, other: &Diagnostic) -> bool {
        // Destructured rather than read field by field, the way
        // `differs_beyond_overrides` is and for the same reason: a field added
        // to `Diagnostic` must not join the key silently. Two are excluded on
        // purpose — `at`, because where a finding sits is what differs between
        // copies and folding them is the point, and `severity`, for the reason
        // above.
        // Both sides destructured, not one: a field added to either type must
        // not join or leave the key silently.
        let Diagnostic {
            rule,
            severity: _,
            at: _,
            message,
        } = other;
        let Self {
            rule: own_rule,
            severity: _,
            message: own_message,
        } = self;
        // The rule is already equal at the only caller, which buckets on it.
        // Comparing it keeps the predicate true to the specification's key, and
        // means dropping the rule from that bucket could not silently fold an
        // `UNSUPPORTED_MOTION` into an `UNSUPPORTED_INTERACTION`.
        own_rule == rule && own_message == message
    }
}

/// Which scope a contention was found at, and the only thing that differs
/// between the two sentences [`contention_sentence`] writes.
///
/// An enum rather than the noun itself: [`contended_transition`] builds an
/// echoed finding, and passing free text into one is what a per-copy token
/// would look like (debt #1137).
pub(super) enum Contention {
    /// Two layers of one instance, whose finding echoes onto every instance of
    /// the member they disagree in and collapses (debt #1056).
    WithinAnInstance,
    /// Two layers reaching the set's own default table, reported once per set
    /// and never collapsed.
    AcrossTheSet,
}

/// The one sentence saying two layers declared different transitions to
/// `member`, and the document carries one per destination.
///
/// Written once for both scopes, and here rather than beside the set-level
/// caller so this file really is every place an echoable message is written. It
/// was two near-copies differing in a single noun, with nothing comparing them,
/// so an edit to either wording left the other stating the old thing.
pub(super) fn contention_sentence(scope: Contention, member: &str) -> String {
    let whose = match scope {
        Contention::WithinAnInstance => "instance",
        Contention::AcrossTheSet => "component set",
    };
    format!(
        "more than one layer of this {whose} declares a CHANGE_TO to \"{member}\" with a \
         different transition, and the document carries one transition per destination, so only \
         one of them lowers",
    )
}

/// Two layers of one instance declaring different transitions to `member`.
///
/// The document carries one transition per destination, so one of them is lost,
/// and naming it is what keeps the widening that created the case from being a
/// silent loss (P4, issue #976).
///
/// It takes the plan and a member **index** rather than the member's name,
/// for the reason [`interaction_diagnostics`] takes no [`Location`]: it
/// interpolates text, so a caller passing its own string could put a
/// per-copy token in the message with no compile error. An index into
/// set-level data means the caller supplies no text at all. The layers that
/// disagree are the master's, so every instance of that member echoes the
/// same contention and the finding collapses (debt #1056).
///
/// Its set-level twin shares the sentence: both go through
/// [`contention_sentence`], which takes a [`Contention`] scope rather than the
/// noun, so the two cannot drift apart and no caller supplies text.
pub(super) fn contended_transition(plan: &Plan<'_>, member: usize) -> Echoable {
    let member = plan.members[member].name.as_str();
    Echoable {
        rule: rule::UNSUPPORTED_MOTION,
        severity: Severity::Warning,
        message: contention_sentence(Contention::WithinAnInstance, member),
    }
}

/// One node's read interactions, as findings the caller places.
///
/// `resolved` runs parallel to `read.switches`: for each one, where it lands
/// ([`Landing`]) and how far it got ([`Reach`]). Both are what `prototype::read`
/// cannot work out on its own, and together they decide what the reader used to
/// guess from a `destinationId`'s presence: whether a refused curve is a degrade
/// or part of an omission (issue #1017), and which of the two omissions a switch
/// that lands nowhere earns (issue #1016). A set that plans does not guarantee a
/// table — `emit` refuses an instance whose geometry disagrees with the member
/// it shows — so `Reach` is what separates a curve that degrades a shipped state
/// change from one that names a loss, and both from a switch nothing else
/// reports.
///
/// Both are per **switch** rather than per node (debt #1064, #1065). One node
/// can declare two switches landing in two different sets, and a layer's switch
/// travels through whichever host belongs to its destination's set, so neither
/// question has a per-node answer.
///
/// **It is handed no [`Location`]**, and that is the guard debt #1137 asked
/// for: the collapse in [`Collapse::report`] is decided by the rendering, so a message
/// shape added here that named the node it was found on would render
/// differently on every copy and silently stop collapsing. See
/// [`Echoable`].
///
/// That removes the node's own location from reach. It does not make every
/// input copy-invariant, and two of them are worth naming.
///
/// **`Reach` is decided per copy**, and it selects between message shapes:
/// `carrying` holds the instances whose table `emit` accepted, so one
/// instance of a member can answer `Reach::Table` while another answers
/// `Reach::Named`. Those two copies render different sentences and do not
/// collapse — correctly, because they report different outcomes, one a
/// degrade of a switch that shipped and one a switch no table carries. The
/// per-instance `UNLOWERABLE_SET` on the refused instance is left
/// uncollapsed for the same reason.
///
/// **`read` is assumed identical on every copy**, and that is Figma's
/// behaviour rather than this crate's. `prototype::read` builds every
/// refusal string from the reaction payload alone, which is checkable here;
/// whether the payload repeats verbatim is not. REST reports a component's
/// interaction on an instance verbatim, and the fixture pins that for a
/// reaction on an instance **root** rather than for a layer inside a master
/// (debt #1067). Copies carrying different reactions would rightly not
/// collapse, so what that assumption carries is this note's reasoning rather
/// than the collapse.
pub(super) fn interaction_diagnostics(
    read: &Interactions,
    resolved: &[(Landing<'_>, Reach)],
    policy: crate::EmitPolicy,
) -> Vec<Echoable> {
    let by_policy = omission_severity(policy);
    let mut out: Vec<Echoable> = read
        .unsupported
        .iter()
        .map(|what| Echoable {
            rule: rule::UNSUPPORTED_INTERACTION,
            severity: by_policy,
            message: format!("{what} is not in the document vocabulary"),
        })
        .collect();

    for (switch, (landing, reach)) in read.switches.iter().zip(resolved) {
        let omission = match landing {
            // The destination is a member of a set one of this node's hosts
            // belongs to, so the switch is expressible — but only a set that
            // lowers a table actually ships it.
            Landing::Set(set) => {
                match set.state {
                    // It ships, so only its curve was ever at risk — unless
                    // this node is not the one that shipped it.
                    SetState::Lowers(_) if matches!(reach, Reach::Table) => None,
                    // No host of this layer belongs to the set at all, and
                    // nothing else in the pass will say so. The layer sits
                    // inside a component of its own, which reaches the screen
                    // independently of this set, so no switch into the set
                    // replaces it — the destination is real and unreachable
                    // from here. Naming it is what keeps the definition
                    // boundary in `hosting` from being a silent drop
                    // (P4): the boundary is right, and its refusal has to be
                    // said out loud.
                    SetState::Lowers(_) if matches!(reach, Reach::Nowhere) => Some((
                        by_policy,
                        format!(
                            "a CHANGE_TO names destination {}, a member of a component set \
                             this layer sits inside — but the layer belongs to a nested \
                             component of its own, which no switch into that set replaces, so \
                             the switch lowers nowhere",
                            switch.destination,
                        ),
                    )),
                    // No switch reaches the document from here, and something
                    // else has already named why: `UNLOWERABLE_SET`, on the
                    // set or on this instance. The curve must not be called a
                    // degrade — "the switch lands in one frame" would claim a
                    // state change no table carries, which is issue #1017's
                    // defect on the neighbouring path — but it must still be
                    // named, because every finding survives one pass (debt
                    // #149) and a curve dropped here would surface for the
                    // first time on the compile after the set is repaired. It
                    // takes the warning that neighbour carries, so the whole
                    // loss stays a degrade end to end.
                    SetState::Lowers(_) | SetState::NamedItsLoss(_) => {
                        if let Some(what) = &switch.refused_motion {
                            // The message names no vehicle. This branch fires
                            // for a set that lowers nothing and for a layer
                            // whose switches reach no table, and an earlier
                            // draft that picked between "this instance" and
                            // "this component set" put the wrong noun on both
                            // — on a member root, which is no instance, and on
                            // a baked child of an instance whose table shipped
                            // perfectly well. What is true in every case is
                            // the thing worth saying.
                            out.push(Echoable {
                                rule: rule::UNSUPPORTED_MOTION,
                                severity: Severity::Warning,
                                message: format!(
                                    "{what}, and no variant table carries the switch it would \
                                     animate, so nothing is left for it to degrade",
                                ),
                            });
                        }
                        continue;
                    }
                    // Nothing named it: a set of fewer than two members has no
                    // alternative state, so a `CHANGE_TO` into it is an
                    // omission like any other and this is the only place that
                    // will say so.
                    SetState::Silent => Some((
                        by_policy,
                        format!(
                            "a CHANGE_TO names destination {}, whose component set has no \
                             second member to switch to, so the switch lowers nowhere",
                            switch.destination,
                        ),
                    )),
                }
            }
            // The file carries a set this node belongs to and the destination
            // is a member of none of them: a `destinationId` the export closure
            // trimmed, or one naming a member of a different set. That is a
            // broken file, and under `Strict` handing over its bytes ships a
            // button whose click does nothing (issue #976).
            Landing::NotAMember => Some((
                by_policy,
                format!(
                    "a CHANGE_TO names destination {}, which is not a member of the component \
                     set this layer belongs to, so the switch lowers nowhere",
                    switch.destination,
                ),
            )),
            // No set, and no missing library either — a plain frame, or an
            // instance of a standalone local `COMPONENT`. There is no variant
            // to switch to and the file is present in full, so this is a
            // broken switch rather than an export that left a library out, and
            // it takes the same severity as one.
            Landing::NoSet => Some((
                by_policy,
                format!(
                    "a CHANGE_TO names destination {}, and this layer belongs to no component \
                     set, so the switch lowers nowhere",
                    switch.destination,
                ),
            )),
            // The node shows a component the file does not contain — the
            // ordinary shape of an instance of a published-library component
            // set, which every real Figma file is full of. A **warning in
            // both policies**
            // (issue #1016), on the neighbouring severity rather than on a new
            // judgement: `UNLOWERABLE_SET` is a warning in both policies for a
            // set the file *carries* and this pass cannot express, because
            // refusing would withhold a document that renders correctly. A set
            // the export never included loses the same variant table and its
            // baked subtree paints the same, so making the absent set the error
            // and the present-but-unlowerable one the warning inverts the two.
            // `figma-component-lowering.md` ("Severity") carries the argument
            // in full, including what the closure independently decided about
            // the same population.
            //
            // What stays at the policy's severity is a construct with no
            // lowering **anywhere** — a refused trigger, action or navigation
            // above. Those are vocabulary gaps whatever file carries the set;
            // this one is a set the export did not include.
            Landing::LibraryAbsent => Some((
                Severity::Warning,
                format!(
                    "a CHANGE_TO names destination {}, and this file does not contain \
                     the component whose instance this layer belongs to, so the switch \
                     lowers nowhere",
                    switch.destination,
                ),
            )),
        };

        match omission {
            None => {
                if let Some(what) = &switch.refused_motion {
                    out.push(Echoable {
                        rule: rule::UNSUPPORTED_MOTION,
                        severity: Severity::Warning,
                        message: format!("{what}; the switch lands in one frame"),
                    });
                }
            }
            Some((earned, message)) => {
                out.push(Echoable {
                    rule: rule::UNSUPPORTED_INTERACTION,
                    severity: earned,
                    message,
                });
                // The curve is part of that omission rather than a degrade:
                // "the switch lands in one frame" claims a state change the
                // document does not carry (issue #1017). It is still named,
                // because every finding survives one pass (debt #149).
                //
                // It takes the omission's severity rather than the policy's,
                // and the reading that says otherwise is worth answering: a
                // `DISSOLVE` has no `dashcue` spelling whatever file carries
                // the set, so it looks like a vocabulary gap that should be an
                // error under `Strict`. What makes it not one here is that
                // there is no switch for it to animate — the curve's loss
                // costs nothing beyond the omission already named. Giving it
                // the policy's severity would also make a library instance
                // withhold the document again whenever its switch declared a
                // curve, which is the whole of what issue #1016 is about.
                if let Some(what) = &switch.refused_motion {
                    out.push(Echoable {
                        rule: rule::UNSUPPORTED_INTERACTION,
                        severity: earned,
                        message: format!("{what}, and the switch it would animate lowers nowhere"),
                    });
                }
            }
        }
    }

    out
}

/// The findings this pass has produced, and the echo collapse's state over
/// them.
///
/// One type rather than two locals because the buckets hold **indices into
/// `diagnostics`** (debt #1142). The replaced key held the message itself
/// and so carried no coupling; these two are meaningful only as a pair, and
/// passing them separately would leave that a comment rather than a fact.
/// Nothing outside can hold one without the other, or reach the bucket
/// representation — [`Self::finish`] is what applies the copy counts.
pub(super) struct Collapse<'a> {
    diagnostics: Vec<Diagnostic>,
    /// For each authored layer and rule, one entry per distinct **rendering**
    /// — see [`Echoable::renders_as`] — holding where the first copy landed
    /// in `diagnostics` and how many copies there were.
    ///
    /// The message is deliberately not part of the key: keying on it cloned
    /// the rendered prose for every diagnostic, the overwhelming majority of
    /// which never collapse. Each bucket's first `push` allocates, so a file
    /// whose findings are all singletons makes the same number of
    /// allocations, each a short vector of indices instead of a copy of the
    /// prose. Exact byte counts are not quoted here: they depend on `Vec`'s
    /// growth policy and the target's pointer width, and `dashc` is built
    /// for `wasm32` as well.
    echoed: BTreeMap<(&'a str, &'static str), Vec<(usize, usize)>>,
}

impl<'a> Collapse<'a> {
    pub(super) fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            echoed: BTreeMap::new(),
        }
    }

    /// A finding that never collapses — one whose multiplicity *is* the
    /// finding, like a refused instance's lost table.
    pub(super) fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Several of those at once, in order.
    pub(super) fn extend(&mut self, findings: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(findings);
    }

    /// Reports `finding` at `at`, or folds it into an identical one already
    /// reported for the same authored layer (debt #1056).
    ///
    /// Every echoed finding goes through here, the per-instance contention
    /// warning included: a diagnostic pushed straight on would keep the
    /// multiplicity this exists to remove.
    ///
    /// It takes an [`Echoable`] rather than a [`Diagnostic`] so the collapse has
    /// one entrance, and an `Echoable` can only be built by this module's two
    /// message writers, neither of which is handed the node's [`Location`]
    /// (debt #1137). That is a claim about **those two functions**, not about
    /// the module: this one takes a `Location` closure, [`Self::push`] and
    /// [`Self::extend`] take finished [`Diagnostic`]s, and the tests build
    /// `Echoable`s from written sentences.
    ///
    /// `at` is a **closure**, not a [`Location`]: a copy that folds never
    /// reaches a [`Diagnostic`], and a `Location` owns its path, so passing
    /// one — by value or by reference — cloned that path for every copy only
    /// to drop it. Each of the fifty instances the collapse exists for is
    /// its own node, so that was 49 clones of the 50. Called only on the
    /// branch that keeps the finding (debt #1142).
    pub(super) fn report(
        &mut self,
        source: &'a str,
        finding: Echoable,
        at: impl FnOnce() -> Location,
    ) {
        let bucket = self.echoed.entry((source, finding.rule)).or_default();
        // A scan of one bucket where the replaced map was a search over the
        // whole document, bounded by how many different things one authored
        // layer says under one rule — one, on the fifty-instance file the
        // collapse exists for.
        let diagnostics = &mut self.diagnostics;
        match bucket
            .iter_mut()
            .find(|(index, _)| finding.renders_as(&diagnostics[*index]))
        {
            Some((_, copies)) => *copies += 1,
            None => {
                let here = at();
                bucket.push((diagnostics.len(), 1));
                diagnostics.push(finding.at(here));
            }
        }
    }

    /// The findings, each survivor of a collapse naming how many copies it
    /// stands for.
    ///
    /// What the copies cost the reader is said on the one finding that
    /// survived them rather than by repeating it. The count is deliberately
    /// on the message and not on the [`Location`]: which node a finding sits
    /// at is `dashscene-validator`'s vocabulary, and one producer's echo is
    /// not a reason to widen it.
    pub(super) fn finish(mut self) -> Vec<Diagnostic> {
        for (index, copies) in self.echoed.into_values().flatten() {
            if copies > 1 {
                // "further copies of the same reaction", not "of this
                // layer": the usual source is one layer echoed onto many
                // instances, but two identical reactions on one node fold
                // together here too, and naming a layer would be wrong for
                // that one.
                // `write!` rather than `push_str(&format!(..))`: the suffix
                // goes straight into the message's own buffer instead of a
                // temporary that is copied in and dropped.
                let _ = write!(
                    self.diagnostics[index].message,
                    " (and {} further {} of the same reaction, not listed separately)",
                    copies - 1,
                    if copies == 2 { "copy" } else { "copies" },
                );
            }
        }
        self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The collapse folds on the specification's key — rule, message and the
    /// authored layer — and on **nothing else**.
    ///
    /// `06-dashc-figma-lowering.md` says findings agreeing in those three "shall
    /// be reported once". Severity is the term most likely to be added back:
    /// two copies of one authored reaction can earn different severities, and
    /// comparing severity reports them twice, against that `shall`. Nothing
    /// else fails if it returns — no document in the corpus or the suite
    /// renders one sentence at two severities — so this test is what stands
    /// between the code and the specification (debt #1219).
    ///
    /// It drives [`Collapse::report`] rather than [`Echoable::renders_as`]
    /// because the predicate is one line inside its only caller, and a test on
    /// the helper stays green while the caller stops using it.
    #[test]
    fn two_copies_differing_only_in_severity_are_still_one_finding() {
        let sentence = "prototype easing GENTLE (…), and the switch it would animate lowers \
                        nowhere";
        let finding = |severity| Echoable {
            rule: rule::UNSUPPORTED_INTERACTION,
            severity,
            message: sentence.to_string(),
        };
        let somewhere = || {
            Location::Node(NodePath {
                index: 0,
                path: "/card (1:14)/bar".to_string(),
            })
        };

        let mut collapse = Collapse::new();
        collapse.report("1:3", finding(Severity::Warning), somewhere);
        collapse.report("1:3", finding(Severity::Error), somewhere);
        let folded = collapse.finish();
        assert_eq!(
            folded.len(),
            1,
            "same rule, same message, same authored layer: the specification says one finding, \
             whatever severity each copy earned — {folded:?}",
        );
        assert_eq!(
            folded[0].severity,
            Severity::Warning,
            "the survivor keeps the **first** copy's severity. The specification does not say \
             it should, and debt #1219 carries the question; this pins what it does today so \
             that answering it is a visible change",
        );
        assert!(
            folded[0]
                .message
                .ends_with(" (and 1 further copy of the same reaction, not listed separately)"),
            "and `finish` names what the collapse cost the reader: {:?}",
            folded[0].message,
        );

        // The other half of the same rule: two different things to say about
        // one layer stay two findings (debt #149).
        let mut collapse = Collapse::new();
        collapse.report("1:3", finding(Severity::Warning), somewhere);
        collapse.report(
            "1:3",
            Echoable {
                rule: rule::UNSUPPORTED_INTERACTION,
                severity: Severity::Warning,
                message: "a different refusal entirely".to_string(),
            },
            somewhere,
        );
        assert_eq!(
            collapse.finish().len(),
            2,
            "two different things to say about one layer are two findings",
        );
    }
}
