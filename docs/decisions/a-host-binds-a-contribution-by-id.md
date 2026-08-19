# A host binds a contribution by id, and two of four states warn

    status   accepted (story #1127, 2026-08-19)
    scope    crates/dashscene-validator — validate_contributions, its two
             rules, and Location::Contribution
    source   issue #851's "two things from the discussion that are recorded
             nowhere else", the first of them

## Context

Story #1126 built the placeholder surface: a node carrying `table Placeholder`
is a declared placeholder, and its presence is the predicate
([`a-placeholder-is-a-table-and-declares-its-measure-size.md`](a-placeholder-is-a-table-and-declares-its-measure-size.md)).
Nothing yet compares that surface against what a host actually fills, so two of
the four states it makes expressible go unreported. Issue #851 names the second
of them, the undeclared overload, as the one nothing else catches and the
expensive one: its cost is a designer maintaining artwork nobody sees, paid
continuously rather than at load.

Three things had to be settled to build the check, and each changes what the
code does.

## Decision 1 — the binding key is the contribution id, not the node

`validate_contributions` takes `bound: &[&str]`, the contribution ids the host
declares it fills.

`Placeholder.contribution_id` is documented in `dashbuf.fbs` as "the id a
runtime producer binds a contribution against", so it is the key the schema
already added for this. The alternative — a host naming the node it covers —
matches issue #851's wording ("host code covering a node") more literally, but
there is no mechanism for it: `dashscene-core` carries no name-to-`NodeId`
lookup at all, only `Arena::name`, which is index-to-name
(`crates/dashscene-core/src/arena.rs`). A node-addressed host has nothing to
resolve a name through, while an id-addressed one has exactly the field the
schema declares.

The consequence for the undeclared overload is that it reports a bound id no
placeholder declares, rather than a node. That is the same defect issue #851
describes — a designer who removed a placeholder marking, or never added one,
while host code still covers that region — reported at the half that is wrong.

## Decision 2 — two of the four states warn, and which two

| node is a placeholder | a host contribution binds it | verdict                                    |
| --------------------- | ---------------------------- | ------------------------------------------ |
| yes                   | yes                          | filled — silent                            |
| yes                   | no                           | `placeholder.unfilled` — see below         |
| no                    | yes                          | `placeholder.undeclared-overload`, on both |
| no                    | no                           | ordinary — silent                          |

`placeholder.unfilled` is suppressed on a `Core` target **that binds nothing a
host contribution can fill**, which is every ordinary `Core` build. A lean
painter has no host-content mechanism, so row 2 is the correct state there, and
one warning per placeholder in every build is how a diagnostic channel stops
being read — issue #851's own constraint, and the reason it gives.

**Why not the profile alone.** A `Core` host that binds one of two declared
placeholders would otherwise receive an **empty report**: the bound one is
declared, so it raises no overload, and the unbound one is suppressed. Such a
caller has contradicted the premise the suppression rests on, and is told what
it left unfilled rather than told nothing.

**Why "a contribution can fill" and not "binds anything".** A binding that
matches nothing in the document is the weakest possible evidence of a
host-content mechanism — a misspelled id is the likelier cause — and admitting
it would raise one warning per placeholder in the document from a single typo,
which is the outcome the suppression exists to prevent. Measured on a
three-placeholder document, one unmatched `Core` binding produced one warning
per placeholder plus the overload; on a fifty-placeholder document that is
fifty-one. The unmatched binding is still named by
`placeholder.undeclared-overload`, so nothing is lost by declining to treat it
as evidence.

**And not "binds a declared id" either.** A placeholder a `fragment_ref` fills
is declared, and no host contribution is ever owed for its id, so a binding
matching only that one is evidence _against_ a host-content mechanism rather
than for it. The test is against the ids a host contribution can fill — the same
set the unfilled rule iterates — which is narrower than what the document
declares. This paragraph is the third statement of this rule on this branch: the
first said "binds anything", the second "binds a declared id", and each was
corrected only after the code had already moved.

`placeholder.undeclared-overload` is raised on both profiles. A `Core` target is
not expected to bind at all, so a binding there is wrong for two independent
reasons.

Both are warnings rather than errors, because neither says the document is
malformed: an unfilled placeholder is an unfinished migration, and an unmatched
binding costs a designer's time rather than a frame. Neither needs a new
concept: `Warning` is already "deferred vocabulary with a declared degrade", a
release build refuses either through `Report::strict`, and a target that accepts
one declares a waiver.

**`interim_fill` is not the argument.** It is what will let an unfilled
placeholder draw something, and no painter reads one yet — neither
`dashscene-skia` nor `dashscene-gpu` names the field. Today an unfilled
placeholder's **contribution** draws nothing, which argues for the warning
rather than against it. The node itself is an ordinary `Node` and still draws
its own paint: neither painter names `interim_fill`, so nothing draws a
placeholder differently. The commit path is not placeholder-agnostic, though —
`Txn::set_prop` has a `Prop::Placeholder` arm that asserts `declared_size` is
finite and non-negative, and `prop_class` classes it as measured-size intent.
(This sentence named `Arena::apply` for six review rounds, a function that
exists nowhere in `dashscene-core`; it was introduced by the round that
corrected a different false claim in the same paragraph.)

## Decision 3 — two placeholder shapes are not row 2

Neither raises `placeholder.unfilled`, and the reason differs.

- **`contribution_id` at `NO_CONTRIBUTION`** is "a box that reserves space
  without naming a binding" (`dashbuf.fbs`) — a shape the schema permits. No
  host binding could ever match it, so the warning would be permanent and no
  host action would clear it. That is the same noise Decision 2 keeps out of
  `Core` builds.
- **`fragment_ref` set at all** declares a fill route that is not a host
  contribution. The schema's own reading of the sentinel is that `NO_FRAGMENT`
  means "the contribution is drawn by the host rather than streamed", so a
  placeholder that names a fragment is owed no host contribution.

  **Set is the whole predicate.** Whether the index resolves — it may be past
  the pool, or name an empty entry — is a question about whether the document is
  well-formed, and this gate asks only what each side _declares_. An unreadable
  pool entry is the load gate's to name (debt #1273); reporting it through a
  rule about host bindings would print a message naming a contribution that does
  not exist, with a workaround hint a host cannot act on.

  Four review rounds moved this line, each arguing from the case rather than
  from the question the gate asks. Two facts settled it: the rule that treated
  an unresolvable fragment as "no route" fired only on placeholders that _also_
  named a contribution id, so a fragment that was the sole declared route stayed
  silent anyway — the case its rationale was written for was the one it missed;
  and `placeholder.unfilled`'s message and hint are about an id, not a subtree.

The two differ in what they leave behind. A `fragment_ref` placeholder still
**declares** its id, so a host binding naming it is not an overload. A
`NO_CONTRIBUTION` placeholder has no id to declare and shields nothing: a host
binding against it is an overload exactly as it would be against a document
carrying no placeholders at all.

## Decision 3a — an empty pool entry declares nothing, and is still readable

A `contribution_id` in range that resolves to an **empty** pool string declares
nothing, exactly as `NO_CONTRIBUTION` does — and, unlike an out-of-range index,
it leaves the declaration set **complete**. The gate read the name; it is known,
not unknown.

The distinction is load-bearing rather than pedantic. Marking the set incomplete
here would switch `placeholder.undeclared-overload` off for the whole document
on the strength of a name that is known, and the load gate raises nothing for an
in-range index, so no caller could learn the rule had been switched off. That is
fail-open on the rule issue #851 calls the expensive one.

An earlier round of this branch treated the two alike and did exactly that.
Naming the empty entry itself belongs to neither gate today, which is debt
#1273.

## Decision 4 — the overload's location carries the id, not a position

`Location::Contribution(String)` is the first variant of that enum that does not
name a document surface, and it has to be: the subject of the diagnostic is a
binding the document does not contain, which is what the diagnostic reports.
`Location`'s own documentation forbids the alternative of wrapping it in
`Location::Node`, where it would resolve as a DFS index and land a reader on an
unrelated layer.

It carries the id rather than an index into `bound`, which is what it carried
first. `Waiver::matches` is
`diagnostic.rule == self.rule && diagnostic.at ==
self.at`, so a positional
location binds a waiver to a position in the host's argument list: reordering
`bound` moves the waiver onto a different binding, suppressing one overload
while the waiver's own recorded reason names the other, and
`StrictReport::applied` reports it as granted. Every other variant indexes a
pool the document owns, which no caller can reorder. `Location::Node` already
carries a `String`, so an id-carrying variant needs nothing new.

## Alternatives considered

- **A node-addressed binding set.** Rejected in Decision 1: no resolution
  mechanism exists, and the schema names the other key.
- **A binding that carries both an id and an optional node target.** Rejected as
  speculative — nothing needs the second form, and the first form is what
  activation will use.
- **Living in `dashc`.** Impossible rather than rejected: the compiler holds the
  document and never the host's bindings (issue #851).
- **Living entirely in an integration crate**, which is issue #851's own
  phrasing. Rejected because the rule ids, `Severity`, `Profile` and the waiver
  machinery are all in `dashscene-validator` — the two `placeholder.*` rules
  #1126 added are already there — and a check placed outside it would not be
  reachable by the second host.

## Consequences

- A fourth gate exists. It is not the first to take a second input —
  `validate_asset_payloads` already does, for a related reason: an `AssetEntry`
  describes bytes the document does not contain. What is new is that this gate's
  second input is not an artifact at all, and exists nowhere in this repository:
  only a host knows which ids it binds. That does not weaken
  [`validator-three-gates.md`](validator-three-gates.md), whose choice was a
  gate per surface rather than one `validate()`.
- **An empty report is not always agreement.** One placeholder whose
  `contribution_id` is out of range silences `placeholder.undeclared-overload`
  for the whole document — and on a `Core` target it can silence
  `placeholder.unfilled` as well, since the unreadable id is absent from the set
  the arming test consults, so a binding whose only match was that id does not
  arm it. Both directions are debt #1275. The rest of this bullet is about the
  overload direction: the name this gate could not read could have been any
  binding's, so there is no subset it is safe to keep reporting. The document is
  blocked by the load gate's own error either way, and blaming the host for it
  would be a second diagnostic pointing at the wrong half. A caller must read
  that empty report as "not checked".
- **The gate walks every root, not the shown one.** A host showing one root of a
  multi-root document is told about placeholders in roots it never loads, and
  cannot clear them. That is the noise Decision 2 is otherwise careful to avoid,
  and it is left open rather than guessed at: a shown root reaches this gate
  through the same seam that will carry the binding list (issue #859), and
  nothing passes either today. Debt: #1272.
- Nothing calls it in this repository yet. The callers are hosts, and the host
  that motivates it is the Unity one (epic #1106); the C ABI's data plane (#859)
  is what will carry a binding list across boundary B. Until then the gate's
  tests are its only caller, which is the same posture story #1126's surface
  shipped in.
- `Placeholder.contribution_id` now has a consumer, so the story #1264 lowering
  of Figma's `dashscene/role = placeholder` gains a reason to carry the
  annotator's reserved `contribution-id` key
  ([`annotator-plugin-contract-frozen.md`](annotator-plugin-contract-frozen.md))
  rather than only the role.
- Placeholder **activation** stays in v1
  ([`../specification/05-qualification.md`](../specification/05-qualification.md)).
  This gate reports on the surface; it fills nothing.

Refs #1127. Refs #851. Refs #1126. Refs #1106.
