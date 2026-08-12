# Waivers key on (rule, target); the workaround hint is derived, not stored

    status   accepted (story #41, 2026-07-16)
    scope    crates/dashscene-validator
    binds    the strict-mode release gate; every producer that reports
             diagnostics; closes exit criterion E4
    extends  docs/decisions/validator-three-gates.md (which reserved
             waivers and the workaround hint for #41)

## Context

`validator-three-gates.md` fixed the three gates and the `Diagnostic` shape at
v0.3, and deferred two named pieces of
`docs/archive/2026-07-14-design-1-seed.md` §6.1 to v0.7 (issue #41): the
**workaround hint**, the fourth element of the diagnostic tuple, and the
**waiver** workflow that lets a release build proceed past a warning.

Story #41 also folds four review-deferred debts into the validator contract
(epic #36's v0.6-close revision): the load gate's clean-path allocation (#127),
a geometry-extent rule (#128), the `TextStyle.weight` range (#129), and a
`Construct` variant for variable-width stroke (#145).

Two constraints shaped the design:

- **The `Diagnostic` shape is load-bearing across the wasm ABI.** `dashc` owns
  serializable mirror types of `Diagnostic`, `Location`, and `Report`
  (`docs/decisions/dashc-wasm-abi.md`, `crates/dashc/src/abi/json.rs`), and
  constructs `Diagnostic { .. }` literals directly. A new struct field breaks
  those literals and would force a change inside `dashc`'s territory — out of
  scope for this story.
- **P4 governs the waiver vocabulary too.** "Vocabulary is validated, never
  discovered" applies to a waiver declaration as much as to a design construct:
  an out-of-scope waiver must be a named diagnostic, not a silent no-op.

## Decisions

### The workaround hint is a rule-keyed derivation, not a struct field

`Diagnostic::workaround(&self) -> Option<&'static str>` returns the
designer-visible workaround (`rule::workaround(self.rule)`), and `Display`
appends it. The hint is a pure function of the rule id — every diagnostic
carrying `profile.noise-or-texture-effect` has the same workaround — so nothing
is lost by deriving it on demand, and the `Diagnostic` struct keeps its four
fields.

Only the import gate's out-of-profile constructs carry a workaround (§04's "bake
it, slot it, design without it"). The referential-integrity and geometry rules
stand in front of producer bugs, not design choices, so they answer `None`:
there is nothing for a designer to rework.

Alternative considered — add `workaround: Option<&'static str>` to `Diagnostic`.
Rejected: it breaks the `Diagnostic` literals `dashc` constructs and the ABI
mirror in `crates/dashc/src/abi/json.rs`, and the data is redundant with the
rule id. If a future need makes the hint vary per occurrence rather than per
rule, the field becomes justified and the ABI mirror is widened in the same
change.

### A waiver names one (rule, target) pair, and only converts a warning

    Waiver { rule: String, at: Location, reason: String }

`Report::strict(&[Waiver]) -> StrictReport` is the release gate: it passes only
when no error remains and every warning is covered by a valid waiver.

- **Never a global mute, but target-complete.** A waiver matches a diagnostic by
  rule id **and** `Location`. A rule-only waiver would silence a whole class of
  warnings across the document; naming the target keeps each exception to one
  place. When a target carries several _identical_ findings — the same rule at
  the same location, which is genuinely reachable (a node with two
  advanced-blend-mode paints triages `profile.advanced-blend-mode` twice at that
  node) — one waiver covers them all. The alternative, one-waiver-per-finding,
  was rejected: identical findings carry no discriminating information to key a
  second waiver on, so it is empty ceremony. `StrictReport::applied` counts such
  a waiver once.
- **A duplicate waiver is surfaced, not silently doubled.** Two waivers with the
  same (rule, target) would both "apply" and double-count. The second, covering
  nothing the first did not, is `waiver.redundant` (a warning) and is not
  counted as a second application — so the audit trail stays honest.
- **An error is never waivable.** An error blocks the document unconditionally —
  only a warning is a "declared degrade" (§04). A waiver that matches an error
  leaves the error blocking and is itself diagnosed (`waiver.covers-an-error`).
- **The waiver vocabulary is validated (P4).** A waiver naming a rule id that is
  not in `rule::ALL` is `waiver.unknown-rule` (an error); a waiver matching
  nothing is `waiver.unused`, and a duplicate is `waiver.redundant` (both
  warnings — surfaced for hygiene, but they protect nothing and break nothing,
  so they do not fail the build). All are named, never silent.
- **Auditable.** Each waiver carries a `reason`, and `StrictReport::applied`
  reports the waivers that actually suppressed a warning — the record of
  exceptions granted, one entry per waiver.

The strict gate is a delivered, tested library contract; no producer calls it
yet and there is no waiver-file format, so it does not tighten E4 today (see
`docs/specification/05-qualification.md`). `Location` matching is exact,
including a node's DFS index. A name-path-only match (more stable across edits)
is deferred to that same wiring step — the machinery here is the strict gate and
its P4 self-validation, not a file format.

### Geometry rules and their gates

- `geometry.rect-invalid-extent` (#128) — a non-finite or negative `RectEntry`
  extent. Paint gate only: a document carries no resolved extent (P1). It names
  what `check_stroke_fits_box` only declined to judge.
- `geometry.corner-radius-invalid` (#128) — a negative or non-finite corner
  radius. Runs on **both** gates, like a stroke width: corners are geometry-free
  authored intent present in the document (`Paint.corners`), and the load gate
  is the only gate `compile_figma` runs, so a document-only check would miss it
  in the importer. A single rule covers negative and non-finite together — the
  producer's fix is the same, and a tester checks one predicate.
- A clipping node's corners are copied verbatim into every `ClipBox` of its
  subtree (`crates/dashscene-core/src/arena.rs`), so validating every
  `PaintEntry.corners` catches an out-of-spec clip at its authoring source. No
  `Location::ClipRegion` variant is added — a clip region has no natural
  identity of its own (it is a derived, deduplicated result, not an authored
  pool entry), so the finding belongs at the paint entry, not the region.

### `TextStyle.weight` range, its pooled `Location`, and the lazy node path

- `text.style-weight-out-of-range` (#129) — a weight outside the CSS scale
  100..=900 the schema pins. Load gate, on the text-style pool.
- A text-style diagnostic points at `Location::TextStyle(pool_index)`, added in
  this story. A text style is a genuine pooled surface — authored, dense,
  referenced by index — like `PaintEntry`/`ImageAsset`/`VariantSet`, so it takes
  its own variant. The first cut mis-keyed it as
  `Location::Node(NodePath::new(pool_index, ..))`, which violates the `Location`
  anti-collision contract: a consumer resolving `.index` to a layer would land
  on an unrelated node. This is the opposite case to a clip region above — a
  text style _has_ a pool identity worth naming, a clip region does not — so
  here a variant is right. It is an additive change to `dashc`'s ABI mirror
  (`crates/dashc/src/abi/json.rs`): one `WireLocation` arm plus its
  serialization test, no wire break (the report JSON gains a possible
  `"kind":"textStyle"`).
- The load gate builds a node's name path only when a rule fires (#127), and
  memoizes it per node, so a clean document allocates none and a
  heavily-malformed node walks its parent chain once, not once per diagnostic.
  The earlier shape built every node's owned path string before any rule ran,
  and the common clean case discarded them.

### Variable-width stroke joins the REJECT band

`Construct::VariableWidthStroke` (#145) is a REJECT-band error in both profiles
— no paint entry can express a per-length width, so it is baked or dropped,
never degraded (`docs/archive/2026-07-14-scope-decisions.md` §8). The
producer→`Construct` mapping stays `dashc`'s (P5); the `effects-2025` fixture
cannot yet carry it (no Figma Plugin API — `corpus/figma-fixtures/README.md`),
so the variant is exercised at the import gate directly.

## Consequences

- **E4 is met.** The complete named-rule set now stands behind the diagnostics a
  dirty Figma file produces; `compile_figma` refuses to emit a document when any
  is an error (`crates/dashc/src/lib.rs`), proven end to end by
  `crates/dashc/tests/figma_lowering.rs`'s
  `the_reject_fixture_is_refused_rather_than_emitted`. See
  `docs/specification/05-qualification.md`.
- **The ABI wire format is unchanged; the mirror gains one arm.** No
  `Diagnostic` field changed. `Location` gained an additive `TextStyle` variant,
  which `dashc`'s mirror learns through one mechanical `WireLocation` arm plus
  its serialization test — the report JSON gains a possible
  `"kind":"textStyle"`, no binary wire break, ABI version unchanged. The Deno
  `Location` type (`importers/figma/src/wasm.ts`) already lags the Rust mirror
  (it lists neither `variantSet` nor `textStyle`) and tolerates unknown kinds at
  runtime, so `just deno-test` stays green; completing that TS type is an
  importer (#39) concern. The new `Construct` variant is additive (the producer
  maps onto it, nothing matches it exhaustively outside the validator).
- **The strict gate is available but not yet wired.** `Report::strict` is the
  release-mode gate; wiring it into `dashc`/the importer with a waiver-file
  format is a later importer step, on this contract.
