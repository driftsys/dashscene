# dashscene-validator — the three gates, diagnostics, and the v0.3 rule set

As-built after stories #15 and #139 (v0.3). The rationale is in
`docs/decisions/validator-three-gates.md`; this record is the component's
shape and its rule table.

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

`docs/archive/2026-07-14-design-1-seed.md` §6.1's tuple, minus the workaround
hint (v0.7, #41):

    pub struct Diagnostic {
        pub rule: &'static str,  // stable, greppable: "paint.gradient.no-stops"
        pub severity: Severity,  // Error blocks the document; Warning degrades
        pub at: Location,        // node, paint-pool entry, or image asset
        pub message: String,
    }

`Report` collects them in document order: `has_errors()` answers "is the
document blocked", `is_empty()` answers "does a strict build pass",
`has(rule)` / `find(rule)` are what tests and callers pin.

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

A pooled paint entry and an image asset are shared by every node that
references them, so each is reported **once, at its own index** — repeating
one authoring mistake per referencing node would bury the rest of the
report. Their indices are _pool_ indices, and `Location` is what stops them
being mistaken for node indices: both are small integers, so a consumer that
resolves a diagnostic to a layer (an editor jumping to it, #41's waiver
machinery keying on it) would otherwise land silently on an unrelated node.

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

## Rule set (v0.1–v0.3 vocabulary)

Load gate — document referential integrity and schema evolution:

| rule                               | why it exists                                                                                                                                                                                                                                                                                                                                                    |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `node.parent-out-of-range`         | the flatbuffer verifier checks structure, not references (#63)                                                                                                                                                                                                                                                                                                   |
| `node.parent-not-before-child`     | the node array is in DFS order, so a parent's index is always lower; a forward reference cycles every consumer that walks up                                                                                                                                                                                                                                     |
| `paint.entry-out-of-range`         | #63                                                                                                                                                                                                                                                                                                                                                              |
| `paint.conflicting-representation` | `paint_entry` supersedes the v0.1 `paint` shorthand, so setting both silently discards one (#63)                                                                                                                                                                                                                                                                 |
| `text.string-out-of-range`         | same bug class as #63                                                                                                                                                                                                                                                                                                                                            |
| `text.style-out-of-range`          | same bug class as #63                                                                                                                                                                                                                                                                                                                                            |
| `vocabulary.unknown-enum`          | the schema's enums are append-only, so a v0.3 reader handed a v0.8 document gets the new value as a raw integer — "range-check and emit a named diagnostic, never default silently". Covers every enum field: `LayoutMode`, `MainAxisAlign`, `CrossAxisAlign`, `AxisSizing`, the `Fill` union tag, `GradientKind`, `ScaleMode`, `StrokeAlign`, and `ImageFormat` |
| `asset.image-no-bytes`             | an image asset whose `bytes` vector is present but empty. The painter decodes behind `expect("image asset decodes (validated upstream, P4)")`, so reading the asset table only for its length would leave that `expect` with no upstream                                                                                                                         |

Paint vocabulary — run by **both** the load gate and the paint gate, since
a scene can be built without ever passing through a document:

| rule                                 | stands in front of                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paint.gradient.no-stops`            | the painter's `stops.first().expect(..)`. `(required)` mandates presence, not non-emptiness — the false assurance #100 names                                                                                                                                                                                                          |
| `paint.gradient.stop-budget`         | the painter's assertion against `dashpaint::MAX_GRADIENT_STOPS`                                                                                                                                                                                                                                                                       |
| `paint.gradient.stop-order`          | offsets that do not increase. Each is individually in `0..=1`, so no range rule catches them, but painters take the offsets as a monotonically increasing ramp (Skia's `positions` array) — unordered stops rasterize unpredictably and differ between painters. Equal offsets are allowed: that is how a hard color stop is authored |
| `paint.gradient.stop-offset-invalid` | a non-finite offset, or one outside `0..=1`                                                                                                                                                                                                                                                                                           |
| `paint.stroke.invalid-width`         | a negative or non-finite width                                                                                                                                                                                                                                                                                                        |
| `paint.image-out-of-range`           | `ImageTable::resolve`'s panic (#63)                                                                                                                                                                                                                                                                                                   |

Paint gate only — needs the solved box:

| rule                       | stands in front of                                                                                                                                                                                                                                                                                                              |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paint.stroke.exceeds-box` | an `Inside` stroke wider than `min(w, h)`. The painter insets by half the width per side, so the stroked box is `w - width` by `h - width`; above the smaller extent it inverts and the stroke collapses instead of drawing (#100). The threshold is strict: exactly at `min(w, h)` the stroke covers the box, which is correct |
| `paint.entry-out-of-range` | `PaintTable::resolve`'s panic, for a rect whose index misses                                                                                                                                                                                                                                                                    |
| `clip.index-out-of-range`  | `ClipTable::resolve`'s panic, for a rect whose resolved clip region misses (#97). Clip regions exist only on a scene: a document carries clip _intent_ (`Paint.clip`, a bool), while the region a painter consumes is the ancestor-intersected result core computes at commit — and by P1 a result never appears in a document  |

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
                    AnimatedBooleanOp, AnimatedVariableFontAxis

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
