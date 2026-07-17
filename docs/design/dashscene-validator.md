# dashscene-validator — the three gates, diagnostics, waivers, and the rule set

As-built after stories #15 and #139 (v0.3) and #41 (v0.7 — full
diagnostics and waivers). The rationale is in
`docs/decisions/validator-three-gates.md` and
`docs/decisions/waivers-and-diagnostic-completion.md`; this record is the
component's shape and its rule table.

## Position

The validator sits _beside_ the semantic model, not inside it. It reads the
document (`dashbuf`) and boundary B (`dashpaint`), and **producers call
it** — the arena does not. It has no `dashscene-core` dependency: core is
published earlier, and `CommittedScene`'s accessors already hand out
`dashpaint` types.

    producer source vocabulary ──► triage ────────────┐
                                                      │
    dashc ──► .dsb document ─────► validate_document ─┼──► Report
                                                      │
    Arena::commit ──► CommittedScene ► validate_scene ┘

## The three gates

| gate   | entry point                                                                    | input                         | catches                                                                         |
| ------ | ------------------------------------------------------------------------------ | ----------------------------- | ------------------------------------------------------------------------------- |
| import | `triage(Construct, Profile, NodePath) -> Diagnostic`                           | the producer's own vocabulary | out-of-profile constructs (`docs/specification/04-figma-vocabulary-profile.md`) |
| load   | `validate_document(&Document) -> Report`                                       | a `.dsb`                      | referential integrity, unknown enum values, geometry-free paint rules           |
| paint  | `validate_scene(&[RectEntry], &PaintTable, &ImageTable, &ClipTable) -> Report` | boundary B                    | geometry budgets, runtime index resolution                                      |

They are not interchangeable — each of the three failure classes is
invisible to the other two gates. See the decision record.

## Diagnostic

`docs/archive/2026-07-14-design-1-seed.md` §6.1's tuple:

    pub struct Diagnostic {
        pub rule: &'static str,  // stable, greppable: "paint.gradient.no-stops"
        pub severity: Severity,  // Error blocks the document; Warning degrades
        pub at: Location,        // node, paint-pool entry, or image asset
        pub message: String,
    }

The tuple's fourth element — the **workaround hint** — is
`Diagnostic::workaround(&self) -> Option<&'static str>`, a rule-keyed
derivation rather than a stored field, and `Display` appends it. The hint
is a pure function of the rule id, and keeping it out of the struct leaves
the `Diagnostic` shape — and the wasm-ABI mirror `dashc` owns of it
(`docs/decisions/dashc-wasm-abi.md`) — unchanged. Only the import gate's
out-of-profile constructs carry one (§04's "bake it, slot it, design
without it"); the referential-integrity and geometry rules stand in front
of producer bugs, so they answer `None`. See
`docs/decisions/waivers-and-diagnostic-completion.md`.

`Report` collects them in document order: `has_errors()` answers "is the
document blocked", `is_empty()` answers "does a normal build carry no
findings", `has(rule)` / `find(rule)` are what tests and callers pin, and
`strict(&[Waiver])` is the release-mode gate (see "Waivers" below).

### A producer assembles its own `Report`

The import gate returns one bare `Diagnostic` per construct, and the producer
that owns the mapping (P5) is the only code that knows when it has found them
all. `Report` therefore implements `FromIterator<Diagnostic>` and
`Extend<Diagnostic>` (story #139): a producer collects its findings and
assembles them, and `Extend` is what lets `dashc::compile_figma` fold the load
gate's report into the import gate's so both gates decide emission from one
merged report. `push` stays `pub(crate)`.

Without this a producer could triage a construct and then have no way to report
it — a silent drop by construction. See
`docs/decisions/producer-assembles-its-own-diagnostics.md`.

### `Location` — not everything reported is a node

    Location::Node(NodePath)     a node: DFS index (= rect index) + name path
    Location::PaintEntry(u32)    an entry of the paint pool, by pool index
    Location::ImageAsset(u32)    an image asset, by asset index
    Location::VariantSet(u32)    a variant set, by pool index (#20)
    Location::TextStyle(u32)     a text style, by pool index (#41)
    Location::Signal(u32)        a signal declaration, by pool index (#167)
    Location::Binding(u32)       a binding row, by row index (#167)

Every pooled surface — paint entry, image asset, variant set, text style —
is shared by every node that references it, so each is reported **once, at
its own index** — repeating one authoring mistake per referencing node would
bury the rest of the report. Their indices are _pool_ indices, and `Location`
is what stops them being mistaken for node indices: both are small integers,
so a consumer that resolves a diagnostic to a layer (an editor jumping to it,
or the waiver machinery keying on it) would otherwise land silently on an
unrelated node. Each pooled surface therefore has its own variant — a pool
index is never wrapped in a `Node`. `dashc`'s wasm-ABI mirror
(`crates/dashc/src/abi/json.rs`) has a matching arm per variant, so a new
pooled surface is an additive mirror change, not a wire break.

`NodePath` carries the document DFS index — which is the rect-table index
too — and the name chain (`/screen/card/badge`) when the surface has names.
Boundary B has none, so a scene node diagnostic renders as `#3`.

## Profiles

`Profile::Core` (lean/native painters) and `Profile::Full` (Unity-class).
At v0.3 they diverge only at the import gate, on the two constructs
`docs/specification/04-figma-vocabulary-profile.md` annotates
`(profile:full)` — backdrop blur and advanced blend modes,
which a `Core` target can never honor and so cannot degrade to anything.

`validate_document` takes no profile: every construct the v0.3 schema can
express is in the NOW band, so there is nothing to select. It regains one at
v0.8 when effects enter the schema.

## Waivers (strict mode)

`docs/design/architecture.md`: an `Error` blocks the document; a `Warning`
is a declared degrade a normal build lets through. A **release build runs
strict** and refuses even a warning, unless a declared waiver records that
the degrade is acceptable for one specific target.

    pub struct Waiver { pub rule: String, pub at: Location, pub reason: String }

    Report::strict(&[Waiver]) -> StrictReport

`StrictReport::passes()` is the release gate: it passes only when no error
remains and every warning is covered by a valid waiver. Three properties,
each recorded in `docs/decisions/waivers-and-diagnostic-completion.md`:

- **Never a global mute, but target-complete.** A waiver matches by rule id
  **and** `Location`, so it suppresses that rule at one target — not a rule
  everywhere. When a target carries several _identical_ findings (the same
  rule at the same location, e.g. two advanced-blend-mode paints on one
  node), one waiver covers them all — they carry no discriminating
  information, so one-waiver-each would be empty ceremony.
- **An error is never waivable.** A waiver matching an error leaves it
  blocking and is itself diagnosed (`waiver.covers-an-error`); only a
  warning is a degrade a waiver can accept.
- **The waiver vocabulary is validated (P4).** A waiver naming a rule id
  not in `rule::ALL` is `waiver.unknown-rule` (error); a waiver matching
  nothing is `waiver.unused`, and a waiver duplicating another (covering
  nothing an earlier one did not) is `waiver.redundant` — both warnings,
  surfaced for hygiene, non-blocking. `StrictReport::applied()` lists the
  waivers that actually suppressed a warning — the audit trail of exceptions
  granted, one entry per waiver even when it covers several findings.

The strict gate exists here; wiring it into `dashc`/the importer with a
waiver-file format is a later importer step, on this contract.

## Rule set

Load gate — document referential integrity and schema evolution:

| rule                               | why it exists                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `node.parent-out-of-range`         | the flatbuffer verifier checks structure, not references (#63)                                                                                                                                                                                                                                                                                                                             |
| `node.parent-not-before-child`     | the node array is in DFS order, so a parent's index is always lower; a forward reference cycles every consumer that walks up                                                                                                                                                                                                                                                               |
| `paint.entry-out-of-range`         | #63                                                                                                                                                                                                                                                                                                                                                                                        |
| `paint.conflicting-representation` | `paint_entry` supersedes the v0.1 `paint` shorthand, so setting both silently discards one (#63)                                                                                                                                                                                                                                                                                           |
| `text.string-out-of-range`         | same bug class as #63                                                                                                                                                                                                                                                                                                                                                                      |
| `text.style-out-of-range`          | same bug class as #63                                                                                                                                                                                                                                                                                                                                                                      |
| `text.style-weight-out-of-range`   | a `TextStyle.weight` outside the CSS scale 100..=900 the schema pins. Font selection would otherwise clamp it silently or pick an unintended face — the silent vocabulary drop P4 forbids (#129). The load gate already iterates the text-style pool for `text.style-no-color`; the weight is one more read                                                                                |
| `vocabulary.unknown-enum`          | the schema's enums are append-only, so a v0.3 reader handed a v0.8 document gets the new value as a raw integer — "range-check and emit a named diagnostic, never default silently". Covers every enum field: `LayoutMode`, `MainAxisAlign`, `CrossAxisAlign`, `AxisSizing`, the `Fill` union tag, `GradientKind`, `ScaleMode`, `StrokeAlign`, `ImageFormat`, and `Binding.channel` (#167) |
| `binding.signal-out-of-range`      | a binding row referencing a signal declaration the document does not carry; the loader resolves it unchecked, so a dangling index would panic at load (#167)                                                                                                                                                                                                                               |
| `binding.node-out-of-range`        | same bug class, for the row's node index (#167)                                                                                                                                                                                                                                                                                                                                            |
| `signal.name-duplicate`            | two signal declarations with one non-empty name; a runtime looks a document signal up by name (`dashlang::attach_live`), so a duplicate would silently shadow one declaration (#167)                                                                                                                                                                                                       |
| `asset.image-no-bytes`             | an image asset whose `bytes` vector is present but empty. The painter decodes behind `expect("image asset decodes (validated upstream, P4)")`, so reading the asset table only for its length would leave that `expect` with no upstream                                                                                                                                                   |

Paint vocabulary — run by **both** the load gate and the paint gate, since
a scene can be built without ever passing through a document:

| rule                                 | stands in front of                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paint.gradient.no-stops`            | the painter's `stops.first().expect(..)`. `(required)` mandates presence, not non-emptiness — the false assurance #100 names                                                                                                                                                                                                                                                                                                                                                                         |
| `paint.gradient.stop-budget`         | the painter's assertion against `dashpaint::MAX_GRADIENT_STOPS`                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `paint.gradient.stop-order`          | offsets that do not increase. Each is individually in `0..=1`, so no range rule catches them, but painters take the offsets as a monotonically increasing ramp (Skia's `positions` array) — unordered stops rasterize unpredictably and differ between painters. Equal offsets are allowed: that is how a hard color stop is authored                                                                                                                                                                |
| `paint.gradient.stop-offset-invalid` | a non-finite offset, or one outside `0..=1`                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `paint.stroke.invalid-width`         | a negative or non-finite width                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `geometry.corner-radius-invalid`     | a negative or non-finite corner radius (#128). Runs on both gates like a stroke width, because corners are geometry-free authored intent (`Paint.corners`) and the load gate is the only gate `compile_figma` runs. A clipping node's corners are copied verbatim into every `ClipBox` of its subtree (`crates/dashscene-core/src/arena.rs`), so checking a paint entry's corners catches an out-of-spec clip at its source — the painter's `RRect::new_rect_radii` does not clamp a negative radius |
| `paint.image-out-of-range`           | `ImageTable::resolve`'s panic (#63)                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |

Paint gate only — needs the solved box:

| rule                           | stands in front of                                                                                                                                                                                                                                                                                                                               |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `geometry.rect-invalid-extent` | a non-finite (NaN/infinite) or negative `RectEntry` width or height (#128). Rects come from the solver, so this is a broken inter-crate contract rather than authoring — but the paint gate is the last checkpoint before a painter rasterizes NaN geometry. It names what `paint.stroke.exceeds-box` only declines to judge on a non-finite box |
| `paint.stroke.exceeds-box`     | an `Inside` stroke wider than `min(w, h)`. The painter insets by half the width per side, so the stroked box is `w - width` by `h - width`; above the smaller extent it inverts and the stroke collapses instead of drawing (#100). The threshold is strict: exactly at `min(w, h)` the stroke covers the box, which is correct                  |
| `paint.entry-out-of-range`     | `PaintTable::resolve`'s panic, for a rect whose index misses                                                                                                                                                                                                                                                                                     |
| `clip.index-out-of-range`      | `ClipTable::resolve`'s panic, for a rect whose resolved clip region misses (#97). Clip regions exist only on a scene: a document carries clip _intent_ (`Paint.clip`, a bool), while the region a painter consumes is the ancestor-intersected result core computes at commit — and by P1 a result never appears in a document                   |

A pool entry is validated **once, at its own index** — it is shared by every
rect referencing it, so reporting per referencing rect would repeat one
authoring mistake N times and bury the rest of the report.

## Import-gate vocabulary

`Construct` names `docs/specification/04-figma-vocabulary-profile.md`'s
LATER and REJECT bands, and nothing else —
the NOW band is simply the schema.

    LATER (warn)    LayerBlur, BackdropBlur*, AdvancedBlendMode*,
                    CornerSmoothing, LuminanceMask, ClipOnRotated,
                    KashidaJustification
    REJECT (error)  NoiseOrTextureEffect, ProgressiveBlur,
                    AnimatedBooleanOp, AnimatedVariableFontAxis,
                    VariableWidthStroke (#145)

    * profile:full-only — an Error under profile:core.

The producer maps its source vocabulary onto these (P5: the validator never
parses Figma JSON). `dashc` owns that mapping from #16.

## What the painter may now assume

Every `expect`/`assert` in `dashscene-skia` documented as "validated
upstream (P4)" has a named rule standing in front of it. `MAX_GRADIENT_STOPS`
moved from a private constant in the painter to `dashpaint` (boundary B) so
the painter's assertion, its test, and the validator's rule read one number
and cannot drift.

Story #97 closed the last of them: `PaintEntry::clip` no longer exists, the
painter's `unimplemented!` on it is gone, and the `ClipTable::resolve` panic
that replaced it has `clip.index-out-of-range` standing in front of it.
